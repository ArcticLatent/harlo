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
    /// The session's own permission rules, applied after its agent's.
    pub permissions: Vec<Rule>,
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
            permissions: v
                .permissions
                .map(|rules| rules.0.into_iter().map(Rule::from).collect())
                .unwrap_or_default(),
        }
    }
}

/// What a permission rule does with the actions it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Do it without asking.
    Allow,
    /// Refuse it.
    Deny,
    /// Ask the user first.
    Ask,
}

impl Effect {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Ask => "ask",
        }
    }
}

/// A permission rule: what `effect` an `action` on a `resource` gets.
///
/// Both may be glob patterns, such as `*`. Where several rules match, the
/// last one wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The action, such as `shell`, `edit` or `*`.
    pub action: String,
    /// What it acts on, such as a command or a path pattern.
    pub resource: String,
    /// What happens when it matches.
    pub effect: Effect,
}

impl Rule {
    /// A rule giving `effect` to `action` on any resource.
    #[must_use]
    pub fn any(action: &str, effect: Effect) -> Self {
        Self {
            action: action.to_owned(),
            resource: "*".to_owned(),
            effect,
        }
    }

    pub(crate) fn json(&self) -> Value {
        serde_json::json!({
            "action": self.action,
            "resource": self.resource,
            "effect": self.effect.as_str(),
        })
    }
}

impl From<g::PermissionRule> for Rule {
    fn from(v: g::PermissionRule) -> Self {
        Self {
            action: v.action,
            resource: v.resource,
            effect: match v.effect {
                g::PermissionEffect::Allow => Effect::Allow,
                g::PermissionEffect::Deny => Effect::Deny,
                g::PermissionEffect::Ask => Effect::Ask,
            },
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
                let (status, input, files) = match t.state {
                    g::SessionMessageAssistantToolState::Streaming(_) => {
                        (ToolStatus::Streaming, Value::Null, Vec::new())
                    }
                    g::SessionMessageAssistantToolState::Running(s) => {
                        (ToolStatus::Running, Value::Object(s.input), Vec::new())
                    }
                    g::SessionMessageAssistantToolState::Completed(s) => {
                        let files = proposed_files(&s.metadata);
                        (ToolStatus::Completed, Value::Object(s.input), files)
                    }
                    g::SessionMessageAssistantToolState::Error(s) => {
                        (ToolStatus::Failed, Value::Object(s.input), Vec::new())
                    }
                };
                let took_ms = t.time.completed.map(|end| (end - t.time.created).max(0.0));
                Self::Tool(ToolCall {
                    id: t.id,
                    name: t.name,
                    status,
                    input,
                    took_ms,
                    files,
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
    /// The files it changed, such as an edit's, once it finished.
    pub files: Vec<FileDiff>,
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
    /// Why it's asking, when the tool says.
    pub message: Option<String>,
    /// The changes an edit would make, one per file.
    pub files: Vec<FileDiff>,
}

/// Reads the file changes an edit request carries in its metadata.
pub(crate) fn proposed_files(metadata: &serde_json::Map<String, Value>) -> Vec<FileDiff> {
    let Some(files) = metadata.get("files").and_then(Value::as_array) else {
        return Vec::new();
    };
    files
        .iter()
        .filter_map(|f| {
            Some(FileDiff {
                file: f["file"].as_str()?.to_owned(),
                patch: f["patch"].as_str().unwrap_or_default().to_owned(),
                additions: f["additions"].as_u64().unwrap_or_default(),
                deletions: f["deletions"].as_u64().unwrap_or_default(),
                status: f["status"].as_str().unwrap_or("modified").to_owned(),
            })
        })
        .collect()
}

impl From<g::PermissionRequest> for Permission {
    fn from(v: g::PermissionRequest) -> Self {
        Self {
            id: v.id.to_string(),
            session_id: v.session_id.to_string(),
            files: proposed_files(&v.metadata),
            action: v.action,
            resources: v.resources,
            save: v.save,
            message: v.message,
        }
    }
}

impl From<g::PermissionAskedData> for Permission {
    fn from(v: g::PermissionAskedData) -> Self {
        Self {
            id: v.id.to_string(),
            session_id: v.session_id.to_string(),
            files: proposed_files(&v.metadata),
            action: v.action,
            resources: v.resources,
            save: v.save,
            message: v.message,
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
    /// The questions, in order. Fields of a kind this client doesn't know are left out.
    pub fields: Vec<Field>,
}

impl Form {
    pub(crate) fn from_json(value: &Value) -> Self {
        Self {
            id: value["id"].as_str().unwrap_or_default().to_owned(),
            session_id: value["sessionID"].as_str().unwrap_or_default().to_owned(),
            title: value["title"].as_str().unwrap_or_default().to_owned(),
            fields: value["fields"]
                .as_array()
                .map(|a| a.iter().filter_map(Field::from_json).collect())
                .unwrap_or_default(),
        }
    }
}

/// One question in a [`Form`].
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The key its answer is sent under.
    pub key: String,
    /// A short heading.
    pub title: Option<String>,
    /// The question itself, or more about it.
    pub description: Option<String>,
    /// Whether it must be answered.
    pub required: bool,
    /// Whether it's kept out of sight; its default is sent as is.
    pub hidden: bool,
    /// It's asked only when all of these hold.
    pub when: Vec<Condition>,
    /// What kind of answer it takes.
    pub kind: FieldKind,
}

/// The kind of answer a [`Field`] takes.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    /// Text, or one of `options`; with `custom`, other text too.
    Text {
        /// Choices to pick from; none means free text.
        options: Vec<Choice>,
        /// Whether text other than the options is accepted.
        custom: bool,
        /// Shown in an empty text box.
        placeholder: Option<String>,
        /// The answer given when the user doesn't change it.
        default: Option<String>,
    },
    /// A number, whole when `integer`.
    Number {
        /// Whether only whole numbers are accepted.
        integer: bool,
        /// The smallest accepted value.
        minimum: Option<f64>,
        /// The largest accepted value.
        maximum: Option<f64>,
        /// The answer given when the user doesn't change it.
        default: Option<f64>,
    },
    /// Yes or no.
    Boolean {
        /// The answer given when the user doesn't change it.
        default: Option<bool>,
    },
    /// Any number of `options`; with `custom`, other text too.
    Multiselect {
        /// Choices to pick from.
        options: Vec<Choice>,
        /// Whether text other than the options is accepted.
        custom: bool,
        /// The fewest choices accepted.
        min_items: Option<u64>,
        /// The most choices accepted.
        max_items: Option<u64>,
        /// The choices made when the user doesn't change them.
        default: Vec<String>,
    },
    /// Something to do elsewhere, such as signing in, at `url`.
    External {
        /// Where to go.
        url: String,
    },
}

/// One choice offered by a [`Field`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// The answer sent when it's chosen.
    pub value: String,
    /// The name shown.
    pub label: String,
    /// More about it.
    pub description: Option<String>,
}

/// A condition on another field's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// The other field's key.
    pub key: String,
    /// Whether the answer must equal `value` (or differ from it).
    pub equal: bool,
    /// The answer compared against.
    pub value: Answer,
}

/// An answer to one [`Field`].
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// Text, or a chosen option's value.
    Text(String),
    /// A number.
    Number(f64),
    /// Yes or no.
    Boolean(bool),
    /// Several choices.
    List(Vec<String>),
}

impl Answer {
    fn from_json(value: &Value) -> Option<Self> {
        Some(match value {
            Value::String(s) => Self::Text(s.clone()),
            Value::Number(n) => Self::Number(n.as_f64()?),
            Value::Bool(b) => Self::Boolean(*b),
            Value::Array(items) => Self::List(
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_owned))
                    .collect(),
            ),
            _ => return None,
        })
    }

    pub(crate) fn json(&self) -> Value {
        match self {
            Self::Text(s) => Value::from(s.as_str()),
            Self::Number(n) => Value::from(*n),
            Self::Boolean(b) => Value::from(*b),
            Self::List(items) => Value::from(items.clone()),
        }
    }
}

