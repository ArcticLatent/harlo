//! Pages that open over the window from the left rail: usage stats across
//! sessions, and Settings, whose sections hold Harlo's own settings, its
//! appearance, the keyboard shortcuts, the server's set-up and About.
//!
//! Harlo's own settings are saved with the layout. The server's set-up
//! (providers, web search, MCP servers, plugins) belongs to the shared
//! server, which other clients use too; it changes only when the user asks.

// Rust guideline compliant 2026-10-09

use std::time::{Duration, UNIX_EPOCH};

use harlo_core::layout::Appearance;
use harlo_core::server::{McpServer, Plugin, Provider, Stats, WebSearch};
use harlo_core::theme::Mode;

use super::{App, Message};
use crate::task::Task;

/// A page over the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    /// Usage across sessions.
    Stats,
    /// Settings, at [`Sheets::section`].
    Settings,
}

/// A section of Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    /// Harlo's general settings.
    #[default]
    General,
    /// The colour theme, light or dark.
    Appearance,
    /// The keyboard shortcuts.
    Shortcuts,
    /// The server's model providers.
    Models,
    /// Web search, MCP servers and plugins.
    Tools,
    /// The server Harlo follows.
    Server,
    /// Harlo's version and Slint's notice.
    About,
}

impl Section {
    /// Every section, in the order Settings lists them.
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Appearance,
        Self::Shortcuts,
        Self::Models,
        Self::Tools,
        Self::Server,
        Self::About,
    ];

    /// Its title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Shortcuts => "Shortcuts",
            Self::Models => "Models",
            Self::Tools => "Tools for agents",
            Self::Server => "Server",
            Self::About => "About",
        }
    }

    /// The group it's listed under, if any.
    #[must_use]
    pub fn group(self) -> Option<&'static str> {
        match self {
            Self::General | Self::Appearance | Self::Shortcuts | Self::About => None,
            Self::Models | Self::Tools => Some("Agents and models"),
            Self::Server => Some("Connections"),
        }
    }
}

/// One of Harlo's own settings that a switch turns on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// [`Preferences::reopen_tabs`].
    ReopenTabs,
    /// [`Preferences::glass`].
    Glass,
}

/// One of Harlo's own settings chosen from a few options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingChoice {
    /// [`Preferences::appearance`].
    Appearance,
    /// [`Preferences::contrast`].
    Contrast,
    /// [`Preferences::diff_colors`].
    DiffColors,
    /// [`Preferences::motion`].
    Motion,
    /// [`Preferences::chat_width`].
    ChatWidth,
    /// [`Preferences::text_size`].
    TextSize,
    /// [`Preferences::code_size`].
    CodeSize,
}

impl SettingChoice {
    /// Every choice, in the order the UI numbers them.
    pub const ALL: [Self; 7] = [
        Self::Appearance,
        Self::Contrast,
        Self::DiffColors,
        Self::Motion,
        Self::ChatWidth,
        Self::TextSize,
        Self::CodeSize,
    ];

    /// The options, as Settings names them, in order.
    #[must_use]
    pub fn options(self) -> &'static [&'static str] {
        match self {
            Self::Appearance => &["System", "Light", "Dark"],
            Self::Contrast => &["Standard", "More"],
            Self::DiffColors => &["Green and red", "Blue and orange"],
            Self::Motion => &["Off", "Fast", "Normal", "Slow"],
            Self::ChatWidth => &["Narrow", "Normal", "Wide", "Full"],
            Self::TextSize => &["Small", "Normal", "Large", "Larger"],
            Self::CodeSize => &["Small", "Normal", "Large"],
        }
    }
}

/// The time the stats cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    /// The last seven days.
    Week,
    /// The last thirty days.
    #[default]
    Month,
    /// Since the first session.
    All,
}

impl Range {
    /// How far back it reaches, or `None` for all time.
    fn span(self) -> Option<Duration> {
        /// Seconds in a day.
        const DAY: u64 = 24 * 60 * 60;
        match self {
            Self::Week => Some(Duration::from_secs(7 * DAY)),
            Self::Month => Some(Duration::from_secs(30 * DAY)),
            Self::All => None,
        }
    }
}

/// What a page loaded, or why it couldn't.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Load<T> {
    /// Not asked for yet.
    #[default]
    Idle,
    /// On its way.
    Loading,
    /// Here.
    Loaded(T),
    /// It failed, for this reason.
    Failed(String),
}

/// The server's set-up, each part loaded on its own.
#[derive(Debug, Clone, PartialEq)]
pub struct SetUp {
    /// The model providers.
    pub providers: Result<Vec<Provider>, String>,
    /// The web search services.
    pub search: Result<Vec<WebSearch>, String>,
    /// The MCP servers.
    pub mcp: Result<Vec<McpServer>, String>,
    /// The plugins.
    pub plugins: Result<Vec<Plugin>, String>,
}

/// Something that happened to the pages.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Open a page, or close it if it's open.
    Toggle(Sheet),
    /// Open Settings at a section, or close it if it's open there.
    Open(Section),
    /// Close the open page.
    Close,
    /// Show another section of Settings.
    Section(Section),
    /// Search Settings for this text; empty shows the section again.
    Query(String),
    /// Turn one of Harlo's own settings on or off.
    Switch(Setting),
    /// Use the theme with this id while the window is in its mode.
    Theme(String),
    /// Take option `.1` (counted from 0) of a choice.
    Choose(SettingChoice, usize),
    /// Show stats for another range.
    Range(Range),
    /// Stats arrived for load `serial`.
    Stats(u64, Result<Stats, String>),
    /// The set-up arrived for load `serial`.
    SetUp(u64, Box<SetUp>),
}

