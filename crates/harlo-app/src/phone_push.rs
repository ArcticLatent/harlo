//! Push notifications to the iOS app while it's closed: a question or an
//! approval waiting (Allow / Deny from the lock screen), a run finished or
//! failed, and the app icon's badge (what waits on you).
//!
//! Only a holder of the app's APNs key can send them, so they go through
//! harlo's relay, which has it (harlo-relay with `--apns-key`). What a
//! notification says is sealed for the phone (NaCl box, as its connection),
//! so the relay and Apple see a placeholder ("A session waits for your
//! approval"); the app's notification extension opens it. Phones connected
//! now get none: the app is open.

use std::sync::{Arc, Mutex, OnceLock, Weak};

use harlo_apns as apns;
use harlo_core::phone::{self, Device};
use serde_json::{Value, json};

use crate::phone_server::Shared;

/// The app's bundle id (APNs' topic).
pub const TOPIC: &str = "com.arcticlatent.harlo";

/// What a notification is about.
#[derive(Debug, Clone)]
pub enum What {
    /// A question or approval (the request, as phones get it).
    Ask(Value),
    Done,
    Failed,
    /// Only the badge (something was answered).
    Badge,
    /// Settings' "Send a test notification".
    Test,
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub what: What,
    /// The session's key.
    pub key: String,
    /// The session's title.
    pub title: String,
    pub body: String,
    /// Questions and approvals waiting, in all.
    pub badge: usize,
    /// Quiet hours: no sound, and it doesn't light the screen.
    pub quiet: bool,
}

fn slot() -> &'static Mutex<Weak<Shared>> {
    static S: OnceLock<Mutex<Weak<Shared>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Weak::new()))
}

/// The phone server's devices and key, once phone access has them.
pub fn attach(shared: &Arc<Shared>) {
    *slot().lock().unwrap() = Arc::downgrade(shared);
}

/// Why the last notification didn't go ("" when it did).
fn last_error() -> &'static Mutex<String> {
    static E: OnceLock<Mutex<String>> = OnceLock::new();
    E.get_or_init(Default::default)
}

pub fn error() -> String {
    last_error().lock().unwrap().clone()
}

/// The relay notifications go through (its page's address).
pub fn relay_base() -> String {
    phone::RelaySettings::load(&phone::dir().join("relay.json")).page_base().unwrap_or_default()
}

/// The relay's client, and the address it was made for.
type Kept = Mutex<Option<(String, Arc<apns::Forwarder>)>>;

/// The relay's client, kept while its address stays.
fn relay() -> Result<Arc<apns::Forwarder>, String> {
    static F: OnceLock<Kept> = OnceLock::new();
    let base = relay_base();
    if base.is_empty() {
        return Err("This harlo was built without a relay to send notifications through.".into());
    }
    let mut kept = F.get_or_init(Default::default).lock().unwrap();
    if let Some((was, f)) = &*kept
        && *was == base
    {
        return Ok(f.clone());
    }
    let f = Arc::new(apns::Forwarder::new(&base).map_err(|e| format!("{e:#}"))?);
    *kept = Some((base, f.clone()));
    Ok(f)
}

/// Notifications to the phones are paused (the rail's phone button): only
/// a test one goes.
static PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_paused(on: bool) {
    PAUSED.store(on, std::sync::atomic::Ordering::SeqCst);
}

pub fn paused() -> bool {
    PAUSED.load(std::sync::atomic::Ordering::SeqCst)
}

/// Sends `alert` to the phones that want it and aren't connected, in the
/// background.
pub fn send(alert: Alert) {
    crate::runtime().spawn(async move {
        let _ = deliver(&alert, false).await;
    });
}

/// Sends `alert` (to connected phones too with `everyone`): how many got it,
/// or why none did.
pub async fn deliver(alert: &Alert, everyone: bool) -> Result<usize, String> {
    if paused() && !matches!(alert.what, What::Test) {
        return Ok(0);
    }
    let Some(shared) = slot().lock().unwrap().upgrade() else { return Err("Phone access is off.".into()) };
    let devices: Vec<Device> = {
        let online = shared.online.lock().unwrap();
        shared.devices.lock().unwrap().list.iter()
            .filter(|d| d.push.as_ref().is_some_and(|p| wants(&alert.what, p.asks, p.done)))
            .filter(|d| everyone || !online.contains_key(&d.id))
            .cloned()
            .collect()
    };
    if devices.is_empty() {
        return Ok(0);
    }
    if std::env::var("HARLO_TEST_PUSH_FILE").is_err()
        && let Err(e) = relay()
    {
        *last_error().lock().unwrap() = e.clone();
        return Err(e);
    }
    let mut sent = 0;
    let mut failure = String::new();
    for d in devices {
        let (Some(push), Some(key)) = (&d.push, phone::unb64(&d.public_key).ok().and_then(|b| <[u8; 32]>::try_from(b).ok())) else { continue };
        let body = payload(alert, &shared.identity.channel(&key));
        let collapse = match &alert.what {
            What::Ask(r) => r["id"].as_str().map(|id| id.chars().take(64).collect::<String>()),
            _ => None,
        };
        let result = forward(&apns::Push { device_token: &push.token, sandbox: push.sandbox, topic: TOPIC, payload: &body, collapse_id: collapse.as_deref(), kind: apns::Kind::Alert }).await;
        match result {
            apns::Sent::Ok => sent += 1,
            apns::Sent::Gone(reason) => {
                tracing::info!("phone {}: its push token is gone ({reason})", d.name);
                shared.devices.lock().unwrap().drop_push_token(&push.token);
                failure = format!("{}'s notifications were turned off or it was reinstalled ({reason}). Open the app to turn them on again.", d.name);
            }
            apns::Sent::Failed(e) => {
                tracing::warn!("phone {}: push: {e}", d.name);
                failure = e;
            }
        }
    }
    *last_error().lock().unwrap() = failure.clone();
    if sent == 0 && !failure.is_empty() { Err(failure) } else { Ok(sent) }
}

