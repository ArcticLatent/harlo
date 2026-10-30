//! Agents that speak the Agent Client Protocol (ACP: JSON-RPC 2.0, one
//! message per line on the agent's stdin and stdout), such as Hermes
//! (`hermes acp`). One connection per agent carries every harlo session
//! with it (`acp_run.rs`): its `session/update`s and requests go to the
//! session they name.
//!
//! An agent's `session/list` needn't list the sessions a client started
//! (Hermes' doesn't), so harlo keeps its own index of them
//! (`<data>/harlo/acp-sessions.json`): what the sidebar lists.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::event::HarnessKind;
use crate::process::RemoteLaunch;
use crate::store::{SavedSession, SessionInfo};

/// How harlo starts an ACP agent.
pub struct Agent {
    pub kind: HarnessKind,
    /// The program's name (found like the other agents').
    pub program: &'static str,
    /// Its ACP mode.
    pub args: &'static [&'static str],
}

/// The ACP agents harlo knows.
pub const AGENTS: [Agent; 1] = [Agent { kind: HarnessKind::Hermes, program: "hermes", args: &["acp"] }];

pub fn agent(kind: HarnessKind) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.kind == kind)
}

pub fn is_acp(kind: HarnessKind) -> bool {
    agent(kind).is_some()
}

// ---- session paths -------------------------------------------------------

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("harlo"))
}

/// Which ACP session a path names: its agent, its id, and the host it runs
/// on ("" here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    pub kind: HarnessKind,
    pub id: String,
    pub host: String,
}

/// How harlo names an ACP session (it has no file of its own): a path whose
/// last part is the session id; on a remote host, `ssh://host/.harlo-acp/…`.
pub fn session_path(kind: HarnessKind, host: &str, id: &str) -> PathBuf {
    if host.is_empty() {
        data_dir().unwrap_or_default().join("acp-sessions").join(kind.as_str()).join(id)
    } else {
        PathBuf::from(format!("ssh://{host}/.harlo-acp/{}/{id}", kind.as_str()))
    }
}

pub fn session_of(path: &Path) -> Option<SessionRef> {
    let s = path.to_str()?;
    let (host, kind, id) = match s.strip_prefix("ssh://") {
        Some(rest) => {
            let (host, rest) = rest.split_once("/.harlo-acp/")?;
            let (kind, id) = rest.split_once('/')?;
            (host.to_owned(), kind, id)
        }
        None => {
            let rest = path.strip_prefix(data_dir()?.join("acp-sessions")).ok()?.to_str()?;
            let (kind, id) = rest.split_once('/')?;
            (String::new(), kind, id)
        }
    };
    let kind = HarnessKind::parse(kind).filter(|k| is_acp(*k))?;
    (!id.is_empty() && !id.contains('/')).then(|| SessionRef { kind, id: id.to_owned(), host })
}

pub fn is_session_path(path: &Path) -> bool {
    session_of(path).is_some()
}

// ---- harlo's index of ACP sessions ----------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Entry {
    pub agent: String,
    /// The remote host it runs on ("" here).
    #[serde(default)]
    pub host: String,
    pub id: String,
    /// Its folder, as its machine sees it.
    pub cwd: String,
    #[serde(default)]
    pub title: String,
    /// The first line of the first prompt.
    #[serde(default)]
    pub first: String,
    /// Unix milliseconds.
    pub created: u64,
    pub updated: u64,
}

impl Entry {
    fn is(&self, r: &SessionRef) -> bool {
        self.agent == r.kind.as_str() && self.host == r.host && self.id == r.id
    }
}

fn index_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("acp-sessions.json"))
}

static INDEX_LOCK: Mutex<()> = Mutex::new(());

