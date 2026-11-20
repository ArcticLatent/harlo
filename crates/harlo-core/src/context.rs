//! Context report for a session: token usage, message counts, cost and a
//! per-message log, computed from the session's `AgentMessage`s so it works
//! for saved sessions as well as live ones.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Usage {
    input: u64,
    output: u64,
    reasoning: u64,
    cache_read: u64,
    cache_write: u64,
    total: u64,
}

fn usage_of(m: &Value) -> Option<Usage> {
    let u = m.get("usage")?;
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let input = n("input");
    let output = n("output");
    let cache_read = n("cacheRead");
    let cache_write = n("cacheWrite");
    let total = u["totalTokens"].as_u64().unwrap_or(input + output + cache_read + cache_write);
    Some(Usage { input, output, reasoning: n("reasoning"), cache_read, cache_write, total })
}

fn text_len(content: &Value) -> usize {
    match content {
        Value::String(s) => s.chars().count(),
        Value::Array(blocks) => blocks.iter().map(block_len).sum(),
        _ => 0,
    }
}

/// A block's length: its text's characters, a call's arguments as JSON.
fn block_len(b: &Value) -> usize {
    match b["type"].as_str() {
        Some("text") => b["text"].as_str().map_or(0, |s| s.chars().count()),
        Some("thinking") => b["thinking"].as_str().map_or(0, |s| s.chars().count()),
        Some("toolCall") => json_len(&b["arguments"]),
        _ => 0,
    }
}

/// How long `v` is written as JSON (without writing it anywhere).
fn json_len(v: &Value) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut n = Count(0);
    let _ = serde_json::to_writer(&mut n, v);
    n.0
}

fn first_text(content: &Value) -> String {
    let text = match content {
        Value::String(s) => s.as_str(),
        Value::Array(blocks) => blocks.iter().find_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten()).unwrap_or(""),
        _ => "",
    };
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    line.chars().take(160).collect()
}

/// What an assistant message said (its text's first line), or else its
/// first tool call and what on ("bash: ls") with how many calls it made
/// (0 when it said something), else "Thought".
fn reply_parts(content: &Value) -> (String, u64) {
    let text = first_text(content);
    let text = text.trim_start_matches(['#', '>', '-', '*', ' ']).trim();
    if !text.is_empty() {
        return (text.to_owned(), 0);
    }
    let calls: Vec<&Value> = content.as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall").collect();
    if let Some(first) = calls.first() {
        let name = first["name"].as_str().unwrap_or("tool");
        let target = crate::actions::target(crate::actions::kind(name), &first["arguments"]);
        let target: String = target.lines().next().unwrap_or("").chars().take(120).collect();
        return (if target.is_empty() { name.to_owned() } else { format!("{name}: {target}") }, calls.len() as u64);
    }
    (if content.as_array().into_iter().flatten().any(|b| b["type"] == "thinking") { "Thought".to_owned() } else { parts_summary(content) }, 0)
}

/// A row's label for calls that came to `calls`: the first, "and N more".
fn calls_label(first: &str, calls: u64) -> String {
    if calls > 1 { format!("{first} and {} more", calls - 1) } else { first.to_owned() }
}

/// "reasoning + text + bash": what an assistant message did, in order.
fn parts_summary(content: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    for b in content.as_array().into_iter().flatten() {
        let name = match b["type"].as_str() {
            Some("thinking") => "reasoning".to_owned(),
            Some("text") => "text".to_owned(),
            Some("toolCall") => b["name"].as_str().unwrap_or("tool").to_owned(),
            Some("image") => "image".to_owned(),
            _ => continue,
        };
        if !parts.contains(&name) {
            parts.push(name);
        }
    }
    if parts.is_empty() { "empty".to_owned() } else { parts.join(" + ") }
}

/// The latest rows a report lists (all its panel shows).
const SHOWN_ROWS: usize = 300;

/// The report, built a message at a time (a session keeps one, so its
/// panels never read the whole conversation again).
#[derive(Debug, Clone, Default)]
pub struct Tally {
    user: usize,
    assistant: usize,
    tool_results: usize,
    chars_user: usize,
    chars_assistant: usize,
    chars_tools: usize,
    chars_other: usize,
    cost: f64,
    last: Option<Usage>,
    model: String,
    provider: String,
    started: Option<i64>,
    /// The latest `SHOWN_ROWS`, oldest first.
    rows: std::collections::VecDeque<Value>,
    actions: crate::actions::Log,
}

impl Tally {
    /// The tally of a whole conversation.
    pub fn of(messages: &[Value]) -> Self {
        let mut tally = Self::default();
        for m in messages {
            tally.add_counts(m);
        }
        // (Its actions read whole: only the listed ones' results worked out.)
        tally.actions = crate::actions::Log::of(messages);
        tally
    }

    pub fn add(&mut self, m: &Value) {
        self.actions.add(m);
        self.add_counts(m);
    }

