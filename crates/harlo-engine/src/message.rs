//! A session's messages, in OpenCode 2's shape.
//!
//! A user message holds what was typed. Each model step is an assistant
//! message of its own, with its text, reasoning and tool calls in order;
//! a tool's result lives in its call's state. A run ends with an idle
//! message saying how it went. Times are Unix milliseconds.
//!
//! The JSON these types make is what a client reads back, so field names
//! follow OpenCode's schema exactly.

// Rust guideline compliant 2026-10-09

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::ModelRef;

/// One message in a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Message {
    /// What a person sent.
    User {
        /// Its ID; it sorts in the session's order.
        id: String,
        /// When it was sent.
        time: Created,
        /// The text.
        text: String,
    },
    /// What the server tells the model in the person's place, such as a
    /// change of agent.
    Synthetic {
        /// Its ID; it sorts in the session's order.
        id: String,
        /// When it was made.
        time: Created,
        /// What the model reads.
        text: String,
        /// One line for people, when they should see it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// One model step.
    Assistant(Box<Assistant>),
    /// The end of a run.
    Idle {
        /// Its ID.
        id: String,
        /// When the run ended.
        time: Created,
        /// `succeeded`, `failed` or `interrupted`.
        outcome: String,
        /// Why it failed, when it did.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ErrorInfo>,
    },
}

impl Message {
    /// The message's ID.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. } | Self::Synthetic { id, .. } | Self::Idle { id, .. } => id,
            Self::Assistant(a) => &a.id,
        }
    }
}

/// When something was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Created {
    /// Unix milliseconds.
    pub created: i64,
}

/// A failure, as OpenCode reports one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// A short kind, such as `provider` or `tool`.
    #[serde(rename = "type")]
    pub kind: String,
    /// What went wrong, for a person.
    pub message: String,
}

/// One model step: what the model said and the tools it called.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assistant {
    /// Its ID.
    pub id: String,
    /// When it began, first streamed, finished streaming and completed.
    pub time: StepTime,
    /// The agent that ran it.
    pub agent: String,
    /// The model that answered.
    pub model: ModelRef,
    /// Text, reasoning and tool calls, in the order they came.
    pub content: Vec<Part>,
    /// Why the model stopped: `stop`, `tool-calls`, `length`, `error`, …
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    /// The server's own word for it.
    #[serde(rename = "rawFinish", skip_serializing_if = "Option::is_none")]
    pub raw_finish: Option<String>,
    /// What the step cost in US dollars, when known.
    pub cost: f64,
    /// The tokens it used.
    pub tokens: Tokens,
    /// Why the step failed, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    /// The workspace before and after the step, and the files it touched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Snapshot>,
}

/// The times of a step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepTime {
    /// When the request went out.
    pub created: i64,
    /// When the first token came.
    #[serde(rename = "firstToken", skip_serializing_if = "Option::is_none")]
    pub first_token: Option<i64>,
    /// When the stream ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamed: Option<i64>,
    /// When its tools had run too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<i64>,
}

/// Token counts, as OpenCode reports them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    /// Input tokens not read from the cache.
    pub input: u64,
    /// Output tokens, reasoning included.
    pub output: u64,
    /// Of the output, reasoning tokens.
    pub reasoning: u64,
    /// Cache reads and writes.
    pub cache: Cache,
}

/// Prompt-cache token counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cache {
    /// Input tokens read from the cache.
    pub read: u64,
    /// Input tokens written to it.
    pub write: u64,
}

/// The workspace around a step.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The snapshot before the step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// The snapshot after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    /// Files the step changed, relative to the session's folder.
    #[serde(default)]
    pub files: Vec<String>,
}

/// One piece of a step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Part {
    /// Text the model wrote.
    Text {
        /// The text.
        text: String,
    },
    /// The model's reasoning, as its server shows it.
    Reasoning {
        /// The reasoning.
        text: String,
    },
    /// A tool call and what came of it.
    Tool {
        /// The call's ID, as the model gave it.
        id: String,
        /// The tool's name.
        name: String,
        /// Where the call stands, with its input and result.
        state: ToolState,
        /// When it was asked for, ran and ended.
        time: ToolTime,
    },
}

