//! Settings → Extensions → Plugins: the list with a field to add one and
//! the plugins people have published, once they're asked for, and
//! a plugin's page: what it brings, what it asks for, what a person fills
//! in for it, and what can be done.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Plugin, PluginState, PublishedPlugin};
use lucide_icons::Icon;

use super::{PROJECT, code_pair, field, glyph, heading, int, note, pair};
use crate::app::{App, Field, Load};
use crate::ui::SheetRow;
use crate::window;

/// What the field that adds a plugin says while it's empty.
const SOURCE_HINT: &str = "https://github.com/someone/plugin, or a folder on the server's computer";

/// The kind of row with a name and a line under it.
const LINED: i32 = 13;

/// How many characters of a commit show: enough to tell two apart.
const COMMIT_SHOWN: usize = 10;

/// The shown plugin's page; `None` when the list shows, as part of the
/// section.
pub(super) fn page_rows(app: &App) -> Option<Vec<SheetRow>> {
    let page = &app.sheets.plugins;
    let plugin = page.plugin()?;
    let mut back = pair(&plugin.name, "A plugin on Harlo's server", 3);
    back.action = "All plugins".into();
    back.setting = window::ALL_PLUGINS;
    let mut rows = vec![back];
    outcome(app, &mut rows);
    if let Some(description) = &plugin.description {
        rows.push(note(description));
    }
    let (state, tone) = state_words(plugin);
    rows.push(pair("State", &state, tone));
    if let Some(why) = &plugin.event_error {
        rows.push(SheetRow {
            kind: LINED,
            name: "Its code failed when a turn last ended".into(),
            value: why.as_str().into(),
            ..SheetRow::default()
        });
    }
    asks_rows(plugin, &mut rows);
    settings_rows(plugin, &mut rows);

    brings_rows(plugin, &mut rows);

    rows.push(heading("Where it's from"));
    // (An address is long: it gets a line of its own under its name.)
    rows.push(SheetRow {
        kind: LINED,
        name: "Source".into(),
        value: plugin.src.as_str().into(),
        ..SheetRow::default()
    });
    let follows = plugin.version.as_deref().unwrap_or("The default branch");
    rows.push(pair("Follows", follows, 3));
    let commit: String = plugin.commit.chars().take(COMMIT_SHOWN).collect();
    let mut kept = code_pair("Kept at", &commit, 3);
    if plugin.state == PluginState::Missing {
        kept.action = "Fetch".into();
        kept.setting = window::INSTALL_PLUGIN;
    } else {
        kept.action = "Update".into();
        kept.setting = window::UPDATE_PLUGIN;
    }
    rows.push(kept);
    if let Some(release) = &plugin.release {
        rows.push(code_pair("Version", release, 3));
    }
    rows.push(note(
        "Harlo fetches a plugin only when you add or update it, never on its own.",
    ));

    rows.push(heading("On the server"));
    let mut used = if plugin.enabled {
        pair("In use", "On", 1)
    } else {
        pair("In use", "Turned off", 3)
    };
    used.action = if plugin.enabled {
        "Turn off"
    } else {
        "Turn on"
    }
    .into();
    used.setting = window::SWITCH_PLUGIN;
    used.choice = int(usize::from(!plugin.enabled));
    rows.push(used);
    let mut remove = if page.removing {
        pair("Remove", "Remove it from the server?", 4)
    } else {
        pair("Remove", "Takes it and what it brings away", 3)
    };
    remove.action = if page.removing {
        "Remove for good"
    } else {
        "Remove"
    }
    .into();
    remove.risky = true;
    remove.setting = window::REMOVE_PLUGIN;
    rows.push(remove);
    rows.push(note(
        "These change Harlo's server, for every app and phone that uses it.",
    ));
    Some(rows)
}

