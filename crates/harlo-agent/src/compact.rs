//! Compaction: when the context nears the model's window, the older part
//! of the conversation is replaced by a summary the model writes, and the
//! newest messages are kept as they are.
//!
//! - When: past the window less [`RESERVE`] (by the last request's usage),
//!   checked before each request; or when a request is refused for being
//!   too long (then it's sent again once, compacted); or when asked.
//! - Where: the newest [`KEEP`] tokens (a quarter of a small window) are
//!   kept, cut before a message of the user's or the model's, never between
//!   a call and its result.
//! - How: the model gets the conversation up to the cut as it already has
//!   it (so its cache serves it) and a request to summarize, with no tools
//!   used. The files read and changed are listed by code, across
//!   compactions. In the log: a `compaction` entry, as pi writes it; the
//!   messages before stay in the file.

use std::collections::BTreeSet;

use serde_json::Value;

/// Room left for the reply and the next steps.
pub const RESERVE: u64 = 16_384;
/// Tokens of the newest messages kept as they are.
pub const KEEP: u64 = 20_000;

/// The context size compaction starts at for a model's window (0: unknown,
/// never). `HARLO_AGENT_COMPACT_AT` (tokens) sets it instead, to try it.
pub fn threshold(window: u64) -> u64 {
    if let Some(at) = std::env::var("HARLO_AGENT_COMPACT_AT").ok().and_then(|v| v.trim().parse::<u64>().ok()).filter(|n| *n > 0) {
        return at;
    }
    if window == 0 {
        return 0;
    }
    // (A small window: a quarter of it held back.)
    window.saturating_sub(RESERVE.min(window / 4))
}

/// How much of the newest conversation to keep for a threshold.
pub fn keep(threshold: u64) -> u64 {
    if threshold == 0 { KEEP } else { KEEP.min(threshold / 4) }
}

/// A message's size in tokens, roughly (4 characters a token): its text,
/// thinking and calls, not the encrypted signatures kept beside them.
pub fn estimate(m: &Value) -> u64 {
    let chars = match &m["content"] {
        Value::String(s) => s.len(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b["type"].as_str() {
                Some("text") => b["text"].as_str().map_or(0, str::len),
                Some("thinking") => b["thinking"].as_str().map_or(0, str::len),
                Some("toolCall") => b["name"].as_str().map_or(0, str::len) + b["arguments"].to_string().len(),
                _ => b.to_string().len(),
            })
            .sum(),
        _ => m["summary"].as_str().map_or(0, str::len),
    };
    // (And what each message adds around it.)
    (chars as u64).div_ceil(4) + 4
}

/// Where to cut `history` to keep its newest `keep` tokens: the index of
/// the first message kept (one of the user's or the model's, so no call is
/// parted from its result). None when there's nothing worth summarizing.
pub fn cut(history: &[Value], keep: u64) -> Option<usize> {
    let mut kept = 0;
    let mut i = history.len();
    while i > 0 && kept < keep {
        i -= 1;
        kept += estimate(&history[i]);
    }
    // (At or before that point: more kept, never less.)
    while i > 0 && !matches!(history[i]["role"].as_str(), Some("user" | "assistant" | "custom")) {
        i -= 1;
    }
    // Only a summary before it: nothing new to summarize.
    let before = &history[..i];
    (!before.is_empty() && !(before.len() == 1 && before[0]["role"] == "compactionSummary")).then_some(i)
}

/// What the model is asked, at the end of the conversation it summarizes.
pub fn request(focus: Option<&str>) -> String {
    let mut s = String::from(
        "Your context is nearly full. The conversation above is about to be replaced by a summary that you write now; \
         the newest messages after it are kept as they are, and you will carry on the task from the summary and them. \
         Don't call any tools: answer with the summary only, no preamble.\n\
         \n\
         Use these sections, and leave out any that would be empty:\n\
         ## Goal\nWhat the user asked for (their words where it matters).\n\
         ## Constraints\nRequirements and preferences the user gave (not your standing instructions: you keep those).\n\
         ## Done\nWhat has been done: files and what changed in them, commands run and what they showed.\n\
         ## In progress\nWhat was being worked on, and how far it got.\n\
         ## Decisions\nChoices made, and why.\n\
         ## Next steps\nWhat's left, in order.\n\
         ## Key facts\nExact paths, names, commands, error messages and values you will need again.\n\
         \n\
         If the conversation starts with an earlier summary, fold it in: keep what still matters. Be specific and brief.",
    );
    if let Some(f) = focus.map(str::trim).filter(|f| !f.is_empty()) {
        s.push_str(&format!("\n\nThe user asks you to focus on: {f}"));
    }
    s
}

/// A summary message as the model reads it.
pub fn replayed(m: &Value) -> String {
    format!(
        "The conversation before this point was summarized to free up context. The summary:\n\n<summary>\n{}\n</summary>",
        m["summary"].as_str().unwrap_or_default().trim()
    )
}

