//! The stats and settings pages against a private `opencode serve` with
//! scratch `HOME`/`XDG_*` folders, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use i_slint_backend_testing::AccessibleRole;

use harlo::app::{Load, Message};
use harlo::model::Picker;
use harlo_core::server::{Connected, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// The window connected to `sandbox`.
fn window(sandbox: &Sandbox) -> Ui {
    let mut app = demo::new_app();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "OpenCode".into(),
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
    ui.press("Providers");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Fake")),
        "the scripted model's provider"
    );
    assert!(ui.shows("Providers"));
    ui.press("Extensions");
    assert!(ui.shows("MCP servers"));
    ui.press("Skills");
    assert!(ui.shows("How to add skills"));

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
    let mut app = demo::new_app();
    // The sandbox's own config folder, never the real one.
    app.opencode_folder = Some(folder.clone());
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "OpenCode".into(),
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

#[test]
fn settings_git_reads_scratch_git_and_turns_snapshots_off_in_opencodes_config() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let folder = sandbox.root().join("config/opencode");
    let gitconfig = sandbox.root().join("home/.gitconfig");
    std::fs::write(&gitconfig, "[user]\n\tname = Scratch Person\n").expect("a git config");
    let mut app = demo::new_app();
    // The sandbox's own configs, never the real ones.
    app.opencode_folder = Some(folder.clone());
    app.git_config = harlo_core::git::Global::new()
        .env("GIT_CONFIG_GLOBAL", gitconfig.to_string_lossy())
        .env("GIT_CONFIG_NOSYSTEM", "1");
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "OpenCode".into(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);

    ui.press("Settings");
    ui.press("Git");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Keep snapshots")
            && matches!(&ui.app().sheets.git, Load::Loaded(git) if git.name == "Scratch Person")),
        "git's name and OpenCode's snapshots"
    );
    assert!(
        ui.shows("OpenCode's default"),
        "no commit-message model yet"
    );
    assert!(matches!(
        ui.app().sheets.git_settings.snapshots,
        Load::Loaded(true)
    ));

    ui.find("Keep snapshots")
        .into_iter()
        .find(|e| e.accessible_role() == Some(AccessibleRole::Checkbox))
        .expect("the switch")
        .invoke_accessible_default_action();
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            ui.app().sheets.git_settings.snapshots,
            Load::Loaded(false)
        )),
        "turned off"
    );
    assert!(!harlo_core::config::snapshots(&folder).expect("read"));
    let text = std::fs::read_to_string(folder.join("opencode.json")).expect("read");
    assert!(text.contains("\"fake\""), "the rest of the config stays");

    // The commit-message model is Harlo's own.
    assert!(
        tasks.until(&ui, WAIT, |ui| !ui.app().models.is_empty()),
        "the models"
    );
    ui.press("Change");
    assert_eq!(ui.app().picker, Some(Picker::CommitModel));
    ui.fill("Filter models", "pickle");
    ui.press("Big Pickle");
    assert_eq!(ui.app().picker, None);
    assert_eq!(
        ui.app().preferences.commit_model.as_deref(),
        Some("opencode/big-pickle")
    );
    assert!(ui.shows("Big Pickle · opencode"));
    ui.press("Use the default");
    assert_eq!(ui.app().preferences.commit_model, None);
}
