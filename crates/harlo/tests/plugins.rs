//! Extensions' plugins: package plugins listed one by one with their
//! updates, built-in ones in a line, and Check for updates and Update asked
//! of a private `harlo-server` (scratch `HOME`/`XDG_*`, never the shared
//! service), which has no package plugins and no network to reach.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use harlo::app::{Message, PluginEvent, SetUp, SheetEvent};
use harlo_core::server::{Connected, Plugin, PluginSource, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{demo, ui};

/// A plugin loaded from `source`.
fn plugin(id: &str, source: PluginSource) -> Plugin {
    Plugin {
        id: id.to_owned(),
        source,
        failed: None,
    }
}

/// A package plugin at `version`, outdated or not.
fn package(target: &str, version: &str, outdated: bool) -> Plugin {
    plugin(
        target,
        PluginSource::Package {
            target: target.to_owned(),
            version: Some(version.to_owned()),
            outdated,
            updating: false,
        },
    )
}

#[test]
fn packages_show_their_updates_and_built_in_ones_are_counted() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.press("Extensions");
    ui.press("Plugins");
    let serial = ui.app().sheets.serial();
    let setup = SetUp {
        providers: Ok(Vec::new()),
        search: Ok(Vec::new()),
        search_choice: None,
        mcp: Ok(Vec::new()),
        plugins: Ok(vec![
            plugin("opencode.agent", PluginSource::Builtin),
            plugin("opencode.plan", PluginSource::Builtin),
            package("opencode-acme", "1.2.0", true),
            package("opencode-wakatime", "0.4.1", true),
            package("opencode-notes", "2.0.0", false),
            Plugin {
                failed: Some("syntax error".to_owned()),
                ..plugin("mine", PluginSource::Local("/home/u/mine".to_owned()))
            },
        ]),
    };
    ui.send(Message::Sheet(SheetEvent::SetUp(serial, Box::new(setup))));
    assert!(ui.shows("opencode-acme"));
    assert!(ui.shows("1.2.0 · update available"));
    assert!(ui.shows("2.0.0"));
    assert!(ui.shows("Failed: syntax error"));
    assert!(ui.shows("2 plugins that come with the server"));
    assert!(!ui.shows("opencode.agent"), "built-in ones aren't listed");
    assert!(ui.shows("Update all 2"));
    assert_eq!(ui.find("Update").len(), 2, "one per outdated package");
}

#[test]
fn checking_and_updating_ask_the_server() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let mut app = demo::new_app();
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);
    ui.send(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot: demo::snapshot(),
    }))));
    ui.press("Settings");
    ui.press("Extensions");
    ui.press("Plugins");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Check for updates")),
        "the plugins"
    );
    ui.press("Check for updates");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui
            .shows("Every package plugin is up to date.")),
        "{:?}",
        ui.app().sheets.plugins.error
    );

    // A package the server doesn't have can't be updated; it says why.
    ui.send(Message::Sheet(SheetEvent::Plugin(PluginEvent::Checked(
        Ok(vec![package("opencode-not-installed", "1.0.0", true)]),
    ))));
    ui.press("Update");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().sheets.plugins.error.is_some()),
        "the update failed"
    );
    assert!(ui.shows("Not done"));
}
