//! OpenCode 2.x as a harness (its HTTP server API). harlo runs its own `opencode serve`: on 127.0.0.1, a
//! free port, a fresh password, its own process group. It never uses
//! OpenCode's shared background service (which plain CLI commands such as
//! `opencode models` start). This module: the server and its API, the
//! models, and OpenCode's saved sessions (read from its database). The
//! running session is `opencode_run.rs`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::event::HarnessKind;
use crate::store::{SavedSession, SessionInfo};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

/// A running private server; dropping it stops it (and what it started).
/// It also stops when harlo does, however harlo ends: it's started from a
/// thread that waits for it, with Linux's parent-death signal.
pub struct Server {
    pub url: String,
    password: String,
    pid: i32,
    exited: Arc<AtomicBool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

fn free_port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn random_password() -> String {
    let mut bytes = [0u8; 24];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `opencode --version` → "2.0.25".
pub fn version_number(output: &str) -> Option<String> {
    let word = output.split_whitespace().find(|w| w.trim_start_matches('v').starts_with(|c: char| c.is_ascii_digit()))?;
    Some(word.trim_start_matches('v').to_owned())
}

/// OpenCode 2.0.20 or later (the first with the API harlo uses).
pub fn supported(version: &str) -> bool {
    let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    parts.first() == Some(&2) && parts >= vec![2, 0, 20]
}

impl Server {
    /// Starts `program serve` in `cwd` and waits until it answers.
    pub fn start(program: &Path, cwd: &Path) -> Result<Server> {
        let port = free_port()?;
        let password = random_password();
        let mut cmd = Command::new(program);
        cmd.args(["serve", "--hostname", "127.0.0.1", "--port", &port.to_string()])
            .current_dir(cwd)
            .env("OPENCODE_PASSWORD", &password)
            .env("OPENCODE_SERVER_PASSWORD", &password)
            .stdin(Stdio::null());
        // harlo's tools (orchestration): its bridge knows it's OpenCode's.
        if let Some(bridge) = crate::mcp_bridge() {
            let config = serde_json::json!({ "mcp": { "harlo": { "type": "local", "command": bridge, "environment": { "HARLO_OPENCODE": "1" }, "enabled": true } } });
            cmd.env("OPENCODE_CONFIG_CONTENT", config.to_string()).env("HARLO_OPENCODE", "1");
        }
        Self::launch(cmd, password, None, None, &program.display().to_string())
    }

    /// Starts `opencode serve` on a remote host, reached through a local
    /// port forwarded over SSH. The password goes in on stdin (not on a
    /// command line), and stdin stays open: when it closes (harlo quits, the
    /// connection drops) the server there is stopped.
    pub fn start_remote(remote: &crate::process::RemoteLaunch) -> Result<Server> {
        let local = free_port()?;
        let mut b = [0u8; 2];
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut b);
        }
        let there = 20000 + (u16::from_le_bytes(b) % 40000);
        let password = random_password();
        // harlo's tools (orchestration): the bridge on the host.
        let tools = crate::mcp_bridge_for(Some(remote))
            .map(|bridge| {
                let config = serde_json::json!({ "mcp": { "harlo": { "type": "local", "command": bridge, "environment": { "HARLO_OPENCODE": "1" }, "enabled": true } } });
                format!("export OPENCODE_CONFIG_CONTENT={} HARLO_OPENCODE=1; ", crate::process::shell_quote(&config.to_string()))
            })
            .unwrap_or_default();
        let script = format!(
            "{} read -r P; export OPENCODE_PASSWORD=\"$P\" OPENCODE_SERVER_PASSWORD=\"$P\"; {tools}cd; opencode serve --hostname 127.0.0.1 --port {there} & s=$!; cat >/dev/null; kill $s 2>/dev/null; wait $s 2>/dev/null",
            remote.prelude
        );
        let mut cmd = Command::new("ssh");
        cmd.args(["-o", "ExitOnForwardFailure=yes", "-L", &format!("{local}:127.0.0.1:{there}")]).args(&remote.ssh_args).arg(script).stdin(Stdio::piped());
        Self::launch(cmd, password.clone(), Some(format!("http://127.0.0.1:{local}")), Some(format!("{password}\n")), &format!("OpenCode on {}", remote.host))
    }

