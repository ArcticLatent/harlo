//! Live chat in an open session: streaming rows, tool groups, stop/queue/steer,
//! the `/` and `@` menus, the pickers and attachments, and the cards asking
//! for approvals and answers.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::PathBuf;

use harlo::{App, Message, ModelChoice, Picker, Variant, ids};
use harlo_core::approvals::Shield;
use harlo_core::server::{
    Agent, Answer, Catalog, Choice, Command, Delivery, Effect, Field, FieldKind, FileDiff, Model,
    PermissionReply, Rule, Skill, Update,
};
use harlo_core::timeline::{Compaction, Entry};
use harlo_core::workspace::{Event, EventKind, Fetched, Form, InboxItem, Permission, Revert};
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
                context: None,
            },
            Model {
                provider_id: "fake".into(),
                id: "deep".into(),
                name: "Deep Model".into(),
                variants: vec!["low".into(), "high".into()],
                images: true,
                context: None,
            },
        ],
        default_model: None,
        agents: vec![
            Agent {
                id: "build".into(),
                name: "build".into(),
                description: None,
                primary: true,
                subagent: false,
                color: None,
                read_only: false,
            },
            Agent {
                id: "plan".into(),
                name: "plan".into(),
                description: None,
                primary: true,
                subagent: false,
                color: None,
                read_only: false,
            },
            Agent {
                id: "explore".into(),
                name: "explore".into(),
                description: Some("Looks around".into()),
                primary: false,
                subagent: true,
                color: None,
                read_only: false,
            },
        ],
        commands: vec![Command {
            name: "review".into(),
            description: Some("Review changes".into()),
            agent: Some("explore".into()),
            subagent: true,
        }],
        skills: vec![Skill {
            id: "browser".into(),
            name: "browser".into(),
            description: None,
        }],
        resources: Vec::new(),
        todo_tool: false,
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
                files: Vec::new(),
                child: None,
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
    assert!(
        shows(&app, "explore · subagent"),
        "it runs as the explore subagent"
    );
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
    // The chip opens its menu, with the current agent ticked.
    let messages = interact(&mut app, |ui| {
        let _ = ui.click(Id::new(ids::AGENT));
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::TogglePicker(Picker::Agent)))
    );
    assert_eq!(app.picker, Some(Picker::Agent));
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("plan");
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::PickAgent(a) if a == "plan")),
        "{messages:?}"
    );
    assert_eq!(app.picker, None, "choosing closes the menu");
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
            message: None,
            files: Vec::new(),
        }),
        2.0,
    ));
    // A card asks instead of the line.
    assert!(shows(&app, "Build wants to run a command"));
    assert!(!shows(&app, "Build is running Bash"));
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

/// Clicks at `at`, as a separate press, in the same view.
fn click_at(ui: &mut iced_test::Simulator<'_, Message>, at: Point) {
    ui.point_at(at);
    let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: at,
    })]);
    let _ = ui.simulate(iced_test::simulator::click());
}

#[test]
fn an_open_picker_closes_when_clicking_anywhere_else() {
    // The editor and the attach button take the click themselves; the menu
    // must close all the same.
    for target in [ids::COMPOSER, ids::ATTACH, ids::SEND] {
        let mut app = chatting();
        let _ = app.update(Message::TogglePicker(Picker::Agent));
        let messages = interact(&mut app, |ui| {
            let elsewhere = match target {
                ids::COMPOSER => {
                    let card = ui.find(Id::new(ids::COMPOSER)).expect("card").bounds();
                    Point::new(card.x + 40.0, card.y + 25.0)
                }
                _ => ui
                    .find(Id::new(target))
                    .expect("a button")
                    .bounds()
                    .center(),
            };
            click_at(ui, elsewhere);
        });
        assert!(
            messages.iter().any(|m| matches!(m, Message::ClosePicker)),
            "the menu stayed open after a click on {target}: {messages:?}"
        );
        assert_eq!(app.picker, None);
    }
    // Escape closes it too, and the chip closes its own menu.
    let mut app = chatting();
    let _ = app.update(Message::TogglePicker(Picker::Agent));
    let _ = interact(&mut app, |ui| {
        let _ = ui.simulate([iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: key::Key::Named(key::Named::Escape),
            modified_key: key::Key::Named(key::Named::Escape),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        })]);
    });
    assert_eq!(app.picker, None, "Escape closes the menu");
    let _ = app.update(Message::TogglePicker(Picker::Agent));
    let messages = interact(&mut app, |ui| {
        let _ = ui.click(Id::new(ids::AGENT));
    });
    assert!(
        matches!(messages.as_slice(), [Message::TogglePicker(Picker::Agent)]),
        "{messages:?}"
    );
    assert_eq!(app.picker, None, "the chip closes its menu");
}

