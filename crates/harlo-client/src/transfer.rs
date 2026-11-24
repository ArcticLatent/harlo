//! Copying, moving, deleting and carrying sessions: forks, moves to another
//! folder of the project (such as a worktree), deletion, and export to a
//! file that another OpenCode server can import.

// Rust guideline compliant 2026-10-09

use reqwest::Method;
use serde_json::{Value, json};

use crate::client::Client;
use crate::error::Error;
use crate::types::Session;
use crate::wire as g;

impl Client {
    /// Starts a new session with a copy of session `session_id`'s history:
    /// all of it, or what came before message `before`.
    ///
    /// # Errors
    ///
    /// Fails when the session or the message doesn't exist, or the server
    /// can't be reached.
    pub async fn fork_session(
        &self,
        session_id: &str,
        before: Option<&str>,
    ) -> Result<Session, Error> {
        let path = format!("/api/session/{session_id}/fork");
        let body = json!({ "before": before });
        let v = Self::json(self.request(Method::POST, &path).json(&body)).await?;
        Ok(serde_json::from_value::<g::SessionInfo>(v["data"].clone())?.into())
    }

    /// Moves session `session_id` to `directory`, another folder of its
    /// project such as one of its worktrees. A working session moves once
    /// its current step ends.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist, the folder isn't the
    /// project's, or the server can't be reached.
    pub async fn move_session(&self, session_id: &str, directory: &str) -> Result<(), Error> {
        let path = format!("/api/session/{session_id}/move");
        let body = json!({ "directory": directory });
        Self::send(self.request(Method::POST, &path).json(&body)).await?;
        Ok(())
    }

    /// Deletes session `session_id` and its subagents' sessions, for good.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), Error> {
        let path = format!("/api/session/{session_id}");
        Self::send(self.request(Method::DELETE, &path)).await?;
        Ok(())
    }

    /// Exports session `session_id` whole, as OpenCode's transcript in JSON: its
    /// details and every finished message, ready for [`import_session`].
    ///
    /// [`import_session`]: Client::import_session
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn export_session(&self, session_id: &str) -> Result<String, Error> {
        let path = format!("/api/experimental/session/{session_id}/export");
        let v = Self::json(self.request(Method::GET, &path)).await?;
        Ok(serde_json::to_string_pretty(&v["data"])?)
    }

    /// Imports a session exported as JSON into `directory`, keeping its ID.
    ///
    /// # Errors
    ///
    /// Fails when `transcript` isn't an exported session, when the server
    /// already has a session with its ID (such as the one it was exported
    /// from), or when the server can't be reached.
    pub async fn import_session(
        &self,
        transcript: &str,
        directory: &str,
    ) -> Result<Session, Error> {
        let transcript: Value = serde_json::from_str(transcript)?;
        let body = json!({
            "info": transcript["info"],
            "messages": transcript["messages"],
            "location": { "directory": directory },
        });
        let req = self
            .request(Method::POST, "/api/experimental/session/import")
            .json(&body);
        let v = Self::json(req).await?;
        Ok(serde_json::from_value::<g::SessionInfo>(v["data"].clone())?.into())
    }
}
