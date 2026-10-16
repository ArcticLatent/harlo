//! Phone access (Settings → Devices, "Pair a device"): a phone pairs with
//! harlo by scanning a QR code and then reaches harlo's sessions over an
//! end-to-end encrypted channel, directly (LAN, Tailscale) or through a
//! relay that only forwards opaque frames.
//!
//! Crypto is NaCl's box (X25519 + XSalsa20-Poly1305), as tweetnacl does in
//! the phone's browser:
//!
//! - harlo has a long-lived key pair; the pairing offer (in the QR code)
//!   carries its public key and a one-time pairing code.
//! - The phone makes its own key pair once. Its first connection presents
//!   the code, which registers the phone's public key as a device; later
//!   connections are admitted by that key alone. Removing the device
//!   revokes it.
//! - Every frame after the plaintext hello is boxed between harlo's and the
//!   device's static keys (so each side knows who the other is), and
//!   carries the connection's random id and a sequence number (so recorded
//!   frames can't be replayed into another connection or reordered).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use crypto_box::aead::rand_core::RngCore;
use crypto_box::aead::{Aead, AeadCore, OsRng};
use crypto_box::{PublicKey, SalsaBox, SecretKey};
use serde_json::{Value, json};

/// Where harlo keeps its phone identity and devices.
pub fn dir() -> PathBuf {
    crate::db::default_path().parent().map(|p| p.join("phone")).unwrap_or_else(|| PathBuf::from("phone"))
}

pub fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>> {
    STANDARD.decode(s.trim()).or_else(|_| URL_SAFE_NO_PAD.decode(s.trim())).context("bad base64")
}

fn key_bytes(s: &str) -> Result<[u8; 32]> {
    unb64(s)?.try_into().map_err(|_| anyhow::anyhow!("a key is 32 bytes"))
}

/// Random bytes, base64url (pairing codes, connection ids).
pub fn random_id(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    std::io::Write::write_all(&mut options.open(&tmp)?, text.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ---- the relay ------------------------------------------------------------

/// harlo's relay (harlo.run): phones reach harlo through it from anywhere,
/// and it sends the iOS app's notifications. A build can name another
/// (`HARLO_DEFAULT_RELAY=https://…`).
const BUILT_IN_RELAY: &str = match option_env!("HARLO_DEFAULT_RELAY") {
    Some(url) => url,
    None => "https://relay.harlo.run",
};

static RELAY_FOR_RUN: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Another relay for this run only (`--relay=`: a test's stand-in relay on
/// this machine).
pub fn use_relay_for_run(url: &str) {
    let _ = RELAY_FOR_RUN.set(url.trim().to_owned());
}

/// The relay harlo uses.
pub fn relay_url() -> &'static str {
    RELAY_FOR_RUN.get().map(String::as_str).unwrap_or(BUILT_IN_RELAY.trim())
}

/// A harlo relay harlo connects to, so phones can reach it from anywhere. The
/// secret proves to the relay that this harlo owns its relay id.
/// (An address saved by earlier harlos, `url`, is no longer read: there's
/// one relay, `relay_url`.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelaySettings {
    pub on: bool,
    pub secret: String,
}

impl RelaySettings {
    /// Loads them, making the secret the first time.
    pub fn load(path: &Path) -> Self {
        let v: Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let secret = v["secret"].as_str().filter(|s| s.len() >= 32).map(str::to_owned).unwrap_or_else(|| random_id(32));
        Self { on: v["on"].as_bool().unwrap_or(false), secret }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_private(path, &json!({ "v": 1, "on": self.on, "secret": self.secret }).to_string())
    }

    /// The relay's WebSocket base (`wss://relay.harlo.run`).
    pub fn ws_base(&self) -> Option<String> {
        ws_base(relay_url())
    }

    /// The relay's page (where the pairing link points).
    pub fn page_base(&self) -> Option<String> {
        page_base(relay_url())
    }
}

/// A relay's WebSocket base from its address: https → wss, http → ws, no
/// scheme → wss.
fn ws_base(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    let base = if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if url.starts_with("wss://") || url.starts_with("ws://") {
        url.to_owned()
    } else if url.contains("://") {
        return None;
    } else {
        format!("wss://{url}")
    };
    Some(base)
}

/// A relay's page from its address.
fn page_base(url: &str) -> Option<String> {
    let base = ws_base(url)?;
    Some(match base.strip_prefix("wss://") {
        Some(rest) => format!("https://{rest}"),
        None => format!("http://{}", base.trim_start_matches("ws://")),
    })
}

// ---- harlo's identity -------------------------------------------------------

pub struct Identity {
    secret: SecretKey,
}

impl Identity {
    /// The saved key pair, or a new one (saved, readable only by you).
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(v) = serde_json::from_str::<Value>(&text)
            && let Some(secret) = v["secret"].as_str().and_then(|s| key_bytes(s).ok())
        {
            return Ok(Self { secret: SecretKey::from(secret) });
        }
        let secret = SecretKey::generate(&mut OsRng);
        write_private(path, &json!({ "v": 1, "secret": b64(&secret.to_bytes()) }).to_string()).with_context(|| format!("saving {}", path.display()))?;
        Ok(Self { secret })
    }

    /// From a secret key's bytes (a phone's side, in tests).
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self { secret: SecretKey::from(secret) }
    }

    pub fn public_b64(&self) -> String {
        b64(self.secret.public_key().as_bytes())
    }

    /// The channel with a device's public key.
    pub fn channel(&self, device_public: &[u8; 32]) -> Channel {
        Channel { cbox: SalsaBox::new(&PublicKey::from(*device_public), &self.secret) }
    }
}

