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
/// How long after a session ends its memory is handed back.
const GIVE_BACK_AFTER: Duration = Duration::from_secs(2);

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
    /// omp tool approval (`Allow tool: …` with Approve/Deny).
    pub is_approval: bool,
    /// Raw request, for methods that need more (omp `ask` questions).
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
    /// The model as `provider/id` (what omp's settings and `--model` take).
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
}

/// A slash command the harness accepts in a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    /// Argument hint (omp), e.g. `<plan|scan>`.
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
    /// omp subagent event (`subagent_lifecycle` / `subagent_progress` frame).
    SubagentActivity(Value),
    /// omp todo phases (`[{ name, tasks: [{ content, status, blocker? }] }]`).
    Todos(Value),
    /// omp side question frame (`btw_delta` / `btw_record`).
    Btw(Value),
    /// The agent calling one of harlo's tools on its pipe (harlo-agent):
    /// the workspace answers it.
    HarloCall { id: String, tool: String, args: Value },
    /// omp goal (`{ id, objective, status, tokenBudget?, tokensUsed,
    /// timeUsedSeconds, … }`), or null when there is none.
    Goal(Value),
    /// Worth a desktop notification (questions come as `UiRequest`).
    Notable(Notable),
    /// Claude's plan limits, as it last said (see `HarnessEvent::Limits`).
    Limits(Value),
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
    /// An omp subagent finished (`status`: completed, failed, aborted).
    SubagentDone { id: String, agent: String, status: String, task: String },
}

/// Receives batched updates on the session task. Implementations forward
/// them to the UI thread.
pub trait SessionSink: Send + 'static {
    fn deliver(&self, updates: Vec<SessionUpdate>);
}

enum Control {
    Subscribe(u64, Box<dyn SessionSink>),
    Unsubscribe(u64),
    Prompt(String, Vec<Value>, SendMode),
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
    Export(oneshot::Sender<String>),
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
        if self.held().is_some() {
            return false;
        }
        let _ = self.control.send(Control::Prompt(text.to_owned(), images, mode));
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
    /// harness.
    pub async fn export_markdown(&self) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        self.control.send(Control::Export(tx)).ok()?;
        rx.await.ok()
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
    shown: Vec<Arc<Row>>,
    status: SessionStatus,
    meta: SessionMetaInfo,
    queue: (Vec<String>, Vec<String>),
    commands: Vec<SlashCommand>,
    requests: Vec<UiRequest>,
    status_entries: Vec<(String, String)>,
    /// Todo phases: omp's from the last `get_state`, pi's from its todo tool.
    todos: Value,
    goal: Value,
    /// Calls of the agent's `todo` tool still running.
    todo_calls: std::collections::HashSet<String>,
    /// Commands the agent left running in the background.
    background: usize,
    /// Asked for the session file during this run (a new session gets
    /// one only once its first message is under way).
    file_asked: bool,
    sinks: Vec<(u64, Box<dyn SessionSink>)>,
    messages: KeptMessages,
    /// Why the current run failed, if it did (reported when it settles).
    run_error: Option<String>,
    /// Subagents already reported as finished.
    subagents_done: std::collections::HashSet<String>,
    /// The current run was stopped by you.
    aborted: bool,
}

/// Complete messages (history plus finished live ones) for reports, as JSON
/// text: only reports read them, and parsed they take twice the memory.
#[derive(Default)]
struct KeptMessages {
    list: Vec<Box<str>>,
    index: std::collections::HashMap<String, usize>,
}

impl KeptMessages {
    /// From this many, what parsing them used goes back to the system.
    const MANY: usize = 2000;

    fn set(&mut self, messages: Vec<Value>) {
        self.list = messages.iter().map(|m| m.to_string().into_boxed_str()).collect();
        self.index.clear();
        drop(messages);
        self.release();
    }

    fn upsert(&mut self, id: &str, message: &Value) {
        let text = message.to_string().into_boxed_str();
        match self.index.get(id) {
            Some(&i) => self.list[i] = text,
            None => {
                self.index.insert(id.to_owned(), self.list.len());
                self.list.push(text);
            }
        }
    }

    fn all(&self) -> Vec<Value> {
        self.list.iter().filter_map(|m| serde_json::from_str(m).ok()).collect()
    }

