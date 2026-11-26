//! Notifications to the phone app while it's closed: a question or an
//! approval waiting, a run finished or failed, a plan limit used up or
//! reset.
//!
//! Only a holder of the app's push key can send them, so they go through
//! the relay, which has it. What a notification says is sealed for the
//! phone, as its connection is; the relay and Apple see a placeholder, such
//! as "A session waits for your approval", and the app opens the rest.
//! Phones connected now get none: the app is open.

// Rust guideline compliant 2026-10-09

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::{Channel, Hub, key_bytes, lock};

/// How long the relay has to take a notification.
const SEND_WAIT: Duration = Duration::from_secs(15);

/// What a notification is about.
#[derive(Debug, Clone)]
pub enum What {
    /// A question or an approval, as the phone reads requests.
    Ask(Value),
    /// A run finished.
    Done,
    /// A run failed.
    Failed,
    /// Settings' "Send a test notification".
    Test,
    /// A plan limit is used up; the alert's key is the limit's tag.
    Limit,
    /// A plan limit, nearly or fully used, has reset.
    Reset,
}

/// One notification.
#[derive(Debug, Clone)]
pub struct Alert {
    /// What it's about.
    pub what: What,
    /// The session's ID.
    pub key: String,
    /// The session's title.
    pub title: String,
    /// What happened, in a line.
    pub body: String,
    /// Questions and approvals waiting, in all: the app icon's badge.
    pub badge: usize,
}

/// Cuts `text` to `n` characters, ending in "…" when cut.
fn clip(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(n).collect::<String>())
    }
}

/// Returns the notification: what Apple shows if the app can't open it, the
/// badge, and (`h`) what it really says, sealed for the phone.
///
/// It stays under the 4 KB Apple takes.
#[must_use]
pub fn payload(alert: &Alert, channel: &Channel) -> Value {
    let (kind, placeholder, category) = match &alert.what {
        What::Ask(r) if r["approval"] == true => {
            ("ask", "A session waits for your approval", "approval")
        }
        What::Ask(r) if r["method"] == "confirm" => {
            ("ask", "A session waits for your answer", "approval")
        }
        What::Ask(_) => ("ask", "A session waits for your answer", "question"),
        What::Done => ("done", "A session finished", ""),
        What::Failed => ("failed", "A session stopped with an error", ""),
        What::Test => ("test", "Harlo can reach this phone", ""),
        What::Limit => ("limit", "A plan limit is used up", ""),
        What::Reset => ("reset", "A plan limit has reset", ""),
    };
    let mut content = json!({ "k": kind, "s": alert.key, "t": clip(&alert.title, 80),
                              "b": clip(&alert.body, 300) });
    if let What::Ask(r) = &alert.what {
        // What Allow and Deny need; a question's fields open in the app.
        let options: Vec<Value> = r["options"]
            .as_array()
            .map(|o| o.iter().take(6).cloned().collect())
            .unwrap_or_default();
        content["r"] = json!({
            "id": r["id"], "method": r["method"], "approval": r["approval"], "options": options,
            "title": clip(r["title"].as_str().unwrap_or_default(), 120),
            "message": clip(r["message"].as_str().unwrap_or_default(), 600),
        });
    }
    let mut aps = json!({ "alert": { "title": "Harlo", "body": placeholder }, "badge": alert.badge,
                          "sound": "default", "mutable-content": 1, "thread-id": alert.key });
    if !category.is_empty() {
        aps["category"] = json!(category);
    }
    json!({ "aps": aps, "h": channel.seal(content.to_string().as_bytes()) })
}

/// Whether the phone registered as `push` wants this notification.
fn wants(what: &What, push: &super::devices::PushTarget) -> bool {
    match what {
        What::Ask(_) => push.asks,
        What::Done | What::Failed => push.done,
        What::Limit | What::Reset => push.limits,
        What::Test => true,
    }
}

/// Sends `alert` to every paired phone that wants it and isn't connected
/// (every one that wants it, with `everyone`), through the relay at `base`.
///
/// # Errors
///
/// When no phone got it, why the last one didn't.
pub(crate) async fn deliver(
    hub: &Arc<Hub>,
    base: &str,
    alert: &Alert,
    everyone: bool,
) -> Result<usize, String> {
    let online = hub.online_ids();
    let devices: Vec<super::Device> = lock(&hub.devices)
        .list()
        .iter()
        .filter(|d| d.push.as_ref().is_some_and(|p| wants(&alert.what, p)))
        .filter(|d| everyone || !online.contains(&d.id))
        .cloned()
        .collect();
    if devices.is_empty() {
        return Ok(0);
    }
    let http = reqwest::Client::builder()
        .timeout(SEND_WAIT)
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}/push", base.trim_end_matches('/'));
    let (mut sent, mut failure) = (0, String::new());
    for device in devices {
        let (Some(push), Ok(key)) = (&device.push, key_bytes(&device.public_key)) else {
            continue;
        };
        let body = payload(alert, &hub.identity.channel(&key));
        let collapse = match &alert.what {
            What::Ask(r) => r["id"]
                .as_str()
                .map(|id| id.chars().take(64).collect::<String>()),
            _ => None,
        };
        let forward = json!({ "token": push.token, "sandbox": push.sandbox, "payload": body, "collapse": collapse });
        match http.post(&url).json(&forward).send().await {
            Ok(r) if r.status().is_success() => sent += 1,
            Ok(r) => {
                let status = r.status().as_u16();
                let v: Value = r.json().await.unwrap_or_default();
                let reason = v["reason"]
                    .as_str()
                    .or(v["error"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                if status == 410 {
                    lock(&hub.devices).drop_push_token(&push.token);
                    hub.bump_devices();
                    failure = format!(
                        "{}'s notifications were turned off, or the app was reinstalled. \
                         Open the app to turn them on again.",
                        device.name
                    );
                } else if reason.is_empty() {
                    failure = format!("The relay refused the notification ({status}).");
                } else {
                    failure = reason;
                }
            }
            Err(e) => failure = format!("Couldn't reach the relay: {e}"),
        }
    }
    if sent == 0 && !failure.is_empty() {
        Err(failure)
    } else {
        Ok(sent)
    }
}
