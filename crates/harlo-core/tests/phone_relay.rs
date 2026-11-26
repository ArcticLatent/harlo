//! Phones reaching Harlo through a relay, and notifications sent through it.
//!
//! A stand-in relay on a loopback port speaks the relay's protocol: Harlo
//! hosts on `/host`, phones connect to `/ws?id=…`, and `POST /push` records
//! the notifications instead of sending them to Apple. Each test runs its
//! own `harlo-server` with scratch folders and the scripted model.

// Rust guideline compliant 2026-10-09

mod claude_stand_in;
#[path = "../../harlo-client/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt as _, StreamExt as _};
use harlo_client::{Delivery, Effect, Prompt, Rule};
use harlo_core::phone::{Identity, Phone, RelayStatus, Sealed, Settings, key_bytes, relay_id};
use harlo_core::server::Location;
use serde_json::{Value, json};
use support::{Sandbox, WAIT, wait_for};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// The stand-in relay's state.
#[derive(Default)]
struct Relay {
    hosts: Mutex<HashMap<String, mpsc::UnboundedSender<String>>>,
    phones: Mutex<HashMap<u64, mpsc::UnboundedSender<Option<String>>>>,
    next: AtomicU64,
    pushes: Mutex<Vec<Value>>,
}

async fn host(ws: WebSocketUpgrade, State(relay): State<Arc<Relay>>) -> axum::response::Response {
    ws.on_upgrade(move |socket| hosting(socket, relay))
}

/// A computer hosting: its hello, then frames to and from its phones.
async fn hosting(socket: WebSocket, relay: Arc<Relay>) {
    let (mut sink, mut stream) = socket.split();
    let Some(Ok(WsMessage::Text(hello))) = stream.next().await else {
        return;
    };
    let hello: Value = serde_json::from_str(&hello).unwrap_or_default();
    let id = relay_id(hello["secret"].as_str().unwrap_or_default());
    let answer = json!({ "type": "hosting", "id": id }).to_string();
    if sink.send(WsMessage::Text(answer.into())).await.is_err() {
        return;
    }
    let (tx, mut rx) = mpsc::unbounded_channel();
    relay.hosts.lock().expect("hosts").insert(id, tx);
    loop {
        tokio::select! {
            out = rx.recv() => {
                let Some(out) = out else { return };
                if sink.send(WsMessage::Text(out.into())).await.is_err() { return }
            }
            message = stream.next() => {
                let Some(Ok(message)) = message else { return };
                let WsMessage::Text(text) = message else { continue };
                let v: Value = serde_json::from_str(&text).unwrap_or_default();
                let c = v["c"].as_u64().unwrap_or(0);
                let phone = relay.phones.lock().expect("phones").get(&c).cloned();
                if let Some(phone) = phone {
                    let frame = (v["type"] == "data").then(|| v["d"].as_str().unwrap_or_default().to_owned());
                    let _ = phone.send(frame);
                }
            }
        }
    }
}

async fn phone_socket(
    ws: WebSocketUpgrade,
    Query(query): Query<HashMap<String, String>>,
    State(relay): State<Arc<Relay>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| {
        phone_link(socket, relay, query.get("id").cloned().unwrap_or_default())
    })
}

/// A phone connected to a computer: its frames to the host and back.
async fn phone_link(socket: WebSocket, relay: Arc<Relay>, id: String) {
    let Some(host) = relay.hosts.lock().expect("hosts").get(&id).cloned() else {
        return;
    };
    let c = relay.next.fetch_add(1, Ordering::SeqCst) + 1;
    let (tx, mut rx) = mpsc::unbounded_channel();
    relay.phones.lock().expect("phones").insert(c, tx);
    let _ = host.send(json!({ "type": "open", "c": c }).to_string());
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(Some(frame)) => if sink.send(WsMessage::Text(frame.into())).await.is_err() { break },
                // The host hung up on this phone.
                Some(None) | None => break,
            },
            message = stream.next() => match message {
                Some(Ok(WsMessage::Text(text))) => {
                    let _ = host.send(json!({ "type": "data", "c": c, "d": text.to_string() }).to_string());
                }
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
    let _ = host.send(json!({ "type": "close", "c": c }).to_string());
    let _ = sink.close().await;
}

async fn push(State(relay): State<Arc<Relay>>, Json(body): Json<Value>) -> Json<Value> {
    relay.pushes.lock().expect("pushes").push(body);
    Json(json!({ "ok": true }))
}

