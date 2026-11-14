//! MCP servers (the Model Context Protocol): their tools, offered to the
//! model beside harlo-agent's own.
//!
//! - Where: `mcp.json` in harlo-agent's config folder, and the project's
//!   `.harlo/mcp.json`, as `{ "mcpServers": { name: entry } }`. An entry is
//!   a command (`command`, `args`, `env`, `cwd`) or an address (`url`,
//!   `headers`; streamable HTTP, or the older HTTP+SSE with `type: sse`),
//!   with `timeout` (seconds, to start) and
//!   `enabled`. `${VAR}`, `${VAR:-default}` and a whole `!command` in a
//!   value are expanded. A project's server of the same name wins.
//!   `HARLO_AGENT_MCP_CONFIG` names a file to read instead of `mcp.json`.
//! - A project's server starts only when that entry, as it is, was
//!   approved for the project (`mcp-approved.json` beside `mcp.json`,
//!   which harlo writes when the user saves it): a repository can bring a
//!   file that would run a command.
//! - Started once per process, all at once, each within its timeout; one
//!   that fails is a note, not an error. Their tools are fixed while a
//!   prompt runs (the cache serves them); a server that says they changed
//!   is asked again at the next.
//! - A tool is `mcp__<server>__<tool>` (at most 64 characters). One the
//!   server marks read-only only looks; others change things (asked like
//!   bash, refused in plan mode).
//! - A server at an address that wants a sign-in: `harlo-agent mcp login
//!   <name>` (`mcp_auth`); its token is sent from then on.
//! - A command's server runs in its own process group, which goes with it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::future::join_all;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::oneshot;

use crate::tools::{Outcome, bash};

const PROTOCOL: &str = "2025-06-18";
/// To start, unless an entry says otherwise. (The first request waits for
/// every server, used or not.)
const START_TIMEOUT: Duration = Duration::from_secs(10);
/// A start that takes longer is said, naming the servers it waits for.
pub const SAID_AFTER: Duration = Duration::from_secs(2);
/// A tool call at most (Stop ends one sooner).
const CALL_TIMEOUT: Duration = Duration::from_secs(600);
/// The most of a result a program gets.
const WHOLE_MAX: usize = 4 << 20;
/// What tool names may be (OpenAI's limit).
const NAME_MAX: usize = 64;
pub const PREFIX: &str = "mcp__";
/// The tool that lists and reads servers' resources (offered when one has
/// them).
pub const RESOURCES: &str = "mcp_resources";
/// Many tools (their schemas past [`LISTED_PAST`]) aren't offered one by
/// one: the model finds them with this, then calls them with [`CALL`].
pub const FIND: &str = "mcp_tools";
pub const CALL: &str = "mcp_call";
/// Past this many bytes of tool schemas (about 3k tokens: 20 tools of a
/// usual size), they're listed by name and looked up, not sent whole with
/// every request (100 tools cost 11k tokens a request, 500 cost 54k).
/// `HARLO_AGENT_MCP_LISTED_PAST` changes it (0: always whole).
const LISTED_PAST: usize = 12_000;
/// How many tools a search gives back.
const FOUND_MAX: usize = 8;
/// How many tool names the instructions list (the rest: found by search).
const NAMES_MAX: usize = 400;

fn listed_past() -> usize {
    std::env::var("HARLO_AGENT_MCP_LISTED_PAST").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(LISTED_PAST)
}
/// Resources listed per server, at most.
const RESOURCES_LISTED: usize = 200;
/// A server's instructions to the model, at most (bytes).
const NOTE_MAX: usize = 2048;

/// A server as configured.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub name: String,
    pub kind: Kind,
    pub env: Vec<(String, String)>,
    pub cwd: Option<String>,
    pub timeout: Duration,
    /// The entry's `oauth` (`clientId`, `clientSecret`, `scopes`), for
    /// signing in (`mcp_auth`).
    pub oauth: Value,
    /// Tools you said only look (the entry's `readOnly`), besides those
    /// the server says do.
    pub read_only: ReadOnly,
    /// Tools not offered to the model (the entry's `hiddenTools`, the
    /// server's names for them).
    pub hidden: HashSet<String>,
}

/// The entry's `readOnly`: `true` (every tool) or the tools' names (as the
/// server calls them).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ReadOnly {
    #[default]
    None,
    All,
    Tools(HashSet<String>),
}

impl ReadOnly {
    fn from_entry(v: &Value) -> Self {
        match v {
            Value::Bool(true) => ReadOnly::All,
            Value::Array(a) => ReadOnly::Tools(a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect()),
            _ => ReadOnly::None,
        }
    }

    fn covers(&self, tool: &str) -> bool {
        match self {
            ReadOnly::None => false,
            ReadOnly::All => true,
            ReadOnly::Tools(t) => t.contains(tool),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Stdio { program: String, args: Vec<String> },
    Http { url: String, headers: Vec<(String, String)> },
    /// The older transport: a stream to GET, whose `endpoint` event says
    /// where to POST.
    Sse { url: String, headers: Vec<(String, String)> },
}

/// The servers configured for a project folder, and notes on those left
/// out (unreadable files, unapproved or unsupported entries).
pub fn load(project: &Path) -> (Vec<Config>, Vec<String>) {
    let dir = crate::auth::config_dir();
    let mut notes = Vec::new();
    // (`HARLO_AGENT_MCP_CONFIG`: another file instead, to try servers
    // without changing the settings.)
    let global = std::env::var_os("HARLO_AGENT_MCP_CONFIG").map(PathBuf::from).unwrap_or_else(|| dir.join("mcp.json"));
    let mut entries: Vec<(String, Value)> = read_servers(&global, &mut notes);
    let project_file = project.join(".harlo/mcp.json");
    let approved = approved_for(&read_json(&dir.join("mcp-approved.json")).unwrap_or(Value::Null), project);
    for (name, entry) in read_servers(&project_file, &mut notes) {
        if entry["enabled"] == false {
            entries.retain(|(n, _)| *n != name);
            continue;
        }
        if approved[&name].as_str() != Some(&entry_hash(&entry)) {
            notes.push(format!("The project's MCP server {name} wasn't started: it isn't one you approved for this project, as it is now (it's in {}). To let Harlo start it, open it in harlo's Settings → MCP for this project and save it.", project_file.display()));
            continue;
        }
        entries.retain(|(n, _)| *n != name);
        entries.push((name, entry));
    }
    let mut configs = Vec::new();
    for (name, entry) in entries {
        if entry["enabled"] == false {
            continue;
        }
        match parse(&name, &entry, project) {
            Ok(c) => configs.push(c),
            Err(e) => notes.push(format!("MCP server {name}: {e}")),
        }
    }
    (configs, notes)
}

/// A project's approvals (`{ name: hash }`): the project's key as harlo
/// wrote it, or the same folder by another path (a symlink).
fn approved_for(all: &Value, project: &Path) -> Value {
    let real = std::fs::canonicalize(project).ok();
    all.as_object()
        .into_iter()
        .flatten()
        .find(|(k, _)| Path::new(k) == project || (real.is_some() && std::fs::canonicalize(k).ok() == real))
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Null)
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn read_servers(path: &Path, notes: &mut Vec<String>) -> Vec<(String, Value)> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            notes.push(format!("Couldn't read {}: {e}", path.display()));
            return Vec::new();
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => v["mcpServers"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default(),
        Err(e) => {
            notes.push(format!("{} isn't valid JSON ({e}); its MCP servers weren't started.", path.display()));
            Vec::new()
        }
    }
}

/// What an entry's approval is kept by: its JSON, keys sorted, hashed.
pub fn entry_hash(entry: &Value) -> String {
    fn canonical(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), canonical(&m[k]))).collect::<Map<_, _>>())
            }
            Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
            v => v.clone(),
        }
    }
    let digest = Sha256::digest(canonical(entry).to_string().as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()))).collect()).unwrap_or_default()
}

