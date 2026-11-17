//! Workflow runs in the app: harlo does what a run's logic
//! (`harlo_core::workflow`) says next, and tells it what happened.
//!
//! A run is saved before each thing it says to do is done, so harlo can be
//! closed (or stop) at any point: when it opens again, once the sessions of
//! the run's machines are listed, the run reconnects to its sessions or
//! pauses, saying why.
//!
//! The implementation and review sessions are found by their live keys
//! while harlo runs (`workflow_keys`), and by their files (with their
//! machine) across restarts.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::db::WorktreeRecord;
use harlo_core::git;
use harlo_core::workflow::{self, Action, Agent, Event, Outcome, Place, Request, Run, StageKind, State, Tone};
use harlo_harness::HarnessKind;
use serde_json::{Value, json};

use super::qobject::Workspace;
use super::{FirstPrompt, ToolCall, send_first_prompt};
use crate::registry;
use harlo_core::agents;

/// The longest a project's tests may run before they count as failed.
const TEST_LIMIT: Duration = Duration::from_mins(30);

/// How long a run on another machine waits for that machine's sessions to
/// be listed after harlo opens, before it pauses.
const HOST_WAIT: Duration = Duration::from_secs(90);

/// A session a run is waiting on: its run, and the dispatch it serves.
#[derive(Clone, Debug)]
pub(super) struct WorkflowSession {
    run: String,
    seq: u64,
    kind: StageKind,
    /// What it reported this turn (`report_done`), for its attempt.
    report: Option<(u32, harlo_core::orchestration::Report)>,
}

/// What the app keeps of its runs.
#[derive(Default)]
pub(super) struct Workflows {
    pub(super) runs: HashMap<String, Run>,
    /// Sessions runs are waiting on, by live key.
    pub(super) keys: HashMap<String, WorkflowSession>,
    /// Runs read at start that haven't been picked up yet.
    unrecovered: Vec<String>,
    /// Machines whose sessions have been listed since harlo opened (""
    /// this one).
    listed: Vec<String>,
    /// Each running check's output so far, by run: its dispatch, the end of
    /// its output, and the seconds it has run (not saved).
    live: HashMap<String, (u64, String, u64)>,
    /// You were told harlo goes on without its window.
    told_background: bool,
}

impl Workflows {
    /// The saved runs, read as harlo opens; the unfinished ones are picked
    /// up once their machines' sessions are listed.
    pub(super) fn load(db: &harlo_core::db::Db) -> Self {
        let runs: HashMap<String, Run> = db
            .workflow_runs()
            .unwrap_or_default()
            .into_iter()
            .map(|mut r| {
                workflow::upgrade(&mut r);
                (r.id.clone(), r)
            })
            .collect();
        let mut unrecovered: Vec<String> = runs.values().filter(|r| !r.finished()).map(|r| r.id.clone()).collect();
        unrecovered.sort();
        Self { runs, unrecovered, ..Self::default() }
    }
}

/// Stops harlo at once, as a crash would, at a test's crash point
/// (`HARLO_TEST_CRASH`: a name, or `name@N` for its Nth time); nothing else
/// uses it.
///
/// The points: `workflow-<part>-before-start` just before a dispatch is done
/// (it's saved: worktree, implement, prompt, check, review) and
/// `workflow-<part>-unrecorded` once a session started but before its file
/// is recorded (implement, review).
fn crash_point(name: &str) {
    static HITS: std::sync::Mutex<Vec<(String, u32)>> = std::sync::Mutex::new(Vec::new());
    let Ok(wanted) = std::env::var("HARLO_TEST_CRASH") else { return };
    let (point, nth) = wanted.split_once('@').map_or((wanted.as_str(), 1), |(p, n)| (p, n.parse().unwrap_or(1)));
    if point != name {
        return;
    }
    let hits = {
        let mut all = HITS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let i = all.iter().position(|(n, _)| n == name).unwrap_or_else(|| {
            all.push((name.to_owned(), 0));
            all.len() - 1
        });
        all[i].1 += 1;
        all[i].1
    };
    if hits != nth {
        return;
    }
    tracing::event!(name: "workflow.test.crash", tracing::Level::WARN, workflow.crash_point = name, "stopping at the test crash point {{workflow.crash_point}}");
    let _ = std::process::Command::new("kill").args(["-9", &std::process::id().to_string()]).status();
    std::process::abort();
}

fn now() -> i64 {
    chrono::Local::now().timestamp()
}

/// A new run's id: `wf-` and eight hex digits.
fn new_id() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mixed = nanos ^ (u128::from(std::process::id()) << 40);
    format!("wf-{:08x}", mixed % (1 << 32))
}

/// A project's test command, guessed from its files (this machine's only).
fn guess_test_command(root: &Path) -> String {
    let has = |f: &str| root.join(f).exists();
    if has("Cargo.toml") {
        "cargo test".into()
    } else if has("package.json") {
        "npm test".into()
    } else if has("go.mod") {
        "go test ./...".into()
    } else if has("pyproject.toml") || has("pytest.ini") || has("setup.py") {
        "pytest".into()
    } else if std::fs::read_to_string(root.join("Makefile")).is_ok_and(|m| m.lines().any(|l| l.starts_with("test:"))) {
        "make test".into()
    } else {
        String::new()
    }
}

/// How often a running check's output goes to its card.
const LIVE_EVERY: Duration = Duration::from_millis(500);

/// The most of a check's output kept while it runs (its end).
const LIVE_BYTES: usize = 256 * 1024;

/// Lines of a running check's output its card shows.
const LIVE_LINES: usize = 12;

