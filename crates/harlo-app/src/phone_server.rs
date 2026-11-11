//! The phone server: serves the phone web app (`phone/`) and its encrypted
//! WebSocket (`/ws`). See `harlo_core::phone` for the pairing and crypto.
//!
//! On the socket the phone first sends a plaintext hello with its public
//! key (and, the first time, the pairing code and a device name); harlo
//! answers `ready` with the connection id, or `denied`. Then every frame is
//! sealed. Phone → harlo: `{ id, op, … }` requests (`sessions`, `open`,
//! `send`, `abort`, `answer`; `projects`, `new`, `modes`, `setModes`,
//! `rename`, `archive`, `pin`, `theme`, answered by the workspace; `models`,
//! `setModel`, `thinking`, `setThinking`, `changes`, `diff`, `unqueue`,
//! `promote`, `branch`, `files`, `file`, `branches`, `switchBranch`; `push`,
//! the app's notifications, and `activity`, a Live Activity's); harlo → phone:
//! `{ id, … }` replies and `{ ev, … }` events (`sessions`, `reset`, `rows`,
//! `status`, `meta`, `request`, `cleared`, `commands`, `queue`, `todos`,
//! `subagents`, `aside`).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use harlo_core::phone::{self, Devices, Identity, Session};
use harlo_core::{SessionSink, SessionUpdate};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot, watch};

/// How long a pairing code works.
pub const CODE_LIFETIME: Duration = Duration::from_secs(600);

pub struct Shared {
    pub identity: Identity,
    pub devices: Mutex<Devices>,
    /// Pairing codes and when they stop working.
    pub codes: Mutex<HashMap<String, Instant>>,
    /// Bumped when devices change (a phone paired, one was removed).
    pub devices_changed: watch::Sender<u64>,
    /// This computer's name, shown on the phone.
    pub name: String,
    /// The port phones reach this server on (0: none yet).
    pub port: std::sync::atomic::AtomicU16,
    /// Funnel's and the relay's endpoints while they're online.
    pub remote_endpoints: Mutex<Vec<String>>,
    /// Devices connected now (their connections), which get no push: the
    /// app is open.
    pub online: Mutex<HashMap<String, usize>>,
}

/// A device counted as connected while this lives.
struct Online(Arc<Shared>, String);

impl Online {
    fn new(shared: &Arc<Shared>, id: &str) -> Self {
        *shared.online.lock().unwrap().entry(id.to_owned()).or_default() += 1;
        Self(shared.clone(), id.to_owned())
    }
}

impl Drop for Online {
    fn drop(&mut self) {
        let mut online = self.0.online.lock().unwrap();
        if let Some(n) = online.get_mut(&self.1) {
            *n -= 1;
            if *n == 0 {
                online.remove(&self.1);
            }
        }
    }
}

impl Shared {
    pub fn new_code(&self) -> String {
        let code = phone::random_id(12);
        let mut codes = self.codes.lock().unwrap();
        codes.retain(|_, until| *until > Instant::now());
        codes.insert(code.clone(), Instant::now() + CODE_LIFETIME);
        code
    }

    fn take_code(&self, code: &str) -> bool {
        let mut codes = self.codes.lock().unwrap();
        codes.retain(|_, until| *until > Instant::now());
        codes.remove(code).is_some()
    }

    pub fn bump_devices(&self) {
        self.devices_changed.send_modify(|n| *n += 1);
    }

    /// Where phones can reach harlo now: its addresses (Tailscale first),
    /// then Funnel and the relay. Phones get them when they connect (and in
    /// pairing codes), so one paired at home finds harlo elsewhere.
    pub fn endpoints(&self) -> Vec<String> {
        let port = self.port.load(std::sync::atomic::Ordering::Relaxed);
        let mut list = if port == 0 { Vec::new() } else { crate::phone::direct_endpoints(port) };
        list.extend(self.remote_endpoints.lock().unwrap().iter().cloned());
        list
    }
}

/// A running server; dropping it stops it.
pub struct Server {
    /// Where it listens (the port, when 0 was asked for).
    #[cfg_attr(not(test), allow(dead_code))]
    pub addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

pub fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/", get(|| async { asset("text/html; charset=utf-8", include_str!("../phone/index.html")) }))
        .route("/app.js", get(|| async { asset("text/javascript; charset=utf-8", include_str!("../phone/app.js")) }))
        .route("/style.css", get(|| async { asset("text/css; charset=utf-8", include_str!("../phone/style.css")) }))
        .route("/nacl.js", get(|| async { asset("text/javascript; charset=utf-8", include_str!("../phone/nacl-fast.min.js")) }))
        .route("/manifest.webmanifest", get(|| async { asset("application/manifest+json", include_str!("../phone/manifest.webmanifest")) }))
        .route("/icon.svg", get(|| async { asset("image/svg+xml", include_str!("../icons/app/harlo.svg")) }))
        .route("/ws", get(|ws: WebSocketUpgrade, State(shared): State<Arc<Shared>>| async move { ws.on_upgrade(move |socket| websocket(socket, shared)) }))
        .with_state(shared)
}

/// Listens on `addr` (port 0: any free one).
pub async fn start(addr: SocketAddr, shared: Arc<Shared>) -> anyhow::Result<Server> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let addr = listener.local_addr()?;
    let (stop, stopped) = oneshot::channel::<()>();
    let app = router(shared);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).with_graceful_shutdown(async { let _ = stopped.await; }).await;
    });
    Ok(Server { addr, stop: Some(stop) })
}

/// Forwards a session's updates to a connection.
struct Forward(mpsc::UnboundedSender<Vec<SessionUpdate>>);

impl SessionSink for Forward {
    fn deliver(&self, updates: Vec<SessionUpdate>) {
        let _ = self.0.send(updates);
    }
}

/// Rows of a session a phone gets: the latest, at most (a long session's
/// whole transcript is far too much for a phone; iOS refuses WebSocket
/// messages over 1 MB, the relay over 4 MB).
const PHONE_ROWS: usize = 300;
/// About how big a frame gets: more rows go in more frames.
const FRAME_BYTES: usize = 512 * 1024;
/// A row's text or HTML on a phone, at most (a huge tool output).
const ROW_BYTES: usize = 64 * 1024;

fn clip(s: &str) -> String {
    if s.len() <= ROW_BYTES {
        return s.to_owned();
    }
    let mut end = ROW_BYTES;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… (cut here; the rest is in harlo on your computer)", &s[..end])
}

/// A command's output cut to its end (where its result is), as the
/// desktop shows it.
fn clip_end(s: &str) -> String {
    if s.len() <= ROW_BYTES {
        return s.to_owned();
    }
    let mut start = s.len() - ROW_BYTES;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    // (From a whole line.)
    let start = s[start..].find('\n').map_or(start, |i| start + i + 1);
    format!("… (its start is in harlo on your computer)\n{}", &s[start..])
}

/// A row for a phone: `label` is what a call is for in the agent's words
/// (harlo's own messages: what happened, in a line), `command` a shell
/// call's whole command, `model` the reply's model; `images` the pictures
/// it shows (`harlo_core::pictures::shown`, which a phone asks for with
/// `picture`).
fn row_json(r: &harlo_core::Row, folder: &std::path::Path) -> Value {
    let text = if r.command.is_empty() { clip(&r.text) } else { clip_end(&r.text) };
    json!({ "key": r.key, "kind": r.kind.as_str(), "html": clip(&r.html), "text": text, "title": r.title, "subtitle": r.subtitle, "state": r.state,
        "group": r.group, "summary": r.summary, "groupOpen": r.group_open, "time": r.time, "ended": r.ended, "result": r.result, "diff": clip(&r.diff),
        "runMs": r.run_ms, "label": r.label, "command": clip(&r.command), "model": r.model, "images": harlo_core::pictures::shown(r, folder) })
}

/// The part of a session's rows a phone has: everything after `skipped`.
/// Changes to the whole list become changes to that part.
#[derive(Default)]
struct Window {
    skipped: usize,
    /// The whole list (for what a phone asks about a row: its place among
    /// the user's messages).
    full: Vec<Arc<harlo_core::Row>>,
    /// The session's file (from its meta), for branching and forking.
    file: Option<std::path::PathBuf>,
    /// The session's folder (its rows' pictures are found from it).
    cwd: std::path::PathBuf,
}

impl Window {
    /// The rows to send for a reset (the last `PHONE_ROWS`).
    fn reset<'a, T>(&mut self, rows: &'a [T]) -> &'a [T] {
        self.skipped = rows.len().saturating_sub(PHONE_ROWS);
        &rows[self.skipped..]
    }

    /// A change to the whole list, as a change to the phone's part (None:
    /// it's all before it).
    fn change<T: Clone>(&mut self, c: &harlo_core::listdiff::ListChange<T>) -> Option<harlo_core::listdiff::ListChange<T>> {
        use harlo_core::listdiff::ListChange;
        let s = self.skipped;
        match c {
            ListChange::Update { at, row } => (*at >= s).then(|| ListChange::Update { at: at - s, row: row.clone() }),
            ListChange::Remove { at, count } => {
                let before = s.saturating_sub(*at).min(*count);
                self.skipped -= before;
                (count - before > 0).then(|| ListChange::Remove { at: at.saturating_sub(s), count: count - before })
            }
            ListChange::Insert { at, rows } if *at < s => {
                self.skipped += rows.len();
                None
            }
            ListChange::Insert { at, rows } => Some(ListChange::Insert { at: at - s, rows: rows.clone() }),
        }
    }
}

