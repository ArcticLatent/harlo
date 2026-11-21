//! Open sessions: tabs, the history timeline, sending, and starting new sessions.
//!
//! Each test renders the real view headless, clicks or types into it, and
//! feeds the resulting messages back through `App::update`. History and send
//! results are injected as the messages their tasks would produce.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo::app::shortcut;
use harlo::{App, Message, Panel, ids};
use harlo_core::layout::Tabs;
use harlo_core::server::{Location, Update};
use harlo_core::timeline::Entry;
use harlo_core::workspace::{Event as ServerEvent, EventKind, ToolStatus};
use iced::keyboard::{self, Key, Modifiers, key};
use iced::widget::Id;
use iced::widget::text_editor::{Action, Edit};
use iced::{Point, mouse};
use support::{demo, interact, shows, ui};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";
/// "hi how are you" in the demo data.
const HI: &str = "ses_demo_00";

fn history() -> Vec<Entry> {
    vec![
        Entry::Prompt {
            key: "msg_1".into(),
            text: "Fix the port in the README".into(),
        },
        Entry::Thought {
            key: "msg_2/0".into(),
            summary: "Checking the connectivity notes".into(),
        },
        Entry::Tool {
            key: "msg_2/1".into(),
            label: "Edit".into(),
            detail: "SPEC.md".into(),
            status: ToolStatus::Completed,
            took: Some(Duration::from_secs(15)),
        },
        Entry::Answer {
            key: "msg_2/2".into(),
            text: "Changed the **port** to 8080.".into(),
        },
        Entry::Stopped {
            key: "msg_3".into(),
            outcome: "interrupted".into(),
        },
    ]
}

fn open(app: &mut App, title: &str) {
    let _ = interact(app, |ui| {
        ui.click(title).expect("the session row");
    });
}

fn server_event(kind: EventKind) -> Message {
    Message::Server(Update::Event(ServerEvent {
        id: "evt".into(),
        seq: None,
        time: 1_795_000_000_000.0,
        kind,
    }))
}

#[test]
fn opening_a_session_shows_its_history() {
    let mut app = demo::app();
    open(&mut app, "pi update");
    assert_eq!(app.tabs, [PI_UPDATE]);
    assert_eq!(app.active.as_deref(), Some(PI_UPDATE));
    assert!(shows(&app, "Loading…"));
    assert!(!shows(&app, "What are we working on?"));

    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(history())));
    assert_eq!(app.timeline_len(PI_UPDATE), Some(5));
    for text in [
        "Fix the port in the README",
        "Thought",
        "Checking the connectivity notes",
        "Edit",
        "SPEC.md",
        "15s",
        "Stopped",
        // The header: title, agent and model.
        "build",
        "model",
        "Changes",
        "Nothing to show for this session yet.",
        "Enter to send · Shift+Enter for a new line",
    ] {
        assert!(shows(&app, text), "shows {text:?}");
    }
    assert!(!shows(&app, "Loading…"));
}

#[test]
fn a_failed_or_empty_history_says_so() {
    let mut app = demo::app();
    open(&mut app, "pi update");
    let _ = app.update(Message::History(PI_UPDATE.into(), Err("timed out".into())));
    assert!(shows(&app, "Couldn't load this session: timed out"));

    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    assert!(shows(&app, "No messages yet."));
}

#[test]
fn tabs_switch_cycle_and_close() {
    let mut app = demo::app();
    open(&mut app, "pi update");
    open(&mut app, "hi how are you");
    assert_eq!(app.tabs, [PI_UPDATE, HI]);
    assert_eq!(app.active.as_deref(), Some(HI));

    // Reopening an open session switches to its tab.
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    assert_eq!(app.tabs, [PI_UPDATE, HI]);
    assert_eq!(app.active.as_deref(), Some(PI_UPDATE));

    // A new tab opens after the active one.
    let third = "ses_demo_05";
    let _ = app.update(Message::OpenSession(third.into()));
    assert_eq!(app.tabs, [PI_UPDATE, third, HI]);

    // Cycling wraps around both ways.
    let _ = app.update(Message::CycleTab { back: false });
    assert_eq!(app.active.as_deref(), Some(HI));
    let _ = app.update(Message::CycleTab { back: false });
    assert_eq!(app.active.as_deref(), Some(PI_UPDATE));
    let _ = app.update(Message::CycleTab { back: true });
    assert_eq!(app.active.as_deref(), Some(HI));

    // The active tab's × closes it; its left neighbour takes over at the end.
    let messages = interact(&mut app, |ui| {
        ui.click(Id::new(ids::TAB_CLOSE)).expect("the close button");
    });
    assert!(
        matches!(messages.as_slice(), [Message::CloseTab(id)] if id == HI),
        "{messages:?}"
    );
    assert_eq!(app.tabs, [PI_UPDATE, third]);
    assert_eq!(app.active.as_deref(), Some(third));

    let _ = app.update(Message::CloseActiveTab);
    let _ = app.update(Message::CloseActiveTab);
    assert!(app.tabs.is_empty());
    assert_eq!(app.active, None);
    assert!(shows(&app, "What are we working on?"));
}

