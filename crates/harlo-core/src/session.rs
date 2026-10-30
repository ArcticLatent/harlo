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

use harlo_harness::{Harness, HarnessConfig, HarnessEvent, SendMode, SessionTarget};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::transcript::{Row, RowChange, Transcript, diff_rows};

/// While events keep arriving, flush at most this often. Text is only
/// revealed in whole lines anyway, so faster flushes buy nothing.
const STREAM_FLUSH: Duration = Duration::from_millis(100);

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
    /// A status-bar entry from an extension (ANSI stripped); `None` clears it.
    StatusEntry { key: String, text: Option<String> },
    /// omp subagent event (`subagent_lifecycle` / `subagent_progress` frame).
    SubagentActivity(Value),
    /// omp todo phases (`[{ name, tasks: [{ content, status, blocker? }] }]`).
    Todos(Value),
    /// omp side question frame (`btw_delta` / `btw_record`).
    Btw(Value),
    /// omp goal (`{ id, objective, status, tokenBudget?, tokensUsed,
    /// timeUsedSeconds, … }`), or null when there is none.
    Goal(Value),
    /// Worth a desktop notification (questions come as `UiRequest`).
    Notable(Notable),
}

/// Events the app may notify about.
#[derive(Debug, Clone, PartialEq)]
pub enum Notable {
    /// A run finished; the agent's last reply (plain text).
    Completed { last_message: String },
    /// A run failed, or the harness reported an error.
    Error { message: String },
    /// An omp subagent finished (`status`: completed, failed, aborted).
    SubagentDone { id: String, agent: String, status: String, task: String },
}

