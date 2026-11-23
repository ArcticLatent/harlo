//! Settings → Phone: phone access while Harlo runs, the relay, pairing a
//! phone with a QR code, and the paired phones.
//!
//! Phone access follows the preferences after every update, as Harlo's own
//! tools do: turned on, it starts; turned off, it stops; the window following
//! another server takes the phones along.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;
use std::sync::Arc;

use futures_util::stream;
use harlo_core::phone::{self, Device, Phone, Relay, RelayStatus, Settings};
use harlo_core::server::Location;

use super::{App, Message};
use crate::task::{Handle, Task};

/// Something that happened to phone access.
#[derive(Debug, Clone)]
pub enum PhoneEvent {
    /// Phone access started, or why it couldn't.
    Started(Result<Arc<Phone>, String>),
    /// Harlo is on the relay, or why it couldn't get on.
    Relayed(Result<Arc<Relay>, String>),
    /// Phones paired, went, connected or disconnected.
    Changed,
    /// The relay connection's status changed.
    Relay(RelayStatus),
    /// Show a new pairing code.
    Pair,
    /// Forget paired phone `n`.
    Remove(usize),
    /// Send a test notification.
    Test,
    /// Hold notifications to phones back, or let them go again.
    TogglePause,
    /// How the test notification went: how many phones got it, or why none did.
    Tested(Result<usize, String>),
}

/// The pairing offer shown: its link, for the QR code and to copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The pairing link, as the QR code carries it.
    pub text: String,
}

/// Phone access's state.
#[derive(Debug)]
pub struct PhoneState {
    /// Where the phone key and paired phones are kept; `None` without a
    /// folder for app data.
    pub folder: Option<PathBuf>,
    /// The port phones connect to directly; 0 picks a free one.
    pub port: u16,
    /// Phone access, while it runs.
    pub running: Option<Arc<Phone>>,
    /// Whether it's starting.
    pub starting: bool,
    /// Why it couldn't start.
    pub error: Option<String>,
    /// The location phone access follows.
    following: Option<Location>,
    /// Harlo on the relay, while it is.
    pub relay: Option<Arc<Relay>>,
    /// Whether Harlo is getting on the relay.
    relaying: bool,
    /// The relay connection's status.
    pub relay_status: Option<RelayStatus>,
    /// The paired phones.
    pub devices: Vec<Device>,
    /// The IDs of the phones connected now.
    pub connected: Vec<String>,
    /// The pairing code shown.
    pub shown: Option<Shown>,
    /// How the last test notification went.
    pub tested: Option<Result<usize, String>>,
    /// Watches the phones and the relay; dropped with phone access.
    watches: Vec<Handle>,
}

impl Default for PhoneState {
    fn default() -> Self {
        Self {
            folder: phone::folder(),
            port: phone::port(),
            running: None,
            starting: false,
            error: None,
            following: None,
            relay: None,
            relaying: false,
            relay_status: None,
            devices: Vec::new(),
            connected: Vec::new(),
            shown: None,
            tested: None,
            watches: Vec::new(),
        }
    }
}

/// A watch's changes as a stream of messages, until it closes.
fn watching<T: Clone + Send + Sync + 'static>(
    receiver: tokio::sync::watch::Receiver<T>,
    message: impl Fn(T) -> Message + Send + Sync + 'static,
) -> Task<Message> {
    let changes = stream::unfold(receiver, |mut rx| async move {
        rx.changed().await.ok()?;
        let value = rx.borrow_and_update().clone();
        Some((value, rx))
    });
    Task::stream(futures_util::StreamExt::map(changes, message))
}

