//! Sessions with an answer nobody has looked at yet.
//!
//! A run that ends while its session isn't on screen leaves the session
//! unread, and the sidebar marks it, until the session is looked at or
//! works again. Kept while Harlo runs; a restart starts with none.

// Rust guideline compliant 2026-10-09

use harlo_core::workspace::EventKind;

use super::App;

impl App {
    /// Whether the user is looking at session `id`: the window is in
    /// front and shows it, or shows one of its subagents' sessions.
    fn watching(&self, id: &str) -> bool {
        let Some(active) = self.active.as_deref().filter(|_| self.notices.focused) else {
            return false;
        };
        active == id
            || self
                .workspace
                .session(active)
                .is_some_and(|shown| shown.parent_id.as_deref() == Some(id))
    }

    /// Follows the server's event `kind`: a run that ended unseen leaves
    /// its session unread.
    pub(super) fn track_unread(&mut self, kind: &EventKind) {
        match kind {
            // At work again, it has no answer to read yet.
            EventKind::Busy { session_id }
            | EventKind::SessionDeleted { session_id }
            | EventKind::SessionArchived { session_id } => {
                self.unread.remove(session_id);
            }
            // A run the user stopped tells them nothing new.
            EventKind::Done {
                session_id,
                outcome,
            } if matches!(outcome.as_str(), "succeeded" | "failed") => {
                // A subagent's run ends inside its parent's, which says so.
                let own = self
                    .workspace
                    .session(session_id)
                    .is_some_and(|s| s.parent_id.is_none());
                if own && !self.watching(session_id) {
                    self.unread.insert(session_id.clone());
                }
            }
            _ => {}
        }
    }

    /// Takes the session on screen, and the one its subagent belongs to,
    /// as read, when the window is in front.
    pub(super) fn see_shown(&mut self) {
        if self.unread.is_empty() || !self.notices.focused {
            return;
        }
        let Some(active) = self.active.as_deref() else {
            return;
        };
        let parent = self
            .workspace
            .session(active)
            .and_then(|shown| shown.parent_id.clone());
        self.unread.remove(active);
        if let Some(parent) = parent {
            self.unread.remove(&parent);
        }
    }
}