pub fn index() -> Vec<Entry> {
    index_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn entry(r: &SessionRef) -> Option<Entry> {
    index().into_iter().find(|e| e.is(r))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Adds or changes a session's entry (its `updated` becomes now). `cwd`:
/// as its machine sees it.
pub fn remember(r: &SessionRef, cwd: &str, change: impl FnOnce(&mut Entry)) {
    let Some(path) = index_path() else { return };
    let _lock = INDEX_LOCK.lock().unwrap();
    let mut all = index();
    let now = now_ms();
    let i = match all.iter().position(|e| e.is(r)) {
        Some(i) => i,
        None => {
            all.push(Entry { agent: r.kind.as_str().to_owned(), host: r.host.clone(), id: r.id.clone(), cwd: cwd.to_owned(), created: now, ..Default::default() });
            all.len() - 1
        }
    };
    change(&mut all[i]);
    all[i].updated = now;
    write_index(&path, &all);
}

/// Takes a session out of harlo's list (the agent keeps it).
pub fn forget(r: &SessionRef) {
    let Some(path) = index_path() else { return };
    let _lock = INDEX_LOCK.lock().unwrap();
    let mut all = index();
    all.retain(|e| !e.is(r));
    write_index(&path, &all);
    let _ = std::fs::remove_file(history_path(r));
}

/// harlo's copy of a session's conversation: shown before the agent is
/// reached, and searched.
fn history_path(r: &SessionRef) -> PathBuf {
    let dir = data_dir().unwrap_or_default().join("acp-sessions").join(r.kind.as_str());
    let dir = if r.host.is_empty() { dir } else { dir.join(format!("@{}", r.host)) };
    dir.join(format!("{}.json", r.id))
}

pub fn save_history(r: &SessionRef, messages: &[Value], model: &str) {
    let path = history_path(r);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json!({ "model": model, "messages": messages }).to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// harlo's copy of a saved session, if it has one.
pub fn cached(path: &Path) -> Option<SavedSession> {
    let r = session_of(path)?;
    let v: Value = serde_json::from_str(&std::fs::read_to_string(history_path(&r)).ok()?).ok()?;
    let name = entry(&r).map(|e| e.title).unwrap_or_default();
    Some(SavedSession { messages: v["messages"].as_array().cloned().unwrap_or_default(), model: v["model"].as_str().unwrap_or_default().to_owned(), thinking: String::new(), name })
}

fn write_index(path: &Path, all: &[Entry]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_string_pretty(all).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// An agent's sessions here in harlo's index: `(path, changed, size)`, the
/// size standing in for what the sidebar shows (it changes with the title).
pub fn list_sessions(kind: HarnessKind) -> Vec<(PathBuf, SystemTime, u64)> {
    index()
        .into_iter()
        .filter(|e| e.agent == kind.as_str() && e.host.is_empty())
        .map(|e| (session_path(kind, "", &e.id), UNIX_EPOCH + Duration::from_millis(e.updated), (e.title.len() + e.first.len()) as u64))
        .collect()
}

fn info(kind: HarnessKind, e: Entry) -> SessionInfo {
    let size = (e.title.len() + e.first.len()) as u64;
    let (path, cwd) = if e.host.is_empty() {
        (session_path(kind, "", &e.id), PathBuf::from(&e.cwd))
    } else {
        (session_path(kind, &e.host, &e.id), PathBuf::from(format!("ssh://{}{}", e.host, e.cwd)))
    };
    SessionInfo {
        harness: kind,
        path,
        id: e.id,
        cwd,
        name: e.title,
        first_message: e.first,
        created: chrono_iso(e.created),
        modified: UNIX_EPOCH + Duration::from_millis(e.updated),
        size,
        parent_session: None,
    }
}

pub fn read_session(path: &Path) -> Option<SessionInfo> {
    let r = session_of(path)?;
    entry(&r).map(|e| info(r.kind, e))
}

/// An agent's sessions on a remote host, in harlo's index (paths and
/// folders as `ssh://host/…`).
pub fn remote_sessions(kind: HarnessKind, host: &str) -> Vec<SessionInfo> {
    index().into_iter().filter(|e| e.agent == kind.as_str() && e.host == host).map(|e| info(kind, e)).collect()
}

/// Unix milliseconds as ISO 8601 (UTC).
fn chrono_iso(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3600, rem % 3600 / 60, rem % 60, ms % 1000)
}

// ---- the connection ------------------------------------------------------

/// What a session gets from the agent.
#[derive(Debug)]
pub enum Incoming {
    /// A `session/update`'s `update`.
    Update(Value),
    /// The agent asks something (`session/request_permission`, …): answer
    /// with [`Conn::respond`].
    Request { id: Value, method: String, params: Value },
    /// The agent's process ended.
    Closed,
}

type Reply = std::result::Result<Value, String>;

/// One running agent process.
pub struct Conn {
    pub kind: HarnessKind,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Reply>>>,
    next: AtomicU64,
    routes: Mutex<HashMap<String, mpsc::UnboundedSender<Incoming>>>,
    /// Updates for a session not routed yet (sent right after its
    /// `session/new` answer): handed over when it is.
    early: Mutex<HashMap<String, Vec<Value>>>,
    alive: AtomicBool,
    stderr_tail: Mutex<Vec<String>>,
    /// Its `initialize` answer.
    pub info: Mutex<Value>,
}

impl Conn {
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn write(&self, msg: &Value) -> Result<()> {
        let mut line = msg.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().unwrap();
        stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()).context("the agent isn't running")
    }

    fn start_request(&self, method: &str, params: Value) -> Result<oneshot::Receiver<Reply>> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if let Err(e) = self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })) {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        Ok(rx)
    }

    /// A request; the agent's error comes back as Err (its details, when
    /// it gives them).
    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let rx = self.start_request(method, params)?;
        rx.await.map_err(|_| anyhow!("{} stopped", self.kind.label()))?.map_err(|e| anyhow!(e))
    }

    /// The same, from a thread that may block (not async code).
    pub fn request_blocking(&self, method: &str, params: Value) -> Result<Value> {
        let rx = self.start_request(method, params)?;
        rx.blocking_recv().map_err(|_| anyhow!("{} stopped", self.kind.label()))?.map_err(|e| anyhow!(e))
    }

    pub fn notify(&self, method: &str, params: Value) {
        let _ = self.write(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Answers one of the agent's requests.
    pub fn respond(&self, id: Value, result: Value) {
        let _ = self.write(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    pub fn respond_error(&self, id: Value, message: &str) {
        let _ = self.write(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": message } }));
    }

    /// A session's updates and requests go to `tx` (instead of whoever had
    /// them), the ones that came before first.
    pub fn route(&self, session: &str, tx: mpsc::UnboundedSender<Incoming>) {
        let mut routes = self.routes.lock().unwrap();
        for u in self.early.lock().unwrap().remove(session).unwrap_or_default() {
            let _ = tx.send(Incoming::Update(u));
        }
        routes.insert(session.to_owned(), tx);
    }

    pub fn unroute(&self, session: &str) {
        self.routes.lock().unwrap().remove(session);
    }

    pub fn stop(&self) {
        let mut child = self.child.lock().unwrap();
        // SAFETY: a plain syscall on the group the agent leads.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGTERM);
        }
        let _ = child.try_wait();
    }

    fn stderr_text(&self) -> String {
        self.stderr_tail.lock().unwrap().join("\n")
    }
}

/// The agents' connections, by agent and host ("" here).
static CONNS: OnceLock<Mutex<HashMap<(HarnessKind, String), Arc<Conn>>>> = OnceLock::new();

/// The running connection to `kind`'s agent, here or on `remote`'s host
/// (started and initialized on first use, again if it stopped). Blocking:
/// call it off async threads. `program`: here, its path; there, its name.
pub fn conn(kind: HarnessKind, program: &Path, remote: Option<&RemoteLaunch>) -> Result<Arc<Conn>> {
    let key = (kind, remote.map(|r| r.host.clone()).unwrap_or_default());
    let mut conns = CONNS.get_or_init(Default::default).lock().unwrap();
    if let Some(c) = conns.get(&key)
        && c.alive()
    {
        return Ok(Arc::clone(c));
    }
    let c = start(kind, program, remote)?;
    let info = c.request_blocking(
        "initialize",
        json!({
            "protocolVersion": 1,
            // Its own tools read, write and run things; harlo shows them.
            "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
            "clientInfo": { "name": "harlo", "version": env!("CARGO_PKG_VERSION") },
        }),
    );
    let info = match info {
        Ok(i) => i,
        Err(e) => {
            c.stop();
            let tail = c.stderr_text();
            let last = tail.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default();
            bail!("{} didn't start: {e:#}{}", kind.label(), if last.is_empty() { String::new() } else { format!(" ({last})") });
        }
    };
    *c.info.lock().unwrap() = info;
    conns.insert(key, Arc::clone(&c));
    Ok(c)
}

/// Stops the agents (harlo is quitting).
pub fn shutdown() {
    if let Some(conns) = CONNS.get() {
        for (_, c) in conns.lock().unwrap().drain() {
            c.stop();
        }
    }
}

fn start(kind: HarnessKind, program: &Path, remote: Option<&RemoteLaunch>) -> Result<Arc<Conn>> {
    use std::os::unix::process::CommandExt;
    let a = agent(kind).with_context(|| format!("{} doesn't speak ACP", kind.label()))?;
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let mut cmd = match remote {
        // There, in its home folder (sessions name their own folders).
        Some(r) => {
            let mut words = vec![crate::process::shell_quote(&program.to_string_lossy())];
            words.extend(a.args.iter().map(|w| crate::process::shell_quote(w)));
            let mut c = Command::new("ssh");
            c.args(&r.ssh_args).arg(format!("{} cd && exec {}", r.prelude, words.join(" ")));
            c
        }
        None => {
            let mut c = Command::new(program);
            c.args(a.args);
            c
        }
    };
    let mut child = cmd
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Its own group: stopped together with what it starts.
        .process_group(0)
        .spawn()
        .with_context(|| format!("failed to start {}", program.display()))?;
    tracing::info!(program = %program.display(), "started ACP agent");
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let conn = Arc::new(Conn {
        kind,
        stdin: Mutex::new(stdin),
        child: Mutex::new(child),
        pending: Mutex::default(),
        next: AtomicU64::new(1),
        routes: Mutex::default(),
        early: Mutex::default(),
        alive: AtomicBool::new(true),
        stderr_tail: Mutex::default(),
        info: Mutex::new(Value::Null),
    });
    let c = Arc::clone(&conn);
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(std::result::Result::ok) {
            let mut tail = c.stderr_tail.lock().unwrap();
            tail.push(line);
            if tail.len() > 40 {
                tail.remove(0);
            }
        }
    });
    let c = Arc::clone(&conn);
    std::thread::spawn(move || {
        for line in BufReader::with_capacity(256 * 1024, stdout).lines().map_while(std::result::Result::ok) {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            incoming(&c, msg);
        }
        c.alive.store(false, Ordering::SeqCst);
        for (_, w) in c.pending.lock().unwrap().drain() {
            let _ = w.send(Err(format!("{} stopped", c.kind.label())));
        }
        for (_, r) in c.routes.lock().unwrap().drain() {
            let _ = r.send(Incoming::Closed);
        }
        let _ = c.child.lock().unwrap().wait();
    });
    Ok(conn)
}

/// One message from the agent: a response, a session's update, or a
/// request of its own.
fn incoming(c: &Conn, msg: Value) {
    let method = msg["method"].as_str();
    match (method, msg.get("id")) {
        (None, Some(id)) => {
            let Some(id) = id.as_u64() else { return };
            if let Some(w) = c.pending.lock().unwrap().remove(&id) {
                let reply = match msg.get("error") {
                    Some(e) => Err(error_text(e)),
                    None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = w.send(reply);
            }
        }
        (Some("session/update"), None) => {
            let p = &msg["params"];
            let Some(session) = p["sessionId"].as_str() else { return };
            let routes = c.routes.lock().unwrap();
            match routes.get(session) {
                Some(tx) => {
                    let _ = tx.send(Incoming::Update(p["update"].clone()));
                }
                None => {
                    let mut early = c.early.lock().unwrap();
                    // (A few sessions' first updates, no more.)
                    if early.len() < 16 || early.contains_key(session) {
                        let list = early.entry(session.to_owned()).or_default();
                        if list.len() < 64 {
                            list.push(p["update"].clone());
                        }
                    }
                }
            }
        }
        (Some(method), Some(id)) => {
            let p = msg["params"].clone();
            let route = p["sessionId"].as_str().and_then(|s| c.routes.lock().unwrap().get(s).cloned());
            match route {
                Some(tx) => {
                    let _ = tx.send(Incoming::Request { id: id.clone(), method: method.to_owned(), params: p });
                }
                None => {
                    let _ = c.write(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("harlo doesn't handle {method}") } }));
                }
            }
        }
        _ => {}
    }
}

