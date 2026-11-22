//! The Notes panel: a note per folder, todos in it, and putting it into
//! the message being written.
//!
//! The demo app has no database, so notes start empty and live for the run.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{Message, Note, NotesEvent};
use harlo::text::Edit;
use support::{Ui, demo, ui};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn notes(event: NotesEvent) -> Message {
    Message::Notes(event)
}

/// The window with "pi update" open and the Notes panel showing.
fn with_notes() -> Ui {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let ui = ui(app);
    ui.press("Notes");
    ui
}

fn text(ui: &Ui) -> String {
    ui.app().note().expect("a note").content.text()
}

/// Types `more` at the end of the note, leaving the cursor after it.
fn type_more(ui: &Ui, more: &str) {
    let now = format!("{}{more}", text(ui));
    ui.fill("Note", &now);
    // Filling the field leaves its cursor at the start, as typing wouldn't.
    let key = ui.app().note_key();
    let end = now.len();
    ui.send(notes(NotesEvent::Edit(key, Edit::at(now, end))));
}

fn dirty(ui: &Ui) -> bool {
    ui.app().note().is_some_and(Note::is_dirty)
}

#[test]
fn each_folder_has_a_note() {
    let ui = with_notes();
    let folder = ui.app().note_key().folder;
    let name = folder.rsplit('/').find(|p| !p.is_empty()).expect("a name");
    assert!(ui.shows(name), "the folder it belongs to");
    type_more(&ui, "Ship on Friday");
    assert_eq!(text(&ui), "Ship on Friday");
    assert!(dirty(&ui));

    // A pause in typing saves it (here, for the run only).
    let key = ui.app().note_key();
    ui.send(notes(NotesEvent::SaveDue { key, edit: 1 }));
    assert!(!dirty(&ui));

    // Another folder has a note of its own.
    ui.send(Message::NewSession(Some("/tmp/elsewhere".into())));
    assert_eq!(ui.app().note_key().folder, "/tmp/elsewhere");
    assert_eq!(text(&ui), "");
    ui.send(Message::OpenSession(PI_UPDATE.into()));
    assert_eq!(text(&ui), "Ship on Friday");
}

#[test]
fn todos_are_added_counted_and_ticked() {
    let ui = with_notes();
    type_more(&ui, "Plan");
    ui.press("Add a todo");
    type_more(&ui, "write the changelog");
    assert_eq!(text(&ui), "Plan\n- [ ] write the changelog");
    assert!(ui.shows("1 of 1 todos open"));

    // Ctrl+Enter ticks the todo under the cursor, and unticks it.
    ui.send(notes(NotesEvent::ToggleTodo));
    assert_eq!(text(&ui), "Plan\n- [x] write the changelog");
    assert!(ui.shows("All 1 todos done"));
    ui.send(notes(NotesEvent::ToggleTodo));
    assert_eq!(text(&ui), "Plan\n- [ ] write the changelog");

    // On a plain line, it makes a todo of it.
    let key = ui.app().note_key();
    ui.send(notes(NotesEvent::Edit(key, Edit::at(text(&ui), 0))));
    ui.send(notes(NotesEvent::ToggleTodo));
    assert_eq!(text(&ui), "- [ ] Plan\n- [ ] write the changelog");
    assert!(ui.shows("2 of 2 todos open"));
}

#[test]
fn the_note_goes_into_the_message() {
    let ui = with_notes();
    assert!(
        !ui.button("Insert into message")
            .accessible_enabled()
            .unwrap_or(true),
        "nothing to insert yet"
    );
    type_more(&ui, "first line\nsecond line\n");
    ui.press("Insert into message");
    assert_eq!(ui.app().composer.text(), "first line\nsecond line");

    // With a selection, only that goes in, after what's already written.
    let ui = with_notes();
    let key = ui.app().note_key();
    ui.send(notes(NotesEvent::Edit(
        key,
        Edit {
            text: "first line\nsecond line".into(),
            cursor: 17,
            anchor: 11,
        },
    )));
    ui.send(Message::Compose(Edit::at("See:", 4)));
    ui.press("Insert into message");
    assert_eq!(ui.app().composer.text(), "See:\nsecond");
}
