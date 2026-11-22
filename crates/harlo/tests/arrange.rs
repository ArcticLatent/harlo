//! How the window is arranged: the tool panel on the right, the side panels
//! making way in a narrow window, the edges the user drags, the terminals
//! under the center, and the center's tabs.
//!
//! Each test opens the real window headless and drives it as the user would:
//! rail buttons, tabs, and pointer drags on the edges.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{Message, Panel};
use harlo::fit;
use support::{demo, ui, ui_at};

/// Two sessions from the demo, and their titles.
const FIRST: (&str, &str) = ("ses_demo_00", "hi how are you");
const SECOND: (&str, &str) = ("ses_demo_03", "pi update");

#[test]
fn the_right_rail_shows_one_panel_at_a_time() {
    let ui = ui(demo::app());
    assert_eq!(ui.app().panel, None, "it starts with no panel");

    ui.press("Todos");
    assert!(ui.app().panel_shown(Panel::Todos));
    assert!(ui.shows("Close Todos"), "its header");

    // The next one takes its place.
    ui.press("Context");
    assert!(ui.app().panel_shown(Panel::Context));
    assert!(!ui.app().panel_shown(Panel::Todos));

    // The lit button closes it, and so does its header's close button.
    ui.press("Context");
    assert_eq!(ui.app().panel, None);
    assert!(!ui.shows("Close Context"));
    ui.press("Notes");
    ui.press("Close Notes");
    assert_eq!(ui.app().panel, None);
}

#[test]
fn a_panel_maximizes_and_comes_back() {
    let ui = ui(demo::app());
    ui.press("Changes");
    ui.press("Maximize");
    assert_eq!(ui.app().maximized, Some(Panel::Changes));
    assert!(ui.shows("Restore"));

    ui.press("Restore");
    assert_eq!(ui.app().maximized, None);
    assert!(ui.app().panel_shown(Panel::Changes), "back on the right");

    // Closing a maximized panel closes it outright.
    ui.press("Maximize");
    ui.press("Close Changes");
    assert_eq!(ui.app().maximized, None);
    assert_eq!(ui.app().panel, None);
}

#[test]
fn a_narrow_window_folds_the_panel_then_the_sidebar() {
    let ui = ui(demo::app());
    ui.press("Changes");
    assert!(ui.app().shown().sidebar.is_some());

    // 1,100 px leaves room for the sidebar beside the main area, not the panel too.
    ui.resize(1100.0);
    assert!(!ui.app().panel_shown(Panel::Changes));
    assert_eq!(ui.app().panel, Some(Panel::Changes), "still chosen");
    assert!(ui.app().shown().sidebar.is_some());
    assert!(ui.app().main_width() >= fit::MAIN_MIN);

    ui.resize(800.0);
    assert_eq!(ui.app().shown().sidebar, None);
    assert!(ui.app().sidebar, "still chosen");

    // Both come back when there's room.
    ui.resize(fit::WINDOW.0);
    assert!(ui.app().panel_shown(Panel::Changes));
    assert!(ui.app().shown().sidebar.is_some());
}

#[test]
fn a_panel_opened_without_room_stays_and_the_sidebar_makes_way() {
    let ui = ui_at(demo::app(), (1100.0, fit::WINDOW.1));
    ui.press("Files");
    assert!(ui.app().panel_shown(Panel::Files), "kept on screen");
    assert_eq!(ui.app().shown().sidebar, None, "the sidebar made way");

    // Opening the sidebar again keeps it instead, and the panel makes way.
    ui.press("Toggle sidebar");
    assert!(ui.app().shown().sidebar.is_some());
    assert!(!ui.app().panel_shown(Panel::Files));

    // Once both fit, neither is kept: narrowing folds the panel first again.
    ui.resize(fit::WINDOW.0);
    assert!(ui.app().panel_shown(Panel::Files));
    ui.resize(1100.0);
    assert!(!ui.app().panel_shown(Panel::Files));
    assert!(ui.app().shown().sidebar.is_some());
}