/// An error object as one line: its details (Hermes puts the reason there),
/// else its message.
fn error_text(e: &Value) -> String {
    e["data"]["details"].as_str().or_else(|| e["data"].as_str()).or_else(|| e["message"].as_str()).unwrap_or("the agent refused").to_owned()
}

// ---- what sessions show ----------------------------------------------------

/// A model as harlo names it: ACP's id (`provider:model` for Hermes) split
/// at its first `:` into a provider and an id.
pub fn split_model(acp_id: &str) -> (String, String) {
    match acp_id.split_once(':') {
        Some((p, m)) => (p.to_owned(), m.to_owned()),
        None => (String::new(), acp_id.to_owned()),
    }
}

/// The model part of a model's name (Hermes: "Nous Portal · vendor/model").
pub fn model_name(name: &str) -> String {
    name.rsplit_once(" · ").map_or(name, |(_, m)| m).to_owned()
}

/// A tool call as harlo's tools: its name and arguments (harlo's names
/// where they match, so they show as they do for the other agents).
pub fn tool_call(update: &Value) -> (String, Value) {
    let title = update["title"].as_str().unwrap_or_default();
    let (tool, preview) = title.split_once(": ").unwrap_or((title, ""));
    let path = update["locations"][0]["path"].as_str().map(str::to_owned);
    let text = content_text(&update["content"]);
    let raw = &update["rawInput"];
    match tool {
        "terminal" => ("bash".into(), json!({ "command": text.strip_prefix("$ ").unwrap_or(&text).lines().next().unwrap_or(preview) })),
        "read_file" => ("read".into(), json!({ "path": path.unwrap_or_else(|| preview.to_owned()) })),
        "write_file" => ("write".into(), json!({ "path": path.unwrap_or_else(|| preview.to_owned()) })),
        "patch" => ("edit".into(), json!({ "path": path.unwrap_or_else(|| preview.to_owned()) })),
        "search_files" => ("grep".into(), json!({ "pattern": preview })),
        _ if raw.is_object() => (tool.to_owned(), raw.clone()),
        _ => (if tool.is_empty() { "tool".into() } else { tool.to_owned() }, json!({ "description": if text.is_empty() { preview.to_owned() } else { text } })),
    }
}

