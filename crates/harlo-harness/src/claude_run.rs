//! A Claude Code session: `claude` in stream-json mode, driven like the
//! Agent SDK drives it, behind harlo's harness interface. harlo's commands (pi
//! RPC shaped: prompt, abort, get_state, set_model, …) come in as frames;
//! Claude's stream goes out as harlo events (messages, blocks, tools,
//! approvals and questions as UI requests).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::claude::{self, MODELS};
use crate::event::{BlockKind, HarnessEvent, HarnessKind, NoticeLevel};
use crate::process::{Harness, HarnessConfig, Outgoing, Pending, SessionTarget};
use crate::protocol_log::ProtocolLog;

/// One assistant message being streamed.
#[derive(Default)]
struct Streaming {
    id: String,
    /// Per block: its kind, text so far, and for a tool its id, name and
    /// argument JSON so far.
    blocks: Vec<(BlockKind, String, String, String)>,
    /// How many `assistant` frames (one per block) arrived for it.
    frames: usize,
    model: String,
}

/// A question or approval Claude is waiting on.
struct Waiting {
    request_id: String,
    tool_use_id: String,
    tool: String,
    input: Value,
    /// Rules Claude suggests remembering (its "always allow").
    suggestions: Value,
}

#[derive(Default)]
struct State {
    session_id: String,
    model: String,
    /// "" (Claude's default), "off", or an effort level.
    thinking: String,
    permission_mode: String,
    version: String,
    commands: Value,
    initialized: bool,
    /// Lines to send once `initialize` is answered.
    held: Vec<String>,
    running: bool,
    streaming: Option<Streaming>,
    /// UI request id → what it answers.
    waiting: HashMap<String, Waiting>,
    /// Our control requests waiting for their answer.
    control: HashMap<String, tokio::sync::oneshot::Sender<Value>>,
    next: u64,
    context_tokens: u64,
    context_window: u64,
    cost: f64,
    /// The conversation so far (pi's message shape), for `get_messages`.
    messages: Vec<Value>,
    /// Text the current turn showed (a `result` with none shows its own).
    turn_text: bool,
    /// Subagents, by the id of the Task / Agent call that started them (as
    /// harlo's dock shows them), and Claude's task ids → those call ids.
    subagents: HashMap<String, Value>,
    tasks: HashMap<String, String>,
    /// A name given before the transcript exists (written once it does).
    pending_title: Option<String>,
}

struct Shared {
    state: Mutex<State>,
    events: mpsc::UnboundedSender<HarnessEvent>,
    /// The running process's stdin (None between restarts).
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    child: tokio::sync::Mutex<Option<tokio::process::Child>>,
    log: Mutex<Option<Arc<ProtocolLog>>>,
    cfg: HarnessConfig,
    program: PathBuf,
    /// Which `claude` this is (a restart starts the next): a reader of an
    /// earlier one leaves the process slot alone.
    generation: std::sync::atomic::AtomicU64,
    /// On another machine: its Claude folder (`$CLAUDE_CONFIG_DIR` or
    /// `~/.claude`), as that machine sees it.
    remote_config: Option<String>,
}

/// The host's Claude folder (one quick SSH call).
fn remote_config_dir(r: &crate::process::RemoteLaunch) -> Result<String> {
    let out = std::process::Command::new("ssh")
        .args(&r.ssh_args)
        .arg(r#"printf '%s' "${CLAUDE_CONFIG_DIR:-$HOME/.claude}""#)
        .stdin(Stdio::null())
        .output()?;
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !out.status.success() || !dir.starts_with('/') {
        bail!("{}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("no answer"));
    }
    Ok(dir)
}

impl Shared {
    /// A session's transcript: here, or `ssh://host/…` on its machine.
    fn transcript(&self, id: &str) -> Option<PathBuf> {
        if id.is_empty() {
            return None;
        }
        match (&self.cfg.remote, &self.remote_config) {
            (Some(r), Some(dir)) => {
                let folder = claude::project_dir_name(Path::new(&crate::process::on_host(&self.cfg.cwd)));
                Some(PathBuf::from(format!("ssh://{}{dir}/projects/{folder}/{id}.jsonl", r.host)))
            }
            (Some(_), None) => None,
            (None, _) => claude::session_file(&self.cfg.cwd, id),
        }
    }

    /// Runs `script` on the session's machine (its output on success).
    fn on_host(&self, script: &str, input: Option<&str>) -> Option<Vec<u8>> {
        use std::io::Write;
        let r = self.cfg.remote.as_ref()?;
        let mut child = std::process::Command::new("ssh")
            .args(&r.ssh_args)
            .arg(script)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
            let _ = stdin.write_all(text.as_bytes());
        }
        let out = child.wait_with_output().ok()?;
        out.status.success().then_some(out.stdout)
    }

    /// A transcript's text (on its machine, over SSH).
    fn read_file(&self, path: &Path) -> Option<String> {
        if self.cfg.remote.is_none() {
            return std::fs::read_to_string(path).ok();
        }
        let q = crate::process::shell_quote(&crate::process::on_host(path));
        self.on_host(&format!("cat -- {q}"), None).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    /// Writes a new transcript.
    fn write_file(&self, path: &Path, text: &str) -> bool {
        if self.cfg.remote.is_none() {
            return std::fs::write(path, text).is_ok();
        }
        let q = crate::process::shell_quote(&crate::process::on_host(path));
        self.on_host(&format!("cat > {q}"), Some(text)).is_some()
    }

    /// Adds to a transcript that exists (false if it doesn't yet).
    fn append_file(&self, path: &Path, text: &str) -> bool {
        if self.cfg.remote.is_none() {
            use std::io::Write;
            return path.is_file() && std::fs::OpenOptions::new().append(true).open(path).and_then(|mut f| f.write_all(text.as_bytes())).is_ok();
        }
        let q = crate::process::shell_quote(&crate::process::on_host(path));
        self.on_host(&format!("[ -f {q} ] && cat >> {q}"), Some(text)).is_some()
    }
}

impl Shared {
    fn emit(&self, ev: HarnessEvent) {
        let _ = self.events.send(ev);
    }

    fn next_id(&self, prefix: &str) -> String {
        let mut st = self.state.lock().unwrap();
        st.next += 1;
        format!("{prefix}{}", st.next)
    }

    async fn write(&self, line: String) {
        if let Some(log) = self.log.lock().unwrap().clone() {
            log.frame("out", line.as_bytes());
        }
        let mut stdin = self.stdin.lock().await;
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(format!("{line}\n").as_bytes()).await;
            let _ = s.flush().await;
        }
    }

    /// A line for Claude, held until it has answered `initialize`.
    async fn send(&self, line: Value) {
        let line = line.to_string();
        {
            let mut st = self.state.lock().unwrap();
            if !st.initialized {
                st.held.push(line);
                return;
            }
        }
        self.write(line).await;
    }

    /// A control request; waits for its answer.
    async fn control(&self, request: Value) -> Value {
        let id = self.next_id("harlo_ctl_");
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.state.lock().unwrap().control.insert(id.clone(), tx);
        self.write(json!({ "type": "control_request", "request_id": id, "request": request }).to_string()).await;
        tokio::time::timeout(std::time::Duration::from_secs(30), rx).await.ok().and_then(Result::ok).unwrap_or(Value::Null)
    }

    async fn reply_control(&self, request_id: &str, response: Value) {
        self.write(json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } }).to_string()).await;
    }
}