/// Starts the stand-in relay on a loopback port.
async fn start_relay() -> (Arc<Relay>, SocketAddr) {
    let relay = Arc::new(Relay::default());
    let router = Router::new()
        .route("/host", get(host))
        .route("/ws", get(phone_socket))
        .route("/push", post(push))
        .with_state(Arc::clone(&relay));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, router).await });
    (relay, address)
}

/// A phone that connected through the relay and paired.
struct TestPhone {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    sealed: Sealed,
    next_id: u64,
}

impl TestPhone {
    async fn call(&mut self, mut request: Value) -> Value {
        self.next_id += 1;
        request["id"] = json!(self.next_id);
        let frame = self.sealed.seal(&request);
        self.socket
            .send(Message::Text(frame.into()))
            .await
            .expect("send");
        loop {
            let Some(Ok(Message::Text(text))) = tokio::time::timeout(WAIT, self.socket.next())
                .await
                .expect("a reply in time")
            else {
                panic!("the relay hung up during {request}");
            };
            let message = self.sealed.open(&text).expect("opens");
            if message["id"] == self.next_id {
                return message;
            }
        }
    }
}

/// Starts phone access for the sandbox's server, keeping its files in
/// `folder`, with no Claude Code to ask for plan limits.
async fn start_phone(sandbox: &Sandbox, folder: &std::path::Path) -> Phone {
    start_phone_with(sandbox, folder, None).await
}

