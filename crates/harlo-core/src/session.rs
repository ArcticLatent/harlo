//! A live session: one harness process, its transcript, and the batching
//! that turns a high-rate event stream into a few UI updates per second.
//!
//! Views subscribe with a [`SessionSink`]. A new subscriber first receives
//! a full snapshot, then the same batched deltas as everyone else, so the
//! process keeps running whether or not anything is showing it.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use harlo_harness::{Harness, HarnessConfig, HarnessEvent, HarnessKind, SendMode, SessionTarget};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::transcript::{Row, RowChange, Transcript, diff_rows};

/// While events keep arriving, flush at most this often. Text is only
/// revealed in whole lines anyway, so faster flushes buy nothing.
const STREAM_FLUSH: Duration = Duration::from_millis(100);
/// Rows a long saved session shows before the rest of it.
const SHOWN_FIRST: usize = 200;
/// Rows this many from the end keep their long text in memory; replies
/// before them that are settled keep it in a page file (`harlo_core::pages`).
/// (More than a phone is sent.)
const KEPT_ROWS: usize = 1000;
/// How long after a session ends its memory is handed back.
const GIVE_BACK_AFTER: Duration = Duration::from_secs(2);

/// harlo's approval modes, strictest first (Harlo's `harlo_set_modes`).
pub const APPROVAL_MODES: [&str; 3] = ["always-ask", "write", "yolo"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// Reading a saved session from disk.
    Loading,
    /// Saved session shown from disk; no process yet.
    Offline,
    Starting,
    Idle,
    Busy,
    Exited,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Offline => "offline",
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Exited => "exited",
        }
    }

    pub fn has_process(self) -> bool {
        matches!(self, Self::Starting | Self::Idle | Self::Busy)
    }
}

/// A blocking dialog from the harness, flattened for display.
#[derive(Debug, Clone, Default)]
pub struct UiRequest {
    pub id: String,
    /// select | confirm | input | editor | ask
    pub method: String,
    pub title: String,
    pub message: String,
    pub options: Vec<String>,
    pub placeholder: String,
    pub prefill: String,
    /// A tool approval (`Allow tool: …` with Approve/Deny).
    pub is_approval: bool,
    /// Raw request, for methods that need more (`ask` questions).
    pub raw: Value,
}

/// Facts about the session that the sidebar and header show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionMetaInfo {
    pub session_file: Option<PathBuf>,
    pub session_id: String,
    pub name: String,
    pub first_message: String,
    pub model: String,
    /// The model as `provider/id` (what `--model` takes).
    pub model_id: String,
    pub thinking: String,
    /// Context used by the next request, from `get_session_stats`.
    pub context_tokens: u64,
    pub context_window: u64,
    /// The context size the agent compacts at on its own (0: it won't, or
    /// isn't known).
    pub compact_at: u64,
    /// The agent refused a message for want of a login or API key.
    pub needs_login: bool,
    /// The agent's last message was refused: its plan's limit is used up.
    pub limit_refused: bool,
    /// Session cost in USD so far.
    pub cost: f64,
    /// What that cost is, when the agent says (Harlo): "plan" or "local"
    /// (nothing extra), "priced", "partly" (some requests' costs unknown:
    /// at least `cost`), "unknown"; "" when it doesn't say.
    pub cost_kind: String,
}

/// A slash command the harness accepts in a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    /// Argument hint, e.g. `<plan|scan>`.
    pub hint: String,
}

#[derive(Debug, Clone)]
pub enum SessionUpdate {
    /// Full row list; sent to a new subscriber before any [`SessionUpdate::Rows`].
    Reset(Vec<Arc<Row>>),
    Rows(Vec<RowChange>),
    Status(SessionStatus),
    Meta(SessionMetaInfo),
    Queue { steering: Vec<String>, follow_up: Vec<String> },
    Commands(Vec<SlashCommand>),
    UiRequest(UiRequest),
    UiCleared(String),
    /// The user answered an approval request the session's agent made with
    /// harlo's tools (see [`SessionHandle::open_gate`]): what it asked,
    /// and the answer (`{ value }`, or `{ cancelled: true }`).
    GateAnswered { id: String, question: String, answer: Value },
    /// The run was stopped (the user's Stop, or another session's
    /// `stop_session`), not finished.
    Stopped,
    /// A status-bar entry from an extension (ANSI stripped); `None` clears it.
    StatusEntry { key: String, text: Option<String> },
    /// A subagent event (`subagent_lifecycle` / `subagent_progress` frame).
    SubagentActivity(Value),
    /// Todo phases (`[{ name, tasks: [{ content, status, blocker? }] }]`,
    /// `todoPhases` in `get_state`).
    Todos(Value),
    /// The agent calling one of harlo's tools on its pipe (harlo-agent):
    /// the workspace answers it.
    HarloCall { id: String, tool: String, args: Value },
    /// Worth a desktop notification (questions come as `UiRequest`).
    Notable(Notable),
    /// Plan limits, as Claude or Harlo last said (see `HarnessEvent::Limits`).
    Limits(Value),
    /// A side question and its answer so far (see `HarnessEvent::Aside`).
    Aside(Value),
    /// A new turn's snapshot is being taken (true) or done: its prompt
    /// waits meanwhile, and Stop drops it. (The status stays as it was, so
    /// a turn's start and end are still the run's.)
    Preparing(bool),
    /// Messages stopped before they were sent (the turn's snapshot was still
    /// being taken), oldest first, whole: they go back in the composer.
    Unsent(Vec<Unsent>),
}

/// A message as its sender typed it: the words (no plan note, no files or
/// sessions read in front) and the files attached. Held with the message,
/// so Stop before it's sent gives it back as it was.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Typed {
    pub words: String,
    pub files: Vec<String>,
    /// You sent it (from a composer, or as a session's first message), not
    /// an agent or a schedule: only then is `/plugin …` harlo's to run.
    pub by_you: bool,
}

/// A message stopped before it was sent (`SessionUpdate::Unsent`): as it
/// would have gone (`text`, `images` as `ImageContent` values, `mode`), and
/// as it was typed, when its sender said (`Typed`).
#[derive(Clone, Debug)]
pub struct Unsent {
    pub text: String,
    pub images: Vec<Value>,
    pub mode: SendMode,
    pub typed: Option<Typed>,
}

impl Unsent {
    /// Its words to give back: as typed, else as it would have gone, less
    /// the plan note and the photos' `<file>` lines in front.
    pub fn words(&self) -> String {
        if let Some(t) = &self.typed {
            return t.words.clone();
        }
        let text = self.text.strip_suffix(harlo_harness::file_blocks::PLAN_NOTE).unwrap_or(&self.text);
        let mut rest = text;
        while let Some(after) = rest.strip_prefix("<file name=\"Photo ").and_then(|r| r.split_once("\"></file>\n")).map(|(_, r)| r) {
            rest = after;
        }
        rest.to_owned()
    }
}

/// The ids of harlo's own requests (approvals its tools asked for).
pub const GATE_PREFIX: &str = "harlo-gate-";
/// Their title.
pub const GATE_TITLE: &str = "Approval requested";

/// Events the app may notify about.
#[derive(Debug, Clone, PartialEq)]
pub enum Notable {
    /// A run finished; the agent's last reply (plain text), and how many
    /// commands it left running in the background (it goes on when one
    /// finishes).
    Completed { last_message: String, background: usize },
    /// A run failed, or the harness reported an error.
    Error { message: String },
    /// A subagent finished (`status`: completed, failed, aborted).
    SubagentDone { id: String, agent: String, status: String, task: String },
}

/// Receives batched updates on the session task. Implementations forward
/// them to the UI thread.
pub trait SessionSink: Send + 'static {
    fn deliver(&self, updates: Vec<SessionUpdate>);
    /// Whether it has a box to type in, which takes messages back
    /// (`SessionUpdate::Unsent`): with none of those there, the session
    /// keeps them for the next one.
    fn composes(&self) -> bool {
        false
    }
}

enum Control {
    Subscribe(u64, Box<dyn SessionSink>),
    Unsubscribe(u64),
    Prompt(String, Vec<Value>, SendMode, Option<Typed>),
    Abort,
    Answer(String, Value),
    /// A request of harlo's (an approval its tools asked for): shown and
    /// answered as the agent's are, but the answer is harlo's.
    Gate(UiRequest),
    /// A raw command for the harness (starts it if needed).
    Raw(Value),
    /// A command whose response data the caller awaits.
    Request(Value, oneshot::Sender<Result<Value, String>>),
    /// Re-read the transcript and state from the harness (after a branch or fork).
    Reload,
    /// Stop the harness if it's idle; the next command starts it again on
    /// this session (it re-reads its config: models, thinking levels).
    RestartIdle,
    RestartWhenIdle,
    /// Context/usage report built from the session's messages.
    ContextReport(oneshot::Sender<Value>),
    /// The session as Markdown (crate::export).
    /// The session as Markdown: written to the file when there's one (and
    /// "" answered), else answered.
    Export(Option<PathBuf>, oneshot::Sender<Result<String, String>>),
}

/// Handle to a session driver. Commands are queued to the driver, which
/// starts the harness on demand. Dropping the last handle stops the harness.
pub struct SessionHandle {
    control: mpsc::UnboundedSender<Control>,
    next_sub: AtomicU64,
    /// Why it takes no messages now (its spending limit), if so.
    hold: std::sync::Mutex<Option<String>>,
}

/// Why no session takes messages now (the daily spending limit), if so.
static DAY_HOLD: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Holds every session's messages (`Some(why)`), or lets them go again.
pub fn set_day_hold(why: Option<String>) {
    *DAY_HOLD.lock().unwrap() = why;
}

impl SessionHandle {
    /// Starts a session with a running harness. Must be called inside a
    /// tokio runtime context.
    pub fn spawn(cfg: HarnessConfig) -> Self {
        Self::launch(cfg, true)
    }

    /// Shows a saved session from its file. The harness starts (resuming
    /// that file) only when a command needs it, e.g. the first prompt.
    pub fn open_saved(cfg: HarnessConfig) -> Self {
        Self::launch(cfg, false)
    }

    fn launch(cfg: HarnessConfig, spawn_now: bool) -> Self {
        let (control, control_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let long = drive(cfg, spawn_now, control_rx).await;
            // A long session's memory back, once the window has let go of its
            // rows too.
            if long {
                tokio::time::sleep(GIVE_BACK_AFTER).await;
                let _ = tokio::task::spawn_blocking(harlo_harness::store::give_back).await;
            }
        });
        Self { control, next_sub: AtomicU64::new(1), hold: Default::default() }
    }

    /// Registers a view. It receives a snapshot first, then deltas.
    pub fn subscribe(&self, sink: impl SessionSink) -> u64 {
        let id = self.next_sub.fetch_add(1, Ordering::Relaxed);
        let _ = self.control.send(Control::Subscribe(id, Box::new(sink)));
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        let _ = self.control.send(Control::Unsubscribe(id));
    }

    /// Sends a prompt; `images` are `ImageContent` values. False when it's
    /// held (a spending limit: `held` says why), and nothing was sent.
    pub fn prompt(&self, text: &str, images: Vec<Value>, mode: SendMode) -> bool {
        self.prompt_typed(text, images, mode, None)
    }

    /// `prompt`, with how it was typed: what Stop gives back if it comes
    /// before the message is sent (`SessionUpdate::Unsent`).
    pub fn prompt_typed(&self, text: &str, images: Vec<Value>, mode: SendMode, typed: Option<Typed>) -> bool {
        if self.held().is_some() {
            return false;
        }
        let _ = self.control.send(Control::Prompt(text.to_owned(), images, mode, typed));
        true
    }

    /// Why it takes no messages now (its own or the day's spending limit).
    pub fn held(&self) -> Option<String> {
        self.hold.lock().unwrap().clone().or_else(|| DAY_HOLD.lock().unwrap().clone())
    }

    pub fn set_hold(&self, why: Option<String>) {
        *self.hold.lock().unwrap() = why;
    }

    pub fn abort(&self) {
        let _ = self.control.send(Control::Abort);
    }

    pub fn answer_ui(&self, request_id: &str, answer: Value) {
        let _ = self.control.send(Control::Answer(request_id.to_owned(), answer));
    }

    /// Asks the user to decide `question` (one of `options`), as the
    /// agent's own questions are asked; the answer comes as
    /// [`SessionUpdate::GateAnswered`]. `title`: "" for [`GATE_TITLE`]. Its id.
    pub fn open_gate(&self, title: &str, question: &str, options: Vec<String>) -> String {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = format!("{GATE_PREFIX}{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let raw = json!({ "id": id, "method": "select", "title": if title.is_empty() { GATE_TITLE } else { title }, "message": question, "options": options, "gate": true });
        let _ = self.control.send(Control::Gate(ui_request(&raw)));
        id
    }

    /// Re-reads the messages and state from the harness, e.g. after it
    /// moved to another branch or a forked session file.
    pub fn reload(&self) {
        let _ = self.control.send(Control::Reload);
    }

    /// Stops the harness if it's idle (not working, nothing asked of you);
    /// the session starts it again when next used, with its config re-read.
    pub fn restart_if_idle(&self) {
        let _ = self.control.send(Control::RestartIdle);
    }

    /// The same, or (working) as soon as it has finished.
    pub fn restart_when_idle(&self) {
        let _ = self.control.send(Control::RestartWhenIdle);
    }

    pub fn send_raw(&self, command: Value) {
        let _ = self.control.send(Control::Raw(command));
    }

    /// Token usage, message counts and a per-message log (see
    /// [`crate::context::report`]). Never starts the harness.
    pub async fn context_report(&self) -> Option<Value> {
        let (tx, rx) = oneshot::channel();
        self.control.send(Control::ContextReport(tx)).ok()?;
        rx.await.ok()
    }

    /// The session as Markdown (see [`crate::export`]). Never starts the
    /// harness. An error says why there's none (never a partial one).
    pub async fn export_markdown(&self) -> Result<String, String> {
        self.export(None).await
    }

    /// [`Self::export_markdown`] written to `path` as it's made (a long
    /// session's never held whole).
    pub async fn export_markdown_to(&self, path: PathBuf) -> Result<(), String> {
        self.export(Some(path)).await.map(drop)
    }

    async fn export(&self, to: Option<PathBuf>) -> Result<String, String> {
        let closed = || "the session closed".to_owned();
        let (tx, rx) = oneshot::channel();
        self.control.send(Control::Export(to, tx)).map_err(|_| closed())?;
        rx.await.map_err(|_| closed())?
    }

    /// Sends a command (starting the harness if needed) and returns the
    /// response `data`.
    pub async fn request(&self, command: Value) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        self.control.send(Control::Request(command, tx)).map_err(|_| "session closed".to_owned())?;
        rx.await.map_err(|_| "session closed".to_owned())?
    }
}

struct DriverState {
    transcript: Transcript,
    /// While a saved session's file loads: what came meanwhile (a message
    /// you sent, its reply), for the loaded transcript too.
    since_open: Option<Vec<HarnessEvent>>,
    shown: Vec<Arc<Row>>,
    status: SessionStatus,
    meta: SessionMetaInfo,
    queue: (Vec<String>, Vec<String>),
    commands: Vec<SlashCommand>,
    requests: Vec<UiRequest>,
    status_entries: Vec<(String, String)>,
    /// Todo phases, from the last `get_state`.
    todos: Value,
    /// Calls of the agent's `todo` tool still running.
    todo_calls: std::collections::HashSet<String>,
    /// Commands the agent left running in the background.
    background: usize,
    /// Asked for the session file during this run (a new session gets
    /// one only once its first message is under way).
    file_asked: bool,
    sinks: Vec<(u64, Box<dyn SessionSink>)>,
    /// Messages given back while nothing with a box to type in was there:
    /// for the next one that is.
    unsent: Vec<Unsent>,
    /// The text of the last reply that had some (for the finished
    /// notification).
    last_reply: String,
    /// How many messages the conversation has (about: those loaded and
    /// those ended since).
    message_count: usize,
    /// The context report and Activity log so far (not the messages: they
    /// aren't kept), and the latest messages not in it yet (one may end
    /// again, as it's filled in).
    tally: crate::context::Tally,
    recent: Vec<(String, Value)>,
    /// Why the current run failed, if it did (reported when it settles).
    run_error: Option<String>,
    /// Subagents already reported as finished.
    subagents_done: std::collections::HashSet<String>,
    /// The current run was stopped by you.
    aborted: bool,
}

