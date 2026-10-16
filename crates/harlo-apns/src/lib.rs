//! Apple's push service (APNs), for the iOS app's notifications. Only the
//! holder of an APNs key of the app's team can send them, so harlo's relay
//! sends them for everyone's harlo with the app's key (`Client`, in
//! harlo-relay), and harlo hands each one to the relay (`Forwarder`,
//! `POST /push`).
//!
//! Each request to Apple carries a token signed with the key (ES256,
//! renewed every 40 minutes; Apple wants one between 20 and 60 minutes
//! old). What a notification says is sealed for the phone beforehand
//! (harlo's `phone_push`), so Apple and the relay carry only a placeholder.
//! Live Activities (a session working, on the lock screen) get updates the
//! same way, to their own tokens ([`Kind::Activity`]); what they say is
//! sealed too, and the app's widget opens it.

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use serde_json::{Value, json};

const PRODUCTION: &str = "https://api.push.apple.com";
const SANDBOX: &str = "https://api.sandbox.push.apple.com";
/// How long a signed token is used.
const TOKEN_AGE: i64 = 40 * 60;

/// The APNs key: its team, its id and the key itself.
pub struct Credentials {
    pub team_id: String,
    pub key_id: String,
    key: EcdsaKeyPair,
}

impl Credentials {
    /// From the `.p8` file's text (PEM, PKCS#8).
    pub fn new(team_id: &str, key_id: &str, p8: &str) -> Result<Self> {
        let body: String = p8.lines().filter(|l| !l.starts_with("-----")).map(str::trim).collect();
        let der = STANDARD.decode(body).context("the .p8 file isn't a key (bad base64)")?;
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &SystemRandom::new())
            .map_err(|e| anyhow!("the .p8 file isn't an APNs key ({e})"))?;
        let (team_id, key_id) = (team_id.trim(), key_id.trim());
        if team_id.len() != 10 || key_id.len() != 10 {
            anyhow::bail!("the Team ID and the Key ID are 10 letters and digits each");
        }
        Ok(Self { team_id: team_id.to_owned(), key_id: key_id.to_owned(), key })
    }

    /// A signed token, issued at `now` (unix seconds).
    pub fn token(&self, now: i64) -> String {
        let b64 = |v: &Value| URL_SAFE_NO_PAD.encode(v.to_string());
        let head = format!("{}.{}", b64(&json!({ "alg": "ES256", "kid": self.key_id })), b64(&json!({ "iss": self.team_id, "iat": now })));
        let sig = self.key.sign(&SystemRandom::new(), head.as_bytes()).expect("signing doesn't fail");
        format!("{head}.{}", URL_SAFE_NO_PAD.encode(sig.as_ref()))
    }

    #[cfg(test)]
    fn public_key(&self) -> Vec<u8> {
        use ring::signature::KeyPair;
        self.key.public_key().as_ref().to_vec()
    }
}

/// How sending went.
#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    Ok,
    /// The phone's token no longer works (the app was removed, or the
    /// token is for the other environment): forget it.
    Gone(String),
    Failed(String),
}

/// What a push is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// A notification (or only the badge).
    #[default]
    Alert,
    /// A Live Activity's update or end, to the activity's own token
    /// (`urgent`: shown at once, else when iOS sees fit).
    Activity { urgent: bool },
}

/// One notification.
pub struct Push<'a> {
    pub device_token: &'a str,
    /// The development environment (an app run from Xcode); TestFlight and
    /// the App Store use production.
    pub sandbox: bool,
    /// The app's bundle id.
    pub topic: &'a str,
    pub payload: &'a Value,
    /// Replaces an earlier notification with the same id (the same request).
    pub collapse_id: Option<&'a str>,
    pub kind: Kind,
}

pub struct Client {
    http: reqwest::Client,
    credentials: Credentials,
    /// Instead of Apple's (tests): a plain HTTP/2 server.
    base: Option<String>,
    token: Mutex<Option<(String, i64)>>,
}

impl Client {
    pub fn new(credentials: Credentials, base: Option<String>) -> Result<Self> {
        let mut http = reqwest::Client::builder().timeout(Duration::from_secs(15));
        if base.as_deref().is_some_and(|b| b.starts_with("http://")) {
            http = http.http2_prior_knowledge();
        }
        Ok(Self { http: http.build()?, credentials, base, token: Mutex::new(None) })
    }

    fn token(&self) -> String {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let mut cached = self.token.lock().unwrap();
        match &*cached {
            Some((t, at)) if now - at < TOKEN_AGE => t.clone(),
            _ => {
                let t = self.credentials.token(now);
                *cached = Some((t.clone(), now));
                t
            }
        }
    }

