//! An open session in the window: a run streaming in, the queue, and the
//! cards asking for approvals, answers and undo.
//!
//! Each test opens the real window headless; server events go through
//! `App::update`, and presses in the window send their messages there too.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{Message, Sheet, SheetEvent};
use harlo::timeline::Event as TimelineEvent;
use harlo_core::approvals::Shield;
use harlo_core::server::{
    Answer, Choice, Delivery, Field, FieldKind, FileDiff, PermissionReply, Update,
};
use harlo_core::timeline::Entry;
use harlo_core::workspace::{Event, EventKind, Fetched, Form, InboxItem, Permission, Revert};
use i_slint_backend_testing::AccessibleRole;
use serde_json::json;
use slint::platform::WindowEvent;
use slint::{ComponentHandle, Model as _};
use support::{Ui, demo, ui, ui_at};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn event(kind: EventKind, time: f64) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time,
        kind,
    }))
}

/// The window with "pi update" open and its history empty.
fn chatting() -> Ui {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let ui = ui(app);
    let _ = ui.messages();
    ui
}

fn busy(ui: &Ui) {
    ui.send(event(
        EventKind::Busy {
            session_id: PI_UPDATE.into(),
        },
        0.0,
    ));
}

fn tool_events(ui: &Ui, call: &str, name: &str, input: serde_json::Value, at: f64) {
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
        ui.send(event(kind, time));
    }
}

fn tool_ended(ui: &Ui, message: &str, call: &str, files: Vec<FileDiff>, time: f64) {
    ui.send(event(
        EventKind::ToolEnded {
            session_id: PI_UPDATE.into(),
            message_id: message.into(),
            call_id: call.into(),
            failed: false,
            error: None,
            files,
            child: None,
        },
        time,
    ));
}

#[test]
fn a_run_streams_in_and_its_tools_fold_up_when_done() {
    let ui = chatting();
    busy(&ui);
    ui.send(event(
        EventKind::InboxAdded {
            session_id: PI_UPDATE.into(),
            item: InboxItem {
                id: "msg_u".into(),
                kind: "user".into(),
                text: Some("Look around the docs".into()),
                attachments: Vec::new(),
                skills: Vec::new(),
                delivery: Delivery::Queue,
            },
        },
        1.0,
    ));
    ui.send(event(
        EventKind::InboxRemoved {
            session_id: PI_UPDATE.into(),
            item_id: "msg_u".into(),
            delivered: true,
        },
        2.0,
    ));
    tool_events(&ui, "c1", "glob", json!({ "pattern": "docs/*.md" }), 100.0);
    tool_events(&ui, "c2", "read", json!({ "filePath": "docs/a.md" }), 200.0);
    for text in [
        "Searched 1 time, read 1 file",
        "Glob",
        "docs/*.md",
        "Stop",
        "Working",
    ] {
        assert!(ui.shows(text), "shows {text:?} while running");
    }

    for delta in ["Found ", "**two** files."] {
        ui.send(event(
            EventKind::TextDelta {
                session_id: PI_UPDATE.into(),
                message_id: "msg_b".into(),
                ordinal: 0,
                delta: delta.into(),
            },
            300.0,
        ));
    }
    let answer = ui
        .app()
        .timeline_entries(PI_UPDATE)
        .unwrap_or_default()
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Answer { key, text, .. } if key == "msg_b/text/0" => Some(text),
            _ => None,
        });
    assert_eq!(answer.as_deref(), Some("Found **two** files."));

    tool_ended(&ui, "msg_a", "c1", Vec::new(), 1_600.0);
    tool_ended(&ui, "msg_a", "c2", Vec::new(), 2_400.0);
    ui.send(event(
        EventKind::Done {
            session_id: PI_UPDATE.into(),
            outcome: "succeeded".into(),
        },
        2_500.0,
    ));
    assert!(ui.shows("Searched 1 time, read 1 file"));
    assert!(!ui.shows("Glob"), "the finished group is folded");
    assert!(!ui.shows("Stop"));

    // Opening the group shows its calls with their times.
    ui.press("Searched 1 time, read 1 file");
    for text in ["Glob", "docs/*.md", "1.5s", "Read", "docs/a.md", "2.2s"] {
        assert!(ui.shows(text), "shows {text:?} once opened");
    }
}

