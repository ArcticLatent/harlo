//! Notes: a note per folder in the Notes panel, saved in Harlo's database
//! shortly after typing stops.
//!
//! Lines like `- [ ] task` are todos: Ctrl+Enter ticks the one under the
//! cursor, and the panel counts the open ones. Part or all of a note can be
//! put into the message being written.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use harlo_core::notes;
use iced::Task;
use iced::widget::{Id, text_editor};

use super::{App, Message, Panel};
use crate::ids;

/// Something that happened to a note.
#[derive(Debug, Clone)]
pub enum NotesEvent {
    /// A note was read from the database.
    Loaded {
        /// The note's server and folder.
        key: NoteKey,
        /// Its text, or why it couldn't be read.
        text: Result<String, String>,
    },
    /// The user edited the note of a folder.
    Edit(NoteKey, text_editor::Action),
    /// Typing paused: save the note if nothing was typed since `edit`.
    SaveDue {
        /// The note's server and folder.
        key: NoteKey,
        /// The edit the pause followed.
        edit: u64,
    },
    /// A save finished.
    Saved {
        /// The note's server and folder.
        key: NoteKey,
        /// The text saved.
        text: String,
        /// Whether it worked.
        result: Result<(), String>,
    },
    /// Tick or untick the todo on the cursor's line, or make the line one.
    ToggleTodo,
    /// Add an open todo at the end and put the cursor there.
    AddTodo,
    /// Put the selection, or the whole note, into the message being written.
    Insert,
}

/// A note's server (by its location label) and folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteKey {
    /// The server's location label.
    pub location: String,
    /// The folder.
    pub folder: String,
}

/// One folder's note.
pub struct Note {
    /// The text being edited.
    pub content: text_editor::Content,
    /// Whether it's been read from the database.
    pub loaded: bool,
    /// The text as last saved or read.
    saved: String,
    /// Counts edits, so only the last pause in typing saves.
    edits: u64,
    /// Why the last load or save failed.
    pub error: Option<String>,
}

impl fmt::Debug for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Note")
            .field("loaded", &self.loaded)
            .field("saved", &self.saved.len())
            .field("edits", &self.edits)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl Note {
    fn new() -> Self {
        Self {
            content: text_editor::Content::new(),
            loaded: false,
            saved: String::new(),
            edits: 0,
            error: None,
        }
    }

    /// Whether the text differs from what's saved.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.content.text() != self.saved
    }
}

/// Every note opened so far.
#[derive(Debug, Default)]
pub struct Notes {
    /// Notes by server and folder.
    pub by_key: HashMap<NoteKey, Note>,
}

impl App {
    /// The key of the note the Notes panel shows: the current folder's.
    #[must_use]
    pub fn note_key(&self) -> NoteKey {
        NoteKey {
            location: self.location.label(),
            folder: self.folder(),
        }
    }

    /// The note the Notes panel shows, once it's been asked for.
    #[must_use]
    pub fn note(&self) -> Option<&Note> {
        self.notes.by_key.get(&self.note_key())
    }

    /// Reads the current folder's note once the Notes panel is shown.
    pub(super) fn follow_notes(&mut self) -> Task<Message> {
        if self.panel != Some(Panel::Notes) || !self.shown().panel {
            return Task::none();
        }
        let key = self.note_key();
        if self.notes.by_key.contains_key(&key) {
            return Task::none();
        }
        let mut note = Note::new();
        let Some(store) = self.store.clone() else {
            // Nowhere to keep notes: start empty, for this run only.
            note.loaded = true;
            self.notes.by_key.insert(key, note);
            return Task::none();
        };
        self.notes.by_key.insert(key.clone(), note);
        let (location, folder) = (key.location.clone(), key.folder.clone());
        Task::perform(store.load_note_later(location, folder), move |text| {
            Message::Notes(NotesEvent::Loaded {
                key: key.clone(),
                text: text.map_err(|e| e.to_string()),
            })
        })
    }