/// The last assistant reply's text (plain, trimmed), if it has some.
fn reply_text(message: &Value) -> Option<String> {
    if message["role"] != "assistant" {
        return None;
    }
    let text = match &message["content"] {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    };
    Some(text.trim().to_owned()).filter(|t| !t.is_empty())
}

/// From this many messages, what reading them used goes back to the system.
const MANY: usize = 2000;

/// Where long sessions' last context reports are kept: beside the history
/// cache. (Tests: a folder a test sets, else none.)
fn reports_dir() -> Option<PathBuf> {
    #[cfg(test)]
    return REPORTS_IN.lock().unwrap().clone();
    #[cfg(not(test))]
    harlo_harness::claude::cache_dir()
}
#[cfg(test)]
static REPORTS_IN: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
/// (Tests setting `REPORTS_IN` take turns.)
#[cfg(test)]
static REPORTS_TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The report last kept for a session file (see [`save_report`]).
fn saved_report(path: &std::path::Path) -> Option<Value> {
    let file = harlo_harness::claude::report_file(&reports_dir()?, path);
    serde_json::from_slice(&std::fs::read(file).ok()?).ok()
}

/// Keeps a long session's report for the next time it opens: the Context
/// and Activity panels show it at once, until the file has loaded (off the
/// session's loop).
fn save_report(path: PathBuf, report: Value) {
    let Some(dir) = reports_dir() else { return };
    tokio::task::spawn_blocking(move || {
        let file = harlo_harness::claude::report_file(&dir, &path);
        keep_file(&dir, &file, &serde_json::to_vec(&report).unwrap_or_default());
    });
}

