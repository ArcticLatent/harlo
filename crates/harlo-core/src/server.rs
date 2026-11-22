//! Connecting to an OpenCode server and following it.
//!
//! [`follow`] keeps one server connected for as long as its stream is polled.
//! Every (re)connection opens the event stream first, drains it on a task of
//! its own (OpenCode drops slow readers), loads a [`Snapshot`], and only then
//! forwards the events that arrived meanwhile. When the connection breaks it
//! waits a little and starts over. Dropping the stream detaches: the server
//! itself is never stopped, restarted or reconfigured.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::Stream;
use futures_util::future::try_join_all;
use opencode_client::{Event, read_registration, registration_path};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::workspace::{Fetch, Fetched, Snapshot};

#[doc(inline)]
pub use opencode_client::{
    Agent, Answer, Attachment, Choice, Client, Command, Condition, Delivery, Effect, Endpoint,
    Error, Field, FieldKind, FileDiff, FileMatch, McpResource, Mention, Model, PermissionReply,
    Prompt, Rule, SessionContext, Skill, Tokens, message_id,
};

/// The longest wait between reconnection attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(10);

/// Which server to follow.
///
/// Its `Debug` output leaves the password out.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// The shared background service on this computer, found through its registration file.
    Shared,
    /// A server at an explicit address, as given with `--server`.
    Server {
        /// Base URL, such as `http://127.0.0.1:4096`.
        url: String,
        /// The server password, if it has one.
        password: Option<String>,
    },
}

impl std::fmt::Debug for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shared => f.write_str("Shared"),
            Self::Server { url, password } => f
                .debug_struct("Server")
                .field("url", url)
                .field("password", &password.as_ref().map(|_| "…"))
                .finish(),
        }
    }
}

impl Location {
    /// Returns whether the server runs on this computer, so its folders are here too.
    #[must_use]
    pub fn is_local(&self) -> bool {
        match self {
            Self::Shared => true,
            Self::Server { url, .. } => {
                let rest = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
                let host = match rest.strip_prefix('[') {
                    Some(v6) => v6.split(']').next().unwrap_or_default(),
                    None => rest.split([':', '/']).next().unwrap_or_default(),
                };
                matches!(host, "localhost" | "::1") || host.starts_with("127.")
            }
        }
    }

    /// The name the location picker shows.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Shared => "Local".to_owned(),
            Self::Server { url, .. } => url
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .to_owned(),
        }
    }
}

/// A connected server: its client, version and what it holds right now.
#[derive(Debug)]
pub struct Connected {
    /// A client for further calls.
    pub client: Client,
    /// The server's OpenCode version.
    pub version: String,
    /// Projects, worktrees and sessions at the moment of connecting.
    pub snapshot: Snapshot,
}

/// What [`follow`] reports.
#[derive(Debug, Clone)]
pub enum Update {
    /// Trying to reach the server; `attempt` counts from 1 after each loss.
    Connecting {
        /// Which attempt this is.
        attempt: u32,
    },
    /// Connected and loaded. Events that follow apply on top of this snapshot.
    Connected(Arc<Connected>),
    /// A live event.
    Event(Event),
    /// The connection failed or broke; the next attempt starts after `retry_in`.
    Lost {
        /// Why, in words for the user.
        reason: String,
        /// How long until the next attempt.
        retry_in: Duration,
    },
}

/// Follows `location` until the returned stream is dropped.
///
/// The connection runs on a task spawned on the first poll, so the stream
/// must be polled inside a Tokio runtime.
#[must_use]
pub fn follow(location: Location) -> Updates {
    Updates {
        start: Some(location),
        rx: None,
        task: None,
    }
}

/// The stream [`follow`] returns. Dropping it stops the connection task.
#[derive(Debug)]
pub struct Updates {
    start: Option<Location>,
    rx: Option<mpsc::UnboundedReceiver<Update>>,
    task: Option<JoinHandle<()>>,
}

impl Stream for Updates {
    type Item = Update;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Update>> {
        if let Some(location) = self.start.take() {
            let (tx, rx) = mpsc::unbounded_channel();
            self.task = Some(tokio::spawn(run(location, tx)));
            self.rx = Some(rx);
        }
        match self.rx.as_mut() {
            Some(rx) => rx.poll_recv(cx),
            None => Poll::Ready(None),
        }
    }
}

