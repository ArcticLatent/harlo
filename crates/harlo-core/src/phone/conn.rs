//! One phone's connection: its hello, then its requests as the server calls
//! and the server's events as the phone's.

// Rust guideline compliant 2026-10-09

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::response::{Html, Response};
use axum::routing::get;
use futures_util::{SinkExt as _, StreamExt as _};
use harlo_client::{Attachment, Delivery, ModelRef, Prompt};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::aside::{self, Aside};
use super::pictures;
use super::requests;
use super::rows::{self, Rows};
use super::work;
use super::{Hub, Sealed, Server, key_bytes, lock, now_secs, random_id};
use crate::composer::{self, Draft, Outgoing};
use crate::workspace::{Event, EventKind, Message};

/// What the phone calls the agent behind every session: Harlo's own,
/// which the phone app names and draws as Harlo.
const HARNESS: &str = "harlo";

/// The largest message a phone may send: a few photos.
const MAX_MESSAGE: usize = 32 * 1024 * 1024;

/// How long a phone has to say hello.
const HELLO_WAIT: Duration = Duration::from_secs(20);

/// How often changed rows and sessions go out: often enough to look live,
/// seldom enough that streamed text isn't a frame per word.
const FLUSH_EVERY: Duration = Duration::from_millis(120);

/// What every connection runs on: frames from the phone in, frames to it out.
#[derive(Debug)]
pub struct Link {
    /// Frames from the phone; closed when it hangs up.
    pub incoming: mpsc::Receiver<String>,
    /// Frames to the phone; dropping it ends the connection.
    pub outgoing: mpsc::Sender<String>,
}

/// The direct server's routes: `/ws` for phones.
pub(crate) fn router(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/ws", get(upgrade))
        .with_state(hub)
}

/// What a browser shows at a pairing link: the code is for the Harlo app.
async fn page() -> Html<&'static str> {
    Html(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\">\
         <title>Harlo</title><body style=\"font:17px system-ui;margin:2em;max-width:30em\">\
         <h1>Harlo</h1><p>This code pairs a phone with Harlo. Scan it with the Harlo app instead \
         of the camera.</p>",
    )
}

async fn upgrade(ws: WebSocketUpgrade, State(hub): State<Arc<Hub>>) -> Response {
    ws.max_message_size(MAX_MESSAGE)
        .on_upgrade(move |socket| websocket(socket, hub))
}

