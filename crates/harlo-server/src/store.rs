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
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A failure to read or write the store.
pub type Error = rusqlite::Error;

/// A session, in OpenCode 2's shape.
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
    /// The undo staged in it, as OpenCode shapes it: `messageID`, the
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
#[derive(Debug)]
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Opens (or creates) the store at `path`.
    ///
    /// # Errors
    ///
    /// Fails when the file can't be opened or its tables made.
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::with(Connection::open(path)?)
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
        let indexed: bool = conn.query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE name = 'message_text'",
            [],
            |r| r.get(0),
        )?;
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
             CREATE TABLE IF NOT EXISTS setting (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS project (id TEXT PRIMARY KEY, settings TEXT NOT NULL);
             CREATE VIRTUAL TABLE IF NOT EXISTS message_text
                 USING fts5(text, session UNINDEXED, message UNINDEXED);",
        )?;
        Self::make_approvals(&conn)?;
        Self::make_automations(&conn)?;
        Self::make_credentials(&conn)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        if !indexed {
            store.index_all()?;
        }
        Ok(store)
    }

    /// Indexes every stored message's text for search (a store made
    /// before search existed).
    fn index_all(&self) -> Result<(), Error> {
        let bodies: Vec<(String, String)> = {
            let conn = self.conn();
            let mut stmt = conn.prepare("SELECT session, body FROM message")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        for (session, body) in bodies {
            if let Ok(message) = serde_json::from_str::<Message>(&body) {
                self.index(session.as_str(), &message)?;
            }
        }
        Ok(())
    }

    /// Indexes `message`'s text (a prompt, or what a step said) for search.
    fn index(&self, session: &str, message: &Message) -> Result<(), Error> {
        let text = match message {
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
            Message::Synthetic { .. } | Message::Idle { .. } | Message::Compaction(_) => {
                String::new()
            }
        };
        let conn = self.conn();
        conn.execute(
            "DELETE FROM message_text WHERE message = ?1",
            [message.id()],
        )?;
        if !text.trim().is_empty() {
            conn.execute(
                "INSERT INTO message_text (text, session, message) VALUES (?1, ?2, ?3)",
                params![text, session, message.id()],
            )?;
        }
        Ok(())
    }

    /// The messages of `project`'s sessions whose text has every word of
    /// `words`, best first, at most `limit`.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn search(&self, project: &str, words: &[String], limit: usize) -> Result<Vec<Hit>, Error> {
        // Each word quoted, so punctuation can't make a query of it.
        let query = words
            .iter()
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ");
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT t.session, t.message, snippet(message_text, 0, '[', ']', '…', 16),
                    json_extract(s.info, '$.title'), json_extract(s.info, '$.time.created')
             FROM message_text t JOIN session s ON s.id = t.session
             WHERE message_text MATCH ?1 AND json_extract(s.info, '$.projectID') = ?2
             ORDER BY rank LIMIT ?3",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        stmt.query_map(params![query, project, limit], |r| {
            Ok(Hit {
                session: r.get(0)?,
                message: r.get(1)?,
                snippet: r.get(2)?,
                title: r.get(3)?,
                created: r.get(4)?,
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
            .conn()
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
        let conn = self.conn();
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
        conn.execute("DELETE FROM message_text WHERE session = ?1", [id])?;
        conn.execute("DELETE FROM message WHERE session = ?1", [id])?;
        conn.execute("DELETE FROM session WHERE id = ?1", [id])?;
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
        self.conn().execute(
            "INSERT INTO message (id, session, body) VALUES (?1, ?2, ?3)
             ON CONFLICT (id) DO UPDATE SET body = excluded.body",
            params![message.id(), session, body],
        )?;
        self.index(session, message)
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
            "DELETE FROM message_text WHERE session = ?1 AND message >= ?2",
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
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT body FROM message WHERE session = ?1 ORDER BY id")?;
        stmt.query_map([session], |r| r.get::<_, String>(0))?
            .map(|b| serde_json::from_str(&b?).map_err(json_error))
            .collect()
    }

    /// A session's newest message, if it has any.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn last_message(&self, session: &str) -> Result<Option<Message>, Error> {
        let conn = self.conn();
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
        Ok(u64::try_from(conn.last_insert_rowid()).unwrap_or_default())
    }

    /// The events after sequence number `after`, in order.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn events_after(&self, after: u64) -> Result<Vec<StoredEvent>, Error> {
        let conn = self.conn();
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

    #[test]
    fn sessions_messages_and_events_come_back_in_order() {
        let store = Store::in_memory().expect("store");
        let session = Session {
            id: "ses_1".into(),
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
        };
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
