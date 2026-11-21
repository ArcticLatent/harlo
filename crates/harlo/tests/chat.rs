//! Live chat in an open session: streaming rows, tool groups, stop/queue/steer,
//! the `/` and `@` menus, the pickers and attachments.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::PathBuf;

use harlo::{App, Message, ModelChoice, Variant, ids};
use harlo_core::server::{Agent, Catalog, Command, Delivery, Model, Skill, Update};
use harlo_core::timeline::{Compaction, Entry};
use harlo_core::workspace::{Event, EventKind, Fetched, InboxItem, Permission};
use iced::Point;
use iced::keyboard::{Modifiers, key};
use iced::widget::Id;
use serde_json::json;
use support::{demo, interact, shows};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn folder_of(id: &str) -> String {
    demo::snapshot()
        .sessions
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.directory)
        .expect("a demo session")
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
        models: vec![
            Model {
                provider_id: "fake".into(),
                id: "plain".into(),
                name: "Plain Model".into(),
                variants: Vec::new(),
                images: false,
            },
            Model {
                provider_id: "fake".into(),
                id: "deep".into(),
                name: "Deep Model".into(),
                variants: vec!["low".into(), "high".into()],
                images: true,
            },
        ],
        default_model: None,
        agents: vec![
            Agent {
                id: "build".into(),
                name: "build".into(),
                description: None,
                primary: true,
            },
            Agent {
                id: "plan".into(),
                name: "plan".into(),
                description: None,
                primary: true,
            },
            Agent {
                id: "explore".into(),
                name: "explore".into(),
                description: Some("Looks around".into()),
                primary: false,
            },
        ],
        commands: vec![Command {
            name: "review".into(),
            description: Some("Review changes".into()),
        }],
        skills: vec![Skill {
            id: "browser".into(),
            name: "browser".into(),
            description: None,
        }],
        resources: Vec::new(),
    }
}

/// The demo app with "pi update" open, its history empty and its folder's catalog loaded.
fn chatting() -> App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let _ = app.update(Message::Catalog(
        folder_of(PI_UPDATE),
        Ok(Box::new(catalog())),
    ));
    app
}

fn text_of(app: &App, key: &str) -> Option<String> {
    app.timeline_entries(PI_UPDATE)?
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Answer { key: k, text, .. } | Entry::Thought { key: k, text } if k == key => {
                Some(text)
            }
            _ => None,
        })
}

fn tool_events(app: &mut App, call: &str, name: &str, input: serde_json::Value, at: f64) {
    for (kind, time) in [
        (
            EventKind::ToolStarted {
                session_id: PI_UPDATE.into(),
                message_id: "msg_a".into(),
                call_id: call.into(),
                name: name.into(),
            },
            at,
        ),
        (
            EventKind::ToolCalled {
                session_id: PI_UPDATE.into(),
                message_id: "msg_a".into(),
                call_id: call.into(),
                input,
            },
            at + 10.0,
        ),
    ] {
        let _ = app.update(event(kind, time));
    }
}