    /// After using `all`.
    fn release(&self) {
        if self.list.len() >= Self::MANY {
            harlo_harness::store::give_back();
        }
    }

    /// The last assistant reply's text (plain, trimmed).
    fn last_reply(&self) -> String {
        self.list
            .iter()
            .rev()
            .filter_map(|m| serde_json::from_str::<Value>(m).ok())
            .filter(|m| m["role"] == "assistant")
            .map(|m| match &m["content"] {
                Value::String(s) => s.clone(),
                Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
                _ => String::new(),
            })
            .find(|t| !t.trim().is_empty())
            .map(|t| t.trim().to_owned())
            .unwrap_or_default()
    }
}

impl DriverState {
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
    /// pi: the shield last set, for when it's started again.
    modes: Option<Value>,
    /// Claude: the thinking level to start on.
    thinking: Option<String>,
    /// Stop it once it has finished what it's doing.
    restart_settled: bool,
    /// omp wouldn't reopen the session: the model it's on isn't there. The
    /// loop starts it again on `model_instead`, once (that start is `fell_back`).
    model_gone: Option<String>,
    model_instead: Option<String>,
    fell_back: bool,
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
        self.fell_back = self.model_instead.is_some();
        if let Some(model) = self.model_instead.take() {
            cfg.extra_args.extend(["--model".to_owned(), model]);
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
                // The shield it's on: pi's is harlo's own; the others', before
                // anything they're sent.
                if let Some(modes) = self.modes.clone() {
                    if harness.kind() == HarnessKind::Pi {
                        let h = harness.clone();
                        tokio::spawn(async move {
                            let _ = h.request(modes).await;
                        });
                    } else {
                        harness.send(modes);
                    }
                }
                Some(harness)
            }
            Err(e) => {
                st.transcript.apply(&match gone {
                    Some(cwd) => HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Error, text: format!("This session can't continue: its folder, {}, is gone.", cwd.display()), system: false },
                    None => HarnessEvent::ProtocolError(format!("{e:#}")),
                });
                set_status(st, SessionStatus::Exited, pending);
                None
            }
        }
    }
}

/// The model omp couldn't reopen a session on, from what it said as it quit.
fn gone_model(stderr: &str) -> Option<String> {
    stderr.lines().find_map(|l| l.trim().strip_prefix("SessionModelRestoreError: Could not restore model ")).map(str::to_owned)
}

/// The model a new session would start on (no session is made, or prompt sent).
async fn default_model(cfg: &HarnessConfig) -> Option<String> {
    let cfg = HarnessConfig { session: SessionTarget::Ephemeral, ..cfg.clone() };
    let (harness, _events) = Harness::spawn(cfg).ok()?;
    let state = tokio::time::timeout(Duration::from_secs(25), harness.request(json!({ "type": "get_state" }))).await;
    harness.kill();
    let state = state.ok()?.ok()?;
    Some(format!("{}/{}", state["model"]["provider"].as_str()?, state["model"]["id"].as_str()?))
}