/// Writes `file` in `dir` whole or not at all.
fn keep_file(dir: &std::path::Path, file: &std::path::Path, data: &[u8]) {
    static N: AtomicU64 = AtomicU64::new(0);
    let tmp = file.with_extension(format!("{}.{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let wrote = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&tmp, data)).and_then(|_| std::fs::rename(&tmp, file));
    if let Err(e) = wrote {
        tracing::debug!("{} wasn't kept: {e}", file.display());
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Where a project's "/" commands are kept, for an agent (and machine):
/// a session that hasn't started its agent yet offers them.
fn commands_file(dir: &std::path::Path, cfg: &HarnessConfig) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (cfg.kind.as_str(), cfg.remote.as_ref().map(|r| r.host.as_str()), &cfg.cwd).hash(&mut h);
    dir.join("commands").join(format!("{}-{:016x}.json", cfg.kind.as_str(), h.finish()))
}

/// The commands last kept for this agent in this folder, else harlo's own.
fn saved_commands(cfg: &HarnessConfig) -> Vec<SlashCommand> {
    let kept = reports_dir().and_then(|d| std::fs::read(commands_file(&d, cfg)).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    parse_commands(&kept.unwrap_or(Value::Null))
}

/// Keeps the agent's commands (as it listed them) for the next time.
fn save_commands(cfg: &HarnessConfig, list: &Value) {
    let Some(dir) = reports_dir().filter(|_| list.is_array()) else { return };
    let (file, data) = (commands_file(&dir, cfg), serde_json::to_vec(list).unwrap_or_default());
    tokio::task::spawn_blocking(move || keep_file(file.parent().unwrap_or(&dir), &file, &data));
}

/// The text a long session's kept rows hold at most.
const KEPT_ROWS_TEXT: usize = 4 << 20;

/// The rows last kept for a session file (see [`save_rows`]).
fn saved_rows(path: &std::path::Path) -> Option<Vec<Arc<Row>>> {
    let file = harlo_harness::claude::rows_file(&reports_dir()?, path);
    let rows: Vec<Row> = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    Some(rows.into_iter().map(Arc::new).collect()).filter(|r: &Vec<_>| !r.is_empty())
}

/// Keeps a long session's last rows (about [`SHOWN_FIRST`], and about
/// [`KEPT_ROWS_TEXT`] of text) for the next time it opens: they show at
/// once, until the file has loaded (off the session's loop).
fn save_rows(path: PathBuf, rows: &[Arc<Row>]) {
    let Some(dir) = reports_dir() else { return };
    let (mut start, mut text) = (rows.len(), 0);
    // (`last_rows` starts at a message of yours, a little past SHOWN_FIRST.)
    while start > 0 && rows.len() - start < 2 * SHOWN_FIRST {
        let r = &rows[start - 1];
        let size = r.html.len() + r.text.len() + r.result.len() + r.diff.len() + r.command.len() + r.summary.len();
        // (Rows with their text in a page file stay out, as does the
        // row before them.)
        if r.paged.is_some() || (text + size > KEPT_ROWS_TEXT && start < rows.len()) {
            break;
        }
        text += size;
        start -= 1;
    }
    let tail = rows[start..].to_vec();
    tokio::task::spawn_blocking(move || {
        let file = harlo_harness::claude::rows_file(&dir, &path);
        let rows: Vec<&Row> = tail.iter().map(|r| &**r).collect();
        keep_file(&dir, &file, &serde_json::to_vec(&rows).unwrap_or_default());
    });
}

/// A saved session opened from its file, or a remote one's copy brought up
/// to date over SSH: its messages read one at a time into its transcript,
/// replies far back paged out as it goes, never all of them held at once.
#[derive(Default)]
struct Opened {
    /// What the file says besides its messages (none here).
    saved: harlo_harness::store::SavedSession,
    /// A remote one's: how much of the host's file didn't come (its end).
    missing: u64,
    transcript: Transcript,
    tally: crate::context::Tally,
    count: usize,
    first_message: Option<String>,
    last_reply: String,
    /// The context its last reply says it used (Claude's).
    context: u64,
}

/// Messages read between pagings while opening.
const PAGE_EVERY: usize = 4096;

impl Opened {
    /// The next message, into its transcript (its tally is added up beside it).
    fn add(&mut self, m: &Value) {
        if self.first_message.is_none() {
            self.first_message = user_text(&m);
        }
        if let Some(text) = reply_text(&m) {
            self.last_reply = text;
        }
        if let Some(n) = harlo_harness::claude::context_of(&m) {
            self.context = n;
        }
        self.transcript.load_one(self.count, m);
        self.count += 1;
        if self.count % PAGE_EVERY == 0 {
            self.transcript.rows();
            self.transcript.page_out(KEPT_ROWS);
        }
    }
}

/// Read a message at a time: each is added to the transcript and the tally
/// as it comes, so the whole file is never held at once.
fn open_session_file(kind: HarnessKind, path: &std::path::Path) -> Opened {
    let mut opened = Opened::default();
    opened.transcript.begin_load();
    let mut tally = crate::context::Tally::default();
    let mut pass = |m: Value| {
        tally.add(&m);
        opened.add(&m);
    };
    let (missing, size, read) = if crate::remote::is_remote(path) {
        match crate::remote::local_copy(path) {
            Ok(copy) => {
                let size = std::fs::metadata(&copy).map_or(0, |m| m.len());
                (harlo_harness::mirror::missing(&copy), size, harlo_harness::store::stream_saved_for_upto(kind, &copy, size, &mut pass).map_err(anyhow::Error::from))
            }
            Err(e) => (0, 0, Err(e)),
        }
    } else {
        // (As long as it is now: what its agent adds while this reads, if
        // something's sent meanwhile, comes from the agent.)
        let size = std::fs::metadata(path).map_or(0, |m| m.len());
        (0, size, harlo_harness::store::stream_saved_for_upto(kind, path, size, &mut pass).map_err(anyhow::Error::from))
    };
    (opened.tally, opened.missing) = (tally, missing);
    match read {
        Ok(saved) => {
            opened.saved = saved;
            opened.transcript.rows();
            opened.transcript.page_out(KEPT_ROWS);
        }
        // (Shown as nothing, as before a read that fails partway.)
        Err(e) => {
            tracing::warn!("reading {}: {e:#}", path.display());
            opened = Opened { missing: opened.missing, ..Default::default() };
        }
    }
    harlo_harness::store::after_reading(size);
    opened
}

/// The session as Markdown, written to `to` (and "" answered) or answered,
/// from its messages passed on one at a time: from the session file (a
/// remote one's local copy), first for Claude, whose transcript has every
/// message that ended, and when nothing runs; else from the running
/// agent, which knows the branch it's on. A file that can't be read is an
/// error, never part of an export.
async fn export_markdown(harness: Option<Arc<Harness>>, kind: HarnessKind, file: Option<PathBuf>, info: crate::export::Info, to: Option<PathBuf>) -> Result<String, String> {
    let file_first = kind == HarnessKind::Claude || harness.is_none();
    let from_file = |file: PathBuf| {
        let (info, to) = (info.clone(), to.clone());
        async move {
            let wrote = tokio::task::spawn_blocking(move || {
                let unread = |e: String| format!("its conversation couldn't be read ({e})");
                let local = if crate::remote::is_remote(&file) {
                    let copy = crate::remote::local_copy(&file).map_err(|e| unread(format!("{e:#}")))?;
                    whole_copy(&copy, &crate::remote::split(&file).map(|(host, _)| host).unwrap_or_default())?;
                    copy
                } else {
                    file
                };
                // (Both of the export's readings stop where the file ended
                // now: a call added meanwhile is in neither, never in the
                // second without its result from the first.)
                let size = std::fs::metadata(&local).map_or(0, |m| m.len());
                let mut read_failed = false;
                let wrote = write_export(
                    &mut |each| harlo_harness::store::each_saved_message_upto(kind, &local, size, each).inspect_err(|_| read_failed = true),
                    &info,
                    to.as_deref(),
                );
                harlo_harness::store::after_reading(size);
                wrote.map_err(|e| if read_failed { unread(e.to_string()) } else { format!("couldn't write it ({e})") })
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            wrote
        }
    };
    let mut failed = None;
    if file_first && let Some(file) = file.clone() {
        match from_file(file).await {
            Err(e) if harness.is_some() => {
                tracing::warn!("export from the session file: {e}");
                failed = Some(e);
            }
            done => return done,
        }
    }
    if let Some(h) = harness {
        match h.request(json!({ "type": "get_messages" })).await {
            Ok(mut data) => match serde_json::from_value::<Vec<Value>>(data["messages"].take()) {
                Ok(messages) => {
                    let (info, to) = (info.clone(), to.clone());
                    return tokio::task::spawn_blocking(move || {
                        let wrote = write_export(
                            &mut |each| {
                                messages.iter().cloned().for_each(each);
                                Ok(())
                            },
                            &info,
                            to.as_deref(),
                        );
                        done_with(messages);
                        wrote.map_err(|e| format!("couldn't write it ({e})"))
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                }
                Err(e) => tracing::warn!("get_messages answered oddly: {e}"),
            },
            Err(e) => tracing::warn!("get_messages failed: {e}"),
        }
        if !file_first && let Some(file) = file {
            return from_file(file).await;
        }
        return Err(failed.unwrap_or_else(|| "its conversation couldn't be read".to_owned()));
    }
    // (Nothing running and no file: a session with nothing in it yet.)
    write_export(&mut |_| Ok(()), &info, to.as_deref()).map_err(|e| e.to_string())
}

/// A remote session's copy here, if it all came over: an export of part
/// of it (its end missing, the host out of reach) is an error, never a
/// shorter Markdown passed off as the whole.
fn whole_copy(copy: &std::path::Path, host: &str) -> Result<(), String> {
    match harlo_harness::mirror::missing(copy) {
        0 => Ok(()),
        n => Err(format!(
            "not all of it came over from {host}: its last {} (the latest messages) are missing. Try again once {host} can be reached",
            crate::attach::human_size(n)
        )),
    }
}

/// The Markdown of the messages `read` passes on, written to `to` (through a
/// file beside it, put in its place once whole) or answered (whole in
/// memory anyway, so nothing spills).
fn write_export(read: &mut dyn FnMut(&mut dyn FnMut(Value)) -> std::io::Result<()>, info: &crate::export::Info, to: Option<&std::path::Path>) -> std::io::Result<String> {
    let Some(to) = to else {
        let mut out = Vec::new();
        crate::export::write_markdown(read, info, &mut out)?;
        return String::from_utf8(out).map_err(std::io::Error::other);
    };
    let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let part = to.with_file_name(format!(".{name}.part"));
    let written = (|| {
        let mut out = std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create(&part)?);
        // (What waits behind a late result spills beside it, not into memory.)
        crate::export::write_markdown_spilling(read, info, &mut out, to.parent())?;
        std::io::Write::flush(&mut out)?;
        drop(out);
        std::fs::rename(&part, to)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written.map(|()| String::new())
}

/// After reading a conversation: what reading it used goes back to the
/// system (on a blocking thread: trimming a big heap takes a moment).
fn done_with(messages: Vec<Value>) {
    if messages.is_empty() {
        return;
    }
    drop(messages);
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => drop(rt.spawn_blocking(harlo_harness::store::give_back)),
        Err(_) => harlo_harness::store::give_back(),
    }
}

/// Messages that ended kept out of the tally for a while (one may end
/// again: a reply filled in after it streamed).
const RECENT: usize = 32;

impl DriverState {
    /// An event into the transcript (kept for the loaded one, while it loads).
    fn apply(&mut self, ev: &HarnessEvent) -> bool {
        if let Some(since) = &mut self.since_open {
            since.push(ev.clone());
        }
        self.transcript.apply(ev)
    }

    /// The context report (the Context and Activity panels', the phone's):
    /// the tally with the messages ended since, and the agent's own figures.
    fn report(&self) -> Value {
        let live = (self.meta.context_window > 0).then_some((self.meta.context_tokens, self.meta.context_window, self.meta.cost));
        let mut tally = self.tally.clone();
        for (_, m) in &self.recent {
            tally.add(m);
        }
        let mut report = tally.report(live);
        report["costKind"] = json!(self.meta.cost_kind);
        report
    }

    /// A message that ended (again, if it's one of the latest).
    fn ended(&mut self, id: &str, message: &Value) {
        match self.recent.iter_mut().find(|(i, _)| i == id) {
            Some(m) => m.1 = message.clone(),
            None => self.recent.push((id.to_owned(), message.clone())),
        }
        if self.recent.len() > RECENT {
            let (_, m) = self.recent.remove(0);
            self.tally.add(&m);
        }
    }

    fn snapshot(&self) -> Vec<SessionUpdate> {
        let mut out = vec![
            SessionUpdate::Reset(self.shown.clone()),
            SessionUpdate::Status(self.status),
            SessionUpdate::Meta(self.meta.clone()),
            SessionUpdate::Queue { steering: self.queue.0.clone(), follow_up: self.queue.1.clone() },
            SessionUpdate::Commands(self.commands.clone()),
        ];
        out.extend(self.requests.iter().cloned().map(SessionUpdate::UiRequest));
        if self.todos.as_array().is_some_and(|a| !a.is_empty()) {
            out.push(SessionUpdate::Todos(self.todos.clone()));
        }
        out.extend(
            self.status_entries
                .iter()
                .map(|(k, v)| SessionUpdate::StatusEntry { key: k.clone(), text: Some(v.clone()) }),
        );
        out
    }

    fn broadcast(&self, updates: Vec<SessionUpdate>) {
        if let Some(((_, last), rest)) = self.sinks.split_last() {
            for (_, sink) in rest {
                sink.deliver(updates.clone());
            }
            last.deliver(updates);
        }
    }
}

struct Driver {
    cfg: HarnessConfig,
    harness: Option<Arc<Harness>>,
    events: Option<mpsc::UnboundedReceiver<HarnessEvent>>,
    /// Ask the harness for its message history once it is ready.
    fetch_history: bool,
    aux_tx: mpsc::UnboundedSender<Aux>,
    /// The modes last set, for when it's started again.
    modes: Option<Value>,
    /// Claude: the thinking level to start on.
    thinking: Option<String>,
    /// Stop it once it has finished what it's doing.
    restart_settled: bool,
}

impl Driver {
    /// Stops an idle harness; the next message starts it again.
    fn stop_idle(&mut self, st: &mut DriverState, pending: &mut Vec<SessionUpdate>) {
        self.restart_settled = false;
        if let Some(h) = self.harness.take() {
            // (Its events dropped first: no "exited" notice for it.)
            self.events = None;
            h.kill();
            set_status(st, SessionStatus::Offline, pending);
            if st.meta.needs_login {
                st.meta.needs_login = false;
                pending.push(SessionUpdate::Meta(st.meta.clone()));
            }
        }
    }

    /// Starts the harness unless one is running. A restart after an exit
    /// resumes the session file, so the conversation continues.
    fn ensure_harness(&mut self, st: &mut DriverState, pending: &mut Vec<SessionUpdate>) -> Option<Arc<Harness>> {
        if st.status.has_process()
            && let Some(h) = &self.harness {
                return Some(h.clone());
            }
        let mut cfg = self.cfg.clone();
        if let Some(file) = &st.meta.session_file {
            cfg.session = SessionTarget::Resume(file.clone());
        }
        if let Some(level) = &self.thinking {
            cfg.extra_args.push(format!("{}{level}", harlo_harness::claude::THINKING_ARG));
        }
        let gone = (cfg.remote.is_none() && !cfg.cwd.is_dir()).then(|| cfg.cwd.clone());
        match Harness::spawn(cfg) {
            Ok((harness, events)) => {
                let harness = Arc::new(harness);
                self.harness = Some(harness.clone());
                self.events = Some(events);
                set_status(st, SessionStatus::Starting, pending);
                // The modes it's on, before anything it's sent.
                if let Some(modes) = self.modes.clone() {
                    harness.send(modes);
                }
                Some(harness)
            }
            Err(e) => {
                st.apply(&match gone {
                    Some(cwd) => HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Error, text: format!("This session can't continue: its folder, {}, is gone.", cwd.display()), system: false },
                    None => HarnessEvent::ProtocolError(format!("{e:#}")),
                });
                set_status(st, SessionStatus::Exited, pending);
                None
            }
        }
    }
}

/// Snapshots before each turn (undo a turn), unless Settings turned them
/// off (`undo_snapshots` "0": for a repository with huge untracked files).
pub fn turn_snapshots_on() -> bool {
    crate::db::Db::open(&crate::db::default_path()).ok().and_then(|d| d.get("settings.undo_snapshots").ok().flatten()).as_deref() != Some("0")
}

/// Messages that weren't sent, back to the box they were typed in, with
/// a word in the transcript: now if one is there, else kept for the next
/// (a Stop from elsewhere, a spending limit, while it's open nowhere).
fn give_back(st: &mut DriverState, pending: &mut Vec<SessionUpdate>, unsent: Vec<Unsent>, why: &str) {
    if unsent.is_empty() {
        return;
    }
    let said = if unsent.len() == 1 { format!("{why}: your message is back in the box.") } else { format!("{why}: your messages are back in the box.") };
    st.apply(&HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Info, text: said, system: false });
    if st.sinks.iter().any(|(_, s)| s.composes()) {
        pending.push(SessionUpdate::Unsent(unsent));
    } else {
        st.unsent.extend(unsent);
    }
}

fn set_status(st: &mut DriverState, s: SessionStatus, pending: &mut Vec<SessionUpdate>) {
    if st.status != s {
        st.status = s;
        pending.push(SessionUpdate::Status(s));
    }
}

/// Runs a session until it's dropped; true if it was a long one.
async fn drive(cfg: HarnessConfig, spawn_now: bool, mut control: mpsc::UnboundedReceiver<Control>) -> bool {
    let kind = cfg.kind;
    let session_file = match &cfg.session {
        SessionTarget::Resume(p) => Some(p.clone()),
        _ => None,
    };
    let (aux_tx, mut aux_rx) = mpsc::unbounded_channel::<Aux>();
    // Markdown exports asked for while the saved messages load.
    let mut exports: Vec<(Option<PathBuf>, oneshot::Sender<Result<String, String>>)> = Vec::new();
    // While the file loads: the report kept from last time, or the reports
    // asked for until there's one.
    let mut early: Option<Value> = None;
    let mut early_asked: Vec<oneshot::Sender<Value>> = Vec::new();
    // A run was going (its end keeps a long session's report).
    let mut was_busy = false;
    let mut st = DriverState {
        transcript: Transcript::new(),
        since_open: (!spawn_now && session_file.is_some()).then(Vec::new),
        shown: Vec::new(),
        status: if spawn_now { SessionStatus::Starting } else { SessionStatus::Loading },
        meta: SessionMetaInfo { session_file: session_file.clone(), ..Default::default() },
        queue: Default::default(),
        // (Those its agent listed last time in this folder: it may not
        // start until something is sent.)
        commands: saved_commands(&cfg),
        requests: Vec::new(),
        status_entries: Vec::new(),
        todos: Value::Null,
        todo_calls: Default::default(),
        background: 0,
        file_asked: false,
        sinks: Vec::new(),
        unsent: Vec::new(),
        last_reply: String::new(),
        message_count: 0,
        tally: Default::default(),
        recent: Vec::new(),
        run_error: None,
        subagents_done: Default::default(),
        aborted: false,
    };
    let mut driver = Driver {
        fetch_history: spawn_now && session_file.is_some(),
        cfg,
        harness: None,
        events: None,
        aux_tx: aux_tx.clone(),
        modes: None,
        thinking: None,
        restart_settled: false,
    };
    let mut pending: Vec<SessionUpdate> = Vec::new();
    let mut dirty = false;
    let mut last_flush = Instant::now() - STREAM_FLUSH;
    // A turn waiting for its snapshot: its prompt (and any sent meanwhile),
    // and the snapshot's number.
    let mut held: Vec<Unsent> = Vec::new();
    let mut snap_turn: u64 = 0;
    // `/plugin …` runs so far (their notices' ids).
    let mut plugin_runs: u64 = 0;
    // A conversation was just loaded: once its rows are built and sent,
    // what building them used goes back to the system.
    let mut loaded = false;
    // Rows kept from the last time are shown, until it has loaded (with
    // what comes meanwhile under them).
    let mut early_rows = false;
    let mut early_list: Vec<Arc<Row>> = Vec::new();

    if spawn_now {
        driver.ensure_harness(&mut st, &mut pending);
    } else if let Some(path) = session_file.clone() {
        let tx = aux_tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(Aux::Early(saved_report(&path).map(Box::new)));
            if let Some(rows) = saved_rows(&path) {
                let _ = tx.send(Aux::EarlyRows(rows));
            }
            let mut opened = open_session_file(kind, &path);
            if opened.count >= MANY {
                // A long one's rows are built here, not on the session's
                // loop, where a message sent meanwhile would wait for them
                // (seconds): its latest first, shown while the rest are.
                let _ = tx.send(Aux::EarlyRows(opened.transcript.last_rows(SHOWN_FIRST)));
                opened.transcript.rows();
                opened.transcript.page_out(KEPT_ROWS);
                harlo_harness::store::give_back();
            }
            let _ = tx.send(Aux::Loaded(Box::new(opened)));
        });
    }

    loop {
        let wait = if dirty || !pending.is_empty() {
            STREAM_FLUSH.saturating_sub(last_flush.elapsed())
        } else {
            Duration::from_secs(3600)
        };
        tokio::select! {
            ev = async { driver.events.as_mut().expect("guarded").recv().await }, if driver.events.is_some() => {
                let Some(ev) = ev else {
                    driver.events = None;
                    driver.harness = None;
                    driver.restart_settled = false;
                    set_status(&mut st, SessionStatus::Exited, &mut pending);
                    continue;
                };
                handle_event(&ev, &mut st, &mut pending, &mut driver);
                // Its host disconnected by hand: said as that, not as a failure.
                let by_hand = matches!(ev, HarnessEvent::Exited { .. }).then(|| driver.cfg.remote.as_ref().map(|r| r.host.clone()).filter(|h| crate::remote::is_disconnected(h))).flatten();
                dirty |= match by_hand {
                    Some(host) => st.apply(&HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Info, text: format!("Disconnected from {host}."), system: false }),
                    None => st.apply(&ev),
                };
                let login = match &ev {
                    HarnessEvent::Response { success: false, error: Some(e), .. } => Some(asks_for_login(e)),
                    HarnessEvent::Exited { stderr_tail, .. } => Some(asks_for_login(stderr_tail)),
                    // (How Claude says it.)
                    HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Error, text, .. } => asks_for_login(text).then_some(true),
                    HarnessEvent::MessageStart { .. } => Some(false),
                    _ => None,
                };
                let refused = match &ev {
                    HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Error, text, .. } => refused_by_limit(text).then_some(true),
                    HarnessEvent::MessageStart { .. } => Some(false),
                    _ => None,
                };
                if let Some(refused) = refused.filter(|r| *r != st.meta.limit_refused) {
                    st.meta.limit_refused = refused;
                    pending.push(SessionUpdate::Meta(st.meta.clone()));
                }
                if let Some(login) = login.filter(|l| *l != st.meta.needs_login) {
                    st.meta.needs_login = login;
                    pending.push(SessionUpdate::Meta(st.meta.clone()));
                }
                if driver.restart_settled && st.status == SessionStatus::Idle {
                    driver.stop_idle(&mut st, &mut pending);
                }
            }
            Some(aux) = aux_rx.recv() => match aux {
                Aux::Loaded(opened) => {
                    let Opened { saved, missing, transcript, tally, count, first_message, last_reply, context } = *opened;
                    st.transcript = transcript;
                    // (What came while it loaded, after it.)
                    for ev in st.since_open.take().unwrap_or_default() {
                        st.transcript.apply(&ev);
                    }
                    early_rows = false;
                    early_list = Vec::new();
                    // (A copy is the start of the host's file: what didn't
                    // come is the end, its latest messages.)
                    if missing > 0 {
                        let host = driver.cfg.remote.as_ref().map(|r| r.host.clone()).unwrap_or_default();
                        let text = format!(
                            "Not all of this session came over from {host}: its last {} (the latest messages) aren't shown. Close it and open it again to fetch the rest.",
                            crate::attach::human_size(missing)
                        );
                        st.apply(&HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Warning, text, system: false });
                    }
                    st.meta.first_message = first_message.unwrap_or_default();
                    if !saved.name.is_empty() {
                        st.meta.name = saved.name;
                    }
                    st.meta.model = saved.model;
                    // (Until the agent runs and says: e.g. to start its server.)
                    if st.meta.model_id.is_empty() {
                        st.meta.model_id = saved.model_id;
                    }
                    st.meta.thinking = driver.thinking.clone().unwrap_or(saved.thinking);
                    // Claude's context before it runs: the last reply's, in
                    // the window Claude said last time for its model.
                    if kind == HarnessKind::Claude && st.meta.context_window == 0
                        && let Some((window, at)) = harlo_harness::claude::known_context(&st.meta.model)
                    {
                        (st.meta.context_tokens, st.meta.context_window, st.meta.compact_at) = (context, window, at);
                    }
                    // None set: what Claude's settings have it on.
                    if kind == HarnessKind::Claude && st.meta.thinking.is_empty() && driver.cfg.remote.is_none() {
                        st.meta.thinking = harlo_harness::claude::default_effort();
                    }
                    pending.push(SessionUpdate::Meta(st.meta.clone()));
                    if st.status == SessionStatus::Loading {
                        set_status(&mut st, SessionStatus::Offline, &mut pending);
                    }
                    // A long one: its latest rows at once, the rest above them
                    // once they're built.
                    if count >= MANY {
                        let first = st.transcript.last_rows(SHOWN_FIRST);
                        let changes = diff_rows(&st.shown, &first);
                        if let Some(path) = &session_file {
                            save_rows(path.clone(), &first);
                        }
                        st.shown = first;
                        if !changes.is_empty() {
                            pending.push(SessionUpdate::Rows(changes));
                        }
                        st.broadcast(std::mem::take(&mut pending));
                        last_flush = Instant::now();
                    }
                    st.last_reply = last_reply;
                    st.message_count = count;
                    (st.tally, st.recent) = (tally, Vec::new());
                    early = None;
                    if !early_asked.is_empty() || count >= MANY {
                        let report = st.report();
                        for reply in early_asked.drain(..) {
                            let _ = reply.send(report.clone());
                        }
                        if count >= MANY && let Some(path) = &session_file {
                            save_report(path.clone(), report);
                        }
                    }
                    loaded = true;
                    for (to, reply) in exports.drain(..) {
                        export(&st, &driver, to, reply);
                    }
                    dirty = true;
                }
                // The rows kept from last time, shown while it loads.
                // (Kept from last time, or its latest once read: with what
                // came since under them.)
                Aux::EarlyRows(rows) => {
                    if st.since_open.is_some() {
                        early_list = rows;
                        early_rows = true;
                        let mut rows = early_list.clone();
                        rows.extend(st.transcript.rows());
                        let changes = diff_rows(&st.shown, &rows);
                        st.shown = rows;
                        pending.push(SessionUpdate::Rows(changes));
                        st.broadcast(std::mem::take(&mut pending));
                        last_flush = Instant::now();
                    }
                }
                Aux::Early(report) => {
                    if let (Some(report), SessionStatus::Loading) = (report, st.status) {
                        for reply in early_asked.drain(..) {
                            let _ = reply.send((*report).clone());
                        }
                        early = Some(*report);
                    }
                }
                Aux::History(messages, tally) => {
                    st.transcript.load_messages(&messages);
                    if st.meta.first_message.is_empty() {
                        st.meta.first_message = first_user_text(&messages);
                        pending.push(SessionUpdate::Meta(st.meta.clone()));
                    }
                    st.last_reply = messages.iter().rev().find_map(reply_text).unwrap_or_default();
                    st.message_count = messages.len();
                    (st.tally, st.recent) = (tally, Vec::new());
                    done_with(messages);
                    loaded = true;
                    dirty = true;
                }
                Aux::State(state) => {
                    if let Some(todos) = state.get("todoPhases").filter(|t| t.is_array())
                        && *todos != st.todos {
                            st.todos = todos.clone();
                            pending.push(SessionUpdate::Todos(st.todos.clone()));
                        }
                    let mut meta = st.meta.clone();
                    apply_state(&mut meta, &state);
                    // A remote harness reports its own paths.
                    if let (Some(remote), Some(file)) = (&driver.cfg.remote, &meta.session_file)
                        && !crate::remote::is_remote(file) {
                            meta.session_file = Some(crate::remote::join(&remote.host, &file.to_string_lossy()));
                        }
                    if meta != st.meta {
                        st.meta = meta;
                        pending.push(SessionUpdate::Meta(st.meta.clone()));
                    }
                }
                Aux::Stats(stats) => {
                    let mut meta = st.meta.clone();
                    apply_stats(&mut meta, &stats);
                    if meta != st.meta {
                        st.meta = meta;
                        pending.push(SessionUpdate::Meta(st.meta.clone()));
                    }
                }
                Aux::Commands(list) => {
                    save_commands(&driver.cfg, &list);
                    st.commands = parse_commands(&list);
                    pending.push(SessionUpdate::Commands(st.commands.clone()));
                }
                Aux::Plugin(id, args, ran) => {
                    st.apply(&HarnessEvent::MessageEnd { id, message: plugin_message(&crate::plugins::notice(&args, Some(&ran)), !ran.ok) });
                    dirty = true;
                    // What's installed changed: the agent starts again to
                    // load it (its skills, its "/" list), now when it's
                    // idle, else once its run ends.
                    if ran.ok && crate::plugins::changes(&args) && driver.harness.is_some() {
                        if st.status == SessionStatus::Idle {
                            driver.stop_idle(&mut st, &mut pending);
                            driver.ensure_harness(&mut st, &mut pending);
                        } else {
                            driver.restart_settled = true;
                        }
                    }
                }
                // The snapshot's done: the turn's prompt goes now (unless
                // it was stopped meanwhile).
                Aux::Snapshotted(n) if n == snap_turn && !held.is_empty() => {
                    pending.push(SessionUpdate::Preparing(false));
                    let mut rest = std::mem::take(&mut held).into_iter().enumerate();
                    while let Some((i, u)) = rest.next() {
                        let Some(h) = driver.ensure_harness(&mut st, &mut pending) else {
                            // (It didn't start: this one and the rest back.)
                            let back = std::iter::once(u).chain(rest.map(|(_, u)| u)).collect();
                            give_back(&mut st, &mut pending, back, "Not sent");
                            dirty = true;
                            break;
                        };
                        h.prompt(&u.text, u.images, if i == 0 { SendMode::Normal } else { u.mode });
                    }
                }
                Aux::Snapshotted(_) => {}
            },
            ctl = control.recv() => match ctl {
                Some(Control::Subscribe(id, sink)) => {
                    // Bring the newcomer up to date with what others have seen
                    // (a turn still being prepared too).
                    let mut now = st.snapshot();
                    if !held.is_empty() {
                        now.push(SessionUpdate::Preparing(true));
                    }
                    // (Messages given back while no box was there: this one's.)
                    if sink.composes() && !st.unsent.is_empty() {
                        now.push(SessionUpdate::Unsent(std::mem::take(&mut st.unsent)));
                    }
                    sink.deliver(now);
                    st.sinks.push((id, sink));
                }
                Some(Control::Unsubscribe(id)) => st.sinks.retain(|(s, _)| *s != id),
                // `/plugin …`: Claude Code's plugin command, run by harlo
                // (not sent to the agent), its result a notice.
                Some(Control::Prompt(text, _, _, typed)) if plugin_args(&text, typed.as_ref()).is_some() => {
                    let args = plugin_args(&text, typed.as_ref()).unwrap_or_default();
                    plugin_runs += 1;
                    let id = format!("harlo-plugin-{plugin_runs}");
                    st.apply(&HarnessEvent::MessageEnd { id: id.clone(), message: plugin_message(&crate::plugins::notice(&args, None), false) });
                    dirty = true;
                    let (tx, cwd, program) = (driver.aux_tx.clone(), driver.cfg.cwd.clone(), claude_program(&driver.cfg));
                    let host = driver.cfg.remote.as_ref().map(|r| crate::remote::hosts().into_iter().find(|h| h.name == r.host).ok_or(r.host.clone()));
                    tokio::task::spawn_blocking(move || {
                        let ran = match &host {
                            Some(Err(name)) => crate::plugins::Ran { ok: false, detail: String::new(), output: format!("{name} isn't among the remote instances any more.") },
                            Some(Ok(h)) => crate::plugins::run(&args, &cwd, Some(h), None),
                            None => crate::plugins::run(&args, &cwd, None, program.as_deref()),
                        };
                        let _ = tx.send(Aux::Plugin(id, args, ran));
                    });
                }
                // (One sent while a turn waits for its snapshot: after it.)
                Some(Control::Prompt(text, images, mode, typed)) if !held.is_empty() => held.push(Unsent { text, images, mode, typed }),
                Some(Control::Prompt(text, images, mode, typed)) => {
                    // A new turn (not steering or a queued one): the folder's
                    // files first, so the turn can be undone (git::snapshot_turn).
                    // That can take seconds (a big or remote repository), so
                    // it's taken beside the loop: a Stop meanwhile still gets
                    // here, and the prompt is never sent.
                    // (A side question, `/btw`, isn't a turn.)
                    if st.status != SessionStatus::Busy && turn_snapshots_on() && !text.trim_start().starts_with("/btw") {
                        let dir = match &driver.cfg.remote {
                            Some(r) => crate::remote::join(&r.host, &driver.cfg.cwd.to_string_lossy()),
                            None => driver.cfg.cwd.clone(),
                        };
                        held.push(Unsent { text, images, mode, typed });
                        snap_turn += 1;
                        pending.push(SessionUpdate::Preparing(true));
                        let (tx, n) = (driver.aux_tx.clone(), snap_turn);
                        tokio::spawn(async move {
                            // (Tests: a slow disk or host.)
                            if let Some(ms) = std::env::var("HARLO_TEST_SLOW_SNAPSHOT_MS").ok().and_then(|v| v.parse::<u64>().ok()) {
                                tokio::time::sleep(Duration::from_millis(ms)).await;
                            }
                            let _ = tokio::task::spawn_blocking(move || crate::git::snapshot_turn(&dir)).await;
                            let _ = tx.send(Aux::Snapshotted(n));
                        });
                        // The agent starts meanwhile (it touches nothing
                        // before it's sent something); one that can't start
                        // gives the message back now.
                        if driver.ensure_harness(&mut st, &mut pending).is_none() {
                            snap_turn += 1;
                            pending.push(SessionUpdate::Preparing(false));
                            give_back(&mut st, &mut pending, std::mem::take(&mut held), "Not sent");
                            dirty = true;
                        }
                    } else if let Some(h) = driver.ensure_harness(&mut st, &mut pending) {
                        // A prompt sent while idle must not carry a queue mode.
                        let mode = if st.status == SessionStatus::Busy { mode } else { SendMode::Normal };
                        h.prompt(&text, images, mode);
                    } else {
                        // (It didn't start: the message back.)
                        give_back(&mut st, &mut pending, vec![Unsent { text, images, mode, typed }], "Not sent");
                        dirty = true;
                    }
                }
                // Stopped while its snapshot was taken: never sent.
                Some(Control::Abort) if !held.is_empty() => {
                    let unsent = std::mem::take(&mut held);
                    let why = if unsent.len() == 1 { "Stopped before it was sent" } else { "Stopped before they were sent" };
                    snap_turn += 1;
                    pending.push(SessionUpdate::Preparing(false));
                    give_back(&mut st, &mut pending, unsent, why);
                    dirty = true;
                }
                Some(Control::Abort) => {
                    // A run you stopped isn't "ready".
                    st.aborted = st.status == SessionStatus::Busy;
                    if let (Some(h), true) = (&driver.harness, st.status.has_process()) {
                        h.abort();
                    }
                }
                Some(Control::Answer(id, answer)) => {
                    if id.starts_with(GATE_PREFIX) {
                        // harlo's: what was asked, for the agent's answer.
                        if let Some(gate) = st.requests.iter().find(|r| r.id == id) {
                            pending.push(SessionUpdate::GateAnswered { id: id.clone(), question: gate.message.clone(), answer });
                        }
                    } else if let Some(h) = &driver.harness {
                        // (In the agent's words, where they differ.)
                        let mut answer = answer;
                        if answer["value"] == "Always allow" {
                            if let Some(said) = st.requests.iter().find(|r| r.id == id).and_then(|r| r.raw["alwaysValue"].as_str()) {
                                answer["value"] = json!(said);
                            }
                        }
                        h.answer_ui(&id, answer);
                    }
                    st.requests.retain(|r| r.id != id);
                    pending.push(SessionUpdate::UiCleared(id));
                }
                Some(Control::Gate(request)) => {
                    st.requests.push(request.clone());
                    pending.push(SessionUpdate::UiRequest(request));
                }
                Some(Control::Raw(cmd)) => {
                    let modes = cmd["type"] == "harlo_set_modes";
                    if modes {
                        driver.modes = Some(cmd.clone());
                    }
                    let thinking = kind == HarnessKind::Claude && cmd["type"] == "set_thinking_level";
                    if thinking {
                        driver.thinking = cmd["level"].as_str().map(str::to_owned);
                    }
                    // Not running: for when it starts, nothing starts for this.
                    if !st.status.has_process() && (modes || thinking) {
                        if thinking {
                            st.meta.thinking = driver.thinking.clone().unwrap_or_default();
                            pending.push(SessionUpdate::Meta(st.meta.clone()));
                        }
                    } else if let Some(h) = driver.ensure_harness(&mut st, &mut pending) {
                        let aux = driver.aux_tx.clone();
                        tokio::spawn(async move {
                            let changes_state = matches!(
                                cmd["type"].as_str(),
                                Some("set_model" | "set_thinking_level" | "cycle_model" | "cycle_thinking_level" | "set_session_name")
                            );
                            match h.request(cmd).await {
                                Ok(_) if changes_state => fetch_state(h, aux).await,
                                Ok(_) => {}
                                Err(e) => tracing::warn!("command failed: {e}"),
                            }
                        });
                    }
                }
                // (A saved session still loading: the report kept from
                // last time, else once it has loaded.)
                Some(Control::ContextReport(reply)) if st.status == SessionStatus::Loading => match &early {
                    Some(report) => drop(reply.send(report.clone())),
                    None => early_asked.push(reply),
                },
                Some(Control::ContextReport(reply)) => {
                    let _ = reply.send(st.report());
                }
                // (A saved session still loading: once it has.)
                Some(Control::Export(to, reply)) if st.status == SessionStatus::Loading => exports.push((to, reply)),
                Some(Control::Export(to, reply)) => export(&st, &driver, to, reply),
                Some(Control::Reload) => {
                    if let Some(h) = driver.harness.clone() {
                        tokio::spawn(fetch_initial(h, driver.aux_tx.clone(), true));
                    }
                }
                Some(Control::RestartIdle) => {
                    if st.status == SessionStatus::Idle {
                        driver.stop_idle(&mut st, &mut pending);
                    }
                }
                Some(Control::RestartWhenIdle) => {
                    if st.status == SessionStatus::Idle {
                        driver.stop_idle(&mut st, &mut pending);
                    } else {
                        driver.restart_settled = driver.harness.is_some();
                    }
                }
                // Claude's levels go by the model: no need to start it.
                Some(Control::Request(cmd, reply)) if kind == HarnessKind::Claude && cmd["type"] == "get_available_thinking_levels" && !st.status.has_process() => {
                    let model = if st.meta.model_id.is_empty() { &st.meta.model } else { &st.meta.model_id };
                    let _ = reply.send(Ok(json!({ "levels": harlo_harness::claude::thinking_levels(model) })));
                }
                Some(Control::Request(cmd, reply)) => match driver.ensure_harness(&mut st, &mut pending) {
                    Some(h) => {
                        tokio::spawn(async move {
                            let _ = reply.send(h.request(cmd).await.map_err(|e| e.to_string()));
                        });
                    }
                    None => {
                        let _ = reply.send(Err("the harness could not be started".into()));
                    }
                },
                // Every handle is gone: stop the harness and finish.
                None => {
                    if let Some(h) = &driver.harness {
                        h.kill();
                    }
                    break;
                }
            },
            _ = tokio::time::sleep(wait), if dirty || !pending.is_empty() => {}
        }

        // A run ended: a long session's report kept for its next opening.
        let busy = st.status == SessionStatus::Busy;
        if was_busy && !busy && st.message_count >= MANY
            && let Some(path) = session_file.clone().or_else(|| st.meta.session_file.clone())
        {
            save_rows(path.clone(), &st.transcript.last_rows(SHOWN_FIRST));
            save_report(path, st.report());
        }
        was_busy = busy;

        if (dirty || !pending.is_empty()) && last_flush.elapsed() >= STREAM_FLUSH {
            // (The rows kept from last time stay until it has loaded, with
            // what came since under them.)
            if dirty && early_rows {
                let mut rows = early_list.clone();
                rows.extend(st.transcript.rows());
                let changes = diff_rows(&st.shown, &rows);
                st.shown = rows;
                if !changes.is_empty() {
                    pending.push(SessionUpdate::Rows(changes));
                }
                dirty = false;
            }
            if dirty {
                let mut rows = st.transcript.rows();
                // (Replies far back keep their long text on disk: sent so.)
                if st.transcript.page_out(KEPT_ROWS) {
                    rows = st.transcript.rows();
                }
                let changes = diff_rows(&st.shown, &rows);
                st.shown = rows;
                if !changes.is_empty() {
                    pending.push(SessionUpdate::Rows(changes));
                }
                dirty = false;
            }
            if !pending.is_empty() {
                st.broadcast(std::mem::take(&mut pending));
            }
            last_flush = Instant::now();
            // (A moment later: the window builds its rows from these.)
            if std::mem::take(&mut loaded) && !dirty {
                tokio::spawn(async {
                    tokio::time::sleep(GIVE_BACK_AFTER).await;
                    let _ = tokio::task::spawn_blocking(harlo_harness::store::give_back).await;
                });
            }
        }
    }
    st.message_count >= MANY
}

