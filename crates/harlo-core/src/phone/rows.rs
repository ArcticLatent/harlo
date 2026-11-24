//! A session's timeline as the phone shows it: rows of JSON, and the
//! changes that bring a phone's copy up to date.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::server::{ShellRun, ShellStatus};
use crate::timeline::{Change, Compaction, Entry, Live};
use crate::workspace::{EventKind, Message, ToolStatus};

/// Rows a phone gets of a session: the latest ones. A long session's whole
/// timeline is far too much for a phone, which refuses WebSocket messages
/// over 1 MB.
pub const PHONE_ROWS: usize = 300;

/// A row's text or HTML on a phone, at most (a huge tool output).
const ROW_BYTES: usize = 64 * 1024;

/// One session's rows, kept current by its live events.
#[derive(Debug, Default)]
pub struct Rows {
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    live: Live,
}

impl Rows {
    /// Starts from a session's stored messages.
    #[must_use]
    pub fn from_messages(messages: &[Message]) -> Self {
        let mut rows = Self::default();
        rows.reload(messages);
        rows
    }

    /// Replaces the rows with a session's stored messages; what's in flight
    /// is kept, as the stream goes on.
    pub fn reload(&mut self, messages: &[Message]) {
        self.entries = crate::timeline::entries(messages);
        self.reindex();
    }

    fn reindex(&mut self) {
        self.index = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key().to_owned(), i))
            .collect();
    }

    fn push(&mut self, entry: Entry) {
        self.index
            .insert(entry.key().to_owned(), self.entries.len());
        self.entries.push(entry);
    }

    /// Applies one of the session's live events; returns whether the
    /// history should be reloaded.
    pub fn apply(&mut self, kind: &EventKind, time: f64) -> bool {
        let mut reload = false;
        for change in self.live.apply(kind, time) {
            match change {
                Change::Put(entry) => match self.index.get(entry.key()) {
                    Some(&at) => self.entries[at] = entry,
                    None => self.push(entry),
                },
                Change::Extend {
                    key,
                    thought,
                    delta,
                } => match self.index.get(&key).copied() {
                    Some(at) => match &mut self.entries[at] {
                        Entry::Answer { text, .. } | Entry::Thought { text, .. } => {
                            text.push_str(&delta);
                        }
                        _ => reload = true,
                    },
                    None if thought => self.push(Entry::Thought { key, text: delta }),
                    None => self.push(Entry::Answer { key, text: delta }),
                },
                Change::Reload => reload = true,
            }
        }
        reload
    }

    /// Returns the session's timeline.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Returns the latest [`PHONE_ROWS`] rows as the phone reads them, and
    /// how many earlier ones it doesn't get.
    #[must_use]
    pub fn for_phone(&self) -> (Vec<Value>, usize) {
        let earlier = self.entries.len().saturating_sub(PHONE_ROWS);
        let mut rows: Vec<Value> = self.entries[earlier..].iter().map(row).collect();
        group_runs(&mut rows);
        (rows, earlier)
    }
}

/// Cuts text to [`ROW_BYTES`], on a character boundary.
fn clip(text: &str) -> String {
    if text.len() <= ROW_BYTES {
        return text.to_owned();
    }
    let mut end = ROW_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n… (the rest is in Harlo on your computer)",
        &text[..end]
    )
}

/// Returns Markdown as HTML, as the phone renders answers.
#[must_use]
pub fn markdown_html(text: &str) -> String {
    use pulldown_cmark::{Options, Parser, html};
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let mut out = String::with_capacity(text.len() * 3 / 2);
    html::push_html(&mut out, Parser::new_ext(text, options));
    out
}

fn state(status: ToolStatus) -> &'static str {
    match status {
        ToolStatus::Streaming | ToolStatus::Running => "running",
        ToolStatus::Completed => "done",
        ToolStatus::Failed => "error",
    }
}

/// A notice row: `level` is `info` or `warning`.
fn notice(key: &str, level: &str, text: &str) -> Value {
    json!({ "key": key, "kind": "notice", "title": level, "text": text, "label": text })
}

/// A shell command the user ran, as the phone shows a tool: the command,
/// how it ended, and what it printed.
fn shell_row(key: &str, run: &ShellRun) -> Value {
    let (state, result) = match (run.status, run.exit) {
        (ShellStatus::Running, _) => ("running", String::new()),
        (ShellStatus::Exited, Some(0) | None) => ("done", String::new()),
        (ShellStatus::Exited, Some(code)) => ("error", format!("Exit {code}")),
        (ShellStatus::Timeout, _) => ("error", "Timed out".to_owned()),
        (ShellStatus::Killed, _) => ("error", "Stopped".to_owned()),
    };
    json!({
        "key": key, "kind": "tool", "title": "Ran", "subtitle": run.command,
        "state": state, "result": result, "output": clip(&run.output),
    })
}

