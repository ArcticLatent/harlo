//! Harlo offers its own tools to the agents of every folder it shows while
//! connected, and the switch in Tools for agents takes them away and back.
//!
//! Runs against a private `opencode serve` with scratch `HOME`/`XDG_*`
//! folders, never the shared service; Harlo's home folder is a scratch one.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use harlo::app::{App, Message};
use harlo_core::server::{Connected, Delivery, Location, Prompt, Update, load};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

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

/// The last tool result the scripted model got back.
fn last_result(sandbox: &Sandbox) -> String {
    let log = std::fs::read_to_string(sandbox.root().join("model.jsonl")).unwrap_or_default();
    let Some(last) = log
        .lines()
        .last()
        .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok())
    else {
        return String::new();
    };
    last["body"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .filter(|m| m["role"] == "tool")
        .find_map(|m| m["content"].as_str())
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn harlos_tools_reach_agents_while_switched_on() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let client = sandbox.client();
    let home = sandbox.root().join("home").display().to_string();

    // An earlier session to find, in the home folder.
    let earlier = tasks
        .block_on(client.create_session(&home))
        .expect("session");
    tasks
        .block_on(client.set_title(&earlier.id, "Plan the release"))
        .expect("title");

    let mut app = App::new(Location::Shared, vec![Location::Shared], demo::layout());
    app.home = home.clone();
    let _ = app.update(Message::Tick(demo::now()));
    let ui = ui(app);
    let snapshot = tasks.block_on(load(&client)).expect("snapshot");
    ui.send(Message::Server(Update::Connected(Arc::new(Connected {
        client: client.clone(),
        server: "OpenCode".into(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot,
    }))));
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().own_tools_offered() >= 1),
        "offered for the home folder"
    );

    // A session in the home folder calls them.
    let now = tasks
        .block_on(client.create_session(&home))
        .expect("session");
    let code = "return await tools.harlo.past_sessions({query: 'release'})";
    let script = format!(
        "tool execute {} ;; say done",
        serde_json::json!({ "code": code })
    );
    // The app's tasks, which answer the agent, run while this waits; the
    // first prompt can come before the tools are listed.
    let mut found = false;
    for _ in 0..10 {
        let before = sandbox.model_requests();
        tasks
            .block_on(client.prompt(&now.id, &Prompt::new(&script, Delivery::Queue)))
            .expect("prompt");
        tasks.until(&ui, WAIT, |_| sandbox.model_requests() >= before + 2);
        if last_result(&sandbox).contains("Plan the release") {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "the agent found it: {} ({:?})",
        last_result(&sandbox),
        ui.app().own_tools_error
    );
    assert!(last_result(&sandbox).contains(&earlier.id));

    // The switch takes them away, and back.
    ui.press("Settings");
    ui.press("Tools for agents");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Harlo's tools")),
        "the section loaded"
    );
    let (offer, on) = switch(&ui, "Past sessions");
    assert!(on);
    offer.invoke_accessible_default_action();
    assert!(!ui.app().preferences.own_tools);
    assert_eq!(ui.app().own_tools_offered(), 0);
    let (offer, on) = switch(&ui, "Past sessions");
    assert!(!on, "the switch shows it");
    offer.invoke_accessible_default_action();
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().own_tools_offered() >= 1),
        "offered again"
    );
}
