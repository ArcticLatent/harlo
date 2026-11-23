//! The phone bridge against a private OpenCode server: pairing, the sealed
//! channel, the session list, a session's rows as they stream, messages,
//! approvals and new sessions, as the phone app sees them.
//!
//! Each test runs its own `opencode serve` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service. The phone is a
//! stand-in that speaks the phone app's protocol.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod support;

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use harlo_core::phone::{Identity, Phone, Sealed, Settings, key_bytes};
use harlo_core::server::Location;
use opencode_client::{Effect, Rule};
use serde_json::{Value, json};
use support::{Sandbox, WAIT};
use tokio_tungstenite::tungstenite::Message;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A phone app stand-in: its key, its socket and its copy of the open
/// session's rows.
struct TestPhone {
    socket: Socket,
    sealed: Sealed,
    next_id: u64,
    rows: Vec<Value>,
    /// Events that arrived while waiting for something else.
    seen: Vec<Value>,
}

/// Connects as the phone with `key`, saying hello (with `code` when it's
/// pairing); returns the phone, or Harlo's plain answer when it was turned away.
async fn connect(
    address: SocketAddr,
    key: &Identity,
    harlo: &str,
    code: &str,
) -> Result<TestPhone, Value> {
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}/ws"))
        .await
        .expect("the phone server answers");
    let mut hello = json!({ "type": "hello", "key": key.public_b64(), "features": [] });
    if !code.is_empty() {
        hello["code"] = json!(code);
        hello["name"] = json!("Test phone");
    }
    socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .expect("hello");
    let answer = tokio::time::timeout(WAIT, socket.next())
        .await
        .expect("an answer in time")
        .expect("an answer")
        .expect("a frame");
    let answer: Value = serde_json::from_str(answer.to_text().expect("text")).expect("JSON");
    if answer["type"] != "ready" {
        return Err(answer);
    }
    let channel = key.channel(&key_bytes(harlo).expect("Harlo's key"));
    let id = answer["conn"].as_str().expect("a connection ID").to_owned();
    Ok(TestPhone {
        socket,
        sealed: Sealed::new(channel, id),
        next_id: 0,
        rows: Vec::new(),
        seen: Vec::new(),
    })
}

impl TestPhone {
    /// The next sealed message; `Null` when Harlo hung up.
    async fn next(&mut self) -> Value {
        loop {
            match tokio::time::timeout(WAIT, self.socket.next())
                .await
                .expect("a message in time")
            {
                Some(Ok(Message::Text(text))) => {
                    let message = self.sealed.open(&text).expect("a sealed frame opens");
                    self.keep(&message);
                    return message;
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => return Value::Null,
                Some(Ok(_)) => {}
            }
        }
    }

    /// Applies row events to the phone's copy, as the app does.
    fn keep(&mut self, message: &Value) {
        match message["ev"].as_str() {
            Some("reset") => {
                self.rows = message["rows"].as_array().cloned().unwrap_or_default();
            }
            Some("rows") => {
                for change in message["changes"].as_array().into_iter().flatten() {
                    let at = usize::try_from(change["at"].as_u64().unwrap_or(0)).unwrap_or(0);
                    match change["op"].as_str() {
                        Some("update") => self.rows[at] = change["row"].clone(),
                        Some("remove") => {
                            let count =
                                usize::try_from(change["count"].as_u64().unwrap_or(0)).unwrap_or(0);
                            self.rows.drain(at..at + count);
                        }
                        Some("insert") => {
                            let new = change["rows"].as_array().cloned().unwrap_or_default();
                            self.rows.splice(at..at, new);
                        }
                        other => panic!("an unknown change {other:?}"),
                    }
                }
            }
            _ => {}
        }
    }

    /// Sends a request and returns its reply; events on the way are kept.
    async fn call(&mut self, mut request: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        request["id"] = json!(id);
        let frame = self.sealed.seal(&request);
        self.socket
            .send(Message::Text(frame.into()))
            .await
            .expect("send");
        loop {
            let message = self.next().await;
            assert!(!message.is_null(), "Harlo hung up during {request}");
            if message["id"] == id {
                return message;
            }
            self.seen.push(message);
        }
    }

    /// Waits for an event that `pick` accepts, looking at kept ones first.
    async fn until<T>(
        &mut self,
        what: &str,
        mut pick: impl FnMut(&Self, &Value) -> Option<T>,
    ) -> T {
        // Messages passed over stay kept, for a later wait.
        let kept = std::mem::take(&mut self.seen);
        for (i, message) in kept.iter().enumerate() {
            if let Some(found) = pick(self, message) {
                self.seen = kept[i + 1..].to_vec();
                return found;
            }
        }
        self.seen = kept;
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            assert!(
                tokio::time::Instant::now() < deadline,
                "waited too long for {what}"
            );
            let message = self.next().await;
            assert!(!message.is_null(), "Harlo hung up while waiting for {what}");
            if let Some(found) = pick(self, &message) {
                return found;
            }
            self.seen.push(message);
        }
    }