#[test]
fn deleted_or_archived_sessions_lose_their_tabs() {
    let mut app = demo::app();
    open(&mut app, "pi update");
    open(&mut app, "hi how are you");
    let _ = app.update(server_event(EventKind::SessionArchived {
        session_id: HI.into(),
    }));
    assert_eq!(app.tabs, [PI_UPDATE]);
    assert_eq!(app.active.as_deref(), Some(PI_UPDATE));
    let _ = app.update(server_event(EventKind::SessionDeleted {
        session_id: PI_UPDATE.into(),
    }));
    assert!(app.tabs.is_empty());
    assert!(shows(&app, "What are we working on?"));
}

#[test]
fn enter_sends_and_a_failed_send_explains_why() {
    let mut app = demo::app();
    open(&mut app, "pi update");
    let messages = interact(&mut app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.typewrite("Also update the docs");
        let _ = ui.tap_key(key::Named::Enter);
    });
    assert!(
        matches!(messages.last(), Some(Message::Send)),
        "Enter sends: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::Compose(Action::Edit(Edit::Enter)))),
        "Enter doesn't add a new line"
    );

    let _ = app.update(Message::Sent(Err(
        "Couldn't send: the server is gone".into()
    )));
    assert!(shows(&app, "Couldn't send: the server is gone"));
    assert_eq!(
        app.composer.text().trim(),
        "Also update the docs",
        "the text stays to retry"
    );
}

#[test]
fn a_new_session_opens_in_a_tab_once_sent() {
    let mut app = demo::app();
    let _ = app.update(Message::NewSession(Some("/home/burce/code/harlo".into())));
    assert_eq!(app.new_folder.as_deref(), Some("/home/burce/code/harlo"));

    let mut session = demo::snapshot().sessions.remove(0);
    session.id = "ses_new".into();
    session.title = Some("A brand new session".into());
    session.directory = "/home/burce/code/harlo".into();
    session.project_id = "p_harlo".into();
    let _ = app.update(Message::Sent(Ok(Box::new(session))));
    assert_eq!(app.tabs, ["ses_new"]);
    assert_eq!(app.active.as_deref(), Some("ses_new"));
    assert!(app.composer.text().trim().is_empty(), "the composer clears");
    assert!(shows(&app, "A brand new session"));

    // Ctrl+N goes back to the start screen, keeping the tab.
    let _ = app.update(Message::NewSession(None));
    assert_eq!(app.active, None);
    assert_eq!(app.tabs, ["ses_new"]);
    assert_eq!(app.new_folder.as_deref(), Some("/home/burce/code/harlo"));
}

#[test]
fn the_folder_picker_lists_projects() {
    let mut app = demo::app();
    let first = app.projects[0].directory.clone();
    let messages = interact(&mut app, |ui| {
        let target = ui.find(Id::new(ids::FOLDER)).expect("the folder picker");
        let bounds = target.bounds();
        let _ = ui.click(Id::new(ids::FOLDER));
        // The menu opens under the picker; each entry is a line of label text
        // plus the picker's vertical padding. Entry 0 is the home folder.
        // The menu hovers what the pointer moves over.
        let entry = harlo::style::size::LABEL * 1.3 + 12.0;
        let position = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height + entry * 1.5,
        );
        ui.point_at(position);
        let _ = ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved { position })]);
        let _ = ui.simulate(iced_test::simulator::click());
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::PickFolder(Some(dir)) if *dir == first)),
        "{messages:?}"
    );
    assert_eq!(app.new_folder.as_deref(), Some(first.as_str()));
}

