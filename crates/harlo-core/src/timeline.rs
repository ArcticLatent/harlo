//! A session's timeline as rows: prompts, answers, thoughts, tool calls.
//!
//! [`entries`] flattens a session's stored messages into rows, oldest first.
//! [`Live`] turns the live event stream into changes to those rows while a
//! session works. Both give a row the same key, so when a run ends and the
//! history is reloaded, the rows that were streamed are kept as they are.
//!
//! Keys: a prompt is its message ID; text and reasoning are
//! `{message}/text/{n}` and `{message}/thought/{n}`, counted per kind within
//! the message; a tool call is `{message}/tool/{call ID}`.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::time::Duration;

use opencode_client::{EventKind, FileDiff, Message, Part, ToolCall, ToolStatus};
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
        /// Names of the files and images sent with it.
        attachments: Vec<String>,
        /// Names of the skills loaded with it.
        skills: Vec<String>,
    },
    /// Answer text from the model, as Markdown.
    Answer {
        /// Stable row key.
        key: String,
        /// The Markdown text.
        text: String,
    },
    /// The model's reasoning, shown as one line (see [`summarize`]).
    Thought {
        /// Stable row key.
        key: String,
        /// The whole reasoning text.
        text: String,
    },
    /// A tool call, shown as one line, with the changes it made below.
    Tool {
        /// Stable row key.
        key: String,
        /// The tool's own name, such as `edit` or `bash`.
        name: String,
        /// The tool's name for people, such as `Edit` or `Bash`.
        label: String,
        /// The call's main argument, such as a path or a command.
        detail: String,
        /// Where the call is.
        status: ToolStatus,
        /// How long it took, once finished.
        took: Option<Duration>,
        /// The files it changed, once it finished: an edit's or a write's.
        files: Vec<FileDiff>,
    },
    /// A run that didn't succeed.
    Stopped {
        /// Stable row key.
        key: String,
        /// `failed` or `interrupted`.
        outcome: String,
    },
    /// The conversation being summarized to free up context.
    Compaction {
        /// Stable row key.
        key: String,
        /// Where it is.
        state: Compaction,
    },
    /// A one-line note: a skill loaded, a retry, a failed step.
    Note {
        /// Stable row key.
        key: String,
        /// What happened.
        text: String,
        /// Whether it's a problem.
        warning: bool,
    },
}

/// Where a compaction is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compaction {
    /// The summary is being written.
    Running,
    /// Done, with the summary.
    Done(String),
    /// It failed, and why.
    Failed(String),
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
            | Self::Stopped { key, .. }
            | Self::Compaction { key, .. }
            | Self::Note { key, .. } => key,
        }
    }

    /// Whether it's a tool call that hasn't finished.
    #[must_use]
    pub fn is_running_tool(&self) -> bool {
        matches!(
            self,
            Self::Tool {
                status: ToolStatus::Streaming | ToolStatus::Running,
                ..
            }
        )
    }
}

