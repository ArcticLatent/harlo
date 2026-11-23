//! The phone: pairing, its encrypted channel, and the bridge to OpenCode.
//!
//! A phone pairs by scanning a QR code ([`Offer`]) and from then on reaches
//! Harlo's sessions over an end-to-end encrypted channel: directly on the
//! network, or through a relay that only forwards sealed frames.
//!
//! On each connection the phone first sends a plain hello with its public key
//! (and, the first time, the pairing code and its name); Harlo answers
//! `ready` or `denied`. Every frame after that is sealed ([`Sealed`]). The
//! phone sends requests (`{ id, op, … }`); Harlo answers them (`{ id, … }`)
//! and sends events (`{ ev, … }`) as sessions change.
//!
//! Harlo speaks this protocol, which the phone app already knows, and
//! translates it to OpenCode: [`Phone`] follows the same server the window
//! does, with its own connection, and every phone's requests become calls to
//! OpenCode's API.

// Rust guideline compliant 2026-10-09

mod conn;
mod crypto;
mod devices;
mod push;
mod relay;
mod requests;
mod rows;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt as _;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

#[doc(inline)]
pub use conn::Link;
#[doc(inline)]
pub use crypto::{Channel, Identity, Sealed, b64, fingerprint, key_bytes, random_id, unb64};
#[doc(inline)]
pub use devices::{Device, Devices, Offer, PushTarget, http_base, relay_id, ws_base};
#[doc(inline)]
pub use push::{Alert, What, payload};
#[doc(inline)]
pub use relay::{Relay, RelayStatus};
#[doc(inline)]
pub use requests::{ALLOW_ALWAYS, ALLOW_ONCE, DENY};
#[doc(inline)]
pub use rows::{PHONE_ROWS, Rows, changes, markdown_html};

use crate::server::{self, Client, Location, Update};
use crate::workspace::{Event, Workspace};

/// How long a pairing code works after it's shown.
const CODE_LIFETIME: Duration = Duration::from_mins(10);

/// Events a slow phone may fall behind by before it reloads instead.
const EVENT_BACKLOG: usize = 4096;

/// The port phones connect to directly when nothing else is chosen.
pub const DEFAULT_PORT: u16 = 7676;

/// Harlo's relay: phones reach Harlo through it from anywhere, and it sends
/// the phone app's notifications.
pub const DEFAULT_RELAY: &str = "https://relay.harlo.run";

/// Returns the relay Harlo uses: [`DEFAULT_RELAY`], unless `HARLO_RELAY`
/// names another (a stand-in in tests).
#[must_use]
pub fn relay_url() -> String {
    std::env::var("HARLO_RELAY")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RELAY.to_owned())
}

/// Returns the port phones connect to directly: [`DEFAULT_PORT`], unless
/// `HARLO_PHONE_PORT` names another; 0 picks a free one, as in tests.
#[must_use]
pub fn port() -> u16 {
    std::env::var("HARLO_PHONE_PORT")
        .ok()
        .and_then(|port| port.trim().parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Returns the folder Harlo keeps its phone key and paired phones in.
#[must_use]
pub fn folder() -> Option<PathBuf> {
    Some(
        dirs::state_dir()
            .or_else(dirs::data_local_dir)?
            .join("harlo")
            .join("phone"),
    )
}

/// Returns where phones on this computer's networks reach Harlo's port:
/// `ws://address:port/ws` for each address that isn't this computer's own
/// loopback, IPv4 first.
#[must_use]
pub fn local_endpoints(port: u16) -> Vec<String> {
    let mut addresses: Vec<std::net::IpAddr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.ip())
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
        .filter(|ip| match ip {
            std::net::IpAddr::V4(v4) => !v4.is_link_local(),
            // Only addresses a phone can be given: not link-local ones.
            std::net::IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) != 0xfe80,
        })
        .collect();
    addresses.sort_by_key(std::net::IpAddr::is_ipv6);
    addresses.dedup();
    addresses
        .into_iter()
        .map(|ip| format!("ws://{}/ws", SocketAddr::new(ip, port)))
        .collect()
}

/// Locks a mutex, carrying on with the data when another thread panicked.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Returns the time in seconds since the Unix epoch.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// What a running [`Phone`] needs.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Where Harlo keeps its phone key and the paired phones.
    pub folder: PathBuf,
    /// This computer's name, as phones show it.
    pub name: String,
    /// The OpenCode server phones see.
    pub location: Location,
    /// Project folders the user removed from the sidebar, which phones don't list.
    pub hidden: Vec<String>,
    /// Where phones connect directly; port 0 picks a free one.
    pub listen: SocketAddr,
}

/// The server phones see, once connected.
pub(crate) struct Server {
    pub(crate) client: Client,
    pub(crate) workspace: Mutex<Workspace>,
    pub(crate) local: bool,
}