/// A short, readable fingerprint of a public key (to compare by eye).
pub fn fingerprint(public_b64: &str) -> String {
    let bytes = unb64(public_b64).unwrap_or_default();
    let hex: String = bytes.iter().take(8).map(|b| format!("{b:02x}")).collect();
    hex.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join(" ")
}

// ---- devices ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// base64.
    pub public_key: String,
    pub paired_at: i64,
    pub last_seen: i64,
    /// Where its notifications go (the iOS app, once it's allowed them).
    pub push: Option<PushTarget>,
}

/// A phone's push registration.
#[derive(Debug, Clone, PartialEq)]
pub struct PushTarget {
    /// Its APNs device token (hex).
    pub token: String,
    /// An app run from Xcode (APNs' development environment).
    pub sandbox: bool,
    /// Wants questions and approvals.
    pub asks: bool,
    /// Wants finished (and failed) runs.
    pub done: bool,
}

impl PushTarget {
    fn from_json(v: &Value) -> Option<Self> {
        let token = v["token"].as_str()?.trim();
        // Hex, as APNs gives it.
        (!token.is_empty() && token.len() <= 200 && token.chars().all(|c| c.is_ascii_hexdigit())).then(|| Self {
            token: token.to_ascii_lowercase(),
            sandbox: v["sandbox"].as_bool().unwrap_or(false),
            asks: v["asks"].as_bool().unwrap_or(true),
            done: v["done"].as_bool().unwrap_or(true),
        })
    }

    fn to_json(&self) -> Value {
        json!({ "token": self.token, "sandbox": self.sandbox, "asks": self.asks, "done": self.done })
    }
}

/// Paired devices, in a JSON file.
pub struct Devices {
    path: PathBuf,
    pub list: Vec<Device>,
}

impl Devices {
    pub fn load(path: &Path) -> Self {
        let list = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["devices"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|d| {
                Some(Device {
                    id: d["id"].as_str()?.to_owned(),
                    name: d["name"].as_str().unwrap_or("Phone").to_owned(),
                    public_key: d["publicKey"].as_str()?.to_owned(),
                    paired_at: d["pairedAt"].as_i64().unwrap_or(0),
                    last_seen: d["lastSeen"].as_i64().unwrap_or(0),
                    push: PushTarget::from_json(&d["push"]),
                })
            })
            .collect();
        Self { path: path.to_owned(), list }
    }

    pub fn save(&self) -> Result<()> {
        let devices: Vec<Value> = self
            .list
            .iter()
            .map(|d| {
                let mut v = json!({ "id": d.id, "name": d.name, "publicKey": d.public_key, "pairedAt": d.paired_at, "lastSeen": d.last_seen });
                if let Some(p) = &d.push {
                    v["push"] = p.to_json();
                }
                v
            })
            .collect();
        write_private(&self.path, &json!({ "v": 1, "devices": devices }).to_string())
    }

