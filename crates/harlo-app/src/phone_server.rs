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
//! `subagents`).

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

fn row_json(r: &harlo_core::Row) -> Value {
    json!({ "key": r.key, "kind": r.kind.as_str(), "html": clip(&r.html), "text": clip(&r.text), "title": r.title, "subtitle": r.subtitle, "state": r.state,
        "group": r.group, "summary": r.summary, "groupOpen": r.group_open, "time": r.time, "ended": r.ended, "result": r.result, "diff": clip(&r.diff),
        "runMs": r.run_ms })
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
            for (i, piece) in pieces(shown.iter().map(|r| row_json(r)).collect()).into_iter().enumerate() {
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
            for c in changes.iter().filter_map(|c| window.change(c)) {
                match c {
                    ListChange::Update { at, row } => values.push(json!({ "op": "update", "at": at, "row": row_json(&row) })),
                    ListChange::Remove { at, count } => values.push(json!({ "op": "remove", "at": at, "count": count })),
                    ListChange::Insert { at, rows } => {
                        let mut at = at;
                        for piece in pieces(rows.iter().map(|r| row_json(r)).collect()) {
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
            "modelId": m.model_id, "thinking": m.thinking, "contextTokens": m.context_tokens, "contextWindow": m.context_window, "cost": m.cost }),
        SessionUpdate::Commands(list) => json!({ "ev": "commands", "key": key,
            "commands": list.iter().map(|c| json!({ "name": c.name, "description": c.description, "hint": c.hint })).collect::<Vec<_>>() }),
        SessionUpdate::Queue { steering, follow_up } => json!({ "ev": "queue", "key": key, "steering": steering, "followUp": follow_up }),
        SessionUpdate::Todos(phases) => json!({ "ev": "todos", "key": key, "phases": phases }),
        SessionUpdate::UiRequest(r) => json!({ "ev": "request", "key": key, "request": request_json(r) }),
        SessionUpdate::UiCleared(id) => json!({ "ev": "cleared", "key": key, "id": id }),
        _ => return None,
    })
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

/// Edit & resend (the session branches there, or a pi session forks, and
/// the message comes back to edit: `text`) or, omp, fork from the user's
/// `ordinal`-th message, as the desktop does.
async fn branch(live: &crate::registry::LiveRef, file: Option<std::path::PathBuf>, ordinal: Option<usize>, fork: bool) -> Value {
    let Some(ordinal) = ordinal else { return json!({ "error": "That isn't one of your messages." }) };
    if !matches!(live.harness.as_str(), "pi" | "omp" | "claude" | "opencode" | "hermes") || (fork && live.harness != "omp") {
        return json!({ "error": "This agent can't do that." });
    }
    let harness = live.harness.clone();
    let entry = tokio::task::spawn_blocking(move || crate::chat_session::user_entry_id(file, &harness, ordinal)).await.ok().flatten();
    let Some(entry) = entry else { return json!({ "error": "That message can't be branched from yet." }) };
    let kind = if live.harness == "omp" && !fork { "branch" } else { "fork" };
    match live.handle.request(json!({ "type": kind, "entryId": entry })).await {
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
        // omp's queue: take a waiting message back, or send a follow-up now (as steering).
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
            return match live.handle.request(json!({ "type": "get_available_models" })).await {
                Ok(data) => json!({ "models": data["models"].as_array().into_iter().flatten().map(|m| json!({
                    "provider": m["provider"], "id": m["id"], "name": m["name"], "reasoning": m["reasoning"].as_bool() == Some(true),
                })).collect::<Vec<_>>() }),
                Err(e) => failed(e),
            };
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

/// Lines of a file sent to a phone, at most.
const FILE_LINES: usize = 5000;

/// A file in the session's folder for a phone to read (a path a reply
/// mentions): its lines, highlighted (`.sN` token spans, as code blocks).
/// Only files inside the folder (or its repository).
fn file_json(cwd: &std::path::Path, path: &str) -> Value {
    let wanted = std::path::Path::new(path.trim());
    let base = harlo_core::git::repo_root(cwd).unwrap_or_else(|| cwd.to_owned());
    let full = if wanted.is_absolute() { wanted.to_owned() } else { cwd.join(wanted) };
    // Relative to the repository when not to the folder (agents write either).
    let full = if full.exists() || wanted.is_absolute() { full } else { base.join(wanted) };
    let (Ok(real), Ok(base_real)) = (full.canonicalize(), base.canonicalize()) else { return json!({ "error": format!("{path} isn't there.") }) };
    let cwd_real = cwd.canonicalize().unwrap_or_else(|_| base_real.clone());
    if !real.starts_with(&base_real) && !real.starts_with(&cwd_real) {
        return json!({ "error": "Only files in the session's folder open here." });
    }
    if real.is_dir() {
        return json!({ "error": format!("{path} is a folder.") });
    }
    let shown = real.strip_prefix(&base_real).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.to_owned());
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
        let lines: Vec<Value> = h.lines.iter().take(left).map(|l| {
            let k = match l.kind { LineKind::Added => "+", LineKind::Removed => "-", LineKind::Context => " ", LineKind::NoNewline => "\\" };
            json!({ "k": k, "o": l.old_no, "n": l.new_no, "t": l.text })
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
    let ready = json!({ "type": "ready", "conn": conn, "name": shared.name, "device": device.name, "endpoints": shared.endpoints() });
    if !send_plain(&link, ready).await {
        return;
    }
    let _online = Online::new(&shared, &device.id);

    let mut sessions = crate::phone_bridge::watch_sessions();
    let mut devices = shared.devices_changed.subscribe();
    let mut wants_sessions = false;
    // The open session's subagents, from their frames.
    let mut subagents: Vec<Value> = Vec::new();
    // The open session's rows the phone has.
    let mut window = Window::default();
    // The session this phone has open: its live key, subscription, updates.
    let mut open: Option<(String, u64)> = None;
    let (updates_tx, mut updates) = mpsc::unbounded_channel::<Vec<SessionUpdate>>();
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
                        json!({ "sessions": sessions.borrow_and_update().clone() })
                    }
                    "open" => {
                        close_open(&mut open);
                        match crate::phone_bridge::open(msg["key"].as_str().unwrap_or_default()).await.and_then(|k| crate::registry::get(&k).map(|l| (k, l))) {
                            Some((key, live)) => {
                                subagents.clear();
                                window = Window::default();
                                let sub = live.handle.subscribe(Forward(updates_tx.clone()));
                                open = Some((key.clone(), sub));
                                json!({ "key": key, "harness": live.harness })
                            }
                            None => json!({ "error": "That session isn't available." }),
                        }
                    }
                    "send" | "projects" | "new" | "modes" | "setModes" | "rename" | "archive" | "pin" | "theme" | "setAvatar" => crate::phone_bridge::call(msg.clone()).await,
                    // Edit & resend (`fork` false) or fork from one of the user's messages.
                    "branch" => match (crate::registry::get(msg["key"].as_str().unwrap_or_default()), &open) {
                        (Some(live), Some((key, _))) if key == msg["key"].as_str().unwrap_or_default() => {
                            let row = msg["row"].as_str().unwrap_or_default();
                            let ordinal = window.full.iter().position(|r| r.key == row).filter(|&i| window.full[i].kind == harlo_core::RowKind::User)
                                .map(|i| window.full[..i].iter().filter(|r| r.kind == harlo_core::RowKind::User).count());
                            branch(&live, window.file.clone(), ordinal, msg["fork"].as_bool() == Some(true)).await
                        }
                        _ => json!({ "error": "Open the session first." }),
                    },
                    // Undo a turn: which of the user's messages have a snapshot from before
                    // them (`undoTargets`), what undoing to one does (`undoPlan`), and
                    // doing it (`undo`). (The snapshot is found here, by the message.)
                    op @ ("undoTargets" | "undoPlan" | "undo") => match (crate::registry::get(msg["key"].as_str().unwrap_or_default()), &open) {
                        (Some(live), Some((key, _))) if key == msg["key"].as_str().unwrap_or_default() => {
                            let prompts: Vec<(String, i64)> = window.full.iter().filter(|r| r.kind == harlo_core::RowKind::User).map(|r| (r.key.clone(), r.time)).collect();
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
                    "ping" => json!({ "pong": true }),
                    other => json!({ "error": format!("unknown op {other}") }),
                };
                let mut reply = reply;
                reply["id"] = id;
                vec![reply]
            }
            Ok(()) = sessions.changed(), if wants_sessions => vec![json!({ "ev": "sessions", "sessions": sessions.borrow_and_update().clone() })],
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
