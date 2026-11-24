//! The todo list: the agent's working plan for a session, which it
//! rewrites whole with the `todowrite` tool and the person sees.
//!
//! As in OpenCode, it's offered only when the configuration's
//! `experimental.todo` turns it on.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// One item on the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    /// What's to be done.
    pub content: String,
    /// `pending`, `in_progress`, `completed` or `cancelled`.
    pub status: String,
}

/// The statuses an item can have.
const STATUSES: [&str; 4] = ["pending", "in_progress", "completed", "cancelled"];

/// The `todowrite` tool's definition.
#[must_use]
pub fn definition() -> Value {
    json!({
        "name": "todowrite",
        "description": "Writes the session's todo list, which the person sees as your working \
            plan. Keep one when a task has three or more steps, or the person gives several \
            tasks or asks for a list; skip it for a single simple task or a conversation.\n\n\
            - Send the whole list each time: it replaces the last one.\n\
            - Mark an item in_progress as you start it, and completed as soon as it's done.\n\
            - Call it only when the list changes.",
        "parameters": { "type": "object", "properties": {
            "todos": { "type": "array", "description": "The whole list, in order.", "items": {
                "type": "object", "properties": {
                    "content": { "type": "string", "description": "The step, briefly." },
                    "status": { "type": "string", "enum": STATUSES },
                }, "required": ["content", "status"],
            }},
        }, "required": ["todos"] },
    })
}

/// The list in a `todowrite` call's input.
///
/// # Errors
///
/// When it's missing, or an item lacks its text or has an unknown status,
/// with a message for the model.
pub fn parse(input: &Map<String, Value>) -> Result<Vec<Todo>, String> {
    let items = input
        .get("todos")
        .and_then(Value::as_array)
        .ok_or("todowrite needs `todos`, a list.")?;
    items
        .iter()
        .map(|item| {
            let content = item["content"]
                .as_str()
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .ok_or("Each todo needs `content`, as text.")?;
            let status = item["status"]
                .as_str()
                .filter(|s| STATUSES.contains(s))
                .ok_or_else(|| format!("A todo's status is one of {}.", STATUSES.join(", ")))?;
            Ok(Todo {
                content: content.to_owned(),
                status: status.to_owned(),
            })
        })
        .collect()
}

/// What the model is told after writing `todos`.
#[must_use]
pub fn summary(todos: &[Todo]) -> String {
    let count = |status: &str| todos.iter().filter(|t| t.status == status).count();
    let cancelled = count("cancelled");
    let mut text = format!(
        "Todo list written: {} of {} done, {} in progress, {} pending.",
        count("completed"),
        todos.len() - cancelled,
        count("in_progress"),
        count("pending"),
    );
    if let Some(now) = todos.iter().find(|t| t.status == "in_progress") {
        let _ = write!(text, " Now: \"{}\"", now.content);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_is_read_and_summed_up() {
        let input = json!({ "todos": [
            { "content": "Write it", "status": "in_progress" },
            { "content": "Test it", "status": "pending" },
            { "content": "Skip it", "status": "cancelled" },
        ]});
        let todos = parse(input.as_object().expect("object")).expect("parsed");
        assert_eq!(
            summary(&todos),
            "Todo list written: 0 of 2 done, 1 in progress, 1 pending. Now: \"Write it\""
        );
        let wrong = json!({ "todos": [{ "content": "x", "status": "later" }] });
        let why = parse(wrong.as_object().expect("object")).expect_err("an unknown status");
        assert!(why.contains("in_progress"), "{why}");
    }
}