/// An entry as a server to start (its values expanded).
fn parse(name: &str, v: &Value, project: &Path) -> Result<Config, String> {
    let ty = v["type"].as_str().unwrap_or_default();
    let url = v["url"].as_str().unwrap_or_default();
    let timeout = v["timeout"].as_f64().filter(|t| *t > 0.0).map(Duration::from_secs_f64).unwrap_or(START_TIMEOUT);
    let env = pairs(&v["env"]).into_iter().map(|(k, val)| (k, expand(&val))).collect();
    let cwd = v["cwd"].as_str().filter(|c| !c.trim().is_empty()).map(|c| {
        let c = expand(c.trim());
        let p = Path::new(&c);
        if p.is_absolute() { c } else { project.join(p).to_string_lossy().into_owned() }
    });
    let kind = if matches!(ty, "http" | "streamable-http" | "sse") || (ty.is_empty() && !url.is_empty() && v.get("command").is_none()) {
        let url = expand(url.trim());
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("its url isn't an http:// or https:// address.".into());
        }
        let headers = pairs(&v["headers"]).into_iter().map(|(k, val)| (k, expand(&val))).collect();
        if ty == "sse" { Kind::Sse { url, headers } } else { Kind::Http { url, headers } }
    } else {
        let mut words: Vec<String> = match &v["command"] {
            Value::String(c) => vec![c.clone()],
            Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
            _ => Vec::new(),
        };
        words.extend(v["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(str::to_owned)));
        let mut words = words.into_iter().map(|w| expand(&w));
        let Some(program) = words.next().filter(|p| !p.trim().is_empty()) else {
            return Err("it has no command (or url).".into());
        };
        Kind::Stdio { program, args: words.collect() }
    };
    Ok(Config { name: name.to_owned(), kind, env, cwd, timeout, oauth: v["oauth"].clone(), read_only: ReadOnly::from_entry(&v["readOnly"]), hidden: v["hiddenTools"].as_array().into_iter().flatten().filter_map(|t| t.as_str().map(str::to_owned)).collect() })
}

/// `${VAR}`, `${VAR:-default}` from the environment; a whole value of
/// `!command` is that command's output.
pub fn expand(value: &str) -> String {
    if let Some(cmd) = value.strip_prefix('!') {
        let out = std::process::Command::new("sh").arg("-c").arg(cmd).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output();
        return out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default();
    }
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &rest[start + 2..start + end];
        let (name, default) = inner.split_once(":-").unwrap_or((inner, ""));
        out.push_str(&std::env::var(name).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_owned()));
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// A tool's name for the model: `mcp__<server>__<tool>`, other characters
/// as `_`; too long, or taken already, cut and told apart by a hash.
pub fn tool_name(server: &str, tool: &str, taken: &HashSet<String>) -> String {
    let clean = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect::<String>();
    let name = format!("{PREFIX}{}__{}", clean(server), clean(tool));
    if name.len() <= NAME_MAX && !taken.contains(&name) {
        return name;
    }
    let hash: String = Sha256::digest(format!("{server}/{tool}").as_bytes()).iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{}_{hash}", &name[..name.len().min(NAME_MAX - 9)])
}

/// A tool's result as text for the model, whether it failed, and its
/// images (pi's blocks, fitted; the text says where each was).
pub fn result_text(result: &Value) -> (String, bool, Vec<Value>) {
    let mut parts = Vec::new();
    let mut images = Vec::new();
    let about = |data: &Value| data.as_str().map_or(0, |d| d.len() * 3 / 4).div_ceil(1024);
    let mut image = |data: &Value, mime: &str, parts: &mut Vec<String>| match crate::images::from_block(&json!({ "data": data })) {
        Ok(f) => {
            parts.push(format!("[image {}: {}]", images.len() + 1, f.describe()));
            images.push(f.block());
        }
        Err(why) => parts.push(format!("[image ({mime}, about {} KB): not shown, {why}]", about(data))),
    };
    for c in result["content"].as_array().into_iter().flatten() {
        let mime = c["mimeType"].as_str().unwrap_or("unknown type");
        match c["type"].as_str().unwrap_or_default() {
            "text" => parts.push(c["text"].as_str().unwrap_or_default().to_owned()),
            "image" => image(&c["data"], mime, &mut parts),
            "audio" => parts.push(format!("[audio ({mime}, about {} KB): not shown, Harlo doesn't take audio]", about(&c["data"]))),
            "resource" => {
                let r = &c["resource"];
                let mime = r["mimeType"].as_str().unwrap_or("binary");
                match r["text"].as_str() {
                    Some(t) => parts.push(t.to_owned()),
                    None if mime.starts_with("image/") && r["blob"].is_string() => image(&r["blob"], mime, &mut parts),
                    None => parts.push(format!("[resource {} ({mime})]", r["uri"].as_str().unwrap_or_default())),
                }
            }
            "resource_link" => parts.push(format!("[link: {} {}]", c["uri"].as_str().unwrap_or_default(), c["name"].as_str().unwrap_or_default()).trim_end().to_owned()),
            other => parts.push(format!("[{other} content]")),
        }
    }
    if parts.is_empty()
        && let Some(s) = result.get("structuredContent")
    {
        parts.push(s.to_string());
    }
    let failed = result["isError"] == true;
    let text = parts.join("\n");
    (if text.trim().is_empty() { if failed { "The tool failed and said nothing more.".into() } else { "(no output)".into() } } else { text }, failed, images)
}

// ------------------------------------------------------------ connections

/// Process groups of command servers running, to end at exit.
static GROUPS: Mutex<Vec<i32>> = Mutex::new(Vec::new());

/// Ends every command server (before the process exits).
pub fn stop_all() {
    let groups: Vec<i32> = std::mem::take(&mut *GROUPS.lock().unwrap());
    if groups.is_empty() {
        return;
    }
    for g in &groups {
        // SAFETY: signalling process groups this process started.
        unsafe { libc::killpg(*g, libc::SIGTERM) };
    }
    let alive = |g: i32| unsafe { libc::killpg(g, 0) } == 0;
    for _ in 0..20 {
        if !groups.iter().any(|g| alive(*g)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    for g in groups {
        unsafe { libc::killpg(g, libc::SIGKILL) };
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

/// A command server: JSON-RPC a line each way.
struct Stdio {
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    pending: Pending,
    next: AtomicU64,
    group: i32,
    /// Its last lines of stderr, to say why it stopped.
    stderr: Arc<Mutex<VecDeque<String>>>,
    gone: Arc<AtomicBool>,
    _child: tokio::process::Child,
}

impl Drop for Stdio {
    fn drop(&mut self) {
        GROUPS.lock().unwrap().retain(|g| *g != self.group);
        // SAFETY: its own process group.
        unsafe { libc::killpg(self.group, libc::SIGTERM) };
    }
}

async fn write_line(stdin: &tokio::sync::Mutex<ChildStdin>, v: &Value) -> std::io::Result<()> {
    let mut line = v.to_string();
    line.push('\n');
    let mut s = stdin.lock().await;
    s.write_all(line.as_bytes()).await?;
    s.flush().await
}

/// A request's answer to a server's own request.
fn answer(msg: &Value, project: &Path) -> Value {
    let result = match msg["method"].as_str().unwrap_or_default() {
        "ping" => json!({}),
        "roots/list" => json!({ "roots": [{ "uri": format!("file://{}", project.display()), "name": project.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() }] }),
        m => return json!({ "jsonrpc": "2.0", "id": msg["id"], "error": { "code": -32601, "message": format!("harlo-agent doesn't do {m}") } }),
    };
    json!({ "jsonrpc": "2.0", "id": msg["id"], "result": result })
}

impl Stdio {
    fn start(c: &Config, program: &str, args: &[String], project: &Path, changed: Arc<AtomicBool>) -> Result<Self, String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .envs(c.env.iter().map(|(k, v)| (k, v)))
            .current_dir(c.cwd.as_deref().map(PathBuf::from).unwrap_or_else(|| project.to_owned()))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("couldn't start {program}: {e}"))?;
        let group = child.id().map_or(0, |p| p as i32);
        GROUPS.lock().unwrap().push(group);
        let stdin = Arc::new(tokio::sync::Mutex::new(child.stdin.take().unwrap()));
        let stdout = child.stdout.take().unwrap();
        let mut err = child.stderr.take().unwrap();
        let pending: Pending = Arc::default();
        let stderr: Arc<Mutex<VecDeque<String>>> = Arc::default();
        let gone = Arc::new(AtomicBool::new(false));
        {
            let stderr = stderr.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let _ = err.read_to_end(&mut buf).await.ok();
                let text = String::from_utf8_lossy(&buf);
                let mut s = stderr.lock().unwrap();
                for l in text.lines().filter(|l| !l.trim().is_empty()) {
                    if s.len() == 5 {
                        s.pop_front();
                    }
                    s.push_back(l.chars().take(300).collect());
                }
            });
        }
        {
            let (pending, stdin, gone, project) = (pending.clone(), stdin.clone(), gone.clone(), project.to_owned());
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                    let id = msg["id"].as_u64();
                    match (msg.get("method"), id) {
                        // An answer.
                        (None, Some(id)) => {
                            if let Some(tx) = pending.lock().unwrap().remove(&id) {
                                let _ = tx.send(msg);
                            }
                        }
                        // The server asking.
                        (Some(_), _) if msg.get("id").is_some() => {
                            let _ = write_line(&stdin, &answer(&msg, &project)).await;
                        }
                        (Some(m), None) if m == "notifications/tools/list_changed" => changed.store(true, Ordering::Relaxed),
                        _ => {}
                    }
                }
                gone.store(true, Ordering::Relaxed);
                // (Those waiting hear it's gone.)
                pending.lock().unwrap().clear();
            });
        }
        Ok(Stdio { stdin, pending, next: AtomicU64::new(1), group, stderr, gone, _child: child })
    }

    fn why_gone(&self) -> String {
        let tail: Vec<String> = self.stderr.lock().unwrap().iter().cloned().collect();
        if tail.is_empty() { "it stopped".into() } else { format!("it stopped: {}", tail.join(" / ")) }
    }

    async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        if self.gone.load(Ordering::Relaxed) {
            return Err(self.why_gone());
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        // Dropped before its answer (Stop): the server is told.
        let mut guard = CancelOnDrop { stdin: Some(self.stdin.clone()), pending: self.pending.clone(), id };
        write_line(&self.stdin, &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).await.map_err(|_| self.why_gone())?;
        let answer = tokio::time::timeout(timeout, rx).await;
        guard.stdin = None;
        self.pending.lock().unwrap().remove(&id);
        match answer {
            Err(_) => Err(format!("no answer to {method} in {} s", timeout.as_secs())),
            Ok(Err(_)) => {
                // (Its last words may still be coming.)
                tokio::time::sleep(Duration::from_millis(100)).await;
                Err(self.why_gone())
            }
            Ok(Ok(msg)) => match msg.get("error") {
                Some(e) => Err(e["message"].as_str().unwrap_or("an error").to_owned()),
                None => Ok(msg["result"].clone()),
            },
        }
    }

    async fn notify(&self, method: &str) {
        let _ = write_line(&self.stdin, &json!({ "jsonrpc": "2.0", "method": method })).await;
    }
}

