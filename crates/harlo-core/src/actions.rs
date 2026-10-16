//! A session's action log: every command it ran and every file it read,
//! changed or wrote, other tools too, in order, with when and how each
//! went. Read from the session's messages (tool calls and their results),
//! so it works for every agent and for saved sessions.

use std::collections::BTreeSet;

use serde_json::{Value, json};

/// What kind of action a tool call is, from its name (omp, pi, Claude
/// Code and OpenCode name theirs differently).
pub fn kind(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    let short = lower.rsplit(['.', ':']).next().unwrap_or(&lower).trim_start_matches("mcp__");
    if short.contains("todo") {
        "todo"
    } else if matches!(short, "bash" | "shell" | "exec" | "command" | "run" | "terminal" | "run_command" | "execute") {
        "command"
    } else if short.contains("edit") || short.contains("patch") || short.contains("replace") {
        "edit"
    } else if short == "write" || short.contains("write") || short.contains("create") {
        "write"
    } else if matches!(short, "read" | "view" | "cat" | "readfile" | "read_file" | "file") {
        "read"
    } else if short.contains("web") || short.contains("fetch") || short.contains("url") || short.contains("browse") {
        "web"
    } else if matches!(short, "grep" | "glob" | "find" | "search" | "rg" | "ls" | "list" | "codesearch") {
        "search"
    } else if matches!(short, "task" | "agent" | "subagent" | "delegate") {
        "agent"
    } else {
        "other"
    }
}

fn arg<'a>(args: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter().find_map(|k| args[*k].as_str().filter(|s| !s.is_empty())).unwrap_or("")
}

