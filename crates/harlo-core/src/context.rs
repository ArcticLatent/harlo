//! Context report for a session: token usage, message counts, cost and a
//! per-message log, computed from the session's `AgentMessage`s so it works
//! for saved sessions as well as live ones.

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
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b["type"].as_str() {
                Some("text") => b["text"].as_str().map_or(0, |s| s.chars().count()),
                Some("thinking") => b["thinking"].as_str().map_or(0, |s| s.chars().count()),
                Some("toolCall") => b["arguments"].to_string().len(),
                _ => 0,
            })
            .sum(),
        _ => 0,
    }
}

fn first_text(content: &Value) -> String {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().find_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten()).unwrap_or("").to_owned(),
        _ => String::new(),
    };
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    line.chars().take(160).collect()
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

/// Builds the report. `live` carries the harness's own figures when a
/// process is running (`get_session_stats`): context tokens and window, cost.
pub fn report(messages: &[Value], live: Option<(u64, u64, f64)>) -> Value {
    let (mut user, mut assistant, mut tool_results) = (0usize, 0usize, 0usize);
    let (mut chars_user, mut chars_assistant, mut chars_tools, mut chars_other) = (0usize, 0usize, 0usize, 0usize);
    let mut cost = 0.0f64;
    let mut last: Option<Usage> = None;
    let mut model = String::new();
    let mut provider = String::new();
    let mut started: Option<i64> = None;
    let mut rows: Vec<Value> = Vec::new();

    for m in messages {
        let ts = m["timestamp"].as_i64();
        if started.is_none() {
            started = ts;
        }
        match m["role"].as_str() {
            Some("user") => {
                user += 1;
                chars_user += text_len(&m["content"]);
                rows.push(json!({ "kind": "user", "label": first_text(&m["content"]), "ts": ts }));
            }
            Some("assistant") => {
                assistant += 1;
                for b in m["content"].as_array().into_iter().flatten() {
                    let n = text_len(&Value::Array(vec![b.clone()]));
                    if b["type"] == "toolCall" { chars_tools += n } else { chars_assistant += n }
                }
                let usage = usage_of(m);
                if let Some(u) = usage {
                    last = Some(u);
                }
                cost += m["usage"]["cost"]["total"].as_f64().unwrap_or(0.0);
                if let Some(s) = m["model"].as_str() {
                    model = s.to_owned();
                }
                if let Some(s) = m["provider"].as_str() {
                    provider = s.to_owned();
                }
                rows.push(json!({
                    "kind": "assistant",
                    "label": parts_summary(&m["content"]),
                    "input": usage.map(|u| u.input),
                    "output": usage.map(|u| u.output),
                    "ts": ts,
                    "error": m["errorMessage"].as_str(),
                }));
                // A turn that ended without asking for tools: the agent went idle.
                let stop = m["stopReason"].as_str().unwrap_or("");
                if !stop.is_empty() && stop != "toolUse" {
                    rows.push(json!({ "kind": "idle", "label": if stop == "stop" { "idle".to_owned() } else { stop.to_owned() }, "ts": ts }));
                }
            }
            Some("toolResult") => {
                tool_results += 1;
                chars_tools += text_len(&m["content"]);
            }
            Some("system") => {}
            _ => {
                chars_other += text_len(&m["content"]) + m["summary"].as_str().map_or(0, |s| s.chars().count());
            }
        }
    }

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
    let total_chars = (chars_user + chars_assistant + chars_tools + chars_other).max(1) as f64;
    let pct = |c: usize| (c as f64 / total_chars * 100.0 * 10.0).round() / 10.0;
    rows.reverse();

    json!({
        "model": model,
        "provider": provider,
        "started": started,
        "contextTokens": context_tokens,
        "contextWindow": context_window,
        "messages": user + assistant + tool_results,
        "user": user,
        "assistant": assistant,
        "toolResults": tool_results,
        "cost": cost,
        "last": last.map(|u| json!({
            "input": u.input, "output": u.output, "reasoning": u.reasoning,
            "cacheRead": u.cache_read, "cacheWrite": u.cache_write, "cacheHit": cache_hit,
        })),
        "breakdown": { "user": pct(chars_user), "assistant": pct(chars_assistant), "tools": pct(chars_tools), "other": pct(chars_other) },
        "rows": rows,
        // What it ran and touched (the Activity panel).
        "actions": crate::actions::log(messages),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(r["contextTokens"], 1055);
        assert!((r["cost"].as_f64().unwrap() - 0.03).abs() < 1e-9);
        assert_eq!(r["last"]["cacheRead"], 1000);
        let hit = r["last"]["cacheHit"].as_f64().unwrap();
        assert!((hit - 1000.0 / 1050.0 * 100.0).abs() < 0.01);
        let rows = r["rows"].as_array().unwrap();
        // Newest first: idle, the final text, the tool turn, the prompt.
        let labels: Vec<&str> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["idle", "text", "reasoning + bash", "Install Unreal"]);
        assert_eq!(r["model"], "gpt-6.1-sol");

        let live = report(&messages, Some((5000, 272_000, 0.5)));
        assert_eq!((live["contextTokens"].as_u64(), live["contextWindow"].as_u64()), (Some(5000), Some(272_000)));
    }
}
