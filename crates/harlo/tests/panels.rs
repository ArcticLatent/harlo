//! The Context and Todos panels: how full the context window is, the
//! session's usage, cost and instruction files, and the agent's todo list.
//!
//! Loads are fed in as their results; the endpoints are tested in
//! `opencode-client` and the instruction rule in `harlo-core`.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::PathBuf;
use std::time::Duration;

use harlo::app::{ContextEvent, ContextLoaded, Message};
use harlo::model::TODO_SETTING;
use harlo_core::instructions::{File, Scope};
use harlo_core::server::{Catalog, Model, SessionContext, Tokens, Update};
use harlo_core::timeline::{Entry, Todo, TodoStatus};
use harlo_core::workspace::{Event, EventKind, ModelRef, ToolStatus};
use serde_json::json;
use support::{Ui, demo, ui};

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

fn event(kind: EventKind) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind,
    }))
}

fn tokens(input: u64, output: u64) -> Tokens {
    Tokens {
        input,
        output,
        ..Tokens::default()
    }
}

/// The window with "pi update" open, its history `rows`, and the folder's
/// catalog.
fn open(rows: Vec<Entry>, catalog: Catalog) -> Ui {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(rows)));
    let _ = app.update(Message::Catalog(folder(), Ok(Box::new(catalog))));
    ui(app)
}

// The Context panel.

/// A model with a 200k window.
fn deep() -> Catalog {
    Catalog {
        models: vec![Model {
            provider_id: "fake".into(),
            id: "deep".into(),
            name: "Deep Model".into(),
            variants: Vec::new(),
            images: false,
            context: Some(200_000),
        }],
        ..Catalog::default()
    }
}

/// The latest step filled 30k tokens, on the catalog's model.
fn seen() -> SessionContext {
    SessionContext {
        messages: 12,
        last_step: Some(tokens(29_000, 1_000)),
        model: Some(ModelRef {
            provider_id: "fake".into(),
            id: "deep".into(),
            variant: None,
        }),
        agent: Some("plan".into()),
        loaded: vec![format!("{}/sub/AGENTS.md", folder())],
    }
}

fn loaded(context: SessionContext, files: Option<Vec<File>>) -> ContextLoaded {
    ContextLoaded {
        context: Ok(context),
        files,
        home: Some(PathBuf::from("/home/demo")),
        root: Some(PathBuf::from(folder())),
    }
}

/// The Context panel from the rail, showing what its first load `found`.
fn context(found: ContextLoaded) -> Ui {
    let ui = open(Vec::new(), deep());
    ui.open("Context");
    ui.send(Message::Context(ContextEvent::Loaded(1, Box::new(found))));
    ui
}

#[test]
fn the_window_shows_how_full_it_is_against_the_models_size() {
    let ui = context(loaded(seen(), Some(Vec::new())));
    assert!(ui.shows("Context window"));
    assert!(ui.shows("30k"), "the latest step's tokens");
    assert!(ui.shows("of 200k tokens"));
    assert!(ui.shows("15% full"));
    assert!(ui.shows("12 messages since the last compaction"));
    assert!(ui.shows("Deep Model"), "the step's model, by name");
    assert!(ui.shows("plan"), "and its agent");
}

#[test]
fn usage_follows_the_sessions_events() {
    let ui = context(loaded(seen(), Some(Vec::new())));
    assert!(ui.shows("$0.00"));
    ui.send(event(EventKind::UsageUpdated {
        session_id: PI_UPDATE.into(),
        cost: 0.4213,
        tokens: Tokens {
            input: 12_340,
            output: 950,
            reasoning: 0,
            cache_read: 1_500_000,
            cache_write: 0,
        },
    }));
    assert!(ui.shows("$0.42"));
    assert!(ui.shows("12.3k"));
    assert!(ui.shows("950"));
    assert!(ui.shows("Cache read"), "counted, so shown");
    assert!(ui.shows("1.5M"));
    assert!(!ui.shows("Reasoning"), "not counted, so left out");
    assert!(!ui.shows("Cache write"));
}

#[test]
fn instruction_files_show_where_they_apply() {
    let files = vec![
        File {
            path: "/home/demo/.config/opencode/AGENTS.md".into(),
            scope: Scope::Global,
        },
        File {
            path: format!("{}/AGENTS.md", folder()).into(),
            scope: Scope::Project,
        },
    ];
    let ui = context(loaded(seen(), Some(files)));
    assert!(ui.shows("~/.config/opencode/AGENTS.md"));
    assert!(ui.shows("everywhere"));
    assert!(ui.shows("AGENTS.md"), "from the project's root");
    assert!(ui.shows("project"));
    assert!(ui.shows("sub/AGENTS.md"));
    assert!(ui.shows("picked up"));
}