/// The plugins, each opening its page, and the field that adds one.
pub(super) fn list_rows(app: &App, rows: &mut Vec<SheetRow>) {
    let page = &app.sheets.plugins;
    rows.push(heading("Plugins"));
    rows.push(note(
        "Packages of skills, commands, agents, MCP servers and tools, for every session on this \
         server.",
    ));
    outcome(app, rows);
    match &page.list {
        Load::Idle | Load::Loading => rows.push(note("Loading…")),
        Load::Failed(why) => rows.push(pair("Couldn't list them", why, 2)),
        Load::Loaded(list) if list.is_empty() => rows.push(note("None yet.")),
        Load::Loaded(list) => {
            for (index, plugin) in list.iter().enumerate() {
                rows.push(SheetRow {
                    kind: PROJECT,
                    name: plugin.name.as_str().into(),
                    value: listed_words(plugin).into(),
                    glyph: glyph(Icon::Package),
                    setting: window::OPEN_PLUGIN,
                    choice: int(index),
                    ..SheetRow::default()
                });
            }
        }
    }
    if matches!(page.list, Load::Loaded(_)) {
        let mut add = field(Field::PluginSource, "Plugin", "", SOURCE_HINT);
        add.action = if page.busy { "Fetching…" } else { "Add" }.into();
        add.saves = false;
        rows.push(add);
        rows.push(note(
            "Adding one fetches its files. What would act for you (programs it starts, code \
             and commands it runs, what its agents may do unasked) waits for your yes.",
        ));
        published_rows(app, rows);
    }
}

/// The plugins people have published, each with a button that adds it,
/// or the button that asks for them.
fn published_rows(app: &App, rows: &mut Vec<SheetRow>) {
    let page = &app.sheets.plugins;
    let ask = |name: &str, value: &str, tone: i32, action: &str| {
        let mut row = pair(name, value, tone);
        row.action = action.into();
        row.setting = window::BROWSE_PLUGINS;
        row
    };
    let list = match &page.published {
        Load::Idle => {
            rows.push(ask(
                "Published plugins",
                "Asks GitHub for the list, from the server's computer",
                3,
                "Browse",
            ));
            return;
        }
        Load::Loading => {
            rows.push(pair("Published plugins", "Asking GitHub…", 3));
            return;
        }
        Load::Failed(why) => {
            rows.push(ask("Couldn't list them", why, 2, "Try again"));
            return;
        }
        Load::Loaded(list) => list,
    };
    rows.push(heading("Published on GitHub"));
    rows.push(note(
        "Anyone can publish a plugin by tagging a repository harlo-plugin. Harlo hasn't \
         reviewed these: read one before you add it, and what it asks for before you allow it.",
    ));
    if list.is_empty() {
        rows.push(note("None published yet."));
    }
    let added: &[Plugin] = match &page.list {
        Load::Loaded(list) => list,
        _ => &[],
    };
    for (at, plugin) in list.iter().enumerate() {
        rows.push(published_row(at, plugin, added));
    }
}

/// The row of the published plugin `plugin`, at place `at` in their list:
/// its repository, what it's for, and the button that adds it unless it's
/// among those `added`.
fn published_row(at: usize, plugin: &PublishedPlugin, added: &[Plugin]) -> SheetRow {
    let same = |src: &str| {
        let plain = |s: &str| {
            s.trim_end_matches('/')
                .trim_end_matches(".git")
                .to_lowercase()
        };
        plain(src) == plain(&plugin.src)
    };
    let is_added = added.iter().any(|a| same(&a.src));
    let stars = match plugin.stars {
        0 => String::new(),
        1 => "1 star".to_owned(),
        n => format!("{n} stars"),
    };
    let mut row = SheetRow {
        kind: LINED,
        name: plugin.name.as_str().into(),
        value: plugin.description.as_deref().unwrap_or(&plugin.src).into(),
        hint: if is_added {
            "Added".into()
        } else {
            stars.into()
        },
        glyph: glyph(Icon::Package),
        ..SheetRow::default()
    };
    if !is_added {
        row.action = "Add".into();
        row.setting = window::ADD_PUBLISHED_PLUGIN;
        row.choice = int(at);
    }
    row
}

