//! `Phone` (QML singleton): Settings → Devices and "Pair a device". Runs
//! the phone server while phone access is on, makes pairing links and QR
//! codes, and lists and removes paired devices.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Phone access is on (saved).
        #[qproperty(bool, enabled)]
        /// The server is listening.
        #[qproperty(bool, running)]
        #[qproperty(i32, port)]
        #[qproperty(QString, error)]
        /// Bumped when devices change.
        #[qproperty(i32, revision)]
        /// harlo's relay is on (saved).
        #[qproperty(bool, relay_on, cxx_name = "relayOn")]
        /// "off", "connecting", "online" or "failed" (see relayError).
        #[qproperty(QString, relay_state, cxx_name = "relayState")]
        #[qproperty(QString, relay_error, cxx_name = "relayError")]
        /// harlo's relay, as people read it ("relay.harlo.run").
        #[qproperty(QString, relay_address, cxx_name = "relayAddress")]
        /// Tailscale Funnel: on (saved), whether tailscale is installed, and
        /// "off", "starting", "approval" (funnelUrl is where to allow it),
        /// "online" (funnelUrl is the public address) or "failed".
        #[qproperty(bool, funnel_on, cxx_name = "funnelOn")]
        #[qproperty(bool, funnel_available, cxx_name = "funnelAvailable")]
        #[qproperty(QString, funnel_state, cxx_name = "funnelState")]
        #[qproperty(QString, funnel_url, cxx_name = "funnelUrl")]
        #[qproperty(QString, funnel_error, cxx_name = "funnelError")]
        /// Opening the firewall (the password dialog is up), why it didn't,
        /// and a count bumped when it may have changed.
        #[qproperty(bool, firewall_busy, cxx_name = "firewallBusy")]
        #[qproperty(QString, firewall_error, cxx_name = "firewallError")]
        #[qproperty(i32, firewall_revision, cxx_name = "firewallRevision")]
        /// How the test notification went ("" before one; "sending").
        #[qproperty(QString, push_note, cxx_name = "pushNote")]
        /// Notifications to the phones are paused (the rail's phone button):
        /// pairings stay, nothing is pushed.
        #[qproperty(bool, paused)]
        type Phone = super::PhoneRust;
    }

    impl cxx_qt::Threading for Phone {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Starts the server if phone access is on (at startup).
        #[qinvokable]
        fn autostart(self: Pin<&mut Phone>);
        #[qinvokable]
        fn set_access(self: Pin<&mut Phone>, on: bool);
        #[qinvokable]
        fn set_access_port(self: Pin<&mut Phone>, port: i32);
        /// This computer's addresses: `[{ address, label }]`, Tailscale first.
        #[qinvokable]
        fn addresses_json(self: &Phone) -> QString;
        /// A pairing link for `address` ("relay": through the relay;
        /// "funnel": Tailscale Funnel) with a new one-time code: `{ url, qr
        /// (data URL), minutes, fingerprint, name }`.
        #[qinvokable]
        fn pairing_json(self: &Phone, address: &QString) -> QString;
        /// `[{ id, name, pairedAt, lastSeen }]`
        #[qinvokable]
        fn devices_json(self: &Phone) -> QString;
        #[qinvokable]
        fn remove_device(self: Pin<&mut Phone>, id: &QString);
        /// Turns phones' way in through harlo's relay on or off (saved).
        #[qinvokable]
        fn use_relay(self: Pin<&mut Phone>, on: bool);
        #[qinvokable]
        fn set_funnel(self: Pin<&mut Phone>, on: bool);
        /// Whether this computer's firewall lets phones on `address`'s
        /// network in: `{ firewall ("ufw", "firewalld" or "" for none),
        /// allowed (true, false, or null: can't tell), network, command (to
        /// type), canAsk (harlo can open it itself), hidden }`.
        #[qinvokable]
        fn firewall_json(self: &Phone, address: &QString) -> QString;
        /// Opens the port to `address`'s network (the system asks for the
        /// password).
        #[qinvokable]
        fn allow_firewall(self: Pin<&mut Phone>, address: &QString);
        /// "It connects already": stops showing the firewall note for
        /// `address`'s network where harlo can't tell.
        #[qinvokable]
        fn hide_firewall_note(self: Pin<&mut Phone>, address: &QString);
        /// The addresses a paired phone tries that the firewall blocks
        /// (while phone access runs): `[{ address, label, tailscale,
        /// network, firewall, canAsk, command }]`, not those put off.
        #[qinvokable]
        fn blocked_json(self: &Phone) -> QString;
        /// The iOS app's notifications: `{ relay, error }` (the relay they
        /// go through; why the last one didn't go).
        #[qinvokable]
        fn push_json(self: &Phone) -> QString;
        /// Sends a test notification to every phone that takes them (see
        /// pushNote).
        #[qinvokable]
        fn send_test_push(self: Pin<&mut Phone>);
        /// Pauses or resumes notifications to the phones (remembered).
        #[qinvokable]
        fn set_notifications_paused(self: Pin<&mut Phone>, paused: bool);
        /// Phones are paired (the rail's button pauses them then).
        #[qinvokable]
        fn has_devices(self: &Phone) -> bool;
    }
}

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::firewall;
use harlo_core::phone::{self, Devices, Identity, Offer, RelaySettings};
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::{phone_funnel, phone_relay};
use crate::phone_server::{self, Server, Shared};
use qobject::Phone;

