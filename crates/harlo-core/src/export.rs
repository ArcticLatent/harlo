//! A session as Markdown (Export / Copy as Markdown, the phone's Share).
//!
//! A title and what it ran on, then each of your messages and each reply,
//! with the agent's tool calls in brief (a command's short output, an
//! edit's lines added and removed). Thinking is left out. From the
//! session's messages, so every agent and saved sessions too.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use serde_json::Value;

/// Reads a session's messages, handing each to the function it is given.
pub type ReadMessages<'a> = dyn FnMut(&mut dyn FnMut(Value)) -> std::io::Result<()> + 'a;

/// About the session, for the head.
#[derive(Debug, Clone, Default)]
pub struct Info {
    pub title: String,
    /// "Claude", "Harlo"…
    pub agent: String,
    pub model: String,
    /// Its folder.
    pub folder: String,
}

/// A command's output is shown when it's at most this many lines.
const OUTPUT_LINES: usize = 12;

fn text_blocks(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| match b["type"].as_str() {
                Some("text") => b["text"].as_str().map(str::to_owned),
                Some("image") => Some("*(image)*".to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

/// `09:41` (local), from unix ms.
fn time_of(ms: Option<i64>) -> String {
    use chrono::TimeZone;
    ms.and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single()).map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

/// Headings in a message two levels down (at most ######), so they sit
/// under the turn's; code blocks are left alone.
fn demote(text: &str) -> String {
    let mut fence: Option<&str> = None;
    text.lines()
        .map(|l| {
            let t = l.trim_start();
            for mark in ["```", "~~~"] {
                if t.starts_with(mark) {
                    fence = match fence {
                        Some(m) if m == mark => None,
                        None => Some(mark),
                        other => other,
                    };
                }
            }
            let hashes = l.chars().take_while(|&c| c == '#').count();
            if fence.is_none() && (1..=6).contains(&hashes) && l[hashes..].starts_with(' ') {
                format!("{}{}", "#".repeat((hashes + 2).min(6)), &l[hashes..])
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A fence longer than any backtick run in `text`.
fn fence(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

/// The session as Markdown.
///
/// # Panics
///
/// Never in practice: it's written to memory, which doesn't fail, and
/// Markdown is text.
pub fn markdown(messages: &[Value], info: &Info) -> String {
    let mut out = Vec::new();
    write_markdown(
        &mut |each| {
            messages.iter().cloned().for_each(each);
            Ok(())
        },
        info,
        &mut out,
    )
    .expect("writing to memory");
    String::from_utf8(out).expect("Markdown is text")
}

/// What the first pass learns of a call id: where its last result is, and
/// how many calls name it.
#[derive(Default)]
struct CallId {
    last_result: Option<usize>,
    calls: usize,
}

/// [`markdown`] written to `out` as it's made.
///
/// From messages `read` passes on one at a time (it's read twice: once for
/// where each call's result is, then to write). What's held at once is a call
/// waiting for its result and what follows it, never the conversation.
///
/// # Errors
///
/// An I/O error from `out`. An error from `read` too.
pub fn write_markdown(read: &mut ReadMessages<'_>, info: &Info, out: &mut dyn std::io::Write) -> std::io::Result<()> {
    write_markdown_spilling(read, info, out, None)
}

/// Text waiting behind a call whose result hasn't come, held in memory up
/// to this much; past it, in a file of its own (see [`write_markdown_spilling`]).
const HELD: usize = 1 << 20;

#[cfg(test)]
thread_local! {
    /// (Tests: what's held at most before spilling, and the most held.)
    static TEST_HELD: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((HELD, 0)) };
}

fn held_limit() -> usize {
    #[cfg(test)]
    return TEST_HELD.get().0;
    #[cfg(not(test))]
    HELD
}

/// Each tool call's id: how many calls name it and which message holds its
/// last result; and when the session began.
fn call_ids(read: &mut ReadMessages<'_>) -> std::io::Result<(std::collections::HashMap<String, CallId>, Option<i64>)> {
    let mut ids: std::collections::HashMap<String, CallId> = std::collections::HashMap::new();
    let mut first_time = None;
    let mut i = 0;
    read(&mut |m| {
        first_time = first_time.or_else(|| m["timestamp"].as_i64());
        match m["role"].as_str() {
            Some("toolResult") => {
                if let Some(id) = m["toolCallId"].as_str() {
                    ids.entry(id.to_owned()).or_default().last_result = Some(i);
                }
            }
            Some("assistant") => {
                for id in m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall").filter_map(|b| b["id"].as_str()) {
                    ids.entry(id.to_owned()).or_default().calls += 1;
                }
            }
            _ => {}
        }
        i += 1;
    })?;
    Ok((ids, first_time))
}

/// The export's title, and its agent, model, folder and date under it.
fn heading(info: &Info, first_time: Option<i64>) -> String {
    let mut out = format!("# {}\n\n", if info.title.trim().is_empty() { "Session" } else { info.title.trim() });
    let date = {
        use chrono::TimeZone;
        first_time.and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single()).map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
    };
    let head: Vec<String> = [info.agent.clone(), info.model.clone(), if info.folder.is_empty() { String::new() } else { format!("`{}`", info.folder) }, date]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if !head.is_empty() {
        out.push('*');
        out.push_str(&head.join(" · "));
        out.push_str("*\n\n");
    }
    out
}

/// [`write_markdown`], keeping what waits behind a late result in a file.
///
/// What waits past [`HELD`] is kept in a file in `spill` (made there and
/// unlinked at once) rather than in memory: a call whose result comes far later
/// holds about nothing.
///
/// # Errors
///
/// An I/O error from `out`. An error from `read`, or when the file in `spill`
/// can't be made or used.
pub fn write_markdown_spilling(read: &mut ReadMessages<'_>, info: &Info, out: &mut dyn std::io::Write, spill: Option<&std::path::Path>) -> std::io::Result<()> {
    let (mut ids, first_time) = call_ids(read)?;

    let mut w = Trimmed { out, held: String::new(), failed: None };
    w.push(&heading(info, first_time));
    // Pieces written in order once nothing before them waits: text, and
    // calls still waiting for their (last) result. A call is made into its
    // text as soon as that result is read, so results aren't held for the
    // calls behind a waiting one.
    let mut queue = Waiting::new(spill);
    // Results read before calls that name them (or for calls still to come).
    let mut results: std::collections::HashMap<String, (String, bool)> = std::collections::HashMap::new();
    // The agent's turn: one heading, however many messages it takes.
    let mut in_reply = false;
    let mut i = 0;
    // (A call's text with what's known of it, counted off its id.)
    let made = |b: &Value, result: Option<&(String, bool)>, ids: &mut std::collections::HashMap<String, CallId>, results: &mut std::collections::HashMap<String, (String, bool)>| {
        let text = tool_call(b, result);
        if let Some(id) = b["id"].as_str()
            && let Some(c) = ids.get_mut(id)
        {
            c.calls = c.calls.saturating_sub(1);
            if c.calls == 0 {
                results.remove(id);
            }
        }
        text
    };
    let mut failed = None;
    read(&mut |m| {
        match m["role"].as_str() {
            Some("toolResult") => {
                if let Some(id) = m["toolCallId"].as_str()
                    && ids.get(id).is_some_and(|c| c.calls > 0 && c.last_result == Some(i))
                {
                    let result = (text_blocks(&m["content"]), m["isError"].as_bool().unwrap_or(false));
                    for b in queue.take_calls(id) {
                        let text = made(&b, Some(&result), &mut ids, &mut results);
                        queue.set_call_text(id, text);
                    }
                    if ids.get(id).is_some_and(|c| c.calls > 0) {
                        results.insert(id.to_owned(), result);
                    }
                }
            }
            Some("user") => {
                use harlo_harness::file_blocks as fb;
                let text = text_blocks(&m["content"]);
                // harlo's own notes riding along with a message stay out
                // (attached files: their names).
                let (files, text) = fb::split_blocks(&text);
                let text = text.split("<harlo-").next().unwrap_or("");
                let text = fb::strip_plan(fb::strip_ask_first(text).0).0.trim().to_owned();
                let text = if files.is_empty() { text } else { format!("{text}\n\n*Attached: {}*", files.join(", ")).trim().to_owned() };
                if !text.is_empty() {
                    let t = time_of(m["timestamp"].as_i64());
                    queue.text(format!("---\n\n## You{}\n\n{}\n\n", if t.is_empty() { String::new() } else { format!(" · {t}") }, demote(&text)));
                    in_reply = false;
                }
            }
            Some("assistant") => {
                if !in_reply {
                    let t = time_of(m["timestamp"].as_i64());
                    queue.text(format!("## {}{}\n\n", if info.agent.is_empty() { "Agent" } else { &info.agent }, if t.is_empty() { String::new() } else { format!(" · {t}") }));
                    in_reply = true;
                }
                let mut m = m;
                for b in m["content"].as_array_mut().map(std::mem::take).into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") => {
                            let text = demote(b["text"].as_str().unwrap_or("").trim());
                            if !text.is_empty() {
                                queue.text(text + "\n\n");
                            }
                        }
                        Some("toolCall") => {
                            let id = b["id"].as_str().map(str::to_owned);
                            let waiting = id.as_deref().and_then(|id| ids.get(id)).and_then(|c| c.last_result).is_some_and(|at| at > i);
                            match id {
                                Some(id) if waiting => queue.call(id, b),
                                id => {
                                    let result = id.as_deref().and_then(|id| results.get(id)).cloned();
                                    let text = made(&b, result.as_ref(), &mut ids, &mut results);
                                    queue.text(text);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(e) = m["errorMessage"].as_str().filter(|e| !e.is_empty()) {
                    queue.text(format!("> **Error:** {e}\n\n"));
                }
            }
            _ => {}
        }
        if let Err(e) = queue.write_ready(&mut w) {
            failed.get_or_insert(e);
        }
        i += 1;
    })?;
    if let Some(e) = failed {
        return Err(e);
    }
    // (Calls whose results never came: as they are.)
    queue.write_all(&mut w, &mut |b| made(b, b["id"].as_str().and_then(|id| results.get(id)).cloned().as_ref(), &mut ids, &mut results))?;
    w.out.write_all(b"\n")?;
    w.failed.map_or(Ok(()), Err)
}

/// The pieces [`write_markdown_spilling`] holds until what's before them
/// is written.
enum Piece {
    Text(String),
    /// Text moved to the spill file: where, and how long.
    Spilled(u64, usize),
    /// A call waiting for its result: its id, and the call.
    Call(String, Value),
}

struct Waiting<'a> {
    pieces: std::collections::VecDeque<Piece>,
    /// Text held in memory, in bytes.
    held: usize,
    dir: Option<&'a std::path::Path>,
    file: Option<(std::fs::File, u64)>,
}

impl<'a> Waiting<'a> {
    fn new(dir: Option<&'a std::path::Path>) -> Self {
        Waiting { pieces: std::collections::VecDeque::default(), held: 0, dir, file: None }
    }

    fn text(&mut self, text: String) {
        self.held += text.len();
        self.pieces.push_back(Piece::Text(text));
        self.check();
    }

    /// Spills when too much is held.
    fn check(&mut self) {
        #[cfg(test)]
        TEST_HELD.set((TEST_HELD.get().0, TEST_HELD.get().1.max(self.held)));
        if self.held > held_limit() {
            self.spill();
        }
    }

    fn call(&mut self, id: String, b: Value) {
        self.pieces.push_back(Piece::Call(id, b));
    }

    /// The waiting calls named `id`, in order, their places kept for
    /// [`Self::set_call_text`].
    fn take_calls(&mut self, id: &str) -> Vec<Value> {
        self.pieces
            .iter_mut()
            .filter_map(|p| match p {
                Piece::Call(of, b) if of == id => Some(std::mem::take(b)),
                _ => None,
            })
            .collect()
    }

    /// The first of `id`'s calls taken becomes its text.
    fn set_call_text(&mut self, id: &str, text: String) {
        if let Some(p) = self.pieces.iter_mut().find(|p| matches!(p, Piece::Call(of, b) if of == id && b.is_null())) {
            self.held += text.len();
            *p = Piece::Text(text);
        }
        self.check();
    }

    /// Text held in memory goes to the spill file (none to be had: it stays).
    fn spill(&mut self) {
        let Some(dir) = self.dir else { return };
        if self.file.is_none() {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = dir.join(format!(".harlo-export-{}-{}.spill", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            match std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
                Ok(f) => {
                    let _ = std::fs::remove_file(&path);
                    self.file = Some((f, 0));
                }
                Err(e) => {
                    tracing::event!(name: "export.spill.unavailable", tracing::Level::DEBUG, error.message = %e, "an export holds what waits in memory: {{error.message}}");
                    self.dir = None;
                    return;
                }
            }
        }
        let Some((file, end)) = self.file.as_mut() else { return };
        use std::os::unix::fs::FileExt;
        for p in &mut self.pieces {
            if let Piece::Text(text) = p {
                if file.write_all_at(text.as_bytes(), *end).is_err() {
                    self.dir = None;
                    return;
                }
                let len = text.len();
                self.held -= len;
                *p = Piece::Spilled(*end, len);
                *end += len as u64;
            }
        }
    }

    fn read_back(&self, at: u64, len: usize) -> std::io::Result<String> {
        use std::os::unix::fs::FileExt;
        let (file, _) = self.file.as_ref().ok_or_else(|| std::io::Error::other("no spill file"))?;
        let mut buf = vec![0; len];
        file.read_exact_at(&mut buf, at)?;
        String::from_utf8(buf).map_err(std::io::Error::other)
    }

    /// Writes what no waiting call is before.
    fn write_ready(&mut self, w: &mut Trimmed) -> std::io::Result<()> {
        while let Some(p) = self.pieces.front() {
            match p {
                Piece::Call(..) => break,
                Piece::Text(text) => {
                    w.push(text);
                    self.held -= text.len();
                }
                &Piece::Spilled(at, len) => w.push(&self.read_back(at, len)?),
            }
            self.pieces.pop_front();
        }
        if self.pieces.is_empty() {
            // (Nothing waits: the file starts over.)
            if let Some((file, end)) = self.file.as_mut() {
                let _ = file.set_len(0);
                *end = 0;
            }
        }
        Ok(())
    }

    /// Writes all of it, the calls still waiting as `call` makes them.
    fn write_all(&mut self, w: &mut Trimmed, call: &mut dyn FnMut(&Value) -> String) -> std::io::Result<()> {
        while let Some(p) = self.pieces.pop_front() {
            match p {
                Piece::Call(_, b) => w.push(&call(&b)),
                Piece::Text(text) => w.push(&text),
                Piece::Spilled(at, len) => w.push(&self.read_back(at, len)?),
            }
        }
        Ok(())
    }
}

/// Writes what it's given but the whitespace it ends with, until more
/// comes (the Markdown ends with its last word, then a newline).
struct Trimmed<'a> {
    out: &'a mut dyn std::io::Write,
    held: String,
    failed: Option<std::io::Error>,
}

impl Trimmed<'_> {
    fn push(&mut self, text: &str) {
        let kept = text.trim_end();
        if kept.is_empty() {
            self.held.push_str(text);
            return;
        }
        if self.failed.is_none()
            && let Err(e) = self.out.write_all(self.held.as_bytes()).and_then(|()| self.out.write_all(kept.as_bytes()))
        {
            self.failed = Some(e);
        }
        text[kept.len()..].clone_into(&mut self.held);
    }
}

/// "- **bash** `cargo test` → 12 passed", and a short output.
fn tool_call(b: &Value, result: Option<&(String, bool)>) -> String {
    let name = b["name"].as_str().unwrap_or("tool");
    let kind = crate::actions::kind(name);
    if kind == "todo" {
        return String::new();
    }
    let args = &b["arguments"];
    let log = crate::actions::log(&[serde_json::json!({ "role": "assistant", "content": [b] })]);
    let target = log["actions"][0]["target"].as_str().unwrap_or("").to_owned();
    let (output, error) = result.cloned().unwrap_or_default();
    let (result, _) = crate::tool_facts::facts(name, args, &output, error);
    let mut line = format!("- **{name}**");
    if !target.is_empty() {
        let f = if target.contains('`') { "``" } else { "`" };
        let _ = write!(line, " {f}{target}{f}");
    }
    if error {
        line.push_str(if result.is_empty() || result == "failed" { " → failed".to_owned() } else { format!(" → failed ({result})") }.as_str());
    } else if !result.is_empty() {
        let _ = write!(line, " → {result}");
    }
    line.push('\n');
    // A command's output, when short.
    let output = output.trim_end();
    if kind == "command" && !output.is_empty() && output.lines().count() <= OUTPUT_LINES {
        let f = fence(output);
        let _ = write!(line, "\n  {f}\n{}\n  {f}\n", output.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n"));
    }
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_session_as_markdown() {
        let messages = vec![
            json!({ "role": "user", "timestamp": 1_760_000_000_000i64, "content": [{ "type": "text", "text": "Fix the failing test" }] }),
            json!({ "role": "assistant", "content": [
                { "type": "thinking", "thinking": "secret plan" },
                { "type": "text", "text": "Let me look." },
                { "type": "toolCall", "id": "a", "name": "bash", "arguments": { "command": "cargo test" } } ] }),
            json!({ "role": "toolResult", "toolCallId": "a", "content": [{ "type": "text", "text": "test result: ok. 3 passed; 0 failed" }] }),
            json!({ "role": "assistant", "content": [
                { "type": "toolCall", "id": "b", "name": "edit", "arguments": { "path": "src/lib.rs", "oldText": "a", "newText": "b\nc" } },
                { "type": "toolCall", "id": "t", "name": "todo", "arguments": {} },
                { "type": "text", "text": "Fixed: the parser skipped `\\n`." } ] }),
            json!({ "role": "toolResult", "toolCallId": "b", "content": "ok" }),
            json!({ "role": "user", "content": "Thanks<harlo-starter id=\"x\">notes</harlo-starter>".to_string() }),
            json!({ "role": "user", "content": format!("Plan it{}", harlo_harness::file_blocks::PLAN_NOTE) }),
        ];
        let md = markdown(&messages, &Info { title: "Fix it".into(), agent: "Claude".into(), model: "Opus".into(), folder: "~/app".into() });
        assert!(md.starts_with("# Fix it\n\n*Claude · Opus · `~/app` · "), "{md}");
        assert!(md.contains("## You · ") && md.contains("Fix the failing test"));
        assert_eq!(md.matches("## Claude").count(), 1, "one heading for the whole reply: {md}");
        assert!(md.contains("- **bash** `cargo test` → 3 passed\n\n  ```\n  test result: ok. 3 passed; 0 failed\n  ```"), "{md}");
        assert!(md.contains("- **edit** `src/lib.rs` → +2 −1"), "{md}");
        assert!(!md.contains("secret plan") && !md.contains("todo"), "thinking and todo updates stay out");
        assert!(md.contains("\n\nThanks\n") && !md.contains("harlo-starter"), "harlo's notes stay out: {md}");
        assert!(md.ends_with("Plan it\n"), "the plan note stays out: {md}");
    }

    /// How it was written before it streamed, whole in memory: the
    /// streamed Markdown must be the same, byte for byte.
    fn whole_markdown(messages: &[Value], info: &Info) -> String {
        let mut results: std::collections::HashMap<&str, (String, bool)> = std::collections::HashMap::new();
        for m in messages {
            if m["role"] == "toolResult"
                && let Some(id) = m["toolCallId"].as_str()
            {
                results.insert(id, (text_blocks(&m["content"]), m["isError"].as_bool().unwrap_or(false)));
            }
        }
        let first_time = messages.iter().find_map(|m| m["timestamp"].as_i64());
        let mut out = format!("# {}\n\n", if info.title.trim().is_empty() { "Session" } else { info.title.trim() });
        let date = {
            use chrono::TimeZone;
            first_time.and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single()).map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
        };
        let head: Vec<String> = [info.agent.clone(), info.model.clone(), if info.folder.is_empty() { String::new() } else { format!("`{}`", info.folder) }, date]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        if !head.is_empty() {
            let _ = write!(out, "*{}*\n\n", head.join(" · "));
        }
        // The agent's turn: one heading, however many messages it takes.
        let mut in_reply = false;
        for m in messages {
            match m["role"].as_str() {
                Some("user") => {
                    use harlo_harness::file_blocks as fb;
                    let text = text_blocks(&m["content"]);
                    // harlo's own notes riding along with a message stay out
                    // (attached files: their names).
                    let (files, text) = fb::split_blocks(&text);
                    let text = text.split("<harlo-").next().unwrap_or("");
                    let text = fb::strip_plan(fb::strip_ask_first(text).0).0.trim().to_owned();
                    let text = if files.is_empty() { text } else { format!("{text}\n\n*Attached: {}*", files.join(", ")).trim().to_owned() };
                    if text.is_empty() {
                        continue;
                    }
                    let t = time_of(m["timestamp"].as_i64());
                    let _ = write!(out, "---\n\n## You{}\n\n{}\n\n", if t.is_empty() { String::new() } else { format!(" · {t}") }, demote(&text));
                    in_reply = false;
                }
                Some("assistant") => {
                    if !in_reply {
                        let t = time_of(m["timestamp"].as_i64());
                        let _ = write!(out, "## {}{}\n\n", if info.agent.is_empty() { "Agent" } else { &info.agent }, if t.is_empty() { String::new() } else { format!(" · {t}") });
                        in_reply = true;
                    }
                    for b in m["content"].as_array().into_iter().flatten() {
                        match b["type"].as_str() {
                            Some("text") => {
                                let text = demote(b["text"].as_str().unwrap_or("").trim());
                                if !text.is_empty() {
                                    out.push_str(&text);
                                    out.push_str("\n\n");
                                }
                            }
                            Some("toolCall") => out.push_str(&whole_tool_call(b, &results)),
                            _ => {}
                        }
                    }
                    if let Some(e) = m["errorMessage"].as_str().filter(|e| !e.is_empty()) {
                        let _ = write!(out, "> **Error:** {e}\n\n");
                    }
                }
                _ => {}
            }
        }
        out.trim_end().to_owned() + "\n"
    }
    fn whole_tool_call(b: &Value, results: &std::collections::HashMap<&str, (String, bool)>) -> String {
        let name = b["name"].as_str().unwrap_or("tool");
        let kind = crate::actions::kind(name);
        if kind == "todo" {
            return String::new();
        }
        let args = &b["arguments"];
        let log = crate::actions::log(&[serde_json::json!({ "role": "assistant", "content": [b] })]);
        let target = log["actions"][0]["target"].as_str().unwrap_or("").to_owned();
        let (output, error) = b["id"].as_str().and_then(|id| results.get(id)).cloned().unwrap_or_default();
        let (result, _) = crate::tool_facts::facts(name, args, &output, error);
        let mut line = format!("- **{name}**");
        if !target.is_empty() {
            let f = if target.contains('`') { "``" } else { "`" };
            let _ = write!(line, " {f}{target}{f}");
        }
        if error {
            line.push_str(if result.is_empty() || result == "failed" { " → failed".to_owned() } else { format!(" → failed ({result})") }.as_str());
        } else if !result.is_empty() {
            let _ = write!(line, " → {result}");
        }
        line.push('\n');
        // A command's output, when short.
        let output = output.trim_end();
        if kind == "command" && !output.is_empty() && output.lines().count() <= OUTPUT_LINES {
            let f = fence(output);
            let _ = write!(line, "\n  {f}\n{}\n  {f}\n", output.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n"));
        }
        line.push('\n');
        line
    }

    #[test]
    fn streamed_is_the_same_as_whole() {
        let call = |id: &str, name: &str, args: Value| json!({ "type": "toolCall", "id": id, "name": name, "arguments": args });
        let result = |id: &str, text: &str, error: bool| json!({ "role": "toolResult", "toolCallId": id, "isError": error, "content": [{ "type": "text", "text": text }] });
        let messages = vec![
            json!({ "role": "user", "timestamp": 1_760_000_000_000i64, "content": "Go   \n\n" }),
            // A result read before its call.
            result("early", "1 passed", false),
            json!({ "role": "assistant", "timestamp": 1_760_000_001_000i64, "content": [call("early", "bash", json!({ "command": "cargo test" }))] }),
            // Results coming later, one twice (the last counts), one never.
            json!({ "role": "assistant", "content": [
                { "type": "text", "text": "# Plan\n\nfirst" },
                call("a", "bash", json!({ "command": "ls" })),
                call("never", "bash", json!({ "command": "sleep 9" })),
                call("b", "edit", json!({ "path": "a.rs", "oldText": "x", "newText": "y\nz" })),
                call("t", "todo", json!({})),
                { "type": "text", "text": "after the calls  \n" } ] }),
            result("a", "a\nb", false),
            json!({ "role": "user", "content": "" }),
            json!({ "role": "assistant", "content": [{ "type": "text", "text": "between" }], "errorMessage": "overloaded" }),
            result("b", "first try", true),
            result("b", "Edited a.rs", false),
            // Two calls with one id: both see its result.
            json!({ "role": "assistant", "content": [call("dup", "bash", json!({ "command": "echo 1" })), call("dup", "bash", json!({ "command": "echo 2" }))] }),
            result("dup", "ok", false),
            result("orphan", "no call", false),
            json!({ "role": "user", "content": [{ "type": "text", "text": "Thanks" }, { "type": "image", "data": "x" }] }),
            json!({ "role": "assistant", "content": [{ "type": "text", "text": "  \n" }] }),
        ];
        for info in [Info { title: "T".into(), agent: "Claude".into(), model: "Opus".into(), folder: "~/x".into() }, Info::default()] {
            let streamed = markdown(&messages, &info);
            assert_eq!(streamed, whole_markdown(&messages, &info));
            for n in 0..messages.len() {
                assert_eq!(markdown(&messages[..n], &info), whole_markdown(&messages[..n], &info), "the first {n}");
            }
        }
    }

    /// [`markdown`] with what waits spilled to a file in `dir` past `limit`
    /// bytes, and the most it held in memory.
    fn spilled_markdown(messages: &[Value], info: &Info, dir: &std::path::Path, limit: usize) -> (String, usize) {
        TEST_HELD.set((limit, 0));
        let mut out = Vec::new();
        write_markdown_spilling(
            &mut |each| {
                messages.iter().cloned().for_each(&mut *each);
                Ok(())
            },
            info,
            &mut out,
            Some(dir),
        )
        .unwrap();
        let peak = TEST_HELD.get().1;
        TEST_HELD.set((HELD, 0));
        (String::from_utf8(out).unwrap(), peak)
    }

    #[test]
    fn what_waits_for_a_late_result_spills_the_same() {
        let dir = crate::scratch::Scratch::new(std::env::temp_dir().join(format!("harlo-export-spill-{}", std::process::id())));
        std::fs::create_dir_all(&*dir).unwrap();
        let call = |id: &str| json!({ "type": "toolCall", "id": id, "name": "bash", "arguments": { "command": id } });
        let result = |id: &str| json!({ "role": "toolResult", "toolCallId": id, "content": [{ "type": "text", "text": format!("done {id}") }] });
        let mut messages = vec![json!({ "role": "user", "content": "Go" }), json!({ "role": "assistant", "content": [call("late"), call("dup"), call("dup")] })];
        for n in 0..40 {
            messages.push(json!({ "role": "assistant", "content": [{ "type": "text", "text": format!("step {n} {}", "x".repeat(300)) }, call(&format!("c{n}"))] }));
            messages.push(result(&format!("c{n}")));
            if n == 20 {
                messages.push(result("dup"));
            }
        }
        messages.push(result("late"));
        messages.push(json!({ "role": "assistant", "content": [{ "type": "text", "text": "end" }] }));
        let info = Info::default();
        for limit in [0, 1000, HELD] {
            let (spilled, peak) = spilled_markdown(&messages, &info, &dir, limit);
            assert_eq!(spilled, whole_markdown(&messages, &info), "spilling past {limit}");
            if limit < HELD {
                assert!(peak <= limit + 400, "held {peak} with {limit}");
            }
        }
        // (The spill file is gone with the export.)
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn a_late_result_holds_little_of_what_came_between() {
        let dir = crate::scratch::Scratch::new(std::env::temp_dir().join(format!("harlo-export-late-{}", std::process::id())));
        std::fs::create_dir_all(&*dir).unwrap();
        let call = |id: &str| json!({ "type": "toolCall", "id": id, "name": "bash", "arguments": { "command": id } });
        let mut messages = vec![json!({ "role": "assistant", "content": [call("late")] })];
        // 32 MiB of replies and calls that answer at once before it does.
        let chunk = "y".repeat(64 << 10);
        for n in 0..512 {
            messages.push(json!({ "role": "assistant", "content": [{ "type": "text", "text": chunk }, call(&format!("c{n}"))] }));
            messages.push(json!({ "role": "toolResult", "toolCallId": format!("c{n}"), "content": [{ "type": "text", "text": "ok" }] }));
        }
        messages.push(json!({ "role": "toolResult", "toolCallId": "late", "content": [{ "type": "text", "text": "done" }] }));
        let (spilled, peak) = spilled_markdown(&messages, &Info::default(), &dir, HELD);
        assert!(spilled.len() > 32 << 20);
        assert!(peak <= HELD + (128 << 10), "held {peak} bytes");
        assert!(spilled.contains("done"));
    }

    #[test]
    fn headings_go_under_the_turns() {
        assert_eq!(demote("# Plan\ntext\n### Step\n#####x\n```\n# not a heading\n```\n##### Deep"), "### Plan\ntext\n##### Step\n#####x\n```\n# not a heading\n```\n###### Deep");
    }

    #[test]
    fn fences_outrun_backticks() {
        assert_eq!(fence("plain"), "```");
        assert_eq!(fence("a ```` b"), "`````");
    }
}
