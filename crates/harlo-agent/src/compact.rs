//! Compaction: when the context nears the model's window, the older part
//! of the conversation is replaced by a summary the model writes, and the
//! newest messages are kept as they are.
//!
//! - When: past the window less [`RESERVE`] (by the last request's usage),
//!   checked before each request; or when a request is refused for being
//!   too long (then it's sent again once, compacted); or when asked.
//! - Where: the newest [`KEEP`] tokens (a quarter of a small window;
//!   [`ASKED_KEEP`] when you ask) are kept, cut before a message of the
//!   user's or the model's, never between a call and its result.
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
/// The same when you ask for it (/compact): the latest exchange or so, so a
/// session shorter than [`KEEP`] can be compacted too.
pub const ASKED_KEEP: u64 = 2_000;

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
/// thinking, calls and images, not the encrypted signatures kept beside
/// them.
pub fn estimate(m: &Value) -> u64 {
    let chars = match &m["content"] {
        Value::String(s) => s.len(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b["type"].as_str() {
                Some("text") => b["text"].as_str().map_or(0, str::len),
                Some("thinking") => b["thinking"].as_str().map_or(0, str::len),
                Some("toolCall") => b["name"].as_str().map_or(0, str::len) + b["arguments"].to_string().len(),
                // (An image costs about the same whatever its bytes.)
                Some("image") => crate::images::TOKENS as usize * 4,
                _ => b.to_string().len(),
            })
            .sum(),
        _ => m["summary"].as_str().map_or(0, str::len),
    };
    // (And what each message adds around it.)
    (chars as u64).div_ceil(4) + 4
}

/// Roughly how many tokens a piece of text is (instructions, tool
/// schemas), as `estimate` counts a message's.
pub fn estimate_text(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4) + 4
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

/// The user's request that the cut falls inside of (its text), when the
/// kept messages carry on a turn whose request is summarized away.
pub fn turn_cut(history: &[Value], cut: usize) -> Option<String> {
    let last_user = history.iter().rposition(|m| m["role"] == "user")?;
    (last_user < cut).then(|| {
        let m = &history[last_user];
        match &m["content"] {
            Value::String(s) => s.clone(),
            c => c.as_array().into_iter().flatten().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        }
    })
}

/// The longest request quoted when the cut falls inside its turn.
const QUOTED: usize = 4000;