/// `--model anthropic/claude-x` → `claude-x`; other arguments pass through.
fn split_args(extra: &[String]) -> (Option<String>, Vec<String>) {
    let mut model = None;
    let mut rest = Vec::new();
    let mut it = extra.iter();
    while let Some(a) = it.next() {
        if a == "--model" {
            model = it.next().map(|m| m.trim_start_matches("anthropic/").to_owned());
        } else if let Some(m) = a.strip_prefix("--model=") {
            model = Some(m.trim_start_matches("anthropic/").to_owned());
        } else {
            rest.push(a.clone());
        }
    }
    (model, rest)
}

pub(crate) fn spawn(cfg: HarnessConfig, program: PathBuf) -> Result<(Harness, mpsc::UnboundedReceiver<HarnessEvent>)> {
    // On another machine: where its Claude keeps sessions.
    let remote_config = match &cfg.remote {
        Some(r) => Some(remote_config_dir(r).context("asking the host where Claude keeps its sessions")?),
        None => None,
    };
    let (tx, mut rx) = mpsc::unbounded_channel::<Outgoing>();
    let (events, events_rx) = mpsc::unbounded_channel();
    let pending: Pending = Arc::default();
    let harness = Harness::from_parts(HarnessKind::Claude, tx, Arc::clone(&pending));
    let (model, _) = split_args(&cfg.extra_args);
    let session_id = match &cfg.session {
        SessionTarget::Resume(path) => path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        _ => String::new(),
    };
    let shared = Arc::new(Shared {
        state: Mutex::new(State { model: model.unwrap_or_default(), session_id, permission_mode: "default".into(), ..Default::default() }),
        events,
        stdin: tokio::sync::Mutex::new(None),
        child: tokio::sync::Mutex::new(None),
        log: Mutex::new(None),
        cfg,
        program,
        generation: Default::default(),
        remote_config,
    });
    // A resumed session: its conversation so far.
    if let SessionTarget::Resume(path) = &shared.cfg.session
        && let Some(text) = shared.read_file(path)
    {
        shared.state.lock().unwrap().messages = claude::history(&text);
    }
    {
        // (A new session: nothing else holds these yet.)
        let (mut stdin, mut child) = (shared.stdin.try_lock().expect("new"), shared.child.try_lock().expect("new"));
        start(&shared, &mut stdin, &mut child).context("starting claude")?;
    }

    let s = Arc::clone(&shared);
    tokio::spawn(async move {
        while let Some(out) = rx.recv().await {
            match out {
                Outgoing::Frame(cmd) => command(&s, &pending, cmd).await,
                Outgoing::Kill => {
                    stop(&s).await;
                    break;
                }
            }
        }
        stop(&s).await;
    });
    Ok((harness, events_rx))
}

/// Starts (or restarts) `claude` with the session's current settings, into
/// the process slots (held by the caller).
fn start(shared: &Arc<Shared>, stdin_slot: &mut Option<tokio::process::ChildStdin>, child_slot: &mut Option<tokio::process::Child>) -> Result<()> {
    let generation = shared.generation.load(std::sync::atomic::Ordering::SeqCst);
    let (model, extra) = split_args(&shared.cfg.extra_args);
    let st = shared.state.lock().unwrap();
    let mut args: Vec<String> = [
        "--output-format", "stream-json", "--verbose", "--input-format", "stream-json", "--include-partial-messages",
        "--permission-prompt-tool", "stdio", "--allow-dangerously-skip-permissions", "--setting-sources=user,project,local",
        "--thinking-display", "summarized",
    ]
    .map(str::to_owned)
    .to_vec();
    // harlo's tools (orchestration): its bridge here, or the one on the host.
    if let Some(bridge) = crate::mcp_bridge_for(shared.cfg.remote.as_ref()) {
        let session = shared.cfg.env.iter().find(|(k, _)| k == "HARLO_SESSION").map(|(_, v)| v.clone()).unwrap_or_default();
        args.push("--mcp-config".into());
        args.push(json!({ "mcpServers": { "harlo": { "type": "stdio", "command": bridge[0], "args": bridge[1..], "env": { "HARLO_SESSION": session } } } }).to_string());
        // harlo's own tools run without asking (the sessions they start ask
        // for what they do).
        args.push("--allowedTools".into());
        args.push("mcp__harlo".into());
    }
    args.extend(["--permission-mode".into(), st.permission_mode.clone()]);
    let model = if st.model.is_empty() { model.unwrap_or_default() } else { st.model.clone() };
    if !model.is_empty() {
        args.extend(["--model".into(), model]);
    }
    match st.thinking.as_str() {
        "" => {}
        "off" => args.extend(["--thinking".into(), "disabled".into()]),
        level => args.extend(["--thinking".into(), "adaptive".into(), "--effort".into(), level.to_owned()]),
    }
    if !st.session_id.is_empty() {
        args.push(format!("--resume={}", st.session_id));
    }
    args.extend(extra.into_iter().filter(|a| !a.starts_with("--config")));
    drop(st);

    // As the SDK runs it.
    let mut env: Vec<(String, String)> = vec![("CLAUDE_CODE_ENTRYPOINT".into(), "sdk-ts".into()), ("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS".into(), "1".into())];
    env.extend(shared.cfg.env.iter().cloned());
    let mut cmd = match &shared.cfg.remote {
        // On another machine: its PATH finds `claude`.
        Some(r) => {
            let mut cmd = tokio::process::Command::new("ssh");
            cmd.args(&r.ssh_args).arg(crate::process::remote_script(r, "claude", &args, &shared.cfg.cwd, &env));
            cmd
        }
        None => {
            let mut cmd = tokio::process::Command::new(&shared.program);
            cmd.args(&args).current_dir(&shared.cfg.cwd);
            // Not as a child of another Claude Code session.
            for k in ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SSE_PORT", "CLAUDE_AGENT_SDK_VERSION"] {
                cmd.env_remove(k);
            }
            cmd.envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            cmd
        }
    };
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).process_group(0);
    let mut child = cmd.spawn().with_context(|| format!("starting {}", shared.program.display()))?;
    tracing::info!(program = %shared.program.display(), cwd = %shared.cfg.cwd.display(), "spawned claude");
    *shared.log.lock().unwrap() = ProtocolLog::open(HarnessKind::Claude, &shared.program, &args, &shared.cfg.cwd);
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    *stdin_slot = child.stdin.take();
    *child_slot = Some(child);
    shared.state.lock().unwrap().initialized = false;

    let s = Arc::clone(shared);
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(log) = s.log.lock().unwrap().clone() {
                log.stderr(&line);
            }
            tail.push(line);
            if tail.len() > 40 {
                tail.remove(0);
            }
        }
    });
    let s = Arc::clone(shared);
    tokio::spawn(async move {
        let mut lines = BufReader::with_capacity(256 * 1024, stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(log) = s.log.lock().unwrap().clone() {
                log.frame("in", line.as_bytes());
            }
            if let Ok(frame) = serde_json::from_str::<Value>(&line) {
                incoming(&s, frame).await;
            }
        }
        // Restarted: the slot is the next one's.
        if s.generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
            return;
        }
        // Gone: a turn it was in has ended.
        let mut child = s.child.lock().await;
        if s.generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
            return;
        }
        let code = match child.as_mut() {
            Some(c) => c.wait().await.ok().and_then(|st| st.code()),
            None => return, // stopped on purpose (a restart)
        };
        *child = None;
        let was_running = std::mem::take(&mut s.state.lock().unwrap().running);
        if was_running {
            s.emit(HarnessEvent::RunEnded);
            s.emit(HarnessEvent::Settled);
        }
        s.emit(HarnessEvent::Exited { code, stderr_tail: String::new() });
    });

    // The handshake: commands, models, account.
    let s = Arc::clone(shared);
    tokio::spawn(async move {
        let id = s.next_id("harlo_init_");
        let (tx, rx) = tokio::sync::oneshot::channel();
        s.state.lock().unwrap().control.insert(id.clone(), tx);
        s.write(json!({ "type": "control_request", "request_id": id, "request": { "subtype": "initialize" } }).to_string()).await;
        let answer = rx.await.unwrap_or(Value::Null);
        let held = {
            let mut st = s.state.lock().unwrap();
            st.initialized = true;
            st.version = answer["claude_code_version"].as_str().unwrap_or_default().to_owned();
            st.commands = Value::Array(
                answer["commands"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| {
                        let name = c["name"].as_str()?;
                        Some(json!({ "name": name, "description": c["description"], "input": { "hint": c["argumentHint"] } }))
                    })
                    .collect(),
            );
            std::mem::take(&mut st.held)
        };
        for line in held {
            s.write(line).await;
        }
        s.emit(HarnessEvent::CommandsChanged(s.state.lock().unwrap().commands.clone()));
        s.emit(HarnessEvent::Ready);
    });
    Ok(())
}

