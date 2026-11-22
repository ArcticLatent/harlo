//! Pages that open over the window from the left rail: the keyboard
//! shortcuts, usage stats across sessions, and the server's set-up with
//! Harlo's About.
//!
//! The set-up is shown, never changed: providers, web search, MCP servers
//! and plugins belong to the shared server, which other clients use too.

// Rust guideline compliant 2026-10-09

use std::time::{Duration, UNIX_EPOCH};

use harlo_core::server::{McpServer, Plugin, Provider, Stats, WebSearch};

use super::{App, Message};
use crate::task::Task;

/// A page over the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    /// The keyboard shortcuts.
    Shortcuts,
    /// Usage across sessions.
    Stats,
    /// The server's set-up, and About.
    Settings,
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
    /// Close the open page.
    Close,
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
    /// Applies something that happened to the pages.
    pub(super) fn sheet_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle(sheet) if self.sheets.open == Some(sheet) => self.sheets.open = None,
            Event::Toggle(sheet) => {
                self.sheets.open = Some(sheet);
                return match sheet {
                    Sheet::Shortcuts => Task::none(),
                    Sheet::Stats => self.load_stats(),
                    Sheet::Settings => self.load_setup(),
                };
            }
            Event::Close => self.sheets.open = None,
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
    fn load_setup(&mut self) -> Task<Message> {
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
