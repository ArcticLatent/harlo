//! Phone access hosted by the server, so phones keep following sessions and
//! answering approvals while no desktop is open.
//!
//! The server runs the same phone bridge the desktop used to run, pointed
//! at its own API. The desktop says how it wants phone access
//! ([`PhoneWanted`]); the server keeps that in its data folder and starts
//! phone access again with it after a restart. The phone's key and the
//! paired phones stay in the folder they were always kept in, so phones
//! paired with the desktop keep working.

// Rust guideline compliant 2026-10-09

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use harlo_client::{AvatarPick, PhoneDevice, PhoneRelay, PhoneState, PhoneWanted};
use harlo_core::phone::{self, Phone, Relay, RelayStatus, Settings};
use harlo_core::server::Location;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// How long a caller waiting for a change waits before getting the state
/// unchanged.
const LONG_POLL: Duration = Duration::from_secs(25);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Phone access as the server runs it.
pub struct PhoneHost {
    /// Where the key and paired phones are kept.
    folder: Option<PathBuf>,
    /// Where the wanted state is kept.
    saved: PathBuf,
    /// This server, as the phone bridge reaches it.
    me: Mutex<Option<Location>>,
    inner: tokio::sync::Mutex<Inner>,
    /// Bumped on every change.
    version: watch::Sender<u64>,
    /// The latest state, for readers that don't wait on `inner`.
    snapshot: Mutex<PhoneState>,
}

#[derive(Default)]
struct Inner {
    wanted: Option<PhoneWanted>,
    running: Option<Arc<Phone>>,
    relay: Option<Relay>,
    error: Option<String>,
    /// Follows the phones and the relay, bumping the version.
    watch: Option<JoinHandle<()>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(w) = self.watch.take() {
            w.abort();
        }
    }
}

impl std::fmt::Debug for PhoneHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhoneHost").finish_non_exhaustive()
    }
}

