//! Settings → Extensions → Plugins against a private `harlo-server` with
//! scratch `HOME`/`XDG_*` folders: a plugin from a git repository in the
//! scratch folder is added, allowed, given its token, turned off and
//! removed, and one is added from the list of published plugins.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod server;
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use harlo::app::{Load, Message, PluginEvent};
use harlo_core::server::{Connected, PluginState, PublishedPlugin, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// The field that takes where a plugin comes from.
const SOURCE: &str = "Plugin";

/// The window connected to `sandbox`.
fn window(sandbox: &Sandbox) -> Ui {
    let mut app = demo::new_app();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    ui(app)
}

/// Runs git in `dir` with no one's own git settings.
fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git")
        .success();
    assert!(ok, "git {args:?}");
}

/// A plugin's repository in the sandbox's scratch folder: a skill, and
/// what waits for a yes: an MCP server, and code that reaches a host.
fn repository(sandbox: &Sandbox) -> PathBuf {
    let repo = sandbox.root().join("repos/helper");
    std::fs::create_dir_all(repo.join("skills/lint")).expect("folders");
    std::fs::write(
        repo.join("plugin.json"),
        r#"{ "description": "Helps with releases.", "version": "1.2.0",
             "mcp": { "helper-tools": { "type": "local", "command": ["true"] } },
             "tools": [{ "name": "shout", "description": "Says it louder." }],
             "settings": [{ "key": "token", "label": "API token", "secret": true }],
             "permissions": { "network": ["api.example.com"] } }"#,
    )
    .expect("the manifest");
    std::fs::write(
        repo.join("plugin.js"),
        "harlo.tool('shout', async (args) => String(args.text).toUpperCase());\n",
    )
    .expect("its code");
    std::fs::write(
        repo.join("skills/lint/SKILL.md"),
        "---\ndescription: Lints.\n---\nLint it.\n",
    )
    .expect("a skill");
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    repo
}

/// How the plugin named `name` stands in `ui`'s list, if it's there.
fn state(ui: &Ui, name: &str) -> Option<PluginState> {
    match &ui.app().sheets.plugins.list {
        Load::Loaded(list) => list.iter().find(|p| p.name == name).map(|p| p.state),
        _ => None,
    }
}

#[test]
fn a_plugin_is_added_allowed_turned_off_and_removed() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let repo = repository(&sandbox);
    let ui = window(&sandbox);

    ui.press("Settings");
    ui.press("Extensions");
    ui.press("Plugins");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("None yet.")),
        "the server's empty list"
    );

    // Something that isn't a plugin's source adds nothing and says why.
    ui.fill(SOURCE, "git@example.com:someone/thing.git");
    ui.press("Add Plugin");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Not done")),
        "the refusal shows"
    );
    assert!(ui.shows("None yet."));

    ui.fill(SOURCE, &repo.to_string_lossy());
    ui.press("Add Plugin");
    assert!(
        tasks.until(&ui, WAIT, |ui| state(ui, "helper")
            == Some(PluginState::Waiting)),
        "it's added, and waits"
    );
    assert!(ui.shows("Added helper. It waits for you to allow what it asks."));

    // Its page says how it stands, what it brings and what it asks for.
    ui.press("helper");
    assert!(ui.shows("Waits for you to allow what it asks"));
    assert!(ui.shows("Helps with releases."));
    assert!(ui.shows("It asks to"));
    assert!(ui.shows("Start the MCP server “helper-tools”: true"));
    assert!(ui.shows("helper-tools"));
    assert!(ui.shows("Run its code (plugin.js) in a sandbox, for its tools and commands"));
    assert!(ui.shows("Reach api.example.com over the network"));
    assert!(ui.shows("shout"), "its tool, by name");
    ui.press("Allow");
    assert!(
        tasks.until(&ui, WAIT, |ui| state(ui, "helper")
            == Some(PluginState::Active)),
        "allowed, it's in use"
    );
    assert!(ui.shows("helper is in use."));
    assert!(ui.shows("You allowed it to"));

    // What it asks a person to fill in: a secret is kept, not shown.
    ui.wheel("You allowed it to", -2);
    assert!(ui.shows("What you fill in"));
    ui.fill("API token", "s3cret");
    ui.press("Save API token");
    let said = |ui: &Ui, what: &str| ui.app().sheets.plugins.done.as_deref() == Some(what);
    assert!(
        tasks.until(&ui, WAIT, |ui| said(ui, "Saved API token.")
            && ui.shows("Filled in")),
        "the token is kept"
    );
    assert!(!ui.shows("s3cret"));
    ui.press("Clear");
    assert!(
        tasks.until(&ui, WAIT, |ui| said(ui, "Cleared API token.")
            && !ui.shows("Filled in")),
        "and cleared"
    );

    // The rest of its page is further down; what a step did is said at
    // its top.
    ui.wheel("What it brings", -40);
    assert!(ui.shows("1.2.0"), "the version it gives itself");
    ui.press("Update");
    assert!(
        tasks.until(&ui, WAIT, |ui| {
            ui.app().sheets.plugins.done.as_deref() == Some("helper is up to date.")
        }),
        "nothing newer to fetch"
    );
    ui.press("Turn off");
    assert!(
        tasks.until(&ui, WAIT, |ui| state(ui, "helper")
            == Some(PluginState::Off)),
        "turned off"
    );
    ui.press("Turn on");
    assert!(
        tasks.until(&ui, WAIT, |ui| state(ui, "helper")
            == Some(PluginState::Active)),
        "turned on again"
    );

    // Removing asks once.
    ui.press("Remove");
    assert!(ui.shows("Remove it from the server?"));
    assert_eq!(state(&ui, "helper"), Some(PluginState::Active));
    ui.press("Remove for good");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Removed helper.")
            && ui.shows("None yet.")),
        "removed, the list shows again"
    );
    assert!(!sandbox.root().join("data/harlo/plugins/helper").exists());
}

#[test]
fn a_published_plugin_is_added_from_the_list_once_its_asked_for() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let repo = repository(&sandbox);
    let ui = window(&sandbox);

    ui.press("Settings");
    ui.press("Extensions");
    ui.press("Plugins");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Published plugins")),
        "the list is offered, not fetched"
    );
    assert!(!ui.shows("Published on GitHub"));

    // The sandbox's server reaches no GitHub: asking says so, and can be
    // asked again.
    ui.press("Browse");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Couldn't list them")),
        "the failure shows"
    );
    assert!(ui.shows("Try again"));

    // The list as the server gives it, with the scratch repository for an
    // address.
    ui.send(Message::Plugins(PluginEvent::Published(Ok(vec![
        PublishedPlugin {
            name: "someone/helper".to_owned(),
            src: repo.to_string_lossy().into_owned(),
            description: Some("Helps with releases.".to_owned()),
            stars: 3,
            updated: None,
        },
    ]))));
    assert!(ui.shows("Published on GitHub"));
    assert!(ui.shows("someone/helper") && ui.shows("3 stars"));
    assert!(ui.shows("Helps with releases."));
    ui.press("Add: someone/helper");
    assert!(
        tasks.until(&ui, WAIT, |ui| state(ui, "helper")
            == Some(PluginState::Waiting)),
        "it's added like one typed in, and waits"
    );
    // Added, it's marked and can't be added twice.
    assert!(tasks.until(&ui, WAIT, |ui| ui.shows("Added")));
    assert!(ui.find("Add: someone/helper").is_empty());
}
