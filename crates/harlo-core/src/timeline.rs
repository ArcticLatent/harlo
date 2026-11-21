//! A session's timeline as rows: prompts, answers, thoughts, tool calls.
//!
//! [`entries`] flattens OpenCode's messages into the rows the timeline shows,
//! oldest first. Each row has a stable key, so a reloaded timeline keeps the
//! rows (and their measured heights) it already had.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use opencode_client::{Message, Part, ToolCall, ToolStatus};
use serde_json::Value;

/// Characters a tool detail or thought summary keeps before "…".
const SUMMARY_CHARS: usize = 120;

/// One row in a session's timeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// Something the user sent.
    Prompt {
        /// Stable row key.
        key: String,
        /// The prompt text.
        text: String,
    },
    /// Answer text from the model, as Markdown.
    Answer {
        /// Stable row key.
        key: String,
        /// The Markdown text.
        text: String,
    },
    /// The model's reasoning, shown as one line.
    Thought {
        /// Stable row key.
        key: String,
        /// The reasoning's first line, without Markdown markers.
        summary: String,
    },
    /// A tool call, shown as one line.
    Tool {
        /// Stable row key.
        key: String,
        /// The tool's name for people, such as `Edit` or `Bash`.
        label: String,
        /// The call's main argument, such as a path or a command.
        detail: String,
        /// Where the call is.
        status: ToolStatus,
        /// How long it took, once finished.
        took: Option<Duration>,
    },
    /// A run that didn't succeed.
    Stopped {
        /// Stable row key.
        key: String,
        /// `failed` or `interrupted`.
        outcome: String,
    },
}

impl Entry {
    /// The row's stable key.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Self::Prompt { key, .. }
            | Self::Answer { key, .. }
            | Self::Thought { key, .. }
            | Self::Tool { key, .. }
            | Self::Stopped { key, .. } => key,
        }
    }
}

/// Turns a session's messages into timeline rows, oldest first.
///
/// Empty text and reasoning are skipped, as are message kinds the timeline
/// doesn't show yet (agent switches, compaction) and successful run ends.
#[must_use]
pub fn entries(messages: &[Message]) -> Vec<Entry> {
    let mut rows = Vec::new();
    for message in messages {
        match message {
            Message::User { id, text } => rows.push(Entry::Prompt {
                key: id.clone(),
                text: text.clone(),
            }),
            Message::Assistant { id, parts, .. } => {
                for (index, part) in parts.iter().enumerate() {
                    let key = format!("{id}/{index}");
                    match part {
                        Part::Text(text) if !text.trim().is_empty() => rows.push(Entry::Answer {
                            key,
                            text: text.clone(),
                        }),
                        Part::Reasoning(text) if !text.trim().is_empty() => {
                            rows.push(Entry::Thought {
                                key,
                                summary: summarize(text),
                            });
                        }
                        Part::Tool(call) => rows.push(tool(key, call)),
                        Part::Text(_) | Part::Reasoning(_) => {}
                    }
                }
            }
            Message::Idle { id, outcome } if outcome != "succeeded" => {
                rows.push(Entry::Stopped {
                    key: id.clone(),
                    outcome: outcome.clone(),
                });
            }
            Message::Idle { .. } | Message::Other { .. } => {}
        }
    }
    rows
}

fn tool(key: String, call: &ToolCall) -> Entry {
    Entry::Tool {
        key,
        label: tool_label(&call.name),
        detail: shorten(&tool_detail(&call.name, &call.input)),
        status: call.status,
        took: call.took_ms.map(|ms| Duration::from_secs_f64(ms / 1000.0)),
    }
}

/// The name people see for a tool: `edit` becomes `Edit`, `todowrite` becomes `Todo`.
#[must_use]
pub fn tool_label(name: &str) -> String {
    match name {
        "bash" | "shell" => "Bash".to_owned(),
        "todowrite" | "todoread" | "todo" => "Todo".to_owned(),
        "webfetch" => "Fetch".to_owned(),
        "websearch" => "Search".to_owned(),
        "apply_patch" | "patch" => "Patch".to_owned(),
        other => {
            let mut chars = other.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

/// The argument that says what a tool call did: its path, command, pattern or URL.
#[must_use]
pub fn tool_detail(name: &str, input: &Value) -> String {
    let field = |key: &str| input.get(key).and_then(Value::as_str);
    let found = match name {
        "bash" | "shell" => field("command"),
        "grep" | "glob" => field("pattern"),
        "webfetch" => field("url"),
        "websearch" => field("query"),
        "task" => field("description"),
        _ => None,
    };
    if let Some(found) = found {
        return first_line(found).to_owned();
    }
    if let Some(path) = ["filePath", "path", "file"].into_iter().find_map(field) {
        return path.to_owned();
    }
    if name.starts_with("todo") {
        let count = input
            .get("todos")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        return match count {
            0 => String::new(),
            1 => "1 item".to_owned(),
            n => format!("{n} items"),
        };
    }
    // Otherwise the first short text argument, if any.
    input
        .as_object()
        .and_then(|o| o.values().find_map(Value::as_str))
        .map(|s| first_line(s).to_owned())
        .unwrap_or_default()
}

/// The first line of some reasoning, without Markdown emphasis or heading marks.
fn summarize(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    let line = line.trim_start_matches('#').trim();
    let line = line.trim_matches(|c| c == '*' || c == '_').trim();
    shorten(line)
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default().trim()
}

fn shorten(text: &str) -> String {
    match text.char_indices().nth(SUMMARY_CHARS) {
        Some((end, _)) => format!("{}…", text[..end].trim_end()),
        None => text.to_owned(),
    }
}
