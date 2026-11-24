//! The HTTP API: the part of OpenCode 2's API Harlo uses for a session's
//! turn, in OpenCode's paths and JSON, so `opencode-client` works against
//! this server unchanged.
//!
//! The event stream adds what reconnecting needs: each durable event goes
//! out with its sequence number as the SSE `id`, and a client that sends
//! `Last-Event-ID` (or `?after=`) gets the events it missed first.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use base64::Engine as _;
use futures_util::{Stream, StreamExt, stream};
use harlo_engine::{Message, Rule, now};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::git;
use crate::server::{Outgoing, Refusal, Server};

type Shared = State<Arc<Server>>;
type Params = Query<HashMap<String, String>>;

/// The automation routes.
fn automation_routes() -> Router<Arc<Server>> {
    Router::new()
        .route(
            "/api/experimental/automation",
            get(automation_list).post(create_automation),
        )
        .route(
            "/api/experimental/automation/{id}",
            get(automation_info).delete(remove_automation),
        )
        .route(
            "/api/experimental/automation/{id}/pause",
            post(pause_automation),
        )
        .route(
            "/api/experimental/automation/{id}/resume",
            post(resume_automation),
        )
}

/// The file routes, and Harlo's own: notes, memory and phone access.
fn file_and_harlo_routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/api/fs/find", get(find_files))
        .route("/api/fs/list", get(list_files))
        .route("/api/fs/read/{*path}", get(read_file))
        .route("/api/experimental/fs/write", post(write_file))
        .route(
            "/api/experimental/fs/watch",
            post(watch_file).delete(unwatch_file),
        )
        .route("/harlo/memory", get(memory).put(set_memory))
        .route("/harlo/notes", get(notes))
        .route("/harlo/notes/{id}", patch(correct_note).delete(forget_note))
        .route("/harlo/phone", get(phone_state).put(want_phone))
        .route("/harlo/phone/pair", post(pair_phone))
        .route("/harlo/phone/device/{id}", delete(remove_phone))
        .route("/harlo/phone/test", post(test_phone))
}

/// The integration and credential routes.
fn integration_routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/api/integration", get(integration_list))
        .route("/api/integration/{id}", get(integration_info))
        .route("/api/integration/{id}/connect/key", post(connect_key))
        .route(
            "/api/integration/{id}/connect/external",
            post(connect_external),
        )
        .route("/api/integration/{id}/connect/oauth", post(begin_oauth))
        .route(
            "/api/integration/{id}/connect/oauth/{attempt}",
            get(oauth_status).delete(cancel_oauth),
        )
        .route(
            "/api/integration/{id}/connect/oauth/{attempt}/complete",
            post(complete_oauth),
        )
        .route("/api/credential", get(credential_list))
        .route(
            "/api/credential/{id}",
            patch(label_credential).delete(remove_credential),
        )
}

/// The routes, behind the password check.
pub fn router(server: Arc<Server>) -> Router {
    Router::new()
        .merge(automation_routes())
        .merge(integration_routes())
        .merge(file_and_harlo_routes())
        .route("/api/info", get(info))
        .route("/api/event", get(events))
        .route("/api/project", get(projects))
        .route("/api/project/{id}", patch(update_project))
        .route(
            "/api/worktree",
            get(worktrees).post(create_worktree).delete(remove_worktree),
        )
        .route("/api/session/{id}/fork", post(fork_session))
        .route("/api/session/{id}/move", post(move_session))
        .route("/api/experimental/session/{id}/export", get(export_session))
        .route("/api/experimental/session/import", post(import_session))
        .route("/api/session", get(sessions).post(create_session))
        .route("/api/session/active", get(active))
        .route(
            "/api/session/{id}",
            get(session).patch(update_session).delete(delete_session),
        )
        .route("/api/session/{id}/message", get(messages))
        .route("/api/session/{id}/diff", get(session_diff))
        .route("/api/session/{id}/form", get(forms))
        .route(
            "/api/session/{id}/form/{form}",
            get(form).delete(dismiss_form),
        )
        .route("/api/session/{id}/form/{form}/reply", post(answer_form))
        .route("/api/form", get(forms_in))
        .route("/api/session/{id}/inbox", get(inbox))
        .route("/api/session/{id}/context", get(context))
        .route(
            "/api/session/{id}/inbox/{item}",
            axum::routing::patch(set_delivery).delete(cancel_inbox),
        )
        .route("/api/session/{id}/permission", get(permissions))
        .route("/api/session/{id}/permission/{request}/reply", post(reply))
        .route("/api/session/{id}/prompt", post(prompt))
        .route("/api/session/{id}/model", post(set_model))
        .route("/api/session/{id}/agent", post(set_agent))
        .route("/api/session/{id}/interrupt", post(interrupt))
        .route("/api/session/{id}/compact", post(compact))
        .route("/api/session/{id}/revert", delete(cancel_revert))
        .route("/api/session/{id}/revert/stage", post(stage_revert))
        .route("/api/session/{id}/revert/commit", post(commit_revert))
        .route("/api/agent", get(agents))
        .route("/api/command", get(command_list))
        .route("/api/skill", get(skill_list))
        .route("/api/session/{id}/command", post(command))
        .route("/api/model", get(models))
        .route("/api/model/default", get(default_model))
        .route("/api/config", get(config))
        .route("/api/mcp", get(mcp_list))
        .route("/api/mcp/resource", get(mcp_resources))
        .route(
            "/api/experimental/mcp/{server}",
            axum::routing::put(mcp_add).delete(mcp_remove),
        )
        .route("/api/experimental/mcp/{server}/connect", post(mcp_connect))
        .route(
            "/api/experimental/mcp/{server}/disconnect",
            post(mcp_disconnect),
        )
        .route("/api/plugin", get(plugin_list))
        .route("/api/plugin/check", post(plugin_check))
        .route("/api/plugin/update", post(plugin_update))
        .route("/api/experimental/session/stats", get(stats))
        .route("/api/provider", get(providers))
        .route("/api/pty", get(pty_list).post(create_pty))
        .route(
            "/api/pty/{id}",
            get(pty_info).put(change_pty).delete(remove_pty),
        )
        .route("/api/pty/{id}/connect", get(connect_pty))
        .route("/api/permission/saved", get(saved_approvals))
        .route(
            "/api/permission/saved/{id}",
            patch(change_approval).delete(remove_approval),
        )
        .route("/api/experimental/job", get(jobs))
        .route("/api/experimental/job/{id}/cancel", post(cancel_job))
        .route("/api/session/{id}/shell", post(user_shell))
        .route("/api/vcs", get(vcs))
        .route("/api/vcs/diff", get(vcs_diff))
        .route("/api/session/{id}/generate", post(aside))
        .route("/api/experimental/generate/text", post(write_text))
        .route("/api/rpc/opencode.goal/get", post(read_goal))
        .route("/api/experimental/client-mcp", get(host_tools))
        .route("/api/websearch/provider", get(search_services))
        .fallback(not_here)
        .layer(middleware::from_fn_with_state(
            Arc::clone(&server),
            password,
        ))
        .with_state(server)
}

