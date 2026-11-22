//! The window's saved layout and the notes, kept in a small SQLite database.
//!
//! The layout covers the project folders in the sidebar, which panels and
//! lists are open, and the open tabs for each server. It's saved after each
//! change, off the UI thread, and read once at start. Notes are kept one per
//! folder on each server.

// Rust guideline compliant 2026-10-09

use std::backtrace::Backtrace;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// The row the layout is stored under.
const LAYOUT_KEY: &str = "layout";

/// How the window was arranged when it was last changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// The project folders in the sidebar, in order.
    pub order: Vec<String>,
    /// Whether the sidebar is shown.
    pub sidebar: bool,
    /// The open right panel's title, if one is open.
    pub panel: Option<String>,
    /// Folders whose session lists show every session.
    pub expanded: Vec<String>,
    /// The open tabs for each server, keyed by its location label.
    pub tabs: BTreeMap<String, Tabs>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            sidebar: true,
            panel: Some("Context".to_owned()),
            expanded: Vec::new(),
            tabs: BTreeMap::new(),
        }
    }
}

/// The sessions open on one server.
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
        if !self.path.exists() {
            return Ok(Layout::default());
        }
        let connection = self.connect()?;
        let saved: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![LAYOUT_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(Error::database)?;
        match saved {
            Some(json) => serde_json::from_str(&json).map_err(Error::format),
            None => Ok(Layout::default()),
        }
    }

    /// Saves the layout, creating the file and its folder if needed.
    ///
    /// This blocks on disk I/O; from async code use [`Store::save_later`].
    ///
    /// # Errors
    ///
    /// Returns an error when the folder or file can't be created or written.
    pub fn save(&self, layout: &Layout) -> Result<(), Error> {
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder).map_err(Error::io)?;
        }
        let json = serde_json::to_string(layout).map_err(Error::format)?;
        self.connect()?
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![LAYOUT_KEY, json],
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
