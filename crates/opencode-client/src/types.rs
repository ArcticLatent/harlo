//! The client's own view of OpenCode's data: sessions, messages, approvals, questions.
//!
//! These types hold what Harlo uses and nothing more. They are built from the
//! generated schema types, which stay private to this crate, so a schema change
//! shows up here as a compile error instead of rippling into the app.

// Rust guideline compliant 2026-10-09

use serde_json::Value;

use crate::generated as g;

/// The server's version and addresses, from `GET /api/info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// OpenCode version, such as `2.3.2`.
    pub version: String,
    /// Addresses the server listens on.
    pub urls: Vec<String>,
}

impl From<g::ServerInfo> for ServerInfo {
    fn from(v: g::ServerInfo) -> Self {
        Self {
            version: v.version,
            urls: v.urls,
        }
    }
}

/// A session as listed by the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// Session ID (`ses_…`).
    pub id: String,
    /// Title, once the server has named the session.
    pub title: Option<String>,
    /// Folder the session works in.
    pub directory: String,
    /// Project the folder belongs to.
    pub project_id: String,
    /// Parent session, for subagent sessions.
    pub parent_id: Option<String>,
    /// Last change, in milliseconds since the Unix epoch.
    pub updated: f64,
    /// A staged undo, when one is waiting to be kept or cancelled.
    pub revert: Option<Revert>,
}

impl From<g::SessionInfo> for Session {
    fn from(v: g::SessionInfo) -> Self {
        Self {
            id: v.id.to_string(),
            title: v.title,
            directory: v.location.directory.to_string(),
            project_id: v.project_id.to_string(),
            parent_id: v.parent_id.map(|p| p.to_string()),
            updated: v.time.updated,
            revert: v.revert.map(Revert::from),
        }
    }
}

/// One entry in a session's timeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Something the user sent.
    User {
        /// Message ID (`msg_…`).
        id: String,
        /// The prompt text.
        text: String,
    },
    /// One model step: text, reasoning and tool calls.
    Assistant {
        /// Message ID (`msg_…`).
        id: String,
        /// The parts, in order.
        parts: Vec<Part>,
        /// Files this step changed, relative to the session folder.
        files: Vec<String>,
    },
    /// The end of a run.
    Idle {
        /// Message ID (`msg_…`).
        id: String,
        /// `succeeded`, `failed` or `interrupted`.
        outcome: String,
    },
    /// A message kind Harlo doesn't show yet (agent switch, compaction, …).
    Other {
        /// Message ID (`msg_…`).
        id: String,
        /// OpenCode's message type.
        kind: String,
    },
}

impl Message {
    /// Returns the message ID.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. }
            | Self::Assistant { id, .. }
            | Self::Idle { id, .. }
            | Self::Other { id, .. } => id,
        }
    }

    pub(crate) fn from_json(value: Value) -> Result<Self, serde_json::Error> {
        let id = value["id"].as_str().unwrap_or_default().to_owned();
        let kind = value["type"].as_str().unwrap_or_default().to_owned();
        Ok(match kind.as_str() {
            "user" => {
                let m: g::SessionMessageUser = serde_json::from_value(value)?;
                Self::User { id, text: m.text }
            }
            "assistant" => {
                let m: g::SessionMessageAssistant = serde_json::from_value(value)?;
                Self::Assistant {
                    id,
                    parts: m.content.into_iter().map(Part::from).collect(),
                    files: m
                        .snapshot
                        .map(|s| s.files.into_iter().map(|f| f.to_string()).collect())
                        .unwrap_or_default(),
                }
            }
            "idle" => {
                let m: g::SessionMessageIdle = serde_json::from_value(value)?;
                Self::Idle {
                    id,
                    outcome: m.outcome.to_string(),
                }
            }
            _ => Self::Other { id, kind },
        })
    }
}

/// A piece of an assistant message.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// Answer text (Markdown).
    Text(String),
    /// The model's reasoning.
    Reasoning(String),
    /// A tool call.
    Tool(ToolCall),
}

impl From<g::SessionMessageAssistantContentItem> for Part {
    fn from(v: g::SessionMessageAssistantContentItem) -> Self {
        match v {
            g::SessionMessageAssistantContentItem::Text(t) => Self::Text(t.text),
            g::SessionMessageAssistantContentItem::Reasoning(r) => Self::Reasoning(r.text),
            g::SessionMessageAssistantContentItem::Tool(t) => {
                let (status, input) = match t.state {
                    g::SessionMessageAssistantToolState::Streaming(_) => {
                        (ToolStatus::Streaming, Value::Null)
                    }
                    g::SessionMessageAssistantToolState::Running(s) => {
                        (ToolStatus::Running, Value::Object(s.input))
                    }
                    g::SessionMessageAssistantToolState::Completed(s) => {
                        (ToolStatus::Completed, Value::Object(s.input))
                    }
                    g::SessionMessageAssistantToolState::Error(s) => {
                        (ToolStatus::Failed, Value::Object(s.input))
                    }
                };
                Self::Tool(ToolCall {
                    id: t.id,
                    name: t.name,
                    status,
                    input,
                })
            }
        }
    }
}