#[test]
fn a_long_model_list_scrolls_and_filters() {
    let mut app = chatting();
    let mut many = catalog();
    many.models = (0..40)
        .map(|i| Model {
            provider_id: "fake".into(),
            id: format!("m{i}"),
            name: format!("Model {i}"),
            variants: Vec::new(),
            images: false,
            context: None,
        })
        .collect();
    let _ = app.update(Message::Catalog(folder_of(PI_UPDATE), Ok(Box::new(many))));
    // A new session keeps its choices here, rather than sending them.
    let _ = app.update(Message::NewSession(Some(folder_of(PI_UPDATE))));
    let _ = app.update(Message::TogglePicker(Picker::Model));
    // Eight rows show; the rest scroll.
    let _ = interact(&mut app, |ui| {
        let list = ui
            .find(Id::new(harlo::MENU_LIST))
            .expect("the menu's list")
            .bounds();
        let rows = f32::from(u8::try_from(harlo::MENU_ROWS).expect("a few rows"));
        assert!(
            (list.height - rows * (harlo::MENU_ROW + harlo::ROW_GAP)).abs() < 0.5,
            "{list:?}"
        );
        // Each row names the model's provider.
        let _ = ui.find("fake").expect("the provider");
    });
    // A filter narrows the list, and Enter picks the first match.
    let _ = app.update(Message::PickerFilter("model 3".into()));
    assert_eq!(app.picker_index, 0);
    let _ = interact(&mut app, |ui| {
        let _ = ui.find("Model 3").expect("a match");
        let _ = ui.find("Model 39").expect("a match");
        let _ = ui.find("Model 4").expect_err("not a match");
    });
    let _ = app.update(Message::PickerMove(1));
    let _ = app.update(Message::PickerMove(-2));
    // Models 3, 13, 23 and 30 to 39 match: moving up from the top wraps
    // round to the last.
    assert_eq!(app.picker_index, 12, "moving up from the top wraps round");
    let _ = app.update(Message::PickerFilter("model 3".into()));
    // Keys a focused field doesn't use move through the menu and pick.
    let press = |named| iced::keyboard::Event::KeyPressed {
        key: key::Key::Named(named),
        modified_key: key::Key::Named(named),
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::empty(),
        text: None,
        repeat: false,
    };
    assert!(matches!(
        harlo::app::menu_key(press(key::Named::ArrowDown)),
        Some(Message::PickerMove(1))
    ));
    assert!(matches!(
        harlo::app::menu_key(press(key::Named::Enter)),
        Some(Message::PickerSubmit)
    ));
    let _ = app.update(Message::PickerSubmit);
    assert_eq!(
        app.current_model().map(|m| m.id),
        Some("m3".to_owned()),
        "Enter picks the first match"
    );
    let _ = app.update(Message::TogglePicker(Picker::Model));
    let _ = app.update(Message::PickerFilter("nothing like it".into()));
    assert!(shows(&app, "Nothing matches"));
}

#[test]
fn the_folder_menu_offers_to_add_a_project() {
    let mut app = chatting();
    let _ = app.update(Message::NewSession(None));
    let _ = app.update(Message::TogglePicker(Picker::Folder));
    assert!(shows(&app, "Home folder"));
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("Add project…");
    });
    assert!(
        messages.iter().any(|m| matches!(m, Message::AddProject)),
        "{messages:?}"
    );
    assert_eq!(app.picker, None);
    // A folder chosen there is where the next session starts, before it's a project.
    let _ = app.update(Message::ProjectChosen(Some(PathBuf::from(
        "/tmp/fresh-idea",
    ))));
    assert_eq!(app.new_folder.as_deref(), Some("/tmp/fresh-idea"));
    assert!(shows(&app, "fresh-idea"));
}

fn approval(id: &str, action: &str, resources: &[&str]) -> Permission {
    Permission {
        id: id.into(),
        session_id: PI_UPDATE.into(),
        action: action.into(),
        resources: resources.iter().map(|r| (*r).to_owned()).collect(),
        save: vec!["cargo *".into()],
        message: None,
        files: Vec::new(),
    }
}