    /// Runs `cmd` (a server, or ssh to one) and waits for it to say it's
    /// listening, then for an answer. `url`: where to reach it (else where it
    /// says it listens); `input`: written to its stdin, which then stays open.
    fn launch(mut cmd: Command, password: String, url: Option<String>, input: Option<String>, what: &str) -> Result<Server> {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut cmd, || {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        // The parent-death signal comes when the spawning thread ends: a
        // thread that lives as long as the server (holding its stdin).
        let exited = Arc::new(AtomicBool::new(false));
        let (spawned_tx, spawned_rx) = mpsc::channel();
        let flag = Arc::clone(&exited);
        std::thread::Builder::new().name("opencode-serve".into()).spawn(move || match cmd.spawn() {
            Ok(mut child) => {
                let stdin = child.stdin.take().map(|mut s| {
                    use std::io::Write;
                    if let Some(text) = &input {
                        let _ = s.write_all(text.as_bytes());
                        let _ = s.flush();
                    }
                    s
                });
                let pipes = (child.id() as i32, child.stdout.take(), child.stderr.take());
                let _ = spawned_tx.send(Ok(pipes));
                let _ = child.wait();
                drop(stdin);
                flag.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                let _ = spawned_tx.send(Err(e));
            }
        })?;
        let (pid, stdout, stderr) = spawned_rx.recv().context("starting OpenCode")?.with_context(|| format!("starting {what}"))?;
        // It prints `server listening on http://…` when it's up.
        let (tx, rx) = mpsc::channel::<String>();
        for pipe in [stdout.map(|p| Box::new(p) as Box<dyn Read + Send>), stderr.map(|p| Box::new(p) as _)].into_iter().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut server = Server { url: String::new(), password, pid, exited };
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        let mut said = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(listening) = line.split("server listening on").nth(1).and_then(|r| r.split_whitespace().next()) {
                        server.url = url.clone().unwrap_or_else(|| listening.trim_end_matches('/').to_owned());
                        break;
                    }
                    said.push(line);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => bail!("{what}: the server didn't start in time"),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let last: Vec<String> = said.iter().rev().take(3).rev().cloned().collect();
                    if last.iter().any(|l| l.contains("command not found") || l.contains("not found")) {
                        bail!("{what}: OpenCode isn't installed there");
                    }
                    bail!("{what}: the server stopped: {}", last.join(" / "))
                }
            }
        }
        // (A forwarded port may take a moment.)
        let mut info = server.get("/api/info", None);
        for _ in 0..20 {
            if info.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(150));
            info = server.get("/api/info", None);
        }
        let info = info.with_context(|| format!("{what}: the server didn't answer"))?;
        let version = info["version"].as_str().unwrap_or_default();
        if !supported(version) {
            bail!("OpenCode {version} is too old: harlo needs 2.0.20 or later");
        }
        Ok(server)
    }

    fn auth(&self) -> String {
        use base64::Engine;
        format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("opencode:{}", self.password)))
    }

    /// GET `path`; `directory` (the project) goes in OpenCode's directory header.
    pub fn get(&self, path: &str, directory: Option<&Path>) -> Result<Value> {
        self.request("GET", path, None, directory)
    }

    /// A request; OpenCode's errors (`{_tag, message}`) come back as Err.
    pub fn request(&self, method: &str, path: &str, body: Option<&Value>, directory: Option<&Path>) -> Result<Value> {
        let agent = ureq::Agent::new_with_config(ureq::config::Config::builder().timeout_global(Some(Duration::from_secs(30))).http_status_as_error(false).build());
        let url = format!("{}{path}", self.url);
        let dir = directory.map(|d| url_encode(&d.to_string_lossy()));
        let mut resp = match (method, body) {
            ("GET", _) | ("DELETE", None) => {
                let mut req = if method == "GET" { agent.get(&url) } else { agent.delete(&url) };
                req = req.header("Authorization", self.auth());
                if let Some(d) = &dir {
                    req = req.header("x-opencode-directory", d);
                }
                req.call()
            }
            (_, body) => {
                let mut req = if method == "PATCH" { agent.patch(&url) } else { agent.post(&url) };
                req = req.header("Authorization", self.auth()).header("Content-Type", "application/json");
                if let Some(d) = &dir {
                    req = req.header("x-opencode-directory", d);
                }
                req.send(body.cloned().unwrap_or(Value::Object(Default::default())).to_string())
            }
        }
        .map_err(|e| anyhow!("OpenCode {path}: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if status >= 400 {
            let why = value["message"].as_str().or_else(|| value["_tag"].as_str()).map(str::to_owned).unwrap_or_else(|| format!("HTTP {status}"));
            bail!("{why}");
        }
        Ok(value)
    }

    /// Follows OpenCode's event stream (blocking): each event's JSON goes to
    /// `each` until it returns false or the stream ends.
    pub fn follow_events(&self, mut each: impl FnMut(Value) -> bool) -> Result<()> {
        let agent = ureq::Agent::new_with_config(ureq::config::Config::builder().timeout_global(None).build());
        let resp = agent
            .get(&format!("{}/api/event", self.url))
            .header("Authorization", self.auth())
            .header("Accept", "text/event-stream")
            .call()
            .map_err(|e| anyhow!("OpenCode events: {e}"))?;
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut data = String::new();
        for line in reader.lines() {
            let line = line?;
            if let Some(rest) = line.strip_prefix("data:") {
                data.push_str(rest.trim_start());
            } else if line.is_empty() && !data.is_empty() {
                if let Ok(v) = serde_json::from_str::<Value>(&data)
                    && !each(v)
                {
                    return Ok(());
                }
                data.clear();
            }
        }
        Ok(())
    }

    /// The server is still running.
    pub fn alive(&self) -> bool {
        !self.exited.load(Ordering::Relaxed)
    }

    /// Stops the server and what it started (its process group).
    pub fn stop(&self) {
        if self.alive() && self.pid > 0 {
            // SAFETY: plain syscall on the group the server leads.
            unsafe {
                libc::kill(-self.pid, libc::SIGTERM);
            }
        }
    }
}

