//! `web_search`: the server's own web search, run in a request of its own.
//!
//! Offered as the server's tool, it came with about 4,400 tokens of the
//! server's instructions in every request, searching or not. As a tool of
//! ours it costs a line; a search sends one small request with the
//! server's search in it (the session's model, low reasoning), and its
//! findings, with their links, come back as the call's result.

use serde_json::{Value, json};

pub const NAME: &str = "web_search";

/// What the search request is told.
pub const INSTRUCTIONS: &str = "Search the web for what you're asked, then report what you found: the facts that answer it, briefly, each with the link it came from (as a Markdown link). No preamble and no advice. If nothing answers it, say so.";

/// The tool's definition (Responses API form).
pub fn spec() -> Value {
    json!({
        "type": "function", "name": NAME, "strict": false,
        "description": "Search the web: what it finds comes back briefly, with links (fetch one to read all of it). For what the project doesn't have: documentation, an API's reference, an issue, a release, news.",
        "parameters": { "type": "object", "properties": { "query": { "type": "string", "description": "What to find out, in words" } }, "required": ["query"], "additionalProperties": false },
    })
}

/// The search request's tools: only the server's search.
pub fn request_specs() -> Value {
    json!([{ "type": "web_search" }])
}

/// The search request's conversation: the question.
pub fn history(query: &str) -> Vec<Value> {
    vec![json!({ "role": "user", "content": [{ "type": "text", "text": query }], "timestamp": crate::session::now_ms() })]
}

/// What the search found: the reply's text.
pub fn findings(reply: &Value) -> String {
    let blocks = reply["content"].as_array().map(Vec::as_slice).unwrap_or_default();
    let text: String = blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n\n");
    let text = text.trim();
    if text.is_empty() { "The search found nothing to report.".into() } else { text.to_owned() }
}

/// The request's usage as a tool result's `details.usage` (which the
/// session counts as its own).
pub fn usage(usage: &Value) -> Value {
    json!({
        "requests": 1,
        "input": usage["input_tokens"].as_u64().unwrap_or(0),
        "cached": usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0),
        "output": usage["output_tokens"].as_u64().unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_are_the_replys_text() {
        let reply = json!({ "content": [{ "type": "webSearch", "query": "q" }, { "type": "text", "text": " Rust 1.99 is out ([blog](https://blog.rust-lang.org)).\n" }] });
        assert_eq!(findings(&reply), "Rust 1.99 is out ([blog](https://blog.rust-lang.org)).");
        assert_eq!(findings(&json!({ "content": [] })), "The search found nothing to report.");
    }

    #[test]
    fn usage_is_counted_as_a_request() {
        let u = json!({ "input_tokens": 5000, "input_tokens_details": { "cached_tokens": 0 }, "output_tokens": 300 });
        assert_eq!(usage(&u), json!({ "requests": 1, "input": 5000, "cached": 0, "output": 300 }));
    }

    #[test]
    fn the_definition_is_short() {
        assert!(spec().to_string().len() < 600);
        assert_eq!(request_specs()[0]["type"], "web_search");
    }
}