fn handle_event(ev: &HarnessEvent, st: &mut DriverState, pending: &mut Vec<SessionUpdate>, driver: &mut Driver) {
    let harness = driver.harness.clone().expect("events imply a harness");
    let aux = &driver.aux_tx;
    // Its cost so far, at each tool call (spending limits; the turn's end
    // fetches it too).
    // And after a compaction, the context left.
    if matches!(ev, HarnessEvent::ToolStart { .. } | HarnessEvent::Compaction { started: false, .. }) && st.status == SessionStatus::Busy {
        tokio::spawn(fetch_stats(harness.clone(), aux.clone()));
    }
    // A new session's file, as soon as its first run is under way (else the
    // sidebar lists the running session and its file apart until it ends).
    if st.status == SessionStatus::Busy && st.meta.session_file.is_none() && !st.file_asked && !matches!(ev, HarnessEvent::RunStarted) {
        st.file_asked = true;
        tokio::spawn(fetch_state(harness.clone(), aux.clone()));
    }
    match ev {
        HarnessEvent::Ready => {
            set_status(st, SessionStatus::Idle, pending);
            let fetch_history = std::mem::take(&mut driver.fetch_history);
            tokio::spawn(fetch_initial(harness.clone(), aux.clone(), fetch_history));
            let (h, tx) = (harness.clone(), aux.clone());
            tokio::spawn(async move {
                if let Ok(data) = h.request(json!({ "type": "get_commands" })).await {
                    let _ = tx.send(Aux::Commands(data["commands"].clone()));
                }
            });
        }
        // (The demo's todo, Claude Code's tasks and its older TodoWrite.)
        HarnessEvent::ToolStart { call_id, name, .. } if matches!(name.as_str(), "todo" | "TodoWrite" | "TaskCreate" | "TaskUpdate") => {
            st.todo_calls.insert(call_id.clone());
        }
        // The plan changed: re-read the todo phases.
        HarnessEvent::ToolEnd { call_id, .. } if st.todo_calls.remove(call_id) => {
            tokio::spawn(fetch_state(harness.clone(), aux.clone()));
        }
        HarnessEvent::BackgroundTasks(n) => st.background = *n,
        HarnessEvent::RunStarted => {
            st.run_error = None;
            st.aborted = false;
            st.file_asked = false;
            set_status(st, SessionStatus::Busy, pending);
        }
        HarnessEvent::Settled => {
            let aborted = std::mem::take(&mut st.aborted);
            if st.status == SessionStatus::Busy {
                let notable = match st.run_error.take() {
                    Some(message) => Some(Notable::Error { message }),
                    None if aborted => None,
                    None => Some(Notable::Completed { last_message: st.last_reply.clone(), background: st.background }),
                };
                pending.extend(notable.map(SessionUpdate::Notable));
                if aborted {
                    pending.push(SessionUpdate::Stopped);
                }
            }
            set_status(st, SessionStatus::Idle, pending);
            // The session file and title may have appeared during the run.
            tokio::spawn(fetch_state(harness.clone(), aux.clone()));
        }
        HarnessEvent::MessageEnd { id, message } => {
            // A reply the model couldn't finish (provider error, …).
            if message["role"] == "assistant" && message["stopReason"] == "error" {
                st.run_error = Some(message["errorMessage"].as_str().unwrap_or("The model returned an error.").to_owned());
            }
            if let Some(text) = reply_text(message) {
                st.last_reply = text;
            }
            st.message_count += 1;
            st.ended(id, message);
        }
        HarnessEvent::Retry { started: false, error: Some(error), .. } => st.run_error = Some(error.clone()),
        HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Error, text, system: false } => {
            pending.push(SessionUpdate::Notable(Notable::Error { message: text.clone() }));
        }
        HarnessEvent::CommandsChanged(list) => {
            let _ = aux.send(Aux::Commands(list.clone()));
        }
        HarnessEvent::Limits(info) => pending.push(SessionUpdate::Limits(info.clone())),
        HarnessEvent::Aside(a) => pending.push(SessionUpdate::Aside(a.clone())),
        HarnessEvent::Exited { code, stderr_tail } => {
            // Mid-run, or with a failure code: worth knowing. (What it said,
            // not the end of a stack trace. Its host disconnected by hand:
            // nothing went wrong, and that was asked first if it was working.)
            let disconnected = driver.cfg.remote.as_ref().is_some_and(|r| crate::remote::is_disconnected(&r.host));
            if !disconnected && (st.status == SessionStatus::Busy || code.is_some_and(|c| c != 0)) {
                let last = stderr_tail.lines().rev().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("at ")).unwrap_or("");
                let message = format!("{} stopped{}{}", driver.cfg.kind.as_str(), code.map_or(String::new(), |c| format!(" (exit {c})")), if last.is_empty() { String::new() } else { format!(": {last}") });
                pending.push(SessionUpdate::Notable(Notable::Error { message }));
            }
            set_status(st, SessionStatus::Exited, pending);
            // (harlo's own requests outlive the agent's process.)
            let (gates, gone): (Vec<UiRequest>, Vec<UiRequest>) = st.requests.drain(..).partition(|r| r.id.starts_with(GATE_PREFIX));
            st.requests = gates;
            for r in gone {
                pending.push(SessionUpdate::UiCleared(r.id));
            }
        }
        HarnessEvent::UiRequest(raw) => {
            let req = ui_request(raw);
            st.requests.push(req.clone());
            pending.push(SessionUpdate::UiRequest(req));
        }
        HarnessEvent::UiCancel { target_id } => {
            st.requests.retain(|r| &r.id != target_id);
            pending.push(SessionUpdate::UiCleared(target_id.clone()));
        }
        HarnessEvent::SubagentActivity(frame) => {
            let p = &frame["payload"];
            let status = p["status"].as_str().or_else(|| p["progress"]["status"].as_str()).unwrap_or_default();
            if matches!(status, "completed" | "failed" | "aborted" | "cancelled")
                && let Some(id) = p["id"].as_str()
                && st.subagents_done.insert(id.to_owned())
            {
                pending.push(SessionUpdate::Notable(Notable::SubagentDone {
                    id: id.to_owned(),
                    agent: p["agent"].as_str().unwrap_or("subagent").to_owned(),
                    status: status.to_owned(),
                    task: p["description"].as_str().or_else(|| p["task"].as_str()).unwrap_or_default().to_owned(),
                }));
            }
            pending.push(SessionUpdate::SubagentActivity(frame.clone()))
        }
        HarnessEvent::HarloCall { id, tool, args } => pending.push(SessionUpdate::HarloCall { id: id.clone(), tool: tool.clone(), args: args.clone() }),
        HarnessEvent::QueueChanged { steering, follow_up } => {
            st.queue = (steering.clone(), follow_up.clone());
            pending.push(SessionUpdate::Queue { steering: steering.clone(), follow_up: follow_up.clone() });
        }
        HarnessEvent::Renamed(name) => {
            st.meta.name = name.clone().unwrap_or_default();
            pending.push(SessionUpdate::Meta(st.meta.clone()));
        }
        HarnessEvent::ModelChanged(_) | HarnessEvent::ThinkingLevel(_) => {
            tokio::spawn(fetch_state(harness.clone(), aux.clone()));
        }
        HarnessEvent::ContextChanged => {
            tokio::spawn(fetch_stats(harness.clone(), aux.clone()));
        }
        HarnessEvent::MessageStart { message, .. } if st.meta.first_message.is_empty() && message["role"] == "user" => {
            st.meta.first_message = first_user_text(std::slice::from_ref(message));
            pending.push(SessionUpdate::Meta(st.meta.clone()));
        }
        HarnessEvent::Status { key, text } => {
            let text = text.as_deref().map(strip_ansi).filter(|t| !t.trim().is_empty());
            st.status_entries.retain(|(k, _)| k != key);
            if let Some(t) = &text {
                st.status_entries.push((key.clone(), t.clone()));
            }
            pending.push(SessionUpdate::StatusEntry { key: key.clone(), text });
        }
        _ => {}
    }
}

