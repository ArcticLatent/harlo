//! MCP servers: the ones the config names (`mcp.servers`), those added
//! while the server runs, and those a client hosts over a WebSocket (such
//! as Harlo's own tools).
//!
//! A server connects the first time a folder that names it is listed or
//! runs a turn, and stays connected while its entry is unchanged. Folders
//! that name the same entry share one connection, unless the server is a
//! program started in the folder: then each folder has its own. Its
//! tools reach agents through Code Mode (`tools.<server>.<tool>`) unless
//! its entry says `codemode: false`; then each is offered on its own as
//! `<server>_<tool>`.
//!
//! A server that offers resources gets one more tool, `read_resource`,
//! which reads one by its uri or lists them; a resource mentioned in a
//! prompt is read the same way and sent with it.
//!
//! A connection that failed is tried again when a turn starts in a folder
//! that names the server, half a minute after it failed and twice as long
//! after each further failure, up to ten minutes. Nothing is tried while
//! no turn starts, and a server that needs a sign-in waits for it.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::process::Stdio;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use futures_util::{SinkExt as _, StreamExt as _};
use harlo_engine::codemode::{Direct, Namespace, ToolInfo};
use rmcp::model::{CallToolRequestParams, ReadResourceRequestParams};
use rmcp::service::{
    NotificationContext, Peer, RoleClient, RunningService, RxJsonRpcMessage, TxJsonRpcMessage,
};
use rmcp::transport::auth::{AuthClient, AuthorizationManager};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{ClientHandler, ServiceExt as _};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::server::{Refusal, Server, lock};

/// How long a server has to start, unless its entry says.
const STARTUP: Duration = Duration::from_secs(30);

/// How long a server that failed is left before a turn tries it again: a
/// computer that woke before its network, or a server started a moment
/// late, shouldn't need the person to reconnect it by hand. Doubled with
/// each try in a row that fails, up to [`RETRY_MOST`], so one that is gone
/// for good costs next to nothing.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The longest a failed server is left between tries.
const RETRY_MOST: Duration = Duration::from_mins(10);

/// Names another [`RETRY_AFTER`], in milliseconds, for tests: theirs
/// can't wait half a minute.
const RETRY_VAR: &str = "HARLO_MCP_RETRY_MS";

/// How long a turn waits for a server it is trying again. One that
/// answers is there in a moment; one that doesn't mustn't hold up every
/// turn for its whole startup time.
const RETRY_WAIT: Duration = Duration::from_secs(2);

/// The tool Harlo adds to a server that offers resources, unless the
/// server has a tool of that name itself.
const READ_RESOURCE: &str = "read_resource";

/// How many of a server's resources the description of [`READ_RESOURCE`]
/// names; it goes with every request, and the tool lists them all.
const NAMED: usize = 8;

/// How long a tool call may take, unless its entry says.
const EXECUTION: Duration = Duration::from_mins(5);

/// The most pictures one tool result brings the model: each costs about as
/// much as a page of text.
const PICTURES: usize = 4;

/// The largest picture a tool result brings the model, in base64 bytes
/// (about 3.7 MB of picture): what providers take in one image.
const PICTURE_BYTES: usize = 5 * 1024 * 1024;

/// The `_meta` key a client-hosted server reads the calling session from.
const SESSION_KEY: &str = "harlo/sessionID";

/// How a server stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Pending,
    Connected,
    Failed(String),
    NeedsAuth(String),
}

/// What's known of a connected server.
#[derive(Default)]
struct State {
    status: Option<Status>,
    tools: Vec<Value>,
    resources: Vec<Value>,
    templates: Vec<Value>,
    peer: Option<Peer<RoleClient>>,
    /// When it failed, while it stands failed.
    failed_at: Option<Instant>,
    /// How many tries in a row have failed before this one.
    attempt: u32,
}