/// Lets a request through when the server has no password or it carries it.
async fn password(State(server): Shared, request: Request, next: Next) -> Response {
    if let Some(password) = &server.password {
        let want = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("opencode:{password}"))
        );
        let given = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        if given != Some(want.as_str()) {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic")],
            )
                .into_response();
        }
    }
    next.run(request).await
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let (status, tag, message) = match self {
            Self::NotFound(m) => (StatusCode::NOT_FOUND, "NotFoundError", m),
            Self::Bad(m) => (StatusCode::BAD_REQUEST, "InvalidRequestError", m),
            Self::Settled(m) => (StatusCode::CONFLICT, "AlreadySettledError", m),
            Self::Store(m) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "StoreError",
                format!("The server's store failed: {m}"),
            ),
            Self::Exists(m) => (StatusCode::CONFLICT, "ConflictError", m),
            Self::Worktree { message, force } => {
                let body = json!({
                    "_tag": "WorktreeError", "name": "WorktreeError",
                    "data": { "message": message, "forceRequired": force },
                });
                return (StatusCode::BAD_REQUEST, axum::Json(body)).into_response();
            }
        };
        let body = json!({ "_tag": tag, "message": message });
        (status, axum::Json(body)).into_response()
    }
}

type Answer = Result<Response, Refusal>;

#[expect(
    clippy::unnecessary_wraps,
    reason = "handlers end with it, and they return a Result"
)]
fn ok(v: Value) -> Answer {
    Ok(axum::Json(v).into_response())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "handlers end with it, and they return a Result"
)]
fn done() -> Answer {
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The folder a catalog request is about.
/// The folder a request is about.
fn directory_of(q: &HashMap<String, String>) -> String {
    q.get("location[directory]").cloned().unwrap_or_default()
}

fn location(q: &HashMap<String, String>) -> Value {
    json!({ "directory": q.get("location[directory]").cloned().unwrap_or_default() })
}

async fn not_here(request: Request) -> Response {
    tracing::info!(name: "api.unknown", method = %request.method(), path = %request.uri(), "a request this server doesn't answer");
    let message = format!(
        "This server doesn't answer {} {}.",
        request.method(),
        request.uri().path()
    );
    (
        StatusCode::NOT_FOUND,
        axum::Json(json!({ "error": { "message": message } })),
    )
        .into_response()
}

async fn info(headers: HeaderMap) -> Answer {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1");
    ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "urls": [format!("http://{host}")],
        "paths": { "tmp": std::env::temp_dir() },
        "capabilities": {
            "server": "harlo", "persistentPty": false, "clientTools": true, "phone": true, "memory": true,
        },
    }))
}

async fn events(
    State(server): Shared,
    headers: HeaderMap,
    Query(q): Params,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let after = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .or_else(|| q.get("after").and_then(|v| v.parse().ok()));
    // Subscribe before reading the store, so nothing falls in between.
    let live = server.subscribe();
    let connected = json!({ "id": harlo_engine::ascending_id("evt"), "created": now(), "type": "server.connected", "data": {} });
    let mut first = vec![Outgoing {
        seq: None,
        json: connected.to_string().into(),
    }];
    let mut last = 0;
    if let Some(after) = after {
        let missed = server.replay(after);
        last = missed.last().and_then(|o| o.seq).unwrap_or(after);
        first.extend(missed);
    }
    let rest = stream::unfold((live, last), |(mut live, last)| async move {
        loop {
            match live.recv().await {
                Ok(o) if o.seq.is_some_and(|s| s <= last) => {}
                Ok(o) => return Some((o, (live, last))),
                // (A client too far behind reconnects and catches up.)
                Err(RecvError::Lagged(_) | RecvError::Closed) => return None,
            }
        }
    });
    let all = stream::iter(first).chain(rest).map(|o| {
        let mut event = SseEvent::default().data(&*o.json);
        if let Some(seq) = o.seq {
            event = event.id(seq.to_string());
        }
        Ok(event)
    });
    Sse::new(all).keep_alive(KeepAlive::default())
}

