//! What the server keeps on disk: sessions, their messages (indexed for
//! search), every durable event in order, notes and settings.
//!
//! One SQLite file. Events carry a sequence number that only grows, so a
//! client that reconnects can name the last one it saw and get the rest.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use harlo_engine::memory::Note;
use harlo_engine::{Message, ModelRef, Rule, Tokens};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::proposals::Proposal;

/// A failure to read or write the store.
pub type Error = rusqlite::Error;

/// Makes the file at `path`, empty when it isn't there, readable by its
/// owner alone: it holds sign-ins and keys. Best effort; SQLite reports a
/// file it can't open.
fn owner_only(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
        if path.exists() {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        } else if path.extension().is_some_and(|e| e == "db") {
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path);
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// How many messages are indexed in one transaction when the search index
/// is made again: enough to be quick, few enough to hold in memory, tool
/// results and all.
const INDEX_BATCH: i64 = 100;

/// How many of the newest events are kept for a client that reconnects to
/// catch up from. A client further behind loads the sessions afresh, as
/// every client does when it first connects.
const KEPT_EVENTS: i64 = 10_000;

/// After how many new events the older ones are removed again: often
/// enough that the table stays near [`KEPT_EVENTS`], seldom enough to cost
/// nothing per event.
const PRUNE_EVERY: u64 = 1_000;

/// Removing more events than this at startup also rewrites the file, to
/// give their room back: a store from before events had a limit.
const SHRINK_AFTER: usize = 50_000;

/// A session, as the store keeps it and clients read it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Its ID.
    pub id: String,
    /// The project its folder belongs to.
    #[serde(rename = "projectID")]
    pub project_id: String,
    /// Its folder.
    pub location: Location,
    /// Its title, once it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// When it was made, changed, last busy and last idle.
    pub time: SessionTime,
    /// What it has cost in US dollars.
    pub cost: f64,
    /// The tokens it has used.
    pub tokens: Tokens,
    /// How its last run ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// The undo staged in it: `messageID`, the
    /// `files` restoring changed, and Harlo's `snapshot` and `report`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revert: Option<serde_json::Value>,
    /// Its own permission rules, applied after the agent's and the config's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<Rule>,
    /// The model it uses, when one was chosen for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    /// The agent it runs as, when one was chosen for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The session that handed it its work, when it's a subagent's.
    #[serde(rename = "parentID", default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// The session it was forked from, and where: `{sessionID, boundary}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork: Option<Value>,
}

/// A folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// Its absolute path.
    pub directory: String,
}

/// A session's times, in Unix milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTime {
    /// When it was made.
    pub created: i64,
    /// When it last changed.
    pub updated: i64,
    /// When its last run started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy: Option<i64>,
    /// When its last run ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle: Option<i64>,
    /// When it was archived, while it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<i64>,
}

/// A message that matched a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Its session.
    pub session: String,
    /// The message.
    pub message: String,
    /// The matching part, with the words in brackets.
    pub snippet: String,
    /// The session's title, if it has one.
    pub title: Option<String>,
    /// When the session was made, in Unix milliseconds.
    pub created: Option<i64>,
    /// The session's folder.
    pub directory: Option<String>,
}

/// A stored event: its sequence number, the session (or other thing) it
/// belongs to, and its JSON without the sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    /// Its place in the order of all events.
    pub seq: u64,
    /// What it's about, such as a session ID.
    pub aggregate: String,
    /// The event.
    pub body: Value,
}

/// The server's SQLite file.
///
/// Writes go through one connection. Reading sessions, messages and
/// events goes through a second one, so a long read (a whole session's
/// history, a search) holds up no agent saving what it streams.
#[derive(Debug)]
pub struct Store {
    conn: Mutex<Connection>,
    /// The connection reads go through; a store in memory has only the one.
    reads: Option<Mutex<Connection>>,
}

/// How long a connection waits for the other to let go of the file.
const BUSY: std::time::Duration = std::time::Duration::from_secs(5);