/// What every phone connection shares.
pub(crate) struct Hub {
    folder: PathBuf,
    pub(crate) identity: Identity,
    pub(crate) devices: Mutex<Devices>,
    codes: Mutex<Vec<(String, Instant)>>,
    online: Mutex<HashMap<String, usize>>,
    pub(crate) name: String,
    endpoints: Mutex<Vec<String>>,
    /// The relay phones reach Harlo through, as its WebSocket base and
    /// Harlo's ID there, while Harlo is on one.
    relay: Mutex<Option<(String, String)>>,
    /// Where notifications are handed in: the relay's web address.
    push: Mutex<Option<String>>,
    hidden: Vec<String>,
    pub(crate) server: watch::Sender<Option<Arc<Server>>>,
    pub(crate) events: broadcast::Sender<Arc<Event>>,
    /// Bumped when the session list may have changed.
    pub(crate) sessions: watch::Sender<u64>,
    /// Bumped when phones pair, go, connect or disconnect.
    pub(crate) devices_changed: watch::Sender<u64>,
}

impl Hub {
    fn bump(counter: &watch::Sender<u64>) {
        counter.send_modify(|n| *n = n.wrapping_add(1));
    }

    pub(crate) fn bump_devices(&self) {
        Self::bump(&self.devices_changed);
    }

    /// Takes a pairing code: whether it was shown and is still fresh.
    pub(crate) fn take_code(&self, code: &str) -> bool {
        let mut codes = lock(&self.codes);
        codes.retain(|(_, made)| made.elapsed() < CODE_LIFETIME);
        let before = codes.len();
        codes.retain(|(c, _)| c != code);
        codes.len() != before
    }

    pub(crate) fn endpoints(&self) -> Vec<String> {
        lock(&self.endpoints).clone()
    }

    /// Notes a phone's connection; the guard notes its end.
    pub(crate) fn online(self: &Arc<Self>, device: &str) -> Online {
        *lock(&self.online).entry(device.to_owned()).or_default() += 1;
        self.bump_devices();
        Online(Arc::clone(self), device.to_owned())
    }

    pub(crate) fn online_ids(&self) -> Vec<String> {
        lock(&self.online).keys().cloned().collect()
    }

    pub(crate) fn current(&self) -> Option<Arc<Server>> {
        self.server.borrow().clone()
    }

    pub(crate) fn hidden(&self) -> &[String] {
        &self.hidden
    }
}

/// A phone connected; dropping it notes the disconnection.
pub(crate) struct Online(Arc<Hub>, String);

impl Drop for Online {
    fn drop(&mut self) {
        {
            let mut online = lock(&self.0.online);
            if let Some(n) = online.get_mut(&self.1) {
                *n -= 1;
                if *n == 0 {
                    online.remove(&self.1);
                }
            }
        }
        self.0.bump_devices();
    }
}

/// Phone access while it's on: the direct WebSocket and the bridge to OpenCode.
///
/// Dropping it disconnects every phone and stops following the server.
pub struct Phone {
    hub: Arc<Hub>,
    address: SocketAddr,
    follow: Mutex<JoinHandle<()>>,
    listener: JoinHandle<()>,
}

impl std::fmt::Debug for Phone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Phone")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl Drop for Phone {
    fn drop(&mut self) {
        lock(&self.follow).abort();
        self.listener.abort();
    }
}

impl Phone {
    /// Starts phone access: loads or makes Harlo's key, listens for phones
    /// and follows the server.
    ///
    /// Must be called inside a Tokio runtime.
    ///
    /// # Errors
    ///
    /// When the key can't be saved or the address can't be listened on.
    pub async fn start(settings: Settings) -> std::io::Result<Self> {
        let identity = Identity::load_or_create(&settings.folder.join("identity.json"))?;
        let devices = Devices::load(&settings.folder.join("devices.json"));
        let hub = Arc::new(Hub {
            folder: settings.folder,
            identity,
            devices: Mutex::new(devices),
            codes: Mutex::new(Vec::new()),
            online: Mutex::new(HashMap::new()),
            name: settings.name,
            endpoints: Mutex::new(Vec::new()),
            relay: Mutex::new(None),
            push: Mutex::new(None),
            hidden: settings.hidden,
            server: watch::Sender::new(None),
            events: broadcast::Sender::new(EVENT_BACKLOG),
            sessions: watch::Sender::new(0),
            devices_changed: watch::Sender::new(0),
        });
        let listener = tokio::net::TcpListener::bind(settings.listen).await?;
        let address = listener.local_addr()?;
        let router = conn::router(Arc::clone(&hub));
        let listener = tokio::spawn(async move {
            // Serving ends only when the listener fails; phones then can't
            // connect directly until phone access is turned on again.
            let _ = axum::serve(listener, router).await;
        });
        let follow = tokio::spawn(follow(Arc::clone(&hub), settings.location));
        Ok(Self {
            hub,
            address,
            follow: Mutex::new(follow),
            listener,
        })
    }