struct CancelOnDrop {
    stdin: Option<Arc<tokio::sync::Mutex<ChildStdin>>>,
    pending: Pending,
    id: u64,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        let Some(stdin) = self.stdin.take() else { return };
        self.pending.lock().unwrap().remove(&self.id);
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": self.id, "reason": "stopped" } });
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(async move {
                let _ = write_line(&stdin, &note).await;
            });
        }
    }
}

/// A server at an address (streamable HTTP).
struct Http {
    client: reqwest::Client,
    name: String,
    url: String,
    headers: Vec<(String, String)>,
    session: Mutex<Option<String>>,
    protocol: Mutex<String>,
    next: AtomicU64,
    changed: Arc<AtomicBool>,
}

impl Http {
    fn new(name: &str, url: &str, headers: &[(String, String)], changed: Arc<AtomicBool>) -> Result<Self, String> {
        let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
        Ok(Http { client, name: name.to_owned(), url: url.to_owned(), headers: headers.to_vec(), session: Mutex::new(None), protocol: Mutex::new(PROTOCOL.into()), next: AtomicU64::new(1), changed })
    }

    /// Sends a message; the answer to it (None for a notification). With a
    /// sign-in, its token goes along (refreshed once if it's refused).
    async fn post(&self, body: &Value, timeout: Duration) -> Result<Option<Value>, String> {
        let mut force = false;
        loop {
            let token = signed_in(&self.url, &self.headers, force).await;
            match self.post_with(body, timeout, token.as_deref()).await {
                Err(Fail::Refused(_)) if token.is_some() && !force => force = true,
                Err(Fail::Refused(status)) => return Err(signin_needed(&self.name, &self.url, status, token.is_some())),
                Err(Fail::Other(e)) => return Err(e),
                Ok(r) => return Ok(r),
            }
        }
    }

    async fn post_with(&self, body: &Value, timeout: Duration, token: Option<&str>) -> Result<Option<Value>, Fail> {
        let mut req = self
            .client
            .post(&self.url)
            .timeout(timeout)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", self.protocol.lock().unwrap().clone());
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(s) = self.session.lock().unwrap().clone() {
            req = req.header("Mcp-Session-Id", s);
        }
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let resp = req.body(body.to_string()).send().await.map_err(|e| format!("couldn't reach {}: {}", self.url, root_cause(&e)))?;
        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            return Err(Fail::Refused(status));
        }
        if let Some(s) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *self.session.lock().unwrap() = Some(s.to_owned());
        }
        if body.get("id").is_none() {
            return Ok(None);
        }
        let stream = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|c| c.starts_with("text/event-stream"));
        if !(200..300).contains(&status) {
            let text = resp.text().await.unwrap_or_default();
            let detail: String = text.trim().chars().take(200).collect();
            return Err(format!("HTTP {status} from {}{}", self.url, if detail.is_empty() { String::new() } else { format!(": {detail}") }).into());
        }
        let mine = |v: &Value| v["id"] == body["id"] && v.get("method").is_none();
        if !stream {
            let v: Value = resp.json().await.map_err(|e| format!("its answer wasn't JSON: {e}"))?;
            return Ok(Some(v));
        }
        // An event stream: read until the answer comes (it may stay open).
        let mut bytes = resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.map_err(|e| format!("reading its answer: {}", root_cause(&e)))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            buf = buf.replace("\r\n", "\n");
            while let Some(end) = buf.find("\n\n") {
                let event: String = buf[..end].to_owned();
                buf.drain(..end + 2);
                let data: Vec<&str> = event.lines().filter_map(|l| l.strip_prefix("data:")).map(|d| d.strip_prefix(' ').unwrap_or(d)).collect();
                let Ok(v) = serde_json::from_str::<Value>(&data.join("\n")) else { continue };
                if mine(&v) {
                    return Ok(Some(v));
                }
                if v["method"] == "notifications/tools/list_changed" {
                    self.changed.store(true, Ordering::Relaxed);
                }
            }
        }
        Err(Fail::Other("its answer ended before it said anything".into()))
    }

    async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let msg = self.post(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }), timeout).await?.unwrap_or_default();
        match msg.get("error") {
            Some(e) => Err(e["message"].as_str().unwrap_or("an error").to_owned()),
            None => Ok(msg["result"].clone()),
        }
    }

    async fn notify(&self, method: &str) {
        let _ = self.post(&json!({ "jsonrpc": "2.0", "method": method }), Duration::from_secs(15)).await;
    }
}

