//! Unread sessions: a run that ends while its session isn't on screen
//! marks its row in the sidebar ("New answer") until it's looked at. The
//! marks come back after a restart, with one for each answer that arrived
//! while Harlo was closed.
//!
//! The demo's sessions and scripted server events; nothing real is reached.

// Rust guideline compliant 2026-10-09

mod support;

use std::collections::BTreeMap;

use harlo::app::Message;
use harlo_core::layout::{Layout, Tabs};
use harlo_core::server::{Location, Update};
use harlo_core::workspace::{Event, EventKind, Snapshot};
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

/// When Harlo was last closed, in the tests that start it again
/// (milliseconds since the Unix epoch).
const CLOSED_AT: i64 = 1_800_000_000_000;

/// The demo's sessions, with `runs` having ended: each a session, how
/// long after [`CLOSED_AT`] in milliseconds (before it when negative),
/// and how.
fn snapshot_with(runs: &[(&str, i64, &str)]) -> Snapshot {
    let mut snapshot = demo::snapshot();
    for (id, after, outcome) in runs {
        let session = snapshot
            .sessions
            .iter_mut()
            .find(|s| s.id == *id)
            .expect("a demo session");
        #[expect(
            clippy::cast_precision_loss,
            reason = "a date in milliseconds, far below 2^52"
        )]
        let ended = (CLOSED_AT + after) as f64;
        session.idle = Some(ended);
        session.outcome = Some((*outcome).to_owned());
    }
    snapshot
}

/// Harlo started with `layout` saved last time, connected to a server
/// with `snapshot`.
fn started(layout: Layout, snapshot: Snapshot) -> Ui {
    let mut app = demo::new_app_with(layout);
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(demo::connected(snapshot));
    ui(app)
}

/// What a layout keeps for the demo's server.
fn kept(layout: &Layout) -> Tabs {
    layout
        .tabs
        .get(&Location::Local.label())
        .cloned()
        .unwrap_or_default()
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

#[test]
fn unread_marks_come_back_after_a_restart() {
    let ui = ui(demo::app());
    ui.send(Message::OpenSession(FIRST.to_owned()));
    run(&ui, SECOND, "succeeded");
    assert!(unread(&ui, SECOND));
    let layout = ui.app().layout();
    assert_eq!(kept(&layout).unread, [SECOND]);

    // Started again: the mark is there, and goes when the session is opened.
    let again = started(layout, demo::snapshot());
    assert!(unread(&again, SECOND));
    assert_eq!(again.find("New answer").len(), 1);
    again.send(Message::OpenSession(SECOND.to_owned()));
    assert!(!again.shows("New answer"));
    assert!(kept(&again.app().layout()).unread.is_empty());

    // A marked session deleted meanwhile leaves no mark behind.
    let mut layout = demo::layout();
    layout.tabs = BTreeMap::from([(
        Location::Local.label(),
        Tabs {
            unread: vec![SECOND.to_owned(), "ses_gone".to_owned()],
            ..Tabs::default()
        },
    )]);
    let pruned = started(layout, demo::snapshot());
    assert_eq!(kept(&pruned.app().layout()).unread, [SECOND]);
}

#[test]
fn an_answer_that_arrived_while_harlo_was_closed_is_marked_at_start() {
    let runs = [
        // Answered a minute after Harlo closed.
        (FIRST, 60_000, "succeeded"),
        // Stopped by the user, on the phone say: nothing new.
        (SECOND, 60_000, "interrupted"),
        // Answered, then set to work again: nothing to read yet.
        ("ses_demo_02", 90_000, "failed"),
        // Answered before Harlo closed, and seen then.
        ("ses_demo_03", -5_000, "succeeded"),
    ];
    let mut snapshot = snapshot_with(&runs);
    snapshot.active = vec!["ses_demo_02".to_owned()];
    let mut layout = demo::layout();
    layout.tabs = BTreeMap::from([(
        Location::Local.label(),
        Tabs {
            followed: CLOSED_AT,
            ..Tabs::default()
        },
    )]);

    let ui = started(layout, snapshot.clone());
    assert_eq!(ui.app().unread.len(), 1, "{:?}", ui.app().unread);
    assert!(unread(&ui, FIRST));
    assert_eq!(ui.find("New answer").len(), 1);
    let now = kept(&ui.app().layout());
    assert_eq!(now.unread, [FIRST]);
    assert_eq!(now.followed, CLOSED_AT + 90_000, "caught up");

    // Cut off and connected again with nothing new: no more marks.
    ui.send(demo::connected(snapshot.clone()));
    assert_eq!(ui.app().unread.len(), 1);

    // A server followed for the first time has nothing unread: all of it
    // would be.
    let first = started(demo::layout(), snapshot);
    assert!(first.app().unread.is_empty());
    assert!(!first.shows("New answer"));
    assert_eq!(kept(&first.app().layout()).followed, CLOSED_AT + 90_000);
}
