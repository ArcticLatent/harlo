//! Live events from `GET /api/event`, decoded into the ones Harlo reacts to.
//!
//! The stream is volatile by OpenCode's contract: a slow reader overflows and
//! the stream fails, and events sent while disconnected are lost. Read it on a
//! task of its own (see [`crate::Client::events`]) and, after every
//! (re)connection, reload what the user can act on before applying events.

// Rust guideline compliant 2026-10-09

use serde_json::Value;

use crate::generated as g;
use crate::types::{
    Delivery, FileDiff, Form, InboxItem, ModelRef, Permission, Revert, Rule, Tokens,
};

/// One server event.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Event ID (`evt_…`).
    pub id: String,
    /// The per-session sequence number, for durable session events.
    pub seq: Option<u64>,
    /// When it happened, in milliseconds since the Unix epoch.
    pub time: f64,
    /// What happened.
    pub kind: Kind,
}

/// What an [`Event`] reports.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Kind {
    /// The stream is open. Everything before it may have been missed.
    Connected,
    /// A session was created.
    SessionCreated { session_id: String },
    /// A session's title changed.
    SessionRenamed { session_id: String, title: String },
    /// A session was deleted.
    SessionDeleted { session_id: String },
    /// A session moved to another folder or project.
    SessionMoved { session_id: String },
    /// A session was archived.
    SessionArchived { session_id: String },
    /// A session was brought back from the archive.
    SessionUnarchived { session_id: String },
    /// A session switched model.
    ModelSelected { session_id: String, model: ModelRef },
    /// A project's details changed, or a project appeared.
    ProjectUpdated { project_id: String },
    /// A project's worktrees changed.
    WorktreesChanged { project_id: String },
    /// A session started working.
    Busy { session_id: String },
    /// A session stopped working, with `succeeded`, `failed` or `interrupted`.
    Done { session_id: String, outcome: String },
    /// A model step started, as a new assistant message.
    StepStarted {
        session_id: String,
        message_id: String,
        /// The agent running the step.
        agent: Option<String>,
        /// The model answering.
        model: Option<ModelRef>,
    },
    /// Answer text streamed into an assistant message.
    TextDelta {
        session_id: String,
        message_id: String,
        ordinal: i64,
        delta: String,
    },
    /// Reasoning text streamed into an assistant message.
    ReasoningDelta {
        session_id: String,
        message_id: String,
        ordinal: i64,
        delta: String,
    },
    /// An answer text part is complete, with its whole text.
    TextEnded {
        session_id: String,
        message_id: String,
        ordinal: i64,
        text: String,
    },
    /// A reasoning part is complete, with its whole text.
    ReasoningEnded {
        session_id: String,
        message_id: String,
        ordinal: i64,
        text: String,
    },
    /// The model started a tool call.
    ToolStarted {
        session_id: String,
        message_id: String,
        call_id: String,
        name: String,
    },
    /// The model finished writing a tool call's arguments; it runs next.
    ToolCalled {
        session_id: String,
        message_id: String,
        call_id: String,
        input: Value,
    },
    /// A tool call finished, successfully or not.
    ToolEnded {
        session_id: String,
        message_id: String,
        call_id: String,
        failed: bool,
        /// Why it failed, when it did.
        error: Option<String>,
        /// The files it changed, such as an edit's.
        files: Vec<FileDiff>,
    },
    /// A model step failed.
    StepFailed {
        session_id: String,
        message_id: String,
        error: String,
    },
    /// A failed model request will be retried.
    RetryScheduled {
        session_id: String,
        /// Which attempt comes next.
        attempt: u64,
        /// When, in milliseconds since the Unix epoch.
        at: f64,
        /// Why the last attempt failed.
        error: String,
    },
    /// A model step ended; the message is complete on the server.
    StepEnded {
        session_id: String,
        message_id: String,
        /// What the step read and wrote: how full its context window was.
        tokens: Tokens,
    },
    /// The session's running cost and token totals changed.
    UsageUpdated {
        session_id: String,
        /// The cost so far, in US dollars.
        cost: f64,
        /// Every token so far.
        tokens: Tokens,
    },
    /// An action needs approval.
    PermissionAsked(Permission),
    /// An approval was answered, here or by another client.
    PermissionReplied {
        session_id: String,
        request_id: String,
    },
    /// A session's own permission rules changed.
    SessionPermissions {
        session_id: String,
        rules: Vec<Rule>,
    },
    /// The agent asked a question.
    FormCreated(Form),
    /// A question was answered or cancelled, here or by another client.
    FormSettled { session_id: String, form_id: String },
    /// A prompt or other item was added to a session's inbox.
    InboxAdded { session_id: String, item: InboxItem },
    /// An inbox item left the inbox: delivered into the session, or cancelled.
    InboxRemoved {
        session_id: String,
        item_id: String,
        delivered: bool,
    },
    /// A waiting item will now be delivered differently.
    InboxDeliveryChanged {
        session_id: String,
        item_id: String,
        delivery: Delivery,
    },
    /// The session started summarizing itself to free up context.
    CompactionStarted { session_id: String },
    /// More of the summary was written.
    CompactionDelta { session_id: String, text: String },
    /// The summary is done.
    CompactionEnded { session_id: String, summary: String },
    /// Summarizing failed.
    CompactionFailed { session_id: String, error: String },
    /// A skill was loaded into the session.
    SkillActivated { session_id: String, name: String },
    /// The session switched agents.
    AgentSelected { session_id: String, agent: String },
    /// The models, agents, commands or skills on offer changed.
    CatalogChanged,
    /// An undo was staged.
    RevertStaged { session_id: String, revert: Revert },
    /// A staged undo was cancelled.
    RevertCleared { session_id: String },
    /// A staged undo was kept.
    RevertCommitted { session_id: String },
    /// Any other event, by its OpenCode type name.
    Other { kind: String },
}

