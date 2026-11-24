//! Phone access hosted by the server: turned on through the API, a phone
//! pairs and approves an edit. After the server restarts, phone access is
//! back without any desktop asking, and the paired phone gets in again.

// Rust guideline compliant 2026-10-09

mod support;
#[path = "../../harlo-core/tests/test_phone/mod.rs"]
mod test_phone;

use harlo_client::{Effect, PhoneWanted, Rule};
use harlo_core::phone::{Identity, Offer};
use serde_json::json;
use support::Sandbox;

fn wanted() -> PhoneWanted {
    PhoneWanted {
        on: true,
        relay: false,
        paused: false,
        port: 0,
        name: "Test computer".to_owned(),
        hidden: Vec::new(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn phone_access_lives_in_the_server_and_survives_its_restart() {
    let mut sandbox = Sandbox::start().await;
    let client = sandbox.client();
    assert!(client.hosts_phone().await.expect("info"));
    let project = sandbox.project();
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("careful");

    let state = client.want_phone(&wanted()).await.expect("phone on");
    let port = state.port.expect("listening");
    let offer = Offer::decode(&client.pair_phone().await.expect("a code")).expect("an offer");
    let key = Identity::generate();
    let address = format!("ws://127.0.0.1:{port}/ws");
    let mut phone = test_phone::connect_to(&address, &key, &offer.key, &offer.code)
        .await
        .expect("pairs");
    let listed = client.phone_state(None).await.expect("state");
    assert_eq!(listed.devices.len(), 1, "{listed:?}");

    // The phone opens the session and sends the edit; the server asks it.
    let mut sessions = Vec::new();
    for _ in 0..100 {
        let list = phone.call(json!({ "op": "sessions" })).await;
        sessions = list["sessions"].as_array().cloned().unwrap_or_default();
        if sessions.iter().any(|s| s["key"] == session.id.as_str()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        sessions.iter().any(|s| s["key"] == session.id.as_str()),
        "{sessions:?}"
    );
    phone.call(json!({ "op": "open", "key": session.id })).await;
    let script = r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"from the phone"} ;; say edited"#;
    let sent = phone
        .call(json!({ "op": "send", "key": session.id, "text": script }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let request = phone
        .until("the approval", |_, m| {
            (m["ev"] == "request").then(|| m["request"].clone())
        })
        .await;
    let answered = phone
        .call(
            json!({ "op": "answer", "key": session.id, "requestId": request["id"],
                      "answer": { "value": "Allow once" } }),
        )
        .await;
    assert_eq!(answered["ok"], true, "{answered}");
    phone
        .until("edited", |p, _| {
            p.text().contains("markdown edited").then_some(())
        })
        .await;
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "from the phone\n"
    );
    drop(phone);

    // The server restarts; phone access comes back as it was wanted.
    sandbox.restart().await;
    let client = sandbox.client();
    let mut state = client.phone_state(None).await.expect("state");
    for _ in 0..100 {
        if state.port.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state = client.phone_state(None).await.expect("state");
    }
    let port = state.port.expect("listening again");
    assert_eq!(state.devices.len(), 1, "still paired");
    let address = format!("ws://127.0.0.1:{port}/ws");
    let mut phone = test_phone::connect_to(&address, &key, &offer.key, "")
        .await
        .expect("the paired phone gets in");
    // The bridge reaches the server a moment after listening.
    let mut list = json!(null);
    for _ in 0..100 {
        list = phone.call(json!({ "op": "sessions" })).await;
        if list["sessions"].is_array() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        list["sessions"]
            .as_array()
            .is_some_and(|l| l.iter().any(|s| s["key"] == session.id.as_str())),
        "{list}"
    );
}
