//! One harness child process speaking JSONL RPC over stdio.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

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
    Normal,
    Steer,
    FollowUp,
}

pub(crate) type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;

pub(crate) enum Outgoing {
    Frame(Value),
    Kill,
}

/// Handle to a running harness. Dropping it closes stdin, which asks the
/// harness to shut down; the process is killed if the handle is dropped
/// before it exits on its own.
pub struct Harness {
    kind: HarnessKind,
    tx: mpsc::UnboundedSender<Outgoing>,
    pending: Pending,
    next_id: AtomicU64,
}

const STDERR_TAIL_LINES: usize = 80;
const READY_TIMEOUT: Duration = Duration::from_secs(15);

impl Harness {
    /// Spawns the harness. Must be called inside a tokio runtime context.
    pub fn spawn(cfg: HarnessConfig) -> Result<(Self, mpsc::UnboundedReceiver<HarnessEvent>)> {
        if cfg.kind == HarnessKind::Demo {
            return Ok(crate::demo::spawn());
        }
        if !cfg.kind.can_chat() {
            anyhow::bail!("harlo can't run {} sessions yet", cfg.kind.label());
        }
        if cfg.kind == HarnessKind::Claude {
            let program = match &cfg.program {
                Some(p) => p.clone(),
                None => resolve_program(cfg.kind).context("could not find the `claude` binary")?,
            };
            return crate::claude_run::spawn(cfg, program);
        }
        if cfg.kind == HarnessKind::OpenCode {
            let program = match &cfg.program {
                Some(p) => p.clone(),
                None => resolve_program(cfg.kind).context("could not find the `opencode` binary")?,
            };
            return crate::opencode_run::spawn(cfg, program);
        }
        if crate::acp::is_acp(cfg.kind) {
            let program = match (&cfg.program, &cfg.remote) {
                (Some(p), _) => p.clone(),
                // There: its PATH finds it.
                (None, Some(_)) => PathBuf::from(cfg.kind.as_str()),
                (None, None) => resolve_program(cfg.kind).with_context(|| format!("could not find the `{}` binary", cfg.kind.as_str()))?,
            };
            return crate::acp_run::spawn(cfg, program);
        }
        let args = launch_args(&cfg);
        let (program, mut cmd) = match &cfg.remote {
            // On the remote machine: its PATH finds the program.
            Some(remote) => {
                let program = cfg.program.clone().unwrap_or_else(|| PathBuf::from(cfg.kind.as_str()));
                // harlo's tools there (pi and omp take the bridge from their
                // config on the host): the bridge and the forward in place.
                if matches!(cfg.kind, HarnessKind::Pi | HarnessKind::Omp) {
                    let _ = crate::mcp_bridge_for(Some(remote));
                }
                let mut cmd = Command::new("ssh");
                cmd.args(&remote.ssh_args)
                    .arg(remote_script(remote, &program.to_string_lossy(), &args, &cfg.cwd, &cfg.env))
                    .current_dir(std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/")));
                (program, cmd)
            }
            None => {
                let program = match &cfg.program {
                    Some(p) => p.clone(),
                    None => resolve_program(cfg.kind).with_context(|| format!("could not find the `{}` binary", cfg.kind.as_str()))?,
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
        let mut child = cmd.spawn().with_context(|| format!("failed to start {}", program.display()))?;
        tracing::info!(program = %program.display(), cwd = %cfg.cwd.display(), remote = cfg.remote.is_some(), "spawned harness");
        let log = ProtocolLog::open(cfg.kind, &program, &args, &cfg.cwd);

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let (tx, rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let pending: Pending = Arc::default();
        let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::default();
        let (ready_tx, ready_rx) = oneshot::channel::<Value>();

        let reader = tokio::spawn(read_loop(cfg.kind, stdout, events_tx.clone(), pending.clone(), ready_tx, log.clone()));
        tokio::spawn(stderr_loop(stderr, stderr_tail.clone(), log.clone()));
        let (kill_tx, kill_rx) = oneshot::channel();
        tokio::spawn(write_loop(cfg.kind, stdin, rx, ready_rx, events_tx.clone(), kill_tx, log));
        tokio::spawn(wait_loop(child, reader, kill_rx, events_tx, stderr_tail));

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
    pub async fn request(&self, mut cmd: Value) -> Result<Value> {
        let id = self.next_id();
        cmd["id"] = Value::String(id.clone());
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id.clone(), tx);
        if self.tx.send(Outgoing::Frame(cmd)).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(anyhow!("harness is not running"));
        }
        let resp = rx.await.map_err(|_| anyhow!("harness exited before responding"))?;
        if resp["success"].as_bool() == Some(true) {
            Ok(resp.get("data").cloned().unwrap_or(Value::Null))
        } else {
            Err(anyhow!(resp["error"].as_str().unwrap_or("command failed").to_owned()))
        }
    }

    /// Sends a user prompt. `images` are `ImageContent` values (see
    /// [`image_from_file`]). Returns the request id (omp `prompt_result`
    /// repeats it).
    pub fn prompt(&self, text: &str, images: Vec<Value>, mode: SendMode) -> String {
        let mut cmd = json!({ "type": "prompt", "message": text });
        if !images.is_empty() {
            cmd["images"] = Value::Array(images);
        }
        match mode {
            SendMode::Normal => {}
            SendMode::Steer => cmd["streamingBehavior"] = json!("steer"),
            SendMode::FollowUp => cmd["streamingBehavior"] = json!("followUp"),
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

/// Reads an image file into an `ImageContent` value for a prompt.
pub fn image_from_file(path: &Path) -> Result<Value> {
    use base64::Engine;
    let mime = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => anyhow::bail!("{} is not a supported image (png, jpeg, gif, webp)", path.display()),
    };
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(json!({
        "type": "image",
        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        "mimeType": mime,
    }))
}

fn launch_args(cfg: &HarnessConfig) -> Vec<String> {
    let mut args: Vec<String> = match cfg.kind {
        HarnessKind::Pi => vec!["--mode".into(), "rpc".into()],
        // rpc-ui registers prompt-capable tools such as `ask`.
        HarnessKind::Omp => vec!["--mode".into(), "rpc-ui".into()],
        HarnessKind::Claude | HarnessKind::OpenCode | HarnessKind::Hermes | HarnessKind::Demo => vec![],
    };
    match &cfg.session {
        SessionTarget::New => {}
        SessionTarget::Ephemeral => args.push("--no-session".into()),
        SessionTarget::Resume(path) => {
            args.push(if cfg.kind == HarnessKind::Omp { "--resume" } else { "--session" }.into());
            args.push(if cfg.remote.is_some() { on_host(path) } else { path.to_string_lossy().into_owned() });
        }
    }
    args.extend(cfg.extra_args.iter().cloned());
    args
}

/// Finds the harness binary on PATH, then in the default install locations
/// (a desktop launch often has a minimal PATH).
pub fn resolve_program(kind: HarnessKind) -> Option<PathBuf> {
    let name = kind.as_str();
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
        HarnessKind::Pi => vec![home.join(".pi/agent/bin/pi"), home.join(".local/bin/pi")],
        HarnessKind::Omp => vec![home.join(".local/bin/omp"), home.join(".bun/bin/omp")],
        HarnessKind::Claude => vec![home.join(".local/bin/claude"), home.join(".claude/local/claude"), PathBuf::from("/usr/local/bin/claude"), PathBuf::from("/usr/bin/claude")],
        HarnessKind::OpenCode => vec![
            home.join(".opencode/bin/opencode"),
            home.join(".bun/bin/opencode"),
            home.join(".local/bin/opencode"),
            PathBuf::from("/usr/local/bin/opencode"),
            home.join("bin/opencode"),
        ],
        HarnessKind::Hermes => vec![home.join(".local/bin/hermes"), home.join(".hermes/hermes-agent/venv/bin/hermes"), home.join(".hermes/hermes-agent/.venv/bin/hermes")],
        HarnessKind::Demo => vec![],
    };
    known.into_iter().find(|p| is_executable(p))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

async fn read_loop(
    kind: HarnessKind,
    stdout: tokio::process::ChildStdout,
    events: mpsc::UnboundedSender<HarnessEvent>,
    pending: Pending,
    ready_tx: oneshot::Sender<Value>,
    log: Option<Arc<ProtocolLog>>,
) {
    let mut reader = BufReader::with_capacity(256 * 1024, stdout);
    let mut buf = Vec::new();
    let mut chunks = ChunkAssembler::default();
    let mut translator = Translator::new(kind);
    let mut ready_tx = Some(ready_tx);

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

        match frame["type"].as_str() {
            Some("ready") => {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(frame);
                }
                continue;
            }
            Some("response") => {
                let waiter = frame["id"].as_str().and_then(|id| pending.lock().unwrap().remove(id));
                if let Some(waiter) = waiter {
                    let _ = waiter.send(frame);
                    continue;
                }
            }
            _ => {}
        }

        for ev in translator.translate(frame) {
            if events.send(ev).is_err() {
                return;
            }
        }
    }
    // Wake anyone still waiting for a response.
    pending.lock().unwrap().clear();
}

async fn write_loop(
    kind: HarnessKind,
    mut stdin: tokio::process::ChildStdin,
    mut rx: mpsc::UnboundedReceiver<Outgoing>,
    ready_rx: oneshot::Receiver<Value>,
    events: mpsc::UnboundedSender<HarnessEvent>,
    kill_tx: oneshot::Sender<()>,
    log: Option<Arc<ProtocolLog>>,
) {
    // omp announces itself with a `ready` frame; hold user commands until
    // then and configure the protocol first. pi has no handshake.
    if kind == HarnessKind::Omp {
        let ready = tokio::time::timeout(READY_TIMEOUT, ready_rx).await;
        let ready = match ready {
            Ok(Ok(frame)) => frame,
            _ => {
                let _ = events.send(HarnessEvent::ProtocolError("omp did not send a ready frame".into()));
                Value::Null
            }
        };
        let supports_v2 = ready["supportedProtocolVersions"]
            .as_array()
            .is_some_and(|v| v.iter().any(|x| x.as_u64() == Some(2)));
        let mut setup = Vec::new();
        if supports_v2 {
            // Oversized frames become rpc_chunk sequences instead of failing.
            setup.push(json!({ "id": "harlo-setup-protocol", "type": "negotiate_protocol", "protocolVersion": 2 }));
        }
        // Without this, every message_update carries a full message snapshot.
        setup.push(json!({ "id": "harlo-setup-filter", "type": "set_event_filter", "events": null, "messageUpdates": "delta" }));
        // One `ask` request per tool call, with every question, instead of a
        // `select` per question. Builds without it answer with a failed
        // response and keep sending `select`, which the UI also handles.
        setup.push(json!({ "id": "harlo-setup-ask", "type": "set_ask_dialog", "enabled": true }));
        // Subagent start/progress/end frames (not their full event streams).
        setup.push(json!({ "id": "harlo-setup-subagents", "type": "set_subagent_subscription", "level": "progress" }));
        for frame in setup {
            if write_frame(&mut stdin, &frame, log.as_deref()).await.is_err() {
                return;
            }
        }
    }
    let _ = events.send(HarnessEvent::Ready);

    let mut kill_tx = Some(kill_tx);
    while let Some(out) = rx.recv().await {
        match out {
            Outgoing::Frame(frame) => {
                if let Err(e) = write_frame(&mut stdin, &frame, log.as_deref()).await {
                    tracing::warn!("harness stdin closed: {e}");
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
        tracing::debug!(target: "harness_stderr", "{line}");
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
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
) {
    // Drain stdout fully before reporting the exit so no events are lost.
    tokio::select! {
        _ = reader => {}
        // Asked to stop: its stdin is closed (an orderly exit; pi left
        // killed holds a lock the next pi waits ~25 s for); killed only if it
        // lingers.
        Ok(()) = kill_rx => {
            if tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await.is_err() {
                let _ = child.start_kill();
            }
        }
    }
    let code = child.wait().await.ok().and_then(|s| s.code());
    let stderr_tail = stderr_tail.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n");
    let _ = events.send(HarnessEvent::Exited { code, stderr_tail });
}

#[cfg(test)]
mod tests {
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
        assert!(image_from_file(&txt).is_err());
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
        assert_eq!(launch_args(&cfg(HarnessKind::Pi, SessionTarget::Resume("/s.jsonl".into()))), ["--mode", "rpc", "--session", "/s.jsonl"]);
        assert_eq!(launch_args(&cfg(HarnessKind::Omp, SessionTarget::Resume("/s.jsonl".into()))), ["--mode", "rpc-ui", "--resume", "/s.jsonl"]);
        assert_eq!(launch_args(&cfg(HarnessKind::Omp, SessionTarget::Ephemeral)), ["--mode", "rpc-ui", "--no-session"]);
    }

    #[test]
    fn remote_command_line() {
        let mut remote = RemoteLaunch { host: "nas".into(), ssh_args: vec!["me@nas".into()], prelude: "PATH=\"$HOME/.local/bin:$PATH\";".into(), cleanup: String::new() };
        let script = remote_script(&remote, "omp", &["--mode".into(), "rpc-ui".into(), "--resume".into(), "/s/it's.jsonl".into()], Path::new("ssh://nas/home/me/my proj"), &[("A".into(), "1 2".into())]);
        assert_eq!(script, r#"PATH="$HOME/.local/bin:$PATH"; cd '/home/me/my proj' && exec env 'A=1 2' omp --mode rpc-ui --resume '/s/it'\''s.jsonl'"#);
        remote.cleanup = "rm -f /tmp/x".into();
        assert_eq!(remote_script(&remote, "pi", &[], Path::new("/w"), &[]), r#"PATH="$HOME/.local/bin:$PATH"; cd /w && pi; status=$?; rm -f /tmp/x; exit $status"#);
    }

    /// omp on a real SSH server, no prompts (not run by default):
    /// `HARLO_TEST_SSH="$(scripts/test-sshd.sh start /tmp/x)" cargo test -p harlo-harness -- --ignored omp_over_ssh`
    #[tokio::test]
    #[ignore]
    async fn omp_over_ssh() {
        let ssh_args: Vec<String> = std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH").split_whitespace().map(str::to_owned).collect();
        let remote = RemoteLaunch { host: "t".into(), ssh_args, prelude: "PATH=\"$HOME/.local/bin:$PATH\"; export PATH;".into(), cleanup: String::new() };
        let cfg = HarnessConfig { kind: HarnessKind::Omp, program: None, cwd: std::env::temp_dir(), session: SessionTarget::Ephemeral, extra_args: vec![], env: vec![], remote: Some(remote) };
        let (harness, _events) = Harness::spawn(cfg).unwrap();
        let state = tokio::time::timeout(Duration::from_secs(30), harness.request(json!({ "type": "get_state" }))).await.unwrap().unwrap();
        println!("model over ssh: {}", state["model"]["id"]);
        assert!(state.get("model").is_some(), "{state}");
    }
}