impl Drop for Updates {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Aborts a task when dropped.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The receiver went away, so the connection task should end.
struct Detached;

async fn run(location: Location, tx: mpsc::UnboundedSender<Update>) {
    let mut attempt = 0_u32;
    let mut started_service = false;
    loop {
        attempt += 1;
        if tx.send(Update::Connecting { attempt }).is_err() {
            return;
        }
        let reason = match session(&location, &tx, &mut started_service).await {
            Ok(Ok(reason)) => {
                // It was connected, so the next loss starts the backoff over.
                attempt = 1;
                reason
            }
            Ok(Err(reason)) => reason,
            Err(Detached) => return,
        };
        let retry_in = backoff(attempt);
        if tx.send(Update::Lost { reason, retry_in }).is_err() {
            return;
        }
        tokio::time::sleep(retry_in).await;
    }
}

/// Connects once and forwards events until the connection ends.
///
/// Returns `Ok(Ok(reason))` when a connection ended after succeeding,
/// `Ok(Err(reason))` when it never got going, and `Err(Detached)` when
/// nobody is listening any more.
async fn session(
    location: &Location,
    tx: &mpsc::UnboundedSender<Update>,
    started_service: &mut bool,
) -> Result<Result<String, String>, Detached> {
    let client = match reach(location, started_service).await {
        Ok(client) => client,
        Err(reason) => return Ok(Err(reason)),
    };
    let version = match client.info().await {
        Ok(info) => info.version,
        Err(e) => return Ok(Err(describe(&e))),
    };
    let mut events = match client.events().await {
        Ok(events) => events,
        Err(e) => return Ok(Err(describe(&e))),
    };
    // Drain the stream at once into a channel that never pushes back.
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let _drain = AbortOnDrop(tokio::spawn(async move {
        while let Some(item) = events.next().await {
            if event_tx.send(item).is_err() {
                return;
            }
        }
    }));
    let snapshot = match load(&client).await {
        Ok(snapshot) => snapshot,
        Err(e) => return Ok(Err(describe(&e))),
    };
    let connected = Connected {
        client,
        version,
        snapshot,
    };
    tx.send(Update::Connected(Arc::new(connected)))
        .map_err(|_closed| Detached)?;
    while let Some(item) = event_rx.recv().await {
        match item {
            Ok(event) => tx.send(Update::Event(event)).map_err(|_closed| Detached)?,
            Err(e) => return Ok(Ok(describe(&e))),
        }
    }
    Ok(Ok("the server closed the connection".to_owned()))
}

/// Finds and checks the server for `location`.
async fn reach(location: &Location, started_service: &mut bool) -> Result<Client, String> {
    match location {
        Location::Server { url, password } => {
            Ok(Client::new(Endpoint::new(url.clone(), password.clone())))
        }
        Location::Shared => {
            if let Some(client) = shared().await? {
                return Ok(client);
            }
            // Starting the shared service when none runs is what OpenCode's own
            // clients do; Harlo never stops or restarts one.
            if !*started_service {
                *started_service = true;
                start_service().await?;
                if let Some(client) = shared().await? {
                    return Ok(client);
                }
            }
            Err("OpenCode's background service isn't running".to_owned())
        }
    }
}

/// Returns a client for the registered shared service, if it answers.
async fn shared() -> Result<Option<Client>, String> {
    let path = registration_file();
    let service =
        read_registration(&path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let Some(service) = service else {
        return Ok(None);
    };
    let client = Client::new(service.endpoint);
    Ok(client.info().await.is_ok().then_some(client))
}

fn registration_file() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
    registration_path(state.as_deref(), &home)
}

async fn start_service() -> Result<(), String> {
    let output = tokio::process::Command::new("opencode")
        .args(["service", "start"])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                "OpenCode isn't installed (no `opencode` on PATH)".to_owned()
            }
            _ => format!("couldn't start OpenCode's background service: {e}"),
        })?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "couldn't start OpenCode's background service: {}",
            stderr.trim()
        ))
    }
}

/// Loads everything the sidebar needs.
///
/// # Errors
///
/// Fails when the server can't be reached or answers with an error.
pub async fn load(client: &Client) -> Result<Snapshot, Error> {
    let (projects, sessions, active) =
        tokio::try_join!(client.projects(), client.root_sessions(), client.active())?;
    let git: Vec<&str> = projects
        .iter()
        .filter(|p| p.git)
        .map(|p| p.id.as_str())
        .collect();
    let folders = try_join_all(git.iter().map(|id| client.worktrees(id))).await?;
    let worktrees = git.into_iter().map(str::to_owned).zip(folders).collect();
    // Only a working session can wait for the user.
    let (permissions, forms) = tokio::try_join!(
        try_join_all(active.iter().map(|id| client.permissions(id))),
        try_join_all(active.iter().map(|id| client.forms(id))),
    )?;
    Ok(Snapshot {
        projects,
        worktrees,
        sessions,
        active,
        permissions: permissions.into_iter().flatten().collect(),
        forms: forms.into_iter().flatten().collect(),
    })
}

