//! Settings → Phone: phone access while Harlo runs, the relay, pairing a
//! phone with a QR code, and the paired phones.
//!
//! Phone access follows the preferences after every update, as Harlo's own
//! tools do: turned on, it starts; turned off, it stops; the window following
//! another server takes the phones along.
//!
//! A server that hosts phone access itself (Harlo's own) runs it instead of
//! this window, so phones keep working while Harlo is closed: Harlo then
//! tells the server how it wants phone access and shows what the server
//! reports. On a server that doesn't, Harlo runs phone access itself.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;
use std::sync::Arc;

use futures_util::stream;
use harlo_core::phone::{self, Phone, Relay, RelayStatus, Settings};
use harlo_core::server::{Client, Location, PhoneRelay, PhoneWanted, ServerPhone};

use harlo_core::layout::Appearance;
use harlo_core::theme::{Look, Mode};

use super::{AVATARS, App, Message};
use crate::task::{Handle, Task};

/// What the theme phones are given is made from: the dark and light themes'
/// IDs, the look, which shows, and the avatar.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ThemeFrom(String, String, Look, Appearance, String);

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
    /// Whether the server hosts phone access itself.
    Hosted(bool),
    /// The server's phone access as it stands, or why it couldn't be read.
    Server(Result<Box<ServerPhone>, String>),
    /// A pairing link from the server, or why there's none.
    ServerPaired(Result<String, String>),
}

/// Who runs phone access for the server Harlo is connected to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    /// Not known yet: Harlo hasn't asked the server.
    Unknown,
    /// This window.
    Window,
    /// The server.
    Server,
}

/// A paired phone, as Settings lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paired {
    /// Its ID.
    pub id: String,
    /// The name it gave when it paired.
    pub name: String,
    /// When it was last connected, in seconds since the Unix epoch.
    pub last_seen: i64,
    /// Whether it takes notifications.
    pub push: bool,
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
    pub devices: Vec<Paired>,
    /// The IDs of the phones connected now.
    pub connected: Vec<String>,
    /// The pairing code shown.
    pub shown: Option<Shown>,
    /// How the last test notification went.
    pub tested: Option<Result<usize, String>>,
    /// Watches the phones and the relay; dropped with phone access.
    watches: Vec<Handle>,
    /// Who runs phone access.
    pub host: Host,
    /// The server's phone access, when the server runs it.
    pub server: Option<ServerPhone>,
    /// What was last asked of the server.
    sent: Option<PhoneWanted>,
    /// The theme phones were last given, and what it was made from.
    theme: Option<(ThemeFrom, phone::theme::Theme)>,
    /// The avatar pick from a phone last taken.
    avatar_taken: Option<(u64, String)>,
    /// Follows the server's phone access while it runs it.
    polling: Option<Handle>,
}