/// A server on the older HTTP+SSE transport: one stream it keeps open
/// (answers and its own requests come there), and an address, from its
/// first `endpoint` event, that messages are POSTed to.
struct Sse {
    client: reqwest::Client,
    name: String,
    url: String,
    /// The sign-in's token, when there is one.
    token: Mutex<Option<String>>,
    headers: Vec<(String, String)>,
    /// Where to POST, once the stream said.
    endpoint: tokio::sync::watch::Receiver<Option<String>>,
    pending: Pending,
    next: AtomicU64,
    gone: Arc<Mutex<Option<String>>>,
    reader: tokio::task::JoinHandle<()>,
}

impl Drop for Sse {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

/// An SSE stream's events, as (event, data), from its bytes.
fn sse_events(buf: &mut String) -> Vec<(String, String)> {
    let mut out = Vec::new();
    *buf = buf.replace("\r\n", "\n");
    while let Some(end) = buf.find("\n\n") {
        let block: String = buf.drain(..end + 2).collect();
        let mut event = "message".to_owned();
        let mut data = Vec::new();
        for l in block.lines() {
            if let Some(e) = l.strip_prefix("event:") {
                event = e.trim().to_owned();
            } else if let Some(d) = l.strip_prefix("data:") {
                data.push(d.strip_prefix(' ').unwrap_or(d).to_owned());
            }
        }
        if !data.is_empty() {
            out.push((event, data.join("\n")));
        }
    }
    out
}

impl Sse {
    async fn start(name: &str, url: &str, headers: &[(String, String)], project: &Path, changed: Arc<AtomicBool>) -> Result<Self, String> {
        let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
        let mut force = false;
        let (resp, token) = loop {
            let token = signed_in(url, headers, force).await;
            let mut req = client.get(url).header("Accept", "text/event-stream");
            for (k, v) in headers {
                req = req.header(k.as_str(), v.as_str());
            }
            if let Some(t) = &token {
                req = req.bearer_auth(t);
            }
            let resp = req.send().await.map_err(|e| format!("couldn't reach {url}: {}", root_cause(&e)))?;
            let status = resp.status().as_u16();
            if status == 401 || status == 403 {
                if token.is_some() && !force {
                    force = true;
                    continue;
                }
                return Err(signin_needed(name, url, status, token.is_some()));
            }
            break (resp, token);
        };
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status} from {url}"));
        }
        let base = resp.url().clone();
        let (tx, endpoint) = tokio::sync::watch::channel(None);
        let pending: Pending = Arc::default();
        let gone: Arc<Mutex<Option<String>>> = Arc::default();
        let reader = {
            let (pending, gone, client, headers, project, bearer) = (pending.clone(), gone.clone(), client.clone(), headers.to_vec(), project.to_owned(), token.clone());
            tokio::spawn(async move {
                let mut bytes = resp.bytes_stream();
                let mut buf = String::new();
                let why = loop {
                    let Some(chunk) = bytes.next().await else { break "its stream ended".to_owned() };
                    let chunk = match chunk {
                        Ok(c) => c,
                        Err(e) => break format!("its stream broke: {}", root_cause(&e)),
                    };
                    buf.push_str(&String::from_utf8_lossy(&chunk));
                    for (event, data) in sse_events(&mut buf) {
                        if event == "endpoint" {
                            if let Ok(u) = base.join(data.trim()) {
                                let _ = tx.send(Some(u.to_string()));
                            }
                            continue;
                        }
                        let Ok(msg) = serde_json::from_str::<Value>(&data) else { continue };
                        match (msg.get("method"), msg["id"].as_u64()) {
                            (None, Some(id)) => {
                                if let Some(tx) = pending.lock().unwrap().remove(&id) {
                                    let _ = tx.send(msg);
                                }
                            }
                            // The server asking: answered by a POST.
                            (Some(_), _) if msg.get("id").is_some() => {
                                let to = tx.borrow().clone();
                                if let Some(to) = to {
                                    let mut r = client.post(to).header("Content-Type", "application/json");
                                    for (k, v) in &headers {
                                        r = r.header(k.as_str(), v.as_str());
                                    }
                                    if let Some(t) = &bearer {
                                        r = r.bearer_auth(t);
                                    }
                                    let _ = r.body(answer(&msg, &project).to_string()).send().await;
                                }
                            }
                            (Some(m), None) if m == "notifications/tools/list_changed" => changed.store(true, Ordering::Relaxed),
                            _ => {}
                        }
                    }
                };
                *gone.lock().unwrap() = Some(why);
                // (Those waiting hear it's gone.)
                pending.lock().unwrap().clear();
            })
        };
        Ok(Sse { client, name: name.to_owned(), url: url.to_owned(), token: Mutex::new(token), headers: headers.to_vec(), endpoint, pending, next: AtomicU64::new(1), gone, reader })
    }

    fn why_gone(&self) -> Option<String> {
        self.gone.lock().unwrap().clone()
    }

    /// Where to POST: once the stream has said (within `timeout`).
    async fn endpoint(&self, timeout: Duration) -> Result<String, String> {
        let mut rx = self.endpoint.clone();
        let got = tokio::time::timeout(timeout, rx.wait_for(Option::is_some)).await;
        match got {
            Ok(Ok(v)) => Ok(v.clone().unwrap_or_default()),
            Ok(Err(_)) => Err(self.why_gone().unwrap_or_else(|| "its stream ended".into())),
            Err(_) => Err(format!("{} didn't say where to send messages within {} s", self.url, timeout.as_secs())),
        }
    }

    async fn post(&self, body: &Value, timeout: Duration) -> Result<(), String> {
        let to = self.endpoint(timeout).await?;
        let mut req = self.client.post(&to).timeout(timeout.min(Duration::from_secs(60))).header("Content-Type", "application/json");
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let token = self.token.lock().unwrap().clone();
        if let Some(t) = &token {
            req = req.bearer_auth(t);
        }
        let resp = req.body(body.to_string()).send().await.map_err(|e| format!("couldn't reach {to}: {}", root_cause(&e)))?;
        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            // (A token refreshed meanwhile, for the next message.)
            if token.is_some() {
                *self.token.lock().unwrap() = signed_in(&self.url, &self.headers, true).await;
            }
            return Err(signin_needed(&self.name, &self.url, status, token.is_some()));
        }
        if !(200..300).contains(&status) {
            let text = resp.text().await.unwrap_or_default();
            let detail: String = text.trim().chars().take(200).collect();
            return Err(format!("HTTP {status} from {to}{}", if detail.is_empty() { String::new() } else { format!(": {detail}") }));
        }
        Ok(())
    }

    async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        if let Some(why) = self.why_gone() {
            return Err(why);
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        // (Dropped before its answer: no longer waited for.)
        struct Forget(Pending, u64);
        impl Drop for Forget {
            fn drop(&mut self) {
                self.0.lock().unwrap().remove(&self.1);
            }
        }
        let _forget = Forget(self.pending.clone(), id);
        self.post(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }), timeout).await?;
        match tokio::time::timeout(timeout, rx).await {
            Err(_) => Err(format!("no answer to {method} in {} s", timeout.as_secs())),
            Ok(Err(_)) => Err(self.why_gone().unwrap_or_else(|| "its stream ended".into())),
            Ok(Ok(msg)) => match msg.get("error") {
                Some(e) => Err(e["message"].as_str().unwrap_or("an error").to_owned()),
                None => Ok(msg["result"].clone()),
            },
        }
    }

    async fn notify(&self, method: &str) {
        let _ = self.post(&json!({ "jsonrpc": "2.0", "method": method }), Duration::from_secs(15)).await;
    }
}