/// Runs `command` in `folder` (over SSH when it's another machine's): its
/// exit status and the end of its output. `live` gets the end of its
/// output so far (and the seconds it has run) about twice a second.
fn run_check(command: &str, folder: &Path, live: &dyn Fn(String, u64)) -> Outcome {
    let started = std::time::Instant::now();
    let cmd = match harlo_core::remote::split(folder).and_then(|(h, p)| Some((harlo_core::remote::host(&h)?, p))) {
        Some((host, p)) => Ok(harlo_core::remote::command(&host, &format!("cd {} && ( {command} ) 2>&1", harlo_core::remote::quote(&p)))),
        None if harlo_core::remote::is_remote(folder) => Err(anyhow::Error::msg("its machine isn't connected")),
        None => {
            let mut cmd = std::process::Command::new("sh");
            cmd.arg("-c").arg(format!("( {command} ) 2>&1")).current_dir(folder).stdin(std::process::Stdio::null());
            Ok(cmd)
        }
    };
    let result = cmd.and_then(|cmd| stream(cmd, started, live));
    let seconds = started.elapsed().as_secs();
    match result {
        Ok((exit, output)) => Outcome { exit, tail: workflow::tail(&output, workflow::OUTPUT_LINES), seconds, error: None, name: String::new() },
        Err(e) => Outcome { exit: None, tail: String::new(), seconds, error: Some(format!("{e:#}")), name: String::new() },
    }
}

