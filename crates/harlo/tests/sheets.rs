//! The pages over the window from the left rail: stats across sessions,
//! and Settings with its sections (Harlo's own settings, the keyboard
//! shortcuts, the server's set-up, About) and its search.
//!
//! Loads are fed in as their results; the endpoints are tested in
//! `opencode-client`, and against a private server in `server_sheets.rs`.

// Rust guideline compliant 2026-10-09

mod support;

use std::collections::BTreeMap;

use harlo::app::{App, Autostart, Message, Range, Section, SetUp, Sheet, SheetEvent};
use harlo_core::layout::{Preferences, Tabs};
use harlo_core::server::Location;
use harlo_core::server::{
    Day, McpServer, McpStatus, ModelUsage, Provider, Stats, Tokens, WebSearch,
};
use slint::ComponentHandle as _;
use slint::Model as _;
use slint::platform::{Key, WindowEvent};
use std::time::Duration;

use i_slint_backend_testing::AccessibleRole;
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// How long a file write off the UI thread may take.
const WAIT: Duration = Duration::from_secs(5);

/// Presses and lets go of `key`, with Control held when `control` is set.
fn key(ui: &Ui, key: impl Into<slint::SharedString>, control: bool) {
    let window = ui.window.window();
    let control_key = slint::SharedString::from(Key::Control);
    if control {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: control_key.clone(),
        });
    }
    let text = key.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
    if control {
        window.dispatch_event(WindowEvent::KeyReleased { text: control_key });
    }
}

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

fn stats() -> Stats {
    Stats {
        sessions: 12,
        subagents: 3,
        prompts: 40,
        steps: 310,
        tokens: Tokens {
            input: 900_000,
            output: 120_000,
            ..Tokens::default()
        },
        cost: 4.5,
        active_days: 9,
        streak: 4,
        activity: vec![
            Day {
                date: "2026-11-20".into(),
                steps: 100,
            },
            Day {
                date: "2026-11-22".into(),
                steps: 210,
            },
        ],
        models: vec![ModelUsage {
            provider: "fake".into(),
            model: "big".into(),
            steps: 310,
            tokens: Tokens {
                input: 900_000,
                output: 120_000,
                ..Tokens::default()
            },
            cost: 4.5,
        }],
        ..Stats::default()
    }
}

fn setup() -> SetUp {
    SetUp {
        providers: Ok(vec![Provider {
            id: "fake".into(),
            name: "Fake".into(),
            enabled: true,
        }]),
        search: Ok(vec![WebSearch {
            id: "exa".into(),
            name: "Exa".into(),
        }]),
        mcp: Ok(vec![McpServer {
            name: "docs".into(),
            title: None,
            status: McpStatus::Failed("no such command".into()),
            integration: None,
        }]),
        plugins: Err("server went away".into()),
    }
}

#[test]
fn the_shortcuts_section_lists_the_keys_and_closes() {
    let ui = ui(demo::app());
    ui.press("Keyboard shortcuts");
    assert_eq!(ui.app().sheets.open, Some(Sheet::Settings));
    assert_eq!(ui.app().sheets.section, Section::Shortcuts);
    assert!(ui.shows("Ctrl+N"));
    assert!(ui.shows("Start a new session"));
    // The terminal's keys are further down.
    ui.wheel("Start a new session", -10);
    assert!(ui.shows("Ctrl+Shift+C"), "the terminal's keys too");

    // Esc closes it.
    key(&ui, Key::Escape, false);
    assert_eq!(ui.app().sheets.open, None);
    assert!(!ui.shows("Start a new session"));

    // So does its button, and the rail button again.
    ui.press("Keyboard shortcuts");
    ui.press("Close");
    assert_eq!(ui.app().sheets.open, None);
    ui.press("Keyboard shortcuts");
    ui.press("Keyboard shortcuts");
    assert_eq!(ui.app().sheets.open, None);

    // Settings reopens where it was left; from another section, the rail
    // button moves to the shortcuts.
    ui.press("Settings");
    assert_eq!(ui.app().sheets.section, Section::Shortcuts);
    ui.press("General");
    ui.press("Keyboard shortcuts");
    assert_eq!(ui.app().sheets.open, Some(Sheet::Settings));
    assert_eq!(ui.app().sheets.section, Section::Shortcuts);
}

#[test]
fn the_phone_rail_button_opens_the_phone_section() {
    let ui = ui(demo::app());
    ui.press("Phone");
    assert_eq!(ui.app().sheets.open, Some(Sheet::Settings));
    assert_eq!(ui.app().sheets.section, Section::Phone);
    // From another section, it moves to Phone; on Phone, it closes.
    ui.press("General");
    let rail = |ui: &support::Ui| {
        ui.find("Phone")
            .into_iter()
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .min_by(|a, b| a.absolute_position().x.total_cmp(&b.absolute_position().x))
            .expect("the rail's phone button")
    };
    rail(&ui).invoke_accessible_default_action();
    assert_eq!(ui.app().sheets.section, Section::Phone);
    rail(&ui).invoke_accessible_default_action();
    assert_eq!(ui.app().sheets.open, None);
}