    pub fn find_by_key(&self, public_b64: &str) -> Option<&Device> {
        self.list.iter().find(|d| d.public_key == public_b64)
    }

    /// Registers a device (or renames it, if its key is known).
    pub fn add(&mut self, name: &str, public_b64: &str, now: i64) -> Result<Device> {
        key_bytes(public_b64)?;
        let name = name.trim().chars().take(60).collect::<String>();
        let name = if name.is_empty() { "Phone".to_owned() } else { name };
        if let Some(d) = self.list.iter_mut().find(|d| d.public_key == public_b64) {
            d.name = name;
            d.last_seen = now;
            let d = d.clone();
            self.save()?;
            return Ok(d);
        }
        let d = Device { id: random_id(9), name, public_key: public_b64.to_owned(), paired_at: now, last_seen: now, push: None };
        self.list.push(d.clone());
        self.save()?;
        Ok(d)
    }

    pub fn remove(&mut self, id: &str) -> Result<bool> {
        let before = self.list.len();
        self.list.retain(|d| d.id != id);
        let removed = self.list.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// A phone's push registration (`{ token, sandbox, asks, done }`; no
    /// token: none). False when it isn't one.
    pub fn set_push(&mut self, id: &str, push: &Value) -> bool {
        let target = PushTarget::from_json(push);
        let ok = target.is_some() || push["token"].as_str().is_none_or(|t| t.trim().is_empty());
        if ok && let Some(d) = self.list.iter_mut().find(|d| d.id == id) && d.push != target {
            d.push = target;
            let _ = self.save();
        }
        ok
    }

    /// Forgets a push token APNs no longer takes.
    pub fn drop_push_token(&mut self, token: &str) {
        let mut changed = false;
        for d in self.list.iter_mut().filter(|d| d.push.as_ref().is_some_and(|p| p.token == token)) {
            d.push = None;
            changed = true;
        }
        if changed {
            let _ = self.save();
        }
    }

    pub fn touch(&mut self, id: &str, now: i64) {
        if let Some(d) = self.list.iter_mut().find(|d| d.id == id) {
            d.last_seen = now;
            let _ = self.save();
        }
    }
}

// ---- the pairing offer ------------------------------------------------------

/// What the QR code carries (in the page URL's fragment, never sent to a
/// server): harlo's public key, a one-time code, harlo's name, and the relay
/// it can be reached through (if any).
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub key: String,
    pub code: String,
    pub name: String,
    /// `wss://relay.example` and harlo's id there, or empty.
    pub relay: String,
    pub relay_id: String,
    /// Every other place harlo can be reached (`ws://…/ws`, `wss://…`), for
    /// apps that try them all.
    pub endpoints: Vec<String>,
}

impl Offer {
    pub fn encode(&self) -> String {
        let mut v = json!({ "v": 1, "k": self.key, "c": self.code, "n": self.name });
        if !self.relay.is_empty() {
            v["r"] = json!(self.relay);
            v["i"] = json!(self.relay_id);
        }
        if !self.endpoints.is_empty() {
            v["e"] = json!(self.endpoints);
        }
        URL_SAFE_NO_PAD.encode(v.to_string())
    }

    pub fn decode(s: &str) -> Result<Self> {
        let v: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(s.trim())?)?;
        if v["v"] != 1 {
            bail!("unknown offer version");
        }
        Ok(Self {
            key: v["k"].as_str().context("key")?.to_owned(),
            code: v["c"].as_str().unwrap_or_default().to_owned(),
            name: v["n"].as_str().unwrap_or_default().to_owned(),
            relay: v["r"].as_str().unwrap_or_default().to_owned(),
            relay_id: v["i"].as_str().unwrap_or_default().to_owned(),
            endpoints: v["e"].as_array().into_iter().flatten().filter_map(|e| e.as_str().map(str::to_owned)).collect(),
        })
    }
}

// ---- the encrypted channel ---------------------------------------------------

/// NaCl box between harlo and one device: frames are base64 of
/// `nonce (24) || ciphertext`.
pub struct Channel {
    cbox: SalsaBox,
}