async fn projects(State(server): Shared) -> Answer {
    ok(json!(server.projects().await?))
}

async fn update_project(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(change): axum::Json<crate::projects::ProjectChange>,
) -> Answer {
    ok(server.update_project(&id, change).await?)
}

async fn worktrees(State(server): Shared, Query(q): Params) -> Answer {
    let project = q.get("projectID").cloned().unwrap_or_default();
    ok(json!(server.worktree_list(&project).await?))
}

/// A worktree to make.
#[derive(Deserialize)]
struct NewWorktree {
    #[serde(rename = "projectID")]
    project: String,
    name: Option<String>,
    branch: Option<String>,
    directory: Option<String>,
}

async fn create_worktree(
    State(server): Shared,
    axum::Json(body): axum::Json<NewWorktree>,
) -> Answer {
    let folder = server
        .create_worktree(&body.project, body.name, body.branch, body.directory)
        .await?;
    ok(json!({ "directory": folder }))
}

/// A worktree to remove.
#[derive(Deserialize)]
struct OldWorktree {
    #[serde(rename = "projectID")]
    project: String,
    directory: String,
    #[serde(default)]
    force: bool,
}

async fn remove_worktree(
    State(server): Shared,
    axum::Json(body): axum::Json<OldWorktree>,
) -> Answer {
    server
        .remove_worktree(&body.project, &body.directory, body.force)
        .await?;
    done()
}

/// Where a fork ends.
#[derive(Deserialize)]
struct ForkAt {
    before: Option<String>,
}

async fn fork_session(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ForkAt>,
) -> Answer {
    ok(json!({ "data": server.fork_session(&id, body.before.as_deref())? }))
}

async fn move_session(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<NewLocation>,
) -> Answer {
    server.move_session(&id, &body.directory).await?;
    done()
}

async fn delete_session(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.delete_session(&id)?;
    done()
}

async fn export_session(State(server): Shared, Path(id): Path<String>) -> Answer {
    ok(json!({ "data": server.export_session(&id)? }))
}

/// An exported session to bring back.
#[derive(Deserialize)]
struct Import {
    info: Value,
    #[serde(default)]
    messages: Vec<Value>,
    location: NewLocation,
}

async fn import_session(State(server): Shared, axum::Json(body): axum::Json<Import>) -> Answer {
    let session = server
        .import_session(body.info, body.messages, &body.location.directory)
        .await?;
    ok(json!({ "data": session }))
}

async fn sessions(State(server): Shared, Query(q): Params) -> Answer {
    let directory = q.get("directory");
    let list: Vec<_> = server
        .store
        .sessions()?
        .into_iter()
        .filter(|s| directory.is_none_or(|d| &s.location.directory == d))
        .filter(|s| match q.get("archived").map(String::as_str) {
            Some("false") => s.time.archived.is_none(),
            Some("true") => s.time.archived.is_some(),
            _ => true,
        })
        // `null` asks for top-level sessions; an ID, for its children.
        .filter(|s| match q.get("parentID").map(String::as_str) {
            None => true,
            Some("null") => s.parent_id.is_none(),
            Some(parent) => s.parent_id.as_deref() == Some(parent),
        })
        .collect();
    ok(json!({ "data": list, "cursor": { "previous": null, "next": null } }))
}

#[derive(Deserialize)]
struct NewSession {
    /// Where; without it, the folder the request names, else the server's.
    #[serde(default)]
    location: Option<NewLocation>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    model: Option<harlo_engine::ModelRef>,
}

#[derive(Deserialize)]
struct NewLocation {
    directory: String,
}

async fn create_session(
    State(server): Shared,
    Query(q): Params,
    axum::Json(body): axum::Json<NewSession>,
) -> Answer {
    let directory = match body.location {
        Some(l) => l.directory,
        None => match q.get("location[directory]").or_else(|| q.get("directory")) {
            Some(d) => d.clone(),
            None => std::env::current_dir()
                .map_err(|e| Refusal::Bad(format!("No folder for the session: {e}")))?
                .to_string_lossy()
                .into_owned(),
        },
    };
    let session = server.create_session(&directory).await?;
    if body.title.is_some() {
        server.update_session(&session.id, None, body.title)?;
    }
    if let Some(agent) = body.agent {
        server.set_agent(&session.id, &agent)?;
    }
    if let Some(model) = body.model {
        server.set_model(&session.id, model)?;
    }
    ok(json!({ "data": server.session(&session.id)? }))
}

async fn active(State(server): Shared) -> Answer {
    let busy: serde_json::Map<String, Value> = server
        .busy()
        .into_iter()
        .map(|id| (id, json!({ "type": "busy" })))
        .collect();
    ok(json!({ "data": busy }))
}

async fn session(State(server): Shared, Path(id): Path<String>) -> Answer {
    ok(json!({ "data": server.session(&id)? }))
}

#[derive(Deserialize)]
struct SessionChange {
    permissions: Option<Vec<Rule>>,
    title: Option<String>,
    archived: Option<bool>,
}

async fn update_session(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<SessionChange>,
) -> Answer {
    server.update_session(&id, body.permissions, body.title)?;
    if let Some(archived) = body.archived {
        server.archive_session(&id, archived)?;
    }
    done()
}