/// Whether the sidebar marks "pi update" as waiting for the user.
fn marked_waiting(app: &App) -> bool {
    app.projects
        .iter()
        .flat_map(|p| p.sessions.iter())
        .any(|s| s.id == PI_UPDATE && s.waiting)
}

#[test]
fn an_approval_card_allows_or_refuses_and_goes_once_answered() {
    let mut app = chatting();
    let _ = app.update(event(
        EventKind::PermissionAsked(approval("per_1", "shell", &["cargo test"])),
        1.0,
    ));
    assert!(shows(&app, "Build wants to run a command"));
    assert!(shows(&app, "cargo test"));
    assert!(shows(&app, "Always allow cargo *"));
    assert!(marked_waiting(&app), "the sidebar marks the session");

    let messages = interact(&mut app, |ui| {
        let _ = ui.click(Id::new(ids::ALLOW));
    });
    assert!(
        messages.iter().any(|m| matches!(
            m,
            Message::Approve { request, reply: PermissionReply::Once } if request == "per_1"
        )),
        "{messages:?}"
    );

    // While the answer is on its way, the card takes no other.
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("Deny");
    });
    assert!(messages.is_empty(), "{messages:?}");
    // A failed answer says so, and the card can be answered again.
    let _ = app.update(Message::Answered(
        PI_UPDATE.into(),
        "per_1".into(),
        Err("Couldn't answer: the server went away".into()),
    ));
    assert!(shows(&app, "Couldn't answer: the server went away"));

    // Refusing with a reason swaps the buttons for a box to say why.
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("Deny with a reason…");
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Refuse(id) if id == "per_1")),
        "{messages:?}"
    );
    assert!(shows(&app, "Tell Build what to do instead"));
    let _ = app.update(Message::RefuseReason("run cargo check".into()));
    let _ = app.update(Message::RefuseCancel);
    assert!(shows(&app, "Allow once"), "cancelled, the buttons are back");

    // Answered, here or elsewhere, the card goes.
    let _ = app.update(Message::Answered(PI_UPDATE.into(), "per_1".into(), Ok(())));
    assert!(!shows(&app, "Build wants to run a command"));
    assert!(!marked_waiting(&app));
}

#[test]
fn an_edit_approval_shows_the_change_and_how_many_wait() {
    let mut app = chatting();
    let edit = Permission {
        files: vec![FileDiff {
            file: "e.txt".into(),
            patch: "--- e.txt\n+++ e.txt\n@@ -1,2 +1,2 @@\n one\n-two\n+three\n".into(),
            additions: 1,
            deletions: 1,
            status: "modified".into(),
        }],
        ..approval("per_1", "edit", &["e.txt"])
    };
    let _ = app.update(event(EventKind::PermissionAsked(edit), 1.0));
    let _ = app.update(event(
        EventKind::PermissionAsked(approval("per_2", "shell", &["ls"])),
        2.0,
    ));
    assert!(shows(&app, "Build wants to edit a file"));
    assert!(shows(&app, "e.txt"));
    assert!(shows(&app, "three"), "the patch's lines");
    assert!(shows(&app, "two"));
    assert!(shows(&app, "1 of 2"));
}

fn colour_question() -> Form {
    let field = |key: &str, title: &str, kind: FieldKind| Field {
        key: key.into(),
        title: Some(title.into()),
        description: None,
        required: true,
        hidden: false,
        when: Vec::new(),
        kind,
    };
    let choices = |labels: &[&str]| -> Vec<Choice> {
        labels
            .iter()
            .map(|l| Choice {
                value: (*l).into(),
                label: (*l).into(),
                description: None,
            })
            .collect()
    };
    Form {
        id: "frm_1".into(),
        session_id: PI_UPDATE.into(),
        title: "Questions".into(),
        fields: vec![
            field(
                "colour",
                "Which colour?",
                FieldKind::Text {
                    options: choices(&["Red", "Blue"]),
                    custom: true,
                    placeholder: None,
                    default: None,
                },
            ),
            field(
                "extras",
                "Which extras?",
                FieldKind::Multiselect {
                    options: choices(&["Tests", "Docs"]),
                    custom: false,
                    min_items: None,
                    max_items: None,
                    default: Vec::new(),
                },
            ),
        ],
    }
}