#[test]
fn a_busy_session_queues_steers_and_stops() {
    let ui = chatting();
    busy(&ui);
    ui.send(Message::Fetched(Ok(Fetched::Inbox(
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
    assert!(ui.shows("Then update the changelog"));

    ui.press("Steer");
    assert!(
        ui.messages()
            .iter()
            .any(|m| matches!(m, Message::SteerQueued(id) if id == "msg_q"))
    );
    ui.press("Cancel");
    assert!(
        ui.messages()
            .iter()
            .any(|m| matches!(m, Message::CancelQueued(id) if id == "msg_q"))
    );
    ui.press("Stop");
    assert!(ui.messages().iter().any(|m| matches!(m, Message::Stop)));

    // Stopped, OpenCode holds what's waiting until the next prompt.
    ui.send(event(
        EventKind::Done {
            session_id: PI_UPDATE.into(),
            outcome: "interrupted".into(),
        },
        1.0,
    ));
    assert!(ui.shows("Then update the changelog"));
    assert!(ui.shows("Sends before your next message"));
    assert!(!ui.shows("Steer"), "steering needs a running session");
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
fn marked_waiting(ui: &Ui) -> bool {
    ui.app()
        .projects
        .iter()
        .flat_map(|p| p.sessions.iter())
        .any(|s| s.id == PI_UPDATE && s.waiting)
}

/// Presses `key` with Alt held, as a keyboard would.
fn alt(ui: &Ui, key: char) {
    let window = ui.window.window();
    let alt = slint::SharedString::from(slint::platform::Key::Alt);
    let key = slint::SharedString::from(key);
    window.dispatch_event(WindowEvent::KeyPressed { text: alt.clone() });
    window.dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text: key });
    window.dispatch_event(WindowEvent::KeyReleased { text: alt });
}

#[test]
fn alt_a_and_alt_d_answer_the_approval_on_screen() {
    let ui = chatting();
    ui.send(event(
        EventKind::PermissionAsked(approval("per_1", "shell", &["cargo test"])),
        1.0,
    ));
    assert!(ui.shows("Alt+A"), "the key shows on Allow once");
    assert!(ui.shows("Alt+D"), "and on Deny");

    alt(&ui, 'a');
    assert!(
        ui.messages().iter().any(|m| matches!(
            m,
            Message::Approve { request, reply: PermissionReply::Once } if request == "per_1"
        )),
        "Alt+A allows once"
    );
    ui.send(Message::Answered(PI_UPDATE.into(), "per_1".into(), Ok(())));

    ui.send(event(
        EventKind::PermissionAsked(approval("per_2", "shell", &["rm -rf build"])),
        2.0,
    ));
    // Under Settings, the card is out of sight: the keys leave it be.
    ui.send(Message::Sheet(SheetEvent::Toggle(Sheet::Settings)));
    alt(&ui, 'd');
    assert!(
        !ui.messages()
            .iter()
            .any(|m| matches!(m, Message::Approve { .. })),
        "nothing answered behind Settings"
    );
    ui.send(Message::Sheet(SheetEvent::Close));
    alt(&ui, 'd');
    assert!(
        ui.messages().iter().any(|m| matches!(
            m,
            Message::Approve { request, reply: PermissionReply::Reject } if request == "per_2"
        )),
        "Alt+D denies"
    );
}

#[test]
fn an_approval_card_allows_or_refuses_and_goes_once_answered() {
    let ui = chatting();
    ui.send(event(
        EventKind::PermissionAsked(approval("per_1", "shell", &["cargo test"])),
        1.0,
    ));
    assert!(ui.shows("Allow Build to run a command?"));
    assert!(ui.shows("cargo test"));
    // What Always allow covers is said in words, on a line of its own,
    // and read out with the button.
    let covers = "Always allow covers commands starting with “cargo” in this project, \
                  for every session. Settings → Agents → Saved approvals takes it back.";
    assert!(ui.shows(covers), "says what it covers and where it's kept");
    assert_eq!(
        ui.button("Always allow")
            .accessible_description()
            .as_deref(),
        Some(covers)
    );
    assert!(marked_waiting(&ui), "the sidebar marks the session");

    ui.press("Allow once");
    assert!(
        ui.messages().iter().any(|m| matches!(
            m,
            Message::Approve { request, reply: PermissionReply::Once } if request == "per_1"
        )),
        "allowed once"
    );

    // While the answer is on its way, the card takes no other.
    assert_eq!(ui.button("Deny").accessible_enabled(), Some(false));
    // A failed answer says so, and the card can be answered again.
    ui.send(Message::Answered(
        PI_UPDATE.into(),
        "per_1".into(),
        Err("Couldn't answer: the server went away".into()),
    ));
    assert!(ui.shows("Couldn't answer: the server went away"));
    ui.press("Always allow");
    assert!(ui.messages().iter().any(|m| matches!(
        m,
        Message::Approve {
            reply: PermissionReply::Always,
            ..
        }
    )));
    ui.send(Message::Answered(
        PI_UPDATE.into(),
        "per_1".into(),
        Err("Still away".into()),
    ));

    // Refusing with a reason swaps the buttons for a box to say why.
    ui.press("Deny with a reason…");
    assert!(
        ui.messages()
            .iter()
            .any(|m| matches!(m, Message::Refuse(id) if id == "per_1"))
    );
    assert!(!ui.shows("Allow once"));
    ui.fill("Tell Build what to do instead", "run cargo check");
    assert_eq!(
        ui.app()
            .refusing
            .as_ref()
            .map(|(_, reason)| reason.as_str()),
        Some("run cargo check")
    );
    ui.press("Cancel");
    assert!(ui.shows("Allow once"), "cancelled, the buttons are back");

    // Answered, here or elsewhere, the card goes.
    ui.send(Message::Answered(PI_UPDATE.into(), "per_1".into(), Ok(())));
    assert!(!ui.shows("Allow Build to run a command?"));
    assert!(!marked_waiting(&ui));
}

#[test]
fn an_edit_approval_shows_the_change_and_how_many_wait() {
    let ui = chatting();
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
    ui.send(event(EventKind::PermissionAsked(edit), 1.0));
    ui.send(event(
        EventKind::PermissionAsked(approval("per_2", "shell", &["ls"])),
        2.0,
    ));
    assert!(ui.shows("Allow Build to edit a file?"));
    assert!(ui.shows("e.txt"));
    assert!(ui.shows("three"), "the patch's lines");
    assert!(ui.shows("two"));
    assert!(ui.shows("1 of 2"));

    // The first answered, the next one shows.
    ui.send(Message::Answered(PI_UPDATE.into(), "per_1".into(), Ok(())));
    assert!(ui.shows("Allow Build to run a command?"));
    assert!(ui.shows("ls"));
    assert!(!ui.shows("1 of 2"));
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
            field("ship", "Ship it?", FieldKind::Boolean { default: None }),
        ],
    }
}

#[test]
fn a_short_window_keeps_the_question_cards_buttons_in_view() {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let ui = ui_at(app, (1000.0, 760.0));
    // Taller than the card can be: the first two questions twice more.
    let mut form = colour_question();
    let more: Vec<_> = form.fields[..2].to_vec();
    for (n, mut field) in more
        .into_iter()
        .chain(form.fields[..2].to_vec())
        .enumerate()
    {
        field.key = format!("{}{n}", field.key);
        form.fields.push(field);
    }
    ui.send(event(EventKind::FormCreated(form), 1.0));
    assert!(ui.shows("Build has a question"));

    // The questions scroll; Send and Dismiss stay above the composer.
    let composer = ui
        .find("Send a message to continue this session · Tab switches agents")
        .into_iter()
        .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("the composer");
    let composer_top = composer.absolute_position().y;
    // The whole card shows, its foot included: the composer's text sits
    // 16px inside its box, and 16px of gaps part the box from the cards.
    let card = ui
        .find("Build has a question")
        .into_iter()
        .find(|e| e.accessible_role() == Some(AccessibleRole::Groupbox))
        .expect("the card");
    let card_bottom = card.absolute_position().y + card.size().height;
    assert!(
        card_bottom <= composer_top - 30.0,
        "the card ends at {card_bottom}, too near the composer's text at {composer_top}"
    );
    for label in ["Send answer", "Dismiss"] {
        let button = ui.button(label);
        let bottom = button.absolute_position().y + button.size().height;
        assert!(
            bottom <= composer_top,
            "{label} ends at {bottom}, under the composer's top at {composer_top}"
        );
        assert!(button.absolute_position().y > 0.0, "{label} is on screen");
    }
}

#[test]
fn a_question_card_takes_answers_and_says_what_is_missing() {
    let ui = chatting();
    ui.send(event(EventKind::FormCreated(colour_question()), 1.0));
    assert!(ui.shows("Build has a question"));
    assert!(ui.shows("Which colour?"));
    assert!(marked_waiting(&ui));

    ui.press("Blue");
    assert!(ui.messages().iter().any(|m| matches!(
        m,
        Message::FormChoose { key, value, .. } if key == "colour" && value == "Blue"
    )));
    ui.press("Yes");
    assert!(ui.messages().iter().any(|m| matches!(
        m,
        Message::FormFlag { key, flag: true, .. } if key == "ship"
    )));

    // Nothing chosen for the extras yet: sending says so instead.
    ui.press("Send answer");
    assert!(ui.shows("Needs an answer"));
    ui.press("Docs");
    assert!(!ui.shows("Needs an answer"), "answering clears it");
    let form = colour_question();
    let answers = ui
        .app()
        .drafts
        .get("frm_1")
        .map(|draft| draft.answers(&form));
    assert_eq!(
        answers,
        Some(Ok(vec![
            ("colour".into(), Answer::Text("Blue".into())),
            ("extras".into(), Answer::List(vec!["Docs".into()])),
            ("ship".into(), Answer::Boolean(true)),
        ]))
    );

    // Something else typed replaces the chosen colour.
    ui.fill("Something else…", "Green");
    assert!(ui.messages().iter().any(|m| matches!(
        m,
        Message::FormType { key, text, .. } if key == "colour" && text == "Green"
    )));

    ui.send(Message::Answered(PI_UPDATE.into(), "frm_1".into(), Ok(())));
    assert!(!ui.shows("Build has a question"));
    assert!(!marked_waiting(&ui));
}

/// An edit's events, ending with its patch.
fn edit_events(ui: &Ui, call: &str, patch: &str, (added, removed): (u64, u64)) {
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
    ];
    for (kind, at) in kinds.into_iter().zip([1.0, 2.0]) {
        ui.send(event(kind, at));
    }
    let file = FileDiff {
        file: "README.md".into(),
        patch: patch.into(),
        additions: added,
        deletions: removed,
        status: "modified".into(),
    };
    tool_ended(ui, "msg_e", call, vec![file], 3.0);
}

