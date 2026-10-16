//! A session as Markdown (Export / Copy as Markdown, the phone's Share):
//! a title and what it ran on, then each of your messages and each reply,
//! with the agent's tool calls in brief (a command's short output, an
//! edit's lines added and removed). Thinking is left out. From the
//! session's messages, so every agent and saved sessions too.

use serde_json::Value;

/// About the session, for the head.
#[derive(Debug, Clone, Default)]
pub struct Info {
    pub title: String,
    /// "Claude", "Oh My Pi"…
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
pub fn markdown(messages: &[Value], info: &Info) -> String {
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
        out.push_str(&format!("*{}*\n\n", head.join(" · ")));
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
                out.push_str(&format!("---\n\n## You{}\n\n{}\n\n", if t.is_empty() { String::new() } else { format!(" · {t}") }, demote(&text)));
                in_reply = false;
            }
            Some("assistant") => {
                if !in_reply {
                    let t = time_of(m["timestamp"].as_i64());
                    out.push_str(&format!("## {}{}\n\n", if info.agent.is_empty() { "Agent" } else { &info.agent }, if t.is_empty() { String::new() } else { format!(" · {t}") }));
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
                        Some("toolCall") => out.push_str(&tool_call(b, &results)),
                        _ => {}
                    }
                }
                if let Some(e) = m["errorMessage"].as_str().filter(|e| !e.is_empty()) {
                    out.push_str(&format!("> **Error:** {e}\n\n"));
                }
            }
            _ => {}
        }
    }
    out.trim_end().to_owned() + "\n"
}

/// "- **bash** `cargo test` → 12 passed", and a short output.
fn tool_call(b: &Value, results: &std::collections::HashMap<&str, (String, bool)>) -> String {
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
        line.push_str(&format!(" {f}{target}{f}"));
    }
    if error {
        line.push_str(if result.is_empty() || result == "failed" { " → failed".to_owned() } else { format!(" → failed ({result})") }.as_str());
    } else if !result.is_empty() {
        line.push_str(&format!(" → {result}"));
    }
    line.push('\n');
    // A command's output, when short.
    let output = output.trim_end();
    if kind == "command" && !output.is_empty() && output.lines().count() <= OUTPUT_LINES {
        let f = fence(output);
        line.push_str(&format!("\n  {f}\n{}\n  {f}\n", output.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n")));
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
            json!({ "role": "user", "content": format!("Thanks<harlo-starter id=\"x\">notes</harlo-starter>") }),
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
