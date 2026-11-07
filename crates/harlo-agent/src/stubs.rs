//! Setting old tool output aside: before the context gets as far as being
//! summarized, the larger tool results older than the newest part of the
//! conversation are replaced by stubs that say where they came from (the
//! tool and its path, command or pattern) and how big they were; `recall`
//! returns one as it was. All at once, so the prompt cache breaks once and
//! then serves again.
//!
//! - When: first when a request is refused as too long; and, when it's
//!   turned on (`HARLO_AGENT_STUB_AT`), past [`threshold`], checked before
//!   each request. Off by default: measured on long tasks against
//!   summarizing alone, it passed as often but saved no tokens, and the
//!   model never recalled anything.
//! - Which: tool results before the newest `keep` tokens (as compaction
//!   keeps), [`MIN_TOKENS`] or more, not set aside already; only when they
//!   free enough to be worth the cache.
//! - In the log: a `harlo_stubs` entry with each result's id and its stub;
//!   the results themselves stay in the file (harlo shows them, `recall`
//!   reads them after a restart).

use serde_json::{Value, json};

use crate::compact;
use crate::tools::Original;

/// A result this small stays (its stub would save next to nothing).
pub const MIN_TOKENS: u64 = 300;
/// Set aside only when it frees at least this many tokens…
const MIN_SAVED: u64 = 2_000;
/// …or this share of the context (whichever is more).
const MIN_SHARE: u64 = 10;

/// Where setting aside starts as the context grows, for compaction's
/// `threshold` (0: it doesn't). `HARLO_AGENT_STUB_AT` turns it on: a number
/// of tokens, or `on` for 70% of where compaction starts.
pub fn threshold(compact_at: u64) -> u64 {
    match std::env::var("HARLO_AGENT_STUB_AT").ok().map(|v| v.trim().to_owned()) {
        Some(v) if v == "on" => compact_at * 7 / 10,
        Some(v) => v.parse().unwrap_or(0),
        None => 0,
    }
}

/// Whether `m` is a stub already.
pub fn is_stub(m: &Value) -> bool {
    m["harloStub"] == true
}

/// The results to set aside in `history` (their indexes), keeping the
/// newest `keep` tokens as they are, and what that saves; nothing when it
/// saves less than is worth it for a context of `context` tokens.
pub fn pick(history: &[Value], keep: u64, context: u64) -> (Vec<usize>, u64) {
    let mut kept = 0;
    let mut boundary = history.len();
    while boundary > 0 && kept < keep {
        boundary -= 1;
        kept += compact::estimate(&history[boundary]);
    }
    let mut picked = Vec::new();
    let mut saved = 0;
    for (i, m) in history[..boundary].iter().enumerate() {
        if m["role"] != "toolResult" || is_stub(m) {
            continue;
        }
        let size = compact::estimate(m);
        if size < MIN_TOKENS {
            continue;
        }
        picked.push(i);
        saved += size.saturating_sub(STUB_TOKENS);
    }
    if saved < MIN_SAVED.max(context * MIN_SHARE / 100) {
        return (Vec::new(), 0);
    }
    (picked, saved)
}

/// About what a stub costs.
const STUB_TOKENS: u64 = 60;

/// The call a result answers (its tool's arguments), from the history.
fn call_of<'a>(history: &'a [Value], id: &str) -> Option<&'a Value> {
    history.iter().rev().filter(|m| m["role"] == "assistant").flat_map(|m| m["content"].as_array().into_iter().flatten()).find(|b| b["type"] == "toolCall" && b["id"] == id)
}

/// A short form of a value for the stub (one line, at most 120 characters).
fn short(s: &str) -> String {
    let line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= 120 { line } else { format!("{}…", line.chars().take(119).collect::<String>()) }
}

/// What a result came from, in a few words: `read src/x.rs (lines 40–90)`,
/// `bash: cargo test`, `grep "fn main" in src`.
fn source(name: &str, args: &Value) -> String {
    let s = |k: &str| args[k].as_str().unwrap_or_default();
    match name {
        "read" => {
            let lines = match (args["offset"].as_u64(), args["limit"].as_u64()) {
                (Some(o), Some(l)) => format!(" (lines {o}–{})", o + l.saturating_sub(1)),
                (Some(o), None) => format!(" (from line {o})"),
                (None, Some(l)) => format!(" (first {l} lines)"),
                (None, None) => String::new(),
            };
            format!("read {}{lines}", short(s("path")))
        }
        "bash" => format!("bash: {}", short(s("command"))),
        "grep" => {
            let place = [s("path"), s("glob")].iter().filter(|p| !p.is_empty()).map(|p| short(p)).collect::<Vec<_>>().join(" ");
            format!("grep {:?}{}", short(s("pattern")), if place.is_empty() { String::new() } else { format!(" in {place}") })
        }
        "find" => format!("find {}", short(s("pattern"))),
        "ls" => format!("ls {}", short(if s("path").is_empty() { "." } else { s("path") })),
        "recall" => format!("recall {}", s("id")),
        other => format!("{other} {}", short(&args.to_string())),
    }
}

/// The stub for a result: where it came from, its size, how to get it back.
pub fn stub_text(history: &[Value], result: &Value) -> String {
    let id = result["toolCallId"].as_str().unwrap_or_default();
    let name = result["toolName"].as_str().unwrap_or("a tool");
    let args = call_of(history, id).map(|c| &c["arguments"]).unwrap_or(&Value::Null);
    let tokens = compact::estimate(result);
    let size = if tokens >= 1000 { format!("{:.1}k tokens", tokens as f64 / 1000.0) } else { format!("{tokens} tokens") };
    let failed = if result["isError"] == true { ", which failed" } else { "" };
    format!("[Output set aside to save context: {}{failed} ({size}). recall {{\"id\": {}}} returns it.]", source(name, args), json!(id))
}