    /// The text of the phone's rows, one per line, to search.
    fn text(&self) -> String {
        self.rows
            .iter()
            .map(|r| {
                format!(
                    "{} {}",
                    r["kind"].as_str().unwrap_or_default(),
                    r["text"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Starts phone access for the sandbox's server, keeping its files in `folder`.
async fn phone_for(sandbox: &Sandbox, folder: &std::path::Path) -> Phone {
    Phone::start(Settings {
        folder: folder.to_owned(),
        name: "Test computer".to_owned(),
        location: Location::Server {
            url: sandbox.url.clone(),
            password: Some(sandbox.password.clone()),
        },
        hidden: Vec::new(),
        listen: "127.0.0.1:0".parse().expect("an address"),
    })
    .await
    .expect("phone access starts")
}

/// Starts phone access and pairs a stand-in phone with it.
async fn paired(sandbox: &Sandbox, folder: &std::path::Path) -> (Phone, TestPhone) {
    let phone = phone_for(sandbox, folder).await;
    let code = phone.offer(None).code;
    let p = connect(
        phone.address(),
        &Identity::generate(),
        &phone.public_key(),
        &code,
    )
    .await
    .expect("pairs");
    (phone, p)
}

/// Asks for the session list until it has `key` (Harlo may still be loading
/// the server), and returns its row.
async fn listed(p: &mut TestPhone, key: &str) -> Value {
    for _ in 0..100 {
        let list = p.call(json!({ "op": "sessions" })).await;
        if let Some(row) = list["sessions"]
            .as_array()
            .and_then(|l| l.iter().find(|s| s["key"] == key))
        {
            return row.clone();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the session {key} was never listed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_pairs_once_and_is_turned_away_when_removed() {
    let sandbox = Sandbox::start().await;
    let folder = tempfile::tempdir().expect("scratch folder");
    let phone = phone_for(&sandbox, folder.path()).await;
    let harlo = phone.public_key();
    let key = Identity::generate();

    // An unpaired phone without a code is turned away.
    let denied = connect(phone.address(), &key, &harlo, "")
        .await
        .err()
        .expect("denied");
    assert_eq!(denied["type"], "denied");

    // With the offer's code it pairs, and pings through the sealed channel.
    let offer = phone.offer(None);
    let mut paired = connect(phone.address(), &key, &harlo, &offer.code)
        .await
        .expect("pairs");
    assert_eq!(paired.call(json!({ "op": "ping" })).await["pong"], true);
    let devices = phone.devices();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "Test phone");

    // The code is spent: another phone can't use it.
    let stranger = Identity::generate();
    let reused = connect(phone.address(), &stranger, &harlo, &offer.code).await;
    assert_eq!(reused.err().expect("denied")["type"], "denied");

    // Paired, the phone comes back without a code.
    let mut again = connect(phone.address(), &key, &harlo, "")
        .await
        .expect("back in");
    assert_eq!(again.call(json!({ "op": "ping" })).await["pong"], true);
    assert_eq!(phone.connected().len(), 1, "one phone, connected twice");

    // The key and the phone are kept for next time.
    let reopened = harlo_core::phone::saved_devices(folder.path());
    assert_eq!(reopened.len(), 1);

    // Removed in Harlo: each connection is told and closed.
    assert!(phone.remove(&devices[0].id).expect("removed"));
    assert_eq!(
        paired
            .until("revoked", |_, m| (m["ev"] == "revoked").then_some(()))
            .await,
        ()
    );
    assert!(paired.next().await.is_null(), "Harlo hangs up");
    assert_eq!(
        again
            .until("revoked", |_, m| (m["ev"] == "revoked").then_some(()))
            .await,
        ()
    );
    let gone = connect(phone.address(), &key, &harlo, "").await;
    assert_eq!(gone.err().expect("denied")["type"], "denied");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_follows_a_session_sends_and_approves() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let folder = tempfile::tempdir().expect("scratch folder");
    let (_phone, mut p) = paired(&sandbox, folder.path()).await;

    // The list has the session, under its project.
    let row = listed(&mut p, &session.id).await;
    assert_eq!(row["project"], "project");
    assert_eq!(row["status"], "idle");
    assert_eq!(row["harness"], "opencode");

    // Opening it: the reply, then its (empty) rows and status.
    let opened = p.call(json!({ "op": "open", "key": session.id })).await;
    assert_eq!(opened["key"], session.id.as_str());
    p.until("its rows", |_, m| (m["ev"] == "reset").then_some(()))
        .await;

    // A message from the phone: the agent's answer streams into its rows.
    let sent = p
        .call(json!({ "op": "send", "key": session.id, "text": "say hello from the phone" }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    p.until("the answer", |phone, _| {
        phone
            .text()
            .contains("markdown hello from the phone")
            .then_some(())
    })
    .await;
    assert!(
        p.text().contains("user say hello from the phone"),
        "{}",
        p.text()
    );
    p.until("idle", |_, m| {
        (m["ev"] == "status" && m["status"] == "idle").then_some(())
    })
    .await;

    // An edit that needs approval: the phone is asked, allows it once, and
    // the agent goes on.
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("rules");
    std::fs::write(sandbox.project().join("e.txt"), "one\ntwo\n").expect("e.txt");
    let script =
        r#"tool edit {"path":"e.txt","oldString":"two","newString":"three"} ;; say edited"#;
    let sent = p
        .call(json!({ "op": "send", "key": session.id, "text": script }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let request = p
        .until("the approval", |_, m| {
            (m["ev"] == "request").then(|| m["request"].clone())
        })
        .await;
    assert_eq!(request["approval"], true);
    assert!(
        request["title"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Allow tool: edit"),
        "{request}"
    );
    let answered = p
        .call(
            json!({ "op": "answer", "key": session.id, "requestId": request["id"],
                      "answer": { "value": "Allow once" } }),
        )
        .await;
    assert_eq!(answered["ok"], true, "{answered}");
    p.until("cleared", |_, m| {
        (m["ev"] == "cleared" && m["id"] == request["id"]).then_some(())
    })
    .await;
    p.until("edited", |phone, _| {
        phone.text().contains("markdown edited").then_some(())
    })
    .await;
    assert_eq!(
        std::fs::read_to_string(sandbox.project().join("e.txt")).expect("e.txt"),
        "one\nthree\n"
    );
    let tool = p
        .rows
        .iter()
        .find(|r| r["kind"] == "tool")
        .expect("a tool row")
        .clone();
    assert_eq!(tool["state"], "done");
    assert!(
        tool["diff"].as_str().unwrap_or_default().contains("+three"),
        "{tool}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_starts_and_renames_sessions() {
    let sandbox = Sandbox::start().await;
    let session = sandbox
        .client()
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let folder = tempfile::tempdir().expect("scratch folder");
    let (_phone, mut p) = paired(&sandbox, folder.path()).await;
    listed(&mut p, &session.id).await;

    // A new session from the phone, in the first project, with a first message.
    let choices = p.call(json!({ "op": "projects" })).await;
    assert_eq!(choices["projects"][0]["name"], "project", "{choices}");
    let started = p
        .call(json!({ "op": "new", "project": 0, "agent": "", "model": "", "text": "say started on the phone" }))
        .await;
    let new_key = started["key"].as_str().expect("a new session").to_owned();
    assert_ne!(new_key, session.id);
    p.until("the new session listed", |_, m| {
        (m["ev"] == "sessions"
            && m["sessions"]
                .as_array()
                .is_some_and(|l| l.iter().any(|s| s["key"] == new_key.as_str())))
        .then_some(())
    })
    .await;

    // Renamed from the phone: the list follows.
    let renamed = p
        .call(json!({ "op": "rename", "key": session.id, "name": "From the phone" }))
        .await;
    assert_eq!(renamed["ok"], true, "{renamed}");
    p.until("the new title", |_, m| {
        (m["ev"] == "sessions"
            && m["sessions"].as_array().is_some_and(|l| {
                l.iter()
                    .any(|s| s["key"] == session.id.as_str() && s["title"] == "From the phone")
            }))
        .then_some(())
    })
    .await;

    // Something Harlo doesn't do from a phone says so.
    let unknown = p.call(json!({ "op": "wiki", "key": session.id })).await;
    assert!(unknown["error"].is_string(), "{unknown}");
}