impl Channel {
    pub fn seal(&self, plaintext: &[u8]) -> String {
        let nonce = SalsaBox::generate_nonce(&mut OsRng);
        let ct = self.cbox.encrypt(&nonce, plaintext).expect("encryption doesn't fail");
        let mut out = nonce.to_vec();
        out.extend(ct);
        b64(&out)
    }

    pub fn open(&self, frame: &str) -> Result<Vec<u8>> {
        let bytes = unb64(frame)?;
        if bytes.len() < 24 + 16 {
            bail!("frame too short");
        }
        let (nonce, ct) = bytes.split_at(24);
        self.cbox.decrypt(crypto_box::Nonce::from_slice(nonce), ct).map_err(|_| anyhow::anyhow!("a frame didn't decrypt"))
    }
}

/// One connection's channel: JSON messages tagged with the connection id
/// and increasing sequence numbers (each direction its own).
pub struct Session {
    channel: Channel,
    pub id: String,
    sent: u64,
    received: u64,
}

impl Session {
    pub fn new(channel: Channel, id: String) -> Self {
        Self { channel, id, sent: 0, received: 0 }
    }

    pub fn seal(&mut self, msg: &Value) -> String {
        self.sent += 1;
        let framed = json!({ "c": self.id, "s": self.sent, "m": msg });
        self.channel.seal(framed.to_string().as_bytes())
    }