/// OpenCode's data folder (`$XDG_DATA_HOME/opencode`).
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("opencode"))
}

fn db_path() -> Option<PathBuf> {
    std::env::var_os("OPENCODE_DB").map(PathBuf::from).or_else(|| data_dir().map(|d| d.join("opencode.db")))
}

fn open_db() -> Option<rusqlite::Connection> {
    let path = db_path()?;
    if !path.is_file() {
        return None;
    }
    rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()
}

/// How harlo names an OpenCode session (it has no file of its own): a path
/// whose last part is the session id.
pub fn session_path(id: &str) -> PathBuf {
    data_dir().unwrap_or_default().join("sessions").join(id)
}

/// How harlo names a remote host's OpenCode session (its database is
/// there; the path only has to be its own, on that host).
pub fn remote_session_path(host: &str, id: &str) -> PathBuf {
    PathBuf::from(format!("ssh://{host}/opencode-sessions/{id}"))
}

/// The session id in such a path.
pub fn session_id(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Every session (not subagents' nor archived ones): `(path, updated, 0)`.
pub fn list_sessions() -> Vec<(PathBuf, SystemTime, u64)> {
    let Some(db) = open_db() else { return Vec::new() };
    let Ok(mut q) = db.prepare("SELECT id, time_updated FROM session_v2 WHERE parent_id IS NULL AND time_archived IS NULL") else { return Vec::new() };
    q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map(|rows| {
            rows.flatten()
                .map(|(id, ms)| (session_path(&id), SystemTime::UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64), 0))
                .collect()
        })
        .unwrap_or_default()
}

/// The sidebar's view of a session.
pub fn read_session(path: &Path) -> Option<SessionInfo> {
    let id = session_id(path);
    let db = open_db()?;
    let (dir, title, created, updated): (String, Option<String>, i64, i64) = db
        .query_row("SELECT directory, title, time_created, time_updated FROM session_v2 WHERE id = ?1", [&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .ok()?;
    let first: Option<String> = db
        .query_row("SELECT data FROM session_message WHERE session_id = ?1 AND type = 'user' ORDER BY seq LIMIT 1", [&id], |r| r.get(0))
        .ok()
        .and_then(|d: String| serde_json::from_str::<Value>(&d).ok())
        .and_then(|v| v["text"].as_str().map(str::to_owned));
    // Not started yet (no message): not shown.
    let first = first?;
    Some(SessionInfo {
        harness: HarnessKind::OpenCode,
        path: path.to_path_buf(),
        id,
        cwd: PathBuf::from(dir),
        name: title.unwrap_or_default(),
        first_message: crate::file_blocks::title_text(&first).chars().take(100).collect(),
        created: created.to_string(),
        modified: SystemTime::UNIX_EPOCH + Duration::from_millis(updated.max(0) as u64),
        size: 0,
        parent_session: None,
    })
}

/// A stored or fetched OpenCode message → pi-shaped messages (an assistant
/// step's tools also give their results).
pub fn to_pi_messages(kind: &str, data: &Value) -> Vec<Value> {
    match kind {
        "user" => vec![json_user(data["text"].as_str().unwrap_or_default())],
        "assistant" => {
            let mut content = Vec::new();
            let mut results = Vec::new();
            for b in data["content"].as_array().into_iter().flatten() {
                match b["type"].as_str() {
                    Some("text") => content.push(serde_json::json!({ "type": "text", "text": b["text"] })),
                    Some("reasoning") => content.push(serde_json::json!({ "type": "thinking", "thinking": b["text"] })),
                    Some("tool") => {
                        let st = &b["state"];
                        content.push(serde_json::json!({ "type": "toolCall", "id": b["id"], "name": b["name"], "arguments": st["input"] }));
                        match st["status"].as_str() {
                            Some("completed") | Some("error") => results.push(serde_json::json!({
                                "role": "toolResult", "toolCallId": b["id"], "isError": st["status"] == "error",
                                "content": [{ "type": "text", "text": tool_text(st) }],
                            })),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            let model = format!("{}/{}", data["model"]["providerID"].as_str().unwrap_or_default(), data["model"]["id"].as_str().unwrap_or_default());
            let mut out = vec![serde_json::json!({ "role": "assistant", "content": content, "model": model })];
            out.extend(results);
            out
        }
        "shell" => vec![serde_json::json!({ "role": "assistant", "content": [{ "type": "text", "text": format!("`$ {}`", data["shell"]["command"].as_str().unwrap_or_default()) }] })],
        _ => Vec::new(),
    }
}

fn json_user(text: &str) -> Value {
    serde_json::json!({ "role": "user", "content": [{ "type": "text", "text": text }] })
}

/// A finished tool's output text (its error for a failure).
pub fn tool_text(state: &Value) -> String {
    let content: Vec<String> = state["content"].as_array().into_iter().flatten().filter_map(|c| c["text"].as_str().map(str::to_owned)).collect();
    if !content.is_empty() {
        return content.join("\n");
    }
    state["error"]["message"].as_str().unwrap_or_default().to_owned()
}

/// A session's conversation from the database, in pi's message shape.
pub fn history(id: &str) -> Vec<Value> {
    let Some(db) = open_db() else { return Vec::new() };
    let Ok(mut q) = db.prepare("SELECT type, data FROM session_message WHERE session_id = ?1 ORDER BY seq") else { return Vec::new() };
    q.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map(|rows| {
            rows.flatten()
                .flat_map(|(kind, data)| serde_json::from_str::<Value>(&data).map(|d| to_pi_messages(&kind, &d)).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
}

/// What a saved session shows before OpenCode runs.
pub fn saved(path: &Path) -> SavedSession {
    let messages = history(&session_id(path));
    let model = messages.iter().rev().find_map(|m| m["model"].as_str().filter(|s| s.len() > 1)).unwrap_or_default().to_owned();
    let name = open_db()
        .and_then(|db| db.query_row("SELECT title FROM session_v2 WHERE id = ?1", [session_id(path)], |r| r.get::<_, Option<String>>(0)).ok().flatten())
        .unwrap_or_default();
    SavedSession { messages, model, thinking: String::new(), name }
}

/// The ids of a session's prompts, in order (as `history` shows them).
pub fn user_entries(path: &Path) -> Vec<String> {
    let Some(db) = open_db() else { return Vec::new() };
    let Ok(mut q) = db.prepare("SELECT id FROM session_message WHERE session_id = ?1 AND type = 'user' ORDER BY seq") else { return Vec::new() };
    q.query_map([session_id(path)], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// A prompt's text.
pub fn prompt_text(message_id: &str) -> Option<String> {
    let db = open_db()?;
    let data: String = db.query_row("SELECT data FROM session_message WHERE id = ?1", [message_id], |r| r.get(0)).ok()?;
    serde_json::from_str::<Value>(&data).ok()?["text"].as_str().map(str::to_owned)
}

/// When a session last changed (its stand-in for a file's stamp), if it's
/// one of OpenCode's.
pub fn stamp(path: &Path) -> Option<String> {
    let dir = data_dir()?.join("sessions");
    if path.parent() != Some(dir.as_path()) {
        return None;
    }
    let db = open_db()?;
    db.query_row("SELECT time_updated FROM session_v2 WHERE id = ?1", [session_id(path)], |r| r.get::<_, i64>(0)).ok().map(|t| t.to_string())
}

/// The tools that start a subagent (`task` before OpenCode 2).
pub const SUBAGENT_TOOLS: [&str; 2] = ["subagent", "task"];

/// The child session a subagent call names: its metadata, else the
/// `sessionID` in its output.
pub fn child_of(state: &Value) -> Option<String> {
    if let Some(id) = state["metadata"]["sessionID"].as_str().or_else(|| state["metadata"]["sessionId"].as_str()) {
        return Some(id.to_owned());
    }
    let text = tool_text(state);
    let rest = text.split("sessionID").nth(1)?;
    let start = rest.find("ses_")?;
    Some(rest[start..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect())
}

/// The subagents (child sessions) of a saved session, as harlo's dock
/// lists them: each linked to the call that started it when the call says
/// which child it was.
pub fn subagents(path: &Path) -> Vec<Value> {
    let id = session_id(path);
    let Some(db) = open_db() else { return Vec::new() };
    let Ok(mut q) = db.prepare("SELECT id, title, agent, idle_outcome FROM session_v2 WHERE parent_id = ?1 ORDER BY time_created") else { return Vec::new() };
    // (id, title, agent, how it ended)
    type Child = (String, Option<String>, Option<String>, Option<String>);
    let children: Vec<Child> =
        q.query_map([&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|rows| rows.flatten().collect()).unwrap_or_default();
    if children.is_empty() {
        return Vec::new();
    }
    // Calls → children, from the parent's stored steps.
    let mut calls: HashMap<String, String> = HashMap::new();
    if let Ok(mut q) = db.prepare("SELECT data FROM session_message WHERE session_id = ?1 AND type = 'assistant' ORDER BY seq") {
        let rows: Vec<String> = q.query_map([&id], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default();
        for data in rows {
            let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
            for b in v["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "tool" && SUBAGENT_TOOLS.contains(&b["name"].as_str().unwrap_or_default())) {
                if let (Some(call), Some(child)) = (b["id"].as_str(), child_of(&b["state"])) {
                    calls.insert(child, call.to_owned());
                }
            }
        }
    }
    children
        .into_iter()
        .map(|(child, title, agent, outcome)| {
            let call = calls.get(&child).cloned().unwrap_or_else(|| child.clone());
            let status = match outcome.as_deref() {
                Some("succeeded") => "completed",
                Some("failed") => "failed",
                Some("interrupted") => "aborted",
                _ => "ended",
            };
            serde_json::json!({
                "id": call, "parentToolCallId": call, "name": title.unwrap_or_else(|| "Subagent".into()), "agent": agent.unwrap_or_else(|| "general".into()),
                "status": status, "sessionFile": session_path(&child).to_string_lossy(), "childSession": child, "canCancel": false, "canSteer": false,
            })
        })
        .collect()
}

pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// The models OpenCode offers in `directory`: `(provider/model, name)`,
/// enabled models of providers that aren't turned off.
pub fn models(server: &Server, directory: &Path) -> Result<Vec<(String, String)>> {
    // A folder's catalog loads on first use: empty at first, filled a moment
    // later.
    for _ in 0..40 {
        let list = models_now(server, directory)?;
        if !list.is_empty() {
            return Ok(list);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Ok(Vec::new())
}

fn models_now(server: &Server, directory: &Path) -> Result<Vec<(String, String)>> {
    let providers = server.get("/api/provider", Some(directory))?;
    let off: Vec<&str> = providers["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["activation"] == "disabled")
        .filter_map(|p| p["id"].as_str())
        .collect();
    let models = server.get("/api/model", Some(directory))?;
    Ok(models["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["enabled"] != false)
        .filter(|m| !off.contains(&m["providerID"].as_str().unwrap_or_default()))
        .filter_map(|m| {
            let provider = m["providerID"].as_str()?;
            let id = m["modelID"].as_str().or_else(|| m["id"].as_str())?;
            Some((format!("{provider}/{id}"), m["name"].as_str().unwrap_or(id).to_owned()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(version_number("opencode v2.0.25").as_deref(), Some("2.0.25"));
        assert_eq!(version_number("2.0.25").as_deref(), Some("2.0.25"));
        assert!(supported("2.0.25") && supported("2.1.0") && !supported("2.0.19") && !supported("1.9.0") && !supported("3.0.0"));
    }

    #[test]
    fn subagent_children() {
        use serde_json::json;
        // The call's metadata, else its output (a foreground report, a
        // background launch, a cancel).
        assert_eq!(child_of(&json!({ "metadata": { "sessionID": "ses_a1" } })).as_deref(), Some("ses_a1"));
        assert_eq!(child_of(&json!({ "content": [{ "type": "text", "text": "<subagent sessionID=\"ses_b2\" state=\"completed\">\nok\n</subagent>" }] })).as_deref(), Some("ses_b2"));
        assert_eq!(child_of(&json!({ "error": { "message": "Subagent cancelled (sessionID: ses_c3)" } })).as_deref(), Some("ses_c3"));
        assert_eq!(child_of(&json!({ "content": [{ "type": "text", "text": "no session here" }] })), None);
    }

    #[test]
    fn basic_auth() {
        let s = Server { url: String::new(), password: "pw".into(), pid: 0, exited: Arc::default() };
        // base64("opencode:pw")
        assert_eq!(s.auth(), "Basic b3BlbmNvZGU6cHc=");
    }
}

#[cfg(test)]
mod this_computer {
    /// Starts a private server with the real `opencode` and lists models
    /// (no prompt is sent): `cargo test -p harlo-harness opencode -- --ignored`.
    #[test]
    #[ignore]
    fn lists_models() {
        let program = crate::resolve_program(crate::HarnessKind::OpenCode).expect("opencode installed");
        let dir = std::env::temp_dir();
        let started = std::time::Instant::now();
        let server = super::Server::start(&program, &dir).unwrap();
        let models = super::models(&server, &dir).unwrap();
        println!("started in {:?}; {} models: {:?}", started.elapsed(), models.len(), models.iter().take(4).collect::<Vec<_>>());
        assert!(!models.is_empty());
    }
}
