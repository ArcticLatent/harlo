//! Phone access hosted by the server: turned on through the API, a phone
//! pairs and approves an edit. After the server restarts, phone access is
//! back without any desktop asking, and the paired phone gets in again.

// Rust guideline compliant 2026-10-09

mod support;
#[path = "../../harlo-core/tests/test_phone/mod.rs"]
mod test_phone;

use std::sync::{Arc, Mutex};

use harlo_client::{Effect, PhoneWanted, Rule};
use harlo_core::phone::{Identity, Offer};
use serde_json::{Value, json};
use support::Sandbox;

fn wanted() -> PhoneWanted {
    PhoneWanted {
        on: true,
        relay: false,
        paused: false,
        port: 0,
        name: "Test computer".to_owned(),
        hidden: Vec::new(),
        theme: serde_json::Value::Null,
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

#[tokio::test(flavor = "multi_thread")]
async fn phones_draw_in_the_desktops_theme_and_pick_its_avatar() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let gruvbox = harlo_core::theme::bundled()
        .into_iter()
        .find(|t| t.id == "gruvbox-dark-medium")
        .expect("bundled");
    let light = harlo_core::theme::harlo(harlo_core::theme::Mode::Light);
    let theme = harlo_core::phone::theme::theme(
        &gruvbox,
        &light,
        harlo_core::theme::Look::default(),
        "dark",
        "glasses",
    );
    let state = client
        .want_phone(&PhoneWanted {
            theme: theme.clone(),
            ..wanted()
        })
        .await
        .expect("phone on");
    let port = state.port.expect("listening");
    let offer = Offer::decode(&client.pair_phone().await.expect("a code")).expect("an offer");
    let address = format!("ws://127.0.0.1:{port}/ws");
    let mut phone =
        test_phone::connect_to(&address, &Identity::generate(), &offer.key, &offer.code)
            .await
            .expect("pairs");

    // The phone gets the desktop's theme, as the phone app reads it.
    let got = phone.call(json!({ "op": "theme" })).await;
    assert_eq!(got["family"], "Gruvbox", "{got}");
    assert_eq!(got["mode"], "dark");
    assert_eq!(got["avatar"], "glasses");
    assert_eq!(
        got["dark"]["surface"]["background"],
        theme["dark"]["surface"]["background"]
    );
    assert!(
        got["light"]["syntax"]["base"]["keyword"].is_string(),
        "{got}"
    );

    // An avatar picked on the phone comes back to the desktop, once.
    let refused = phone
        .call(json!({ "op": "setAvatar", "avatar": "../x" }))
        .await;
    assert!(refused["error"].is_string(), "{refused}");
    let picked = phone
        .call(json!({ "op": "setAvatar", "avatar": "scarf" }))
        .await;
    assert_eq!(picked["avatar"], "scarf", "{picked}");
    let mut state = client.phone_state(None).await.expect("state");
    for _ in 0..50 {
        if state.avatar_pick.is_some() {
            break;
        }
        state = client
            .phone_state(Some(state.version))
            .await
            .expect("state");
    }
    let pick = state.avatar_pick.expect("the pick");
    assert_eq!(pick.name, "scarf");
    // The desktop takes it: its theme now has that avatar, and the pick goes.
    let taken = harlo_core::phone::theme::with_avatar(theme, "scarf");
    let state = client
        .want_phone(&PhoneWanted {
            theme: taken,
            ..wanted()
        })
        .await
        .expect("taken");
    assert_eq!(state.avatar_pick, None);
    let got = phone.call(json!({ "op": "theme" })).await;
    assert_eq!(got["avatar"], "scarf");
}

/// Waits until the phone's session list holds session `key`; returns it.
async fn listed(phone: &mut test_phone::TestPhone, key: &str) -> Value {
    for _ in 0..100 {
        let list = phone.call(json!({ "op": "sessions" })).await;
        if let Some(s) = list["sessions"]
            .as_array()
            .and_then(|l| l.iter().find(|s| s["key"] == key))
        {
            return s.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("{key} never listed");
}

/// The phone reads the session's folder after its agent changed `a.txt`:
/// the changes, the file's diff, the file, its search and the branches.
async fn reads_the_folder(phone: &mut test_phone::TestPhone, key: &str) {
    let changes = phone.call(json!({ "op": "changes", "key": key })).await;
    assert_eq!(changes["files"][0]["path"], "a.txt", "{changes}");
    assert_eq!(changes["files"][0]["status"], "M");
    let branch = changes["branch"].as_str().expect("a branch").to_owned();
    assert!(!branch.is_empty());
    let diff = phone
        .call(json!({ "op": "diff", "key": key, "path": "a.txt" }))
        .await;
    assert_eq!(
        (diff["added"].as_u64(), diff["removed"].as_u64()),
        (Some(1), Some(1)),
        "{diff}"
    );
    let found = phone
        .call(json!({ "op": "files", "key": key, "query": "a.t" }))
        .await;
    assert!(
        found["files"]
            .as_array()
            .is_some_and(|f| f.iter().any(|p| p == "a.txt")),
        "{found}"
    );
    let file = phone
        .call(json!({ "op": "file", "key": key, "path": "a.txt" }))
        .await;
    assert_eq!(file["lines"], json!(["beta"]), "{file}");
    let branches = phone.call(json!({ "op": "branches", "key": key })).await;
    assert_eq!(branches["current"], branch.as_str(), "{branches}");
    assert_eq!(branches["changed"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_reads_the_sessions_folder_and_undoes_a_turn() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("*", Effect::Allow)])
        .await
        .expect("trusted");
    let state = client.want_phone(&wanted()).await.expect("phone on");
    let offer = Offer::decode(&client.pair_phone().await.expect("a code")).expect("an offer");
    let address = format!("ws://127.0.0.1:{}/ws", state.port.expect("listening"));
    let mut phone =
        test_phone::connect_to(&address, &Identity::generate(), &offer.key, &offer.code)
            .await
            .expect("pairs");
    let key = session.id.as_str();
    listed(&mut phone, key).await;

    // The agent edits a file.
    phone.call(json!({ "op": "open", "key": key })).await;
    let script =
        r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"beta"} ;; say edited"#;
    let sent = phone
        .call(json!({ "op": "send", "key": key, "text": script }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    phone
        .until("edited", |p, _| {
            p.text().contains("markdown edited").then_some(())
        })
        .await;
    for _ in 0..100 {
        if listed(&mut phone, key).await["status"] == "idle" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    reads_the_folder(&mut phone, key).await;

    // What the agent did, and the conversation as Markdown.
    let actions = phone.call(json!({ "op": "actions", "key": key })).await;
    assert_eq!(actions["actions"][0]["kind"], "edit", "{actions}");
    assert_eq!(actions["changed"], json!(["a.txt"]));
    let markdown = phone.call(json!({ "op": "markdown", "key": key })).await;
    assert!(
        markdown["markdown"]
            .as_str()
            .is_some_and(|m| m.contains("edited")),
        "{markdown}"
    );

    // Approval modes.
    let modes = phone.call(json!({ "op": "modes", "key": key })).await;
    assert_eq!(modes["approval"], "yolo", "{modes}");
    let chosen = phone
        .call(json!({ "op": "setModes", "key": key, "approval": "always-ask" }))
        .await;
    assert_eq!(chosen["approval"], "always-ask", "{chosen}");

    // Pinned on phones.
    let pinned = phone
        .call(json!({ "op": "pin", "key": key, "pinned": true }))
        .await;
    assert_eq!(pinned["ok"], true, "{pinned}");
    assert_eq!(listed(&mut phone, key).await["pinned"], true);

    // Undo: what it would do, then edit and resend, which does it.
    let targets = phone.call(json!({ "op": "undoTargets", "key": key })).await;
    assert_eq!(targets["git"], true, "{targets}");
    let row = targets["targets"]
        .as_object()
        .and_then(|t| t.keys().next().cloned())
        .expect("the prompt");
    let plan = phone
        .call(json!({ "op": "undoPlan", "key": key, "row": row }))
        .await;
    assert_eq!(plan["restore"], json!(["a.txt"]), "{plan}");
    assert_eq!(
        (plan["later"].as_u64(), plan["done"].as_bool()),
        (Some(0), Some(false))
    );
    let edit = phone
        .call(json!({ "op": "branch", "key": key, "row": row, "fork": false }))
        .await;
    assert_eq!(edit["text"], script, "{edit}");
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "alpha\n"
    );
}

/// Serves a stand-in relay's `/push` on a free port, keeping what it's
/// handed; returns its address.
async fn stand_in_relay(pushed: Arc<Mutex<Vec<Value>>>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let app = axum::Router::new().route(
        "/push",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            if let Ok(mut list) = pushed.lock() {
                list.push(body);
            }
            async { axum::Json(json!({ "ok": true })) }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await });
    at
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_activity_follows_the_run_through_the_relay() {
    let pushed = Arc::new(Mutex::new(Vec::new()));
    let relay = stand_in_relay(Arc::clone(&pushed)).await;
    let sandbox = Sandbox::start_with_env(&[("HARLO_RELAY", &relay)]).await;
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    let state = client
        .want_phone(&PhoneWanted {
            relay: true,
            ..wanted()
        })
        .await
        .expect("phone on");
    let offer = Offer::decode(&client.pair_phone().await.expect("a code")).expect("an offer");
    let address = format!("ws://127.0.0.1:{}/ws", state.port.expect("listening"));
    let me = Identity::generate();
    let mut phone = test_phone::connect_to(&address, &me, &offer.key, &offer.code)
        .await
        .expect("pairs");
    let key = session.id.as_str();
    listed(&mut phone, key).await;

    // The app starts an activity as it sends a message.
    let started = phone
        .call(json!({ "op": "activity", "key": key, "token": "ab12cd34", "sandbox": true }))
        .await;
    assert_eq!(started["ok"], true, "{started}");
    let refused = phone
        .call(json!({ "op": "activity", "key": key, "token": "not hex", "sandbox": true }))
        .await;
    assert!(refused["error"].is_string(), "{refused}");
    let sent = phone
        .call(json!({ "op": "send", "key": key, "text": "say All done here." }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");

    // Updates go through the relay, and the end once the run is over.
    let end = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let found = pushed.lock().ok().and_then(|list| {
                list.iter()
                    .find(|p| p["payload"]["aps"]["event"] == "end")
                    .cloned()
            });
            if let Some(end) = found {
                return end;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("the activity ended");
    assert_eq!(end["activity"], true, "{end}");
    assert_eq!(
        (end["token"].as_str(), end["sandbox"].as_bool()),
        (Some("ab12cd34"), Some(true))
    );
    let content = &end["payload"]["aps"]["content-state"];
    assert_eq!(content["phase"], "done", "{end}");
    // What it says is sealed for this phone alone.
    let harlo = harlo_core::phone::key_bytes(&offer.key).expect("Harlo's key");
    let opened = me
        .channel(&harlo)
        .open(content["sealed"].as_str().expect("sealed"))
        .expect("opens");
    let said: Value = serde_json::from_slice(&opened).expect("JSON");
    assert_eq!(said["now"], "All done here.", "{said}");
}