#[test]
fn a_run_streams_in_and_its_tools_fold_up_when_done() {
    let mut app = chatting();
    let _ = app.update(event(
        EventKind::Busy {
            session_id: PI_UPDATE.into(),
        },
        0.0,
    ));
    let _ = app.update(event(
        EventKind::InboxAdded {
            session_id: PI_UPDATE.into(),
            item: InboxItem {
                id: "msg_u".into(),
                kind: "user".into(),
                text: Some("Look around the docs".into()),
                attachments: vec!["shot.png".into()],
                skills: vec!["release-notes".into()],
                delivery: Delivery::Queue,
            },
        },
        1.0,
    ));
    let _ = app.update(event(
        EventKind::InboxRemoved {
            session_id: PI_UPDATE.into(),
            item_id: "msg_u".into(),
            delivered: true,
        },
        2.0,
    ));
    for delta in ["**Scanning", " the docs**"] {
        let _ = app.update(event(
            EventKind::ReasoningDelta {
                session_id: PI_UPDATE.into(),
                message_id: "msg_a".into(),
                ordinal: 0,
                delta: delta.into(),
            },
            3.0,
        ));
    }
    tool_events(
        &mut app,
        "c1",
        "glob",
        json!({ "pattern": "docs/*.md" }),
        100.0,
    );
    tool_events(
        &mut app,
        "c2",
        "read",
        json!({ "filePath": "docs/a.md" }),
        200.0,
    );

    for text in [
        "Look around the docs",
        "shot.png",
        "release-notes",
        "Scanning the docs",
        "Searched 1 time, read 1 file",
        "Glob",
        "docs/*.md",
    ] {
        assert!(shows(&app, text), "shows {text:?} while running");
    }
    assert!(ui_has(&app, ids::STOP), "Stop is offered");

    for delta in ["Found ", "**two** files."] {
        let _ = app.update(event(
            EventKind::TextDelta {
                session_id: PI_UPDATE.into(),
                message_id: "msg_b".into(),
                ordinal: 0,
                delta: delta.into(),
            },
            300.0,
        ));
    }
    assert_eq!(
        text_of(&app, "msg_b/text/0").as_deref(),
        Some("Found **two** files.")
    );
    for (call, time) in [("c1", 1_600.0), ("c2", 2_400.0)] {
        let _ = app.update(event(
            EventKind::ToolEnded {
                session_id: PI_UPDATE.into(),
                message_id: "msg_a".into(),
                call_id: call.into(),
                failed: false,
                error: None,
            },
            time,
        ));
    }
    let _ = app.update(event(
        EventKind::Done {
            session_id: PI_UPDATE.into(),
            outcome: "succeeded".into(),
        },
        2_500.0,
    ));
    assert!(shows(&app, "Searched 1 time, read 1 file"));
    assert!(!shows(&app, "Glob"), "the finished group is folded");
    assert!(!ui_has(&app, ids::STOP));

    // Opening the group shows its calls with their times.
    let _ = interact(&mut app, |ui| {
        ui.click("Searched 1 time, read 1 file")
            .expect("the group header");
    });
    for text in ["Glob", "docs/*.md", "1.5s", "Read", "docs/a.md", "2.2s"] {
        assert!(shows(&app, text), "shows {text:?} once opened");
    }
}

#[test]
fn compaction_retries_and_skills_show_as_rows() {
    let mut app = chatting();
    let _ = app.update(event(
        EventKind::CompactionStarted {
            session_id: PI_UPDATE.into(),
        },
        0.0,
    ));
    assert!(shows(&app, "Summarizing the conversation…"));
    let _ = app.update(event(
        EventKind::CompactionEnded {
            session_id: PI_UPDATE.into(),
            summary: "## Objective".into(),
        },
        1.0,
    ));
    let _ = app.update(event(
        EventKind::RetryScheduled {
            session_id: PI_UPDATE.into(),
            attempt: 2,
            at: 0.0,
            error: "rate limited".into(),
        },
        2.0,
    ));
    let _ = app.update(event(
        EventKind::SkillActivated {
            session_id: PI_UPDATE.into(),
            name: "browser".into(),
        },
        3.0,
    ));
    for text in [
        "Summarized the conversation to free up context",
        "Retrying (attempt 2): rate limited",
        "Loaded the browser skill",
    ] {
        assert!(shows(&app, text), "shows {text:?}");
    }
    assert!(
        app.timeline_entries(PI_UPDATE)
            .is_some_and(|rows| rows.iter().any(|row| matches!(
                row,
                Entry::Compaction {
                    state: Compaction::Done(_),
                    ..
                }
            )))
    );
}

fn ui_has(app: &App, id: &'static str) -> bool {
    support::ui(app).find(Id::new(id)).is_ok()
}

fn type_in(app: &mut App, text: &str) -> Vec<Message> {
    interact(app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.typewrite(text);
    })
}