#[test]
fn settings_list_their_sections_and_open_with_ctrl_comma() {
    let ui = ui(demo::app());
    key(&ui, ',', true);
    assert_eq!(ui.app().sheets.open, Some(Sheet::Settings));
    assert_eq!(ui.app().sheets.section, Section::General);
    // The list scrolls: the first sections show, then the last ones.
    for section in &Section::ALL[..6] {
        assert!(ui.shows(section.title()), "{} is listed", section.title());
    }
    ui.wheel("Models", -10);
    for section in &Section::ALL[Section::ALL.len() - 4..] {
        assert!(ui.shows(section.title()), "{} is listed", section.title());
    }
    assert!(ui.shows("Agents and models"), "a group heading");
    assert!(ui.shows("Reopen tabs"));

    ui.press("Server");
    assert_eq!(ui.app().sheets.section, Section::Server);
    assert!(ui.shows("Connected · OpenCode 2.3.2"));
    assert!(!ui.shows("Reopen tabs"), "only the section shown");

    key(&ui, ',', true);
    assert_eq!(ui.app().sheets.open, None);
}

#[test]
fn searching_settings_finds_rows_in_every_section() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.send(Message::Sheet(SheetEvent::SetUp(1, Box::new(setup()))));
    ui.fill("Search settings", "ctrl+w");
    assert_eq!(ui.app().sheets.query, "ctrl+w");
    assert!(ui.shows("Search results"));
    assert!(ui.shows("Close the tab"));
    assert!(!ui.shows("Reopen tabs"));

    ui.fill("Search settings", "exa");
    assert!(ui.shows("Exa"), "the server's set-up is searched too");
    assert!(ui.shows("Tools for agents"));

    ui.fill("Search settings", "nothing like this");
    assert!(ui.shows("No setting matches “nothing like this”."));

    // Picking a section ends the search.
    ui.press("General");
    assert_eq!(ui.app().sheets.query, "");
    assert!(ui.shows("Reopen tabs"));
    let query = ui.window.global::<harlo::ui::State>().get_settings_query();
    assert_eq!(query, "", "the field is cleared too");
}

#[test]
fn the_reopen_tabs_switch_turns_off_reopening() {
    let ui = ui(demo::app());
    ui.press("Settings");
    let (reopen, on) = switch(&ui, "Reopen tabs");
    assert!(on);
    reopen.invoke_accessible_default_action();
    assert!(!ui.app().preferences.reopen_tabs);
    assert!(!switch(&ui, "Reopen tabs").1, "the switch shows it");

    // Saved tabs stay closed when it's off, and reopen when it's on.
    let opened = |reopen_tabs: bool| {
        let mut layout = demo::layout();
        layout.tabs = BTreeMap::from([(
            Location::Shared.label(),
            Tabs {
                open: vec!["ses_demo_03".into()],
                active: Some("ses_demo_03".into()),
            },
        )]);
        let mut app = App::new(Location::Shared, vec![Location::Shared], layout).with_preferences(
            Preferences {
                reopen_tabs,
                ..Preferences::default()
            },
        );
        let _ = app.update(Message::Tick(demo::now()));
        let _ = app.update(demo::connected(demo::snapshot()));
        app.tabs.clone()
    };
    assert!(opened(false).is_empty());
    assert_eq!(opened(true), vec!["ses_demo_03".to_owned()]);
}

#[test]
fn the_stats_page_counts_and_changes_range() {
    let ui = ui(demo::app());
    ui.press("Stats");
    assert!(ui.shows("Counting…"));
    ui.send(Message::Sheet(SheetEvent::Stats(1, Ok(stats()))));
    assert!(ui.shows("310"), "model steps");
    assert!(ui.shows("1M"), "tokens, shortened");
    assert!(ui.shows("$4.50"));
    assert!(ui.shows("fake/big"));
    assert!(ui.shows("2026-11-22: 210 steps"), "a day's bar");
    let bars = ui.window.global::<harlo::ui::State>().get_stat_days();
    let slots: Vec<i32> = (0..bars.row_count())
        .filter_map(|i| bars.row_data(i))
        .map(|bar| bar.slot)
        .collect();
    assert_eq!(slots, [27, 29], "placed by date, the day between empty");
    assert!(ui.shows("Sessions leave out the 3 subagent sessions they started."));

    ui.press("All time");
    assert_eq!(ui.app().sheets.range, Range::All);
    // The first load's answer, arriving late, is dropped.
    let mut older = stats();
    older.steps = 5;
    ui.send(Message::Sheet(SheetEvent::Stats(1, Ok(older))));
    assert!(
        ui.shows("310"),
        "the shown stats stay until the new ones come"
    );
    ui.send(Message::Sheet(SheetEvent::Stats(2, Ok(Stats::default()))));
    assert!(ui.shows("No sessions in this time."));
}

