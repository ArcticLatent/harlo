//! The terminals, in a strip under the center.
//!
//! For now a terminal is a tab without a shell behind it; the shells
//! (local, and on the server over OpenCode's PTY API) come with the terminal
//! view.

// Rust guideline compliant 2026-10-09

use super::{App, Message};
use crate::task::Task;

/// Something that happened to the terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Show the terminals, starting one if there's none, or hide them.
    Toggle,
    /// Start another terminal and show it.
    New,
    /// Show terminal `id` in the strip.
    Select(u32),
    /// End terminal `id`.
    Close(u32),
}

/// One terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    /// The terminal's number, which its tab refers to.
    pub id: u32,
    /// Its title: its folder's name.
    pub title: String,
    /// The folder it started in.
    pub folder: String,
}

/// The open terminals, in the order they were started.
#[derive(Debug, Default)]
pub struct Terminals {
    shells: Vec<Shell>,
    /// The number the next terminal gets; numbers aren't reused.
    next: u32,
    /// The folder to start in when no session is open, instead of the home
    /// folder.
    folder: Option<String>,
    /// Whether the strip is shown.
    shown: bool,
    /// The terminal the strip shows.
    active: Option<u32>,
}

impl Terminals {
    /// The open terminals.
    #[must_use]
    pub fn shells(&self) -> &[Shell] {
        &self.shells
    }

    /// The terminal with number `id`.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Shell> {
        self.shells.iter().find(|s| s.id == id)
    }

    /// The terminal the strip shows.
    #[must_use]
    pub fn active(&self) -> Option<&Shell> {
        self.active.and_then(|id| self.get(id))
    }
}

impl App {
    /// Starts terminals in `folder` when no session is open; for tests.
    #[must_use]
    pub fn terminal_folder(mut self, folder: impl Into<String>) -> Self {
        self.terminals.folder = Some(folder.into());
        self
    }

    /// Whether the terminals are on screen.
    #[must_use]
    pub fn terminals_shown(&self) -> bool {
        self.terminals.shown && !self.terminals.shells.is_empty() && self.maximized.is_none()
    }

    /// Applies something that happened to the terminals.
    pub(super) fn terminal_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle if self.maximized.is_some() => {
                // The terminals were under the maximized panel: bring them up.
                self.maximized = None;
                if self.terminals.shells.is_empty() {
                    self.new_terminal();
                }
                self.terminals.shown = true;
            }
            Event::Toggle if self.terminals.shells.is_empty() => self.new_terminal(),
            Event::Toggle => self.terminals.shown = !self.terminals.shown,
            Event::New => self.new_terminal(),
            Event::Select(id) if self.terminals.get(id).is_some() => {
                self.terminals.active = Some(id);
            }
            Event::Close(id) => {
                let Some(index) = self.terminals.shells.iter().position(|s| s.id == id) else {
                    return Task::none();
                };
                self.terminals.shells.remove(index);
                if self.terminals.active == Some(id) {
                    // The neighbour takes its place.
                    let next = index.min(self.terminals.shells.len().saturating_sub(1));
                    self.terminals.active = self.terminals.shells.get(next).map(|s| s.id);
                }
                if self.terminals.shells.is_empty() {
                    self.terminals.shown = false;
                }
            }
            Event::Select(_) => {}
        }
        Task::none()
    }

    /// Starts a new terminal in the open session's folder and shows it.
    fn new_terminal(&mut self) {
        let id = self.terminals.next;
        self.terminals.next += 1;
        let folder = self
            .active
            .as_deref()
            .and_then(|s| self.session_folder(s))
            .or_else(|| self.terminals.folder.clone())
            .unwrap_or_else(super::home_folder);
        let title = std::path::Path::new(&folder)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&folder)
            .to_owned();
        self.terminals.shells.push(Shell { id, title, folder });
        self.terminals.active = Some(id);
        self.terminals.shown = true;
    }
}
