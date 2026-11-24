//! MCP servers: the ones the config names (`mcp.servers`), those added
//! while the server runs, and those a client hosts over a WebSocket (such
//! as Harlo's own tools).
//!
//! A server connects the first time a folder that names it is listed or
//! runs a turn, and stays connected while its entry is unchanged. Its
//! tools reach agents through Code Mode (`tools.<server>.<tool>`) unless
//! its entry says `codemode: false`; then each is offered on its own as
//! `<server>_<tool>`.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::process::Stdio;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use harlo_engine::codemode::{Direct, Namespace, ToolInfo};
use rmcp::model::CallToolRequestParams;
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

/// How long a tool call may take, unless its entry says.
const EXECUTION: Duration = Duration::from_mins(5);

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
        lock(&self.state).status = Some(status);
    }

    /// Notes it's connected through `peer`.
    fn connected(&self, peer: Peer<RoleClient>) {
        let mut state = lock(&self.state);
        state.peer = Some(peer);
        state.status = Some(Status::Connected);
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

    /// Calls tool `tool` with `args`, for session `session`.
    async fn call(&self, tool: &str, args: Value, session: Option<&str>) -> Result<Value, String> {
        let peer = lock(&self.state)
            .peer
            .clone()
            .ok_or_else(|| format!("The MCP server {} isn't connected.", self.name))?;
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
        answer(&serde_json::to_value(result).unwrap_or_default())
    }
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
fn object_type(schema: &Value) -> String {
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
            tools: lock(&l.state)
                .tools
                .iter()
                .map(|t| ToolInfo {
                    name: t["name"].as_str().unwrap_or_default().to_owned(),
                    description: t["description"].as_str().unwrap_or_default().to_owned(),
                    signature: format!("({}): unknown", object_type(&t["inputSchema"])),
                })
                .collect(),
        })
        .collect()
}

/// The tools `links` offer on their own.
pub(crate) fn direct(links: &[Arc<Link>]) -> Vec<Direct> {
    links
        .iter()
        .filter(|l| !l.codemode && l.status() == Status::Connected)
        .flat_map(|l| {
            lock(&l.state)
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
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Calls tool `tool` of server `namespace` among `links`.
pub(crate) async fn call(
    links: &[Arc<Link>],
    namespace: &str,
    tool: &str,
    args: Value,
    session: Option<&str>,
) -> Option<Result<Value, String>> {
    let link = links.iter().find(|l| l.name == namespace)?;
    Some(link.call(tool, args, session).await)
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

/// Starts server entry `spec`, in `directory` unless it names a `cwd`.
async fn start(
    watch: Watch,
    spec: &Value,
    directory: &str,
    cancel: CancellationToken,
) -> Result<RunningService<RoleClient, Watch>, Status> {
    let failed = |e: &dyn fmt::Display| Status::Failed(e.to_string());
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
                    Status::Failed(said)
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

/// The connections, and what was changed while the server runs.
#[derive(Default)]
pub(crate) struct Mcp {
    /// Configured servers' links, by name and entry.
    links: Mutex<HashMap<String, Arc<Link>>>,
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
        let key = format!("{name}\n{spec}");
        let mut links = lock(&self.mcp.links);
        if let Some(link) = links.get(&key) {
            return Arc::clone(link);
        }
        // Another entry by the same name stops.
        links.retain(|k, l| {
            let same = k.split('\n').next() == Some(name);
            if same {
                l.cancel.cancel();
            }
            !same
        });
        let link = Arc::new(Link::new(name, spec, false));
        links.insert(key, Arc::clone(&link));
        drop(links);
        let (server, spec, directory) = (Arc::clone(self), spec.clone(), directory.to_owned());
        let started = Arc::clone(&link);
        tokio::spawn(async move {
            server.run_link(started, &spec, &directory).await;
        });
        link
    }

    /// Connects `link` and follows it until it ends.
    async fn run_link(self: Arc<Self>, link: Arc<Link>, spec: &Value, directory: &str) {
        let watch = Watch {
            link: Arc::downgrade(&link),
            server: Arc::downgrade(&self),
        };
        let started = tokio::time::timeout(
            link.startup,
            start(watch, spec, directory, link.cancel.clone()),
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
            .map(|(name, spec)| self.mcp_link(directory, name, spec))
            .collect();
        for link in &links {
            let until = tokio::time::Instant::now() + link.startup;
            while link.status() == Status::Pending && tokio::time::Instant::now() < until {
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
        lock(&self.mcp.links).retain(|k, l| {
            let stale = k.split('\n').next() == Some(name) && l.status() != Status::Connected;
            if stale {
                l.cancel.cancel();
            }
            !stale
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
        lock(&self.mcp.links).retain(|k, l| {
            let same = k.split('\n').next() == Some(name);
            if same {
                l.cancel.cancel();
            }
            !same
        });
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
        specs.extend(lock(&self.mcp.links).keys().filter_map(|k| {
            let (name, spec) = k.split_once('\n')?;
            Some((name.to_owned(), serde_json::from_str(spec).ok()?))
        }));
        specs
    }

    /// Connects server `name` again, everywhere, after its sign-in changed.
    pub(crate) fn mcp_signed_in(&self, name: &str) {
        lock(&self.mcp.links).retain(|k, l| {
            let same = k.split('\n').next() == Some(name);
            if same {
                l.cancel.cancel();
            }
            !same
        });
        self.emit("mcp.status.changed", json!({ "server": name }), None, None);
    }

    /// Removes server `name` added while running.
    pub(crate) fn mcp_remove(&self, name: &str) {
        lock(&self.mcp.added).remove(name);
        lock(&self.mcp.links).retain(|k, l| {
            let same = k.split('\n').next() == Some(name);
            if same {
                l.cancel.cancel();
            }
            !same
        });
        self.emit("mcp.status.changed", json!({ "server": name }), None, None);
    }

    /// The resources and templates `directory`'s connected servers offer.
    pub(crate) fn mcp_resources(self: &Arc<Self>, directory: &str) -> Value {
        let _ = self.mcp_servers(directory);
        let off = lock(&self.mcp.off).clone();
        let entries = self.mcp_entries(directory);
        let links: Vec<Arc<Link>> = lock(&self.mcp.links)
            .values()
            .filter(|l| entries.contains_key(&l.name) && !off.contains(&l.name))
            .cloned()
            .collect();
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
            watch.serve_with_ct((Box::pin(sink), Box::pin(stream)), link.cancel.clone()),
        )
        .await
        .map_err(|_late| Status::Failed("It took too long to start.".to_owned()))
        .and_then(|r| r.map_err(|e| Status::Failed(e.to_string())));
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
    fn schemas_read_as_signatures() {
        let schema = json!({
            "type": "object", "required": ["path"],
            "properties": { "path": { "type": "string" }, "lines": { "type": "array", "items": { "type": "integer" } } },
        });
        assert_eq!(object_type(&schema), "{lines?: number[], path: string}");
        assert_eq!(direct_name("my server", "read.file"), "my_server_read_file");
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