/// Why a message didn't get through.
enum Fail {
    /// Refused for want of a sign-in (401, 403).
    Refused(u16),
    Other(String),
}

impl From<String> for Fail {
    fn from(s: String) -> Self {
        Fail::Other(s)
    }
}

/// The sign-in's token for `url`, unless the entry sends its own
/// Authorization header.
async fn signed_in(url: &str, headers: &[(String, String)], force: bool) -> Option<String> {
    if headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("authorization")) {
        return None;
    }
    crate::mcp_auth::bearer(url, force).await
}

/// What a server that wants a sign-in is refused with.
fn signin_needed(name: &str, url: &str, status: u16, had: bool) -> String {
    let what = if had { "refused the sign-in" } else { "wants you to sign in" };
    format!("{url} {what} (HTTP {status}): sign in with `harlo-agent mcp login {name}`, or Sign in under harlo's Settings → MCP.")
}

fn root_cause(e: &(dyn std::error::Error + 'static)) -> String {
    let mut cur = e;
    while let Some(next) = cur.source() {
        cur = next;
    }
    cur.to_string()
}

enum Conn {
    Stdio(Stdio),
    Http(Http),
    Sse(Sse),
}

impl Conn {
    async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        match self {
            Conn::Stdio(s) => s.request(method, params, timeout).await,
            Conn::Http(h) => h.request(method, params, timeout).await,
            Conn::Sse(e) => e.request(method, params, timeout).await,
        }
    }

    async fn notify(&self, method: &str) {
        match self {
            Conn::Stdio(s) => s.notify(method).await,
            Conn::Http(h) => h.notify(method).await,
            Conn::Sse(e) => e.notify(method).await,
        }
    }
}

struct Server {
    name: String,
    conn: Conn,
    tools: Vec<Value>,
    changed: Arc<AtomicBool>,
    timeout: Duration,
    read_only: ReadOnly,
    /// It offers resources (`resources/list`, `resources/read`).
    resources: bool,
    hidden: HashSet<String>,
    /// What it says about using it (`initialize`'s `instructions`), cut at
    /// [`NOTE_MAX`].
    note: String,
}

