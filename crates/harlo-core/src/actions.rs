//! A session's action log: every command it ran and every file it read,
//! changed or wrote, other tools too, in order, with when and how each
//! went. Read from the session's messages (tool calls and their results),
//! so it works for every agent and for saved sessions.


use serde_json::{Value, json};

/// What kind of action a tool call is, from its name (Claude Code and
/// Harlo name theirs differently).
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
pub(crate) fn target(kind: &str, args: &Value) -> String {
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

/// The latest actions a log lists (a long session has tens of thousands;
/// its totals count them all).
pub const KEPT: usize = 2000;

/// A call waiting for its result (what its result is read with).
#[derive(Clone)]
struct Pending {
    /// Its place among all the actions.
    index: usize,
    tool: String,
    args: Value,
    kind: &'static str,
    target: String,
}

/// The log, built a message at a time.
#[derive(Clone, Default)]
pub struct Log {
    /// The latest `KEPT` actions, oldest first.
    actions: std::collections::VecDeque<Value>,
    total: usize,
    pending: std::collections::HashMap<String, Pending>,
    /// Results whose call hasn't ended yet (an agent may end the message
    /// with the call after its result), the latest few.
    early: std::collections::VecDeque<(String, String, bool)>,
    commands: usize,
    failed: usize,
    /// Paths and how many calls (not failed) changed or read each.
    changed: std::collections::BTreeMap<String, usize>,
    read: std::collections::BTreeMap<String, usize>,
    kinds: std::collections::BTreeMap<&'static str, usize>,
    /// Actions before this one won't be listed (a whole conversation is
    /// being read): their results aren't worked out, only counted.
    listed_from: usize,
}

impl Log {
    /// For a whole conversation: only its latest actions' results are
    /// worked out (those listed).
    pub fn of(messages: &[Value]) -> Self {
        let calls = messages
            .iter()
            .filter(|m| m["role"] == "assistant")
            .flat_map(|m| m["content"].as_array().into_iter().flatten())
            .filter(|b| b["type"] == "toolCall" && kind(b["name"].as_str().unwrap_or("tool")) != "todo")
            .count();
        let mut log = Self { listed_from: calls.saturating_sub(KEPT), ..Self::default() };
        for m in messages {
            log.add(m);
        }
        log
    }

    pub fn add(&mut self, m: &Value) {
        match m["role"].as_str() {
            Some("assistant") => {
                let ts = m["timestamp"].as_i64();
                for b in m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall") {
                    let tool = b["name"].as_str().unwrap_or("tool");
                    let kind = kind(tool);
                    if kind == "todo" {
                        continue;
                    }
                    let args = &b["arguments"];
                    let target = target(kind, args);
                    // (Until its result says otherwise, it went fine.)
                    self.count(kind, &target, 1);
                    *self.kinds.entry(kind).or_default() += 1;
                    if kind == "command" {
                        self.commands += 1;
                    }
                    let id = b["id"].as_str().unwrap_or_default();
                    if !id.is_empty() {
                        // (Its arguments only if its result will be worked out.)
                        let args = if self.total >= self.listed_from { args.clone() } else { Value::Null };
                        self.pending.insert(id.to_owned(), Pending { index: self.total, tool: tool.to_owned(), args, kind, target: target.clone() });
                    }
                    self.actions.push_back(json!({ "ts": ts, "kind": kind, "tool": tool, "target": target, "result": "", "error": false, "pending": true }));
                    self.total += 1;
                    if self.actions.len() > KEPT {
                        self.actions.pop_front();
                    }
                    if let Some(i) = self.early.iter().position(|(e, _, _)| !id.is_empty() && e == id) {
                        let (_, output, error) = self.early.remove(i).unwrap_or_default();
                        self.result(id, &output, error);
                    }
                }
                // (Calls that never got a result, long ago: let go.)
                if self.pending.len() > KEPT {
                    let oldest = self.total.saturating_sub(KEPT);
                    self.pending.retain(|_, p| p.index >= oldest);
                }
            }
            Some("toolResult") => {
                let Some(id) = m["toolCallId"].as_str() else { return };
                let (output, error) = (text_of(&m["content"]), m["isError"].as_bool().unwrap_or(false));
                if self.pending.contains_key(id) {
                    self.result(id, &output, error);
                } else {
                    self.early.retain(|(e, _, _)| e != id);
                    self.early.push_back((id.to_owned(), output, error));
                    if self.early.len() > 64 {
                        self.early.pop_front();
                    }
                }
            }
            _ => {}
        }
    }

    /// Call `id`'s result.
    fn result(&mut self, id: &str, output: &str, error: bool) {
        let Some(p) = self.pending.remove(id) else { return };
        let first = self.total - self.actions.len();
        let listed = p.index >= self.listed_from.max(first);
        let result = if listed { crate::tool_facts::facts(&p.tool, &p.args, output, error).0 } else { String::new() };
        if error {
            self.failed += 1;
            self.count(p.kind, &p.target, -1);
        }
        if let Some(a) = p.index.checked_sub(first).and_then(|i| self.actions.get_mut(i)).filter(|_| listed) {
            a["result"] = json!(result);
            a["error"] = json!(error);
            a["pending"] = json!(false);
        }
    }

    /// A file it changed or read, counted in (1) or out (-1).
    fn count(&mut self, kind: &str, target: &str, by: isize) {
        if target.is_empty() {
            return;
        }
        let (map, paths): (_, Vec<&str>) = match kind {
            "edit" | "write" => (&mut self.changed, target.split(", ").collect()),
            "read" => (&mut self.read, vec![target]),
            _ => return,
        };
        for p in paths {
            let n = map.entry(p.to_owned()).or_default();
            *n = n.saturating_add_signed(by);
            if *n == 0 {
                map.remove(p);
            }
        }
    }

    /// `{ actions: [{ ts, kind, tool, target, result, error, pending }]
    /// (the latest KEPT), total, kinds: {kind: n}, commands, failed,
    /// changed: [paths], read: [paths] }`. Todo-list updates are left out.
    pub fn value(&self) -> Value {
        json!({
            "actions": self.actions,
            "total": self.total,
            "kinds": self.kinds,
            "commands": self.commands,
            "failed": self.failed,
            "changed": self.changed.keys().collect::<Vec<_>>(),
            "read": self.read.keys().collect::<Vec<_>>(),
        })
    }
}

