//! Tools for agents' MCP servers: the list, a server's page, and adding one
//! from the catalog or by hand.

// Rust guideline compliant 2026-10-09

use harlo_core::config::McpRun;
use harlo_core::mcp_catalog::{OFFERED, Offered, Reach};
use harlo_core::server::{McpServer, McpStatus};
use lucide_icons::Icon;

use super::{PROJECT, choice, glyph, heading, int, live_field, note, pair};
use crate::app::{App, Field, Load, McpView, SetUp, Setting, SettingChoice};
use crate::ui::SheetRow;
use crate::window;

/// The MCP servers: the list, one server's page, or adding one. `None`
/// when the list shows, as part of the section.
pub(super) fn page_rows(app: &App, setup: Option<&SetUp>) -> Option<Vec<SheetRow>> {
    match &app.sheets.mcp.view {
        McpView::List => None,
        McpView::Add => Some(add_rows(app)),
        McpView::Shown(name) => Some(server_rows(app, setup, name)),
    }
}

/// The servers, each opening its page, and Add.
pub(super) fn list_rows(app: &App, setup: &SetUp, rows: &mut Vec<SheetRow>) {
    rows.push(heading("MCP servers"));
    rows.push(note(
        "Tools from programs and services, shared by every app on this server.",
    ));
    outcome(app, rows);
    match &setup.mcp {
        Ok(servers) if servers.is_empty() => rows.push(note("None yet.")),
        Ok(servers) => {
            for (index, server) in servers.iter().enumerate() {
                rows.push(SheetRow {
                    kind: PROJECT,
                    name: server.title.as_deref().unwrap_or(&server.name).into(),
                    value: status_words(&server.status).0.into(),
                    glyph: glyph(Icon::Plug),
                    setting: window::OPEN_MCP,
                    choice: int(index),
                    ..SheetRow::default()
                });
            }
        }
        Err(why) => rows.push(note(&format!("Couldn't list them: {why}"))),
    }
    if app.global_config().is_some() {
        let mut add = pair("", "", 3);
        add.action = "Add an MCP server".into();
        add.setting = window::ADD_MCP;
        rows.push(add);
    } else {
        rows.push(note(
            "Add MCP servers in OpenCode's config on the server's computer.",
        ));
    }
}

/// The switch for Harlo's own tools, with how they stand.
pub(super) fn own_tools_rows(app: &App, rows: &mut Vec<SheetRow>) {
    rows.push(heading("Harlo's tools"));
    rows.push(super::switch(
        Setting::OwnTools,
        "Past sessions",
        "Agents can find and read earlier sessions while Harlo is open.",
        app.preferences.own_tools,
    ));
    if let Some(why) = app
        .own_tools_error
        .as_ref()
        .filter(|_| app.preferences.own_tools)
    {
        let label = if app.own_tools_unavailable() {
            "Not available on this server"
        } else {
            "Not offered"
        };
        rows.push(pair(label, why, 2));
    }
}

/// How `status` reads, and its tone.
fn status_words(status: &McpStatus) -> (String, i32) {
    match status {
        McpStatus::Connected => ("Connected".to_owned(), 1),
        McpStatus::Pending => ("Connecting…".to_owned(), 3),
        McpStatus::Disabled => ("Not connected".to_owned(), 3),
        McpStatus::Failed(why) if why.is_empty() => ("Failed".to_owned(), 2),
        McpStatus::Failed(why) => (format!("Failed: {why}"), 2),
        McpStatus::NeedsAuth(_) => ("Needs signing in".to_owned(), 2),
        McpStatus::Other(what) => (what.clone(), 3),
    }
}

/// What the last step did, or why it failed.
fn outcome(app: &App, rows: &mut Vec<SheetRow>) {
    let page = &app.sheets.mcp;
    if let Some(done) = &page.done {
        rows.push(pair("", done, 1));
    }
    if let Some(why) = &page.error {
        rows.push(pair("Not done", why, 2));
    }
}

