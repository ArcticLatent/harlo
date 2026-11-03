//! The shared plan: a session's steps for the task at hand, which the
//! agent works through and you edit (add, change, remove, reorder, skip)
//! at any time. With the plan on for a project (its settings), its
//! sessions get harlo's plan tools (`plan_read`, `plan_set`,
//! `plan_start`, `plan_done`) through harlo's MCP bridge, for every agent.
//!
//! A step finished (`plan_done`) is committed on its own (when the
//! project commits steps): harlo notes which files are modified when the
//! step starts, and commits what changed since, only when that's provably
//! the step's own work: none of those files had changes from before the
//! step, the branch didn't move, and no other session worked there
//! meanwhile (harlo checks that). Otherwise it says why and leaves the
//! changes for you. A finished step can then be reviewed by another
//! agent.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Pending,
    Active,
    Done,
    Skipped,
}

/// The repository when a step started: its HEAD and modified files (path
/// → content hash, "" for deleted).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Snapshot {
    pub head: String,
    pub dirty: BTreeMap<String, String>,
    /// Unix seconds.
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Step {
    pub id: u32,
    pub title: String,
    pub detail: String,
    pub status: Status,
    /// What the agent says it did (or why it skipped it).
    pub summary: String,
    /// Its commit, when harlo made one.
    pub commit: String,
    /// Why harlo didn't commit it, when it didn't.
    pub commit_note: String,
    /// Added or last changed by you (the agent sees that).
    pub yours: bool,
    /// Runs at the same time as the step before it (in a run with helpers:
    /// [`Plan::stages`]).
    pub alongside: bool,
    /// In a run with helpers: the session doing it, and the branch of the
    /// worktree it did it in ("" for the session's own folder).
    pub helper: String,
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub next: u32,
}