impl PhoneState {
    /// The port phones connect to directly, while listening.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        match (&self.running, &self.server) {
            (Some(running), _) => Some(running.address().port()),
            (None, Some(server)) => server.port,
            (None, None) => None,
        }
    }

    /// Whether phone access is listening, here or on the server.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.port().is_some()
    }

    /// Whether Harlo is on the relay, here or through the server.
    #[must_use]
    pub fn on_relay(&self) -> bool {
        self.relay.is_some()
            || self
                .server
                .as_ref()
                .is_some_and(|s| s.relay != PhoneRelay::Off)
    }
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
            host: Host::Unknown,
            server: None,
            sent: None,
            polling: None,
            theme: None,
            avatar_taken: None,
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
    /// The theme phones draw in: the desktop's, as the phone app reads it.
    /// Made again only when what it's made from changes.
    fn phone_theme(&mut self) -> phone::theme::Theme {
        let look = Look {
            contrast: self.preferences.contrast,
            diff: self.preferences.diff_colors,
        };
        let (dark, light) = (self.theme_for(Mode::Dark), self.theme_for(Mode::Light));
        let from = ThemeFrom(
            dark.id.clone(),
            light.id.clone(),
            look,
            self.preferences.appearance,
            self.preferences.avatar.clone(),
        );
        if let Some((made_from, theme)) = &self.phone.theme
            && *made_from == from
        {
            return theme.clone();
        }
        let appearance = match self.preferences.appearance {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        };
        let theme = phone::theme::theme(dark, light, look, appearance, &self.preferences.avatar);
        self.phone.theme = Some((from, theme.clone()));
        theme
    }

    /// Takes avatar `pick` made on a phone, once.
    fn take_avatar_pick(&mut self, pick: Option<(u64, String)>) -> Task<Message> {
        if pick.is_none() || pick == self.phone.avatar_taken {
            return Task::none();
        }
        self.phone.avatar_taken.clone_from(&pick);
        match pick.and_then(|(_, name)| AVATARS.iter().position(|a| *a == name)) {
            Some(index) => self.pick_avatar(index),
            None => Task::none(),
        }
    }

    /// Starts or stops phone access and the relay as the preferences say,
    /// and keeps it following the window's server.
    pub(super) fn follow_phone(&mut self) -> Task<Message> {
        match self.phone.host {
            Host::Server => return self.follow_server_phone(),
            // Asked on connecting; nothing starts before the answer, so
            // this window and the server don't both listen. Not connected,
            // the window runs it.
            Host::Unknown if matches!(self.status, super::Status::Connected { .. }) => {
                return Task::none();
            }
            Host::Unknown | Host::Window => {}
        }
        let theme = if self.preferences.phone {
            self.phone_theme()
        } else {
            phone::theme::Theme::Null
        };
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
                theme,
                claude: Some(self.claude_program()),
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
        running.set_theme(theme);
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

    /// Asks the newly connected server whether it hosts phone access.
    pub(super) fn ask_phone_host(&mut self, client: Client) -> Task<Message> {
        self.phone.host = Host::Unknown;
        Task::perform(
            async move { client.hosts_phone().await.unwrap_or(false) },
            |hosted| Message::Phone(PhoneEvent::Hosted(hosted)),
        )
    }

    /// The server runs phone access: tells it what's wanted when that
    /// changes, and follows what it reports.
    fn follow_server_phone(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let wanted = PhoneWanted {
            on: self.preferences.phone,
            relay: self.preferences.phone_relay,
            paused: self.preferences.phone_paused,
            port: self.phone.port,
            name: computer_name(),
            hidden: self.workspace.hidden().to_vec(),
            theme: self.phone_theme(),
        };
        let state = &mut self.phone;
        let mut tasks = Vec::new();
        if state.sent.as_ref() != Some(&wanted) {
            state.sent = Some(wanted.clone());
            if !wanted.on {
                state.shown = None;
            }
            let client = client.clone();
            tasks.push(Task::perform(
                async move { client.want_phone(&wanted).await.map_err(|e| e.to_string()) },
                |r| Message::Phone(PhoneEvent::Server(r.map(Box::new))),
            ));
        }
        if state.polling.is_none() {
            let start = state.server.as_ref().map_or(0, |s| s.version);
            let changes = stream::unfold((client, start), |(client, after)| async move {
                loop {
                    match client.phone_state(Some(after)).await {
                        Ok(s) if s.version != after => {
                            let next = s.version;
                            return Some((Ok(s), (client, next)));
                        }
                        Ok(_) => {}
                        Err(e) if e.is_transport() => return None,
                        Err(e) => {
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            return Some((Err(e.to_string()), (client, after)));
                        }
                    }
                }
            });
            let (task, handle) = Task::stream(futures_util::StreamExt::map(changes, |r| {
                Message::Phone(PhoneEvent::Server(r.map(Box::new)))
            }))
            .abortable();
            state.polling = Some(handle.abort_on_drop());
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    /// Shows what the server reports of its phone access, and takes an
    /// avatar picked on a phone.
    fn show_server_phone(&mut self, phone: ServerPhone) -> Task<Message> {
        let pick = phone
            .avatar_pick
            .as_ref()
            .map(|p| (p.serial, p.name.clone()));
        let state = &mut self.phone;
        let before = state.devices.len();
        state.devices = phone
            .devices
            .iter()
            .map(|d| Paired {
                id: d.id.clone(),
                name: d.name.clone(),
                last_seen: d.last_seen,
                push: d.push,
            })
            .collect();
        if state.devices.len() > before {
            state.shown = None;
        }
        state.connected.clone_from(&phone.connected);
        state.error.clone_from(&phone.error);
        state.relay_status = match &phone.relay {
            PhoneRelay::Off => None,
            PhoneRelay::Connecting => Some(RelayStatus::Connecting),
            PhoneRelay::Online(id) => Some(RelayStatus::Online(id.clone())),
            PhoneRelay::Failed(why) => Some(RelayStatus::Failed(why.clone())),
        };
        if phone.port.is_none() {
            state.shown = None;
        }
        state.server = Some(phone);
        self.take_avatar_pick(pick)
    }

    /// Handles something the server-hosted phone access did.
    fn server_phone_event(&mut self, event: PhoneEvent) -> Task<Message> {
        let client = self.client.clone();
        match event {
            PhoneEvent::Server(Ok(phone)) => return self.show_server_phone(*phone),
            PhoneEvent::Server(Err(why)) | PhoneEvent::ServerPaired(Err(why)) => {
                self.phone.error = Some(why);
            }
            PhoneEvent::ServerPaired(Ok(link)) => self.phone.shown = Some(Shown { text: link }),
            PhoneEvent::Pair => {
                if let Some(client) = client {
                    return Task::perform(
                        async move { client.pair_phone().await.map_err(|e| e.to_string()) },
                        |r| Message::Phone(PhoneEvent::ServerPaired(r)),
                    );
                }
            }
            PhoneEvent::Remove(index) => {
                if let (Some(client), Some(device)) = (client, self.phone.devices.get(index)) {
                    let id = device.id.clone();
                    return Task::perform(
                        async move {
                            client.remove_phone(&id).await.map_err(|e| e.to_string())?;
                            client.phone_state(None).await.map_err(|e| e.to_string())
                        },
                        |r| Message::Phone(PhoneEvent::Server(r.map(Box::new))),
                    );
                }
            }
            PhoneEvent::Test => {
                if let Some(client) = client {
                    self.phone.tested = None;
                    return Task::perform(
                        async move { client.test_phone().await.map_err(|e| e.to_string()) },
                        |r| Message::Phone(PhoneEvent::Tested(r)),
                    );
                }
            }
            PhoneEvent::Tested(result) => self.phone.tested = Some(result),
            PhoneEvent::TogglePause => {
                self.preferences.phone_paused = !self.preferences.phone_paused;
                return self.save_preferences();
            }
            // The window's own phone access isn't running.
            _ => {}
        }
        Task::none()
    }

    /// Learns whether the server hosts phone access; when it does, this
    /// window's stops.
    fn phone_hosted(&mut self, hosted: bool) {
        let state = &mut self.phone;
        state.host = if hosted { Host::Server } else { Host::Window };
        state.sent = None;
        state.polling = None;
        state.server = None;
        if hosted {
            // The server runs it now: this window's stops.
            state.running = None;
            state.relay = None;
            state.relay_status = None;
            state.watches.clear();
            state.shown = None;
            state.connected.clear();
            state.following = None;
            state.error = None;
        }
    }

    /// Handles something that happened to phone access.
    pub(super) fn phone_event(&mut self, event: PhoneEvent) -> Task<Message> {
        if let PhoneEvent::Hosted(hosted) = event {
            self.phone_hosted(hosted);
            return Task::none();
        }
        if self.phone.host == Host::Server {
            return self.server_phone_event(event);
        }
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
            PhoneEvent::Changed => {
                let pick = state.running.as_ref().and_then(|p| p.avatar_pick());
                self.refresh_phones();
                return self.take_avatar_pick(pick);
            }
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
            // Only from a server that hosts phone access.
            PhoneEvent::Hosted(_) | PhoneEvent::Server(_) | PhoneEvent::ServerPaired(_) => {}
        }
        Task::none()
    }

    /// Reads the paired and connected phones again.
    fn refresh_phones(&mut self) {
        if self.phone.host == Host::Server {
            return;
        }
        let state = &mut self.phone;
        let paired = |d: harlo_core::phone::Device| Paired {
            push: d.push.is_some(),
            id: d.id,
            name: d.name,
            last_seen: d.last_seen,
        };
        if let Some(running) = &state.running {
            let before = state.devices.len();
            state.devices = running.devices().into_iter().map(paired).collect();
            state.connected = running.connected();
            // A phone just paired with the code shown: the code is spent.
            if state.devices.len() > before {
                state.shown = None;
            }
        } else if let Some(folder) = &state.folder {
            state.devices = phone::saved_devices(folder)
                .into_iter()
                .map(paired)
                .collect();
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