/// ACP content (a block, or a list of `{type: "content", content}` /
/// `diff` items) as text.
pub fn content_text(content: &Value) -> String {
    let one = |c: &Value| -> String {
        match c["type"].as_str() {
            Some("content") => c["content"]["text"].as_str().unwrap_or_default().to_owned(),
            Some("text") => c["text"].as_str().unwrap_or_default().to_owned(),
            Some("diff") => format!("{}\n{}", c["path"].as_str().unwrap_or_default(), c["newText"].as_str().unwrap_or_default()),
            Some("resource_link") => c["uri"].as_str().unwrap_or_default().to_owned(),
            _ => String::new(),
        }
    };
    match content {
        Value::Array(items) => items.iter().map(one).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n"),
        Value::Object(_) => one(content),
        _ => String::new(),
    }
}

/// A session's history, as the updates its `session/load` replays, as
/// harlo's messages (pi's shapes).
#[derive(Default)]
pub struct Replay {
    pub messages: Vec<Value>,
    /// Index of the assistant message being filled.
    open: Option<usize>,
}

impl Replay {
    fn assistant(&mut self) -> &mut Vec<Value> {
        let i = match self.open {
            Some(i) => i,
            None => {
                self.messages.push(json!({ "role": "assistant", "content": [] }));
                self.open = Some(self.messages.len() - 1);
                self.messages.len() - 1
            }
        };
        self.messages[i]["content"].as_array_mut().expect("content list")
    }

