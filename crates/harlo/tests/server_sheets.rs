//! The stats and settings pages against a private `opencode serve` with
//! scratch `HOME`/`XDG_*` folders, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use harlo::app::{App, Load, Message};
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
