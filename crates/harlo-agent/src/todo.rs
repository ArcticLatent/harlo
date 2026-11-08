//! `todo`: the model's list of steps for a task, which it keeps up to date
//! as it goes (the whole list each time). harlo shows the latest as the
//! session's todos (`todoPhases` in `get_state`). Offered only when
//! `HARLO_AGENT_TODO=on`: measured on the longest tasks against none, it
//! passed as often but took 67% more requests and 68% more tokens (each
//! update is a round of its own), so it's off by default.

use serde_json::{Value, json};

use crate::tools::Outcome;

const STATUSES: &[&str] = &["pending", "in_progress", "completed"];

/// Whether the tool is offered (opt-in).
pub fn on() -> bool {
    std::env::var("HARLO_AGENT_TODO").is_ok_and(|v| v.trim() == "on")
}

pub fn spec() -> Value {
    json!({
        "type": "function", "name": "todo", "strict": false,
        "description": "Your list of steps for this task, shown to the user. For a task of three or more steps: write the list when you start, then call it again (with the whole list) as each step starts and ends; one step in_progress at a time. Not for a task of one or two steps.",
        "parameters": { "type": "object", "properties": { "todos": { "type": "array", "items": { "type": "object", "properties": {
            "content": { "type": "string", "description": "The step, in a few words" },
            "status": { "type": "string", "enum": STATUSES },
        }, "required": ["content", "status"], "additionalProperties": false } } }, "required": ["todos"], "additionalProperties": false },
    })
}

/// The list as given: what it says back, or why it's refused.
pub fn run(args: &Value) -> Outcome {
    let Some(list) = args["todos"].as_array() else { return Outcome::new("Give todos: the whole list, each with content and status.", true) };
    let mut done = 0;
    let mut now = Vec::new();
    for (i, t) in list.iter().enumerate() {
        let content = t["content"].as_str().map(str::trim).unwrap_or_default();
        if content.is_empty() {
            return Outcome::new(format!("Step {} has no content.", i + 1), true);
        }
        match t["status"].as_str() {
            Some("completed") => done += 1,
            Some("in_progress") => now.push(content),
            Some("pending") => {}
            other => return Outcome::new(format!("Step {}'s status is {other:?}: it's pending, in_progress or completed.", i + 1), true),
        }
    }
    if list.is_empty() {
        return Outcome::new("The list is cleared.", false);
    }
    let mut text = format!("{done} of {} done.", list.len());
    match now.as_slice() {
        [] if done < list.len() => text.push_str(" Nothing is in progress: mark the step you start."),
        [] => {}
        [one] => text.push_str(&format!(" Now: {one}.")),
        many => text.push_str(&format!(" {} steps are in progress; one at a time is clearer.", many.len())),
    }
    Outcome::new(text, false)
}

/// The latest list in `messages` (from the last `todo` call that worked),
/// as harlo's todo phases.
pub fn phases(messages: &[Value]) -> Value {
    let worked = |id: &str| messages.iter().any(|m| m["role"] == "toolResult" && m["toolCallId"] == id && m["isError"] != true);
    let last = messages
        .iter()
        .rev()
        .filter(|m| m["role"] == "assistant")
        .flat_map(|m| m["content"].as_array().into_iter().flatten().rev())
        .find(|b| b["type"] == "toolCall" && b["name"] == "todo" && worked(b["id"].as_str().unwrap_or_default()));
    let tasks: Vec<Value> = last
        .and_then(|b| b["arguments"]["todos"].as_array())
        .into_iter()
        .flatten()
        .map(|t| json!({ "content": t["content"], "status": t["status"] }))
        .collect();
    if tasks.is_empty() { json!([]) } else { json!([{ "name": "", "tasks": tasks }]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_list_and_says_where_it_is() {
        let list = |s: &[(&str, &str)]| json!({ "todos": s.iter().map(|(c, st)| json!({ "content": c, "status": st })).collect::<Vec<_>>() });
        let a = list(&[("read the spec", "completed"), ("write the parser", "in_progress"), ("test it", "pending")]);
        assert_eq!(run(&a).text, "1 of 3 done. Now: write the parser.");
        assert!(run(&list(&[("x", "doing")])).failed);
        assert!(run(&list(&[("a", "pending")])).text.contains("Nothing is in progress"));
        let messages = vec![
            json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": "t1", "name": "todo", "arguments": a }] }),
            json!({ "role": "toolResult", "toolCallId": "t1", "content": [] }),
            json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": "t2", "name": "todo", "arguments": list(&[("x", "doing")]) }] }),
            json!({ "role": "toolResult", "toolCallId": "t2", "content": [], "isError": true }),
        ];
        let p = phases(&messages);
        assert_eq!(p[0]["tasks"][1], json!({ "content": "write the parser", "status": "in_progress" }), "the last list that worked");
        assert_eq!(phases(&[]), json!([]));
    }
}
