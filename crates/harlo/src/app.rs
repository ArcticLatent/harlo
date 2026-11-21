//! The app's state, messages and update, and the window's top-level layout.

// Rust guideline compliant 2026-10-09

use std::collections::HashSet;

use harlo_core::sessions::Project;
use harlo_core::theme::{self, Mode, Tokens};
use iced::widget::{row, text_editor};
use iced::{Element, Fill, Task, Theme};

use crate::assets::Assets;
use crate::view;

/// Panels the right rail opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Context window use and session facts.
    Context,
    /// The session's todo list.
    Todos,
    /// Files the session changed.
    Changes,
    /// The project's files.
    Files,
    /// The project's notes.
    Notes,
}

impl Panel {
    /// Every panel, in rail order.
    pub const ALL: [Self; 5] = [
        Self::Context,
        Self::Todos,
        Self::Changes,
        Self::Files,
        Self::Notes,
    ];

    /// The widget ID of the panel's rail button.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Context => "panel-context",
            Self::Todos => "panel-todos",
            Self::Changes => "panel-changes",
            Self::Files => "panel-files",
            Self::Notes => "panel-notes",
        }
    }

    /// The panel's title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Context => "Context",
            Self::Todos => "Todos",
            Self::Changes => "Changes",
            Self::Files => "Files",
            Self::Notes => "Notes",
        }
    }
}

/// Something the user did.
#[derive(Debug, Clone)]
pub enum Message {
    /// Show or hide the sidebar.
    ToggleSidebar,
    /// The session filter changed.
    Filter(String),
    /// Show all of a project's sessions, or fewer again.
    ToggleMore(usize),
    /// Open a right panel, or close it when it's already open.
    OpenPanel(Panel),
    /// Close the right panel.
    ClosePanel,
    /// The composer was edited.
    Compose(text_editor::Action),
}

/// Everything the window shows.
#[derive(Debug)]
pub struct App {
    /// Colours for every widget.
    pub tokens: Tokens,
    /// Bundled images.
    pub assets: Assets,
    theme: Theme,
    /// Projects and their sessions.
    pub projects: Vec<Project>,
    /// Text in the session filter.
    pub filter: String,
    /// Projects whose full session list is open, by index.
    pub expanded: HashSet<usize>,
    /// Whether the sidebar is shown.
    pub sidebar: bool,
    /// The open right panel.
    pub panel: Option<Panel>,
    /// The composer's text.
    pub composer: text_editor::Content,
}

impl App {
    /// Creates the window's state, with the given projects in the sidebar.
    ///
    /// # Panics
    ///
    /// Panics if the bundled default theme doesn't parse. `harlo-core`'s
    /// tests check that it does, so this would be a build mistake.
    #[must_use]
    pub fn new(projects: Vec<Project>) -> Self {
        let theme = theme::parse(harlo_core::DEFAULT_THEME).expect("the bundled theme is valid");
        let tokens = theme.tokens(Mode::Dark);
        Self {
            theme: crate::style::iced_theme(&theme.name, &tokens),
            tokens,
            assets: Assets::load(),
            projects,
            filter: String::new(),
            expanded: HashSet::new(),
            sidebar: true,
            panel: Some(Panel::Context),
            composer: text_editor::Content::new(),
        }
    }

    /// Applies a message.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleSidebar => self.sidebar = !self.sidebar,
            Message::Filter(filter) => self.filter = filter,
            Message::ToggleMore(project) => {
                if !self.expanded.remove(&project) {
                    self.expanded.insert(project);
                }
            }
            Message::OpenPanel(panel) => {
                self.panel = if self.panel == Some(panel) {
                    None
                } else {
                    Some(panel)
                };
            }
            Message::ClosePanel => self.panel = None,
            Message::Compose(action) => self.composer.perform(action),
        }
        Task::none()
    }

    /// Lays out the window: rails on both edges, the sidebar, the main area and the panel.
    pub fn view(&self) -> Element<'_, Message> {
        let t = &self.tokens;
        let mut layout = row![view::rails::left(t, &self.assets)];
        if self.sidebar {
            layout = layout.push(view::sidebar::view(self));
        }
        layout = layout.push(view::home::view(t, &self.composer));
        if let Some(panel) = self.panel {
            layout = layout.push(view::panel::view(t, panel));
        }
        layout
            .push(view::rails::right(t, self.panel))
            .height(Fill)
            .into()
    }

    /// The iced theme supplying widget defaults.
    #[must_use]
    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }
}