#[test]
fn an_edit_shows_its_diff_under_the_row() {
    let ui = chatting();
    edit_events(
        &ui,
        "call_e",
        "@@ -11 +11 @@\n-Open Blender; it serves 9876.\n+Open Blender; it serves 9877.\n",
        (1, 1),
    );
    assert!(ui.shows("Edit"));
    assert!(ui.shows("+1"));
    assert!(ui.shows("−1"));
    assert!(ui.shows("11"), "line numbers in the gutter");
    assert!(ui.shows("Open Blender; it serves 9877."));

    // Clicking a changed file reviews it.
    let line = ui.find("Open Blender; it serves 9877.");
    assert!(!line.is_empty());

    // A long one shows its first lines, and the rest on request.
    let long = (1..=30).fold(String::new(), |all, n| all + &format!("+line {n}\n"));
    edit_events(&ui, "call_l", &format!("@@ -0,0 +1,30 @@\n{long}"), (30, 0));
    // Two calls fold into a group, closed once they're done.
    ui.send(Message::Timeline(
        PI_UPDATE.into(),
        TimelineEvent::ToggleGroup("msg_e/tool/call_e".into()),
    ));
    assert!(ui.shows("Edited 2 files"));
    assert!(ui.shows("… 18 more lines"));
    ui.press("… 18 more lines");
    assert!(
        ui.messages()
            .iter()
            .any(|m| matches!(m, Message::Timeline(_, TimelineEvent::ToggleDiff(_)))),
        "asks for the whole diff"
    );
    assert!(!ui.shows("… 18 more lines"), "shown whole");
}