/// Runs a [`Fetch`] the workspace asked for.
///
/// # Errors
///
/// Fails when the server can't be reached or answers with an error other
/// than "not found" for a session, which means it was deleted.
pub async fn fetch(client: &Client, what: Fetch) -> Result<Fetched, Error> {
    match what {
        Fetch::Session(id) => match client.session(&id).await {
            Ok(session) => Ok(Fetched::Session(id, Some(Box::new(session)))),
            Err(e) if e.http_status() == Some(404) => Ok(Fetched::Session(id, None)),
            Err(e) => Err(e),
        },
        Fetch::Projects => Ok(Fetched::Projects(client.projects().await?)),
        Fetch::Worktrees(project) => {
            let folders = client.worktrees(&project).await?;
            Ok(Fetched::Worktrees(project, folders))
        }
        Fetch::Inbox(id) => {
            let items = client.inbox(&id).await?;
            Ok(Fetched::Inbox(id, items))
        }
        Fetch::Permissions(id) => {
            let waiting = client.permissions(&id).await?;
            Ok(Fetched::Permissions(id, waiting))
        }
        Fetch::Forms(id) => {
            let waiting = client.forms(&id).await?;
            Ok(Fetched::Forms(id, waiting))
        }
    }
}

/// Describes a connection error for the location picker.
fn describe(error: &Error) -> String {
    if error.is_unauthorized() {
        "the server rejected the password".to_owned()
    } else if error.is_transport() {
        "can't reach the server".to_owned()
    } else {
        error.to_string()
    }
}

/// 1 s, 2 s, 4 s, 8 s, then 10 s between attempts.
fn backoff(attempt: u32) -> Duration {
    let secs = 1_u64 << attempt.saturating_sub(1).min(4);
    Duration::from_secs(secs).min(MAX_BACKOFF)
}

/// What a folder offers: models, agents, commands, skills and MCP resources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// Enabled models, in the server's order.
    pub models: Vec<Model>,
    /// The model new sessions use, when one is configured.
    pub default_model: Option<Model>,
    /// Agents a session can run, and subagents a prompt can name.
    pub agents: Vec<Agent>,
    /// Slash commands.
    pub commands: Vec<Command>,
    /// Skills a prompt can load.
    pub skills: Vec<Skill>,
    /// Resources MCP servers offer.
    pub resources: Vec<McpResource>,
}

impl Catalog {
    /// Returns the model `provider_id`/`id`, if it's offered.
    #[must_use]
    pub fn model(&self, provider_id: &str, id: &str) -> Option<&Model> {
        self.models
            .iter()
            .find(|m| m.provider_id == provider_id && m.id == id)
    }

    /// Agents a session can run, in the server's order.
    pub fn primary_agents(&self) -> impl Iterator<Item = &Agent> {
        self.agents.iter().filter(|a| a.primary)
    }
}

/// Loads what `directory` offers, all lists at once.
///
/// MCP resources are optional: when they fail to load (no MCP servers, or
/// one that's down), the list is empty rather than failing the rest.
///
/// # Errors
///
/// Fails when the server can't be reached or a list doesn't decode.
pub async fn catalog(client: &Client, directory: &str) -> Result<Catalog, Error> {
    let mut catalog = load_catalog(client, directory).await?;
    // The first request for a folder can arrive while OpenCode is still
    // starting that folder's instance, and gets an empty agent list. There's
    // always a primary agent once it's up, so empty means "ask again".
    for _ in 0..CATALOG_RETRIES {
        if !catalog.agents.is_empty() {
            break;
        }
        tokio::time::sleep(CATALOG_RETRY_DELAY).await;
        catalog = load_catalog(client, directory).await?;
    }
    Ok(catalog)
}

/// How many times a catalog without agents is loaded again.
///
/// With [`CATALOG_RETRY_DELAY`], about two seconds: a folder's instance
/// starts well within that. A folder that still has no agents keeps the
/// empty list, and the agent picker stays hidden.
const CATALOG_RETRIES: u32 = 10;

/// The wait before loading a catalog without agents again.
const CATALOG_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(200);

async fn load_catalog(client: &Client, directory: &str) -> Result<Catalog, Error> {
    let (models, default_model, agents, commands, skills, resources) = futures_util::join!(
        client.models(directory),
        client.default_model(directory),
        client.agents(directory),
        client.commands(directory),
        client.skills(directory),
        client.mcp_resources(directory),
    );
    Ok(Catalog {
        models: models?,
        default_model: default_model?,
        agents: agents?,
        commands: commands?,
        skills: skills?,
        resources: resources.unwrap_or_default(),
    })
}