    fn append(&mut self, kind: &str, text: &str) {
        let key = if kind == "thinking" { "thinking" } else { "text" };
        let blocks = self.assistant();
        match blocks.last_mut() {
            Some(b) if b["type"] == kind => {
                let joined = format!("{}{text}", b[key].as_str().unwrap_or_default());
                b[key] = json!(joined);
            }
            _ => blocks.push(json!({ "type": kind, key: text })),
        }
    }

    pub fn update(&mut self, u: &Value) {
        let text = u["content"]["text"].as_str().unwrap_or_default();
        match u["sessionUpdate"].as_str().unwrap_or_default() {
            "user_message_chunk" => {
                self.open = None;
                match self.messages.last_mut() {
                    Some(m) if m["role"] == "user" => {
                        let joined = format!("{}{text}", m["content"][0]["text"].as_str().unwrap_or_default());
                        m["content"][0]["text"] = json!(joined);
                    }
                    _ => self.messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": text }] })),
                }
            }
            "agent_message_chunk" => self.append("text", text),
            "agent_thought_chunk" => self.append("thinking", text),
            "tool_call" => {
                let (name, args) = tool_call(u);
                let id = u["toolCallId"].as_str().unwrap_or_default().to_owned();
                self.assistant().push(json!({ "type": "toolCall", "id": id, "name": name, "arguments": args }));
            }
            "tool_call_update" if matches!(u["status"].as_str(), Some("completed" | "failed")) => {
                let id = u["toolCallId"].as_str().unwrap_or_default();
                // Only calls it showed (not its approvals' own ids).
                let known = self.messages.iter().any(|m| m["content"].as_array().is_some_and(|c| c.iter().any(|b| b["type"] == "toolCall" && b["id"] == id)));
                if known {
                    self.open = None;
                    self.messages.push(json!({ "role": "toolResult", "toolCallId": id, "isError": u["status"] == "failed", "content": [{ "type": "text", "text": content_text(&u["content"]) }] }));
                }
            }
            _ => {}
        }
    }
}

