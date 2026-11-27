//! A phone against a server that runs a real model: it pairs, opens a
//! session, sends a message, allows the edit it is asked about and reads
//! the answer as it comes, then asks a side question.
//!
//! This test starts nothing and is skipped unless asked for: it is run by
//! `tools/real/features.py` (the case `phone`), which starts a private
//! server on a scratch sign-in and names it here through `HARLO_REAL_URL`,
//! `HARLO_REAL_PASSWORD`, `HARLO_REAL_SESSION` and `HARLO_REAL_PROJECT`.
//! The phone is the stand-in that speaks the phone app's protocol; the
//! app's own screens are tested on a simulator elsewhere.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-core/tests/test_phone/mod.rs"]
mod test_phone;

use std::time::Duration;

use harlo_client::{Client, Endpoint, PhoneWanted};
use harlo_core::phone::{Identity, Offer};
use serde_json::{Value, json};
use test_phone::TestPhone;

/// How long a real model may take over one thing the phone waits for.
const PATIENCE: Duration = Duration::from_secs(240);

/// How long Harlo sends nothing before the phone's screen counts as settled.
const QUIET: Duration = Duration::from_secs(2);

fn named(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} isn't set: run tools/real/features.py"))
}

/// Reads what Harlo sends until `pick` finds what it waits for.
///
/// A real model can be quiet for longer than the stand-in phone waits for
/// one message, so each read is given up before that and tried again.
async fn wait<T>(
    phone: &mut TestPhone,
    what: &str,
    mut pick: impl FnMut(&TestPhone, &Value) -> Option<T>,
) -> T {
    if let Some(found) = pick(phone, &Value::Null) {
        return found;
    }
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "waited too long for {what}: {}",
            phone.text()
        );
        let read = tokio::time::timeout(test_phone::WAIT / 2, phone.next()).await;
        let message = read.unwrap_or(Value::Null);
        if let Some(found) = pick(phone, &message) {
            return found;
        }
    }
}

/// Reads what Harlo sends until it has sent nothing for a moment.
///
/// The last of an answer's text can reach the phone just after the word
/// that the run has ended.
async fn settled(phone: &mut TestPhone) {
    // (Nothing at all is read once the connection has closed.)
    while tokio::time::timeout(QUIET, phone.next())
        .await
        .is_ok_and(|message| !message.is_null())
    {}
}

/// Phone access as this check wants it: on or off, on this machine only.
fn wanted(on: bool) -> PhoneWanted {
    PhoneWanted {
        on,
        relay: false,
        paused: false,
        port: 0,
        name: "Real check".to_owned(),
        hidden: Vec::new(),
        theme: Value::Null,
    }
}

/// Turns phone access on and pairs a phone with the server.
async fn paired(client: &Client) -> TestPhone {
    let state = client
        .want_phone(&wanted(true))
        .await
        .expect("phone access on");
    let port = state.port.expect("listening");
    let offer = Offer::decode(&client.pair_phone().await.expect("a code")).expect("an offer");
    let key = Identity::generate();
    let address = format!("ws://127.0.0.1:{port}/ws");
    test_phone::connect_to(&address, &key, &offer.key, &offer.code)
        .await
        .expect("pairs")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a server on a real model; tools/real/features.py runs it"]
async fn a_phone_follows_a_real_models_session_and_allows_its_edit() {
    let client = Client::new(Endpoint::new(
        named("HARLO_REAL_URL"),
        Some(named("HARLO_REAL_PASSWORD")),
    ));
    let session = named("HARLO_REAL_SESSION");
    let project = std::path::PathBuf::from(named("HARLO_REAL_PROJECT"));

    let mut phone = paired(&client).await;

    // The session is in the phone's list.
    let mut listed = false;
    for _ in 0..100 {
        let list = phone.call(json!({ "op": "sessions" })).await;
        listed = list["sessions"]
            .as_array()
            .is_some_and(|all| all.iter().any(|s| s["key"] == session.as_str()));
        if listed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(listed, "the session isn't in the phone's list");
    phone.call(json!({ "op": "open", "key": session })).await;

    // Sent from the phone: the agent wants to change a file, and asks.
    let asked = "Add a function double(x) to stats.py that returns x times two. Change only that file \
                 and don't run anything.";
    let sent = phone
        .call(json!({ "op": "send", "key": session, "text": asked }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let request = wait(&mut phone, "the approval", |_, m| {
        (m["ev"] == "request").then(|| m["request"].clone())
    })
    .await;
    println!("PHONE asked: {request}");
    let file = || std::fs::read_to_string(project.join("stats.py")).unwrap_or_default();
    assert!(
        !file().contains("def double"),
        "the file changed before the phone allowed it"
    );
    let answered = phone
        .call(
            json!({ "op": "answer", "key": session, "requestId": request["id"],
                      "answer": { "value": "Allow once" } }),
        )
        .await;
    assert_eq!(answered["ok"], true, "{answered}");

    // The phone follows the run to its end and reads the answer.
    let mut idle = false;
    wait(&mut phone, "the run to end", |p, m| {
        idle |= m["ev"] == "status" && m["status"] == "idle";
        (idle && file().contains("def double") && p.text().contains("markdown ")).then_some(())
    })
    .await;
    settled(&mut phone).await;
    println!("PHONE rows:\n{}", phone.text());
    assert!(
        phone.text().contains("user Add a function double"),
        "the phone's own message is shown"
    );

    // A side question from the phone, answered beside the session.
    let rows = phone.text();
    let sent = phone
        .call(json!({ "op": "send", "key": session,
                      "text": "/btw In one short line: which file did you just change?" }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let answer = wait(&mut phone, "the side answer", |_, m| {
        (m["ev"] == "jobs")
            .then(|| m["items"].as_array().cloned().unwrap_or_default())
            .and_then(|items| {
                items
                    .into_iter()
                    .find(|row| row["title"] == "Side question" && row["working"] == false)
            })
    })
    .await;
    settled(&mut phone).await;
    println!("PHONE aside: {}", answer["detail"]);
    assert!(
        answer["detail"]
            .as_str()
            .is_some_and(|a| a.contains("stats.py")),
        "{answer}"
    );
    assert_eq!(phone.text(), rows, "a side question joins no conversation");

    drop(phone);
    for device in client.phone_state(None).await.expect("state").devices {
        client.remove_phone(&device.id).await.expect("unpaired");
    }
    client
        .want_phone(&wanted(false))
        .await
        .expect("phone access off");
}
