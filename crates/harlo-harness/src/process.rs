//! One harness child process speaking JSONL RPC over stdio.

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use crate::error::{Context as _, Error, ErrorKind};
use crate::event::{HarnessEvent, HarnessKind, Translator};
use crate::framing::{ChunkAssembler, read_record};
use crate::protocol_log::ProtocolLog;

/// How the process picks its session file.
#[derive(Debug, Clone)]
pub enum SessionTarget {
    /// Start a new persisted session.
    New,
    /// Do not persist anything.
    Ephemeral,
    /// Resume an existing session file.
    Resume(PathBuf),
}

#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub kind: HarnessKind,
    /// Explicit binary; otherwise resolved from PATH and known install locations.
    pub program: Option<PathBuf>,
    pub cwd: PathBuf,
    pub session: SessionTarget,
    pub extra_args: Vec<String>,
    /// Environment overrides applied on top of the inherited environment.
    pub env: Vec<(String, String)>,
    /// Run on another machine: ssh's arguments up to the destination, and a
    /// remote shell snippet to run first (it sets PATH). `cwd`, `program`
    /// and session paths are then paths on that machine.
    pub remote: Option<RemoteLaunch>,
}

#[derive(Debug, Clone)]
pub struct RemoteLaunch {
    /// The host's name in harlo (`ssh://<host>/…` paths).
    pub host: String,
    pub ssh_args: Vec<String>,
    pub prelude: String,
    /// Run after the harness exits (e.g. removing a file the prelude made).
    pub cleanup: String,
}

/// A path as the remote machine sees it: `ssh://host/a/b` → `/a/b`.
pub fn on_host(path: &Path) -> String {
    let s = path.to_string_lossy();
    match s.strip_prefix("ssh://") {
        Some(rest) => rest.find('/').map_or_else(|| "/".to_owned(), |i| rest[i..].to_owned()),
        None => s.into_owned(),
    }
}

/// A word for a POSIX shell, single-quoted when needed.
pub(crate) fn shell_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+=:@%,".contains(c)) {
        return s.to_owned();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The remote command line: `cd <cwd> && exec env K=V <program> <args>`.
pub(crate) fn remote_script(remote: &RemoteLaunch, program: &str, args: &[String], cwd: &Path, env: &[(String, String)]) -> String {
    // With cleanup to do, the shell waits for the harness instead of
    // becoming it.
    let mut words = if remote.cleanup.is_empty() { vec!["exec".to_owned()] } else { Vec::new() };
    if !env.is_empty() {
        words.push("env".into());
        words.extend(env.iter().map(|(k, v)| shell_quote(&format!("{k}={v}"))));
    }
    words.push(shell_quote(program));
    words.extend(args.iter().map(|a| shell_quote(a)));
    let run = format!("{} cd {} && {}", remote.prelude, shell_quote(&on_host(cwd)), words.join(" "));
    if remote.cleanup.is_empty() { run } else { format!("{run}; status=$?; {}; exit $status", remote.cleanup) }
}

/// While streaming, a prompt must say how it should be queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendMode {
    /// A new turn (or, while one runs, as the agent queues by default).
    Normal,
    /// Into the turn under way, at its next step.
    Steer,
    /// For after the turn under way.
    FollowUp,
    /// The turn under way stopped, and this one sent in its place (what's
    /// queued stays queued).
    StopAndSend,
}

pub(crate) type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;

pub(crate) enum Outgoing {
    Frame(Value),
    Kill,
}

/// Handle to a running harness.
///
/// Dropping it closes stdin, which asks the harness to shut down; the
/// process is killed if the handle is dropped before it exits on its own.
pub struct Harness {
    kind: HarnessKind,
    tx: mpsc::UnboundedSender<Outgoing>,
    pending: Pending,
    next_id: AtomicU64,
}

impl std::fmt::Debug for Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let waiting = self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len();
        f.debug_struct("Harness").field("kind", &self.kind).field("waiting", &waiting).finish_non_exhaustive()
    }
}

const STDERR_TAIL_LINES: usize = 80;

