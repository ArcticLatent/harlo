//! Extensions' MCP servers: one's own server added to a private
//! `opencode serve` (scratch `HOME`/`XDG_*` folders, never the shared
//! service), connected, disconnected, turned off and removed; and a catalog
//! server added to a scratch config without a server, so nothing reaches
//! the network.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use harlo::app::{Load, McpView, Message};
use harlo_core::config;
use harlo_core::server::{Connected, McpStatus, Update, load};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// A stdio MCP server with no tools: enough to connect.
const PROBE: &str = r#"import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "probe", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": []}
    elif method in ("prompts/list", "resources/list", "resources/templates/list"):
        result = {method.split("/")[0]: []} if method != "resources/templates/list" else {"resourceTemplates": []}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": msg["id"],
                          "error": {"code": -32601, "message": "no"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
"#;

/// The probe's status as the server lists it.
fn status(ui: &Ui) -> Option<McpStatus> {
    match &ui.app().sheets.setup {
        Load::Loaded(setup) => setup
            .mcp
            .as_ref()
            .ok()?
            .iter()
            .find(|s| s.name == "probe")
            .map(|s| s.status.clone()),
        Load::Idle | Load::Loading | Load::Failed(_) => None,
    }
}

/// The shown server's tools: through scripts, OpenCode's Code Mode and
/// its default, or listed one by one, and back.
fn lists_and_scripts(ui: &Ui, tasks: &mut Tasks, folder: &std::path::Path) {
    // OpenCode's Code Mode is the default; its tools can be listed instead.
    assert!(ui.shows("Through scripts"));
    ui.press("List them");
    assert!(
        tasks.until(ui, WAIT, |ui| ui.shows("Its tools are listed one by one.")
            && ui.shows("Use scripts"))
    );
    assert!(!config::mcp_servers(folder).expect("config")[0].codemode);
    ui.press("Use scripts");
    assert!(tasks.until(ui, WAIT, |ui| {
        ui.shows("Its tools are reached through scripts.")
    }));
    assert!(config::mcp_servers(folder).expect("config")[0].codemode);
}

#[test]
fn ones_own_server_is_added_connected_turned_off_and_removed() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let probe = sandbox.root().join("probe.py");
    std::fs::write(&probe, PROBE).expect("probe.py");
    let folder = sandbox.root().join("config/opencode");

    let mut app = demo::new_app();
    app.opencode_folder = Some(folder.clone());
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "OpenCode".into(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot: tasks.block_on(load(&sandbox.client())).expect("snapshot"),
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);
    ui.press("Settings");
    ui.press("Extensions");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Add an MCP server")),
        "the section"
    );
    assert!(ui.shows("None yet."));

    ui.press("Add an MCP server");
    assert_eq!(ui.app().sheets.mcp.view, McpView::Add);
    assert!(ui.shows("Context7"), "the catalog");
    // Checked before it's written.
    ui.wheel("Ready to add", -40);
    ui.press("Add server");
    assert!(ui.shows("Give it a name first."));
    ui.fill("Name", "probe");
    ui.press("Add server");
    assert!(ui.shows("Type the command that starts it first."));
    ui.fill("Command", &format!("python3 '{}'", probe.display()));
    ui.press("Add server");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Added probe.")),
        "added: {:?}",
        ui.app().sheets.mcp.error
    );
    let saved = config::mcp_servers(&folder).expect("config");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "probe");

    // OpenCode starts it from its config.
    assert_eq!(ui.app().sheets.mcp.view, McpView::List);
    assert!(
        tasks.until(&ui, WAIT, |ui| status(ui) == Some(McpStatus::Connected)),
        "connected: {:?}\nsetup: {:?}\nserver: {:?}\nconfig: {}",
        status(&ui),
        ui.app().sheets.setup,
        tasks.block_on(sandbox.client().mcp_servers(&sandbox.project_str())),
        std::fs::read_to_string(folder.join("opencode.json")).unwrap_or_default()
    );
    ui.press("probe");
    assert!(ui.shows("Connected"));
    assert!(ui.shows(&format!("python3 {}", probe.display())));

    ui.press("Disconnect");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Disconnected.")),
        "{:?}",
        ui.app().sheets.mcp.error
    );
    assert!(
        tasks.until(&ui, WAIT, |ui| status(ui) != Some(McpStatus::Connected)),
        "{:?}",
        status(&ui)
    );
    ui.press("Connect");
    assert!(
        tasks.until(&ui, WAIT, |ui| status(ui) == Some(McpStatus::Connected)),
        "{:?} {:?}",
        status(&ui),
        ui.app().sheets.mcp.error
    );

    ui.press("Turn off");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Turned off.")),
        "{:?}",
        ui.app().sheets.mcp.error
    );
    assert!(config::mcp_servers(&folder).expect("config")[0].disabled);
    assert!(tasks.until(&ui, WAIT, |ui| ui.shows("Turn on")));
    ui.press("Turn on");
    assert!(tasks.until(&ui, WAIT, |ui| ui.shows("Turned on.")
        && ui.shows("Turn off")));
    assert!(!config::mcp_servers(&folder).expect("config")[0].disabled);

    lists_and_scripts(&ui, &mut tasks, &folder);

    ui.press("Remove");
    assert!(ui.shows("Remove it for every app using this server?"));
    ui.press("Remove for good");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Removed probe.")),
        "{:?}",
        ui.app().sheets.mcp.error
    );
    assert_eq!(ui.app().sheets.mcp.view, McpView::List);
    assert!(config::mcp_servers(&folder).expect("config").is_empty());
}

#[test]
fn a_catalog_server_is_added_to_the_config() {
    let mut tasks = Tasks::new();
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path().join("opencode");
    let mut app = demo::new_app();
    app.opencode_folder = Some(folder.clone());
    let ui = ui(app);
    ui.press("Settings");
    ui.press("Extensions");
    // Without a server the list can't load; the catalog still adds to the
    // config, which a server reads when it starts.
    ui.send(Message::Sheet(harlo::app::SheetEvent::Mcp(
        harlo::app::McpEvent::Add,
    )));
    assert!(ui.shows("DeepWiki"));
    assert!(ui.shows("Answers questions about public GitHub repositories."));
    // The Add on DeepWiki's line.
    let line = ui.find("DeepWiki")[0].absolute_position().y;
    ui.find("Add")
        .into_iter()
        .min_by(|a, b| {
            let off =
                |e: &i_slint_backend_testing::ElementHandle| (e.absolute_position().y - line).abs();
            off(a).total_cmp(&off(b))
        })
        .expect("an Add")
        .invoke_accessible_default_action();
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Added DeepWiki.")),
        "{:?}",
        ui.app().sheets.mcp.error
    );
    let saved = config::mcp_servers(&folder).expect("config");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "deepwiki");
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            ui.app().sheets.mcp.saved,
            Load::Loaded(_)
        ) && ui.shows("Added")),
        "shown as added"
    );
}
