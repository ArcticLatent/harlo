//! Offering Harlo's own tools to OpenCode's agents while Harlo is connected.
//!
//! OpenCode makes a client's tools available to the sessions of the folder
//! the client attached for, so Harlo attaches once for each folder it shows:
//! the home folder, each project and each worktree. A connection that breaks
//! while the server stays reachable is made again after a pause.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use harlo_core::own_tools::{NAME, OwnTools};

use super::{App, Message, Status};
use crate::task::Task;

/// How long to wait before attaching again for a folder whose connection
/// broke while the server stayed reachable.
const RETRY: Duration = Duration::from_secs(5);

/// Something that happened to Harlo's tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnToolsEvent {
    /// The connection for a folder ended, with why when it failed.
    Ended {
        /// The folder.
        folder: String,
        /// Why it failed, if it did.
        why: Option<String>,
    },
    /// Attach again for the folders that need it.
    Retry,
}

impl App {
    /// Handles `event` on Harlo's tools.
    pub(super) fn own_tools_event(&mut self, event: OwnToolsEvent) -> Task<Message> {
        match event {
            OwnToolsEvent::Ended { folder, why } => {
                self.own_tools.remove(&folder);
                self.own_tools_error = why;
                // Not at once: a server that refuses would be asked in a
                // tight loop.
                self.own_tools_paused = true;
                Task::future(async {
                    tokio::time::sleep(RETRY).await;
                    Message::OwnTools(OwnToolsEvent::Retry)
                })
            }
            OwnToolsEvent::Retry => {
                self.own_tools_paused = false;
                Task::none()
            }
        }
    }

    /// The folders Harlo's tools should be offered for now: none when
    /// they're off or Harlo isn't connected.
    fn own_tools_folders(&self) -> BTreeSet<String> {
        if !self.preferences.own_tools
            || self.client.is_none()
            || !matches!(self.status, Status::Connected { .. })
        {
            return BTreeSet::new();
        }
        let mut folders = BTreeSet::from([self.home.clone()]);
        for project in &self.projects {
            folders.insert(project.directory.clone());
            folders.extend(project.worktrees.iter().map(|w| w.directory.clone()));
        }
        folders
    }

    /// Attaches for the folders that should have Harlo's tools and doesn't
    /// yet, and detaches from those that no longer should.
    pub(super) fn follow_own_tools(&mut self) -> Task<Message> {
        let wanted = self.own_tools_folders();
        self.own_tools.retain(|folder, _| wanted.contains(folder));
        if self.own_tools_paused {
            return Task::none();
        }
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        for folder in wanted {
            if self.own_tools.contains_key(&folder) {
                continue;
            }
            let client = client.clone();
            let host = Arc::new(OwnTools::new(client.clone()));
            let at = folder.clone();
            let (task, handle) = Task::future(async move {
                let why = client
                    .host_tools(&at, NAME, host)
                    .await
                    .err()
                    .map(|e| e.to_string());
                Message::OwnTools(OwnToolsEvent::Ended { folder: at, why })
            })
            .abortable();
            self.own_tools.insert(folder, handle.abort_on_drop());
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    /// Stops offering Harlo's tools everywhere: dropping the connections
    /// takes them away from the agents.
    pub(super) fn withdraw_own_tools(&mut self) {
        self.own_tools.clear();
    }

    /// How many folders Harlo's tools are offered for right now.
    #[must_use]
    pub fn own_tools_offered(&self) -> usize {
        self.own_tools.len()
    }
}