/// The text of `message` that search looks in: a prompt, a command, or
/// what a step said.
pub(crate) fn searched(message: &Message) -> String {
    match message {
        Message::User { text, .. } => text.clone(),
        Message::Shell(run) => run.command.clone(),
        Message::Assistant(step) => step
            .content
            .iter()
            .filter_map(|p| match p {
                harlo_engine::Part::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Message::Synthetic { .. } | Message::Idle { .. } | Message::Compaction(_) => String::new(),
    }
}

/// Indexes `message`'s text for search in place of the index's row `old`,
/// the one it had before; returns its row now, none when it has no text.
///
/// The message keeps its row's number (`message.text_row`), because a row
/// of the index is found quickly by its number alone: looking one up by
/// its message or session reads the whole index.
fn index(
    conn: &Connection,
    old: Option<i64>,
    session: &str,
    message: &Message,
) -> Result<Option<i64>, Error> {
    if let Some(old) = old {
        conn.execute("DELETE FROM message_text WHERE rowid = ?1", [old])?;
    }
    let text = searched(message);
    if text.trim().is_empty() {
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO message_text (text, session, message) VALUES (?1, ?2, ?3)",
        params![text, session, message.id()],
    )?;
    Ok(Some(conn.last_insert_rowid()))
}

impl Store {
    /// Opens (or creates) the store at `path`.
    ///
    /// # Errors
    ///
    /// Fails when the file can't be opened or its tables made.
    pub fn open(path: &Path) -> Result<Self, Error> {
        owner_only(path);
        let mut store = Self::with(Connection::open(path)?)?;
        // (Its journal files take the file's permissions, but were maybe
        // made before it was owner-only.)
        for end in ["-wal", "-shm"] {
            let mut journal = path.as_os_str().to_owned();
            journal.push(end);
            owner_only(Path::new(&journal));
        }
        let reads = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        reads.busy_timeout(BUSY)?;
        store.reads = Some(Mutex::new(reads));
        Ok(store)
    }

    /// A store in memory, for tests.
    ///
    /// # Errors
    ///
    /// Fails when its tables can't be made.
    pub fn in_memory() -> Result<Self, Error> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(conn: Connection) -> Result<Self, Error> {
        conn.busy_timeout(BUSY)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS session (id TEXT PRIMARY KEY, info TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS message (
                 id TEXT PRIMARY KEY, session TEXT NOT NULL, body TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS message_by_session ON message (session, id);
             CREATE TABLE IF NOT EXISTS event (
                 seq INTEGER PRIMARY KEY AUTOINCREMENT, aggregate TEXT NOT NULL, body TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS decision (
                 request TEXT PRIMARY KEY, session TEXT NOT NULL, reply TEXT NOT NULL,
                 message TEXT, time INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS note (
                 id TEXT PRIMARY KEY, project TEXT, body TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS proposal (
                 id TEXT PRIMARY KEY, project TEXT, body TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS setting (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS project (id TEXT PRIMARY KEY, settings TEXT NOT NULL);
             CREATE VIRTUAL TABLE IF NOT EXISTS message_text
                 USING fts5(text, session UNINDEXED, message UNINDEXED);",
        )?;
        Self::make_approvals(&conn)?;
        Self::make_automations(&conn)?;
        Self::make_credentials(&conn)?;
        // A message knows its row in the search index. A store from before
        // it did gets the column, and its index made again, once.
        let numbered: bool = conn.query_row(
            "SELECT count(*) > 0 FROM pragma_table_info('message') WHERE name = 'text_row'",
            [],
            |r| r.get(0),
        )?;
        if !numbered {
            conn.execute_batch("ALTER TABLE message ADD COLUMN text_row INTEGER")?;
        }
        let store = Self {
            conn: Mutex::new(conn),
            reads: None,
        };
        if !numbered {
            store.index_all()?;
        }
        // Events kept from before there was a limit go, and the file gives
        // their room back. (A store that can't be rewritten, on a full
        // disk, still opens.)
        if store.prune_events()? > SHRINK_AFTER
            && let Err(e) = store.conn().execute_batch("VACUUM")
        {
            tracing::warn!(name: "store.shrink.failed", error = %e, "couldn't shrink the store");
        }
        Ok(store)
    }

    /// Indexes every stored message's text for search again, a few
    /// hundred at a time.
    fn index_all(&self) -> Result<(), Error> {
        self.conn().execute("DELETE FROM message_text", [])?;
        let mut after = String::new();
        loop {
            let conn = self.conn();
            let rows: Vec<(String, String, String)> = conn
                .prepare(
                    "SELECT id, session, body FROM message WHERE id > ?1 ORDER BY id LIMIT ?2",
                )?
                .query_map(params![after, INDEX_BATCH], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })?
                .collect::<Result<_, _>>()?;
            let Some((last, _, _)) = rows.last() else {
                return Ok(());
            };
            after.clone_from(last);
            let batch = conn.unchecked_transaction()?;
            for (id, session, body) in &rows {
                // (A message this version can't read stays out of the index.)
                let row = match serde_json::from_str::<Message>(body) {
                    Ok(message) => index(&batch, None, session, &message)?,
                    Err(_) => None,
                };
                batch.execute(
                    "UPDATE message SET text_row = ?1 WHERE id = ?2",
                    params![row, id],
                )?;
            }
            batch.commit()?;
        }
    }

    /// The messages whose text has every word of `words`, best first, at
    /// most `limit`: of `project`'s sessions, or of every project's, and
    /// never of session `asking`.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn search(
        &self,
        project: Option<&str>,
        asking: Option<&str>,
        words: &[String],
        limit: usize,
    ) -> Result<Vec<Hit>, Error> {
        // Each word quoted, so punctuation can't make a query of it.
        let query = words
            .iter()
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ");
        let conn = self.reader();
        let mut stmt = conn.prepare(
            "SELECT t.session, t.message, snippet(message_text, 0, '[', ']', '…', 16),
                    json_extract(s.info, '$.title'), json_extract(s.info, '$.time.created'),
                    json_extract(s.info, '$.location.directory')
             FROM message_text t JOIN session s ON s.id = t.session
             WHERE message_text MATCH ?1
               AND (?2 IS NULL OR json_extract(s.info, '$.projectID') = ?2)
               AND (?4 IS NULL OR t.session != ?4)
             ORDER BY rank LIMIT ?3",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        stmt.query_map(params![query, project, limit, asking], |r| {
            Ok(Hit {
                session: r.get(0)?,
                message: r.get(1)?,
                snippet: r.get(2)?,
                title: r.get(3)?,
                created: r.get(4)?,
                directory: r.get(5)?,
            })
        })?
        .collect()
    }

    /// Saves a note, new or changed.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn put_note(&self, note: &Note) -> Result<(), Error> {
        let body = serde_json::to_string(note).map_err(json_error)?;
        self.conn().execute(
            "INSERT INTO note (id, project, body) VALUES (?1, ?2, ?3)
             ON CONFLICT (id) DO UPDATE SET project = excluded.project, body = excluded.body",
            params![note.id, note.project, body],
        )?;
        Ok(())
    }

    /// Note `id`, if it's kept.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn note(&self, id: &str) -> Result<Option<Note>, Error> {
        self.conn()
            .query_row("SELECT body FROM note WHERE id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .map(|b| serde_json::from_str(&b).map_err(json_error))
            .transpose()
    }

    /// The user's notes and, with `project`, that project's, oldest first;
    /// replaced ones included.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn notes(&self, project: Option<&str>) -> Result<Vec<Note>, Error> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT body FROM note WHERE project IS NULL OR project = ?1 ORDER BY id")?;
        stmt.query_map([project], |r| r.get::<_, String>(0))?
            .map(|b| serde_json::from_str(&b?).map_err(json_error))
            .collect()
    }

    /// Forgets note `id`; whether there was one.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn delete_note(&self, id: &str) -> Result<bool, Error> {
        Ok(self
            .conn()
            .execute("DELETE FROM note WHERE id = ?1", [id])?
            > 0)
    }

    /// Saves a proposed note, new or decided.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn put_proposal(&self, proposal: &Proposal) -> Result<(), Error> {
        let body = serde_json::to_string(proposal).map_err(json_error)?;
        self.conn().execute(
            "INSERT INTO proposal (id, project, body) VALUES (?1, ?2, ?3)
             ON CONFLICT (id) DO UPDATE SET project = excluded.project, body = excluded.body",
            params![proposal.id, proposal.project, body],
        )?;
        Ok(())
    }

    /// Proposed note `id`, if there is one.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn proposal(&self, id: &str) -> Result<Option<Proposal>, Error> {
        self.conn()
            .query_row("SELECT body FROM proposal WHERE id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .map(|b| serde_json::from_str(&b).map_err(json_error))
            .transpose()
    }

    /// The notes proposed for the user and, with `project`, for that
    /// project, oldest first; decided ones included.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn proposals(&self, project: Option<&str>) -> Result<Vec<Proposal>, Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT body FROM proposal WHERE project IS NULL OR project = ?1 ORDER BY id",
        )?;
        stmt.query_map([project], |r| r.get::<_, String>(0))?
            .map(|b| serde_json::from_str(&b?).map_err(json_error))
            .collect()
    }

    /// Setting `key`, if it was set.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn setting(&self, key: &str) -> Result<Option<String>, Error> {
        self.conn()
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
    }

    /// The settings whose key starts with `prefix`, as key and value.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn settings_under(&self, prefix: &str) -> Result<Vec<(String, String)>, Error> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT key, value FROM setting WHERE substr(key, 1, length(?1)) = ?1")?;
        stmt.query_map([prefix], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }

    /// Sets setting `key` to `value`.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn put_setting(&self, key: &str, value: &str) -> Result<(), Error> {
        self.conn().execute(
            "INSERT INTO setting (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The connection to read sessions, messages and events through.
    fn reader(&self) -> MutexGuard<'_, Connection> {
        self.reads
            .as_ref()
            .unwrap_or(&self.conn)
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Changes session `id` as `change` says and saves it, as one step.
    ///
    /// Several things change a session at once: its run adds what a step
    /// cost while its title arrives and a person picks another model. Each
    /// change is made to the session as it stands, so none undoes another.
    /// Returns the session as saved, or `None` when there's no such one.
    ///
    /// # Errors
    ///
    /// Fails when the read or the write fails.
    pub fn change_session(
        &self,
        id: &str,
        change: impl FnOnce(&mut Session),
    ) -> Result<Option<Session>, Error> {
        let conn = self.conn();
        let both = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)?;
        let info: Option<String> = both
            .query_row("SELECT info FROM session WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        let Some(info) = info else {
            return Ok(None);
        };
        let mut session: Session = serde_json::from_str(&info).map_err(json_error)?;
        change(&mut session);
        let info = serde_json::to_string(&session).map_err(json_error)?;
        both.execute(
            "UPDATE session SET info = ?2 WHERE id = ?1",
            params![id, info],
        )?;
        both.commit()?;
        Ok(Some(session))
    }

    /// Saves a session, new or changed.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn put_session(&self, session: &Session) -> Result<(), Error> {
        let info = serde_json::to_string(session).map_err(json_error)?;
        self.conn().execute(
            "INSERT INTO session (id, info) VALUES (?1, ?2)
             ON CONFLICT (id) DO UPDATE SET info = excluded.info",
            params![session.id, info],
        )?;
        Ok(())
    }

    /// The session with ID `id`.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn session(&self, id: &str) -> Result<Option<Session>, Error> {
        let info: Option<String> = self
            .reader()
            .query_row("SELECT info FROM session WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        info.map(|i| serde_json::from_str(&i).map_err(json_error))
            .transpose()
    }

    /// Every session, most recently changed first.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn sessions(&self) -> Result<Vec<Session>, Error> {
        let conn = self.reader();
        let mut stmt = conn.prepare("SELECT info FROM session")?;
        let mut out = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .map(|i| serde_json::from_str::<Session>(&i?).map_err(json_error))
            .collect::<Result<Vec<_>, _>>()?;
        out.sort_by_key(|s| std::cmp::Reverse(s.time.updated));
        Ok(out)
    }

    /// Removes session `id` and its messages for good.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn delete_session(&self, id: &str) -> Result<(), Error> {
        let conn = self.conn();
        let all = conn.unchecked_transaction()?;
        all.execute(
            "DELETE FROM message_text WHERE rowid IN
                 (SELECT text_row FROM message WHERE session = ?1)",
            [id],
        )?;
        all.execute("DELETE FROM message WHERE session = ?1", [id])?;
        all.execute("DELETE FROM event WHERE aggregate = ?1", [id])?;
        all.execute("DELETE FROM session WHERE id = ?1", [id])?;
        // Its goal and the goal's checkpoint go with it, and where the
        // look for notes to propose had come to.
        all.execute(
            "DELETE FROM setting WHERE key IN ('goal/' || ?1, 'proposals/' || ?1)",
            [id],
        )?;
        all.commit()?;
        Ok(())
    }

    /// Project `id`'s own settings (`name`, `icon`, `commands`), as an
    /// object; empty when it has none.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn project_settings(&self, id: &str) -> Result<Value, Error> {
        let settings: Option<String> = self
            .conn()
            .query_row("SELECT settings FROM project WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(settings
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())))
    }

    /// Saves project `id`'s own settings.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn put_project_settings(&self, id: &str, settings: &Value) -> Result<(), Error> {
        self.conn().execute(
            "INSERT INTO project (id, settings) VALUES (?1, ?2)
             ON CONFLICT (id) DO UPDATE SET settings = excluded.settings",
            params![id, settings.to_string()],
        )?;
        Ok(())
    }

    /// Saves a message of session `session`, new or changed.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn put_message(&self, session: &str, message: &Message) -> Result<(), Error> {
        let body = serde_json::to_string(message).map_err(json_error)?;
        let conn = self.conn();
        let both = conn.unchecked_transaction()?;
        let old: Option<i64> = both
            .query_row(
                "SELECT text_row FROM message WHERE id = ?1",
                [message.id()],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let row = index(&both, old, session, message)?;
        both.execute(
            "INSERT INTO message (id, session, body, text_row) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (id) DO UPDATE SET body = excluded.body, text_row = excluded.text_row",
            params![message.id(), session, body, row],
        )?;
        both.commit()?;
        Ok(())
    }

    /// Whether message `id` is stored.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn has_message(&self, id: &str) -> Result<bool, Error> {
        self.conn()
            .query_row("SELECT 1 FROM message WHERE id = ?1", [id], |_| Ok(()))
            .optional()
            .map(|found| found.is_some())
    }

    /// Records the decision on permission request `request`, once: false
    /// when it was already decided.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn record_decision(
        &self,
        request: &str,
        session: &str,
        reply: &str,
        message: Option<&str>,
        time: i64,
    ) -> Result<bool, Error> {
        let changed = self.conn().execute(
            "INSERT OR IGNORE INTO decision (request, session, reply, message, time)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![request, session, reply, message, time],
        )?;
        Ok(changed == 1)
    }

    /// The decision on permission request `request` of `session`, if one
    /// was made: its reply (`once`, `always` or `reject`).
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn decision(&self, session: &str, request: &str) -> Result<Option<String>, Error> {
        self.conn()
            .query_row(
                "SELECT reply FROM decision WHERE request = ?1 AND session = ?2",
                [request, session],
                |r| r.get(0),
            )
            .optional()
    }

    /// How many decisions were made in `session`.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn decisions(&self, session: &str) -> Result<i64, Error> {
        self.conn().query_row(
            "SELECT COUNT(*) FROM decision WHERE session = ?1",
            [session],
            |r| r.get(0),
        )
    }

    /// Removes `session`'s messages from `from` on.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn remove_messages_from(&self, session: &str, from: &str) -> Result<usize, Error> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM message_text WHERE rowid IN
                 (SELECT text_row FROM message WHERE session = ?1 AND id >= ?2)",
            [session, from],
        )?;
        conn.execute(
            "DELETE FROM message WHERE session = ?1 AND id >= ?2",
            [session, from],
        )
    }

    /// A session's messages, oldest first.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn messages(&self, session: &str) -> Result<Vec<Message>, Error> {
        let conn = self.reader();
        let mut stmt = conn.prepare("SELECT body FROM message WHERE session = ?1 ORDER BY id")?;
        stmt.query_map([session], |r| r.get::<_, String>(0))?
            .map(|b| serde_json::from_str(&b?).map_err(json_error))
            .collect()
    }

    /// A session's messages after the one with ID `past`, oldest first;
    /// all of them without one. IDs sort in a session's order.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn messages_after(&self, session: &str, past: Option<&str>) -> Result<Vec<Message>, Error> {
        let conn = self.reader();
        let mut stmt =
            conn.prepare("SELECT body FROM message WHERE session = ?1 AND id > ?2 ORDER BY id")?;
        stmt.query_map([session, past.unwrap_or_default()], |r| {
            r.get::<_, String>(0)
        })?
        .map(|b| serde_json::from_str(&b?).map_err(json_error))
        .collect()
    }

    /// Up to `limit` of a session's messages, and whether more lie beyond.
    ///
    /// Oldest first, from the start or from after the message `past`; with
    /// `newest_first`, newest first, from the end or from before `past`.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn message_page(
        &self,
        session: &str,
        newest_first: bool,
        past: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Message>, bool), Error> {
        // (Every ID sorts after the empty one.)
        let (side, from) = match past {
            Some(id) if newest_first => ("<", id),
            Some(id) => (">", id),
            None => (">", ""),
        };
        let order = if newest_first { "DESC" } else { "ASC" };
        // One more than asked for says whether the page is the last.
        let wanted = i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1);
        let conn = self.reader();
        let mut stmt = conn.prepare(&format!(
            "SELECT body FROM message WHERE session = ?1 AND id {side} ?2
             ORDER BY id {order} LIMIT ?3"
        ))?;
        let mut page = stmt
            .query_map(params![session, from, wanted], |r| r.get::<_, String>(0))?
            .map(|b| serde_json::from_str(&b?).map_err(json_error))
            .collect::<Result<Vec<Message>, Error>>()?;
        let more = page.len() > limit;
        page.truncate(limit);
        Ok((page, more))
    }

    /// The message with ID `id`, and the session it's in.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn message(&self, id: &str) -> Result<Option<(String, Message)>, Error> {
        let found: Option<(String, String)> = self
            .reader()
            .query_row(
                "SELECT session, body FROM message WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        found
            .map(|(session, body)| Ok((session, serde_json::from_str(&body).map_err(json_error)?)))
            .transpose()
    }

    /// A session's newest message, if it has any.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn last_message(&self, session: &str) -> Result<Option<Message>, Error> {
        let conn = self.reader();
        let mut stmt =
            conn.prepare("SELECT body FROM message WHERE session = ?1 ORDER BY id DESC LIMIT 1")?;
        let mut rows = stmt.query_map([session], |r| r.get::<_, String>(0))?;
        rows.next()
            .transpose()?
            .map(|b| serde_json::from_str(&b).map_err(json_error))
            .transpose()
    }

    /// Appends an event and returns its sequence number.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn append_event(&self, aggregate: &str, body: &Value) -> Result<u64, Error> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO event (aggregate, body) VALUES (?1, ?2)",
            params![aggregate, body.to_string()],
        )?;
        let seq = u64::try_from(conn.last_insert_rowid()).unwrap_or_default();
        drop(conn);
        if seq.is_multiple_of(PRUNE_EVERY) {
            self.prune_events()?;
        }
        Ok(seq)
    }

    /// Removes all but the newest events; how many went.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn prune_events(&self) -> Result<usize, Error> {
        self.conn().execute(
            "DELETE FROM event WHERE seq <= (SELECT coalesce(max(seq), 0) FROM event) - ?1",
            [KEPT_EVENTS],
        )
    }

    /// The events after sequence number `after`, in order.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn events_after(&self, after: u64) -> Result<Vec<StoredEvent>, Error> {
        let conn = self.reader();
        let mut stmt =
            conn.prepare("SELECT seq, aggregate, body FROM event WHERE seq > ?1 ORDER BY seq")?;
        stmt.query_map([i64::try_from(after).unwrap_or(i64::MAX)], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .map(|row| {
            let (seq, aggregate, body) = row?;
            Ok(StoredEvent {
                seq: u64::try_from(seq).unwrap_or_default(),
                aggregate,
                body: serde_json::from_str(&body).map_err(json_error)?,
            })
        })
        .collect()
    }
}

