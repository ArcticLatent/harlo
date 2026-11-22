//! The Notes panel: a note per folder, todos in it, and putting it into
//! the message being written.
//!
//! The demo app has no database, so notes start empty and live for the run.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::Arc;

use harlo::{App, Message, NotesEvent, Panel};
use iced::widget::text_editor::{Action, Cursor, Edit, Motion, Position};
use support::{demo, interact, shows};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn notes(event: NotesEvent) -> Message {
    Message::Notes(event)
}

/// The demo app with "pi update" open and the Notes panel showing.
fn with_notes() -> App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::OpenPanel(Panel::Notes));
    app
}

fn write(app: &mut App, text: &str) {
    let key = app.note_key();
    let _ = app.update(notes(NotesEvent::Edit(
        key,
        Action::Edit(Edit::Paste(Arc::new(text.to_owned()))),
    )));
}

fn text(app: &App) -> String {
    app.note().expect("a note").content.text()
}

#[test]
fn each_folder_has_a_note() {
    let mut app = with_notes();
    let folder = app.note_key().folder;
    let name = folder.rsplit('/').find(|p| !p.is_empty()).expect("a name");
    assert!(shows(&app, name), "the folder it belongs to");
    write(&mut app, "Ship on Friday");
    assert_eq!(text(&app), "Ship on Friday");
    assert!(app.note().is_some_and(harlo::app::Note::is_dirty));

    // A pause in typing saves it (here, for the run only).
    let _ = app.update(notes(NotesEvent::SaveDue {
        key: app.note_key(),
        edit: 1,
    }));
    assert!(!app.note().is_some_and(harlo::app::Note::is_dirty));

    // Another folder has a note of its own.
    let _ = app.update(Message::NewSession(Some("/tmp/elsewhere".into())));
    assert_eq!(app.note_key().folder, "/tmp/elsewhere");
    assert_eq!(text(&app), "");
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    assert_eq!(text(&app), "Ship on Friday");
}

#[test]
fn todos_are_added_counted_and_ticked() {
    let mut app = with_notes();
    write(&mut app, "Plan");
    let _ = interact(&mut app, |ui| {
        let _ = ui.click("Add a todo");
    });
    write(&mut app, "write the changelog");
    assert_eq!(text(&app), "Plan\n- [ ] write the changelog");
    assert!(shows(&app, "1 of 1 todos open"));

    // Ctrl+Enter ticks the todo under the cursor, and unticks it.
    let _ = app.update(notes(NotesEvent::ToggleTodo));
    assert_eq!(text(&app), "Plan\n- [x] write the changelog");
    assert!(shows(&app, "All 1 todos done"));
    let _ = app.update(notes(NotesEvent::ToggleTodo));
    assert_eq!(text(&app), "Plan\n- [ ] write the changelog");

    // On a plain line, it makes a todo of it.
    let key = app.note_key();
    let _ = app.update(notes(NotesEvent::Edit(
        key,
        Action::Move(Motion::DocumentStart),
    )));
    let _ = app.update(notes(NotesEvent::ToggleTodo));
    assert_eq!(text(&app), "- [ ] Plan\n- [ ] write the changelog");
    assert!(shows(&app, "2 of 2 todos open"));
}

#[test]
fn the_note_goes_into_the_message() {
    let mut app = with_notes();
    write(&mut app, "first line\nsecond line\n");
    let _ = interact(&mut app, |ui| {
        let _ = ui.click("Insert into message");
    });
    assert_eq!(app.composer.text(), "first line\nsecond line");

    // With a selection, only that goes in, after what's already written.
    let mut app = with_notes();
    write(&mut app, "first line\nsecond line");
    let note = app.notes.by_key.get_mut(&app.note_key()).expect("a note");
    note.content.move_to(Cursor {
        position: Position { line: 1, column: 6 },
        selection: Some(Position { line: 1, column: 0 }),
    });
    let _ = app.update(Message::Compose(Action::Edit(Edit::Paste(Arc::new(
        "See:".to_owned(),
    )))));
    let _ = app.update(notes(NotesEvent::Insert));
    assert_eq!(app.composer.text(), "See:\nsecond");
}
