//! How the window is arranged: the sidebar on the left, the documents in the
//! center with the terminals under them, and one tool panel on the right.
//!
//! The sidebar, the right panel and the terminals take the sizes the user
//! drags their edges to. When the window gets too narrow for the main area,
//! the panels make way; see [`fit`].

// Rust guideline compliant 2026-10-09

use super::{App, FilesEvent, Message, Panel};
use crate::fit::{self, Shown, Side, Wanted};
use crate::task::{Effect, Task};
use crate::timeline::Timeline;

/// What a tab in the center shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    /// The start screen, for a new session.
    Home,
    /// A session, by ID.
    Session(String),
    /// The file editor.
    Editor,
}

/// An edge the user drags to resize what's beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// The sidebar's right edge.
    Sidebar,
    /// The right panel's left edge.
    Panel,
    /// The terminals' top edge.
    Terminal,
}

impl App {
    /// The document shown in the center: the file being edited, the active
    /// session, or the start screen.
    #[must_use]
    pub fn current_document(&self) -> Document {
        if self.fs.shown.is_some() {
            Document::Editor
        } else if let Some(id) = &self.active {
            Document::Session(id.clone())
        } else {
            Document::Home
        }
    }

    /// The center's tabs, in order: the open sessions, the editor while a
    /// file is open, and the start screen while it's shown.
    #[must_use]
    pub fn documents(&self) -> Vec<Document> {
        let mut documents: Vec<Document> =
            self.tabs.iter().cloned().map(Document::Session).collect();
        if !self.fs.open.is_empty() {
            documents.push(Document::Editor);
        }
        if self.current_document() == Document::Home {
            documents.push(Document::Home);
        }
        documents
    }

    /// Which side panels are on screen at the window's width, and how wide.
    #[must_use]
    pub fn shown(&self) -> Shown {
        fit::shown(self.width, self.wanted())
    }

    /// The side panels the user wants open, at their widths.
    fn wanted(&self) -> Wanted {
        Wanted {
            sidebar: self.sidebar.then_some(self.sidebar_width),
            panel: self.panel.map(|_| self.panel_width),
            keep: self.keep,
        }
    }

    /// The width left for the main area.
    #[must_use]
    pub fn main_width(&self) -> f32 {
        self.shown().main_width(self.width)
    }

    /// How tall the terminals are, when they're shown.
    #[must_use]
    pub fn terminal_height(&self) -> f32 {
        fit::terminal_height(self.height, self.terminal_height)
    }

    /// Whether a tool panel is on screen: open on the right and fitting
    /// there, or covering the space between the rails.
    #[must_use]
    pub fn panel_shown(&self, tool: Panel) -> bool {
        match self.maximized {
            Some(big) => big == tool,
            None => self.panel == Some(tool) && self.shown().panel.is_some(),
        }
    }

    /// The tool panel on screen on the right, if any.
    #[must_use]
    pub fn shown_panel(&self) -> Option<Panel> {
        self.panel.filter(|_| self.shown().panel.is_some())
    }