/// A direct WebSocket as a [`Link`].
async fn websocket(socket: WebSocket, hub: Arc<Hub>) {
    let (mut sink, mut stream) = socket.split();
    let (in_tx, incoming) = mpsc::channel(64);
    let (outgoing, mut out_rx) = mpsc::channel::<String>(64);
    let reader = async move {
        while let Some(Ok(message)) = stream.next().await {
            match message {
                WsMessage::Text(text) => {
                    if in_tx.send(text.to_string()).await.is_err() {
                        break;
                    }
                }
                WsMessage::Close(_) => break,
                _ => {}
            }
        }
    };
    let writer = async move {
        while let Some(text) = out_rx.recv().await {
            if sink.send(WsMessage::Text(text.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    };
    // The socket closing ends the reader, which closes the link's incoming;
    // the connection ending closes the writer.
    tokio::join!(connection(Link { incoming, outgoing }, hub), async {
        tokio::select! {
            () = reader => {}
            () = writer => {}
        }
    });
}

async fn send_plain(link: &Link, v: &Value) -> bool {
    link.outgoing.send(v.to_string()).await.is_ok()
}

/// Turns a phone away, then lets it hang up: closing at once can lose the
/// reason on the way.
async fn deny(link: &mut Link, reason: &str) {
    if send_plain(link, &json!({ "type": "denied", "reason": reason })).await {
        let _ = tokio::time::timeout(Duration::from_secs(3), async {
            while link.incoming.recv().await.is_some() {}
        })
        .await;
    }
}

/// A phone's hello: its device and key, or `None` when it's turned away.
async fn greet(link: &mut Link, hub: &Hub) -> Option<(super::Device, [u8; 32])> {
    let Ok(Some(text)) = tokio::time::timeout(HELLO_WAIT, link.incoming.recv()).await else {
        return None;
    };
    let hello: Value = serde_json::from_str(&text).unwrap_or_default();
    let key_b64 = hello["key"].as_str().unwrap_or_default().to_owned();
    let Ok(key) = key_bytes(&key_b64) else {
        deny(link, "That isn't a phone's hello.").await;
        return None;
    };
    let known = lock(&hub.devices).find_by_key(&key_b64).cloned();
    if let Some(device) = known {
        return Some((device, key));
    }
    let code = hello["code"].as_str().unwrap_or_default();
    if code.is_empty() || !hub.take_code(code) {
        let reason = if code.is_empty() {
            "This phone isn't paired, or was removed. Pair it again from Harlo."
        } else {
            "That pairing code was used or has expired. Show a new QR code in Harlo."
        };
        deny(link, reason).await;
        return None;
    }
    let name = hello["name"].as_str().unwrap_or("Phone");
    let added = lock(&hub.devices).add(name, &key_b64, now_secs());
    match added {
        Ok(device) => {
            hub.bump_devices();
            Some((device, key))
        }
        Err(e) => {
            deny(link, &e).await;
            None
        }
    }
}

/// What this Harlo does for a phone that an older one didn't, by the names
/// the app looks for: a session's pictures, and a refusal's reason passed on
/// to the agent.
const FEATURES: [&str; 2] = ["pictures", "reasons"];

/// One phone's connection, from its hello to its last frame.
pub(crate) async fn connection(mut link: Link, hub: Arc<Hub>) {
    let Some((device, key)) = greet(&mut link, &hub).await else {
        return;
    };
    lock(&hub.devices).touch(&device.id, now_secs());
    let id = random_id(12);
    let sealed = Sealed::new(hub.identity.channel(&key), id.clone());
    let ready = json!({ "type": "ready", "conn": id, "name": hub.name, "device": device.name,
                        "endpoints": hub.endpoints(), "features": FEATURES });
    if !send_plain(&link, &ready).await {
        return;
    }
    let _online = hub.online(&device.id);
    let (later_tx, later) = mpsc::unbounded_channel();
    let mut conn = Conn {
        hub: Arc::clone(&hub),
        device,
        key_b64: super::b64(&key),
        wants_sessions: false,
        sessions_dirty: false,
        open: None,
        later_tx,
    };
    conn.run(link, sealed, later).await;
}

/// Something that finished off the connection's loop.
enum Later {
    /// A reply that took a while.
    Reply(Value),
    /// A session's messages, loaded again.
    Reloaded(String, Vec<Message>),
    /// An event that took a while to put together.
    Event(Value),
}

/// The session a phone has open.
struct Open {
    key: String,
    /// How full its context is, for the phone's context bar.
    context: ContextFill,
    rows: Rows,
    /// The rows the phone has.
    sent: Vec<Value>,
    /// How many earlier rows it doesn't have.
    earlier: usize,
    dirty: bool,
    reloading: bool,
    /// Its background work and goal.
    aside: Aside,
    /// The `jobs` and `todos` events the phone has.
    sent_jobs: Value,
    sent_todos: Value,
}

/// What a phone's connection keeps between its requests.
struct Conn {
    hub: Arc<Hub>,
    device: super::Device,
    key_b64: String,
    wants_sessions: bool,
    sessions_dirty: bool,
    open: Option<Open>,
    later_tx: mpsc::UnboundedSender<Later>,
}

/// A reply that says what went wrong.
fn error(text: impl Into<String>) -> Value {
    json!({ "error": text.into() })
}

/// The status a session shows on a phone.
fn status(server: &Server, id: &str) -> &'static str {
    let workspace = lock(&server.workspace);
    if workspace.needs_user(id) {
        "input"
    } else if workspace.is_busy(id) {
        "busy"
    } else {
        "idle"
    }
}

/// A folder as the phone shows it: under the home folder as `~/…`.
fn tilde(directory: &str) -> String {
    match dirs::home_dir().and_then(|h| {
        let h = h.to_string_lossy().into_owned();
        directory.strip_prefix(&h).map(str::to_owned)
    }) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
        _ => directory.to_owned(),
    }
}

/// What waits on the user in a session: approvals, then questions.
fn waiting(server: &Server, id: &str) -> Vec<Value> {
    let workspace = lock(&server.workspace);
    workspace
        .permissions(id)
        .iter()
        .map(requests::permission)
        .chain(workspace.forms(id).iter().map(requests::form))
        .collect()
}

/// The projects phones see, in the sidebar's order.
fn projects(server: &Server) -> Vec<crate::sessions::Project> {
    lock(&server.workspace).sidebar(SystemTime::now())
}

/// The projects a new session can start in: the sidebar's, without the
/// group of sessions started without one (the phone offers the home
/// folder for that).
fn real_projects(server: &Server) -> Vec<crate::sessions::Project> {
    let mut projects = projects(server);
    projects.retain(|p| !p.other);
    projects
}

/// Every session as the phone's list shows it (the phone's `SessionSummary`).
fn summaries(hub: &Hub, server: &Server) -> Vec<Value> {
    let projects = projects(server);
    let mut out = Vec::new();
    for project in &projects {
        let lists =
            std::iter::once(&project.sessions).chain(project.worktrees.iter().map(|w| &w.sessions));
        for summary in lists.flatten() {
            let (modified, directory, model) = {
                let workspace = lock(&server.workspace);
                let session = workspace.session(&summary.id);
                (
                    session.map_or(0.0, |s| s.updated / 1000.0),
                    session.map(|s| s.directory.clone()).unwrap_or_default(),
                    session.and_then(|s| s.model.clone()),
                )
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "seconds since the epoch fit in i64"
            )]
            let modified = modified as i64;
            out.push(json!({
                "key": summary.id,
                "title": if summary.title.is_empty() { "New session" } else { summary.title.as_str() },
                "project": project.name,
                "harness": HARNESS,
                "status": status(server, &summary.id),
                "modified": modified,
                "where": tilde(&directory),
                "requests": waiting(server, &summary.id),
                "pinned": hub.pinned(&summary.id),
                "modelName": model.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
                "maker": model.map(|m| m.provider_id).unwrap_or_default(),
            }));
        }
    }
    out
}