#[test]
fn a_failed_count_says_why() {
    let ui = ui(demo::app());
    ui.press("Stats");
    ui.send(Message::Sheet(SheetEvent::Stats(
        1,
        Err("server went away".into()),
    )));
    assert!(ui.shows("Couldn't count: server went away"));
}

#[test]
fn settings_show_the_servers_set_up_and_about() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.press("Models");
    assert!(ui.shows("Loading…"));
    ui.send(Message::Sheet(SheetEvent::SetUp(1, Box::new(setup()))));
    assert!(ui.shows("Fake"));
    assert!(ui.shows("No sign-in needed"));
    ui.press("Tools for agents");
    // Opening it asks again.
    let serial = ui.app().sheets.serial();
    ui.send(Message::Sheet(SheetEvent::SetUp(serial, Box::new(setup()))));
    assert!(ui.shows("Exa"));
    // A server's row opens its page, and says how it stands.
    let docs = ui
        .find("docs")
        .into_iter()
        .next()
        .expect("the server's row");
    assert_eq!(
        docs.accessible_description().as_deref(),
        Some("Failed: no such command"),
        "an MCP server's failure"
    );
    assert!(ui.shows("Couldn't list them: server went away"));
    // The last section, at the end of the list.
    ui.wheel("Models", -10);
    ui.press("About");
    assert!(ui.shows(env!("CARGO_PKG_VERSION")));
    assert!(ui.shows("2.3.2"), "the server's version");
}

#[test]
fn start_at_login_writes_and_removes_the_autostart_entry() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path().join("autostart");
    let mut app = demo::app();
    // A scratch autostart folder, never the real one.
    app.autostart = Some(Autostart {
        folder: folder.clone(),
        program: "/opt/harlo/harlo".into(),
        on: false,
    });
    let ui = ui(app);
    let mut tasks = Tasks::new();
    ui.press("Settings");
    ui.press("General");
    let checked = |ui: &Ui| {
        ui.find("Start at login")
            .iter()
            .any(|e| e.accessible_checked() == Some(true))
    };
    assert!(!checked(&ui));

    let flip = |ui: &Ui| {
        for switch in ui.find("Start at login") {
            if switch.accessible_checked().is_some() {
                switch.invoke_accessible_default_action();
            }
        }
    };
    flip(&ui);
    assert!(tasks.until(&ui, WAIT, checked), "on once written");
    assert!(harlo_core::autostart::enabled(&folder));
    let entry = std::fs::read_to_string(folder.join("harlo.desktop")).expect("the entry");
    assert!(entry.contains("Exec=/opt/harlo/harlo\n"), "{entry}");

    flip(&ui);
    assert!(
        tasks.until(&ui, WAIT, |ui| !checked(ui)),
        "off once removed"
    );
    assert!(!harlo_core::autostart::enabled(&folder));
}

#[test]
fn start_at_login_is_left_out_without_an_autostart_folder() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.press("General");
    assert!(ui.shows("Reopen tabs"));
    assert!(!ui.shows("Start at login"));
}

#[test]
fn control_q_quits() {
    let ui = ui(demo::app());
    let tasks = Tasks::new();
    key(&ui, "q", true);
    assert!(
        ui.messages().iter().any(|m| matches!(m, Message::Quit)),
        "the shortcut"
    );
    let effects: Vec<_> = ui
        .driver
        .take_tasks()
        .into_iter()
        .filter_map(harlo::task::Task::into_stream)
        .flat_map(|stream| tasks.block_on(futures_util::StreamExt::collect::<Vec<_>>(stream)))
        .collect();
    assert!(
        effects
            .iter()
            .any(|a| matches!(a, harlo::task::Action::Effect(harlo::task::Effect::Quit))),
        "it quits: {effects:?}"
    );
}

#[test]
fn keeping_running_in_the_tray_shows_only_with_a_tray() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.press("General");
    assert!(!ui.shows("Keep running in the tray"), "no tray, no switch");

    let mut app = demo::app();
    app.tray = true;
    let ui = support::ui(app);
    ui.press("Settings");
    ui.press("General");
    let (_, on) = switch(&ui, "Keep running in the tray");
    assert!(on, "on by default");
    ui.send(Message::Sheet(SheetEvent::Switch(
        harlo::app::Setting::KeepInTray,
    )));
    assert!(!ui.app().preferences.keep_in_tray);
    assert!(!switch(&ui, "Keep running in the tray").1);
}
