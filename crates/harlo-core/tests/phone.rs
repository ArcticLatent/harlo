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
mod test_phone;

use std::time::Duration;

use harlo_core::phone::{Identity, Offer, Phone, Settings};
use harlo_core::server::Location;
use opencode_client::{Effect, Rule};
use serde_json::{Value, json};
use support::Sandbox;
use test_phone::{TestPhone, connect};

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
        paused: false,
    })
    .await
    .expect("phone access starts")
}

/// Starts phone access and pairs a stand-in phone with it.
async fn paired(sandbox: &Sandbox, folder: &std::path::Path) -> (Phone, TestPhone) {
    let phone = phone_for(sandbox, folder).await;
    let code = phone.offer().code;
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

    // The QR code's link leads to Harlo's own page, with the offer after `#p=`.
    let address = phone.address();
    phone.set_endpoints(vec![format!("ws://{address}/ws")]);
    let offer = phone.offer();
    let link = offer.link();
    assert!(link.starts_with(&format!("http://{address}/#p=")), "{link}");
    assert_eq!(Offer::decode(&link).expect("the link reads"), offer);
    let page = reqwest::get(format!("http://{address}/"))
        .await
        .expect("the page")
        .text()
        .await
        .expect("its text");
    assert!(page.contains("Harlo app"), "{page}");

    // With the offer's code it pairs, and pings through the sealed channel.
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