/// "pi update" with one stored prompt and an answer after it.
fn with_history() -> Ui {
    let ui = chatting();
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
    ui.send(Message::History(PI_UPDATE.into(), Ok(rows)));
    let _ = ui.messages();
    ui
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
    let ui = with_history();
    assert!(!ui.shows("Undo"), "offered only under the pointer");
    ui.hover("Change the port to 9877");
    ui.press("Undo");
    assert!(
        ui.messages().iter().any(|m| matches!(
            m,
            Message::Timeline(_, TimelineEvent::Undo(id)) if id == "msg_p1"
        )),
        "undo asked for"
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
    ui.send(Message::UndoStaged(PI_UPDATE.into(), Ok((agent, revert))));
    assert!(ui.shows("Undo “Change the port to 9877” and everything after it?"));
    assert!(ui.shows("README.md"));
    assert!(ui.shows("This overwrites your own edits to README.md (1 line)."));

    // The card's own Undo, not the prompt's.
    let card = ui
        .find("Undo")
        .into_iter()
        .rfind(|e| e.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::Button))
        .expect("the card's Undo");
    card.invoke_accessible_default_action();
    assert!(
        ui.messages()
            .iter()
            .any(|m| matches!(m, Message::UndoKeep(id) if id == PI_UPDATE)),
        "kept"
    );
    ui.send(Message::UndoSettled {
        session: PI_UPDATE.into(),
        kept: true,
        result: Ok(()),
    });
    assert!(!ui.shows("Undo “Change the port to 9877” and everything after it?"));
    assert_eq!(
        ui.app().composer.text().trim(),
        "Change the port to 9877",
        "the prompt comes back to send again"
    );
}

