//! The loop: send the history, run the tools the model calls, send their
//! results, until it answers without calling any.
//!
//! The history is pi's messages, each written to the session log as it's
//! made; every request's input is derived from them, so a resumed session
//! sends what a running one would. Tools that only look run at once; the
//! rest one after another, in the order called.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, bail};
use futures_util::future::{BoxFuture, join_all};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::providers::{self, Api};
use crate::session::{Log, Past, now_ms};
use crate::tools::{self, Tools};
use crate::{chat, compact, responses, stubs};

/// Rounds of tool calls in one run, at most.
const MAX_ROUNDS: usize = 200;

/// Rounds of tool calls a run may take (`HARLO_AGENT_MAX_ROUNDS` sets it,
/// to try it).
fn max_rounds() -> usize {
    std::env::var("HARLO_AGENT_MAX_ROUNDS").ok().and_then(|v| v.trim().parse().ok()).filter(|n| *n > 0).unwrap_or(MAX_ROUNDS)
}

/// The round a run is told to wrap up at: four fifths of the way, when
/// the cap isn't tiny.
fn wrap_up_round(max: usize) -> Option<usize> {
    (max >= 5).then(|| max * 4 / 5)
}

/// Told (hidden) when a run nears its last round.
const ROUNDS_NOTE: &str = "[harlo] You're near the most rounds of tool calls one turn may take: a few are left. Finish what you can now; if the task isn't done, end your turn with what you did and what's left, so it can go on from there.";

/// In place of each call in a reply cut off at the length limit.
const CUT_OFF: &str = "Not run: your reply was cut off at the length limit before this call was complete. Make the call again, smaller (write a big file in parts, or change it with edit).";

/// Left in the history when the user stops a turn.
const STOPPED_NOTE: &str = "<turn_stopped>The user stopped this turn. A call marked interrupted was stopped partway and may have partly run: check its effects before relying on them.</turn_stopped>";
const INTERRUPTED: &str = "Interrupted: the user stopped the turn while this call ran. It may have partly run.";
const NOT_RUN: &str = "Not run: the user stopped the turn before this call started.";
/// Told a helper (once a run) that would end its turn without reporting.
const REPORT_REMINDER: &str = "[harlo] Before you end your turn: you're a helper, and the session that started you is waiting for your result. If you're done or can't go on, call report_done (outcome succeeded or failed, a three-sentence summary, the files you changed). If you're waiting on an answer, call ask_parent (your parent) or request_approval (the user) instead. Then end your turn.";
/// harlo's tools a helper may end its turn with: it reported, or it waits
/// for its parent or the user.
const HAND_OVERS: [&str; 4] = ["report_done", "ask_parent", "request_approval", "propose_helpers"];

/// How a run ended.
#[derive(Debug, PartialEq)]
pub enum Ended {
    Done,
    /// The user stopped it.
    Stopped,
}