#[test]
fn elsewhere_only_files_picked_up_are_known() {
    let ui = context(loaded(seen(), None));
    assert!(
        ui.shows("The files given from the start can only be listed for folders on this computer.")
    );
    assert!(ui.shows("sub/AGENTS.md"));

    let ui = context(loaded(SessionContext::default(), Some(Vec::new())));
    assert!(ui.shows("No AGENTS.md or CLAUDE.md applies to this session."));
    assert!(ui.shows("Nothing in it until the model answers."));
}

#[test]
fn a_step_reloads_it_and_a_late_answer_is_ignored() {
    let ui = context(loaded(SessionContext::default(), Some(Vec::new())));
    ui.send(event(EventKind::StepEnded {
        session_id: PI_UPDATE.into(),
        message_id: "msg_a".into(),
        tokens: tokens(29_000, 1_000),
    }));
    // The first load's answer, arriving again late, changes nothing.
    ui.send(Message::Context(ContextEvent::Loaded(
        1,
        Box::new(loaded(seen(), Some(Vec::new()))),
    )));
    assert!(ui.shows("Nothing in it until the model answers."));
    ui.send(Message::Context(ContextEvent::Loaded(
        2,
        Box::new(loaded(seen(), Some(Vec::new()))),
    )));
    assert!(ui.shows("of 200k tokens"));
}

#[test]
fn a_failed_load_says_why() {
    let ui = context(ContextLoaded {
        context: Err("server went away".into()),
        files: None,
        home: None,
        root: None,
    });
    assert!(ui.shows("Couldn't load it: server went away"));
}

#[test]
fn counts_near_a_million_round_to_millions() {
    let ui = context(loaded(seen(), Some(Vec::new())));
    ui.send(event(EventKind::UsageUpdated {
        session_id: PI_UPDATE.into(),
        cost: 0.004,
        tokens: tokens(999_600, 999_400),
    }));
    assert!(ui.shows("1M"), "not 1000k");
    assert!(ui.shows("999k"), "no decimal from 100k up");
    assert!(ui.shows("<$0.01"), "a little, not nothing");
}

#[test]
fn without_a_session_it_says_so() {
    let ui = ui(demo::app());
    ui.open("Context");
    assert!(ui.shows("No session selected."));
    ui.open("Todos");
    assert!(ui.shows("No session selected."));
}

// The Todos panel.

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
        pictures: Vec::new(),
    }
}

/// The folder's catalog, with the todo tool on or off.
fn todo_tool(on: bool) -> Catalog {
    Catalog {
        todo_tool: on,
        ..Catalog::default()
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
    let ui = open(
        vec![written("a", first), written("b", halfway())],
        todo_tool(true),
    );
    ui.open("Todos");
    assert!(ui.shows("1 of 3 done"), "dropped items don't count");
    for item in ["Read the spec", "Write the parser", "Add tests"] {
        assert!(ui.shows(item), "{item}");
    }
    assert!(ui.shows("Dropped"));
}

#[test]
fn the_open_panel_follows_the_next_list() {
    let ui = open(vec![written("a", halfway())], todo_tool(true));
    ui.open("Todos");
    ui.send(event(EventKind::ToolStarted {
        session_id: PI_UPDATE.into(),
        message_id: "msg_b".into(),
        call_id: "call_t".into(),
        name: "todowrite".into(),
    }));
    ui.send(event(EventKind::ToolCalled {
        session_id: PI_UPDATE.into(),
        message_id: "msg_b".into(),
        call_id: "call_t".into(),
        input: json!({ "todos": [
            { "content": "Read the spec", "status": "completed" },
            { "content": "Write the parser", "status": "completed" },
        ] }),
    }));
    assert!(ui.shows("2 of 2 done"));
    assert!(!ui.shows("Add tests"), "the old list's items went");
}

#[test]
fn without_a_list_the_panel_says_why() {
    let ui = open(Vec::new(), todo_tool(true));
    ui.open("Todos");
    assert!(
        ui.shows("No list yet. The agent writes one when a task has several steps."),
        "the tool is on"
    );

    let ui = open(Vec::new(), todo_tool(false));
    ui.open("Todos");
    assert!(ui.shows("Agents here don't keep a todo list."));
    assert!(ui.shows(TODO_SETTING), "how to turn it on");

    let ui = open(vec![written("a", Vec::new())], todo_tool(true));
    ui.open("Todos");
    assert!(ui.shows("The agent cleared its list."));
}