    /// `add`, but for the actions.
    fn add_counts(&mut self, m: &Value) {
        let ts = m["timestamp"].as_i64();
        if self.started.is_none() {
            self.started = ts;
        }
        match m["role"].as_str() {
            Some("user") => {
                self.user += 1;
                self.chars_user += text_len(&m["content"]);
                self.rows.push_back(json!({ "kind": "user", "label": first_text(&m["content"]), "ts": ts }));
            }
            Some("assistant") => {
                self.assistant += 1;
                for b in m["content"].as_array().into_iter().flatten() {
                    let n = block_len(b);
                    if b["type"] == "toolCall" { self.chars_tools += n } else { self.chars_assistant += n }
                }
                let usage = usage_of(m);
                if let Some(u) = usage {
                    self.last = Some(u);
                }
                self.cost += m["usage"]["cost"]["total"].as_f64().unwrap_or(0.0);
                if let Some(s) = m["model"].as_str() {
                    s.clone_into(&mut self.model);
                }
                if let Some(s) = m["provider"].as_str() {
                    s.clone_into(&mut self.provider);
                }
                let (first, calls) = reply_parts(&m["content"]);
                // Replies that only called tools, one after another (a dozen
                // helpers started one by one): one row.
                let run = self.rows.back_mut().filter(|r| r["calls"].as_u64().unwrap_or(0) > 0 && r["error"].is_null());
                match run {
                    Some(prev) if calls > 0 && m["errorMessage"].is_null() => {
                        let total = prev["calls"].as_u64().unwrap_or(0) + calls;
                        let add = |a: &Value, b: Option<u64>| match (a.as_u64(), b) {
                            (Some(a), Some(b)) => json!(a + b),
                            (a, b) => json!(a.or(b)),
                        };
                        prev["input"] = add(&prev["input"], usage.map(|u| u.input));
                        prev["output"] = add(&prev["output"], usage.map(|u| u.output));
                        prev["label"] = json!(calls_label(prev["first"].as_str().unwrap_or(""), total));
                        prev["calls"] = json!(total);
                        prev["ts"] = json!(ts);
                    }
                    _ => self.rows.push_back(json!({
                        "kind": "assistant",
                        "label": calls_label(&first, calls),
                        "first": first,
                        "calls": calls,
                        "input": usage.map(|u| u.input),
                        "output": usage.map(|u| u.output),
                        "ts": ts,
                        "error": m["errorMessage"].as_str(),
                    })),
                }
                // A turn that ended without asking for tools: the agent went idle.
                let stop = m["stopReason"].as_str().unwrap_or("");
                if !stop.is_empty() && stop != "toolUse" {
                    self.rows.push_back(json!({ "kind": "idle", "label": if stop == "stop" { "idle".to_owned() } else { stop.to_owned() }, "ts": ts }));
                }
            }
            Some("toolResult") => {
                self.tool_results += 1;
                self.chars_tools += text_len(&m["content"]);
            }
            Some("system") => {}
            _ => {
                self.chars_other += text_len(&m["content"]) + m["summary"].as_str().map_or(0, |s| s.chars().count());
            }
        }
        // (The panel lists the latest: a long session has tens of thousands.)
        while self.rows.len() > SHOWN_ROWS {
            self.rows.pop_front();
        }
    }

    /// The report. `live` carries the harness's own figures when a process
    /// is running (`get_session_stats`): context tokens and window, cost.
    pub fn report(&self, live: Option<(u64, u64, f64)>) -> Value {
        let last = self.last;
        let mut cost = self.cost;
        // Context in use is what the latest request sent plus what it produced.
        let mut context_tokens = last.map_or(0, |u| u.total);
        let mut context_window = 0;
        if let Some((tokens, window, live_cost)) = live {
            if tokens > 0 {
                context_tokens = tokens;
            }
            context_window = window;
            if live_cost > 0.0 {
                cost = live_cost;
            }
        }
        let cache_hit = last.map(|u| {
            let read = u.cache_read as f64;
            let denom = (u.input + u.cache_read + u.cache_write) as f64;
            if denom > 0.0 { read / denom * 100.0 } else { 0.0 }
        });
        let total_chars = (self.chars_user + self.chars_assistant + self.chars_tools + self.chars_other).max(1) as f64;
        let pct = |c: usize| (c as f64 / total_chars * 100.0 * 10.0).round() / 10.0;
        // (Newest first.)
        let rows: Vec<&Value> = self.rows.iter().rev().collect();
        json!({
            "model": self.model,
            "provider": self.provider,
            "started": self.started,
            "contextTokens": context_tokens,
            "contextWindow": context_window,
            "messages": self.user + self.assistant + self.tool_results,
            "user": self.user,
            "assistant": self.assistant,
            "toolResults": self.tool_results,
            "cost": cost,
            "last": last.map(|u| json!({
                "input": u.input, "output": u.output, "reasoning": u.reasoning,
                "cacheRead": u.cache_read, "cacheWrite": u.cache_write, "cacheHit": cache_hit,
            })),
            "breakdown": { "user": pct(self.chars_user), "assistant": pct(self.chars_assistant), "tools": pct(self.chars_tools), "other": pct(self.chars_other) },
            "rows": rows,
            // What it ran and touched (the Activity panel).
            "actions": self.actions.value(),
        })
    }
}

