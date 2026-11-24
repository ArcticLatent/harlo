//! Session goals, set with OpenCode's `/goal` command: an objective the
//! agent keeps working toward, round after round, until a reviewer finds
//! it met.
//!
//! The server owns the goal and runs the loop. Harlo reads a session's
//! goal when it opens, follows its changes, and pauses, resumes or clears
//! it with the same command.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Delivery, Goal};
use harlo_core::workspace::EventKind;

use super::{App, Message};
use crate::task::Task;

/// What the `/goal` command can do to a session's goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalAction {
    /// Stop working toward it, for now.
    Pause,
    /// Work toward a paused goal again, with a new budget of rounds.
    Resume,
    /// Drop it, or dismiss a reached one.
    Clear,
}

impl GoalAction {
    /// The word the command takes, such as `pause`.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Clear => "clear",
        }
    }

    /// The action `word` names.
    #[must_use]
    pub fn named(word: &str) -> Option<Self> {
        [Self::Pause, Self::Resume, Self::Clear]
            .into_iter()
            .find(|a| a.word() == word)
    }
}

/// Something that happened to a session's goal.
#[derive(Debug, Clone)]
pub enum GoalEvent {
    /// A session's goal was read from the server.
    Loaded {
        /// The session.
        session: String,
        /// Its goal, none, or why it couldn't be read.
        result: Result<Option<Goal>, String>,
    },
    /// Pause, resume or clear the open session's goal.
    Act(GoalAction),
}

impl App {
    /// A session's goal, once read and while it has one.
    #[must_use]
    pub fn goal(&self, session: &str) -> Option<&Goal> {
        self.per_session.goals.get(session)
    }

    /// Reads a session's goal from the server.
    pub(super) fn load_goal(&self, id: String) -> Task<Message> {
        let (Some(client), Some(session)) = (self.client.clone(), self.workspace.session(&id))
        else {
            return Task::none();
        };
        let directory = session.directory.clone();
        Task::perform(
            async move {
                let result = client
                    .goal(&id, &directory)
                    .await
                    .map_err(|e| e.to_string());
                GoalEvent::Loaded {
                    session: id,
                    result,
                }
            },
            Message::Goal,
        )
    }

    /// Handles something that happened to a goal.
    pub(super) fn goal_event(&mut self, event: GoalEvent) -> Task<Message> {
        match event {
            GoalEvent::Loaded { session, result } => {
                // A server without the goal plugin has none to show.
                if let Ok(goal) = result {
                    self.set_goal(session, goal);
                }
                Task::none()
            }
            GoalEvent::Act(action) => self.act(move |client, id| async move {
                client
                    .run_command(&id, "goal", action.word(), Delivery::Queue)
                    .await
            }),
        }
    }

    /// Follows a goal the server reports changed.
    pub(super) fn goal_changed(&mut self, kind: &EventKind) {
        if let EventKind::GoalChanged { session_id, goal } = kind {
            self.set_goal(session_id.clone(), goal.clone());
        }
    }

    /// Keeps `goal` as `session`'s, or forgets its goal for `None`.
    fn set_goal(&mut self, session: String, goal: Option<Goal>) {
        match goal {
            Some(goal) => {
                self.per_session.goals.insert(session, goal);
            }
            None => {
                self.per_session.goals.remove(&session);
            }
        }
    }
}