    /// Returns where phones connect directly.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Follows another server, as the window switched to it.
    ///
    /// Must be called inside a Tokio runtime.
    pub fn follow(&self, location: Location) {
        let mut task = lock(&self.follow);
        task.abort();
        self.hub.server.send_replace(None);
        *task = tokio::spawn(follow(Arc::clone(&self.hub), location));
    }

    /// Sets the addresses a pairing offer lists, such as `ws://192.168.1.4:7676/ws`.
    pub fn set_endpoints(&self, endpoints: Vec<String>) {
        *lock(&self.hub.endpoints) = endpoints;
    }

    /// Returns Harlo's public key, as base64.
    #[must_use]
    pub fn public_key(&self) -> String {
        self.hub.identity.public_b64()
    }

    /// Makes a pairing offer with a new one-time code.
    ///
    /// It names the relay too while Harlo is on one.
    #[must_use]
    pub fn offer(&self) -> Offer {
        let code = random_id(9);
        {
            let mut codes = lock(&self.hub.codes);
            codes.retain(|(_, made)| made.elapsed() < CODE_LIFETIME);
            codes.push((code.clone(), Instant::now()));
        };
        let (relay, relay_id) = lock(&self.hub.relay).clone().unwrap_or_default();
        Offer {
            key: self.public_key(),
            code,
            name: self.hub.name.clone(),
            relay,
            relay_id,
            endpoints: self.hub.endpoints(),
        }
    }

    /// Connects to the relay at `url`, so phones reach Harlo from anywhere
    /// and get notifications; Harlo stays on it until the [`Relay`] is dropped
    /// and [`leave_relay`](Self::leave_relay) is called.
    ///
    /// Must be called inside a Tokio runtime.
    ///
    /// # Errors
    ///
    /// When `url` isn't an address, or the relay's secret can't be saved.
    pub fn relay(&self, url: &str) -> Result<Relay, String> {
        let (Some(ws), Some(http)) = (ws_base(url), http_base(url)) else {
            return Err(format!("“{url}” isn't a relay's address."));
        };
        let secret = relay_secret(&self.hub.folder.join("relay.json"))
            .map_err(|e| format!("Couldn't save the relay's secret: {e}"))?;
        *lock(&self.hub.relay) = Some((ws.clone(), relay_id(&secret)));
        *lock(&self.hub.push) = Some(http);
        Ok(relay::start(Arc::clone(&self.hub), ws, secret))
    }

    /// Stops naming the relay in offers and sending notifications through it.
    pub fn leave_relay(&self) {
        *lock(&self.hub.relay) = None;
        *lock(&self.hub.push) = None;
    }

    /// Sends a test notification to every phone that takes them.
    ///
    /// # Errors
    ///
    /// When Harlo isn't on a relay, or no phone got it.
    pub async fn test_notification(&self) -> Result<usize, String> {
        let base = lock(&self.hub.push)
            .clone()
            .ok_or_else(|| "Notifications go through the relay: turn it on first.".to_owned())?;
        let alert = Alert {
            what: What::Test,
            key: String::new(),
            title: "Test".to_owned(),
            body: "Notifications from Harlo arrive on this phone.".to_owned(),
            badge: 0,
        };
        push::deliver(&self.hub, &base, &alert, true).await
    }

    /// Returns the paired phones.
    #[must_use]
    pub fn devices(&self) -> Vec<Device> {
        lock(&self.hub.devices).list().to_vec()
    }

    /// Returns the IDs of the phones connected now.
    #[must_use]
    pub fn connected(&self) -> Vec<String> {
        lock(&self.hub.online).keys().cloned().collect()
    }

    /// Forgets a phone; a connected one is told and disconnected.
    ///
    /// # Errors
    ///
    /// When the phones can't be saved.
    pub fn remove(&self, id: &str) -> std::io::Result<bool> {
        let removed = lock(&self.hub.devices).remove(id)?;
        if removed {
            self.hub.bump_devices();
        }
        Ok(removed)
    }

    /// Returns a receiver that changes when phones pair, go, connect or disconnect.
    #[must_use]
    pub fn watch(&self) -> watch::Receiver<u64> {
        self.hub.devices_changed.subscribe()
    }

    /// Handles a phone's connection that arrived another way, such as
    /// through a relay, until it ends.
    pub async fn connect(&self, link: Link) {
        conn::connection(link, Arc::clone(&self.hub)).await;
    }
}

/// The secret that proves Harlo owns its relay ID, made the first time.
fn relay_secret(path: &Path) -> std::io::Result<String> {
    let saved: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if let Some(secret) = saved["secret"].as_str().filter(|s| s.len() >= 32) {
        return Ok(secret.to_owned());
    }
    let secret = random_id(32);
    crypto::write_private(
        path,
        &serde_json::json!({ "v": 1, "secret": secret }).to_string(),
    )?;
    Ok(secret)
}