#[test]
fn dragging_the_edges_resizes_within_limits() {
    let ui = ui(demo::app());
    ui.press("Notes");

    ui.drag("Sidebar width", (40.0, 0.0));
    assert_eq!(ui.app().shown().sidebar, Some(fit::SIDEBAR + 40.0));
    ui.drag("Panel width", (-60.0, 0.0));
    assert_eq!(ui.app().shown().panel, Some(fit::PANEL + 60.0));

    // Neither gets narrower than its least, nor squeezes the main area.
    ui.drag("Sidebar width", (-500.0, 0.0));
    assert_eq!(ui.app().shown().sidebar, Some(fit::SIDEBAR_RANGE.0));
    ui.drag("Panel width", (-2000.0, 0.0));
    let widest = fit::WINDOW.0 - 2.0 * fit::RAIL - fit::SIDEBAR_RANGE.0 - fit::MAIN_MIN;
    assert_eq!(ui.app().shown().panel, Some(widest));
    assert!((ui.app().main_width() - fit::MAIN_MIN).abs() < 0.5);

    // The sizes are saved with the layout.
    let layout = ui.app().layout();
    assert_eq!(layout.sidebar_width, Some(220));
    assert_eq!(layout.panel_width, Some(696));
    assert_eq!(layout.panel.as_deref(), Some("Notes"));
}

#[test]
fn the_terminals_open_in_a_strip_under_the_center() {
    // Real shells, in a scratch folder named like the project.
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path().join("harlo");
    std::fs::create_dir(&folder).expect("the folder");
    let ui = ui(demo::app().terminal_shell("/bin/sh", folder.to_string_lossy()));
    assert!(!ui.app().terminals_shown());

    // The rail button starts the first terminal.
    ui.press("Terminal");
    assert!(ui.app().terminals_shown());
    assert_eq!(ui.tabs("harlo").len(), 1);

    // Another one opens beside it and is shown.
    ui.press("New terminal");
    let ids: Vec<u32> = ui.app().terminals.shells().iter().map(|s| s.id).collect();
    assert_eq!(ui.tabs("harlo").len(), 2);
    assert_eq!(
        ui.app().terminals.active().map(|s| s.id),
        ids.last().copied()
    );
    ui.tabs("harlo")[0].invoke_accessible_default_action();
    assert_eq!(
        ui.app().terminals.active().map(|s| s.id),
        ids.first().copied()
    );

    // Closing one shows its neighbour.
    ui.press("Close harlo");
    assert_eq!(ui.app().terminals.shells().len(), 1);
    assert!(ui.app().terminals.active().is_some());

    // The top edge drags taller.
    ui.drag("Terminal height", (0.0, -100.0));
    assert!((ui.app().terminal_height() - (fit::TERMINAL + 100.0)).abs() < 0.5);
    assert_eq!(ui.app().layout().terminal_height, Some(360));

    // Hiding keeps them for the rail button to bring back.
    ui.press("Hide terminals");
    assert!(!ui.app().terminals_shown());
    assert_eq!(ui.app().terminals.shells().len(), 1);
    ui.press("Terminal");
    assert!(ui.app().terminals_shown());
}

#[test]
fn the_center_has_a_tab_for_each_session() {
    let ui = ui(demo::app());
    assert_eq!(ui.tabs("New session").len(), 1, "the start screen's tab");

    ui.send(Message::OpenSession(FIRST.0.into()));
    ui.send(Message::OpenSession(SECOND.0.into()));
    assert!(
        ui.tabs("New session").is_empty(),
        "gone once a session shows"
    );
    assert_eq!(ui.tabs(FIRST.1).len(), 1);
    assert_eq!(ui.app().active.as_deref(), Some(SECOND.0));

    // Clicking a tab shows its session; Ctrl+Tab goes round.
    ui.tabs(FIRST.1)[0].invoke_accessible_default_action();
    assert_eq!(ui.app().active.as_deref(), Some(FIRST.0));
    ui.send(Message::CycleTab { back: false });
    assert_eq!(ui.app().active.as_deref(), Some(SECOND.0));
    ui.send(Message::CycleTab { back: false });
    assert_eq!(
        ui.app().active.as_deref(),
        Some(FIRST.0),
        "round from the end"
    );

    // Its close button closes it.
    ui.press(&format!("Close {}", FIRST.1));
    assert!(ui.tabs(FIRST.1).is_empty());
    assert_eq!(ui.app().active.as_deref(), Some(SECOND.0));
}