/// Values in groups of about `FRAME_BYTES`.
fn pieces(values: Vec<Value>) -> Vec<Vec<Value>> {
    let mut out: Vec<Vec<Value>> = vec![Vec::new()];
    let mut size = 0;
    for v in values {
        let n = v.to_string().len();
        if size + n > FRAME_BYTES && !out.last().is_some_and(Vec::is_empty) {
            out.push(Vec::new());
            size = 0;
        }
        size += n;
        out.last_mut().unwrap().push(v);
    }
    out
}

/// A session's rows as phone frames: a reset (the latest rows, `earlier`
/// saying how many came before) or changes, split so no frame is huge.
fn row_frames(key: &str, window: &mut Window, u: &SessionUpdate) -> Vec<Value> {
    use harlo_core::listdiff::ListChange;
    match u {
        SessionUpdate::Reset(rows) => {
            window.full = rows.clone();
            let shown = window.reset(rows);
            let mut frames = Vec::new();
            let mut at = 0;
            for (i, piece) in pieces(shown.iter().map(|r| row_json(r, &window.cwd)).collect()).into_iter().enumerate() {
                let n = piece.len();
                frames.push(if i == 0 {
                    json!({ "ev": "reset", "key": key, "rows": piece, "earlier": window.skipped })
                } else {
                    json!({ "ev": "rows", "key": key, "changes": [{ "op": "insert", "at": at, "rows": piece }] })
                });
                at += n;
            }
            frames
        }
        SessionUpdate::Rows(changes) => {
            harlo_core::listdiff::apply(&mut window.full, changes.clone());
            let mut values = Vec::new();
            let cwd = window.cwd.clone();
            let changes: Vec<_> = changes.iter().filter_map(|c| window.change(c)).collect();
            for c in changes {
                match c {
                    ListChange::Update { at, row } => values.push(json!({ "op": "update", "at": at, "row": row_json(&row, &cwd) })),
                    ListChange::Remove { at, count } => values.push(json!({ "op": "remove", "at": at, "count": count })),
                    ListChange::Insert { at, rows } => {
                        let mut at = at;
                        for piece in pieces(rows.iter().map(|r| row_json(r, &cwd)).collect()) {
                            let n = piece.len();
                            values.push(json!({ "op": "insert", "at": at, "rows": piece }));
                            at += n;
                        }
                    }
                }
            }
            pieces(values).into_iter().filter(|p| !p.is_empty()).map(|p| json!({ "ev": "rows", "key": key, "changes": p })).collect()
        }
        _ => Vec::new(),
    }
}

/// A session update (besides its rows) as a phone event (None: not
/// shown on phones).
fn event(key: &str, u: &SessionUpdate) -> Option<Value> {
    Some(match u {
        SessionUpdate::Status(s) => json!({ "ev": "status", "key": key, "status": s.as_str() }),
        SessionUpdate::Meta(m) => json!({ "ev": "meta", "key": key, "title": if m.name.is_empty() { &m.first_message } else { &m.name }, "model": m.model,
            "modelId": m.model_id, "thinking": m.thinking, "contextTokens": m.context_tokens, "contextWindow": m.context_window, "cost": m.cost,
            // Where it compacts on its own (0 not known); its plan's limit used up.
            "compactAt": m.compact_at, "limitRefused": m.limit_refused }),
        SessionUpdate::Commands(list) => json!({ "ev": "commands", "key": key,
            "commands": list.iter().map(|c| json!({ "name": c.name, "description": c.description, "hint": c.hint })).collect::<Vec<_>>() }),
        SessionUpdate::Queue { steering, follow_up } => json!({ "ev": "queue", "key": key, "steering": steering, "followUp": follow_up }),
        SessionUpdate::Todos(phases) => json!({ "ev": "todos", "key": key, "phases": phases }),
        SessionUpdate::UiRequest(r) => json!({ "ev": "request", "key": key, "request": request_json(r) }),
        SessionUpdate::UiCleared(id) => json!({ "ev": "cleared", "key": key, "id": id }),
        // A side question (/btw) and its answer so far: shown, not kept.
        SessionUpdate::Aside(a) => json!({ "ev": "aside", "key": key, "aside": a }),
        _ => return None,
    })
}

/// What harlo can do for a phone, beyond the first app's (in `ready`):
/// `sessionMentions` (the folder's other sessions for `@`, read in when a
/// message names them),
/// `serverStart` (a message starts its model's server first), `machines`
/// (sessions say their machine), `settled` (settled sessions, for a phone
/// that says it shows them), `limits` (plan limits: the `limits` call and
/// event), `compaction` (a session's meta says where it compacts).
const FEATURES: &[&str] = &["sessionMentions", "serverStart", "machines", "settled", "limits", "compaction", "connect", "modelServers", "putBack", "stoppedModels", "plan", "diagrams", "visuals", "planRun", "helpers", "answerHelper", "team", "pictures"];

/// The session list for one phone: settled sessions only for one that
/// shows them.
fn for_phone(list: Value, shows_settled: bool) -> Value {
    match list {
        Value::Array(rows) if !shows_settled => Value::Array(rows.into_iter().filter(|r| r["settled"].as_str().is_none_or(str::is_empty)).collect()),
        list => list,
    }
}

/// A phone's message (`send`, or `new` with a first message), its
/// model's server started first when harlo can start it and it's stopped,
/// as the desktop sends one. (A server that doesn't start: the message
/// isn't sent, and the reply says why.)
async fn send_after_server(msg: Value) -> Value {
    let has_message = msg["text"].as_str().is_some_and(|t| !t.trim().is_empty()) || msg["images"].as_array().is_some_and(|a| !a.is_empty());
    // A new session starts first, without its message.
    let created = if msg["op"] == "new" {
        let mut bare = msg.clone();
        bare["text"] = json!("");
        bare["images"] = json!([]);
        let created = crate::phone_bridge::call(bare).await;
        if !has_message || created.get("error").is_some() {
            return created;
        }
        Some(created)
    } else {
        None
    };
    let key = created.as_ref().map_or(&msg["key"], |c| &c["key"]).as_str().unwrap_or_default().to_owned();
    let of = crate::phone_bridge::call(json!({ "op": "serverOf", "key": key })).await;
    let model = match of["model"].as_str().unwrap_or_default() {
        "" => created.as_ref().and_then(|c| c["model"].as_str()).unwrap_or_default().to_owned(),
        m => m.to_owned(),
    };
    let (agent, host) = (of["agent"].as_str().unwrap_or_default().to_owned(), of["host"].as_str().unwrap_or_default().to_owned());
    let started = tokio::task::spawn_blocking(move || {
        if agent.is_empty() || model.is_empty() {
            return Ok(());
        }
        let server = if host.is_empty() { crate::accounts::managed_server(&agent, &model) } else { crate::accounts::host_server(&host, &agent, &model) };
        match server.filter(|s| s.needs_start()) {
            Some(s) => s.start().map_err(|e| format!("{} didn't start, so your message wasn't sent: {e}", s.name())),
            None => Ok(()),
        }
    })
    .await
    .unwrap_or(Ok(()));
    let sent = match started {
        Ok(()) => crate::phone_bridge::call(json!({ "op": "send", "key": key, "text": msg["text"], "images": msg["images"], "mode": msg["mode"] })).await,
        Err(e) => json!({ "error": e }),
    };
    match created {
        // (The session started: the phone opens it, with what went wrong.)
        Some(mut created) => {
            if let Some(e) = sent.get("error") {
                created["error"] = e.clone();
            }
            created
        }
        None => sent,
    }
}

/// Connects one of harlo's machines (as picking it on the desktop does):
/// `{ host, home, agents }` once it answers, else why not.
async fn connect_machine(msg: &Value) -> Value {
    use harlo_core::remote;
    let name = msg["host"].as_str().unwrap_or_default().to_owned();
    let Some(host) = remote::host(&name) else { return json!({ "error": "harlo doesn't have that machine." }) };
    let was_off = remote::is_disconnected(&name);
    remote::set_disconnected(&name, false);
    match tokio::task::spawn_blocking(move || remote::check(&host)).await {
        Ok(Ok(_)) => crate::phone_bridge::call(json!({ "op": "hostConnected", "host": name })).await,
        failed => {
            // (Left as it was: harlo's background work stays away from it.)
            if was_off {
                remote::set_disconnected(&name, true);
            }
            let why = match failed {
                Ok(Err(e)) => format!("{e:#}"),
                _ => "It didn't answer.".to_owned(),
            };
            json!({ "error": format!("Couldn't connect to {name}: {why}") })
        }
    }
}

/// The machines' connected now (`""` this computer).
fn connected_machines() -> Vec<String> {
    use harlo_core::remote;
    let mut hosts = vec![String::new()];
    hosts.extend(remote::hosts().into_iter().map(|h| h.name).filter(|n| !remote::is_disconnected(n) && remote::info_of(n).is_some()));
    hosts
}

