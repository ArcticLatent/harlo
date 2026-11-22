//! The Notes panel: the current folder's note, its todo count, and how its
//! last save went.

// Rust guideline compliant 2026-10-09

use harlo_core::notes::todo_counts;

use crate::app::{App, NoteKey, Panel};
use crate::ui::State;

/// What the note's field last loaded.
#[derive(Debug, Default)]
pub struct Notes {
    /// The note and text revision the field last loaded.
    loaded: Option<(NoteKey, u64)>,
    /// Counts the times the field was given new text.
    revision: i32,
}

impl Notes {
    /// Fills the Notes panel, when it's on screen.
    pub fn show(&mut self, app: &App, state: &State<'_>) {
        if !app.panel_shown(Panel::Notes) {
            // Shown again, the field loads afresh.
            self.loaded = None;
            return;
        }
        let key = app.note_key();
        let name = key
            .folder
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or("/");
        state.set_notes_folder(name.into());
        let Some(note) = app.note().filter(|note| note.loaded) else {
            state.set_notes_loaded(false);
            return;
        };
        state.set_notes_loaded(true);
        let now = (key, note.content.revision());
        if self.loaded.as_ref() != Some(&now) {
            self.loaded = Some(now);
            self.revision = self.revision.wrapping_add(1);
            state.set_notes_text(note.content.as_str().into());
            state.set_notes_cursor(offset(note.content.offset()));
            state.set_notes_anchor(offset(note.content.anchor()));
            state.set_notes_revision(self.revision);
        }
        let text = note.content.as_str();
        state.set_notes_has_text(!text.trim().is_empty());
        let (open, all) = todo_counts(text);
        let todos = match (open, all) {
            (_, 0) => String::new(),
            (0, all) => format!("All {all} todos done"),
            (open, all) => format!("{open} of {all} todos open"),
        };
        state.set_notes_todos(todos.into());
        state.set_notes_error(note.error.clone().unwrap_or_default().into());
    }
}

/// A byte offset as the UI's `int`.
fn offset(at: usize) -> i32 {
    i32::try_from(at).unwrap_or(i32::MAX)
}
