//! The window shell: what it shows on start, and how its controls respond.
//!
//! Each test opens the real window headless, presses or types into it, and
//! the messages it sends go through `App::update`.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo::app::{App, Message, Status};
use harlo::model::status_text;
use harlo_core::layout::Layout;
use harlo_core::server::{Location, Update};
use harlo_core::workspace::{Event as ServerEvent, EventKind, Snapshot};
use support::{demo, ui};

#[test]
fn start_shows_the_design_content() {
    let ui = ui(demo::app());
    for text in [
        "Harlo",
        "+ Project",
        "Local",
        "New session",
        "Ctrl+N",
        "ArcticLatent.github.io",
        "harlo",
        "burce",
        "31",
        "hi how are you",
        "4h",
        "Show 19 more",
        "What are we working on?",
        "Enter to start · Shift+Enter for a new line",
        "Changes",
        "Notes",
        "Home folder",
        "Attach files",
        "Send",
    ] {
        assert!(ui.shows(text), "{text}");
    }
    // Only the first twelve sessions are listed.
    assert!(!ui.shows("Older session 1"), "hidden until expanded");
}

#[test]
fn long_titles_keep_their_full_text() {
    // The row shortens the title on screen; its text is still the whole title.
    let ui = ui(demo::app());
    assert!(
        ui.shows("Write this project's specs: SPEC.md files that hold its intent between sessions")
    );
}

#[test]
fn show_more_lists_every_session_then_fewer_again() {
    let ui = ui(demo::app());
    ui.press("Show 19 more");
    let messages = ui.messages();
    assert!(
        matches!(messages.as_slice(), [Message::ToggleMore(folder)] if folder == demo::HOME),
        "{messages:?}"
    );
    assert!(ui.app().expanded.contains(demo::HOME));

    // Pressing it again, now saying "Show fewer", folds the list.
    ui.send(Message::ToggleMore(demo::HOME.into()));
    assert!(!ui.app().expanded.contains(demo::HOME));
    assert!(ui.shows("Show 19 more"));
}

#[test]
fn the_filter_narrows_the_list() {
    let ui = ui(demo::app());
    ui.fill("Filter sessions", "H");
    let messages = ui.messages();
    assert!(
        matches!(messages.as_slice(), [Message::Filter(text)] if text == "H"),
        "{messages:?}"
    );

    ui.send(Message::Filter("houdini".into()));
    assert!(ui.shows("You are a senior Houdini programmer. Write a SOP"));
    assert!(!ui.shows("hi how are you"));
    // Projects without a match drop out; the one with matches stays.
    assert!(!ui.shows("harlo"));
    assert!(ui.shows("burce"));
}

#[test]
fn the_sidebar_toggles() {
    let ui = ui(demo::app());
    ui.press("Toggle sidebar");
    assert!(!ui.app().sidebar);
    assert!(ui.shows("Ctrl+N"), "it slides away rather than vanishing");
    ui.wait(250);
    assert!(!ui.shows("Ctrl+N"));
    assert!(ui.shows("What are we working on?"));

    ui.press("Toggle sidebar");
    ui.wait(100);
    assert!(ui.shows("Ctrl+N"), "halfway in");
    ui.wait(150);
    assert!(ui.shows("Ctrl+N"));
}

#[test]
fn the_composer_accepts_text() {
    let ui = ui(demo::app());
    ui.fill("Ask, or describe what to build · Tab switches agents", "hi");
    let messages = ui.messages();
    assert!(
        !messages.is_empty() && messages.iter().all(|m| matches!(m, Message::Compose(_))),
        "{messages:?}"
    );
    assert_eq!(ui.app().composer.text(), "hi");
}

#[test]
fn before_connecting_the_sidebar_says_so() {
    let ui = ui(App::new(
        Location::Shared,
        vec![Location::Shared],
        Layout::default(),
    ));
    assert!(ui.shows("Connecting to OpenCode…"));

    ui.send(Message::Server(Update::Lost {
        reason: "can't reach the server".into(),
        retry_in: Duration::from_secs(4),
    }));
    assert!(ui.shows("Can't connect: can't reach the server."));

    ui.send(demo::connected(Snapshot::default()));
    assert!(ui.shows("No projects yet. Add a folder with + Project."));
}

#[test]
fn the_location_picker_shows_servers_and_the_connection() {
    let ui = ui(demo::app());
    ui.press("Location: Local");
    assert!(ui.app().picking_location);
    assert!(ui.shows(&format!(
        "Connected · OpenCode {}",
        harlo_core::OPENCODE_VERSION
    )));
    assert!(ui.shows("OpenCode's background service on this computer"));

    ui.press("Location: Local");
    assert!(!ui.app().picking_location);
}

