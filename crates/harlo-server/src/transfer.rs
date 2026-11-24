//! Sessions copied, moved, archived, deleted, exported and imported.

// Rust guideline compliant 2026-10-09

use harlo_engine::{Message, ascending_id, descending_id, now};
use serde_json::{Value, json};

use crate::git;
use crate::server::{Refusal, Server, lock};
use crate::store::{Location, Session};

/// `message` with a new ID, the same otherwise.
fn renumbered(message: &Message) -> Option<Message> {
    let mut v = serde_json::to_value(message).ok()?;
    v["id"] = json!(ascending_id("msg"));
    serde_json::from_value(v).ok()
}

impl Server {
    /// Copies session `id` into a new session: its whole history, or what
    /// came before message `before`. The original stays as it was.
    ///
    /// # Errors
    ///
    /// When there's no such session or message, or the store fails.
    pub(crate) fn fork_session(&self, id: &str, before: Option<&str>) -> Result<Session, Refusal> {
        let from = self.session(id)?;
        let history = self.store.messages(id)?;
        let (kept, boundary) = match before {
            Some(m) => {
                let at = history
                    .iter()
                    .position(|h| h.id() == m)
                    .ok_or_else(|| Refusal::NotFound(format!("There's no message {m}.")))?;
                (
                    &history[..at],
                    Some(json!({ "type": "before", "messageID": m })),
                )
            }
            None => (
                history.as_slice(),
                history
                    .last()
                    .map(|h| json!({ "type": "through", "messageID": h.id() })),
            ),
        };
        let forks = self
            .store
            .sessions()?
            .iter()
            .filter(|s| s.fork.as_ref().is_some_and(|f| f["sessionID"] == id))
            .count();
        let created = now();
        let mut session = from.clone();
        session.id = descending_id("ses");
        session.title = from
            .title
            .as_ref()
            .map(|t| format!("{t} (fork #{})", forks + 1));
        session.time = crate::store::SessionTime {
            created,
            updated: created,
            ..crate::store::SessionTime::default()
        };
        session.cost = 0.0;
        session.tokens = harlo_engine::Tokens::default();
        session.outcome = None;
        session.revert = None;
        session.fork = boundary
            .clone()
            .map(|b| json!({ "sessionID": id, "boundary": b }));
        self.store.put_session(&session)?;
        for message in kept {
            if let Some(copy) = renumbered(message) {
                self.store.put_message(&session.id, &copy)?;
            }
        }
        self.created(&session);
        self.emit(
            "session.forked",
            json!({ "sessionID": session.id, "parentID": id, "boundary": boundary }),
            Some(&session.location.directory),
            Some(&session.id),
        );
        Ok(session)
    }

    /// Announces a session that was just made.
    fn created(&self, session: &Session) {
        self.emit(
            "session.created",
            json!({
                "sessionID": session.id, "slug": session.id, "version": env!("CARGO_PKG_VERSION"),
                "projectID": session.project_id, "location": session.location, "subpath": "",
            }),
            Some(&session.location.directory),
            Some(&session.id),
        );
    }

    /// Moves session `id` to folder `directory`, such as one of its
    /// project's worktrees; its history goes along.
    ///
    /// # Errors
    ///
    /// When there's no such session or folder, or the store fails.
    pub(crate) async fn move_session(&self, id: &str, directory: &str) -> Result<(), Refusal> {
        if !std::path::Path::new(directory).is_dir() {
            return Err(Refusal::Bad(format!("{directory} isn't a folder.")));
        }
        let mut session = self.session(id)?;
        session.project_id = git::project_id(directory).await;
        session.location = Location {
            directory: directory.to_owned(),
        };
        session.time.updated = now();
        self.store.put_session(&session)?;
        self.forget_files(id);
        self.emit(
            "session.moved",
            json!({
                "sessionID": id, "location": session.location,
                "projectID": session.project_id, "subpath": "",
            }),
            Some(directory),
            Some(id),
        );
        Ok(())
    }

    /// Archives session `id`, or brings it back: archived sessions are
    /// left out of the list.
    ///
    /// # Errors
    ///
    /// When there's no such session, or the store fails.
    pub(crate) fn archive_session(&self, id: &str, archived: bool) -> Result<(), Refusal> {
        let mut session = self.session(id)?;
        session.time.archived = archived.then(now);
        session.time.updated = now();
        self.store.put_session(&session)?;
        let kind = if archived {
            "session.archived"
        } else {
            "session.unarchived"
        };
        self.emit(
            kind,
            json!({ "sessionID": id }),
            Some(&session.location.directory),
            Some(id),
        );
        Ok(())
    }

    /// Deletes session `id` for good, with the sessions it handed work
    /// to; a run going is stopped first.
    ///
    /// # Errors
    ///
    /// When there's no such session, or the store fails.
    pub(crate) fn delete_session(&self, id: &str) -> Result<(), Refusal> {
        let session = self.session(id)?;
        for child in self.store.sessions()? {
            if child.parent_id.as_deref() == Some(id) {
                self.delete_session(&child.id)?;
            }
        }
        self.interrupt(id);
        lock(&self.live).inbox.remove(id);
        self.forget_files(id);
        self.store.delete_session(id)?;
        self.emit(
            "session.deleted",
            json!({ "sessionID": id }),
            Some(&session.location.directory),
            Some(id),
        );
        Ok(())
    }

    /// Session `id` and its whole history, as one document.
    ///
    /// # Errors
    ///
    /// When there's no such session, or the store fails.
    pub(crate) fn export_session(&self, id: &str) -> Result<Value, Refusal> {
        let session = self.session(id)?;
        let messages = self.store.messages(id)?;
        Ok(json!({ "info": session, "messages": messages }))
    }

    /// Brings back an exported session, as it was, in folder `directory`.
    ///
    /// # Errors
    ///
    /// When the server already has it, the document isn't an exported
    /// session, or the store fails.
    pub(crate) async fn import_session(
        &self,
        info: Value,
        messages: Vec<Value>,
        directory: &str,
    ) -> Result<Session, Refusal> {
        let mut session: Session = serde_json::from_value(info)
            .map_err(|e| Refusal::Bad(format!("That isn't an exported session: {e}")))?;
        if self.store.session(&session.id)?.is_some() {
            return Err(Refusal::Exists(format!(
                "Session already exists: {}",
                session.id
            )));
        }
        let messages = messages
            .into_iter()
            .map(serde_json::from_value::<Message>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Refusal::Bad(format!("A message didn't read: {e}")))?;
        session.location = Location {
            directory: directory.to_owned(),
        };
        session.project_id = git::project_id(directory).await;
        session.time.updated = now();
        self.store.put_session(&session)?;
        for message in &messages {
            self.store.put_message(&session.id, message)?;
        }
        self.created(&session);
        Ok(session)
    }
}