impl Kind {
    /// The session the event is about, if it's about one.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::SessionCreated { session_id }
            | Self::SessionRenamed { session_id, .. }
            | Self::SessionDeleted { session_id }
            | Self::SessionMoved { session_id }
            | Self::SessionArchived { session_id }
            | Self::SessionUnarchived { session_id }
            | Self::ModelSelected { session_id, .. }
            | Self::Busy { session_id }
            | Self::Done { session_id, .. }
            | Self::StepStarted { session_id, .. }
            | Self::TextDelta { session_id, .. }
            | Self::ReasoningDelta { session_id, .. }
            | Self::TextEnded { session_id, .. }
            | Self::ReasoningEnded { session_id, .. }
            | Self::ToolStarted { session_id, .. }
            | Self::ToolCalled { session_id, .. }
            | Self::ToolEnded { session_id, .. }
            | Self::StepEnded { session_id, .. }
            | Self::UsageUpdated { session_id, .. }
            | Self::StepFailed { session_id, .. }
            | Self::RetryScheduled { session_id, .. }
            | Self::InboxDeliveryChanged { session_id, .. }
            | Self::CompactionStarted { session_id }
            | Self::CompactionDelta { session_id, .. }
            | Self::CompactionEnded { session_id, .. }
            | Self::CompactionFailed { session_id, .. }
            | Self::SkillActivated { session_id, .. }
            | Self::AgentSelected { session_id, .. }
            | Self::PermissionReplied { session_id, .. }
            | Self::FormSettled { session_id, .. }
            | Self::InboxAdded { session_id, .. }
            | Self::InboxRemoved { session_id, .. }
            | Self::RevertStaged { session_id, .. }
            | Self::RevertCleared { session_id }
            | Self::RevertCommitted { session_id }
            | Self::SessionPermissions { session_id, .. } => Some(session_id),
            Self::PermissionAsked(p) => Some(&p.session_id),
            Self::FormCreated(f) => Some(&f.session_id),
            Self::Connected
            | Self::ProjectUpdated { .. }
            | Self::WorktreesChanged { .. }
            | Self::CatalogChanged
            | Self::Other { .. } => None,
        }
    }
}

impl Event {
    /// Decodes one SSE `data` payload.
    ///
    /// # Errors
    ///
    /// Returns an error when the payload isn't JSON, or when an event Harlo
    /// handles doesn't match the pinned schema.
    pub fn parse(data: &str) -> Result<Self, serde_json::Error> {
        let mut raw: Value = serde_json::from_str(data)?;
        let id = raw["id"].as_str().unwrap_or_default().to_owned();
        let seq = raw["durable"]["seq"].as_u64();
        let time = raw["created"].as_f64().unwrap_or_default();
        let kind = raw["type"].as_str().unwrap_or_default().to_owned();
        let data = raw["data"].take();
        Ok(Self {
            id,
            seq,
            time,
            kind: decode(&kind, data)?,
        })
    }
}