fn json_error(e: serde_json::Error) -> Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use harlo_engine::Created;
    use serde_json::json;

    #[cfg(unix)]
    #[test]
    fn the_file_is_its_owners_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("scratch");
        let path = dir.path().join("harlo.db");
        let mode = |p: &Path| std::fs::metadata(p).expect("file").permissions().mode() & 0o777;
        let store = Store::open(&path).expect("store");
        store.put_setting("k", "v").expect("written");
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&dir.path().join("harlo.db-wal")), 0o600);
        drop(store);
        // One made before is made owner-only.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        drop(Store::open(&path).expect("store"));
        assert_eq!(mode(&path), 0o600);
    }

    /// A session `id` of project "p".
    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            project_id: "p".into(),
            location: Location {
                directory: "/tmp/p".into(),
            },
            title: None,
            time: SessionTime {
                created: 1,
                updated: 1,
                busy: None,
                idle: None,
                archived: None,
            },
            cost: 0.0,
            tokens: Tokens::default(),
            outcome: None,
            revert: None,
            permissions: Vec::new(),
            model: None,
            agent: None,
            parent_id: None,
            fork: None,
        }
    }

    fn prompt(id: &str, text: &str) -> Message {
        Message::User {
            id: id.into(),
            time: Created { created: 0 },
            text: text.into(),
            skills: Vec::new(),
            files: Vec::new(),
        }
    }

    /// The messages search finds for `word`, and how many rows its index has.
    fn found(store: &Store, word: &str) -> (Vec<String>, i64) {
        let mut hits: Vec<String> = store
            .search(Some("p"), None, &[word.to_owned()], 10)
            .expect("search")
            .into_iter()
            .map(|hit| hit.message)
            .collect();
        hits.sort();
        let rows = store
            .conn()
            .query_row("SELECT count(*) FROM message_text", [], |r| r.get(0))
            .expect("count");
        (hits, rows)
    }

    #[test]
    fn the_search_index_follows_the_messages() {
        let store = Store::in_memory().expect("store");
        for id in ["ses_1", "ses_2"] {
            store.put_session(&session(id)).expect("session");
        }
        store
            .put_message("ses_1", &prompt("msg_1", "apple pie"))
            .expect("put");
        store
            .put_message("ses_1", &prompt("msg_2", "apple tart"))
            .expect("put");
        store
            .put_message("ses_2", &prompt("msg_3", "apple juice"))
            .expect("put");
        assert_eq!(
            found(&store, "apple"),
            (vec!["msg_1".into(), "msg_2".into(), "msg_3".into()], 3)
        );

        // A changed message is found by its new text only, once.
        store
            .put_message("ses_1", &prompt("msg_1", "cherry pie"))
            .expect("put");
        assert_eq!(found(&store, "cherry"), (vec!["msg_1".into()], 3));
        assert_eq!(found(&store, "apple").0, ["msg_2", "msg_3"]);
        // One left without text leaves the index, and comes back with text.
        store
            .put_message("ses_1", &prompt("msg_1", " "))
            .expect("put");
        assert_eq!(found(&store, "cherry"), (Vec::new(), 2));
        store
            .put_message("ses_1", &prompt("msg_1", "cherry cake"))
            .expect("put");
        assert_eq!(found(&store, "cherry"), (vec!["msg_1".into()], 3));

        // Messages removed leave it too; another session's stay.
        assert_eq!(
            store
                .remove_messages_from("ses_1", "msg_2")
                .expect("remove"),
            1
        );
        assert_eq!(found(&store, "apple"), (vec!["msg_3".into()], 2));
        store.delete_session("ses_1").expect("delete");
        assert_eq!(found(&store, "cherry"), (Vec::new(), 1));
        assert_eq!(found(&store, "apple").0, ["msg_3"]);
    }

    #[test]
    fn a_row_of_the_search_index_is_found_by_its_number() {
        let store = Store::in_memory().expect("store");
        let conn = store.conn();
        // By a row's number the index is looked up ("0:="); by its
        // message or session it is read from end to end ("0:").
        let plan = |sql: &str| -> String {
            conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .expect("plan")
                .query_map([], |r| r.get::<_, String>(3))
                .expect("rows")
                .map(|line| line.expect("line"))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let by_number = "DELETE FROM message_text WHERE rowid IN
            (SELECT text_row FROM message WHERE session = 'ses_1')";
        assert!(plan(by_number).contains("message_text VIRTUAL TABLE INDEX 0:=;"));
        assert!(plan("DELETE FROM message_text WHERE rowid = 1").ends_with("INDEX 0:="));
    }

    #[test]
    fn a_store_from_before_messages_knew_their_row_is_indexed_again() {
        let conn = Connection::open_in_memory().expect("database");
        conn.execute_batch(
            "CREATE TABLE message (
                 id TEXT PRIMARY KEY, session TEXT NOT NULL, body TEXT NOT NULL);
             CREATE VIRTUAL TABLE message_text
                 USING fts5(text, session UNINDEXED, message UNINDEXED);",
        )
        .expect("the earlier tables");
        for (id, text) in [("msg_1", "apple pie"), ("msg_2", "apple tart")] {
            let body = serde_json::to_string(&prompt(id, text)).expect("json");
            conn.execute(
                "INSERT INTO message (id, session, body) VALUES (?1, 'ses_1', ?2)",
                params![id, body],
            )
            .expect("message");
            conn.execute(
                "INSERT INTO message_text (text, session, message) VALUES (?1, 'ses_1', ?2)",
                params![text, id],
            )
            .expect("its text");
        }
        // (Left behind by a message that is gone.)
        conn.execute(
            "INSERT INTO message_text (text, session, message)
             VALUES ('apple crumble', 'ses_1', 'msg_0')",
            [],
        )
        .expect("a stale row");

        let store = Store::with(conn).expect("store");
        store.put_session(&session("ses_1")).expect("session");
        assert_eq!(
            found(&store, "apple"),
            (vec!["msg_1".into(), "msg_2".into()], 2)
        );
        store
            .put_message("ses_1", &prompt("msg_1", "cherry pie"))
            .expect("put");
        assert_eq!(found(&store, "apple"), (vec!["msg_2".into()], 2));
        store.delete_session("ses_1").expect("delete");
        assert_eq!(found(&store, "apple"), (Vec::new(), 0));
    }

    #[test]
    fn changes_to_a_session_made_at_once_all_hold() {
        let store = std::sync::Arc::new(Store::in_memory().expect("store"));
        store.put_session(&session("ses_a")).expect("session");
        // A run adding what its steps cost, while a title arrives.
        let adding: Vec<_> = (0..8)
            .map(|_| {
                let store = std::sync::Arc::clone(&store);
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        store
                            .change_session("ses_a", |s| s.tokens.input += 1)
                            .expect("changed");
                    }
                })
            })
            .collect();
        store
            .change_session("ses_a", |s| s.title = Some("Named".to_owned()))
            .expect("changed");
        for thread in adding {
            thread.join().expect("joined");
        }
        let kept = store.session("ses_a").expect("read").expect("there");
        assert_eq!(kept.tokens.input, 400);
        assert_eq!(kept.title.as_deref(), Some("Named"));
        assert!(
            store
                .change_session("ses_none", |_| ())
                .expect("read")
                .is_none()
        );
    }

    #[test]
    fn a_read_under_way_holds_up_no_write() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = std::sync::Arc::new(Store::open(&dir.path().join("store.db")).expect("store"));
        store.put_session(&session("ses_a")).expect("session");
        // A read that has begun and isn't over: its connection is taken.
        let reading = store.reader();
        let writer = std::sync::Arc::clone(&store);
        let (done, wrote) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let saved = writer.put_message("ses_a", &prompt("msg_1", "hello"));
            let _ = done.send(saved.is_ok());
        });
        let wrote = wrote.recv_timeout(std::time::Duration::from_secs(5));
        assert_eq!(wrote, Ok(true), "the write waited for the read");
        drop(reading);
        // What was written is there to read.
        assert_eq!(store.messages("ses_a").expect("read").len(), 1);
    }

    #[test]
    fn only_the_newest_events_are_kept() {
        let store = Store::in_memory().expect("store");
        let count = |store: &Store| -> i64 {
            store
                .conn()
                .query_row("SELECT count(*) FROM event", [], |r| r.get(0))
                .expect("count")
        };
        let extra = 2 * PRUNE_EVERY + 7;
        let total = u64::try_from(KEPT_EVENTS).expect("positive") + extra;
        let mut last = 0;
        for n in 0..total {
            let aggregate = if n % 2 == 0 { "ses_1" } else { "ses_2" };
            last = store
                .append_event(aggregate, &json!({ "n": n }))
                .expect("append");
        }
        assert_eq!(last, total, "numbers go on, never used twice");
        // Pruned every so often: at most that many more than are kept.
        let kept = count(&store);
        assert!(kept >= KEPT_EVENTS, "{kept}");
        assert!(
            kept < KEPT_EVENTS + i64::try_from(PRUNE_EVERY).expect("small"),
            "{kept}"
        );
        // What is left is the newest, without a gap.
        let after = store.events_after(0).expect("events");
        assert_eq!(after.last().map(|e| e.seq), Some(last));
        assert_eq!(
            u64::try_from(after.len()).expect("small"),
            last - after[0].seq + 1
        );
        assert_eq!(store.events_after(last - 1).expect("events").len(), 1);

        // A deleted session's events go with it; the other session's stay.
        let others = after.iter().filter(|e| e.aggregate == "ses_2").count();
        store.put_session(&session("ses_1")).expect("session");
        store.delete_session("ses_1").expect("delete");
        let left = store.events_after(0).expect("events");
        assert!(left.iter().all(|e| e.aggregate == "ses_2"));
        assert_eq!(left.len(), others);
    }

    #[test]
    fn sessions_messages_and_events_come_back_in_order() {
        let store = Store::in_memory().expect("store");
        let session = session("ses_1");
        store.put_session(&session).expect("put");
        assert_eq!(store.session("ses_1").expect("get"), Some(session));
        for (id, text) in [("msg_2", "second"), ("msg_1", "first")] {
            let m = Message::User {
                id: id.into(),
                time: Created { created: 0 },
                text: text.into(),
                skills: Vec::new(),
                files: Vec::new(),
            };
            store.put_message("ses_1", &m).expect("put");
        }
        let ids: Vec<_> = store
            .messages("ses_1")
            .expect("list")
            .iter()
            .map(|m| m.id().to_owned())
            .collect();
        assert_eq!(ids, ["msg_1", "msg_2"]);
        assert!(store.has_message("msg_1").expect("has"));
        let first = store
            .append_event("ses_1", &json!({ "type": "a" }))
            .expect("append");
        let second = store
            .append_event("ses_1", &json!({ "type": "b" }))
            .expect("append");
        assert!(second > first);
        let after = store.events_after(first).expect("after");
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].body["type"], "b");
    }
}