async fn messages(State(server): Shared, Path(id): Path<String>, Query(q): Params) -> Answer {
    server.session(&id)?;
    let mut list = server.store.messages(&id)?;
    if q.get("order").is_some_and(|o| o == "desc") {
        list.reverse();
    }
    ok(json!({ "data": list, "cursor": { "previous": null, "next": null } }))
}

async fn session_diff(State(server): Shared, Path(id): Path<String>) -> Answer {
    let session = server.session(&id)?;
    let mut files: Vec<String> = Vec::new();
    for message in server.store.messages(&id)? {
        if let Message::Assistant(step) = message {
            for f in step.snapshot.map(|s| s.files).unwrap_or_default() {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
        }
    }
    ok(json!({ "data": git::changes(&session.location.directory, Some(&files)).await }))
}

/// The version control of a folder: git and its branches, or nothing.
async fn vcs(Query(q): Params) -> Answer {
    let directory = directory_of(&q);
    let data = if !directory.is_empty() && git::is_repo(&directory).await {
        let (current, default) = git::branches(&directory).await;
        json!({ "provider": "git", "branch": { "current": current, "default": default } })
    } else {
        json!({ "branch": {} })
    };
    ok(json!({ "location": location(&q), "data": data }))
}

/// A prompt to answer.
#[derive(Deserialize)]
struct Asked {
    prompt: String,
    #[serde(default)]
    model: Option<Value>,
}

async fn aside(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Asked>,
) -> Answer {
    ok(json!({ "data": server.aside(&id, &body.prompt).await? }))
}

async fn write_text(State(server): Shared, axum::Json(body): axum::Json<Asked>) -> Answer {
    let model = body
        .model
        .as_ref()
        .and_then(harlo_engine::ModelRef::from_value);
    let text = server.write_text(&body.prompt, model).await?;
    ok(json!({ "data": { "text": text } }))
}

async fn vcs_diff(Query(q): Params) -> Answer {
    let location = location(&q);
    let directory = location["directory"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let data = if directory.is_empty() {
        Vec::new()
    } else {
        git::changes(&directory, None).await
    };
    ok(json!({ "location": location, "data": data }))
}

/// A new way for a waiting prompt to reach its run.
#[derive(Deserialize)]
struct DeliveryChange {
    delivery: String,
}

async fn set_delivery(
    State(server): Shared,
    Path((id, item)): Path<(String, String)>,
    axum::Json(body): axum::Json<DeliveryChange>,
) -> Answer {
    if !matches!(body.delivery.as_str(), "queue" | "steer") {
        return Err(Refusal::Bad(format!(
            "Delivery is queue or steer, not {}.",
            body.delivery
        )));
    }
    server.set_delivery(&id, &item, &body.delivery)?;
    done()
}

async fn cancel_inbox(State(server): Shared, Path((id, item)): Path<(String, String)>) -> Answer {
    server.cancel_inbox(&id, &item)?;
    done()
}

/// The messages the model reads now: from the last summary on.
async fn context(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.session(&id)?;
    let messages = server.store.messages(&id)?;
    let current = harlo_engine::compaction::since_summary(&messages);
    ok(json!({ "data": current }))
}

async fn inbox(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.session(&id)?;
    ok(json!({ "data": server.inbox(&id) }))
}

async fn forms(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.session(&id)?;
    ok(json!({ "data": server.forms(&id) }))
}

async fn forms_in(State(server): Shared, Query(q): Params) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    ok(json!({ "location": location(&q), "data": server.forms_in(&directory) }))
}

async fn form(State(server): Shared, Path((id, form)): Path<(String, String)>) -> Answer {
    ok(json!({ "data": server.form(&id, &form)? }))
}

/// A person's answers to a form.
#[derive(Debug, serde::Deserialize)]
struct FormReply {
    answer: Map<String, Value>,
}

async fn answer_form(
    State(server): Shared,
    Path((id, form)): Path<(String, String)>,
    axum::Json(body): axum::Json<FormReply>,
) -> Answer {
    server.settle_form(&id, &form, Some(&body.answer))?;
    done()
}

async fn dismiss_form(State(server): Shared, Path((id, form)): Path<(String, String)>) -> Answer {
    server.settle_form(&id, &form, None)?;
    done()
}

async fn permissions(State(server): Shared, Path(id): Path<String>) -> Answer {
    ok(json!({ "data": server.asks(&id) }))
}

#[derive(Deserialize)]
struct Reply {
    decision: String,
    message: Option<String>,
}

async fn reply(
    State(server): Shared,
    Path((id, request)): Path<(String, String)>,
    axum::Json(body): axum::Json<Reply>,
) -> Answer {
    server.reply(&id, &request, &body.decision, body.message)?;
    done()
}

#[derive(Deserialize)]
struct Prompt {
    id: Option<String>,
    text: String,
    delivery: Option<String>,
    #[serde(default)]
    skills: Vec<SkillRef>,
    #[serde(default)]
    files: Vec<FileRef>,
}

/// A file sent with a prompt: a `file://` path on this machine, or the
/// bytes in a `data:` URI.
#[derive(Deserialize)]
struct FileRef {
    uri: String,
    name: Option<String>,
    mention: Option<Value>,
}

/// The largest file a prompt may carry.
const FILE_LIMIT: u64 = 20 * 1024 * 1024;

impl FileRef {
    /// The file, read.
    fn read(self) -> Result<harlo_engine::attachment::File, Refusal> {
        use base64::Engine as _;
        let mut file = if let Some(path) = self.uri.strip_prefix("file://") {
            let size = std::fs::metadata(path)
                .map_err(|e| Refusal::Bad(format!("Couldn't read {path}: {e}")))?
                .len();
            if size > FILE_LIMIT {
                return Err(Refusal::Bad(format!("{path} is too large to send.")));
            }
            let bytes = std::fs::read(path)
                .map_err(|e| Refusal::Bad(format!("Couldn't read {path}: {e}")))?;
            let mime = match mime_guess::from_path(path).first() {
                Some(mime) => mime.essence_str().to_owned(),
                None if std::str::from_utf8(&bytes).is_ok() => "text/plain".to_owned(),
                None => "application/octet-stream".to_owned(),
            };
            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            harlo_engine::attachment::File::new(
                &bytes,
                &mime,
                json!({ "type": "uri", "uri": self.uri }),
                self.name.clone().or(name),
            )
        } else if let Some(rest) = self.uri.strip_prefix("data:") {
            let (head, data) = rest
                .split_once(',')
                .ok_or_else(|| Refusal::Bad("A data URI needs a comma.".to_owned()))?;
            let mime = head.trim_end_matches(";base64");
            let bytes = if head.ends_with(";base64") {
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|e| Refusal::Bad(format!("The data URI isn't base64: {e}")))?
            } else {
                data.as_bytes().to_vec()
            };
            harlo_engine::attachment::File::new(
                &bytes,
                if mime.is_empty() { "text/plain" } else { mime },
                json!({ "type": "inline" }),
                self.name.clone(),
            )
        } else {
            return Err(Refusal::Bad(format!(
                "Files are sent as file:// or data: URIs, not {}.",
                self.uri
            )));
        };
        file.mention = self.mention;
        Ok(file)
    }
}

