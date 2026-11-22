//! The pages over the window from the left rail: keyboard shortcuts, stats
//! across sessions, and the server's set-up with About.
//!
//! Loads are fed in as their results; the endpoints are tested in
//! `opencode-client`, and against a private server in `server_sheets.rs`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{Message, Range, SetUp, Sheet, SheetEvent};
use harlo_core::server::{
    Day, McpServer, McpStatus, ModelUsage, Plugin, PluginSource, Provider, Stats, Tokens, WebSearch,
};
use slint::ComponentHandle as _;
use slint::Model as _;
use slint::platform::{Key, WindowEvent};
use support::{demo, ui};

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
        }]),
        plugins: Err("server went away".into()),
    }
}

#[test]
fn the_shortcuts_page_lists_the_keys_and_closes() {
    let ui = ui(demo::app());
    ui.press("Keyboard shortcuts");
    assert_eq!(ui.app().sheets.open, Some(Sheet::Shortcuts));
    assert!(ui.shows("Ctrl+N"));
    assert!(ui.shows("Start a new session"));
    assert!(ui.shows("Ctrl+Shift+C"), "the terminal's keys too");

    // Esc closes it.
    let escape = slint::SharedString::from(Key::Escape);
    let window = ui.window.window();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: escape.clone(),
    });
    window.dispatch_event(WindowEvent::KeyReleased { text: escape });
    assert_eq!(ui.app().sheets.open, None);
    assert!(!ui.shows("Start a new session"));

    // So does its button, and the rail button again.
    ui.press("Keyboard shortcuts");
    ui.press("Close");
    assert_eq!(ui.app().sheets.open, None);
    ui.press("Keyboard shortcuts");
    ui.press("Keyboard shortcuts");
    assert_eq!(ui.app().sheets.open, None);
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
    assert!(ui.shows("Connected · OpenCode 2.3.2"));
    assert!(ui.shows("Loading…"));
    ui.send(Message::Sheet(SheetEvent::SetUp(1, Box::new(setup()))));
    assert!(ui.shows("Fake"));
    assert!(ui.shows("on"));
    assert!(ui.shows("Exa"));
    assert!(
        ui.shows("failed: no such command"),
        "an MCP server's failure"
    );
    assert!(ui.shows("Couldn't list them: server went away"));
    assert!(ui.shows("About"));
    assert!(ui.shows(&format!(
        "Harlo {}. A desktop window for coding agents run by OpenCode.",
        env!("CARGO_PKG_VERSION")
    )));
}

#[test]
fn plugins_show_where_they_come_from() {
    let ui = ui(demo::app());
    ui.press("Settings");
    let mut set_up = setup();
    set_up.plugins = Ok(vec![
        Plugin {
            id: "opencode.agent".into(),
            source: PluginSource::Builtin,
            failed: None,
        },
        Plugin {
            id: "lint".into(),
            source: PluginSource::Package {
                target: "@acme/lint".into(),
                version: Some("1.2.0".into()),
                outdated: true,
            },
            failed: None,
        },
        Plugin {
            id: "broken".into(),
            source: PluginSource::Local("/tmp/broken.ts".into()),
            failed: Some("syntax error".into()),
        },
    ]);
    ui.send(Message::Sheet(SheetEvent::SetUp(1, Box::new(set_up))));
    assert!(ui.shows("built in"));
    assert!(ui.shows("@acme/lint 1.2.0 · update available"));
    assert!(ui.shows("failed: syntax error"));
}