/// Runs `cmd`, its output read as it comes ([`run_check`]), ended after
/// [`TEST_LIMIT`].
fn stream(mut cmd: std::process::Command, started: std::time::Instant, live: &dyn Fn(String, u64)) -> anyhow::Result<(Option<i32>, String)> {
    use std::io::Read as _;
    let mut child = cmd.stdout(std::process::Stdio::piped()).spawn()?;
    let mut stdout = child.stdout.take().ok_or_else(|| anyhow::Error::msg("no output pipe"))?;
    let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let sink = std::sync::Arc::clone(&output);
    let reader = std::thread::spawn(move || {
        let mut chunk = [0_u8; 8192];
        while let Ok(n) = stdout.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let mut out = sink.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            out.extend_from_slice(&chunk[..n]);
            if out.len() > 2 * LIVE_BYTES {
                let cut = out.len() - LIVE_BYTES;
                out.drain(..cut);
            }
        }
    });
    let deadline = started + TEST_LIMIT;
    let mut told = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow::anyhow!("still running after {} minutes: stopped", TEST_LIMIT.as_secs() / 60));
        }
        if told.elapsed() >= LIVE_EVERY {
            told = std::time::Instant::now();
            let text = String::from_utf8_lossy(&output.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned();
            live(workflow::tail(&text, LIVE_LINES), started.elapsed().as_secs());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = reader.join();
    let out = String::from_utf8_lossy(&output.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned();
    Ok((status.code(), out))
}

impl Workspace {
    /// Starts a workflow in a project: `form` is `{ workflow, task,
    /// criteria, testCommand, implementer, implementerModel, reviewer,
    /// reviewerModel, approval }` (`workflow` "": the built-in one). The
    /// problem, if any ("" once started).
    pub fn start_workflow(mut self: Pin<&mut Self>, project_id: i32, form: &QString) -> QString {
        let form: Value = serde_json::from_str(&form.to_string()).unwrap_or_default();
        let field = |k: &str| form[k].as_str().unwrap_or_default().trim().to_owned();
        let Some(project) = self.projects.iter().find(|p| p.id == i64::from(project_id)).cloned() else {
            return QString::from("That project isn't open in harlo.");
        };
        if !self.orchestration_on {
            return QString::from("Workflows need orchestration (Settings → Orchestration).");
        }
        let Some(definition) = self.workflow_definition(&field("workflow")) else {
            return QString::from("That workflow isn't saved any more: pick another.");
        };
        if let Err(why) = definition.validate() {
            return QString::from(&why);
        }
        let (task, test_command) = (field("task"), field("testCommand"));
        if task.is_empty() {
            return QString::from("Say what to implement.");
        }
        if test_command.is_empty() && definition.needs_test_command() {
            return QString::from("Give the command that runs the project's tests.");
        }
        let agent = |name: &str, model: &str, fallback: &str| {
            let harness = HarnessKind::parse(&field(name)).map_or_else(|| fallback.to_owned(), |k| k.as_str().to_owned());
            Agent { harness, model: field(model) }
        };
        let request = Request {
            project_id: project.id,
            project: Place::of(&project.path),
            definition,
            task,
            criteria: field("criteria"),
            test_command: test_command.clone(),
            implementer: agent("implementer", "implementerModel", "harlo"),
            reviewer: agent("reviewer", "reviewerModel", "claude"),
            approval: Some(field("approval")).filter(|m| harlo_core::session::APPROVAL_MODES.contains(&m.as_str())).unwrap_or_default(),
        };
        if !test_command.is_empty() {
            self.save_setting(&format!("workflow_test_{project_id}"), &test_command);
        }
        self.save_setting(&format!("workflow_last_{project_id}"), &request.definition.id);
        let mut run = workflow::new_run(&new_id(), request, now());
        let actions = workflow::step(&mut run, Event::Begin, now());
        let id = run.id.clone();
        self.as_mut().keep_workflow(run);
        self.as_mut().do_workflow(&id, actions);
        QString::default()
    }

    /// The test command a project's workflow runs: the last one used,
    /// else one guessed from its files ("" when there's no guess).
    pub fn workflow_test_command(&self, project_id: i32) -> QString {
        let saved = self.setting_str(&format!("workflow_test_{project_id}"));
        if !saved.is_empty() {
            return QString::from(&saved);
        }
        let Some(path) = self.projects.iter().find(|p| p.id == i64::from(project_id)).map(|p| p.path.clone()) else { return QString::default() };
        if harlo_core::remote::is_remote(&path) {
            return QString::default();
        }
        QString::from(&guess_test_command(&git::repo_root(&path).unwrap_or(path)))
    }

    /// The workflow last started in a project ("" none yet).
    pub fn workflow_choice(&self, project_id: i32) -> QString {
        QString::from(&self.setting_str(&format!("workflow_last_{project_id}")))
    }

    /// A workflow by its id ("": the built-in one); None when it's gone.
    fn workflow_definition(&self, id: &str) -> Option<workflow::Definition> {
        if id.is_empty() || id == workflow::IMPLEMENT_AND_VERIFY {
            return Some(workflow::implement_and_verify());
        }
        self.db()?.workflows().ok()?.into_iter().find(|w| w.id == id)
    }

    /// The workflows to pick from, the built-in one first: each as saved,
    /// with `builtin`, `needsTestCommand` and `needsReviewer`.
    pub fn workflow_definitions_json(&self) -> QString {
        let saved = self.db().and_then(|db| db.workflows().ok()).unwrap_or_default();
        let list: Vec<Value> = std::iter::once((workflow::implement_and_verify(), true))
            .chain(saved.into_iter().map(|w| (w, false)))
            .map(|(w, builtin)| {
                let mut v = serde_json::to_value(&w).unwrap_or_default();
                v["builtin"] = json!(builtin);
                v["needsTestCommand"] = json!(w.needs_test_command());
                v["needsReviewer"] = json!(w.needs_reviewer());
                v
            })
            .collect();
        QString::from(&Value::Array(list).to_string())
    }

    /// Saves one of your workflows (a new one when its `id` is "" or the
    /// built-in one's): `{ id, error }`.
    pub fn save_workflow_definition(mut self: Pin<&mut Self>, definition: &QString) -> QString {
        let answer = |id: &str, error: &str| QString::from(&json!({ "id": id, "error": error }).to_string());
        let mut flow: workflow::Definition = match serde_json::from_str(&definition.to_string()) {
            Ok(f) => f,
            Err(e) => return answer("", &format!("harlo couldn't read the workflow: {e}")),
        };
        flow.name = flow.name.trim().to_owned();
        if let Err(why) = flow.validate() {
            return answer(&flow.id, &why);
        }
        if flow.id.is_empty() || flow.id == workflow::IMPLEMENT_AND_VERIFY {
            flow.id = new_id().replacen("wf-", "wfd-", 1);
        }
        let saved = self.db().map(|db| db.save_workflow(&flow));
        if let Some(Err(e)) = saved {
            return answer(&flow.id, &format!("It couldn't be saved: {e:#}"));
        }
        self.as_mut().workflows_changed();
        answer(&flow.id, "")
    }

    /// Deletes one of your workflows (runs keep their own copy).
    pub fn delete_workflow_definition(mut self: Pin<&mut Self>, id: &QString) {
        if let Some(db) = self.db() {
            let _ = db.delete_workflow(&id.to_string());
        }
        self.as_mut().workflows_changed();
    }

    /// The run a session belongs to (its implementation or one of its
    /// reviewers), as its card shows it; "" when it's in none.
    pub fn workflow_for(&self, key: &QString) -> QString {
        let key = key.to_string();
        let run = self.workflows.keys.get(&key).and_then(|s| self.workflows.runs.get(&s.run)).or_else(|| {
            let file = self.file_for_key(&key)?;
            self.workflows.runs.values().filter(|r| r.implementation.as_ref().is_some_and(|p| p.is(&file)) || r.wait.as_ref().and_then(|w| w.session.as_ref()).is_some_and(|p| p.is(&file))).max_by_key(|r| r.created_at)
        });
        run.map_or_else(QString::default, |r| QString::from(&self.workflow_card(r, &key).to_string()))
    }

    /// Every run, newest first, as their cards show them (with their
    /// implementation session's file).
    pub fn workflows_json(&self) -> QString {
        let mut runs: Vec<&Run> = self.workflows.runs.values().collect();
        runs.sort_by_key(|r| std::cmp::Reverse((r.created_at, r.id.clone())));
        let cards: Vec<Value> = runs.into_iter().map(|r| self.workflow_card(r, "")).collect();
        QString::from(&Value::Array(cards).to_string())
    }

    /// Runs under way (running, not paused).
    pub fn workflows_under_way(&self) -> i32 {
        i32::try_from(self.workflows.runs.values().filter(|r| r.state == State::Running).count()).unwrap_or(i32::MAX)
    }

    /// The window closed while runs are under way: a notification says
    /// harlo goes on without it (once per start).
    pub fn workflows_backgrounded(mut self: Pin<&mut Self>) {
        if self.workflows.told_background {
            return;
        }
        self.as_mut().rust_mut().workflows.told_background = true;
        let n = self.workflows_under_way();
        let body = format!(
            "{} under way: harlo goes on without its window and quits when {} over. Open harlo to see {}.",
            if n == 1 { "A workflow run is".to_owned() } else { format!("{n} workflow runs are") },
            if n == 1 { "it's" } else { "they're" },
            if n == 1 { "it" } else { "them" }
        );
        self.as_mut().deliver("workflow-background", "completion", "", "Harlo keeps running", &body, false);
    }

    /// Pause, Resume or Stop a run (`action`: pause | resume | stop).
    pub fn workflow_action(mut self: Pin<&mut Self>, id: &QString, action: &QString) {
        let id = id.to_string();
        // (Waiting for its machine's sessions: looked for again.)
        if action.to_string() == "resume"
            && let Some(hosts) = self.workflows.runs.get(&id).filter(|r| matches!(r.state, State::Paused { next: workflow::Next::Recover, .. })).map(Run::hosts)
        {
            if hosts.iter().all(|h| self.workflows.listed.contains(h)) {
                self.as_mut().rust_mut().workflows.unrecovered.retain(|u| *u != id);
                return self.recover_workflow(&id);
            }
            return self.set_notice_text(&format!("harlo still can't list the sessions on {}: connect it first.", hosts.join(", ")), true);
        }
        let event = match action.to_string().as_str() {
            "pause" => Event::Pause,
            "resume" => Event::Resume,
            "stop" => Event::Stop,
            "merge" => return self.finish_workflow(&id, Ending::Merge),
            "publish" => return self.finish_workflow(&id, Ending::Publish),
            "remove" => return self.finish_workflow(&id, Ending::Remove),
            _ => return,
        };
        self.as_mut().workflow_event(&id, event);
    }

    /// Does `ending` with a run's work (in the background, on its project's
    /// machine): its uncommitted changes committed, then its branch merged
    /// into the project's checkout or pushed for a pull request; or its
    /// worktree removed (the run stopped first, its branch deleted once
    /// merged). The run's log and the notice say how it went.
    fn finish_workflow(mut self: Pin<&mut Self>, id: &str, ending: Ending) {
        let Some(mut run) = self.workflows.runs.get(id).cloned() else { return };
        let (Some(tree), Some((_, root))) = (run.worktree.clone(), self.project_root(i32::try_from(run.project_id).unwrap_or(-1))) else {
            return self.set_notice_text("The run's project or worktree isn't known to harlo any more.", true);
        };
        if run.finish.removed {
            return self.set_notice_text("Its worktree is removed already.", true);
        }
        if ending != Ending::Remove && run.state != State::Done {
            return self.set_notice_text("Only a finished run's work is merged or published.", true);
        }
        if ending == Ending::Remove {
            if !run.finished() {
                workflow::step(&mut run, Event::Stop, now());
            }
            // (Its sessions there are closed first.)
            let inside: Vec<String> = self.live.values().filter(|e| e.info.cwd.starts_with(&tree.place.path)).map(|e| e.info.key.clone()).collect();
            for k in inside {
                self.as_mut().close_session(&QString::from(&k));
            }
            self.as_mut().rust_mut().workflows.keys.retain(|_, s| s.run != run.id);
        }
        self.as_mut().keep_workflow(run.clone());
        let message = format!("{}\n\n{} (harlo workflow {}), attempt {}: {}.", run.title(), run.definition.name, run.id, run.attempt, run.definition.done_text());
        let merged = run.finish.merged;
        self.as_mut().set_notice_text(&format!("{} {}…", ending.doing(), tree.branch), false);
        let id = id.to_owned();
        self.background(
            move || -> anyhow::Result<String> {
                let main = git::repo_root(&root).ok_or_else(|| anyhow::Error::msg("The project isn't a git repository."))?;
                let path = &tree.place.path;
                if ending != Ending::Remove && git::status(path).is_ok_and(|st| !st.files.is_empty()) {
                    git::stage_all(path)?;
                    git::commit(path, &message)?;
                }
                match ending {
                    Ending::Merge => git::merge_branch(&main, &tree.branch),
                    Ending::Publish => git::push(path, true).map(|_| format!("Pushed {}: open its pull request in the Changes panel", tree.branch)),
                    Ending::Remove => {
                        git::remove_worktree(&main, path, true)?;
                        if merged && git::delete_branch(&main, &tree.branch, false).is_ok() {
                            return Ok(format!("Removed the worktree and the merged branch {}", tree.branch));
                        }
                        Ok(format!("Removed the worktree (branch {} kept)", tree.branch))
                    }
                }
            },
            move |mut this, result| {
                let Some(mut run) = this.workflows.runs.get(&id).cloned() else { return };
                match &result {
                    Ok(said) => {
                        match ending {
                            Ending::Merge => run.finish.merged = true,
                            Ending::Publish => run.finish.published = true,
                            Ending::Remove => {
                                run.finish.removed = true;
                                if let (Some(db), Some(tree)) = (this.db(), run.worktree.as_ref()) {
                                    let _ = db.mark_worktree_removed(&tree.place.path);
                                }
                            }
                        }
                        run.record(now(), Tone::Good, format!("{said}."));
                        this.as_mut().set_notice_text(said, false);
                    }
                    Err(e) => {
                        let said = format!("{e:#}").split_whitespace().collect::<Vec<_>>().join(" ");
                        run.record(now(), Tone::Bad, format!("{} failed: {said}", ending.doing()));
                        this.as_mut().set_notice_text(&format!("{e:#}"), true);
                    }
                }
                this.as_mut().keep_workflow(run);
                this.as_mut().reload_worktrees();
            },
        );
    }

    /// A run as its card reads it.
    fn workflow_card(&self, run: &Run, key: &str) -> Value {
        let (state, why) = match &run.state {
            State::Running => ("running", String::new()),
            State::Paused { why, .. } => ("paused", why.clone()),
            State::Done => ("done", String::new()),
            State::Stopped => ("stopped", String::new()),
        };
        let stages: Vec<Value> = run
            .definition
            .stages
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let status = match &run.state {
                    State::Done => "done",
                    _ if i < run.stage => "done",
                    State::Paused { .. } if i == run.stage => "paused",
                    State::Running if i == run.stage => "active",
                    _ => "todo",
                };
                let what = match s.kind {
                    StageKind::Implement => agents::label(&run.implementer.harness).to_owned(),
                    StageKind::Check => format!("`{}`", s.command),
                    StageKind::Review => agents::label(&s.agent.harness).to_owned(),
                };
                json!({ "label": s.name, "status": status, "kind": stage_kind_name(s.kind), "what": what })
            })
            .collect();
        let current = run.current();
        let waiting = match (&run.state, run.wait.as_ref().map(|w| w.kind)) {
            (State::Running, Some(StageKind::Implement)) => format!("The implementation session is working on attempt {}", run.attempt),
            (State::Running, Some(StageKind::Check)) => format!("Running `{}`", current.map(|s| s.command.as_str()).unwrap_or_default()),
            (State::Running, Some(StageKind::Review)) => {
                format!("{} is reviewing attempt {}", agents::label(&current.map(|s| s.agent.harness.clone()).unwrap_or_default()), run.attempt)
            }
            (State::Running, None) if run.worktree.is_none() => "Making the worktree".to_owned(),
            _ => String::new(),
        };
        let role = match self.workflows.keys.get(key).map(|s| s.kind) {
            Some(StageKind::Review) => "review",
            Some(_) => "implementation",
            None if self.file_for_key(key).is_some_and(|f| run.implementation.as_ref().is_some_and(|p| p.is(&f))) => "implementation",
            None => "review",
        };
        let log: Vec<Value> = run.log.iter().rev().take(40).rev().map(|e| json!({ "at": e.at, "attempt": e.attempt, "tone": tone_name(e.tone), "text": e.text })).collect();
        let live = self
            .workflows
            .live
            .get(&run.id)
            .filter(|l| run.state != State::Stopped && run.wait.as_ref().is_some_and(|w| w.seq == l.0 && w.kind == StageKind::Check))
            .map(|l| json!({ "name": current.map(|s| s.name.clone()).unwrap_or_default(), "output": l.1, "seconds": l.2 }));
        let check = run.last_check.as_ref().map(|t| json!({ "name": t.name, "passed": t.passed(), "exit": t.exit, "seconds": t.seconds, "tail": workflow::tail(&t.tail, 12) }));
        json!({
            "id": run.id, "title": run.title(), "workflow": run.definition.name, "doneText": run.definition.done_text(),
            "project": run.project.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), "createdAt": run.created_at,
            "state": state, "why": why, "attempt": run.attempt, "stages": stages,
            "waiting": waiting, "role": role, "branch": run.worktree.as_ref().map(|w| w.branch.clone()).unwrap_or_default(),
            "implementer": agents::label(&run.implementer.harness), "reviewer": agents::label(&run.reviewer.harness),
            "testCommand": run.test_command, "lastCheck": check.unwrap_or(Value::Null), "liveCheck": live.unwrap_or(Value::Null), "log": log,
            "host": run.project.host, "merged": run.finish.merged, "published": run.finish.published, "removed": run.finish.removed,
            "base": run.worktree.as_ref().map(|w| w.base.clone()).unwrap_or_default(),
            "worktree": run.worktree.as_ref().map(|w| w.place.path.to_string_lossy().into_owned()).unwrap_or_default(),
            "implementation": run.implementation.as_ref().map(|p| p.path.to_string_lossy().into_owned()).unwrap_or_default(),
            "session": run.wait.as_ref().and_then(|w| w.session.as_ref()).map(|p| p.path.to_string_lossy().into_owned()).unwrap_or_default(),
        })
    }

    /// Keeps a run (in memory and saved) and tells the views.
    fn keep_workflow(mut self: Pin<&mut Self>, run: Run) {
        if let Some(db) = self.db()
            && let Err(e) = db.save_workflow_run(&run)
        {
            tracing::event!(name: "workflow.run.save.failed", tracing::Level::WARN, workflow.id = %run.id, error.message = %format_args!("{e:#}"), "workflow {{workflow.id}} wasn't saved: {{error.message}}");
        }
        self.as_mut().rust_mut().workflows.runs.insert(run.id.clone(), run);
        self.as_mut().workflows_changed();
    }

    /// Something happened to run `id`: its logic decides, the run is
    /// saved, then what it says is done.
    fn workflow_event(mut self: Pin<&mut Self>, id: &str, event: Event) {
        let Some(mut run) = self.workflows.runs.get(id).cloned() else { return };
        let actions = workflow::step(&mut run, event, now());
        self.as_mut().keep_workflow(run);
        self.as_mut().do_workflow(id, actions);
    }

    /// Does what run `id` says (it's saved already).
    fn do_workflow(mut self: Pin<&mut Self>, id: &str, actions: Vec<Action>) {
        for action in actions {
            let Some(run) = self.workflows.runs.get(id).cloned() else { return };
            let part = match &action {
                Action::MakeWorktree { .. } => "worktree",
                Action::StartImplementation { .. } => "implement",
                Action::Prompt { .. } => "prompt",
                Action::RunCheck { .. } => "check",
                Action::StartReview { .. } => "review",
                Action::Tell { .. } => "",
            };
            if !part.is_empty() {
                crash_point(&format!("workflow-{part}-before-start"));
            }
            match action {
                Action::MakeWorktree { branch } => self.as_mut().workflow_worktree(&run, branch),
                Action::StartImplementation { seq, prompt } => {
                    let agent = run.implementer.clone();
                    let title = workflow_title(&run, StageKind::Implement);
                    self.as_mut().workflow_session(&run, seq, StageKind::Implement, &agent, &title, prompt, true);
                }
                Action::StartReview { seq, agent, prompt } => {
                    let title = workflow_title(&run, StageKind::Review);
                    self.as_mut().workflow_session(&run, seq, StageKind::Review, &agent, &title, prompt, false);
                }
                Action::Prompt { seq, kind, session, text } => self.as_mut().workflow_prompt(&run, seq, kind, &session, &text),
                Action::RunCheck { seq, attempt, command, folder, .. } => {
                    let (thread, id) = (self.qt_thread(), id.to_owned());
                    self.as_mut().rust_mut().workflows.live.insert(id.clone(), (seq, String::new(), 0));
                    crate::runtime().spawn_blocking(move || {
                        let live = |text: String, seconds: u64| {
                            let id = id.clone();
                            let _ = thread.queue(move |this| this.workflow_live(&id, seq, text, seconds));
                        };
                        let outcome = run_check(&command, &folder.path, &live);
                        let _ = thread.queue(move |mut this| {
                            if this.workflows.live.get(&id).is_some_and(|l| l.0 == seq) {
                                this.as_mut().rust_mut().workflows.live.remove(&id);
                            }
                            this.workflow_event(&id, Event::Checked { seq, attempt, outcome });
                        });
                    });
                }
                Action::Tell { title, body, attention } => {
                    let key = self.workflow_key(&run).unwrap_or_default();
                    self.as_mut().tell_about_workflow(&key, &title, &body, attention);
                }
            }
        }
    }

    /// Runs or workflows changed: the views (and phones with a run's card
    /// open) read them again.
    fn workflows_changed(mut self: Pin<&mut Self>) {
        let next = self.workflows_revision + 1;
        self.as_mut().set_workflows_revision(next);
        crate::phone_bridge::workflows_changed();
    }

    /// A phone's call about runs: `workflow` (the run of session `key`, as
    /// its card shows it, or null), `workflows` (every run) and
    /// `workflowAction` (`id`, `action` as [`Self::workflow_action`]).
    pub(super) fn phone_workflow(mut self: Pin<&mut Self>, msg: &Value) -> Value {
        let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
        match msg["op"].as_str().unwrap_or_default() {
            "workflow" => {
                let card = self.workflow_for(&QString::from(&text("key"))).to_string();
                json!({ "run": serde_json::from_str::<Value>(&card).unwrap_or(Value::Null) })
            }
            "workflows" => json!({ "runs": serde_json::from_str::<Value>(&self.workflows_json().to_string()).unwrap_or_default() }),
            _ => {
                let id = text("id");
                if !self.workflows.runs.contains_key(&id) {
                    return json!({ "error": "That run isn't in harlo any more." });
                }
                self.as_mut().workflow_action(&QString::from(&id), &QString::from(&text("action")));
                let notice = self.notice.to_string();
                if self.notice_error && !notice.is_empty() {
                    return json!({ "error": notice });
                }
                json!({ "ok": true })
            }
        }
    }

    /// A running check's output so far: shown on its run's card.
    fn workflow_live(mut self: Pin<&mut Self>, id: &str, seq: u64, text: String, seconds: u64) {
        let mut this = self.as_mut().rust_mut();
        let Some(entry) = this.workflows.live.get_mut(id).filter(|l| l.0 == seq) else { return };
        if entry.1 == text && entry.2 == seconds {
            return;
        }
        *entry = (seq, text, seconds);
        self.as_mut().workflows_changed();
    }

    /// The live key of a run's implementation session, if it's open.
    fn workflow_key(&self, run: &Run) -> Option<String> {
        self.workflows.keys.iter().find(|(_, s)| s.run == run.id && s.kind == StageKind::Implement).map(|(k, _)| k.clone()).or_else(|| run.implementation.as_ref().and_then(|p| self.live_key_for_file(&p.path)))
    }

    /// A notification about a run (desktop and phone).
    fn tell_about_workflow(mut self: Pin<&mut Self>, key: &str, title: &str, body: &str, attention: bool) {
        let settings = harlo_core::notifications::Settings::load(&self.setting_str("notify"), &self.setting_str("notifications"));
        crate::phone_push::send(crate::phone_push::Alert {
            what: if attention { crate::phone_push::What::Failed } else { crate::phone_push::What::Done },
            key: key.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            badge: self.waiting_count(),
            quiet: settings.quiet.hushes_now(false),
        });
        if settings.enabled && !settings.quiet.hushes_now(false) {
            self.as_mut().deliver(&format!("workflow-{key}"), if attention { "error" } else { "completion" }, key, title, body, settings.sound);
        }
    }

    /// Makes a run's worktree (on its project's machine), then tells the run.
    fn workflow_worktree(self: Pin<&mut Self>, run: &Run, branch: String) {
        let id = run.id.clone();
        let Some((project, root)) = self.project_root(i32::try_from(run.project_id).unwrap_or(-1)) else {
            return self.workflow_event(&id, Event::WorktreeFailed("the project isn't open in harlo, or isn't a git repository".into()));
        };
        let folder = format!("{}-{}", git::branch_slug(&project.name), project.id);
        let host = harlo_core::remote::host_of(&root).and_then(|h| harlo_core::remote::host(&h));
        let slug = git::branch_slug(&branch);
        let local = self.worktree_base().join(&folder).join(&slug);
        let setup = project.setup.clone();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let made = (|| {
                let path = match &host {
                    Some(host) => {
                        let data = harlo_core::remote::run_ok(host, r#"printf '%s' "${XDG_DATA_HOME:-$HOME/.local/share}""#, Duration::from_secs(20))?;
                        harlo_core::remote::join(&host.name, &format!("{}/harlo/worktrees/{folder}/{slug}", data.trim()))
                    }
                    None => local,
                };
                let base = git::head_branch(&root);
                git::create_worktree(&root, &path, &branch, &base)?;
                let start = git::output_in(&path, &["rev-parse", "--verify", "-q", "HEAD"]).trim().to_owned();
                let path = if host.is_some() { path } else { path.canonicalize().unwrap_or(path) };
                Ok::<_, anyhow::Error>(workflow::Worktree { place: Place::of(&path), branch: branch.clone(), base, base_commit: start })
            })();
            let _ = thread.queue(move |mut this| match made {
                Ok(tree) => {
                    let rec = WorktreeRecord { path: tree.place.path.clone(), project_id: project.id, branch: tree.branch.clone(), base: tree.base.clone(), base_commit: tree.base_commit.clone(), removed: false };
                    if let Some(db) = this.db() {
                        let _ = db.upsert_worktree(&rec);
                    }
                    this.as_mut().reload_worktrees();
                    let path = tree.place.path.clone();
                    this.as_mut().workflow_event(&id, Event::WorktreeReady(tree));
                    this.run_setup(path, &setup);
                }
                Err(e) => this.workflow_event(&id, Event::WorktreeFailed(format!("{e:#}"))),
            });
        });
    }

    /// Starts a run's session (its implementation, or a reviewer) in its
    /// worktree with its first message.
    #[expect(clippy::too_many_arguments, reason = "one call site per kind of session; a struct would only rename them")]
    fn workflow_session(mut self: Pin<&mut Self>, run: &Run, seq: u64, kind: StageKind, agent: &Agent, title: &str, prompt: String, activate: bool) {
        let Some(tree) = run.worktree.clone() else { return };
        let Some(harness) = HarnessKind::parse(&agent.harness) else {
            return self.workflow_event(&run.id, Event::StartFailed { seq, why: format!("harlo doesn't know the agent {}", agent.harness) });
        };
        let key = self.as_mut().start_session_budget(harness, tree.place.path.clone(), &agent.model, "", activate, None);
        self.as_mut().rust_mut().workflows.keys.insert(key.clone(), WorkflowSession { run: run.id.clone(), seq, kind, report: None });
        if let Some(live) = registry::get(&key) {
            live.handle.send_raw(json!({ "type": "set_session_name", "name": title }));
        }
        if kind == StageKind::Implement && !run.approval.is_empty() {
            self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(&run.approval));
        }
        let first = FirstPrompt { text: prompt, attachments: Vec::new(), server: Some((agent.harness.clone(), agent.model.clone())), by_you: false };
        send_first_prompt(&key, first, Some(self.qt_thread()));
        self.as_mut().workflow_event(&run.id, Event::Started { seq });
    }

    /// Sends a run's message to a session it knows by its file (opened
    /// from it when it isn't running).
    fn workflow_prompt(mut self: Pin<&mut Self>, run: &Run, seq: u64, kind: StageKind, session: &Place, text: &str) {
        let key = self.live_key_for_file(&session.path).or_else(|| self.as_mut().make_live(&session.path.to_string_lossy()));
        let Some(live) = key.as_deref().and_then(registry::get) else {
            let why = format!("its session ({}) isn't listed by harlo", session.path.display());
            return self.workflow_event(&run.id, Event::StartFailed { seq, why });
        };
        let key = key.unwrap_or_default();
        // (Another session of the run's that waited on this kind of part
        // no longer does.)
        self.as_mut().rust_mut().workflows.keys.retain(|k, s| *k == key || s.run != run.id || s.kind != kind);
        self.as_mut().rust_mut().workflows.keys.insert(key.clone(), WorkflowSession { run: run.id.clone(), seq, kind, report: None });
        // (Its name again: one set while harlo closed mid-turn may not
        // have been kept.)
        live.handle.send_raw(json!({ "type": "set_session_name", "name": workflow_title(run, kind) }));
        // (Its approval mode again too: one set just before harlo stopped,
        // before its file was known, wasn't kept.)
        if kind == StageKind::Implement && !run.approval.is_empty() {
            self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(&run.approval));
        }
        let busy = self.live.get(&key).is_some_and(|e| e.info.status == harlo_core::SessionStatus::Busy);
        live.handle.prompt(text, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
        self.as_mut().workflow_event(&run.id, Event::Started { seq });
    }

    /// Whether a session calling harlo's tools serves a run (it's offered
    /// report_done).
    pub(super) fn serves_workflow(&self, key: &str) -> bool {
        self.workflows.keys.contains_key(key)
    }

    /// A run's session called report_done (or ask_parent): taken when it's
    /// for the attempt the run waits on, refused (with why) when not.
    pub(super) fn workflow_report(mut self: Pin<&mut Self>, call: &ToolCall<'_>, tool: &str) -> anyhow::Result<Value> {
        let Some((key, session)) = call.caller.as_ref().and_then(|c| self.workflows.keys.get(c).map(|s| (c.clone(), s.clone()))) else {
            return Err(anyhow::Error::msg("This session isn't part of a workflow."));
        };
        if tool != "report_done" {
            return Err(anyhow::Error::msg("A workflow's session has no session to ask: write your question as your reply and end your turn; the user answers it here."));
        }
        let report = harlo_core::orchestration::Report::parse(call.args)?;
        let Some(attempt) = report.attempt else {
            return Err(anyhow::Error::msg("Give the attempt your message named (attempt: N)."));
        };
        let run = self.workflows.runs.get(&session.run).ok_or_else(|| anyhow::Error::msg("Its workflow run is gone."))?;
        if let Err(why) = workflow::check_report(run, session.seq, attempt) {
            let id = session.run.clone();
            if let Some(mut r) = self.workflows.runs.get(&id).cloned() {
                r.log.push(workflow::Entry { at: now(), attempt: r.attempt, tone: Tone::Info, text: format!("Refused a report for attempt {attempt}: {why}") });
                self.as_mut().keep_workflow(r);
            }
            return Err(anyhow::Error::msg(why));
        }
        if let Some(s) = self.as_mut().rust_mut().workflows.keys.get_mut(&key) {
            s.report = Some((attempt, report));
        }
        Ok(json!({ "reported": true, "attempt": attempt, "next": "End your turn now: harlo takes your report when it ends." }))
    }

    /// A run's session's file is known: kept with the run (a reviewer's
    /// shown under its implementation session).
    pub(super) fn workflow_file_known(mut self: Pin<&mut Self>, key: &str) {
        let (Some(session), Some(file)) = (self.workflows.keys.get(key).cloned(), self.file_for_key(key)) else { return };
        crash_point(&format!("workflow-{}-unrecorded", if session.kind == StageKind::Review { "review" } else { "implement" }));
        if session.kind == StageKind::Review {
            let implementation = self.workflows.runs.get(&session.run).and_then(|r| r.implementation.clone());
            if let Some(parent) = implementation {
                let parent = parent.path.to_string_lossy().into_owned();
                self.as_mut().update_meta(key, |m| m.parent = parent);
            }
        }
        self.workflow_event(&session.run, Event::SessionKnown { seq: session.seq, session: Place::of(&file) });
    }

    /// A run's session's turn ended: its report (or that it didn't
    /// report) goes to the run. Not while it waits for you.
    pub(super) fn workflow_turn_ended(mut self: Pin<&mut Self>, key: &str, notes: &[super::Note]) {
        let Some(session) = self.workflows.keys.get(key).cloned() else { return };
        if self.live.get(key).is_some_and(|e| !e.requests.is_empty()) {
            return;
        }
        if self.workflows.runs.get(&session.run).and_then(|r| r.wait.as_ref()).is_none_or(|w| w.seq != session.seq) {
            return;
        }
        // (Its file, should it not be known yet.)
        if self.workflows.runs.get(&session.run).and_then(|r| r.wait.as_ref()).is_some_and(|w| w.session.is_none()) {
            self.as_mut().workflow_file_known(key);
        }
        if let Some(s) = self.as_mut().rust_mut().workflows.keys.get_mut(key) {
            s.report = None;
        }
        let event = match session.report {
            Some((attempt, report)) => Event::Report { seq: session.seq, attempt, ok: report.succeeded, summary: report.summary },
            None => {
                let error = notes.iter().find_map(|n| match n {
                    super::Note::Event(harlo_core::Notable::Error { message }) => Some(message.clone()),
                    _ => None,
                });
                Event::NoReport { seq: session.seq, error }
            }
        };
        self.workflow_event(&session.run, event);
    }

    /// A machine's sessions have been listed (`host` "" this one's): the
    /// runs waiting for them are picked up.
    pub(super) fn workflows_listed(mut self: Pin<&mut Self>, host: &str) {
        if !self.workflows.listed.iter().any(|h| h == host) {
            self.as_mut().rust_mut().workflows.listed.push(host.to_owned());
        }
        // (The first time: runs on machines that don't answer pause.)
        if host.is_empty() && self.workflows.unrecovered.iter().any(|id| self.workflows.runs.get(id).is_some_and(|r| !r.hosts().is_empty())) {
            let thread = self.qt_thread();
            crate::runtime().spawn(async move {
                // (HARLO_TEST_HOST_WAIT: seconds, for the workflow lab.)
                let wait = std::env::var("HARLO_TEST_HOST_WAIT").ok().and_then(|s| s.parse().ok()).map_or(HOST_WAIT, Duration::from_secs);
                tokio::time::sleep(wait).await;
                let _ = thread.queue(Workspace::workflows_unreachable);
            });
        }
        let ready: Vec<String> = self
            .workflows
            .unrecovered
            .iter()
            .filter(|id| self.workflows.runs.get(*id).is_some_and(|r| r.hosts().iter().chain(std::iter::once(&String::new())).all(|h| self.workflows.listed.contains(h))))
            .cloned()
            .collect();
        for id in ready {
            self.as_mut().rust_mut().workflows.unrecovered.retain(|u| *u != id);
            self.as_mut().recover_workflow(&id);
        }
    }

    /// Runs whose machines haven't answered since harlo opened: paused,
    /// saying so (picked up when their machine is listed, or on Resume).
    fn workflows_unreachable(mut self: Pin<&mut Self>) {
        for id in self.workflows.unrecovered.clone() {
            let Some(mut run) = self.workflows.runs.get(&id).cloned().filter(|r| r.state == State::Running) else { continue };
            let hosts = run.hosts().join(", ");
            let why = format!("harlo couldn't list the sessions on {hosts} since it opened, so it can't tell where the run was. It goes on once {hosts} answers (or Resume to look again)");
            run.state = State::Paused { why: why.clone(), next: workflow::Next::Recover };
            run.log.push(workflow::Entry { at: now(), attempt: run.attempt, tone: Tone::Warn, text: format!("Paused: {why}") });
            let key = self.workflow_key(&run).unwrap_or_default();
            self.as_mut().keep_workflow(run.clone());
            self.as_mut().tell_about_workflow(&key, "Workflow paused", &format!("{}: {why}", run.title()), true);
        }
    }

    /// Picks up a run after harlo opened again: reconnects to its sessions
    /// or pauses, saying why (`harlo_core::workflow::recover`).
    fn recover_workflow(mut self: Pin<&mut Self>, id: &str) {
        let Some(mut run) = self.workflows.runs.get(id).cloned() else { return };
        // (Paused while its machine didn't answer: it has now.)
        if matches!(run.state, State::Paused { next: workflow::Next::Recover, .. }) {
            run.state = State::Running;
            let text = if run.hosts().is_empty() { "Resumed: picking up what was under way." } else { "Its machine's sessions are listed: picking it up." };
            run.log.push(workflow::Entry { at: now(), attempt: run.attempt, tone: Tone::Info, text: text.into() });
        }
        let folder = run.worktree.as_ref().map(|w| w.place.clone());
        let found = |marker: &str| -> Option<Place> {
            let folder = folder.as_ref()?;
            self.sessions
                .values()
                .filter(|s| s.first_message.lines().next().is_some_and(|l| l.trim() == marker))
                .filter(|s| Place::of(&s.path).host == folder.host && s.cwd.starts_with(&folder.path))
                .max_by_key(|s| s.modified)
                .map(|s| Place::of(&s.path))
        };
        let actions = workflow::recover(&mut run, &found, now());
        self.as_mut().keep_workflow(run);
        self.as_mut().do_workflow(id, actions);
    }
}