enum Aux {
    /// The session file's branch, model and thinking level; with a remote
    /// one's, how much of it didn't come over (its end); and its tally (see
    /// `DriverState::tally`).
    Loaded(Box<Opened>),
    /// The report kept from the last time the file was open, read before
    /// it loads (None: none kept).
    Early(Option<Box<Value>>),
    /// The rows kept from the last time the file was open, read before it
    /// loads.
    EarlyRows(Vec<Arc<Row>>),
    /// Messages from the harness (`get_messages`), and their tally.
    History(Vec<Value>, crate::context::Tally),
    State(Value),
    Stats(Value),
    Commands(Value),
    /// The snapshot taken before turn `n` (see `Control::Prompt`) is done.
    Snapshotted(u64),
    /// A `/plugin …` run (its notice's id, its words) ended.
    Plugin(String, Vec<String>, crate::plugins::Ran),
}

async fn fetch_initial(harness: Arc<Harness>, tx: mpsc::UnboundedSender<Aux>, resume: bool) {
    if resume {
        match harness.request(json!({ "type": "get_messages" })).await {
            Ok(mut data) => {
                let messages: Vec<Value> = serde_json::from_value(data["messages"].take()).unwrap_or_default();
                // (Off the session's loop: a long one takes a moment.)
                let (messages, tally) = tokio::task::spawn_blocking(move || {
                    let tally = crate::context::Tally::of(&messages);
                    (messages, tally)
                })
                .await
                .unwrap_or_default();
                let _ = tx.send(Aux::History(messages, tally));
            }
            Err(e) => tracing::warn!("get_messages failed: {e}"),
        }
    }
    fetch_state(harness, tx).await;
}

/// The session as Markdown, as it is now, to `reply`.
fn export(st: &DriverState, driver: &Driver, to: Option<PathBuf>, reply: oneshot::Sender<Result<String, String>>) {
    let info = crate::export::Info {
        title: if st.meta.name.is_empty() { st.meta.first_message.clone() } else { st.meta.name.clone() },
        agent: driver.cfg.kind.label().to_owned(),
        model: st.meta.model.clone(),
        folder: crate::sidebar::tilde(&driver.cfg.cwd),
    };
    let made = export_markdown(driver.harness.clone(), driver.cfg.kind, st.meta.session_file.clone(), info, to);
    tokio::spawn(async move {
        let _ = reply.send(made.await);
    });
}

/// Refreshes model, thinking level, session file/name and context stats.
async fn fetch_state(harness: Arc<Harness>, tx: mpsc::UnboundedSender<Aux>) {
    match harness.request(json!({ "type": "get_state" })).await {
        Ok(state) => {
            let _ = tx.send(Aux::State(state));
        }
        Err(e) => tracing::debug!("get_state failed: {e}"),
    }
    fetch_stats(harness, tx).await;
}

/// Refreshes the context and cost.
async fn fetch_stats(harness: Arc<Harness>, tx: mpsc::UnboundedSender<Aux>) {
    if let Ok(stats) = harness.request(json!({ "type": "get_session_stats" })).await {
        let _ = tx.send(Aux::Stats(stats));
    }
}

fn apply_stats(meta: &mut SessionMetaInfo, stats: &Value) {
    let usage = &stats["contextUsage"];
    meta.context_tokens = usage["tokens"].as_u64().unwrap_or(0);
    meta.context_window = usage["contextWindow"].as_u64().unwrap_or(0);
    meta.compact_at = usage["compactAt"].as_u64().unwrap_or(0);
    meta.cost = stats["cost"].as_f64().unwrap_or(0.0);
    meta.cost_kind = stats["costKind"].as_str().unwrap_or_default().to_owned();
}