/// The last assistant reply's text in `messages` (plain, trimmed).
pub fn last_assistant_text(messages: &[Value]) -> String {
    messages
        .iter()
        .rev()
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
    /// A raw command for the harness (starts it if needed).
    Raw(Value),
    /// A command whose response data the caller awaits.
    Request(Value, oneshot::Sender<Result<Value, String>>),
    /// Re-read the transcript and state from the harness (after a branch or fork).
    Reload,
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
        tokio::spawn(drive(cfg, spawn_now, control_rx));
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

    /// Re-reads the messages and state from the harness, e.g. after it
    /// moved to another branch or a forked session file.
    pub fn reload(&self) {
        let _ = self.control.send(Control::Reload);
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
    /// omp todo phases from the last `get_state`.
    todos: Value,
    goal: Value,
    /// Tool calls of omp's `todo` tool still running.
    todo_calls: std::collections::HashSet<String>,
    sinks: Vec<(u64, Box<dyn SessionSink>)>,
    /// Complete messages (history plus finished live ones) for reports.
    messages: Vec<Value>,
    message_index: std::collections::HashMap<String, usize>,
    /// Why the current run failed, if it did (reported when it settles).
    run_error: Option<String>,
    /// Subagents already reported as finished.
    subagents_done: std::collections::HashSet<String>,
    /// The current run was stopped by you.
    aborted: bool,
}

impl DriverState {
    fn set_messages(&mut self, messages: &[Value]) {
        self.messages = messages.to_vec();
        self.message_index.clear();
    }

    fn upsert_message(&mut self, id: &str, message: &Value) {
        match self.message_index.get(id) {
            Some(&i) => self.messages[i] = message.clone(),
            None => {
                self.message_index.insert(id.to_owned(), self.messages.len());
                self.messages.push(message.clone());
            }
        }
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
}

impl Driver {
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
        match Harness::spawn(cfg) {
            Ok((harness, events)) => {
                let harness = Arc::new(harness);
                self.harness = Some(harness.clone());
                self.events = Some(events);
                set_status(st, SessionStatus::Starting, pending);
                Some(harness)
            }
            Err(e) => {
                st.transcript.apply(&HarnessEvent::ProtocolError(format!("{e:#}")));
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

fn set_status(st: &mut DriverState, s: SessionStatus, pending: &mut Vec<SessionUpdate>) {
    if st.status != s {
        st.status = s;
        pending.push(SessionUpdate::Status(s));
    }
}

async fn drive(cfg: HarnessConfig, spawn_now: bool, mut control: mpsc::UnboundedReceiver<Control>) {
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
        sinks: Vec::new(),
        messages: Vec::new(),
        message_index: Default::default(),
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
                let _ = tx.send(Aux::Loaded(saved));
                return;
            }
            // An ACP agent's: its replay (it has no file either).
            if harlo_harness::acp::is_acp(kind) {
                let program = if remote.is_some() { Some(std::path::PathBuf::from(kind.as_str())) } else { program };
                let saved = program.map(|p| harlo_harness::acp::saved(&path, &p, remote.as_ref())).unwrap_or_default();
                let _ = tx.send(Aux::Loaded(saved));
                return;
            }
            // A remote session's file comes over SSH.
            let text = if crate::remote::is_remote(&path) {
                crate::remote::read(&path).map(|b| String::from_utf8_lossy(&b).into_owned())
            } else {
                std::fs::read_to_string(&path).map_err(anyhow::Error::from)
            };
            let saved = text.map(|t| harlo_harness::store::parse_saved_for(kind, &t)).unwrap_or_else(|e| {
                tracing::warn!("reading {}: {e:#}", path.display());
                Default::default()
            });
            let _ = tx.send(Aux::Loaded(saved));
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
                    set_status(&mut st, SessionStatus::Exited, &mut pending);
                    continue;
                };
                handle_event(&ev, &mut st, &mut pending, &mut driver);
                dirty |= st.transcript.apply(&ev);
            }
            Some(aux) = aux_rx.recv() => match aux {
                Aux::Loaded(saved) => {
                    st.set_messages(&saved.messages);
                    st.transcript.load_messages(&saved.messages);
                    st.meta.first_message = first_user_text(&saved.messages);
                    if !saved.name.is_empty() {
                        st.meta.name = saved.name;
                    }
                    st.meta.model = saved.model;
                    st.meta.thinking = saved.thinking;
                    pending.push(SessionUpdate::Meta(st.meta.clone()));
                    if st.status == SessionStatus::Loading {
                        set_status(&mut st, SessionStatus::Offline, &mut pending);
                    }
                    for reply in exports.drain(..) {
                        let _ = reply.send(export(&st, &driver));
                    }
                    dirty = true;
                }
                Aux::History(messages) => {
                    st.set_messages(&messages);
                    st.transcript.load_messages(&messages);
                    if st.meta.first_message.is_empty() {
                        st.meta.first_message = first_user_text(&messages);
                        pending.push(SessionUpdate::Meta(st.meta.clone()));
                    }
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
                    if let Some(h) = &driver.harness {
                        h.answer_ui(&id, answer);
                    }
                    st.requests.retain(|r| r.id != id);
                    pending.push(SessionUpdate::UiCleared(id));
                }
                Some(Control::Raw(cmd)) => {
                    if let Some(h) = driver.ensure_harness(&mut st, &mut pending) {
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
                    let _ = reply.send(crate::context::report(&st.messages, live));
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
}

fn handle_event(ev: &HarnessEvent, st: &mut DriverState, pending: &mut Vec<SessionUpdate>, driver: &mut Driver) {
    let harness = driver.harness.clone().expect("events imply a harness");
    let aux = &driver.aux_tx;
    // Its cost so far, at each tool call (spending limits; the turn's end
    // fetches it too).
    if matches!(ev, HarnessEvent::ToolStart { .. }) && st.status == SessionStatus::Busy {
        tokio::spawn(fetch_stats(harness.clone(), aux.clone()));
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
        // The plan changed: re-read the todo phases.
        HarnessEvent::ToolEnd { call_id, .. } if st.todo_calls.remove(call_id) => {
            tokio::spawn(fetch_state(harness.clone(), aux.clone()));
        }
        HarnessEvent::RunStarted => {
            st.run_error = None;
            st.aborted = false;
            set_status(st, SessionStatus::Busy, pending);
        }
        HarnessEvent::Settled => {
            let aborted = std::mem::take(&mut st.aborted);
            if st.status == SessionStatus::Busy {
                let notable = match st.run_error.take() {
                    Some(message) => Some(Notable::Error { message }),
                    None if aborted => None,
                    None => Some(Notable::Completed { last_message: last_assistant_text(&st.messages) }),
                };
                pending.extend(notable.map(SessionUpdate::Notable));
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
            st.upsert_message(id, message);
        }
        HarnessEvent::Retry { started: false, error: Some(error), .. } => st.run_error = Some(error.clone()),
        HarnessEvent::Notice { level: harlo_harness::NoticeLevel::Error, text, system: false } => {
            pending.push(SessionUpdate::Notable(Notable::Error { message: text.clone() }));
        }
        HarnessEvent::CommandsChanged(list) => {
            let _ = aux.send(Aux::Commands(list.clone()));
        }
        HarnessEvent::Exited { code, stderr_tail } => {
            // Mid-run, or with a failure code: worth knowing.
            if st.status == SessionStatus::Busy || code.is_some_and(|c| c != 0) {
                let last = stderr_tail.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
                let message = format!("{} stopped{}{}", driver.cfg.kind.as_str(), code.map_or(String::new(), |c| format!(" (exit {c})")), if last.is_empty() { String::new() } else { format!(": {last}") });
                pending.push(SessionUpdate::Notable(Notable::Error { message }));
            }
            set_status(st, SessionStatus::Exited, pending);
            for r in st.requests.drain(..) {
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
    /// The session file's branch, model and thinking level.
    Loaded(harlo_harness::store::SavedSession),
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
    crate::export::markdown(&st.messages, &info)
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
    meta.cost = stats["cost"].as_f64().unwrap_or(0.0);
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
        raw: raw.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