/// Turns a session's messages into timeline rows, oldest first.
///
/// Empty text and reasoning are skipped, as are message kinds the timeline
/// doesn't show (agent switches, shells) and successful run ends.
#[must_use]
pub fn entries(messages: &[Message]) -> Vec<Entry> {
    let mut rows = Vec::new();
    for message in messages {
        match message {
            Message::User {
                id,
                text,
                attachments,
                skills,
            } => rows.push(Entry::Prompt {
                key: id.clone(),
                text: text.clone(),
                attachments: attachments.clone(),
                skills: skills.clone(),
            }),
            Message::Assistant { id, parts, .. } => {
                let (mut texts, mut thoughts) = (0, 0);
                for part in parts {
                    match part {
                        Part::Text(text) => {
                            let key = text_key(id, texts);
                            texts += 1;
                            if !text.trim().is_empty() {
                                rows.push(Entry::Answer {
                                    key,
                                    text: text.clone(),
                                });
                            }
                        }
                        Part::Reasoning(text) => {
                            let key = thought_key(id, thoughts);
                            thoughts += 1;
                            if !text.trim().is_empty() {
                                rows.push(Entry::Thought {
                                    key,
                                    text: text.clone(),
                                });
                            }
                        }
                        Part::Tool(call) => rows.push(tool(id, call)),
                    }
                }
            }
            Message::Idle { id, outcome } if outcome != "succeeded" => {
                rows.push(Entry::Stopped {
                    key: id.clone(),
                    outcome: outcome.clone(),
                });
            }
            Message::Compaction {
                id,
                status,
                summary,
                error,
            } => rows.push(Entry::Compaction {
                key: id.clone(),
                state: match status.as_str() {
                    "completed" => Compaction::Done(summary.clone().unwrap_or_default()),
                    "failed" => Compaction::Failed(error.clone().unwrap_or_default()),
                    _ => Compaction::Running,
                },
            }),
            Message::Skill { id, name } => rows.push(Entry::Note {
                key: id.clone(),
                text: format!("Loaded the {name} skill"),
                warning: false,
            }),
            Message::Idle { .. } | Message::Other { .. } => {}
        }
    }
    rows
}

fn text_key(message: &str, ordinal: impl std::fmt::Display) -> String {
    format!("{message}/text/{ordinal}")
}

fn thought_key(message: &str, ordinal: impl std::fmt::Display) -> String {
    format!("{message}/thought/{ordinal}")
}

fn tool_key(message: &str, call: &str) -> String {
    format!("{message}/tool/{call}")
}

fn tool(message: &str, call: &ToolCall) -> Entry {
    Entry::Tool {
        key: tool_key(message, &call.id),
        name: call.name.clone(),
        label: tool_label(&call.name),
        detail: shorten(&tool_detail(&call.name, &call.input)),
        status: call.status,
        took: call.took_ms.map(millis),
        files: changes(&call.name, call.status, &call.input, &call.files),
    }
}

/// The files a finished tool call changed.
///
/// An edit reports its patches. A write reports only its input, so the
/// content it wrote is shown as added.
fn changes(name: &str, status: ToolStatus, input: &Value, files: &[FileDiff]) -> Vec<FileDiff> {
    if status != ToolStatus::Completed {
        return Vec::new();
    }
    if !files.is_empty() || name != "write" {
        return files.to_vec();
    }
    let path = input["path"]
        .as_str()
        .or_else(|| input["filePath"].as_str());
    match (path, input["content"].as_str()) {
        (Some(path), Some(content)) => vec![crate::diff::written(path, content)],
        _ => Vec::new(),
    }
}

fn millis(ms: f64) -> Duration {
    Duration::from_secs_f64(ms.max(0.0) / 1000.0)
}

/// A change to a timeline's rows, from [`Live::apply`].
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Replace the row with this key, or add it at the end.
    Put(Entry),
    /// Add streamed text to an answer or thought; a missing row starts with it.
    Extend {
        /// The row's key.
        key: String,
        /// Whether the row is a thought rather than an answer.
        thought: bool,
        /// The new text.
        delta: String,
    },
    /// The stream and the rows may disagree: reload the history.
    Reload,
}

/// A tool call seen starting, so later events can complete its row.
#[derive(Debug, Clone)]
struct OpenTool {
    key: String,
    name: String,
    started: f64,
    detail: String,
    input: Value,
}

/// A prompt waiting in the inbox, shown once it's delivered.
#[derive(Debug, Clone)]
struct Waiting {
    text: String,
    attachments: Vec<String>,
    skills: Vec<String>,
}

/// Turns one session's live events into row changes.
///
/// It remembers just enough to finish rows: tool calls that started, and
/// prompts waiting in the inbox. When it can't place an event, or a run
/// ends, it asks for a reload, which makes the rows match the server again.
#[derive(Debug, Clone, Default)]
pub struct Live {
    tools: HashMap<String, OpenTool>,
    waiting: HashMap<String, Waiting>,
}

