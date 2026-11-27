//! The finder: one field that finds a session, a page of Settings or
//! something Harlo does, by a few letters of its name.
//!
//! It opens over whatever shows, with Ctrl+K. The letters typed need not
//! be next to each other (`stapp` finds "Settings: Appearance"); the best
//! fit is first, and the newest session among equals. With nothing typed
//! it lists the sessions used last, then what Harlo does. `>` before the
//! letters leaves the sessions out.

// Rust guideline compliant 2026-10-09

use harlo_core::finder::rank;
use lucide_icons::Icon;

use super::terminal::Event as TerminalEvent;
use super::{App, Message, Panel, Section, Sheet, SheetEvent, Task};
use crate::model::{Item, Picker};

/// The most rows the finder lists.
///
/// More than a screen of them is never read: typing another letter is
/// quicker than scrolling.
const LISTED: usize = 50;

/// How many sessions it lists before anything is typed, newest first.
const RECENT: usize = 8;

/// What a query starts with to leave the sessions out.
const NO_SESSIONS: char = '>';

/// Where a row of the finder goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Go {
    /// Open this session.
    Session(String),
    /// Start a session, in this folder when one is named.
    NewSession(Option<String>),
    /// Open Settings at this section.
    Section(Section),
    /// Open Settings, searched for this text.
    Settings(String),
    /// Open this page: the stats, Settings or the setup guide.
    Sheet(Sheet),
    /// Open or close this tool panel.
    Panel(Panel),
    /// Show or hide the sidebar.
    Sidebar,
    /// Show or hide the terminals.
    Terminal,
    /// Choose a folder to add as a project.
    AddProject,
}

/// A row of the finder.
#[derive(Debug, Clone)]
pub struct Row {
    /// The row as the menu shows it.
    pub item: Item,
    /// The characters of its name the letters typed fell on, by their
    /// place in it, ascending.
    pub marked: Vec<usize>,
}

/// Something the finder can find.
struct Entry {
    /// Its name.
    label: String,
    /// The note on its right: a session's project, or a shortcut.
    detail: String,
    /// Other words it's found by, which don't show.
    words: &'static str,
    glyph: Icon,
    go: Go,
}

impl Entry {
    fn does(label: impl Into<String>, detail: &str, glyph: Icon, go: Go) -> Self {
        Self {
            label: label.into(),
            detail: detail.to_owned(),
            words: "",
            glyph,
            go,
        }
    }
}

/// What a section of Settings holds, in the words people look for it by.
fn covers(section: Section) -> &'static str {
    match section {
        Section::General => "start at login tray reopen tabs updates avatar name",
        Section::Appearance => "theme colours colors dark light font size glass",
        Section::Notifications => "sounds quiet hours alerts",
        Section::Shortcuts => "keyboard keys",
        Section::Voice => "dictation microphone speech language",
        Section::Projects => "folders worktrees icons",
        Section::Git => "commits name email snapshots undo commit messages",
        Section::Providers => "sign in api keys accounts usage limits plans",
        Section::Models => "model menus hidden",
        Section::LocalModels => "engines on this computer download",
        Section::Agents => "approvals permissions rules",
        Section::Extensions => "mcp servers skills plugins",
        Section::Tools => "web search fetch",
        Section::Memory => "notes kept between sessions",
        Section::Experimental => "features being tried",
        Section::Schedules => "automations timed prompts",
        Section::Server => "ssh hosts connection import sessions",
        Section::Phone => "pair iphone relay",
        Section::About => "version licences",
    }
}

/// The sessions in the sidebar, newest first, each with where it is.
fn sessions(app: &App) -> Vec<Entry> {
    let mut found = Vec::new();
    for project in &app.projects {
        let groups = std::iter::once((project.name.clone(), &project.sessions)).chain(
            project
                .worktrees
                .iter()
                .map(|w| (format!("{} / {}", project.name, w.name), &w.sessions)),
        );
        for (place, sessions) in groups {
            for session in sessions {
                let label = if session.title.trim().is_empty() {
                    "Untitled session".to_owned()
                } else {
                    session.title.clone()
                };
                found.push((
                    session.age,
                    Entry {
                        label,
                        detail: place.clone(),
                        words: "",
                        glyph: Icon::MessageSquare,
                        go: Go::Session(session.id.clone()),
                    },
                ));
            }
        }
    }
    // (A stable sort: sessions of one age keep the sidebar's order.)
    found.sort_by_key(|(age, _)| *age);
    found.into_iter().map(|(_, entry)| entry).collect()
}