    pub async fn send(&self, push: &Push<'_>) -> Sent {
        let base = self.base.as_deref().unwrap_or(if push.sandbox { SANDBOX } else { PRODUCTION });
        let (topic, kind, priority) = match push.kind {
            Kind::Alert => (push.topic.to_owned(), "alert", "10"),
            Kind::Activity { urgent } => (format!("{}.push-type.liveactivity", push.topic), "liveactivity", if urgent { "10" } else { "5" }),
        };
        let mut req = self
            .http
            .post(format!("{base}/3/device/{}", push.device_token))
            .bearer_auth(self.token())
            .header("apns-topic", topic)
            .header("apns-push-type", kind)
            .header("apns-priority", priority)
            .json(push.payload);
        if let Some(id) = push.collapse_id {
            req = req.header("apns-collapse-id", id);
        }
        match req.send().await {
            Ok(r) if r.status().is_success() => Sent::Ok,
            Ok(r) => {
                let status = r.status().as_u16();
                let reason = r.json::<Value>().await.ok().and_then(|v| v["reason"].as_str().map(str::to_owned)).unwrap_or_default();
                // The token's gone, or isn't one (or is the other environment's).
                if status == 410 || matches!(reason.as_str(), "BadDeviceToken" | "DeviceTokenNotForTopic" | "Unregistered") {
                    Sent::Gone(reason)
                } else {
                    Sent::Failed(format!("Apple refused it ({status} {reason})"))
                }
            }
            Err(e) => Sent::Failed(format!("couldn't reach Apple: {e}")),
        }
    }
}

/// One notification as harlo hands it to a relay (`POST /push`).
pub fn forward_json(push: &Push<'_>) -> Value {
    let mut v = json!({ "token": push.device_token, "sandbox": push.sandbox, "payload": push.payload, "collapse": push.collapse_id });
    if let Kind::Activity { urgent } = push.kind {
        v["activity"] = json!(true);
        v["urgent"] = json!(urgent);
    }
    v
}

/// A notification handed to a relay, checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Forwarded {
    pub token: String,
    pub sandbox: bool,
    pub payload: Value,
    pub collapse: Option<String>,
    pub kind: Kind,
}

/// Reads what harlo handed a relay. The payload must be a notification (an
/// `aps`) of at most 4 KB, as Apple takes; a Live Activity's, an update or
/// an end with its state.
pub fn read_forward(v: &Value) -> Result<Forwarded, String> {
    let token = v["token"].as_str().unwrap_or_default();
    if token.is_empty() || token.len() > 200 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("not a device token".into());
    }
    let payload = v["payload"].clone();
    if !payload["aps"].is_object() || payload.to_string().len() > 4096 {
        return Err("not a notification (an aps, at most 4 KB)".into());
    }
    let kind = if v["activity"].as_bool() == Some(true) {
        let aps = &payload["aps"];
        if !matches!(aps["event"].as_str(), Some("update" | "end")) || !aps["content-state"].is_object() || !aps["timestamp"].is_i64() {
            return Err("not a Live Activity update (an event, its content-state and timestamp)".into());
        }
        Kind::Activity { urgent: v["urgent"].as_bool() == Some(true) }
    } else {
        Kind::Alert
    };
    let collapse = v["collapse"].as_str().map(|c| c.chars().take(64).collect());
    Ok(Forwarded { token: token.to_ascii_lowercase(), sandbox: v["sandbox"].as_bool().unwrap_or(false), payload, collapse, kind })
}

/// Hands notifications to a harlo relay, which sends them to Apple with the
/// app's key.
pub struct Forwarder {
    http: reqwest::Client,
    /// The relay's address (`https://relay.example.com`).
    base: String,
}

impl Forwarder {
    pub fn new(base: &str) -> Result<Self> {
        Ok(Self { http: reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?, base: base.trim_end_matches('/').to_owned() })
    }

    /// Whether the relay sends notifications (it has the app's key).
    pub async fn available(&self) -> Result<bool, String> {
        let r = self.http.get(format!("{}/push", self.base)).send().await.map_err(|e| format!("couldn't reach the relay: {e}"))?;
        Ok(r.status().is_success() && r.json::<Value>().await.is_ok_and(|v| v["push"] == true))
    }

    pub async fn send(&self, push: &Push<'_>) -> Sent {
        let sent = self.http.post(format!("{}/push", self.base)).json(&forward_json(push)).send().await;
        match sent {
            Ok(r) if r.status().is_success() => Sent::Ok,
            Ok(r) => {
                let status = r.status().as_u16();
                let v: Value = r.json().await.unwrap_or_default();
                let reason = v["reason"].as_str().or(v["error"].as_str()).unwrap_or_default().to_owned();
                match status {
                    410 => Sent::Gone(reason),
                    404 | 405 if reason.is_empty() => Sent::Failed("this relay doesn't send notifications (an older harlo-relay)".into()),
                    _ => Sent::Failed(if reason.is_empty() { format!("the relay refused it ({status})") } else { reason }),
                }
            }
            Err(e) => Sent::Failed(format!("couldn't reach the relay: {e}")),
        }
    }
}

/// A new key in a `.p8` file's form (tests).
#[doc(hidden)]
pub fn test_p8() -> String {
    let der = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new()).unwrap();
    format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", STANDARD.encode(der.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey};

    #[test]
    fn tokens_are_signed_by_the_key() {
        let c = Credentials::new("ABCDE12345", "KEY1234567", &test_p8()).unwrap();
        let token = c.token(1_700_000_000);
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let head: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(head, json!({ "alg": "ES256", "kid": "KEY1234567" }));
        assert_eq!(claims, json!({ "iss": "ABCDE12345", "iat": 1_700_000_000 }));
        let sig = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, c.public_key()).verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig).unwrap();
    }

    #[test]
    fn bad_keys_are_told() {
        assert!(Credentials::new("ABCDE12345", "KEY1234567", "not a key").is_err());
        assert!(Credentials::new("short", "KEY1234567", &test_p8()).is_err());
    }
}