/// What the model is asked, at the end of the conversation it summarizes
/// (`turn`: the request the kept messages carry on, when the cut falls
/// inside its turn).
pub fn request(focus: Option<&str>, turn: Option<&str>) -> String {
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
    if let Some(t) = turn.map(str::trim).filter(|t| !t.is_empty()) {
        let quoted: String = t.chars().take(QUOTED).collect();
        let more = if quoted.len() < t.len() { " (cut short here)" } else { "" };
        s.push_str(&format!(
            "\n\nThis is the middle of a turn: the messages kept after the summary continue your work on the user's latest request, which is in the part summarized. \
             Under Goal, quote that request word for word, and under In progress say exactly where the work stands, so the kept messages make sense after it. The request:\n<request>\n{quoted}{more}\n</request>"
        ));
    }
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

/// Whether the server's own compaction is used (on a Responses API): the
/// conversation up to the cut goes to the model with a compaction trigger,
/// and the opaque item it sends back stands in for it (the entry's
/// `native`), for that model only. By default for the Codex sign-in, where
/// it's known to work and measured better than summarizing (bench H20);
/// `HARLO_AGENT_COMPACT=native` asks for it elsewhere, `=summary` turns it off.
pub fn native(provider: &str) -> bool {
    match std::env::var("HARLO_AGENT_COMPACT").as_deref() {
        Ok("native") => true,
        Ok("summary") => false,
        _ => provider == crate::providers::CODEX,
    }
}

/// Of the user's requests, how many a native compaction's note keeps, and
/// how much of each.
const NOTE_REQUESTS: usize = 10;
const NOTE_CHARS: usize = 600;

/// The user's requests in the part a native compaction takes (each cut to
/// [`NOTE_CHARS`]), those an earlier one took first: an earlier note's
/// requests aren't lost when its item is compacted again.
pub fn native_requests(history: &[Value]) -> Vec<String> {
    history
        .iter()
        .flat_map(|m| -> Vec<String> {
            if m["role"] == "compactionSummary" {
                return m["native"]["requests"].as_array().into_iter().flatten().filter_map(|r| r.as_str().map(str::to_owned)).collect();
            }
            if m["role"] != "user" {
                return Vec::new();
            }
            let text = match &m["content"] {
                Value::String(s) => s.clone(),
                Value::Array(b) => b.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
                _ => String::new(),
            };
            let text = text.trim();
            if text.is_empty() {
                return Vec::new();
            }
            let short: String = text.chars().take(NOTE_CHARS).collect();
            vec![if short.len() < text.len() { format!("{short}…") } else { short }]
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(KEPT_REQUESTS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// The most requests carried from one native compaction to the next.
const KEPT_REQUESTS: usize = 50;

/// A native compaction's summary, for any other model (and for reading):
/// what the user asked in the part compacted (`asked`: from
/// [`native_requests`]).
pub fn native_note(asked: &[String], model: &str) -> String {
    let mut note = format!("The earlier conversation was compacted by {model}'s own compaction, which only that model can read.");
    if !asked.is_empty() {
        let from = asked.len().saturating_sub(NOTE_REQUESTS);
        note.push_str("\n\nThe user's requests in it, the newest last:\n");
        for a in &asked[from..] {
            note.push_str(&format!("\n- {}", a.replace('\n', "\n  ")));
        }
    }
    note
}

/// The files the calls in `history` read and changed, added to `known`.
pub fn files(history: &[Value], known: &(Vec<String>, Vec<String>)) -> (Vec<String>, Vec<String>) {
    let mut read: BTreeSet<String> = known.0.iter().cloned().collect();
    let mut changed: BTreeSet<String> = known.1.iter().cloned().collect();
    for b in history.iter().filter(|m| m["role"] == "assistant").flat_map(|m| m["content"].as_array().into_iter().flatten()).filter(|b| b["type"] == "toolCall") {
        if b["name"] == "apply_patch" {
            for op in crate::patch::parse(b["arguments"]["input"].as_str().unwrap_or_default()).unwrap_or_default() {
                changed.insert(op.path().to_owned());
                if let crate::patch::Op::Update { move_to: Some(to), .. } = op {
                    changed.insert(to);
                }
            }
            continue;
        }
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

    #[test]
    fn a_native_compactions_note_names_the_requests() {
        let u = |t: &str| serde_json::json!({ "role": "user", "content": [{ "type": "text", "text": t }] });
        let mut h: Vec<Value> = (0..12).map(|i| u(&format!("ask {i}"))).collect();
        h.push(u(&"y".repeat(NOTE_CHARS + 10)));
        let note = native_note(&native_requests(&h), "codex/m");
        assert!(note.starts_with("The earlier conversation was compacted by codex/m's own compaction"));
        assert!(!note.contains("ask 2\n") && note.contains("- ask 3\n") && note.contains("- ask 11"), "{note}");
        assert!(note.ends_with('…'));
        assert!(!native_note(&[], "x").contains("requests"));
    }

    #[test]
    fn requests_survive_a_second_native_compaction() {
        let u = |t: &str| serde_json::json!({ "role": "user", "content": t });
        let first = native_requests(&[u("build the parser"), u("and its tests")]);
        let note = serde_json::json!({ "role": "compactionSummary", "summary": "…", "native": { "requests": first } });
        let second = native_requests(&[note, u("now the docs")]);
        assert_eq!(second, ["build the parser", "and its tests", "now the docs"]);
        let many: Vec<Value> = (0..60).map(|i| u(&format!("r{i}"))).collect();
        let kept = native_requests(&many);
        assert_eq!((kept.len(), kept[0].as_str(), kept[49].as_str()), (KEPT_REQUESTS, "r10", "r59"));
    }

    #[test]
    fn a_patch_counts_its_files_as_changed() {
        let call = |name: &str, args: Value| serde_json::json!({ "role": "assistant", "content": [{ "type": "toolCall", "name": name, "arguments": args }] });
        let h = [
            call("read", serde_json::json!({ "path": "a.rs" })),
            call("read", serde_json::json!({ "path": "old.rs" })),
            call("apply_patch", serde_json::json!({ "input": "*** Begin Patch\n*** Add File: new.rs\n+x\n*** Update File: old.rs\n*** Move to: moved.rs\n@@\n-a\n+b\n*** Delete File: gone.rs\n*** End Patch" })),
        ];
        let (read, changed) = files(&h, &(Vec::new(), Vec::new()));
        assert_eq!(read, ["a.rs"]);
        assert_eq!(changed, ["gone.rs", "moved.rs", "new.rs", "old.rs"]);
    }

    #[test]
    fn a_cut_inside_a_turn_names_its_request() {
        let u = |t: &str| serde_json::json!({ "role": "user", "content": [{ "type": "text", "text": t }] });
        let a = serde_json::json!({ "role": "assistant", "content": [] });
        let history = vec![u("first"), a.clone(), u("the latest"), a.clone(), a.clone(), a];
        assert_eq!(turn_cut(&history, 4).as_deref(), Some("the latest"));
        assert_eq!(turn_cut(&history, 2), None);
        let asked = request(None, Some("the latest"));
        assert!(asked.contains("middle of a turn") && asked.contains("<request>\nthe latest\n</request>"), "{asked}");
        assert!(!request(None, None).contains("middle of a turn"));
    }
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
    fn an_image_counts_the_same_whatever_its_bytes() {
        let img = |n: usize| json!({ "role": "user", "content": [{ "type": "image", "data": "A".repeat(n), "mimeType": "image/png" }] });
        assert_eq!(estimate(&img(10)), estimate(&img(1_000_000)));
        assert_eq!(estimate(&img(10)), crate::images::TOKENS + 4);
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
    fn asked_for_a_shorter_session_compacts_too() {
        // About 10k tokens: under KEEP, so nothing on its own; asked for, the
        // older part goes and the newest exchanges stay.
        let mut h = Vec::new();
        for _ in 0..10 {
            h.push(user(&"q".repeat(400)));
            h.push(json!({ "role": "assistant", "content": [{ "type": "text", "text": "a".repeat(3600) }] }));
        }
        assert_eq!(cut(&h, KEEP), None);
        assert_eq!(cut(&h, ASKED_KEEP), Some(16), "the last two exchanges kept");
        // A couple of short messages: nothing, even asked.
        assert_eq!(cut(&[user("say one"), user("say two")], ASKED_KEEP), None);
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