impl Assistant {
    /// Ends a step that never ended, as stopped: its running calls fail
    /// with `why` and so does the step. Returns whether it changed.
    ///
    /// A step is left unfinished when the server stops during it; what it
    /// had streamed stays.
    pub fn end_unfinished(&mut self, time: i64, why: &str) -> bool {
        if self.time.completed.is_some() {
            return false;
        }
        let aborted = || ErrorInfo {
            kind: "aborted".to_owned(),
            message: why.to_owned(),
        };
        for part in &mut self.content {
            if let Part::Tool { state, time: t, .. } = part
                && let ToolState::Running { input, metadata } = state
            {
                *state = ToolState::Error {
                    input: std::mem::take(input),
                    error: aborted(),
                    metadata: std::mem::take(metadata),
                };
                t.completed = Some(time);
            }
        }
        if self.error.is_none() {
            self.error = Some(aborted());
        }
        if self.finish.is_none() {
            self.finish = Some("unknown".to_owned());
        }
        self.time.completed = Some(time);
        true
    }
}

/// A tool call's times.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolTime {
    /// When the model asked for it.
    pub created: i64,
    /// When it started running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ran: Option<i64>,
    /// When it ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<i64>,
}

/// Where a tool call stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ToolState {
    /// Asked for; waiting for permission or running.
    Running {
        /// Its arguments.
        input: Map<String, Value>,
        /// What the tool has said so far.
        metadata: Map<String, Value>,
    },
    /// Done.
    Completed {
        /// Its arguments.
        input: Map<String, Value>,
        /// What it returned to the model.
        content: Vec<Value>,
        /// What it reports for people, such as a diff.
        #[serde(default, skip_serializing_if = "Map::is_empty")]
        metadata: Map<String, Value>,
    },
    /// Failed or refused.
    Error {
        /// Its arguments.
        input: Map<String, Value>,
        /// Why.
        error: ErrorInfo,
        /// What it reports for people.
        #[serde(default, skip_serializing_if = "Map::is_empty")]
        metadata: Map<String, Value>,
    },
}

impl ToolState {
    /// The call's arguments.
    #[must_use]
    pub fn input(&self) -> &Map<String, Value> {
        match self {
            Self::Running { input, .. }
            | Self::Completed { input, .. }
            | Self::Error { input, .. } => input,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn messages_round_trip_in_opencodes_shape() {
        let step = Message::Assistant(Box::new(Assistant {
            id: "msg_1".into(),
            time: StepTime {
                created: 1,
                first_token: Some(2),
                streamed: Some(3),
                completed: Some(4),
            },
            agent: "build".into(),
            model: ModelRef {
                provider: "fake".into(),
                model: "fake-coder".into(),
                variant: None,
            },
            content: vec![
                Part::Text { text: "hi".into() },
                Part::Tool {
                    id: "call_1".into(),
                    name: "edit".into(),
                    state: ToolState::Completed {
                        input: Map::new(),
                        content: vec![json!({ "type": "text", "text": "Edited" })],
                        metadata: Map::new(),
                    },
                    time: ToolTime {
                        created: 2,
                        ran: Some(3),
                        completed: Some(4),
                    },
                },
            ],
            finish: Some("tool-calls".into()),
            raw_finish: Some("tool_calls".into()),
            cost: 0.0,
            tokens: Tokens::default(),
            error: None,
            snapshot: None,
        }));
        let v = serde_json::to_value(&step).expect("json");
        assert_eq!(v["type"], "assistant");
        assert_eq!(
            v["model"],
            json!({ "id": "fake-coder", "providerID": "fake" })
        );
        assert_eq!(v["time"]["firstToken"], 2);
        assert_eq!(v["content"][1]["state"]["status"], "completed");
        assert_eq!(serde_json::from_value::<Message>(v).expect("back"), step);
        let user = serde_json::to_value(Message::User {
            id: "msg_0".into(),
            time: Created { created: 1 },
            text: "hi".into(),
        })
        .expect("json");
        assert_eq!(
            user,
            json!({ "type": "user", "id": "msg_0", "time": { "created": 1 }, "text": "hi" })
        );
    }
}