#[test]
fn a_question_card_takes_answers_and_says_what_is_missing() {
    let mut app = chatting();
    let _ = app.update(event(EventKind::FormCreated(colour_question()), 1.0));
    assert!(shows(&app, "Build has a question"));
    assert!(shows(&app, "Which colour?"));
    assert!(marked_waiting(&app));

    let messages = interact(&mut app, |ui| {
        let _ = ui.click("Blue");
    });
    assert!(messages.iter().any(|m| matches!(
        m,
        Message::FormChoose { key, value, .. } if key == "colour" && value == "Blue"
    )));

    // Nothing chosen for the extras yet: sending says so instead.
    let _ = app.update(Message::FormSend("frm_1".into()));
    assert!(shows(&app, "Needs an answer"));
    let _ = interact(&mut app, |ui| {
        let _ = ui.click("Docs");
    });
    assert!(!shows(&app, "Needs an answer"), "answering clears it");
    let draft = app.drafts.get("frm_1").expect("a draft");
    let form = colour_question();
    assert_eq!(
        draft.answers(&form),
        Ok(vec![
            ("colour".into(), Answer::Text("Blue".into())),
            ("extras".into(), Answer::List(vec!["Docs".into()])),
        ])
    );

    let _ = app.update(Message::Answered(PI_UPDATE.into(), "frm_1".into(), Ok(())));
    assert!(!shows(&app, "Build has a question"));
    assert!(!marked_waiting(&app));
}

#[test]
fn the_shield_sets_what_asks_first() {
    let mut app = chatting();
    assert_eq!(app.current_shield(), Shield::Agent);
    // Rules set from another client show here.
    let _ = app.update(event(
        EventKind::SessionPermissions {
            session_id: PI_UPDATE.into(),
            rules: vec![Rule::any("*", Effect::Allow)],
        },
        1.0,
    ));
    assert_eq!(app.current_shield(), Shield::Trusted);

    // A new session keeps its choice for when it's created.
    let _ = app.update(Message::NewSession(None));
    let messages = interact(&mut app, |ui| {
        let _ = ui.click(Id::new(ids::SHIELD));
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::TogglePicker(Picker::Shield)))
    );
    assert!(shows(&app, "Ask before edits and commands"));
    assert!(shows(&app, "The agent's own rules decide"));
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("Ask before edits and commands");
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::PickShield(Shield::Careful)))
    );
    assert_eq!(app.current_shield(), Shield::Careful);
    assert_eq!(app.picker, None, "choosing closes the menu");
}

/// An edit's events, ending with its patch.
fn edit_events(app: &mut App, call: &str, patch: &str, (added, removed): (u64, u64)) {
    let kinds = [
        EventKind::ToolStarted {
            session_id: PI_UPDATE.into(),
            message_id: "msg_e".into(),
            call_id: call.into(),
            name: "edit".into(),
        },
        EventKind::ToolCalled {
            session_id: PI_UPDATE.into(),
            message_id: "msg_e".into(),
            call_id: call.into(),
            input: json!({ "path": "README.md" }),
        },
        EventKind::ToolEnded {
            session_id: PI_UPDATE.into(),
            message_id: "msg_e".into(),
            call_id: call.into(),
            failed: false,
            error: None,
            files: vec![FileDiff {
                file: "README.md".into(),
                patch: patch.into(),
                additions: added,
                deletions: removed,
                status: "modified".into(),
            }],
            child: None,
        },
    ];
    for (kind, at) in kinds.into_iter().zip([1.0, 2.0, 3.0]) {
        let _ = app.update(event(kind, at));
    }
    support::settle(app);
}

#[test]
fn an_edit_shows_its_diff_under_the_row() {
    let mut app = chatting();
    edit_events(
        &mut app,
        "call_e",
        "@@ -11 +11 @@\n-Open Blender; it serves 9876.\n+Open Blender; it serves 9877.\n",
        (1, 1),
    );
    assert!(shows(&app, "Edit"));
    assert!(shows(&app, "+1"));
    assert!(shows(&app, "−1"));
    assert!(shows(&app, "11"), "line numbers in the gutter");

    // A long one shows its first lines, and the rest on request.
    let long = (1..=30).fold(String::new(), |all, n| all + &format!("+line {n}\n"));
    edit_events(
        &mut app,
        "call_l",
        &format!("@@ -0,0 +1,30 @@\n{long}"),
        (30, 0),
    );
    // Two calls fold into a group, closed once they're done.
    let _ = app.update(Message::Timeline(
        PI_UPDATE.into(),
        harlo::timeline::Event::ToggleGroup("msg_e/tool/call_e".into()),
    ));
    assert!(shows(&app, "Edited 2 files"));
    assert!(shows(&app, "… 18 more lines"));
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("… 18 more lines");
    });
    assert!(
        messages.iter().any(|m| matches!(
            m,
            Message::Timeline(_, harlo::timeline::Event::ToggleDiff(_))
        )),
        "{messages:?}"
    );
    assert!(!shows(&app, "… 18 more lines"), "shown whole");
}

