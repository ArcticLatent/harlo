//! Settings → Memory: the notes the agent keeps between sessions, on
//! Harlo's own server. Turned on or off for the whole server; each note
//! shown with where it came from, to correct or forget.
//!
//! Notes the server proposes, from what the agent missed, wait here too:
//! each is kept, with its words changed or not, or dismissed.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Client, Note as KeptNote, Proposal};

use super::sheets::Load;
use super::{App, Message};
use crate::task::Task;

/// What the server says about notes.
#[derive(Debug, Clone, Default)]
pub struct Kept {
    /// Whether the server keeps notes at all (the server doesn't).
    pub supported: bool,
    /// Whether notes are on.
    pub on: bool,
    /// The user's notes and the current project's, oldest first.
    pub notes: Vec<KeptNote>,
    /// The notes the server proposes for them, waiting for the person.
    pub proposals: Vec<Proposal>,
}

/// Settings → Memory's state.
#[derive(Debug, Default)]
pub struct Memory {
    /// What the server said.
    pub kept: Load<Kept>,
    /// The note being corrected, by ID.
    pub editing: Option<String>,
    /// The proposed note being looked at, by ID.
    pub reviewing: Option<String>,
    /// Why the last change failed, if it did.
    pub error: Option<String>,
}

/// Something that happened in Settings → Memory.
#[derive(Debug, Clone)]
pub enum MemoryEvent {
    /// The server's notes came, or couldn't.
    Loaded(Result<Kept, String>),
    /// The switch was flipped.
    Toggle,
    /// A note's Edit button: the note at this place.
    Edit(usize),
    /// A proposed note's Review button: the proposal at this place.
    Review(usize),
    /// Correcting a note, or looking at a proposed one, was cancelled.
    Cancel,
    /// The text from the field: the note's corrected words, or the words
    /// the proposed note is kept with.
    Save(String),
    /// The proposed note being looked at is turned down.
    Dismiss,
    /// Open the session the proposed note being looked at came from.
    OpenSource,
    /// The note being corrected is to be forgotten.
    Forget,
    /// A change was made, or failed; the notes load again.
    Changed(Result<(), String>),
}

impl App {
    /// Takes an event of Settings → Memory.
    pub(super) fn memory_event(&mut self, event: MemoryEvent) -> Task<Message> {
        let memory = &mut self.sheets.memory;
        match event {
            MemoryEvent::Loaded(result) => {
                memory.kept = match result {
                    Ok(kept) => Load::Loaded(kept),
                    Err(why) => Load::Failed(why),
                };
                Task::none()
            }
            MemoryEvent::Toggle => {
                let Load::Loaded(kept) = &memory.kept else {
                    return Task::none();
                };
                let on = !kept.on;
                self.change(move |client| async move { client.set_notes_on(on).await })
            }
            MemoryEvent::Edit(index) => {
                if let Load::Loaded(kept) = &memory.kept {
                    memory.editing = kept.notes.get(index).map(|n| n.id.clone());
                }
                memory.reviewing = None;
                memory.error = None;
                Task::none()
            }
            MemoryEvent::Review(index) => {
                if let Load::Loaded(kept) = &memory.kept {
                    memory.reviewing = kept.proposals.get(index).map(|p| p.id.clone());
                }
                memory.editing = None;
                memory.error = None;
                Task::none()
            }
            MemoryEvent::Cancel => {
                memory.editing = None;
                memory.reviewing = None;
                Task::none()
            }
            MemoryEvent::Save(text) => {
                if let Some(id) = memory.reviewing.clone() {
                    if text.trim().is_empty() {
                        memory.error = Some("A note needs text; Dismiss turns it down.".to_owned());
                        return Task::none();
                    }
                    memory.reviewing = None;
                    return self.change(move |client| async move {
                        client.accept_proposal(&id, &text).await
                    });
                }
                let Some(id) = memory.editing.clone() else {
                    return Task::none();
                };
                if text.trim().is_empty() {
                    memory.error = Some("A note needs text; Forget removes it.".to_owned());
                    return Task::none();
                }
                memory.editing = None;
                self.change(move |client| async move { client.correct_note(&id, &text).await })
            }
            MemoryEvent::Dismiss => {
                let Some(id) = memory.reviewing.take() else {
                    return Task::none();
                };
                self.change(move |client| async move { client.dismiss_proposal(&id).await })
            }
            MemoryEvent::OpenSource => {
                let source = match (&memory.kept, &memory.reviewing) {
                    (Load::Loaded(kept), Some(id)) => kept
                        .proposals
                        .iter()
                        .find(|p| p.id == *id)
                        .map(|p| p.source.session.clone()),
                    _ => None,
                };
                let Some(session) = source else {
                    return Task::none();
                };
                Task::batch([
                    Task::done(Message::Sheet(super::SheetEvent::Close)),
                    Task::done(Message::OpenSession(session)),
                ])
            }
            MemoryEvent::Forget => {
                let Some(id) = memory.editing.take() else {
                    return Task::none();
                };
                self.change(move |client| async move { client.forget_note(&id).await })
            }
            MemoryEvent::Changed(result) => {
                memory.error = result.err();
                self.load_memory()
            }
        }
    }

    /// Loads whether the server keeps notes, whether they're on, and the
    /// notes for the current folder's project.
    pub(super) fn load_memory(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.sheets.memory.kept = Load::Failed("Not connected to the server yet.".to_owned());
            return Task::none();
        };
        if !matches!(self.sheets.memory.kept, Load::Loaded(_)) {
            self.sheets.memory.kept = Load::Loading;
        }
        let project = self.notes_project();
        Task::perform(
            async move {
                if !client.keeps_notes().await.map_err(|e| e.to_string())? {
                    return Ok(Kept::default());
                }
                let on = client.notes_on().await.map_err(|e| e.to_string())?;
                let notes = client
                    .notes(project.as_deref())
                    .await
                    .map_err(|e| e.to_string())?;
                // (An older server keeps notes and proposes none.)
                let proposals = if client.proposes_notes().await.map_err(|e| e.to_string())? {
                    client
                        .note_proposals(project.as_deref())
                        .await
                        .map_err(|e| e.to_string())?
                } else {
                    Vec::new()
                };
                Ok(Kept {
                    supported: true,
                    on,
                    notes,
                    proposals,
                })
            },
            |result| Message::Memory(MemoryEvent::Loaded(result)),
        )
    }

    /// The project whose notes Settings shows: the open session's, or the
    /// current folder's.
    fn notes_project(&self) -> Option<String> {
        let open = self
            .active
            .as_deref()
            .and_then(|id| self.workspace.session(id))
            .map(|s| s.project_id.clone());
        open.or_else(|| self.workspace.project(&self.folder()).map(|p| p.id.clone()))
    }

    /// Makes a change on the server, then loads the notes again.
    fn change<F, Fut>(&self, make: F) -> Task<Message>
    where
        F: FnOnce(Client) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), harlo_core::server::Error>> + Send + 'static,
    {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        Task::perform(
            async move { make(client).await.map_err(|e| e.to_string()) },
            |result| Message::Memory(MemoryEvent::Changed(result)),
        )
    }
}