/// One server connection.
pub(crate) struct Link {
    /// Its name.
    pub(crate) name: String,
    /// Whether its tools go through Code Mode.
    codemode: bool,
    /// Whether a client hosts it, so calls say which session made them.
    hosted: bool,
    startup: Duration,
    execution: Duration,
    /// When it was started.
    started: Instant,
    state: Mutex<State>,
    /// Ends the connection.
    cancel: CancellationToken,
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Link {
    /// A link to server `name`, as entry `spec` describes it.
    fn new(name: &str, spec: &Value, hosted: bool) -> Self {
        let millis = |key: &str, default: Duration| {
            spec["timeout"][key]
                .as_u64()
                .map_or(default, Duration::from_millis)
        };
        Self {
            name: name.to_owned(),
            codemode: spec["codemode"].as_bool().unwrap_or(true),
            hosted,
            startup: millis("startup", STARTUP),
            execution: millis("execution", EXECUTION),
            started: Instant::now(),
            state: Mutex::new(State {
                status: Some(Status::Pending),
                ..State::default()
            }),
            cancel: CancellationToken::new(),
        }
    }

    fn status(&self) -> Status {
        lock(&self.state).status.clone().unwrap_or(Status::Pending)
    }

    fn set_status(&self, status: Status) {
        let mut state = lock(&self.state);
        state.failed_at = matches!(status, Status::Failed(_)).then(Instant::now);
        state.status = Some(status);
    }

    /// Notes it's connected through `peer`.
    fn connected(&self, peer: Peer<RoleClient>) {
        let mut state = lock(&self.state);
        state.peer = Some(peer);
        state.status = Some(Status::Connected);
        state.failed_at = None;
        state.attempt = 0;
    }

    /// The count of this try when it failed long enough ago to be tried
    /// again; none while it stands any other way.
    fn due(&self) -> Option<u32> {
        let state = lock(&self.state);
        let failed_at = state.failed_at?;
        (failed_at.elapsed() >= retry_after(state.attempt)).then_some(state.attempt)
    }

    /// Until when a turn waits for it while it is starting: its startup
    /// time on a first start, a moment when it is being tried again.
    fn awaited(&self) -> Instant {
        let patience = if lock(&self.state).attempt > 0 {
            self.startup.min(RETRY_WAIT)
        } else {
            self.startup
        };
        self.started + patience
    }

    /// How it stands, as clients read it.
    fn status_json(&self) -> Value {
        match self.status() {
            Status::Pending => json!({ "status": "pending" }),
            Status::Connected => json!({ "status": "connected" }),
            Status::Failed(error) => json!({ "status": "failed", "error": error }),
            Status::NeedsAuth(error) => json!({ "status": "needs_auth", "error": error }),
        }
    }

    /// Asks the server for its tools and resources again.
    async fn refresh(&self, peer: &Peer<RoleClient>) {
        let tools = peer
            .list_all_tools()
            .await
            .map(|l| values(&l))
            .unwrap_or_default();
        let resources = peer
            .list_all_resources()
            .await
            .map(|l| values(&l))
            .unwrap_or_default();
        let templates = peer
            .list_all_resource_templates()
            .await
            .map(|l| values(&l))
            .unwrap_or_default();
        let mut state = lock(&self.state);
        state.tools = tools;
        state.resources = resources;
        state.templates = templates;
    }

    /// Calls tool `tool` with `args`, for session `session`; returns the
    /// server's result as it sent it.
    async fn result(
        &self,
        tool: &str,
        args: Value,
        session: Option<&str>,
    ) -> Result<Value, String> {
        let peer = lock(&self.state)
            .peer
            .clone()
            .ok_or_else(|| format!("The MCP server {} isn't connected.", self.name))?;
        if tool == READ_RESOURCE && resource_tool(&lock(&self.state)).is_some() {
            return self.read_resource(&peer, &args).await;
        }
        let mut params = json!({ "name": tool, "arguments": args });
        if self.hosted
            && let Some(session) = session
        {
            params["_meta"] = json!({ SESSION_KEY: session });
        }
        let params: CallToolRequestParams = serde_json::from_value(params)
            .map_err(|e| format!("The arguments don't fit {tool}: {e}"))?;
        let result = tokio::time::timeout(self.execution, peer.call_tool(params))
            .await
            .map_err(|_late| format!("{}.{tool} took too long.", self.name))?
            .map_err(|e| format!("{}.{tool} failed: {e}", self.name))?;
        Ok(serde_json::to_value(result).unwrap_or_default())
    }

    /// Reads the resource `args` names by `uri`, or lists what the server
    /// offers when it names none; returns it in a tool result's form.
    async fn read_resource(&self, peer: &Peer<RoleClient>, args: &Value) -> Result<Value, String> {
        let Some(uri) = args["uri"].as_str().filter(|uri| !uri.is_empty()) else {
            let state = lock(&self.state);
            let offered = json!({ "resources": state.resources, "templates": state.templates });
            return Ok(json!({ "content": [{ "type": "text", "text": offered.to_string() }] }));
        };
        let content: Vec<Value> = self
            .contents(peer, uri)
            .await?
            .iter()
            .map(|part| {
                let mime = part["mimeType"].as_str().unwrap_or_default();
                match (part["text"].as_str(), part["blob"].as_str()) {
                    (Some(text), _) => json!({ "type": "text", "text": text }),
                    (None, Some(blob)) if mime.starts_with("image/") => {
                        json!({ "type": "image", "data": blob, "mimeType": mime })
                    }
                    _ => json!({ "type": "text", "text": format!("({mime}: not text)") }),
                }
            })
            .collect();
        Ok(json!({ "content": content }))
    }

    /// What the server gives for the resource at `uri`: its parts, each
    /// with `text` or a base64 `blob`, as the server sent them.
    async fn contents(&self, peer: &Peer<RoleClient>, uri: &str) -> Result<Vec<Value>, String> {
        let params: ReadResourceRequestParams = serde_json::from_value(json!({ "uri": uri }))
            .map_err(|e| format!("That isn't a resource's uri: {e}"))?;
        let read = tokio::time::timeout(self.execution, peer.read_resource(params))
            .await
            .map_err(|_late| format!("Reading {uri} from {} took too long.", self.name))?
            .map_err(|e| format!("{} couldn't read {uri}: {e}", self.name))?;
        let read = serde_json::to_value(read).unwrap_or_default();
        Ok(read["contents"].as_array().cloned().unwrap_or_default())
    }
}

/// How long a server whose last `attempt` tries in a row failed is left
/// before the next.
fn retry_after(attempt: u32) -> Duration {
    let first = std::env::var(RETRY_VAR)
        .ok()
        .and_then(|millis| millis.parse().ok())
        .map_or(RETRY_AFTER, Duration::from_millis);
    first
        .saturating_mul(2_u32.saturating_pow(attempt))
        .min(RETRY_MOST)
}

/// What [`READ_RESOURCE`] says of itself on a server that stands as
/// `state`; none when the server offers nothing to read, or has a tool of
/// that name itself.
fn resource_tool(state: &State) -> Option<String> {
    let offered = state.resources.len() + state.templates.len();
    if offered == 0 || state.tools.iter().any(|t| t["name"] == READ_RESOURCE) {
        return None;
    }
    let resources = state.resources.iter().map(|r| {
        let uri = r["uri"].as_str().unwrap_or_default();
        match r["name"]
            .as_str()
            .filter(|name| !name.is_empty() && *name != uri)
        {
            Some(name) => format!("{uri} ({name})"),
            None => uri.to_owned(),
        }
    });
    let templates = state
        .templates
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap_or_default().to_owned());
    let named: Vec<String> = resources.chain(templates).take(NAMED).collect();
    let more = match offered - named.len() {
        0 => String::new(),
        more => format!(", and {more} more"),
    };
    Some(format!(
        "Reads a resource this server offers, by its uri; with no uri, lists them all. Offered: {}{more}.",
        named.join(", ")
    ))
}