#[test]
fn picking_another_server_switches_and_reconnects() {
    let server = Location::Server {
        url: "http://127.0.0.1:4096".into(),
        password: None,
    };
    let app = App::new(
        Location::Shared,
        vec![Location::Shared, server.clone()],
        demo::layout(),
    );
    let ui = ui(app);
    ui.send(demo::connected(demo::snapshot()));
    ui.send(Message::ToggleLocations);
    ui.press("127.0.0.1:4096");
    assert_eq!(ui.app().location, server);
    assert_eq!(ui.app().status, Status::Connecting { attempt: 1 });
    // The sidebar keeps the user's folders but drops the old server's sessions.
    assert!(!ui.shows("hi how are you"));
    assert!(ui.shows("ArcticLatent.github.io"));
}

#[test]
fn status_text_covers_every_state() {
    let pinned = harlo_core::OPENCODE_VERSION;
    assert_eq!(
        status_text(&Status::Connecting { attempt: 1 }),
        "Connecting…"
    );
    assert_eq!(
        status_text(&Status::Connecting { attempt: 3 }),
        "Reconnecting (attempt 3)…"
    );
    assert_eq!(
        status_text(&Status::Connected {
            version: "9.9.9".into()
        }),
        format!("Connected · OpenCode 9.9.9. Harlo is tested with {pinned}.")
    );
    assert_eq!(
        status_text(&Status::Lost {
            reason: "the server rejected the password".into(),
            retry_in: Duration::from_secs(2),
        }),
        "Can't connect: the server rejected the password. Trying again in 2 s."
    );
}

#[test]
fn live_events_update_the_sidebar() {
    let ui = ui(demo::app());
    let event = |kind| {
        Message::Server(Update::Event(ServerEvent {
            id: "evt".into(),
            seq: None,
            time: 1_795_000_000_000.0,
            kind,
        }))
    };
    ui.send(event(EventKind::SessionRenamed {
        session_id: "ses_demo_03".into(),
        title: "Renamed while open".into(),
    }));
    assert!(ui.shows("Renamed while open"));
    assert!(!ui.shows("pi update"));

    ui.send(event(EventKind::SessionDeleted {
        session_id: "ses_demo_00".into(),
    }));
    assert!(!ui.shows("hi how are you"));
    assert!(ui.shows("30"), "the count follows");
    assert!(ui.shows("Show 18 more"));
}

#[test]
fn clicking_a_session_opens_it() {
    let ui = ui(demo::app());
    ui.press("pi update");
    let messages = ui.messages();
    assert!(
        matches!(messages.as_slice(), [Message::OpenSession(id)] if id == "ses_demo_03"),
        "{messages:?}"
    );
    assert_eq!(ui.app().active.as_deref(), Some("ses_demo_03"));
    // Its tab says what it is.
    assert!(
        ui.find("pi update")
            .iter()
            .any(|e| e.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::Tab))
    );
}

#[test]
fn worktrees_group_their_sessions_under_the_project() {
    let mut snapshot = demo::snapshot();
    let worktree = "/home/burce/.worktrees/feature-x";
    snapshot
        .worktrees
        .insert("p_home".into(), vec![demo::HOME.into(), worktree.into()]);
    snapshot.sessions[0].directory = worktree.into();
    snapshot.sessions[0].title = Some("Working in the worktree".into());
    let app = App::new(Location::Shared, vec![Location::Shared], demo::layout());
    let ui = ui(app);
    ui.send(Message::Tick(demo::now()));
    ui.send(demo::connected(snapshot));
    assert!(ui.shows("feature-x"));
    assert!(ui.shows("Working in the worktree"));

    // The filter keeps the worktree heading when it has matches.
    ui.send(Message::Filter("worktree".into()));
    assert!(ui.shows("feature-x"));
    assert!(!ui.shows("hi how are you"));
}

#[test]
fn a_picker_opens_its_menu_and_picks() {
    let ui = ui(demo::app());
    ui.press("Home folder");
    assert!(ui.app().picker.is_some());
    assert!(ui.shows("Add project…"));
    // Escape closes it, through the window's keys.
    let key = harlo::keys::KeyPress::new("\u{1b}", false, false, false);
    assert!(matches!(
        harlo::window::window_key(&ui.app(), &key),
        Some(Message::ClosePicker)
    ));
    ui.send(Message::ClosePicker);
    assert!(!ui.shows("Add project…"));
}