async fn stop(shared: &Shared) {
    shared.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    *shared.stdin.lock().await = None;
    if let Some(mut child) = shared.child.lock().await.take() {
        if let Some(pid) = child.id() {
            // SAFETY: plain syscall on the group the child leads (its MCP
            // servers too).
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
        let _ = child.kill().await;
    }
}

/// Restarts `claude` resuming the session (thinking changes need it).
async fn restart(shared: &Arc<Shared>) {
    stop(shared).await;
    let (mut stdin, mut child) = (shared.stdin.lock().await, shared.child.lock().await);
    if let Err(e) = start(shared, &mut stdin, &mut child) {
        shared.emit(HarnessEvent::Notice { level: NoticeLevel::Error, text: format!("Couldn't restart Claude: {e:#}"), system: false });
    }
}

fn current_model(st: &State) -> Value {
    let id = st.model.clone();
    let name = MODELS.iter().find(|m| m.id == id).map(|m| m.label.to_owned()).unwrap_or_else(|| id.clone());
    if id.is_empty() { Value::Null } else { json!({ "provider": "anthropic", "id": id, "name": name }) }
}

fn thinking_levels(model: &str) -> Vec<&'static str> {
    let m = MODELS.iter().find(|m| m.id == model);
    let mut levels = Vec::new();
    if m.is_none_or(|m| m.thinking_off) {
        levels.push("off");
    }
    levels.extend(["low", "medium", "high"]);
    if m.is_none_or(|m| m.xhigh) {
        levels.push("xhigh");
    }
    levels.push("max");
    levels
}