/// A skill named in a prompt.
#[derive(Deserialize)]
struct SkillRef {
    id: String,
}

/// A command run in a session.
#[derive(Deserialize)]
struct CommandRun {
    name: String,
    #[serde(default)]
    text: String,
    delivery: Option<String>,
    #[serde(default)]
    skills: Vec<SkillRef>,
}

async fn command(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<CommandRun>,
) -> Answer {
    let delivery = body.delivery.unwrap_or_else(|| "queue".to_owned());
    let skills: Vec<String> = body.skills.into_iter().map(|s| s.id).collect();
    server
        .run_command(&id, &body.name, &body.text, &delivery, &skills)
        .await?;
    done()
}

async fn command_list(State(server): Shared, Query(q): Params) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    let list: Vec<Value> = server
        .commands_for(&directory)
        .await
        .iter()
        .map(harlo_engine::commands::Command::to_json)
        .chain([json!({ "name": "goal", "description": crate::goals::DESCRIPTION })])
        .collect();
    ok(json!({ "location": location(&q), "data": list }))
}

async fn skill_list(State(server): Shared, Query(q): Params) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    let list: Vec<Value> = server
        .skills_for(&directory)
        .await
        .iter()
        .map(harlo_engine::skills::Skill::to_json)
        .collect();
    ok(json!({ "location": location(&q), "data": list }))
}

async fn prompt(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Prompt>,
) -> Answer {
    let delivery = body.delivery.unwrap_or_else(|| "queue".to_owned());
    let directory = server.session(&id)?.location.directory;
    let ids: Vec<String> = body.skills.into_iter().map(|s| s.id).collect();
    let skills = server.attach(&directory, &ids).await?;
    let files = body
        .files
        .into_iter()
        .map(FileRef::read)
        .collect::<Result<Vec<_>, _>>()?;
    ok(server.prompt(&id, body.id, &body.text, &delivery, skills, files)?)
}

async fn compact(State(server): Shared, Path(id): Path<String>) -> Answer {
    ok(server.compact(&id)?)
}

async fn interrupt(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.session(&id)?;
    server.interrupt(&id);
    done()
}

async fn agents(State(server): Shared, Query(q): Params) -> Answer {
    let folder = server.folder(&directory_of(&q));
    ok(json!({ "location": location(&q), "data": folder.agents_json() }))
}

/// A model chosen for a session.
#[derive(Debug, serde::Deserialize)]
struct ModelChoice {
    model: harlo_engine::ModelRef,
}

async fn set_model(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ModelChoice>,
) -> Answer {
    server.set_model(&id, body.model)?;
    done()
}

/// An agent chosen for a session.
#[derive(Debug, serde::Deserialize)]
struct AgentChoice {
    agent: String,
}

async fn set_agent(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<AgentChoice>,
) -> Answer {
    server.set_agent(&id, &body.agent)?;
    done()
}

fn model_json(provider: &harlo_engine::ProviderSpec, m: &harlo_engine::ModelSpec) -> Value {
    json!({
        "id": m.id, "modelID": m.id, "providerID": provider.id, "name": m.name, "type": "chat",
        "enabled": true, "status": "active",
        "variants": m.variants.iter().map(|v| json!({ "id": v.id })).collect::<Vec<_>>(),
        "capabilities": { "tools": true, "input": ["text"], "output": ["text"] },
        "limit": { "context": m.context, "output": m.output },
    })
}