/// What a run shows, as it happens (pi's events, roughly).
pub enum Shown<'a> {
    MessageStart(&'a Value),
    /// A block of the reply streaming: pi's `assistantMessageEvent`
    /// (`text_delta`, `thinking_start`, `toolcall_end`, …).
    Stream(&'a Value),
    /// A message, complete (the reply's final form).
    MessageEnd(&'a Value),
    /// The reply that started streaming is dropped (it's sent again, or
    /// it failed): it ends as nothing, and isn't kept.
    Withdrawn,
    /// The session's model changed by itself (`provider/model`): it went
    /// on with the fallback, or back to the plan's.
    ModelChanged(&'a str),
    ToolStart { call_id: &'a str, name: &'a str, args: &'a Value },
    ToolEnd { call_id: &'a str, name: &'a str, text: &'a str, failed: bool },
    Usage { model: &'a str, usage: &'a Value },
    /// The messages waiting to be delivered changed.
    Queue { steering: &'a [String], follow_up: &'a [String] },
    /// A compaction starting (`reason`: threshold, overflow or manual).
    CompactionStart { reason: &'a str },
    /// …and ending: its result, or why it failed (neither: stopped).
    CompactionEnd { reason: &'a str, result: Option<&'a Value>, error: Option<&'a str>, will_retry: bool },
    /// The plan's limits, as a response gave them: `{ provider, account,
    /// windows }`.
    Limits(&'a Value),
    /// Old tool outputs set aside (see `stubs`): how many, the tokens that
    /// frees, the context's size about now.
    SetAside { count: usize, saved: u64, after: u64 },
    /// Something the user should know ("warning", "info"): an MCP server
    /// that didn't start, say.
    Notice { level: &'a str, text: &'a str },
}

/// How a compaction went.
pub enum Compacted {
    /// The summary's entry: summary, firstKeptEntryId, tokensBefore,
    /// estimatedTokensAfter.
    Done(Value),
    /// Too little to summarize yet.
    Nothing,
    Stopped,
    Failed(String),
}

/// Decides whether a call that changes something may run: `Err` says why
/// not (the call's result).
pub trait Gate: Send + Sync {
    fn check(&self, name: &str, args: &Value) -> BoxFuture<'static, Result<(), String>>;
    /// Asks the user yes or no, in every approval mode (`title` as an
    /// approval's: "Allow tool: …\n<why>"; `message`: the call).
    fn confirm(&self, title: &str, message: &str) -> BoxFuture<'static, bool>;
    /// Puts questions to the user (`ask`'s, as harlo's card takes them):
    /// their answers, or None when they dismissed them.
    fn ask(&self, questions: Value) -> BoxFuture<'static, Option<Value>>;
}

/// What a launch may set for a session (the same for `-p` and RPC).
#[derive(Clone, Debug, Default)]
pub struct Launch {
    /// `--task-model`: what subagents run on when a task doesn't say.
    pub task_model: Option<String>,
    /// `--budget`: the most tokens the session may use.
    pub budget: Option<u64>,
    /// `--fallback`: what it goes on with when the plan's used up.
    pub fallback: Option<String>,
}

/// A session's token budget: tokens sent and received, not counting those
/// the cache served, its subagents' included (they share it).
pub struct Budget {
    pub limit: u64,
    pub spent: std::sync::atomic::AtomicU64,
}

impl Budget {
    /// Why no more requests may go, once it's used.
    fn used(&self) -> Option<String> {
        let spent = self.spent.load(std::sync::atomic::Ordering::Relaxed);
        (spent >= self.limit).then(|| {
            format!(
                "This session used its token budget of {} ({} tokens sent and received, not counting those the cache served), so it makes no more requests.",
                tokens(self.limit),
                tokens(spent)
            )
        })
    }
}

/// `412k`, `1.2m`.
pub fn tokens(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{}k", n / 1_000),
        _ => format!("{:.1}m", n as f64 / 1e6).replace(".0m", "m"),
    }
}

/// `400k`, `2m`, `1.5m`, `400000`.
pub fn parse_tokens(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase().replace(['_', ','], "");
    let (num, times) = match s.strip_suffix('k') {
        Some(n) => (n.to_owned(), 1e3),
        None => match s.strip_suffix('m') {
            Some(n) => (n.to_owned(), 1e6),
            None => (s.clone(), 1.0),
        },
    };
    let v: f64 = num.trim().parse().ok()?;
    (v > 0.0).then(|| (v * times).round() as u64)
}

impl Launch {
    /// Takes `arg` (and its value, from `rest`) if it's one of these.
    pub fn take(&mut self, arg: &str, rest: &mut impl Iterator<Item = String>) -> anyhow::Result<bool> {
        let mut value = |name: &str| rest.next().filter(|v| !v.is_empty()).ok_or_else(|| anyhow::anyhow!("{name} needs a value"));
        match arg {
            "--task-model" => self.task_model = Some(crate::providers::full(&value(arg)?)),
            "--fallback" => self.fallback = Some(crate::providers::full(&value(arg)?)),
            "--budget" => {
                let v = value(arg)?;
                self.budget = Some(parse_tokens(&v).ok_or_else(|| anyhow::anyhow!("--budget takes tokens, as 400k or 2m (not {v})"))?);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

/// harlo, when it started this agent (`--harlo-tools`): its tools (start
/// other sessions, report to a parent, the project's plan and specs)
/// called through it. `_tools` and `_instructions` say what they are.
pub trait Harlo: Send + Sync {
    fn call(&self, tool: &str, args: &Value) -> BoxFuture<'static, Result<Value, String>>;
}

/// Frames for harlo beside a run's own events (its subagents').
pub type Notify = Arc<dyn Fn(Value) + Send + Sync>;

/// Messages sent while a run works: steering goes in at the next step,
/// follow-ups when it would stop (one at a time).
#[derive(Default)]
pub struct Inbox {
    pub steering: Vec<Queued>,
    pub follow_up: Vec<Queued>,
}

/// A message waiting: its text, and the images with it (pi's blocks).
pub struct Queued {
    pub text: String,
    pub images: Vec<Value>,
}

impl Inbox {
    /// The texts waiting: steering, then follow-ups.
    pub fn texts(&self) -> (Vec<String>, Vec<String>) {
        let texts = |q: &[Queued]| q.iter().map(|m| m.text.clone()).collect();
        (texts(&self.steering), texts(&self.follow_up))
    }
}

/// What's known of the models used: their context windows (none or 0: not
/// known), and whether they take images (see `ModelInfo::images`).
#[derive(Clone, Default)]
pub struct Known {
    windows: HashMap<String, u64>,
    images: HashMap<String, Option<bool>>,
}

impl Known {
    /// Models as listed (a provider's, or harlo's list).
    pub fn add<'a>(&mut self, models: impl IntoIterator<Item = &'a providers::ModelInfo>) {
        for m in models {
            if m.context_window > 0 {
                self.windows.insert(m.key(), m.context_window);
            }
            self.images.insert(m.key(), m.images);
        }
    }
}

pub struct Agent {
    pub model: String,
    pub effort: Option<String>,
    cwd: PathBuf,
    instructions: String,
    tools: Tools,
    history: Vec<Value>,
    /// Each message's entry id in the log.
    ids: Vec<String>,
    /// The files read and changed, as the last compaction listed them.
    files: (Vec<String>, Vec<String>),
    pub known: Known,
    /// The model last told it isn't sent images (told once).
    told_no_images: Option<String>,
    /// Whether the user's been told the sandbox can't start here.
    told_sandbox: bool,
    log: Log,
    /// The model and thinking level the log last recorded.
    logged: (Option<String>, Option<String>),
    /// Totals of this process's requests.
    pub totals: Totals,
    /// The last request's usage (the context's size).
    pub last_usage: Value,
    pub gate: Option<Arc<dyn Gate>>,
    pub harlo: Option<Arc<dyn Harlo>>,
    /// A subagent's tools (None: all of them, and `task`).
    pub allowed: Option<&'static [&'static str]>,
    /// The agent profiles `task` may name (found when it starts).
    profiles: Vec<crate::profiles::Profile>,
    /// What subagents run on when a task doesn't say (`--task-model`).
    pub task_model: Option<String>,
    /// Its token budget (`--budget`), shared with its subagents.
    pub budget: Option<Arc<Budget>>,
    /// What it goes on with when its model's plan is used up (`--fallback`).
    pub fallback: Option<String>,
    /// On the fallback: the model it left, and when to try it again (unix s).
    fell_back: Option<(String, i64)>,
    /// What its requests share a cache by (None: its session's id).
    pub cache_key: Option<String>,
    pub notify: Option<Notify>,
    /// Its subagents running, by id.
    pub subagents: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// harlo's tools as the model gets them, and what they're for (asked
    /// at each run's start).
    harlo_tools: Vec<Value>,
    harlo_text: String,
    /// MCP servers, once started (at the first prompt; not a subagent's).
    mcp: Option<crate::mcp::Mcp>,
    pub inbox: Arc<Mutex<Inbox>>,
}

#[derive(Default)]
pub struct Totals {
    pub requests: u64,
    pub input: u64,
    pub cached: u64,
    pub output: u64,
}

impl Agent {
    /// `past`: what the log already holds (nothing for a new session).
    pub fn new(cwd: PathBuf, log: Log, past: Past, model: String, effort: Option<String>) -> Self {
        let spill = std::env::temp_dir().join("harlo-agent");
        let sandbox = crate::sandbox::for_dir(&cwd);
        let profiles = crate::profiles::find(&cwd, &home(), &crate::auth::config_dir());
        Agent {
            instructions: instructions(&cwd, sandbox.as_deref()),
            tools: {
                let mut t = Tools::new(cwd.clone(), spill);
                t.set_aside(past.originals, true);
                t.sandbox = sandbox;
                t
            },
            cwd,
            model,
            effort,
            history: past.messages,
            ids: past.ids,
            files: past.files,
            known: Known::default(),
            told_no_images: None,
            told_sandbox: false,
            log,
            logged: (past.model, past.effort),
            totals: Totals::default(),
            last_usage: Value::Null,
            gate: None,
            harlo: None,
            allowed: None,
            profiles,
            task_model: None,
            budget: None,
            fallback: None,
            fell_back: None,
            cache_key: None,
            notify: None,
            subagents: Arc::default(),
            harlo_tools: Vec::new(),
            harlo_text: String::new(),
            mcp: None,
            inbox: Arc::default(),
        }
    }

    pub fn session_file(&self) -> &Path {
        &self.log.path
    }

    pub fn session_id(&self) -> &str {
        &self.log.id
    }

    pub fn messages(&self) -> &[Value] {
        &self.history
    }

    /// Starts over in a new session (same folder, model and level).
    pub fn new_session(&mut self, log: Log) {
        // (Its background commands end with it.)
        self.tools.jobs.end_all();
        self.log = log;
        self.history.clear();
        self.ids.clear();
        self.files = Default::default();
        self.tools.set_aside([], true);
        self.logged = (None, None);
    }

    /// Goes on in another session (a fork): its log and history.
    pub fn switch(&mut self, loaded: crate::session::Loaded) {
        let (log, past, _) = loaded.into_parts();
        self.log = log;
        self.history = past.messages;
        self.ids = past.ids;
        self.files = past.files;
        self.tools.set_aside(past.originals, true);
        self.logged = (past.model, past.effort);
    }

    pub fn rename(&mut self, name: &str) -> Result<()> {
        self.log.append(json!({ "type": "session_info", "name": name })).map(|_| ())
    }


    fn push(&mut self, message: Value, show: &mut (dyn FnMut(Shown) + Send)) -> Result<()> {
        let id = self.log.message(&message)?;
        if message["role"] != "assistant" {
            show(Shown::MessageStart(&message));
        }
        show(Shown::MessageEnd(&message));
        self.history.push(message);
        self.ids.push(id);
        Ok(())
    }

    /// `images`: pi's image blocks, after the text.
    fn push_user(&mut self, text: &str, images: &[Value], show: &mut (dyn FnMut(Shown) + Send)) -> Result<()> {
        // (`/skill:<name> …`: the skill, sent with it.)
        let expanded = text.trim_start().starts_with("/skill:").then(|| crate::skills::expand(text, &crate::skills::find(&self.cwd, &home(), &crate::auth::config_dir()))).flatten();
        let text = expanded.as_deref().unwrap_or(text);
        let mut content = Vec::new();
        if !text.is_empty() || images.is_empty() {
            content.push(json!({ "type": "text", "text": text }));
        }
        content.extend(images.iter().cloned());
        self.push(json!({ "role": "user", "content": content, "timestamp": now_ms() }), show)
    }

    /// Delivers the steering messages waiting (true if there were any).
    fn take_steering(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<bool> {
        let waiting: Vec<Queued> = std::mem::take(&mut self.inbox.lock().unwrap().steering);
        if waiting.is_empty() {
            return Ok(false);
        }
        self.show_queue(show);
        for m in waiting {
            self.push_user(&m.text, &m.images, show)?;
        }
        Ok(true)
    }

    fn take_follow_up(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<bool> {
        let next = {
            let mut inbox = self.inbox.lock().unwrap();
            (!inbox.follow_up.is_empty()).then(|| inbox.follow_up.remove(0))
        };
        let Some(m) = next else { return Ok(false) };
        self.show_queue(show);
        self.push_user(&m.text, &m.images, show)?;
        Ok(true)
    }

    fn show_queue(&self, show: &mut (dyn FnMut(Shown) + Send)) {
        let (steering, follow_up) = self.inbox.lock().unwrap().texts();
        show(Shown::Queue { steering: &steering, follow_up: &follow_up });
    }

    /// Works on `prompt` (and `images` with it, pi's blocks) until the
    /// model is done, or until `stop` is cancelled: then what was running
    /// ends (commands with everything they started), and the history says
    /// what was cut off.
    pub async fn run(&mut self, prompt: &str, images: &[Value], stop: &CancellationToken, show: &mut (dyn FnMut(Shown) + Send)) -> Result<Ended> {
        self.back_from_fallback(show);
        if self.logged.0.as_deref() != Some(&self.model) {
            let (provider, id) = self.model.split_once('/').unwrap_or((providers::CHATGPT, &self.model));
            self.log.append(json!({ "type": "model_change", "provider": provider, "modelId": id }))?;
            self.logged.0 = Some(self.model.clone());
        }
        if self.logged.1 != self.effort {
            self.log.append(json!({ "type": "thinking_level_change", "thinkingLevel": self.effort.as_deref().unwrap_or("off") }))?;
            self.logged.1 = self.effort.clone();
        }
        self.ask_harlo().await;
        self.start_mcp(show).await;
        self.push_user(prompt, images, show)?;
        self.turn(stop, show).await
    }

    /// Works on what the background commands that ended said (a run with
    /// no message of the user's): Done at once when nothing ended.
    pub async fn wake(&mut self, stop: &CancellationToken, show: &mut (dyn FnMut(Shown) + Send)) -> Result<Ended> {
        if !self.take_notices(show)? {
            return Ok(Ended::Done);
        }
        self.turn(stop, show).await
    }

    /// Whether background commands ended and the model hasn't been told.
    pub fn has_notices(&self) -> bool {
        !self.tools.jobs.notices.lock().unwrap().is_empty()
    }

    /// The jobs' hook for when one ends (RPC: a run of its own).
    pub fn on_job_end(&self, f: Arc<dyn Fn() + Send + Sync>) {
        self.tools.jobs.set_wake(f);
    }

    /// Background commands still running.
    pub fn jobs_running(&self) -> usize {
        self.tools.jobs.running()
    }

    /// Tells the model which background commands ended (true if any did):
    /// a notice each, shown to the user too.
    fn take_notices(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<bool> {
        let notices: Vec<String> = std::mem::take(&mut *self.tools.jobs.notices.lock().unwrap());
        for n in &notices {
            self.push(json!({ "role": "custom", "customType": "harlo_job", "content": n, "display": true, "timestamp": now_ms() }), show)?;
        }
        Ok(!notices.is_empty())
    }

    /// The model's work on what's in the history now, until it's done.
    async fn turn(&mut self, stop: &CancellationToken, show: &mut (dyn FnMut(Shown) + Send)) -> Result<Ended> {
        // A request too long for the model: sent again with old outputs set
        // aside, then (still too long) compacted: 0, 1, 2 steps taken.
        let mut overflow = 0;
        // A helper (harlo gave it report_done): whether it reported (or
        // asked) in this run, and whether it was reminded to.
        let helper = self.harlo_tools.iter().any(|t| t["name"] == "report_done");
        let (mut handed_over, mut reminded) = (false, false);
        let max = max_rounds();
        for round in 0..max {
            if round > 0 {
                self.take_steering(show)?;
                self.take_notices(show)?;
            }
            if Some(round) == wrap_up_round(max) {
                self.push(json!({ "role": "custom", "customType": "harlo_rounds_note", "content": ROUNDS_NOTE, "display": false, "timestamp": now_ms() }), show)?;
            }
            // Getting full: old tool outputs set aside first, when that's
            // turned on (cheap, and `recall` gets them back).
            if let Some(keep) = self.set_aside_point().await {
                self.set_aside(keep, 0, show)?;
            }
            // Near the end of the window: the older part summarized first
            // (if that fails, the request goes as it is).
            if self.full().await && matches!(self.compact("threshold", None, stop, show).await, Compacted::Stopped) {
                self.stopped(show)?;
                return Ok(Ended::Stopped);
            }
            // A reply cut off is dropped whole: its parts can't go back alone.
            let asked = tokio::select! {
                biased;
                _ = stop.cancelled() => None,
                r = self.ask(show) => Some(r),
            };
            let Some(reply) = asked else {
                self.stopped(show)?;
                return Ok(Ended::Stopped);
            };
            let reply = match reply {
                Ok(r) => r,
                Err(e) if overflow < 2 && compact::overflowed(&e) => {
                    // (The reply it started shows as nothing.)
                    show(Shown::Withdrawn);
                    // (All but the newest message's, for a context that's
                    // already too long.)
                    if overflow == 0 {
                        overflow = 1;
                        if self.set_aside(1, 0, show)? > 0 {
                            continue;
                        }
                    }
                    overflow = 2;
                    match self.compact("overflow", None, stop, show).await {
                        Compacted::Done(_) => continue,
                        Compacted::Stopped => {
                            self.stopped(show)?;
                            return Ok(Ended::Stopped);
                        }
                        Compacted::Nothing | Compacted::Failed(_) => return Err(e),
                    }
                }
                Err(e) => return Err(e),
            };
            overflow = 0;
            let calls: Vec<Value> = reply["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall").cloned().collect();
            let cut_off = reply["stopReason"] == "length";
            self.push(reply, show)?;
            // A reply cut off at the length limit: its calls may be
            // incomplete (a file's content cut short), so none run.
            if cut_off && !calls.is_empty() {
                for c in &calls {
                    let (id, name) = (c["id"].as_str().unwrap_or_default(), c["name"].as_str().unwrap_or_default());
                    show(Shown::ToolStart { call_id: id, name, args: &c["arguments"] });
                    show(Shown::ToolEnd { call_id: id, name, text: CUT_OFF, failed: true });
                    let result = json!({ "role": "toolResult", "toolCallId": c["id"], "toolName": c["name"], "content": [{ "type": "text", "text": CUT_OFF }], "isError": true, "timestamp": now_ms() });
                    self.push(result, show)?;
                }
                continue;
            }
            if calls.is_empty() {
                if self.take_steering(show)? || self.take_notices(show)? || self.take_follow_up(show)? {
                    continue;
                }
                // A helper ending its turn with no result for its parent:
                // reminded once (then it ends as it answers).
                if helper && !handed_over && !reminded {
                    reminded = true;
                    self.push(json!({ "role": "custom", "customType": "harlo_report_reminder", "content": REPORT_REMINDER, "display": false, "timestamp": now_ms() }), show)?;
                    continue;
                }
                return Ok(Ended::Done);
            }
            // Arguments that weren't JSON go as null: the call says so.
            let args: Vec<Value> = calls.iter().map(|c| if c.get("argumentsError").is_some() { Value::Null } else { c["arguments"].clone() }).collect();
            let names: Vec<&str> = calls.iter().map(|c| c["name"].as_str().unwrap_or_default()).collect();
            let ids: Vec<&str> = calls.iter().map(|c| c["id"].as_str().unwrap_or_default()).collect();
            for ((id, name), args) in ids.iter().zip(&names).zip(&args) {
                show(Shown::ToolStart { call_id: id, name, args });
            }
            let triples: Vec<(&str, &str, &Value)> = ids.iter().copied().zip(names.iter().copied()).zip(&args).map(|((i, n), a)| (i, n, a)).collect();
            // (Subagents: what they take from this session, when asked for.)
            let task = names.contains(&"task").then(|| self.task_ctx());
            let extra = Extra {
                gate: self.gate.as_deref(),
                harlo: self.harlo.as_deref().map(|h| (h, self.harlo_tools.as_slice())),
                allowed: self.allowed,
                task: task.as_ref(),
                mcp: self.mcp.as_ref(),
            };
            let outcomes = run_calls(&self.tools, &triples, &extra, stop).await;
            handed_over |= hands_over(&names, &outcomes);
            for ((c, name), o) in calls.iter().zip(&names).zip(outcomes) {
                show(Shown::ToolEnd { call_id: c["id"].as_str().unwrap_or_default(), name, text: &o.text, failed: o.failed });
                let mut content = vec![json!({ "type": "text", "text": o.text })];
                content.extend(o.images);
                let mut result = json!({
                    "role": "toolResult", "toolCallId": c["id"], "toolName": c["name"],
                    "content": content, "isError": o.failed, "timestamp": now_ms(),
                });
                if !o.details.is_null() {
                    // (Subagents' requests count as this session's.)
                    let u = &o.details["usage"];
                    self.totals.requests += u["requests"].as_u64().unwrap_or(0);
                    self.totals.input += u["input"].as_u64().unwrap_or(0);
                    self.totals.cached += u["cached"].as_u64().unwrap_or(0);
                    self.totals.output += u["output"].as_u64().unwrap_or(0);
                    result["details"] = o.details;
                }
                self.push(result, show)?;
            }
            if stop.is_cancelled() {
                self.stopped(show)?;
                return Ok(Ended::Stopped);
            }
        }
        bail!("The turn ended after {max} rounds of tool calls, the most one turn may take. Send a message to go on.")
    }

    /// The note a stopped turn leaves, for the model's next turn (not shown).
    fn stopped(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<()> {
        self.push(json!({ "role": "custom", "customType": "turn_stopped", "content": STOPPED_NOTE, "display": false }), show)
    }

    /// Records a failed request as pi does: an assistant message with the
    /// error (it adds nothing to what the model sees next).
    pub fn failed(&mut self, error: &str, show: &mut (dyn FnMut(Shown) + Send)) -> Result<()> {
        let (provider, id) = self.model.split_once('/').unwrap_or((providers::CHATGPT, &self.model));
        let mut m = responses::assistant_message(&[], provider, id, &Value::Null);
        m["stopReason"] = json!("error");
        m["errorMessage"] = json!(error);
        show(Shown::MessageStart(&m));
        self.push(m, show)
    }

    /// On the fallback since the plan's model was used up: back to it once
    /// it has reset (or, after a server that kept failing, the next prompt).
    /// A model chosen meanwhile stays.
    fn back_from_fallback(&mut self, show: &mut (dyn FnMut(Shown) + Send)) {
        let Some((plan, at)) = self.fell_back.clone() else { return };
        if self.fallback.as_deref() != Some(self.model.as_str()) {
            self.fell_back = None;
            return;
        }
        if crate::clock::now().unix < at {
            return;
        }
        self.fell_back = None;
        self.model = plan;
        show(Shown::Notice { level: "info", text: &format!("Back on {}.", self.model) });
        show(Shown::ModelChanged(&self.model));
    }

    /// Whether `e` moves the session to its fallback: the plan used up, or
    /// a server that kept failing through the retries (when it reset, if
    /// it said). Never for a refusal that names the request or the sign-in.
    fn falls_back(&self, e: &anyhow::Error) -> Option<i64> {
        self.fallback.as_deref().filter(|f| *f != self.model)?;
        let now = crate::clock::now().unix;
        match crate::retry::used_up(e) {
            Some(at) => Some(at.unwrap_or(now + 3600)),
            None => crate::retry::transient(e).map(|_| now),
        }
    }

    /// One request with the history: the reply, as pi's assistant message.
    async fn ask(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<Value> {
        let images = self.takes_images().await;
        if !images && self.told_no_images.as_ref() != Some(&self.model) && self.history.iter().any(crate::images::has_image) {
            show(Shown::Notice { level: "warning", text: &format!("{} doesn't take images: they aren't sent to it, and it's told there was one.", self.model) });
            self.told_no_images = Some(self.model.clone());
        }
        if let Some(why) = self.tools.sandbox.as_ref().and_then(|sb| sb.broken.as_ref()).filter(|_| !self.told_sandbox) {
            show(Shown::Notice { level: "warning", text: &format!("Harlo's commands can't run: its sandbox can't start here ({why}). Install bubblewrap, or turn the sandbox off in Settings → Providers → Harlo.") });
            self.told_sandbox = true;
        }
        if let Some(why) = self.budget.as_ref().and_then(|b| b.used()) {
            bail!("{why}");
        }
        let (system, specs) = (self.system(), self.specs());
        let key = self.cache_key.clone().unwrap_or_else(|| self.log.id.clone());
        let asked = Asked { model: &self.model, effort: self.effort.as_deref(), instructions: &system, history: &self.history, session: &key, specs: &specs, tools: true, images };
        let (message, usage) = match send(asked, show).await {
            Err(e) if let Some(back) = self.falls_back(&e) => {
                // Goes on with the fallback: said, and in the log.
                let (plan, fb) = (self.model.clone(), self.fallback.clone().unwrap_or_default());
                show(Shown::Withdrawn);
                let when = if back > crate::clock::now().unix + 60 { "once its limit resets" } else { "with the next prompt" };
                show(Shown::Notice { level: "warning", text: &format!("{plan}: {e:#} Going on with {fb}; {plan} is tried again {when}.") });
                self.fell_back = Some((plan, back));
                self.model = fb;
                let (provider, id) = self.model.split_once('/').unwrap_or((providers::CHATGPT, &self.model));
                self.log.append(json!({ "type": "model_change", "provider": provider, "modelId": id }))?;
                self.logged.0 = Some(self.model.clone());
                show(Shown::ModelChanged(&self.model));
                let images = self.takes_images().await;
                let asked = Asked { model: &self.model, effort: self.effort.as_deref(), instructions: &system, history: &self.history, session: &key, specs: &specs, tools: true, images };
                send(asked, show).await?
            }
            r => r?,
        };
        self.count(&usage);
        show(Shown::Usage { model: message["model"].as_str().unwrap_or_default(), usage: &usage });
        self.last_usage = usage;
        Ok(message)
    }

    fn task_ctx(&self) -> crate::task::Ctx {
        crate::task::Ctx {
            model: self.model.clone(),
            effort: self.effort.clone(),
            cwd: self.cwd.clone(),
            gate: self.gate.clone(),
            parent_file: (!self.log.path.as_os_str().is_empty()).then(|| self.log.path.clone()),
            cache_key: self.cache_key.clone().unwrap_or_else(|| self.log.id.clone()),
            notify: self.notify.clone(),
            known: self.known.clone(),
            running: self.subagents.clone(),
            profiles: self.profiles.clone(),
            task_model: self.task_model.clone(),
            budget: self.budget.clone(),
            fallback: self.fallback.clone(),
        }
    }

    pub fn launch(&mut self, l: &Launch) {
        self.task_model = l.task_model.clone();
        self.fallback = l.fallback.clone();
        // (What the session used before, when it goes on.)
        self.budget = l.budget.map(|limit| Arc::new(Budget { limit, spent: crate::session::spent(&self.log.path).into() }));
    }

    /// Added to the instructions (an agent profile's, for a subagent).
    pub fn add_instructions(&mut self, text: &str) {
        self.instructions.push_str(text);
    }

    /// Starts the MCP servers configured (once; not for a subagent), or
    /// lists again the tools of those that said they changed.
    async fn start_mcp(&mut self, show: &mut (dyn FnMut(Shown) + Send)) {
        if self.allowed.is_some() {
            return;
        }
        let notes = match &mut self.mcp {
            Some(m) => m.refresh().await,
            None => {
                let (configs, mut notes) = crate::mcp::load(&self.cwd);
                let (m, started) = crate::mcp::Mcp::start(configs, &self.cwd).await;
                notes.extend(started);
                self.mcp = Some(m);
                notes
            }
        };
        for n in &notes {
            show(Shown::Notice { level: "warning", text: n });
        }
    }

    /// The system prompt: the project's, then what harlo's tools are for.
    fn system(&self) -> String {
        // (Asked which model it is, it can say: it changes when you switch.)
        let (provider, id) = self.model.split_once('/').unwrap_or((providers::CHATGPT, &self.model));
        let model = format!("\nYou run on the model {id} (from {provider}).\n");
        if self.harlo_text.is_empty() {
            return format!("{}{model}", self.instructions);
        }
        format!("{}{model}\n<harlo>\n{}\n</harlo>\n", self.instructions, self.harlo_text.trim())
    }

    /// The tools the model gets: its own, then harlo's.
    fn specs(&self) -> Value {
        let mut all = tools::specs_of(self.allowed, (self.allowed.is_none() && crate::task::enabled()).then_some(self.profiles.as_slice()));
        if let Some(list) = all.as_array_mut() {
            // (`recall` once there's something to recall: the request that
            // first has a stub breaks the cache anyway.)
            if !self.tools.has_originals() {
                list.retain(|t| t["name"] != "recall");
            }
            // (Questions for the user: only where someone can answer.)
            if self.gate.is_none() {
                list.retain(|t| t["name"] != "ask");
            }
            if let Some(m) = self.mcp.as_ref().filter(|_| self.allowed.is_none()) {
                list.extend(m.specs().iter().cloned());
            }
            list.extend(self.harlo_tools.iter().cloned());
        }
        all
    }

    /// harlo's tools and what they're for, as harlo says now (none when it
    /// doesn't answer, or orchestration is off in it). Unchanged, the
    /// requests start as before, so their cache still serves.
    async fn ask_harlo(&mut self) {
        let Some(h) = self.harlo.clone() else { return };
        let listed = h.call(HARLO_TOOLS, &json!({})).await;
        self.harlo_tools = listed.map(|l| harlo_specs(&l)).unwrap_or_default();
        self.harlo_text = if self.harlo_tools.is_empty() {
            String::new()
        } else {
            h.call(HARLO_INSTRUCTIONS, &json!({})).await.ok().and_then(|t| t.as_str().map(str::to_owned)).unwrap_or_default()
        };
    }

    fn count(&mut self, usage: &Value) {
        let (input, cached, output) = (usage["input_tokens"].as_u64().unwrap_or(0), usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0), usage["output_tokens"].as_u64().unwrap_or(0));
        self.totals.requests += 1;
        self.totals.input += input;
        self.totals.cached += cached;
        self.totals.output += output;
        if let Some(b) = &self.budget {
            b.spent.fetch_add(input.saturating_sub(cached) + output, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// The context's size: the last request's, with its reply.
    pub fn context_tokens(&self) -> u64 {
        self.last_usage["input_tokens"].as_u64().unwrap_or(0) + self.last_usage["output_tokens"].as_u64().unwrap_or(0)
    }

    /// The model's context window (0: not known), asked of its provider
    /// once.
    async fn window(&mut self) -> u64 {
        if !self.known.windows.contains_key(&self.model) {
            self.ask_models().await;
        }
        *self.known.windows.entry(self.model.clone()).or_insert(0)
    }

    /// Whether the model takes images (when it doesn't say, it's taken to),
    /// asked of its provider once.
    async fn takes_images(&mut self) -> bool {
        if !self.known.images.contains_key(&self.model) {
            self.ask_models().await;
        }
        self.known.images.entry(self.model.clone()).or_insert(None).unwrap_or(true)
    }

    /// What the model's provider lists of it (and its others): asked once,
    /// so what's not said stays unknown.
    async fn ask_models(&mut self) {
        if let Ok((p, id)) = providers::resolve(&self.model)
            && let Ok(Ok(list)) = tokio::time::timeout(Duration::from_secs(10), p.models()).await
        {
            self.known.add(&list);
            // (Under the name the session uses.)
            if let Some(m) = list.iter().find(|m| m.id == id) {
                self.known.windows.insert(self.model.clone(), m.context_window);
                self.known.images.insert(self.model.clone(), m.images);
            }
        }
        self.known.windows.entry(self.model.clone()).or_insert(0);
        self.known.images.entry(self.model.clone()).or_insert(None);
    }

    /// Whether the context is past where old outputs are set aside: how
    /// much of the newest conversation to keep as it is.
    async fn set_aside_point(&mut self) -> Option<u64> {
        let tokens = self.context_tokens();
        if tokens == 0 {
            return None;
        }
        let at = compact::threshold(self.window().await);
        let from = stubs::threshold(at);
        (from > 0 && tokens >= from).then(|| compact::keep(at))
    }

    /// Sets old tool outputs aside (see `stubs`), keeping the newest `keep`
    /// tokens as they are; `context`: the context's size (0: the last
    /// request's). The tokens freed (0: not enough to be worth it).
    fn set_aside(&mut self, keep: u64, context: u64, show: &mut (dyn FnMut(Shown) + Send)) -> Result<u64> {
        let context = if context == 0 { self.context_tokens() } else { context };
        let (picked, saved) = stubs::pick(&self.history, keep, context);
        if picked.is_empty() {
            return Ok(0);
        }
        let texts: Vec<String> = picked.iter().map(|&i| stubs::stub_text(&self.history, &self.history[i])).collect();
        let originals: Vec<_> = picked.iter().map(|&i| stubs::original(&self.history, &self.history[i])).collect();
        let list: Vec<Value> = picked.iter().zip(&texts).map(|(&i, t)| json!({ "id": self.history[i]["toolCallId"], "text": t })).collect();
        self.log.append(json!({ "type": "harlo_stubs", "stubs": list, "tokensBefore": context, "tokensSaved": saved }))?;
        for (&i, t) in picked.iter().zip(&texts) {
            self.history[i] = stubs::as_stub(&self.history[i], t);
        }
        self.tools.set_aside(originals, false);
        let after = context.saturating_sub(saved);
        if context > 0 {
            self.last_usage = json!({ "input_tokens": after, "output_tokens": 0 });
        }
        show(Shown::SetAside { count: picked.len(), saved, after });
        Ok(saved)
    }

    /// Whether the context is past where it's compacted.
    async fn full(&mut self) -> bool {
        let tokens = self.context_tokens();
        if tokens == 0 {
            return false;
        }
        let at = compact::threshold(self.window().await);
        at > 0 && tokens >= at
    }

    /// Replaces the older part of the history with a summary the model
    /// writes (`focus`: what the user asked it to keep). Shown as it goes,
    /// unless there's too little to summarize.
    pub async fn compact(&mut self, reason: &str, focus: Option<&str>, stop: &CancellationToken, show: &mut (dyn FnMut(Shown) + Send)) -> Compacted {
        let at = compact::threshold(self.window().await);
        // (Too long even so: only the newest message kept.)
        let keep = if reason == "manual" { compact::keep(at).min(compact::ASKED_KEEP) } else { compact::keep(at) };
        let cut = compact::cut(&self.history, keep).or_else(|| if reason == "overflow" { compact::cut(&self.history, 1) } else { None });
        let Some(cut) = cut else { return Compacted::Nothing };
        if let Some(why) = self.budget.as_ref().and_then(|b| b.used()) {
            return Compacted::Failed(why);
        }
        let Some(first_kept) = self.ids.get(cut).cloned() else { return Compacted::Nothing };
        show(Shown::CompactionStart { reason });
        // (The last request's count when there was one since the start.)
        let tokens_before = match self.context_tokens() {
            0 => compact::estimate(&json!(self.instructions)) + self.history.iter().map(compact::estimate).sum::<u64>(),
            n => n,
        };
        let mut asked: Vec<Value> = self.history[..cut].to_vec();
        let turn = compact::turn_cut(&self.history, cut);
        asked.push(json!({ "role": "user", "content": [{ "type": "text", "text": compact::request(focus, turn.as_deref()) }], "timestamp": now_ms() }));
        let (system, specs) = (self.system(), self.specs());
        let key = self.cache_key.clone().unwrap_or_else(|| self.log.id.clone());
        let images = self.takes_images().await;
        let request = Asked { model: &self.model, effort: self.effort.as_deref(), instructions: &system, history: &asked, session: &key, specs: &specs, tools: false, images };
        // (The summary isn't shown as it's written.)
        let mut quiet = |_: Shown| {};
        let sent = tokio::select! {
            biased;
            _ = stop.cancelled() => None,
            r = send(request, &mut quiet) => Some(r),
        };
        let failed = |show: &mut (dyn FnMut(Shown) + Send), why: String| {
            show(Shown::CompactionEnd { reason, result: None, error: Some(&why), will_retry: false });
            Compacted::Failed(why)
        };
        let (reply, usage) = match sent {
            None => {
                show(Shown::CompactionEnd { reason, result: None, error: None, will_retry: false });
                return Compacted::Stopped;
            }
            Some(Err(e)) => return failed(show, format!("{e:#}")),
            Some(Ok(r)) => r,
        };
        self.count(&usage);
        let text: String = reply["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("");
        if reply["stopReason"] == "length" {
            return failed(show, "The summary was cut off".into());
        }
        if text.trim().is_empty() {
            return failed(show, "The model wrote no summary".into());
        }
        let files = compact::files(&self.history[..cut], &self.files);
        let summary = compact::with_files(&text, &files);
        let entry = json!({
            "type": "compaction", "summary": summary, "firstKeptEntryId": first_kept, "tokensBefore": tokens_before,
            "details": { "readFiles": files.0, "modifiedFiles": files.1 },
            // (What writing it cost: a request of its own.)
            "usage": reply["usage"],
        });
        let id = match self.log.append(entry.clone()) {
            Ok(id) => id,
            Err(e) => return failed(show, format!("{e:#}")),
        };
        self.history.splice(..cut, [crate::session::summary_message(&entry)]);
        self.ids.splice(..cut, [id]);
        self.files = files;
        // The project's instructions as they are now.
        self.instructions = instructions(&self.cwd, self.tools.sandbox.as_deref());
        let after = compact::estimate(&json!(self.instructions)) + self.history.iter().map(compact::estimate).sum::<u64>();
        self.last_usage = json!({ "input_tokens": after, "output_tokens": 0 });
        let result = json!({ "summary": summary, "firstKeptEntryId": first_kept, "tokensBefore": tokens_before, "estimatedTokensAfter": after });
        show(Shown::CompactionEnd { reason, result: Some(&result), error: None, will_retry: reason == "overflow" });
        Compacted::Done(result)
    }
}

/// A request: the session's model and settings, and the history to send.
#[derive(Clone, Copy)]
struct Asked<'a> {
    /// `provider/model`.
    model: &'a str,
    effort: Option<&'a str>,
    instructions: &'a str,
    history: &'a [Value],
    session: &'a str,
    /// The tools' definitions (Responses API form).
    specs: &'a Value,
    /// Whether the model may call tools (they're sent either way, so the
    /// request starts as the session's others do and their cache serves it).
    tools: bool,
    /// Whether the model takes images (otherwise a note goes in their place).
    images: bool,
}

/// Sends a request: the reply, as pi's assistant message, and its usage.
/// One that failed for a passing reason (see `retry`) is sent again, the
/// reply it had started dropped, with a notice.
async fn send(a: Asked<'_>, show: &mut (dyn FnMut(Shown) + Send)) -> Result<(Value, Value)> {
    let mut tried = 0;
    loop {
        let e = match send_once(a, show).await {
            Ok(r) => return Ok(r),
            Err(e) => e,
        };
        let Some((why, asked)) = crate::retry::transient(&e).filter(|_| tried < crate::retry::TRIES) else { return Err(e) };
        tried += 1;
        let wait = crate::retry::delay(tried, asked);
        show(Shown::Withdrawn);
        let secs = wait.as_secs_f64();
        let when = if secs < 1.5 { "in a second".to_owned() } else { format!("in {} s", secs.round() as u64) };
        show(Shown::Notice { level: "info", text: &format!("Retrying {when} ({why}: {e:#}).") });
        tokio::time::sleep(wait).await;
    }
}

/// One try. A token the plan refuses is refreshed once and the request
/// sent again.
async fn send_once(a: Asked<'_>, show: &mut (dyn FnMut(Shown) + Send)) -> Result<(Value, Value)> {
    let (provider, model) = providers::resolve(a.model)?;
    let pid = provider.id.clone();
    let api = if provider.api == Api::Chat { "openai-completions" } else { "openai-responses" };
    show(Shown::MessageStart(&json!({ "role": "assistant", "content": [], "api": api, "provider": pid, "model": model, "timestamp": now_ms() })));
    let effort = provider.effort(&model, a.effort);
    if provider.api == Api::Chat {
        let mut body = chat::request(&model, a.instructions, &chat::messages(a.history, &model, a.images), a.specs, effort.as_deref());
        if !a.tools {
            body["tool_choice"] = json!("none");
        }
        let token = provider.token(false).await?;
        return chat::stream(&provider.base_url, token.as_deref(), &body, &pid, &model, |e| show(Shown::Stream(e))).await;
    }
    let input = responses::input(a.history, &pid, &model, a.images);
    let mut body = responses::request(&model, a.instructions, &input, a.specs.clone(), effort.as_deref(), a.session);
    if !a.tools {
        body["tool_choice"] = json!("none");
    }
    // (The plan's affinity headers keep a session's requests where its cache is.)
    let affinity = provider.is_plan().then_some(a.session);
    let mut force = false;
    loop {
        let token = provider.token(force).await?;
        let headers = provider.headers();
        let mut items = Vec::new();
        let mut done: Option<(Value, Vec<Value>)> = None;
        let mut blocks = Blocks::default();
        let result = responses::stream(&provider.base_url, token.as_deref(), affinity, &headers, &body, |ev| match ev {
            responses::Event::Item { index, item } => {
                if let Some(end) = blocks.end(index, &item, &pid, &model) {
                    show(Shown::Stream(&end));
                }
                items.push(item);
            }
            responses::Event::Completed { usage, output } => done = Some((usage, output)),
            // The plan's limits (the Codex backend sends them; the account by
            // its email).
            responses::Event::Limits(windows) => {
                if let Some(sign) = provider.sign() {
                    let account = crate::auth::load(sign).map(|c| c.email).unwrap_or_default();
                    show(Shown::Limits(&json!({ "provider": "openai-codex", "account": account, "windows": windows })));
                }
            }
            other => {
                for e in blocks.stream(other) {
                    show(Shown::Stream(&e));
                }
            }
        })
        .await;
        match result {
            Ok(()) => {
                let (usage, output) = done.unwrap_or_default();
                // The completed response's own list when it has one.
                let items = if output.is_empty() { items } else { output };
                return Ok((responses::assistant_message(&items, &pid, &model, &usage), usage));
            }
            // The plan's token, refused: refreshed once.
            Err(e) if provider.is_plan() && !force && e.downcast_ref::<responses::ApiError>().is_some_and(|a| a.status == 401) => force = true,
            Err(e) => return Err(e),
        }
    }
}

/// The reply's blocks as they stream: each output item is a block of the
/// assistant message (as `assistant_message` makes them, in order).
#[derive(Default)]
struct Blocks {
    /// Output index → (block index, summary part last seen).
    at: std::collections::HashMap<usize, (usize, usize)>,
    next: usize,
}

impl Blocks {
    fn stream(&mut self, ev: responses::Event) -> Vec<Value> {
        use responses::Event as E;
        match ev {
            E::Added { index, item } => {
                let ci = self.next;
                self.next += 1;
                self.at.insert(index, (ci, 0));
                match item["type"].as_str() {
                    Some("reasoning") => vec![json!({ "type": "thinking_start", "contentIndex": ci })],
                    Some("message") => vec![json!({ "type": "text_start", "contentIndex": ci })],
                    Some("function_call") => vec![json!({
                        "type": "toolcall_start", "contentIndex": ci,
                        "id": format!("{}|{}", item["call_id"].as_str().unwrap_or_default(), item["id"].as_str().unwrap_or_default()),
                        "toolName": item["name"],
                    })],
                    _ => {
                        self.next -= 1;
                        self.at.remove(&index);
                        Vec::new()
                    }
                }
            }
            E::Text { index, delta } => self.at.get(&index).map(|(ci, _)| vec![json!({ "type": "text_delta", "contentIndex": ci, "delta": delta })]).unwrap_or_default(),
            E::Args { index, delta } => self.at.get(&index).map(|(ci, _)| vec![json!({ "type": "toolcall_delta", "contentIndex": ci, "delta": delta })]).unwrap_or_default(),
            E::Reasoning { index, part, delta } => {
                let Some((ci, last)) = self.at.get_mut(&index) else { return Vec::new() };
                // Summary parts are joined by a blank line.
                let sep = if part != *last { "\n\n" } else { "" };
                *last = part;
                vec![json!({ "type": "thinking_delta", "contentIndex": *ci, "delta": format!("{sep}{delta}") })]
            }
            E::Item { .. } | E::Completed { .. } | E::Limits(_) => Vec::new(),
        }
    }

    /// An item done: its block's end, with the block as it will be kept.
    fn end(&mut self, index: usize, item: &Value, provider: &str, model: &str) -> Option<Value> {
        let (ci, _) = *self.at.get(&index)?;
        let block = responses::assistant_message(std::slice::from_ref(item), provider, model, &Value::Null)["content"][0].clone();
        Some(match block["type"].as_str()? {
            "thinking" => json!({ "type": "thinking_end", "contentIndex": ci, "content": block["thinking"] }),
            "text" => json!({ "type": "text_end", "contentIndex": ci, "content": block["text"] }),
            "toolCall" => json!({ "type": "toolcall_end", "contentIndex": ci, "toolCall": block }),
            _ => return None,
        })
    }
}

/// Runs one round's calls: those that only look at once, otherwise one
/// after another, in the order called. When `stop` is cancelled, the
/// call running ends (dropping it ends its command) and what's left
/// isn't started.
/// What a round's calls may go through besides the tools themselves.
#[derive(Default)]
struct Extra<'a> {
    gate: Option<&'a dyn Gate>,
    harlo: Option<(&'a dyn Harlo, &'a [Value])>,
    /// A subagent's tools (None: all).
    allowed: Option<&'a [&'a str]>,
    task: Option<&'a crate::task::Ctx>,
    mcp: Option<&'a crate::mcp::Mcp>,
}

impl Extra<'_> {
    /// Whether a call only looks (several of those run at once, and they
    /// don't need the user's yes).
    fn read_only(&self, name: &str) -> bool {
        tools::read_only(name) || self.mcp.is_some_and(|m| m.read_only(name))
    }
}

/// A call of ours or of an MCP server's.
async fn call_tool(tools: &Tools, mcp: Option<&crate::mcp::Mcp>, name: &str, args: &Value) -> tools::Outcome {
    match mcp.filter(|m| m.has(name)) {
        Some(m) => m.call(name, args, &tools.spill_dir).await,
        None => tools.call(name, args).await,
    }
}

const NO_ONE_TO_ASK: &str = "Commands can't run outside the sandbox here: there's no one to ask. Do it another way inside the sandbox, or say what the user should run.";

/// `calls`: (id, name, arguments).
async fn run_calls(tools: &Tools, calls: &[(&str, &str, &Value)], extra: &Extra<'_>, stop: &CancellationToken) -> Vec<tools::Outcome> {
    let bad_args = || tools::Outcome::new("The arguments weren't valid JSON; call the tool again.", true);
    let cut = |text: &str| tools::Outcome::new(text, true);
    let (gate, harlo) = (extra.gate, extra.harlo);
    // (A subagent's: only its own.)
    let not_here = |name: &str| extra.allowed.is_some_and(|only| !only.contains(&name));
    let mcp = extra.mcp;
    if calls.iter().all(|(_, n, _)| extra.read_only(n) && !not_here(n)) {
        let all = join_all(calls.iter().map(|(_, name, args)| async move { if args.is_null() { bad_args() } else { call_tool(tools, mcp, name, args).await } }));
        return tokio::select! {
            biased;
            _ = stop.cancelled() => calls.iter().map(|_| cut(INTERRUPTED)).collect(),
            o = all => o,
        };
    }
    let mut out = Vec::new();
    for (id, name, args) in calls {
        if not_here(name) {
            out.push(cut(&format!("You can't use {name} here: your tools are {}.", extra.allowed.unwrap_or_default().join(", "))));
            continue;
        }
        if extra.allowed.is_some() && *name == "bash" && args["background"] == true {
            out.push(cut("Subagents can't run commands in the background: run it in the foreground (with a longer timeout if it needs one)."));
            continue;
        }
        if stop.is_cancelled() {
            out.push(cut(NOT_RUN));
            continue;
        }
        if args.is_null() {
            out.push(bad_args());
            continue;
        }
        // harlo's: it asks the user itself where it should.
        if let Some((h, _)) = harlo.filter(|(_, specs)| specs.iter().any(|s| s["name"] == *name)) {
            let ran = tokio::select! {
                biased;
                _ = stop.cancelled() => None,
                r = h.call(name, args) => Some(r),
            };
            out.push(match ran {
                None => cut(INTERRUPTED),
                Some(Ok(Value::String(text))) => tools::Outcome::new(text, false),
                Some(Ok(v)) => tools::Outcome::new(v.to_string(), false),
                Some(Err(e)) => cut(&e),
            });
            continue;
        }
        // Questions for the user, on harlo's card.
        if *name == "ask" {
            let Some(g) = gate.filter(|_| extra.allowed.is_none()) else {
                out.push(cut("No one is here to answer questions: decide yourself, and say what you assumed."));
                continue;
            };
            let questions = match ask_questions(args) {
                Ok(q) => q,
                Err(e) => {
                    out.push(cut(&e));
                    continue;
                }
            };
            let answered = tokio::select! {
                biased;
                _ = stop.cancelled() => {
                    out.push(cut(INTERRUPTED));
                    continue;
                }
                a = g.ask(questions.clone()) => a,
            };
            out.push(match answered {
                Some(a) => tools::Outcome::new(ask_answers(&questions, &a), false),
                None => tools::Outcome::new("The user dismissed the questions without answering. Don't ask them again: go on as you think best and say what you assumed, or end your turn and ask in your reply.", false),
            });
            continue;
        }
        // Subagents: their own calls ask where they should.
        if *name == "task"
            && let Some(ctx) = extra.task
        {
            out.push(crate::task::run(args, id, ctx, stop).await);
            continue;
        }
        // A command that can't be undone: asked about in every mode (and
        // then not asked again), refused with no one to ask.
        let mut confirmed = false;
        if *name == "bash"
            && let Some(why) = crate::danger::check(args["command"].as_str().unwrap_or_default(), &tools.project(), &home())
        {
            let Some(g) = gate else {
                out.push(cut(&format!("Not run: {why}, and there's no one here to ask. Do it another way, or say what the user should run.")));
                continue;
            };
            let yes = tokio::select! {
                biased;
                _ = stop.cancelled() => None,
                y = g.confirm(&format!("Allow tool: bash\nReason: this can't be undone: {why}"), args["command"].as_str().unwrap_or_default()) => Some(y),
            };
            match yes {
                None => {
                    out.push(cut(NOT_RUN));
                    continue;
                }
                Some(false) => {
                    out.push(cut(&format!("The user said no: {why}. Don't try it another way; ask what they'd like instead.")));
                    continue;
                }
                Some(true) => confirmed = true,
            }
        }
        // Outside the sandbox: only with the user's yes.
        if gate.is_none() && *name == "bash" && args["outside_sandbox"] == true && tools.sandbox.is_some() {
            out.push(cut(NO_ONE_TO_ASK));
            continue;
        }
        // What changes something may need the user's yes first.
        if let Some(gate) = gate.filter(|_| !extra.read_only(name) && !(confirmed && args["outside_sandbox"] != true)) {
            let verdict = tokio::select! {
                biased;
                _ = stop.cancelled() => Err(NOT_RUN.to_owned()),
                v = gate.check(name, args) => v,
            };
            if let Err(why) = verdict {
                out.push(cut(&why));
                continue;
            }
        }
        let ran = tokio::select! {
            biased;
            _ = stop.cancelled() => None,
            o = call_tool(tools, mcp, name, args) => Some(o),
        };
        out.push(ran.unwrap_or_else(|| cut(INTERRUPTED)));
    }
    out
}

/// `ask`'s questions as harlo's card takes them (`q0`, `q1`… as ids), or
/// what's wrong with them.
fn ask_questions(args: &Value) -> Result<Value, String> {
    let list = args["questions"].as_array().filter(|l| !l.is_empty()).ok_or("Give questions: one to four, each with a question and its options.")?;
    if list.len() > 4 {
        return Err("At most four questions at a time.".into());
    }
    let mut out = Vec::new();
    for (i, q) in list.iter().enumerate() {
        let question = q["question"].as_str().map(str::trim).filter(|t| !t.is_empty()).ok_or(format!("Question {} has no text.", i + 1))?;
        let options: Vec<Value> = q["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| {
                let label = o["label"].as_str().or(o.as_str()).map(str::trim).filter(|l| !l.is_empty())?;
                Some(json!({ "label": label, "description": o["description"].as_str().unwrap_or_default() }))
            })
            .collect();
        if options.len() < 2 {
            return Err(format!("Question {} needs at least two options (they can also write their own answer).", i + 1));
        }
        let header = q["header"].as_str().map(str::trim).filter(|h| !h.is_empty()).map_or_else(|| format!("Question {}", i + 1), str::to_owned);
        out.push(json!({ "id": format!("q{i}"), "header": header, "question": question, "multi": q["multiple"] == true, "options": options }));
    }
    Ok(Value::Array(out))
}

/// The user's answers (`[{id, selectedOptions, customInput?}]`), for the model.
fn ask_answers(questions: &Value, answers: &Value) -> String {
    let mut text = String::from("The user answered:");
    for q in questions.as_array().into_iter().flatten() {
        let a = answers.as_array().into_iter().flatten().find(|a| a["id"] == q["id"]);
        let mut said: Vec<String> = a.map(|a| a["selectedOptions"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_owned)).collect()).unwrap_or_default();
        if let Some(own) = a.and_then(|a| a["customInput"].as_str()).map(str::trim).filter(|c| !c.is_empty()) {
            said.push(format!("(in their words) {own}"));
        }
        let said = if said.is_empty() { "(nothing picked)".to_owned() } else { said.join("; ") };
        text.push_str(&format!("\n- {}\n  {said}", q["question"].as_str().unwrap_or_default()));
    }
    text
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// Whether a round's calls handed the turn over (a report, or a question
/// for the parent or the user, that harlo took).
fn hands_over(names: &[&str], outcomes: &[tools::Outcome]) -> bool {
    names.iter().zip(outcomes).any(|(n, o)| HAND_OVERS.contains(n) && !o.failed)
}

/// What harlo is asked for its tools, and for what they're for.
pub const HARLO_TOOLS: &str = "_tools";
pub const HARLO_INSTRUCTIONS: &str = "_instructions";

/// harlo's tools (MCP's `{name, description, inputSchema}`) as the model
/// gets them; one named as a tool of ours is left out.
fn harlo_specs(listed: &Value) -> Vec<Value> {
    let ours: Vec<Value> = tools::specs().as_array().cloned().unwrap_or_default();
    listed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let name = t["name"].as_str().filter(|n| !n.is_empty())?;
            if ours.iter().any(|o| o["name"] == name) {
                return None;
            }
            let mut params = t["inputSchema"].clone();
            if !params.is_object() {
                params = json!({ "type": "object", "properties": {} });
            }
            Some(json!({ "type": "function", "name": name, "description": t["description"].as_str().unwrap_or_default(), "strict": false, "parameters": params }))
        })
        .collect()
}

/// The system prompt: short; the project's own instructions after it.
fn instructions(cwd: &Path, sandbox: Option<&crate::sandbox::Sandbox>) -> String {
    let mut s = String::from(
        "You are harlo-agent, a coding agent. You work in the user's project with your tools, and finish the task you're given.\n\
         \n\
         - Look before you change: read a file before editing it. When the task names a file, read it; search (grep, find) only when you don't know where something is.\n\
         - Change files with edit (small, exact replacements) or write (new files). Never edit files through bash.\n\
         - Run commands with bash to build, test and check your work. Commands can't take input.\n\
         - When a tool fails, read its message: it says what to do next.\n\
         - Keep going until the task is done, then reply with a short summary of what you changed and anything left open. Be concise.\n",
    );
    for (path, text) in project_instructions(cwd) {
        s.push_str(&format!("\n<project_instructions path=\"{}\">\n{}\n</project_instructions>\n", path.display(), text.trim()));
    }
    let (rules, scoped) = other_rules(cwd);
    for (path, text) in rules {
        s.push_str(&format!("\n<project_instructions path=\"{}\">\n{}\n</project_instructions>\n", path.display(), text.trim()));
    }
    if !scoped.is_empty() {
        s.push_str("\n<project_rules>\nRules for some of the project's files: read one before you work on the files it's for.\n");
        for (path, about) in scoped {
            s.push_str(&format!("- {} ({about})\n", path.display()));
        }
        s.push_str("</project_rules>\n");
    }
    s.push_str(&crate::skills::listing(&crate::skills::find(cwd, &home(), &crate::auth::config_dir())));
    s.push_str(&format!("\nWorking directory: {}\n", cwd.display()));
    // (The day only, so the instructions stay the same all day; the time
    // is a tool.)
    s.push_str(&format!("The session started on {}; current_time gives the time now.\n", crate::clock::now().date()));
    if let Some(sb) = sandbox {
        s.push_str(&format!("{}\n", sb.describe()));
    }
    if let Some(listing) = listing(cwd) {
        s.push_str(&format!("\nIts files when the session started:\n{listing}\n"));
    }
    s
}

/// Entries of the working directory shown in the instructions, at most.
const LISTED: usize = 50;

/// The working directory's entries (folders end in /), sorted, without
/// .git; a note says how many more there are.
fn listing(cwd: &Path) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(cwd)
        .ok()?
        .flatten()
        .filter(|e| e.file_name() != ".git")
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.file_type().is_ok_and(|t| t.is_dir()) { format!("{name}/") } else { name }
        })
        .collect();
    if names.is_empty() {
        return Some("(empty)".into());
    }
    names.sort();
    let more = names.len().saturating_sub(LISTED);
    names.truncate(LISTED);
    let mut out = names.join("\n");
    if more > 0 {
        out.push_str(&format!("\n… and {more} more (use ls)"));
    }
    Some(out)
}

/// AGENTS.md (or CLAUDE.md) in the working directory and the folders
/// above it, outermost first.
fn project_instructions(cwd: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for dir in cwd.ancestors() {
        for name in ["AGENTS.md", "CLAUDE.md"] {
            let p = dir.join(name);
            if let Ok(text) = std::fs::read_to_string(&p) {
                found.push((p, text));
                break;
            }
        }
    }
    found.reverse();
    found
}

/// Of another agent's rule file, what goes in the instructions.
const RULES_KEPT: usize = 20_000;

/// Other agents' rules in the project's top folder (the repository's, else
/// the working directory): Copilot's, Cursor's, Cline's and Windsurf's.
/// Cursor's rules for some files only are listed (path, and what they're
/// for), not sent.
fn other_rules(cwd: &Path) -> (Vec<(PathBuf, String)>, Vec<(PathBuf, String)>) {
    let top = cwd.ancestors().find(|d| d.join(".git").exists()).unwrap_or(cwd);
    let keep = |t: String| -> Option<String> { (!t.trim().is_empty()).then(|| t.chars().take(RULES_KEPT).collect()) };
    let mut sent = Vec::new();
    for name in [".github/copilot-instructions.md", ".cursorrules", ".windsurfrules"] {
        let p = top.join(name);
        if let Some(t) = std::fs::read_to_string(&p).ok().and_then(keep) {
            sent.push((p, t));
        }
    }
    // Cline's: a file, or a folder of them.
    let cline = top.join(".clinerules");
    if cline.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&cline).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "md" || e == "txt")).collect();
        files.sort();
        sent.extend(files.into_iter().filter_map(|p| std::fs::read_to_string(&p).ok().and_then(keep).map(|t| (p, t))));
    } else if let Some(t) = std::fs::read_to_string(&cline).ok().and_then(keep) {
        sent.push((cline, t));
    }
    // Cursor's: always applied, sent; for some files, or asked for by
    // description, listed.
    let mut scoped = Vec::new();
    let mut mdc: Vec<PathBuf> = std::fs::read_dir(top.join(".cursor/rules")).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "mdc" || e == "md")).collect();
    mdc.sort();
    for p in mdc {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let (head, body) = match text.strip_prefix("---\n").and_then(|r| r.split_once("\n---")) {
            Some((h, b)) => (h, b.split_once('\n').map_or("", |(_, b)| b)),
            None => ("", text.as_str()),
        };
        let field = |k: &str| head.lines().find_map(|l| l.split_once(':').filter(|(key, _)| key.trim() == k).map(|(_, v)| v.trim().trim_matches('"').to_owned())).filter(|v| !v.is_empty());
        if field("alwaysApply").as_deref() == Some("true") || head.is_empty() {
            if let Some(t) = keep(body.to_owned()) {
                sent.push((p, t));
            }
        } else {
            let about = match (field("globs"), field("description")) {
                (Some(g), Some(d)) => format!("for {g}: {d}"),
                (Some(g), None) => format!("for {g}"),
                (None, Some(d)) => d,
                (None, None) => continue,
            };
            scoped.push((p, about));
        }
    }
    (sent, scoped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn token_counts() {
        assert_eq!([parse_tokens("400k"), parse_tokens("2M"), parse_tokens("1.5m"), parse_tokens("400_000"), parse_tokens("x"), parse_tokens("0")], [Some(400_000), Some(2_000_000), Some(1_500_000), Some(400_000), None, None]);
        assert_eq!([tokens(950), tokens(412_345), tokens(2_000_000), tokens(1_250_000)], ["950", "412k", "2m", "1.2m"]);
        let b = Budget { limit: 1000, spent: 999.into() };
        assert!(b.used().is_none());
        b.spent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        assert!(b.used().unwrap().starts_with("This session used its token budget of 1k (1k tokens"));
    }

    #[test]
    fn other_agents_rules() {
        let top = std::env::temp_dir().join(format!("harlo-agent-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        for d in [".git", ".github", ".clinerules", ".cursor/rules", "sub"] {
            std::fs::create_dir_all(top.join(d)).unwrap();
        }
        let w = |p: &str, t: &str| std::fs::write(top.join(p), t).unwrap();
        w(".github/copilot-instructions.md", "Use tabs.");
        w(".cursorrules", "  ");
        w(".clinerules/b.md", "Cline two.");
        w(".clinerules/a.md", "Cline one.");
        w(".cursor/rules/always.mdc", "---\ndescription: everywhere\nalwaysApply: true\n---\nAlways this.\n");
        w(".cursor/rules/ts.mdc", "---\ndescription: TypeScript style\nglobs: src/**/*.ts\nalwaysApply: false\n---\nTS only.\n");
        w(".cursor/rules/plain.mdc", "No frontmatter.\n");
        let (sent, scoped) = other_rules(&top.join("sub"));
        let names: Vec<String> = sent.iter().map(|(p, _)| p.strip_prefix(&top).unwrap().display().to_string()).collect();
        assert_eq!(names, [".github/copilot-instructions.md", ".clinerules/a.md", ".clinerules/b.md", ".cursor/rules/always.mdc", ".cursor/rules/plain.mdc"]);
        assert_eq!(sent[3].1.trim(), "Always this.");
        assert_eq!(scoped, [(top.join(".cursor/rules/ts.mdc"), "for src/**/*.ts: TypeScript style".to_owned())]);
        let text = instructions(&top.join("sub"), None);
        assert!(text.contains("Use tabs.") && text.contains("<project_rules>") && text.contains("ts.mdc (for src/**/*.ts: TypeScript style)") && !text.contains("TS only."), "{text}");
        let _ = std::fs::remove_dir_all(&top);
    }

    #[tokio::test]
    async fn stopping_ends_the_running_call_and_skips_the_rest() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tools = Tools::new(dir.clone(), dir.join("spill"));
        let marker = dir.join("late");
        let first = json!({ "command": "echo first" });
        let slow = json!({ "command": format!("sleep 30; touch {}", marker.display()) });
        let after = json!({ "command": format!("touch {}", marker.display()) });
        let stop = CancellationToken::new();
        let s2 = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            s2.cancel();
        });
        let t = Instant::now();
        let out = run_calls(&tools, &[("1", "bash", &first), ("2", "bash", &slow), ("3", "bash", &after)], &Extra::default(), &stop).await;
        assert!(t.elapsed() < Duration::from_secs(5), "the stop didn't end the command");
        assert_eq!(out[0].text, "first");
        assert_eq!(out[1].text, INTERRUPTED);
        assert_eq!(out[2].text, NOT_RUN);
        assert!(!marker.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn questions_and_their_answers() {
        assert!(ask_questions(&json!({ "questions": [] })).is_err());
        assert!(ask_questions(&json!({ "questions": [{ "question": "Which?", "options": [{ "label": "one" }] }] })).unwrap_err().contains("two options"));
        let q = ask_questions(&json!({ "questions": [
            { "question": "Which cache?", "header": "Cache", "options": [{ "label": "Redis", "description": "fast" }, { "label": "None" }] },
            { "question": "Which parts?", "multiple": true, "options": [{ "label": "api" }, { "label": "ui" }] },
        ] }))
        .unwrap();
        assert_eq!(q[0], json!({ "id": "q0", "header": "Cache", "question": "Which cache?", "multi": false, "options": [{ "label": "Redis", "description": "fast" }, { "label": "None", "description": "" }] }));
        assert_eq!((q[1]["id"].as_str(), q[1]["header"].as_str(), q[1]["multi"].as_bool()), (Some("q1"), Some("Question 2"), Some(true)));
        let text = ask_answers(&q, &json!([{ "id": "q0", "selectedOptions": ["Redis"] }, { "id": "q1", "selectedOptions": [], "customInput": "only the api, for now" }]));
        assert_eq!(text, "The user answered:\n- Which cache?\n  Redis\n- Which parts?\n  (in their words) only the api, for now");
    }

    #[test]
    fn the_instructions_list_the_folder() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-list-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("b.py"), "").unwrap();
        std::fs::write(dir.join("a.py"), "").unwrap();
        assert_eq!(listing(&dir).as_deref(), Some("a.py\nb.py\nsrc/"));
        for i in 0..60 {
            std::fs::write(dir.join(format!("f{i:02}")), "").unwrap();
        }
        let l = listing(&dir).unwrap();
        assert_eq!(l.lines().count(), LISTED + 1);
        assert!(l.ends_with("… and 13 more (use ls)"), "{l}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn stopping_reads_interrupts_them_all() {
        let dir = std::env::temp_dir();
        let tools = Tools::new(dir.clone(), dir.join("harlo-agent"));
        let stop = CancellationToken::new();
        stop.cancel();
        let args = json!({ "path": "." });
        let out = run_calls(&tools, &[("1", "ls", &args), ("2", "ls", &args)], &Extra::default(), &stop).await;
        assert!(out.iter().all(|o| o.failed));
    }

    /// harlo answering: a list, a tool's result, an error.
    struct FakeHarlo;
    impl Harlo for FakeHarlo {
        fn call(&self, tool: &str, args: &Value) -> BoxFuture<'static, Result<Value, String>> {
            let answer = match tool {
                "list_sessions" => Ok(json!({ "sessions": [], "asked": args.clone() })),
                "_instructions" => Ok(json!("What harlo's tools are for.")),
                "say" => Ok(json!("plain text")),
                _ => Err(format!("no {tool}")),
            };
            Box::pin(async move { answer })
        }
    }

    #[test]
    fn harlos_tools_as_the_model_gets_them() {
        let listed = json!([
            { "name": "start_session", "description": "Starts one.", "inputSchema": { "type": "object", "properties": { "agent": { "type": "string" } }, "required": ["agent"] } },
            { "name": "read", "description": "ours already", "inputSchema": {} },
            { "name": "", "description": "no name" },
            { "name": "list_agents", "description": "Lists them." },
        ]);
        let specs = harlo_specs(&listed);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0], json!({ "type": "function", "name": "start_session", "description": "Starts one.", "strict": false,
            "parameters": { "type": "object", "properties": { "agent": { "type": "string" } }, "required": ["agent"] } }));
        assert_eq!(specs[1]["parameters"], json!({ "type": "object", "properties": {} }));
    }

    #[tokio::test]
    async fn harlos_calls_go_to_harlo() {
        let dir = std::env::temp_dir();
        let tools = Tools::new(dir.clone(), dir.join("harlo-agent"));
        let specs = harlo_specs(&json!([{ "name": "list_sessions" }, { "name": "say" }, { "name": "start_session" }]));
        let (asked, none) = (json!({ "x": 1 }), json!({}));
        let calls = [("1", "list_sessions", &asked), ("2", "say", &none), ("3", "start_session", &none), ("4", "nope", &none)];
        let extra = Extra { harlo: Some((&FakeHarlo, &specs)), ..Default::default() };
        let out = run_calls(&tools, &calls, &extra, &CancellationToken::new()).await;
        assert_eq!((serde_json::from_str::<Value>(&out[0].text).unwrap(), out[0].failed), (json!({ "sessions": [], "asked": { "x": 1 } }), false));
        assert_eq!((out[1].text.as_str(), out[1].failed), ("plain text", false));
        assert_eq!((out[2].text.as_str(), out[2].failed), ("no start_session", true));
        // (Not one of harlo's: ours, and there's no such tool.)
        assert!(out[3].failed && out[3].text.contains("no tool called nope"), "{}", out[3].text);
    }

    #[test]
    fn a_run_is_told_to_wrap_up_near_the_cap() {
        assert_eq!(wrap_up_round(200), Some(160));
        assert_eq!(wrap_up_round(10), Some(8));
        assert_eq!(wrap_up_round(4), None);
    }

    #[test]
    fn what_hands_a_helpers_turn_over() {
        let o = |failed: bool| tools::Outcome::new("", failed);
        assert!(!hands_over(&["report_done"], &[o(true)]), "a refused report");
        assert!(hands_over(&["read", "report_done"], &[o(false), o(false)]));
        assert!(hands_over(&["ask_parent"], &[o(false)]));
        assert!(hands_over(&["request_approval"], &[o(false)]));
        assert!(!hands_over(&["list_sessions", "bash"], &[o(false), o(false)]));
    }
}
