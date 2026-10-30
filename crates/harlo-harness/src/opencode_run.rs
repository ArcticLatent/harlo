//! An OpenCode session behind harlo's harness interface. All of harlo's
//! OpenCode sessions share one private server (`opencode.rs`); its event
//! stream (`/api/event`) is read once and handed to each session by id.
//! harlo's commands (pi RPC shaped) become API calls; OpenCode's events
//! become harlo events (steps → messages, reasoning / text / tools →
//! blocks, permissions and forms → UI requests).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Result, bail};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::event::{BlockKind, HarnessEvent, HarnessKind, NoticeLevel};
use crate::opencode::{self, Server};
use crate::process::{Harness, HarnessConfig, Outgoing, Pending, RemoteLaunch, SessionTarget};
use crate::store::{SavedSession, SessionInfo};

/// The shared server and who gets which session's events.
struct Hub {
    server: Arc<Server>,
    sessions: Mutex<HashMap<String, mpsc::UnboundedSender<Value>>>,
}

/// The servers: this machine's ("") and each remote host's, by name.
static HUBS: OnceLock<Mutex<HashMap<String, Arc<Hub>>>> = OnceLock::new();

/// The running server here, or on `remote`'s host (started on first use,
/// again if it died).
fn hub(program: &Path, remote: Option<&RemoteLaunch>) -> Result<Arc<Hub>> {
    let key = remote.map(|r| r.host.clone()).unwrap_or_default();
    let mut hubs = HUBS.get_or_init(Default::default).lock().unwrap();
    if let Some(h) = hubs.get(&key)
        && h.server.get("/api/info", None).is_ok()
    {
        return Ok(Arc::clone(h));
    }
    let server = Arc::new(match remote {
        Some(r) => Server::start_remote(r)?,
        None => Server::start(program, &std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir))?,
    });
    let hub = Arc::new(Hub { server: Arc::clone(&server), sessions: Mutex::new(HashMap::new()) });
    // One reader for every session; it reconnects while the server lives.
    let h = Arc::downgrade(&hub);
    std::thread::spawn(move || {
        loop {
            let Some(hub) = h.upgrade() else { return };
            let server = Arc::clone(&hub.server);
            drop(hub);
            let h2 = h.clone();
            let _ = server.follow_events(|ev| {
                let Some(hub) = h2.upgrade() else { return false };
                let id = event_session(&ev);
                let mut sessions = hub.sessions.lock().unwrap();
                // A subagent's session: its events go where its parent's go.
                if ev["type"] == "session.created"
                    && let (Some(child), Some(parent)) = (id.as_ref(), ev["data"]["parentID"].as_str().or_else(|| ev["data"]["info"]["parentID"].as_str()))
                    && let Some(tx) = sessions.get(parent).cloned()
                {
                    sessions.insert(child.clone(), tx);
                }
                match id.as_ref().and_then(|id| sessions.get(id)) {
                    Some(tx) => {
                        let _ = tx.send(ev);
                    }
                    // Not tied to a session (catalog changes, a restart): all
                    // sessions may care.
                    None if id.is_none() => {
                        for tx in sessions.values() {
                            let _ = tx.send(ev.clone());
                        }
                    }
                    None => {}
                }
                true
            });
            if server.get("/api/info", None).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    hubs.insert(key, Arc::clone(&hub));
    Ok(hub)
}

/// Stops the servers (harlo is quitting).
pub fn shutdown() {
    if let Some(hubs) = HUBS.get() {
        for (_, hub) in hubs.lock().unwrap().drain() {
            hub.server.stop();
        }
    }
}

/// A remote host's OpenCode sessions in `roots` (its project folders, as
/// it sees them), from its server's API (the database is there).
pub fn remote_sessions(remote: &RemoteLaunch, roots: &[String]) -> Result<Vec<SessionInfo>> {
    let hub = hub(Path::new("opencode"), Some(remote))?;
    let mut out = Vec::new();
    for root in roots {
        let dir = PathBuf::from(root);
        let list = hub.server.get(&format!("/api/session?limit=200&directory={}", opencode::url_encode(root)), Some(&dir))?;
        for s in list["data"].as_array().into_iter().flatten().filter(|s| s["parentID"].as_str().is_none_or(str::is_empty)) {
            let Some(id) = s["id"].as_str() else { continue };
            let Some(folder) = s["location"]["directory"].as_str() else { continue };
            let ms = |k: &str| s["time"][k].as_i64().unwrap_or(0).max(0) as u64;
            out.push(SessionInfo {
                harness: HarnessKind::OpenCode,
                path: opencode::remote_session_path(&remote.host, id),
                id: id.to_owned(),
                cwd: PathBuf::from(format!("ssh://{}{folder}", remote.host)),
                name: s["title"].as_str().unwrap_or_default().to_owned(),
                first_message: String::new(),
                created: ms("created").to_string(),
                modified: std::time::UNIX_EPOCH + Duration::from_millis(ms("updated")),
                size: 0,
                parent_session: None,
            });
        }
    }
    Ok(out)
}

/// A remote session's conversation, from its host's server.
fn remote_history(hub: &Hub, id: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut cursor = String::new();
    for _ in 0..50 {
        let path = format!("/api/session/{id}/message?limit=200&order=asc{}", if cursor.is_empty() { String::new() } else { format!("&cursor={}", opencode::url_encode(&cursor)) });
        let Ok(page) = hub.server.get(&path, None) else { break };
        for m in page["data"].as_array().into_iter().flatten() {
            out.extend(opencode::to_pi_messages(m["type"].as_str().unwrap_or_default(), m));
        }
        match page["cursor"].as_str().filter(|c| !c.is_empty()) {
            Some(c) if page["data"].as_array().is_some_and(|d| !d.is_empty()) => cursor = c.to_owned(),
            _ => break,
        }
    }
    out
}

/// What a saved remote session shows before it runs (blocking).
pub fn remote_saved(remote: &RemoteLaunch, path: &Path) -> SavedSession {
    let Ok(hub) = hub(Path::new("opencode"), Some(remote)) else { return SavedSession::default() };
    let id = opencode::session_id(path);
    let messages = remote_history(&hub, &id);
    let model = messages.iter().rev().find_map(|m| m["model"].as_str().filter(|s| s.len() > 1)).unwrap_or_default().to_owned();
    let name = hub.server.get(&format!("/api/session/{id}"), None).ok().and_then(|v| v["data"]["title"].as_str().map(str::to_owned)).unwrap_or_default();
    SavedSession { messages, model, thinking: String::new(), name }
}

/// A saved remote session's subagents (its child sessions), from its
/// host's server, linked to the calls that started them.
pub fn remote_subagents(remote: &RemoteLaunch, path: &Path) -> Vec<Value> {
    let Ok(hub) = hub(Path::new("opencode"), Some(remote)) else { return Vec::new() };
    let id = opencode::session_id(path);
    let Some(folder) = hub.server.get(&format!("/api/session/{id}"), None).ok().and_then(|v| v["data"]["location"]["directory"].as_str().map(str::to_owned)) else { return Vec::new() };
    let dir = PathBuf::from(&folder);
    let Ok(list) = hub.server.get(&format!("/api/session?limit=200&parentID={id}&directory={}", opencode::url_encode(&folder)), Some(&dir)) else { return Vec::new() };
    let children: Vec<&Value> = list["data"].as_array().into_iter().flatten().filter(|c| c["parentID"] == id.as_str()).collect();
    if children.is_empty() {
        return Vec::new();
    }
    // Calls → children, from the parent's steps.
    let mut calls: HashMap<String, String> = HashMap::new();
    let page = hub.server.get(&format!("/api/session/{id}/message?limit=200&order=asc"), None).unwrap_or_default();
    for m in page["data"].as_array().into_iter().flatten().filter(|m| m["type"] == "assistant") {
        for b in m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "tool" && opencode::SUBAGENT_TOOLS.contains(&b["name"].as_str().unwrap_or_default())) {
            if let (Some(call), Some(child)) = (b["id"].as_str(), opencode::child_of(&b["state"])) {
                calls.insert(child, call.to_owned());
            }
        }
    }
    children
        .into_iter()
        .filter_map(|c| {
            let child = c["id"].as_str()?;
            let call = calls.get(child).cloned().unwrap_or_else(|| child.to_owned());
            let status = match c["outcome"].as_str() {
                Some("succeeded") => "completed",
                Some("failed") => "failed",
                Some("interrupted") => "aborted",
                _ => "ended",
            };
            Some(json!({
                "id": call, "parentToolCallId": call, "name": c["title"].as_str().unwrap_or("Subagent"), "agent": c["agent"].as_str().unwrap_or("general"),
                "status": status, "sessionFile": opencode::remote_session_path(&remote.host, child).to_string_lossy(), "childSession": child,
                "canCancel": false, "canSteer": false,
            }))
        })
        .collect()
}

/// A remote session's prompt ids, in order (Edit & resend).
pub fn remote_user_entries(remote: &RemoteLaunch, path: &Path) -> Vec<String> {
    let Ok(hub) = hub(Path::new("opencode"), Some(remote)) else { return Vec::new() };
    let id = opencode::session_id(path);
    let page = hub.server.get(&format!("/api/session/{id}/message?limit=200&order=asc"), None).unwrap_or_default();
    page["data"].as_array().into_iter().flatten().filter(|m| m["type"] == "user").filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()
}

/// Deletes a session and its subagents' sessions, through the server here
/// or on `remote`'s host (blocking). OpenCode keeps them in its database,
/// so there's no file to move to the trash.
pub fn delete_session(program: &Path, remote: Option<&RemoteLaunch>, path: &Path) -> Result<()> {
    let hub = hub(program, remote)?;
    hub.server.request("DELETE", &format!("/api/session/{}", opencode::session_id(path)), None, None)?;
    Ok(())
}

/// Which session an event is about.
fn event_session(ev: &Value) -> Option<String> {
    let d = &ev["data"];
    d["sessionID"]
        .as_str()
        .or_else(|| d["form"]["sessionID"].as_str())
        .or_else(|| (ev["type"] == "session.created").then(|| d["info"]["id"].as_str()).flatten())
        .or_else(|| d["info"]["metadata"]["sessionID"].as_str())
        .map(str::to_owned)
}

/// An assistant step being streamed.
#[derive(Default)]
struct Step {
    id: String,
    /// Per block: its kind, text so far, and for a tool its id, name and input.
    blocks: Vec<(BlockKind, String, String, String, Value)>,
    reasoning: HashMap<u64, usize>,
    text: HashMap<u64, usize>,
    tools: HashMap<String, usize>,
    model: String,
    /// Its message is in the transcript (only from its first block, so the
    /// retries before it show above it).
    shown: bool,
}

impl Step {
    fn show(&mut self, out: &mut Vec<HarnessEvent>) {
        if !std::mem::replace(&mut self.shown, true) {
            out.push(HarnessEvent::MessageStart { id: self.id.clone(), message: json!({ "role": "assistant", "content": [] }) });
        }
    }
}

#[derive(Default, Clone)]
struct Model {
    provider: String,
    id: String,
    variant: String,
}

#[derive(Default)]
struct State {
    session_id: String,
    model: Model,
    agent: String,
    /// What OpenCode already has (switches are sent when they differ).
    sent_model: Option<(String, String, String)>,
    sent_agent: String,
    running: bool,
    step: Option<Step>,
    /// UI request id → (kind: "permission" | "form", OpenCode's id, form
    /// fields, the session that asked: this one or a subagent's).
    waiting: HashMap<String, (String, String, Value, String)>,
    /// Sessions (this one, subagents') whose permission was just denied:
    /// OpenCode then stops them with a "shutdown" interruption, their end.
    declined: HashSet<String>,
    /// Subagents by the call that started them (as harlo's dock shows
    /// them); their sessions → those calls; their tool calls' names.
    subagents: HashMap<String, Value>,
    /// A name given before the session exists (it's created with it).
    pending_title: Option<String>,
    /// harlo's permission shield: always-ask, write (edits), yolo.
    approval: String,
    children: HashMap<String, String>,
    child_tools: HashMap<String, String>,
    next: u64,
    context_tokens: u64,
    cost: f64,
    messages: Vec<Value>,
    commands: Vec<String>,
    /// The models' variants (`provider/id` → ids) and context sizes.
    variants: HashMap<String, Vec<String>>,
    windows: HashMap<String, u64>,
}

struct Session {
    state: Mutex<State>,
    events: mpsc::UnboundedSender<HarnessEvent>,
    hub: Mutex<Option<Arc<Hub>>>,
    /// The project folder as OpenCode's machine sees it.
    dir: PathBuf,
    program: PathBuf,
    inbox: mpsc::UnboundedSender<Value>,
    /// On another machine: how to reach it.
    remote: Option<RemoteLaunch>,
}

impl Session {
    fn emit(&self, ev: HarnessEvent) {
        let _ = self.events.send(ev);
    }

    fn next_id(&self, prefix: &str) -> String {
        let mut st = self.state.lock().unwrap();
        st.next += 1;
        format!("{prefix}{}", st.next)
    }

    /// An API call off the async threads.
    async fn api(&self, method: &'static str, path: String, body: Option<Value>) -> Result<Value> {
        let hub = {
            let existing = self.hub.lock().unwrap().clone();
            match existing {
                Some(h) => h,
                None => {
                    let (program, remote) = (self.program.clone(), self.remote.clone());
                    let h = tokio::task::spawn_blocking(move || hub(&program, remote.as_ref())).await??;
                    *self.hub.lock().unwrap() = Some(Arc::clone(&h));
                    h
                }
            }
        };
        let dir = self.dir.clone();
        tokio::task::spawn_blocking(move || hub.server.request(method, &path, body.as_ref(), Some(&dir))).await?
    }

    /// How harlo names a session (here: in OpenCode's data folder; remote:
    /// on its host).
    fn session_path(&self, id: &str) -> PathBuf {
        match &self.remote {
            Some(r) => opencode::remote_session_path(&r.host, id),
            None => opencode::session_path(id),
        }
    }

    /// A session's conversation (here from the database, remote from its
    /// server).
    async fn history(&self, id: &str) -> Vec<Value> {
        if self.remote.is_none() {
            return opencode::history(id);
        }
        let Some(hub) = self.hub.lock().unwrap().clone() else { return Vec::new() };
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || remote_history(&hub, &id)).await.unwrap_or_default()
    }

    /// Creates the OpenCode session on first use.
    async fn ensure_session(self: &Arc<Self>) -> Result<String> {
        let existing = self.state.lock().unwrap().session_id.clone();
        if !existing.is_empty() {
            self.subscribe(&existing);
            return Ok(existing);
        }
        let (model, agent) = {
            let st = self.state.lock().unwrap();
            (st.model.clone(), st.agent.clone())
        };
        let mut body = json!({ "location": { "directory": self.dir } });
        if !model.id.is_empty() {
            body["model"] = model_json(&model);
        }
        if !agent.is_empty() {
            body["agent"] = json!(agent);
        }
        if let Some(title) = self.state.lock().unwrap().pending_title.take().filter(|t| !t.is_empty()) {
            body["title"] = json!(title);
        }
        let created = self.api("POST", "/api/session".into(), Some(body)).await?;
        let id = created["data"]["id"].as_str().unwrap_or_default().to_owned();
        if id.is_empty() {
            bail!("OpenCode didn't create a session");
        }
        {
            let mut st = self.state.lock().unwrap();
            st.session_id = id.clone();
            st.sent_model = (!model.id.is_empty()).then(|| (model.provider.clone(), model.id.clone(), model.variant.clone()));
            st.sent_agent = agent;
        }
        self.subscribe(&id);
        Ok(id)
    }

    fn subscribe(&self, id: &str) {
        if let Some(h) = self.hub.lock().unwrap().as_ref() {
            h.sessions.lock().unwrap().insert(id.to_owned(), self.inbox.clone());
        }
    }

    /// Sends the model / agent the user picked, if OpenCode has another.
    async fn sync_choices(self: &Arc<Self>, id: &str) {
        let (model, agent, sent_model, sent_agent) = {
            let st = self.state.lock().unwrap();
            (st.model.clone(), st.agent.clone(), st.sent_model.clone(), st.sent_agent.clone())
        };
        let want = (model.provider.clone(), model.id.clone(), model.variant.clone());
        if !model.id.is_empty() && sent_model.as_ref() != Some(&want) && self.api("POST", format!("/api/session/{id}/model"), Some(json!({ "model": model_json(&model) }))).await.is_ok() {
            self.state.lock().unwrap().sent_model = Some(want);
        }
        if !agent.is_empty() && agent != sent_agent && self.api("POST", format!("/api/session/{id}/agent"), Some(json!({ "agent": agent }))).await.is_ok() {
            self.state.lock().unwrap().sent_agent = agent;
        }
    }
}