#[test]
fn a_cancelled_undo_leaves_the_composer_alone() {
    let ui = with_history();
    let revert = Revert {
        message_id: "msg_p1".into(),
        files: Vec::new(),
    };
    ui.send(Message::UndoStaged(
        PI_UPDATE.into(),
        Ok((Vec::new(), revert)),
    ));
    assert!(ui.shows("No files change; the conversation goes back to before this prompt."));
    ui.press("Keep everything");
    assert!(
        ui.messages()
            .iter()
            .any(|m| matches!(m, Message::UndoCancel(id) if id == PI_UPDATE))
    );
    ui.send(Message::UndoSettled {
        session: PI_UPDATE.into(),
        kept: false,
        result: Ok(()),
    });
    assert!(!ui.shows("No files change; the conversation goes back to before this prompt."));
    assert!(ui.app().composer.text().trim().is_empty());
}

#[test]
fn a_working_session_offers_no_undo() {
    let ui = with_history();
    busy(&ui);
    ui.hover("Change the port to 9877");
    assert!(!ui.shows("Undo"));
}

/// Whether the timeline shows a row reading `text`, not counting the rail's
/// mark for a prompt of the same words.
fn in_view(ui: &Ui, text: &str) -> bool {
    ui.find(text)
        .iter()
        .any(|e| e.accessible_role() != Some(AccessibleRole::Button))
}