/// Which of an agent's models (`provider/id`) are on a model server harlo
/// can start that's stopped now: model → that server's name. (Each server
/// asked once.)
async fn stopped_models(agent: String, host: String, models: Vec<String>) -> std::collections::HashMap<String, String> {
    tokio::task::spawn_blocking(move || {
        let mut down: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
        let mut out = std::collections::HashMap::new();
        for id in models {
            let server = if host.is_empty() { crate::accounts::managed_server(&agent, &id) } else { crate::accounts::host_server(&host, &agent, &id) };
            let Some(server) = server else { continue };
            if *down.entry(format!("{server:?}")).or_insert_with(|| server.needs_start()) {
                out.insert(id, server.name().to_owned());
            }
        }
        out
    })
    .await
    .unwrap_or_default()
}

/// A Mermaid diagram drawn for the phone: `{ png (base64), width, height }`
/// (points), in the phone's colours (`colors`: `{ background, surface,
/// text, muted, accent }`).
async fn diagram(msg: &Value) -> Value {
    use base64::Engine as _;
    let (source, light, colors) = (msg["source"].as_str().unwrap_or_default().to_owned(), msg["light"].as_bool().unwrap_or(false), msg["colors"].clone());
    match tokio::task::spawn_blocking(move || crate::diagrams::for_phone(&source, light, &colors)).await {
        Ok(Ok((png, w, h))) => json!({ "png": base64::engine::general_purpose::STANDARD.encode(png), "width": w, "height": h }),
        Ok(Err(e)) => json!({ "error": e }),
        Err(_) => json!({ "error": "merman couldn't draw this diagram." }),
    }
}

/// A mockup (a ```mockup block of the session `key`) for the phone: its
/// pictures, as `visuals::for_phone` gives them.
async fn mockup(msg: &Value) -> Value {
    let cwd = crate::phone_bridge::call(json!({ "op": "sessionCwd", "key": msg["key"] })).await;
    let Some(cwd) = cwd["cwd"].as_str().filter(|c| !c.is_empty()).map(std::path::PathBuf::from) else {
        return json!({ "error": "That session isn't available." });
    };
    let source = msg["source"].as_str().unwrap_or_default().to_owned();
    tokio::task::spawn_blocking(move || crate::visuals::for_phone(&source, &cwd)).await.unwrap_or_else(|_| json!({ "error": "harlo couldn't show this mockup." }))
}

/// A server as phones show it.
fn server_json(s: &Value) -> Value {
    let models: Vec<Value> = s["models"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str().or_else(|| m.as_str())).map(|m| json!(m)).collect();
    json!({ "url": s["baseUrl"], "name": s["name"], "engine": s["engine"], "models": models, "running": s["running"].as_bool().unwrap_or(false) })
}

/// The model servers harlo set up, on this computer and each machine
/// connected: `{ machines: [{ host, servers: [{ url, name, engine, models,
/// running }], error? }] }`.
async fn model_servers() -> Value {
    let asks = connected_machines().into_iter().map(|host| {
        tokio::task::spawn_blocking(move || {
            let found = if host.is_empty() {
                Ok(harlo_core::servers::list())
            } else {
                harlo_core::engine_host::json_if_there(&host, &["servers"], std::time::Duration::from_secs(30))
                    .map(|v| v.and_then(|v| v.as_array().cloned()).unwrap_or_default())
                    .map_err(|e| format!("{e:#}"))
            };
            match found {
                Ok(list) => json!({ "host": host, "servers": list.iter().map(server_json).collect::<Vec<_>>() }),
                Err(e) => json!({ "host": host, "servers": [], "error": e }),
            }
        })
    });
    let mut machines = Vec::new();
    for ask in asks.collect::<Vec<_>>() {
        if let Ok(m) = ask.await {
            machines.push(m);
        }
    }
    json!({ "machines": machines })
}

/// Starts (until it serves) or stops one of harlo's model servers, here or
/// on a machine connected: `{ running }`, or why not.
async fn start_or_stop_server(msg: &Value, start: bool) -> Value {
    let (host, url) = (msg["host"].as_str().unwrap_or_default().to_owned(), msg["url"].as_str().unwrap_or_default().to_owned());
    if !connected_machines().contains(&host) {
        return json!({ "error": format!("Connect to {host} first.") });
    }
    let done = tokio::task::spawn_blocking(move || -> Result<bool, String> {
        if host.is_empty() {
            if !harlo_core::servers::set_up_servers().iter().any(|(u, _)| *u == url) {
                return Err("harlo didn't set up that server.".into());
            }
            if start {
                harlo_core::servers::ensure_running(&url)?;
            } else {
                harlo_core::servers::stop_any(&url);
            }
            return Ok(harlo_core::model_servers::answers(&url));
        }
        let limit = std::time::Duration::from_secs(if start { 20 * 60 } else { 60 });
        let v = harlo_core::engine_host::json(&host, &[if start { "start" } else { "stop" }, &url], limit).map_err(|e| format!("{e:#}"))?;
        if let Some(e) = v["error"].as_str() {
            return Err(e.to_owned());
        }
        Ok(harlo_core::engine_host::json(&host, &["needs-start", &url], std::time::Duration::from_secs(30)).is_ok_and(|v| v == false))
    })
    .await
    .unwrap_or_else(|_| Err("It stopped partway.".into()));
    match done {
        Ok(running) if running != start => json!({ "running": running, "error": if start { "It didn't start." } else { "It still answers." } }),
        Ok(running) => json!({ "running": running }),
        Err(e) => json!({ "error": e }),
    }
}

/// A question or approval, as phones show it.
pub fn request_json(r: &harlo_core::UiRequest) -> Value {
    json!({
        "id": r.id, "method": r.method, "title": r.title, "message": r.message, "options": r.options,
        "placeholder": r.placeholder, "prefill": r.prefill, "approval": r.is_approval, "questions": r.raw["questions"],
    })
}

/// A subagent as phones show it.
fn subagent_json(s: &Value) -> Value {
    json!({ "id": s["id"], "agent": s["agent"], "description": s["description"], "status": s["status"] })
}

/// Edit & resend from the user's `ordinal`-th message, as the desktop
/// does: the session forks there and the message comes back to edit
/// (`text`). (`fork`, a fork that keeps the message, no agent does now.)
async fn branch(live: &crate::registry::LiveRef, file: Option<std::path::PathBuf>, ordinal: Option<usize>, fork: bool) -> Value {
    let Some(ordinal) = ordinal else { return json!({ "error": "That isn't one of your messages." }) };
    if !matches!(live.harness.as_str(), "claude" | "harlo") || fork {
        return json!({ "error": "This agent can't do that." });
    }
    let harness = live.harness.clone();
    let entry = tokio::task::spawn_blocking(move || crate::chat_session::user_entry_id(file, &harness, ordinal)).await.ok().flatten();
    let Some(entry) = entry else { return json!({ "error": "That message can't be branched from yet." }) };
    match live.handle.request(json!({ "type": "fork", "entryId": entry })).await {
        Ok(data) if data["cancelled"].as_bool() == Some(true) => json!({ "cancelled": true }),
        Ok(data) => {
            live.handle.reload();
            json!({ "ok": true, "text": data["text"] })
        }
        Err(e) => json!({ "error": e }),
    }
}

/// Lines of a diff sent to a phone, at most (the rest is cut).
const DIFF_LINES: usize = 4000;