async fn list_tools(conn: &Conn, timeout: Duration) -> Result<Vec<Value>, String> {
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..50 {
        let params = match &cursor {
            Some(c) => json!({ "cursor": c }),
            None => json!({}),
        };
        let result = conn.request("tools/list", params, timeout).await?;
        tools.extend(result["tools"].as_array().into_iter().flatten().filter(|t| t["name"].as_str().is_some_and(|n| !n.is_empty())).cloned());
        cursor = result["nextCursor"].as_str().filter(|c| !c.is_empty()).map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    Ok(tools)
}

async fn start(c: Config, project: PathBuf) -> Result<Server, String> {
    let changed = Arc::new(AtomicBool::new(false));
    let conn = match &c.kind {
        Kind::Stdio { program, args } => Conn::Stdio(Stdio::start(&c, program, args, &project, changed.clone())?),
        Kind::Http { url, headers } => Conn::Http(Http::new(&c.name, url, headers, changed.clone())?),
        Kind::Sse { url, headers } => Conn::Sse(tokio::time::timeout(c.timeout, Sse::start(&c.name, url, headers, &project, changed.clone())).await.map_err(|_| format!("it didn't answer within {} s", c.timeout.as_secs()))??),
    };
    let init = json!({ "protocolVersion": PROTOCOL, "capabilities": { "roots": {} }, "clientInfo": { "name": "harlo-agent", "version": env!("CARGO_PKG_VERSION") } });
    // (The whole start within its timeout, said as that.)
    let ready = async {
        let r = conn.request("initialize", init, CALL_TIMEOUT).await?;
        if let (Conn::Http(h), Some(v)) = (&conn, r["protocolVersion"].as_str()) {
            *h.protocol.lock().unwrap() = v.to_owned();
        }
        conn.notify("notifications/initialized").await;
        let caps = &r["capabilities"];
        let resources = caps["resources"].is_object();
        // (One with resources only has no tools to list.)
        let tools = if resources && !caps["tools"].is_object() { Vec::new() } else { list_tools(&conn, CALL_TIMEOUT).await? };
        let note = r["instructions"].as_str().unwrap_or_default().trim();
        let note = if note.len() > NOTE_MAX { format!("{}…", &note[..note.floor_char_boundary(NOTE_MAX)]) } else { note.to_owned() };
        Ok::<_, String>((tools, resources, note))
    };
    let (tools, resources, note) = tokio::time::timeout(c.timeout, ready).await.map_err(|_| format!("it didn't answer within {} s", c.timeout.as_secs()))??;
    Ok(Server { name: c.name, conn, tools, changed, timeout: c.timeout, read_only: c.read_only, resources, hidden: c.hidden, note })
}

/// The servers running, and their tools as the model has them.
#[derive(Default)]
pub struct Mcp {
    servers: Vec<Server>,
    /// Model's name → (server, the server's name for it).
    by_name: HashMap<String, (usize, String)>,
    read_only: HashSet<String>,
    specs: Vec<Value>,
    /// Every tool's spec, as it'd be offered (the same as `specs` unless
    /// they're listed).
    all: Vec<Value>,
    /// Too many to offer whole: listed by name, found with [`FIND`] and
    /// called with [`CALL`].
    listed: bool,
}

impl Mcp {
    /// Starts `configs` (all at once); notes on those that didn't.
    /// `pending`: the ones not started yet, with their timeouts.
    pub async fn start(configs: Vec<Config>, project: &Path, pending: &Mutex<Vec<(String, Duration)>>) -> (Mcp, Vec<String>) {
        let names: Vec<String> = configs.iter().map(|c| c.name.clone()).collect();
        *pending.lock().unwrap() = configs.iter().map(|c| (c.name.clone(), c.timeout)).collect();
        let started = join_all(configs.into_iter().map(|c| async move {
            let name = c.name.clone();
            let s = start(c, project.to_owned()).await;
            pending.lock().unwrap().retain(|(n, _)| *n != name);
            s
        }))
        .await;
        let mut mcp = Mcp::default();
        let mut notes = Vec::new();
        for (name, s) in names.into_iter().zip(started) {
            match s {
                Ok(s) => mcp.servers.push(s),
                Err(e) => notes.push(format!("MCP server {name} didn't start: {e}")),
            }
        }
        mcp.index();
        (mcp, notes)
    }

    /// What the first request waits for, after `SAID_AFTER`: the servers
    /// still starting (None: none).
    pub fn waiting_for(pending: &[(String, Duration)]) -> Option<String> {
        let most = pending.iter().map(|(_, t)| *t).max()?.saturating_sub(SAID_AFTER).as_secs_f64().ceil();
        let names: Vec<&str> = pending.iter().map(|(n, _)| n.as_str()).collect();
        let s = if names.len() == 1 { "" } else { "s" };
        Some(format!("Waiting for MCP server{s} {} to start (up to {most} s more).", names.join(", ")))
    }

    fn index(&mut self) {
        self.by_name.clear();
        self.read_only.clear();
        self.specs.clear();
        self.all.clear();
        let mut taken = HashSet::new();
        for (i, s) in self.servers.iter().enumerate() {
            for t in &s.tools {
                let tool = t["name"].as_str().unwrap_or_default();
                if s.hidden.contains(tool) {
                    continue;
                }
                let name = tool_name(&s.name, tool, &taken);
                taken.insert(name.clone());
                let mut parameters = t["inputSchema"].clone();
                if !parameters.is_object() {
                    parameters = json!({ "type": "object", "properties": {} });
                }
                if parameters.get("type").is_none() {
                    parameters["type"] = json!("object");
                }
                let description = t["description"].as_str().or(t["title"].as_str()).unwrap_or_default().trim();
                self.all.push(json!({ "type": "function", "name": name, "description": format!("(MCP server {}) {description}", s.name).trim_end(), "parameters": parameters, "strict": false }));
                if t["annotations"]["readOnlyHint"] == true || s.read_only.covers(tool) {
                    self.read_only.insert(name.clone());
                }
                self.by_name.insert(name, (i, tool.to_owned()));
            }
        }
        let past = listed_past();
        self.listed = past > 0 && serde_json::to_string(&self.all).map_or(0, |t| t.len()) > past;
        if self.listed {
            self.specs.push(json!({
                "type": "function", "name": FIND, "strict": false,
                "description": "Find MCP tools (the servers' tools named in the instructions): what each does and its parameters, to call it with mcp_call. Search by what it should do or by its name.",
                "parameters": { "type": "object", "properties": {
                    "query": { "type": "string", "description": "Words of what it does, or its name." },
                    "server": { "type": "string", "description": "Only this server's (with no query: its tools)." },
                }, "additionalProperties": false },
            }));
            self.specs.push(json!({
                "type": "function", "name": CALL, "strict": false,
                "description": "Call an MCP tool found with mcp_tools: its name, and its arguments as its parameters say.",
                "parameters": { "type": "object", "properties": {
                    "name": { "type": "string" },
                    "arguments": { "type": "object" },
                }, "required": ["name"], "additionalProperties": false },
            }));
        } else {
            self.specs.extend(self.all.iter().cloned());
        }
        let with: Vec<&str> = self.servers.iter().filter(|s| s.resources).map(|s| s.name.as_str()).collect();
        if !with.is_empty() {
            self.specs.push(json!({
                "type": "function", "name": RESOURCES, "strict": false,
                "description": "List or read what MCP servers offer as resources (documents, files, records they expose by URI). Without uri: a server's resources (URIs, names, types) and the URI templates it fills in. With uri: that resource's contents.",
                "parameters": { "type": "object", "properties": {
                    "server": { "type": "string", "enum": with },
                    "uri": { "type": "string" },
                }, "additionalProperties": false },
            }));
        }
    }

    pub fn specs(&self) -> &[Value] {
        &self.specs
    }

    /// Every tool's spec, listed or not (what a program may call).
    pub fn all_specs(&self) -> &[Value] {
        &self.all
    }

    /// Whether the tools are listed by name rather than offered whole.
    pub fn listed(&self) -> bool {
        self.listed
    }

    /// A [`CALL`] as the call of the tool it names: (its name, its
    /// arguments). None for any other call, or a name no tool has.
    pub fn resolve(&self, name: &str, args: &Value) -> Option<(String, Value)> {
        if name != CALL || !self.listed {
            return None;
        }
        let asked = args["name"].as_str()?.trim();
        let found = self.find_name(asked)?;
        let arguments = match &args["arguments"] {
            Value::Object(_) => args["arguments"].clone(),
            // (Some models send the arguments as a JSON string.)
            Value::String(t) => serde_json::from_str::<Value>(t).ok().filter(Value::is_object).unwrap_or_else(|| json!({})),
            _ => json!({}),
        };
        Some((found, arguments))
    }

    /// A tool by its full name, its own name (if only one server has it),
    /// or server/tool.
    fn find_name(&self, asked: &str) -> Option<String> {
        if self.by_name.contains_key(asked) {
            return Some(asked.to_owned());
        }
        let split = asked.split_once('/').or_else(|| asked.split_once("__").filter(|(s, _)| self.servers.iter().any(|v| v.name == *s)));
        let mut hits = self.by_name.iter().filter(|(_, (i, tool))| match split {
            Some((server, t)) => self.servers[*i].name == server && tool == t,
            None => tool == asked,
        });
        let first = hits.next()?.0.clone();
        hits.next().is_none().then_some(first)
    }

    /// [`FIND`]: the tools whose name or description has the query's
    /// words (most first), whole, as a model calls them.
    fn find(&self, args: &Value) -> Outcome {
        let query = args["query"].as_str().unwrap_or_default().to_lowercase();
        let server = args["server"].as_str().map(str::trim).filter(|s| !s.is_empty());
        if query.trim().is_empty() && server.is_none() {
            return Outcome::new("Say what the tool should do in query (or name a server).", true);
        }
        let words: Vec<&str> = query.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 1).collect();
        let mut scored: Vec<(usize, &Value)> = self
            .all
            .iter()
            .filter(|t| server.is_none_or(|s| self.by_name.get(t["name"].as_str().unwrap_or_default()).is_some_and(|(i, _)| self.servers[*i].name == s)))
            .filter_map(|t| {
                let name = t["name"].as_str().unwrap_or_default().to_lowercase();
                let about = t["description"].as_str().unwrap_or_default().to_lowercase();
                if words.is_empty() {
                    return Some((0, t));
                }
                let score: usize = words.iter().map(|w| if name.contains(w) { 3 } else if about.contains(w) { 1 } else { 0 }).sum();
                let exact = name == query.trim() || name.ends_with(&format!("__{}", query.trim()));
                (score > 0 || exact).then_some((score + if exact { 100 } else { 0 }, t))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1["name"].as_str().cmp(&b.1["name"].as_str())));
        if scored.is_empty() {
            return Outcome::new("No MCP tool matches. Try other words, or a server's name with no query to see its tools.", false);
        }
        let total = scored.len();
        let shown: Vec<String> = scored
            .iter()
            .take(FOUND_MAX)
            .map(|(_, t)| json!({ "name": t["name"], "description": t["description"], "parameters": t["parameters"] }).to_string())
            .collect();
        let more = if total > FOUND_MAX { format!("\n[{total} match; the first {FOUND_MAX} shown. Narrow the query for the rest.]") } else { String::new() };
        Outcome::new(format!("{}{more}", shown.join("\n")), false)
    }

    /// What the servers say about using them, for the instructions ("" when
    /// none says anything).
    pub fn notes(&self) -> String {
        let mut out = String::new();
        for s in self.servers.iter().filter(|s| !s.note.is_empty()) {
            out.push_str(&format!("\n<mcp_server name=\"{}\">\n{}\n</mcp_server>\n", s.name, s.note));
        }
        if self.listed {
            // (Their names only, by server: what each does comes with a search.)
            let mut by_server: Vec<(&str, Vec<&str>)> = Vec::new();
            let mut names: Vec<(&usize, &str)> = self.by_name.values().map(|(i, t)| (i, t.as_str())).collect();
            names.sort();
            for (i, tool) in names {
                let server = self.servers[*i].name.as_str();
                match by_server.iter_mut().find(|(s, _)| *s == server) {
                    Some((_, list)) => list.push(tool),
                    None => by_server.push((server, vec![tool])),
                }
            }
            out.push_str("\n<mcp_tools>\nThe MCP servers' tools aren't in your tool list: find one with mcp_tools (what it does and its parameters), then call it with mcp_call. They are:");
            let mut left = NAMES_MAX;
            for (server, list) in &by_server {
                let shown = list.len().min(left);
                left -= shown;
                let more = if shown < list.len() { format!(" (and {} more)", list.len() - shown) } else { String::new() };
                out.push_str(&format!("\n- {server}: {}{more}", list[..shown].join(", ")));
            }
            out.push_str("\n</mcp_tools>\n");
        }
        out
    }

    pub fn has(&self, name: &str) -> bool {
        self.by_name.contains_key(name) || (name == RESOURCES && self.servers.iter().any(|s| s.resources)) || (self.listed && (name == FIND || name == CALL))
    }

    /// Whether any server's tools only look.
    pub fn has_read_only(&self) -> bool {
        !self.read_only.is_empty() || self.has(RESOURCES)
    }

    /// Whether the tool only looks: its server says so, or its entry does.
    pub fn read_only(&self, name: &str) -> bool {
        // (A CALL that names a tool was made that tool's call already: one
        // left names none, and only fails.)
        self.read_only.contains(name) || (name == RESOURCES && self.has(RESOURCES)) || (self.listed && (name == FIND || name == CALL))
    }

    /// Lists again the tools of servers that said they changed.
    pub async fn refresh(&mut self) -> Vec<String> {
        let mut notes = Vec::new();
        let mut any = false;
        for s in &mut self.servers {
            if !s.changed.swap(false, Ordering::Relaxed) {
                continue;
            }
            match list_tools(&s.conn, s.timeout).await {
                Ok(t) => {
                    s.tools = t;
                    any = true;
                }
                Err(e) => notes.push(format!("MCP server {}: couldn't list its tools again: {e}", s.name)),
            }
        }
        if any {
            self.index();
        }
        notes
    }

    pub async fn call(&self, name: &str, args: &Value, spill_dir: &Path) -> Outcome {
        self.call_as(name, args, Some(spill_dir)).await
    }

    /// A call for a program (`run_code`): the whole text, up to
    /// [`WHOLE_MAX`] bytes, since the program reads it rather than the model.
    pub async fn call_whole(&self, name: &str, args: &Value) -> Outcome {
        self.call_as(name, args, None).await
    }

    async fn call_as(&self, name: &str, args: &Value, spill_dir: Option<&Path>) -> Outcome {
        if name == RESOURCES && self.has(RESOURCES) {
            return self.resources(args, spill_dir).await;
        }
        if name == FIND && self.listed {
            return self.find(args);
        }
        // (A call of a tool that's there was made that tool's call before it
        // got here: `resolve`.)
        if name == CALL && self.listed {
            let asked = args["name"].as_str().unwrap_or_default();
            return Outcome::new(format!("No MCP tool {asked}: find it with {FIND} and use the name it gives."), true);
        }
        let Some((i, tool)) = self.by_name.get(name) else {
            return Outcome::new(format!("No MCP tool {name}."), true);
        };
        let s = &self.servers[*i];
        let args = if args.is_object() { args.clone() } else { json!({}) };
        match s.conn.request("tools/call", json!({ "name": tool, "arguments": args }), CALL_TIMEOUT).await {
            Ok(result) => {
                let (text, failed, images) = result_text(&result);
                let text = match spill_dir {
                    Some(dir) => bash::shorten(&text, text.len(), dir),
                    None if text.len() > WHOLE_MAX => format!("{}\n[Cut at {} MB of {} bytes]", &text[..text.floor_char_boundary(WHOLE_MAX)], WHOLE_MAX >> 20, text.len()),
                    None => text,
                };
                let mut out = Outcome::new(text, failed);
                out.images = images;
                out
            }
            Err(e) => Outcome::new(format!("MCP server {}: {e}", s.name), true),
        }
    }
}