/// A session's title, model and cost (the phone's `SessionInfo`).
/// How full a session's context is: the latest step's tokens, and the
/// model's window (0 when unknown).
#[derive(Debug, Clone, Copy, Default)]
struct ContextFill {
    tokens: u64,
    window: u64,
}

/// Returns how full session `key`'s context is now.
async fn context_fill(server: &Server, key: &str) -> ContextFill {
    let tokens = server
        .client
        .context(key)
        .await
        .ok()
        .and_then(|c| c.last_step)
        .map_or(0, harlo_client::Tokens::total);
    let (directory, model) = {
        let workspace = lock(&server.workspace);
        let Some(session) = workspace.session(key) else {
            return ContextFill::default();
        };
        (session.directory.clone(), session.model.clone())
    };
    let window = match model {
        Some(m) => crate::server::catalog(&server.client, &directory)
            .await
            .ok()
            .and_then(|c| {
                c.model(&m.provider_id, &m.id)
                    .and_then(|known| known.context)
            })
            .unwrap_or(0),
        None => 0,
    };
    ContextFill { tokens, window }
}

fn meta(server: &Server, key: &str, context: ContextFill) -> Value {
    let workspace = lock(&server.workspace);
    let Some(session) = workspace.session(key) else {
        return json!({ "ev": "meta", "key": key });
    };
    let model = session.model.clone();
    json!({
        "contextTokens": context.tokens, "contextWindow": context.window,
        "ev": "meta", "key": key,
        "title": session.title.clone().unwrap_or_default(),
        "model": model.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
        "modelId": model.as_ref().map(|m| format!("{}/{}", m.provider_id, m.id)).unwrap_or_default(),
        "maker": model.as_ref().map(|m| m.provider_id.clone()).unwrap_or_default(),
        "thinking": model.and_then(|m| m.variant).unwrap_or_default(),
        "cost": session.cost,
    })
}

/// The messages waiting for a session's agent: steering, then follow-ups.
fn queue(server: &Server, key: &str) -> Value {
    let workspace = lock(&server.workspace);
    let texts = |delivery: Delivery| -> Vec<String> {
        workspace
            .inbox(key)
            .iter()
            .filter(|i| i.kind == "user" && i.delivery == delivery)
            .filter_map(|i| i.text.clone())
            .collect()
    };
    json!({ "ev": "queue", "key": key, "steering": texts(Delivery::Steer), "followUp": texts(Delivery::Queue) })
}

/// Photos a phone sent (`[{ data, mimeType }]`, base64) as attachments.
fn photos(list: &Value) -> Vec<Attachment> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter(|p| {
            matches!(
                p["mimeType"].as_str(),
                Some("image/jpeg" | "image/png" | "image/gif" | "image/webp")
            )
        })
        .filter_map(|p| {
            let bytes = super::unb64(p["data"].as_str()?).ok()?;
            Some((p["mimeType"].as_str()?.to_owned(), bytes))
        })
        .enumerate()
        .map(|(i, (mime, bytes))| Attachment::inline(&mime, &bytes, &format!("Photo {}", i + 1)))
        .collect()
}

/// `provider/model` as a model, without a reasoning level.
fn model_ref(text: &str) -> Option<ModelRef> {
    let (provider, id) = text.split_once('/')?;
    Some(ModelRef {
        provider_id: provider.to_owned(),
        id: id.to_owned(),
        variant: None,
    })
}