/// The agents harlo started that are running: each leads its own process
/// group (what it starts is in it too). They end with harlo ([`stop_all`]):
/// the runtime that holds them is never dropped, so nothing else would.
static GROUPS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// An agent started (its pid leads its group).
pub(crate) fn started(pid: Option<u32>) {
    if let Some(pid) = pid {
        GROUPS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(pid);
    }
}

/// How a harness ended, for its log line: "with code 0", or "by a signal"
/// when it has no exit code (it was killed).
pub(crate) fn exit_text(code: Option<i32>) -> std::borrow::Cow<'static, str> {
    code.map_or(std::borrow::Cow::Borrowed("by a signal"), |c| std::borrow::Cow::Owned(format!("with code {c}")))
}

/// An agent ended and was waited for (its pid can be another's now).
pub(crate) fn ended(pid: Option<u32>) {
    if let Some(pid) = pid {
        GROUPS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).retain(|p| *p != pid);
    }
}

/// Ends every agent harlo started, and what they started: asked first, then
/// made to, after a moment. For when harlo quits.
pub fn stop_all() {
    stop_groups(&std::mem::take(&mut *GROUPS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)));
}

fn stop_groups(groups: &[u32]) {
    let signal = |sig| {
        for pid in groups {
            // SAFETY: plain syscall on a group harlo started.
            unsafe {
                libc::kill(-i32::try_from(*pid).unwrap_or(i32::MAX), sig);
            }
        }
    };
    signal(libc::SIGTERM);
    // (Ended once nothing's left in them; one that's done but not waited
    // for yet still counts, so this waits a second at most.)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    // SAFETY: signal 0 only asks whether the group is there.
    while std::time::Instant::now() < deadline && groups.iter().any(|pid| unsafe { libc::kill(-i32::try_from(*pid).unwrap_or(i32::MAX), 0) } == 0) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    signal(libc::SIGKILL);
}

impl Harness {
    /// Spawns the harness. Must be called inside a tokio runtime context.
    ///
    /// # Errors
    ///
    /// An [`Error`] that [`is_start`](Error::is_start) when the kind can't
    /// chat, or its program can't be found or started.
    pub fn spawn(cfg: HarnessConfig) -> Result<(Self, mpsc::UnboundedReceiver<HarnessEvent>), Error> {
        if cfg.kind == HarnessKind::Demo {
            return Ok(crate::demo::spawn());
        }
        if !cfg.kind.can_chat() {
            return Err(Error::new(ErrorKind::Start, format!("harlo can't run {} sessions yet", cfg.kind.label())));
        }
        if cfg.kind == HarnessKind::Claude {
            let program = match (&cfg.program, &cfg.remote) {
                (Some(p), _) => p.clone(),
                // There: its PATH finds it. (It needn't be on this PC too.)
                (None, Some(_)) => PathBuf::from(cfg.kind.program_name()),
                (None, None) => resolve_program(cfg.kind).or_error(ErrorKind::Start, || "could not find the `claude` binary".to_owned())?,
            };
            return crate::claude_run::spawn(cfg, program);
        }
        let args = launch_args(&cfg);
        let (program, mut cmd) = match &cfg.remote {
            // On the remote machine: its PATH finds the program.
            Some(remote) => {
                let program = match cfg.program.clone() {
                    Some(p) => p,
                    None => crate::remote_program(&remote.host, cfg.kind)?,
                };
                let mut cmd = Command::new("ssh");
                cmd.args(&remote.ssh_args)
                    .arg(remote_script(remote, &program.to_string_lossy(), &args, &cfg.cwd, &cfg.env))
                    .current_dir(std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from));
                (program, cmd)
            }
            None => {
                let program = match &cfg.program {
                    Some(p) => p.clone(),
                    None => resolve_program(cfg.kind).or_error(ErrorKind::Start, || format!("could not find the `{}` binary", cfg.kind.program_name()))?,
                };
                let mut cmd = Command::new(&program);
                cmd.args(&args).current_dir(&cfg.cwd);
                for (k, v) in &cfg.env {
                    cmd.env(k, v);
                }
                (program, cmd)
            }
        };
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // Own process group, so terminal signals aimed at us don't hit it
            // and the whole tree can be torn down together later.
            .process_group(0);
        let mut child = cmd.spawn().or_error(ErrorKind::Start, || format!("failed to start {}", program.display()))?;
        started(child.id());
        tracing::event!(
            name: "harness.spawn.success", tracing::Level::INFO,
            harness.kind = cfg.kind.as_str(),
            process.executable.path = %program.display(),
            process.working_directory = %cfg.cwd.display(),
            process.pid = child.id().unwrap_or(0),
            remote = cfg.remote.is_some(),
            "spawned {{harness.kind}}: {{process.executable.path}} in {{process.working_directory}}",
        );
        let log = ProtocolLog::open(cfg.kind, &program, &args, &cfg.cwd);