/// The report of a whole conversation (see [`Tally::report`]).
pub fn report(messages: &[Value], live: Option<(u64, u64, f64)>) -> Value {
    Tally::of(messages).report(live)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Built a message at a time, or with some held back and added later,
    /// it's the same report.
    #[test]
    fn a_tally_reads_as_the_whole() {
        let mut messages = Vec::new();
        for i in 0..400 {
            messages.push(json!({ "role": "user", "content": format!("ask {i}"), "timestamp": i }));
            messages.push(json!({ "role": "assistant", "timestamp": i, "model": "m", "usage": { "input": 10, "output": 2, "cost": { "total": 0.01 } }, "content": [
                { "type": "text", "text": "ok" }, { "type": "toolCall", "id": format!("t{i}"), "name": "bash", "arguments": { "command": "ls" } }] }));
            messages.push(json!({ "role": "toolResult", "toolCallId": format!("t{i}"), "content": [{ "type": "text", "text": "a\nb" }] }));
        }
        let whole = report(&messages, None);
        let mut tally = Tally::default();
        for m in &messages[..1000] {
            tally.add(m);
        }
        let mut later = tally.clone();
        for m in &messages[1000..] {
            later.add(m);
        }
        assert_eq!(later.report(None), whole);
        assert_eq!(whole["rows"].as_array().map(Vec::len), Some(SHOWN_ROWS));
        assert_eq!(whole["rows"][0]["label"], "ok", "newest first");
        assert_eq!(whole["messages"], 1200);
        assert_eq!(tally.report(None)["messages"], 1000, "the clone added to, not it");
    }

    #[test]
    fn reports_usage_and_rows() {
        let messages = vec![
            json!({"role":"user","content":"Install Unreal","timestamp":1000}),
            json!({"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"toolCall","name":"bash","arguments":{"command":"ls"}}],
                   "usage":{"input":100,"output":10,"cacheRead":900,"cacheWrite":0,"totalTokens":1010,"cost":{"total":0.01}},
                   "model":"gpt-6.1-sol","provider":"openai","stopReason":"toolUse","timestamp":2000}),
            json!({"role":"toolResult","toolCallId":"c","content":[{"type":"text","text":"file.txt"}],"timestamp":2500}),
            json!({"role":"assistant","content":[{"type":"text","text":"Done."}],
                   "usage":{"input":50,"output":5,"cacheRead":1000,"cacheWrite":0,"totalTokens":1055,"cost":{"total":0.02}},
                   "model":"gpt-6.1-sol","provider":"openai","stopReason":"stop","timestamp":3000}),
        ];
        let r = report(&messages, None);
        assert_eq!((r["user"].as_u64(), r["assistant"].as_u64(), r["messages"].as_u64()), (Some(1), Some(2), Some(4)));
        // Replies that only call tools, one after another: one row.
        let agent = |d: &str| json!({"role":"assistant","content":[{"type":"toolCall","name":"Agent","arguments":{"description":d}}],"stopReason":"toolUse","usage":{"input":10,"output":1}});
        let folded = report(&[json!({"role":"user","content":"go"}), agent("Map the notifications"), agent("Plan the settings page"), agent("Read the docs"),
                              json!({"role":"assistant","content":[{"type":"text","text":"Done."}],"stopReason":"stop"})], None);
        let labels: Vec<&str> = folded["rows"].as_array().unwrap().iter().map(|r| r["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["idle", "Done.", "Agent: Map the notifications and 2 more", "go"]);
        assert_eq!(folded["rows"][2]["input"], 30);
        assert_eq!(r["contextTokens"], 1055);
        assert!((r["cost"].as_f64().unwrap() - 0.03).abs() < 1e-9);
        assert_eq!(r["last"]["cacheRead"], 1000);
        let hit = r["last"]["cacheHit"].as_f64().unwrap();
        assert!((hit - 1000.0 / 1050.0 * 100.0).abs() < 0.01);
        let rows = r["rows"].as_array().unwrap();
        // Newest first: idle, the final text, the tool turn, the prompt.
        let labels: Vec<&str> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["idle", "Done.", "bash: ls", "Install Unreal"]);
        assert_eq!(r["model"], "gpt-6.1-sol");

        let live = report(&messages, Some((5000, 272_000, 0.5)));
        assert_eq!((live["contextTokens"].as_u64(), live["contextWindow"].as_u64()), (Some(5000), Some(272_000)));
    }
}