    /// The message, if the frame is for this connection and newer than the
    /// last one.
    pub fn open(&mut self, frame: &str) -> Result<Value> {
        let v: Value = serde_json::from_slice(&self.channel.open(frame)?)?;
        if v["c"] != self.id.as_str() {
            bail!("a frame from another connection");
        }
        let seq = v["s"].as_u64().unwrap_or(0);
        if seq <= self.received {
            bail!("a frame out of order (replayed?)");
        }
        self.received = seq;
        Ok(v["m"].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("harlo-phone-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn identity_persists() {
        let d = scratch("id");
        let a = Identity::load_or_create(&d.join("identity.json")).unwrap();
        let b = Identity::load_or_create(&d.join("identity.json")).unwrap();
        assert_eq!(a.public_b64(), b.public_b64());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(d.join("identity.json")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(fingerprint(&a.public_b64()).len(), 19);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn both_sides_talk_and_replays_fail() {
        let d = scratch("chan");
        let harlo = Identity::load_or_create(&d.join("identity.json")).unwrap();
        let phone = SecretKey::generate(&mut OsRng);
        let harlo_pub = key_bytes(&harlo.public_b64()).unwrap();
        // harlo's side, and the phone's (the same box, the other way round).
        let mut ours = Session::new(harlo.channel(phone.public_key().as_bytes()), "conn1".into());
        let mut theirs = Session::new(Channel { cbox: SalsaBox::new(&PublicKey::from(harlo_pub), &phone) }, "conn1".into());
        let f1 = theirs.seal(&json!({ "op": "sessions" }));
        assert_eq!(ours.open(&f1).unwrap(), json!({ "op": "sessions" }));
        // The same frame again: refused.
        assert!(ours.open(&f1).is_err());
        let reply = ours.seal(&json!({ "ok": true }));
        assert_eq!(theirs.open(&reply).unwrap()["ok"], true);
        // Another connection's frame: refused.
        let mut other = Session::new(Channel { cbox: SalsaBox::new(&PublicKey::from(harlo_pub), &phone) }, "conn2".into());
        assert!(ours.open(&other.seal(&json!({}))).is_err());
        // A stranger's key: doesn't decrypt.
        let stranger = SecretKey::generate(&mut OsRng);
        let mut fake = Session::new(Channel { cbox: SalsaBox::new(&PublicKey::from(harlo_pub), &stranger) }, "conn1".into());
        let _ = fake.seal(&json!({}));
        assert!(ours.open(&fake.seal(&json!({ "op": "x" }))).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn devices_and_offers() {
        let d = scratch("dev");
        let path = d.join("devices.json");
        let mut devs = Devices::load(&path);
        let key = b64(SecretKey::generate(&mut OsRng).public_key().as_bytes());
        let dev = devs.add("Robin's phone", &key, 100).unwrap();
        assert!(devs.add("x", "not a key", 1).is_err());
        let again = Devices::load(&path);
        assert_eq!(again.find_by_key(&key).unwrap().name, "Robin's phone");
        let mut again = again;
        // Push: registered, kept, refused when it isn't a token, dropped.
        assert!(again.set_push(&dev.id, &json!({ "token": "ABcd12", "sandbox": true, "done": false })));
        assert!(!again.set_push(&dev.id, &json!({ "token": "not hex!" })));
        let push = Devices::load(&path).find_by_key(&key).unwrap().push.clone().unwrap();
        assert_eq!((push.token.as_str(), push.sandbox, push.asks, push.done), ("abcd12", true, true, false));
        again.drop_push_token("abcd12");
        assert!(Devices::load(&path).find_by_key(&key).unwrap().push.is_none());
        assert!(again.remove(&dev.id).unwrap());
        assert!(Devices::load(&path).find_by_key(&key).is_none());
        let offer = Offer { key: key.clone(), code: random_id(16), name: "desk".into(), relay: "wss://r.example".into(), relay_id: "abc".into(),
            endpoints: vec!["ws://100.64.0.2:7676/ws".into(), "ws://192.168.1.5:7676/ws".into()] };
        assert_eq!(Offer::decode(&offer.encode()).unwrap(), offer);
        assert!(Offer::decode("!!").is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    /// The phone's side in JavaScript (tweetnacl, as the phone page uses)
    /// reads harlo's frame and answers (skipped without node).
    #[test]
    fn talks_with_tweetnacl() {
        if std::process::Command::new("node").arg("--version").output().is_err() {
            return;
        }
        let nacl = concat!(env!("CARGO_MANIFEST_DIR"), "/../harlo-app/phone/nacl-fast.min.js");
        let d = scratch("js");
        let harlo = Identity::load_or_create(&d.join("identity.json")).unwrap();
        let phone = SecretKey::generate(&mut OsRng);
        let mut ours = Session::new(harlo.channel(phone.public_key().as_bytes()), "c9".into());
        let frame = ours.seal(&json!({ "hello": "phone" }));
        let script = format!(
            r#"const nacl = require({nacl:?});
const b = s => Uint8Array.from(Buffer.from(s, "base64"));
const sk = b({sk:?}), pk = b({pk:?});
const raw = b({frame:?});
const msg = nacl.box.open(raw.subarray(24), raw.subarray(0, 24), pk, sk);
const got = JSON.parse(Buffer.from(msg).toString());
const nonce = nacl.randomBytes(24);
const reply = Buffer.from(JSON.stringify({{ c: got.c, s: 1, m: {{ echo: got.m.hello }} }}));
const box = nacl.box(reply, nonce, pk, sk);
process.stdout.write(Buffer.concat([Buffer.from(nonce), Buffer.from(box)]).toString("base64"));"#,
            nacl = nacl,
            sk = b64(&phone.to_bytes()),
            pk = harlo.public_b64(),
            frame = frame,
        );
        let out = std::process::Command::new("node").arg("-e").arg(script).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let reply = ours.open(&String::from_utf8_lossy(&out.stdout)).unwrap();
        assert_eq!(reply, json!({ "echo": "phone" }));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn relay_settings() {
        let dir = std::env::temp_dir().join(format!("harlo-relay-settings-{}", std::process::id()));
        let path = dir.join("relay.json");
        let mut r = RelaySettings::load(&path);
        assert!(r.secret.len() >= 32);
        r.on = true;
        r.save(&path).unwrap();
        let again = RelaySettings::load(&path);
        assert_eq!(again, r);
        assert_eq!(again.ws_base().as_deref(), Some("wss://relay.harlo.run"));
        assert_eq!(again.page_base().as_deref(), Some("https://relay.harlo.run"));
        assert_eq!(ws_base("https://relay.example.com/").as_deref(), Some("wss://relay.example.com"));
        assert_eq!(ws_base("http://10.0.0.2:8080").as_deref(), Some("ws://10.0.0.2:8080"));
        assert_eq!(page_base("http://10.0.0.2:8080").as_deref(), Some("http://10.0.0.2:8080"));
        assert_eq!(page_base("relay.example.com").as_deref(), Some("https://relay.example.com"));
        assert_eq!(ws_base("ftp://x"), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

}
