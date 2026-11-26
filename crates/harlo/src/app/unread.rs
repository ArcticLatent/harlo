//! Sessions with an answer nobody has looked at yet.
//!
//! A run that ends while its session isn't on screen leaves the session
//! unread, and the sidebar marks it, until the session is looked at or
//! works again. The marks are saved with the layout, for each server, and
//! come back at the next start; a run that ended while Harlo wasn't
//! following the server (closed, or cut off from it) is marked when it
//! connects, from when the server says each session's last run ended.

// Rust guideline compliant 2026-10-09

use harlo_core::layout::Tabs;
use harlo_core::workspace::{EventKind, millis};

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

    /// Follows the server's event `kind`, of `time` (milliseconds since
    /// the Unix epoch): a run that ended unseen leaves its session unread.
    pub(super) fn track_unread(&mut self, kind: &EventKind, time: f64) {
        if matches!(kind, EventKind::Done { .. }) {
            self.followed = self.followed.max(millis(time));
        }
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

    /// Takes up the marks saved for this server in `saved`, then marks
    /// the sessions that answered while Harlo wasn't following it.
    ///
    /// Call once the server's sessions are known. A server followed for
    /// the first time gets no marks: everything in it would be new.
    pub(super) fn catch_up_unread(&mut self, saved: Option<&Tabs>) {
        if let Some(saved) = saved {
            self.unread = saved.unread.iter().cloned().collect();
            self.followed = saved.followed;
        }
        if self.followed > 0 {
            let answered: Vec<String> = self
                .workspace
                .answered_since(self.followed)
                .map(str::to_owned)
                .collect();
            self.unread.extend(answered);
        }
        // Deleted, archived or at work again while away: nothing to read.
        let workspace = &self.workspace;
        self.unread
            .retain(|id| workspace.session(id).is_some() && !workspace.is_busy(id));
        self.followed = self.followed.max(self.workspace.last_ended());
    }

    /// The unread marks as they are saved: sorted, so the same marks are
    /// the same layout.
    pub(super) fn unread_saved(&self) -> Vec<String> {
        let mut unread: Vec<String> = self.unread.iter().cloned().collect();
        unread.sort_unstable();
        unread
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