/// Starts phone access as [`start_phone`] does, with `claude` as the
/// Claude Code whose plan limits phones are shown.
async fn start_phone_with(
    sandbox: &Sandbox,
    folder: &std::path::Path,
    claude: Option<std::path::PathBuf>,
) -> Phone {
    Phone::start(Settings {
        claude: claude.map(Into::into),
        theme: serde_json::Value::Null,
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

/// Waits until Harlo is on the relay; its ID there.
async fn online(hosting: &harlo_core::phone::Relay) -> String {
    let mut status = hosting.status();
    tokio::time::timeout(WAIT, async {
        loop {
            if let RelayStatus::Online(id) = status.borrow_and_update().clone() {
                return id;
            }
            status.changed().await.expect("status");
        }
    })
    .await
    .expect("online in time")
}

/// Pairs the phone with `key` through the relay the offer names.
async fn pair_through(offer: &harlo_core::phone::Offer, key: &Identity) -> TestPhone {
    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("{}/ws?id={}", offer.relay, offer.relay_id))
            .await
            .expect("the relay answers");
    let hello = json!({ "type": "hello", "key": key.public_b64(), "code": offer.code, "name": "Relay phone" });
    socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .expect("hello");
    let Some(Ok(Message::Text(ready))) = socket.next().await else {
        panic!("no answer")
    };
    let ready: Value = serde_json::from_str(&ready).expect("JSON");
    assert_eq!(ready["type"], "ready", "{ready}");
    let channel = key.channel(&key_bytes(&offer.key).expect("key"));
    TestPhone {
        socket,
        sealed: Sealed::new(channel, ready["conn"].as_str().expect("conn").to_owned()),
        next_id: 0,
    }
}

/// Runs a prompt in `session` to its end: its answer saved and the session
/// no longer working.
async fn run_to_end(client: &harlo_client::Client, session: &str, text: &str) {
    let before = client.messages(session).await.expect("messages").len();
    client
        .prompt(session, &Prompt::new(text, Delivery::Steer))
        .await
        .expect("prompt");
    wait_for("the run to end", || async {
        let now = client.messages(session).await.ok()?.len();
        let active = client.active().await.ok()?;
        (now >= before + 2 && !active.iter().any(|id| id == session)).then_some(())
    })
    .await;
}

/// Paused, a finished run isn't sent but a test notification is; resumed,
/// the next finished run is sent again.
async fn paused_phones_hear_only_tests(
    phone: &Phone,
    relay: &Relay,
    client: &harlo_client::Client,
    session: &str,
) {
    let pushes = || relay.pushes.lock().expect("pushes").len();
    phone.set_paused(true);
    let before = pushes();
    run_to_end(client, session, "say again").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(pushes(), before, "nothing sent while paused");
    assert_eq!(phone.test_notification().await, Ok(1), "tests still go");

    phone.set_paused(false);
    let before = pushes();
    run_to_end(client, session, "say once more").await;
    wait_for("a notification after resuming", || async {
        (pushes() > before).then_some(())
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn phones_reach_harlo_through_a_relay_and_get_notifications() {
    let (relay, relay_address) = start_relay().await;
    let sandbox = Sandbox::start().await;
    let folder = tempfile::tempdir().expect("scratch folder");
    let phone = start_phone(&sandbox, folder.path()).await;

    // Harlo hosts on the relay; the offer names it.
    let hosting = phone
        .relay(&format!("http://{relay_address}"))
        .expect("a relay");
    let id = online(&hosting).await;
    let offer = phone.offer();
    assert_eq!(offer.relay, format!("ws://{relay_address}"));
    assert_eq!(offer.relay_id, id);

    // A phone pairs through the relay, sealed end to end.
    let key = Identity::generate();
    let mut p = pair_through(&offer, &key).await;
    assert_eq!(p.call(json!({ "op": "ping" })).await["pong"], true);

    // The app allows notifications, then closes.
    let registered = p
        .call(json!({ "op": "push", "token": "ABCDEF0123", "asks": true, "done": true }))
        .await;
    assert_eq!(registered["ok"], true, "{registered}");
    drop(p);
    wait_for("the phone gone", || async {
        phone.connected().is_empty().then_some(())
    })
    .await;

    // A session asks for approval: the closed app is told, sealed for it.
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("rules");
    std::fs::write(sandbox.project().join("e.txt"), "one\ntwo\n").expect("e.txt");
    let script =
        r#"tool edit {"path":"e.txt","oldString":"two","newString":"three"} ;; say edited"#;
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    let harlo = key_bytes(&phone.public_key()).expect("key");
    let opened = |kind: &str| {
        let pushes = relay.pushes.lock().expect("pushes").clone();
        pushes.into_iter().find_map(|p| {
            let content: Value = serde_json::from_slice(
                &key.channel(&harlo).open(p["payload"]["h"].as_str()?).ok()?,
            )
            .ok()?;
            (content["k"] == kind).then_some((p, content))
        })
    };
    let (asked, content) =
        wait_for("the approval's notification", || async { opened("ask") }).await;
    assert_eq!(asked["token"], "abcdef0123", "tokens are lowercase");
    assert_eq!(asked["payload"]["aps"]["category"], "approval");
    assert_eq!(asked["payload"]["aps"]["badge"], 1);
    assert!(
        !asked.to_string().contains("e.txt"),
        "the relay sees only the placeholder"
    );
    assert_eq!(content["s"], session.id.as_str());
    assert!(
        content["b"].as_str().unwrap_or_default().contains("edit"),
        "{content}"
    );

    // Allowed elsewhere, the run finishes: the phone hears that too.
    let waiting = wait_for("the request", || async {
        client
            .permissions(&session.id)
            .await
            .ok()
            .and_then(|l| l.into_iter().next())
    })
    .await;
    client
        .reply_permission(
            &session.id,
            &waiting.id,
            harlo_client::PermissionReply::Once,
            None,
        )
        .await
        .expect("allowed");
    let (_, done) = wait_for("the finished notification", || async { opened("done") }).await;
    assert_eq!(done["s"], session.id.as_str());

    // A test notification reaches it even while it's connected.
    assert_eq!(phone.test_notification().await, Ok(1));
    wait_for("the test notification", || async { opened("test") }).await;

    paused_phones_hear_only_tests(&phone, &relay, &client, &session.id).await;

    // Off the relay: offers stop naming it.
    drop(hosting);
    phone.leave_relay();
    assert!(phone.offer().relay.is_empty());
    tokio::time::sleep(Duration::from_millis(50)).await;
}

/// The phone hands its lock-screen activity over a moment after it sends a
/// message, and a quick answer is there first. The activity must end as
/// done, with the answer, not stay on the lock screen as still working.
#[tokio::test(flavor = "multi_thread")]
async fn an_activity_handed_over_after_a_quick_answer_ends_as_done() {
    let (relay, relay_address) = start_relay().await;
    let sandbox = Sandbox::start().await;
    let folder = tempfile::tempdir().expect("scratch folder");
    let phone = start_phone(&sandbox, folder.path()).await;
    let hosting = phone
        .relay(&format!("http://{relay_address}"))
        .expect("a relay");
    online(&hosting).await;
    let key = Identity::generate();
    let mut p = pair_through(&phone.offer(), &key).await;

    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    run_to_end(&client, &session.id, "say I am well, thanks.").await;

    let taken = p
        .call(json!({ "op": "activity", "key": session.id, "token": "AB12CD", "sandbox": false }))
        .await;
    assert_eq!(taken["ok"], true, "{taken}");
    let activity_pushes = || {
        let pushes = relay.pushes.lock().expect("pushes").clone();
        pushes
            .into_iter()
            .filter(|p| p["activity"] == true && p["token"] == "ab12cd")
            .collect::<Vec<_>>()
    };
    let end = wait_for("the activity's end", || async {
        activity_pushes()
            .into_iter()
            .find(|p| p["payload"]["aps"]["event"] == "end")
    })
    .await;
    let aps = &end["payload"]["aps"];
    assert_eq!(aps["content-state"]["phase"], "done", "{aps}");
    let stays = aps["dismissal-date"].as_i64().unwrap_or_default()
        - aps["timestamp"].as_i64().unwrap_or_default();
    assert!(stays > 60, "the answer stays to be read: {aps}");
    let harlo = key_bytes(&phone.public_key()).expect("key");
    let sealed = aps["content-state"]["sealed"].as_str().expect("sealed");
    let said: Value =
        serde_json::from_slice(&key.channel(&harlo).open(sealed).expect("opens")).expect("JSON");
    assert_eq!(said["now"], "I am well, thanks.");
    // Nothing told the lock screen it was still at work.
    assert!(
        activity_pushes()
            .iter()
            .all(|p| p["payload"]["aps"]["content-state"]["phase"] == "done"),
        "{:?}",
        activity_pushes()
    );
}

/// A plan limit that is used up is told to the phones whose app is
/// closed, once, sealed for each; a phone that turned those notifications
/// off hears nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_closed_app_is_told_when_a_plan_limit_is_used_up() {
    let (relay, relay_address) = start_relay().await;
    let sandbox = Sandbox::start().await;
    let folder = tempfile::tempdir().expect("scratch folder");
    let claude = claude_stand_in::write(folder.path(), 100);
    let phone = start_phone_with(&sandbox, folder.path(), Some(claude)).await;
    let hosting = phone
        .relay(&format!("http://{relay_address}"))
        .expect("a relay");
    online(&hosting).await;

    // Two phones allow notifications and close their apps; the second
    // wants none about limits.
    let key = Identity::generate();
    for (key, token, limits) in [
        (&key, "AAAA01", true),
        (&Identity::generate(), "BBBB02", false),
    ] {
        let mut p = pair_through(&phone.offer(), key).await;
        let registered = p
            .call(json!({ "op": "push", "token": token, "limits": limits }))
            .await;
        assert_eq!(registered["ok"], true, "{registered}");
    }
    wait_for("the phones gone", || async {
        phone.connected().is_empty().then_some(())
    })
    .await;

    // A third opens its app, which asks for the limits: they are read.
    let mut open = pair_through(&phone.offer(), &Identity::generate()).await;
    let first = open.call(json!({ "op": "limits" })).await;
    assert_eq!(first["here"]["accounts"], json!([]), "{first}");

    let harlo = key_bytes(&phone.public_key()).expect("key");
    let told = wait_for("the limit's notification", || async {
        let pushes = relay.pushes.lock().expect("pushes").clone();
        pushes.into_iter().find(|p| p["token"] == "aaaa01")
    })
    .await;
    assert_eq!(
        told["payload"]["aps"]["alert"]["body"],
        "A plan limit is used up"
    );
    let sealed = told["payload"]["h"].as_str().expect("sealed");
    let content: Value =
        serde_json::from_slice(&key.channel(&harlo).open(sealed).expect("opens")).expect("JSON");
    assert_eq!(content["k"], "limit");
    assert_eq!(content["s"], "claude:session");
    assert_eq!(content["t"], "Claude's session limit is used up");
    assert!(
        content["b"]
            .as_str()
            .unwrap_or_default()
            .starts_with("It resets in "),
        "{content}"
    );

    // Asked again, the limits are the same: nobody is told twice, and the
    // phone that wants no such notification got none.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let pushes = relay.pushes.lock().expect("pushes").clone();
    assert_eq!(pushes.len(), 1, "{pushes:#?}");
    drop(open);
}