async fn models(State(server): Shared, Query(q): Params) -> Answer {
    let folder = server.folder(&directory_of(&q));
    let list: Vec<Value> = folder
        .config
        .providers
        .iter()
        .flat_map(|p| p.models.iter().map(move |m| model_json(p, m)))
        .collect();
    ok(json!({ "location": location(&q), "data": list }))
}

async fn default_model(State(server): Shared, Query(q): Params) -> Answer {
    let config = server.folder(&directory_of(&q)).config;
    let model = config
        .default_model()
        .and_then(|r| config.resolve(&r).map(|(p, m)| model_json(p, m)))
        .unwrap_or(Value::Null);
    ok(json!({ "location": location(&q), "data": model }))
}

async fn config(State(server): Shared, Query(q): Params) -> Answer {
    ok(json!(server.folder(&directory_of(&q)).entries()))
}

async fn mcp_resources(State(server): Shared, Query(q): Params) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    ok(json!({ "location": location(&q), "data": server.mcp_resources(&directory) }))
}

async fn mcp_list(State(server): Shared, Query(q): Params) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    ok(json!({ "location": location(&q), "data": server.mcp_servers(&directory) }))
}

async fn mcp_connect(State(server): Shared, Path(name): Path<String>, Query(q): Params) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    server.mcp_connect(&directory, &name)?;
    done()
}

async fn mcp_disconnect(
    State(server): Shared,
    Path(name): Path<String>,
    Query(q): Params,
) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    server.mcp_disconnect(&directory, &name)?;
    done()
}

/// A server to add while running.
#[derive(Deserialize)]
struct McpAdd {
    config: Value,
}

async fn mcp_add(
    State(server): Shared,
    Path(name): Path<String>,
    Query(q): Params,
    axum::Json(body): axum::Json<McpAdd>,
) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    server.mcp_add(&directory, &name, body.config);
    done()
}

async fn mcp_remove(State(server): Shared, Path(name): Path<String>) -> Answer {
    server.mcp_remove(&name);
    done()
}

/// The engine's own parts, as built-in plugins: it loads no others.
fn plugins() -> Vec<Value> {
    let parts = [
        "harlo.agent",
        "harlo.command",
        "harlo.skill",
        "harlo.memory",
        "harlo.goal",
        "harlo.codemode",
        "harlo.automation",
        "harlo.mcp",
        "harlo.vcs.git",
    ];
    parts
        .iter()
        .map(|id| {
            json!({
                "id": id, "source": { "type": "builtin" }, "features": { "server": true },
                "state": { "status": "active" },
            })
        })
        .collect()
}

async fn plugin_list(Query(q): Params) -> Answer {
    ok(json!({ "location": location(&q), "data": plugins() }))
}

/// Nothing to check: the engine has no package plugins.
async fn plugin_check(Query(q): Params) -> Answer {
    ok(json!({ "location": location(&q), "data": plugins() }))
}

/// What's to update.
#[derive(Deserialize)]
struct PluginTargets {
    #[serde(default)]
    targets: Vec<String>,
}

/// Nothing to update; a named package isn't one the engine has.
async fn plugin_update(axum::Json(body): axum::Json<PluginTargets>) -> Answer {
    match body.targets.first() {
        Some(target) => Err(Refusal::NotFound(format!(
            "There's no package plugin {target}."
        ))),
        None => done(),
    }
}

/// A client's MCP server, hosted over a WebSocket, whose tools agents get
/// while it stays open.
async fn host_tools(
    State(server): Shared,
    Query(q): Params,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> Answer {
    let name = q
        .get("name")
        .cloned()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| Refusal::Bad("Name the tools' server.".to_owned()))?;
    Ok(upgrade
        .on_upgrade(move |socket| server.host_mcp(name, socket))
        .into_response())
}

/// The web search services agents can use.
async fn search_services(State(server): Shared, Query(q): Params) -> Answer {
    let data: Vec<Value> = server
        .search_services()
        .into_iter()
        .map(|(id, name)| json!({ "id": id, "name": name }))
        .collect();
    ok(json!({ "location": location(&q), "data": data }))
}

async fn jobs(State(server): Shared, Query(q): Params) -> Answer {
    let session = q.get("sessionID").map(String::as_str);
    ok(json!({ "data": server.jobs_of(session) }))
}

/// The providers set up for a folder: names and states, never their keys.
async fn providers(State(server): Shared, Query(q): Params) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    let folder = server.folder(&directory);
    let catalog = server.catalog();
    let list: Vec<Value> = folder
        .config
        .providers
        .iter()
        .map(|p| {
            let mut info = json!({ "id": p.id, "name": p.name, "activation": "enabled" });
            if catalog.get(&p.id).is_some() {
                info["integrationID"] = json!(p.id);
                info["activation"] = json!("auto");
            }
            info
        })
        .collect();
    ok(json!({ "location": location(&q), "data": list }))
}

async fn stats(State(server): Shared, Query(q): Params) -> Answer {
    let from = q.get("from").and_then(|f| f.parse().ok());
    ok(json!({ "data": server.stats(from)? }))
}

async fn pty_list(State(server): Shared, Query(q): Params) -> Answer {
    ok(json!({ "location": location(&q), "data": server.ptys() }))
}

async fn create_pty(
    State(server): Shared,
    Query(q): Params,
    axum::Json(body): axum::Json<crate::pty::NewTerminal>,
) -> Answer {
    let directory = q.get("location[directory]").cloned().unwrap_or_default();
    let info = server.create_pty(&directory, body)?;
    ok(json!({ "location": location(&q), "data": info }))
}