/// A saved session's conversation (blocking): harlo's copy, else loaded
/// into the agent and its replay collected (and kept).
pub fn saved(path: &Path, program: &Path, remote: Option<&RemoteLaunch>) -> SavedSession {
    if let Some(s) = cached(path) {
        return s;
    }
    let Some(r) = session_of(path) else { return SavedSession::default() };
    let Some(entry) = entry(&r) else { return SavedSession::default() };
    let Ok(c) = conn(r.kind, program, remote) else { return SavedSession::default() };
    let (tx, mut rx) = mpsc::unbounded_channel();
    c.route(&r.id, tx);
    let loaded = c.request_blocking("session/load", json!({ "sessionId": r.id, "cwd": entry.cwd, "mcpServers": [] }));
    c.unroute(&r.id);
    let mut replay = Replay::default();
    while let Ok(i) = rx.try_recv() {
        if let Incoming::Update(u) = i {
            replay.update(&u);
        }
    }
    let model = loaded.ok().and_then(|r| r["models"]["currentModelId"].as_str().map(str::to_owned)).unwrap_or_default();
    save_history(&r, &replay.messages, &model);
    SavedSession { messages: replay.messages, model, thinking: String::new(), name: entry.title }
}

/// The models an agent offers (blocking): `(id, name)` from a session it
/// starts for the question (not kept: an agent keeps a session only once
/// it's prompted).
pub fn models(kind: HarnessKind, program: &Path) -> Result<Vec<(String, String)>> {
    let c = conn(kind, program, None)?;
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let r = c.request_blocking("session/new", json!({ "cwd": home, "mcpServers": [] }))?;
    Ok(r["models"]["availableModels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["modelId"].as_str()?.to_owned(), model_name(m["name"].as_str().unwrap_or_default()))))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        let p = session_path(HarnessKind::Hermes, "", "local-abc");
        assert_eq!(session_of(&p), Some(SessionRef { kind: HarnessKind::Hermes, id: "local-abc".into(), host: String::new() }));
        let r = session_path(HarnessKind::Hermes, "box", "local-abc");
        assert_eq!(r, PathBuf::from("ssh://box/.harlo-acp/hermes/local-abc"));
        assert_eq!(session_of(&r), Some(SessionRef { kind: HarnessKind::Hermes, id: "local-abc".into(), host: "box".into() }));
        assert!(!is_session_path(Path::new("/home/me/.claude/projects/x/1.jsonl")));
        assert!(!is_session_path(Path::new("ssh://box/home/me/.pi/agent/sessions/x/1.jsonl")));
    }

    #[test]
    fn names() {
        assert_eq!(split_model("nous:anthropic/claude-fable-5.1"), ("nous".to_owned(), "anthropic/claude-fable-5.1".to_owned()));
        assert_eq!(model_name("Nous Portal · anthropic/claude-fable-5.1"), "anthropic/claude-fable-5.1");
        assert_eq!(chrono_iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(chrono_iso(1_793_395_728_936), "2026-10-30T21:28:48.936Z");
    }

    #[test]
    fn tools() {
        let t = json!({ "title": "terminal: ls -la", "kind": "execute", "content": [{ "type": "content", "content": { "type": "text", "text": "$ ls -la" } }] });
        assert_eq!(tool_call(&t), ("bash".to_owned(), json!({ "command": "ls -la" })));
        let r = json!({ "title": "read_file: notes.txt", "kind": "read", "locations": [{ "path": "/w/notes.txt" }] });
        assert_eq!(tool_call(&r), ("read".to_owned(), json!({ "path": "/w/notes.txt" })));
    }

    /// A real Hermes 0.22.5 `session/load`: its replay as messages.
    #[test]
    fn replays_history() {
        let mut replay = Replay::default();
        // The second process's (after the live turn).
        let log = include_str!("../tests/fixtures/hermes-acp.jsonl");
        for line in log[log.find("=== second process").unwrap()..].lines() {
            let Some(body) = line.strip_prefix("<- ") else { continue };
            let msg: Value = serde_json::from_str(body).unwrap();
            if msg["method"] == "session/update" {
                replay.update(&msg["params"]["update"]);
            }
        }
        let roles: Vec<&str> = replay.messages.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["user", "assistant", "toolResult", "assistant", "toolResult", "assistant"]);
        assert_eq!(replay.messages[1]["content"][1]["type"], "toolCall");
        assert_eq!(replay.messages[4]["content"][0]["text"], "Exit code: 0\n\nOutput:\nhello.txt\n");
        assert!(replay.messages[5]["content"][1]["text"].as_str().unwrap().starts_with("Created `hello.txt`"));
    }
}
