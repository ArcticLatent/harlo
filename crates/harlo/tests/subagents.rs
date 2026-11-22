//! Subagents in the window: the card for work handed to one and how it
//! stands, its report, the child session under its parent in the sidebar
//! and above the composer, and the way between parent and child.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::{App, Message};
use harlo_core::server::{Agent, Catalog, Update};
use harlo_core::workspace::{Event, EventKind, Fetched, Permission, Report, Session};
use serde_json::json;
use support::{demo, interact, shows};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

/// The child session the subagent works in.
const CHILD: &str = "ses_child";

fn parent() -> Session {
    demo::snapshot()
        .sessions
        .into_iter()
        .find(|s| s.id == PI_UPDATE)
        .expect("a demo session")
}

fn child() -> Session {
    let parent = parent();
    Session {
        id: CHILD.into(),
        title: Some("Find the docs".into()),
        parent_id: Some(PI_UPDATE.into()),
        agent: Some("explore".into()),
        ..parent
    }
}

fn event(kind: EventKind, time: f64) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time,
        kind,
    }))
}

fn catalog() -> Catalog {
    Catalog {
        agents: vec![Agent {
            id: "explore".into(),
            name: "Explore".into(),
            description: Some("Searches the code.".into()),
            primary: false,
            subagent: true,
            color: Some("#4f9dff".into()),
            read_only: true,
        }],
        ..Catalog::default()
    }
}

/// The demo app with "pi update" open, its catalog loaded, and its agent
/// having handed "Find the docs" to Explore, whose session started.
fn delegated() -> App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let _ = app.update(Message::Catalog(
        parent().directory,
        Ok(Box::new(catalog())),
    ));
    let _ = app.update(event(
        EventKind::Busy {
            session_id: PI_UPDATE.into(),
        },
        0.5,
    ));
    let call = |kind| event(kind, 1.0);
    let _ = app.update(call(EventKind::ToolStarted {
        session_id: PI_UPDATE.into(),
        message_id: "msg_a".into(),
        call_id: "call_s".into(),
        name: "subagent".into(),
    }));
    let _ = app.update(call(EventKind::ToolCalled {
        session_id: PI_UPDATE.into(),
        message_id: "msg_a".into(),
        call_id: "call_s".into(),
        input: json!({ "agent": "explore", "description": "Find the docs", "prompt": "Look." }),
    }));
    assert!(shows(&app, "Starting…"), "while it's handed over");
    let _ = app.update(Message::Fetched(Ok(Fetched::Children(
        PI_UPDATE.into(),
        vec![child()],
    ))));
    let _ = app.update(event(
        EventKind::Busy {
            session_id: CHILD.into(),
        },
        1.5,
    ));
    let _ = app.update(call(EventKind::ToolEnded {
        session_id: PI_UPDATE.into(),
        message_id: "msg_a".into(),
        call_id: "call_s".into(),
        failed: false,
        error: None,
        files: Vec::new(),
        child: Some(CHILD.into()),
    }));
    app
}

#[test]
fn the_card_follows_the_child_session() {
    let mut app = delegated();
    assert!(shows(&app, "Explore"), "the agent by name");
    assert!(shows(&app, "Find the docs"));
    assert!(shows(&app, "Working"));

    let _ = app.update(event(
        EventKind::PermissionAsked(Permission {
            id: "per_1".into(),
            session_id: CHILD.into(),
            action: "shell".into(),
            resources: vec!["ls".into()],
            save: Vec::new(),
            message: None,
            files: Vec::new(),
        }),
        2.0,
    ));
    assert!(shows(&app, "Needs you"), "the child asks for approval");

    let _ = app.update(event(
        EventKind::Done {
            session_id: CHILD.into(),
            outcome: "succeeded".into(),
        },
        3.0,
    ));
    assert!(shows(&app, "Done"));
    assert!(!shows(&app, "Needs you"));
}

#[test]
fn a_report_shows_what_the_subagent_said() {
    let mut app = delegated();
    let _ = app.update(event(
        EventKind::SubagentReported {
            session_id: PI_UPDATE.into(),
            id: "msg_r".into(),
            report: Report {
                child: CHILD.into(),
                agent: "Explore".into(),
                task: "Find the docs".into(),
                state: "completed".into(),
                text: "Found README.md\nand docs/setup.md\nand docs/api.md\nand more".into(),
            },
        },
        3.0,
    ));
    assert!(shows(&app, "Explore reported back"));
    assert!(
        shows(&app, "Found README.md\nand docs/setup.md\nand docs/api.md…"),
        "its first lines"
    );
    // A command run as a subagent reports with the agent's ID.
    let _ = app.update(event(
        EventKind::SubagentReported {
            session_id: PI_UPDATE.into(),
            id: "msg_c".into(),
            report: Report {
                child: CHILD.into(),
                agent: "explore".into(),
                task: "Look again".into(),
                state: "failed".into(),
                text: String::new(),
            },
        },
        4.0,
    ));
    assert!(
        shows(&app, "Explore stopped: failed"),
        "by name all the same"
    );
}

#[test]
fn open_on_the_card_opens_the_child_session() {
    let mut app = delegated();
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("Open");
    });
    assert!(
        messages.iter().any(|m| matches!(
            m,
            Message::Timeline(_, harlo::timeline::Event::OpenSession(id)) if id == CHILD
        )),
        "{messages:?}"
    );
    assert_eq!(app.active.as_deref(), Some(CHILD));
    assert_eq!(app.tabs.len(), 2, "beside its parent's tab");
}

#[test]
fn a_child_session_leads_back_to_its_parent() {
    let mut app = delegated();
    let _ = app.update(Message::OpenSession(CHILD.into()));
    let _ = app.update(Message::History(CHILD.into(), Ok(Vec::new())));
    let title = parent().title.expect("a title");
    let link = format!("Subagent of {title}");
    assert!(shows(&app, &link));
    assert!(shows(&app, "Explore"), "its agent, though not one to pick");
    let messages = interact(&mut app, |ui| {
        let _ = ui.click(link.as_str());
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::OpenSession(id) if id == PI_UPDATE))
    );
    assert_eq!(app.active.as_deref(), Some(PI_UPDATE));
}

#[test]
fn a_finished_subagent_leaves_the_sidebar_until_its_parent_is_open() {
    let mut app = delegated();
    let _ = app.update(event(
        EventKind::Done {
            session_id: CHILD.into(),
            outcome: "succeeded".into(),
        },
        3.0,
    ));
    assert!(shows(&app, "Find the docs"), "under the open parent");
    // Elsewhere, a finished child is out of the way.
    let _ = app.update(Message::NewSession(None));
    assert!(!shows(&app, "Find the docs"));
    let _ = app.update(event(
        EventKind::Busy {
            session_id: CHILD.into(),
        },
        4.0,
    ));
    assert!(shows(&app, "Find the docs"), "back while it works again");
}