fn model_json(m: &Model) -> Value {
    let mut v = json!({ "id": m.id, "providerID": m.provider });
    if !m.variant.is_empty() && m.variant != "default" {
        v["variant"] = json!(m.variant);
    }
    v
}

/// OpenCode's message ids: `msg_` + 12 hex (time) + 14 base62.
fn message_id() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = ms.wrapping_mul(0x1000).wrapping_add(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xfff);
    let mut bytes = [0u8; 14];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut bytes);
    }
    const B62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let tail: String = bytes.iter().map(|b| B62[(*b as usize) % 62] as char).collect();
    format!("msg_{:012x}{tail}", n & 0xffff_ffff_ffff)
}

/// `provider/model` (harlo's form) → the parts.
fn split_model(s: &str) -> Model {
    match s.split_once('/') {
        Some((p, id)) => Model { provider: p.to_owned(), id: id.to_owned(), variant: String::new() },
        None => Model { provider: String::new(), id: s.to_owned(), variant: String::new() },
    }
}

pub(crate) fn spawn(cfg: HarnessConfig, program: PathBuf) -> Result<(Harness, mpsc::UnboundedReceiver<HarnessEvent>)> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Outgoing>();
    let (events, events_rx) = mpsc::unbounded_channel();
    let pending: Pending = Arc::default();
    let harness = Harness::from_parts(HarnessKind::OpenCode, tx, Arc::clone(&pending));
    let (inbox, mut inbox_rx) = mpsc::unbounded_channel::<Value>();
    let mut state = State::default();
    let mut it = cfg.extra_args.iter();
    while let Some(a) = it.next() {
        if a == "--model"
            && let Some(m) = it.next()
        {
            state.model = split_model(m);
        }
    }
    if let SessionTarget::Resume(path) = &cfg.session {
        state.session_id = opencode::session_id(path);
        // (A remote one's comes from its server, once reached.)
        if cfg.remote.is_none() {
            state.messages = opencode::history(&state.session_id);
        }
        // The model it last used (unless harlo picked one).
        if state.model.id.is_empty()
            && let Some(m) = state.messages.iter().rev().find_map(|m| m["model"].as_str().filter(|s| s.contains('/')))
        {
            state.model = split_model(m);
            state.sent_model = Some((state.model.provider.clone(), state.model.id.clone(), String::new()));
        }
    }
    let dir = if cfg.remote.is_some() { PathBuf::from(crate::process::on_host(&cfg.cwd)) } else { cfg.cwd.clone() };
    let session = Arc::new(Session { state: Mutex::new(state), events, hub: Mutex::new(None), dir, program, inbox, remote: cfg.remote.clone() });

    // Start (or reach) the server, then say we're ready.
    let s = Arc::clone(&session);
    tokio::spawn(async move {
        match s.api("GET", "/api/info".into(), None).await {
            Ok(_) => {
                let id = s.state.lock().unwrap().session_id.clone();
                if !id.is_empty() {
                    s.subscribe(&id);
                    // A remote session: its conversation, from there.
                    if s.remote.is_some() {
                        let messages = s.history(&id).await;
                        let mut st = s.state.lock().unwrap();
                        if st.model.id.is_empty()
                            && let Some(m) = messages.iter().rev().find_map(|m| m["model"].as_str().filter(|s| s.contains('/')))
                        {
                            st.model = split_model(m);
                            st.sent_model = Some((st.model.provider.clone(), st.model.id.clone(), String::new()));
                        }
                        st.messages = messages;
                    }
                }
                s.emit(HarnessEvent::Ready);
                load_catalog(&s).await;
            }
            Err(e) => {
                s.emit(HarnessEvent::Notice { level: NoticeLevel::Error, text: format!("OpenCode didn't start: {e:#}"), system: false });
                s.emit(HarnessEvent::Exited { code: None, stderr_tail: String::new() });
            }
        }
    });
    // OpenCode's events for this session.
    let s = Arc::clone(&session);
    tokio::spawn(async move {
        while let Some(ev) = inbox_rx.recv().await {
            event(&s, &ev);
        }
    });
    // harlo's commands.
    let s = Arc::clone(&session);
    tokio::spawn(async move {
        while let Some(out) = rx.recv().await {
            match out {
                Outgoing::Frame(cmd) => command(&s, &pending, cmd).await,
                Outgoing::Kill => break,
            }
        }
        // Gone: stop listening (the shared server keeps running for others).
        let id = s.state.lock().unwrap().session_id.clone();
        if let Some(h) = s.hub.lock().unwrap().as_ref() {
            h.sessions.lock().unwrap().remove(&id);
        }
    });
    Ok((harness, events_rx))
}

