//! Agents in the window: the picker with each agent's colour, description
//! and whether it leaves files alone, the read-only note on the chip and in
//! the session's header, and Tab in the composer to switch agents.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::{App, Message, Picker, ids};
use harlo_core::server::{Agent, Catalog, Delivery, Update};
use harlo_core::workspace::{Event, EventKind, Fetched, InboxItem};
use iced::keyboard::{Modifiers, key};
use iced::widget::Id;
use support::{demo, interact, shows};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn folder() -> String {
    demo::snapshot()
        .sessions
        .into_iter()
        .find(|s| s.id == PI_UPDATE)
        .map(|s| s.directory)
        .expect("a demo session")
}

fn agent(id: &str, name: &str, description: &str) -> Agent {
    Agent {
        id: id.into(),
        name: name.into(),
        description: Some(description.into()),
        primary: true,
        subagent: false,
        color: None,
        read_only: false,
    }
}

/// Build, Plan (read-only), a coloured Reviewer usable both ways, and a
/// subagent that a session can't run.
fn catalog() -> Catalog {
    Catalog {
        agents: vec![
            agent("build", "Build", "Writes the code."),
            Agent {
                read_only: true,
                ..agent("plan", "Plan", "Plans without editing.")
            },
            Agent {
                subagent: true,
                color: Some("#ff6b6b".into()),
                ..agent("reviewer", "Reviewer", "Reviews changes.")
            },
            Agent {
                primary: false,
                subagent: true,
                read_only: true,
                ..agent("explore", "Explore", "Searches the code.")
            },
        ],
        ..Catalog::default()
    }
}

/// The demo app with "pi update" open and its folder's catalog loaded.
fn open() -> App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let _ = app.update(Message::Catalog(folder(), Ok(Box::new(catalog()))));
    app
}

/// The app with a new session about to start in the demo folder.
fn starting() -> App {
    let mut app = open();
    let _ = app.update(Message::NewSession(Some(folder())));
    app
}

fn select(app: &mut App, agent: &str) {
    let _ = app.update(Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind: EventKind::AgentSelected {
            session_id: PI_UPDATE.into(),
            agent: agent.into(),
        },
    })));
}

fn tab(app: &mut App, modifiers: Modifiers) -> Vec<Message> {
    interact(app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let tab = key::Key::Named(key::Named::Tab);
        let _ = ui.simulate([iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: tab.clone(),
            modified_key: tab,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: iced::keyboard::Location::Standard,
            modifiers,
            repeat: false,
            text: None,
        })]);
    })
}

#[test]
fn the_picker_lists_session_agents_with_what_they_are_for() {
    let mut app = starting();
    let _ = app.update(Message::TogglePicker(Picker::Agent));
    for name in ["Build", "Plan", "Reviewer"] {
        assert!(shows(&app, name), "{name} is offered");
    }
    assert!(!shows(&app, "Explore"), "a subagent isn't run by a session");
    assert!(shows(&app, "read-only"), "Plan leaves files alone");
    // The highlighted agent's description shows under the list.
    assert!(shows(&app, "Writes the code."));
    let _ = app.update(Message::PickerMove(1));
    assert!(shows(&app, "Plans without editing."));
    assert!(!shows(&app, "Writes the code."));
}

#[test]
fn a_read_only_agent_is_marked_and_shown_by_name() {
    let mut app = open();
    select(&mut app, "build");
    assert!(!shows(&app, "read-only"));
    select(&mut app, "plan");
    assert!(shows(&app, "read-only"));
    assert!(shows(&app, "Plan"), "the agent by name, not its ID");
    assert!(!shows(&app, "plan"));
}

#[test]
fn tab_in_the_composer_switches_agents() {
    let mut app = starting();
    assert_eq!(app.current_agent().as_deref(), Some("build"));
    let messages = tab(&mut app, Modifiers::empty());
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::CycleAgent { back: false })),
        "{messages:?}"
    );
    assert_eq!(app.current_agent().as_deref(), Some("plan"));
    let _ = app.update(Message::CycleAgent { back: false });
    assert_eq!(app.current_agent().as_deref(), Some("reviewer"));
    let _ = app.update(Message::CycleAgent { back: false });
    assert_eq!(
        app.current_agent().as_deref(),
        Some("build"),
        "round again, past the subagent"
    );
    let messages = tab(&mut app, Modifiers::SHIFT);
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::CycleAgent { back: true }))
    );
    assert_eq!(app.current_agent().as_deref(), Some("reviewer"));
}

#[test]
fn opencodes_own_note_on_switching_isnt_shown_as_queued() {
    let mut app = open();
    select(&mut app, "plan");
    let item = |id: &str, kind: &str, text: &str| InboxItem {
        id: id.into(),
        kind: kind.into(),
        text: Some(text.into()),
        attachments: Vec::new(),
        skills: Vec::new(),
        delivery: Delivery::Steer,
    };
    let _ = app.update(Message::Fetched(Ok(Fetched::Inbox(
        PI_UPDATE.into(),
        vec![
            item(
                "msg_s",
                "synthetic",
                "<system-reminder>You are in Plan mode.",
            ),
            item("msg_u", "user", "Then plan the docs"),
        ],
    ))));
    assert!(shows(&app, "Then plan the docs"), "the user's own stays");
    assert!(!shows(&app, "<system-reminder>You are in Plan mode."));
}