    /// Handles the messages that rearrange the window.
    pub(super) fn arrange(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleSidebar => {
                if self.shown().sidebar.is_some() {
                    self.sidebar = false;
                    self.forget_keep(Side::Sidebar);
                } else {
                    self.sidebar = true;
                    self.keep_if_hidden(Side::Sidebar);
                }
            }
            Message::OpenPanel(tool) => {
                if !self.panel_shown(tool) {
                    self.toggle_panel(tool);
                }
            }
            Message::TogglePanel(tool) => self.toggle_panel(tool),
            Message::ClosePanel => {
                self.maximized = None;
                self.panel = None;
                self.forget_keep(Side::Panel);
            }
            Message::Maximize(tool) if self.panel == Some(tool) => {
                self.maximized = Some(tool);
                return self.on_maximize(tool);
            }
            Message::Restore if self.maximized.is_some() => {
                self.maximized = None;
                return Task::effect(Effect::Focus(crate::ids::COMPOSER_TEXT));
            }
            Message::Resized(width, height) => {
                self.width = width;
                self.height = height;
                // Once the kept panel fits by itself, it no longer needs keeping.
                let unkept = Wanted {
                    keep: None,
                    ..self.wanted()
                };
                if let Some(side) = self.keep
                    && fit::shown(self.width, unkept).has(side)
                {
                    self.keep = None;
                }
            }
            Message::Resize { edge, by } => self.resize(edge, by),
            Message::ResizeEnd => self.resizing = false,
            Message::ShowDocument(document) => return self.show_document(document),
            Message::CloseDocument(document) => return self.close_document(document),
            Message::CycleTab { back } => return self.cycle_documents(back),
            _ => {}
        }
        Task::none()
    }

    /// Opens a tool panel on the right in place of the one there, or closes
    /// it when it's shown; a maximized one is put back instead.
    fn toggle_panel(&mut self, tool: Panel) {
        if self.maximized == Some(tool) {
            self.maximized = None;
        } else if self.panel_shown(tool) {
            self.panel = None;
            self.forget_keep(Side::Panel);
        } else {
            self.maximized = None;
            self.panel = Some(tool);
            self.keep_if_hidden(Side::Panel);
        }
    }

    /// Keeps `side` on screen when it was opened without room for it.
    fn keep_if_hidden(&mut self, side: Side) {
        if !self.shown().has(side) {
            self.keep = Some(side);
        }
    }

    /// Stops keeping `side` on screen.
    fn forget_keep(&mut self, side: Side) {
        if self.keep == Some(side) {
            self.keep = None;
        }
    }

    /// Moves an edge `by` logical pixels, right or down, within the room the
    /// main area leaves.
    fn resize(&mut self, edge: Edge, by: f32) {
        self.resizing = true;
        let shown = self.shown();
        match edge {
            Edge::Sidebar => {
                let most = fit::widest(Side::Sidebar, self.width, shown);
                self.sidebar_width = (self.sidebar_width + by).clamp(fit::SIDEBAR_RANGE.0, most);
            }
            Edge::Panel => {
                let most = fit::widest(Side::Panel, self.width, shown);
                self.panel_width = (self.panel_width - by).clamp(fit::PANEL_RANGE.0, most);
            }
            Edge::Terminal => {
                // From the height on screen, which a short window may have cut.
                let wanted = self.terminal_height() - by;
                self.terminal_height = fit::terminal_height(self.height, wanted);
            }
        }
    }

    /// Makes a tab the current document, as when it's clicked.
    fn show_document(&mut self, document: Document) -> Task<Message> {
        match document {
            Document::Home => self.apply(Message::NewSession(None)),
            Document::Session(id) => {
                if self.fs.shown.is_some() || self.active.as_deref() != Some(&id) {
                    self.open(id)
                } else {
                    Task::none()
                }
            }
            Document::Editor => match self.fs.open.first().map(|f| f.key.clone()) {
                Some(file) if self.fs.shown.as_deref() != Some(&file) => {
                    self.apply(Message::Files(FilesEvent::Show(file)))
                }
                _ => Task::none(),
            },
        }
    }

    /// Closes a tab the user asked to close, as its kind closes.
    fn close_document(&mut self, document: Document) -> Task<Message> {
        match document {
            Document::Session(id) => self.apply(Message::CloseTab(id)),
            Document::Editor => match self.fs.open.first().map(|f| f.key.clone()) {
                Some(file) => self.apply(Message::Files(FilesEvent::Close(file))),
                None => Task::none(),
            },
            Document::Home => Task::none(),
        }
    }

    /// Shows the next (or previous) tab in the center, round from the end.
    fn cycle_documents(&mut self, back: bool) -> Task<Message> {
        let documents = self.documents();
        let current = self.current_document();
        let Some(at) = documents.iter().position(|d| *d == current) else {
            return Task::none();
        };
        let count = documents.len();
        let next = if back {
            (at + count - 1) % count
        } else {
            (at + 1) % count
        };
        match documents.into_iter().nth(next) {
            Some(document) if next != at => self.show_document(document),
            _ => Task::none(),
        }
    }

    /// Loads the shown session's history when it has none yet.
    pub(super) fn follow_document(&mut self) -> Task<Message> {
        match self.current_document() {
            Document::Session(id) if !self.timelines.contains_key(&id) => {
                self.timelines.insert(id.clone(), Timeline::new(&id));
                self.load(id)
            }
            _ => Task::none(),
        }
    }
}