pub const DEFAULT_PORT: i32 = 7676;

pub struct PhoneRust {
    enabled: bool,
    running: bool,
    port: i32,
    error: QString,
    revision: i32,
    relay_on: bool,
    relay_state: QString,
    relay_error: QString,
    relay_address: QString,
    funnel_on: bool,
    funnel_available: bool,
    funnel_state: QString,
    funnel_url: QString,
    funnel_error: QString,
    funnel: Option<phone_funnel::Funnel>,
    funnel_generation: u64,
    firewall_busy: bool,
    firewall_error: QString,
    firewall_revision: i32,
    push_note: QString,
    paused: bool,
    shared: Option<Arc<Shared>>,
    server: Option<Server>,
    relay: Option<phone_relay::Client>,
    /// Which relay client is current (late news from an old one is ignored).
    relay_generation: u64,
}

impl Default for PhoneRust {
    fn default() -> Self {
        Self {
            enabled: false,
            running: false,
            port: DEFAULT_PORT,
            error: QString::default(),
            revision: 0,
            relay_on: false,
            relay_state: QString::from("off"),
            relay_error: QString::default(),
            relay_address: QString::from(phone::relay_url().split("://").last().unwrap_or_default().trim_end_matches('/')),
            funnel_on: false,
            funnel_available: false,
            funnel_state: QString::from("off"),
            funnel_url: QString::default(),
            funnel_error: QString::default(),
            funnel: None,
            funnel_generation: 0,
            firewall_busy: false,
            firewall_error: QString::default(),
            firewall_revision: 0,
            push_note: QString::default(),
            paused: false,
            shared: None,
            server: None,
            relay: None,
            relay_generation: 0,
        }
    }
}

fn relay_path() -> std::path::PathBuf {
    phone::dir().join("relay.json")
}

fn db() -> Option<harlo_core::db::Db> {
    harlo_core::db::Db::open(&harlo_core::db::default_path()).ok()
}

fn setting(key: &str) -> String {
    db().and_then(|d| d.get(&format!("settings.{key}")).ok().flatten()).unwrap_or_default()
}

fn set_setting(key: &str, value: &str) {
    if let Some(d) = db() {
        let _ = d.set(&format!("settings.{key}"), value);
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_owned()).ok().filter(|h| !h.is_empty()).unwrap_or_else(|| "harlo".to_owned())
}

/// IPv4 addresses phones could reach, with their labels: Tailscale first,
/// then the LAN.
fn addresses() -> Vec<(firewall::Network, String)> {
    let out = std::process::Command::new("ip").args(["-o", "-4", "addr", "show"]).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let mut list: Vec<(firewall::Network, String)> = text
        .lines()
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            let iface = *parts.get(1)?;
            let (addr, prefix) = parts.get(3)?.split_once('/')?;
            let address: std::net::Ipv4Addr = addr.parse().ok()?;
            if iface == "lo" || address.is_loopback() {
                return None;
            }
            // Tailscale: its interface, or its 100.64.0.0/10 range.
            let tailscale = iface.starts_with("tailscale") || firewall::is_tailscale(address);
            let label = if tailscale { format!("Tailscale ({iface})") } else { format!("Local network ({iface})") };
            Some((firewall::Network { address, prefix: prefix.parse().ok()?, interface: iface.to_owned() }, label))
        })
        .collect();
    list.sort_by_key(|(_, l)| !l.starts_with("Tailscale"));
    list
}

