//! Desktop notifications: which events notify, when they don't, what they
//! say, and Settings → Notifications.
//!
//! The app here shows no real notifications; what it meant to show is read
//! from `App::last_notice`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, Field, Message, Notice, Section, SheetEvent};
use harlo::task::{Action, Effect};
use harlo_core::notifications::{Kind, local_minute};
use harlo_core::server::Update;
use harlo_core::workspace::{Event, EventKind, Permission};

use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// The demo's newest session, "hi how are you".
const SESSION: &str = "ses_demo_00";

fn event(kind: EventKind) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind,
    }))
}

/// A run of `session` that says `text`, then ends as `outcome`.
fn run(ui: &Ui, session: &str, text: &str, outcome: &str) {
    ui.send(event(EventKind::Busy {
        session_id: session.into(),
    }));
    ui.send(event(EventKind::TextEnded {
        session_id: session.into(),
        message_id: "msg_1".into(),
        ordinal: 0,
        text: text.into(),
    }));
    ui.send(event(EventKind::Done {
        session_id: session.into(),
        outcome: outcome.into(),
    }));
}

/// The demo, with the window in the background.
fn away() -> Ui {
    let ui = ui(demo::app());
    ui.send(Message::Focused(false));
    ui
}

fn last(ui: &Ui) -> Option<Notice> {
    ui.app().last_notice.clone()
}

#[test]
fn a_finished_run_notifies_with_its_last_words() {
    let ui = away();
    run(
        &ui,
        SESSION,
        "## Done\nAll  **tests**\npass: `cargo test`.",
        "succeeded",
    );
    assert_eq!(
        last(&ui),
        Some(Notice {
            kind: Kind::Finished,
            session_id: Some(SESSION.to_owned()),
            title: "Build finished".to_owned(),
            body: "hi how are you: Done All tests pass: cargo test.".to_owned(),
        })
    );
}

#[test]
fn a_failed_run_says_its_error_and_a_stopped_one_says_nothing() {
    let ui = away();
    ui.send(event(EventKind::Busy {
        session_id: SESSION.into(),
    }));
    ui.send(event(EventKind::StepFailed {
        session_id: SESSION.into(),
        message_id: "msg_1".into(),
        error: "rate limited".into(),
    }));
    ui.send(event(EventKind::Done {
        session_id: SESSION.into(),
        outcome: "failed".into(),
    }));
    let notice = last(&ui).expect("a notice");
    assert_eq!(notice.kind, Kind::Failed);
    assert_eq!(notice.title, "Build stopped with an error");
    assert_eq!(notice.body, "hi how are you: rate limited");

    let ui = away();
    run(&ui, SESSION, "Halfway", "interrupted");
    assert_eq!(last(&ui), None, "the user stopped it");
}

#[test]
fn an_approval_asked_for_notifies() {
    let ui = away();
    ui.send(event(EventKind::PermissionAsked(Box::new(Permission {
        id: "per_1".into(),
        session_id: SESSION.into(),
        action: "shell".into(),
        resources: vec!["cargo test".into()],
        save: Vec::new(),
        message: None,
        files: Vec::new(),
        call: None,
    }))));
    let notice = last(&ui).expect("a notice");
    assert_eq!(notice.kind, Kind::Asks);
    assert_eq!(notice.title, "Build needs you");
    assert!(
        notice.body.starts_with("hi how are you: Wants to "),
        "{}",
        notice.body
    );
}

#[test]
fn the_session_in_view_and_a_focused_window_stay_quiet() {
    let ui = ui(demo::app());
    run(&ui, SESSION, "Done", "succeeded");
    assert_eq!(
        last(&ui),
        None,
        "focused, and notifying while focused is off"
    );

    ui.send(Message::Sheet(SheetEvent::Switch(
        harlo::app::Setting::NotifyFocused,
    )));
    ui.send(Message::OpenSession(SESSION.into()));
    run(&ui, SESSION, "Done", "succeeded");
    assert_eq!(last(&ui), None, "never for the session in view");
    run(&ui, "ses_demo_01", "Done too", "succeeded");
    assert_eq!(
        last(&ui).and_then(|n| n.session_id).as_deref(),
        Some("ses_demo_01"),
        "another session, while focused"
    );
}