impl Field {
    fn from_json(value: &Value) -> Option<Self> {
        let text = |key: &str| value[key].as_str().map(str::to_owned);
        let number = |key: &str| value[key].as_f64();
        let choices = || -> Vec<Choice> {
            value["options"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|o| {
                            Some(Choice {
                                value: o["value"].as_str()?.to_owned(),
                                label: o["label"].as_str()?.to_owned(),
                                description: o["description"].as_str().map(str::to_owned),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let custom = value["custom"].as_bool().unwrap_or(false);
        let kind = match value["type"].as_str()? {
            "string" => FieldKind::Text {
                options: choices(),
                custom,
                placeholder: text("placeholder"),
                default: text("default"),
            },
            kind @ ("number" | "integer") => FieldKind::Number {
                integer: kind == "integer",
                minimum: number("minimum"),
                maximum: number("maximum"),
                default: number("default"),
            },
            "boolean" => FieldKind::Boolean {
                default: value["default"].as_bool(),
            },
            "multiselect" => FieldKind::Multiselect {
                options: choices(),
                custom,
                min_items: value["minItems"].as_u64(),
                max_items: value["maxItems"].as_u64(),
                default: value["default"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            "external" => FieldKind::External { url: text("url")? },
            _ => return None,
        };
        let when = value["when"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| {
                        Some(Condition {
                            key: c["key"].as_str()?.to_owned(),
                            equal: c["op"].as_str()? == "eq",
                            value: Answer::from_json(&c["value"])?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            key: value["key"].as_str()?.to_owned(),
            title: text("title"),
            description: text("description"),
            required: value["required"].as_bool().unwrap_or(false),
            hidden: value["hidden"].as_bool().unwrap_or(false),
            when,
            kind,
        })
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