/// harlo's own server at each address (`ws://<address>:<port>/ws`).
pub fn direct_endpoints(port: u16) -> Vec<String> {
    addresses().into_iter().map(|(n, _)| format!("ws://{}:{port}/ws", n.address)).collect()
}

fn network_of(address: &str) -> Option<firewall::Network> {
    addresses().into_iter().map(|(n, _)| n).find(|n| n.address.to_string() == address)
}

fn qr_data_url(text: &str) -> String {
    use qrcode::render::svg;
    let Ok(code) = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::M) else { return String::new() };
    let image = code.render::<svg::Color>().min_dimensions(260, 260).quiet_zone(true).dark_color(svg::Color("#111111")).light_color(svg::Color("#ffffff")).build();
    format!("data:image/svg+xml;base64,{}", phone::b64(image.as_bytes()))
}

impl Phone {
    fn shared(mut self: Pin<&mut Self>) -> Option<Arc<Shared>> {
        if let Some(s) = &self.shared {
            return Some(s.clone());
        }
        let dir = phone::dir();
        let identity = match Identity::load_or_create(&dir.join("identity.json")) {
            Ok(i) => i,
            Err(e) => {
                self.as_mut().set_error(QString::from(&format!("{e:#}")));
                return None;
            }
        };
        let shared = Arc::new(Shared {
            identity,
            devices: Mutex::new(Devices::load(&dir.join("devices.json"))),
            codes: Mutex::new(HashMap::new()),
            devices_changed: watch::channel(0).0,
            name: hostname(),
            port: Default::default(),
            remote_endpoints: Default::default(),
            online: Default::default(),
        });
        crate::phone_push::attach(&shared);
        // Devices paired or removed: the settings list follows.
        let mut changed = shared.devices_changed.subscribe();
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            while changed.changed().await.is_ok() {
                if thread.queue(|mut this| { let n = this.revision + 1; this.as_mut().set_revision(n); }).is_err() {
                    break;
                }
            }
        });
        self.as_mut().rust_mut().shared = Some(shared.clone());
        Some(shared)
    }

    pub fn autostart(mut self: Pin<&mut Self>) {
        let paused = setting("phone_paused") == "1";
        crate::phone_push::set_paused(paused);
        self.as_mut().set_paused(paused);
        let port = setting("phone_port").parse::<i32>().ok().filter(|p| (1..=65535).contains(p)).unwrap_or(DEFAULT_PORT);
        self.as_mut().set_port(port);
        let on = setting("phone_enabled") == "1";
        self.as_mut().set_enabled(on);
        let relay = RelaySettings::load(&relay_path());
        self.as_mut().set_relay_on(relay.on);
        self.as_mut().set_funnel_on(setting("phone_funnel") == "1");
        self.as_mut().set_funnel_available(phone_funnel::program().is_some());
        if on {
            self.as_mut().restart();
        }
        self.as_mut().update_relay();
        self.update_funnel();
    }

    pub fn set_access(mut self: Pin<&mut Self>, on: bool) {
        set_setting("phone_enabled", if on { "1" } else { "0" });
        self.as_mut().set_enabled(on);
        if on {
            self.as_mut().restart();
        } else {
            self.as_mut().rust_mut().server = None;
            self.as_mut().set_running(false);
            self.as_mut().set_error(QString::default());
        }
        self.as_mut().update_relay();
        self.update_funnel();
    }

    pub fn set_funnel(mut self: Pin<&mut Self>, on: bool) {
        set_setting("phone_funnel", if on { "1" } else { "0" });
        self.as_mut().set_funnel_on(on);
        self.as_mut().set_funnel_available(phone_funnel::program().is_some());
        self.update_funnel();
    }

    /// Funnel's and the relay's endpoints while they're online, for phones
    /// (see `Shared::endpoints`).
    fn share_remote_endpoints(&self) {
        let Some(shared) = &self.shared else { return };
        let mut list = Vec::new();
        let funnel = self.funnel_url.to_string();
        if self.funnel_state.to_string() == "online" && !funnel.is_empty() {
            list.push(format!("wss://{}/ws", funnel.trim_start_matches("https://").trim_end_matches('/')));
        }
        if self.relay_state.to_string() == "online" {
            let relay = RelaySettings::load(&relay_path());
            if let Some(ws) = relay.ws_base() {
                list.push(format!("{}/ws?id={}", ws.trim_end_matches('/'), harlo_relay::relay_id(&relay.secret)));
            }
        }
        *shared.remote_endpoints.lock().unwrap() = list;
    }

    /// Runs Tailscale Funnel while phone access and Funnel are on.
    fn update_funnel(mut self: Pin<&mut Self>) {
        let program = phone_funnel::program().filter(|_| self.enabled && self.funnel_on);
        let Some(program) = program else {
            self.as_mut().rust_mut().funnel = None;
            self.as_mut().set_funnel_state(QString::from("off"));
            self.as_mut().set_funnel_url(QString::default());
            let error = if self.funnel_on && self.enabled { "Tailscale isn't installed." } else { "" };
            self.as_mut().set_funnel_error(QString::from(error));
            return;
        };
        if self.funnel.is_some() {
            return;
        }
        let generation = self.funnel_generation + 1;
        self.as_mut().rust_mut().funnel_generation = generation;
        let thread = self.qt_thread();
        let status = move |s: phone_funnel::Status| {
            let _ = thread.queue(move |mut this| {
                if this.funnel_generation != generation || this.funnel.is_none() {
                    return;
                }
                let (state, url, error) = match s {
                    phone_funnel::Status::Starting => ("starting", String::new(), String::new()),
                    phone_funnel::Status::NeedsApproval(u) => ("approval", u, String::new()),
                    phone_funnel::Status::Online(u) => ("online", u, String::new()),
                    phone_funnel::Status::Failed(e) => ("failed", String::new(), e),
                };
                // The state last: what reacts to it reads the rest.
                this.as_mut().set_funnel_url(QString::from(&url));
                this.as_mut().set_funnel_error(QString::from(&error));
                this.as_mut().set_funnel_state(QString::from(state));
                this.share_remote_endpoints();
            });
        };
        let _guard = crate::runtime().enter();
        let funnel = phone_funnel::start(program, self.port as u16, status);
        self.as_mut().rust_mut().funnel = Some(funnel);
        self.as_mut().set_funnel_state(QString::from("starting"));
    }

    pub fn use_relay(mut self: Pin<&mut Self>, on: bool) {
        let path = relay_path();
        let mut relay = RelaySettings::load(&path);
        relay.on = on;
        if let Err(e) = relay.save(&path) {
            self.as_mut().set_relay_error(QString::from(&format!("Couldn't save the relay: {e:#}")));
            return;
        }
        self.as_mut().set_relay_on(on);
        self.as_mut().set_relay_error(QString::default());
        self.update_relay();
    }

    /// Connects to the relay while phone access and the relay are on.
    fn update_relay(mut self: Pin<&mut Self>) {
        let relay = RelaySettings::load(&relay_path());
        let base = relay.ws_base().filter(|_| self.enabled && relay.on);
        let Some(base) = base else {
            self.as_mut().rust_mut().relay = None;
            self.as_mut().set_relay_state(QString::from("off"));
            return;
        };
        if self.relay.is_some() {
            return;
        }
        let Some(shared) = self.as_mut().shared() else { return };
        let generation = self.relay_generation + 1;
        self.as_mut().rust_mut().relay_generation = generation;
        let thread = self.qt_thread();
        let status = move |s: phone_relay::Status| {
            let _ = thread.queue(move |mut this| {
                if this.relay_generation != generation || this.relay.is_none() {
                    return;
                }
                let (state, error) = match s {
                    phone_relay::Status::Connecting => ("connecting", String::new()),
                    phone_relay::Status::Online(_) => ("online", String::new()),
                    phone_relay::Status::Failed(e) => ("failed", e),
                };
                this.as_mut().set_relay_error(QString::from(&error));
                this.as_mut().set_relay_state(QString::from(state));
                this.share_remote_endpoints();
            });
        };
        let _guard = crate::runtime().enter();
        let client = phone_relay::start(base, relay.secret, shared, status);
        self.as_mut().rust_mut().relay = Some(client);
        self.as_mut().set_relay_state(QString::from("connecting"));
    }

    pub fn set_access_port(mut self: Pin<&mut Self>, port: i32) {
        if !(1024..=65535).contains(&port) || port == self.port {
            return;
        }
        set_setting("phone_port", &port.to_string());
        self.as_mut().set_port(port);
        if self.enabled {
            self.as_mut().restart();
        }
        // Funnel points at the port.
        self.as_mut().rust_mut().funnel = None;
        self.update_funnel();
    }

    /// (Re)starts the server on the port.
    fn restart(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().server = None;
        self.as_mut().set_running(false);
        let Some(shared) = self.as_mut().shared() else { return };
        let addr: std::net::SocketAddr = ([0, 0, 0, 0], self.port as u16).into();
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            let result = phone_server::start(addr, shared).await;
            let _ = thread.queue(move |mut this| match result {
                Ok(server) if this.enabled => {
                    if let Some(shared) = &this.shared {
                        shared.port.store(addr.port(), std::sync::atomic::Ordering::Relaxed);
                    }
                    this.as_mut().rust_mut().server = Some(server);
                    this.as_mut().set_running(true);
                    this.as_mut().set_error(QString::default());
                }
                Ok(_) => {}
                Err(e) => this.as_mut().set_error(QString::from(&format!("Could not listen on port {}: {e}", addr.port()))),
            });
        });
    }

    pub fn addresses_json(&self) -> QString {
        let list: Vec<Value> = addresses().into_iter().map(|(n, l)| json!({ "address": n.address.to_string(), "label": l, "network": n.cidr() })).collect();
        QString::from(&Value::from(list).to_string())
    }

    pub fn pairing_json(&self, address: &QString) -> QString {
        let Some(shared) = self.shared.clone() else { return QString::from("{}") };
        let address = address.to_string();
        let mut offer = Offer {
            key: shared.identity.public_b64(),
            code: shared.new_code(),
            name: shared.name.clone(),
            relay: String::new(),
            relay_id: String::new(),
            endpoints: shared.endpoints(),
        };
        let url = if address == "funnel" {
            // Funnel's public address leads to this server: same origin.
            if self.funnel_state.to_string() != "online" {
                return QString::from("{}");
            }
            format!("{}/#p={}", self.funnel_url.to_string().trim_end_matches('/'), offer.encode())
        } else if address == "relay" {
            // The relay serves the page; the phone reaches harlo through it.
            let relay = RelaySettings::load(&relay_path());
            let (Some(page), Some(ws)) = (relay.page_base(), relay.ws_base()) else { return QString::from("{}") };
            offer.relay = ws;
            offer.relay_id = harlo_relay::relay_id(&relay.secret);
            format!("{page}/#p={}", offer.encode())
        } else {
            format!("http://{}:{}/#p={}", address, self.port, offer.encode())
        };
        QString::from(
            &json!({
                "url": url, "qr": qr_data_url(&url), "minutes": phone_server::CODE_LIFETIME.as_secs() / 60,
                "fingerprint": phone::fingerprint(&offer.key), "name": shared.name,
            })
            .to_string(),
        )
    }

    pub fn firewall_json(&self, address: &QString) -> QString {
        let none = || QString::from(&json!({ "firewall": "" }).to_string());
        let (Some(kind), Some(network)) = (firewall::detect(), network_of(&address.to_string())) else { return none() };
        let port = self.port as u16;
        let key = format!("{}:{port}", network.cidr());
        let hidden = setting("phone_firewall_hidden").split(' ').any(|k| k == key);
        QString::from(
            &json!({
                "firewall": kind.name(),
                "allowed": firewall::allows(kind, &network, port),
                "network": network.cidr(),
                "command": firewall::manual_command(kind, &network, port),
                "canAsk": firewall::pkexec().is_some(),
                "hidden": hidden,
            })
            .to_string(),
        )
    }

    pub fn allow_firewall(mut self: Pin<&mut Self>, address: &QString) {
        let (Some(kind), Some(network)) = (firewall::detect(), network_of(&address.to_string())) else { return };
        if self.firewall_busy {
            return;
        }
        self.as_mut().set_firewall_busy(true);
        self.as_mut().set_firewall_error(QString::default());
        let port = self.port as u16;
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = firewall::allow(kind, &network, port);
            let _ = thread.queue(move |mut this| {
                this.as_mut().set_firewall_busy(false);
                if let Err(e) = result {
                    this.as_mut().set_firewall_error(QString::from(&e));
                }
                let n = this.firewall_revision + 1;
                this.as_mut().set_firewall_revision(n);
            });
        });
    }

    pub fn hide_firewall_note(mut self: Pin<&mut Self>, address: &QString) {
        let Some(network) = network_of(&address.to_string()) else { return };
        let key = format!("{}:{}", network.cidr(), self.port);
        let mut hidden: Vec<String> = setting("phone_firewall_hidden").split(' ').filter(|k| !k.is_empty()).map(str::to_owned).collect();
        if !hidden.contains(&key) {
            hidden.push(key);
        }
        set_setting("phone_firewall_hidden", &hidden.join(" "));
        let n = self.firewall_revision + 1;
        self.as_mut().set_firewall_revision(n);
    }

    pub fn blocked_json(&self) -> QString {
        let empty = || QString::from("[]");
        let paired = self.shared.as_ref().is_some_and(|s| !s.devices.lock().unwrap().list.is_empty());
        let Some(kind) = firewall::detect().filter(|_| self.enabled && self.running && paired) else { return empty() };
        let port = self.port as u16;
        let hidden = setting("phone_firewall_hidden");
        let list: Vec<Value> = addresses()
            .into_iter()
            .filter(|(n, _)| firewall::allows(kind, n, port) == Some(false) && !hidden.split(' ').any(|k| k == format!("{}:{port}", n.cidr())))
            .map(|(n, label)| json!({
                "address": n.address.to_string(), "tailscale": label.starts_with("Tailscale"), "label": label,
                "network": n.cidr(), "firewall": kind.name(), "canAsk": firewall::pkexec().is_some(),
                "command": firewall::manual_command(kind, &n, port),
            }))
            .collect();
        QString::from(&Value::from(list).to_string())
    }

    pub fn devices_json(&self) -> QString {
        // Read from disk when the server isn't up (devices still listed).
        let list = match &self.shared {
            Some(s) => s.devices.lock().unwrap().list.clone(),
            None => Devices::load(&phone::dir().join("devices.json")).list,
        };
        let list: Vec<Value> = list.iter().map(|d| json!({ "id": d.id, "name": d.name, "pairedAt": d.paired_at, "lastSeen": d.last_seen, "push": d.push.is_some() })).collect();
        QString::from(&Value::from(list).to_string())
    }

    pub fn set_notifications_paused(mut self: Pin<&mut Self>, paused: bool) {
        crate::phone_push::set_paused(paused);
        set_setting("phone_paused", if paused { "1" } else { "0" });
        self.as_mut().set_paused(paused);
    }

    pub fn has_devices(&self) -> bool {
        match &self.shared {
            Some(s) => !s.devices.lock().unwrap().list.is_empty(),
            None => !Devices::load(&phone::dir().join("devices.json")).list.is_empty(),
        }
    }

    pub fn remove_device(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        match self.shared.clone() {
            Some(s) => {
                let _ = s.devices.lock().unwrap().remove(&id);
                s.bump_devices();
            }
            None => {
                let mut d = Devices::load(&phone::dir().join("devices.json"));
                let _ = d.remove(&id);
            }
        }
        let n = self.revision + 1;
        self.as_mut().set_revision(n);
    }

    pub fn push_json(&self) -> QString {
        QString::from(&json!({ "relay": crate::phone_push::relay_base(), "error": crate::phone_push::error() }).to_string())
    }

    pub fn send_test_push(mut self: Pin<&mut Self>) {
        if self.as_mut().shared().is_none() {
            return;
        }
        self.as_mut().set_push_note(QString::from("sending"));
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            let alert = crate::phone_push::Alert {
                what: crate::phone_push::What::Test, key: String::new(), title: "Test notification".into(),
                body: "Notifications from harlo reach this phone.".into(), badge: 0, quiet: false,
            };
            let note = match crate::phone_push::deliver(&alert, true).await {
                Ok(0) => "No phone takes notifications yet: open the harlo app on your iPhone and allow them.".to_owned(),
                Ok(1) => "Sent to 1 phone.".to_owned(),
                Ok(n) => format!("Sent to {n} phones."),
                Err(e) => e,
            };
            let _ = thread.queue(move |mut this| this.as_mut().set_push_note(QString::from(&note)));
        });
    }
}