/// The files the calls in `history` read and changed, added to `known`.
pub fn files(history: &[Value], known: &(Vec<String>, Vec<String>)) -> (Vec<String>, Vec<String>) {
    let mut read: BTreeSet<String> = known.0.iter().cloned().collect();
    let mut changed: BTreeSet<String> = known.1.iter().cloned().collect();
    for b in history.iter().filter(|m| m["role"] == "assistant").flat_map(|m| m["content"].as_array().into_iter().flatten()).filter(|b| b["type"] == "toolCall") {
        let Some(path) = b["arguments"]["path"].as_str().filter(|p| !p.is_empty()) else { continue };
        match b["name"].as_str() {
            Some("read") => {
                read.insert(path.to_owned());
            }
            Some("write" | "edit") => {
                changed.insert(path.to_owned());
            }
            _ => {}
        }
    }
    // (Changed ones aren't listed as only read.)
    read.retain(|p| !changed.contains(p));
    (read.into_iter().collect(), changed.into_iter().collect())
}

/// The summary with the file lists after it.
pub fn with_files(summary: &str, files: &(Vec<String>, Vec<String>)) -> String {
    let mut s = summary.trim().to_owned();
    if !files.0.is_empty() {
        s.push_str(&format!("\n\n## Files read\n{}", files.0.join("\n")));
    }
    if !files.1.is_empty() {
        s.push_str(&format!("\n\n## Files changed\n{}", files.1.join("\n")));
    }
    s
}

/// Whether a request was refused for being longer than the model takes
/// (not for a rate limit).
pub fn overflowed(error: &anyhow::Error) -> bool {
    let e = format!("{error:#}").to_lowercase();
    if e.contains("rate limit") || e.contains("rate_limit") {
        return false;
    }
    [
        "context_length_exceeded",
        "context length",
        "context window",
        "maximum context",
        "exceed_context_size",
        "exceeds the available context",
        "too many tokens",
        "prompt is too long",
        "input is too long",
        "string_above_max_length",
        "request too large",
    ]
    .iter()
    .any(|p| e.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user(t: &str) -> Value {
        json!({ "role": "user", "content": [{ "type": "text", "text": t }] })
    }

    fn call(id: &str, name: &str, path: &str) -> Value {
        json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": id, "name": name, "arguments": { "path": path } }] })
    }

    fn result(id: &str, text: &str) -> Value {
        json!({ "role": "toolResult", "toolCallId": id, "content": [{ "type": "text", "text": text }] })
    }

    #[test]
    fn cuts_before_a_message_never_at_a_result() {
        let big = "x".repeat(4000);
        let h = vec![user("task"), call("1", "read", "a.rs"), result("1", &big), call("2", "edit", "a.rs"), result("2", &big), user("more")];
        // Keeping ~1000 tokens: the last result alone is that much, so the
        // cut moves back to the call before it.
        assert_eq!(cut(&h, 1000), Some(3));
        assert_eq!(cut(&h, 3), Some(5));
        // Everything fits: nothing to summarize.
        assert_eq!(cut(&h, 100_000), None);
        let after_summary = vec![json!({ "role": "compactionSummary", "summary": "s" }), user(&big)];
        assert_eq!(cut(&after_summary, 10), None);
    }

    #[test]
    fn signatures_dont_count() {
        let m = json!({ "role": "assistant", "content": [
            { "type": "thinking", "thinking": "abcd", "thinkingSignature": "e".repeat(40_000) },
            { "type": "text", "text": "efgh", "textSignature": "{\"id\":\"msg\"}" }] });
        assert_eq!(estimate(&m), 2 + 4);
        assert_eq!(estimate(&user(&"y".repeat(400))), 100 + 4);
    }

    #[test]
    fn thresholds_and_files() {
        assert_eq!((threshold(0), threshold(400_000), threshold(8192)), (0, 400_000 - RESERVE, 8192 - 2048));
        assert_eq!((keep(0), keep(6144), keep(1_000_000)), (KEEP, 1536, KEEP));
        let h = vec![call("1", "read", "a.rs"), call("2", "read", "b.rs"), call("3", "edit", "b.rs"), call("4", "write", "c.rs")];
        let f = files(&h, &(vec!["old.rs".into()], vec!["d.rs".into()]));
        assert_eq!(f, (vec!["a.rs".to_owned(), "old.rs".into()], vec!["b.rs".to_owned(), "c.rs".into(), "d.rs".into()]));
        let s = with_files("## Goal\nx", &f);
        assert!(s.ends_with("## Files changed\nb.rs\nc.rs\nd.rs") && s.contains("## Files read\na.rs\nold.rs"), "{s}");
    }

    #[test]
    fn knows_an_overflow() {
        let e = |t: &str| anyhow::anyhow!(t.to_owned());
        assert!(overflowed(&e("400: Your input exceeds the context window of this model. (context_length_exceeded)")));
        assert!(overflowed(&e("the request exceeds the available context size, try increasing it (exceed_context_size_error)")));
        assert!(!overflowed(&e("429: rate limit reached: too many tokens per minute")));
        assert!(!overflowed(&e("500: server error")));
    }
}