#[test]
fn a_busy_session_queues_steers_and_stops() {
    let mut app = chatting();
    let _ = app.update(event(
        EventKind::Busy {
            session_id: PI_UPDATE.into(),
        },
        0.0,
    ));
    let _ = app.update(Message::Fetched(Ok(Fetched::Inbox(
        PI_UPDATE.into(),
        vec![InboxItem {
            id: "msg_q".into(),
            kind: "user".into(),
            text: Some("Then update the changelog".into()),
            attachments: Vec::new(),
            skills: Vec::new(),
            delivery: Delivery::Queue,
        }],
    ))));
    assert!(shows(&app, "Then update the changelog"));

    let messages = interact(&mut app, |ui| {
        ui.click("Steer").expect("the queued prompt's Steer");
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::SteerQueued(id) if id == "msg_q"))
    );
    let messages = interact(&mut app, |ui| {
        ui.click(Id::new(ids::STOP)).expect("the stop button");
    });
    assert!(messages.iter().any(|m| matches!(m, Message::Stop)));

    let _ = type_in(&mut app, "also this");
    let messages = interact(&mut app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.tap_key(key::Named::Enter);
    });
    assert!(messages.iter().any(|m| matches!(m, Message::Send)));
    let messages = interact(&mut app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let enter = key::Key::Named(key::Named::Enter);
        let _ = ui.simulate([iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: enter.clone(),
            modified_key: enter,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::CTRL,
            repeat: false,
            text: None,
        })]);
    });
    assert!(messages.iter().any(|m| matches!(m, Message::Steer)));

    // Stopped, OpenCode holds what's waiting until the next prompt.
    let _ = app.update(event(
        EventKind::Done {
            session_id: PI_UPDATE.into(),
            outcome: "interrupted".into(),
        },
        1.0,
    ));
    assert!(shows(&app, "Then update the changelog"));
    assert!(shows(&app, "Sends before your next message"));
    assert!(!shows(&app, "Steer"), "steering needs a running session");
}

#[test]
fn slash_and_at_open_menus_from_the_catalog() {
    let mut app = chatting();
    let _ = type_in(&mut app, "/rev");
    assert!(shows(&app, "/review"), "the command is offered");
    assert!(shows(&app, "Review changes"));
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.tap_key(key::Named::Enter);
    });
    assert_eq!(app.composer.text(), "/review ");
    assert!(app.menu.is_none(), "picking closes the menu");

    let _ = type_in(&mut app, "with @exp");
    assert!(shows(&app, "@explore"), "the subagent is offered");
    let _ = interact(&mut app, |ui| {
        ui.click("@explore").expect("the menu row");
    });
    assert_eq!(app.composer.text(), "/review with @explore ");
    assert_eq!(app.draft.mentions.len(), 1);

    // Escape closes a menu until its word changes.
    let _ = type_in(&mut app, "@");
    assert!(app.menu.is_some());
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.tap_key(key::Named::Escape);
    });
    assert!(app.menu.is_none());
}

#[test]
fn pickers_choose_the_agent_model_and_reasoning_level() {
    let mut app = chatting();
    // A new session's choices are kept for when it's created.
    let _ = app.update(Message::NewSession(Some(folder_of(PI_UPDATE))));
    assert_eq!(app.current_agent().as_deref(), Some("build"));
    let messages = interact(&mut app, |ui| {
        let target = ui.find(Id::new(ids::AGENT)).expect("the agent picker");
        let bounds = target.bounds();
        let _ = ui.click(Id::new(ids::AGENT));
        // Menu entries aren't text the simulator finds: point at the second
        // one ("plan") under the picker, then click it.
        let entry = harlo::style::size::LABEL * 1.3 + 12.0;
        let position = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height + entry * 1.5,
        );
        ui.point_at(position);
        let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
            position,
        })]);
        let _ = ui.simulate(iced_test::simulator::click());
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::PickAgent(a) if a == "plan")),
        "{messages:?}"
    );
    assert_eq!(app.current_agent().as_deref(), Some("plan"));

    assert!(
        !ui_has(&app, ids::VARIANT),
        "no levels without a model that has them"
    );
    let _ = app.update(Message::PickModel(ModelChoice {
        provider_id: "fake".into(),
        id: "deep".into(),
        name: "Deep Model".into(),
    }));
    assert!(ui_has(&app, ids::VARIANT));
    let _ = app.update(Message::PickVariant(Variant(Some("high".into()))));
    let model = app.current_model().expect("a model");
    assert_eq!(
        (model.id.as_str(), model.variant.as_deref()),
        ("deep", Some("high"))
    );
}