/// Loads the paired phones saved in `folder`, without starting anything.
#[must_use]
pub fn saved_devices(folder: &Path) -> Vec<Device> {
    Devices::load(&folder.join("devices.json")).list().to_vec()
}

/// Follows `location` for the phones, keeping the hub's workspace current.
async fn follow(hub: Arc<Hub>, location: Location) {
    let local = location.is_local();
    let mut updates = server::follow(location);
    while let Some(update) = updates.next().await {
        match update {
            Update::Connected(connected) => {
                let mut workspace = Workspace::new(Vec::new());
                workspace.set_hidden(hub.hidden().to_vec());
                workspace.resync(connected.snapshot.clone());
                hub.server.send_replace(Some(Arc::new(Server {
                    client: connected.client.clone(),
                    workspace: Mutex::new(workspace),
                    local,
                })));
                Hub::bump(&hub.sessions);
            }
            Update::Event(event) => {
                if let Some(server) = hub.current() {
                    let fetches = lock(&server.workspace).apply(&event.kind, event.time);
                    if let (Some(alert), Some(base)) =
                        (alert(&server, &event.kind), lock(&hub.push).clone())
                    {
                        let hub = Arc::clone(&hub);
                        // A notification that doesn't arrive is only missed.
                        tokio::spawn(async move {
                            let _ = push::deliver(&hub, &base, &alert, false).await;
                        });
                    }
                    if !fetches.is_empty() {
                        tokio::spawn(fetch(Arc::clone(&hub), server, fetches));
                    }
                }
                if !quiet(&event.kind) {
                    Hub::bump(&hub.sessions);
                }
                // No phone connected: nobody listens, which is fine.
                let _ = hub.events.send(Arc::new(event));
            }
            Update::Lost { .. } => {
                hub.server.send_replace(None);
                Hub::bump(&hub.sessions);
            }
            Update::Connecting { .. } => {}
        }
    }
}

/// The notification an event makes, if any: a question or an approval
/// asked, or a top-level session's run finished or failed.
fn alert(server: &Server, kind: &crate::workspace::EventKind) -> Option<Alert> {
    use crate::workspace::EventKind as K;
    let (what, key, body) = match kind {
        K::PermissionAsked(p) => {
            let body = match p.resources.first() {
                Some(r) => format!("{}: {r}", p.action),
                None => p.action.clone(),
            };
            (
                What::Ask(requests::permission(p)),
                p.session_id.clone(),
                body,
            )
        }
        K::FormCreated(f) => (
            What::Ask(requests::form(f)),
            f.session_id.clone(),
            f.title.clone(),
        ),
        K::Done {
            session_id,
            outcome,
        } => {
            let what = match outcome.as_str() {
                "succeeded" => What::Done,
                "failed" => What::Failed,
                _ => return None,
            };
            let body = if matches!(what, What::Done) {
                "Finished."
            } else {
                "Stopped with an error."
            };
            (what, session_id.clone(), body.to_owned())
        }
        _ => return None,
    };
    let workspace = lock(&server.workspace);
    let session = workspace.session(&key)?;
    if session.parent_id.is_some() && !matches!(what, What::Ask(_)) {
        // A subagent's run ending is part of its parent's run.
        return None;
    }
    let title = session
        .title
        .clone()
        .unwrap_or_else(|| "New session".to_owned());
    let badge = workspace
        .sidebar(SystemTime::now())
        .iter()
        .flat_map(|p| {
            p.sessions
                .iter()
                .chain(p.worktrees.iter().flat_map(|w| &w.sessions))
        })
        .filter(|s| s.waiting)
        .count();
    Some(Alert {
        what,
        key,
        title,
        body,
        badge,
    })
}

/// Whether an event leaves the session list as it is: streamed text.
fn quiet(kind: &crate::workspace::EventKind) -> bool {
    use crate::workspace::EventKind as K;
    matches!(
        kind,
        K::TextDelta { .. }
            | K::ReasoningDelta { .. }
            | K::CompactionDelta { .. }
            | K::ToolStarted { .. }
            | K::ToolCalled { .. }
    )
}

/// Runs what the workspace asked for to follow an event, and what that asks for.
async fn fetch(hub: Arc<Hub>, server: Arc<Server>, mut todo: Vec<crate::workspace::Fetch>) {
    while let Some(what) = todo.pop() {
        // A failed fetch leaves that part as it was; the next event or
        // reconnection puts it right.
        if let Ok(result) = server::fetch(&server.client, what).await {
            todo.extend(lock(&server.workspace).fetched(result));
            Hub::bump(&hub.sessions);
        }
    }
}
