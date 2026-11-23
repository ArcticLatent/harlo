//! The stats and settings pages against a private `opencode serve` with
//! scratch `HOME`/`XDG_*` folders, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use harlo::app::{App, Load, Message};
use harlo::model::Picker;
use harlo_core::server::{Connected, Location, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// The window connected to `sandbox`.
fn window(sandbox: &Sandbox) -> Ui {
    let mut app = App::new(Location::Shared, vec![Location::Shared], demo::layout());
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    ui(app)
}

#[test]
fn settings_and_stats_load_from_the_server() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = window(&sandbox);

    ui.press("Settings");
    ui.press("Models");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Fake")),
        "the scripted model's provider"
    );
    assert!(ui.shows("Model providers"));
    ui.press("Tools for agents");
    assert!(ui.shows("Plugins"));

    ui.press("Stats");
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            ui.app().sheets.stats,
            Load::Loaded(_)
        )),
        "the stats load"
    );
    assert!(ui.shows("No sessions in this time."));
    ui.press("All time");
    assert!(tasks.until(&ui, WAIT, |ui| matches!(
        ui.app().sheets.stats,
        Load::Loaded(_)
    )));
}

#[test]
fn the_model_for_new_sessions_is_set_in_opencodes_config() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let folder = sandbox.root().join("config/opencode");
    let mut app = App::new(Location::Shared, vec![Location::Shared], demo::layout());
    // The sandbox's own config folder, never the real one.
    app.opencode_folder = Some(folder.clone());
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);

    ui.press("Settings");
    ui.press("General");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Fake Coder · fake")),
        "OpenCode's default"
    );
    let mut events = tasks
        .block_on(sandbox.client().events())
        .expect("the server's events");
    ui.press("Change");
    assert_eq!(ui.app().picker, Some(Picker::DefaultModel));
    ui.fill("Filter models", "pickle");
    assert!(ui.shows("Big Pickle"), "the models, over Settings");
    ui.press("Big Pickle");
    assert_eq!(ui.app().picker, None);

    // The server reloads its config and says so; Harlo hears each event as
    // the running app would, and shows the new default.
    let deadline = std::time::Instant::now() + WAIT;
    while !ui.shows("Big Pickle · opencode") && std::time::Instant::now() < deadline {
        let next = tasks.block_on(async {
            tokio::time::timeout(std::time::Duration::from_millis(300), events.next()).await
        });
        if let Ok(Some(Ok(event))) = next {
            ui.send(Message::Server(Update::Event(event)));
        }
        let _ = tasks.until(&ui, std::time::Duration::from_millis(100), |ui| {
            ui.shows("Big Pickle · opencode")
        });
    }
    assert!(ui.shows("Big Pickle · opencode"), "the new default");
    assert_eq!(
        harlo_core::config::model(&folder).expect("read").as_deref(),
        Some("opencode/big-pickle")
    );
    let text = std::fs::read_to_string(folder.join("opencode.json")).expect("read");
    assert!(text.contains("\"fake\""), "the rest of the config stays");
    assert_eq!(ui.app().sheets.model_error, None);
}

#[test]
fn another_servers_default_model_is_only_shown() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.press("General");
    assert!(ui.shows("Model"));
    assert!(ui.find("Change").is_empty(), "nothing to change here");
}