#[expect(clippy::too_many_lines, reason = "one short arm per event type")]
fn decode(kind: &str, data: Value) -> Result<Kind, serde_json::Error> {
    use serde_json::from_value as from;
    Ok(match kind {
        "server.connected" => Kind::Connected,
        "session.created" => {
            let d: g::SessionCreatedData = from(data)?;
            Kind::SessionCreated {
                session_id: d.session_id.to_string(),
            }
        }
        "session.renamed" => {
            let d: g::SessionRenamedData = from(data)?;
            Kind::SessionRenamed {
                session_id: d.session_id.to_string(),
                title: d.title,
            }
        }
        "session.deleted" => {
            let d: g::SessionDeletedData = from(data)?;
            Kind::SessionDeleted {
                session_id: d.session_id.to_string(),
            }
        }
        "session.moved" => {
            let d: g::SessionMovedData = from(data)?;
            Kind::SessionMoved {
                session_id: d.session_id.to_string(),
            }
        }
        "session.archived" => Kind::SessionArchived {
            session_id: session_of(&data),
        },
        "session.unarchived" => Kind::SessionUnarchived {
            session_id: session_of(&data),
        },
        "session.model.selected" => {
            let d: g::SessionModelSelectedData = from(data)?;
            Kind::ModelSelected {
                session_id: d.session_id.to_string(),
                model: d.model.into(),
            }
        }
        "project.updated" => Kind::ProjectUpdated {
            project_id: data["id"].as_str().unwrap_or_default().to_owned(),
        },
        "worktree.updated" | "worktree.resolved" => Kind::WorktreesChanged {
            project_id: data["projectID"].as_str().unwrap_or_default().to_owned(),
        },
        "session.execution.started" => {
            let d: g::SessionExecutionStartedData = from(data)?;
            Kind::Busy {
                session_id: d.session_id.to_string(),
            }
        }
        "session.execution.succeeded"
        | "session.execution.failed"
        | "session.execution.interrupted" => {
            let outcome = kind.rsplit('.').next().unwrap_or_default().to_owned();
            let session_id = data["sessionID"].as_str().unwrap_or_default().to_owned();
            Kind::Done {
                session_id,
                outcome,
            }
        }
        "session.step.started" => Kind::StepStarted {
            session_id: session_of(&data),
            message_id: text_of(&data, "assistantMessageID"),
            agent: data["agent"].as_str().map(str::to_owned),
            model: serde_json::from_value::<g::ModelRef>(data["model"].clone())
                .ok()
                .map(ModelRef::from),
        },
        "session.text.ended" | "session.reasoning.ended" => {
            let (session_id, message_id) =
                (session_of(&data), text_of(&data, "assistantMessageID"));
            let ordinal = data["ordinal"].as_i64().unwrap_or_default();
            let text = text_of(&data, "text");
            if kind == "session.text.ended" {
                Kind::TextEnded {
                    session_id,
                    message_id,
                    ordinal,
                    text,
                }
            } else {
                Kind::ReasoningEnded {
                    session_id,
                    message_id,
                    ordinal,
                    text,
                }
            }
        }
        "session.tool.called" => Kind::ToolCalled {
            session_id: session_of(&data),
            message_id: text_of(&data, "assistantMessageID"),
            call_id: text_of(&data, "id"),
            input: data["input"].clone(),
        },
        "session.step.failed" => Kind::StepFailed {
            session_id: session_of(&data),
            message_id: text_of(&data, "assistantMessageID"),
            error: error_of(&data),
        },
        "session.retry.scheduled" => Kind::RetryScheduled {
            session_id: session_of(&data),
            attempt: data["attempt"].as_u64().unwrap_or(1),
            at: data["at"].as_f64().unwrap_or_default(),
            error: error_of(&data),
        },
        "session.inbox.delivery.changed" => Kind::InboxDeliveryChanged {
            session_id: session_of(&data),
            item_id: text_of(&data, "inboxID"),
            delivery: if data["delivery"] == "queue" {
                Delivery::Queue
            } else {
                Delivery::Steer
            },
        },
        "session.compaction.started" => Kind::CompactionStarted {
            session_id: session_of(&data),
        },
        "session.compaction.delta" => Kind::CompactionDelta {
            session_id: session_of(&data),
            text: text_of(&data, "text"),
        },
        "session.compaction.ended" => Kind::CompactionEnded {
            session_id: session_of(&data),
            summary: text_of(&data, "text"),
        },
        "session.compaction.failed" => Kind::CompactionFailed {
            session_id: session_of(&data),
            error: error_of(&data),
        },
        "session.skill.activated" => Kind::SkillActivated {
            session_id: session_of(&data),
            name: text_of(&data, "name"),
        },
        "session.permissions" => Kind::SessionPermissions {
            session_id: session_of(&data),
            rules: from::<Option<g::PermissionRuleset>>(data["permissions"].clone())?
                .map(|rules| rules.0.into_iter().map(Rule::from).collect())
                .unwrap_or_default(),
        },
        "session.agent.selected" => Kind::AgentSelected {
            session_id: session_of(&data),
            agent: text_of(&data, "agent"),
        },
        "model.updated" | "provider.updated" | "agent.updated" | "command.updated"
        | "skill.updated" => Kind::CatalogChanged,
        "session.text.delta" => {
            let d: g::SessionTextDeltaData = from(data)?;
            Kind::TextDelta {
                session_id: d.session_id.to_string(),
                message_id: d.assistant_message_id.to_string(),
                ordinal: d.ordinal,
                delta: d.delta,
            }
        }
        "session.reasoning.delta" => {
            let d: g::SessionReasoningDeltaData = from(data)?;
            Kind::ReasoningDelta {
                session_id: d.session_id.to_string(),
                message_id: d.assistant_message_id.to_string(),
                ordinal: d.ordinal,
                delta: d.delta,
            }
        }
        "session.tool.input.started" => {
            let d: g::SessionToolInputStartedData = from(data)?;
            Kind::ToolStarted {
                session_id: d.session_id.to_string(),
                message_id: d.assistant_message_id.to_string(),
                call_id: d.id,
                name: d.name,
            }
        }
        "session.tool.success" | "session.tool.failed" => Kind::ToolEnded {
            session_id: session_of(&data),
            message_id: text_of(&data, "assistantMessageID"),
            call_id: text_of(&data, "id"),
            failed: kind == "session.tool.failed",
            error: (kind == "session.tool.failed").then(|| error_of(&data)),
            files: data["metadata"]
                .as_object()
                .map(crate::types::proposed_files)
                .unwrap_or_default(),
        },
        "session.step.ended" => {
            let d: g::SessionStepEndedData = from(data)?;
            Kind::StepEnded {
                session_id: d.session_id.to_string(),
                message_id: d.assistant_message_id.to_string(),
                tokens: d.tokens.into(),
            }
        }
        "session.usage.updated" => {
            let d: g::SessionUsageUpdatedData = from(data)?;
            Kind::UsageUpdated {
                session_id: d.session_id.to_string(),
                cost: *d.cost,
                tokens: d.tokens.into(),
            }
        }
        "permission.asked" => Kind::PermissionAsked(from::<g::PermissionAskedData>(data)?.into()),
        "permission.replied" => {
            let d: g::PermissionRepliedData = from(data)?;
            Kind::PermissionReplied {
                session_id: d.session_id.to_string(),
                request_id: d.request_id.to_string(),
            }
        }
        "form.created" => Kind::FormCreated(Form::from_json(&data["form"])),
        "form.replied" | "form.cancelled" => Kind::FormSettled {
            session_id: data["sessionID"].as_str().unwrap_or_default().to_owned(),
            form_id: data["id"].as_str().unwrap_or_default().to_owned(),
        },
        "session.inbox.enqueued" => {
            let mut item = data["item"].clone();
            item["id"] = data["inboxID"].clone();
            Kind::InboxAdded {
                session_id: session_of(&data),
                item: InboxItem::from_json(&item),
            }
        }
        "session.inbox.delivered" | "session.inbox.cancelled" => Kind::InboxRemoved {
            session_id: session_of(&data),
            item_id: text_of(&data, "inboxID"),
            delivered: kind == "session.inbox.delivered",
        },
        "session.revert.staged" => {
            let d: g::SessionRevertStagedData = from(data)?;
            Kind::RevertStaged {
                session_id: d.session_id.to_string(),
                revert: d.revert.into(),
            }
        }
        "session.revert.cleared" => Kind::RevertCleared {
            session_id: data["sessionID"].as_str().unwrap_or_default().to_owned(),
        },
        "session.revert.committed" => Kind::RevertCommitted {
            session_id: data["sessionID"].as_str().unwrap_or_default().to_owned(),
        },
        other => Kind::Other {
            kind: other.to_owned(),
        },
    })
}

/// Reads `sessionID` from an event's data.
fn session_of(data: &Value) -> String {
    text_of(data, "sessionID")
}

/// Reads a text field from an event's data; empty when it's missing.
fn text_of(data: &Value, key: &str) -> String {
    data[key].as_str().unwrap_or_default().to_owned()
}

/// Reads an event's error as one line for people.
fn error_of(data: &Value) -> String {
    let error = &data["error"];
    error["message"]
        .as_str()
        .or_else(|| error["data"]["message"].as_str())
        .or_else(|| error["type"].as_str())
        .unwrap_or("unknown error")
        .to_owned()
}