/// harlo's commands.
async fn command(shared: &Arc<Shared>, pending: &Pending, cmd: Value) {
    let id = cmd["id"].as_str().map(str::to_owned);
    let ty = cmd["type"].as_str().unwrap_or_default().to_owned();
    let reply = |ok: bool, data: Value, error: &str| {
        if let Some(w) = id.as_deref().and_then(|id| pending.lock().unwrap().remove(id)) {
            let _ = w.send(json!({ "type": "response", "id": id, "command": ty, "success": ok, "data": data, "error": error }));
        }
    };
    match ty.as_str() {
        "prompt" => {
            let text = cmd["message"].as_str().unwrap_or_default().to_owned();
            let mut content = vec![];
            for img in cmd["images"].as_array().into_iter().flatten() {
                content.push(json!({ "type": "image", "source": { "type": "base64", "media_type": img["mimeType"], "data": img["data"] } }));
            }
            content.push(json!({ "type": "text", "text": text }));
            let uuid = new_uuid();
            let mut line = json!({ "type": "user", "message": { "role": "user", "content": content }, "parent_tool_use_id": null, "uuid": uuid,
                                   "session_id": shared.state.lock().unwrap().session_id });
            if cmd["streamingBehavior"] == "steer" {
                line["priority"] = json!("next");
            }
            let message = json!({ "role": "user", "content": content.iter().map(to_pi_block).collect::<Vec<_>>() });
            shared.state.lock().unwrap().messages.push(message.clone());
            shared.emit(HarnessEvent::MessageEnd { id: format!("u{uuid}"), message });
            shared.send(line).await;
            reply(true, json!({ "disposition": "started" }), "");
        }
        "abort" => {
            let running = shared.state.lock().unwrap().running;
            if running {
                shared.control(json!({ "subtype": "interrupt" })).await;
                // Stop ends the subagents the turn waits for (not background ones).
                let stopped: Vec<String> = shared.state.lock().unwrap().subagents.iter().filter(|(_, e)| e["status"] == "running" && e["detached"] != true).map(|(k, _)| k.clone()).collect();
                for call in stopped {
                    subagent_update(shared, &call, json!({ "status": "aborted" }));
                }
            }
            reply(true, Value::Null, "");
        }
        "get_state" => {
            let st = shared.state.lock().unwrap();
            let file = shared.transcript(&st.session_id);
            reply(
                true,
                json!({ "model": current_model(&st), "thinkingLevel": st.thinking, "sessionId": st.session_id,
                        "sessionFile": file.map(|f| f.to_string_lossy().into_owned()), "isStreaming": st.running,
                        "permissionMode": st.permission_mode }),
                "",
            );
        }
        "get_session_stats" => {
            let st = shared.state.lock().unwrap();
            reply(true, json!({ "contextUsage": { "tokens": st.context_tokens, "contextWindow": st.context_window }, "cost": st.cost }), "");
        }
        "get_available_models" => {
            let version = shared.state.lock().unwrap().version.clone();
            let list: Vec<Value> = claude::models((!version.is_empty()).then_some(version.as_str()), claude::config_dir().map(|d| d.join("settings.json")).as_deref())
                .into_iter()
                .map(|(id, name)| json!({ "provider": "anthropic", "id": id, "name": name, "reasoning": true }))
                .collect();
            reply(true, json!({ "models": list }), "");
        }
        "set_model" => {
            let model = cmd["modelId"].as_str().unwrap_or_default().to_owned();
            shared.state.lock().unwrap().model = model.clone();
            let running = shared.child.lock().await.is_some();
            if running {
                shared.control(json!({ "subtype": "set_model", "model": model })).await;
            }
            let m = current_model(&shared.state.lock().unwrap());
            shared.emit(HarnessEvent::ModelChanged(m.clone()));
            reply(true, m, "");
        }
        "get_available_thinking_levels" => {
            let model = shared.state.lock().unwrap().model.clone();
            reply(true, json!({ "levels": thinking_levels(&model) }), "");
        }
        "set_thinking_level" => {
            let level = cmd["level"].as_str().unwrap_or_default().to_owned();
            let busy = {
                let mut st = shared.state.lock().unwrap();
                st.thinking = level.clone();
                st.running
            };
            reply(true, Value::Null, "");
            shared.emit(HarnessEvent::ThinkingLevel(level));
            // Effort is a launch option: restart (resuming) when idle;
            // mid-run it applies from the next restart.
            if !busy {
                restart(shared).await;
            }
        }
        "harlo_set_modes" => {
            let mode = claude_mode(cmd["approval"].as_str().unwrap_or_default(), cmd["plan"].as_bool().unwrap_or(false));
            shared.state.lock().unwrap().permission_mode = mode.to_owned();
            shared.control(json!({ "subtype": "set_permission_mode", "mode": mode })).await;
            reply(true, Value::Null, "");
        }
        "get_commands" | "get_available_commands" => {
            let commands = shared.state.lock().unwrap().commands.clone();
            reply(true, json!({ "commands": commands }), "");
        }
        "get_messages" => {
            let messages = shared.state.lock().unwrap().messages.clone();
            reply(true, json!({ "messages": messages }), "");
        }
        "compact" => {
            let uuid = new_uuid();
            let session = shared.state.lock().unwrap().session_id.clone();
            shared.send(json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "text", "text": "/compact" }] }, "parent_tool_use_id": null, "uuid": uuid, "session_id": session })).await;
            reply(true, Value::Null, "");
        }
        "extension_ui_response" => answer(shared, &cmd).await,
        // The dock's list comes from the subagents' events.
        "get_subagents" => reply(true, json!({ "subagents": [] }), ""),
        // A name of your own: Claude Code's `custom-title` line.
        "set_session_name" => {
            let name = cmd["name"].as_str().unwrap_or_default().trim().to_owned();
            shared.state.lock().unwrap().pending_title = Some(name.clone());
            write_title(shared);
            shared.emit(HarnessEvent::Renamed((!name.is_empty()).then_some(name)));
            reply(true, Value::Null, "");
        }
        // Edit & resend: a new session with the conversation before that
        // prompt (the original stays), and the prompt back for editing.
        "fork" => {
            let entry = cmd["entryId"].as_str().unwrap_or_default().to_owned();
            let (running, file) = {
                let st = shared.state.lock().unwrap();
                (st.running, shared.transcript(&st.session_id))
            };
            if running {
                return reply(false, Value::Null, "Stop Claude first.");
            }
            // (Beside the original, on its machine.)
            let forked = file.as_ref().and_then(|f| shared.read_file(f)).map(|t| claude::fork_text(&t, &entry).ok_or("that message isn't in the session"));
            let forked = match forked {
                Some(Ok((id, out, text))) => match file.as_ref().map(|f| f.with_file_name(format!("{id}.jsonl"))) {
                    Some(new) if shared.write_file(&new, &out) => Some(Ok((id, text, out))),
                    _ => Some(Err("couldn't write the new session")),
                },
                Some(Err(e)) => Some(Err(e)),
                None => None,
            };
            match forked {
                Some(Ok((id, text, out))) => {
                    let messages = claude::history(&out);
                    {
                        let mut st = shared.state.lock().unwrap();
                        st.session_id = id;
                        st.messages = messages;
                        st.subagents.clear();
                        st.tasks.clear();
                    }
                    // Answered first: the history is here already; Claude
                    // resumes the fork meanwhile.
                    reply(true, json!({ "text": text, "cancelled": false }), "");
                    restart(shared).await;
                }
                Some(Err(e)) => reply(false, Value::Null, &format!("Couldn't fork: {e}")),
                None => reply(false, Value::Null, "This session has no transcript yet."),
            }
        }
        _ => reply(false, Value::Null, &format!("Claude sessions don't support {ty}")),
    }
}

/// harlo's permission shield and Plan → Claude's permission mode.
pub fn claude_mode(approval: &str, plan: bool) -> &'static str {
    if plan {
        return "plan";
    }
    match approval {
        "write" => "acceptEdits",
        "auto" | "yolo" => "bypassPermissions",
        _ => "default",
    }
}

fn new_uuid() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// Claude's content block → pi's (as harlo's transcript reads them).
pub(crate) fn to_pi_block(b: &Value) -> Value {
    match b["type"].as_str() {
        Some("text") => json!({ "type": "text", "text": b["text"] }),
        Some("thinking") => json!({ "type": "thinking", "thinking": b["thinking"] }),
        Some("tool_use") | Some("server_tool_use") | Some("mcp_tool_use") => json!({ "type": "toolCall", "id": b["id"], "name": b["name"], "arguments": b["input"] }),
        Some("image") => json!({ "type": "image", "data": b["source"]["data"], "mimeType": b["source"]["media_type"] }),
        _ => b.clone(),
    }
}

