//! A session's timeline as the phone shows it: rows of JSON, and the
//! changes that bring a phone's copy up to date.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;

use serde_json::{Value, json};

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

    /// Returns the latest [`PHONE_ROWS`] rows as the phone reads them, and
    /// how many earlier ones it doesn't get.
    #[must_use]
    pub fn for_phone(&self) -> (Vec<Value>, usize) {
        let earlier = self.entries.len().saturating_sub(PHONE_ROWS);
        (self.entries[earlier..].iter().map(row).collect(), earlier)
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
        Entry::Tool {
            key,
            label,
            detail,
            status,
            took,
            files,
            error,
            ..
        } => {
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
            json!({
                "key": key, "kind": "tool", "title": label, "subtitle": detail, "state": state(*status),
                "diff": clip(&diff), "result": result,
                "runMs": took.map_or(0, |t| u64::try_from(t.as_millis()).unwrap_or(u64::MAX)),
            })
        }
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
    }
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