impl Plan {
    pub fn parse(text: &str) -> Plan {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    fn new_id(&mut self) -> u32 {
        self.next = self.next.max(self.steps.iter().map(|s| s.id).max().unwrap_or(0)) + 1;
        self.next
    }

    pub fn step(&self, id: u32) -> Option<&Step> {
        self.steps.iter().find(|s| s.id == id)
    }

    fn step_mut(&mut self, id: u32) -> Result<&mut Step, String> {
        self.steps.iter_mut().find(|s| s.id == id).ok_or_else(|| format!("There's no step {id}; plan_read lists them."))
    }

    /// What the agent reads (and phones and the dock show).
    pub fn json(&self) -> Value {
        let steps: Vec<Value> = self
            .steps
            .iter()
            .map(|s| {
                let mut v = json!({ "id": s.id, "title": s.title, "status": s.status });
                for (k, val) in [("detail", &s.detail), ("summary", &s.summary), ("commit", &s.commit), ("commitNote", &s.commit_note)] {
                    if !val.is_empty() {
                        v[k] = json!(val);
                    }
                }
                if s.yours {
                    v["editedByUser"] = json!(true);
                }
                if s.alongside {
                    v["alongside"] = json!(true);
                }
                for (k, val) in [("helper", &s.helper), ("branch", &s.branch)] {
                    if !val.is_empty() {
                        v[k] = json!(val);
                    }
                }
                v
            })
            .collect();
        json!({ "steps": steps })
    }

    /// The agent's steps from here on: the ones not started are replaced
    /// (the done, skipped and active ones stay).
    /// (title, detail, alongside the one before.)
    pub fn set(&mut self, steps: &[(String, String, bool)]) -> Result<(), String> {
        let steps: Vec<&(String, String, bool)> = steps.iter().filter(|(t, ..)| !t.trim().is_empty()).collect();
        if steps.is_empty() {
            return Err("Give at least one step with a title.".into());
        }
        self.steps.retain(|s| s.status != Status::Pending);
        for (title, detail, alongside) in steps {
            let id = self.new_id();
            self.steps.push(Step { id, title: title.trim().to_owned(), detail: detail.trim().to_owned(), alongside: *alongside, ..Default::default() });
        }
        Ok(())
    }

    /// The steps in the order they run with helpers: each stage's steps at
    /// the same time (a step `alongside` joins the stage of the one before),
    /// a stage once the one before has finished.
    pub fn stages(&self) -> Vec<Vec<u32>> {
        let mut stages: Vec<Vec<u32>> = Vec::new();
        for s in &self.steps {
            match stages.last_mut() {
                Some(last) if s.alongside => last.push(s.id),
                _ => stages.push(vec![s.id]),
            }
        }
        stages
    }

    fn finished(&self, id: u32) -> bool {
        self.step(id).is_some_and(|s| matches!(s.status, Status::Done | Status::Skipped))
    }

    /// The stage that runs now (its steps), None when every step has
    /// finished.
    pub fn current_stage(&self) -> Option<Vec<u32>> {
        self.stages().into_iter().find(|stage| !stage.iter().all(|id| self.finished(*id)))
    }

    /// What a run starts next: the current stage's steps not started yet.
    pub fn ready(&self) -> Vec<u32> {
        self.current_stage().unwrap_or_default().into_iter().filter(|id| self.step(*id).is_some_and(|s| s.status == Status::Pending)).collect()
    }

    /// The steps before the current stage that were done in worktrees:
    /// (title, branch), for the next helper to merge.
    pub fn branches_before(&self, stage: &[u32]) -> Vec<(String, String)> {
        let first = stage.first().and_then(|id| self.steps.iter().position(|s| s.id == *id)).unwrap_or(0);
        self.steps[..first].iter().filter(|s| s.status == Status::Done && !s.branch.is_empty()).map(|s| (s.title.clone(), s.branch.clone())).collect()
    }

    /// A run gives a step to a helper (`helper`: its session, `branch`: its
    /// worktree's, "" for none). Steps of a stage run at once.
    pub fn hand_out(&mut self, id: u32, helper: &str, branch: &str) -> Result<(), String> {
        let step = self.step_mut(id)?;
        if step.status != Status::Pending {
            return Err(format!("Step {id} isn't waiting to start."));
        }
        step.status = Status::Active;
        step.helper = helper.to_owned();
        step.branch = branch.to_owned();
        Ok(())
    }

    /// A helper didn't finish its step: it waits to be run again.
    pub fn put_back(&mut self, id: u32, why: &str) -> Result<(), String> {
        let step = self.step_mut(id)?;
        step.status = Status::Pending;
        step.helper.clear();
        step.summary = why.trim().to_owned();
        Ok(())
    }

    /// Starts a step (`snapshot`: the repository now, None outside one).
    pub fn start(&mut self, id: u32, snapshot: Option<Snapshot>) -> Result<(), String> {
        let step = self.step_mut(id)?;
        if matches!(step.status, Status::Done | Status::Skipped) {
            return Err(format!("Step {id} is already finished."));
        }
        step.status = Status::Active;
        step.start = snapshot;
        // One step at a time.
        for s in self.steps.iter_mut().filter(|s| s.id != id && s.status == Status::Active) {
            s.status = Status::Pending;
            s.start = None;
        }
        Ok(())
    }

    /// Finishes a step (`skipped`: not done, `summary` says why).
    pub fn finish(&mut self, id: u32, summary: &str, skipped: bool) -> Result<&mut Step, String> {
        let step = self.step_mut(id)?;
        step.status = if skipped { Status::Skipped } else { Status::Done };
        step.summary = summary.trim().to_owned();
        Ok(step)
    }

    // ---- your edits ----

    pub fn add(&mut self, title: &str, detail: &str, after: Option<u32>) -> u32 {
        let id = self.new_id();
        let step = Step { id, title: title.trim().to_owned(), detail: detail.trim().to_owned(), yours: true, ..Default::default() };
        let at = after.and_then(|a| self.steps.iter().position(|s| s.id == a)).map_or(self.steps.len(), |i| i + 1);
        self.steps.insert(at, step);
        id
    }

    pub fn edit(&mut self, id: u32, title: &str, detail: &str) -> Result<(), String> {
        let step = self.step_mut(id)?;
        step.title = title.trim().to_owned();
        step.detail = detail.trim().to_owned();
        step.yours = true;
        Ok(())
    }

    pub fn remove(&mut self, id: u32) {
        self.steps.retain(|s| s.id != id);
    }

    /// Moves a step up (-1) or down (+1).
    pub fn shift(&mut self, id: u32, by: i32) {
        let Some(i) = self.steps.iter().position(|s| s.id == id) else { return };
        let j = i as i64 + by as i64;
        if j >= 0 && (j as usize) < self.steps.len() {
            self.steps.swap(i, j as usize);
        }
    }

    /// You make a step run at the same time as the one before, or after it.
    pub fn set_alongside(&mut self, id: u32, alongside: bool) -> Result<(), String> {
        let step = self.step_mut(id)?;
        step.alongside = alongside;
        step.yours = true;
        Ok(())
    }

    /// You mark a step skipped, or not.
    pub fn set_skipped(&mut self, id: u32, skipped: bool) -> Result<(), String> {
        let step = self.step_mut(id)?;
        step.status = if skipped { Status::Skipped } else { Status::Pending };
        step.yours = true;
        Ok(())
    }
}

// ---- the repository ----

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run_in(dir, args, None)
}

