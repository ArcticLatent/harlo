//! Undoing a turn: staging it, then keeping or cancelling it.
//!
//! Undo on a prompt first asks the server for the agent's own changes from
//! that turn on, then stages the undo with files. That restores the files at
//! once, but cancelling puts every file back, so nothing is lost until the
//! user keeps it. The card above the composer shows what was restored and,
//! by comparing the two, which of the user's own edits the restore overwrote
//! (see [`harlo_core::undo`]).

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, HashSet};

use crate::task::Task;
use harlo_core::server::FileDiff;
use harlo_core::workspace::Revert;

use super::{App, Message};

/// What Harlo knows about the undos it staged.
#[derive(Debug, Default)]
pub struct Undos {
    /// The agent's own changes over each staged undo's turns, by session.
    pub agent: HashMap<String, Vec<FileDiff>>,
    /// Sessions with an undo request on its way.
    pub busy: HashSet<String>,
    /// Sessions whose staged undo's agent changes are being fetched.
    checking: HashSet<String>,
    /// The undone prompt's text, by session, to write again once kept.
    prompts: HashMap<String, String>,
}

/// A staged undo, with the agent's changes it was checked against.
pub type Staged = (Vec<FileDiff>, Revert);

/// The open session's staged undo, as its card shows it.
#[derive(Debug, Clone, Copy)]
pub struct StagedUndo<'a> {
    /// What the server staged.
    pub revert: &'a Revert,
    /// The undone prompt's text, if it's shown.
    pub prompt: Option<&'a str>,
    /// The agent's changes it was checked against, if Harlo staged it.
    pub agent: Option<&'a [FileDiff]>,
}

impl App {
    /// Applies a message about undoing a turn.
    pub(super) fn undo_action(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UndoStaged(session, result) => {
                self.undos.busy.remove(&session);
                match result {
                    Ok((agent, revert)) => {
                        self.undos.agent.insert(session.clone(), agent);
                        self.workspace.set_revert(&session, Some(revert));
                    }
                    Err(reason) => {
                        self.undos.prompts.remove(&session);
                        self.send_error = Some(format!("Couldn't undo: {reason}"));
                    }
                }
            }
            Message::UndoChecked(session, result) => {
                self.undos.checking.remove(&session);
                // Without them, the card says it can't tell whose edits are whose.
                if let Ok(agent) = result {
                    self.undos.agent.insert(session, agent);
                }
            }
            Message::UndoKeep(session) => return self.settle_undo(session, true),
            Message::UndoCancel(session) => return self.settle_undo(session, false),
            Message::UndoSettled {
                session,
                kept,
                result,
            } => {
                self.undos.busy.remove(&session);
                if let Err(reason) = result {
                    self.send_error = Some(format!("Couldn't finish the undo: {reason}"));
                    return Task::none();
                }
                self.undos.agent.remove(&session);
                self.workspace.set_revert(&session, None);
                let prompt = self.undos.prompts.remove(&session);
                if !kept {
                    return Task::none();
                }
                // The undone prompt goes back into an empty composer, to send again or change.
                if let Some(text) = prompt
                    && self.active.as_deref() == Some(session.as_str())
                    && self.composer.text().trim().is_empty()
                {
                    self.composer.set(&text);
                }
                return self.load_history(session);
            }
            // Other messages are routed elsewhere by `apply`.
            _ => {}
        }
        Task::none()
    }

    /// The open session's staged undo, with the undone prompt's text and the
    /// agent's changes it was checked against, if Harlo staged it.
    #[must_use]
    pub fn staged_undo(&self) -> Option<StagedUndo<'_>> {
        let id = self.active.as_deref()?;
        let revert = self.workspace.session(id)?.revert.as_ref()?;
        let prompt = self
            .timelines
            .get(id)
            .and_then(|t| t.prompt_text(&revert.message_id));
        let agent = self.undos.agent.get(id).map(Vec::as_slice);
        Some(StagedUndo {
            revert,
            prompt,
            agent,
        })
    }

    /// Stages an undo of the open session's prompt `message_id` and all after it.
    pub(super) fn start_undo(&mut self, session: String, message_id: String) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if self.is_busy(&session) || !self.undos.busy.insert(session.clone()) {
            return Task::none();
        }
        let timeline = self.timelines.get(&session);
        let last = timeline.and_then(|t| t.last_prompt()).map(str::to_owned);
        if let Some(text) = timeline.and_then(|t| t.prompt_text(&message_id)) {
            self.undos.prompts.insert(session.clone(), text.to_owned());
        }
        let id = session.clone();
        Task::perform(
            async move {
                // The agent's changes first: once staged, the restore is on disk.
                let to = last.as_deref().filter(|last| *last != message_id);
                let agent = client.diff_since(&id, &message_id, to).await;
                // Without them the undo still works; it just can't warn.
                let agent = agent.unwrap_or_default();
                let revert = client.stage_revert(&id, &message_id, true).await;
                revert
                    .map(|revert| (agent, revert))
                    .map_err(|e| e.to_string())
            },
            move |result| Message::UndoStaged(session.clone(), result),
        )
    }

    /// Fetches the agent's changes for a session's staged undo, if it has one
    /// Harlo didn't stage (or staged before it was restarted).
    pub(super) fn check_undo(&mut self, session: &str) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let Some(revert) = self
            .workspace
            .session(session)
            .and_then(|s| s.revert.as_ref())
        else {
            return Task::none();
        };
        if self.undos.agent.contains_key(session) || !self.undos.checking.insert(session.to_owned())
        {
            return Task::none();
        }
        let from = revert.message_id.clone();
        let last = self
            .timelines
            .get(session)
            .and_then(|t| t.last_prompt())
            .filter(|last| *last != from)
            .map(str::to_owned);
        let id = session.to_owned();
        Task::perform(
            async move {
                let agent = client.diff_since(&id, &from, last.as_deref()).await;
                (id, agent.map_err(|e| e.to_string()))
            },
            |(id, result)| Message::UndoChecked(id, result),
        )
    }

    /// Keeps a staged undo, or cancels it and puts every file back.
    fn settle_undo(&mut self, session: String, kept: bool) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if !self.undos.busy.insert(session.clone()) {
            return Task::none();
        }
        let id = session.clone();
        Task::perform(
            async move {
                let result = if kept {
                    client.commit_revert(&id).await
                } else {
                    client.cancel_revert(&id).await
                };
                result.map_err(|e| e.to_string())
            },
            move |result| Message::UndoSettled {
                session: session.clone(),
                kept,
                result,
            },
        )
    }
}