    /// Handles a [`NotesEvent`].
    pub(super) fn notes_action(&mut self, event: NotesEvent) -> Task<Message> {
        match event {
            NotesEvent::Loaded { key, text } => {
                let Some(note) = self.notes.by_key.get_mut(&key) else {
                    return Task::none();
                };
                note.loaded = true;
                match text {
                    Ok(text) => {
                        note.content = text_editor::Content::with_text(&text);
                        note.saved = text;
                    }
                    Err(error) => note.error = Some(format!("Couldn't read the note: {error}")),
                }
            }
            NotesEvent::Edit(key, action) => {
                let Some(note) = self.notes.by_key.get_mut(&key) else {
                    return Task::none();
                };
                // Typing before the saved text arrives would be overwritten by it.
                if !note.loaded {
                    return Task::none();
                }
                let edit = action.is_edit();
                note.content.perform(action);
                if edit {
                    return Self::save_later(key, note);
                }
            }
            NotesEvent::SaveDue { key, edit } => return self.save_note(key, edit),
            NotesEvent::Saved { key, text, result } => {
                if let Some(note) = self.notes.by_key.get_mut(&key) {
                    match result {
                        Ok(()) => {
                            note.saved = text;
                            note.error = None;
                        }
                        Err(error) => note.error = Some(format!("Couldn't save the note: {error}")),
                    }
                }
            }
            NotesEvent::ToggleTodo => {
                let key = self.note_key();
                let Some(note) = self.loaded_note(&key) else {
                    return Task::none();
                };
                toggle_line(&mut note.content);
                return Self::save_later(key, note);
            }
            NotesEvent::AddTodo => {
                let key = self.note_key();
                let Some(note) = self.loaded_note(&key) else {
                    return Task::none();
                };
                note.content =
                    text_editor::Content::with_text(&notes::add_todo(&note.content.text()));
                note.content
                    .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
                let save = Self::save_later(key, note);
                return Task::batch([save, iced::widget::operation::focus(Id::new(ids::NOTES))]);
            }
            NotesEvent::Insert => return self.insert_note(),
        }
        Task::none()
    }

    fn loaded_note(&mut self, key: &NoteKey) -> Option<&mut Note> {
        self.notes.by_key.get_mut(key).filter(|note| note.loaded)
    }

    /// Saves `note` once typing pauses.
    fn save_later(key: NoteKey, note: &mut Note) -> Task<Message> {
        note.edits += 1;
        let edit = note.edits;
        Task::perform(notes::pause_before_saving(), move |()| {
            Message::Notes(NotesEvent::SaveDue {
                key: key.clone(),
                edit,
            })
        })
    }

    /// Writes a note, unless it was edited again since `edit` or is saved.
    fn save_note(&mut self, key: NoteKey, edit: u64) -> Task<Message> {
        let store = self.store.clone();
        let Some(note) = self.notes.by_key.get_mut(&key) else {
            return Task::none();
        };
        if note.edits != edit || !note.is_dirty() {
            return Task::none();
        }
        let text = note.content.text();
        let Some(store) = store else {
            note.saved = text;
            return Task::none();
        };
        let (location, folder) = (key.location.clone(), key.folder.clone());
        Task::perform(
            store.save_note_later(location, folder, text.clone()),
            move |result| {
                Message::Notes(NotesEvent::Saved {
                    key: key.clone(),
                    text: text.clone(),
                    result: result.map_err(|e| e.to_string()),
                })
            },
        )
    }

    /// Puts the selection, or the whole note, into the message being written
    /// and shows the composer.
    fn insert_note(&mut self) -> Task<Message> {
        let Some(note) = self.note() else {
            return Task::none();
        };
        let text = note
            .content
            .selection()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| note.content.text());
        let text = text.trim_end().to_owned();
        if text.is_empty() {
            return Task::none();
        }
        // The composer sits under the session, not under an open file.
        self.fs.shown = None;
        let mut insert = text;
        if !self.composer.text().trim().is_empty() {
            self.composer
                .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
            insert.insert(0, '\n');
        }
        self.composer
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                Arc::new(insert),
            )));
        iced::widget::operation::focus(Id::new(ids::COMPOSER_TEXT))
    }
}

/// Ticks, unticks or makes a todo of the line the cursor is on, keeping the
/// cursor where it was in the text.
fn toggle_line(content: &mut text_editor::Content) {
    let cursor = content.cursor();
    let line = cursor.position.line;
    let Some(old) = content.line(line).map(|l| l.text.to_string()) else {
        return;
    };
    let new = notes::toggle_todo(&old);
    let column = (cursor.position.column + new.chars().count()).saturating_sub(old.chars().count());
    let end = old.chars().count();
    // Select the line and type over it.
    content.move_to(text_editor::Cursor {
        position: text_editor::Position { line, column: end },
        selection: Some(text_editor::Position { line, column: 0 }),
    });
    content.perform(text_editor::Action::Edit(text_editor::Edit::Paste(
        Arc::new(new),
    )));
    content.move_to(text_editor::Cursor {
        position: text_editor::Position { line, column },
        selection: None,
    });
}