#[test]
fn a_subagents_run_doesnt_notify_on_its_own() {
    let mut snapshot = demo::snapshot();
    snapshot.sessions[1].parent_id = Some(SESSION.to_owned());
    let mut app = App::new(
        harlo_core::server::Location::Shared,
        vec![harlo_core::server::Location::Shared],
        demo::layout(),
    );
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(demo::connected(snapshot));
    let ui = ui(app);
    ui.send(Message::Focused(false));
    run(&ui, "ses_demo_01", "Child done", "succeeded");
    assert_eq!(last(&ui), None);
}

#[test]
fn switches_and_quiet_hours_hold_notifications_back() {
    let ui = away();
    ui.send(Message::Sheet(SheetEvent::Switch(
        harlo::app::Setting::NotifyOn(Kind::Finished),
    )));
    run(&ui, SESSION, "Done", "succeeded");
    assert_eq!(last(&ui), None, "run finished is off");

    let ui = away();
    // Quiet for the minute the app's clock reads, in this time zone.
    let now = local_minute(demo::now()).expect("the local time");
    let clock = |m: u16| harlo_core::notifications::clock(m % (24 * 60));
    ui.send(Message::Sheet(SheetEvent::Switch(
        harlo::app::Setting::Quiet,
    )));
    ui.send(Message::Sheet(SheetEvent::Text(
        Field::QuietFrom,
        clock(now),
    )));
    ui.send(Message::Sheet(SheetEvent::Text(
        Field::QuietTo,
        clock(now + 1),
    )));
    run(&ui, SESSION, "Done", "succeeded");
    assert_eq!(last(&ui), None, "quiet hours");
}

#[test]
fn a_click_shows_the_window_on_its_session() {
    let ui = away();
    let tasks = Tasks::new();
    drop(ui.driver.take_tasks());
    ui.send(Message::NoticeOpened(SESSION.into()));
    assert_eq!(ui.app().active.as_deref(), Some(SESSION));
    let actions: Vec<_> = ui
        .driver
        .take_tasks()
        .into_iter()
        .filter_map(harlo::task::Task::into_stream)
        .flat_map(|stream| tasks.block_on(futures_util::StreamExt::collect::<Vec<_>>(stream)))
        .collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Effect(Effect::Raise))),
        "{actions:?}"
    );
}

#[test]
fn settings_change_templates_times_and_send_a_test() {
    let ui = ui(demo::app());
    ui.send(Message::Sheet(SheetEvent::Open(Section::Notifications)));
    assert!(ui.shows("Run finished"));
    assert!(!ui.shows("From"), "quiet hours are off");

    ui.press("Try it");
    assert_eq!(
        last(&ui).map(|n| n.title).as_deref(),
        Some("Build finished"),
        "a test, though the window has focus"
    );

    // A template, saved from its field further down.
    ui.wheel("Run finished", -20);
    ui.enter("Run finished title", "{model_name} is done");
    assert_eq!(
        ui.app().preferences.notifications.finished.title,
        "{model_name} is done"
    );

    // Quiet hours, back up, with a time that isn't one.
    ui.wheel("Agent needs you", 20);
    ui.send(Message::Sheet(SheetEvent::Switch(
        harlo::app::Setting::Quiet,
    )));
    assert!(ui.shows("From"));
    ui.send(Message::Sheet(SheetEvent::Text(
        Field::QuietFrom,
        "7:5".into(),
    )));
    ui.wheel("Quiet hours", -4);
    assert!(ui.shows("“7:5” isn't a time; use one like 22:00."));
    ui.send(Message::Sheet(SheetEvent::Text(
        Field::QuietFrom,
        "07:30".into(),
    )));
    assert_eq!(ui.app().preferences.notifications.quiet.from, 7 * 60 + 30);
    assert!(!ui.shows("“7:5” isn't a time; use one like 22:00."));

    // Off hides the rest.
    ui.send(Message::Sheet(SheetEvent::Switch(
        harlo::app::Setting::Notify,
    )));
    assert!(!ui.shows("Run finished"));
}
