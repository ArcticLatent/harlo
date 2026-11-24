//! Settings → Phone: turning phone access on, pairing a stand-in phone with
//! the code shown, seeing it connected, and removing it.
//!
//! Phone access follows a private `harlo-server` with scratch folders and
//! keeps its key and paired phones in a scratch folder; it listens on a free
//! loopback port. The relay stays off: nothing leaves this computer.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use harlo::app::{Message, Section, SheetEvent};
use harlo_core::phone::{Identity, Offer};
use harlo_core::server::Location;
use server::Sandbox;
use support::tasks::Tasks;
use support::{Ui, demo, ui};
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// How long starting and pairing may take.
const WAIT: Duration = Duration::from_secs(30);

/// The switch labelled `label`, and whether it's on.
fn switch(ui: &Ui, label: &str) -> (i_slint_backend_testing::ElementHandle, bool) {
    let found = ui
        .find(label)
        .into_iter()
        .find(|e| e.accessible_checkable() == Some(true))
        .unwrap_or_else(|| panic!("no switch labelled {label:?}"));
    let on = found.accessible_checked() == Some(true);
    (found, on)
}

#[test]
fn a_phone_pairs_with_the_code_shown_and_is_removed() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let here = Location::Server {
        url: sandbox.url.clone(),
        password: Some(sandbox.password.clone()),
    };
    let mut app = demo::new_app();
    app.location = here.clone();
    app.locations = vec![here];
    app.phone.folder = Some(scratch.path().join("phone"));
    app.phone.port = 0;
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::Phone)));
    assert!(ui.shows("Turn on phone access to pair a phone."));
    assert!(ui.shows("No phones yet."));

    let (access, on) = switch(&ui, "Phone access");
    assert!(!on);
    access.invoke_accessible_default_action();
    assert!(ui.app().preferences.phone);
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().phone.running.is_some()),
        "phone access started: {:?}",
        ui.app().phone.error
    );
    assert!(ui.shows("Listening"));

    ui.press("Show a code");
    assert!(!ui.find("Pairing QR code").is_empty(), "the QR code shows");
    let offer = {
        let app = ui.app();
        let link = &app.phone.shown.as_ref().expect("a code").text;
        let offer = Offer::decode(link).expect("an offer");
        // The link opens Harlo's first address, as the phone app expects.
        if let Some(first) = offer.endpoints.first() {
            let page = first.replace("ws://", "http://").replace("/ws", "/#p=");
            assert!(link.starts_with(&page), "{link}");
        }
        offer
    };
    let port = ui
        .app()
        .phone
        .running
        .as_ref()
        .expect("running")
        .address()
        .port();

    // A phone scans it and pairs.
    let key = Identity::generate();
    let (_socket, ready) = tasks.block_on(async {
        let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .expect("Harlo answers phones");
        let hello = serde_json::json!({ "type": "hello", "key": key.public_b64(),
                                        "code": offer.code, "name": "Test phone" });
        socket
            .send(WsMessage::Text(hello.to_string().into()))
            .await
            .expect("hello");
        let Some(Ok(WsMessage::Text(ready))) = socket.next().await else {
            panic!("no answer");
        };
        // The phone stays connected while the test looks.
        let ready = serde_json::from_str::<serde_json::Value>(&ready).expect("JSON");
        (socket, ready)
    });
    assert_eq!(ready["type"], "ready", "{ready}");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().phone.connected.len() == 1),
        "the phone shows as connected"
    );
    assert!(ui.shows("Test phone"));
    assert!(ui.shows("Connected"));
    assert!(ui.app().phone.shown.is_none(), "the spent code goes");
    assert!(ui.find("Pairing QR code").is_empty());

    // With a phone paired, the rail's phone button pauses its notifications.
    let rail = || ui.button("Phone notifications");
    assert_eq!(rail().accessible_description().as_deref(), Some("On"));
    rail().invoke_accessible_default_action();
    let running = ui.app().phone.running.clone().expect("running");
    assert!(
        ui.app().preferences.phone_paused && running.paused(),
        "paused"
    );
    assert_eq!(rail().accessible_description().as_deref(), Some("Paused"));
    rail().invoke_accessible_default_action();
    assert!(
        !ui.app().preferences.phone_paused && !running.paused(),
        "resumed"
    );

    ui.press("Remove");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().phone.devices.is_empty()),
        "the phone is forgotten"
    );
    assert!(ui.shows("No phones yet."));
    assert!(!ui.find("Phone").is_empty(), "unpaired, it's Phone again");

    let (access, _) = switch(&ui, "Phone access");
    access.invoke_accessible_default_action();
    assert!(ui.app().phone.running.is_none(), "turned off, it stops");
    assert!(ui.shows("Turn on phone access to pair a phone."));
}
