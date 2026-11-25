//! The server's background jobs and the user's own shell commands.
//!
//! A job is work a session started that runs on without blocking it: a
//! shell command or a subagent sent to the background, a Code Mode script,
//! a picture or a video being made. They're experimental, and the API
//! may change between releases.
//!
//! A shell command the user runs in a session (`!command` in the
//! composer) runs in the session's folder with no model; its output
//! joins the conversation for the agent to read on its next turn.

// Rust guideline compliant 2026-10-09

use reqwest::Method;
use serde_json::{Value, json};

use crate::client::Client;
use crate::error::Error;

/// A background job a session started.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// The job's ID.
    pub id: String,
    /// The session that started it.
    pub session_id: String,
    /// What it is.
    pub kind: JobKind,
    /// Whether it still runs, and how it ended.
    pub status: JobStatus,
    /// Its title, when it has one.
    pub title: Option<String>,
    /// When it started, in ms since the Unix epoch.
    pub started: f64,
    /// When it ended, in ms since the Unix epoch.
    pub completed: Option<f64>,
}

/// What a background job is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobKind {
    /// A shell command.
    Shell {
        /// The command.
        command: String,
    },
    /// A subagent working in a child session.
    Subagent {
        /// The child session.
        child: String,
        /// The agent's ID, such as `explore`.
        agent: String,
        /// The task's short label.
        description: String,
    },
    /// A picture being made.
    Image {
        /// Where it's saved.
        path: String,
    },
    /// A video being made.
    Video {
        /// Where it's saved.
        path: String,
    },
    /// A Code Mode script.
    Script,
}

/// Where a background job is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    /// Still at work.
    Running,
    /// Finished.
    Completed,
    /// Failed.
    Error,
    /// Stopped before it finished.
    Cancelled,
}

impl Job {
    /// Reads a job from the server's JSON; `None` for a kind or status this
    /// version doesn't know.
    pub(crate) fn from_json(v: &Value) -> Option<Self> {
        let text = |key: &str| v[key].as_str().unwrap_or_default().to_owned();
        let kind = match v["kind"].as_str()? {
            "shell" => JobKind::Shell {
                command: text("command"),
            },
            "subagent" => JobKind::Subagent {
                child: text("childSessionID"),
                agent: text("agent"),
                description: text("description"),
            },
            "image" => JobKind::Image { path: text("path") },
            "video" => JobKind::Video { path: text("path") },
            "codemode" => JobKind::Script,
            _ => return None,
        };
        let status = match v["status"].as_str()? {
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "error" => JobStatus::Error,
            "cancelled" => JobStatus::Cancelled,
            _ => return None,
        };
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            session_id: v["sessionID"].as_str()?.to_owned(),
            kind,
            status,
            title: v["title"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .map(str::to_owned),
            started: v["time"]["started"].as_f64().unwrap_or_default(),
            completed: v["time"]["completed"].as_f64(),
        })
    }
}

impl Client {
    /// Lists the background jobs session `session_id` started, running or
    /// recently ended.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or doesn't know the session.
    pub async fn jobs(&self, session_id: &str) -> Result<Vec<Job>, Error> {
        let req = self
            .request(Method::GET, "/api/experimental/job")
            .query(&[("sessionID", session_id)]);
        let v = Self::json(req).await?;
        Ok(v["data"]
            .as_array()
            .map(|list| list.iter().filter_map(Job::from_json).collect())
            .unwrap_or_default())
    }

    /// Stops background job `id`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn cancel_job(&self, id: &str) -> Result<(), Error> {
        let path = format!("/api/experimental/job/{id}/cancel");
        Self::send(self.request(Method::POST, &path)).await?;
        Ok(())
    }

    /// Sends the command that session `session_id`'s tool call `call` is
    /// running to the background: the agent stops waiting for it, and
    /// gets its output when it ends.
    ///
    /// # Errors
    ///
    /// Fails when the call isn't a command still running, or the server
    /// can't be reached.
    pub async fn move_to_background(&self, session_id: &str, call: &str) -> Result<(), Error> {
        let path = format!("/api/session/{session_id}/tool/{call}/background");
        Self::send(self.request(Method::POST, &path)).await?;
        Ok(())
    }

    /// Runs `command` in session `session_id`'s folder with no model, and
    /// returns once it has ended. Its output joins the conversation.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    /// A command that fails still returns `Ok`; its exit code is in the
    /// session's [`Message::Shell`](crate::Message::Shell).
    pub async fn run_shell(&self, session_id: &str, command: &str) -> Result<(), Error> {
        let path = format!("/api/session/{session_id}/shell");
        Self::send(
            self.request(Method::POST, &path)
                .json(&json!({ "command": command })),
        )
        .await?;
        Ok(())
    }
}