        let piped = || format!("{} started without its pipes", program.display());
        let stdin = child.stdin.take().or_error(ErrorKind::Start, piped)?;
        let stdout = child.stdout.take().or_error(ErrorKind::Start, piped)?;
        let stderr = child.stderr.take().or_error(ErrorKind::Start, piped)?;

        let (tx, rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let pending: Pending = Arc::default();
        let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::default();
        let reader = tokio::spawn(read_loop(cfg.kind, stdout, events_tx.clone(), Arc::clone(&pending), log.clone()));
        let stderr_done = tokio::spawn(stderr_loop(stderr, Arc::clone(&stderr_tail), log.clone()));
        let (kill_tx, kill_rx) = oneshot::channel();
        tokio::spawn(write_loop(stdin, rx, events_tx.clone(), kill_tx, log));
        tokio::spawn(wait_loop(child, reader, kill_rx, events_tx, stderr_done, stderr_tail));

        Ok((Self { kind: cfg.kind, tx, pending, next_id: AtomicU64::new(1) }, events_rx))
    }

    pub(crate) fn from_parts(kind: HarnessKind, tx: mpsc::UnboundedSender<Outgoing>, pending: Pending) -> Self {
        Self { kind, tx, pending, next_id: AtomicU64::new(1) }
    }

    pub fn kind(&self) -> HarnessKind {
        self.kind
    }