/// A tool result's text (and a note for images).
pub(crate) fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match p["type"].as_str() {
                Some("text") => p["text"].as_str().unwrap_or_default().to_owned(),
                Some("image") => "[image]".to_owned(),
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Claude's frames.
async fn incoming(shared: &Arc<Shared>, frame: Value) {
    // Inside a subagent: not part of the main conversation; what it's
    // doing shows on its entry.
    if let Some(parent) = frame["parent_tool_use_id"].as_str() {
        subagent_frame(shared, parent, &frame);
        return;
    }
    match frame["type"].as_str().unwrap_or_default() {
        "control_response" => {
            let r = &frame["response"];
            let id = r["request_id"].as_str().unwrap_or_default();
            if let Some(w) = shared.state.lock().unwrap().control.remove(id) {
                let _ = w.send(r["response"].clone());
            }
        }
        "control_request" => {
            let request_id = frame["request_id"].as_str().unwrap_or_default().to_owned();
            let req = &frame["request"];
            if req["subtype"] == "can_use_tool" {
                ask(shared, request_id, req);
            } else {
                shared.reply_control(&request_id, json!({})).await;
            }
        }
        "control_cancel_request" => {
            let request_id = frame["request_id"].as_str().unwrap_or_default();
            let ui = {
                let mut st = shared.state.lock().unwrap();
                let key = st.waiting.iter().find(|(_, w)| w.request_id == request_id).map(|(k, _)| k.clone());
                key.inspect(|k| {
                    st.waiting.remove(k);
                })
            };
            if let Some(target_id) = ui {
                shared.emit(HarnessEvent::UiCancel { target_id });
            }
        }
        "system" => system(shared, &frame),
        "stream_event" => stream(shared, &frame["event"]),
        "assistant" => {
            assistant(shared, &frame["message"]);
            subagents_started(shared, &frame["message"]);
        }
        "user" => {
            // Tool results.
            for b in frame["message"]["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "tool_result") {
                let call_id = b["tool_use_id"].as_str().unwrap_or_default().to_owned();
                let text = result_text(&b["content"]);
                let is_error = b["is_error"].as_bool().unwrap_or(false);
                shared.state.lock().unwrap().messages.push(json!({
                    "role": "toolResult", "toolCallId": call_id, "content": [{ "type": "text", "text": text }], "isError": is_error
                }));
                subagent_result(shared, &call_id, &text, is_error);
                shared.emit(HarnessEvent::ToolEnd { call_id, result: json!({ "content": [{ "type": "text", "text": text }] }), is_error });
            }
        }
        "result" => {
            let (show, ended) = {
                let mut st = shared.state.lock().unwrap();
                st.cost = frame["total_cost_usd"].as_f64().unwrap_or(st.cost);
                if let Some(window) = frame["modelUsage"].as_object().and_then(|m| m.values().find_map(|u| u["contextWindow"].as_u64())) {
                    st.context_window = window;
                }
                let show = !st.turn_text;
                st.turn_text = false;
                (show, std::mem::take(&mut st.running))
            };
            let text = frame["result"].as_str().unwrap_or_default();
            // Built-in commands (/usage, …) answer only here.
            if show && !text.is_empty() && frame["subtype"] == "success" {
                let message = json!({ "role": "assistant", "content": [{ "type": "text", "text": text }] });
                shared.state.lock().unwrap().messages.push(message.clone());
                shared.emit(HarnessEvent::MessageEnd { id: shared.next_id("r"), message });
            }
            if frame["subtype"] != "success" || frame["is_error"] == true {
                let errors: Vec<&str> = frame["errors"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                let text = if errors.is_empty() { text.to_owned() } else { errors.join("\n") };
                if !text.is_empty() {
                    shared.emit(HarnessEvent::Notice { level: NoticeLevel::Error, text, system: false });
                }
            }
            // A name given before the transcript existed.
            write_title(shared);
            if ended {
                shared.emit(HarnessEvent::RunEnded);
                shared.emit(HarnessEvent::Settled);
            }
        }
        _ => {}
    }
}

fn system(shared: &Arc<Shared>, frame: &Value) {
    match frame["subtype"].as_str().unwrap_or_default() {
        "init" => {
            let started = {
                let mut st = shared.state.lock().unwrap();
                if let Some(id) = frame["session_id"].as_str() {
                    st.session_id = id.to_owned();
                }
                if let Some(m) = frame["model"].as_str() {
                    st.model = m.to_owned();
                }
                if let Some(m) = frame["permissionMode"].as_str() {
                    st.permission_mode = m.to_owned();
                }
                !std::mem::replace(&mut st.running, true)
            };
            if started {
                shared.emit(HarnessEvent::RunStarted);
            }
        }
        "status" => {
            if frame["status"] == "compacting" {
                shared.emit(HarnessEvent::Compaction { started: true, reason: None, error: None });
            }
            if let Some(m) = frame["permissionMode"].as_str() {
                shared.state.lock().unwrap().permission_mode = m.to_owned();
            }
        }
        "compact_boundary" => shared.emit(HarnessEvent::Compaction { started: false, reason: None, error: None }),
        "task_started" | "task_updated" | "task_progress" | "task_notification" => task_frame(shared, frame),
        _ => {}
    }
}

fn stream(shared: &Arc<Shared>, ev: &Value) {
    let mut out = Vec::new();
    {
        let mut st = shared.state.lock().unwrap();
        match ev["type"].as_str().unwrap_or_default() {
            "message_start" => {
                let m = &ev["message"];
                let id = m["id"].as_str().unwrap_or_default().to_owned();
                let u = &m["usage"];
                st.context_tokens = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"].iter().filter_map(|k| u[*k].as_u64()).sum();
                st.streaming = Some(Streaming { id: id.clone(), model: m["model"].as_str().unwrap_or_default().to_owned(), ..Default::default() });
                out.push(HarnessEvent::MessageStart { id, message: json!({ "role": "assistant", "content": [] }) });
            }
            "content_block_start" => {
                let index = ev["index"].as_u64().unwrap_or(0) as usize;
                let b = &ev["content_block"];
                let kind = match b["type"].as_str() {
                    Some("thinking") | Some("redacted_thinking") => BlockKind::Thinking,
                    Some("tool_use") | Some("server_tool_use") | Some("mcp_tool_use") => BlockKind::ToolCall,
                    _ => BlockKind::Text,
                };
                if let Some(s) = st.streaming.as_mut() {
                    while s.blocks.len() <= index {
                        s.blocks.push((BlockKind::Text, String::new(), String::new(), String::new()));
                    }
                    let (call_id, name) = (b["id"].as_str().unwrap_or_default().to_owned(), b["name"].as_str().unwrap_or_default().to_owned());
                    s.blocks[index] = (kind, String::new(), call_id.clone(), name.clone());
                    out.push(HarnessEvent::BlockStart { id: s.id.clone(), index, kind, tool: (kind == BlockKind::ToolCall).then_some((call_id, name)) });
                }
            }
            "content_block_delta" => {
                let index = ev["index"].as_u64().unwrap_or(0) as usize;
                let d = &ev["delta"];
                if let Some(s) = st.streaming.as_mut()
                    && let Some(block) = s.blocks.get_mut(index)
                {
                    let (kind, text) = match d["type"].as_str() {
                        Some("text_delta") => (BlockKind::Text, d["text"].as_str().unwrap_or_default()),
                        Some("thinking_delta") => (BlockKind::Thinking, d["thinking"].as_str().unwrap_or_default()),
                        Some("input_json_delta") => {
                            block.1.push_str(d["partial_json"].as_str().unwrap_or_default());
                            (BlockKind::ToolCall, "")
                        }
                        _ => (BlockKind::Text, ""),
                    };
                    if !text.is_empty() && kind != BlockKind::ToolCall {
                        block.1.push_str(text);
                        out.push(HarnessEvent::BlockDelta { id: s.id.clone(), index, kind, delta: text.to_owned() });
                    }
                }
            }
            "content_block_stop" => {
                let index = ev["index"].as_u64().unwrap_or(0) as usize;
                if let Some(s) = st.streaming.as_ref()
                    && let Some((kind, text, call_id, name)) = s.blocks.get(index)
                {
                    let content = match kind {
                        BlockKind::ToolCall => {
                            let args: Value = serde_json::from_str(text).unwrap_or(json!({}));
                            json!({ "type": "toolCall", "id": call_id, "name": name, "arguments": args })
                        }
                        _ => json!(text),
                    };
                    out.push(HarnessEvent::BlockEnd { id: s.id.clone(), index, kind: *kind, content: content.clone() });
                    if *kind == BlockKind::ToolCall {
                        out.push(HarnessEvent::ToolStart { call_id: call_id.clone(), name: name.clone(), args: content["arguments"].clone() });
                    } else if *kind == BlockKind::Text && !text.is_empty() {
                        st.turn_text = true;
                    }
                }
            }
            "message_stop" => {
                if let Some(s) = st.streaming.take() {
                    let content: Vec<Value> = s
                        .blocks
                        .iter()
                        .map(|(kind, text, call_id, name)| match kind {
                            BlockKind::Text => json!({ "type": "text", "text": text }),
                            BlockKind::Thinking => json!({ "type": "thinking", "thinking": text }),
                            BlockKind::ToolCall => json!({ "type": "toolCall", "id": call_id, "name": name, "arguments": serde_json::from_str::<Value>(text).unwrap_or(json!({})) }),
                        })
                        .collect();
                    let message = json!({ "role": "assistant", "content": content, "model": s.model });
                    st.messages.push(message.clone());
                    out.push(HarnessEvent::MessageEnd { id: s.id, message });
                }
            }
            _ => {}
        }
    }
    for ev in out {
        shared.emit(ev);
    }
}

/// A whole assistant message (one per block, after its deltas). Fills in
/// what the stream left empty (thinking shown only as a summary), and
/// carries messages that weren't streamed (e.g. a usage-limit notice).
fn assistant(shared: &Arc<Shared>, message: &Value) {
    let id = message["id"].as_str().unwrap_or_default().to_owned();
    let blocks = message["content"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    {
        let mut st = shared.state.lock().unwrap();
        match st.streaming.as_mut().filter(|s| s.id == id) {
            Some(s) => {
                let index = s.frames;
                s.frames += 1;
                if let (Some(b), Some(slot)) = (blocks.first(), s.blocks.get_mut(index)) {
                    let full = b["thinking"].as_str().or_else(|| b["text"].as_str()).unwrap_or_default();
                    if slot.1.is_empty() && !full.is_empty() && slot.0 != BlockKind::ToolCall {
                        slot.1 = full.to_owned();
                        out.push(HarnessEvent::BlockDelta { id: id.clone(), index, kind: slot.0, delta: full.to_owned() });
                    }
                }
            }
            None => {
                let content: Vec<Value> = blocks.iter().map(to_pi_block).collect();
                let message = json!({ "role": "assistant", "content": content, "model": message["model"] });
                if blocks.iter().any(|b| b["type"] == "text") {
                    st.turn_text = true;
                }
                st.messages.push(message.clone());
                out.push(HarnessEvent::MessageEnd { id: if id.is_empty() { format!("a{}", st.next) } else { id }, message });
            }
        }
    }
    for ev in out {
        shared.emit(ev);
    }
}

/// Writes a name you gave as Claude Code does (`custom-title`), once the
/// transcript exists.
fn write_title(shared: &Arc<Shared>) {
    let (title, file, id) = {
        let st = shared.state.lock().unwrap();
        (st.pending_title.clone(), shared.transcript(&st.session_id), st.session_id.clone())
    };
    let (Some(title), Some(file)) = (title, file) else { return };
    let line = json!({ "type": "custom-title", "customTitle": title, "sessionId": id }).to_string() + "\n";
    if shared.append_file(&file, &line) {
        shared.state.lock().unwrap().pending_title = None;
    }
}

// ---- Subagents (Claude's Task / Agent tool) ---------------------------------
// One entry per subagent, keyed by the call that started it, in the shape
// harlo's dock folds (omp's `subagent_lifecycle`). Claude announces them
// with `system/task_*` frames (2.1.2xx); the call and its result cover
// older versions and fill in. Claude can't stop one subagent on its own.

const SUBAGENT_TOOLS: [&str; 2] = ["Task", "Agent"];

/// Merges `patch` into the subagent's entry and tells the dock.
fn subagent_update(shared: &Arc<Shared>, call: &str, patch: Value) {
    let entry = {
        let mut st = shared.state.lock().unwrap();
        let e = st.subagents.entry(call.to_owned()).or_insert_with(|| json!({ "id": call, "parentToolCallId": call, "canCancel": false, "canSteer": false }));
        if let (Some(dst), Some(src)) = (e.as_object_mut(), patch.as_object()) {
            for (k, v) in src {
                dst.insert(k.clone(), v.clone());
            }
        }
        e.clone()
    };
    shared.emit(HarnessEvent::SubagentActivity(json!({ "type": "subagent_lifecycle", "payload": entry })));
}

/// Task / Agent calls in Claude's message: subagents starting.
fn subagents_started(shared: &Arc<Shared>, message: &Value) {
    for b in message["content"].as_array().into_iter().flatten() {
        if b["type"] != "tool_use" || !SUBAGENT_TOOLS.contains(&b["name"].as_str().unwrap_or_default()) {
            continue;
        }
        let (Some(call), input) = (b["id"].as_str(), &b["input"]) else { continue };
        if shared.state.lock().unwrap().subagents.contains_key(call) {
            continue;
        }
        let name = input["name"].as_str().or_else(|| input["description"].as_str()).unwrap_or("Subagent");
        subagent_update(shared, call, json!({
            "name": name, "agent": input["subagent_type"].as_str().unwrap_or("general-purpose"), "task": input["prompt"],
            "status": "running", "detached": input["run_in_background"] == true,
        }));
    }
}

/// `system/task_*`: a subagent announced, its state, its progress.
fn task_frame(shared: &Arc<Shared>, frame: &Value) {
    let task = frame["task_id"].as_str().unwrap_or_default();
    let call = match frame["subtype"].as_str() {
        Some("task_started") => {
            // Subagents only (not background shells), with a transcript.
            let kind = frame["task_type"].as_str();
            let agent = matches!(kind, Some("local_agent") | Some("local_workflow")) || (kind.is_none() && frame["subagent_type"].as_str().is_some_and(|t| !t.is_empty()));
            let Some(call) = frame["tool_use_id"].as_str().filter(|_| agent && frame["skip_transcript"] != true) else { return };
            shared.state.lock().unwrap().tasks.insert(task.to_owned(), call.to_owned());
            let file = {
                let st = shared.state.lock().unwrap();
                shared.transcript(&st.session_id).map(|f| f.with_extension("").join("subagents").join(format!("agent-{task}.jsonl")))
            };
            let mut patch = json!({ "status": "running", "detached": frame["is_backgrounded"] == true });
            for (key, from) in [("agent", "subagent_type"), ("task", "prompt")] {
                if frame[from].as_str().is_some_and(|v| !v.is_empty()) {
                    patch[key] = frame[from].clone();
                }
            }
            if let Some(d) = frame["description"].as_str().filter(|d| !d.is_empty()) {
                patch["name"] = json!(d);
            }
            if let Some(f) = file {
                patch["sessionFile"] = json!(f.to_string_lossy());
            }
            subagent_update(shared, call, patch);
            return;
        }
        _ => match shared.state.lock().unwrap().tasks.get(task) {
            Some(c) => c.clone(),
            None => return,
        },
    };
    let mut patch = json!({});
    let status = frame["patch"]["status"].as_str().or_else(|| frame["status"].as_str());
    if let Some(s) = status.and_then(task_status) {
        patch["status"] = json!(s);
    }
    if frame["patch"]["is_backgrounded"] == true {
        patch["detached"] = json!(true);
    }
    if let Some(summary) = frame["summary"].as_str() {
        patch["result"] = json!(summary);
    }
    if frame["subtype"] == "task_progress" {
        let u = &frame["usage"];
        let mut progress = shared.state.lock().unwrap().subagents.get(&call).map(|e| e["progress"].clone()).filter(Value::is_object).unwrap_or_else(|| json!({}));
        progress["toolCount"] = u["tool_uses"].clone();
        progress["tokens"] = u["total_tokens"].clone();
        if let Some(tool) = frame["last_tool_name"].as_str()
            && progress["currentTool"] != tool
        {
            progress["currentTool"] = json!(tool);
            progress["currentToolArgs"] = Value::Null;
        }
        patch["progress"] = progress;
    }
    subagent_update(shared, &call, patch);
}

/// Claude's task states → the dock's.
fn task_status(s: &str) -> Option<&'static str> {
    match s {
        "pending" | "running" | "paused" => Some("running"),
        "completed" => Some("completed"),
        "failed" => Some("failed"),
        "killed" | "stopped" => Some("aborted"),
        _ => None,
    }
}

/// A frame from inside a subagent: the tool it's running now.
fn subagent_frame(shared: &Arc<Shared>, parent: &str, frame: &Value) {
    if frame["type"] != "assistant" || !shared.state.lock().unwrap().subagents.contains_key(parent) {
        return;
    }
    let Some(tool) = frame["message"]["content"].as_array().into_iter().flatten().rev().find(|b| b["type"] == "tool_use") else { return };
    let mut progress = shared.state.lock().unwrap().subagents.get(parent).map(|e| e["progress"].clone()).filter(Value::is_object).unwrap_or_else(|| json!({}));
    progress["currentTool"] = tool["name"].clone();
    progress["currentToolArgs"] = tool["input"].clone();
    subagent_update(shared, parent, json!({ "progress": progress }));
}

/// The Task / Agent call's result: a subagent the turn waited for is done
/// (a background one has only started).
fn subagent_result(shared: &Arc<Shared>, call: &str, text: &str, is_error: bool) {
    let known = shared.state.lock().unwrap().subagents.get(call).map(|e| e["status"] == "running");
    match known {
        Some(_) if text.starts_with("Async agent launched") => subagent_update(shared, call, json!({ "detached": true })),
        Some(true) => subagent_update(shared, call, json!({ "status": if is_error { "failed" } else { "completed" } })),
        _ => {}
    }
}

/// What Claude's suggested permission updates would remember, in words
/// (None: nothing to remember).
fn suggestions_text(suggestions: &Value) -> Option<String> {
    let parts: Vec<String> = suggestions
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| match s["type"].as_str()? {
            "addRules" | "replaceRules" => {
                let rules: Vec<String> = s["rules"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| {
                        let tool = r["toolName"].as_str()?;
                        Some(r["ruleContent"].as_str().map_or(tool.to_owned(), |c| format!("{tool}({c})")))
                    })
                    .collect();
                (!rules.is_empty()).then(|| rules.join(", "))
            }
            "setMode" => Some(format!("{} mode", s["mode"].as_str().unwrap_or("another"))),
            "addDirectories" => Some(format!("access to {}", s["directories"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))),
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| {
        let session = suggestions.as_array().into_iter().flatten().all(|s| s["destination"] == "session");
        format!("{}{}", parts.join("; "), if session { " (for this session)" } else { "" })
    })
}

/// An approval or a question from Claude: a UI request for harlo.
fn ask(shared: &Arc<Shared>, request_id: String, req: &Value) {
    let tool = req["tool_name"].as_str().unwrap_or_default().to_owned();
    let input = req["input"].clone();
    let ui_id = shared.next_id("claude_ui_");
    let request = match tool.as_str() {
        "AskUserQuestion" => json!({
            "type": "extension_ui_request", "id": ui_id, "method": "ask",
            "questions": input["questions"].as_array().into_iter().flatten().enumerate().map(|(i, q)| json!({
                "id": format!("q{i}"), "header": q["header"], "question": q["question"], "multi": q["multiSelect"].as_bool().unwrap_or(false),
                "options": q["options"].as_array().into_iter().flatten().map(|o| json!({ "label": o["label"], "description": o["description"] })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
        "ExitPlanMode" => json!({
            "type": "extension_ui_request", "id": ui_id, "method": "select", "title": "Claude's plan is ready",
            "message": input["plan"].as_str().unwrap_or_default(), "options": ["Start with this plan", "Keep planning"],
        }),
        _ => {
            let detail = ["command", "file_path", "path", "url", "pattern", "query"]
                .iter()
                .find_map(|k| input[*k].as_str())
                .or_else(|| req["description"].as_str())
                .unwrap_or_default();
            let mut r = json!({
                "type": "extension_ui_request", "id": ui_id, "method": "select",
                "title": format!("Allow tool: {}", req["display_name"].as_str().unwrap_or(&tool)), "message": detail,
                "options": ["Approve", "Deny"],
            });
            // Rules Claude would remember: "Always allow".
            if let Some(hint) = suggestions_text(&req["permission_suggestions"]) {
                r["options"] = json!(["Approve", "Deny", "Always allow"]);
                r["alwaysHint"] = json!(hint);
            }
            r
        }
    };
    let suggestions = req["permission_suggestions"].clone();
    shared.state.lock().unwrap().waiting.insert(ui_id, Waiting { request_id, tool_use_id: req["tool_use_id"].as_str().unwrap_or_default().to_owned(), tool, input, suggestions });
    shared.emit(HarnessEvent::UiRequest(request));
}

/// harlo's answer to an approval or a question.
async fn answer(shared: &Arc<Shared>, cmd: &Value) {
    let ui_id = cmd["id"].as_str().unwrap_or_default();
    let Some(w) = shared.state.lock().unwrap().waiting.remove(ui_id) else { return };
    let deny = |message: &str| json!({ "behavior": "deny", "message": message, "interrupt": false, "toolUseID": w.tool_use_id });
    let response = if cmd["cancelled"] == true {
        deny("The user dismissed this.")
    } else {
        match w.tool.as_str() {
            "AskUserQuestion" => {
                let questions = w.input["questions"].as_array().cloned().unwrap_or_default();
                let mut answers = serde_json::Map::new();
                for a in cmd["answers"].as_array().into_iter().flatten() {
                    let i: usize = a["id"].as_str().and_then(|id| id.strip_prefix('q')).and_then(|n| n.parse().ok()).unwrap_or(0);
                    let Some(q) = questions.get(i) else { continue };
                    let mut picked: Vec<String> = a["selectedOptions"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_owned)).collect();
                    if let Some(custom) = a["customInput"].as_str().filter(|c| !c.trim().is_empty()) {
                        picked.push(custom.to_owned());
                    }
                    answers.insert(q["question"].as_str().unwrap_or_default().to_owned(), json!(picked.join(", ")));
                }
                let mut input = w.input.clone();
                input["answers"] = Value::Object(answers);
                json!({ "behavior": "allow", "updatedInput": input, "toolUseID": w.tool_use_id })
            }
            "ExitPlanMode" => {
                if cmd["value"] == "Start with this plan" {
                    let mode = "default";
                    shared.state.lock().unwrap().permission_mode = mode.into();
                    json!({ "behavior": "allow", "updatedInput": w.input, "toolUseID": w.tool_use_id })
                } else {
                    deny("The user wants to keep planning.")
                }
            }
            _ => {
                if cmd["value"] == "Always allow" {
                    json!({ "behavior": "allow", "updatedInput": w.input, "updatedPermissions": w.suggestions, "toolUseID": w.tool_use_id })
                } else if cmd["value"] == "Approve" || cmd["confirmed"] == true {
                    json!({ "behavior": "allow", "updatedInput": w.input, "toolUseID": w.tool_use_id })
                } else {
                    deny("The user denied this.")
                }
            }
        }
    };
    shared.reply_control(&w.request_id, response).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> (Arc<Shared>, mpsc::UnboundedReceiver<HarnessEvent>) {
        let (events, rx) = mpsc::unbounded_channel();
        let cfg = HarnessConfig { kind: HarnessKind::Claude, program: None, cwd: "/work".into(), session: SessionTarget::New, extra_args: vec![], env: vec![], remote: None };
        let shared = Arc::new(Shared {
            state: Mutex::new(State { initialized: true, ..Default::default() }),
            events,
            stdin: tokio::sync::Mutex::new(None),
            child: tokio::sync::Mutex::new(None),
            log: Mutex::new(None),
            cfg,
            program: "claude".into(),
            generation: Default::default(),
            remote_config: None,
        });
        (shared, rx)
    }

    /// A real recording (Claude Code 2.1.296: a Bash call, then a reply;
    /// then a second turn) goes through as harlo events.
    #[tokio::test]
    async fn replays_a_real_session() {
        let (s, mut rx) = shared();
        for line in include_str!("../tests/fixtures/claude-turns.jsonl").lines() {
            incoming(&s, serde_json::from_str(line).unwrap()).await;
        }
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let ends: Vec<&Value> = events.iter().filter_map(|e| if let HarnessEvent::MessageEnd { message, .. } = e { Some(message) } else { None }).collect();
        let texts: Vec<String> = ends.iter().flat_map(|m| m["content"].as_array().cloned().unwrap_or_default()).filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str().map(str::to_owned)).collect();
        assert_eq!(texts, ["hello from harlo", "OK"], "{ends:#?}");
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::ToolStart { name, .. } if name == "Bash")));
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::ToolEnd { result, .. } if result["content"][0]["text"] == "hello from harlo")));
        let starts = events.iter().filter(|e| matches!(e, HarnessEvent::RunStarted)).count();
        let settled = events.iter().filter(|e| matches!(e, HarnessEvent::Settled)).count();
        assert_eq!((starts, settled), (2, 2), "one run per turn");
        let st = s.state.lock().unwrap();
        assert!(!st.session_id.is_empty() && st.context_tokens > 0 && st.cost > 0.0);
    }

    #[test]
    fn modes_and_args() {
        assert_eq!(claude_mode("always-ask", false), "default");
        assert_eq!(claude_mode("write", false), "acceptEdits");
        assert_eq!(claude_mode("auto", false), "bypassPermissions");
        assert_eq!(claude_mode("write", true), "plan");
        let (model, rest) = split_args(&["--model".into(), "anthropic/claude-haiku-4-5".into(), "--x".into()]);
        assert_eq!((model.as_deref(), rest), (Some("claude-haiku-4-5"), vec!["--x".to_owned()]));
        assert_eq!(thinking_levels("claude-haiku-4-5"), ["low", "medium", "high", "max"]);
        assert_eq!(thinking_levels("claude-opus-4-8"), ["off", "low", "medium", "high", "xhigh", "max"]);
        assert_eq!(new_uuid().len(), 36);
    }
}