impl Live {
    /// Creates a reducer with nothing in flight.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the changes `kind`, which happened at `time` (ms since the Unix
    /// epoch), makes to the session's rows.
    ///
    /// Events for other sessions are the caller's to filter out.
    #[expect(clippy::too_many_lines, reason = "one short arm per event type")]
    pub fn apply(&mut self, kind: &EventKind, time: f64) -> Vec<Change> {
        match kind {
            EventKind::InboxAdded { item, .. } => {
                if let Some(text) = &item.text {
                    self.waiting.insert(
                        item.id.clone(),
                        Waiting {
                            text: text.clone(),
                            attachments: item.attachments.clone(),
                            skills: item.skills.clone(),
                        },
                    );
                }
                Vec::new()
            }
            EventKind::InboxRemoved {
                item_id, delivered, ..
            } => match (self.waiting.remove(item_id), delivered) {
                (Some(waiting), true) => vec![Change::Put(Entry::Prompt {
                    key: item_id.clone(),
                    text: waiting.text,
                    attachments: waiting.attachments,
                    skills: waiting.skills,
                })],
                (None, true) => vec![Change::Reload],
                (_, false) => Vec::new(),
            },
            EventKind::TextDelta {
                message_id,
                ordinal,
                delta,
                ..
            } => vec![Change::Extend {
                key: text_key(message_id, ordinal),
                thought: false,
                delta: delta.clone(),
            }],
            EventKind::ReasoningDelta {
                message_id,
                ordinal,
                delta,
                ..
            } => vec![Change::Extend {
                key: thought_key(message_id, ordinal),
                thought: true,
                delta: delta.clone(),
            }],
            EventKind::TextEnded {
                message_id,
                ordinal,
                text,
                ..
            } if !text.trim().is_empty() => vec![Change::Put(Entry::Answer {
                key: text_key(message_id, ordinal),
                text: text.clone(),
            })],
            EventKind::ReasoningEnded {
                message_id,
                ordinal,
                text,
                ..
            } if !text.trim().is_empty() => vec![Change::Put(Entry::Thought {
                key: thought_key(message_id, ordinal),
                text: text.clone(),
            })],
            EventKind::ToolStarted {
                message_id,
                call_id,
                name,
                ..
            } => {
                let open = OpenTool {
                    key: tool_key(message_id, call_id),
                    name: name.clone(),
                    started: time,
                    detail: String::new(),
                    input: Value::Null,
                };
                let change = Change::Put(open.entry(ToolStatus::Streaming, None, &[]));
                self.tools.insert(call_id.clone(), open);
                vec![change]
            }
            EventKind::ToolCalled { call_id, input, .. } => match self.tools.get_mut(call_id) {
                Some(open) => {
                    open.detail = shorten(&tool_detail(&open.name, input));
                    open.input = input.clone();
                    vec![Change::Put(open.entry(ToolStatus::Running, None, &[]))]
                }
                None => vec![Change::Reload],
            },
            EventKind::ToolEnded {
                call_id,
                failed,
                files,
                ..
            } => match self.tools.remove(call_id) {
                Some(open) => {
                    let status = if *failed {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Completed
                    };
                    let took = millis(time - open.started);
                    vec![Change::Put(open.entry(status, Some(took), files))]
                }
                None => vec![Change::Reload],
            },
            EventKind::StepFailed {
                message_id, error, ..
            } => vec![Change::Put(Entry::Note {
                key: format!("{message_id}/failed"),
                text: format!("The model failed: {error}"),
                warning: true,
            })],
            EventKind::RetryScheduled {
                session_id,
                attempt,
                error,
                ..
            } => vec![Change::Put(Entry::Note {
                key: format!("{session_id}/retry"),
                text: format!("Retrying (attempt {attempt}): {error}"),
                warning: true,
            })],
            EventKind::SkillActivated { name, .. } => vec![Change::Put(Entry::Note {
                key: format!("skill/{name}/{time}"),
                text: format!("Loaded the {name} skill"),
                warning: false,
            })],
            EventKind::CompactionStarted { session_id } => {
                vec![Change::Put(Entry::Compaction {
                    key: format!("{session_id}/compaction"),
                    state: Compaction::Running,
                })]
            }
            EventKind::CompactionEnded {
                session_id,
                summary,
            } => vec![Change::Put(Entry::Compaction {
                key: format!("{session_id}/compaction"),
                state: Compaction::Done(summary.clone()),
            })],
            EventKind::CompactionFailed { session_id, error } => {
                vec![Change::Put(Entry::Compaction {
                    key: format!("{session_id}/compaction"),
                    state: Compaction::Failed(error.clone()),
                })]
            }
            // A kept undo removed messages.
            EventKind::RevertCommitted { .. } => vec![Change::Reload],
            // A finished run reloads, so the rows match what the server stored.
            EventKind::Done { .. } => {
                self.tools.clear();
                vec![Change::Reload]
            }
            _ => Vec::new(),
        }
    }
}