    fn next_id(&self) -> String {
        format!("harlo-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Sends a command without waiting. Its response arrives as
    /// [`HarnessEvent::Response`]. Returns the assigned id.
    pub fn send(&self, mut cmd: Value) -> String {
        let id = self.next_id();
        cmd["id"] = Value::String(id.clone());
        let _ = self.tx.send(Outgoing::Frame(cmd));
        id
    }

    /// Sends a command and waits for its response `data`.
    ///
    /// # Errors
    ///
    /// An [`Error`] when the agent isn't running or ends before answering
    /// ([`is_gone`](Error::is_gone)), or answers with an error
    /// ([`is_refused`](Error::is_refused)), its own text the message.
    pub async fn request(&self, cmd: Value) -> Result<Value, Error> {
        // (A queued message that went out meanwhile can't be changed.)
        if self.in_steps(&cmd) {
            let data = self.rpc(cmd).await?;
            return if data["removed"] == false || data["promoted"] == false {
                Err(Error::new(ErrorKind::AlreadySent, "That message has been sent already."))
            } else {
                Ok(data)
            };
        }
        self.rpc(cmd).await
    }

    /// Whether [`request`](Self::request) checks this command's answer:
    /// removing or promoting a queued message, which fails when the
    /// message was sent meanwhile; [`ask`](Self::ask) only sends.
    #[must_use]
    pub fn in_steps(&self, cmd: &Value) -> bool {
        matches!(cmd["type"].as_str(), Some("remove_queued_message" | "promote_queued_message"))
    }

    /// Sends a command now and returns its answer to wait for.
    ///
    /// The command goes out in the order of this call, among prompts and
    /// other commands, whenever the answer is awaited. Use this rather than
    /// [`request`](Self::request) in a spawned task when order matters: a
    /// mode change then a prompt.
    ///
    /// # Errors
    ///
    /// An [`Error`] that [`is_gone`](Error::is_gone) when the agent isn't
    /// running.
    pub fn ask(&self, mut cmd: Value) -> Result<Answer, Error> {
        let id = self.next_id();
        cmd["id"] = Value::String(id.clone());
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(id.clone(), tx);
        if self.tx.send(Outgoing::Frame(cmd)).is_err() {
            self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&id);
            return Err(Error::new(ErrorKind::NotRunning, "harness is not running"));
        }
        Ok(Answer(rx))
    }

    /// One command to the harness, and its answer.
    async fn rpc(&self, cmd: Value) -> Result<Value, Error> {
        self.ask(cmd)?.get().await
    }

    /// Sends a user prompt. `images` are `ImageContent` values (see
    /// [`image_from_file`]). Returns the request id.
    pub fn prompt(&self, text: &str, images: Vec<Value>, mode: SendMode) -> String {
        let mut cmd = json!({ "type": "prompt", "message": text });
        if !images.is_empty() {
            cmd["images"] = Value::Array(images);
        }
        match mode {
            SendMode::Normal => {}
            SendMode::Steer => cmd["streamingBehavior"] = json!("steer"),
            SendMode::FollowUp => cmd["streamingBehavior"] = json!("followUp"),
            SendMode::StopAndSend => cmd["type"] = json!("abort_and_prompt"),
        }
        self.send(cmd)
    }

    pub fn abort(&self) {
        self.send(json!({ "type": "abort" }));
    }

    /// Answers a blocking `extension_ui_request`. `answer` holds the
    /// method-specific fields (`value`, `confirmed`, `cancelled`, `answers`).
    pub fn answer_ui(&self, request_id: &str, answer: Value) {
        // Answers can come from a phone: anything but an object is wrapped
        // (an `ask`'s list of answers as `answers`, anything else as `value`).
        let mut answer = match answer {
            Value::Object(_) => answer,
            Value::Array(_) => json!({ "answers": answer }),
            other => json!({ "value": other }),
        };
        answer["type"] = json!("extension_ui_response");
        answer["id"] = json!(request_id);
        // Not a command: no id of our own, no response expected.
        let _ = self.tx.send(Outgoing::Frame(answer));
    }

    /// Stops the process: closes its stdin (an orderly exit), and kills it
    /// if it hasn't gone a few seconds later.
    pub fn kill(&self) {
        let _ = self.tx.send(Outgoing::Kill);
    }
}

/// A command's answer from the agent, still to come (from [`Harness::ask`]).
#[derive(Debug)]
pub struct Answer(oneshot::Receiver<Value>);

impl Answer {
    /// Waits for the answer's `data`.
    ///
    /// # Errors
    ///
    /// An [`Error`] that [`is_gone`](Error::is_gone) when the agent ends
    /// before answering, or [`is_refused`](Error::is_refused) when it
    /// answers with an error.
    pub async fn get(self) -> Result<Value, Error> {
        let resp = self.0.await.ok().or_error(ErrorKind::Exited, || "harness exited before responding".to_owned())?;
        if resp["success"].as_bool() == Some(true) {
            Ok(resp.get("data").cloned().unwrap_or(Value::Null))
        } else {
            Err(Error::new(ErrorKind::Refused, resp["error"].as_str().unwrap_or("command failed")))
        }
    }
}

/// Reads an image file into an `ImageContent` value for a prompt.
///
/// # Errors
///
/// An [`Error`] that [`is_image`](Error::is_image) when the file isn't PNG,
/// JPEG, GIF or WebP by its name, or can't be read.
pub fn image_from_file(path: &Path) -> Result<Value, Error> {
    use base64::Engine;
    let mime = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => return Err(Error::new(ErrorKind::Image, format!("{} is not a supported image (png, jpeg, gif, webp)", path.display()))),
    };
    let bytes = std::fs::read(path).or_error(ErrorKind::Image, || format!("reading {}", path.display()))?;
    Ok(json!({
        "type": "image",
        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        "mimeType": mime,
    }))
}