/// Models (with their variants and context sizes) and slash commands. A
/// folder's catalog loads on first use: empty at first, filled a moment later.
async fn load_catalog(s: &Arc<Session>) {
    for _ in 0..40 {
        let Ok(models) = s.api("GET", "/api/model".into(), None).await else { break };
        if models["data"].as_array().is_none_or(Vec::is_empty) {
            tokio::time::sleep(Duration::from_millis(150)).await;
            continue;
        }
        let mut st = s.state.lock().unwrap();
        for m in models["data"].as_array().into_iter().flatten() {
            let key = format!("{}/{}", m["providerID"].as_str().unwrap_or_default(), m["modelID"].as_str().or_else(|| m["id"].as_str()).unwrap_or_default());
            st.variants.insert(key.clone(), m["variants"].as_array().into_iter().flatten().filter_map(|v| v["id"].as_str().map(str::to_owned)).collect());
            if let Some(w) = m["limit"]["context"].as_u64() {
                st.windows.insert(key, w);
            }
        }
        break;
    }
    if let Ok(cmds) = s.api("GET", "/api/command".into(), None).await {
        let names: Vec<String> = cmds["data"].as_array().into_iter().flatten().filter_map(|c| c["name"].as_str().map(str::to_owned)).collect();
        let list: Vec<Value> = cmds["data"].as_array().into_iter().flatten().map(|c| json!({ "name": c["name"], "description": c["description"] })).collect();
        s.state.lock().unwrap().commands = names;
        s.emit(HarnessEvent::CommandsChanged(Value::Array(list)));
    }
}

