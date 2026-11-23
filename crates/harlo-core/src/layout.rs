//! The window's saved layout, Harlo's own settings and the notes, kept in a
//! small SQLite database.
//!
//! The layout covers the project folders in the sidebar, which panels and
//! lists are open and how big the user made them, and the open tabs for
//! each server. The preferences are what the user chose in Settings that
//! belongs to Harlo rather than to the OpenCode server. Both are saved after
//! each change, off the UI thread, and read once at start. Notes are kept one
//! per folder on each server.

// Rust guideline compliant 2026-10-09

use std::backtrace::Backtrace;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::theme::{Contrast, DiffColors};

/// The row the layout is stored under.
const LAYOUT_KEY: &str = "layout";

/// The row the preferences are stored under.
const PREFERENCES_KEY: &str = "preferences";

/// Harlo's own settings, as the user chose them in Settings.
///
/// Settings that belong to the OpenCode server aren't here; Harlo reads and
/// changes those through the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent on/off settings, each its own switch, saved as they are"
)]
pub struct Preferences {
    /// Whether the sessions open when Harlo last closed open again.
    pub reopen_tabs: bool,
    /// Whether closing the window leaves Harlo running in the tray, when
    /// the desktop shows one.
    pub keep_in_tray: bool,
    /// Whether the setup guide was finished or skipped, so it no longer
    /// opens on its own at start.
    pub setup_done: bool,
    /// Desktop notifications.
    pub notifications: crate::notifications::Notifications,
    /// The theme shown while the window is dark, by its id, such as
    /// `harlo-dark` or `gruvbox-dark-medium`.
    pub dark_theme: String,
    /// The theme shown while the window is light, by its id.
    pub light_theme: String,
    /// Whether the dark or the light theme shows.
    pub appearance: Appearance,
    /// How strongly text stands out from the background.
    pub contrast: Contrast,
    /// The colours added and removed lines take in diffs.
    pub diff_colors: DiffColors,
    /// How fast panels slide and new things fade in, or not at all.
    pub motion: Speed,
    /// How wide the chat's column gets.
    pub chat_width: ChatWidth,
    /// The size of the interface's and the chat's text.
    pub text_size: TextSize,
    /// The size of code, diffs and the terminal.
    pub code_size: TextSize,
    /// Whether menus and dialogs are translucent, blurring what's under them.
    pub glass: bool,
    /// The model new sessions in a project start with, as
    /// `provider/model`, by the project's folder; kept here, not on the
    /// server.
    pub project_models: BTreeMap<String, String>,
    /// The model that writes commit messages, as `provider/model`; `None`
    /// leaves it to the server's default model.
    pub commit_model: Option<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            reopen_tabs: true,
            keep_in_tray: true,
            setup_done: false,
            notifications: crate::notifications::Notifications::default(),
            dark_theme: crate::theme::HARLO_DARK.to_owned(),
            light_theme: crate::theme::HARLO_LIGHT.to_owned(),
            appearance: Appearance::System,
            contrast: Contrast::Standard,
            diff_colors: DiffColors::GreenRed,
            motion: Speed::Normal,
            chat_width: ChatWidth::Normal,
            text_size: TextSize::Normal,
            code_size: TextSize::Normal,
            glass: false,
            project_models: BTreeMap::new(),
            commit_model: None,
        }
    }
}

/// Whether the dark or the light theme shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    /// Light or dark as the desktop is.
    #[default]
    System,
    /// Always the light theme.
    Light,
    /// Always the dark theme.
    Dark,
}

/// How fast motion runs: panels sliding, new things fading in, work
/// breathing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    /// No motion: everything changes at once, and nothing breathes.
    Off,
    /// Quicker than usual.
    Fast,
    /// As designed.
    #[default]
    Normal,
    /// Slower than usual.
    Slow,
}

impl Speed {
    /// How long motion takes, as a share of its designed length.
    #[must_use]
    pub fn factor(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Fast => FAST_MOTION,
            Self::Normal => 1.0,
            Self::Slow => SLOW_MOTION,
        }
    }
}

/// How long fast motion takes, as a share of its designed length.
const FAST_MOTION: f32 = 0.6;

/// How long slow motion takes, as a share of its designed length.
const SLOW_MOTION: f32 = 1.6;

/// How wide the chat's column gets in a wide window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatWidth {
    /// Short lines, 640 pixels at most.
    Narrow,
    /// As designed, 780 pixels at most.
    #[default]
    Normal,
    /// Longer lines, 960 pixels at most.
    Wide,
    /// As wide as the window lets it.
    Full,
}

impl ChatWidth {
    /// The column's widest, in logical pixels; `None` for no limit.
    #[must_use]
    pub fn pixels(self) -> Option<f32> {
        match self {
            Self::Narrow => Some(NARROW_CHAT),
            Self::Normal => Some(NORMAL_CHAT),
            Self::Wide => Some(WIDE_CHAT),
            Self::Full => None,
        }
    }
}