/// A request about a running session that the phone server answers itself.
async fn session_op(op: &str, msg: &Value, live: &crate::registry::LiveRef) -> Value {
    let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
    let failed = |e: String| json!({ "error": e });
    match op {
        "abort" => live.handle.abort(),
        "answer" => live.handle.answer_ui(&text("requestId"), msg["answer"].clone()),
        "setModel" => live.handle.send_raw(json!({ "type": "set_model", "provider": text("provider"), "modelId": text("model") })),
        "setThinking" => live.handle.send_raw(json!({ "type": "set_thinking_level", "level": text("level") })),
        // The queue: take a waiting message back, or send a follow-up now (as steering).
        "unqueue" | "promote" => {
            let command = if op == "unqueue" {
                json!({ "type": "remove_queued_message", "message": text("text"), "queue": text("queue") })
            } else {
                json!({ "type": "promote_queued_message", "message": text("text") })
            };
            if let Err(e) = live.handle.request(command).await {
                return failed(e);
            }
        }
        "models" => {
            let data = match live.handle.request(json!({ "type": "get_available_models" })).await {
                Ok(data) => data,
                Err(e) => return failed(e),
            };
            let mut models: Vec<Value> = data["models"].as_array().into_iter().flatten().map(|m| json!({
                "provider": m["provider"], "id": m["id"], "name": m["name"], "reasoning": m["reasoning"].as_bool() == Some(true),
            })).collect();
            // Those on a model server that's stopped (it starts with the next message).
            let of = crate::phone_bridge::call(json!({ "op": "serverOf", "key": msg["key"] })).await;
            let ids: Vec<String> = models.iter().map(|m| format!("{}/{}", m["provider"].as_str().unwrap_or_default(), m["id"].as_str().unwrap_or_default())).collect();
            let stopped = stopped_models(live.harness.clone(), of["host"].as_str().unwrap_or_default().to_owned(), ids.clone()).await;
            for (m, id) in models.iter_mut().zip(&ids) {
                if let Some(server) = stopped.get(id) {
                    m["stoppedServer"] = json!(server);
                }
            }
            return json!({ "models": models });
        }
        // The session as Markdown (to share).
        "markdown" => {
            return match live.handle.export_markdown().await {
                Some(md) => json!({ "markdown": md }),
                None => failed("That session isn't running.".into()),
            };
        }
        // What it ran and touched (harlo_core::actions), from its messages.
        "actions" => {
            return match live.handle.context_report().await {
                Some(report) => report["actions"].clone(),
                None => failed("That session isn't running.".into()),
            };
        }
        "thinking" => {
            return match live.handle.request(json!({ "type": "get_available_thinking_levels" })).await {
                Ok(data) => json!({ "levels": data["levels"] }),
                Err(e) => failed(e),
            };
        }
        "changes" | "diff" => {
            let (cwd, path, op) = (live.cwd.clone(), text("path"), op.to_owned());
            return tokio::task::spawn_blocking(move || {
                use harlo_core::git;
                let file_json = |f: &git::FileChange, prefix: &str| json!({
                    "path": format!("{prefix}{}", f.path), "from": f.orig_path.as_ref().map(|o| format!("{prefix}{o}")),
                    "status": if f.untracked { '?' } else if f.worktree != '.' { f.worktree } else { f.index },
                    "staged": f.staged(), "untracked": f.untracked, "conflicted": f.conflicted,
                });
                // A folder of repositories: each with changes, its files' paths
                // starting with its place in the folder.
                let Some(root) = git::repo_root(&cwd) else {
                    let repos: Vec<(String, git::GitStatus)> = git::nested_repos(&cwd).into_iter().filter_map(|r| {
                        let rel = r.strip_prefix(&cwd).ok()?.to_string_lossy().into_owned();
                        Some((rel, git::status(&r).ok()?))
                    }).collect();
                    if repos.is_empty() {
                        return json!({ "error": "This session's folder isn't a git repository, and holds none." });
                    }
                    if op == "changes" {
                        let files: Vec<Value> = repos.iter().flat_map(|(rel, st)| st.files.iter().map(move |f| (rel, f))).map(|(rel, f)| file_json(f, &format!("{rel}/"))).collect();
                        let list: Vec<Value> = repos.iter().map(|(rel, st)| json!({ "name": rel, "branch": if st.branch.is_empty() { &st.head } else { &st.branch },
                            "ahead": st.ahead, "behind": st.behind, "changes": st.files.len() })).collect();
                        return json!({ "branch": "", "ahead": 0, "behind": 0, "repos": list, "files": files });
                    }
                    let Some((rel, st)) = repos.iter().filter(|(rel, _)| path.starts_with(&format!("{rel}/"))).max_by_key(|(rel, _)| rel.len()) else {
                        return json!({ "error": "That file has no changes now." });
                    };
                    let inner = &path[rel.len() + 1..];
                    let Some(file) = st.files.iter().find(|f| f.path == inner) else { return json!({ "error": "That file has no changes now." }) };
                    return match git::diff_file(&st.root, &file.path, !file.unstaged(), file.untracked) {
                        Ok(mut d) => {
                            d.path = path.clone();
                            diff_json(&d)
                        }
                        Err(e) => json!({ "error": format!("{e:#}") }),
                    };
                };
                let status = match git::status(&root) {
                    Ok(s) => s,
                    Err(e) => return json!({ "error": format!("{e:#}") }),
                };
                if op == "changes" {
                    return json!({ "branch": status.branch, "ahead": status.ahead, "behind": status.behind, "files": status.files.iter().map(|f| file_json(f, "")).collect::<Vec<_>>() });
                }
                let Some(file) = status.files.iter().find(|f| f.path == path) else { return json!({ "error": "That file has no changes now." }) };
                match git::diff_file(&root, &file.path, !file.unstaged(), file.untracked) {
                    Ok(d) => diff_json(&d),
                    Err(e) => json!({ "error": format!("{e:#}") }),
                }
            })
            .await
            .unwrap_or_else(|e| json!({ "error": e.to_string() }));
        }
        // `@` mentions: the session folder's files matching what's typed.
        "files" => {
            let (root, query) = (live.cwd.clone(), text("query"));
            return tokio::task::spawn_blocking(move || json!({ "files": harlo_core::files::index_for(&root).search(&query, 30) }))
                .await
                .unwrap_or_else(|e| json!({ "error": e.to_string() }));
        }
        "file" => {
            let (cwd, path) = (live.cwd.clone(), text("path"));
            return tokio::task::spawn_blocking(move || file_json(&cwd, &path)).await.unwrap_or_else(|e| json!({ "error": e.to_string() }));
        }
        "branches" | "switchBranch" => {
            let (cwd, name, op) = (live.cwd.clone(), text("name"), op.to_owned());
            let (remote, stash) = (msg["remote"].as_bool() == Some(true), msg["stash"].as_bool() == Some(true));
            return tokio::task::spawn_blocking(move || {
                let Some(root) = harlo_core::git::repo_root(&cwd) else { return json!({ "error": "This session's folder isn't a git repository." }) };
                if op == "switchBranch" {
                    return match harlo_core::git::switch_branch(&root, &name, false, remote, stash) {
                        Ok(said) => json!({ "ok": true, "message": said }),
                        Err(e) => json!({ "error": format!("{e:#}") }),
                    };
                }
                let changed = harlo_core::git::status(&root).map(|s| s.files.len()).unwrap_or(0);
                match harlo_core::git::branches(&root) {
                    Ok(b) => {
                        let entry = |b: &harlo_core::git::BranchInfo| json!({ "name": b.name, "upstream": b.upstream, "ahead": b.ahead, "date": b.date });
                        json!({ "current": b.current, "local": b.local.iter().map(entry).collect::<Vec<_>>(),
                                "remote": b.remote.iter().map(entry).collect::<Vec<_>>(), "changed": changed })
                    }
                    Err(e) => json!({ "error": format!("{e:#}") }),
                }
            })
            .await
            .unwrap_or_else(|e| json!({ "error": e.to_string() }));
        }
        _ => {}
    }
    json!({ "ok": true })
}

/// A path a reply mentions, found in the session's folder (or its
/// repository): the file and its path as shown. Only files inside it.
fn in_folder(cwd: &std::path::Path, path: &str) -> Result<(std::path::PathBuf, String), String> {
    let wanted = std::path::Path::new(path.trim());
    let base = harlo_core::git::repo_root(cwd).unwrap_or_else(|| cwd.to_owned());
    // (On another machine, an absolute path is that machine's.)
    let host = harlo_core::remote::host_of(cwd);
    let absolute = |p: &std::path::Path| match &host {
        Some(h) => std::path::PathBuf::from(harlo_core::remote::join(h, &p.to_string_lossy())),
        None => p.to_owned(),
    };
    let full = if wanted.is_absolute() { absolute(wanted) } else { cwd.join(wanted) };
    // Relative to the repository when not to the folder (agents write either).
    let full = if wanted.is_absolute() || real_path(&full).is_some() { full } else { base.join(wanted) };
    let (Some((real, is_dir)), Some((base_real, _))) = (real_path(&full), real_path(&base)) else { return Err(format!("{path} isn't there.")) };
    let cwd_real = real_path(cwd).map_or_else(|| base_real.clone(), |(p, _)| p);
    if !real.starts_with(&base_real) && !real.starts_with(&cwd_real) {
        return Err("Only files in the session's folder open here.".into());
    }
    if is_dir {
        return Err(format!("{path} is a folder."));
    }
    let shown = real.strip_prefix(&base_real).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.to_owned());
    Ok((real, shown))
}

/// A picture for a phone (`picture`): one the conversation shows
/// (`shown`), else a picture file in the session's folder on this
/// computer; its bytes in base64, type and size.
fn picture_json(cwd: &std::path::Path, path: &str, shown: bool, thumb: bool) -> Value {
    use base64::Engine as _;
    let file = if shown {
        std::path::PathBuf::from(path)
    } else {
        if !harlo_core::pictures::is_picture_name(path) {
            return json!({ "error": format!("{path} isn't a picture.") });
        }
        if harlo_core::remote::host_of(cwd).is_some() {
            return json!({ "error": "Pictures on another machine don't open on the phone." });
        }
        match in_folder(cwd, path) {
            Ok((real, _)) => real,
            Err(e) => return json!({ "error": e }),
        }
    };
    if !file.is_file() {
        return json!({ "error": "That picture is no longer on the computer." });
    }
    match crate::image_view::for_phone(&file.to_string_lossy(), thumb) {
        Some((bytes, kind, w, h)) => json!({ "data": base64::engine::general_purpose::STANDARD.encode(bytes), "type": kind, "width": w, "height": h }),
        None => json!({ "error": "harlo couldn't read that picture." }),
    }
}

/// Lines of a file sent to a phone, at most.
const FILE_LINES: usize = 5000;