async fn command(s: &Arc<Session>, pending: &Pending, cmd: Value) {
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
            let files: Vec<Value> = cmd["images"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(i, img)| json!({ "uri": format!("data:{};base64,{}", img["mimeType"].as_str().unwrap_or("image/png"), img["data"].as_str().unwrap_or_default()), "name": format!("image-{}", i + 1) }))
                .collect();
            let message = json!({ "role": "user", "content": [{ "type": "text", "text": text }] });
            s.state.lock().unwrap().messages.push(message.clone());
            s.emit(HarnessEvent::MessageEnd { id: s.next_id("u"), message });
            let sid = match s.ensure_session().await {
                Ok(id) => id,
                Err(e) => {
                    s.emit(HarnessEvent::Notice { level: NoticeLevel::Error, text: format!("OpenCode: {e:#}"), system: false });
                    return reply(false, Value::Null, &format!("{e:#}"));
                }
            };
            s.sync_choices(&sid).await;
            // `/name args` for one of OpenCode's commands.
            let command = text.strip_prefix('/').and_then(|rest| {
                let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                s.state.lock().unwrap().commands.iter().any(|c| c == name).then(|| (name.to_owned(), args.trim().to_owned()))
            });
            let result = match command {
                Some((name, args)) => s.api("POST", format!("/api/session/{sid}/command"), Some(json!({ "name": name, "text": args, "files": files }))).await,
                None => {
                    let mut body = json!({ "id": message_id(), "text": text, "files": files });
                    match cmd["streamingBehavior"].as_str() {
                        Some("steer") => body["delivery"] = json!("steer"),
                        Some("followUp") => body["delivery"] = json!("queue"),
                        _ => {}
                    }
                    s.api("POST", format!("/api/session/{sid}/prompt"), Some(body)).await
                }
            };
            match result {
                Ok(_) => reply(true, json!({ "disposition": "started" }), ""),
                Err(e) => {
                    s.emit(HarnessEvent::Notice { level: NoticeLevel::Error, text: format!("OpenCode: {e:#}"), system: false });
                    reply(false, Value::Null, &format!("{e:#}"))
                }
            }
        }
        "abort" => {
            let sid = s.state.lock().unwrap().session_id.clone();
            if !sid.is_empty() {
                let _ = s.api("POST", format!("/api/session/{sid}/interrupt"), None).await;
            }
            reply(true, Value::Null, "");
        }
        "get_state" => {
            let st = s.state.lock().unwrap();
            let model = if st.model.id.is_empty() { Value::Null } else { json!({ "provider": st.model.provider, "id": st.model.id, "name": st.model.id }) };
            let file = (!st.session_id.is_empty()).then(|| s.session_path(&st.session_id).to_string_lossy().into_owned());
            reply(true, json!({ "model": model, "thinkingLevel": st.model.variant, "sessionId": st.session_id, "sessionFile": file, "isStreaming": st.running }), "");
        }
        "get_session_stats" => {
            let st = s.state.lock().unwrap();
            let window = st.windows.get(&format!("{}/{}", st.model.provider, st.model.id)).copied().unwrap_or(0);
            reply(true, json!({ "contextUsage": { "tokens": st.context_tokens, "contextWindow": window }, "cost": st.cost }), "");
        }
        "get_available_models" => {
            let list = match s.api("GET", "/api/model".into(), None).await {
                Ok(v) => v["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|m| m["enabled"] != false)
                    .map(|m| json!({ "provider": m["providerID"], "id": m["modelID"].as_str().or_else(|| m["id"].as_str()), "name": m["name"], "reasoning": m["variants"].as_array().is_some_and(|v| !v.is_empty()), "baseUrl": m["settings"]["baseURL"] }))
                    .collect(),
                Err(_) => Vec::new(),
            };
            reply(true, json!({ "models": list }), "");
        }
        "set_model" => {
            let model = Model { provider: cmd["provider"].as_str().unwrap_or_default().to_owned(), id: cmd["modelId"].as_str().unwrap_or_default().to_owned(), variant: String::new() };
            s.state.lock().unwrap().model = model.clone();
            let sid = s.state.lock().unwrap().session_id.clone();
            if !sid.is_empty() {
                s.sync_choices(&sid).await;
            }
            let m = json!({ "provider": model.provider, "id": model.id, "name": model.id });
            s.emit(HarnessEvent::ModelChanged(m.clone()));
            reply(true, m, "");
        }
        "get_available_thinking_levels" => {
            let st = s.state.lock().unwrap();
            let levels = st.variants.get(&format!("{}/{}", st.model.provider, st.model.id)).cloned().unwrap_or_default();
            reply(true, json!({ "levels": levels }), "");
        }
        "set_thinking_level" => {
            let level = cmd["level"].as_str().unwrap_or_default().to_owned();
            s.state.lock().unwrap().model.variant = level.clone();
            let sid = s.state.lock().unwrap().session_id.clone();
            if !sid.is_empty() {
                s.sync_choices(&sid).await;
            }
            s.emit(HarnessEvent::ThinkingLevel(level));
            reply(true, Value::Null, "");
        }
        "harlo_set_modes" => {
            {
                let mut st = s.state.lock().unwrap();
                st.agent = if cmd["plan"].as_bool().unwrap_or(false) { "plan".into() } else { "build".into() };
                st.approval = cmd["approval"].as_str().unwrap_or_default().to_owned();
            }
            let sid = s.state.lock().unwrap().session_id.clone();
            if !sid.is_empty() {
                s.sync_choices(&sid).await;
            }
            reply(true, Value::Null, "");
        }
        "get_commands" | "get_available_commands" => {
            let cmds = s.api("GET", "/api/command".into(), None).await.unwrap_or(Value::Null);
            let list: Vec<Value> = cmds["data"].as_array().into_iter().flatten().map(|c| json!({ "name": c["name"], "description": c["description"] })).collect();
            reply(true, json!({ "commands": list }), "");
        }
        "get_messages" => {
            let messages = s.state.lock().unwrap().messages.clone();
            reply(true, json!({ "messages": messages }), "");
        }
        "compact" => {
            let sid = s.state.lock().unwrap().session_id.clone();
            if !sid.is_empty() {
                let _ = s.api("POST", format!("/api/session/{sid}/compact"), None).await;
            }
            reply(true, Value::Null, "");
        }
        "extension_ui_response" => answer(s, &cmd).await,
        // The dock's list comes from the subagents' events.
        "get_subagents" => reply(true, json!({ "subagents": [] }), ""),
        // A name of your own (before the session exists: it's created with it).
        "set_session_name" => {
            let name = cmd["name"].as_str().unwrap_or_default().trim().to_owned();
            let sid = s.state.lock().unwrap().session_id.clone();
            let result = if sid.is_empty() {
                s.state.lock().unwrap().pending_title = Some(name.clone());
                Ok(Value::Null)
            } else {
                s.api("PATCH", format!("/api/session/{sid}"), Some(json!({ "title": name }))).await
            };
            match result {
                Ok(_) => {
                    s.emit(HarnessEvent::Renamed((!name.is_empty()).then_some(name)));
                    reply(true, Value::Null, "");
                }
                Err(e) => reply(false, Value::Null, &format!("{e:#}")),
            }
        }
        // Edit & resend: OpenCode forks the session before that prompt (the
        // original stays); this session goes on in the fork.
        "fork" => {
            let entry = cmd["entryId"].as_str().unwrap_or_default().to_owned();
            let (sid, running) = {
                let st = s.state.lock().unwrap();
                (st.session_id.clone(), st.running)
            };
            if running || sid.is_empty() {
                return reply(false, Value::Null, if running { "Stop OpenCode first." } else { "Nothing to fork yet." });
            }
            let text = if s.remote.is_none() {
                opencode::prompt_text(&entry).unwrap_or_default()
            } else {
                let hub = s.hub.lock().unwrap().clone();
                let (sid2, entry2) = (sid.clone(), entry.clone());
                tokio::task::spawn_blocking(move || {
                    let page = hub.and_then(|h| h.server.get(&format!("/api/session/{sid2}/message?limit=200&order=asc"), None).ok()).unwrap_or_default();
                    page["data"].as_array().into_iter().flatten().find(|m| m["id"] == entry2.as_str()).and_then(|m| m["text"].as_str().map(str::to_owned)).unwrap_or_default()
                })
                .await
                .unwrap_or_default()
            };
            match s.api("POST", format!("/api/session/{sid}/fork"), Some(json!({ "before": entry }))).await {
                Ok(v) => {
                    let id = v["data"]["id"].as_str().or_else(|| v["data"]["info"]["id"].as_str()).or_else(|| v["id"].as_str()).unwrap_or_default().to_owned();
                    if id.is_empty() {
                        return reply(false, Value::Null, "OpenCode didn't fork the session.");
                    }
                    let messages = s.history(&id).await;
                    {
                        let mut st = s.state.lock().unwrap();
                        st.session_id = id.clone();
                        st.messages = messages;
                        st.subagents.clear();
                        st.children.clear();
                        // The fork gets the choices with the next prompt.
                        st.sent_model = None;
                        st.sent_agent.clear();
                    }
                    s.subscribe(&id);
                    reply(true, json!({ "text": text, "cancelled": false }), "");
                }
                Err(e) => reply(false, Value::Null, &format!("{e:#}")),
            }
        }
        // A subagent stops when its session is interrupted.
        "cancel_subagent" => {
            let child = {
                let st = s.state.lock().unwrap();
                st.subagents.get(cmd["subagentId"].as_str().unwrap_or_default()).filter(|e| e["status"] == "running").and_then(|e| e["childSession"].as_str().map(str::to_owned))
            };
            match child {
                Some(child) => {
                    let ok = s.api("POST", format!("/api/session/{child}/interrupt"), None).await.is_ok();
                    reply(true, json!({ "cancelled": ok }), "");
                }
                None => reply(true, json!({ "cancelled": false }), ""),
            }
        }
        _ => reply(false, Value::Null, &format!("OpenCode sessions don't support {ty}")),
    }
}