async fn pty_info(State(server): Shared, Path(id): Path<String>, Query(q): Params) -> Answer {
    server.pty(&id)?;
    let info = server.ptys().into_iter().find(|p| p["id"] == id.as_str());
    ok(json!({ "location": location(&q), "data": info }))
}

async fn change_pty(
    State(server): Shared,
    Path(id): Path<String>,
    Query(q): Params,
    axum::Json(body): axum::Json<crate::pty::TerminalChange>,
) -> Answer {
    let info = server.change_pty(&id, body)?;
    ok(json!({ "location": location(&q), "data": info }))
}

async fn remove_pty(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.remove_pty(&id)?;
    done()
}

async fn connect_pty(
    State(server): Shared,
    Path(id): Path<String>,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> Answer {
    let term = server.pty(&id)?;
    Ok(upgrade
        .on_upgrade(move |socket| crate::pty::serve(term, socket))
        .into_response())
}

async fn saved_approvals(State(server): Shared, Query(q): Params) -> Answer {
    let project = q.get("projectID").map(String::as_str);
    ok(json!({ "data": server.store.approvals(project)? }))
}

/// A change to a saved approval.
#[derive(Deserialize)]
struct ApprovalChange {
    action: Option<String>,
    resource: Option<String>,
}

async fn change_approval(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ApprovalChange>,
) -> Answer {
    let changed = server
        .store
        .change_approval(&id, body.action.as_deref(), body.resource.as_deref())?
        .ok_or_else(|| Refusal::NotFound(format!("There's no saved approval {id}.")))?;
    ok(json!({ "data": changed }))
}

async fn remove_approval(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.store.delete_approval(&id)?;
    done()
}

async fn integration_list(State(server): Shared, Query(q): Params) -> Answer {
    ok(json!({ "location": location(&q), "data": server.integrations()? }))
}

async fn integration_info(
    State(server): Shared,
    Path(id): Path<String>,
    Query(q): Params,
) -> Answer {
    ok(json!({ "location": location(&q), "data": server.integration(&id)? }))
}

async fn connect_key(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<crate::credentials::KeySignIn>,
) -> Answer {
    server.connect_key(&id, body)?;
    done()
}

async fn connect_external(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<crate::credentials::ExternalSignIn>,
) -> Answer {
    server.connect_external(&id, body)?;
    done()
}

/// A browser sign-in to start.
#[derive(Deserialize)]
struct OAuthStart {
    #[serde(rename = "methodID")]
    method: String,
}

async fn begin_oauth(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<OAuthStart>,
) -> Answer {
    ok(json!({ "data": server.begin_oauth(&id, &body.method).await? }))
}

async fn oauth_status(State(server): Shared, Path((_, attempt)): Path<(String, String)>) -> Answer {
    ok(json!({ "data": server.oauth_status(&attempt)? }))
}

async fn cancel_oauth(State(server): Shared, Path((_, attempt)): Path<(String, String)>) -> Answer {
    server.cancel_oauth(&attempt);
    done()
}

/// The address the browser ended on, or its code.
#[derive(Deserialize)]
struct OAuthCode {
    code: String,
}

async fn complete_oauth(
    State(server): Shared,
    Path((_, attempt)): Path<(String, String)>,
    axum::Json(body): axum::Json<OAuthCode>,
) -> Answer {
    server.complete_oauth(&attempt, &body.code)?;
    done()
}

async fn credential_list(State(server): Shared) -> Answer {
    ok(json!({ "data": server.store.credentials()? }))
}

/// A credential's new label.
#[derive(Deserialize)]
struct CredentialLabel {
    label: String,
}

async fn label_credential(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<CredentialLabel>,
) -> Answer {
    server.label_credential(&id, &body.label)?;
    done()
}

async fn remove_credential(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.remove_credential(&id)?;
    done()
}

async fn automation_list(State(server): Shared, Query(q): Params) -> Answer {
    let project = q.get("projectID").map(String::as_str);
    ok(json!({ "data": server.automation_list(project)? }))
}

async fn create_automation(
    State(server): Shared,
    axum::Json(body): axum::Json<crate::automations::NewAutomation>,
) -> Answer {
    ok(json!({ "data": server.create_automation(body)? }))
}

async fn automation_info(State(server): Shared, Path(id): Path<String>) -> Answer {
    ok(json!({ "data": server.automation(&id)? }))
}

async fn remove_automation(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.delete_automation(&id)?;
    done()
}

async fn pause_automation(State(server): Shared, Path(id): Path<String>) -> Answer {
    ok(json!({ "data": server.set_automation_status(&id, "paused")? }))
}

async fn resume_automation(State(server): Shared, Path(id): Path<String>) -> Answer {
    ok(json!({ "data": server.set_automation_status(&id, "active")? }))
}

async fn cancel_job(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.cancel_job(&id)?;
    done()
}

/// A person's command.
#[derive(Deserialize)]
struct UserShell {
    command: String,
}

async fn user_shell(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<UserShell>,
) -> Answer {
    server.run_user_shell(&id, &body.command).await?;
    done()
}

#[derive(Deserialize)]
struct Stage {
    #[serde(rename = "messageID")]
    message_id: String,
    files: Option<bool>,
}

async fn stage_revert(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Stage>,
) -> Answer {
    let revert = server
        .stage_revert(&id, &body.message_id, body.files.unwrap_or(false))
        .await?;
    ok(json!({ "data": revert }))
}

async fn commit_revert(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.commit_revert(&id)?;
    done()
}

async fn cancel_revert(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.cancel_revert(&id).await?;
    done()
}

#[derive(Deserialize)]
struct After {
    after: Option<u64>,
}

/// The folder a request names in `location[directory]`, which must be set.
fn folder_of(q: &HashMap<String, String>) -> Result<std::path::PathBuf, Refusal> {
    q.get("location[directory]")
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| Refusal::Bad("Which folder? Set location[directory].".to_owned()))
}