/// A transcript of `count` questions with answers of different lengths.
fn long_history(count: usize) -> Vec<Entry> {
    (0..count)
        .flat_map(|i| {
            [
                Entry::Prompt {
                    key: format!("msg_p{i}"),
                    text: format!("Question number {i}"),
                    attachments: Vec::new(),
                    skills: Vec::new(),
                },
                Entry::Answer {
                    key: format!("msg_a{i}/text/0"),
                    text: "A paragraph long enough to wrap onto a second line of the \
                           transcript, so rows differ in height. "
                        .repeat(1 + i % 4),
                },
            ]
        })
        .collect()
}

/// Lets the timeline catch up with its end, as the event loop's next turns would.
fn settle_at_end(ui: &Ui, newest: &str) {
    for _ in 0..5 {
        // Looking lays the rows out, as a frame would.
        let _ = in_view(ui, newest);
        ui.wait(50);
    }
}

#[test]
fn a_long_session_opens_at_its_end() {
    let ui = chatting();
    ui.send(Message::History(PI_UPDATE.into(), Ok(long_history(300))));
    settle_at_end(&ui, "Question number 299");
    assert!(in_view(&ui, "Question number 299"), "the newest prompt");
    assert!(
        !in_view(&ui, "Question number 0"),
        "the oldest is far above"
    );
}

#[test]
fn scrolled_up_a_button_jumps_back_to_the_end() {
    let ui = chatting();
    ui.send(Message::History(PI_UPDATE.into(), Ok(long_history(60))));
    settle_at_end(&ui, "Question number 59");
    assert!(!ui.shows("Jump to the end"), "none while at the end");

    ui.wheel("Question number 59", 8);
    assert!(!in_view(&ui, "Question number 59"), "scrolled up");
    ui.press("Jump to the end");
    settle_at_end(&ui, "Question number 59");
    assert!(in_view(&ui, "Question number 59"), "back at the newest");
    assert!(!ui.shows("Jump to the end"), "gone again");
}

#[test]
fn another_session_opens_at_its_end_after_scrolling_up() {
    let ui = chatting();
    ui.send(Message::History(PI_UPDATE.into(), Ok(long_history(60))));
    settle_at_end(&ui, "Question number 59");
    ui.wheel("Question number 59", 8);
    assert!(!in_view(&ui, "Question number 59"), "scrolled up");

    let other = "ses_demo_04";
    ui.send(Message::OpenSession(other.into()));
    let rows = long_history(40)
        .into_iter()
        .map(|entry| match entry {
            Entry::Prompt {
                key,
                text,
                attachments,
                skills,
            } => Entry::Prompt {
                key,
                text: text.replace("Question", "Other question"),
                attachments,
                skills,
            },
            entry => entry,
        })
        .collect();
    ui.send(Message::History(other.into(), Ok(rows)));
    settle_at_end(&ui, "Other question number 39");
    assert!(
        in_view(&ui, "Other question number 39"),
        "the other's newest"
    );
    assert!(!ui.shows("Jump to the end"), "following its end");
}

/// How far below the timeline's top the row reading `text` starts.
///
/// # Panics
///
/// Panics if the timeline doesn't show it.
fn below_top(ui: &Ui, text: &str) -> f32 {
    let timeline = ui.find("Timeline").into_iter().next().expect("a timeline");
    let row = ui
        .find(text)
        .into_iter()
        .find(|e| e.accessible_role() != Some(AccessibleRole::Button))
        .unwrap_or_else(|| panic!("no row reading {text:?}"));
    row.absolute_position().y - timeline.absolute_position().y
}