/// The narrow chat column's widest, in logical pixels.
const NARROW_CHAT: f32 = 640.0;

/// The chat column's designed widest, in logical pixels.
const NORMAL_CHAT: f32 = 780.0;

/// The wide chat column's widest, in logical pixels.
const WIDE_CHAT: f32 = 960.0;

/// A size for text, relative to the design.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSize {
    /// A step smaller.
    Small,
    /// As designed.
    #[default]
    Normal,
    /// A step larger.
    Large,
    /// Two steps larger.
    Larger,
}

impl TextSize {
    /// The scale applied to the designed sizes.
    #[must_use]
    pub fn scale(self) -> f32 {
        match self {
            Self::Small => SMALLER_TEXT,
            Self::Normal => 1.0,
            Self::Large => LARGER_TEXT,
            Self::Larger => LARGEST_TEXT,
        }
    }
}

/// The scale of small text.
const SMALLER_TEXT: f32 = 0.92;

/// The scale of large text.
const LARGER_TEXT: f32 = 1.1;

/// The scale of larger text.
const LARGEST_TEXT: f32 = 1.2;

/// How the window was arranged when it was last changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// The project folders in the sidebar, in order.
    pub order: Vec<String>,
    /// Project folders removed from the sidebar.
    pub hidden: Vec<String>,
    /// Whether the sidebar is shown.
    pub sidebar: bool,
    /// Folders whose session lists show every session.
    pub expanded: Vec<String>,
    /// The tool panel open on the right, by its title, if one is.
    pub panel: Option<String>,
    /// How wide the user made the sidebar, in logical pixels.
    pub sidebar_width: Option<u32>,
    /// How wide the user made the right panel, in logical pixels.
    pub panel_width: Option<u32>,
    /// How tall the user made the terminals, in logical pixels.
    pub terminal_height: Option<u32>,
    /// The open tabs for each server, keyed by its location label.
    pub tabs: BTreeMap<String, Tabs>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            hidden: Vec::new(),
            sidebar: true,
            expanded: Vec::new(),
            panel: None,
            sidebar_width: None,
            panel_width: None,
            terminal_height: None,
            tabs: BTreeMap::new(),
        }
    }
}

/// The sessions open on one server.
///
/// Layouts saved while the main area could be split into panes also hold
/// that arrangement; it's ignored when read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tabs {
    /// Open session IDs, in tab order.
    pub open: Vec<String>,
    /// The session shown, if any.
    pub active: Option<String>,
}