impl PhoneHost {
    /// Phone access kept in `folder` (the phone's key and paired phones),
    /// with the wanted state saved at `saved`. Nothing starts until
    /// [`attach`](Self::attach).
    #[must_use]
    pub fn new(folder: Option<PathBuf>, saved: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            folder,
            saved,
            me: Mutex::new(None),
            inner: tokio::sync::Mutex::new(Inner::default()),
            version: watch::Sender::new(0),
            snapshot: Mutex::new(PhoneState {
                version: 0,
                wanted: None,
                port: None,
                error: None,
                relay: PhoneRelay::Off,
                devices: Vec::new(),
                connected: Vec::new(),
                avatar_pick: None,
            }),
        })
    }

    /// Points phone access at this server, now listening at `url` with
    /// `password`, and starts it as last wanted.
    pub async fn attach(self: &Arc<Self>, url: String, password: Option<String>) {
        *lock(&self.me) = Some(Location::Server { url, password });
        let saved = std::fs::read_to_string(&self.saved)
            .ok()
            .and_then(|t| serde_json::from_str::<PhoneWanted>(&t).ok());
        if let Some(wanted) = saved {
            self.apply(wanted, false).await;
        }
    }

    /// The state now.
    #[must_use]
    pub fn state(&self) -> PhoneState {
        lock(&self.snapshot).clone()
    }

    /// The state once its version is past `after`, or after a while.
    pub async fn state_after(&self, after: u64) -> PhoneState {
        let mut rx = self.version.subscribe();
        let _ = tokio::time::timeout(LONG_POLL, rx.wait_for(|v| *v > after)).await;
        self.state()
    }

    /// Runs phone access as `wanted` says, and keeps that for next time.
    pub async fn want(self: &Arc<Self>, wanted: PhoneWanted) -> PhoneState {
        self.apply(wanted, true).await;
        self.state()
    }

    async fn apply(self: &Arc<Self>, wanted: PhoneWanted, save: bool) {
        if save && let Ok(text) = serde_json::to_string_pretty(&wanted) {
            if let Some(dir) = self.saved.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(&self.saved, text) {
                tracing::warn!(name: "phone.save.failed", error = %e, "couldn't keep the phone settings");
            }
        }
        let mut inner = self.inner.lock().await;
        let before = inner.wanted.replace(wanted.clone());
        if !wanted.on {
            inner.relay = None;
            inner.running = None;
            inner.error = None;
            if let Some(w) = inner.watch.take() {
                w.abort();
            }
            drop(inner);
            self.refresh().await;
            return;
        }
        // A new port, name or hidden list needs a fresh start.
        let restart = before.as_ref().is_none_or(|b| {
            !b.on || b.port != wanted.port || b.name != wanted.name || b.hidden != wanted.hidden
        });
        if restart || inner.running.is_none() {
            inner.relay = None;
            inner.running = None;
            inner.error = None;
            match self.start(&wanted).await {
                Ok(phone) => inner.running = Some(Arc::new(phone)),
                Err(why) => inner.error = Some(why),
            }
        }
        if let Some(running) = inner.running.clone() {
            running.set_paused(wanted.paused);
            running.set_theme(wanted.theme.clone());
            if wanted.relay && inner.relay.is_none() {
                match running.relay(&phone::relay_url()) {
                    Ok(relay) => inner.relay = Some(relay),
                    Err(why) => inner.error = Some(why),
                }
            } else if !wanted.relay && inner.relay.is_some() {
                inner.relay = None;
                running.leave_relay();
            }
            if let Some(w) = inner.watch.take() {
                w.abort();
            }
            inner.watch = Some(self.follow(&running, inner.relay.as_ref()));
        }
        drop(inner);
        self.refresh().await;
    }

    async fn start(&self, wanted: &PhoneWanted) -> Result<Phone, String> {
        let folder = self
            .folder
            .clone()
            .ok_or_else(|| "This system has no folder for app data.".to_owned())?;
        let location = lock(&self.me)
            .clone()
            .ok_or_else(|| "The server isn't listening yet.".to_owned())?;
        let phone = Phone::start(Settings {
            folder,
            name: wanted.name.clone(),
            location,
            hidden: wanted.hidden.clone(),
            listen: SocketAddr::from(([0, 0, 0, 0], wanted.port)),
            paused: wanted.paused,
            theme: wanted.theme.clone(),
        })
        .await
        .map_err(|e| format!("Couldn't listen for phones: {e}"))?;
        phone.set_endpoints(phone::local_endpoints(phone.address().port()));
        Ok(phone)
    }

    /// Bumps the state whenever phones or the relay change.
    fn follow(self: &Arc<Self>, phone: &Phone, relay: Option<&Relay>) -> JoinHandle<()> {
        let mut devices = phone.watch();
        let mut status = relay.map(Relay::status);
        let host = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                let relay_changed = async {
                    match status.as_mut() {
                        Some(s) => s.changed().await.is_ok(),
                        None => std::future::pending().await,
                    }
                };
                let alive = tokio::select! {
                    r = devices.changed() => r.is_ok(),
                    r = relay_changed => r,
                };
                if !alive {
                    break;
                }
                host.refresh().await;
            }
        })
    }

    /// Rebuilds the state from what runs, and bumps its version.
    async fn refresh(&self) {
        let inner = self.inner.lock().await;
        let avatar_pick = inner
            .running
            .as_ref()
            .and_then(|p| p.avatar_pick())
            .map(|(serial, name)| AvatarPick { serial, name });
        let (port, devices, connected) = match &inner.running {
            Some(p) => (Some(p.address().port()), p.devices(), p.connected()),
            None => (
                None,
                self.folder
                    .as_deref()
                    .map(phone::saved_devices)
                    .unwrap_or_default(),
                Vec::new(),
            ),
        };
        let relay = match inner.relay.as_ref().map(|r| r.status().borrow().clone()) {
            None => PhoneRelay::Off,
            Some(RelayStatus::Connecting) => PhoneRelay::Connecting,
            Some(RelayStatus::Online(id)) => PhoneRelay::Online(id),
            Some(RelayStatus::Failed(why)) => PhoneRelay::Failed(why),
        };
        let mut version = 0;
        self.version.send_modify(|v| {
            *v += 1;
            version = *v;
        });
        *lock(&self.snapshot) = PhoneState {
            version,
            wanted: inner.wanted.clone(),
            port,
            error: inner.error.clone(),
            relay,
            devices: devices
                .into_iter()
                .map(|d| PhoneDevice {
                    id: d.id,
                    name: d.name,
                    last_seen: d.last_seen,
                    push: d.push.is_some(),
                })
                .collect(),
            connected,
            avatar_pick,
        };
    }

    /// A new pairing link.
    ///
    /// # Errors
    ///
    /// When phone access isn't running.
    pub async fn pair(&self) -> Result<String, String> {
        let inner = self.inner.lock().await;
        let running = inner.running.as_ref().ok_or_else(not_running)?;
        Ok(running.offer().link())
    }

    /// Forgets the phone with ID `id`.
    ///
    /// # Errors
    ///
    /// When phone access isn't running or the phones can't be saved.
    pub async fn remove(&self, id: &str) -> Result<bool, String> {
        let removed = {
            let inner = self.inner.lock().await;
            let running = inner.running.as_ref().ok_or_else(not_running)?;
            running
                .remove(id)
                .map_err(|e| format!("Couldn't save the phones: {e}"))?
        };
        self.refresh().await;
        Ok(removed)
    }

    /// Sends a test notification: how many phones got it.
    ///
    /// # Errors
    ///
    /// When phone access isn't running, or none got it.
    pub async fn test(&self) -> Result<usize, String> {
        let running = self
            .inner
            .lock()
            .await
            .running
            .clone()
            .ok_or_else(not_running)?;
        running.test_notification().await
    }
}

fn not_running() -> String {
    "Phone access isn't on.".to_owned()
}