/// A file in the session's folder for a phone to read (a path a reply
/// mentions): its lines, highlighted (`.sN` token spans, as code blocks).
/// Only files inside the folder (or its repository).
fn file_json(cwd: &std::path::Path, path: &str) -> Value {
    let (real, shown) = match in_folder(cwd, path) {
        Ok(found) => found,
        Err(e) => return json!({ "error": e }),
    };
    match harlo_core::files::read_text(&real) {
        Ok(harlo_core::files::FileContent::Text(t)) => {
            let lines: Vec<&str> = t.lines().collect();
            let cut = lines.len() > FILE_LINES;
            let lines = &lines[..lines.len().min(FILE_LINES)];
            json!({ "path": shown, "lines": harlo_core::markdown::highlight_lines(&shown, lines), "total": t.lines().count(), "cut": cut })
        }
        Ok(harlo_core::files::FileContent::Binary) => json!({ "error": format!("{shown} isn't text.") }),
        Ok(harlo_core::files::FileContent::TooLarge(n)) => json!({ "error": format!("{shown} is too big to show here ({} MB).", n / 1_000_000) }),
        Err(e) => json!({ "error": format!("{shown}: {e}") }),
    }
}

/// A path with its links resolved, and whether it's a folder; None when
/// it isn't there. On another machine, asked of it (`realpath`).
fn real_path(path: &std::path::Path) -> Option<(std::path::PathBuf, bool)> {
    let Some((name, there)) = harlo_core::remote::split(path) else {
        let real = path.canonicalize().ok()?;
        let dir = real.is_dir();
        return Some((real, dir));
    };
    let host = harlo_core::remote::host(&name)?;
    let q = harlo_core::remote::quote(&there);
    let out = harlo_core::remote::run(&host, &format!("r=$(realpath -e -- {q}) || exit 1; printf '%s\\n' \"$r\"; [ -d \"$r\" ] && echo dir || echo file"), std::time::Duration::from_secs(20)).ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let real = lines.next().filter(|l| l.starts_with('/'))?;
    Some((std::path::PathBuf::from(harlo_core::remote::join(&name, real)), lines.next() == Some("dir")))
}

/// A file's diff for a phone: hunks of lines (`k`: `+`, `-`, ` `, `\\`),
/// cut after `DIFF_LINES` lines.
fn diff_json(d: &harlo_core::git::FileDiff) -> Value {
    use harlo_core::git::LineKind;
    let mut left = DIFF_LINES;
    let mut hunks = Vec::new();
    for h in &d.hunks {
        if left == 0 {
            break;
        }
        // Coloured as code, each side on its own (the old file's lines, then
        // the new one's), so a comment or string started on one side colours
        // the lines after it on that side; `h` is a line's HTML, as a file's.
        let shown = &h.lines[..h.lines.len().min(left)];
        let mut colored: Vec<Option<String>> = vec![None; shown.len()];
        for side in [LineKind::Removed, LineKind::Added] {
            let at: Vec<usize> = (0..shown.len()).filter(|&i| shown[i].kind == side || shown[i].kind == LineKind::Context).collect();
            let texts: Vec<&str> = at.iter().map(|&i| shown[i].text.as_str()).collect();
            for (i, html) in at.into_iter().zip(harlo_core::markdown::highlight_lines(&d.path, &texts)) {
                colored[i].get_or_insert(html);
            }
        }
        let lines: Vec<Value> = shown.iter().zip(colored).map(|(l, html)| {
            let k = match l.kind { LineKind::Added => "+", LineKind::Removed => "-", LineKind::Context => " ", LineKind::NoNewline => "\\" };
            json!({ "k": k, "o": l.old_no, "n": l.new_no, "t": l.text, "h": html })
        }).collect();
        left -= lines.len();
        hunks.push(json!({ "header": h.header, "lines": lines }));
    }
    let total: usize = d.hunks.iter().map(|h| h.lines.len()).sum();
    json!({ "path": d.path, "binary": d.binary, "added": d.added, "removed": d.removed, "hunks": hunks, "cut": total > DIFF_LINES })
}

/// A phone's frames in, harlo's frames out (a WebSocket here, or one of the
/// connections a relay carries).
pub struct Link {
    pub incoming: mpsc::Receiver<String>,
    pub outgoing: mpsc::Sender<String>,
}