impl App {
    /// Starts or stops phone access and the relay as the preferences say,
    /// and keeps it following the window's server.
    pub(super) fn follow_phone(&mut self) -> Task<Message> {
        let state = &mut self.phone;
        if !self.preferences.phone {
            if state.running.is_some() || state.relay.is_some() {
                state.running = None;
                state.relay = None;
                state.relay_status = None;
                state.watches.clear();
                state.shown = None;
                state.connected.clear();
                state.following = None;
            }
            return Task::none();
        }
        let Some(running) = state.running.clone() else {
            if state.starting || state.error.is_some() {
                return Task::none();
            }
            let Some(folder) = state.folder.clone() else {
                state.error = Some("This system has no folder for app data.".to_owned());
                return Task::none();
            };
            state.starting = true;
            state.following = Some(self.location.clone());
            let settings = Settings {
                folder,
                name: computer_name(),
                location: self.location.clone(),
                hidden: self.workspace.hidden().to_vec(),
                listen: std::net::SocketAddr::from(([0, 0, 0, 0], state.port)),
                paused: self.preferences.phone_paused,
            };
            return Task::perform(
                async move {
                    let phone = Phone::start(settings)
                        .await
                        .map_err(|e| format!("Couldn't listen for phones: {e}"))?;
                    phone.set_endpoints(phone::local_endpoints(phone.address().port()));
                    Ok(Arc::new(phone))
                },
                |result| Message::Phone(PhoneEvent::Started(result)),
            );
        };
        let mut tasks = Vec::new();
        if state.following.as_ref() != Some(&self.location) {
            state.following = Some(self.location.clone());
            let (phone, location) = (Arc::clone(&running), self.location.clone());
            tasks.push(Task::future(async move { phone.follow(location) }).discard());
        }
        let wants_relay = self.preferences.phone_relay;
        if wants_relay && state.relay.is_none() && !state.relaying {
            state.relaying = true;
            state.relay_status = Some(RelayStatus::Connecting);
            let phone = Arc::clone(&running);
            tasks.push(Task::perform(
                async move { phone.relay(&phone::relay_url()).map(Arc::new) },
                |result| Message::Phone(PhoneEvent::Relayed(result)),
            ));
        } else if !wants_relay && (state.relay.is_some() || state.relay_status.is_some()) {
            state.relay = None;
            state.relay_status = None;
            running.leave_relay();
        }
        Task::batch(tasks)
    }

    /// Handles something that happened to phone access.
    pub(super) fn phone_event(&mut self, event: PhoneEvent) -> Task<Message> {
        let state = &mut self.phone;
        match event {
            PhoneEvent::Started(result) => {
                state.starting = false;
                match result {
                    // Turned off while it started: it stops again.
                    Ok(_) if !self.preferences.phone => {}
                    Ok(running) => {
                        state.watches.clear();
                        let (task, handle) =
                            watching(running.watch(), |_| Message::Phone(PhoneEvent::Changed))
                                .abortable();
                        state.watches.push(handle.abort_on_drop());
                        state.running = Some(running);
                        self.refresh_phones();
                        return task;
                    }
                    Err(why) => state.error = Some(why),
                }
            }
            PhoneEvent::Relayed(result) => {
                state.relaying = false;
                match result {
                    Ok(_) if !self.preferences.phone_relay || state.running.is_none() => {
                        if let Some(running) = &state.running {
                            running.leave_relay();
                        }
                    }
                    Ok(relay) => {
                        state.relay_status = Some(relay.status().borrow().clone());
                        let (task, handle) =
                            watching(relay.status(), |s| Message::Phone(PhoneEvent::Relay(s)))
                                .abortable();
                        state.watches.push(handle.abort_on_drop());
                        state.relay = Some(relay);
                        return task;
                    }
                    Err(why) => state.relay_status = Some(RelayStatus::Failed(why)),
                }
            }
            PhoneEvent::Relay(status) => {
                if state.relay.is_some() {
                    state.relay_status = Some(status);
                }
            }
            PhoneEvent::Changed => self.refresh_phones(),
            PhoneEvent::Pair => {
                if let Some(running) = &state.running {
                    state.shown = Some(Shown {
                        text: running.offer().link(),
                    });
                }
            }
            PhoneEvent::Remove(index) => {
                if let (Some(running), Some(device)) = (&state.running, state.devices.get(index)) {
                    // A failed save leaves the phone listed, which says so.
                    let _ = running.remove(&device.id);
                    self.refresh_phones();
                }
            }
            PhoneEvent::Test => {
                if let Some(running) = state.running.clone() {
                    state.tested = None;
                    return Task::perform(async move { running.test_notification().await }, |r| {
                        Message::Phone(PhoneEvent::Tested(r))
                    });
                }
            }
            PhoneEvent::Tested(result) => state.tested = Some(result),
            PhoneEvent::TogglePause => {
                self.preferences.phone_paused = !self.preferences.phone_paused;
                if let Some(running) = &self.phone.running {
                    running.set_paused(self.preferences.phone_paused);
                }
                return self.save_preferences();
            }
        }
        Task::none()
    }

    /// Reads the paired and connected phones again.
    fn refresh_phones(&mut self) {
        let state = &mut self.phone;
        if let Some(running) = &state.running {
            let before = state.devices.len();
            state.devices = running.devices();
            state.connected = running.connected();
            // A phone just paired with the code shown: the code is spent.
            if state.devices.len() > before {
                state.shown = None;
            }
        } else if let Some(folder) = &state.folder {
            state.devices = phone::saved_devices(folder);
        }
    }

    /// Turns phone access on again after it failed to start.
    pub(super) fn retry_phone(&mut self) {
        self.phone.error = None;
    }
}

/// This computer's name, as phones show it.
fn computer_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "Harlo".to_owned())
}