/// `result` as its stub (its id and the rest kept).
pub fn as_stub(result: &Value, text: &str) -> Value {
    let mut m = result.clone();
    m["content"] = json!([{ "type": "text", "text": text }]);
    m["harloStub"] = json!(true);
    // (What the log kept beside it isn't sent anyway.)
    if let Some(o) = m.as_object_mut() {
        o.remove("details");
    }
    m
}

/// The file a `read` result read (None for other tools).
pub fn read_path(history: &[Value], result: &Value) -> Option<std::path::PathBuf> {
    if result["toolName"] != "read" {
        return None;
    }
    let call = call_of(history, result["toolCallId"].as_str()?)?;
    call["arguments"]["path"].as_str().filter(|p| !p.is_empty()).map(std::path::PathBuf::from)
}

/// The original of a result set aside, for `recall`.
pub fn original(history: &[Value], result: &Value) -> (String, Original) {
    (result["toolCallId"].as_str().unwrap_or_default().to_owned(), Original { result: result.clone(), read: read_path(history, result) })
}

/// A `harlo_stubs` entry's stubs applied to `messages` (a log replayed):
/// the originals, by call id.
pub fn replay(entry: &Value, messages: &mut [Value]) -> Vec<(String, Original)> {
    let mut originals = Vec::new();
    for s in entry["stubs"].as_array().into_iter().flatten() {
        let (Some(id), Some(text)) = (s["id"].as_str(), s["text"].as_str()) else { continue };
        let Some(i) = messages.iter().position(|m| m["role"] == "toolResult" && m["toolCallId"] == id && !is_stub(m)) else { continue };
        originals.push(original(messages, &messages[i]));
        messages[i] = as_stub(&messages[i], text);
    }
    originals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, name: &str, args: Value) -> Value {
        json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": id, "name": name, "arguments": args }] })
    }

    fn result(id: &str, name: &str, chars: usize) -> Value {
        json!({ "role": "toolResult", "toolCallId": id, "toolName": name, "content": [{ "type": "text", "text": "x".repeat(chars) }], "isError": false, "details": { "kept": 1 } })
    }

    fn user(text: &str) -> Value {
        json!({ "role": "user", "content": [{ "type": "text", "text": text }] })
    }

    #[test]
    fn picks_the_old_big_ones() {
        let history = vec![
            user("go"),
            call("c1", "read", json!({ "path": "src/a.rs" })),
            result("c1", "read", 12_000),
            call("c2", "ls", json!({})),
            result("c2", "ls", 400),
            call("c3", "bash", json!({ "command": "cargo test" })),
            result("c3", "bash", 20_000),
            user("and now"),
            call("c4", "read", json!({ "path": "src/b.rs" })),
            result("c4", "read", 12_000),
        ];
        // The newest 3000 tokens kept: c4's read stays; c2's is too small.
        let (picked, saved) = pick(&history, 3_000, 20_000);
        assert_eq!(picked, [2, 6]);
        assert!(saved > 7_000, "{saved}");
        // Too little freed for the context: nothing.
        assert_eq!(pick(&history, 3_000, 200_000).0, Vec::<usize>::new());
        // Already set aside: not again.
        let mut again = history.clone();
        again[2] = as_stub(&again[2], "s");
        again[6] = as_stub(&again[6], "s");
        assert!(pick(&again, 3_000, 20_000).0.is_empty());
    }

    #[test]
    fn a_stub_says_where_it_came_from() {
        let history = vec![call("c1", "read", json!({ "path": "src/a.rs", "offset": 40, "limit": 51 })), result("c1", "read", 8_000)];
        let text = stub_text(&history, &history[1]);
        assert_eq!(text, "[Output set aside to save context: read src/a.rs (lines 40–90) (2.0k tokens). recall {\"id\": \"c1\"} returns it.]");
        let mut failed = result("c2", "bash", 2_000);
        failed["isError"] = json!(true);
        let history = vec![call("c2", "bash", json!({ "command": "cargo   test\n --all" })), failed];
        assert!(stub_text(&history, &history[1]).contains("bash: cargo test --all, which failed (504 tokens)"));
        assert!(stub_text(&[], &result("c9", "grep", 2_000)).contains("grep \"\""));
        let s = as_stub(&history[1], "stub");
        assert!(is_stub(&s) && s.get("details").is_none() && s["toolCallId"] == "c2" && s["isError"] == true);
    }

    #[test]
    fn replays_an_entry() {
        let mut messages = vec![call("c1", "read", json!({ "path": "a" })), result("c1", "read", 4_000), result("c2", "read", 4_000)];
        let entry = json!({ "type": "harlo_stubs", "stubs": [{ "id": "c1", "text": "[c1 aside]" }, { "id": "gone", "text": "x" }] });
        let originals = replay(&entry, &mut messages);
        assert_eq!(originals.len(), 1);
        assert_eq!(originals[0].0, "c1");
        assert_eq!(originals[0].1.result["content"][0]["text"].as_str().unwrap().len(), 4_000);
        assert_eq!(originals[0].1.read.as_deref(), Some(std::path::Path::new("a")));
        assert_eq!(messages[1]["content"][0]["text"], "[c1 aside]");
        assert!(!is_stub(&messages[2]));
    }

    #[test]
    fn where_it_starts() {
        // (Off unless HARLO_AGENT_STUB_AT is set, as the benchmark does.)
        if std::env::var_os("HARLO_AGENT_STUB_AT").is_none() {
            assert_eq!(threshold(100_000), 0);
        }
    }
}