impl Mcp {
    /// `mcp_resources`: a server's resources listed, or one read.
    async fn resources(&self, args: &Value, spill_dir: Option<&Path>) -> Outcome {
        let fail = |t: String| Outcome::new(t, true);
        let with: Vec<&Server> = self.servers.iter().filter(|s| s.resources).collect();
        let named = args["server"].as_str().map(str::trim).filter(|n| !n.is_empty());
        let uri = args["uri"].as_str().map(str::trim).filter(|u| !u.is_empty());
        let chosen: Vec<&Server> = match named {
            Some(n) => match with.iter().find(|s| s.name == n) {
                Some(s) => vec![*s],
                None => return fail(format!("No MCP server {n} with resources: {} have them.", with.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", "))),
            },
            None => with,
        };
        let shorten = |text: String| match spill_dir {
            Some(dir) => bash::shorten(&text, text.len(), dir),
            None => text,
        };
        if let Some(uri) = uri {
            let [s] = chosen[..] else {
                return fail(format!("Say which server's resource {uri} is: {}.", chosen.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")));
            };
            return match s.conn.request("resources/read", json!({ "uri": uri }), CALL_TIMEOUT).await {
                Ok(r) => {
                    // (Each part as a tool's embedded resource.)
                    let parts: Vec<Value> = r["contents"].as_array().into_iter().flatten().map(|c| json!({ "type": "resource", "resource": c })).collect();
                    let (text, _, images) = result_text(&json!({ "content": parts }));
                    let mut out = Outcome::new(shorten(text), false);
                    out.images = images;
                    out
                }
                Err(e) => fail(format!("MCP server {}: {e}", s.name)),
            };
        }
        let mut out = Vec::new();
        for s in chosen {
            let mut rows = Vec::new();
            let mut cursor: Option<String> = None;
            for _ in 0..20 {
                let params = cursor.as_ref().map_or(json!({}), |c| json!({ "cursor": c }));
                match s.conn.request("resources/list", params, s.timeout.max(Duration::from_secs(30))).await {
                    Ok(r) => {
                        for x in r["resources"].as_array().into_iter().flatten() {
                            rows.push(resource_row(x, "uri"));
                        }
                        cursor = r["nextCursor"].as_str().filter(|c| !c.is_empty()).map(str::to_owned);
                    }
                    Err(e) => {
                        rows.push(format!("(couldn't list them: {e})"));
                        cursor = None;
                    }
                }
                if cursor.is_none() || rows.len() >= RESOURCES_LISTED {
                    break;
                }
            }
            // (Templates: optional, so a server without them says nothing.)
            if let Ok(r) = s.conn.request("resources/templates/list", json!({}), s.timeout.max(Duration::from_secs(30))).await {
                let t: Vec<String> = r["resourceTemplates"].as_array().into_iter().flatten().map(|x| resource_row(x, "uriTemplate")).collect();
                if !t.is_empty() {
                    rows.push(format!("URI templates (fill in the {{…}} parts):\n{}", t.join("\n")));
                }
            }
            let more = rows.len().saturating_sub(RESOURCES_LISTED);
            rows.truncate(RESOURCES_LISTED);
            if more > 0 {
                rows.push(format!("[{more} more not shown]"));
            }
            out.push(format!("{}:\n{}", s.name, if rows.is_empty() { "(no resources)".to_owned() } else { rows.join("\n") }));
        }
        Outcome::new(shorten(out.join("\n\n")), false)
    }
}

/// A listed resource (or template): `uri  name (type): description`.
fn resource_row(x: &Value, key: &str) -> String {
    let s = |k: &str| x[k].as_str().map(str::trim).unwrap_or_default();
    let mut row = s(key).to_owned();
    let name = if s("title").is_empty() { s("name") } else { s("title") };
    if !name.is_empty() && name != s(key) {
        row.push_str(&format!("  {name}"));
    }
    if !s("mimeType").is_empty() {
        row.push_str(&format!(" ({})", s("mimeType")));
    }
    let about = s("description").lines().next().unwrap_or_default();
    if !about.is_empty() {
        let about: String = if about.chars().count() > 120 { format!("{}…", about.chars().take(120).collect::<String>()) } else { about.to_owned() };
        row.push_str(&format!(": {about}"));
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_the_start_waits_for() {
        let s = |n: &str, t: u64| (n.to_owned(), Duration::from_secs(t));
        assert_eq!(Mcp::waiting_for(&[]), None);
        assert_eq!(Mcp::waiting_for(&[s("docs", 10)]).unwrap(), "Waiting for MCP server docs to start (up to 8 s more).");
        assert_eq!(Mcp::waiting_for(&[s("a", 10), s("b", 60)]).unwrap(), "Waiting for MCP servers a, b to start (up to 58 s more).");
        assert_eq!(START_TIMEOUT, Duration::from_secs(10));
    }

    #[test]
    fn names_tools() {
        let mut taken = HashSet::new();
        assert_eq!(tool_name("github", "create_issue", &taken), "mcp__github__create_issue");
        assert_eq!(tool_name("my server", "do.it", &taken), "mcp__my_server__do_it");
        let long = tool_name("a-very-long-server-name", &"x".repeat(60), &taken);
        assert!(long.len() <= NAME_MAX && long.starts_with("mcp__a-very-long"), "{long}");
        taken.insert("mcp__s__t".to_owned());
        let other = tool_name("s", "t", &taken);
        assert!(other != "mcp__s__t" && other.len() <= NAME_MAX && other.starts_with("mcp__s__t_"), "{other}");
    }

    #[test]
    fn parses_entries() {
        let p = Path::new("/proj");
        // SAFETY: a variable no other test reads.
        unsafe { std::env::set_var("HARLO_MCP_TEST_TOKEN", "t0k") };
        let c = parse("gh", &json!({ "type": "http", "url": "https://x.test/mcp", "headers": { "Authorization": "Bearer ${HARLO_MCP_TEST_TOKEN}" } }), p).unwrap();
        assert_eq!(c.kind, Kind::Http { url: "https://x.test/mcp".into(), headers: vec![("Authorization".into(), "Bearer t0k".into())] });
        let c = parse("fs", &json!({ "command": "npx", "args": ["-y", "srv", "${HARLO_NOPE:-.}"], "cwd": "tools", "timeout": 5 }), p).unwrap();
        assert_eq!(c.kind, Kind::Stdio { program: "npx".into(), args: vec!["-y".into(), "srv".into(), ".".into()] });
        assert_eq!((c.cwd.as_deref(), c.timeout), (Some("/proj/tools"), Duration::from_secs(5)));
        // (A url alone is an address; a command given as an argv works too.)
        assert!(matches!(parse("u", &json!({ "url": "http://h/mcp" }), p).unwrap().kind, Kind::Http { .. }));
        assert_eq!(parse("o", &json!({ "command": ["srv", "--x"] }), p).unwrap().kind, Kind::Stdio { program: "srv".into(), args: vec!["--x".into()] });
        assert_eq!(parse("s", &json!({ "type": "sse", "url": "http://h/sse" }), p).unwrap().kind, Kind::Sse { url: "http://h/sse".into(), headers: vec![] });
        assert!(parse("n", &json!({}), p).unwrap_err().contains("no command"));
        assert!(parse("b", &json!({ "type": "http", "url": "ftp://h" }), p).is_err());
        // Tools said to only look: all, some, or none.
        assert_eq!(parse("a", &json!({ "command": "srv", "readOnly": true }), p).unwrap().read_only, ReadOnly::All);
        let some = parse("t", &json!({ "command": "srv", "readOnly": ["get_scene", "render_preview"] }), p).unwrap().read_only;
        assert!(some.covers("get_scene") && !some.covers("execute_code"));
        for none in [json!(false), json!("yes"), Value::Null] {
            assert_eq!(parse("x", &json!({ "command": "srv", "readOnly": none }), p).unwrap().read_only, ReadOnly::None);
        }
    }

    #[test]
    fn hashes_entries_whatever_the_key_order() {
        let a = entry_hash(&json!({ "command": "srv", "env": { "A": "1", "B": "2" } }));
        let b = entry_hash(&json!({ "env": { "B": "2", "A": "1" }, "command": "srv" }));
        assert_eq!(a, b);
        assert_ne!(a, entry_hash(&json!({ "command": "srv2", "env": { "A": "1", "B": "2" } })));
        assert_eq!(a.len(), 64);
        // (As harlo writes it: keys sorted, compact.)
        let want: String = Sha256::digest(br#"{"command":"srv","env":{"A":"1","B":"2"}}"#).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(a, want);
    }

    #[test]
    fn turns_results_into_text() {
        let dot = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let r = json!({ "content": [
            { "type": "text", "text": "hello" },
            { "type": "image", "mimeType": "image/png", "data": "A".repeat(4000) },
            { "type": "image", "mimeType": "image/png", "data": dot },
            { "type": "audio", "mimeType": "audio/wav", "data": "AAAA" },
            { "type": "resource", "resource": { "uri": "file:///a", "text": "inside" } },
            { "type": "resource", "resource": { "uri": "file:///b", "mimeType": "application/pdf", "blob": "AA" } },
            { "type": "resource", "resource": { "uri": "file:///d.png", "mimeType": "image/png", "blob": dot } },
            { "type": "resource_link", "uri": "file:///c", "name": "c" },
        ] });
        let (text, failed, images) = result_text(&r);
        assert!(!failed);
        assert_eq!(text, "hello\n[image (image/png, about 3 KB): not shown, it isn't a PNG, JPEG, GIF or WebP image]\n[image 1: 1×1 PNG, 1 KB]\n[audio (audio/wav, about 1 KB): not shown, Harlo doesn't take audio]\ninside\n[resource file:///b (application/pdf)]\n[image 2: 1×1 PNG, 1 KB]\n[link: file:///c c]");
        assert_eq!(images.len(), 2);
        assert_eq!((images[0]["type"].as_str(), images[0]["mimeType"].as_str(), images[0]["data"].as_str()), (Some("image"), Some("image/png"), Some(dot)));
        assert_eq!(result_text(&json!({ "content": [], "structuredContent": { "n": 1 } })), ("{\"n\":1}".into(), false, vec![]));
        assert_eq!(result_text(&json!({ "content": [], "isError": true })).1, true);
        assert_eq!(result_text(&json!({ "content": [] })).0, "(no output)");
    }

    #[test]
    fn answers_the_servers_requests() {
        let p = Path::new("/home/u/proj");
        assert_eq!(answer(&json!({ "id": 3, "method": "ping" }), p)["result"], json!({}));
        assert_eq!(answer(&json!({ "id": 4, "method": "roots/list" }), p)["result"]["roots"][0]["uri"], "file:///home/u/proj");
        assert_eq!(answer(&json!({ "id": 5, "method": "sampling/createMessage" }), p)["error"]["code"], -32601);
    }
}
