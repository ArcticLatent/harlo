//! One harness child process speaking JSONL RPC over stdio.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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
            let program = match (&cfg.program, &cfg.remote) {
                (Some(p), _) => p.clone(),
                // There: its PATH finds it. (It needn't be on this PC too.)
                (None, Some(_)) => PathBuf::from(cfg.kind.program_name()),
                (None, None) => resolve_program(cfg.kind).context("could not find the `claude` binary")?,
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
                    .current_dir(std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/")));
                (program, cmd)
            }
            None => {
                let program = match &cfg.program {
                    Some(p) => p.clone(),
                    None => resolve_program(cfg.kind).with_context(|| format!("could not find the `{}` binary", cfg.kind.program_name()))?,
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
        let reader = tokio::spawn(read_loop(cfg.kind, stdout, events_tx.clone(), pending.clone(), log.clone()));
        let stderr_done = tokio::spawn(stderr_loop(stderr, stderr_tail.clone(), log.clone()));
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
    pub async fn request(&self, cmd: Value) -> Result<Value> {
        // The queue can only be cleared whole: what's kept goes back in, in
        // order (as text: a queued message's images don't come back).
        if self.kind == HarnessKind::Harlo && matches!(cmd["type"].as_str(), Some("remove_queued_message" | "promote_queued_message")) {
            let promote = cmd["type"] == "promote_queued_message";
            let text = cmd["message"].as_str().unwrap_or_default();
            let queue = if promote { "followUp" } else { cmd["queue"].as_str().unwrap_or("followUp") };
            let had = self.rpc(json!({ "type": "clear_queue" })).await?;
            let list = |key: &str| had[key].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>();
            let (mut steering, mut follow_up) = (list("steering"), list("followUp"));
            let from = if queue == "steering" { &mut steering } else { &mut follow_up };
            let taken = from.iter().position(|t| t == text).map(|at| from.remove(at));
            if promote && let Some(t) = &taken {
                steering.push(t.clone());
            }
            for t in &steering {
                self.rpc(json!({ "type": "steer", "message": t })).await?;
            }
            for t in &follow_up {
                self.rpc(json!({ "type": "follow_up", "message": t })).await?;
            }
            return match taken {
                Some(_) => Ok(Value::Null),
                None => Err(anyhow!("That message has been sent already.")),
            };
        }
        self.rpc(cmd).await
    }

    /// One command to the harness, and its answer.
    async fn rpc(&self, mut cmd: Value) -> Result<Value> {
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
    path.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
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
            let waiter = frame["id"].as_str().and_then(|id| pending.lock().unwrap().remove(id));
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
    pending.lock().unwrap().clear();
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
    stderr_done: tokio::task::JoinHandle<()>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
) {
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
    // What it said last (a child that kept stderr open isn't waited for).
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), stderr_done).await;
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
        fn put(_: &str, kind: HarnessKind) -> anyhow::Result<Option<String>> {
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

    #[tokio::test]
    async fn unanswered_requests_end_with_the_stop() {
        let dir = std::env::temp_dir().join(format!("harlo-silent-agent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("harlo-agent");
        std::fs::write(&program, "#!/bin/sh\nwhile read -r _; do :; done\n").unwrap();
        std::fs::set_permissions(&program, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let cfg = HarnessConfig { kind: HarnessKind::Harlo, program: Some(program), cwd: dir.clone(), session: SessionTarget::Ephemeral, extra_args: vec![], env: vec![], remote: None };
        let spawned = {
            let _dir = crate::protocol_log::TEST_DIR.lock().unwrap_or_else(|e| e.into_inner());
            Harness::spawn(cfg)
        };
        let (harness, _events) = spawned.unwrap();
        let wait = Duration::from_secs(5);
        let asked = harness.request(json!({ "type": "get_available_models" }));
        tokio::pin!(asked);
        assert!(tokio::time::timeout(Duration::from_millis(200), &mut asked).await.is_err(), "it never answers");
        harness.kill();
        let stopped = tokio::time::timeout(wait, asked).await.expect("ended by the stop").unwrap_err();
        assert_eq!(stopped.to_string(), "harness exited before responding");
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