/// Returns one row as the phone reads it (the phone's `Row`).
#[must_use]
pub fn row(entry: &Entry) -> Value {
    match entry {
        Entry::Prompt {
            key,
            text,
            attachments,
            ..
        } => {
            let text = if attachments.is_empty() {
                text.clone()
            } else {
                format!("{text}\n\n{}", attachments.join(", "))
            };
            json!({ "key": key, "kind": "user", "text": clip(&text), "state": "done" })
        }
        Entry::Answer { key, text } => json!({
            "key": key, "kind": "markdown", "text": clip(text), "html": clip(&markdown_html(text)),
            "state": "done",
        }),
        Entry::Thought { key, text } => {
            json!({ "key": key, "kind": "thinking", "text": clip(text), "state": "done" })
        }
        Entry::Tool { .. } => tool_row(entry),
        Entry::Subagent {
            key,
            agent,
            task,
            status,
            ..
        } => json!({
            "key": key, "kind": "tool", "title": format!("Subagent: {agent}"), "subtitle": task,
            "state": state(*status),
        }),
        Entry::Report { key, report } => {
            let text = format!(
                "**{}** reported: {}\n\n{}",
                report.agent, report.task, report.text
            );
            json!({ "key": key, "kind": "markdown", "text": clip(&text), "html": clip(&markdown_html(&text)), "state": "done" })
        }
        Entry::Stopped {
            key,
            outcome,
            error,
        } => {
            let text = match (outcome.as_str(), error) {
                ("interrupted", _) => "Stopped.".to_owned(),
                (_, Some(why)) => format!("The run failed: {why}"),
                (_, None) => "The run failed.".to_owned(),
            };
            notice(key, "warning", &text)
        }
        Entry::Compaction { key, state } => match state {
            Compaction::Running => notice(key, "info", "Summarizing the conversation…"),
            Compaction::Done(_) => notice(key, "info", "Summarized the conversation."),
            Compaction::Failed(why) => notice(
                key,
                "warning",
                &format!("Couldn't summarize the conversation: {why}"),
            ),
        },
        Entry::Note { key, text, warning } => {
            notice(key, if *warning { "warning" } else { "info" }, text)
        }
        Entry::Shell { key, run } => shell_row(key, run),
    }
}

