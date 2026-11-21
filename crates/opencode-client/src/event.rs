//! Live events from `GET /api/event`, decoded into the ones Harlo reacts to.
//!
//! The stream is volatile by OpenCode's contract: a slow reader overflows and
//! the stream fails, and events sent while disconnected are lost. Read it on a
//! task of its own (see [`crate::Client::events`]) and, after every
//! (re)connection, reload what the user can act on before applying events.

// Rust guideline compliant 2026-10-09

use serde_json::Value;

use crate::generated as g;
use crate::types::{Form, ModelRef, Permission, Revert};

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
    /// The model started a tool call.
    ToolStarted {
        session_id: String,
        message_id: String,
        call_id: String,
        name: String,
    },
    /// A tool call finished, successfully or not.
    ToolEnded {
        session_id: String,
        message_id: String,
        call_id: String,
        failed: bool,
    },
    /// A model step ended; the message is complete on the server.
    StepEnded {
        session_id: String,
        message_id: String,
    },
    /// An action needs approval.
    PermissionAsked(Permission),
    /// An approval was answered, here or by another client.
    PermissionReplied {
        session_id: String,
        request_id: String,
    },
    /// The agent asked a question.
    FormCreated(Form),
    /// A question was answered or cancelled, here or by another client.
    FormSettled { session_id: String, form_id: String },
    /// A prompt or other item was added to a session's inbox.
    InboxAdded { session_id: String, item_id: String },
    /// An inbox item left the inbox (delivered or cancelled).
    InboxRemoved { session_id: String, item_id: String },
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
            | Self::TextDelta { session_id, .. }
            | Self::ReasoningDelta { session_id, .. }
            | Self::ToolStarted { session_id, .. }
            | Self::ToolEnded { session_id, .. }
            | Self::StepEnded { session_id, .. }
            | Self::PermissionReplied { session_id, .. }
            | Self::FormSettled { session_id, .. }
            | Self::InboxAdded { session_id, .. }
            | Self::InboxRemoved { session_id, .. }
            | Self::RevertStaged { session_id, .. }
            | Self::RevertCleared { session_id }
            | Self::RevertCommitted { session_id } => Some(session_id),
            Self::PermissionAsked(p) => Some(&p.session_id),
            Self::FormCreated(f) => Some(&f.session_id),
            Self::Connected
            | Self::ProjectUpdated { .. }
            | Self::WorktreesChanged { .. }
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
            session_id: data["sessionID"].as_str().unwrap_or_default().to_owned(),
            message_id: data["assistantMessageID"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            call_id: data["id"].as_str().unwrap_or_default().to_owned(),
            failed: kind == "session.tool.failed",
        },
        "session.step.ended" => {
            let d: g::SessionStepEndedData = from(data)?;
            Kind::StepEnded {
                session_id: d.session_id.to_string(),
                message_id: d.assistant_message_id.to_string(),
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
            let d: g::SessionInboxEnqueuedData = from(data)?;
            Kind::InboxAdded {
                session_id: d.session_id.to_string(),
                item_id: d.inbox_id.to_string(),
            }
        }
        "session.inbox.delivered" | "session.inbox.cancelled" => Kind::InboxRemoved {
            session_id: data["sessionID"].as_str().unwrap_or_default().to_owned(),
            item_id: data["inboxID"].as_str().unwrap_or_default().to_owned(),
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
    data["sessionID"].as_str().unwrap_or_default().to_owned()
}