/// The pages' state.
#[derive(Debug, Default)]
pub struct Sheets {
    /// The page shown, if any.
    pub open: Option<Sheet>,
    /// The section of Settings shown.
    pub section: Section,
    /// What Settings is searched for; empty when it isn't.
    pub query: String,
    /// The stats' range.
    pub range: Range,
    /// The stats.
    pub stats: Load<Stats>,
    /// The set-up.
    pub setup: Load<Box<SetUp>>,
    /// Counts loads, so a late answer to an older one is dropped.
    serial: u64,
}

impl App {
    /// Applies something that happened to the pages, and checks the Claude
    /// subscription when Models comes into view.
    pub(super) fn sheet_event(&mut self, event: Event) -> Task<Message> {
        let models_shown = |app: &Self| {
            app.sheets.open == Some(Sheet::Settings) && app.sheets.section == Section::Models
        };
        let before = models_shown(self);
        let task = self.apply_sheet(event);
        if models_shown(self) && !before {
            return Task::batch([task, self.check_claude()]);
        }
        task
    }

    fn apply_sheet(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle(sheet) if self.sheets.open == Some(sheet) => self.sheets.open = None,
            Event::Toggle(sheet) => {
                self.sheets.open = Some(sheet);
                return match sheet {
                    Sheet::Stats => self.load_stats(),
                    Sheet::Settings => {
                        self.sheets.query.clear();
                        self.load_setup()
                    }
                };
            }
            Event::Open(section)
                if self.sheets.open == Some(Sheet::Settings)
                    && self.sheets.section == section
                    && self.sheets.query.is_empty() =>
            {
                self.sheets.open = None;
            }
            Event::Open(section) => {
                self.sheets.section = section;
                self.sheets.query.clear();
                if self.sheets.open != Some(Sheet::Settings) {
                    self.sheets.open = Some(Sheet::Settings);
                    return self.load_setup();
                }
            }
            Event::Close => self.sheets.open = None,
            Event::Section(section) => {
                self.sheets.section = section;
                self.sheets.query.clear();
            }
            Event::Query(query) => self.sheets.query = query,
            Event::Switch(setting) => {
                match setting {
                    Setting::ReopenTabs => {
                        self.preferences.reopen_tabs = !self.preferences.reopen_tabs;
                    }
                    Setting::Glass => self.preferences.glass = !self.preferences.glass,
                }
                return self.save_preferences();
            }
            Event::Theme(id) => {
                if let Some(mode) = self.themes.iter().find(|t| t.id == id).map(|t| t.mode) {
                    match mode {
                        Mode::Dark => self.preferences.dark_theme = id,
                        Mode::Light => self.preferences.light_theme = id,
                    }
                    // A fixed appearance turns to the theme picked, so it
                    // shows; System keeps following the desktop.
                    if self.preferences.appearance != Appearance::System {
                        self.preferences.appearance = match mode {
                            Mode::Dark => Appearance::Dark,
                            Mode::Light => Appearance::Light,
                        };
                    }
                    self.retheme();
                    return self.save_preferences();
                }
            }
            Event::Choose(choice, option) => {
                if self.choose(choice, option) {
                    self.retheme();
                    return self.save_preferences();
                }
            }
            Event::Range(range) => {
                self.sheets.range = range;
                return self.load_stats();
            }
            Event::Stats(serial, result) if serial == self.sheets.serial => {
                self.sheets.stats = match result {
                    Ok(stats) => Load::Loaded(stats),
                    Err(error) => Load::Failed(error),
                };
            }
            Event::SetUp(serial, setup) if serial == self.sheets.serial => {
                self.sheets.setup = Load::Loaded(setup);
            }
            Event::Stats(..) | Event::SetUp(..) => {}
        }
        Task::none()
    }

    /// Saves Harlo's own settings, off the UI thread; a failed save keeps
    /// them for this run, and the next change tries again.
    fn save_preferences(&self) -> Task<Message> {
        match self.store.clone() {
            Some(store) => {
                Task::future(store.save_preferences_later(self.preferences.clone())).discard()
            }
            None => Task::none(),
        }
    }

    /// Asks for the stats over the chosen range.
    fn load_stats(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.sheets.stats = Load::Failed("Not connected to OpenCode yet.".to_owned());
            return Task::none();
        };
        self.sheets.serial += 1;
        let serial = self.sheets.serial;
        if !matches!(self.sheets.stats, Load::Loaded(_)) {
            self.sheets.stats = Load::Loading;
        }
        let from = self.sheets.range.span().and_then(|span| {
            let since = self
                .now
                .checked_sub(span)?
                .duration_since(UNIX_EPOCH)
                .ok()?;
            u64::try_from(since.as_millis()).ok()
        });
        Task::future(async move {
            let result = client.stats(from).await.map_err(|e| e.to_string());
            Message::Sheet(Event::Stats(serial, result))
        })
    }

    /// Asks for the server's set-up in the current folder.
    pub(super) fn load_setup(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.sheets.setup = Load::Failed("Not connected to OpenCode yet.".to_owned());
            return Task::none();
        };
        self.sheets.serial += 1;
        let serial = self.sheets.serial;
        if !matches!(self.sheets.setup, Load::Loaded(_)) {
            self.sheets.setup = Load::Loading;
        }
        let folder = self.folder();
        Task::future(async move {
            let text = |e: harlo_core::server::Error| e.to_string();
            let (providers, search, mcp, plugins) = futures_util::join!(
                client.providers(&folder),
                client.web_search(&folder),
                client.mcp_servers(&folder),
                client.plugins(&folder),
            );
            let setup = SetUp {
                providers: providers.map_err(text),
                search: search.map_err(text),
                mcp: mcp.map_err(text),
                plugins: plugins.map_err(text),
            };
            Message::Sheet(Event::SetUp(serial, Box::new(setup)))
        })
    }
}