/// A server's page: what it runs, how it stands, and what can be done.
fn server_rows(app: &App, setup: Option<&SetUp>, name: &str) -> Vec<SheetRow> {
    let page = &app.sheets.mcp;
    let server = setup
        .and_then(|setup| setup.mcp.as_ref().ok())
        .and_then(|servers| servers.iter().find(|s| s.name == name));
    let title = server.and_then(|s| s.title.as_deref()).unwrap_or(name);
    let mut back = pair(title, "An MCP server of the OpenCode server", 3);
    back.action = "All servers".into();
    back.setting = window::ALL_MCP;
    let mut rows = vec![back];
    outcome(app, &mut rows);

    match page.saved(name).map(|entry| &entry.run) {
        Some(McpRun::Local { command, .. }) => {
            rows.push(pair("Runs", &shell_words::join(command), 3));
        }
        Some(McpRun::Remote { url, .. }) => rows.push(pair("Address", url, 3)),
        None => rows.push(note(
            "It's set up in a project's opencode.json or by a plugin; change it there.",
        )),
    }
    if let Some(server) = server {
        connection_row(server, &mut rows);
    }
    rows.push(note(
        "Connecting and disconnecting last until the OpenCode server restarts; its config \
         decides what starts then.",
    ));

    if let Some(entry) = page.saved(name) {
        rows.push(heading("In OpenCode's config"));
        let mut starts = if entry.disabled {
            pair("Starts with OpenCode", "Turned off", 3)
        } else {
            pair("Starts with OpenCode", "On", 1)
        };
        starts.action = if entry.disabled {
            "Turn on"
        } else {
            "Turn off"
        }
        .into();
        starts.setting = window::TURN_OFF_MCP;
        starts.choice = int(usize::from(!entry.disabled));
        rows.push(starts);
        // OpenCode's Code Mode: agents call these tools from short scripts,
        // so their definitions don't fill every request.
        let mut scripts = if entry.codemode {
            pair("Agents use its tools", "Through scripts", 1)
        } else {
            pair("Agents use its tools", "Listed one by one", 3)
        };
        scripts.action = if entry.codemode {
            "List them"
        } else {
            "Use scripts"
        }
        .into();
        scripts.setting = window::MCP_SCRIPTS;
        scripts.choice = int(usize::from(!entry.codemode));
        rows.push(scripts);
        let mut remove = if page.removing {
            pair("Remove", "Remove it for every app using this server?", 4)
        } else {
            pair("Remove", "Takes it out of OpenCode's config", 3)
        };
        remove.action = if page.removing {
            "Remove for good"
        } else {
            "Remove"
        }
        .into();
        remove.risky = true;
        remove.setting = window::REMOVE_MCP;
        rows.push(remove);
        rows.push(note(
            "These change OpenCode's config, for every app using this server.",
        ));
    }
    rows
}

/// How `server` stands, with the step it can take.
fn connection_row(server: &McpServer, rows: &mut Vec<SheetRow>) {
    let (words, tone) = status_words(&server.status);
    let mut row = pair("Connection", &words, tone);
    match &server.status {
        McpStatus::Connected => {
            row.action = "Disconnect".into();
            row.setting = window::CONNECT_MCP;
            row.choice = 0;
        }
        McpStatus::Failed(_) | McpStatus::Disabled | McpStatus::Other(_) => {
            row.action = "Connect".into();
            row.setting = window::CONNECT_MCP;
            row.choice = 1;
        }
        McpStatus::NeedsAuth(_) if server.integration.is_some() => {
            row.action = "Sign in".into();
            row.setting = window::SIGN_IN_MCP;
        }
        McpStatus::NeedsAuth(_) => {
            rows.push(row);
            rows.push(note(&format!(
                "Sign in with opencode mcp auth {} where the OpenCode server runs.",
                server.name
            )));
            return;
        }
        McpStatus::Pending => {}
    }
    rows.push(row);
}

/// The catalog, then the form for a server of one's own.
fn add_rows(app: &App) -> Vec<SheetRow> {
    let page = &app.sheets.mcp;
    let mut back = pair(
        "Add an MCP server",
        "Saved in OpenCode's config, for every app using this server",
        3,
    );
    back.action = "All servers".into();
    back.setting = window::ALL_MCP;
    let mut rows = vec![back];
    // What a catalog server's Add did; the form's problems show by it.
    if let Some(done) = &page.done {
        rows.push(pair("", done, 1));
    }

    rows.push(heading("Ready to add"));
    for (index, offered) in OFFERED.iter().enumerate() {
        let added = page.saved(offered.name).is_some();
        let mut row = pair(
            offered.title,
            if added { "Added" } else { how(offered) },
            if added { 1 } else { 3 },
        );
        if !added {
            row.action = "Add".into();
            row.setting = window::ADD_OFFERED_MCP;
            row.choice = int(index);
        }
        rows.push(row);
        rows.push(note(offered.about));
    }

    rows.push(heading("Your own"));
    rows.push(live_field(Field::McpName, "Name", &page.name, "e.g. docs"));
    rows.push(choice(
        app,
        SettingChoice::McpKind,
        "It is",
        "A program on the server's computer, or a service at an address",
    ));
    rows.push(if page.local {
        live_field(
            Field::McpCommand,
            "Command",
            &page.command,
            "e.g. npx -y some-mcp-server",
        )
    } else {
        live_field(
            Field::McpUrl,
            "Address",
            &page.url,
            "https://example.com/mcp",
        )
    });
    let mut add = pair("", "", 3);
    add.action = "Add server".into();
    add.setting = window::SAVE_MCP;
    rows.push(add);
    if let Some(why) = &page.error {
        rows.push(pair("Not added", why, 2));
    }
    if matches!(page.saved, Load::Failed(_)) {
        rows.push(note(
            "OpenCode's config couldn't be read, so nothing can be added.",
        ));
    }
    rows
}

/// How an offered server connects, in a few words.
fn how(offered: &Offered) -> &'static str {
    match (offered.reach, offered.signs_in) {
        (Reach::Remote(_), true) => "Asks you to sign in",
        (Reach::Remote(_), false) => "No sign-in",
        (Reach::Local(_), _) => "Runs on the server's computer",
    }
}