impl OpenTool {
    fn entry(&self, status: ToolStatus, took: Option<Duration>, files: &[FileDiff]) -> Entry {
        Entry::Tool {
            key: self.key.clone(),
            name: self.name.clone(),
            label: tool_label(&self.name),
            detail: self.detail.clone(),
            status,
            took,
            files: changes(&self.name, status, &self.input, files),
        }
    }
}

/// Describes a run of tool calls in a few words: "Edited 4 files, ran 1 command".
///
/// `names` are the tools' own names, in order. Calls of the same kind are
/// counted together, in the order each kind first appears.
#[must_use]
pub fn tools_summary<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let mut kinds: Vec<(&'static str, usize)> = Vec::new();
    for name in names {
        let kind = tool_kind(name);
        match kinds.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, count)) => *count += 1,
            None => kinds.push((kind, 1)),
        }
    }
    let phrases: Vec<String> = kinds
        .into_iter()
        .map(|(kind, n)| {
            let plural = |one: &str, many: &str| {
                if n == 1 {
                    one.to_owned()
                } else {
                    many.to_owned()
                }
            };
            match kind {
                "edit" => format!("edited {n} {}", plural("file", "files")),
                "read" => format!("read {n} {}", plural("file", "files")),
                "search" => format!("searched {n} {}", plural("time", "times")),
                "bash" => format!("ran {n} {}", plural("command", "commands")),
                "todo" => "updated the todos".to_owned(),
                "web" => format!("looked up {n} {}", plural("page", "pages")),
                "task" => format!("started {n} {}", plural("agent", "agents")),
                _ => format!("used {n} {}", plural("tool", "tools")),
            }
        })
        .collect();
    let mut summary = phrases.join(", ");
    if let Some(first) = summary.get(..1) {
        summary = first.to_uppercase() + &summary[1..];
    }
    summary
}

fn tool_kind(name: &str) -> &'static str {
    match name {
        "edit" | "write" | "apply_patch" | "patch" | "multiedit" => "edit",
        "read" => "read",
        "grep" | "glob" | "list" | "ls" | "codesearch" => "search",
        "bash" | "shell" => "bash",
        "todowrite" | "todoread" | "todo" => "todo",
        "webfetch" | "websearch" => "web",
        "task" => "task",
        _ => "other",
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
#[must_use]
pub fn summarize(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    let line = line.trim_start_matches('#').trim();
    // Emphasis and code markers anywhere in the line, as in "**Planning** the look".
    // Single underscores stay: they're usually in names such as `snake_case`.
    let plain = line.replace(['*', '`'], "").replace("__", "");
    shorten(plain.trim())
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
