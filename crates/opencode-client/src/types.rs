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

/// A project: a folder OpenCode works in, usually a git repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// Project ID.
    pub id: String,
    /// The project's main folder.
    pub directory: String,
    /// A name the user gave the project, if any.
    pub name: Option<String>,
    /// Whether the folder is under git.
    pub git: bool,
}

impl From<g::Project> for Project {
    fn from(v: g::Project) -> Self {
        Self {
            id: v.id.to_string(),
            directory: v.canonical.to_string(),
            name: v.name.filter(|n| !n.trim().is_empty()),
            git: v.vcs.is_some_and(|vcs| vcs.as_str() == "git"),
        }
    }
}

/// A model, by provider and model ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    /// Provider ID, such as `anthropic`.
    pub provider_id: String,
    /// Model ID within the provider.
    pub id: String,
    /// Reasoning level, such as `high`; `None` uses the model's default.
    pub variant: Option<String>,
}

impl From<g::ModelRef> for ModelRef {
    fn from(v: g::ModelRef) -> Self {
        Self {
            provider_id: v.provider_id.to_string(),
            id: v.id,
            // OpenCode reports "default" when no reasoning level was chosen.
            variant: v.variant.filter(|s| !s.is_empty() && s != "default"),
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
    /// The agent the session runs, such as `build`, once one is chosen.
    pub agent: Option<String>,
    /// The model the session uses, once one is chosen.
    pub model: Option<ModelRef>,
    /// Creation time, in milliseconds since the Unix epoch.
    pub created: f64,
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
            agent: v.agent.map(|a| a.to_string()),
            model: v.model.map(ModelRef::from),
            created: v.time.created,
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
        /// Names of the files and images sent with it.
        attachments: Vec<String>,
        /// Names of the skills loaded with it.
        skills: Vec<String>,
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
    /// The conversation was summarized to free up context.
    Compaction {
        /// Message ID (`msg_…`).
        id: String,
        /// `running`, `completed` or `failed`.
        status: String,
        /// The summary, once written.
        summary: Option<String>,
        /// Why it failed, when it did.
        error: Option<String>,
    },
    /// A skill was loaded into the session.
    Skill {
        /// Message ID (`msg_…`).
        id: String,
        /// The skill's name.
        name: String,
    },
    /// A message kind Harlo doesn't show yet (agent switch, shell, …).
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
            | Self::Compaction { id, .. }
            | Self::Skill { id, .. }
            | Self::Other { id, .. } => id,
        }
    }

    pub(crate) fn from_json(value: Value) -> Result<Self, serde_json::Error> {
        let id = value["id"].as_str().unwrap_or_default().to_owned();
        let kind = value["type"].as_str().unwrap_or_default().to_owned();
        Ok(match kind.as_str() {
            "user" => {
                let attachments = value["files"]
                    .as_array()
                    .map(|files| {
                        files
                            .iter()
                            .map(|f| attachment_name(f).to_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                let skills = skill_names(&value["skills"]);
                let m: g::SessionMessageUser = serde_json::from_value(value)?;
                Self::User {
                    id,
                    text: m.text,
                    attachments,
                    skills,
                }
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
            "compaction" => Self::Compaction {
                id,
                status: value["status"].as_str().unwrap_or("running").to_owned(),
                summary: value["summary"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_owned),
                error: value["error"]["message"].as_str().map(str::to_owned),
            },
            "skill" => Self::Skill {
                id,
                name: value["name"].as_str().unwrap_or_default().to_owned(),
            },
            _ => Self::Other { id, kind },
        })
    }
}

/// The name an attachment shows: its name, or the last part of its URI.
pub(crate) fn attachment_name(file: &Value) -> &str {
    file["name"]
        .as_str()
        .or_else(|| file["source"]["uri"].as_str())
        .or_else(|| file["uri"].as_str())
        .map_or("attachment", |name| name.rsplit('/').next().unwrap_or(name))
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
                let took_ms = t.time.completed.map(|end| (end - t.time.created).max(0.0));
                Self::Tool(ToolCall {
                    id: t.id,
                    name: t.name,
                    status,
                    input,
                    took_ms,
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
    /// How long the call took, in milliseconds, once it finished.
    pub took_ms: Option<f64>,
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
    /// Names of the files and images sent with a queued prompt.
    pub attachments: Vec<String>,
    /// Names of the skills loaded with a queued prompt.
    pub skills: Vec<String>,
    /// How it will be delivered.
    pub delivery: Delivery,
}

/// Names of the skills in a list of `{id, name?}` objects.
fn skill_names(list: &Value) -> Vec<String> {
    list.as_array()
        .map(|skills| {
            skills
                .iter()
                .filter_map(|s| s["name"].as_str().or_else(|| s["id"].as_str()))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

impl InboxItem {
    pub(crate) fn from_json(value: &Value) -> Self {
        Self {
            id: value["id"].as_str().unwrap_or_default().to_owned(),
            kind: value["type"].as_str().unwrap_or_default().to_owned(),
            text: value["payload"]["text"].as_str().map(str::to_owned),
            attachments: value["payload"]["files"]
                .as_array()
                .map(|files| {
                    files
                        .iter()
                        .map(|f| attachment_name(f).to_owned())
                        .collect()
                })
                .unwrap_or_default(),
            skills: skill_names(&value["payload"]["skills"]),
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