/// `list` as JSON values.
fn values<T: serde::Serialize>(list: &[T]) -> Vec<Value> {
    list.iter()
        .filter_map(|v| serde_json::to_value(v).ok())
        .collect()
}

/// A tool result as a value: its structured content, its text read as
/// JSON when it is JSON, or its text.
fn answer(result: &Value) -> Result<Value, String> {
    let text: Vec<String> = result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|block| match block["type"].as_str() {
            Some("text") => block["text"].as_str().unwrap_or_default().to_owned(),
            Some("resource") => block["resource"]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            Some(other) => format!(
                "({other} {})",
                block["mimeType"].as_str().unwrap_or_default()
            ),
            None => String::new(),
        })
        .collect();
    let text = text.join("\n");
    if result["isError"].as_bool() == Some(true) {
        return Err(text);
    }
    if let Some(structured) = result.get("structuredContent").filter(|v| !v.is_null()) {
        return Ok(structured.clone());
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

/// The pictures in a tool result, for the model to look at: the first few,
/// those of a size a provider takes.
fn pictures(result: &Value) -> Vec<harlo_engine::attachment::File> {
    result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "image")
        .filter_map(|block| {
            let data = block["data"].as_str()?;
            let mime = block["mimeType"].as_str()?;
            (mime.starts_with("image/") && data.len() <= PICTURE_BYTES).then(|| {
                harlo_engine::attachment::File {
                    data: data.to_owned(),
                    mime: mime.to_owned(),
                    source: json!({ "type": "inline" }),
                    name: None,
                    mention: None,
                }
            })
        })
        .take(PICTURES)
        .collect()
}

/// A JSON Schema type as TypeScript writes it.
fn type_of(schema: &Value) -> String {
    match schema["type"].as_str() {
        Some("string") => "string".to_owned(),
        Some("number" | "integer") => "number".to_owned(),
        Some("boolean") => "boolean".to_owned(),
        Some("array") => format!("{}[]", type_of(&schema["items"])),
        Some("object") => object_type(schema),
        _ => "unknown".to_owned(),
    }
}

/// An object schema as a TypeScript object type.
pub(crate) fn object_type(schema: &Value) -> String {
    let required: HashSet<&str> = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let fields: Vec<String> = schema["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, field)| {
            let optional = if required.contains(name.as_str()) {
                ""
            } else {
                "?"
            };
            format!("{name}{optional}: {}", type_of(field))
        })
        .collect();
    format!("{{{}}}", fields.join(", "))
}