/// What the shown plugin brings, by kind.
fn brings_rows(plugin: &Plugin, rows: &mut Vec<SheetRow>) {
    rows.push(heading("What it brings"));
    rows.push(pair("Skills", &plugin.skills.to_string(), 3));
    rows.push(pair("Commands", &plugin.commands.to_string(), 3));
    rows.push(pair("Agents", &plugin.agents.to_string(), 3));
    let servers = if plugin.mcp.is_empty() {
        "None".to_owned()
    } else {
        plugin.mcp.join(", ")
    };
    rows.push(pair("MCP servers", &servers, 3));
    let tools = if plugin.tools.is_empty() {
        "None".to_owned()
    } else {
        plugin.tools.join(", ")
    };
    rows.push(pair("Tools", &tools, 3));
    if !plugin.events.is_empty() {
        rows.push(pair("Runs by itself", "When a session's turn ends", 3));
    }
    rows.push(note(
        "Your own skills, commands, agents and MCP servers win over a plugin's of the same name.",
    ));
}

/// What the shown plugin asks for, a line each, and the button that allows
/// it.
fn asks_rows(plugin: &Plugin, rows: &mut Vec<SheetRow>) {
    if plugin.asks.is_empty() {
        return;
    }
    let waiting = plugin.state == PluginState::Waiting;
    if waiting {
        rows.push(heading("It asks to"));
        rows.push(note(
            "Each of these would act for you. Nothing of the plugin is used until you allow them.",
        ));
    } else {
        rows.push(heading("You allowed it to"));
        rows.push(note(
            "An update that changes any of these waits for you again.",
        ));
    }
    rows.extend(plugin.asks.iter().map(|ask| pair(ask, "", 3)));
    if waiting {
        let mut allow = pair("Allow these", "", 4);
        allow.action = "Allow".into();
        allow.setting = window::APPROVE_PLUGIN;
        rows.push(allow);
    }
}

/// What a person fills in for the shown plugin: a field each, and for a
/// secret that's filled in, only that it is.
fn settings_rows(plugin: &Plugin, rows: &mut Vec<SheetRow>) {
    if plugin.settings.is_empty() {
        return;
    }
    rows.push(heading("What you fill in"));
    for (at, setting) in plugin.settings.iter().enumerate() {
        if setting.secret && setting.set {
            let mut filled = pair(&setting.label, "Filled in", 1);
            filled.action = "Clear".into();
            filled.setting = window::CLEAR_PLUGIN_SETTING;
            filled.choice = int(at);
            rows.push(filled);
            continue;
        }
        let place = u8::try_from(at).unwrap_or(u8::MAX);
        let value = setting.value.as_deref().unwrap_or_default();
        let mut row = field(Field::PluginSetting(place), &setting.label, value, "");
        if setting.secret {
            row.action = "Save".into();
            row.saves = false;
            row.on = true;
        }
        rows.push(row);
    }
    rows.push(note(
        "Kept on Harlo's server for this plugin's code alone. A secret isn't shown again.",
    ));
}

/// How `plugin` stands, as its row in the list says it.
fn listed_words(plugin: &Plugin) -> String {
    let state = state_words(plugin).0;
    if plugin.state == PluginState::Active && plugin.event_error.is_some() {
        format!("{state}; its code failed when a turn last ended")
    } else {
        state
    }
}

/// How `plugin` stands, and its tone.
fn state_words(plugin: &Plugin) -> (String, i32) {
    match plugin.state {
        PluginState::Active => ("In use".to_owned(), 1),
        PluginState::Off => ("Turned off".to_owned(), 3),
        PluginState::Waiting => ("Waits for you to allow what it asks".to_owned(), 4),
        PluginState::Missing => ("Not fetched on this server yet".to_owned(), 4),
        PluginState::Failed => match plugin.error.as_deref() {
            Some(why) if !why.is_empty() => (format!("Can't be used: {why}"), 2),
            _ => ("Can't be used".to_owned(), 2),
        },
    }
}

/// What the last step did, or why it failed.
fn outcome(app: &App, rows: &mut Vec<SheetRow>) {
    let page = &app.sheets.plugins;
    if page.busy {
        rows.push(pair("", "Working…", 3));
    }
    if let Some(done) = &page.done {
        rows.push(pair("", done, 1));
    }
    if let Some(why) = &page.error {
        rows.push(pair("Not done", why, 2));
    }
}