/// A tool call's row: what it did, its diff and how long it took.
fn tool_row(entry: &Entry) -> Value {
    let Entry::Tool {
        key,
        name,
        detail,
        status,
        took,
        files,
        error,
        ..
    } = entry
    else {
        return Value::Null;
    };
    let diff: String = files
        .iter()
        .map(|f| f.patch.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let (added, removed) = files
        .iter()
        .fold((0, 0), |(a, r), f| (a + f.additions, r + f.deletions));
    let result = if let Some(why) = error {
        why.clone()
    } else if files.is_empty() {
        String::new()
    } else {
        format!("+{added} −{removed}")
    };
    let command = if is_command(name) {
        detail.as_str()
    } else {
        ""
    };
    json!({
        "key": key, "kind": "tool", "tool": name, "title": name, "subtitle": detail,
        "command": command, "state": state(*status), "diff": clip(&diff), "result": result,
        "runMs": took.map_or(0, |t| u64::try_from(t.as_millis()).unwrap_or(u64::MAX)),
    })
}

/// Whether tool `name` runs a command.
fn is_command(name: &str) -> bool {
    matches!(name, "bash" | "shell")
}

/// Folds each run of two or more tool calls in a row (with the model's
/// thoughts between them) under one group, as the phone shows it: the
/// group is its first row's key, which carries the run's summary ("Read 1
/// file, ran 2 commands") and, once it's done, how long it took; the run
/// stays open while a call in it runs.
fn group_runs(rows: &mut [Value]) {
    let mut i = 0;
    while i < rows.len() {
        if rows[i]["kind"] != "tool" {
            i += 1;
            continue;
        }
        // The run: tools, and thoughts followed by another tool.
        let mut end = i + 1;
        let mut last_tool = i;
        while end < rows.len() {
            match rows[end]["kind"].as_str() {
                Some("tool") => last_tool = end,
                Some("thinking") => {}
                _ => break,
            }
            end += 1;
        }
        let run = &mut rows[i..=last_tool];
        let tools = run.iter().filter(|r| r["kind"] == "tool").count();
        if tools >= 2 {
            let group = run[0]["key"].clone();
            let open = run.iter().any(|r| r["state"] == "running");
            let took: u64 = run.iter().filter_map(|r| r["runMs"].as_u64()).sum();
            for r in run.iter_mut() {
                r["group"] = group.clone();
                r["groupOpen"] = Value::Bool(open);
            }
            run[0]["summary"] = Value::String(summary(run));
            if !open {
                run[0]["runMs"] = Value::from(took);
            }
        }
        i = last_tool + 1;
    }
}

/// A run's tool calls in words: "Read 2 files, edited 1 file, ran 1 command".
fn summary(run: &[Value]) -> String {
    let (mut read, mut edited, mut ran, mut searched, mut other) = (0, 0, 0, 0, 0);
    for r in run.iter().filter(|r| r["kind"] == "tool") {
        match r["tool"].as_str().unwrap_or_default() {
            "read" => read += 1,
            "edit" | "write" | "apply_patch" | "multiedit" => edited += 1,
            name if is_command(name) => ran += 1,
            "grep" | "glob" | "list" | "codesearch" => searched += 1,
            _ => other += 1,
        }
    }
    let count =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let parts: Vec<String> = [
        (read, "read", ("file", "files")),
        (edited, "edited", ("file", "files")),
        (ran, "ran", ("command", "commands")),
        (searched, "searched", ("time", "times")),
        (other, "used", ("other tool", "other tools")),
    ]
    .into_iter()
    .filter(|(n, ..)| *n > 0)
    .map(|(n, verb, (one, many))| format!("{verb} {}", count(n, one, many)))
    .collect();
    let mut text = parts.join(", ");
    if let Some(first) = text.get(..1) {
        text.replace_range(..1, &first.to_uppercase());
    }
    text
}

/// Returns the changes that turn the phone's rows `sent` into `now`
/// (`{ op: update | remove | insert, at, … }`), as few as it takes when only
/// the end changed.
#[must_use]
pub fn changes(sent: &[Value], now: &[Value]) -> Vec<Value> {
    let same = sent.iter().zip(now).take_while(|(a, b)| a == b).count();
    let mut out = Vec::new();
    let mut at = same;
    // Rows that stayed in place, changed.
    while at < sent.len() && at < now.len() && sent[at]["key"] == now[at]["key"] {
        if sent[at] != now[at] {
            out.push(json!({ "op": "update", "at": at, "row": now[at] }));
        }
        at += 1;
    }
    if at < sent.len() {
        out.push(json!({ "op": "remove", "at": at, "count": sent.len() - at }));
    }
    if at < now.len() {
        out.push(json!({ "op": "insert", "at": at, "rows": now[at..] }));
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    fn tool(key: &str, name: &str, state: &str, ms: u64) -> Value {
        json!({ "key": key, "kind": "tool", "tool": name, "state": state, "runMs": ms })
    }

    #[test]
    fn runs_of_tool_calls_fold_under_their_summary() {
        let mut rows = vec![
            json!({ "key": "u", "kind": "user" }),
            json!({ "key": "t0", "kind": "thinking" }),
            tool("a", "read", "done", 10),
            json!({ "key": "t1", "kind": "thinking" }),
            tool("b", "grep", "done", 5),
            tool("c", "edit", "done", 1),
            tool("d", "shell", "done", 100),
            tool("e", "shell", "done", 20),
            json!({ "key": "m", "kind": "markdown" }),
            tool("f", "read", "done", 3),
            json!({ "key": "n", "kind": "markdown" }),
            tool("g", "read", "done", 1),
            tool("h", "write", "running", 0),
        ];
        super::group_runs(&mut rows);
        let groups: Vec<&str> = rows
            .iter()
            .map(|r| r["group"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(
            groups,
            ["", "", "a", "a", "a", "a", "a", "a", "", "", "", "g", "g"]
        );
        assert_eq!(
            rows[2]["summary"],
            "Read 1 file, edited 1 file, ran 2 commands, searched 1 time"
        );
        assert_eq!(rows[2]["runMs"], 136);
        assert_eq!(rows[2]["groupOpen"], false);
        // A lone call stays on its own; a run still working stays open.
        assert!(rows[9].get("summary").is_none());
        assert_eq!(rows[11]["groupOpen"], true);
        assert_eq!(rows[11]["summary"], "Read 1 file, edited 1 file");
    }
}