/// The name a tool is offered under on its own: `<server>_<tool>`, only
/// letters, digits, `_` and `-`, at most 64 characters.
fn direct_name(server: &str, tool: &str) -> String {
    format!("{server}_{tool}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

/// The Code Mode namespaces `links` offer.
pub(crate) fn namespaces(links: &[Arc<Link>]) -> Vec<Namespace> {
    links
        .iter()
        .filter(|l| l.codemode && l.status() == Status::Connected)
        .map(|l| Namespace {
            name: l.name.clone(),
            description: format!("The tools of the MCP server {}.", l.name),
            tools: {
                let state = lock(&l.state);
                let reader = resource_tool(&state).map(|description| ToolInfo {
                    name: READ_RESOURCE.to_owned(),
                    description,
                    signature: "({uri?: string}): unknown".to_owned(),
                });
                state
                    .tools
                    .iter()
                    .map(|t| ToolInfo {
                        name: t["name"].as_str().unwrap_or_default().to_owned(),
                        description: t["description"].as_str().unwrap_or_default().to_owned(),
                        signature: format!("({}): unknown", object_type(&t["inputSchema"])),
                    })
                    .chain(reader)
                    .collect()
            },
        })
        .collect()
}

/// The tools `links` offer on their own.
pub(crate) fn direct(links: &[Arc<Link>]) -> Vec<Direct> {
    links
        .iter()
        .filter(|l| !l.codemode && l.status() == Status::Connected)
        .flat_map(|l| {
            let state = lock(&l.state);
            let reader = resource_tool(&state).map(|description| Direct {
                name: direct_name(&l.name, READ_RESOURCE),
                description,
                parameters: json!({ "type": "object", "properties": { "uri": { "type": "string" } } }),
                namespace: l.name.clone(),
                tool: READ_RESOURCE.to_owned(),
            });
            state
                .tools
                .iter()
                .map(|t| {
                    let tool = t["name"].as_str().unwrap_or_default().to_owned();
                    Direct {
                        name: direct_name(&l.name, &tool),
                        description: t["description"].as_str().unwrap_or_default().to_owned(),
                        parameters: t["inputSchema"].clone(),
                        namespace: l.name.clone(),
                        tool,
                    }
                })
                .chain(reader)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Calls tool `tool` of server `namespace` among `links`, and returns its
/// answer with the pictures it came with.
pub(crate) async fn call_seen(
    links: &[Arc<Link>],
    namespace: &str,
    tool: &str,
    args: Value,
    session: Option<&str>,
) -> Option<Result<(Value, Vec<harlo_engine::attachment::File>), String>> {
    let link = links.iter().find(|l| l.name == namespace)?;
    let result = match link.result(tool, args, session).await {
        Ok(result) => result,
        Err(e) => return Some(Err(e)),
    };
    Some(answer(&result).map(|value| (value, pictures(&result))))
}

/// Follows a server's notices that its lists changed.
struct Watch {
    link: Weak<Link>,
    server: Weak<Server>,
}

impl Watch {
    /// Reads the lists again and tells clients.
    async fn changed(&self, peer: &Peer<RoleClient>, kind: &str) {
        let (Some(link), Some(server)) = (self.link.upgrade(), self.server.upgrade()) else {
            return;
        };
        link.refresh(peer).await;
        server.emit(kind, json!({ "server": link.name }), None, None);
    }
}

impl ClientHandler for Watch {
    async fn on_tool_list_changed(&self, context: NotificationContext<RoleClient>) {
        self.changed(&context.peer, "mcp.status.changed").await;
    }

    async fn on_resource_list_changed(&self, context: NotificationContext<RoleClient>) {
        self.changed(&context.peer, "mcp.resources.changed").await;
    }
}

/// What a connection that failed said, as the person reads it.
///
/// The MCP library words a failure with the Rust types it passed through
/// and the step it was at, such as `Send message error Transport [<type>]
/// error: <what>, when send initialize request`; only what went wrong is
/// kept, and an address nothing answered at is named plainly.
fn plain(said: &str) -> String {
    let what = said.split_once("] error: ").map_or(said, |(_, what)| what);
    let what = what
        .split_once(", when send ")
        .map_or(what, |(what, _)| what);
    let unreached = what
        .split_once("error sending request for url (")
        .and_then(|(_, rest)| rest.split_once(')'));
    match unreached {
        Some((url, _)) => format!("Couldn't reach {url}."),
        None => what.trim().to_owned(),
    }
}

/// Starts server entry `spec`, in `directory` unless it names a `cwd`.
async fn start(
    watch: Watch,
    spec: &Value,
    directory: &str,
    cancel: CancellationToken,
) -> Result<RunningService<RoleClient, Watch>, Status> {
    let failed = |e: &dyn fmt::Display| Status::Failed(plain(&e.to_string()));
    match spec["type"].as_str() {
        Some("local") => {
            let parts: Vec<&str> = spec["command"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let (program, args) = parts
                .split_first()
                .ok_or_else(|| Status::Failed("Its entry has no command.".to_owned()))?;
            let mut command = tokio::process::Command::new(program);
            command
                .args(args)
                .current_dir(spec["cwd"].as_str().unwrap_or(directory));
            for (name, value) in spec["environment"].as_object().into_iter().flatten() {
                command.env(name, value.as_str().unwrap_or_default());
            }
            let (transport, _) = TokioChildProcess::builder(command)
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| Status::Failed(format!("Couldn't start {program}: {e}")))?;
            watch
                .serve_with_ct(transport, cancel)
                .await
                .map_err(|e| failed(&e))
        }
        Some("remote") => {
            let url = spec["url"].as_str().unwrap_or_default();
            let headers: HashMap<_, _> = spec["headers"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(name, value)| {
                    Some((
                        reqwest::header::HeaderName::from_bytes(name.as_bytes()).ok()?,
                        reqwest::header::HeaderValue::from_str(value.as_str()?).ok()?,
                    ))
                })
                .collect();
            let config = StreamableHttpClientTransportConfig::with_uri(url).custom_headers(headers);
            let read = |e: &dyn fmt::Display| {
                let said = e.to_string();
                if crate::mcp_auth::wants_sign_in(&said) {
                    Status::NeedsAuth(said)
                } else {
                    Status::Failed(plain(&said))
                }
            };
            if let Some(manager) = signed_in(&watch, spec, url).await? {
                let client = AuthClient::new(reqwest::Client::default(), manager);
                let transport = StreamableHttpClientTransport::with_client(client, config);
                return watch
                    .serve_with_ct(transport, cancel)
                    .await
                    .map_err(|e| read(&e));
            }
            let transport = StreamableHttpClientTransport::from_config(config);
            watch
                .serve_with_ct(transport, cancel)
                .await
                .map_err(|e| read(&e))
        }
        _ => Err(Status::Failed(
            "Its entry is neither local nor remote.".to_owned(),
        )),
    }
}

/// The sign-in remote server entry `spec` at `url` has, ready to use,
/// when one was kept.
///
/// # Errors
///
/// When the kept sign-in can't be used, which asks for a new one.
async fn signed_in(
    watch: &Watch,
    spec: &Value,
    url: &str,
) -> Result<Option<AuthorizationManager>, Status> {
    let (Some(id), Some(server), Some(link)) = (
        crate::mcp_auth::integration_id(spec),
        watch.server.upgrade(),
        watch.link.upgrade(),
    ) else {
        return Ok(None);
    };
    if !server.has_credential(&id) {
        return Ok(None);
    }
    let again = |e: &dyn fmt::Display| Status::NeedsAuth(format!("Sign in again: {e}"));
    let mut manager = AuthorizationManager::new(url)
        .await
        .map_err(|e| again(&e))?;
    manager.set_credential_store(crate::mcp_auth::Kept::new(
        Arc::downgrade(&server),
        &id,
        &link.name,
    ));
    if manager
        .initialize_from_store()
        .await
        .map_err(|e| again(&e))?
    {
        Ok(Some(manager))
    } else {
        Ok(None)
    }
}

/// What tells one connection from another.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    /// The server's name.
    name: String,
    /// Its entry, as JSON text.
    spec: String,
    /// The folder it starts in, when that's the folder that names it;
    /// empty for a server that's the same from anywhere.
    place: String,
}

impl Key {
    /// The key of server `name` with entry `spec`, as `directory` names it.
    fn new(directory: &str, name: &str, spec: &Value) -> Self {
        // A program started without a folder of its own runs in the folder
        // that names it, and works on that folder.
        let local = spec["type"].as_str() == Some("local") && spec["cwd"].as_str().is_none();
        Self {
            name: name.to_owned(),
            spec: spec.to_string(),
            place: if local {
                directory.to_owned()
            } else {
                String::new()
            },
        }
    }
}

/// A connection, and the folders using it.
struct Held {
    link: Arc<Link>,
    folders: HashSet<String>,
}

/// Stops the connections in `links` that `gone` picks, and forgets them.
fn stop(links: &mut HashMap<Key, Held>, gone: impl Fn(&Key, &Held) -> bool) {
    links.retain(|key, held| {
        let gone = gone(key, held);
        if gone {
            held.link.cancel.cancel();
        }
        !gone
    });
}

/// The connections, and what was changed while the server runs.
#[derive(Default)]
pub(crate) struct Mcp {
    /// Configured servers' links.
    links: Mutex<HashMap<Key, Held>>,
    /// Servers clients host, by name.
    hosted: Mutex<BTreeMap<String, Arc<Link>>>,
    /// Servers turned off until connected again.
    off: Mutex<HashSet<String>>,
    /// Servers added while running, by name.
    added: Mutex<BTreeMap<String, Value>>,
}

impl fmt::Debug for Mcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mcp").finish_non_exhaustive()
    }
}

/// Whether entry `spec` is turned off.
fn disabled(spec: &Value) -> bool {
    spec["disabled"].as_bool() == Some(true) || spec["enabled"].as_bool() == Some(false)
}

impl Server {
    /// The servers `directory`'s config names, with those added since.
    fn mcp_entries(&self, directory: &str) -> BTreeMap<String, Value> {
        let mut entries: BTreeMap<String, Value> = self
            .folder(directory)
            .config
            .mcp
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        entries.extend(lock(&self.mcp.added).clone());
        entries
    }

    /// The link to server `name` with entry `spec`, started if it's new.
    fn mcp_link(self: &Arc<Self>, directory: &str, name: &str, spec: &Value) -> Arc<Link> {
        let key = Key::new(directory, name, spec);
        let mut links = lock(&self.mcp.links);
        // The entry this folder had by the name before is left, and stops
        // once no folder uses it. Another folder's stays as it is.
        links.retain(|k, held| {
            if k.name != name || *k == key || !held.folders.remove(directory) {
                return true;
            }
            if held.folders.is_empty() {
                held.link.cancel.cancel();
            }
            !held.folders.is_empty()
        });
        if let Some(held) = links.get_mut(&key) {
            held.folders.insert(directory.to_owned());
            return Arc::clone(&held.link);
        }
        let link = Arc::new(Link::new(name, spec, false));
        links.insert(
            key,
            Held {
                link: Arc::clone(&link),
                folders: HashSet::from([directory.to_owned()]),
            },
        );
        drop(links);
        let (server, spec, directory) = (Arc::clone(self), spec.clone(), directory.to_owned());
        let started = Arc::clone(&link);
        tokio::spawn(async move {
            server.run_link(started, &spec, &directory).await;
        });
        link
    }

    /// Starts server `name` of `directory` afresh when its connection
    /// failed long enough ago; gives the new link.
    fn mcp_again(self: &Arc<Self>, directory: &str, name: &str, spec: &Value) -> Option<Arc<Link>> {
        let link = {
            let mut links = lock(&self.mcp.links);
            let held = links.get_mut(&Key::new(directory, name, spec))?;
            let attempt = held.link.due()?;
            held.link.cancel.cancel();
            let link = Arc::new(Link::new(name, spec, false));
            lock(&link.state).attempt = attempt + 1;
            held.link = Arc::clone(&link);
            link
        };
        tracing::info!(name: "mcp.retried", server = %name, "trying a failed MCP server again");
        self.emit("mcp.status.changed", json!({ "server": name }), None, None);
        let (server, spec, directory) = (Arc::clone(self), spec.clone(), directory.to_owned());
        let started = Arc::clone(&link);
        tokio::spawn(async move {
            server.run_link(started, &spec, &directory).await;
        });
        Some(link)
    }

    /// Connects `link` and follows it until it ends.
    async fn run_link(self: Arc<Self>, link: Arc<Link>, spec: &Value, directory: &str) {
        let watch = Watch {
            link: Arc::downgrade(&link),
            server: Arc::downgrade(&self),
        };
        let started = tokio::time::timeout(
            link.startup,
            // (The library cancels the token it is given once the
            // connection ends: a child of the link's, so that the link's
            // own still says whether Harlo ended it.)
            start(watch, spec, directory, link.cancel.child_token()),
        )
        .await
        .unwrap_or_else(|_late| Err(Status::Failed("It took too long to start.".to_owned())));
        self.follow(link, started).await;
    }

    /// Keeps `started`'s tools on `link` and notes when it ends.
    async fn follow(
        &self,
        link: Arc<Link>,
        started: Result<RunningService<RoleClient, Watch>, Status>,
    ) {
        let service = match started {
            Ok(service) => service,
            Err(status) => {
                tracing::warn!(name: "mcp.failed", server = %link.name, status = ?status, "an MCP server didn't start");
                link.set_status(status);
                self.emit(
                    "mcp.status.changed",
                    json!({ "server": link.name }),
                    None,
                    None,
                );
                return;
            }
        };
        let peer = service.peer().clone();
        link.refresh(&peer).await;
        link.connected(peer);
        self.emit(
            "mcp.status.changed",
            json!({ "server": link.name }),
            None,
            None,
        );
        let _ = service.waiting().await;
        lock(&link.state).peer = None;
        if !link.cancel.is_cancelled() {
            link.set_status(Status::Failed("The server stopped.".to_owned()));
            self.emit(
                "mcp.status.changed",
                json!({ "server": link.name }),
                None,
                None,
            );
        }
    }

    /// The servers `directory` names and how each stands; starts those
    /// not yet started.
    pub(crate) fn mcp_servers(self: &Arc<Self>, directory: &str) -> Vec<Value> {
        let off = lock(&self.mcp.off).clone();
        self.mcp_entries(directory)
            .iter()
            .map(|(name, spec)| {
                let status = if disabled(spec) || off.contains(name) {
                    json!({ "status": "disabled" })
                } else {
                    self.mcp_link(directory, name, spec).status_json()
                };
                let mut entry = json!({ "name": name, "status": status });
                if let Some(id) = crate::mcp_auth::integration_id(spec) {
                    entry["integrationID"] = json!(id);
                }
                entry
            })
            .collect()
    }

    /// The connected servers a turn in `directory` can use, after waiting
    /// for those still starting.
    pub(crate) async fn mcp_ready(self: &Arc<Self>, directory: &str) -> Vec<Arc<Link>> {
        let off = lock(&self.mcp.off).clone();
        let mut links: Vec<Arc<Link>> = self
            .mcp_entries(directory)
            .iter()
            .filter(|(name, spec)| !disabled(spec) && !off.contains(*name))
            .map(|(name, spec)| {
                let link = self.mcp_link(directory, name, spec);
                self.mcp_again(directory, name, spec).unwrap_or(link)
            })
            .collect();
        for link in &links {
            let until = link.awaited();
            while link.status() == Status::Pending && Instant::now() < until {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        links.extend(lock(&self.mcp.hosted).values().cloned());
        links.retain(|l| l.status() == Status::Connected);
        links
    }

    /// Connects server `name` again, as `directory` names it.
    ///
    /// # Errors
    ///
    /// When `directory` names no such server.
    pub(crate) fn mcp_connect(
        self: &Arc<Self>,
        directory: &str,
        name: &str,
    ) -> Result<(), Refusal> {
        let entries = self.mcp_entries(directory);
        let spec = entries.get(name).ok_or_else(|| not_found(name))?;
        lock(&self.mcp.off).remove(name);
        // A failed connection is tried again.
        stop(&mut lock(&self.mcp.links), |k, held| {
            k.name == name
                && held.folders.contains(directory)
                && held.link.status() != Status::Connected
        });
        let _ = self.mcp_link(directory, name, spec);
        self.emit("mcp.status.changed", json!({ "server": name }), None, None);
        Ok(())
    }

    /// Disconnects server `name` until it's connected again.
    ///
    /// # Errors
    ///
    /// When `directory` names no such server.
    pub(crate) fn mcp_disconnect(&self, directory: &str, name: &str) -> Result<(), Refusal> {
        if !self.mcp_entries(directory).contains_key(name) {
            return Err(not_found(name));
        }
        lock(&self.mcp.off).insert(name.to_owned());
        stop(&mut lock(&self.mcp.links), |k, _| k.name == name);
        self.emit("mcp.status.changed", json!({ "server": name }), None, None);
        Ok(())
    }

    /// Adds server `name` with entry `spec` while running, or replaces it,
    /// and connects it.
    pub(crate) fn mcp_add(self: &Arc<Self>, directory: &str, name: &str, spec: Value) {
        lock(&self.mcp.added).insert(name.to_owned(), spec.clone());
        lock(&self.mcp.off).remove(name);
        let _ = self.mcp_link(directory, name, &spec);
        drop(spec);
        self.emit("mcp.status.changed", json!({ "server": name }), None, None);
    }

    /// The entries of the servers added while running, then of those
    /// connected or connecting, by name.
    pub(crate) fn mcp_specs(&self) -> Vec<(String, Value)> {
        let mut specs: Vec<(String, Value)> = lock(&self.mcp.added)
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        specs.extend(
            lock(&self.mcp.links)
                .keys()
                .filter_map(|k| Some((k.name.clone(), serde_json::from_str(&k.spec).ok()?))),
        );
        specs
    }

    /// Connects server `name` again, everywhere, after its sign-in changed.
    pub(crate) fn mcp_signed_in(&self, name: &str) {
        stop(&mut lock(&self.mcp.links), |k, _| k.name == name);
        self.emit("mcp.status.changed", json!({ "server": name }), None, None);
    }

    /// Removes server `name` added while running.
    pub(crate) fn mcp_remove(&self, name: &str) {
        lock(&self.mcp.added).remove(name);
        stop(&mut lock(&self.mcp.links), |k, _| k.name == name);
        self.emit("mcp.status.changed", json!({ "server": name }), None, None);
    }

    /// The links to `directory`'s servers as they stand now, those turned
    /// off left out; starts those not yet started.
    fn mcp_links(self: &Arc<Self>, directory: &str) -> Vec<Arc<Link>> {
        let _ = self.mcp_servers(directory);
        let off = lock(&self.mcp.off).clone();
        let entries = self.mcp_entries(directory);
        lock(&self.mcp.links)
            .iter()
            .filter(|(k, held)| {
                entries
                    .get(&k.name)
                    .is_some_and(|spec| **k == Key::new(directory, &k.name, spec))
                    && held.folders.contains(directory)
                    && !off.contains(&k.name)
            })
            .map(|(_, held)| Arc::clone(&held.link))
            .collect()
    }

    /// The resource at `uri` as a file to send with a prompt, named
    /// `name`: read from the connected server of `directory` that offers
    /// it.
    ///
    /// # Errors
    ///
    /// When no connected server of the folder offers it, or the server
    /// can't read it.
    pub(crate) async fn mcp_file(
        self: &Arc<Self>,
        directory: &str,
        uri: &str,
        name: Option<String>,
    ) -> Result<harlo_engine::attachment::File, Refusal> {
        let offering = self.mcp_links(directory).into_iter().find_map(|link| {
            let peer = {
                let state = lock(&link.state);
                let offers = state.resources.iter().any(|r| r["uri"] == uri);
                state.peer.clone().filter(|_| offers)
            }?;
            Some((link, peer))
        });
        let Some((link, peer)) = offering else {
            return Err(Refusal::Bad(format!(
                "No connected MCP server offers {uri}."
            )));
        };
        let contents = link.contents(&peer, uri).await.map_err(Refusal::Bad)?;
        let part = contents.first().cloned().unwrap_or_default();
        let source = json!({ "type": "uri", "uri": uri });
        match (part["text"].as_str(), part["blob"].as_str()) {
            (Some(text), _) => Ok(harlo_engine::attachment::File::new(
                text.as_bytes(),
                part["mimeType"].as_str().unwrap_or("text/plain"),
                source,
                name,
            )),
            (None, Some(blob)) => Ok(harlo_engine::attachment::File {
                data: blob.to_owned(),
                mime: part["mimeType"]
                    .as_str()
                    .unwrap_or("application/octet-stream")
                    .to_owned(),
                source,
                name,
                mention: None,
            }),
            _ => Err(Refusal::Bad(format!(
                "{} gave nothing for {uri}.",
                link.name
            ))),
        }
    }

    /// The resources and templates `directory`'s connected servers offer.
    pub(crate) fn mcp_resources(self: &Arc<Self>, directory: &str) -> Value {
        let links = self.mcp_links(directory);
        let mut resources = Vec::new();
        let mut templates = Vec::new();
        for link in &links {
            let state = lock(&link.state);
            let mark = |v: &Value| {
                let mut v = v.clone();
                v["server"] = json!(link.name);
                v
            };
            resources.extend(state.resources.iter().map(mark));
            templates.extend(state.templates.iter().map(mark));
        }
        json!({ "resources": resources, "templates": templates })
    }

    /// Takes the MCP server a client hosts over `socket` as `name`, until
    /// the socket closes.
    pub(crate) async fn host_mcp(
        self: Arc<Self>,
        name: String,
        socket: axum::extract::ws::WebSocket,
    ) {
        use axum::extract::ws::Message;
        let (sink, stream) = socket.split();
        // One JSON-RPC message per text frame each way.
        let sink = sink.with(|message: TxJsonRpcMessage<RoleClient>| async move {
            serde_json::to_string(&message)
                .map(|text| Message::Text(text.into()))
                .map_err(axum::Error::new)
        });
        let stream = stream.filter_map(|frame| async move {
            match frame {
                Ok(Message::Text(text)) => {
                    serde_json::from_str::<RxJsonRpcMessage<RoleClient>>(&text).ok()
                }
                _ => None,
            }
        });
        let link = Arc::new(Link::new(&name, &Value::Object(Map::new()), true));
        if let Some(old) = lock(&self.mcp.hosted).insert(name.clone(), Arc::clone(&link)) {
            old.cancel.cancel();
        }
        let watch = Watch {
            link: Arc::downgrade(&link),
            server: Arc::downgrade(&self),
        };
        let started = tokio::time::timeout(
            link.startup,
            watch.serve_with_ct(
                (Box::pin(sink), Box::pin(stream)),
                link.cancel.child_token(),
            ),
        )
        .await
        .map_err(|_late| Status::Failed("It took too long to start.".to_owned()))
        .and_then(|r| r.map_err(|e| Status::Failed(plain(&e.to_string()))));
        self.follow(Arc::clone(&link), started).await;
        let mut hosted = lock(&self.mcp.hosted);
        if hosted.get(&name).is_some_and(|l| Arc::ptr_eq(l, &link)) {
            hosted.remove(&name);
        }
    }
}

/// The refusal for a server no config names.
fn not_found(name: &str) -> Refusal {
    Refusal::NotFound(format!("MCP server not found: {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_connection_is_worded_without_type_names() {
        let said = "Send message error Transport \
                    [rmcp::transport::worker::WorkerTransport<rmcp::transport::\
                    streamable_http_client::StreamableHttpClientWorker<reqwest::async_impl::\
                    client::Client>>] error: Client error: error sending request for url \
                    (http://127.0.0.1:9/mcp), when send initialize request";
        assert_eq!(plain(said), "Couldn't reach http://127.0.0.1:9/mcp.");
        let said = "Send message error Transport [a::B<c::D>] error: Unexpected content type: \
                    text/html, when send initialize request";
        assert_eq!(plain(said), "Unexpected content type: text/html");
        // Anything worded another way is kept as it is.
        assert_eq!(
            plain("connection closed: initialize response"),
            "connection closed: initialize response"
        );
    }

    #[test]
    fn a_failed_server_is_left_twice_as_long_after_each_failure() {
        assert_eq!(retry_after(0), RETRY_AFTER);
        assert_eq!(retry_after(1), RETRY_AFTER * 2);
        assert_eq!(retry_after(3), RETRY_AFTER * 8);
        assert_eq!(retry_after(9), RETRY_MOST);
        assert_eq!(retry_after(u32::MAX), RETRY_MOST);
    }

    #[test]
    fn a_failed_link_is_due_once_its_wait_is_over() {
        let ago = |seconds: u64| Instant::now().checked_sub(Duration::from_secs(seconds));
        let link = Link::new("probe", &json!({}), false);
        assert_eq!(link.due(), None, "it is starting");
        link.set_status(Status::Failed("no".to_owned()));
        assert_eq!(link.due(), None, "it only just failed");
        lock(&link.state).failed_at = ago(45);
        assert_eq!(link.due(), Some(0));
        // After a try that failed too, it is left twice as long.
        lock(&link.state).attempt = 1;
        assert_eq!(link.due(), None);
        lock(&link.state).failed_at = ago(61);
        assert_eq!(link.due(), Some(1));
        // One that waits for a sign-in, or stopped being failed, isn't tried.
        link.set_status(Status::NeedsAuth("sign in".to_owned()));
        assert_eq!(link.due(), None);
        // A first start is waited for as long as it may take, a retry briefly.
        assert_eq!(link.awaited(), link.started + RETRY_WAIT);
        lock(&link.state).attempt = 0;
        assert_eq!(link.awaited(), link.started + STARTUP);
    }

    #[test]
    fn the_resource_tool_names_what_is_offered() {
        let mut state = State::default();
        assert_eq!(resource_tool(&state), None, "nothing to read");
        state.resources = (1..=10)
            .map(|n| json!({ "uri": format!("n://{n}"), "name": format!("Note {n}") }))
            .collect();
        state.templates = vec![json!({ "uriTemplate": "n://by/{id}" })];
        let said = resource_tool(&state).expect("a tool");
        assert!(
            said.contains("Offered: n://1 (Note 1), n://2 (Note 2)"),
            "{said}"
        );
        assert!(said.ends_with("n://8 (Note 8), and 3 more."), "{said}");
        // The server's own tool of that name is the one that answers.
        state.tools = vec![json!({ "name": READ_RESOURCE })];
        assert_eq!(resource_tool(&state), None);
    }

    #[test]
    fn schemas_read_as_signatures() {
        let schema = json!({
            "type": "object", "required": ["path"],
            "properties": { "path": { "type": "string" }, "lines": { "type": "array", "items": { "type": "integer" } } },
        });
        assert_eq!(object_type(&schema), "{lines?: number[], path: string}");
        assert_eq!(direct_name("my server", "read.file"), "my_server_read_file");
    }

    #[test]
    fn a_results_pictures_are_kept_for_the_model() {
        let result = json!({ "content": [
            { "type": "text", "text": "The window:" },
            { "type": "image", "data": "aGVsbG8=", "mimeType": "image/png" },
            { "type": "image", "data": "x".repeat(PICTURE_BYTES + 1), "mimeType": "image/png" },
            { "type": "audio", "data": "aGVsbG8=", "mimeType": "audio/wav" },
        ] });
        let found = pictures(&result);
        assert_eq!(found.len(), 1, "the one of a size to send");
        assert_eq!(
            (found[0].mime.as_str(), found[0].data.as_str()),
            ("image/png", "aGVsbG8=")
        );
        // The text still says what came, pictures included.
        assert_eq!(
            answer(&result),
            Ok(json!(
                "The window:\n(image image/png)\n(image image/png)\n(audio audio/wav)"
            ))
        );
        let many = json!({ "content": vec![
            json!({ "type": "image", "data": "aGVsbG8=", "mimeType": "image/png" }); PICTURES + 2
        ] });
        assert_eq!(pictures(&many).len(), PICTURES);
    }

    #[test]
    fn results_read_as_values() {
        let text = json!({ "content": [{ "type": "text", "text": "{\"a\":1}" }] });
        assert_eq!(answer(&text), Ok(json!({ "a": 1 })));
        let plain = json!({ "content": [{ "type": "text", "text": "hi" }] });
        assert_eq!(answer(&plain), Ok(json!("hi")));
        let error = json!({ "content": [{ "type": "text", "text": "no" }], "isError": true });
        assert_eq!(answer(&error), Err("no".to_owned()));
        let structured = json!({ "content": [], "structuredContent": { "b": 2 } });
        assert_eq!(answer(&structured), Ok(json!({ "b": 2 })));
    }
}