/// OpenCode's events for this session.
fn event(s: &Arc<Session>, ev: &Value) {
    let d = &ev["data"];
    let mut out = Vec::new();
    {
        let mut st = s.state.lock().unwrap();
        let own = st.session_id.clone();
        let asker = event_session(ev).unwrap_or_else(|| own.clone());
        // A subagent's session: what it does shows on its entry; its
        // permissions and forms are asked here.
        let asks = matches!(ev["type"].as_str(), Some("permission.asked" | "permission.replied" | "form.created" | "form.replied" | "form.cancelled"));
        if !own.is_empty() && asker != own && !asks {
            child_event(s, &mut st, &asker, ev, &mut out);
            drop(st);
            for e in out {
                s.emit(e);
            }
            return;
        }
        match ev["type"].as_str().unwrap_or_default() {
            "session.execution.started" => {
                if !std::mem::replace(&mut st.running, true) {
                    out.push(HarnessEvent::RunStarted);
                }
            }
            "session.execution.succeeded" | "session.execution.failed" | "session.execution.interrupted" => {
                // A restart's "shutdown" isn't the end: OpenCode carries on
                // (after a denied permission it is).
                let declined = st.declined.remove(&own);
                if ev["type"] == "session.execution.interrupted" && d["reason"] == "shutdown" && !declined {
                    return;
                }
                if let Some(step) = st.step.take() {
                    end_step(&mut st, step, None, &mut out);
                }
                if ev["type"] == "session.execution.failed" {
                    out.push(HarnessEvent::Notice { level: NoticeLevel::Error, text: d["error"]["message"].as_str().unwrap_or("OpenCode failed.").to_owned(), system: false });
                }
                if std::mem::take(&mut st.running) {
                    out.push(HarnessEvent::RunEnded);
                    out.push(HarnessEvent::Settled);
                }
            }
            "session.step.started" => {
                // It carried on after the denial.
                st.declined.remove(&own);
                let id = d["assistantMessageID"].as_str().unwrap_or_default().to_owned();
                // A retry starts the same step again.
                if st.step.as_ref().is_some_and(|s| s.id == id) {
                    return;
                }
                if let Some(step) = st.step.take() {
                    end_step(&mut st, step, None, &mut out);
                }
                let m = &d["model"];
                // The model OpenCode chose, when harlo didn't pick one.
                let (provider, model_id) = (m["providerID"].as_str().unwrap_or_default(), m["id"].as_str().unwrap_or_default());
                if !model_id.is_empty() && st.model.id.is_empty() {
                    st.model = Model { provider: provider.to_owned(), id: model_id.to_owned(), variant: String::new() };
                    st.sent_model = Some((provider.to_owned(), model_id.to_owned(), String::new()));
                    out.push(HarnessEvent::ModelChanged(json!({ "provider": provider, "id": model_id, "name": model_id })));
                }
                st.step = Some(Step { id, model: format!("{}/{}", m["providerID"].as_str().unwrap_or_default(), m["id"].as_str().unwrap_or_default()), ..Default::default() });
            }
            "session.step.ended" | "session.step.failed" => {
                if let Some(t) = d["tokens"].as_object() {
                    st.context_tokens = t["input"].as_u64().unwrap_or(0) + t["cache"]["read"].as_u64().unwrap_or(0);
                }
                let error = (ev["type"] == "session.step.failed").then(|| d["error"]["message"].as_str().unwrap_or("The step failed.").to_owned());
                if let Some(step) = st.step.take() {
                    end_step(&mut st, step, error, &mut out);
                }
            }
            "session.usage.updated" => {
                st.cost = d["cost"].as_f64().unwrap_or(st.cost);
            }
            kind @ ("session.reasoning.started" | "session.text.started") => {
                let block = if kind == "session.reasoning.started" { BlockKind::Thinking } else { BlockKind::Text };
                if let Some(step) = st.step.as_mut() {
                    step.show(&mut out);
                    let index = step.blocks.len();
                    step.blocks.push((block, String::new(), String::new(), String::new(), Value::Null));
                    let ord = d["ordinal"].as_u64().unwrap_or(0);
                    if block == BlockKind::Thinking { step.reasoning.insert(ord, index) } else { step.text.insert(ord, index) };
                    out.push(HarnessEvent::BlockStart { id: step.id.clone(), index, kind: block, tool: None });
                }
            }
            kind @ ("session.reasoning.delta" | "session.text.delta" | "session.reasoning.ended" | "session.text.ended") => {
                let thinking = kind.starts_with("session.reasoning");
                if let Some(step) = st.step.as_mut() {
                    let ord = d["ordinal"].as_u64().unwrap_or(0);
                    let index = if thinking { step.reasoning.get(&ord) } else { step.text.get(&ord) }.copied();
                    if let Some(index) = index {
                        let kind_b = if thinking { BlockKind::Thinking } else { BlockKind::Text };
                        if kind.ends_with("delta") {
                            let delta = d["delta"].as_str().unwrap_or_default();
                            step.blocks[index].1.push_str(delta);
                            out.push(HarnessEvent::BlockDelta { id: step.id.clone(), index, kind: kind_b, delta: delta.to_owned() });
                        } else {
                            let text = d["text"].as_str().unwrap_or_default().to_owned();
                            step.blocks[index].1 = text.clone();
                            out.push(HarnessEvent::BlockEnd { id: step.id.clone(), index, kind: kind_b, content: json!(text) });
                        }
                    }
                }
            }
            "session.tool.input.started" => {
                if let Some(step) = st.step.as_mut() {
                    step.show(&mut out);
                    let (call, name) = (d["id"].as_str().unwrap_or_default().to_owned(), d["name"].as_str().unwrap_or_default().to_owned());
                    let index = step.blocks.len();
                    step.blocks.push((BlockKind::ToolCall, String::new(), call.clone(), name.clone(), Value::Null));
                    step.tools.insert(call.clone(), index);
                    out.push(HarnessEvent::BlockStart { id: step.id.clone(), index, kind: BlockKind::ToolCall, tool: Some((call, name)) });
                }
            }
            "session.tool.called" => {
                let mut started = None;
                if let Some(step) = st.step.as_mut() {
                    let call = d["id"].as_str().unwrap_or_default().to_owned();
                    if let Some(&index) = step.tools.get(&call) {
                        step.blocks[index].4 = d["input"].clone();
                        let name = step.blocks[index].3.clone();
                        out.push(HarnessEvent::BlockEnd { id: step.id.clone(), index, kind: BlockKind::ToolCall, content: json!({ "type": "toolCall", "id": call, "name": name, "arguments": d["input"] }) });
                        out.push(HarnessEvent::ToolStart { call_id: call.clone(), name: name.clone(), args: d["input"].clone() });
                        started = opencode::SUBAGENT_TOOLS.contains(&name.as_str()).then_some(call);
                    }
                }
                // A subagent starting.
                if let Some(call) = started {
                    let i = &d["input"];
                    out.push(subagent_patch(&mut st, &call, json!({
                        "name": i["description"].as_str().unwrap_or("Subagent"), "agent": i["agent"].as_str().unwrap_or("general"), "task": i["prompt"],
                        "status": "running", "detached": i["background"] == true,
                    })));
                    if let Some(child) = i["sessionID"].as_str().filter(|c| !c.is_empty()) {
                        link_child(s, &mut st, &call, child, &mut out);
                    }
                }
            }
            // Which session a subagent call runs in.
            "session.tool.progress" => {
                let call = d["id"].as_str().unwrap_or_default().to_owned();
                if st.subagents.contains_key(&call)
                    && let Some(child) = opencode::child_of(d)
                {
                    link_child(s, &mut st, &call, &child, &mut out);
                }
            }
            // A background subagent's report.
            "session.synthetic" => {
                let m = &d["metadata"];
                if m["source"] == "subagent"
                    && let Some(call) = m["childID"].as_str().and_then(|c| st.children.get(c).cloned())
                {
                    let status = match m["state"].as_str() {
                        Some("completed") => "completed",
                        Some("cancelled") => "aborted",
                        _ => "failed",
                    };
                    out.push(subagent_patch(&mut st, &call, json!({ "status": status })));
                }
            }
            kind @ ("session.tool.success" | "session.tool.failed") => {
                let call = d["id"].as_str().unwrap_or_default().to_owned();
                let is_error = kind == "session.tool.failed";
                let text = opencode::tool_text(d);
                // A subagent call's end: it finished, failed, was cancelled,
                // or went on in the background.
                if st.subagents.contains_key(&call) {
                    if let Some(child) = opencode::child_of(d) {
                        link_child(s, &mut st, &call, &child, &mut out);
                    }
                    let patch = if is_error {
                        json!({ "status": if text.contains("Subagent cancelled") { "aborted" } else { "failed" } })
                    } else if d["metadata"]["status"] == "running" {
                        json!({ "detached": true })
                    } else {
                        json!({ "status": "completed" })
                    };
                    if st.subagents[&call]["status"] == "running" {
                        out.push(subagent_patch(&mut st, &call, patch));
                    }
                }
                st.messages.push(json!({ "role": "toolResult", "toolCallId": call, "isError": is_error, "content": [{ "type": "text", "text": text }] }));
                out.push(HarnessEvent::ToolEnd { call_id: call, result: json!({ "content": [{ "type": "text", "text": text }] }), is_error });
            }
            "session.retry.scheduled" => {
                out.push(HarnessEvent::Retry { started: true, attempt: d["attempt"].as_u64().unwrap_or(0), max_attempts: 0, error: d["error"]["message"].as_str().map(str::to_owned) });
            }
            "session.renamed" => out.push(HarnessEvent::Renamed(d["title"].as_str().map(str::to_owned))),
            "session.compaction.started" => out.push(HarnessEvent::Compaction { started: true, reason: None, error: None }),
            "session.compaction.ended" => out.push(HarnessEvent::Compaction { started: false, reason: None, error: None }),
            "session.compaction.failed" => out.push(HarnessEvent::Compaction { started: false, reason: None, error: d["error"]["message"].as_str().map(str::to_owned) }),
            // The shield answers what it covers; the rest is asked.
            "permission.asked" if shield_allows(&st.approval, d["action"].as_str().unwrap_or_default()) => {
                let (s2, rid, asker) = (Arc::clone(s), d["id"].as_str().unwrap_or_default().to_owned(), asker.clone());
                tokio::spawn(async move {
                    let _ = s2.api("POST", format!("/api/session/{asker}/permission/{rid}/reply"), Some(json!({ "decision": "once" }))).await;
                });
            }
            "permission.asked" => {
                let ui = format!("opencode_ui_{}", st.next);
                st.next += 1;
                let action = d["action"].as_str().unwrap_or("tool");
                let resources: Vec<&str> = d["resources"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                st.waiting.insert(ui.clone(), ("permission".into(), d["id"].as_str().unwrap_or_default().to_owned(), Value::Null, asker.clone()));
                // "always": OpenCode remembers it (the patterns in `save`).
                let save: Vec<&str> = d["save"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                let hint = if save.is_empty() { format!("{action}, from now on") } else { save.join(", ") };
                out.push(HarnessEvent::UiRequest(json!({
                    "type": "extension_ui_request", "id": ui, "method": "select",
                    "title": format!("Allow tool: {action}"), "message": resources.join("\n"), "options": ["Approve", "Deny", "Always allow"],
                    "alwaysHint": hint,
                })));
            }
            "permission.replied" => {
                if d["reply"] == "reject" {
                    st.declined.insert(asker.clone());
                }
                let rid = d["requestID"].as_str().unwrap_or_default();
                let ui = st.waiting.iter().find(|(_, w)| w.1 == rid).map(|(k, _)| k.clone());
                if let Some(ui) = ui {
                    st.waiting.remove(&ui);
                    out.push(HarnessEvent::UiCancel { target_id: ui });
                }
            }
            "form.created" => {
                let form = &d["form"];
                let ui = format!("opencode_ui_{}", st.next);
                st.next += 1;
                let questions: Vec<Value> = form["fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|f| f["type"] != "external" && f["hidden"] != true)
                    .map(|f| {
                        let options: Vec<Value> = match f["type"].as_str() {
                            Some("boolean") => vec![json!({ "label": "Yes" }), json!({ "label": "No" })],
                            _ => f["options"].as_array().into_iter().flatten().map(|o| json!({ "label": o["label"], "description": o["description"] })).collect(),
                        };
                        json!({ "id": f["key"], "header": f["title"], "question": f["description"].as_str().or_else(|| f["title"].as_str()).unwrap_or_default(),
                                "options": options, "multi": f["type"] == "multiselect" })
                    })
                    .collect();
                st.waiting.insert(ui.clone(), ("form".into(), form["id"].as_str().unwrap_or_default().to_owned(), form["fields"].clone(), asker.clone()));
                out.push(HarnessEvent::UiRequest(json!({ "type": "extension_ui_request", "id": ui, "method": "ask", "title": form["title"], "questions": questions })));
            }
            "form.replied" | "form.cancelled" => {
                let fid = d["id"].as_str().or_else(|| d["form"]["id"].as_str()).unwrap_or_default();
                let ui = st.waiting.iter().find(|(_, w)| w.1 == fid).map(|(k, _)| k.clone());
                if let Some(ui) = ui {
                    st.waiting.remove(&ui);
                    out.push(HarnessEvent::UiCancel { target_id: ui });
                }
            }
            "session.model.selected" => {
                let m = &d["model"];
                st.model = Model { provider: m["providerID"].as_str().unwrap_or_default().to_owned(), id: m["id"].as_str().unwrap_or_default().to_owned(), variant: m["variant"].as_str().unwrap_or_default().to_owned() };
                out.push(HarnessEvent::ModelChanged(json!({ "provider": st.model.provider, "id": st.model.id, "name": st.model.id })));
            }
            _ => {}
        }
    }
    for e in out {
        s.emit(e);
    }
}

/// harlo's shield lets it through without asking: everything (yolo), or
/// reading and editing files in the project (write).
fn shield_allows(approval: &str, action: &str) -> bool {
    match approval {
        "yolo" | "auto" => true,
        "write" => matches!(action, "edit" | "write" | "patch" | "multiedit" | "read" | "list" | "glob" | "grep"),
        _ => false,
    }
}

/// Merges `patch` into a subagent's entry: the event that tells the dock.
fn subagent_patch(st: &mut State, call: &str, patch: Value) -> HarnessEvent {
    let e = st.subagents.entry(call.to_owned()).or_insert_with(|| json!({ "id": call, "parentToolCallId": call, "canCancel": true, "canSteer": false }));
    if let (Some(dst), Some(src)) = (e.as_object_mut(), patch.as_object()) {
        for (k, v) in src {
            dst.insert(k.clone(), v.clone());
        }
    }
    HarnessEvent::SubagentActivity(json!({ "type": "subagent_lifecycle", "payload": e.clone() }))
}

/// A subagent call's session: its events come here, its transcript is it.
fn link_child(s: &Arc<Session>, st: &mut State, call: &str, child: &str, out: &mut Vec<HarnessEvent>) {
    if st.children.get(child).is_some_and(|c| c == call) {
        return;
    }
    st.children.insert(child.to_owned(), call.to_owned());
    s.subscribe(child);
    out.push(subagent_patch(st, call, json!({ "childSession": child, "sessionFile": s.session_path(child).to_string_lossy() })));
}

/// An event from a subagent's session: the tool it's running, how it ended.
fn child_event(s: &Arc<Session>, st: &mut State, child: &str, ev: &Value, out: &mut Vec<HarnessEvent>) {
    let d = &ev["data"];
    if ev["type"] == "session.created" && !st.children.contains_key(child) {
        // Not linked by its call yet: the running subagent with its title.
        let title = d["title"].as_str().or_else(|| d["info"]["title"].as_str()).unwrap_or_default();
        let call = st.subagents.iter().find(|(c, e)| e["status"] == "running" && e["childSession"].is_null() && e["name"] == title && !st.children.values().any(|v| v == *c)).map(|(c, _)| c.clone());
        if let Some(call) = call {
            link_child(s, st, &call, child, out);
        }
        return;
    }
    let Some(call) = st.children.get(child).cloned() else { return };
    let running = st.subagents.get(&call).is_some_and(|e| e["status"] == "running");
    match ev["type"].as_str().unwrap_or_default() {
        "session.tool.input.started" => {
            st.child_tools.insert(d["id"].as_str().unwrap_or_default().to_owned(), d["name"].as_str().unwrap_or("tool").to_owned());
        }
        "session.tool.called" if running => {
            let name = st.child_tools.get(d["id"].as_str().unwrap_or_default()).cloned().unwrap_or_else(|| "tool".into());
            let mut progress = st.subagents[&call]["progress"].clone();
            if !progress.is_object() {
                progress = json!({});
            }
            progress["currentTool"] = json!(name);
            progress["currentToolArgs"] = d["input"].clone();
            progress["toolCount"] = json!(progress["toolCount"].as_u64().unwrap_or(0) + 1);
            out.push(subagent_patch(st, &call, json!({ "progress": progress })));
        }
        "session.execution.succeeded" if running => out.push(subagent_patch(st, &call, json!({ "status": "completed" }))),
        "session.execution.failed" if running => out.push(subagent_patch(st, &call, json!({ "status": "failed" }))),
        "session.step.started" => {
            st.declined.remove(child);
        }
        "session.execution.interrupted" if running && (d["reason"] != "shutdown" || st.declined.remove(child)) => out.push(subagent_patch(st, &call, json!({ "status": "aborted" }))),
        _ => {}
    }
}

/// A finished step as its message (an error shows on it). A step with
/// nothing in it (a retry's) leaves nothing.
fn end_step(st: &mut State, mut step: Step, error: Option<String>, out: &mut Vec<HarnessEvent>) {
    if step.blocks.is_empty() && error.is_none() {
        return;
    }
    step.show(out);
    let content: Vec<Value> = step
        .blocks
        .iter()
        .map(|(kind, text, call, name, input)| match kind {
            BlockKind::Text => json!({ "type": "text", "text": text }),
            BlockKind::Thinking => json!({ "type": "thinking", "thinking": text }),
            BlockKind::ToolCall => json!({ "type": "toolCall", "id": call, "name": name, "arguments": input }),
        })
        .collect();
    let mut message = json!({ "role": "assistant", "content": content, "model": step.model });
    if let Some(e) = error {
        message["stopReason"] = json!("error");
        message["errorMessage"] = json!(e);
    }
    st.messages.push(message.clone());
    out.push(HarnessEvent::MessageEnd { id: step.id, message });
}

/// harlo's answer to a permission or a form.
async fn answer(s: &Arc<Session>, cmd: &Value) {
    let ui = cmd["id"].as_str().unwrap_or_default();
    let Some((kind, oc_id, fields, sid)) = s.state.lock().unwrap().waiting.remove(ui) else { return };
    if kind == "permission" {
        let decision = if cmd["value"] == "Always allow" {
            "always"
        } else if cmd["value"] == "Approve" || cmd["confirmed"] == true {
            "once"
        } else {
            "reject"
        };
        let _ = s.api("POST", format!("/api/session/{sid}/permission/{oc_id}/reply"), Some(json!({ "decision": decision }))).await;
        return;
    }
    if cmd["cancelled"] == true {
        let _ = s.api("DELETE", format!("/api/session/{sid}/form/{oc_id}"), None).await;
        return;
    }
    // Labels back to the fields' values.
    let mut answer = serde_json::Map::new();
    for a in cmd["answers"].as_array().into_iter().flatten() {
        let key = a["id"].as_str().unwrap_or_default();
        let Some(field) = fields.as_array().into_iter().flatten().find(|f| f["key"] == key) else { continue };
        let picked: Vec<String> = a["selectedOptions"].as_array().into_iter().flatten().filter_map(|l| l.as_str()).map(|label| {
            field["options"].as_array().into_iter().flatten().find(|o| o["label"] == label).and_then(|o| o["value"].as_str()).unwrap_or(label).to_owned()
        }).collect();
        let custom = a["customInput"].as_str().map(str::to_owned);
        let value = match field["type"].as_str() {
            Some("boolean") => json!(picked.first().is_some_and(|p| p == "Yes")),
            Some("multiselect") => json!(picked.into_iter().chain(custom).collect::<Vec<_>>()),
            Some("number") | Some("integer") => custom.as_deref().and_then(|c| c.trim().parse::<f64>().ok()).map_or(Value::Null, |n| json!(n)),
            _ => json!(custom.or_else(|| picked.into_iter().next()).unwrap_or_default()),
        };
        answer.insert(key.to_owned(), value);
    }
    let _ = s.api("POST", format!("/api/session/{sid}/form/{oc_id}/reply"), Some(json!({ "answer": answer }))).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (Arc<Session>, mpsc::UnboundedReceiver<HarnessEvent>) {
        let (events, rx) = mpsc::unbounded_channel();
        let (inbox, _) = mpsc::unbounded_channel();
        (Arc::new(Session { state: Mutex::new(State::default()), events, hub: Mutex::new(None), dir: "/work".into(), program: "opencode".into(), inbox, remote: None }), rx)
    }

    /// A real OpenCode 2.0.25 recording (a shell call and a write, each
    /// approved, retries, then "done") goes through as harlo events.
    #[test]
    fn replays_a_real_session() {
        let (s, mut rx) = session();
        for line in include_str!("../tests/fixtures/opencode-events.jsonl").lines() {
            event(&s, &serde_json::from_str(line).unwrap());
        }
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let texts: Vec<String> = events
            .iter()
            .filter_map(|e| if let HarnessEvent::MessageEnd { message, .. } = e { Some(message.clone()) } else { None })
            .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str().map(str::to_owned))
            .collect();
        assert_eq!(texts, ["done"]);
        let tools: Vec<&str> = events.iter().filter_map(|e| if let HarnessEvent::ToolStart { name, .. } = e { Some(name.as_str()) } else { None }).collect();
        assert_eq!(tools, ["shell", "write"]);
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::ToolEnd { result, .. } if result["content"][0]["text"] == "hello from harlo\n")));
        let asks: Vec<&Value> = events.iter().filter_map(|e| if let HarnessEvent::UiRequest(r) = e { Some(r) } else { None }).collect();
        assert_eq!((asks.len(), asks[0]["title"].as_str(), asks[0]["message"].as_str()), (2, Some("Allow tool: shell"), Some("cat notes.txt")));
        // Replied elsewhere (by the recorder): the cards go.
        assert_eq!(events.iter().filter(|e| matches!(e, HarnessEvent::UiCancel { .. })).count(), 2);
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::Retry { attempt: 2, .. })));
        // The reply starts after its retries, so they show above it.
        let last = |f: fn(&HarnessEvent) -> bool| events.iter().rposition(f).unwrap();
        assert!(last(|e| matches!(e, HarnessEvent::Retry { .. })) < last(|e| matches!(e, HarnessEvent::MessageStart { .. })));
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::Renamed(Some(t)) if t == "Reading notes.txt and creating hi.txt")));
        assert_eq!(events.iter().filter(|e| matches!(e, HarnessEvent::Settled)).count(), 1);
    }

    /// A real OpenCode 2.1.4 recording: a shell call denied. OpenCode ends
    /// with only a "shutdown" interruption, and that's the run's end.
    #[test]
    fn a_denied_permission_ends_the_run() {
        let lines: Vec<Value> = include_str!("../tests/fixtures/opencode-denied.jsonl").lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let (s, mut rx) = session();
        s.state.lock().unwrap().session_id = lines[0]["data"]["sessionID"].as_str().unwrap().to_owned();
        for ev in &lines {
            event(&s, ev);
        }
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::ToolEnd { is_error: true, .. })));
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::RunEnded)));
        assert_eq!(events.iter().filter(|e| matches!(e, HarnessEvent::Settled)).count(), 1);
        assert!(!s.state.lock().unwrap().running);

        // Without the denial, the same interruption is a restart's: still running.
        let (s, mut rx) = session();
        s.state.lock().unwrap().session_id = lines[0]["data"]["sessionID"].as_str().unwrap().to_owned();
        for ev in lines.iter().filter(|e| e["type"] != "permission.replied") {
            event(&s, ev);
        }
        assert!(std::iter::from_fn(|| rx.try_recv().ok()).all(|e| !matches!(e, HarnessEvent::RunEnded)));
        assert!(s.state.lock().unwrap().running);
    }

    #[test]
    fn ids() {
        let a = message_id();
        assert!(a.starts_with("msg_") && a.len() == 4 + 12 + 14, "{a}");
        assert_eq!(split_model("opencode/ling-3.1-flash-free").id, "ling-3.1-flash-free");
    }
}