/// Snapshots before each turn (undo a turn), unless Settings turned them
/// off (`undo_snapshots` "0": for a repository with huge untracked files).
pub fn turn_snapshots_on() -> bool {
    crate::db::Db::open(&crate::db::default_path()).ok().and_then(|d| d.get("settings.undo_snapshots").ok().flatten()).as_deref() != Some("0")
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
    let mut exports: Vec<oneshot::Sender<String>> = Vec::new();
    let mut st = DriverState {
        transcript: Transcript::new(),
        shown: Vec::new(),
        status: if spawn_now { SessionStatus::Starting } else { SessionStatus::Loading },
        meta: SessionMetaInfo { session_file: session_file.clone(), ..Default::default() },
        queue: Default::default(),
        commands: Vec::new(),
        requests: Vec::new(),
        status_entries: Vec::new(),
        todos: Value::Null,
        goal: Value::Null,
        todo_calls: Default::default(),
        background: 0,
        file_asked: false,
        sinks: Vec::new(),
        messages: KeptMessages::default(),
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
        model_gone: None,
        model_instead: None,
        fell_back: false,
    };
    let mut pending: Vec<SessionUpdate> = Vec::new();
    let mut dirty = false;
    let mut last_flush = Instant::now() - STREAM_FLUSH;

    if spawn_now {
        driver.ensure_harness(&mut st, &mut pending);
    } else if let Some(path) = session_file {
        let tx = aux_tx.clone();
        let remote = driver.cfg.remote.clone();
        let program = driver.cfg.program.clone().or_else(|| harlo_harness::resolve_program(kind));
        tokio::task::spawn_blocking(move || {
            // OpenCode's sessions are in its database, not a file (a remote
            // host's: through its server).
            if kind == harlo_harness::HarnessKind::OpenCode {
                let saved = match &remote {
                    Some(r) => harlo_harness::opencode_remote_saved(r, &path),
                    None => harlo_harness::opencode::saved(&path),
                };
                let _ = tx.send(Aux::Loaded(saved, 0));
                return;
            }
            // An ACP agent's: its replay (it has no file either).
            if harlo_harness::acp::is_acp(kind) {
                let program = if remote.is_some() { Some(std::path::PathBuf::from(kind.as_str())) } else { program };
                let saved = program.map(|p| harlo_harness::acp::saved(&path, &p, remote.as_ref())).unwrap_or_default();
                let _ = tx.send(Aux::Loaded(saved, 0));
                return;
            }
            // A remote session: read from its copy here, brought up to date
            // over SSH with what was added.
            let (saved, size, missing) = if crate::remote::is_remote(&path) {
                match crate::remote::local_copy(&path) {
                    Ok(copy) => (
                        harlo_harness::store::read_saved_for(kind, &copy).map_err(anyhow::Error::from),
                        std::fs::metadata(&copy).map_or(0, |m| m.len()),
                        harlo_harness::mirror::missing(&copy),
                    ),
                    Err(e) => (Err(e), 0, 0),
                }
            } else {
                let size = std::fs::metadata(&path).map_or(0, |m| m.len());
                (harlo_harness::store::read_saved_for(kind, &path).map_err(anyhow::Error::from), size, 0)
            };
            let saved = saved.unwrap_or_else(|e| {
                tracing::warn!("reading {}: {e:#}", path.display());
                Default::default()
            });
            harlo_harness::store::after_reading(size);
            let _ = tx.send(Aux::Loaded(saved, missing));
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
                // Its model is gone: the session opens on the default instead
                // of on its stack trace. (A pi session does the same itself.)
                if let Some(model) = driver.model_gone.take() {
                    driver.events = None;
                    driver.harness = None;
                    match default_model(&driver.cfg).await {
                        Some(instead) => {
                            let text = format!("{model} isn't available any more. This session is on {instead} now; pick another model below if you'd rather.");
                            dirty |= st.transcript.apply(&HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Warning, text, system: false });
                            driver.model_instead = Some(instead);
                            driver.ensure_harness(&mut st, &mut pending);
                        }
                        None => {
                            let message = format!("{} can't open this session: its model, {model}, isn't available, and no other model is.", driver.cfg.kind.as_str());
                            pending.push(SessionUpdate::Notable(Notable::Error { message }));
                        }
                    }
                    continue;
                }
                // Its host disconnected by hand: said as that, not as a failure.
                let by_hand = matches!(ev, HarnessEvent::Exited { .. }).then(|| driver.cfg.remote.as_ref().map(|r| r.host.clone()).filter(|h| crate::remote::is_disconnected(h))).flatten();
                dirty |= match by_hand {
                    Some(host) => st.transcript.apply(&HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Info, text: format!("Disconnected from {host}."), system: false }),
                    None => st.transcript.apply(&ev),
                };
                let login = match &ev {
                    HarnessEvent::Response { success: false, error: Some(e), .. } | HarnessEvent::PromptResult { error: Some(e), .. } => Some(asks_for_login(e)),
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
                Aux::Loaded(saved, missing) => {
                    st.transcript.load_messages(&saved.messages);
                    // (A copy is the start of the host's file: what didn't
                    // come is the end, its latest messages.)
                    if missing > 0 {
                        let host = driver.cfg.remote.as_ref().map(|r| r.host.clone()).unwrap_or_default();
                        let text = format!(
                            "Not all of this session came over from {host}: its last {} (the latest messages) aren't shown. Close it and open it again to fetch the rest.",
                            crate::attach::human_size(missing)
                        );
                        st.transcript.apply(&HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Warning, text, system: false });
                    }
                    st.meta.first_message = first_user_text(&saved.messages);
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
                        (st.meta.context_tokens, st.meta.context_window, st.meta.compact_at) = (harlo_harness::claude::context_used(&saved.messages), window, at);
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
                    if saved.messages.len() >= KeptMessages::MANY {
                        let first = st.transcript.last_rows(SHOWN_FIRST);
                        let changes = diff_rows(&st.shown, &first);
                        st.shown = first;
                        if !changes.is_empty() {
                            pending.push(SessionUpdate::Rows(changes));
                        }
                        st.broadcast(std::mem::take(&mut pending));
                        last_flush = Instant::now();
                    }
                    st.messages.set(saved.messages);
                    for reply in exports.drain(..) {
                        let _ = reply.send(export(&st, &driver));
                    }
                    dirty = true;
                }
                Aux::History(messages) => {
                    st.transcript.load_messages(&messages);
                    // pi: its to-do list, from the last time it changed (none
                    // while the extension is off: it wouldn't be kept up).
                    if kind == HarnessKind::Pi {
                        let host = driver.cfg.remote.as_ref().map(|r| r.host.clone()).unwrap_or_default();
                        let todos = Some(&messages).filter(|_| crate::pi_packages::todos_enabled_on(&host)).and_then(|m| crate::pi_packages::todos_in(m)).unwrap_or_else(|| json!([]));
                        if todos != st.todos && !(st.todos.is_null() && todos == json!([])) {
                            st.todos = todos;
                            pending.push(SessionUpdate::Todos(st.todos.clone()));
                        }
                    }
                    if st.meta.first_message.is_empty() {
                        st.meta.first_message = first_user_text(&messages);
                        pending.push(SessionUpdate::Meta(st.meta.clone()));
                    }
                    st.messages.set(messages);
                    dirty = true;
                }
                Aux::State(state) => {
                    if let Some(todos) = state.get("todoPhases").filter(|t| t.is_array())
                        && *todos != st.todos {
                            st.todos = todos.clone();
                            pending.push(SessionUpdate::Todos(st.todos.clone()));
                        }
                    // `goal`: `{ enabled, mode, goal }`, or null.
                    if let Some(goal_state) = state.get("goal") {
                        let goal = goal_state.get("goal").cloned().unwrap_or(Value::Null);
                        if goal != st.goal {
                            st.goal = goal;
                            pending.push(SessionUpdate::Goal(st.goal.clone()));
                        }
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
                    if stats["contextUsage"].get("compactAt").is_none() {
                        meta.compact_at = compact_at(&driver.cfg, &meta);
                    }
                    if meta != st.meta {
                        st.meta = meta;
                        pending.push(SessionUpdate::Meta(st.meta.clone()));
                    }
                }
                Aux::Commands(list) => {
                    st.commands = parse_commands(&list);
                    pending.push(SessionUpdate::Commands(st.commands.clone()));
                }
            },
            ctl = control.recv() => match ctl {
                Some(Control::Subscribe(id, sink)) => {
                    // Bring the newcomer up to date with what others have seen.
                    sink.deliver(st.snapshot());
                    st.sinks.push((id, sink));
                }
                Some(Control::Unsubscribe(id)) => st.sinks.retain(|(s, _)| *s != id),
                Some(Control::Prompt(text, images, mode)) => {
                    // A new turn (not steering or a queued one): the folder's
                    // files first, so the turn can be undone (git::snapshot_turn).
                    if st.status != SessionStatus::Busy && turn_snapshots_on() {
                        let dir = match &driver.cfg.remote {
                            Some(r) => crate::remote::join(&r.host, &driver.cfg.cwd.to_string_lossy()),
                            None => driver.cfg.cwd.clone(),
                        };
                        let _ = tokio::task::spawn_blocking(move || crate::git::snapshot_turn(&dir)).await;
                    }
                    if let Some(h) = driver.ensure_harness(&mut st, &mut pending) {
                        // A prompt sent while idle must not carry a queue mode.
                        let mode = if st.status == SessionStatus::Busy { mode } else { SendMode::Normal };
                        h.prompt(&text, images, mode);
                    }
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
                Some(Control::ContextReport(reply)) => {
                    let live = (st.meta.context_window > 0).then_some((st.meta.context_tokens, st.meta.context_window, st.meta.cost));
                    let _ = reply.send(crate::context::report(&st.messages.all(), live));
                    st.messages.release();
                }
                // (A saved session still loading: once it has.)
                Some(Control::Export(reply)) if st.status == SessionStatus::Loading => exports.push(reply),
                Some(Control::Export(reply)) => {
                    let _ = reply.send(export(&st, &driver));
                }
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

        if (dirty || !pending.is_empty()) && last_flush.elapsed() >= STREAM_FLUSH {
            if dirty {
                let rows = st.transcript.rows();
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
        }
    }
    st.messages.list.len() >= KeptMessages::MANY
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
            let commands = match driver.cfg.kind {
                harlo_harness::HarnessKind::Omp => "get_available_commands",
                _ => "get_commands",
            };
            let (h, tx) = (harness.clone(), aux.clone());
            tokio::spawn(async move {
                if let Ok(data) = h.request(json!({ "type": commands })).await {
                    let _ = tx.send(Aux::Commands(data["commands"].clone()));
                }
            });
        }
        HarnessEvent::ToolStart { call_id, name, .. } if name == "todo" => {
            st.todo_calls.insert(call_id.clone());
        }
        // The plan changed: re-read the todo phases (pi's todo package says
        // the list in its result).
        HarnessEvent::ToolEnd { call_id, result, .. } if st.todo_calls.remove(call_id) => {
            if driver.cfg.kind != HarnessKind::Pi {
                tokio::spawn(fetch_state(harness.clone(), aux.clone()));
            } else if let Some(todos) = crate::pi_packages::todo_phases(result)
                && todos != st.todos
            {
                st.todos = todos;
                pending.push(SessionUpdate::Todos(st.todos.clone()));
            }
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
                    None => Some(Notable::Completed { last_message: st.messages.last_reply(), background: st.background }),
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
            st.messages.upsert(id, message);
        }
        HarnessEvent::Retry { started: false, error: Some(error), .. } => st.run_error = Some(error.clone()),
        HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Error, text, system: false } => {
            pending.push(SessionUpdate::Notable(Notable::Error { message: text.clone() }));
        }
        HarnessEvent::CommandsChanged(list) => {
            let _ = aux.send(Aux::Commands(list.clone()));
        }
        HarnessEvent::Limits(info) => pending.push(SessionUpdate::Limits(info.clone())),
        HarnessEvent::Exited { code, stderr_tail } => {
            if driver.cfg.kind == HarnessKind::Omp && !driver.fell_back {
                driver.model_gone = gone_model(stderr_tail);
            }
            driver.fell_back = false;
            // Mid-run, or with a failure code: worth knowing. (What it said,
            // not the end of a stack trace. Its host disconnected by hand:
            // nothing went wrong, and that was asked first if it was working.)
            let disconnected = driver.cfg.remote.as_ref().is_some_and(|r| crate::remote::is_disconnected(&r.host));
            if !disconnected && driver.model_gone.is_none() && (st.status == SessionStatus::Busy || code.is_some_and(|c| c != 0)) {
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
        HarnessEvent::Btw(frame) => pending.push(SessionUpdate::Btw(frame.clone())),
        HarnessEvent::HarloCall { id, tool, args } => pending.push(SessionUpdate::HarloCall { id: id.clone(), tool: tool.clone(), args: args.clone() }),
        HarnessEvent::Goal(goal) => {
            st.goal = goal.clone();
            pending.push(SessionUpdate::Goal(st.goal.clone()));
        }
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
    /// one's, how much of it didn't come over (its end).
    Loaded(harlo_harness::store::SavedSession, u64),
    /// Messages from the harness (`get_messages`).
    History(Vec<Value>),
    State(Value),
    Stats(Value),
    Commands(Value),
}

async fn fetch_initial(harness: Arc<Harness>, tx: mpsc::UnboundedSender<Aux>, resume: bool) {
    if resume {
        match harness.request(json!({ "type": "get_messages" })).await {
            Ok(data) => {
                let messages = data["messages"].as_array().cloned().unwrap_or_default();
                let _ = tx.send(Aux::History(messages));
            }
            Err(e) => tracing::warn!("get_messages failed: {e}"),
        }
    }
    fetch_state(harness, tx).await;
}

/// The session as Markdown, as it is now.
fn export(st: &DriverState, driver: &Driver) -> String {
    let info = crate::export::Info {
        title: if st.meta.name.is_empty() { st.meta.first_message.clone() } else { st.meta.name.clone() },
        agent: driver.cfg.kind.label().to_owned(),
        model: st.meta.model.clone(),
        folder: crate::sidebar::tilde(&driver.cfg.cwd),
    };
    let markdown = crate::export::markdown(&st.messages.all(), &info);
    st.messages.release();
    markdown
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
}

/// Where the agent compacts, from its settings (another machine's as last
/// read there: until they have been, none).
fn compact_at(cfg: &HarnessConfig, meta: &SessionMetaInfo) -> u64 {
    if meta.context_window == 0 {
        return 0;
    }
    let window = meta.context_window;
    let (provider, model) = meta.model_id.split_once('/').unwrap_or(("", &meta.model_id));
    let at = match (&cfg.remote, cfg.kind) {
        (Some(r), HarnessKind::Pi) => crate::compaction::pi_on(&r.host, &harlo_harness::on_host(&cfg.cwd), &meta.model_id, window),
        (Some(r), HarnessKind::Omp) => crate::compaction::omp_on(&r.host, &meta.model_id, window),
        (Some(r), HarnessKind::Hermes) => crate::compaction::hermes_on(&r.host, model, provider, window),
        (Some(_), _) => None,
        (None, HarnessKind::Pi) => {
            let dir = cfg.env.iter().find(|(k, _)| k == "PI_CODING_AGENT_DIR").map(|(_, v)| std::path::Path::new(v));
            crate::compaction::pi(dir, &cfg.cwd, &meta.model_id, window)
        }
        (None, HarnessKind::Omp) => crate::compaction::omp(&meta.model_id, window),
        (None, HarnessKind::Hermes) => crate::compaction::hermes(model, provider, window),
        _ => None,
    };
    at.unwrap_or(0)
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
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
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
    for m in messages {
        if m["role"] != "user" {
            continue;
        }
        let content = &m["content"];
        let text = content.as_str().map(str::to_owned).or_else(|| {
            content
                .as_array()
                .and_then(|a| a.iter().find_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten()))
                .map(str::to_owned)
        });
        if let Some(t) = text {
            return harlo_harness::file_blocks::title_text(&t).chars().take(100).collect();
        }
    }
    String::new()
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

    #[test]
    fn kept_messages_read_back_as_they_were() {
        let (user, reply, partial) = (
            json!({ "role": "user", "content": "hi", "timestamp": 1_792_000_000_000_i64 }),
            json!({ "role": "assistant", "content": [{ "type": "text", "text": " one " }, { "type": "text", "text": "two" }], "usage": { "cost": { "total": 0.0125 } } }),
            json!({ "role": "assistant", "content": [{ "type": "toolCall", "name": "bash" }] }),
        );
        let mut kept = KeptMessages::default();
        kept.set(vec![user.clone(), reply.clone()]);
        assert_eq!(kept.all(), [user.clone(), reply.clone()]);
        assert_eq!(kept.last_reply(), "one \ntwo", "plain and trimmed");

        kept.upsert("live", &partial);
        assert_eq!(kept.last_reply(), "one \ntwo", "a reply without text is passed over");
        let done = json!({ "role": "assistant", "content": "done" });
        kept.upsert("live", &done);
        assert_eq!(kept.all(), [user, reply, done], "the same message again replaces it");
        assert_eq!(kept.last_reply(), "done");

        kept.set(Vec::new());
        assert_eq!(kept.last_reply(), "");
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

    #[test]
    fn reads_the_model_omp_lost() {
        let said = "37862 |       throw new iK(En[0]);\n                    ^\nSessionModelRestoreError: Could not restore model strata/qwen3.8\n      at cI (/$bunfs/root/chunks/cli.js:37862:13)\n";
        assert_eq!(gone_model(said).as_deref(), Some("strata/qwen3.8"));
        assert_eq!(gone_model("Some other failure"), None);
    }

    /// A real omp (`HARLO_TEST_OMP`), its home a scratch folder
    /// (`HARLO_TEST_HOME`, with a models.yml): a session whose model is gone
    /// opens on the default, with its file left as it was.
    #[tokio::test]
    #[ignore]
    async fn omp_session_with_a_gone_model_opens() {
        let home = PathBuf::from(std::env::var("HARLO_TEST_HOME").expect("a scratch HARLO_TEST_HOME"));
        assert!(home.starts_with("/tmp/"), "not a scratch folder: {home:?}");
        let file = home.join("gone.jsonl");
        let text = [
            json!({ "type": "session", "version": 3, "id": "01a19574-688a-7346-9c3d-487053aa3ec2", "timestamp": "2026-11-01T04:53:16.554Z", "cwd": home }),
            json!({ "type": "model_change", "id": "a1", "parentId": null, "timestamp": "2026-11-01T04:53:17.000Z", "model": "gone/old-model" }),
            json!({ "type": "message", "id": "a2", "parentId": "a1", "timestamp": "2026-11-01T04:53:18.000Z", "message": { "role": "user", "content": [{ "type": "text", "text": "hello" }], "timestamp": 1793508798000u64 } }),
        ]
        .map(|l| l.to_string() + "\n")
        .concat();
        std::fs::write(&file, &text).unwrap();
        let cfg = HarnessConfig {
            kind: HarnessKind::Omp,
            program: Some(std::env::var("HARLO_TEST_OMP").expect("HARLO_TEST_OMP").into()),
            cwd: home.clone(),
            session: SessionTarget::Resume(file.clone()),
            extra_args: vec![],
            env: vec![("HOME".to_owned(), home.to_string_lossy().into_owned())],
            remote: None,
        };
        let handle = SessionHandle::open_saved(cfg);
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        // (What was asked of the one that quit isn't answered.)
        assert!(handle.request(json!({ "type": "get_state" })).await.is_err());
        let state = handle.request(json!({ "type": "get_state" })).await.unwrap();
        assert_eq!(state["model"]["id"], "m1");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);

        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Idle))).await);
        while let Ok(Some(u)) = tokio::time::timeout(Duration::from_millis(400), rx.recv()).await {
            if let SessionUpdate::Rows(changes) = u {
                apply(&mut rows, changes);
            }
        }
        let shown = format!("{rows:?}");
        assert!(shown.contains("gone/old-model isn't available any more") && shown.contains("fake/m1"), "{shown}");
        assert!(!shown.contains("SessionModelRestoreError"), "{shown}");
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
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while let Ok(Some(u)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match u {
                SessionUpdate::Rows(changes) => apply(&mut rows, changes),
                SessionUpdate::UiRequest(r) => {
                    asked = Some(r);
                    break;
                }
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
        let brief = crate::orchestration::helper_brief("main", "Create add.py with a function add(a, b) that returns their sum.", true, "");
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

    #[tokio::test]
    async fn saved_session_opens_without_a_process() {
        let path = std::env::temp_dir().join(format!("harlo-saved-{}.jsonl", std::process::id()));
        std::fs::write(&path, concat!(
            r#"{"type":"session","id":"s","cwd":"/w"}"#, "\n",
            r#"{"type":"message","id":"a","parentId":null,"message":{"role":"user","content":"saved prompt"}}"#, "\n",
            r#"{"type":"message","id":"b","parentId":"a","message":{"role":"assistant","content":[{"type":"text","text":"saved answer"}]}}"#, "\n",
        )).unwrap();
        // pi is never started: nothing is sent to the handle.
        let handle = SessionHandle::open_saved(cfg(HarnessKind::Pi, SessionTarget::Resume(path.clone())));
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle.subscribe(ChannelSink(tx));
        let mut rows = Vec::new();
        assert!(collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Status(SessionStatus::Offline))).await);
        collect_until(&mut rx, &mut rows, |u| matches!(u, SessionUpdate::Rows(_))).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "saved prompt");
        assert!(rows[1].html.contains("saved answer"));
        let _ = std::fs::remove_file(path);
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