/// Lets the timeline lay out where it was sent, as frames would.
fn settle(ui: &Ui, text: &str) {
    for _ in 0..5 {
        let _ = in_view(ui, text);
        ui.wait(50);
    }
}

#[test]
fn a_prompt_mark_jumps_up_to_its_question() {
    let ui = chatting();
    ui.send(Message::History(PI_UPDATE.into(), Ok(long_history(60))));
    settle_at_end(&ui, "Question number 59");

    ui.press("Question number 20");
    settle(&ui, "Question number 20");
    assert!(in_view(&ui, "Question number 20"), "the question shows");
    assert!(
        (0.0..60.0).contains(&below_top(&ui, "Question number 20")),
        "at the top, {} below it",
        below_top(&ui, "Question number 20")
    );
    assert!(!in_view(&ui, "Question number 59"), "the end is far below");
    assert!(ui.shows("Jump to the end"), "away from the end");
}

#[test]
fn a_narrow_chat_keeps_room_for_the_prompt_rail() {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    // Narrow enough that the widest column would leave no room beside it.
    let ui = ui_at(app, (1000.0, 800.0));
    ui.send(Message::History(PI_UPDATE.into(), Ok(long_history(60))));
    settle_at_end(&ui, "Question number 59");

    ui.press("Question number 20");
    settle(&ui, "Question number 20");
    assert!(
        in_view(&ui, "Question number 20"),
        "the rail is there and works"
    );
}

#[test]
fn a_prompt_mark_jumps_down_to_a_later_question() {
    let ui = chatting();
    ui.send(Message::History(PI_UPDATE.into(), Ok(long_history(60))));
    settle_at_end(&ui, "Question number 59");
    ui.press("Question number 12");
    settle(&ui, "Question number 12");

    ui.press("Question number 40");
    settle(&ui, "Question number 40");
    assert!(
        (0.0..60.0).contains(&below_top(&ui, "Question number 40")),
        "at the top, {} below it",
        below_top(&ui, "Question number 40")
    );
    assert!(
        !in_view(&ui, "Question number 12"),
        "the earlier one is gone"
    );
}

#[test]
fn a_tight_column_keeps_send_inside_the_composer() {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let ui = ui_at(app, (1000.0, 760.0));
    ui.open("Changes");

    let field = ui
        .find("Send a message to continue this session · Tab switches agents")
        .into_iter()
        .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("the composer");
    let field_end = field.absolute_position().x + field.size().width;
    let send = ui.button("Send");
    let send_end = send.absolute_position().x + send.size().width;
    assert!(
        send_end <= field_end + 1.0,
        "Send ends at {send_end}, past the composer's text at {field_end}"
    );
    // The chips shrink to their icons, still named for screen readers.
    let chips = ui.window.global::<harlo::ui::State>().get_chips();
    let labels: Vec<String> = (0..chips.row_count())
        .filter_map(|i| chips.row_data(i))
        .map(|chip| chip.label.to_string())
        .collect();
    assert!(!labels.is_empty(), "the composer has its chips");
    for chip in &labels {
        let button = ui.button(chip);
        assert!(
            button.size().width < 40.0,
            "{chip} is {} wide",
            button.size().width
        );
    }
}

#[test]
fn a_session_that_doesnt_ask_says_so_in_its_header_and_sidebar_row() {
    let ui = chatting();
    assert!(!ui.shows("Don't ask"));
    // Its rules, as another client or the shield menu set them.
    ui.send(event(
        EventKind::SessionPermissions {
            session_id: PI_UPDATE.into(),
            rules: Shield::Trusted.rules().expect("rules"),
        },
        2.0,
    ));
    let marks: Vec<_> = ui
        .find("Don't ask")
        .into_iter()
        .map(|e| e.absolute_position())
        .collect();
    // The header, the composer's shield and the sidebar row.
    assert!(marks.len() >= 3, "marked in {} places", marks.len());
    assert!(marks.iter().any(|at| at.x < 344.0), "on the sidebar row");
}
