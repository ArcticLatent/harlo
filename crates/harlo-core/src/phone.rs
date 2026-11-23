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
    pub(crate) identity: Identity,
    pub(crate) devices: Mutex<Devices>,
    codes: Mutex<Vec<(String, Instant)>>,
    online: Mutex<HashMap<String, usize>>,
    pub(crate) name: String,
    endpoints: Mutex<Vec<String>>,
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
            identity,
            devices: Mutex::new(devices),
            codes: Mutex::new(Vec::new()),
            online: Mutex::new(HashMap::new()),
            name: settings.name,
            endpoints: Mutex::new(Vec::new()),
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
    /// `relay` is the relay's WebSocket base and Harlo's ID there, when
    /// phones can reach Harlo through one.
    #[must_use]
    pub fn offer(&self, relay: Option<(String, String)>) -> Offer {
        let code = random_id(9);
        {
            let mut codes = lock(&self.hub.codes);
            codes.retain(|(_, made)| made.elapsed() < CODE_LIFETIME);
            codes.push((code.clone(), Instant::now()));
        };
        let (relay, relay_id) = relay.unwrap_or_default();
        Offer {
            key: self.public_key(),
            code,
            name: self.hub.name.clone(),
            relay,
            relay_id,
            endpoints: self.hub.endpoints(),
        }
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