#[test]
fn shortcuts_map_to_messages() {
    let press = |key: Key, modifiers: Modifiers| {
        shortcut(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    let ctrl = Modifiers::CTRL;
    assert!(matches!(
        press(Key::Character("n".into()), ctrl),
        Some(Message::NewSession(None))
    ));
    assert!(matches!(
        press(Key::Character("w".into()), ctrl),
        Some(Message::CloseActiveTab)
    ));
    assert!(matches!(
        press(Key::Named(key::Named::Tab), ctrl),
        Some(Message::CycleTab { back: false })
    ));
    assert!(matches!(
        press(Key::Named(key::Named::Tab), ctrl | Modifiers::SHIFT),
        Some(Message::CycleTab { back: true })
    ));
    assert!(press(Key::Character("n".into()), Modifiers::empty()).is_none());
    assert!(press(Key::Character("q".into()), ctrl).is_none());
}

#[test]
fn the_send_button_waits_for_text() {
    let mut app = demo::app();
    let messages = interact(&mut app, |ui| {
        let _ = ui.click(Id::new(ids::SEND));
    });
    assert!(
        !messages.iter().any(|m| matches!(m, Message::Send)),
        "{messages:?}"
    );
    let _ = ui(&app)
        .find(Id::new(ids::SEND))
        .expect("the send button is shown");
}

#[test]
fn the_layout_follows_the_window() {
    let mut app = demo::app();
    // Projects with sessions join the saved folders after the user's own.
    let mut order = demo::order();
    order.push(demo::HOME.into());
    assert_eq!(app.layout().order, order);

    open(&mut app, "pi update");
    open(&mut app, "hi how are you");
    let _ = app.update(Message::ToggleSidebar);
    let _ = app.update(Message::OpenPanel(Panel::Notes));
    let _ = app.update(Message::ToggleMore(demo::HOME.into()));
    let layout = app.layout();
    assert!(!layout.sidebar);
    assert_eq!(layout.panel.as_deref(), Some("Notes"));
    assert_eq!(layout.expanded, [demo::HOME]);
    assert_eq!(
        layout.tabs.get("Local"),
        Some(&Tabs {
            open: vec![PI_UPDATE.into(), HI.into()],
            active: Some(HI.into()),
        })
    );
    assert_eq!(layout.order, order);
}

#[test]
fn saved_tabs_reopen_once_connected() {
    let mut layout = demo::layout();
    layout.sidebar = false;
    layout.panel = None;
    layout.tabs.insert(
        "Local".into(),
        Tabs {
            open: vec![PI_UPDATE.into(), "ses_gone".into(), HI.into()],
            active: Some(PI_UPDATE.into()),
        },
    );
    let mut app = App::new(Location::Shared, vec![Location::Shared], layout.clone());
    let _ = app.update(Message::Tick(demo::now()));
    assert!(!app.sidebar);
    assert_eq!(app.panel, None);
    assert!(app.tabs.is_empty(), "tabs wait for the server");
    assert_eq!(app.layout(), layout, "waiting tabs are still saved");

    let _ = app.update(demo::connected(demo::snapshot()));
    assert_eq!(
        app.tabs,
        [PI_UPDATE, HI],
        "sessions that are gone stay closed"
    );
    assert_eq!(app.active.as_deref(), Some(PI_UPDATE));
    assert!(shows(&app, "Loading…"), "the active tab loads its history");
}

#[test]
fn each_server_keeps_its_own_tabs() {
    let server = Location::Server {
        url: "http://127.0.0.1:4096".into(),
        password: None,
    };
    let mut app = App::new(
        Location::Shared,
        vec![Location::Shared, server.clone()],
        demo::layout(),
    );
    let _ = app.update(demo::connected(demo::snapshot()));
    open(&mut app, "pi update");

    let _ = app.update(Message::PickLocation(server));
    assert!(app.tabs.is_empty());
    assert_eq!(
        app.layout().tabs.get("Local").map(|t| t.open.clone()),
        Some(vec![PI_UPDATE.to_owned()]),
        "the local tabs are kept"
    );

    let _ = app.update(Message::PickLocation(Location::Shared));
    let _ = app.update(demo::connected(demo::snapshot()));
    assert_eq!(app.tabs, [PI_UPDATE], "and reopen on the way back");
}