fn parse_commands(list: &Value) -> Vec<SlashCommand> {
    let mut out: Vec<SlashCommand> = list
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    Some(SlashCommand {
                        name: c["name"].as_str()?.trim_start_matches('/').to_owned(),
                        description: c["description"].as_str().unwrap_or_default().to_owned(),
                        hint: c["input"]["hint"].as_str().unwrap_or_default().to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // (harlo's own: `/plugin`, for either agent.)
    if !out.iter().any(|c| c.name == "plugin") {
        out.push(crate::plugins::command());
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// `/plugin …`'s words, when it's harlo's to run: sent by you (an agent's
/// or a schedule's goes to the agent as written).
fn plugin_args(text: &str, typed: Option<&Typed>) -> Option<Vec<String>> {
    typed.filter(|t| t.by_you)?;
    crate::plugins::parse(text)
}

/// A `/plugin` run's notice, as a message (`failed`: drawn as an error).
fn plugin_message(text: &str, failed: bool) -> Value {
    json!({ "role": "custom", "customType": "harlo_plugin", "display": true, "content": text, "failed": failed })
}

/// The Claude Code to run `claude plugin` with here: Settings' (else found
/// on PATH), or this Claude session's own.
fn claude_program(cfg: &HarnessConfig) -> Option<PathBuf> {
    let set = crate::db::Db::open(&crate::db::default_path()).ok().and_then(|d| d.get("settings.claude_path").ok().flatten()).filter(|p| !p.is_empty()).map(PathBuf::from);
    set.or_else(|| (cfg.kind == HarnessKind::Claude).then(|| cfg.program.clone()).flatten())
}

fn apply_state(meta: &mut SessionMetaInfo, state: &Value) {
    let model = &state["model"];
    // (Not known yet, as an agent starts: what's shown stays.)
    if !model.is_null() {
        meta.model = model["name"]
            .as_str()
            .or_else(|| model["id"].as_str())
            .or_else(|| model.as_str())
            .unwrap_or("")
            .to_owned();
        meta.model_id = match (model["provider"].as_str(), model["id"].as_str()) {
            (Some(p), Some(id)) => format!("{p}/{id}"),
            _ => String::new(),
        };
    }
    meta.thinking = state["thinkingLevel"].as_str().unwrap_or("").to_owned();
    if let Some(f) = state["sessionFile"].as_str() {
        meta.session_file = Some(PathBuf::from(f));
    }
    if let Some(id) = state["sessionId"].as_str() {
        meta.session_id = id.to_owned();
    }
    if let Some(name) = state["sessionName"].as_str() {
        meta.name = name.to_owned();
    }
}

fn first_user_text(messages: &[Value]) -> String {
    messages.iter().find_map(user_text).unwrap_or_default()
}

/// A message of yours as a session's first words (None: not yours, or no
/// words in it).
fn user_text(m: &Value) -> Option<String> {
    if m["role"] != "user" {
        return None;
    }
    let content = &m["content"];
    let text = content.as_str().map(str::to_owned).or_else(|| {
        content
            .as_array()
            .and_then(|a| a.iter().find_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten()))
            .map(str::to_owned)
    })?;
    Some(harlo_harness::file_blocks::title_text(&text).chars().take(100).collect())
}

/// Removes ANSI escape sequences (extension status lines often carry colours).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters until a final byte in @..~
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: until BEL or ST (ESC \)
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn ui_request(raw: &Value) -> UiRequest {
    let s = |k: &str| raw[k].as_str().unwrap_or_default().to_owned();
    let options: Vec<String> = raw["options"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|o| o.as_str().map(str::to_owned).or_else(|| o["label"].as_str().map(str::to_owned)))
                .collect()
        })
        .unwrap_or_default();
    let title = s("title");
    let mut options = options;
    let mut raw = raw.clone();
    // omp's session-long answer ("Approve write for this session") is
    // harlo's "Always allow"; the answer goes back in its words.
    if title.starts_with("Allow tool:") {
        if let Some(i) = options.iter().position(|o| o.starts_with("Approve ") && o.ends_with(" for this session")) {
            let said = options.remove(i);
            raw["alwaysHint"] = json!(said.trim_start_matches("Approve "));
            raw["alwaysValue"] = json!(said);
            options.push("Always allow".to_owned());
            raw["options"] = json!(options);
        }
    }
    // An approval: Approve / Deny, and "Always allow" where the agent can
    // remember it (`alwaysHint` says what it would remember).
    let is_approval = title.starts_with("Allow tool:") && options.len() >= 2 && options[..2] == ["Approve", "Deny"] && options[2..].iter().all(|o| o == "Always allow");
    UiRequest {
        id: s("id"),
        method: s("method"),
        message: s("message"),
        placeholder: s("placeholder"),
        prefill: s("prefill"),
        title,
        options,
        is_approval,
        raw,
    }
}

/// An agent's error that says it has no login or key for the model.
fn refused_by_limit(error: &str) -> bool {
    error.contains("You've hit your")
}

fn asks_for_login(error: &str) -> bool {
    ["/login", "No API key", "Failed to authenticate", "Not logged in", "Not signed in", "sign-in has ended", "refused the sign-in", "Invalid API key"].iter().any(|said| error.contains(said))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_refusal_for_a_used_up_limit_is_told_apart() {
        assert!(super::refused_by_limit("You've hit your 5-hour limit · resets 4:10pm (America/Toronto)"));
        assert!(super::refused_by_limit("You've hit your weekly limit · resets Oct 18, 1pm (America/Toronto)"));
        assert!(!super::refused_by_limit("Rate limit reached"));
    }

    #[test]
    fn a_refusal_for_want_of_a_login_is_told_apart() {
        assert!(super::asks_for_login("No API key found for openai.\n\nUse /login to log into a provider via OAuth or API key."));
        assert!(super::asks_for_login("No default model selected. Use /login, set an API key environment variable"));
        assert!(super::asks_for_login("Failed to authenticate: OAuth session expired and could not be refreshed"));
        assert!(super::asks_for_login("Not signed in: run `harlo-agent login`"));
        assert!(!super::asks_for_login("Rate limit reached"));
    }

    use super::*;

    /// A session that hasn't started its agent offers the commands that
    /// agent listed last time in the same folder; anywhere else, harlo's
    /// own (/plugin).
    #[tokio::test]
    async fn commands_are_kept_for_the_next_session() {
        let _turn = REPORTS_TURN.lock().await;
        let dir = std::env::temp_dir().join(format!("harlo-commands-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        *REPORTS_IN.lock().unwrap() = Some(dir.clone());
        let cfg = |cwd: &str| HarnessConfig { kind: HarnessKind::Claude, program: None, cwd: cwd.into(), session: SessionTarget::New, extra_args: vec![], env: vec![], remote: None };
        let names = |c: Vec<SlashCommand>| c.into_iter().map(|c| c.name).collect::<Vec<_>>();
        assert_eq!(names(saved_commands(&cfg("/p/a"))), ["plugin"]);
        save_commands(&cfg("/p/a"), &json!([{ "name": "compact", "description": "Compact" }, { "name": "review" }]));
        let file = commands_file(&dir, &cfg("/p/a"));
        for _ in 0..100 {
            if file.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(names(saved_commands(&cfg("/p/a"))), ["compact", "plugin", "review"]);
        assert_eq!(names(saved_commands(&cfg("/p/b"))), ["plugin"]);
        assert_eq!(names(saved_commands(&HarnessConfig { kind: HarnessKind::Harlo, ..cfg("/p/a") })), ["plugin"]);
        *REPORTS_IN.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `/plugin …` is harlo's to run only when you sent it.
    #[test]
    fn plugin_commands_only_from_you() {
        let you = Typed { words: "/plugin list".into(), files: Vec::new(), by_you: true };
        let agent = Typed { by_you: false, ..you.clone() };
        assert_eq!(plugin_args("/plugin install x@y", Some(&you)), Some(vec!["install".to_owned(), "x@y".to_owned()]));
        assert_eq!(plugin_args("/plugin install x@y", Some(&agent)), None);
        assert_eq!(plugin_args("/plugin install x@y", None), None);
        assert_eq!(plugin_args("install x@y", Some(&you)), None);
    }

    #[test]
    fn a_replys_text_is_plain_and_trimmed() {
        let reply = json!({ "role": "assistant", "content": [{ "type": "text", "text": " one " }, { "type": "toolCall", "name": "bash" }, { "type": "text", "text": "two" }] });
        assert_eq!(reply_text(&reply).as_deref(), Some("one \ntwo"));
        assert_eq!(reply_text(&json!({ "role": "assistant", "content": "done" })).as_deref(), Some("done"));
        assert_eq!(reply_text(&json!({ "role": "assistant", "content": [{ "type": "toolCall", "name": "bash" }] })), None, "a reply without text is passed over");
        assert_eq!(reply_text(&json!({ "role": "user", "content": "hi" })), None);
    }

    use harlo_harness::HarnessKind;

    struct ChannelSink(mpsc::UnboundedSender<SessionUpdate>);
    impl SessionSink for ChannelSink {
        fn deliver(&self, updates: Vec<SessionUpdate>) {
            for u in updates {
                let _ = self.0.send(u);
            }
        }
    }

    fn cfg(kind: HarnessKind, session: SessionTarget) -> HarnessConfig {
        HarnessConfig { kind, program: None, cwd: std::env::temp_dir(), session, extra_args: vec![], env: vec![], remote: None }
    }

    /// Collects updates until `done` returns true (or times out).
    async fn collect_until(
        rx: &mut mpsc::UnboundedReceiver<SessionUpdate>,
        rows: &mut Vec<Arc<Row>>,
        mut done: impl FnMut(&SessionUpdate) -> bool,
    ) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match &u {
                SessionUpdate::Reset(r) => *rows = r.clone(),
                SessionUpdate::Rows(changes) => apply(rows, changes.clone()),
                _ => {}
            }
            if done(&u) {
                return true;
            }
        }
        false
    }

    fn apply(rows: &mut Vec<Arc<Row>>, changes: Vec<RowChange>) {
        crate::listdiff::apply(rows, changes);
    }

    /// A real harlo-agent signed in with the Codex sign-in
    /// (`HARLO_TEST_HARLO_AGENT`, scratch `HARLO_TEST_HOME`): a reply on a
    /// `codex/…` model, and the plan's limits with it as harlo takes them.
    #[tokio::test]
    #[ignore]
    async fn harlo_agent_codex_limits() {
        let home = PathBuf::from(std::env::var("HARLO_TEST_HOME").expect("a scratch HARLO_TEST_HOME"));
        assert!(home.starts_with("/tmp/"), "not a scratch folder: {home:?}");
        let work = home.join("work");
        std::fs::create_dir_all(&work).unwrap();
        harlo_harness::protocol_log::set_dir(Some(home.join("protocol")));
        let agent = std::env::var("HARLO_TEST_HARLO_AGENT").expect("HARLO_TEST_HARLO_AGENT");
        let out = std::process::Command::new(&agent).args(["models", "--json"]).output().unwrap();
        let models: Value = serde_json::from_slice(&out.stdout).unwrap();
        let model = models.as_array().unwrap().iter().find(|m| m["provider"] == "codex").map(|m| format!("codex/{}", m["id"].as_str().unwrap())).expect("a codex/… model: sign in with `harlo-agent login --codex`");
        let handle = SessionHandle::spawn(HarnessConfig {
            kind: HarnessKind::Harlo,
            program: Some(agent.into()),
            cwd: work.clone(),
            session: SessionTarget::New,
            extra_args: vec!["--model".into(), model.clone(), "--thinking".into(), "low".into()],
            env: vec![("XDG_DATA_HOME".to_owned(), home.join("data").to_string_lossy().into_owned())],
            remote: None,
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        handle.prompt("Reply with just the word ok.", Vec::new(), SendMode::Normal);
        let (mut limits, mut meta, mut busy) = (Vec::new(), SessionMetaInfo::default(), false);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::Meta(m) => meta = m,
                SessionUpdate::Limits(l) => limits.push(l),
                // (The turn's end, not an Idle from before it began.)
                SessionUpdate::Status(SessionStatus::Idle) if busy => break,
                SessionUpdate::Status(SessionStatus::Idle) => {}
                SessionUpdate::Status(_) => busy = true,
                _ => {}
            }
        }
        // (What comes just after.)
        while let Ok(Some(u)) = tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::Meta(m) => meta = m,
                SessionUpdate::Limits(l) => limits.push(l),
                _ => {}
            }
        }
        assert!(rows.iter().any(|r| r.kind == crate::RowKind::Markdown), "{rows:?}");
        assert!(meta.model_id.starts_with("codex/"), "{meta:?}");
        assert_eq!(crate::quota::session_provider("harlo", &meta.model_id), "openai-codex");
        let l = limits.first().expect("the plan's limits with the reply");
        assert_eq!(l["provider"], "openai-codex");
        let w = l["windows"].as_array().expect("windows");
        assert!(!w.is_empty() && w.iter().all(|w| w["usedPercent"].is_number() && w["windowMinutes"].as_i64().is_some_and(|m| m > 0)), "{w:?}");
        let mut accounts = Vec::new();
        crate::quota::apply_live(&mut accounts, l);
        println!("{model}: {}", accounts[0]["windows"].as_array().unwrap().iter().map(|w| format!("{} {:.0}%", w["label"], w["usedFraction"].as_f64().unwrap() * 100.0)).collect::<Vec<_>>().join(", "));
        // And the meter's own ask, between turns.
        let usage = crate::quota::fetch_harlo(std::path::Path::new(&std::env::var("HARLO_TEST_HARLO_AGENT").unwrap())).unwrap().expect("signed in");
        assert!(!usage["windows"].as_array().unwrap().is_empty(), "{:?}", usage["windows"]);
    }

    /// A real harlo-agent (`HARLO_TEST_HARLO_AGENT`, signed in), its
    /// sessions in a scratch folder (`HARLO_TEST_HOME`): it asks before it
    /// writes, takes the answer, says its model, level and file, a run can be
    /// stopped, and the session opens again with what it said.
    #[tokio::test]
    #[ignore]
    async fn harlo_agent_session() {
        let home = PathBuf::from(std::env::var("HARLO_TEST_HOME").expect("a scratch HARLO_TEST_HOME"));
        assert!(home.starts_with("/tmp/"), "not a scratch folder: {home:?}");
        let work = home.join("work");
        std::fs::create_dir_all(&work).unwrap();
        harlo_harness::protocol_log::set_dir(Some(home.join("protocol")));
        let cfg = |session| HarnessConfig {
            kind: HarnessKind::Harlo,
            program: Some(std::env::var("HARLO_TEST_HARLO_AGENT").expect("HARLO_TEST_HARLO_AGENT").into()),
            cwd: work.clone(),
            session,
            extra_args: vec![],
            env: vec![("XDG_DATA_HOME".to_owned(), home.join("data").to_string_lossy().into_owned())],
            remote: None,
        };
        let handle = SessionHandle::spawn(cfg(SessionTarget::New));
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        handle.send_raw(json!({ "type": "harlo_set_modes", "approval": "always-ask", "plan": false }));
        handle.send_raw(json!({ "type": "set_thinking_level", "level": "low" }));
        handle.prompt("Use the write tool to create hello.txt containing exactly: hi", Vec::new(), SendMode::Normal);

        // Asked first, as an approval harlo shows with "Always allow".
        let mut asked = None;
        let mut limits = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::UiRequest(r) => {
                    asked = Some(r);
                    break;
                }
                SessionUpdate::Limits(l) => limits.push(l),
                _ => {}
            }
        }
        let asked = asked.expect("an approval");
        assert!(asked.is_approval && asked.title.starts_with("Allow tool: write"), "{asked:?}");
        assert_eq!(asked.options, ["Approve", "Deny", "Always allow"]);
        assert!(!work.join("hello.txt").exists(), "nothing written before the answer");
        handle.answer_ui(&asked.id, json!({ "value": "Approve" }));
        let mut meta = SessionMetaInfo::default();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::Meta(m) => meta = m,
                SessionUpdate::Status(SessionStatus::Idle) => break,
                SessionUpdate::Limits(l) => limits.push(l),
                _ => {}
            }
        }
        while let Ok(Some(u)) = tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::Meta(m) => meta = m,
                _ => {}
            }
        }
        assert_eq!(std::fs::read_to_string(work.join("hello.txt")).unwrap().trim(), "hi");
        let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds.first(), Some(&crate::RowKind::User));
        assert!(kinds.contains(&crate::RowKind::Tool), "{rows:?}");
        let file = meta.session_file.clone().expect("its file");
        assert!(file.starts_with(home.join("data/harlo-agent/sessions")), "{file:?}");
        assert!(!meta.model.is_empty() && meta.model_id.starts_with("chatgpt/"), "{meta:?}");
        assert_eq!(meta.thinking, "low");
        assert!(meta.context_window > 0, "{meta:?}");
        // The official sign-in says no usage (the Codex sign-in does: see
        // harlo_agent_codex_limits). If this fails, OpenAI now sends some.
        assert!(limits.is_empty(), "{limits:?}");

        // Stopped mid-command: it says so, and the command is gone.
        handle.send_raw(json!({ "type": "harlo_set_modes", "approval": "yolo", "plan": false }));
        handle.prompt("Run this exact bash command and nothing else: sleep 60; touch slept.txt", Vec::new(), SendMode::Normal);
        let mut running = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            if let SessionUpdate::Rows(changes) = u {
                apply(&mut rows, changes);
                if rows.iter().any(|r| r.kind == crate::RowKind::Tool && r.state == "running") {
                    running = true;
                    break;
                }
            }
        }
        assert!(running, "the command started");
        handle.abort();
        let mut stopped = false;
        assert!(collect_until(&mut rx, &mut rows, |u| { stopped |= matches!(u, SessionUpdate::Stopped); matches!(u, SessionUpdate::Status(SessionStatus::Idle)) }).await);
        assert!(stopped);
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(!work.join("slept.txt").exists());
        drop(handle);

        // Opened again: what it said, with no process.
        let again = SessionHandle::open_saved(cfg(SessionTarget::Resume(file.clone())));
        let (tx, mut rx) = mpsc::unbounded_channel();
        again.subscribe(ChannelSink(tx));
        let mut back = Vec::new();
        assert!(collect_until(&mut rx, &mut back, |u| matches!(u, SessionUpdate::Status(SessionStatus::Offline))).await);
        collect_until(&mut rx, &mut back, |u| matches!(u, SessionUpdate::Rows(_))).await;
        assert!(back.iter().any(|r| r.kind == crate::RowKind::User && r.text.contains("hello.txt")), "{back:?}");
        assert!(back.iter().any(|r| r.kind == crate::RowKind::Tool), "{back:?}");
        // And it carries on: the agent continues the same file.
        again.prompt("Reply with just the word: done", Vec::new(), SendMode::Normal);
        assert!(collect_until(&mut rx, &mut back, |u| matches!(u, SessionUpdate::Status(SessionStatus::Busy))).await);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut back, changes),
                SessionUpdate::Status(SessionStatus::Idle) => break,
                _ => {}
            }
        }
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("just the word: done"), "the resumed run went to the same file");
        let sessions = std::fs::read_dir(file.parent().unwrap()).unwrap().count();
        assert_eq!(sessions, 1, "no second session file");

        // Edit & resend from the second message (as the app finds it): a
        // new session with the first exchange, the old one left as it was.
        let users = harlo_harness::branches::current_user_entries(&file).unwrap();
        assert_eq!(users.len(), 3, "{users:?}");
        let before = std::fs::read_to_string(&file).unwrap();
        let fork = again.request(json!({ "type": "fork", "entryId": users[1] })).await.unwrap();
        assert_eq!(fork["cancelled"], false);
        assert!(fork["text"].as_str().unwrap().contains("sleep 60"), "{fork}");
        again.reload();
        let state = again.request(json!({ "type": "get_state" })).await.unwrap();
        let forked = PathBuf::from(state["sessionFile"].as_str().unwrap());
        assert_ne!(forked, file);
        let messages = again.request(json!({ "type": "get_messages" })).await.unwrap();
        let roles: Vec<&str> = messages["messages"].as_array().unwrap().iter().filter_map(|m| m["role"].as_str()).collect();
        assert_eq!(roles.first(), Some(&"user"));
        assert!(!messages.to_string().contains("sleep 60"), "the fork stops before that message");
        again.prompt("Reply with just the word: forked", Vec::new(), SendMode::Normal);
        assert!(collect_until(&mut rx, &mut back, |u| matches!(u, SessionUpdate::Status(SessionStatus::Busy))).await);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut back, changes),
                SessionUpdate::Status(SessionStatus::Idle) => break,
                _ => {}
            }
        }
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before, "the old session is as it was");
        let text = std::fs::read_to_string(&forked).unwrap();
        assert!(text.contains("just the word: forked") && !text.contains("sleep 60"));
        let header: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(header["parentSession"].as_str(), file.to_str());
        // The sidebar lists it as forked from the first.
        let info = harlo_harness::store::read_session(HarnessKind::Harlo, &forked).unwrap();
        assert_eq!(info.parent_session.as_deref(), file.to_str());
        // Renamed (as the sidebar does): the sidebar reads the name back.
        again.request(json!({ "type": "set_session_name", "name": "Forked try" })).await.unwrap();
        assert_eq!(harlo_harness::store::read_session(HarnessKind::Harlo, &forked).unwrap().name, "Forked try");
        drop(again);

        // Compacted (early, to try it): harlo shows it, the model still
        // knows what it was told, and the saved session shows the summary.
        let mut early = cfg(SessionTarget::New);
        early.env.push(("HARLO_AGENT_COMPACT_AT".to_owned(), "800".to_owned()));
        let small = SessionHandle::spawn(early);
        let (tx, mut rx) = mpsc::unbounded_channel();
        small.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        let mut meta = SessionMetaInfo::default();
        for prompt in ["Remember this code word for later: tangerine. Then write a 150-word paragraph about how oranges are grown.", "Reply with just: two", "What was the code word? Reply with just the word."] {
            small.prompt(prompt, Vec::new(), SendMode::Normal);
            assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Busy))).await);
            let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
            while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
                match u {
                    SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                    SessionUpdate::Meta(m) => meta = m,
                    SessionUpdate::Status(SessionStatus::Idle) => break,
                    _ => {}
                }
            }
        }
        // (Rows still on their way after it settled.)
        while let Ok(Some(u)) = tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::Meta(m) => meta = m,
                _ => {}
            }
        }
        let notices: Vec<&str> = rows.iter().filter(|r| r.kind == crate::RowKind::Notice).map(|r| r.text.as_str()).collect();
        assert!(notices.contains(&"Context compacted"), "{notices:?}");
        let replies: Vec<&str> = rows.iter().filter(|r| r.kind == crate::RowKind::Markdown).map(|r| r.html.as_str()).collect();
        assert!(replies.last().is_some_and(|r| r.to_lowercase().contains("tangerine")), "{:?}", replies.iter().map(|r| r.chars().take(40).collect::<String>()).collect::<Vec<_>>());
        assert_eq!(meta.compact_at, 800, "{meta:?}");
        let saved = meta.session_file.clone().expect("its file");
        assert!(std::fs::read_to_string(&saved).unwrap().contains("\"type\":\"compaction\""));
        let roles: Vec<String> = harlo_harness::store::read_messages(&saved).unwrap().iter().filter_map(|m| m["role"].as_str().map(str::to_owned)).collect();
        assert!(roles.iter().any(|r| r == "compactionSummary"), "{roles:?}");
        drop(small);
        // Everything harlo asked, it answered.
        harlo_harness::protocol_log::set_dir(None);
        let mut refused = Vec::new();
        for log in std::fs::read_dir(home.join("protocol")).unwrap().flatten() {
            for line in std::fs::read_to_string(log.path()).unwrap().lines() {
                let v: Value = serde_json::from_str(line).unwrap_or_default();
                if v["dir"] == "in" && v["frame"]["type"] == "response" && v["frame"]["success"] == false {
                    refused.push(format!("{}: {}", v["frame"]["command"], v["frame"]["error"]));
                }
            }
        }
        assert!(refused.is_empty(), "{refused:?}");
    }

    /// harlo-agent with harlo's tools on its pipe (`--harlo-tools`), on
    /// the plan: its calls come as `HarloCall`, the answer goes back with
    /// `send_raw` (whose command gets an id of its own), and the model uses
    /// the result. Scratch folder as above.
    #[tokio::test]
    #[ignore]
    async fn harlo_agent_calls_harlos_tools() {
        let home = PathBuf::from(std::env::var("HARLO_TEST_HOME").expect("a scratch HARLO_TEST_HOME"));
        assert!(home.starts_with("/tmp/"), "not a scratch folder: {home:?}");
        let work = home.join("work-tools");
        std::fs::create_dir_all(&work).unwrap();
        let handle = SessionHandle::spawn(HarnessConfig {
            kind: HarnessKind::Harlo,
            program: Some(std::env::var("HARLO_TEST_HARLO_AGENT").expect("HARLO_TEST_HARLO_AGENT").into()),
            cwd: work.clone(),
            session: SessionTarget::New,
            extra_args: vec!["--harlo-tools".into()],
            env: vec![("XDG_DATA_HOME".to_owned(), home.join("data").to_string_lossy().into_owned())],
            remote: None,
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        handle.send_raw(json!({ "type": "set_thinking_level", "level": "low" }));
        handle.prompt("Call the list_sessions tool, then tell me the title of the session it lists, exactly.", Vec::new(), SendMode::Normal);
        let mut asked: Vec<(String, Value)> = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::HarloCall { id, tool, args } => {
                    let result = match tool.as_str() {
                        "_tools" => json!([{ "name": "list_sessions", "description": "harlo's open sessions: each one's id and title.", "inputSchema": { "type": "object", "properties": {} } }]),
                        "_instructions" => json!("harlo's sessions: list them with list_sessions."),
                        _ => json!({ "sessions": [{ "id": "s-1", "title": "marmalade-otter", "agent": "pi", "status": "idle" }] }),
                    };
                    asked.push((tool, args));
                    handle.send_raw(json!({ "type": "harlo_result", "call": id, "ok": true, "result": result }));
                }
                SessionUpdate::Status(SessionStatus::Idle) => break,
                _ => {}
            }
        }
        while let Ok(Some(u)) = tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await {
            if let SessionUpdate::Rows(changes) = u {
                apply(&mut rows, changes);
            }
        }
        let tools: Vec<&str> = asked.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tools, ["_tools", "_instructions", "list_sessions"], "{rows:?}");
        let replies: Vec<&str> = rows.iter().filter(|r| r.kind == crate::RowKind::Markdown).map(|r| r.html.as_str()).collect();
        assert!(replies.last().is_some_and(|r| r.contains("marmalade-otter")), "{replies:?}");
        assert!(rows.iter().any(|r| r.kind == crate::RowKind::Tool), "{rows:?}");
    }

    /// harlo-agent as a helper, on the plan: briefed as harlo briefs one,
    /// with the helper's tools, it does the task and reports (reminded
    /// once if it ends without). The report is checked as harlo checks it.
    #[tokio::test]
    #[ignore]
    async fn harlo_agent_reports_as_a_helper() {
        let home = PathBuf::from(std::env::var("HARLO_TEST_HOME").expect("a scratch HARLO_TEST_HOME"));
        assert!(home.starts_with("/tmp/"), "not a scratch folder: {home:?}");
        let work = home.join("work-helper");
        std::fs::create_dir_all(&work).unwrap();
        let handle = SessionHandle::spawn(HarnessConfig {
            kind: HarnessKind::Harlo,
            program: Some(std::env::var("HARLO_TEST_HARLO_AGENT").expect("HARLO_TEST_HARLO_AGENT").into()),
            cwd: work.clone(),
            session: SessionTarget::New,
            extra_args: vec!["--harlo-tools".into()],
            env: vec![("XDG_DATA_HOME".to_owned(), home.join("data").to_string_lossy().into_owned())],
            remote: None,
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        handle.send_raw(json!({ "type": "harlo_set_modes", "approval": "yolo", "plan": false }));
        handle.send_raw(json!({ "type": "set_thinking_level", "level": "low" }));
        let brief = crate::orchestration::helper_brief("main", "Create add.py with a function add(a, b) that returns their sum.", true);
        handle.prompt(&brief, Vec::new(), SendMode::Normal);
        let mut reports: Vec<Result<crate::orchestration::Report, String>> = Vec::new();
        let mut file = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(240);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::Meta(m) => file = m.session_file.or(file),
                SessionUpdate::HarloCall { id, tool, args } => {
                    let answer = match tool.as_str() {
                        "_tools" => Ok(crate::orchestration::helper_tools()),
                        "_instructions" => Ok(json!(crate::orchestration::BASE_INSTRUCTIONS)),
                        "report_done" => {
                            let r = crate::orchestration::Report::parse(&args);
                            reports.push(r.clone());
                            r.map(|_| json!({ "reported": true, "next": "End your turn now: harlo sends your report when it ends." }))
                        }
                        other => Err(format!("harlo has no tool {other}.")),
                    };
                    let mut a = json!({ "type": "harlo_result", "call": id, "ok": answer.is_ok() });
                    match answer {
                        Ok(v) => a["result"] = v,
                        Err(e) => a["error"] = json!(e),
                    }
                    handle.send_raw(a);
                }
                SessionUpdate::Status(SessionStatus::Idle) => break,
                _ => {}
            }
        }
        let text = std::fs::read_to_string(work.join("add.py")).unwrap_or_default();
        assert!(text.contains("def add"), "{text:?}");
        let good = reports.iter().filter_map(|r| r.as_ref().ok()).collect::<Vec<_>>();
        assert!(good.len() == 1 && good[0].succeeded && good[0].files.iter().any(|f| f.ends_with("add.py")), "{reports:?}");
        // (Whether it needed the reminder: in the log, for the record.)
        let reminded = file.and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default().contains("harlo_report_reminder");
        eprintln!("reported {} time(s); reminded: {reminded}", reports.len());
    }

    /// harlo-agent's task subagents, on the plan: two explore subagents,
    /// one per folder, at the same time; the Subagents box hears them start
    /// and end, the reply has what each found, and the saved session lists
    /// them. Scratch folder as above.
    #[tokio::test]
    #[ignore]
    async fn harlo_agent_runs_task_subagents() {
        let home = PathBuf::from(std::env::var("HARLO_TEST_HOME").expect("a scratch HARLO_TEST_HOME"));
        assert!(home.starts_with("/tmp/"), "not a scratch folder: {home:?}");
        let work = home.join("work-task");
        for (dir, code) in [("alpha", "4417"), ("beta", "9023")] {
            std::fs::create_dir_all(work.join(dir)).unwrap();
            std::fs::write(work.join(dir).join("notes.txt"), format!("Some notes.\nThe {dir} code is {code}.\n")).unwrap();
        }
        let handle = SessionHandle::spawn(HarnessConfig {
            kind: HarnessKind::Harlo,
            program: Some(std::env::var("HARLO_TEST_HARLO_AGENT").expect("HARLO_TEST_HARLO_AGENT").into()),
            cwd: work.clone(),
            session: SessionTarget::New,
            extra_args: vec![],
            env: vec![("XDG_DATA_HOME".to_owned(), home.join("data").to_string_lossy().into_owned())],
            remote: None,
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        handle.send_raw(json!({ "type": "harlo_set_modes", "approval": "yolo", "plan": false }));
        handle.send_raw(json!({ "type": "set_thinking_level", "level": "low" }));
        handle.prompt(
            "Use the task tool once, with two explore subagents at the same time: one finds the code in alpha/notes.txt, the other the code in beta/notes.txt. Don't read the files yourself. Then tell me both codes.",
            Vec::new(),
            SendMode::Normal,
        );
        let (mut frames, mut meta) = (Vec::new(), SessionMetaInfo::default());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(240);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::SubagentActivity(f) => frames.push(f),
                SessionUpdate::Meta(m) => meta = m,
                SessionUpdate::Status(SessionStatus::Idle) => break,
                _ => {}
            }
        }
        while let Ok(Some(u)) = tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::Meta(m) => meta = m,
                _ => {}
            }
        }
        let with = |status: &str| frames.iter().filter(|f| f["type"] == "subagent_lifecycle" && f["payload"]["status"] == status).count();
        assert_eq!((with("running"), with("completed")), (2, 2), "{frames:?}");
        let replies: Vec<&str> = rows.iter().filter(|r| r.kind == crate::RowKind::Markdown).map(|r| r.html.as_str()).collect();
        assert!(replies.last().is_some_and(|r| r.contains("4417") && r.contains("9023")), "{replies:?}");
        let file = meta.session_file.expect("its file");
        let saved = crate::subagents::from_session_file(&file);
        assert!(saved.len() == 2 && saved.iter().all(|s| s["status"] == "completed" && s["sessionFile"].is_string()), "{saved:?}");
        // What it used counts its subagents' requests too.
        let own = crate::context::spent(&harlo_harness::store::read_messages(&file).unwrap());
        let all = crate::context::spent_in_file(&file).unwrap();
        assert!(own.requests >= 2 && all.requests >= own.requests + 2 && all.input > own.input, "{own:?} {all:?}");
        eprintln!("used: {}", all.line(&meta.model_id));
    }

    #[tokio::test]
    async fn demo_run_reaches_subscribers() {
        let handle = SessionHandle::spawn(cfg(HarnessKind::Demo, SessionTarget::New));
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        handle.prompt("hello fast", Vec::new(), SendMode::Normal);
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Busy))).await);
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        // Let the final flush land.
        while let Ok(Some(u)) = tokio::time::timeout(Duration::from_millis(400), rx.recv()).await {
            if let SessionUpdate::Rows(changes) = u {
                apply(&mut rows, changes);
            }
        }
        let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds.first(), Some(&crate::RowKind::User));
        assert!(kinds.contains(&crate::RowKind::Tool) && kinds.contains(&crate::RowKind::Code));

        // A late subscriber gets the same rows as a snapshot.
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx2));
        let mut late = Vec::new();
        assert!(collect_until(&mut rx2, &mut late, |u| matches!(u, SessionUpdate::Reset(_))).await);
        assert_eq!(late, rows);
    }

    /// An idle session's harness stops (to read its config again) and the
    /// next message starts it again; a working one is left alone.
    #[tokio::test]
    async fn idle_session_restarts_when_used_again() {
        let handle = SessionHandle::spawn(cfg(HarnessKind::Demo, SessionTarget::New));
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        handle.restart_if_idle();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Offline))).await);
        handle.prompt("hello fast", Vec::new(), SendMode::Normal);
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Starting))).await);
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Busy))).await);
        // Working: not stopped.
        handle.restart_if_idle();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
    }

    /// An approval harlo asks for: shown as the agent's questions are,
    /// kept when its process stops, and its answer harlo's (not the agent's).
    #[tokio::test]
    async fn approval_gates_are_harlos() {
        let handle = SessionHandle::spawn(cfg(HarnessKind::Demo, SessionTarget::New));
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        let id = handle.open_gate("", "Merge it?", vec!["Approve".into(), "Reject".into()]);
        assert!(id.starts_with(GATE_PREFIX));
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::UiRequest(r) if r.id == id && r.method == "select" && r.message == "Merge it?" && r.options.len() == 2)).await);
        // The agent's process stops: the request stays.
        handle.restart_if_idle();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Offline))).await);
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx2));
        assert!(collect_until(&mut rx2, &mut Vec::new(), |u| matches!(u, SessionUpdate::UiRequest(r) if r.id == id)).await);
        // Answered: to harlo, with what was asked; the agent isn't started for it.
        handle.answer_ui(&id, json!({ "value": "Approve" }));
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::GateAnswered { question, answer, .. } if question == "Merge it?" && answer["value"] == "Approve")).await);
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::UiCleared(c) if *c == id)).await);
        let started = tokio::time::timeout(Duration::from_secs(1), collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Starting)))).await;
        assert!(!matches!(started, Ok(true)));
    }

    /// A sink with a box to type in (the window's, a phone's).
    struct ComposingSink(mpsc::UnboundedSender<SessionUpdate>);
    impl SessionSink for ComposingSink {
        fn deliver(&self, updates: Vec<SessionUpdate>) {
            for u in updates {
                let _ = self.0.send(u);
            }
        }
        fn composes(&self) -> bool {
            true
        }
    }

    /// Messages given back from what `rx` has now.
    async fn given_back(rx: &mut mpsc::UnboundedReceiver<SessionUpdate>) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(Some(u)) = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            if let SessionUpdate::Unsent(held) = u {
                out.extend(held.into_iter().map(|u| u.text));
            }
        }
        out
    }

    /// A message its agent couldn't start for comes back to the box; with
    /// no box open, the session keeps it for the first one that opens, and
    /// only that one.
    #[tokio::test]
    async fn a_message_that_couldnt_be_sent_comes_back() {
        let gone = std::env::temp_dir().join(format!("harlo-gone-{}", std::process::id()));
        let handle = SessionHandle::spawn(HarnessConfig { kind: HarnessKind::Harlo, program: Some("/nonexistent/harlo-agent".into()), cwd: gone, session: SessionTarget::New, extra_args: vec![], env: vec![], remote: None });
        // Only a watcher (as the workspace's is): nothing to give it back to.
        let (tx, mut watcher) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        handle.prompt("first words", Vec::new(), harlo_harness::SendMode::Normal);
        assert!(given_back(&mut watcher).await.is_empty(), "a watcher has no box");
        // The first box opened gets it, with the snapshot; the next doesn't.
        let (tx, mut window) = mpsc::unbounded_channel();
        handle.subscribe(ComposingSink(tx));
        assert_eq!(given_back(&mut window).await, ["first words"]);
        // With a box open, it comes back at once.
        handle.prompt("second words", Vec::new(), harlo_harness::SendMode::Normal);
        assert_eq!(given_back(&mut window).await, ["second words"]);
        let (tx, mut phone) = mpsc::unbounded_channel();
        handle.subscribe(ComposingSink(tx));
        assert!(given_back(&mut phone).await.is_empty(), "taken already");
        let mut rows = Vec::new();
        let (tx, mut late) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        collect_until(&mut late, &mut rows, |u| matches!(u, SessionUpdate::Reset(_))).await;
        assert!(rows.iter().any(|r| r.text.contains("Not sent: your message is back in the box.")), "said in the transcript");
    }

    /// A run you stop says so (its helpers stop with it); one that finishes doesn't.
    #[tokio::test]
    async fn a_stopped_run_says_so() {
        let handle = SessionHandle::spawn(cfg(HarnessKind::Demo, SessionTarget::New));
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        handle.prompt("hello", Vec::new(), harlo_harness::SendMode::Normal);
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Busy))).await);
        handle.abort();
        let mut stopped = false;
        assert!(collect_until(&mut rx, &mut rows, |u| { stopped |= matches!(u, SessionUpdate::Stopped); matches!(u, SessionUpdate::Status(SessionStatus::Idle)) }).await);
        assert!(stopped);
        handle.prompt("fast", Vec::new(), harlo_harness::SendMode::Normal);
        let mut stopped = false;
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Busy))).await);
        assert!(collect_until(&mut rx, &mut rows, |u| { stopped |= matches!(u, SessionUpdate::Stopped); matches!(u, SessionUpdate::Status(SessionStatus::Idle)) }).await);
        assert!(!stopped);
    }

    /// A long session opens with the report kept from last time (the
    /// panels show it at once) and gives the exact one once it has loaded,
    /// which is kept for its next opening; with none kept, the first report
    /// is the loaded one, never one of zeros.
    #[tokio::test]
    async fn a_long_session_shows_its_last_report_while_it_opens() {
        let _turn = REPORTS_TURN.lock().await;
        let dir = std::env::temp_dir().join(format!("harlo-reports-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reports = dir.join("history");
        std::fs::create_dir_all(&reports).unwrap();
        *REPORTS_IN.lock().unwrap() = Some(reports.clone());
        let mut text = String::from("{\"type\":\"session\",\"id\":\"s\",\"cwd\":\"/w\"}\n");
        let mut parent = "null".to_owned();
        for i in 0..3000 {
            text += &format!("{{\"type\":\"message\",\"id\":\"u{i}\",\"parentId\":{parent},\"message\":{{\"role\":\"user\",\"content\":\"request {i}\"}}}}\n");
            text += &format!("{{\"type\":\"message\",\"id\":\"a{i}\",\"parentId\":\"u{i}\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"answer {i}\"}}]}}}}\n");
            parent = format!("\"a{i}\"");
        }
        let (kept_one, none_kept) = (dir.join("kept.jsonl"), dir.join("none.jsonl"));
        std::fs::write(&kept_one, &text).unwrap();
        std::fs::write(&none_kept, &text).unwrap();
        let kept = harlo_harness::claude::report_file(&reports, &kept_one);
        std::fs::write(&kept, r#"{"messages":5998,"kept":true}"#).unwrap();

        let handle = SessionHandle::open_saved(cfg(HarnessKind::Harlo, SessionTarget::Resume(kept_one.clone())));
        let early = handle.context_report().await.expect("a report");
        assert_eq!(early["kept"], true, "the kept one while it loads: {early}");
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        assert!(collect_until(&mut rx, &mut Vec::new(), |u| matches!(u, SessionUpdate::Status(SessionStatus::Offline))).await);
        let loaded = handle.context_report().await.expect("a report");
        assert_eq!((loaded["messages"].as_u64(), loaded.get("kept")), (Some(6000), None), "loaded: the exact one");
        // Kept for next time (off the loop: a moment later).
        let deadline = Instant::now() + Duration::from_secs(10);
        let now_kept = loop {
            let r: Value = std::fs::read(&kept).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            if r["messages"] == 6000 || Instant::now() > deadline {
                break r;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(now_kept["messages"], 6000, "kept: {now_kept}");

        let handle = SessionHandle::open_saved(cfg(HarnessKind::Harlo, SessionTarget::Resume(none_kept.clone())));
        let first = handle.context_report().await.expect("a report");
        assert_eq!(first["messages"].as_u64(), Some(6000), "none kept: the loaded one first");
        *REPORTS_IN.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A long session opens with the rows kept from last time, shown before
    /// it has loaded, then its own; those are kept for its next opening.
    #[tokio::test]
    async fn a_long_session_shows_its_last_rows_while_it_opens() {
        let _turn = REPORTS_TURN.lock().await;
        let dir = std::env::temp_dir().join(format!("harlo-kept-rows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reports = dir.join("history");
        std::fs::create_dir_all(&reports).unwrap();
        *REPORTS_IN.lock().unwrap() = Some(reports.clone());
        let mut text = String::from("{\"type\":\"session\",\"id\":\"s\",\"cwd\":\"/w\"}\n");
        let mut parent = "null".to_owned();
        for i in 0..3000 {
            text += &format!("{{\"type\":\"message\",\"id\":\"u{i}\",\"parentId\":{parent},\"message\":{{\"role\":\"user\",\"content\":\"request {i}\"}}}}\n");
            text += &format!("{{\"type\":\"message\",\"id\":\"a{i}\",\"parentId\":\"u{i}\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"answer {i}\"}}]}}}}\n");
            parent = format!("\"a{i}\"");
        }
        let file = dir.join("long.jsonl");
        std::fs::write(&file, &text).unwrap();
        let kept = harlo_harness::claude::rows_file(&reports, &file);
        let row = Row { key: "kept".into(), kind: crate::transcript::RowKind::Markdown, text: "kept from last time".into(), ..Default::default() };
        std::fs::write(&kept, serde_json::to_vec(&[&row]).unwrap()).unwrap();

        let handle = SessionHandle::open_saved(cfg(HarnessKind::Harlo, SessionTarget::Resume(file.clone())));
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        // Before it has loaded: the kept row.
        let mut rows = Vec::new();
        let mut before = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut offline = false;
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Reset(r) => rows = r,
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::Status(SessionStatus::Offline) => offline = true,
                _ => {}
            }
            if before.is_empty() && !rows.is_empty() && !offline {
                before = rows.iter().map(|r| r.text.clone()).collect();
            }
            if offline && rows.iter().any(|r| r.html.contains("answer 2999")) {
                break;
            }
        }
        assert_eq!(before, ["kept from last time"], "shown while it loads");
        let shown: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert!(!shown.contains(&"kept from last time"), "replaced once loaded");
        assert!(rows.iter().any(|r| r.html.contains("answer 2999")), "its own rows: {:?}", &shown[shown.len().saturating_sub(3)..]);
        // Its own last rows kept for next time (off the loop: a moment later).
        let deadline = Instant::now() + Duration::from_secs(10);
        let now_kept = loop {
            let r: Vec<Row> = std::fs::read(&kept).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            if r.iter().any(|r| r.html.contains("answer 2999")) || Instant::now() > deadline {
                break r;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert!(now_kept.iter().any(|r| r.html.contains("answer 2999")) && now_kept.len() >= SHOWN_FIRST && now_kept.len() <= 2 * SHOWN_FIRST, "kept {}", now_kept.len());
        assert_eq!(now_kept[0].kind, crate::transcript::RowKind::User, "from a message of yours");
        *REPORTS_IN.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A message sent while a long session still loads shows at once, under
    /// the rows kept from last time, and stays, once, when it has loaded.
    #[tokio::test]
    async fn a_message_sent_while_a_long_session_loads_shows_at_once() {
        let _turn = REPORTS_TURN.lock().await;
        let dir = std::env::temp_dir().join(format!("harlo-send-while-loading-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reports = dir.join("history");
        std::fs::create_dir_all(&reports).unwrap();
        std::fs::create_dir_all(dir.join("work")).unwrap();
        *REPORTS_IN.lock().unwrap() = Some(reports.clone());
        let mut text = String::from("{\"type\":\"session\",\"id\":\"s\",\"cwd\":\"/w\"}\n");
        let mut parent = "null".to_owned();
        let pad = "x".repeat(400);
        for i in 0..30000 {
            text += &format!("{{\"type\":\"message\",\"id\":\"q{i}\",\"parentId\":{parent},\"message\":{{\"role\":\"user\",\"content\":\"request {i}\"}}}}\n");
            text += &format!("{{\"type\":\"message\",\"id\":\"r{i}\",\"parentId\":\"q{i}\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"answer {i} {pad}\"}}]}}}}\n");
            parent = format!("\"r{i}\"");
        }
        let file = dir.join("long.jsonl");
        std::fs::write(&file, &text).unwrap();
        let row = Row { key: "kept".into(), kind: crate::transcript::RowKind::Markdown, text: "kept from last time".into(), ..Default::default() };
        std::fs::write(harlo_harness::claude::rows_file(&reports, &file), serde_json::to_vec(&[&row]).unwrap()).unwrap();

        let mut config = cfg(HarnessKind::Demo, SessionTarget::Resume(file.clone()));
        config.cwd = dir.join("work");
        let handle = SessionHandle::open_saved(config);
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows: Vec<Arc<Row>> = Vec::new();
        let (mut sent, mut shown_early, mut loaded) = (false, false, false);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Reset(r) => rows = r,
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                _ => {}
            }
            let has = |t: &str| rows.iter().any(|r| r.text.contains(t) || r.html.contains(t));
            if !sent && has("kept from last time") {
                handle.prompt("show the todos now", Vec::new(), harlo_harness::SendMode::Normal);
                sent = true;
            }
            if sent && !loaded && has("show the todos now") && has("kept from last time") && !has("answer 29999") {
                shown_early = true;
            }
            if has("answer 29999") && has("show the todos now") {
                loaded = true;
                break;
            }
        }
        assert!(shown_early, "shown under the kept rows before it had loaded");
        assert!(loaded, "with its own rows once loaded");
        let mine = rows.iter().filter(|r| r.kind == crate::transcript::RowKind::User && r.text.contains("show the todos now")).count();
        assert_eq!(mine, 1, "once");
        assert!(!rows.iter().any(|r| r.text == "kept from last time"), "the kept rows gone once loaded");
        *REPORTS_IN.lock().unwrap() = None;
        drop(handle);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A remote copy known to lack its end (the host said it was longer than
    /// what came) isn't exported; a whole one is.
    #[test]
    fn a_copy_short_of_its_end_isnt_exported() {
        let slot = std::env::temp_dir().join(format!("harlo-short-copy-{}", std::process::id()));
        std::fs::create_dir_all(&slot).unwrap();
        let copy = slot.join("s.jsonl");
        std::fs::write(&copy, "x".repeat(1000)).unwrap();
        std::fs::write(slot.join("source.json"), r#"{"host":"nas","path":"/s.jsonl","used":0,"size":1107}"#).unwrap();
        let short = whole_copy(&copy, "nas");
        assert!(short.as_ref().is_err_and(|e| e.contains("not all of it came over from nas") && e.contains("107")), "{short:?}");
        std::fs::write(slot.join("source.json"), r#"{"host":"nas","path":"/s.jsonl","used":0,"size":1000}"#).unwrap();
        assert_eq!(whole_copy(&copy, "nas"), Ok(()));
        let _ = std::fs::remove_dir_all(&slot);
    }

    /// An export reads its file twice; a call (and its failed result) added
    /// between the two is in neither, and the next export has it whole.
    #[test]
    fn an_export_reads_the_file_as_it_was() {
        let path = std::env::temp_dir().join(format!("harlo-export-grows-{}.jsonl", std::process::id()));
        std::fs::write(&path, concat!(
            r#"{"type":"session","id":"s","cwd":"/w"}"#, "\n",
            r#"{"type":"message","id":"a","parentId":null,"message":{"role":"user","content":"build it"}}"#, "\n",
            r#"{"type":"message","id":"b","parentId":"a","message":{"role":"assistant","content":[{"type":"text","text":"Building."}]}}"#, "\n",
        )).unwrap();
        let added = concat!(
            r#"{"type":"message","id":"c","parentId":"b","message":{"role":"assistant","content":[{"type":"toolCall","id":"t1","name":"bash","arguments":{"command":"make all"}}]}}"#, "\n",
            r#"{"type":"message","id":"d","parentId":"c","message":{"role":"toolResult","toolCallId":"t1","toolName":"bash","content":[{"type":"text","text":"make: *** no rule"}],"isError":true}}"#, "\n",
        );
        let info = crate::export::Info { title: "t".into(), agent: "Harlo".into(), ..Default::default() };
        let export = |path: &std::path::Path, grow: bool| {
            let size = std::fs::metadata(path).unwrap().len();
            let mut passes = 0;
            write_export(
                &mut |each| {
                    let read = harlo_harness::store::each_saved_message_upto(HarnessKind::Harlo, path, size, each);
                    passes += 1;
                    if grow && passes == 1 {
                        use std::io::Write;
                        std::fs::OpenOptions::new().append(true).open(path).unwrap().write_all(added.as_bytes()).unwrap();
                    }
                    read
                },
                &info,
                None,
            )
            .unwrap()
        };
        let first = export(&path, true);
        assert!(first.contains("Building.") && !first.contains("make all"), "added between the passes: in neither\n{first}");
        let next = export(&path, false);
        assert!(next.contains("make all") && next.contains("failed") && next.contains("no rule"), "the next export has it whole\n{next}");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn saved_session_opens_without_a_process() {
        let path = std::env::temp_dir().join(format!("harlo-saved-{}.jsonl", std::process::id()));
        std::fs::write(&path, concat!(
            r#"{"type":"session","id":"s","cwd":"/w"}"#, "\n",
            r#"{"type":"message","id":"a","parentId":null,"message":{"role":"user","content":"saved prompt"}}"#, "\n",
            r#"{"type":"message","id":"b","parentId":"a","message":{"role":"assistant","content":[{"type":"text","text":"saved answer"}]}}"#, "\n",
        )).unwrap();
        // The agent is never started: nothing is sent to the handle.
        let handle = SessionHandle::open_saved(cfg(HarnessKind::Harlo, SessionTarget::Resume(path.clone())));
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Offline))).await);
        collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Rows(_))).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "saved prompt");
        assert!(rows[1].html.contains("saved answer"));
        // Reports read the file as it is (the conversation isn't kept).
        let markdown = handle.export_markdown().await.expect("an export");
        assert!(markdown.contains("saved prompt") && markdown.contains("saved answer"), "{markdown}");
        // Written to a file as it's made: the same, and nothing left beside it.
        let out = path.with_extension("md");
        handle.export_markdown_to(out.clone()).await.expect("an export to a file");
        assert_eq!(std::fs::read_to_string(&out).unwrap(), markdown);
        let part = out.with_file_name(format!(".{}.part", out.file_name().unwrap().to_string_lossy()));
        assert!(!part.exists());
        let _ = std::fs::remove_file(&out);
        let report = handle.context_report().await.expect("a report");
        assert_eq!((report["messages"].as_u64(), report["user"].as_u64()), (Some(2), Some(1)), "{report}");
        // Its file gone, an export is an error, not an empty one.
        let _ = std::fs::remove_file(path);
        let gone = handle.export_markdown().await;
        assert!(gone.as_ref().is_err_and(|e| e.contains("couldn't be read")), "{gone:?}");
    }

    #[test]
    fn omps_session_long_approval_is_always_allow() {
        let r = ui_request(&json!({ "id": "1", "method": "select", "title": "Allow tool: write\nPath: a.py",
            "options": ["Approve", "Approve write for this session", "Deny"] }));
        assert!(r.is_approval);
        assert_eq!(r.options, ["Approve", "Deny", "Always allow"]);
        assert_eq!((r.raw["alwaysHint"].as_str(), r.raw["alwaysValue"].as_str()), (Some("write for this session"), Some("Approve write for this session")));
        // The older Approve / Deny is as it was.
        let r = ui_request(&json!({ "id": "2", "method": "select", "title": "Allow tool: bash", "options": ["Approve", "Deny"] }));
        assert!(r.is_approval && r.raw["alwaysValue"].is_null());
    }

    #[test]
    fn strips_ansi_sequences() {
        assert_eq!(strip_ansi("\u{1b}[1;32mok\u{1b}[0m done"), "ok done");
        assert_eq!(strip_ansi("\u{1b}]8;;http://x\u{7}link\u{1b}]8;;\u{7}"), "link");
    }

    #[test]
    fn state_fills_meta() {
        let mut meta = SessionMetaInfo::default();
        apply_state(&mut meta, &json!({"model":{"name":"GPT"},"thinkingLevel":"high","sessionFile":"/s/a.jsonl","sessionId":"id1","sessionName":"Fix"}));
        assert_eq!(meta.session_file.as_deref(), Some(std::path::Path::new("/s/a.jsonl")));
        assert_eq!((meta.model.as_str(), meta.thinking.as_str(), meta.name.as_str()), ("GPT", "high", "Fix"));
        apply_state(&mut meta, &json!({"model":null,"thinkingLevel":"high"}));
        assert_eq!(meta.model, "GPT", "a model not known yet leaves what's shown");
    }
}
