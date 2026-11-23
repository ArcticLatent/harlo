//! Paired phones, the pairing offer, and the relay's settings.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::crypto::{key_bytes, random_id, write_private};

/// A paired phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Harlo's ID for it.
    pub id: String,
    /// The name it gave when it paired, such as "Burce's iPhone".
    pub name: String,
    /// Its public key, as base64.
    pub public_key: String,
    /// When it paired, in seconds since the Unix epoch.
    pub paired_at: i64,
    /// When it was last connected, in seconds since the Unix epoch.
    pub last_seen: i64,
    /// Where its notifications go, once the app allows them.
    pub push: Option<PushTarget>,
}

/// A phone's push registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    /// Its APNs device token, as lowercase hex.
    pub token: String,
    /// Whether the app runs from Xcode, so APNs' development servers apply.
    pub sandbox: bool,
    /// Whether it wants questions and approvals.
    pub asks: bool,
    /// Whether it wants finished and failed runs.
    pub done: bool,
}

impl PushTarget {
    /// Reads a registration the phone sent, `{ token, sandbox, asks, done }`.
    #[must_use]
    pub fn from_json(v: &Value) -> Option<Self> {
        let token = v["token"].as_str()?.trim();
        let hex = token.len() <= 200 && token.chars().all(|c| c.is_ascii_hexdigit());
        (!token.is_empty() && hex).then(|| Self {
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

/// The paired phones, kept in a JSON file only the user can read.
#[derive(Debug)]
pub struct Devices {
    path: PathBuf,
    list: Vec<Device>,
}

impl Devices {
    /// Loads the phones saved at `path`; none when it's missing or unreadable.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let saved: Value = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let list = saved["devices"]
            .as_array()
            .into_iter()
            .flatten()
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
        Self {
            path: path.to_owned(),
            list,
        }
    }

    /// Returns the phones, in the order they paired.
    #[must_use]
    pub fn list(&self) -> &[Device] {
        &self.list
    }

    fn save(&self) -> std::io::Result<()> {
        let devices: Vec<Value> = self
            .list
            .iter()
            .map(|d| {
                let mut v = json!({ "id": d.id, "name": d.name, "publicKey": d.public_key,
                                    "pairedAt": d.paired_at, "lastSeen": d.last_seen });
                if let Some(p) = &d.push {
                    v["push"] = p.to_json();
                }
                v
            })
            .collect();
        write_private(
            &self.path,
            &json!({ "v": 1, "devices": devices }).to_string(),
        )
    }

    /// Returns the phone with this public key.
    #[must_use]
    pub fn find_by_key(&self, public_b64: &str) -> Option<&Device> {
        self.list.iter().find(|d| d.public_key == public_b64)
    }

    /// Pairs a phone, or renames it when its key is known already.
    ///
    /// # Errors
    ///
    /// When `public_b64` isn't a key, or the phones can't be saved.
    pub fn add(&mut self, name: &str, public_b64: &str, now: i64) -> Result<Device, String> {
        key_bytes(public_b64)?;
        let name: String = name.trim().chars().take(60).collect();
        let name = if name.is_empty() {
            "Phone".to_owned()
        } else {
            name
        };
        let device = if let Some(d) = self.list.iter_mut().find(|d| d.public_key == public_b64) {
            d.name = name;
            d.last_seen = now;
            d.clone()
        } else {
            let d = Device {
                id: random_id(9),
                name,
                public_key: public_b64.to_owned(),
                paired_at: now,
                last_seen: now,
                push: None,
            };
            self.list.push(d.clone());
            d
        };
        self.save()
            .map_err(|e| format!("Couldn't save the paired phones: {e}"))?;
        Ok(device)
    }

    /// Forgets a phone; returns whether it was paired.
    ///
    /// # Errors
    ///
    /// When the phones can't be saved.
    pub fn remove(&mut self, id: &str) -> std::io::Result<bool> {
        let before = self.list.len();
        self.list.retain(|d| d.id != id);
        let removed = self.list.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// Sets a phone's push registration; `false` when `push` isn't one.
    ///
    /// A registration without a token turns its notifications off.
    pub fn set_push(&mut self, id: &str, push: &Value) -> bool {
        let target = PushTarget::from_json(push);
        let ok = target.is_some() || push["token"].as_str().is_none_or(|t| t.trim().is_empty());
        if ok
            && let Some(d) = self.list.iter_mut().find(|d| d.id == id)
            && d.push != target
        {
            d.push = target;
            // A lost registration comes back the next time the app connects.
            let _ = self.save();
        }
        ok
    }

    /// Forgets a push token APNs no longer takes.
    pub fn drop_push_token(&mut self, token: &str) {
        let mut changed = false;
        for d in &mut self.list {
            if d.push.as_ref().is_some_and(|p| p.token == token) {
                d.push = None;
                changed = true;
            }
        }
        if changed {
            let _ = self.save();
        }
    }

    /// Notes that a phone is connected now.
    pub fn touch(&mut self, id: &str, now: i64) {
        if let Some(d) = self.list.iter_mut().find(|d| d.id == id) {
            d.last_seen = now;
            let _ = self.save();
        }
    }
}

/// What the pairing QR code carries: Harlo's public key, a one-time code,
/// Harlo's name, and where Harlo can be reached.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Offer {
    /// Harlo's public key, as base64.
    pub key: String,
    /// The one-time pairing code.
    pub code: String,
    /// This computer's name.
    pub name: String,
    /// The relay's WebSocket base, such as `wss://relay.example`, or empty.
    pub relay: String,
    /// Harlo's ID on that relay.
    pub relay_id: String,
    /// Every other address Harlo answers on, such as `ws://192.168.1.4:7676/ws`.
    pub endpoints: Vec<String>,
}

impl Offer {
    /// Encodes the offer as unpadded URL-safe base64 of its JSON.
    #[must_use]
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

    /// Returns the link a pairing QR code carries: the offer in the fragment
    /// of the page the phone first tries.
    ///
    /// That's the relay's page when the offer names a relay, otherwise Harlo
    /// at its first direct address (`http://192.168.1.5:7676/#p=…`). An offer
    /// with neither is the offer alone.
    #[must_use]
    pub fn link(&self) -> String {
        let page = if self.relay.is_empty() {
            self.endpoints
                .first()
                .and_then(|e| http_base(e))
                .map(|base| base.trim_end_matches("/ws").to_owned())
        } else {
            http_base(&self.relay)
        };
        match page {
            Some(page) => format!("{page}/#p={}", self.encode()),
            None => self.encode(),
        }
    }

    /// Reads an offer from a QR code's text: its link, or the offer alone.
    ///
    /// # Errors
    ///
    /// When `text` isn't an offer: not base64 JSON, another version, or no key.
    pub fn decode(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let text = text
            .split_once('#')
            .and_then(|(_, fragment)| fragment.split('&').find_map(|p| p.strip_prefix("p=")))
            .unwrap_or(text);
        let bytes = URL_SAFE_NO_PAD.decode(text).map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if v["v"] != 1 {
            return Err("an offer of another version".to_owned());
        }
        let text = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Ok(Self {
            key: v["k"]
                .as_str()
                .ok_or_else(|| "an offer without a key".to_owned())?
                .to_owned(),
            code: text("c"),
            name: text("n"),
            relay: text("r"),
            relay_id: text("i"),
            endpoints: v["e"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(str::to_owned))
                .collect(),
        })
    }
}

/// Returns Harlo's ID on a relay: the first 16 bytes of the secret's SHA-256,
/// as unpadded URL-safe base64.
#[must_use]
pub fn relay_id(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    URL_SAFE_NO_PAD.encode(&digest[..16])
}

/// Returns a relay's WebSocket base from its address.
///
/// `https` becomes `wss`, `http` becomes `ws`, and an address without a
/// scheme is taken as `wss`.
#[must_use]
pub fn ws_base(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    Some(if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if url.starts_with("wss://") || url.starts_with("ws://") {
        url.to_owned()
    } else if url.contains("://") {
        return None;
    } else {
        format!("wss://{url}")
    })
}

/// Returns a relay's web address from its address: where pairing links point
/// and notifications are handed in.
#[must_use]
pub fn http_base(url: &str) -> Option<String> {
    let base = ws_base(url)?;
    Some(match base.strip_prefix("wss://") {
        Some(rest) => format!("https://{rest}"),
        None => format!("http://{}", base.trim_start_matches("ws://")),
    })
}