fn launch_args(cfg: &HarnessConfig) -> Vec<String> {
    let mut args: Vec<String> = match cfg.kind {
        HarnessKind::Harlo => vec!["--mode".into(), "rpc".into()],
        HarnessKind::Claude | HarnessKind::Demo => vec![],
    };
    match &cfg.session {
        SessionTarget::New => {}
        SessionTarget::Ephemeral => args.push("--no-session".into()),
        SessionTarget::Resume(path) => {
            args.push("--session".into());
            args.push(if cfg.remote.is_some() { on_host(path) } else { path.to_string_lossy().into_owned() });
        }
    }
    args.extend(cfg.extra_args.iter().cloned());
    args
}

/// Finds the harness binary on PATH, then in the default install locations
/// (a desktop launch often has a minimal PATH).
pub fn resolve_program(kind: HarnessKind) -> Option<PathBuf> {
    let name = kind.program_name();
    // harlo's own agent: the one built and shipped with this harlo first.
    if kind == HarnessKind::Harlo
        && let Some(shipped) = std::env::current_exe().ok().and_then(|exe| shipped_with(&exe, name))
    {
        return Some(shipped);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let known = match kind {
        HarnessKind::Claude => vec![home.join(".local/bin/claude"), home.join(".claude/local/claude"), PathBuf::from("/usr/local/bin/claude"), PathBuf::from("/usr/bin/claude")],
        HarnessKind::Harlo => vec![home.join(".local/bin/harlo-agent")],
        HarnessKind::Demo => vec![],
    };
    known.into_iter().find(|p| is_executable(p))
}

/// A program shipped with harlo (`exe`): beside it (a build), else in
/// `lib/harlo` next to its folder (the packages: `usr/bin` and
/// `usr/lib/harlo`, `/opt/harlo/bin` and `/opt/harlo/lib/harlo`).
fn shipped_with(exe: &Path, name: &str) -> Option<PathBuf> {
    let dir = exe.parent()?;
    [dir.join(name), dir.join("../lib/harlo").join(name)].into_iter().find(|p| is_executable(p))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

async fn read_loop(
    kind: HarnessKind,
    stdout: tokio::process::ChildStdout,
    events: mpsc::UnboundedSender<HarnessEvent>,
    pending: Pending,
    log: Option<Arc<ProtocolLog>>,
) {
    let mut reader = BufReader::with_capacity(256 * 1024, stdout);
    let mut buf = Vec::new();
    let mut chunks = ChunkAssembler::default();
    let mut translator = Translator::new(kind);

    loop {
        let record = match read_record(&mut reader, &mut buf).await {
            Ok(Some(r)) => r,
            Ok(None) => break,
            Err(e) => {
                let _ = events.send(HarnessEvent::ProtocolError(e.to_string()));
                break;
            }
        };
        if record.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Some(log) = &log {
            log.frame("in", &record);
        }
        let mut frame: Value = match serde_json::from_slice(&record) {
            Ok(v) => v,
            Err(e) => {
                let preview = String::from_utf8_lossy(&record[..record.len().min(200)]).into_owned();
                let _ = events.send(HarnessEvent::ProtocolError(format!("invalid JSON on stdout ({e}): {preview}")));
                continue;
            }
        };

        if frame["type"] == "rpc_chunk" {
            match chunks.push(&frame) {
                Ok(Some(full)) => frame = full,
                Ok(None) => continue,
                Err(e) => {
                    let _ = events.send(HarnessEvent::ProtocolError(e.to_string()));
                    continue;
                }
            }
        } else if chunks.in_progress() {
            chunks.abandon();
            let _ = events.send(HarnessEvent::ProtocolError("rpc_chunk sequence interrupted".into()));
        }

        if frame["type"] == "response" {
            let waiter = frame["id"].as_str().and_then(|id| pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(id));
            if let Some(waiter) = waiter {
                let _ = waiter.send(frame);
                continue;
            }
        }

        for ev in translator.translate(frame) {
            if events.send(ev).is_err() {
                return;
            }
        }
    }
    // Wake anyone still waiting for a response.
    pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
}

async fn write_loop(
    mut stdin: tokio::process::ChildStdin,
    mut rx: mpsc::UnboundedReceiver<Outgoing>,
    events: mpsc::UnboundedSender<HarnessEvent>,
    kill_tx: oneshot::Sender<()>,
    log: Option<Arc<ProtocolLog>>,
) {
    let _ = events.send(HarnessEvent::Ready);

    let mut kill_tx = Some(kill_tx);
    while let Some(out) = rx.recv().await {
        match out {
            Outgoing::Frame(frame) => {
                if let Err(e) = write_frame(&mut stdin, &frame, log.as_deref()).await {
                    tracing::event!(name: "harness.stdin.closed", tracing::Level::WARN, error.message = %e, "harness stdin closed: {{error.message}}");
                    break;
                }
            }
            Outgoing::Kill => {
                if let Some(tx) = kill_tx.take() {
                    let _ = tx.send(());
                }
                break;
            }
        }
    }
    // Dropping stdin asks the harness for an orderly shutdown.
}

async fn write_frame(stdin: &mut tokio::process::ChildStdin, frame: &Value, log: Option<&ProtocolLog>) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(frame).expect("serializable frame");
    if let Some(log) = log {
        log.frame("out", &line);
    }
    line.push(b'\n');
    stdin.write_all(&line).await?;
    stdin.flush().await
}

async fn stderr_loop(stderr: tokio::process::ChildStderr, tail: Arc<Mutex<VecDeque<String>>>, log: Option<Arc<ProtocolLog>>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::event!(name: "harness.stderr.line", target: "harness_stderr", tracing::Level::DEBUG, line = %line, "{{line}}");
        if let Some(log) = &log {
            log.stderr(&line);
        }
        let mut tail = tail.lock().unwrap();
        if tail.len() == STDERR_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }
}