/// Sends one push through harlo's relay (tests: `HARLO_TEST_PUSH_FILE`,
/// where it's written instead, for the phone's side to read).
pub async fn forward(push: &apns::Push<'_>) -> apns::Sent {
    if let Ok(path) = std::env::var("HARLO_TEST_PUSH_FILE") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{}", apns::forward_json(push));
        }
        return apns::Sent::Ok;
    }
    match relay() {
        Ok(r) => r.send(push).await,
        Err(e) => apns::Sent::Failed(e),
    }
}

fn wants(what: &What, asks: bool, done: bool) -> bool {
    match what {
        What::Ask(_) => asks,
        What::Done | What::Failed => done,
        What::Badge => asks || done,
        What::Test => true,
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

/// The notification: what Apple shows if the app can't open it, the badge,
/// and (`h`) what it really says, sealed for the phone. Small enough for
/// APNs' 4 KB.
pub fn payload(alert: &Alert, channel: &phone::Channel) -> Value {
    if matches!(alert.what, What::Badge) {
        return json!({ "aps": { "badge": alert.badge } });
    }
    let (kind, placeholder, category) = match &alert.what {
        What::Ask(r) if r["approval"].as_bool() == Some(true) => ("ask", "A session waits for your approval", "approval"),
        What::Ask(r) if r["method"] == "confirm" => ("ask", "A session waits for your answer", "approval"),
        What::Ask(_) => ("ask", "A session waits for your answer", "question"),
        What::Done => ("done", "A session finished", ""),
        What::Failed => ("failed", "A session stopped with an error", ""),
        What::Test => ("test", "harlo can reach this phone", ""),
        What::Badge => unreachable!(),
    };
    let mut content = json!({ "k": kind, "s": alert.key, "t": clip(&alert.title, 80), "b": clip(&alert.body, 300) });
    if let What::Ask(r) = &alert.what {
        // What Allow and Deny need, without `ask`'s questions (opened in the app).
        content["r"] = json!({
            "id": r["id"], "method": r["method"], "title": clip(r["title"].as_str().unwrap_or_default(), 120),
            "message": clip(r["message"].as_str().unwrap_or_default(), 600), "approval": r["approval"],
            "options": r["options"].as_array().map(|o| o.iter().take(6).cloned().collect::<Vec<_>>()).unwrap_or_default(),
        });
    }
    let mut aps = json!({ "alert": { "title": "harlo", "body": placeholder }, "badge": alert.badge, "sound": "default", "mutable-content": 1, "thread-id": alert.key });
    if alert.quiet {
        // Into Notification Centre, without a sound or lighting the screen.
        aps.as_object_mut().unwrap().remove("sound");
        aps["interruption-level"] = json!("passive");
    }
    if !category.is_empty() {
        aps["category"] = json!(category);
    }
    json!({ "aps": aps, "h": channel.seal(content.to_string().as_bytes()) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto_box::SecretKey;
    use crypto_box::aead::OsRng;

    #[test]
    fn sealed_for_the_phone_and_small() {
        let harlo = phone::Identity::from_secret(SecretKey::generate(&mut OsRng).to_bytes());
        let phone_secret = SecretKey::generate(&mut OsRng);
        let harlo_public: [u8; 32] = phone::unb64(&harlo.public_b64()).unwrap().try_into().unwrap();
        let to_phone = harlo.channel(phone_secret.public_key().as_bytes());
        let at_phone = phone::Identity::from_secret(phone_secret.to_bytes()).channel(&harlo_public);
        let request = json!({ "id": "r1", "method": "select", "title": "Allow tool: bash", "message": "x".repeat(5000), "approval": true,
                              "options": ["Approve", "Deny"], "questions": [{ "question": "y".repeat(5000) }] });
        let alert = Alert { what: What::Ask(request), key: "live:3".into(), title: "Fix the list".into(), body: "bash: rm -rf target".into(), badge: 2, quiet: false };
        let p = payload(&alert, &to_phone);
        assert!(p.to_string().len() < 4000, "{} bytes", p.to_string().len());
        assert_eq!(p["aps"]["category"], "approval");
        assert_eq!(p["aps"]["badge"], 2);
        assert!(!p.to_string().contains("rm -rf"), "Apple sees only the placeholder");
        let content: Value = serde_json::from_slice(&at_phone.open(p["h"].as_str().unwrap()).unwrap()).unwrap();
        assert_eq!((content["k"].as_str(), content["s"].as_str(), content["b"].as_str()), (Some("ask"), Some("live:3"), Some("bash: rm -rf target")));
        assert_eq!(content["r"]["options"], json!(["Approve", "Deny"]));
        assert!(content["r"].get("questions").is_none());
        let done = payload(&Alert { what: What::Done, key: "k".into(), title: "t".into(), body: "b".into(), badge: 0, quiet: false }, &to_phone);
        assert!(done["aps"].get("category").is_none());
        assert_eq!(done["aps"]["sound"], "default");
        let hushed = payload(&Alert { what: What::Done, key: "k".into(), title: "t".into(), body: "b".into(), badge: 0, quiet: true }, &to_phone);
        assert!(hushed["aps"].get("sound").is_none() && hushed["aps"]["interruption-level"] == "passive", "quiet hours: {hushed}");
        let badge = payload(&Alert { what: What::Badge, key: String::new(), title: String::new(), body: String::new(), badge: 1, quiet: false }, &to_phone);
        assert_eq!(badge, json!({ "aps": { "badge": 1 } }));
    }
}