async fn find_files(Query(q): Params) -> Answer {
    let root = folder_of(&q)?;
    let query = q.get("query").cloned().unwrap_or_default();
    let kind = q.get("type").cloned();
    let limit = q
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(50_usize);
    let data =
        tokio::task::spawn_blocking(move || crate::fs::find(&root, &query, kind.as_deref(), limit))
            .await
            .unwrap_or_default();
    ok(json!({ "location": location(&q), "data": data }))
}

async fn list_files(Query(q): Params) -> Answer {
    let root = folder_of(&q)?;
    let path = q.get("path").cloned().unwrap_or_default();
    let data = crate::fs::list(&root, &path).map_err(Refusal::NotFound)?;
    ok(json!({ "location": location(&q), "data": data }))
}

async fn read_file(Path(path): Path<String>, Query(q): Params) -> Answer {
    let root = folder_of(&q)?;
    let file = crate::fs::inside(&root, &path).map_err(Refusal::Bad)?;
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|e| Refusal::NotFound(format!("Couldn't read {path}: {e}")))?;
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
}

async fn write_file(Query(q): Params, body: axum::body::Bytes) -> Answer {
    let root = folder_of(&q)?;
    let path = q.get("path").cloned().unwrap_or_default();
    let file = crate::fs::inside(&root, &path).map_err(Refusal::Bad)?;
    if let Some(parent) = file.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| Refusal::Bad(format!("Couldn't make the folder for {path}: {e}")))?;
    }
    tokio::fs::write(&file, &body)
        .await
        .map_err(|e| Refusal::Bad(format!("Couldn't write {path}: {e}")))?;
    ok(json!({ "location": location(&q), "data": { "path": file } }))
}

/// A file to watch, relative to the folder.
#[derive(Deserialize)]
struct Watched {
    path: String,
}

async fn watch_file(
    State(server): Shared,
    Query(q): Params,
    axum::Json(body): axum::Json<Watched>,
) -> Answer {
    let root = folder_of(&q)?;
    let file = crate::fs::inside(&root, &body.path).map_err(Refusal::Bad)?;
    if !file.exists() {
        return Err(Refusal::NotFound(format!("There's no {}.", body.path)));
    }
    server.watch(&directory_of(&q), file);
    done()
}

async fn unwatch_file(
    State(server): Shared,
    Query(q): Params,
    axum::Json(body): axum::Json<Watched>,
) -> Answer {
    let root = folder_of(&q)?;
    let file = crate::fs::inside(&root, &body.path).map_err(Refusal::Bad)?;
    server.unwatch(&file);
    done()
}

/// Whether notes are on.
#[derive(Debug, serde::Deserialize)]
struct MemorySwitch {
    enabled: bool,
}

async fn memory(State(server): Shared) -> Answer {
    ok(json!({ "enabled": server.memory_on() }))
}

async fn set_memory(State(server): Shared, axum::Json(switch): axum::Json<MemorySwitch>) -> Answer {
    server.set_memory(switch.enabled)?;
    ok(json!({ "enabled": server.memory_on() }))
}

async fn notes(State(server): Shared, Query(q): Params) -> Answer {
    let notes = server.notes(q.get("projectID").map(String::as_str))?;
    ok(json!({ "data": notes }))
}

/// A note's corrected text.
#[derive(Debug, serde::Deserialize)]
struct Correction {
    text: String,
}

async fn correct_note(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(c): axum::Json<Correction>,
) -> Answer {
    let note = server.correct_note(&id, &c.text)?;
    ok(json!({ "data": note }))
}

async fn forget_note(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.forget_note(&id)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn phone_state(State(server): Shared, Query(q): Query<After>) -> Answer {
    let state = match q.after {
        Some(after) => server.phone.state_after(after).await,
        None => server.phone.state(),
    };
    ok(serde_json::to_value(state).unwrap_or_default())
}

async fn want_phone(
    State(server): Shared,
    axum::Json(wanted): axum::Json<opencode_client::PhoneWanted>,
) -> Answer {
    let state = server.phone.want(wanted).await;
    ok(serde_json::to_value(state).unwrap_or_default())
}

async fn pair_phone(State(server): Shared) -> Answer {
    let link = server.phone.pair().await.map_err(Refusal::Bad)?;
    ok(json!({ "link": link }))
}

async fn remove_phone(State(server): Shared, Path(id): Path<String>) -> Answer {
    if server.phone.remove(&id).await.map_err(Refusal::Bad)? {
        done()
    } else {
        Err(Refusal::NotFound(format!("There's no paired phone {id}.")))
    }
}

async fn test_phone(State(server): Shared) -> Answer {
    let sent = server.phone.test().await.map_err(Refusal::Bad)?;
    ok(json!({ "sent": sent }))
}

async fn read_goal(State(server): Shared, axum::Json(body): axum::Json<Value>) -> Answer {
    let session = body["input"]["sessionID"].as_str().unwrap_or_default();
    ok(json!({ "output": server.goal(session) }))
}