async fn wait_loop(
    mut child: Child,
    reader: tokio::task::JoinHandle<()>,
    kill_rx: oneshot::Receiver<()>,
    events: mpsc::UnboundedSender<HarnessEvent>,
    stderr_done: tokio::task::JoinHandle<()>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
) {
    // (Taken first: once waited for, the child has no pid to give.)
    let pid = child.id();
    // Drain stdout fully before reporting the exit so no events are lost.
    tokio::select! {
        _ = reader => {}
        // Asked to stop: its stdin is closed (an orderly exit); killed only
        // if it lingers.
        Ok(()) = kill_rx => {
            if tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await.is_err() {
                let _ = child.start_kill();
            }
        }
    }
    let code = child.wait().await.ok().and_then(|s| s.code());
    ended(pid);
    tracing::event!(name: "harness.exit", tracing::Level::INFO, process.pid = pid.unwrap_or(0), process.exit = %exit_text(code), "harness {{process.pid}} exited {{process.exit}}");
    // What it said last (a child that kept stderr open isn't waited for).
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), stderr_done).await;
    let stderr_tail = stderr_tail.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n");
    let _ = events.send(HarnessEvent::Exited { code, stderr_tail });
}

#[cfg(test)]
mod tests {
    /// An agent and what it started end with harlo, even one that doesn't
    /// stop when asked.
    #[test]
    fn its_agents_end_with_it() {
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("sh")
            .args(["-c", "trap '' TERM; sleep 60 & sleep 60; wait"])
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = child.id();
        std::thread::sleep(std::time::Duration::from_millis(200));
        // SAFETY: kill(2) with signal 0 only checks the group exists; it
        // takes integers and touches no memory of this process.
        assert_eq!(unsafe { libc::kill(-i32::try_from(pid).unwrap_or(i32::MAX), 0) }, 0, "running");
        let started = std::time::Instant::now();
        stop_groups(&[pid]);
        let _ = child.wait();
        std::thread::sleep(std::time::Duration::from_millis(100));
        // SAFETY: kill(2) with signal 0 only checks the group exists; it
        // takes integers and touches no memory of this process.
        assert_ne!(unsafe { libc::kill(-i32::try_from(pid).unwrap_or(i32::MAX), 0) }, 0, "all of it gone");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    use super::*;

    #[test]
    fn encodes_images_and_rejects_others() {
        let dir = std::env::temp_dir().join(format!("harlo-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.PNG");
        std::fs::write(&png, [0x89, b'P', b'N', b'G']).unwrap();
        let img = image_from_file(&png).unwrap();
        assert_eq!((img["type"].as_str(), img["mimeType"].as_str(), img["data"].as_str()), (Some("image"), Some("image/png"), Some("iVBORw==")));
        let txt = dir.join("b.txt");
        std::fs::write(&txt, "x").unwrap();
        image_from_file(&txt).unwrap_err();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An answer that isn't an object (a phone's) is wrapped, not a panic.
    #[test]
    fn answers_of_any_shape() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let h = Harness::from_parts(HarnessKind::Demo, tx, Pending::default());
        let mut sent = |answer: Value| {
            h.answer_ui("ui_1", answer);
            match rx.try_recv() {
                Ok(Outgoing::Frame(v)) => v,
                _ => panic!("no frame"),
            }
        };
        assert_eq!(sent(json!({ "confirmed": true })), json!({ "type": "extension_ui_response", "id": "ui_1", "confirmed": true }));
        assert_eq!(sent(json!([{ "id": "db" }])), json!({ "type": "extension_ui_response", "id": "ui_1", "answers": [{ "id": "db" }] }));
        assert_eq!(sent(json!("Approve")), json!({ "type": "extension_ui_response", "id": "ui_1", "value": "Approve" }));
    }

    #[test]
    fn launch_args_per_harness() {
        let cfg = |kind, session| HarnessConfig { kind, program: None, cwd: PathBuf::from("/"), session, extra_args: vec![], env: vec![], remote: None };
        assert_eq!(launch_args(&cfg(HarnessKind::Harlo, SessionTarget::Resume("/s.jsonl".into()))), ["--mode", "rpc", "--session", "/s.jsonl"]);
        assert_eq!(launch_args(&cfg(HarnessKind::Harlo, SessionTarget::Ephemeral)), ["--mode", "rpc", "--no-session"]);
        assert_eq!(launch_args(&cfg(HarnessKind::Claude, SessionTarget::New)), Vec::<String>::new());
    }

    #[test]
    fn remote_command_line() {
        let mut remote = RemoteLaunch { host: "nas".into(), ssh_args: vec!["me@nas".into()], prelude: "PATH=\"$HOME/.local/bin:$PATH\";".into(), cleanup: String::new() };
        let script = remote_script(&remote, "harlo-agent", &["--mode".into(), "rpc".into(), "--session".into(), "/s/it's.jsonl".into()], Path::new("ssh://nas/home/me/my proj"), &[("A".into(), "1 2".into())]);
        assert_eq!(script, r#"PATH="$HOME/.local/bin:$PATH"; cd '/home/me/my proj' && exec env 'A=1 2' harlo-agent --mode rpc --session '/s/it'\''s.jsonl'"#);
        remote.cleanup = "rm -f /tmp/x".into();
        assert_eq!(remote_script(&remote, "harlo-agent", &[], Path::new("/w"), &[]), r#"PATH="$HOME/.local/bin:$PATH"; cd /w && harlo-agent; status=$?; rm -f /tmp/x; exit $status"#);
    }

    #[test]
    fn a_program_harlo_puts_there() {
        #[expect(clippy::unnecessary_wraps, reason = "the signature set_remote_program takes")]
        fn put(_: &str, kind: HarnessKind) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
            Ok((kind == HarnessKind::Harlo).then(|| "/home/me/.cache/harlo/harlo-agent-00ff".to_owned()))
        }
        crate::set_remote_program(put);
        assert_eq!(crate::remote_program("nas", HarnessKind::Harlo).unwrap(), Path::new("/home/me/.cache/harlo/harlo-agent-00ff"));
        // Others: that machine's PATH finds them.
        assert_eq!(crate::remote_program("nas", HarnessKind::Claude).unwrap(), Path::new("claude"));
    }

    /// An agent that reads and never answers: what's left unanswered ends
    /// when it's stopped.
    use std::time::Duration;

    /// A stand-in agent running `script` (sh) in a folder of its own, named
    /// `name` and the test's pid.
    async fn stand_in(name: &str, script: &str) -> (Harness, mpsc::UnboundedReceiver<HarnessEvent>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("harlo-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("harlo-agent");
        std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&program, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let cfg = HarnessConfig { kind: HarnessKind::Harlo, program: Some(program), cwd: dir.clone(), session: SessionTarget::Ephemeral, extra_args: vec![], env: vec![], remote: None };
        // Another test thread forking while the script was still open for
        // writing keeps it busy for a moment ("Text file busy"): try again.
        let mut tries = 0;
        loop {
            let spawned = {
                let _dir = crate::protocol_log::TEST_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                Harness::spawn(cfg.clone())
            };
            match spawned {
                Err(e) if tries < 20 && format!("{e:#}").contains("Text file busy") => {
                    tries += 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                other => {
                    let (harness, events) = other.unwrap();
                    break (harness, events, dir);
                }
            }
        }
    }

    /// A command sent with `ask` goes out before what's sent after it, even
    /// when its answer is awaited in a task that runs later (here: one
    /// thread, so it runs only after the prompt is sent).
    #[tokio::test]
    async fn asked_commands_go_out_in_the_order_sent() {
        let (harness, _events, dir) = stand_in("ordered-agent", "while read -r line; do printf '%s\\n' \"$line\" >> sent; done").await;
        let harness = std::sync::Arc::new(harness);
        let answer = harness.ask(json!({ "type": "harlo_set_modes", "approval": "write", "plan": false })).unwrap();
        let waiting = tokio::spawn(answer.get());
        harness.prompt("and now", Vec::new(), SendMode::Normal);
        let sent = dir.join("sent");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let lines = loop {
            let lines: Vec<Value> = std::fs::read_to_string(&sent).unwrap_or_default().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
            if lines.len() >= 2 || std::time::Instant::now() > deadline {
                break lines;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let types: Vec<&str> = lines.iter().map(|l| l["type"].as_str().unwrap_or_default()).collect();
        assert_eq!(types, ["harlo_set_modes", "prompt"]);
        harness.kill();
        assert!(waiting.await.unwrap().unwrap_err().is_gone(), "unanswered: ended by the stop");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn unanswered_requests_end_with_the_stop() {
        let (harness, mut events, dir) = stand_in("silent-agent", "while read -r _; do :; done").await;
        let wait = Duration::from_secs(5);
        let asked = harness.request(json!({ "type": "get_available_models" }));
        tokio::pin!(asked);
        assert!(tokio::time::timeout(Duration::from_millis(200), &mut asked).await.is_err(), "it never answers");
        // Its group, among those ended when harlo quits (other tests' too).
        let ours = |pid: &u32| std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|c| String::from_utf8_lossy(&c).contains(&*dir.to_string_lossy()));
        let pid = GROUPS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().copied().find(ours).expect("its group is kept");
        harness.kill();
        let stopped = tokio::time::timeout(wait, asked).await.expect("ended by the stop").unwrap_err();
        assert_eq!(stopped.to_string(), "harness exited before responding");
        // Stopped and waited for: its pid is let go, as it may be another's next.
        while !matches!(tokio::time::timeout(wait, events.recv()).await.expect("its exit is reported"), Some(HarnessEvent::Exited { .. }) | None) {}
        assert!(!GROUPS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).contains(&pid), "its group is forgotten once it ends");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_what_ships_with_harlo() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("harlo-shipped-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let exe = root.join("opt/harlo/bin/harlo");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::create_dir_all(root.join("opt/harlo/lib/harlo")).unwrap();
        let make = |p: &Path| {
            std::fs::write(p, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        assert_eq!(shipped_with(&exe, "harlo-agent"), None);
        // The packages': in lib/harlo.
        let lib = root.join("opt/harlo/lib/harlo/harlo-agent");
        make(&lib);
        assert_eq!(shipped_with(&exe, "harlo-agent").map(|p| p.canonicalize().unwrap()), Some(lib.canonicalize().unwrap()));
        // A build's, beside it, comes first.
        let beside = root.join("opt/harlo/bin/harlo-agent");
        make(&beside);
        assert_eq!(shipped_with(&exe, "harlo-agent"), Some(beside));
        let _ = std::fs::remove_dir_all(&root);
    }
}