/// A tool call inside an assistant message.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// Call ID, as the model named it.
    pub id: String,
    /// Tool name, such as `edit` or `shell`.
    pub name: String,
    /// Where the call is.
    pub status: ToolStatus,
    /// The call's arguments.
    pub input: Value,
}

/// Where a tool call is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    /// The model is still writing its arguments.
    Streaming,
    /// The tool is running.
    Running,
    /// The tool finished.
    Completed,
    /// The tool failed.
    Failed,
}

/// A pending request to approve an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    /// Request ID (`per_…`).
    pub id: String,
    /// Session that asked.
    pub session_id: String,
    /// What it wants to do, such as `shell` or `edit`.
    pub action: String,
    /// What it acts on: commands, paths, …
    pub resources: Vec<String>,
    /// Patterns "Allow always" would save.
    pub save: Vec<String>,
}

impl From<g::PermissionRequest> for Permission {
    fn from(v: g::PermissionRequest) -> Self {
        Self {
            id: v.id.to_string(),
            session_id: v.session_id.to_string(),
            action: v.action,
            resources: v.resources,
            save: v.save,
        }
    }
}

impl From<g::PermissionAskedData> for Permission {
    fn from(v: g::PermissionAskedData) -> Self {
        Self {
            id: v.id.to_string(),
            session_id: v.session_id.to_string(),
            action: v.action,
            resources: v.resources,
            save: v.save,
        }
    }
}

/// An answer to a [`Permission`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionReply {
    /// Allow this request only.
    Once,
    /// Allow it and save the proposed patterns for the project.
    Always,
    /// Reject it, and every other pending request in the session.
    Reject,
}

impl PermissionReply {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Always => "always",
            Self::Reject => "reject",
        }
    }
}

/// A pending question from the agent, shown as a form.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    /// Form ID (`frm_…`).
    pub id: String,
    /// Session that asked.
    pub session_id: String,
    /// Form title.
    pub title: String,
    /// The questions, as OpenCode sent them (keys, titles, options).
    pub fields: Vec<Value>,
}

impl Form {
    pub(crate) fn from_json(value: &Value) -> Self {
        Self {
            id: value["id"].as_str().unwrap_or_default().to_owned(),
            session_id: value["sessionID"].as_str().unwrap_or_default().to_owned(),
            title: value["title"].as_str().unwrap_or_default().to_owned(),
            fields: value["fields"].as_array().cloned().unwrap_or_default(),
        }
    }
}

/// How a prompt reaches a busy session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Into the current turn, at the next step.
    Steer,
    /// After the current turn ends.
    Queue,
}

impl Delivery {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Steer => "steer",
            Self::Queue => "queue",
        }
    }
}

/// Work waiting in a session's inbox: a queued prompt, a compaction, …
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxItem {
    /// Item ID (the future message ID).
    pub id: String,
    /// `user`, `synthetic`, `compaction` or `move`.
    pub kind: String,
    /// The prompt text, for a queued prompt.
    pub text: Option<String>,
    /// How it will be delivered.
    pub delivery: Delivery,
}

impl InboxItem {
    pub(crate) fn from_json(value: &Value) -> Self {
        Self {
            id: value["id"].as_str().unwrap_or_default().to_owned(),
            kind: value["type"].as_str().unwrap_or_default().to_owned(),
            text: value["payload"]["text"].as_str().map(str::to_owned),
            delivery: if value["delivery"] == "queue" {
                Delivery::Queue
            } else {
                Delivery::Steer
            },
        }
    }
}

/// One file's change, as a unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// Path relative to the session folder.
    pub file: String,
    /// Unified diff text.
    pub patch: String,
    /// Lines added.
    pub additions: u64,
    /// Lines removed.
    pub deletions: u64,
    /// `added`, `deleted` or `modified`.
    pub status: String,
}

impl From<g::FileDiffInfo> for FileDiff {
    fn from(v: g::FileDiffInfo) -> Self {
        Self {
            file: v.file,
            patch: v.patch,
            additions: v.additions.try_into().unwrap_or_default(),
            deletions: v.deletions.try_into().unwrap_or_default(),
            status: v.status.to_string(),
        }
    }
}

/// A staged undo: the boundary it returns to and the file changes it makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revert {
    /// The first message the undo removes.
    pub message_id: String,
    /// What restoring the files changes, relative to the files right now.
    ///
    /// Empty when the undo was staged without applying files.
    pub files: Vec<FileDiff>,
}

impl From<g::SessionRevert> for Revert {
    fn from(v: g::SessionRevert) -> Self {
        Self {
            message_id: v.message_id.to_string(),
            files: v.files.into_iter().map(FileDiff::from).collect(),
        }
    }
}