/// What Harlo does and the pages it has, in the order they're listed.
fn actions(app: &App) -> Vec<Entry> {
    let mut found = vec![Entry::does(
        "New session",
        "Ctrl+N",
        Icon::Plus,
        Go::NewSession(None),
    )];
    for folder in app.folder_choices() {
        if let Some(directory) = folder.directory {
            found.push(Entry::does(
                format!("New session in {}", folder.label),
                "",
                Icon::Plus,
                Go::NewSession(Some(directory)),
            ));
        }
    }
    found.push(Entry::does(
        "Add project…",
        "",
        Icon::FolderPlus,
        Go::AddProject,
    ));
    found.push(Entry::does(
        "Show or hide the sidebar",
        "",
        Icon::PanelLeft,
        Go::Sidebar,
    ));
    for panel in Panel::ALL {
        found.push(Entry::does(
            format!("{} panel", panel.title()),
            "",
            Icon::PanelRight,
            Go::Panel(panel),
        ));
    }
    found.push(Entry::does(
        "Terminal",
        "",
        Icon::SquareTerminal,
        Go::Terminal,
    ));
    found.push(Entry::does(
        "Usage stats",
        "",
        Icon::ChartColumn,
        Go::Sheet(Sheet::Stats),
    ));
    found.push(Entry::does(
        "Setup guide",
        "",
        Icon::Compass,
        Go::Sheet(Sheet::Setup),
    ));
    found.push(Entry::does(
        "Settings",
        "Ctrl+,",
        Icon::Settings,
        Go::Sheet(Sheet::Settings),
    ));
    for section in Section::ALL {
        found.push(Entry {
            words: covers(section),
            ..Entry::does(
                format!("Settings: {}", section.title()),
                "",
                Icon::Settings,
                Go::Section(section),
            )
        });
    }
    found
}

/// The finder's rows for what is typed in it, best first.
#[must_use]
pub fn rows(app: &App) -> Vec<Row> {
    let typed = app.picker_filter.trim();
    let (query, with_sessions) = match typed.strip_prefix(NO_SESSIONS) {
        Some(rest) => (rest.trim(), false),
        None => (typed, true),
    };
    let mut entries = Vec::new();
    if with_sessions {
        entries = sessions(app);
        if query.is_empty() {
            entries.truncate(RECENT);
        }
    }
    entries.extend(actions(app));

    let searched: Vec<String> = entries
        .iter()
        .map(|e| format!("{} {}", e.label, e.words))
        .collect();
    let searched: Vec<&str> = searched.iter().map(String::as_str).collect();
    let mut entries: Vec<Option<Entry>> = entries.into_iter().map(Some).collect();
    let mut rows: Vec<Row> = rank(query, &searched)
        .into_iter()
        .take(LISTED)
        .filter_map(|found| {
            let entry = entries.get_mut(found.index)?.take()?;
            // Letters that fell on the words that don't show mark nothing.
            let shown = entry.label.chars().count();
            let marked = found.marked.into_iter().filter(|at| *at < shown).collect();
            Some(Row {
                item: Item {
                    label: entry.label,
                    detail: (!entry.detail.is_empty()).then_some(entry.detail),
                    below: None,
                    dot: None,
                    glyph: Some(entry.glyph),
                    selected: false,
                    message: Message::Go(entry.go),
                },
                marked,
            })
        })
        .collect();
    // Settings has a search of its own, over every row of every section.
    if !query.is_empty() {
        rows.push(Row {
            item: Item {
                label: format!("Search Settings for “{query}”"),
                detail: None,
                below: None,
                dot: None,
                glyph: Some(Icon::Search),
                selected: false,
                message: Message::Go(Go::Settings(query.to_owned())),
            },
            marked: Vec::new(),
        });
    }
    rows
}

impl App {
    /// Goes where a row of the finder leads, closing the finder.
    ///
    /// A page that covers the window closes too, unless the row leads to
    /// one.
    pub(super) fn go(&mut self, go: Go) -> Task<Message> {
        if self.picker == Some(Picker::Finder) {
            self.picker = None;
        }
        let settings = self.sheets.open == Some(Sheet::Settings);
        let message = match go {
            Go::Section(section) if settings => Message::Sheet(SheetEvent::Section(section)),
            Go::Section(section) => Message::Sheet(SheetEvent::Open(section)),
            Go::Settings(query) => {
                let open = if settings {
                    Task::none()
                } else {
                    self.update(Message::Sheet(SheetEvent::Toggle(Sheet::Settings)))
                };
                let search = self.update(Message::Sheet(SheetEvent::Query(query)));
                return Task::batch([open, search]);
            }
            Go::Sheet(sheet) if self.sheets.open == Some(sheet) => return Task::none(),
            Go::Sheet(sheet) => Message::Sheet(SheetEvent::Toggle(sheet)),
            other => {
                self.sheets.open = None;
                match other {
                    Go::Session(id) => Message::OpenSession(id),
                    Go::NewSession(folder) => Message::NewSession(folder),
                    Go::Panel(panel) => Message::TogglePanel(panel),
                    Go::Sidebar => Message::ToggleSidebar,
                    Go::Terminal => Message::Terminal(TerminalEvent::Toggle),
                    Go::AddProject => Message::AddProject,
                    Go::Section(_) | Go::Settings(_) | Go::Sheet(_) => return Task::none(),
                }
            }
        };
        self.update(message)
    }
}