/// "pi update" with one stored prompt and the agent's edit after it.
fn with_history() -> App {
    let mut app = chatting();
    let rows = vec![
        Entry::Prompt {
            key: "msg_p1".into(),
            text: "Change the port to 9877".into(),
            attachments: Vec::new(),
            skills: Vec::new(),
        },
        Entry::Answer {
            key: "msg_a1/text/0".into(),
            text: "Done.".into(),
        },
    ];
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(rows)));
    support::settle(&mut app);
    app
}

fn patch(file: &str, patch: &str) -> FileDiff {
    FileDiff {
        file: file.into(),
        patch: patch.into(),
        additions: 1,
        deletions: 1,
        status: "modified".into(),
    }
}

#[test]
fn undo_shows_what_goes_back_and_whose_edits_it_overwrites() {
    let mut app = with_history();
    assert!(!shows(&app, "Undo"), "offered only under the pointer");
    let _ = app.update(Message::Timeline(
        PI_UPDATE.into(),
        harlo::timeline::Event::Hover(Some("msg_p1".into())),
    ));
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("Undo");
    });
    assert!(
        messages.iter().any(|m| matches!(
            m,
            Message::Timeline(_, harlo::timeline::Event::Undo(id)) if id == "msg_p1"
        )),
        "{messages:?}"
    );

    // Staged: the agent changed 9876 to 9877, and the user added a line since.
    let agent = vec![patch("README.md", "@@ -1 +1 @@\n-port 9876\n+port 9877\n")];
    let revert = Revert {
        message_id: "msg_p1".into(),
        files: vec![patch(
            "README.md",
            "@@ -1,2 +1 @@\n-port 9877\n-my own note\n+port 9876\n",
        )],
    };
    let _ = app.update(Message::UndoStaged(PI_UPDATE.into(), Ok((agent, revert))));
    assert!(shows(
        &app,
        "Undo “Change the port to 9877” and everything after it?"
    ));
    assert!(shows(&app, "README.md"));
    assert!(shows(
        &app,
        "This overwrites your own edits to README.md (1 line)."
    ));

    let messages = interact(&mut app, |ui| {
        let _ = ui.click(Id::new(ids::UNDO_KEEP));
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::UndoKeep(id) if id == PI_UPDATE)),
        "{messages:?}"
    );
    let _ = app.update(Message::UndoSettled {
        session: PI_UPDATE.into(),
        kept: true,
        result: Ok(()),
    });
    assert!(!shows(&app, "and everything after it?"), "the card goes");
    assert_eq!(
        app.composer.text().trim(),
        "Change the port to 9877",
        "the prompt comes back to send again"
    );
}

#[test]
fn a_cancelled_undo_leaves_the_composer_alone() {
    let mut app = with_history();
    let revert = Revert {
        message_id: "msg_p1".into(),
        files: Vec::new(),
    };
    let _ = app.update(Message::UndoStaged(
        PI_UPDATE.into(),
        Ok((Vec::new(), revert)),
    ));
    assert!(shows(
        &app,
        "No files change; the conversation goes back to before this prompt."
    ));
    let _ = app.update(Message::UndoSettled {
        session: PI_UPDATE.into(),
        kept: false,
        result: Ok(()),
    });
    assert!(!shows(&app, "and everything after it?"));
    assert!(app.composer.text().trim().is_empty());
}

#[test]
fn a_working_session_offers_no_undo() {
    let mut app = with_history();
    let _ = app.update(event(
        EventKind::Busy {
            session_id: PI_UPDATE.into(),
        },
        1.0,
    ));
    let _ = app.update(Message::Timeline(
        PI_UPDATE.into(),
        harlo::timeline::Event::Hover(Some("msg_p1".into())),
    ));
    assert!(!shows(&app, "Undo"));
}