/// The log of a whole conversation (see [`Log::value`]).
pub fn log(messages: &[Value]) -> Value {
    Log::of(messages).value()
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

    /// A long session: the latest KEPT actions, every one counted; a result
    /// for one no longer listed still counts.
    #[test]
    fn keeps_the_latest_and_counts_them_all() {
        let mut log = Log::default();
        let n = KEPT + 500;
        for i in 0..n {
            let (tool, args) = if i % 2 == 0 { ("bash", json!({ "command": format!("echo {i}") })) } else { ("edit", json!({ "path": format!("f{}.rs", i % 7), "oldText": "a", "newText": "b" })) };
            log.add(&json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": format!("c{i}"), "name": tool, "arguments": args }] }));
            // (All answered at once but the first edit and the last two.)
            if i != 1 && i < n - 2 {
                log.add(&json!({ "role": "toolResult", "toolCallId": format!("c{i}"), "content": "done" }));
            }
        }
        // The first edit failed (its action is long gone from the list).
        log.add(&json!({ "role": "toolResult", "toolCallId": "c1", "content": "no such text", "isError": true }));
        log.add(&json!({ "role": "toolResult", "toolCallId": format!("c{}", n - 2), "content": "done" }));
        let v = log.value();
        let a = v["actions"].as_array().unwrap();
        assert_eq!((a.len(), v["total"].as_u64()), (KEPT, Some(n as u64)));
        assert_eq!(a.last().unwrap()["target"], format!("f{}.rs", (n - 1) % 7));
        assert_eq!(a[a.len() - 2]["pending"], false, "the latest results land in their row");
        assert_eq!((v["commands"].as_u64(), v["failed"].as_u64()), (Some(n as u64 / 2), Some(1)));
        assert_eq!(v["kinds"], json!({ "command": n / 2, "edit": n / 2 }));
        assert_eq!(v["changed"].as_array().unwrap().len(), 7, "f1.rs is still changed by later edits");
        // Read whole (only the listed results worked out), it's the same.
        let mut messages = Vec::new();
        for i in 0..n {
            messages.push(json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": format!("w{i}"), "name": "edit", "arguments": { "path": format!("f{}.rs", i % 7), "oldText": "a", "newText": "b\nc" } }] }));
            messages.push(json!({ "role": "toolResult", "toolCallId": format!("w{i}"), "content": "ok", "isError": i % 100 == 0 }));
        }
        let mut one_by_one = Log::default();
        messages.iter().for_each(|m| one_by_one.add(m));
        assert_eq!(Log::of(&messages).value(), one_by_one.value());
        assert_eq!(Log::of(&messages).value()["actions"][0]["result"], "+2 −1");
        // A result before the message with its call (as some agents end them).
        let mut early = Log::default();
        early.add(&json!({ "role": "toolResult", "toolCallId": "r", "content": "test result: ok. 12 passed; 0 failed" }));
        early.add(&json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": "r", "name": "bash", "arguments": { "command": "cargo test" } }, { "type": "text", "text": "done" }] }));
        assert_eq!((early.value()["actions"][0]["result"].as_str(), early.value()["actions"][0]["pending"].as_bool()), (Some("12 passed"), Some(false)));
        // An edit that failed and was the only one on its file: not changed.
        let mut one = Log::default();
        one.add(&json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": "x", "name": "edit", "arguments": { "path": "x.rs" } }] }));
        assert_eq!(one.value()["changed"], json!(["x.rs"]), "until it's known, it went fine");
        one.add(&json!({ "role": "toolResult", "toolCallId": "x", "content": "failed", "isError": true }));
        assert_eq!(one.value()["changed"], json!([]));
    }
}
