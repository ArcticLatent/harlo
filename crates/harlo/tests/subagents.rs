//! The Subagents panel: a session's subagents are listed on the right, not
//! in the sidebar, and each opens its own session.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, Message, Panel};
use harlo_core::server::Location;
use slint::{ComponentHandle, Model};
use support::{demo, ui};

#[test]
fn a_sessions_subagents_show_in_their_panel_and_open_from_it() {
    let mut snapshot = demo::snapshot();
    let parent = snapshot.sessions[0].id.clone();
    let mut child = snapshot.sessions[0].clone();
    child.id = "ses_child_1".into();
    child.parent_id = Some(parent.clone());
    child.title = Some("Find the config".into());
    child.agent = Some("explore".into());
    snapshot.sessions.push(child);
    let app = App::new(Location::Shared, vec![Location::Shared], demo::layout());
    let ui = ui(app);
    ui.send(Message::Tick(demo::now()));
    ui.send(demo::connected(snapshot));
    ui.send(Message::OpenSession(parent.clone()));

    // Not in the sidebar.
    let state = ui.window.global::<harlo::ui::State>();
    let sidebar = || {
        let rows = state.get_sidebar_rows();
        (0..rows.row_count())
            .filter_map(|i| rows.row_data(i))
            .map(|row| (row.title.to_string(), row.selected))
            .collect::<Vec<_>>()
    };
    assert!(
        !sidebar()
            .iter()
            .any(|(title, _)| title == "Find the config")
    );

    ui.press("Subagents");
    assert!(ui.app().panel_shown(Panel::Subagents));
    ui.wait(support::SLIDE_MS);
    assert!(ui.shows("Explore"), "its agent");
    ui.press("Explore: Find the config");
    assert_eq!(ui.app().active.as_deref(), Some("ses_child_1"));
    // The parent's row stays marked, and the panel keeps the list.
    assert!(
        sidebar()
            .iter()
            .any(|(title, selected)| title == "hi how are you" && *selected),
        "{:?}",
        sidebar()
    );
    assert!(ui.shows("Explore: Find the config"));

    // A session without any says so.
    ui.send(Message::OpenSession("ses_demo_01".into()));
    assert!(ui.shows(
        "No subagents in this session. An agent starts one to work on part of a task apart."
    ));
}
