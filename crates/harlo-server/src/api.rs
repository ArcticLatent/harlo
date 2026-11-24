//! The HTTP API: the part of OpenCode 2's API Harlo uses for a session's
//! turn, in OpenCode's paths and JSON, so `opencode-client` works against
//! this server unchanged.
//!
//! The event stream adds what reconnecting needs: each durable event goes
//! out with its sequence number as the SSE `id`, and a client that sends
//! `Last-Event-ID` (or `?after=`) gets the events it missed first.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeSet, HashMap};
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

/// The routes, behind the password check.
pub fn router(server: Arc<Server>) -> Router {
    Router::new()
        .route("/api/info", get(info))
        .route("/api/event", get(events))
        .route("/api/project", get(projects))
        .route("/api/worktree", get(worktrees))
        .route("/api/session", get(sessions).post(create_session))
        .route("/api/session/active", get(active))
        .route("/api/session/{id}", get(session).patch(update_session))
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
        .route("/api/session/{id}/permission", get(permissions))
        .route("/api/session/{id}/permission/{request}/reply", post(reply))
        .route("/api/session/{id}/prompt", post(prompt))
        .route("/api/session/{id}/model", post(set_model))
        .route("/api/session/{id}/agent", post(set_agent))
        .route("/api/session/{id}/interrupt", post(interrupt))
        .route("/api/session/{id}/revert", delete(cancel_revert))
        .route("/api/session/{id}/revert/stage", post(stage_revert))
        .route("/api/session/{id}/revert/commit", post(commit_revert))
        .route("/api/agent", get(agents))
        .route("/api/model", get(models))
        .route("/api/model/default", get(default_model))
        .route("/api/command", get(located_empty))
        .route("/api/skill", get(located_empty))
        .route("/api/config", get(config))
        .route("/api/mcp/resource", get(mcp_resources))
        .route("/api/experimental/job", get(empty_data))
        .route("/api/vcs/diff", get(vcs_diff))
        .route("/api/rpc/opencode.goal/get", post(no_goal))
        .route("/api/experimental/client-mcp", get(no_client_tools))
        // No web search tool yet: truthfully, no providers.
        .route("/api/websearch/provider", get(located_empty))
        // Harlo's own: phone access hosted here.
        .route("/api/fs/find", get(find_files))
        .route("/api/fs/list", get(list_files))
        .route("/api/fs/read/{*path}", get(read_file))
        .route("/api/experimental/fs/write", post(write_file))
        .route("/harlo/memory", get(memory).put(set_memory))
        .route("/harlo/notes", get(notes))
        .route("/harlo/notes/{id}", patch(correct_note).delete(forget_note))
        .route("/harlo/phone", get(phone_state).put(want_phone))
        .route("/harlo/phone/pair", post(pair_phone))
        .route("/harlo/phone/device/{id}", delete(remove_phone))
        .route("/harlo/phone/test", post(test_phone))
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
        "capabilities": { "persistentPty": false, "clientTools": false, "phone": true, "memory": true },
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
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for s in server.store.sessions()?.iter().rev() {
        if seen.insert(s.project_id.clone()) {
            let vcs = git::is_repo(&s.location.directory).await.then_some("git");
            let time = json!({ "created": s.time.created, "updated": s.time.updated, "active": s.time.updated });
            let mut p = json!({ "id": s.project_id, "canonical": s.location.directory, "time": time, "sandboxes": [] });
            if let Some(vcs) = vcs {
                p["vcs"] = json!(vcs);
            }
            out.push(p);
        }
    }
    ok(json!(out))
}

async fn worktrees(State(server): Shared, Query(q): Params) -> Answer {
    let project = q.get("projectID").cloned().unwrap_or_default();
    let folders: BTreeSet<String> = server
        .store
        .sessions()?
        .into_iter()
        .filter(|s| s.project_id == project)
        .map(|s| s.location.directory)
        .collect();
    ok(json!(
        folders
            .into_iter()
            .map(|d| json!({ "directory": d }))
            .collect::<Vec<_>>()
    ))
}

async fn sessions(State(server): Shared, Query(q): Params) -> Answer {
    let directory = q.get("directory");
    let list: Vec<_> = server
        .store
        .sessions()?
        .into_iter()
        .filter(|s| directory.is_none_or(|d| &s.location.directory == d))
        .collect();
    ok(json!({ "data": list, "cursor": { "previous": null, "next": null } }))
}

#[derive(Deserialize)]
struct NewSession {
    location: NewLocation,
}

#[derive(Deserialize)]
struct NewLocation {
    directory: String,
}

async fn create_session(State(server): Shared, axum::Json(body): axum::Json<NewSession>) -> Answer {
    let session = server.create_session(&body.location.directory).await?;
    ok(json!({ "data": session }))
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
}

async fn update_session(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<SessionChange>,
) -> Answer {
    server.update_session(&id, body.permissions, body.title)?;
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
}

async fn prompt(
    State(server): Shared,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Prompt>,
) -> Answer {
    let delivery = body.delivery.unwrap_or_else(|| "queue".to_owned());
    ok(server.prompt(&id, body.id, &body.text, &delivery)?)
}

async fn interrupt(State(server): Shared, Path(id): Path<String>) -> Answer {
    server.session(&id)?;
    server.interrupt(&id);
    done()
}

async fn agents(State(server): Shared, Query(q): Params) -> Answer {
    ok(json!({ "location": location(&q), "data": server.agents_json() }))
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
    let list: Vec<Value> = server
        .config
        .providers
        .iter()
        .flat_map(|p| p.models.iter().map(move |m| model_json(p, m)))
        .collect();
    ok(json!({ "location": location(&q), "data": list }))
}

async fn default_model(State(server): Shared, Query(q): Params) -> Answer {
    let model = server
        .config
        .default_model()
        .and_then(|r| server.config.resolve(&r).map(|(p, m)| model_json(p, m)))
        .unwrap_or(Value::Null);
    ok(json!({ "location": location(&q), "data": model }))
}

async fn config(State(server): Shared) -> Answer {
    let mut out = Vec::new();
    if let (Some(path), Some(info)) = (&server.config.source, &server.config_text) {
        out.push(json!({ "type": "document", "path": path, "info": info }));
    }
    out.push(json!({ "type": "directory", "path": server.config_dir }));
    ok(json!(out))
}

async fn mcp_resources(Query(q): Params) -> Answer {
    ok(json!({ "location": location(&q), "data": { "resources": [], "templates": [] } }))
}

async fn located_empty(Query(q): Params) -> Answer {
    ok(json!({ "location": location(&q), "data": [] }))
}

async fn empty_data() -> Answer {
    ok(json!({ "data": [] }))
}

/// Client tools aren't taken yet: a plain "unavailable", so a client stops
/// asking and shows why, rather than an empty success that would look like
/// its tools were registered.
async fn no_client_tools() -> Response {
    let body = json!({
        "_tag": "CapabilityUnavailableError",
        "message": "This server doesn't offer clients' tools to its agents yet.",
        "data": { "capability": "clientTools" },
    });
    (StatusCode::NOT_IMPLEMENTED, axum::Json(body)).into_response()
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

async fn no_goal() -> Answer {
    ok(json!({ "output": null }))
}