/// A direct WebSocket as a link.
async fn websocket(socket: WebSocket, shared: Arc<Shared>) {
    use futures_util::{SinkExt, StreamExt};
    let (mut sink, mut stream) = socket.split();
    let (in_tx, incoming) = mpsc::channel(64);
    let (outgoing, mut out_rx) = mpsc::channel::<String>(64);
    let reader = async move {
        while let Some(Ok(msg)) = stream.next().await {
            match msg {
                Message::Text(t) => {
                    if in_tx.send(t.to_string()).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    };
    let writer = async move {
        while let Some(t) = out_rx.recv().await {
            if sink.send(Message::Text(t.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    };
    // The socket closing ends the reader (the link's incoming closes); the
    // connection ending closes the writer.
    tokio::join!(connection(Link { incoming, outgoing }, shared), async {
        tokio::select! {
            _ = reader => {}
            _ = writer => {}
        }
    });
}

async fn send_plain(link: &Link, v: Value) -> bool {
    link.outgoing.send(v.to_string()).await.is_ok()
}

/// Refuses a phone, then lets it hang up (a few seconds at most): closing
/// at once can lose the reason on the way (Foundation's WebSocket on Linux
/// drops a frame that arrives with the close).
async fn deny(link: &mut Link, reason: &str) {
    if send_plain(link, json!({ "type": "denied", "reason": reason })).await {
        let _ = tokio::time::timeout(Duration::from_secs(3), async { while link.incoming.recv().await.is_some() {} }).await;
    }
}

/// One phone's connection, from its hello to its last frame.
pub async fn connection(mut link: Link, shared: Arc<Shared>) {
    // Hello: the phone's key, and the first time its pairing code.
    let Ok(Some(text)) = tokio::time::timeout(Duration::from_secs(20), link.incoming.recv()).await else { return };
    let hello: Value = serde_json::from_str(&text).unwrap_or_default();
    let key_b64 = hello["key"].as_str().unwrap_or_default().to_owned();
    let Some(key) = phone::unb64(&key_b64).ok().and_then(|b| <[u8; 32]>::try_from(b).ok()) else {
        deny(&mut link, "bad hello").await;
        return;
    };
    let known = shared.devices.lock().unwrap().find_by_key(&key_b64).cloned();
    let device = match known {
        Some(d) => d,
        None => {
            let code = hello["code"].as_str().unwrap_or_default();
            if code.is_empty() || !shared.take_code(code) {
                let reason = if code.is_empty() { "This phone isn't paired (or was removed). Pair it again from harlo." } else { "That pairing code was used or has expired. Show a new QR code in harlo." };
                deny(&mut link, reason).await;
                return;
            }
            let added = shared.devices.lock().unwrap().add(hello["name"].as_str().unwrap_or("Phone"), &key_b64, now());
            match added {
                Ok(d) => {
                    shared.bump_devices();
                    d
                }
                Err(e) => {
                    deny(&mut link, &format!("{e:#}")).await;
                    return;
                }
            }
        }
    };
    shared.devices.lock().unwrap().touch(&device.id, now());
    let conn = phone::random_id(12);
    let mut session = Session::new(shared.identity.channel(&key), conn.clone());
    let ready = json!({ "type": "ready", "conn": conn, "name": shared.name, "device": device.name, "endpoints": shared.endpoints(), "features": FEATURES });
    // What the phone shows of what harlo sends (an older app: none of the
    // later things, so harlo leaves them out).
    let shows_settled = hello["features"].as_array().is_some_and(|f| f.iter().any(|f| f == "settled"));
    if !send_plain(&link, ready).await {
        return;
    }
    let _online = Online::new(&shared, &device.id);

    let mut sessions = crate::phone_bridge::watch_sessions();
    let mut devices = shared.devices_changed.subscribe();
    let mut wants_sessions = false;
    let mut limits = crate::phone_bridge::watch_limits();
    let mut wants_limits = false;
    // (A phone that asked for a plan hears when one changes.)
    let mut plans = crate::phone_bridge::watch_plans();
    let mut wants_plans = false;
    // The open session's subagents, from their frames.
    let mut subagents: Vec<Value> = Vec::new();
    // The open session's rows the phone has.
    let mut window = Window::default();
    // The session this phone has open: its live key, subscription, updates.
    let mut open: Option<(String, u64)> = None;
    let (updates_tx, mut updates) = mpsc::unbounded_channel::<Vec<SessionUpdate>>();
    // Replies that come later (a message waiting on its model's server).
    let (later_tx, mut later) = mpsc::unbounded_channel::<Value>();
    let close_open = |open: &mut Option<(String, u64)>| {
        if let Some((key, sub)) = open.take()
            && let Some(live) = crate::registry::get(&key)
        {
            live.handle.unsubscribe(sub);
        }
    };

    loop {
        let out: Vec<Value> = tokio::select! {
            frame = link.incoming.recv() => {
                let Some(text) = frame else { break };
                let msg = match session.open(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::info!("phone {}: {e:#}", device.name);
                        break;
                    }
                };
                let id = msg["id"].clone();
                let reply = match msg["op"].as_str().unwrap_or_default() {
                    "sessions" => {
                        wants_sessions = true;
                        let _ = crate::phone_bridge::call(json!({ "op": "publish" })).await;
                        json!({ "sessions": for_phone(sessions.borrow_and_update().clone(), shows_settled) })
                    }
                    "open" => {
                        close_open(&mut open);
                        match crate::phone_bridge::open(msg["key"].as_str().unwrap_or_default()).await.and_then(|k| crate::registry::get(&k).map(|l| (k, l))) {
                            Some((key, live)) => {
                                subagents.clear();
                                window = Window { cwd: live.cwd.clone(), ..Window::default() };
                                let sub = live.handle.subscribe(Forward(updates_tx.clone()));
                                open = Some((key.clone(), sub));
                                json!({ "key": key, "harness": live.harness })
                            }
                            None => json!({ "error": "That session isn't available." }),
                        }
                    }
                    // (Off the loop: a model's server may start first.)
                    "send" | "new" => {
                        let (msg, later, id) = (msg.clone(), later_tx.clone(), id.clone());
                        tokio::spawn(async move {
                            let mut reply = send_after_server(msg).await;
                            reply["id"] = id;
                            let _ = later.send(reply);
                        });
                        Value::Null
                    }
                    // (Off the loop: machines and model servers are asked over SSH.)
                    "connect" | "servers" | "stoppedModels" | "startServer" | "stopServer" | "diagram" | "mockup" => {
                        let (msg, later, id) = (msg.clone(), later_tx.clone(), id.clone());
                        tokio::spawn(async move {
                            let mut reply = match msg["op"].as_str().unwrap_or_default() {
                                "connect" => connect_machine(&msg).await,
                                "servers" => model_servers().await,
                                "diagram" => diagram(&msg).await,
                                "mockup" => mockup(&msg).await,
                                "stoppedModels" => {
                                    let models: Vec<String> = msg["models"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(str::to_owned)).collect();
                                    let (agent, host) = (msg["agent"].as_str().unwrap_or_default().to_owned(), msg["host"].as_str().unwrap_or_default().to_owned());
                                    json!({ "stopped": stopped_models(agent, host, models).await })
                                }
                                op => start_or_stop_server(&msg, op == "startServer").await,
                            };
                            reply["id"] = id;
                            let _ = later.send(reply);
                        });
                        Value::Null
                    }
                    "projects" | "sessionMentions" | "limitOf" | "resumeAtReset" | "cancelResume" | "modes" | "setModes" | "rename" | "archive" | "pin" | "putBack" | "reviewStep" | "specs" | "theme" | "setAvatar" => crate::phone_bridge::call(msg.clone()).await,
                    "plan" | "planEdit" | "planRun" | "planStop" | "helpers" | "answerHelper" => {
                        wants_plans = true;
                        crate::phone_bridge::call(msg.clone()).await
                    }
                    // A picture the conversation shows (or a picture file in the
                    // session's folder), as a thumbnail or fitted to a phone's
                    // screen. (Off the loop: it's decoded.)
                    "picture" => match (crate::registry::get(msg["key"].as_str().unwrap_or_default()), &open) {
                        (Some(live), Some((key, _))) if key == msg["key"].as_str().unwrap_or_default() => {
                            let path = msg["path"].as_str().unwrap_or_default().to_owned();
                            let shown = !path.is_empty() && window.full.iter().any(|r| harlo_core::pictures::shown(r, &window.cwd).contains(&path));
                            let (cwd, later, id, thumb) = (live.cwd.clone(), later_tx.clone(), id.clone(), msg["size"].as_str() == Some("thumb"));
                            tokio::task::spawn_blocking(move || {
                                let mut reply = picture_json(&cwd, &path, shown, thumb);
                                reply["id"] = id;
                                let _ = later.send(reply);
                            });
                            Value::Null
                        }
                        _ => json!({ "error": "Open the session first." }),
                    },
                    // Edit & resend (`fork` false) or fork from one of the user's messages.
                    "branch" => match (crate::registry::get(msg["key"].as_str().unwrap_or_default()), &open) {
                        (Some(live), Some((key, _))) if key == msg["key"].as_str().unwrap_or_default() => {
                            let row = msg["row"].as_str().unwrap_or_default();
                            let ordinal = window.full.iter().position(|r| r.key == row).filter(|&i| window.full[i].kind == harlo_core::RowKind::User)
                                .map(|i| window.full[..i].iter().filter(|r| r.is_sent()).count());
                            branch(&live, window.file.clone(), ordinal, msg["fork"].as_bool() == Some(true)).await
                        }
                        _ => json!({ "error": "Open the session first." }),
                    },
                    // Undo a turn: which of the user's messages have a snapshot from before
                    // them (`undoTargets`), what undoing to one does (`undoPlan`), and
                    // doing it (`undo`). (The snapshot is found here, by the message.)
                    op @ ("undoTargets" | "undoPlan" | "undo") => match (crate::registry::get(msg["key"].as_str().unwrap_or_default()), &open) {
                        (Some(live), Some((key, _))) if key == msg["key"].as_str().unwrap_or_default() => {
                            let prompts: Vec<(String, i64)> = window.full.iter().filter(|r| r.is_sent()).map(|r| (r.key.clone(), r.time)).collect();
                            let (cwd, row, op) = (live.cwd.clone(), msg["row"].as_str().unwrap_or_default().to_owned(), op.to_owned());
                            tokio::task::spawn_blocking(move || {
                                let mut reply = undo_op(&op, &cwd, &prompts, &row);
                                // (And whether its changes have diffs: a repository, or a folder of them.)
                                if op == "undoTargets" {
                                    reply["git"] = json!(harlo_core::git::repo_root(&cwd).is_some() || !harlo_core::git::nested_repos(&cwd).is_empty());
                                }
                                reply
                            })
                            .await
                            .unwrap_or_else(|e| json!({ "error": e.to_string() }))
                        }
                        _ => json!({ "error": "Open the session first." }),
                    },
                    op @ ("abort" | "answer" | "models" | "setModel" | "thinking" | "setThinking" | "changes" | "diff" | "actions" | "markdown" | "unqueue" | "promote"
                          | "files" | "file" | "branches" | "switchBranch") => match crate::registry::get(msg["key"].as_str().unwrap_or_default()) {
                        Some(live) => session_op(op, &msg, &live).await,
                        None => json!({ "error": "That session isn't running." }),
                    },
                    // The app's notifications: its token and what it wants.
                    "push" => {
                        if shared.devices.lock().unwrap().set_push(&device.id, &msg) {
                            shared.bump_devices();
                            json!({ "ok": true })
                        } else {
                            json!({ "error": "That isn't a push token." })
                        }
                    }
                    // A Live Activity for a session: its push token ("" ended).
                    "activity" => match crate::phone_activity::register(&shared, &device, msg["key"].as_str().unwrap_or_default(),
                                                                         msg["token"].as_str().unwrap_or_default(), msg["sandbox"].as_bool() == Some(true)) {
                        Ok(()) => json!({ "ok": true }),
                        Err(e) => json!({ "error": e }),
                    },
                    // Plan limits: this computer's and each machine's (then as they change).
                    "limits" => {
                        wants_limits = true;
                        limits.borrow_and_update().clone()
                    }
                    "ping" => json!({ "pong": true }),
                    other => json!({ "error": format!("unknown op {other}") }),
                };
                if reply.is_null() {
                    Vec::new()
                } else {
                    let mut reply = reply;
                    reply["id"] = id;
                    vec![reply]
                }
            }
            Some(reply) = later.recv() => vec![reply],
            Ok(()) = plans.changed(), if wants_plans => {
                plans.borrow_and_update();
                match &open {
                    Some((key, _)) => vec![json!({ "ev": "plan", "key": key })],
                    None => Vec::new(),
                }
            }
            Ok(()) = limits.changed(), if wants_limits => {
                let mut ev = limits.borrow_and_update().clone();
                ev["ev"] = json!("limits");
                vec![ev]
            }
            Ok(()) = sessions.changed(), if wants_sessions => vec![json!({ "ev": "sessions", "sessions": for_phone(sessions.borrow_and_update().clone(), shows_settled) })],
            Some(batch) = updates.recv() => match &open {
                Some((key, _)) => {
                    let mut out: Vec<Value> = Vec::new();
                    for u in &batch {
                        if let SessionUpdate::Meta(m) = u {
                            window.file = m.session_file.clone();
                        }
                        out.extend(row_frames(key, &mut window, u));
                        out.extend(event(key, u));
                    }
                    // Subagents: their frames folded into one list.
                    let frames: Vec<&Value> = batch.iter().filter_map(|u| match u { SessionUpdate::SubagentActivity(f) => Some(f), _ => None }).collect();
                    if !frames.is_empty() {
                        frames.into_iter().for_each(|f| harlo_core::subagents::apply_event(&mut subagents, f));
                        out.push(json!({ "ev": "subagents", "key": key, "subagents": subagents.iter().map(subagent_json).collect::<Vec<_>>() }));
                    }
                    out
                }
                None => Vec::new(),
            },
            Ok(()) = devices.changed() => {
                // Removed while connected: goodbye.
                if shared.devices.lock().unwrap().find_by_key(&key_b64).is_none() {
                    let _ = link.outgoing.send(session.seal(&json!({ "ev": "revoked" }))).await;
                    break;
                }
                Vec::new()
            }
        };
        for v in out {
            if link.outgoing.send(session.seal(&v)).await.is_err() {
                close_open(&mut open);
                return;
            }
        }
    }
    close_open(&mut open);
    shared.devices.lock().unwrap().touch(&device.id, now());
}

/// Undo a turn, for a phone: `undoTargets`, `undoPlan` or `undo` (see the
/// connection's ops), with the open session's messages (key, time).
fn undo_op(op: &str, cwd: &std::path::Path, prompts: &[(String, i64)], row: &str) -> Value {
    use harlo_core::git;
    // Undo turned off (Settings → Git): nothing offered.
    if !harlo_core::session::turn_snapshots_on() {
        return if op == "undoTargets" { json!({ "targets": {} }) } else { json!({ "error": "Undo is off in harlo's Settings → Git." }) };
    }
    // (Its repository, or each in a folder of them.)
    let snaps = git::folder_turn_snapshots(cwd);
    if snaps.is_empty() {
        return if op == "undoTargets" { json!({ "targets": {} }) } else { json!({ "error": "There are no snapshots in this session's folder (it isn't a git repository, or holds none)." }) };
    }
    let times: Vec<i64> = prompts.iter().map(|(_, t)| *t).collect();
    let matched = git::match_turns(&snaps, &times);
    if op == "undoTargets" {
        let targets: serde_json::Map<String, Value> = prompts.iter().zip(&matched).filter_map(|((k, _), s)| Some((k.clone(), json!(s.as_ref()?)))).collect();
        return json!({ "targets": targets });
    }
    let Some(i) = prompts.iter().position(|(k, _)| k == row) else { return json!({ "error": "That message isn't in the session." }) };
    let Some(snap) = matched[i].clone() else { return json!({ "error": "There's no snapshot from before that message." }) };
    let later = prompts.len() - i - 1;
    let result = if op == "undo" { git::folder_undo_turn(cwd, &snap) } else { git::folder_undo_plan(cwd, &snap) };
    match result {
        Ok(p) => json!({ "restore": p.restore, "remove": p.remove, "headMoved": p.head_moved, "later": later, "done": op == "undo" }),
        Err(e) => json!({ "error": format!("{e:#}") }),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A row says what its call is for, its whole command and its reply's
    /// model; a long command's output keeps its end (its result), a long
    /// read its start.
    #[test]
    fn rows_say_more_and_commands_keep_their_end() {
        let long: String = (0..20_000).map(|i| format!("line {i}\n")).collect();
        let mut r = harlo_core::Row { key: "c".into(), kind: harlo_core::transcript::RowKind::Tool, title: "bash".into(), text: long.clone(),
            label: "Run the tests".into(), command: "cargo test".into(), model: "codex/gpt-6.1-sol".into(), ..Default::default() };
        let v = row_json(&r, std::path::Path::new(""));
        assert_eq!((v["label"].as_str(), v["command"].as_str(), v["model"].as_str()), (Some("Run the tests"), Some("cargo test"), Some("codex/gpt-6.1-sol")));
        let text = v["text"].as_str().unwrap();
        assert!(text.ends_with("line 19999\n") && text.starts_with("… (its start is in harlo on your computer)\nline "), "{}", &text[..80]);
        assert!(text.len() <= ROW_BYTES + 60);
        r.command.clear();
        let text = row_json(&r, std::path::Path::new(""))["text"].as_str().unwrap().to_owned();
        assert!(text.starts_with("line 0\n") && text.ends_with("(cut here; the rest is in harlo on your computer)"));
    }

    /// A diff's lines come coloured as code, each side on its own: a block
    /// comment opened in a removed line colours the removed line after it,
    /// not the added one.
    #[test]
    fn diffs_reach_phones_coloured() {
        let dir = std::env::temp_dir().join(format!("harlo-phone-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).current_dir(&dir).output().unwrap();
            assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("a.rs"), "fn main() {\n    /* old\n    still */\n}\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "a"]);
        std::fs::write(dir.join("a.rs"), "fn main() {\n    let x = 1;\n    let y = 2;\n}\n").unwrap();
        let d = harlo_core::git::diff_file(&dir, "a.rs", false, false).unwrap();
        let v = diff_json(&d);
        let lines = v["hunks"][0]["lines"].as_array().unwrap();
        let html = |t: &str| lines.iter().find(|l| l["t"].as_str().unwrap().trim() == t).map(|l| l["h"].as_str().unwrap_or("").to_owned()).unwrap();
        assert!(html("fn main() {").contains("<span class=\"s"), "{lines:?}");
        let (opened, old, new) = (html("/* old"), html("still */"), html("let y = 2;"));
        let comment = &opened[opened.find('<').unwrap()..opened.find('>').unwrap() + 1];
        assert!(comment.starts_with("<span class=\"s") && old.split("</span>").filter(|p| !p.is_empty()).all(|p| p.starts_with(comment)),
                "the comment goes on: {opened} / {old}");
        assert!(new.contains("let</span>"), "the new side isn't in the comment: {new}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A row names its pictures (ones that exist); a phone gets one the
    /// conversation shows, or a picture in the session's folder, as a
    /// thumbnail or fitted to its screen, and nothing else.
    #[test]
    fn pictures_reach_phones() {
        use base64::Engine as _;
        let dir = std::env::temp_dir().join(format!("harlo-phone-pictures-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("out")).unwrap();
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAQAAAADCAIAAAA7ljmRAAAAEElEQVR4nGP4z8AARww4OQD1MQv1NXv7ggAAAABJRU5ErkJggg==")
            .unwrap();
        std::fs::write(dir.join("out/cat.png"), &png).unwrap();
        std::fs::write(dir.join("notes.txt"), "hi").unwrap();
        let r = harlo_core::Row { key: "t".into(), kind: harlo_core::transcript::RowKind::Tool, mentions: vec!["out/cat.png".into(), "out/gone.png".into()], ..Default::default() };
        let cat = dir.join("out/cat.png").to_string_lossy().into_owned();
        assert_eq!(row_json(&r, &dir)["images"], json!([cat]), "only the pictures that are there");
        let v = picture_json(&dir, &cat, true, true);
        assert_eq!((v["width"].as_i64(), v["height"].as_i64(), v["type"].as_str()), (Some(4), Some(3), Some("image/jpeg")), "{v}");
        let bytes = base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().unwrap()).unwrap();
        assert!(bytes.starts_with(&[0xff, 0xd8]), "a JPEG");
        let screen = picture_json(&dir, "out/cat.png", false, false);
        assert_eq!(screen["width"].as_i64(), Some(4), "a picture in the folder, by its path");
        assert_eq!(screen["type"].as_str(), Some("image/png"), "a PNG on screen stays one (a screenshot's text stays sharp)");
        assert!(picture_json(&dir, "notes.txt", false, true)["error"].as_str().unwrap().contains("isn't a picture"));
        let outside = std::env::temp_dir().join(format!("harlo-phone-outside-{}.png", std::process::id()));
        std::fs::write(&outside, &png).unwrap();
        assert!(picture_json(&dir, &outside.to_string_lossy(), false, true)["error"].as_str().unwrap().contains("Only files in the session's folder"));
        assert!(picture_json(&dir, &dir.join("out/gone.png").to_string_lossy(), true, true)["error"].as_str().unwrap().contains("no longer"));
        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A side question's answer goes to phones as it comes.
    #[test]
    fn side_questions_reach_phones() {
        let a = json!({ "question": "why?", "text": "Because.", "done": true, "error": null });
        let ev = event("live:1", &SessionUpdate::Aside(a.clone())).unwrap();
        assert_eq!(ev, json!({ "ev": "aside", "key": "live:1", "aside": a }));
    }

    /// The phone's part follows the whole list through every kind of change
    /// (before it, across its start, inside it).
    #[test]
    fn window_follows_the_list() {
        use harlo_core::listdiff::ListChange;
        let mut full: Vec<usize> = (0..PHONE_ROWS + 50).collect();
        let mut w = Window::default();
        let mut phone: Vec<usize> = w.reset(&full).to_vec();
        assert_eq!((w.skipped, phone.len()), (50, PHONE_ROWS));
        let mut next = 10_000;
        let changes = [
            ListChange::Update { at: 3, row: 0 },
            ListChange::Update { at: 60, row: 0 },
            ListChange::Insert { at: 10, rows: vec![0, 0] },
            ListChange::Insert { at: 52, rows: vec![0] },
            ListChange::Insert { at: full.len() + 3, rows: vec![0, 0, 0] },
            ListChange::Remove { at: 5, count: 3 },
            ListChange::Remove { at: 45, count: 20 },
            ListChange::Remove { at: 100, count: 4 },
        ];
        for c in changes {
            // The whole list, with fresh values for new or updated rows.
            let c = match c {
                ListChange::Update { at, .. } => { next += 1; ListChange::Update { at, row: next } }
                ListChange::Insert { at, rows } => ListChange::Insert { at: at.min(full.len()), rows: rows.iter().map(|_| { next += 1; next }).collect() },
                other => other,
            };
            match &c {
                ListChange::Update { at, row } => full[*at] = *row,
                ListChange::Insert { at, rows } => { full.splice(*at..*at, rows.iter().copied()); }
                ListChange::Remove { at, count } => { full.drain(*at..*at + count); }
            }
            match w.change(&c) {
                Some(ListChange::Update { at, row }) => phone[at] = row,
                Some(ListChange::Insert { at, rows }) => { phone.splice(at..at, rows); }
                Some(ListChange::Remove { at, count }) => { phone.drain(at..at + count); }
                None => {}
            }
            assert_eq!(phone, full[w.skipped..], "after {c:?}");
        }
    }

    /// A long session reaches a phone as its latest rows, in frames of
    /// about half a megabyte, with huge rows cut.
    #[test]
    fn long_sessions_in_small_frames() {
        let row = |i: usize, size: usize| {
            Arc::new(harlo_core::Row {
                key: format!("r{i}"),
                kind: harlo_core::transcript::RowKind::Tool,
                text: "x".repeat(size),
                title: "bash".into(),
                state: "done".into(),
                ..Default::default()
            })
        };
        let mut rows: Vec<Arc<harlo_core::Row>> = (0..2000).map(|i| row(i, 20_000)).collect();
        rows.push(row(2000, 3_000_000));
        let frames = row_frames("live:1", &mut Window::default(), &SessionUpdate::Reset(rows));
        assert_eq!(frames[0]["ev"], "reset");
        assert_eq!(frames[0]["earlier"], 2001 - PHONE_ROWS);
        let sent: usize = frames[0]["rows"].as_array().unwrap().len()
            + frames[1..].iter().map(|f| f["changes"][0]["rows"].as_array().unwrap().len()).sum::<usize>();
        assert_eq!(sent, PHONE_ROWS);
        assert!(frames.len() > 5);
        for f in &frames {
            assert!(f.to_string().len() < 1024 * 1024, "a frame of {} bytes", f.to_string().len());
        }
        let last = frames.last().unwrap()["changes"][0]["rows"].as_array().unwrap().last().unwrap()["text"].as_str().unwrap().to_owned();
        assert!(last.len() < ROW_BYTES + 100 && last.ends_with("in harlo on your computer)"));
    }
    /// Settled sessions only reach a phone that says it shows them.
    #[test]
    fn settled_sessions_for_phones_that_show_them() {
        let list = json!([{ "key": "a", "settled": "" }, { "key": "b", "settled": "merged" }, { "key": "c" }]);
        let keys = |v: Value| v.as_array().unwrap().iter().map(|r| r["key"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        assert_eq!(keys(for_phone(list.clone(), false)), ["a", "c"]);
        assert_eq!(keys(for_phone(list, true)), ["a", "b", "c"]);
    }

    use crypto_box::aead::OsRng;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    pub(crate) fn shared(dir: &std::path::Path) -> Arc<Shared> {
        Arc::new(Shared {
            identity: Identity::load_or_create(&dir.join("identity.json")).unwrap(),
            devices: Mutex::new(Devices::load(&dir.join("devices.json"))),
            codes: Mutex::new(HashMap::new()),
            devices_changed: watch::channel(0).0,
            name: "desk".into(),
            port: Default::default(),
            remote_endpoints: Default::default(),
            online: Default::default(),
        })
    }

    /// The phone's side, in Rust (the web app does the same in JS).
    pub(crate) struct Phone {
        ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        session: Option<Session>,
    }

    impl Phone {
        async fn connect(addr: SocketAddr, secret: &crypto_box::SecretKey, harlo_public: &str, code: &str) -> (Self, Value) {
            Self::connect_url(&format!("ws://{addr}/ws"), secret, harlo_public, code).await
        }

        pub(crate) async fn connect_url(url: &str, secret: &crypto_box::SecretKey, harlo_public: &str, code: &str) -> (Self, Value) {
            let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
            let hello = json!({ "type": "hello", "key": phone::b64(secret.public_key().as_bytes()), "code": code, "name": "Test phone" });
            ws.send(WsMessage::Text(hello.to_string().into())).await.unwrap();
            let WsMessage::Text(t) = ws.next().await.unwrap().unwrap() else { panic!() };
            let ready: Value = serde_json::from_str(&t).unwrap();
            let session = (ready["type"] == "ready").then(|| {
                let harlo: [u8; 32] = phone::unb64(harlo_public).unwrap().try_into().unwrap();
                // The phone's box: its secret with harlo's public key.
                Session::new(phone_channel(secret, &harlo), ready["conn"].as_str().unwrap().to_owned())
            });
            (Phone { ws, session }, ready)
        }

        pub(crate) async fn call(&mut self, msg: Value) -> Value {
            let frame = self.session.as_mut().unwrap().seal(&msg);
            self.ws.send(WsMessage::Text(frame.into())).await.unwrap();
            self.next().await
        }

        pub(crate) async fn next(&mut self) -> Value {
            loop {
                match tokio::time::timeout(Duration::from_secs(5), self.ws.next()).await.unwrap() {
                    Some(Ok(WsMessage::Text(t))) => return self.session.as_mut().unwrap().open(&t).unwrap(),
                    Some(Ok(_)) => continue,
                    _ => return Value::Null,
                }
            }
        }
    }

    fn phone_channel(secret: &crypto_box::SecretKey, harlo: &[u8; 32]) -> phone::Channel {
        // The same box as harlo's, from the phone's keys.
        let id = Identity::from_secret(secret.to_bytes());
        id.channel(harlo)
    }

    #[tokio::test]
    async fn pairs_lists_and_revokes() {
        let dir = std::env::temp_dir().join(format!("harlo-phone-server-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let shared = shared(&dir);
        let server = start("127.0.0.1:0".parse().unwrap(), shared.clone()).await.unwrap();
        let harlo_public = shared.identity.public_b64();
        let secret = crypto_box::SecretKey::generate(&mut OsRng);

        // The web app is served.
        let page = reqwest_get(server.addr, "/").await;
        assert!(page.contains("<title>harlo</title>"), "{page}");

        // Without a code: denied.
        let (_, denied) = Phone::connect(server.addr, &secret, &harlo_public, "").await;
        assert_eq!(denied["type"], "denied");
        // With a code: paired, and the code is used up.
        let code = shared.new_code();
        let (mut phone, ready) = Phone::connect(server.addr, &secret, &harlo_public, &code).await;
        assert_eq!((ready["type"].as_str(), ready["name"].as_str()), (Some("ready"), Some("desk")));
        assert!(ready["features"].as_array().unwrap().iter().any(|f| f == "limits"));
        assert_eq!(shared.devices.lock().unwrap().list.len(), 1);
        let other = crypto_box::SecretKey::generate(&mut OsRng);
        assert_eq!(Phone::connect(server.addr, &other, &harlo_public, &code).await.1["type"], "denied");

        // Sessions (published by the workspace).
        crate::phone_bridge::publish_sessions(json!([{ "key": "live:1", "title": "Fix it" }]));
        let reply = phone.call(json!({ "id": 1, "op": "sessions" })).await;
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["sessions"][0]["title"], "Fix it");
        crate::phone_bridge::publish_sessions(json!([{ "key": "live:1", "title": "Fix it" }, { "key": "live:2", "title": "New" }]));
        let ev = phone.next().await;
        assert_eq!((ev["ev"].as_str(), ev["sessions"].as_array().map(|a| a.len())), (Some("sessions"), Some(2)));

        // Plan limits: asked, then as they change.
        crate::phone_bridge::publish_limits(json!({ "here": { "accounts": [{ "provider": "Claude" }] }, "hosts": {} }));
        let reply = phone.call(json!({ "id": 2, "op": "limits" })).await;
        assert_eq!((reply["id"].as_i64(), reply["here"]["accounts"][0]["provider"].as_str()), (Some(2), Some("Claude")));
        crate::phone_bridge::publish_limits(json!({ "here": { "accounts": [] }, "hosts": { "nas": { "accounts": [] } } }));
        let ev = phone.next().await;
        assert_eq!(ev["ev"], "limits");
        assert!(ev["hosts"]["nas"].is_object());

        // Known now: no code needed.
        let (mut again, ready) = Phone::connect(server.addr, &secret, &harlo_public, "").await;
        assert_eq!(ready["type"], "ready");
        assert_eq!(again.call(json!({ "id": 7, "op": "ping" })).await["pong"], true);

        // Removed: told, disconnected, and refused after.
        let id = shared.devices.lock().unwrap().list[0].id.clone();
        shared.devices.lock().unwrap().remove(&id).unwrap();
        shared.bump_devices();
        assert_eq!(again.next().await["ev"], "revoked");
        assert_eq!(Phone::connect(server.addr, &secret, &harlo_public, "").await.1["type"], "denied");
        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }

    async fn reqwest_get(addr: SocketAddr, path: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        s.write_all(format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }
}
