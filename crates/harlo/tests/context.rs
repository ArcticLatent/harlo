//! The Context panel: how full the context window is, the session's usage
//! and cost, its instruction files, and reloading after each step.
//!
//! Loads are fed in as their results; the endpoints are tested in
//! `opencode-client` and the instruction rule in `harlo-core`.

// Rust guideline compliant 2026-10-09

#[expect(
    dead_code,
    reason = "shared UI helpers; these tests feed results rather than click"
)]
mod support;

use std::path::PathBuf;

use harlo::{App, ContextEvent, ContextLoaded, Message, Panel};
use harlo_core::instructions::{File, Scope};
use harlo_core::server::{Catalog, Model, SessionContext, Tokens, Update};
use harlo_core::workspace::{Event, EventKind, ModelRef};
use support::{demo, shows};

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

/// A model with a 200k window, in the session's folder.
fn catalog() -> Catalog {
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

/// The demo app with "pi update" open and the Context panel showing `found`.
fn panel(found: ContextLoaded) -> App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let _ = app.update(Message::Catalog(folder(), Ok(Box::new(catalog()))));
    // It opens on the Context panel; opening it again would close it.
    if app.panel != Some(Panel::Context) {
        let _ = app.update(Message::OpenPanel(Panel::Context));
    }
    // Showing it for the session started the first load.
    let _ = app.update(Message::Context(ContextEvent::Loaded(1, Box::new(found))));
    support::settle(&mut app);
    app
}

#[test]
fn the_window_shows_how_full_it_is_against_the_models_size() {
    let app = panel(loaded(seen(), Some(Vec::new())));
    assert!(shows(&app, "Context window"));
    assert!(shows(&app, "30k"), "the latest step's tokens");
    assert!(shows(&app, "of 200k tokens"));
    assert!(shows(&app, "15% full"));
    assert!(shows(&app, "12 messages since the last compaction"));
    assert!(shows(&app, "Deep Model"), "the step's model, by name");
    assert!(shows(&app, "plan"), "and its agent");
}

#[test]
fn usage_follows_the_sessions_events() {
    let mut app = panel(loaded(seen(), Some(Vec::new())));
    assert!(shows(&app, "$0.00"));
    let _ = app.update(event(EventKind::UsageUpdated {
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
    assert!(shows(&app, "$0.42"));
    assert!(shows(&app, "12.3k"));
    assert!(shows(&app, "950"));
    assert!(shows(&app, "Cache read"), "counted, so shown");
    assert!(shows(&app, "1.5M"));
    assert!(!shows(&app, "Reasoning"), "not counted, so left out");
    assert!(!shows(&app, "Cache write"));
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
    let app = panel(loaded(seen(), Some(files)));
    assert!(shows(&app, "~/.config/opencode/AGENTS.md"));
    assert!(shows(&app, "everywhere"));
    assert!(shows(&app, "AGENTS.md"), "from the project's root");
    assert!(shows(&app, "project"));
    assert!(shows(&app, "sub/AGENTS.md"));
    assert!(shows(&app, "picked up"));
}

#[test]
fn elsewhere_only_files_picked_up_are_known() {
    let app = panel(loaded(seen(), None));
    assert!(shows(
        &app,
        "The files given from the start can only be listed for folders on this computer."
    ));
    assert!(shows(&app, "sub/AGENTS.md"));
    let app = panel(loaded(SessionContext::default(), Some(Vec::new())));
    assert!(shows(
        &app,
        "No AGENTS.md or CLAUDE.md applies to this session."
    ));
    assert!(shows(&app, "Nothing in it until the model answers."));
}

#[test]
fn a_step_reloads_it_and_a_late_answer_is_ignored() {
    let mut app = panel(loaded(SessionContext::default(), Some(Vec::new())));
    let _ = app.update(event(EventKind::StepEnded {
        session_id: PI_UPDATE.into(),
        message_id: "msg_a".into(),
        tokens: tokens(29_000, 1_000),
    }));
    // The first load's answer, arriving again late, changes nothing.
    let _ = app.update(Message::Context(ContextEvent::Loaded(
        1,
        Box::new(loaded(SessionContext::default(), Some(Vec::new()))),
    )));
    assert!(shows(&app, "Nothing in it until the model answers."));
    let _ = app.update(Message::Context(ContextEvent::Loaded(
        2,
        Box::new(loaded(seen(), Some(Vec::new()))),
    )));
    assert!(shows(&app, "of 200k tokens"));
}

#[test]
fn a_failed_load_says_why() {
    let app = panel(ContextLoaded {
        context: Err("server went away".into()),
        files: None,
        home: None,
        root: None,
    });
    assert!(shows(&app, "Couldn't load it: server went away"));
}

#[test]
fn counts_near_a_million_round_to_millions() {
    let mut app = panel(loaded(seen(), Some(Vec::new())));
    let _ = app.update(event(EventKind::UsageUpdated {
        session_id: PI_UPDATE.into(),
        cost: 0.004,
        tokens: tokens(999_600, 999_400),
    }));
    assert!(shows(&app, "1M"), "not 1000k");
    assert!(shows(&app, "999k"), "no decimal from 100k up");
    assert!(shows(&app, "<$0.01"), "a little, not nothing");
}