#[test]
fn files_and_pasted_images_are_attached_and_removable() {
    let mut app = chatting();
    let _ = app.update(Message::Attached(vec![PathBuf::from("/work/notes.txt")]));
    let _ = app.update(Message::Pasted(Some(b"\x89PNG".to_vec())));
    assert!(shows(&app, "notes.txt"));
    assert!(shows(&app, "pasted-image-1.png"));
    assert_eq!(
        app.draft.attached[0].attachment.uri,
        "file:///work/notes.txt"
    );
    assert!(
        app.draft.attached[1]
            .attachment
            .uri
            .starts_with("data:image/png;base64,")
    );
    // The demo session's model isn't in this catalog, so nothing is known about images.
    assert!(app.send_error.is_none());

    // A model that can't see images says so.
    let _ = app.update(Message::NewSession(Some(folder_of(PI_UPDATE))));
    let _ = app.update(Message::PickModel(ModelChoice {
        provider_id: "fake".into(),
        id: "plain".into(),
        name: "Plain Model".into(),
    }));
    let _ = app.update(Message::Pasted(Some(b"\x89PNG".to_vec())));
    assert!(shows(
        &app,
        "The chosen model can't see images; pick one that can before sending."
    ));

    let _ = app.update(Message::Detach(0));
    assert!(!shows(&app, "notes.txt"));
    assert!(ui_has(&app, ids::SEND));
}

#[test]
fn an_image_only_prompt_shows_its_chip_at_once() {
    let mut app = chatting();
    let _ = app.update(event(
        EventKind::InboxAdded {
            session_id: PI_UPDATE.into(),
            item: InboxItem {
                id: "msg_img".into(),
                kind: "user".into(),
                text: Some(String::new()),
                attachments: vec!["pasted-image-1.png".into()],
                skills: Vec::new(),
                delivery: Delivery::Queue,
            },
        },
        0.0,
    ));
    let _ = app.update(event(
        EventKind::InboxRemoved {
            session_id: PI_UPDATE.into(),
            item_id: "msg_img".into(),
            delivered: true,
        },
        1.0,
    ));
    // The newest row is measured at its full height, not squeezed to its
    // estimate; the sensors report on the next redraw.
    let messages = interact(&mut app, |ui| {
        let _ = ui.simulate([iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))]);
    });
    let height = messages
        .iter()
        .find_map(|m| match m {
            Message::Timeline(_, harlo::timeline::Event::Measured(key, size))
                if key == "msg_img" =>
            {
                Some(size.height)
            }
            _ => None,
        })
        .expect("the prompt row is measured");
    // Padding, plus a chip at least a line tall.
    assert!(height > 40.0, "measured at {height}");
}

#[test]
fn a_narrow_composer_puts_the_pickers_on_their_own_line() {
    let mut app = chatting();
    let (width, height) = harlo::fit::WINDOW_MIN;
    let _ = app.update(Message::Resized(iced::Size::new(width, height)));
    let mut ui = iced_test::Simulator::with_size(support::settings(), (width, height), app.view());
    let model = ui
        .find(Id::new(ids::MODEL))
        .expect("the model picker")
        .bounds();
    let send = ui
        .find(Id::new(ids::SEND))
        .expect("the send button")
        .bounds();
    assert!(
        model.y + model.height <= send.y,
        "the pickers sit above the tool row: {model:?} vs {send:?}"
    );
    assert!(
        model.x + model.width <= width,
        "the model picker stays in the window: {model:?}"
    );
}

/// Types `text` into the composer and presses Enter.
fn send(app: &mut App, text: &str) {
    let _ = type_in(app, text);
    let messages = interact(app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.tap_key(key::Named::Enter);
    });
    assert!(messages.iter().any(|m| matches!(m, Message::Send)));
}