/// What's done with a run's work from its card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    Merge,
    Publish,
    Remove,
}

impl Ending {
    fn doing(self) -> &'static str {
        match self {
            Self::Merge => "Merging",
            Self::Publish => "Pushing",
            Self::Remove => "Removing the worktree of",
        }
    }
}

/// The name of a run's session: its implementation's, or its reviewer's
/// ("Review: …", by its stage's name).
fn workflow_title(run: &Run, kind: StageKind) -> String {
    match (kind, run.current()) {
        (StageKind::Review, Some(stage)) => format!("{}: {} (attempt {})", stage.name, run.title(), run.attempt),
        _ => run.title(),
    }
}

fn stage_kind_name(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Implement => "implement",
        StageKind::Check => "check",
        StageKind::Review => "review",
    }
}

fn tone_name(tone: Tone) -> &'static str {
    match tone {
        Tone::Info => "info",
        Tone::Good => "good",
        Tone::Bad => "bad",
        Tone::Warn => "warn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_s_test_command_is_guessed_from_its_files() {
        let dir = std::env::temp_dir().join(format!("harlo-wf-guess-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(guess_test_command(&dir), "");
        std::fs::write(dir.join("Makefile"), "build:\n\tcc x.c\ntest:\n\t./t\n").unwrap();
        assert_eq!(guess_test_command(&dir), "make test");
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        assert_eq!(guess_test_command(&dir), "cargo test");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tests_here_say_their_status_and_output() {
        let dir = std::env::temp_dir();
        let seen = std::sync::Mutex::new(Vec::new());
        let failed = run_check("echo one; echo two >&2; sleep 1.2; exit 3", &dir, &|text, _| seen.lock().unwrap().push(text));
        assert_eq!((failed.exit, failed.passed()), (Some(3), false));
        assert!(failed.tail.contains("one") && failed.tail.contains("two"), "{}", failed.tail);
        assert!(seen.lock().unwrap().iter().any(|t| t == "one\ntwo"), "its output, as it ran: {:?}", seen.lock().unwrap());
        assert!(run_check("true", &dir, &|_, _| ()).passed());
        let gone = run_check("true", Path::new("ssh://no-such-host-harlo/x"), &|_, _| ());
        assert!(gone.error.is_some() && !gone.passed());
    }

    #[test]
    fn run_ids_differ() {
        let a = new_id();
        std::thread::sleep(Duration::from_millis(2));
        assert!(a.starts_with("wf-") && a.len() == 11 && a != new_id());
    }
}