/// Where the layout and notes are saved: a SQLite file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    /// A store at `path`; nothing is created until the first save.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The store in the user's state folder (`$XDG_STATE_HOME/harlo/harlo.db`).
    ///
    /// Returns `None` when the system has no such folder.
    #[must_use]
    pub fn user() -> Option<Self> {
        let folder = dirs::state_dir().or_else(dirs::data_local_dir)?;
        Some(Self::new(folder.join("harlo").join("harlo.db")))
    }

    /// The database file's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the saved layout, or the default when none was saved.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but can't be read, or holds a
    /// layout this version can't parse.
    pub fn load(&self) -> Result<Layout, Error> {
        self.read(LAYOUT_KEY)
    }

    /// Reads the saved preferences, or the defaults when none were saved.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but can't be read, or holds
    /// preferences this version can't parse.
    pub fn load_preferences(&self) -> Result<Preferences, Error> {
        self.read(PREFERENCES_KEY)
    }

    /// Saves the layout, creating the file and its folder if needed.
    ///
    /// This blocks on disk I/O; from async code use [`Store::save_later`].
    ///
    /// # Errors
    ///
    /// Returns an error when the folder or file can't be created or written.
    pub fn save(&self, layout: &Layout) -> Result<(), Error> {
        self.write(LAYOUT_KEY, layout)
    }

    /// Saves the preferences, creating the file and its folder if needed.
    ///
    /// This blocks on disk I/O; from async code use
    /// [`Store::save_preferences_later`].
    ///
    /// # Errors
    ///
    /// Returns an error when the folder or file can't be created or written.
    pub fn save_preferences(&self, preferences: &Preferences) -> Result<(), Error> {
        self.write(PREFERENCES_KEY, preferences)
    }

    /// Saves the preferences on a blocking thread.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Store::save_preferences`], or an error when
    /// the save thread is cancelled.
    pub async fn save_preferences_later(self, preferences: Preferences) -> Result<(), Error> {
        tokio::task::spawn_blocking(move || self.save_preferences(&preferences))
            .await
            .map_err(|join| Error::new(Kind::Cancelled(join.to_string())))?
    }

    /// Reads the value saved under `key`, or its default when none was saved.
    fn read<T: serde::de::DeserializeOwned + Default>(&self, key: &str) -> Result<T, Error> {
        if !self.path.exists() {
            return Ok(T::default());
        }
        let saved: Option<String> = self
            .connect()?
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Error::database)?;
        match saved {
            Some(json) => serde_json::from_str(&json).map_err(Error::format),
            None => Ok(T::default()),
        }
    }

    /// Saves `value` under `key`, creating the file and its folder if needed.
    fn write(&self, key: &str, value: &impl Serialize) -> Result<(), Error> {
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder).map_err(Error::io)?;
        }
        let json = serde_json::to_string(value).map_err(Error::format)?;
        self.connect()?
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, json],
            )
            .map_err(Error::database)?;
        Ok(())
    }

    /// Saves the layout on a blocking thread, so the caller isn't held up by disk I/O.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Store::save`], or an error when the save
    /// thread is cancelled.
    pub async fn save_later(self, layout: Layout) -> Result<(), Error> {
        tokio::task::spawn_blocking(move || self.save(&layout))
            .await
            .map_err(|join| Error::new(Kind::Cancelled(join.to_string())))?
    }

    /// Reads the note for `folder` on the server labelled `location`; empty
    /// when there's none.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but can't be read.
    pub fn load_note(&self, location: &str, folder: &str) -> Result<String, Error> {
        if !self.path.exists() {
            return Ok(String::new());
        }
        let text: Option<String> = self
            .connect()?
            .query_row(
                "SELECT text FROM notes WHERE location = ?1 AND folder = ?2",
                params![location, folder],
                |row| row.get(0),
            )
            .optional()
            .map_err(Error::database)?;
        Ok(text.unwrap_or_default())
    }

    /// Saves the note for `folder` on the server labelled `location`; an
    /// empty note is removed.
    ///
    /// This blocks on disk I/O; from async code use [`Store::save_note_later`].
    ///
    /// # Errors
    ///
    /// Returns an error when the folder or file can't be created or written.
    pub fn save_note(&self, location: &str, folder: &str, text: &str) -> Result<(), Error> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(Error::io)?;
        }
        let connection = self.connect()?;
        if text.is_empty() {
            connection.execute(
                "DELETE FROM notes WHERE location = ?1 AND folder = ?2",
                params![location, folder],
            )
        } else {
            connection.execute(
                "INSERT INTO notes (location, folder, text) VALUES (?1, ?2, ?3)
                 ON CONFLICT (location, folder) DO UPDATE SET text = excluded.text",
                params![location, folder, text],
            )
        }
        .map_err(Error::database)?;
        Ok(())
    }

    /// Reads a note on a blocking thread.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Store::load_note`], or an error when the
    /// thread is cancelled.
    pub async fn load_note_later(self, location: String, folder: String) -> Result<String, Error> {
        tokio::task::spawn_blocking(move || self.load_note(&location, &folder))
            .await
            .map_err(|join| Error::new(Kind::Cancelled(join.to_string())))?
    }

    /// Saves a note on a blocking thread.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Store::save_note`], or an error when the
    /// thread is cancelled.
    pub async fn save_note_later(
        self,
        location: String,
        folder: String,
        text: String,
    ) -> Result<(), Error> {
        tokio::task::spawn_blocking(move || self.save_note(&location, &folder, &text))
            .await
            .map_err(|join| Error::new(Kind::Cancelled(join.to_string())))?
    }

    fn connect(&self) -> Result<Connection, Error> {
        let connection = Connection::open(&self.path).map_err(Error::database)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS settings (
                     key TEXT PRIMARY KEY NOT NULL,
                     value TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS notes (
                     location TEXT NOT NULL,
                     folder TEXT NOT NULL,
                     text TEXT NOT NULL,
                     PRIMARY KEY (location, folder)
                 );",
            )
            .map_err(Error::database)?;
        Ok(connection)
    }
}

/// Why the layout or a note couldn't be loaded or saved.
#[derive(Debug)]
pub struct Error {
    kind: Kind,
    backtrace: Backtrace,
}

#[derive(Debug)]
enum Kind {
    Io(std::io::Error),
    Database(rusqlite::Error),
    Format(serde_json::Error),
    Cancelled(String),
}

impl Error {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            backtrace: Backtrace::capture(),
        }
    }

    fn io(error: std::io::Error) -> Self {
        Self::new(Kind::Io(error))
    }

    fn database(error: rusqlite::Error) -> Self {
        Self::new(Kind::Database(error))
    }

    fn format(error: serde_json::Error) -> Self {
        Self::new(Kind::Format(error))
    }

    /// Returns where the error was created, when backtraces are enabled.
    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::Io(e) => write!(f, "can't write Harlo's data folder: {e}"),
            Kind::Database(e) => write!(f, "Harlo's database failed: {e}"),
            Kind::Format(e) => write!(f, "the saved layout can't be read: {e}"),
            Kind::Cancelled(reason) => write!(f, "the database work was cancelled: {reason}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            Kind::Io(e) => Some(e),
            Kind::Database(e) => Some(e),
            Kind::Format(e) => Some(e),
            Kind::Cancelled(_) => None,
        }
    }
}
