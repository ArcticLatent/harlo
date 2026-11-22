//! The agent's todo list in the window: the Todos panel, the line above the
//! composer while there's work left, and the note when the todo tool is off.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo::{App, Message, Panel, TODO_SETTING};
use harlo_core::server::{Catalog, Update};
use harlo_core::timeline::{Entry, Todo, TodoStatus};
use harlo_core::workspace::{Event, EventKind, Session, ToolStatus};
use serde_json::json;
use support::{demo, interact, shows};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn session() -> Session {
    demo::snapshot()
        .sessions
        .into_iter()
        .find(|s| s.id == PI_UPDATE)
        .expect("a demo session")
}

fn todo(content: &str, status: TodoStatus) -> Todo {
    Todo {
        content: content.into(),
        status,
    }
}

/// A finished todo call that wrote `todos`.
fn written(key: &str, todos: Vec<Todo>) -> Entry {
    Entry::Tool {
        key: key.into(),
        name: "todowrite".into(),
        label: "Todo".into(),
        detail: String::new(),
        status: ToolStatus::Completed,
        took: Some(Duration::from_millis(9)),
        files: Vec::new(),
        todos: Some(todos),
    }
}

/// The demo app with "pi update" open, its history `rows`, and the folder's
/// todo tool on or off.
fn open_with(rows: Vec<Entry>, todo_tool: bool) -> App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(rows)));
    let _ = app.update(Message::Catalog(
        session().directory,
        Ok(Box::new(Catalog {
            todo_tool,
            ..Catalog::default()
        })),
    ));
    app
}

fn open_todos(app: &mut App) {
    if app.panel != Some(Panel::Todos) {
        let _ = app.update(Message::OpenPanel(Panel::Todos));
    }
}

fn halfway() -> Vec<Todo> {
    vec![
        todo("Read the spec", TodoStatus::Completed),
        todo("Write the parser", TodoStatus::InProgress),
        todo("Add tests", TodoStatus::Pending),
        todo("Try the old flag", TodoStatus::Cancelled),
    ]
}

#[test]
fn the_panel_lists_the_last_list_written() {
    let first = vec![todo("Read the spec", TodoStatus::InProgress)];
    let mut app = open_with(vec![written("a", first), written("b", halfway())], true);
    open_todos(&mut app);
    assert!(shows(&app, "1 of 3 done"), "dropped items don't count");
    for item in ["Read the spec", "Write the parser", "Add tests"] {
        assert!(shows(&app, item), "{item}");
    }
    assert!(shows(&app, "Dropped"));
}

#[test]
fn a_line_above_the_composer_follows_the_work_left() {
    let mut app = open_with(vec![written("a", halfway())], true);
    if app.panel == Some(Panel::Todos) {
        let _ = app.update(Message::ClosePanel);
    }
    assert!(shows(&app, "1 of 3"));
    assert!(shows(&app, "Write the parser"), "what it's on");
    let _ = interact(&mut app, |ui| {
        let _ = ui.click("Todos");
    });
    assert_eq!(app.panel, Some(Panel::Todos), "it opens the whole list");

    // The next call, live, finishes the list: the line goes.
    let event = |kind| {
        Message::Server(Update::Event(Event {
            id: "evt".into(),
            seq: None,
            time: 1.0,
            kind,
        }))
    };
    let _ = app.update(event(EventKind::ToolStarted {
        session_id: PI_UPDATE.into(),
        message_id: "msg_b".into(),
        call_id: "call_t".into(),
        name: "todowrite".into(),
    }));
    let _ = app.update(event(EventKind::ToolCalled {
        session_id: PI_UPDATE.into(),
        message_id: "msg_b".into(),
        call_id: "call_t".into(),
        input: json!({ "todos": [
            { "content": "Read the spec", "status": "completed" },
            { "content": "Write the parser", "status": "completed" },
        ] }),
    }));
    assert!(!shows(&app, "2 of 2"), "nothing left to do");
    assert!(shows(&app, "2 of 2 done"), "the open panel follows it");
}

#[test]
fn without_a_list_the_panel_says_why() {
    let mut app = open_with(Vec::new(), true);
    open_todos(&mut app);
    assert!(
        shows(
            &app,
            "No list yet. The agent writes one when a task has several steps."
        ),
        "the tool is on"
    );

    let mut app = open_with(Vec::new(), false);
    open_todos(&mut app);
    assert!(shows(&app, "Agents here don't keep a todo list."));
    assert!(shows(&app, TODO_SETTING), "how to turn it on");

    let mut app = open_with(vec![written("a", Vec::new())], true);
    open_todos(&mut app);
    assert!(shows(&app, "The agent cleared its list."));
}