fn pi_update() -> Box<harlo_core::workspace::Session> {
    Box::new(
        demo::snapshot()
            .sessions
            .into_iter()
            .find(|s| s.id == PI_UPDATE)
            .expect("a demo session"),
    )
}

fn prompts(app: &App) -> Vec<String> {
    app.timeline_entries(PI_UPDATE)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Prompt { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn a_sent_prompt_shows_at_once_and_says_when_its_gone_out() {
    let mut app = chatting();
    send(&mut app, "Add a test for it");

    // Before OpenCode answers, the prompt is in the timeline, the line
    // above the composer says it's on its way, and the composer is free.
    assert_eq!(prompts(&app), ["Add a test for it"]);
    assert!(shows(&app, "Sending to Build…"));
    assert!(app.composer.text().trim().is_empty());
    assert!(app.is_sending());

    let _ = app.update(Message::Sent(Ok(pi_update())));
    assert!(!shows(&app, "Sending to Build…"));
    assert_eq!(prompts(&app), ["Add a test for it"], "the row stays");
    assert!(!app.is_sending());
}

#[test]
fn a_prompt_that_fails_to_send_goes_back_to_the_composer() {
    let mut app = chatting();
    send(&mut app, "Add a test for it");
    let _ = app.update(Message::Sent(Err("OpenCode isn't answering.".into())));
    assert!(prompts(&app).is_empty(), "the row is taken back");
    assert_eq!(app.composer.text().trim(), "Add a test for it");
    support::settle(&mut app);
    assert!(shows(&app, "OpenCode isn't answering."));
}

#[test]
fn a_prompt_for_a_working_session_shows_in_its_queue_until_queued() {
    let mut app = chatting();
    let _ = app.update(event(
        EventKind::Busy {
            session_id: PI_UPDATE.into(),
        },
        0.0,
    ));
    send(&mut app, "Then update the changelog");
    assert!(prompts(&app).is_empty(), "it waits for the run to finish");
    assert!(shows(&app, "Then update the changelog"));
    assert!(shows(&app, "Sending…"));

    // OpenCode queues it: the queue's own row takes over.
    let item = app
        .queued_sending(PI_UPDATE)
        .expect("the prompt on its way")
        .clone();
    let _ = app.update(Message::Fetched(Ok(Fetched::Inbox(
        PI_UPDATE.into(),
        vec![item],
    ))));
    assert!(!shows(&app, "Sending…"));
    assert!(shows(&app, "Then update the changelog"));
}

#[test]
fn the_line_above_the_composer_says_what_the_agent_is_doing() {
    let mut app = chatting();
    assert_eq!(app.activity(PI_UPDATE), None);
    let _ = app.update(event(
        EventKind::Busy {
            session_id: PI_UPDATE.into(),
        },
        0.0,
    ));
    assert!(shows(&app, "Build is thinking…"));

    tool_events(
        &mut app,
        "call_1",
        "bash",
        json!({"command": "cargo test"}),
        1.0,
    );
    assert!(shows(&app, "Build is running Bash"));
    assert!(shows(&app, "cargo test"));

    // Asked to approve the command, it says it's waiting rather than running.
    let _ = app.update(event(
        EventKind::PermissionAsked(Permission {
            id: "per_1".into(),
            session_id: PI_UPDATE.into(),
            action: "shell".into(),
            resources: vec!["cargo test".into()],
            save: Vec::new(),
        }),
        2.0,
    ));
    assert!(shows(&app, "Build is waiting for approval to shell"));
    let _ = app.update(event(
        EventKind::PermissionReplied {
            session_id: PI_UPDATE.into(),
            request_id: "per_1".into(),
        },
        3.0,
    ));
    assert!(shows(&app, "Build is running Bash"));

    let _ = app.update(event(
        EventKind::Done {
            session_id: PI_UPDATE.into(),
            outcome: "completed".into(),
        },
        30.0,
    ));
    assert_eq!(app.activity(PI_UPDATE), None);
    assert!(!shows(&app, "Build is running Bash"));
}
