//! Unread sessions: a run that ends while its session isn't on screen
//! marks its row in the sidebar ("New answer") until it's looked at.
//!
//! The demo's sessions and scripted server events; nothing real is reached.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::Message;
use harlo_core::server::Update;
use harlo_core::workspace::{Event, EventKind};
use support::{Ui, demo, ui};

/// The demo's newest session, and another.
const FIRST: &str = "ses_demo_00";
const SECOND: &str = "ses_demo_01";

fn event(kind: EventKind) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind,
    }))
}

/// A run of `session` that ends as `outcome`.
fn run(ui: &Ui, session: &str, outcome: &str) {
    ui.send(event(EventKind::Busy {
        session_id: session.into(),
    }));
    ui.send(event(EventKind::Done {
        session_id: session.into(),
        outcome: outcome.into(),
    }));
}

fn unread(ui: &Ui, session: &str) -> bool {
    ui.app().unread.contains(session)
}

#[test]
fn a_run_that_ends_elsewhere_marks_its_session_until_it_is_opened() {
    let ui = ui(demo::app());
    ui.send(Message::OpenSession(FIRST.to_owned()));
    assert!(!ui.shows("New answer"));

    // The session on screen is being read as it answers.
    run(&ui, FIRST, "succeeded");
    assert!(!unread(&ui, FIRST));

    // Another one answers meanwhile: its row says so.
    run(&ui, SECOND, "succeeded");
    assert!(unread(&ui, SECOND));
    assert_eq!(ui.find("New answer").len(), 1);

    // A failed run is news too; one the user stopped is not.
    ui.send(Message::OpenSession(SECOND.to_owned()));
    assert!(!unread(&ui, SECOND), "opening it reads it");
    assert!(!ui.shows("New answer"));
    run(&ui, FIRST, "stopped");
    assert!(!unread(&ui, FIRST));
    run(&ui, FIRST, "failed");
    assert!(unread(&ui, FIRST));

    // At work again, it has nothing to read yet.
    ui.send(event(EventKind::Busy {
        session_id: FIRST.into(),
    }));
    assert!(!unread(&ui, FIRST));
    assert!(!ui.shows("New answer"));
}

#[test]
fn an_answer_that_came_while_harlo_was_in_the_background_waits_for_it() {
    let ui = ui(demo::app());
    ui.send(Message::OpenSession(FIRST.to_owned()));
    ui.send(Message::Focused(false));

    // On screen, but nobody is looking at the window.
    run(&ui, FIRST, "succeeded");
    assert!(unread(&ui, FIRST));
    assert!(ui.shows("New answer"));

    // Back in front, with that session showing: it's read.
    ui.send(Message::Focused(true));
    assert!(!unread(&ui, FIRST));
    assert!(!ui.shows("New answer"));
}