/// What it acted on: the command, the file, the search, the address.
fn target(kind: &str, args: &Value) -> String {
    const PATH: [&str; 7] = ["path", "file_path", "filePath", "file", "filename", "notebook_path", "target"];
    let s = match kind {
        "command" => arg(args, &["command", "cmd", "script"]),
        "edit" | "write" | "read" => arg(args, &PATH),
        "search" => arg(args, &["pattern", "query", "glob", "regex", "path"]),
        "web" => arg(args, &["url", "query", "q"]),
        "agent" => arg(args, &["description", "task", "prompt", "agent"]),
        _ => "",
    };
    let s = if s.is_empty() && kind == "edit" {
        // A patch: the files it names.
        let patch = arg(args, &["patch", "input", "diff"]);
        let files: Vec<&str> = patch.lines().filter_map(|l| l.strip_prefix("*** Update File: ").or_else(|| l.strip_prefix("*** Add File: ")).or_else(|| l.strip_prefix("+++ b/"))).collect();
        return files.join(", ");
    } else {
        s
    };
    let one_line: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.chars().take(400).collect()
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// The log: `{ actions: [{ ts, kind, tool, target, result, error, pending }],
/// commands, failed, changed: [paths], read: [paths] }`. Todo-list updates
/// are left out.
pub fn log(messages: &[Value]) -> Value {
    let mut results: std::collections::HashMap<&str, (String, bool)> = std::collections::HashMap::new();
    for m in messages {
        if m["role"] == "toolResult"
            && let Some(id) = m["toolCallId"].as_str()
        {
            results.insert(id, (text_of(&m["content"]), m["isError"].as_bool().unwrap_or(false)));
        }
    }
    let mut actions = Vec::new();
    let (mut commands, mut failed) = (0usize, 0usize);
    let (mut changed, mut read) = (BTreeSet::new(), BTreeSet::new());
    for m in messages.iter().filter(|m| m["role"] == "assistant") {
        let ts = m["timestamp"].as_i64();
        for b in m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall") {
            let tool = b["name"].as_str().unwrap_or("tool");
            let kind = kind(tool);
            if kind == "todo" {
                continue;
            }
            let args = &b["arguments"];
            let target = target(kind, args);
            let done = b["id"].as_str().and_then(|id| results.get(id));
            let (output, error) = done.map_or(("", false), |(o, e)| (o.as_str(), *e));
            let result = if done.is_some() { crate::tool_facts::facts(tool, args, output, error).0 } else { String::new() };
            match kind {
                "command" => commands += 1,
                "edit" | "write" if !error && !target.is_empty() => {
                    for p in target.split(", ") {
                        changed.insert(p.to_owned());
                    }
                }
                "read" if !error && !target.is_empty() => {
                    read.insert(target.clone());
                }
                _ => {}
            }
            failed += usize::from(error);
            actions.push(json!({ "ts": ts, "kind": kind, "tool": tool, "target": target, "result": result, "error": error, "pending": done.is_none() }));
        }
    }
    json!({ "actions": actions, "commands": commands, "failed": failed, "changed": changed, "read": read })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_across_agents() {
        for (name, k) in [("bash", "command"), ("Bash", "command"), ("Read", "read"), ("read", "read"), ("Edit", "edit"), ("MultiEdit", "edit"),
                          ("edit", "edit"), ("patch", "edit"), ("Write", "write"), ("write", "write"), ("Grep", "search"), ("glob", "search"),
                          ("WebFetch", "web"), ("webfetch", "web"), ("Task", "agent"), ("task", "agent"), ("TodoWrite", "todo"), ("todo", "todo"),
                          ("mcp__github__create_issue", "write"), ("lsp", "other")] {
            assert_eq!(kind(name), k, "{name}");
        }
    }

    #[test]
    fn logs_calls_with_results() {
        let messages = vec![
            json!({ "role": "user", "content": "fix it" }),
            json!({ "role": "assistant", "timestamp": 1000, "content": [
                { "type": "toolCall", "id": "a", "name": "bash", "arguments": { "command": "cargo  test\n--release" } },
                { "type": "toolCall", "id": "b", "name": "Read", "arguments": { "file_path": "/p/src/lib.rs" } },
                { "type": "toolCall", "id": "t", "name": "todo", "arguments": {} } ] }),
            json!({ "role": "toolResult", "toolCallId": "a", "content": [{ "type": "text", "text": "test result: FAILED. 1 passed; 1 failed" }], "isError": true }),
            json!({ "role": "toolResult", "toolCallId": "b", "content": [{ "type": "text", "text": "a\nb\nc" }] }),
            json!({ "role": "assistant", "timestamp": 2000, "content": [
                { "type": "toolCall", "id": "c", "name": "edit", "arguments": { "path": "src/lib.rs", "oldText": "a", "newText": "b\nc" } },
                { "type": "toolCall", "id": "d", "name": "patch", "arguments": { "patch": "*** Update File: x.rs\n*** Add File: y.rs\n" } },
                { "type": "toolCall", "id": "e", "name": "bash", "arguments": { "command": "ls" } } ] }),
            json!({ "role": "toolResult", "toolCallId": "c", "content": "ok" }),
            json!({ "role": "toolResult", "toolCallId": "d", "content": "ok" }),
        ];
        let l = log(&messages);
        let a = l["actions"].as_array().unwrap();
        assert_eq!(a.len(), 5, "todo updates are left out");
        assert_eq!((a[0]["kind"].as_str(), a[0]["target"].as_str(), a[0]["error"].as_bool()), (Some("command"), Some("cargo test --release"), Some(true)));
        assert_eq!(a[1]["result"], "3 lines");
        assert_eq!((a[2]["target"].as_str(), a[2]["result"].as_str()), (Some("src/lib.rs"), Some("+2 −1")));
        assert_eq!(a[3]["target"], "x.rs, y.rs");
        assert_eq!((a[4]["pending"].as_bool(), a[4]["ts"].as_i64()), (Some(true), Some(2000)), "no result yet: still running");
        assert_eq!((l["commands"].as_u64(), l["failed"].as_u64()), (Some(2), Some(1)));
        assert_eq!(l["changed"], json!(["src/lib.rs", "x.rs", "y.rs"]));
        assert_eq!(l["read"], json!(["/p/src/lib.rs"]));
    }
}