/// The repository `folder` is in (here or on its host).
fn top(folder: &Path) -> Result<PathBuf, String> {
    crate::git::repo_root(folder).ok_or_else(|| "It isn't in a git repository.".to_owned())
}

/// Modified files (from the repository's root), with their content's hash.
fn dirty(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let status = git(root, &["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
    let mut paths = Vec::new();
    let mut entries = status.split('\0').filter(|e| !e.is_empty());
    while let Some(e) = entries.next() {
        let (code, path) = (e.get(..2).unwrap_or(""), e.get(3..).unwrap_or(""));
        // A rename's old name follows it.
        if (code.starts_with('R') || code.starts_with('C'))
            && let Some(old) = entries.next()
        {
            paths.push((old.to_owned(), false));
        }
        paths.push((path.to_owned(), true));
    }
    // (Their hashes in one go: on another machine, one command.)
    let check: Vec<String> = paths.iter().filter(|(_, c)| *c).map(|(p, _)| p.clone()).collect();
    let present: Vec<String> = check.iter().zip(crate::git::files_among(root, &check)).filter(|(_, f)| *f).map(|(p, _)| p.clone()).collect();
    let hashes: Vec<String> = if present.is_empty() {
        Vec::new()
    } else {
        let input = present.iter().map(|p| format!("{p}\n")).collect::<String>();
        git_input(root, &["hash-object", "--stdin-paths"], input.as_bytes())?.lines().map(|h| h.trim().to_owned()).collect()
    };
    let hash_of: HashMap<&str, &str> = present.iter().map(String::as_str).zip(hashes.iter().map(String::as_str)).collect();
    Ok(paths.iter().map(|(p, _)| (p.clone(), hash_of.get(p.as_str()).copied().unwrap_or_default().to_owned())).collect())
}

fn git_input(dir: &Path, args: &[&str], input: &[u8]) -> Result<String, String> {
    crate::git::run_in(dir, args, Some(input))
}

/// The repository a step starts in (None outside one).
pub fn snapshot(folder: &Path, now: i64) -> Option<Snapshot> {
    let root = top(folder).ok()?;
    Some(Snapshot { head: git(&root, &["rev-parse", "HEAD"]).map(|h| h.trim().to_owned()).unwrap_or_default(), dirty: dirty(&root).ok()?, at: now })
}

/// The files a step changed since `start`, or why they aren't provably its
/// own.
pub fn changed_since(folder: &Path, start: &Snapshot) -> Result<Vec<String>, String> {
    let root = top(folder)?;
    let head = git(&root, &["rev-parse", "HEAD"]).map(|h| h.trim().to_owned()).unwrap_or_default();
    if head != start.head {
        return Err("The branch moved during the step (a commit or a checkout), so harlo can't tell its changes apart.".into());
    }
    let now = dirty(&root)?;
    let mut changed = Vec::new();
    let mut mixed = Vec::new();
    for (path, hash) in &now {
        match start.dirty.get(path) {
            None => changed.push(path.clone()),
            Some(before) if before != hash => mixed.push(path.clone()),
            Some(_) => {}
        }
    }
    // Put back as they were in HEAD during the step: changed too, but
    // nothing of them to commit unless they had changes before.
    for path in start.dirty.keys().filter(|p| !now.contains_key(*p)) {
        mixed.push(path.clone());
    }
    if !mixed.is_empty() {
        mixed.sort();
        return Err(format!("{} had changes from before the step, so harlo can't tell the step's own apart.", list_words(&mixed)));
    }
    Ok(changed)
}

fn list_words(paths: &[String]) -> String {
    match paths {
        [one] => one.clone(),
        [first, rest @ ..] if rest.len() <= 2 => format!("{first} and {}", rest.join(", ")),
        [first, second, rest @ ..] => format!("{first}, {second} and {} more", rest.len()),
        [] => String::new(),
    }
}

/// Commits those files alone (other staged changes stay as they are):
/// the commit's hash.
pub fn commit(folder: &Path, files: &[String], subject: &str, body: &str) -> Result<String, String> {
    let root = top(folder)?;
    let mut add = vec!["add", "-A", "--"];
    add.extend(files.iter().map(String::as_str));
    git(&root, &add)?;
    let mut args = vec!["commit", "-q", "-m", subject];
    if !body.trim().is_empty() {
        args.extend(["-m", body.trim()]);
    }
    args.push("--");
    args.extend(files.iter().map(String::as_str));
    git(&root, &args)?;
    Ok(git(&root, &["rev-parse", "HEAD"])?.trim().to_owned())
}

/// What harlo tells agents in a project with the plan on.
pub const INSTRUCTIONS: &str = "This project uses a shared plan, which you work through and the user edits. For a task of three or more real steps, write the plan first with plan_set (each step one verifiable piece of work, outcome-named), then for each step: plan_read (the user may have added, changed, reordered or skipped steps; their edits win), plan_start, do the work, plan_done with a one-line summary. harlo commits each finished step on its own when its changes are provably the step's, so finish one step before starting the next, and don't commit yourself. Mark a step alongside when it doesn't need the step before it: the user can run the plan with helpers (a session per step), and alongside steps then run at the same time. For a small task, no plan is needed.";

/// What a helper is asked for a step of a run (before the helper brief):
/// the step, where it stands in the plan, what to merge first, and whether
/// to commit.
pub fn step_prompt(plan: &Plan, id: u32, merge: &[(String, String)], commit: bool, worktree: bool) -> String {
    let Some(step) = plan.step(id) else { return String::new() };
    let n = plan.steps.iter().position(|s| s.id == id).map_or(0, |i| i + 1);
    let mut text = format!("Do step {n} of {} of a plan: {}", plan.steps.len(), step.title);
    if !step.detail.is_empty() {
        text.push_str(&format!("\n\n{}", step.detail));
    }
    let done: Vec<String> = plan.steps.iter().filter(|s| s.status == Status::Done && s.id != id).map(|s| format!("- {}{}", s.title, if s.summary.is_empty() { String::new() } else { format!(": {}", s.summary) })).collect();
    if !done.is_empty() {
        text.push_str(&format!("\n\nDone already:\n{}", done.join("\n")));
    }
    let after: Vec<String> = plan.steps.iter().skip(n).filter(|s| s.status == Status::Pending).map(|s| format!("- {}", s.title)).collect();
    if !after.is_empty() {
        text.push_str(&format!("\n\nOther steps (not yours; leave them):\n{}", after.join("\n")));
    }
    if !merge.is_empty() {
        let list: Vec<String> = merge.iter().map(|(title, branch)| format!("- {branch} ({title})")).collect();
        text.push_str(&format!("\n\nEarlier steps were done on these branches: merge them into yours first (git merge), resolving any conflicts:\n{}", list.join("\n")));
    }
    text.push_str(if worktree {
        "\n\nYou work in a worktree of your own, while other steps run at the same time. When the step works, commit it, with the step as the message."
    } else if commit {
        "\n\nWhen the step works, commit it, with the step as the message."
    } else {
        "\n\nDon't commit: the user does."
    });
    text
}

/// What a helper said its outcome was in words, when it had no harlo
/// tools ("Outcome: succeeded" / "Outcome: failed"): Some(succeeded).
pub fn outcome_in(reply: &str) -> Option<bool> {
    let said = reply.lines().rev().find_map(|l| {
        let l = l.trim().trim_start_matches(['*', '_', '-', ' ']).to_lowercase();
        l.strip_prefix("outcome:").map(|o| o.split_whitespace().next().unwrap_or_default().trim_matches(['*', '_', '.', ',', ';']).to_owned())
    })?;
    match said.as_str() {
        "succeeded" | "success" | "done" => Some(true),
        "failed" | "failure" => Some(false),
        _ => None,
    }
}

/// The plan tools, as MCP lists them.
pub fn tools(commits: bool) -> Value {
    let s = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    let done = if commits {
        "Finishes the active step, with a one-line summary of what it did. harlo then commits the step's changes on their own and says so (or why it couldn't)."
    } else {
        "Finishes the active step, with a one-line summary of what it did."
    };
    json!([
        {
            "name": "plan_read",
            "description": format!("The shared plan of this session: its steps, with their ids and statuses (pending, active, done, skipped), and which ones the user edited. Read it before each step. {INSTRUCTIONS}"),
            "inputSchema": s(json!({}), &[]),
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "plan_set",
            "description": "Writes the steps still to do: the plan's steps not started yet are replaced by these (finished and active ones stay).",
            "inputSchema": s(json!({
                "steps": { "type": "array", "items": { "type": "object", "properties": {
                    "title": { "type": "string", "description": "The step's outcome, short." },
                    "detail": { "type": "string", "description": "How, or what to watch for (optional)." },
                    "alongside": { "type": "boolean", "description": "It can be done at the same time as the step before it (independent of it), when the plan runs with helpers (default false)." }
                }, "required": ["title"] } }
            }), &["steps"]),
        },
        {
            "name": "plan_start",
            "description": "Starts a step (one at a time): call it before working on the step.",
            "inputSchema": s(json!({ "id": { "type": "integer" } }), &["id"]),
        },
        {
            "name": "plan_done",
            "description": done,
            "inputSchema": s(json!({
                "id": { "type": "integer" },
                "summary": { "type": "string", "description": "What the step did, in a line (or why it was skipped)." },
                "skipped": { "type": "boolean", "description": "It wasn't done (default false)." }
            }), &["id", "summary"]),
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(p: &Plan) -> Vec<(u32, &str, Status)> {
        p.steps.iter().map(|s| (s.id, s.title.as_str(), s.status.clone())).collect()
    }

    #[test]
    fn the_agent_and_you_edit_the_plan() {
        let mut p = Plan::default();
        assert!(p.set(&[(" ".into(), String::new(), false)]).is_err());
        p.set(&[("Parse".into(), String::new(), false), ("Store".into(), "in sqlite".into(), false), ("Show".into(), String::new(), false)]).unwrap();
        p.start(1, None).unwrap();
        p.finish(1, "parses", false).unwrap();
        p.start(2, None).unwrap();
        // You add one after "Store", change "Show", move it up.
        let yours = p.add("Test it", "", Some(2));
        assert_eq!(yours, 4);
        p.edit(3, "Show in the UI", "").unwrap();
        p.shift(3, -1);
        assert_eq!(steps(&p), [(1, "Parse", Status::Done), (2, "Store", Status::Active), (3, "Show in the UI", Status::Pending), (4, "Test it", Status::Pending)]);
        // The agent rewrites what's left; the finished and active stay.
        p.set(&[("Show".into(), String::new(), false)]).unwrap();
        assert_eq!(steps(&p), [(1, "Parse", Status::Done), (2, "Store", Status::Active), (5, "Show", Status::Pending)]);
        // Starting another leaves one active.
        p.start(5, None).unwrap();
        assert_eq!(p.step(2).unwrap().status, Status::Pending);
        assert!(p.start(1, None).is_err());
        assert!(p.start(9, None).unwrap_err().contains("no step 9"));
        p.set_skipped(2, true).unwrap();
        p.remove(5);
        let v = Plan::parse(&p.to_text()).json();
        assert_eq!(v, json!({ "steps": [
            { "id": 1, "title": "Parse", "status": "done", "summary": "parses" },
            { "id": 2, "title": "Store", "status": "skipped", "detail": "in sqlite", "editedByUser": true },
        ] }));
    }

    #[test]
    fn a_run_goes_stage_by_stage() {
        let mut p = Plan::default();
        let t = |s: &str, along: bool| (s.to_owned(), String::new(), along);
        p.set(&[t("Schema", false), t("API", false), t("UI", true), t("Docs", true), t("Ship", false)]).unwrap();
        assert_eq!(p.stages(), [vec![1], vec![2, 3, 4], vec![5]]);
        assert_eq!(p.ready(), [1]);
        p.hand_out(1, "live:1", "").unwrap();
        assert!(p.ready().is_empty() && p.hand_out(1, "live:9", "").is_err());
        p.finish(1, "made it", false).unwrap();
        // The next stage: all at once, in worktrees.
        assert_eq!(p.ready(), [2, 3, 4]);
        for (id, b) in [(2, "plan-api"), (3, "plan-ui"), (4, "plan-docs")] {
            p.hand_out(id, "live:x", b).unwrap();
        }
        p.finish(2, "", false).unwrap();
        p.set_skipped(4, true).unwrap();
        // One failed: back to waiting, and run again.
        p.put_back(3, "Failed: no tests").unwrap();
        assert_eq!(p.ready(), [3]);
        p.hand_out(3, "live:y", "plan-ui2").unwrap();
        p.finish(3, "", false).unwrap();
        assert_eq!(p.current_stage(), Some(vec![5]));
        // The last stage merges what the worktrees did (the skipped one: nothing).
        assert_eq!(p.branches_before(&[5]), [("API".to_owned(), "plan-api".to_owned()), ("UI".to_owned(), "plan-ui2".to_owned())]);
        let prompt = step_prompt(&p, 5, &p.branches_before(&[5]), true, false);
        assert!(prompt.starts_with("Do step 5 of 5 of a plan: Ship") && prompt.contains("- Schema: made it") && prompt.contains("- plan-api (API)") && prompt.ends_with("with the step as the message."), "{prompt}");
        assert!(step_prompt(&p, 5, &[], false, false).ends_with("Don't commit: the user does."));
        p.finish(5, "", false).unwrap();
        assert!(p.current_stage().is_none() && p.ready().is_empty());
        // You move a step to run with the one before.
        p.set_alongside(5, true).unwrap();
        assert_eq!(p.stages().last().unwrap(), &vec![2, 3, 4, 5]);
    }

    #[test]
    fn reads_an_outcome_said_in_words() {
        assert_eq!(outcome_in("Did it.\n\n**Outcome: succeeded**"), Some(true));
        assert_eq!(outcome_in("Outcome: failed. Tests don't build.\nmore"), Some(false));
        assert_eq!(outcome_in("Outcome: maybe"), None);
        assert_eq!(outcome_in("- Outcome: Failed"), Some(false));
        assert_eq!(outcome_in("no outcome here"), None);
    }

    #[test]
    fn commits_a_steps_own_changes() {
        let dir = std::env::temp_dir().join(format!("harlo-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let g = |args: &[&str]| git(&dir, args).unwrap();
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@example.com"]);
        g(&["config", "user.name", "T"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        std::fs::write(dir.join("old.txt"), "old\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-q", "-m", "start"]);
        // Your own change before the step stays out of it.
        std::fs::write(dir.join("notes.txt"), "mine\n").unwrap();
        let start = snapshot(&dir.join("src"), 1).unwrap();
        assert_eq!(start.dirty.keys().collect::<Vec<_>>(), ["notes.txt"]);
        std::fs::write(dir.join("a.txt"), "a2\n").unwrap();
        std::fs::write(dir.join("src/new.rs"), "fn x() {}\n").unwrap();
        std::fs::remove_file(dir.join("old.txt")).unwrap();
        let files = changed_since(&dir, &start).unwrap();
        assert_eq!(files, ["a.txt", "old.txt", "src/new.rs"]);
        let sha = commit(&dir, &files, "Store", "stores it").unwrap();
        assert_eq!(g(&["log", "-1", "--format=%s%n%b"]).trim(), "Store\nstores it");
        assert_eq!(g(&["show", "--name-only", "--format=", &sha]).trim(), "a.txt\nold.txt\nsrc/new.rs");
        assert_eq!(g(&["status", "--porcelain"]).trim(), "?? notes.txt");

        // Changing a file that had changes before: not provably the step's.
        let start = snapshot(&dir, 2).unwrap();
        std::fs::write(dir.join("notes.txt"), "mine, then the agent's\n").unwrap();
        assert!(changed_since(&dir, &start).unwrap_err().starts_with("notes.txt had changes from before"));
        // The branch moved.
        let start = snapshot(&dir, 3).unwrap();
        g(&["commit", "-q", "--allow-empty", "-m", "meanwhile"]);
        assert!(changed_since(&dir, &start).unwrap_err().contains("branch moved"));
        assert!(snapshot(&std::env::temp_dir().join("harlo-no-repo-here"), 0).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same on another machine (`HARLO_TEST_SSH`, e.g. "ssh myhost").
    #[test]
    #[ignore]
    fn commits_a_steps_own_changes_there() {
        let args = crate::remote::parse_ssh_command(&std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH")).unwrap();
        let host = crate::remote::Host { name: "t".into(), ssh: args };
        crate::remote::set_hosts(vec![host.clone()]);
        let there = format!("{}/harlo-plan-test-{}", crate::remote::home(&host).unwrap(), std::process::id());
        let sh = |script: &str| {
            let out = crate::remote::run(&host, &format!("cd {} && {script}", crate::remote::quote(&there)), std::time::Duration::from_secs(30)).unwrap();
            assert!(out.status.success(), "{script}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        };
        crate::remote::run(&host, &format!("mkdir -p {}/src", crate::remote::quote(&there)), std::time::Duration::from_secs(30)).unwrap();
        let dir = crate::remote::join("t", &there);
        sh("git init -q && git config user.email t@example.com && git config user.name T && echo a > a.txt && echo old > old.txt && echo s > 'with space.txt' && git add . && git commit -q -m start");
        sh("echo mine > notes.txt");
        let start = snapshot(&dir.join("src"), 1).unwrap();
        assert_eq!(start.dirty.keys().collect::<Vec<_>>(), ["notes.txt"]);
        assert!(!start.dirty["notes.txt"].is_empty());
        sh("echo a2 > a.txt && echo 'fn x() {}' > src/new.rs && rm old.txt && echo s2 > 'with space.txt'");
        let files = changed_since(&dir, &start).unwrap();
        assert_eq!(files, ["a.txt", "old.txt", "src/new.rs", "with space.txt"]);
        let sha = commit(&dir, &files, "Store", "stores it").unwrap();
        assert_eq!(sh("git log -1 --format=%s%n%b"), "Store\nstores it");
        assert_eq!(sh(&format!("git show --name-only --format= {sha}")), "a.txt\nold.txt\nsrc/new.rs\nwith space.txt");
        assert_eq!(sh("git status --porcelain"), "?? notes.txt");
        let start = snapshot(&dir, 2).unwrap();
        sh("echo 'mine, then the agent' > notes.txt");
        assert!(changed_since(&dir, &start).unwrap_err().starts_with("notes.txt had changes from before"));
        sh(&format!("rm -rf {}", crate::remote::quote(&there)));
    }
}
