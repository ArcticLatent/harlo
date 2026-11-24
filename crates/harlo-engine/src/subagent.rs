//! Subagents: a session hands a task to an agent in a child session.
//!
//! The child works in the background. The call returns at once with the
//! child's ID; its answer comes back as a message each time it goes idle.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use futures_util::future::BoxFuture;
use serde_json::{Map, Value, json};

use crate::config::{ModelRef, model_ref};

/// A task handed to a subagent, or a follow-up to one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The subagent's agent, such as `explore`.
    pub agent: String,
    /// A short label for people.
    pub description: String,
    /// The task.
    pub prompt: String,
    /// The model, when asked for one.
    pub model: Option<ModelRef>,
    /// The child to send a follow-up to; none starts a new one.
    pub session: Option<String>,
}

/// Starts subagents for a turn: where child sessions are made.
pub trait Subagents: Send + Sync + std::fmt::Debug {
    /// Starts `request` for the step `message`'s call `call`, or sends it to
    /// the child it names; returns the child session's ID.
    ///
    /// # Errors
    ///
    /// With a message for the model when the agent or child isn't there.
    fn start<'a>(
        &'a self,
        message: &'a str,
        call: &'a str,
        request: Request,
    ) -> BoxFuture<'a, Result<String, String>>;
}

/// What a child session's first prompt starts with.
pub const PREAMBLE: &str = "You are a subagent: another session handed you this task.";

/// The request in a `subagent` call's input.
///
/// # Errors
///
/// When a required field is missing, with a message for the model.
pub fn parse(input: &Map<String, Value>) -> Result<Request, String> {
    let text = |key: &str| -> Result<String, String> {
        input
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("subagent needs `{key}`, as text."))
    };
    let model = match input.get("model").and_then(Value::as_str) {
        Some(m) if !m.is_empty() => Some(model_ref(&json!(m)).ok_or_else(|| {
            format!("The model is written provider/model, or provider/model#variant, not {m}.")
        })?),
        _ => None,
    };
    Ok(Request {
        agent: text("agent")?,
        description: text("description")?,
        prompt: text("prompt")?,
        model,
        session: input
            .get("sessionID")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    })
}

/// The `subagent` tool's definition, listing the agents `(id, what for)`.
#[must_use]
pub fn definition(agents: &[(String, String)]) -> Value {
    let mut description = "Hands a task to an agent working in a child session. It runs in \
        the background: the call returns at once with the child's session ID, and its answer \
        comes to you as a <background-result kind=\"subagent\"> message each time it finishes, \
        later rounds included.\n\
        Carry on with work that doesn't overlap, or tell the person briefly what you started and \
        end your reply. Don't wait for it, poll it or check on it.\n\
        A new child knows nothing of this conversation: give it all the context and \
        instructions it needs. To send a child a follow-up, pass its sessionID; one sent while \
        it runs joins its current run.\n\nAgents:"
        .to_owned();
    for (id, what) in agents {
        let _ = write!(description, "\n- {id}: {what}");
    }
    json!({
        "name": "subagent",
        "description": description,
        "parameters": { "type": "object", "properties": {
            "agent": { "type": "string", "description": "The agent to hand it to, from the list." },
            "description": { "type": "string", "description": "A label of three to five words for people." },
            "prompt": { "type": "string", "description": "The task." },
            "model": { "type": "string", "description": "Only when the person asks for a model: provider/model, or provider/model#variant." },
            "sessionID": { "type": "string", "description": "A child's session ID, to send it a follow-up. Leave it out to start a new one." },
        }, "required": ["agent", "description", "prompt"] },
    })
}

/// What the model is told once a child has its task.
#[must_use]
pub fn started(session: &str) -> String {
    format!(
        "The subagent is working in the background (sessionID: {session}). Its answer will come \
         as a <background-result kind=\"subagent\"> message when it finishes, and again after \
         each later round. Don't wait for it, poll it or do its work: carry on with other work, \
         or tell the person briefly what you started and end your reply. To send it a follow-up \
         or change its instructions, call subagent again with this sessionID."
    )
}

/// The message a parent gets when child `session` goes idle: its last
/// answer, and how its run ended (`completed`, `failed` or `cancelled`).
#[must_use]
pub fn report(session: &str, state: &str, description: &str, answer: &str) -> String {
    let escape = |s: &str| s.replace('&', "&amp;").replace('"', "&quot;");
    format!(
        "<background-result kind=\"subagent\" sessionID=\"{session}\" state=\"{state}\" \
         description=\"{}\">\n{}\n</background-result>",
        escape(description),
        answer.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_read() {
        let input = json!({ "agent": "explore", "description": "Find it", "prompt": "Look.",
                            "model": "fake/fake-coder#fast" });
        let request = parse(input.as_object().expect("object")).expect("parsed");
        assert_eq!(request.agent, "explore");
        assert_eq!(
            request.model.and_then(|m| m.variant).as_deref(),
            Some("fast")
        );
        let missing = json!({ "agent": "explore" });
        let why = parse(missing.as_object().expect("object")).expect_err("no description");
        assert!(why.contains("description"), "{why}");
        assert!(report("ses_1", "completed", "Say \"hi\"", " Hi. ").contains("\nHi.\n"));
    }
}
