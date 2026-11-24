//! Offering Harlo's own tools to the server's agents while Harlo is
//! connected.
//!
//! The server makes a client's tools available to every session, whatever
//! its folder, and keeps one connection per name: Harlo attaches once,
//! for its home folder. A connection that breaks while the server stays
//! reachable is made again after a pause. A server
//! that says it can't take client tools at all isn't asked again until
//! Harlo connects anew, and Settings says why.

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

/// Whether attaching for Harlo's tools waits, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hold {
    /// It doesn't.
    None,
    /// A moment, after a connection broke.
    Paused,
    /// Until the next connection: the server can't take client tools.
    Unavailable,
}

/// Something that happened to Harlo's tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnToolsEvent {
    /// The connection for a folder ended, with why when it failed.
    Ended {
        /// The folder.
        folder: String,
        /// Why it failed, if it did.
        why: Option<String>,
        /// Whether the server can't take client tools at all.
        unavailable: bool,
    },
    /// Attach again for the folders that need it.
    Retry,
}

impl App {
    /// Handles `event` on Harlo's tools.
    pub(super) fn own_tools_event(&mut self, event: OwnToolsEvent) -> Task<Message> {
        match event {
            OwnToolsEvent::Ended {
                folder,
                why,
                unavailable,
            } => {
                self.own_tools.remove(&folder);
                self.own_tools_error = why;
                if unavailable {
                    // Asking again won't help: stop until the next connection.
                    self.own_tools_hold = Hold::Unavailable;
                    self.own_tools.clear();
                    return Task::none();
                }
                // Not at once: a server that refuses would be asked in a
                // tight loop.
                self.own_tools_hold = Hold::Paused;
                Task::future(async {
                    tokio::time::sleep(RETRY).await;
                    Message::OwnTools(OwnToolsEvent::Retry)
                })
            }
            OwnToolsEvent::Retry => {
                if self.own_tools_hold == Hold::Paused {
                    self.own_tools_hold = Hold::None;
                }
                Task::none()
            }
        }
    }

    /// The folder Harlo's tools are attached for now (the home folder;
    /// every session gets them): none when they're off or Harlo isn't
    /// connected.
    fn own_tools_folders(&self) -> BTreeSet<String> {
        if !self.preferences.own_tools
            || self.own_tools_hold == Hold::Unavailable
            || self.client.is_none()
            || !matches!(self.status, Status::Connected { .. })
        {
            return BTreeSet::new();
        }
        BTreeSet::from([self.home.clone()])
    }

    /// Attaches for the folders that should have Harlo's tools and doesn't
    /// yet, and detaches from those that no longer should.
    pub(super) fn follow_own_tools(&mut self) -> Task<Message> {
        let wanted = self.own_tools_folders();
        self.own_tools.retain(|folder, _| wanted.contains(folder));
        if self.own_tools_hold == Hold::Paused {
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
                let (why, unavailable) = match client.host_tools(&at, NAME, host).await {
                    Ok(()) => (None, false),
                    Err(e) if e.is_unavailable() => (
                        Some(
                            e.server_message()
                                .map_or_else(|| e.to_string(), str::to_owned),
                        ),
                        true,
                    ),
                    Err(e) => (Some(e.to_string()), false),
                };
                Message::OwnTools(OwnToolsEvent::Ended {
                    folder: at,
                    why,
                    unavailable,
                })
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

    /// Whether the server said it can't take Harlo's tools at all; Harlo
    /// asks again only on its next connection.
    #[must_use]
    pub fn own_tools_unavailable(&self) -> bool {
        self.own_tools_hold == Hold::Unavailable
    }

    /// How many connections offer Harlo's tools right now: 0 or 1.
    #[must_use]
    pub fn own_tools_offered(&self) -> usize {
        self.own_tools.len()
    }
}
