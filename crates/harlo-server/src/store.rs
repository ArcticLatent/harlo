//! What the server keeps on disk: sessions, their messages, and every
//! durable event in order.
//!
//! One SQLite file. Events carry a sequence number that only grows, so a
//! client that reconnects can name the last one it saw and get the rest.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

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
    /// Its own permission rules, applied after the agent's and the config's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<Rule>,
    /// The model it uses, when one was chosen for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
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
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS session (id TEXT PRIMARY KEY, info TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS message (
                 id TEXT PRIMARY KEY, session TEXT NOT NULL, body TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS message_by_session ON message (session, id);
             CREATE TABLE IF NOT EXISTS event (
                 seq INTEGER PRIMARY KEY AUTOINCREMENT, aggregate TEXT NOT NULL, body TEXT NOT NULL);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
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
            },
            cost: 0.0,
            tokens: Tokens::default(),
            outcome: None,
            permissions: Vec::new(),
            model: None,
        };
        store.put_session(&session).expect("put");
        assert_eq!(store.session("ses_1").expect("get"), Some(session));
        for (id, text) in [("msg_2", "second"), ("msg_1", "first")] {
            let m = Message::User {
                id: id.into(),
                time: Created { created: 0 },
                text: text.into(),
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
