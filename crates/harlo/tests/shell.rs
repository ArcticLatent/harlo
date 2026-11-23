//! The window shell: what it shows on start, and how its controls respond.
//!
//! Each test opens the real window headless, presses or types into it, and
//! the messages it sends go through `App::update`.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo::app::{App, Message, Status};
use harlo::model::status_text;
use harlo_core::approvals::Shield;
use harlo_core::layout::Layout;
use harlo_core::server::{Location, Update};
use harlo_core::workspace::{Event as ServerEvent, EventKind, Snapshot};
use support::{demo, ui};

#[test]
fn start_shows_the_design_content() {
    let ui = ui(demo::app());
    for text in [
        "Harlo",
        "Add a project",
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
    assert!(ui.shows("No projects yet.\nAdd a folder with + Project."));
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

#[test]
fn escape_still_closes_menus_after_the_model_menu() {
    use slint::ComponentHandle;
    use slint::platform::WindowEvent;
    let ui = ui(demo::app());
    // The model menu's filter takes the keyboard, and is gone once it closes.
    ui.send(Message::TogglePicker(harlo::model::Picker::Model));
    assert!(ui.shows("Filter models"));
    drop(ui.driver.take_tasks());
    ui.send(Message::ClosePicker);
    // Its focus request goes to the window, as the runtime would carry it.
    let mut tasks = support::tasks::Tasks::new();
    let _ = tasks.until(&ui, Duration::from_millis(100), |_| false);
    ui.press("Permissions");
    assert!(ui.app().picker.is_some(), "the permissions menu is open");
    let escape = slint::SharedString::from(slint::platform::Key::Escape);
    let window = ui.window.window();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: escape.clone(),
    });
    window.dispatch_event(WindowEvent::KeyReleased { text: escape });
    ui.wait(50);
    assert!(ui.app().picker.is_none(), "a real Escape closes it");
}

#[test]
fn the_permissions_mode_is_named_beside_the_shield_unless_its_the_agents() {
    let mut app = demo::app();
    app.active = None;
    let ui = ui(app);
    let shield = || ui.button("Permissions").accessible_description();
    assert_eq!(
        shield().as_deref(),
        Some(""),
        "the agent's own rules go unnamed"
    );
    ui.send(Message::PickShield(Shield::Trusted));
    assert_eq!(shield().as_deref(), Some("Don't ask"));
    assert!(ui.shows("Don't ask"), "and the words show beside it");
    ui.send(Message::PickShield(Shield::Careful));
    assert_eq!(shield().as_deref(), Some("Ask before edits"));
}

#[test]
fn each_permissions_mode_says_what_it_means_under_its_name() {
    let mut app = demo::app();
    app.active = None;
    let ui = ui(app);
    ui.press("Permissions");
    for mode in Shield::CHOICES {
        let row = ui.button(mode.label());
        assert_eq!(
            row.accessible_description().as_deref(),
            Some(mode.detail()),
            "{}",
            mode.label()
        );
        assert!(
            ui.shows(mode.detail()),
            "{} shows its meaning",
            mode.label()
        );
    }
    assert!(ui.shows("Agent's own rules"));
}

#[test]
fn a_rail_button_shows_its_name_and_keys_when_the_pointer_rests_on_it() {
    use slint::ComponentHandle as _;
    let ui = ui(demo::app());
    // Drawn for sight only: the button already has the name for screen
    // readers, so the tip is read from the window's state.
    let tip = || {
        let tips = ui.window.global::<harlo::ui::Tips>();
        (tips.get_text().to_string(), tips.get_keys().to_string())
    };
    ui.hover("Settings");
    ui.wait(100);
    assert_eq!(tip().0, "", "not while the pointer only passes");
    ui.wait(500);
    assert_eq!(tip(), ("Settings".to_owned(), "Ctrl+,".to_owned()));
    // Pressing it puts the name away for as long as the pointer stays.
    // A click where the pointer rests: pressed and let go on the spot.
    ui.drag("Settings", (0.0, 0.0));
    ui.wait(600);
    assert_eq!(tip().0, "", "gone once pressed, and not back while resting");
    // Off the button, it goes.
    ui.hover("New session");
    ui.wait(50);
    assert_eq!(tip().0, "");
}

#[test]
fn the_avatar_picker_shows_six_and_keeps_the_one_chosen() {
    let ui = ui(demo::app());
    assert_eq!(
        ui.button("Your avatar").accessible_description().as_deref(),
        Some("Classic")
    );
    ui.press("Your avatar");
    assert!(ui.shows("Your avatar"), "the picker's heading");
    for name in [
        "Classic",
        "Beanie",
        "Glasses",
        "Headphones",
        "Jacket",
        "Scarf",
    ] {
        assert!(!ui.find(name).is_empty(), "{name} is offered");
    }
    ui.find("Scarf")
        .into_iter()
        .next()
        .expect("the scarf")
        .invoke_accessible_default_action();
    assert_eq!(ui.app().preferences.avatar, "scarf");
    assert_eq!(
        ui.button("Your avatar").accessible_description().as_deref(),
        Some("Scarf")
    );
}