impl Conn {
    /// Serves the phone until it hangs up or is removed.
    async fn run(
        &mut self,
        mut link: Link,
        mut sealed: Sealed,
        mut later: mpsc::UnboundedReceiver<Later>,
    ) {
        let mut events = self.hub.events.subscribe();
        let mut sessions = self.hub.sessions.subscribe();
        let mut servers = self.hub.server.subscribe();
        let mut devices = self.hub.devices_changed.subscribe();
        let mut tick = tokio::time::interval(FLUSH_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let out: Vec<Value> = tokio::select! {
                frame = link.incoming.recv() => {
                    let Some(text) = frame else { break };
                    let Ok(message) = sealed.open(&text) else { break };
                    self.request(&message).await
                }
                Some(done) = later.recv() => self.finished(done),
                event = events.recv() => match event {
                    Ok(event) => self.event(&event),
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        self.reload();
                        Vec::new()
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                Ok(()) = sessions.changed() => {
                    self.sessions_dirty = true;
                    Vec::new()
                }
                Ok(()) = servers.changed() => {
                    servers.borrow_and_update();
                    self.reload();
                    Vec::new()
                }
                Ok(()) = devices.changed() => {
                    devices.borrow_and_update();
                    if lock(&self.hub.devices).find_by_key(&self.key_b64).is_none() {
                        let _ = link.outgoing.send(sealed.seal(&json!({ "ev": "revoked" }))).await;
                        break;
                    }
                    Vec::new()
                }
                _ = tick.tick() => self.flush(),
            };
            for message in out {
                if link.outgoing.send(sealed.seal(&message)).await.is_err() {
                    return;
                }
            }
        }
        lock(&self.hub.devices).touch(&self.device.id, now_secs());
    }

    /// Changed rows and sessions, as events.
    fn flush(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        if self.sessions_dirty && self.wants_sessions {
            self.sessions_dirty = false;
            if let Some(server) = self.hub.current() {
                out.push(json!({ "ev": "sessions", "sessions": summaries(&self.hub, &server) }));
            }
        }
        if let Some(open) = &mut self.open
            && open.dirty
        {
            open.dirty = false;
            let (now, earlier) = open.rows.for_phone();
            let moved = earlier != open.earlier;
            if moved || open.sent.is_empty() {
                out.push(
                    json!({ "ev": "reset", "key": open.key, "rows": now, "earlier": earlier }),
                );
            } else {
                let changes = rows::changes(&open.sent, &now);
                if !changes.is_empty() {
                    out.push(json!({ "ev": "rows", "key": open.key, "changes": changes }));
                }
            }
            open.sent = now;
            open.earlier = earlier;
            // What stands beside the conversation follows from its rows.
            let waits = self
                .hub
                .current()
                .is_some_and(|s| lock(&s.workspace).needs_user(&open.key));
            let jobs = open.aside.event(&open.key, open.rows.entries(), waits);
            if jobs != open.sent_jobs {
                out.push(jobs.clone());
                open.sent_jobs = jobs;
            }
            let todos = aside::todos_event(&open.key, open.rows.entries());
            if todos != open.sent_todos {
                out.push(todos.clone());
                open.sent_todos = todos;
            }
        }
        out
    }

    /// Loads the open session's messages again, off the loop.
    fn reload(&mut self) {
        let (Some(open), Some(server)) = (&mut self.open, self.hub.current()) else {
            return;
        };
        if open.reloading {
            return;
        }
        open.reloading = true;
        let (key, later) = (open.key.clone(), self.later_tx.clone());
        tokio::spawn(async move {
            if let Ok(messages) = server.client.messages(&key).await {
                let _ = later.send(Later::Reloaded(key, messages));
            }
        });
    }

    fn finished(&mut self, done: Later) -> Vec<Value> {
        match done {
            Later::Reply(reply) => vec![reply],
            Later::Reloaded(key, messages) => {
                if let Some(open) = &mut self.open
                    && open.key == key
                {
                    open.rows.reload(&messages);
                    open.reloading = false;
                    open.dirty = true;
                }
                Vec::new()
            }
            // (Only for the session still open.)
            Later::Event(event) => match &self.open {
                Some(open) if event["key"] == open.key.as_str() => vec![event],
                _ => Vec::new(),
            },
        }
    }

    /// The events for the phone that a server event makes.
    fn event(&mut self, event: &Event) -> Vec<Value> {
        let Some(open) = &mut self.open else {
            return Vec::new();
        };
        if event.kind.session_id() != Some(open.key.as_str()) {
            return Vec::new();
        }
        let key = open.key.clone();
        let reload = open.rows.apply(&event.kind, event.time);
        if let EventKind::StepEnded { tokens, .. } = &event.kind {
            open.context.tokens = tokens.total();
        }
        let context = open.context;
        open.dirty = true;
        let mut out = Vec::new();
        let server = self.hub.current();
        match &event.kind {
            EventKind::Busy { .. } | EventKind::Done { .. } => {
                let status = match (&event.kind, &server) {
                    (EventKind::Busy { .. }, _) => "busy",
                    (_, Some(s)) if lock(&s.workspace).needs_user(&key) => "input",
                    _ => "idle",
                };
                out.push(json!({ "ev": "status", "key": key, "status": status }));
            }
            EventKind::PermissionAsked(p) => {
                out.push(
                    json!({ "ev": "request", "key": key, "request": requests::permission(p) }),
                );
            }
            EventKind::FormCreated(f) => {
                out.push(json!({ "ev": "request", "key": key, "request": requests::form(f) }));
            }
            EventKind::PermissionReplied { request_id, .. } => {
                out.push(json!({ "ev": "cleared", "key": key, "id": request_id }));
            }
            EventKind::FormSettled { form_id, .. } => {
                out.push(json!({ "ev": "cleared", "key": key, "id": form_id }));
            }
            EventKind::SessionRenamed { .. }
            | EventKind::ModelSelected { .. }
            | EventKind::UsageUpdated { .. }
            | EventKind::StepEnded { .. } => {
                if let Some(s) = &server {
                    out.push(meta(s, &key, context));
                }
            }
            EventKind::InboxAdded { .. }
            | EventKind::InboxRemoved { .. }
            | EventKind::InboxDeliveryChanged { .. } => {
                if let Some(s) = &server {
                    out.push(queue(s, &key));
                }
            }
            EventKind::JobUpdated(job) => open.aside.job_changed(job),
            EventKind::GoalChanged { goal, .. } => open.aside.goal_changed(goal.as_ref()),
            _ => {}
        }
        if reload {
            self.reload();
        }
        out
    }

    /// The reply to a phone's request, with its ID, and the events that follow it.
    async fn request(&mut self, message: &Value) -> Vec<Value> {
        let id = message["id"].clone();
        let op = message["op"].as_str().unwrap_or_default();
        let (mut reply, after) = match self.hub.current() {
            // The theme is the desktop's, so it's there without the server.
            _ if op == "theme" => (self.hub.theme(), Vec::new()),
            _ if op == "setAvatar" => {
                let picked = self
                    .hub
                    .pick_avatar(message["avatar"].as_str().unwrap_or_default());
                (
                    picked.unwrap_or_else(|| error("Harlo has no such avatar.")),
                    Vec::new(),
                )
            }
            None if op == "ping" => (json!({ "pong": true }), Vec::new()),
            None => (
                error("Harlo isn't connected to its server right now."),
                Vec::new(),
            ),
            Some(server) => self.answer(op, message, &server).await,
        };
        if reply.is_null() {
            return after;
        }
        reply["id"] = id;
        std::iter::once(reply).chain(after).collect()
    }

    /// Answers one request; `Null` when the reply comes later.
    async fn answer(&mut self, op: &str, m: &Value, server: &Arc<Server>) -> (Value, Vec<Value>) {
        let key = m["key"].as_str().unwrap_or_default();
        let client = &server.client;
        let done = |r: Result<(), harlo_client::Error>| match r {
            Ok(()) => json!({ "ok": true }),
            Err(e) => error(e.to_string()),
        };
        let reply = match op {
            "ping" => json!({ "pong": true }),
            "sessions" => {
                self.wants_sessions = true;
                self.sessions_dirty = false;
                json!({ "sessions": summaries(&self.hub, server) })
            }
            "open" => return self.open(key, server).await,
            "send" => {
                self.spawn_send(m.clone(), Arc::clone(server));
                Value::Null
            }
            "new" => {
                self.spawn_new(m.clone(), Arc::clone(server));
                Value::Null
            }
            "abort" => done(client.interrupt(key).await),
            "answer" => self.reply(key, m, client).await,
            "projects" => choices(server).await,
            "rename" => done(
                client
                    .set_title(key, m["name"].as_str().unwrap_or_default())
                    .await,
            ),
            "archive" => done(
                client
                    .set_archived(key, m["archived"].as_bool().unwrap_or(true))
                    .await,
            ),
            "models" => models(server, key).await,
            "setModel" => {
                let model = ModelRef {
                    provider_id: m["provider"].as_str().unwrap_or_default().to_owned(),
                    id: m["model"].as_str().unwrap_or_default().to_owned(),
                    variant: None,
                };
                done(client.set_model(key, &model).await)
            }
            "thinking" => thinking(server, key).await,
            "setThinking" => {
                let current = lock(&server.workspace)
                    .session(key)
                    .and_then(|s| s.model.clone());
                match current {
                    Some(mut model) => {
                        let level = m["level"].as_str().unwrap_or_default();
                        model.variant = (!level.is_empty()).then(|| level.to_owned());
                        done(client.set_model(key, &model).await)
                    }
                    None => error("The session has no model yet."),
                }
            }
            "activity" => {
                let token = m["token"].as_str().unwrap_or_default();
                let sandbox = m["sandbox"].as_bool() == Some(true);
                match super::activity::register(&self.hub, &self.device, key, token, sandbox) {
                    Ok(()) => json!({ "ok": true }),
                    Err(why) => error(why),
                }
            }
            "pin" => match self.hub.pin(key, m["pinned"].as_bool().unwrap_or(true)) {
                Ok(()) => {
                    self.sessions_dirty = true;
                    json!({ "ok": true })
                }
                Err(e) => error(format!("Couldn't keep the pin: {e}")),
            },
            "background" => done(
                client
                    .move_to_background(key, m["call"].as_str().unwrap_or_default())
                    .await,
            ),
            "jobAction" => {
                match aside::act(client, key, m["row"].as_str().unwrap_or_default()).await {
                    Ok(()) => json!({ "ok": true }),
                    Err(why) => error(why),
                }
            }
            "picture" => self.picture(key, m).await,
            "push" => {
                if lock(&self.hub.devices).set_push(&self.device.id, m) {
                    self.hub.bump_devices();
                    json!({ "ok": true })
                } else {
                    error("That isn't a push token.")
                }
            }
            other => work::answer(other, server, key, m)
                .await
                .unwrap_or_else(|| error(format!("Harlo doesn't do `{other}` from a phone."))),
        };
        (reply, Vec::new())
    }

    /// Opens a session: the reply, then its rows, status, details and what
    /// waits on the user.
    async fn open(&mut self, key: &str, server: &Arc<Server>) -> (Value, Vec<Value>) {
        let messages = match server.client.messages(key).await {
            Ok(messages) => messages,
            Err(e) => {
                return (
                    error(format!("Couldn't open that session: {e}")),
                    Vec::new(),
                );
            }
        };
        let rows = Rows::from_messages(&messages);
        let (now, earlier) = rows.for_phone();
        let context = context_fill(server, key).await;
        let directory = folder_of(server, key);
        let beside = Aside::load(&server.client, key, directory.as_deref()).await;
        let waits = lock(&server.workspace).needs_user(key);
        let jobs = beside.event(key, rows.entries(), waits);
        let todos = aside::todos_event(key, rows.entries());
        let mut after = vec![
            json!({ "ev": "reset", "key": key, "rows": now, "earlier": earlier }),
            json!({ "ev": "status", "key": key, "status": status(server, key) }),
            meta(server, key, context),
            queue(server, key),
            jobs.clone(),
            todos.clone(),
        ];
        after.extend(
            waiting(server, key)
                .into_iter()
                .map(|r| json!({ "ev": "request", "key": key, "request": r })),
        );
        self.open = Some(Open {
            key: key.to_owned(),
            context,
            rows,
            sent: now,
            earlier,
            dirty: false,
            reloading: false,
            aside: beside,
            sent_jobs: jobs,
            sent_todos: todos,
        });
        // What can be typed after "/" in its folder, once the server says.
        if let Some(directory) = directory {
            let (server, key, later) = (Arc::clone(server), key.to_owned(), self.later_tx.clone());
            tokio::spawn(async move {
                if let Ok(catalog) = crate::server::catalog(&server.client, &directory).await {
                    let _ = later.send(Later::Event(commands(&key, &catalog)));
                }
            });
        }
        (json!({ "key": key, "harness": HARNESS }), after)
    }

    /// One of the open session's pictures, drawn to fit a phone, off the
    /// loop: decoding a large one takes a moment.
    async fn picture(&self, key: &str, m: &Value) -> Value {
        let name = m["path"].as_str().unwrap_or_default();
        let found = self
            .open
            .as_ref()
            .filter(|open| open.key == key)
            .and_then(|open| open.rows.picture(name))
            .cloned();
        let Some(picture) = found else {
            return error("That picture isn't in the session any more.");
        };
        let thumb = m["size"] == "thumb";
        tokio::task::spawn_blocking(move || pictures::fitted(&picture, thumb))
            .await
            .unwrap_or_else(|_| Err("Harlo couldn't read that picture.".to_owned()))
            .unwrap_or_else(error)
    }

    /// Answers an approval or a question.
    async fn reply(&self, key: &str, m: &Value, client: &harlo_client::Client) -> Value {
        let request = m["requestId"].as_str().unwrap_or_default();
        let answer = &m["answer"];
        let result = if request.starts_with("per") {
            let reply = requests::permission_reply(answer);
            // A reason goes with a refusal only.
            let reason = (reply == harlo_client::PermissionReply::Reject)
                .then(|| requests::refusal_reason(answer))
                .flatten();
            client.reply_permission(key, request, reply, reason).await
        } else {
            let forms = match client.forms(key).await {
                Ok(forms) => forms,
                Err(e) => return error(e.to_string()),
            };
            let Some(form) = forms.iter().find(|f| f.id == request) else {
                // Answered elsewhere already: nothing is left to do.
                return json!({ "ok": true });
            };
            match requests::form_answers(form, answer) {
                Some(answers) => client.reply_form(key, request, &answers).await,
                None => client.cancel_form(key, request).await,
            }
        };
        match result {
            Ok(()) => json!({ "ok": true }),
            Err(e) if e.is_settled() => json!({ "ok": true }),
            Err(e) => error(e.to_string()),
        }
    }

    /// Sends a phone's message, off the loop.
    fn spawn_send(&self, m: Value, server: Arc<Server>) {
        let later = self.later_tx.clone();
        tokio::spawn(async move {
            let key = m["key"].as_str().unwrap_or_default();
            let mut reply = match send(&server, key, &m).await {
                Ok(()) => json!({ "ok": true }),
                Err(e) => error(e),
            };
            reply["id"] = m["id"].clone();
            let _ = later.send(Later::Reply(reply));
        });
    }

    /// Starts a session with the phone's first message, off the loop.
    fn spawn_new(&self, m: Value, server: Arc<Server>) {
        let later = self.later_tx.clone();
        tokio::spawn(async move {
            let mut reply = match start(&server, &m).await {
                Ok(key) => json!({ "key": key, "harness": HARNESS }),
                Err(e) => error(e),
            };
            reply["id"] = m["id"].clone();
            let _ = later.send(Later::Reply(reply));
        });
    }
}

/// Sends `m`'s text and photos to session `key`, as its `mode` says.
async fn send(server: &Server, key: &str, m: &Value) -> Result<(), String> {
    let text = m["text"].as_str().unwrap_or_default();
    let files = photos(&m["images"]);
    if text.trim().is_empty() && files.is_empty() {
        return Err("The message is empty.".to_owned());
    }
    let mode = m["mode"].as_str().unwrap_or_default();
    if mode == "stopAndSend" {
        server
            .client
            .interrupt(key)
            .await
            .map_err(|e| e.to_string())?;
    }
    let delivery = if mode == "followUp" {
        Delivery::Queue
    } else {
        Delivery::Steer
    };
    let client = &server.client;
    let sent = match typed(server, key, text, delivery).await {
        Outgoing::Command { name, arguments } => {
            client.run_command(key, &name, &arguments, delivery).await
        }
        Outgoing::Compact => client.compact(key).await,
        Outgoing::Prompt(mut prompt) => {
            prompt.files.extend(files);
            client.prompt(key, &prompt).await
        }
        // The phone has no side questions or commands of its own to run:
        // those go to the agent as they were typed.
        Outgoing::Aside(_) | Outgoing::Shell(_) => {
            let mut prompt = Prompt::new(text, delivery);
            prompt.files = files;
            client.prompt(key, &prompt).await
        }
    };
    sent.map_err(|e| e.to_string())
}

/// What a phone's message is: a prompt, or, when it starts with the name
/// of one of the folder's commands or skills, that, as the window reads
/// what's typed in it.
async fn typed(server: &Server, key: &str, text: &str, delivery: Delivery) -> Outgoing {
    let plain = || Outgoing::Prompt(Prompt::new(text, delivery));
    if !text.trim_start().starts_with('/') {
        return plain();
    }
    let Some(directory) = folder_of(server, key) else {
        return plain();
    };
    match crate::server::catalog(&server.client, &directory).await {
        Ok(catalog) => composer::outgoing(text, &Draft::default(), &catalog, &directory, delivery),
        Err(_) => plain(),
    }
}

/// The `commands` event for session `key`: what can be typed after "/" in
/// its folder, as the window's menu lists it.
fn commands(key: &str, catalog: &crate::server::Catalog) -> Value {
    let mut list: Vec<Value> = catalog
        .commands
        .iter()
        .map(
            |c| json!({ "name": c.name, "description": c.description.clone().unwrap_or_default() }),
        )
        .collect();
    if !catalog.commands.iter().any(|c| c.name == composer::COMPACT) {
        list.push(json!({ "name": composer::COMPACT,
                          "description": "Summarize the conversation to free up context" }));
    }
    list.extend(catalog.skills.iter().map(|s| {
        json!({ "name": s.name,
                "description": s.description.clone().unwrap_or_else(|| "Skill".to_owned()) })
    }));
    json!({ "ev": "commands", "key": key, "commands": list })
}

/// Starts a session in the chosen project with its first message.
async fn start(server: &Server, m: &Value) -> Result<String, String> {
    let index = m["project"].as_i64().unwrap_or(-1);
    let directory = match usize::try_from(index) {
        Ok(i) => real_projects(server)
            .get(i)
            .map(|p| p.directory.clone())
            .ok_or_else(|| "That project is gone.".to_owned())?,
        Err(_) if server.local => dirs::home_dir()
            .map(|h| h.to_string_lossy().into_owned())
            .ok_or_else(|| "Choose a project for the session.".to_owned())?,
        Err(_) => return Err("Choose a project for the session.".to_owned()),
    };
    let client = &server.client;
    let session = client
        .create_session(&directory)
        .await
        .map_err(|e| e.to_string())?;
    let agent = m["agent"].as_str().unwrap_or_default();
    if !agent.is_empty() {
        client
            .set_agent(&session.id, agent)
            .await
            .map_err(|e| e.to_string())?;
    }
    if let Some(model) = model_ref(m["model"].as_str().unwrap_or_default()) {
        client
            .set_model(&session.id, &model)
            .await
            .map_err(|e| e.to_string())?;
    }
    // The approval mode picked on the phone, before its first message runs.
    let approval = m["approval"].as_str().unwrap_or_default();
    if !approval.is_empty() {
        super::work::put_in_mode(server, &session.id, approval).await?;
    }
    send(server, &session.id, m).await?;
    Ok(session.id)
}

/// The folder a session works in.
fn folder_of(server: &Server, key: &str) -> Option<String> {
    lock(&server.workspace)
        .session(key)
        .map(|s| s.directory.clone())
}

/// What a new session can be: the projects, agents and models.
async fn choices(server: &Server) -> Value {
    let projects = real_projects(server);
    let first = projects.first().map(|p| p.directory.clone()).or_else(|| {
        server
            .local
            .then(|| dirs::home_dir().map(|h| h.to_string_lossy().into_owned()))
            .flatten()
    });
    let catalog = match &first {
        Some(directory) => crate::server::catalog(&server.client, directory)
            .await
            .unwrap_or_default(),
        None => crate::server::Catalog::default(),
    };
    let agents: Vec<Value> = catalog
        .primary_agents()
        .map(|a| json!({ "id": a.id, "label": a.name }))
        .collect();
    let models: Vec<Value> = catalog
        .models
        .iter()
        .map(|m| json!({ "id": format!("{}/{}", m.provider_id, m.id), "name": m.name }))
        .collect();
    let agent_models: serde_json::Map<String, Value> = catalog
        .primary_agents()
        .map(|a| (a.id.clone(), Value::from(models.clone())))
        .collect();
    // A new session starts with no rules of its own: the agent's ("write").
    let approvals: serde_json::Map<String, Value> = catalog
        .primary_agents()
        .map(|a| (a.id.clone(), Value::from("write")))
        .collect();
    let list: Vec<Value> = projects
        .iter()
        .enumerate()
        .map(|(i, p)| {
            json!({ "id": i, "name": p.name, "where": tilde(&p.directory), "approvals": approvals })
        })
        .collect();
    json!({
        "projects": list,
        "agents": agents,
        "defaultAgent": catalog.primary_agents().next().map(|a| a.id.clone()).unwrap_or_default(),
        "models": [],
        "agentModels": agent_models,
        "approvals": approvals,
    })
}

/// The models a session can switch to.
async fn models(server: &Server, key: &str) -> Value {
    let Some(directory) = folder_of(server, key) else {
        return error("That session is gone.");
    };
    match crate::server::catalog(&server.client, &directory).await {
        Ok(catalog) => {
            let models: Vec<Value> = catalog
                .models
                .iter()
                .map(|m| json!({ "provider": m.provider_id, "id": m.id, "name": m.name,
                                 "reasoning": !m.variants.is_empty(), "providerName": m.provider_id }))
                .collect();
            json!({ "models": models })
        }
        Err(e) => error(e.to_string()),
    }
}

/// The reasoning levels the session's model offers.
async fn thinking(server: &Server, key: &str) -> Value {
    let (Some(directory), Some(model)) = (
        folder_of(server, key),
        lock(&server.workspace)
            .session(key)
            .and_then(|s| s.model.clone()),
    ) else {
        return json!({ "levels": [] });
    };
    match crate::server::catalog(&server.client, &directory).await {
        Ok(catalog) => json!({
            "levels": catalog.model(&model.provider_id, &model.id).map(|m| m.variants.clone()).unwrap_or_default(),
        }),
        Err(e) => error(e.to_string()),
    }
}
