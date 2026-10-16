//! Session transcript: a reducer over [`HarnessEvent`]s that produces flat
//! display rows, plus a row diff that turns snapshots into minimal model
//! changes (so the UI updates one row instead of resetting the list).

use std::collections::HashMap;
use std::sync::Arc;

use harlo_harness::{BlockKind, HarnessEvent, NoticeLevel};
use serde_json::Value;

use crate::listdiff::{ListChange, diff_by};
use crate::markdown::{MarkdownRenderer, MdBlockKind, committed_prefix, escape_into};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowKind {
    User,
    #[default]
    Markdown,
    Code,
    Thinking,
    Tool,
    Notice,
}

impl RowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Markdown => "markdown",
            Self::Code => "code",
            Self::Thinking => "thinking",
            Self::Tool => "tool",
            Self::Notice => "notice",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    /// Stable identity across snapshots.
    pub key: String,
    pub kind: RowKind,
    /// Rich text (markdown/code rows).
    pub html: String,
    /// Plain text: user prompt, thinking, raw code, tool output, notice.
    pub text: String,
    /// Tool name, code language, notice level.
    pub title: String,
    /// Tool argument summary.
    pub subtitle: String,
    /// `streaming` / `done` for messages; `running` / `done` / `error` for tools.
    pub state: String,
    /// The tool call's id (tool rows), e.g. to find the subagents it started.
    pub call_id: String,
    /// A run of tool calls and thoughts (two or more rows in a row) shows
    /// as one summary line that folds the run: the key of the run's first
    /// row, on every row of the run ("" outside one).
    pub group: String,
    /// The run's summary ("Thought, edited 1 file, ran 2 commands"), on its
    /// first row only.
    pub summary: String,
    /// The run is still going (a call runs or waits, or nothing follows it
    /// yet): it shows open unless folded by hand.
    pub group_open: bool,
    /// When it started (unix ms: the message's own time, or when harlo saw
    /// it); 0 unknown.
    pub time: i64,
    /// Tool rows: when the call returned (unix ms); 0 while it runs or when
    /// unknown.
    pub ended: i64,
    /// Tool rows: what the call came to, in a word or two ("+4 −2", "48
    /// passed", "exit 1"; see `tool_facts`).
    pub result: String,
    /// Tool rows: an edit's or a write's diff (`+`, `-`, ` ` lines).
    pub diff: String,
    /// A finished run's first row: how long the run took (ms; 0 unknown or
    /// still going).
    pub run_ms: i64,
}

/// Now, in unix ms.
fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// A message's own time (unix ms; pi and omp write it), else now.
fn message_time(message: &Value) -> i64 {
    message["timestamp"].as_i64().filter(|t| *t > 0).unwrap_or_else(now_ms)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    User,
    Assistant,
    Notice(NoticeLevel),
    Other,
}

#[derive(Debug, Clone)]
enum Block {
    Text(String),
    Thinking(String),
    Tool { call_id: String, name: String, args: Value },
    Image,
}

struct Item {
    id: String,
    role: Role,
    blocks: Vec<Block>,
    done: bool,
    error: Option<String>,
    rev: u64,
    rows: Option<(u64, Vec<Arc<Row>>)>,
    /// When it started (unix ms).
    time: i64,
}

#[derive(Debug, Clone, Default)]
struct ToolRun {
    running: bool,
    /// The call returned (end event or result message).
    ended: bool,
    is_error: bool,
    output: String,
    /// When it started and returned (unix ms; 0 unknown).
    started: i64,
    ended_at: i64,
}

/// Tool output beyond this is cut from the front for display.
const TOOL_OUTPUT_DISPLAY_CHARS: usize = 6000;

#[derive(Default)]
pub struct Transcript {
    items: Vec<Item>,
    by_id: HashMap<String, usize>,
    tools: HashMap<String, ToolRun>,
    tool_owner: HashMap<String, usize>,
    notice_seq: u64,
    md: MarkdownRenderer,
    /// Last snapshot's rows that carry a run, so unchanged ones keep their `Arc`.
    grouped: HashMap<String, Arc<Row>>,
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Applies one event. Returns true if anything visible changed.
    pub fn apply(&mut self, ev: &HarnessEvent) -> bool {
        use HarnessEvent as E;
        match ev {
            E::MessageStart { id, message } => self.upsert_message(id, message, false),
            E::MessageEnd { id, message } => self.upsert_message(id, message, true),
            E::BlockStart { id, index, kind, tool } => {
                let Some(item) = self.item_mut(id) else { return false };
                ensure_block(item, *index, *kind);
                if let (Some((call_id, name)), Some(Block::Tool { call_id: c, name: n, .. })) =
                    (tool, item.blocks.get_mut(*index))
                {
                    *c = call_id.clone();
                    *n = name.clone();
                }
                let pos = self.by_id[id];
                if let Some((call_id, _)) = tool {
                    self.tool_owner.insert(call_id.clone(), pos);
                }
                self.touch(pos);
                true
            }
            E::BlockDelta { id, index, kind, delta } => {
                let Some(item) = self.item_mut(id) else { return false };
                ensure_block(item, *index, *kind);
                match &mut item.blocks[*index] {
                    Block::Text(t) | Block::Thinking(t) => t.push_str(delta),
                    // Partial argument JSON: not shown until the call completes.
                    Block::Tool { .. } | Block::Image => return false,
                }
                let pos = self.by_id[id];
                self.touch(pos);
                true
            }
            E::BlockEnd { id, index, kind, content } => {
                let Some(item) = self.item_mut(id) else { return false };
                ensure_block(item, *index, *kind);
                let block = match kind {
                    BlockKind::Text => Block::Text(content.as_str().unwrap_or_default().to_owned()),
                    BlockKind::Thinking => Block::Thinking(content.as_str().unwrap_or_default().to_owned()),
                    BlockKind::ToolCall => tool_block(content),
                };
                if let Block::Tool { call_id, .. } = &block {
                    let call_id = call_id.clone();
                    item.blocks[*index] = block;
                    let pos = self.by_id[id];
                    self.tool_owner.insert(call_id, pos);
                } else {
                    item.blocks[*index] = block;
                }
                let pos = self.by_id[id];
                self.touch(pos);
                true
            }
            E::ToolStart { call_id, .. } => {
                let run = self.tools.entry(call_id.clone()).or_default();
                run.running = true;
                if run.started == 0 {
                    run.started = now_ms();
                }
                self.touch_tool(call_id)
            }
            E::ToolUpdate { call_id, partial } => {
                let run = self.tools.entry(call_id.clone()).or_default();
                // omp's background tasks keep reporting on their tool call
                // after it returned; a finished call stays finished.
                if run.ended {
                    return false;
                }
                run.running = true;
                run.output = content_text(&partial["content"]);
                self.touch_tool(call_id)
            }
            E::ToolEnd { call_id, result, is_error } => {
                let run = self.tools.entry(call_id.clone()).or_default();
                run.running = false;
                run.ended = true;
                run.is_error = *is_error;
                run.output = content_text(&result["content"]);
                if run.ended_at == 0 {
                    run.ended_at = now_ms();
                }
                self.touch_tool(call_id)
            }
            // The harness's own info notices (tool mounts, cache warming…) are
            // noise in a conversation; warnings and errors still show.
            E::Notice { level: NoticeLevel::Info, system: true, text } => {
                tracing::debug!("harness notice: {text}");
                false
            }
            E::Notice { level, text, .. } => self.push_notice(*level, text.clone()),
            E::Compaction { started: true, reason, .. } => self.push_notice(
                NoticeLevel::Info,
                format!("Compacting context{}…", reason.as_deref().map(|r| format!(" ({r})")).unwrap_or_default()),
            ),
            E::Compaction { started: false, error: Some(err), .. } => {
                self.push_notice(NoticeLevel::Error, format!("Compaction failed: {err}"))
            }
            E::Compaction { started: false, error: None, .. } => {
                self.push_notice(NoticeLevel::Info, "Context compacted".into())
            }
            E::Retry { started: true, attempt, max_attempts, error } => self.push_notice(
                NoticeLevel::Warning,
                match max_attempts {
                    0 => format!("Retrying (attempt {attempt}): {}", error.as_deref().unwrap_or("provider error")),
                    max => format!("Retrying ({attempt}/{max}): {}", error.as_deref().unwrap_or("provider error")),
                },
            ),
            E::Retry { started: false, error: Some(err), .. } => {
                self.push_notice(NoticeLevel::Error, format!("Retry failed: {err}"))
            }
            E::Exited { code, stderr_tail } => {
                let mut text = format!("Harness exited (code {})", code.map_or("?".into(), |c| c.to_string()));
                if !stderr_tail.is_empty() {
                    text.push_str("\n\n");
                    text.push_str(stderr_tail);
                }
                self.push_notice(if *code == Some(0) { NoticeLevel::Info } else { NoticeLevel::Error }, text)
            }
            E::ProtocolError(err) => self.push_notice(NoticeLevel::Error, format!("Protocol error: {err}")),
            E::Response { success: false, command, error, .. } => self.push_notice(
                NoticeLevel::Error,
                format!("{command} failed: {}", error.as_deref().unwrap_or("unknown error")),
            ),
            _ => false,
        }
    }

    /// Loads a message list (from `get_messages`), replacing the transcript.
    pub fn load_messages(&mut self, messages: &[Value]) {
        *self = Self { md: std::mem::take(&mut self.md), ..Self::default() };
        for (i, m) in messages.iter().enumerate() {
            self.upsert_message(&format!("h{i}"), m, true);
        }
    }

    fn item_mut(&mut self, id: &str) -> Option<&mut Item> {
        let pos = *self.by_id.get(id)?;
        self.items.get_mut(pos)
    }

    fn touch(&mut self, pos: usize) {
        if let Some(item) = self.items.get_mut(pos) {
            item.rev += 1;
        }
    }

    fn touch_tool(&mut self, call_id: &str) -> bool {
        match self.tool_owner.get(call_id).copied() {
            Some(pos) => {
                self.touch(pos);
                true
            }
            None => false,
        }
    }

    fn push_notice(&mut self, level: NoticeLevel, text: String) -> bool {
        self.notice_seq += 1;
        let id = format!("notice-{}", self.notice_seq);
        self.by_id.insert(id.clone(), self.items.len());
        self.items.push(Item {
            id,
            role: Role::Notice(level),
            blocks: vec![Block::Text(text)],
            done: true,
            error: None,
            rev: 0,
            rows: None,
            time: now_ms(),
        });
        true
    }

    fn upsert_custom(&mut self, id: &str, message: &Value) -> bool {
        if message["display"] == false {
            return false;
        }
        let raw = content_text(&message["content"]);
        let text = raw.replace("<system-notice>", "").replace("</system-notice>", "").trim().to_owned();
        if text.is_empty() {
            return false;
        }
        if let Some(&pos) = self.by_id.get(id) {
            self.items[pos].blocks = vec![Block::Text(text)];
            self.touch(pos);
            return true;
        }
        self.by_id.insert(id.to_owned(), self.items.len());
        self.items.push(Item {
            id: id.to_owned(),
            role: Role::Notice(NoticeLevel::Info),
            blocks: vec![Block::Text(text)],
            done: true,
            error: None,
            rev: 0,
            rows: None,
            time: message_time(message),
        });
        true
    }

    fn upsert_message(&mut self, id: &str, message: &Value, done: bool) -> bool {
        let role = match message["role"].as_str() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            // The system prompt is not part of the conversation.
            Some("system") => return false,
            Some("toolResult") => {
                // Tool results render inside the tool call row.
                let call_id = message["toolCallId"].as_str().unwrap_or_default().to_owned();
                let run = self.tools.entry(call_id.clone()).or_default();
                run.running = false;
                run.ended = true;
                run.is_error = message["isError"].as_bool().unwrap_or(false);
                run.output = content_text(&message["content"]);
                if run.ended_at == 0 {
                    run.ended_at = message_time(message);
                }
                return self.touch_tool(&call_id);
            }
            // omp's own messages (background job results, notes from its
            // modes): hidden ones stay hidden, the rest show as a notice.
            Some("custom") => return self.upsert_custom(id, message),
            _ => Role::Other,
        };
        let blocks = parse_blocks(message);
        let error = message["errorMessage"].as_str().filter(|s| !s.is_empty()).map(str::to_owned);

        let pos = match self.by_id.get(id) {
            Some(&pos) => pos,
            None => {
                let pos = self.items.len();
                self.by_id.insert(id.to_owned(), pos);
                self.items.push(Item {
                    id: id.to_owned(),
                    role,
                    blocks: Vec::new(),
                    done: false,
                    error: None,
                    rev: 0,
                    rows: None,
                    time: message_time(message),
                });
                pos
            }
        };
        for b in &blocks {
            if let Block::Tool { call_id, .. } = b {
                self.tool_owner.insert(call_id.clone(), pos);
            }
        }
        let item = &mut self.items[pos];
        // message_start of a streaming assistant message is empty; keep any
        // blocks that deltas already built rather than wiping them.
        if !blocks.is_empty() || done {
            item.blocks = blocks;
        }
        item.role = role;
        item.done = done;
        item.error = error;
        item.rev += 1;
        true
    }

    /// Current display rows. Rows of unchanged items are reused.
    pub fn rows(&mut self) -> Vec<Arc<Row>> {
        let mut out = Vec::new();
        for i in 0..self.items.len() {
            let fresh = matches!(&self.items[i].rows, Some((rev, _)) if *rev == self.items[i].rev);
            if !fresh {
                let rows = self.build_rows(i);
                let item = &mut self.items[i];
                item.rows = Some((item.rev, rows));
            }
            out.extend(self.items[i].rows.as_ref().expect("rows built").1.iter().cloned());
        }
        self.group_runs(out)
    }

    /// Marks the runs of tool calls and thoughts (see [`Row::group`]).
    fn group_runs(&mut self, mut rows: Vec<Arc<Row>>) -> Vec<Arc<Row>> {
        let activity = |r: &Row| matches!(r.kind, RowKind::Tool | RowKind::Thinking);
        let mut grouped = HashMap::new();
        let mut i = 0;
        while i < rows.len() {
            if !activity(&rows[i]) {
                i += 1;
                continue;
            }
            let start = i;
            while i < rows.len() && activity(&rows[i]) {
                i += 1;
            }
            if i - start < 2 {
                continue;
            }
            let run = &rows[start..i];
            let group = run[0].key.clone();
            let summary = run_summary(run);
            let open = i == rows.len() || run.iter().any(|r| r.kind == RowKind::Tool && matches!(r.state.as_str(), "running" | "pending"));
            // How long it took: from its first step to what came next (or
            // its last call's return).
            let end = rows.get(i).map(|r| r.time).into_iter().chain(run.iter().map(|r| r.ended)).max().unwrap_or(0);
            let run_ms = if open || run[0].time == 0 { 0 } else { (end - run[0].time).max(0) };
            for (n, slot) in rows[start..i].iter_mut().enumerate() {
                let mut r = (**slot).clone();
                r.group = group.clone();
                r.summary = if n == 0 { summary.clone() } else { String::new() };
                r.run_ms = if n == 0 { run_ms } else { 0 };
                r.group_open = open;
                let r = match self.grouped.get(&r.key) {
                    Some(prev) if **prev == r => prev.clone(),
                    _ => Arc::new(r),
                };
                grouped.insert(r.key.clone(), r.clone());
                *slot = r;
            }
        }
        self.grouped = grouped;
        rows
    }

    fn build_rows(&mut self, i: usize) -> Vec<Arc<Row>> {
        let item = &self.items[i];
        let id = item.id.clone();
        let streaming = !item.done;
        let state = if streaming { "streaming" } else { "done" };
        let mut rows = Vec::new();
        let time = item.time;
        let row = |key: String, kind: RowKind| Row { key, kind, state: state.to_owned(), time, ..Row::default() };

        match item.role {
            Role::User => {
                let text: Vec<&str> = item
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        Block::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect();
                let images = item.blocks.iter().filter(|b| matches!(b, Block::Image)).count();
                let mut r = row(format!("{id}:u"), RowKind::User);
                // Attached files (`<file>` blocks) show as names, not contents.
                let (files, rest) = crate::attach::split_blocks(&text.join("\n"));
                let (rest, starter) = crate::starters::strip(&rest);
                let (rest, ask_first) = harlo_harness::file_blocks::strip_ask_first(rest);
                let (rest, plan) = harlo_harness::file_blocks::strip_plan(rest);
                r.text = rest.to_owned();
                r.title = files.join("\n");
                let mut tags = Vec::new();
                if images > 0 && files.is_empty() {
                    tags.push(format!("{images} image{}", if images == 1 { "" } else { "s" }));
                }
                if let Some(s) = starter {
                    tags.push(s.label.to_owned());
                }
                if plan {
                    tags.push("Plan".to_owned());
                }
                if ask_first {
                    tags.push("Ask first".to_owned());
                }
                r.subtitle = tags.join(" · ");
                rows.push(Arc::new(r));
            }
            Role::Notice(level) => {
                let mut r = row(format!("{id}:n"), RowKind::Notice);
                r.title = match level {
                    NoticeLevel::Info => "info",
                    NoticeLevel::Warning => "warning",
                    NoticeLevel::Error => "error",
                }
                .into();
                if let Some(Block::Text(t)) = item.blocks.first() {
                    r.text = t.clone();
                }
                rows.push(Arc::new(r));
            }
            Role::Assistant | Role::Other => {
                let blocks = item.blocks.clone();
                let error = item.error.clone();
                let last_text = blocks.iter().rposition(|b| matches!(b, Block::Text(_)));
                for (bi, block) in blocks.iter().enumerate() {
                    match block {
                        Block::Text(text) => {
                            // Only the block still being written is held back.
                            let shown = if streaming && Some(bi) == last_text { committed_prefix(text) } else { text };
                            for (mi, md) in self.md.render(shown).into_iter().enumerate() {
                                let kind = if md.kind == MdBlockKind::Code { RowKind::Code } else { RowKind::Markdown };
                                let mut r = row(format!("{id}:{bi}:{mi}"), kind);
                                r.html = md.html.clone();
                                r.text = md.code.clone();
                                r.title = md.lang.clone();
                                rows.push(Arc::new(r));
                            }
                        }
                        Block::Thinking(text) => {
                            if text.trim().is_empty() {
                                continue;
                            }
                            let mut r = row(format!("{id}:{bi}"), RowKind::Thinking);
                            r.text = text.clone();
                            rows.push(Arc::new(r));
                        }
                        Block::Tool { call_id, name, args } => {
                            let run = self.tools.get(call_id).cloned().unwrap_or_default();
                            let mut r = row(format!("{id}:{bi}"), RowKind::Tool);
                            r.title = name.clone();
                            r.call_id = call_id.clone();
                            r.subtitle = summarize_args(args);
                            r.text = tail_chars(&run.output, TOOL_OUTPUT_DISPLAY_CHARS);
                            if run.started > 0 {
                                r.time = run.started;
                            }
                            r.ended = run.ended_at;
                            if run.ended {
                                (r.result, r.diff) = crate::tool_facts::facts(name, args, &run.output, run.is_error);
                            }
                            r.state = if run.running {
                                "running"
                            } else if run.is_error {
                                "error"
                            } else if self.tools.contains_key(call_id) {
                                "done"
                            } else {
                                "pending"
                            }
                            .into();
                            rows.push(Arc::new(r));
                            // A subagent hands back its result through `yield`:
                            // show it as text, not as collapsed arguments.
                            if name == "yield"
                                && let Some(result) = yield_markdown(args)
                            {
                                for (mi, md) in self.md.render(&result).into_iter().enumerate() {
                                    let kind = if md.kind == MdBlockKind::Code { RowKind::Code } else { RowKind::Markdown };
                                    let mut r = row(format!("{id}:{bi}:y{mi}"), kind);
                                    r.html = md.html.clone();
                                    r.text = md.code.clone();
                                    r.title = md.lang.clone();
                                    rows.push(Arc::new(r));
                                }
                            }
                        }
                        Block::Image => {}
                    }
                }
                if let Some(err) = error {
                    let mut r = row(format!("{id}:err"), RowKind::Notice);
                    r.title = "error".into();
                    r.text = err;
                    rows.push(Arc::new(r));
                }
            }
        }
        rows
    }
}

fn ensure_block(item: &mut Item, index: usize, kind: BlockKind) {
    while item.blocks.len() <= index {
        item.blocks.push(Block::Text(String::new()));
    }
    let matches = matches!(
        (&item.blocks[index], kind),
        (Block::Text(_), BlockKind::Text) | (Block::Thinking(_), BlockKind::Thinking) | (Block::Tool { .. }, BlockKind::ToolCall)
    );
    if !matches {
        item.blocks[index] = match kind {
            BlockKind::Text => Block::Text(String::new()),
            BlockKind::Thinking => Block::Thinking(String::new()),
            BlockKind::ToolCall => Block::Tool { call_id: String::new(), name: String::new(), args: Value::Null },
        };
    }
}

fn tool_block(v: &Value) -> Block {
    Block::Tool {
        call_id: v["id"].as_str().unwrap_or_default().to_owned(),
        name: v["name"].as_str().unwrap_or_default().to_owned(),
        args: v["arguments"].clone(),
    }
}

fn parse_blocks(message: &Value) -> Vec<Block> {
    let content = &message["content"];
    if let Some(s) = content.as_str() {
        return vec![Block::Text(s.to_owned())];
    }
    let mut blocks: Vec<Block> = content
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|b| match b["type"].as_str() {
                    Some("text") => Block::Text(b["text"].as_str().unwrap_or_default().to_owned()),
                    Some("thinking") => Block::Thinking(b["thinking"].as_str().unwrap_or_default().to_owned()),
                    Some("toolCall") => tool_block(b),
                    Some("image") => Block::Image,
                    _ => Block::Text(String::new()),
                })
                .collect()
        })
        .unwrap_or_default();
    // Coding-agent messages without content blocks (bash executions,
    // summaries) carry their text in other fields.
    if blocks.is_empty() {
        for key in ["summary", "output", "text"] {
            if let Some(s) = message[key].as_str() {
                let prefix = message["command"].as_str().map(|c| format!("`$ {c}`\n\n")).unwrap_or_default();
                blocks.push(Block::Text(format!("{prefix}{s}")));
                break;
            }
        }
    }
    blocks
}

/// Text of a tool result. ANSI colour codes are removed (shown as plain text for now).
fn content_text(content: &Value) -> String {
    let text = match content.as_str() {
        Some(s) => s.to_owned(),
        None => content
            .as_array()
            .map(|arr| arr.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default(),
    };
    if text.contains('\u{1b}') { crate::session::strip_ansi(&text) } else { text }
}

/// What a subagent's `yield` call returns, as markdown: its error, or the
/// summary and report (omp's structured result), else the data as JSON.
fn yield_markdown(args: &Value) -> Option<String> {
    if let Some(err) = args["error"].as_str().filter(|e| !e.trim().is_empty()) {
        return Some(format!("**Failed:** {err}"));
    }
    let data = &args["data"];
    if let Some(text) = data.as_str() {
        return (!text.trim().is_empty()).then(|| text.to_owned());
    }
    if data.is_null() {
        return None;
    }
    let summary = data["summary"].as_str().unwrap_or_default().trim();
    let report = data["report"].as_str().unwrap_or_default().trim();
    if summary.is_empty() && report.is_empty() {
        return Some(format!("```json\n{}\n```", serde_json::to_string_pretty(data).unwrap_or_default()));
    }
    Some([format!("**{summary}**"), report.to_owned()].into_iter().filter(|s| s != "****" && !s.is_empty()).collect::<Vec<_>>().join("\n\n"))
}

/// One-line summary of tool arguments: the obvious field for common tools
/// (`command`, `path`, …), otherwise compact `key=value` pairs. MCP proxy
/// calls (`{tool, args}`) show the inner tool name first.
fn summarize_args(args: &Value) -> String {
    for key in ["command", "path", "file_path", "filePath", "pattern", "query", "url"] {
        if let Some(s) = args[key].as_str() {
            return first_line(s, 200);
        }
    }
    let Some(obj) = args.as_object() else {
        return if args.is_null() { String::new() } else { first_line(&args.to_string(), 200) };
    };
    let mut parts = Vec::new();
    if let Some(inner) = obj.get("tool").and_then(Value::as_str) {
        parts.push(inner.to_owned());
        if let Some(inner_args) = obj.get("args") {
            let nested = summarize_args(inner_args);
            if !nested.is_empty() {
                parts.push(nested);
            }
        }
    } else {
        for (k, v) in obj {
            let value = match v {
                Value::String(s) => first_line(s, 60),
                Value::Number(_) | Value::Bool(_) => v.to_string(),
                Value::Null => continue,
                Value::Array(a) => format!("[{}]", a.len()),
                Value::Object(_) => "{…}".to_owned(),
            };
            parts.push(format!("{k}={value}"));
        }
    }
    first_line(&parts.join("  "), 200)
}

fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or_default();
    let mut out: String = line.chars().take(max).collect();
    if out.len() < line.len() || s.lines().nth(1).is_some() {
        out.push('…');
    }
    out
}

fn tail_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_owned();
    }
    let skip = count - max;
    let start = s.char_indices().nth(skip).map(|(i, _)| i).unwrap_or(0);
    format!("…\n{}", &s[start..])
}

/// Escapes plain text for a rich-text context.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    escape_into(&mut out, s);
    out
}

/// What a run of tool calls and thoughts did, in words: "Thought, read 2
/// files, ran 1 command" (and how many failed).
fn run_summary(run: &[Arc<Row>]) -> String {
    // (verb phrase, singular, plural) in the order they're told.
    const KINDS: [(&str, &str, &str); 8] = [
        ("read", "file", "files"),
        ("edited", "file", "files"),
        ("wrote", "file", "files"),
        ("ran", "command", "commands"),
        ("searched", "time", "times"),
        ("fetched", "page", "pages"),
        ("started", "subagent", "subagents"),
        ("used", "tool", "tools"),
    ];
    let mut counts = [0usize; 8];
    let (mut thought, mut failed, mut todos) = (false, 0, false);
    for r in run {
        if r.kind == RowKind::Thinking {
            thought = true;
            continue;
        }
        if r.state == "error" {
            failed += 1;
        }
        let name = r.title.to_lowercase();
        let n = name.rsplit(['.', '_', ':']).next().unwrap_or(&name);
        let slot = match n {
            _ if name.contains("todo") => {
                todos = true;
                continue;
            }
            "read" | "view" | "cat" | "file" | "readfile" | "notebookread" => 0,
            _ if name.contains("edit") || name.contains("patch") || name.contains("replace") => 1,
            "write" | "create" | "writefile" => 2,
            "bash" | "shell" | "exec" | "command" | "run" | "terminal" | "python" => 3,
            "grep" | "glob" | "find" | "search" | "ls" | "list" | "rg" | "codesearch" | "ast" => 4,
            _ if name.starts_with("web") || name.contains("fetch") => 5,
            "task" | "agent" | "subagent" => 6,
            _ => 7,
        };
        counts[slot] += 1;
    }
    let mut parts: Vec<String> = Vec::new();
    if thought {
        parts.push("thought".into());
    }
    for (k, (verb, one, many)) in KINDS.iter().enumerate() {
        let c = counts[k];
        if c > 0 {
            parts.push(format!("{verb} {c} {}", if c == 1 { one } else { many }));
        }
    }
    if todos {
        parts.push("updated the todos".into());
    }
    let mut out = parts.join(", ");
    if let Some(first) = out.get(..1) {
        out = first.to_uppercase() + &out[1..];
    }
    if failed > 0 {
        out.push_str(&format!(" · {failed} failed"));
    }
    out
}

/// One change to the row list; see [`ListChange`].
pub type RowChange = ListChange<Arc<Row>>;

/// Diffs two row snapshots by key. Unchanged rows usually share the same
/// `Arc`, so most comparisons are a pointer check.
pub fn diff_rows(old: &[Arc<Row>], new: &[Arc<Row>]) -> Vec<RowChange> {
    diff_by(old, new, |a, b| a.key == b.key, |a, b| Arc::ptr_eq(a, b) || **a == **b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn apply_changes(rows: &mut Vec<Arc<Row>>, changes: Vec<RowChange>) {
        crate::listdiff::apply(rows, changes);
    }

    #[test]
    fn yield_results_show_as_text() {
        let report = json!({ "type": null, "error": null, "data": { "summary": "2 files found.", "report": "| File | Size |\n|---|---:|\n| a.md | 6 |", "files": [] } });
        assert_eq!(yield_markdown(&report).unwrap(), "**2 files found.**\n\n| File | Size |\n|---|---:|\n| a.md | 6 |");
        assert_eq!(yield_markdown(&json!({ "data": "plain answer" })).unwrap(), "plain answer");
        assert_eq!(yield_markdown(&json!({ "error": "no access" })).unwrap(), "**Failed:** no access");
        assert!(yield_markdown(&json!({ "data": { "x": 1 } })).unwrap().starts_with("```json"));
        assert!(yield_markdown(&json!({})).is_none());

        let mut t = Transcript::default();
        let call = json!({"type":"toolCall","id":"y1","name":"yield","arguments": report});
        t.apply(&HarnessEvent::MessageEnd { id: "a1".into(), message: json!({"role":"assistant","content":[call]}) });
        let rows = t.rows();
        assert_eq!(rows[0].kind, RowKind::Tool);
        assert!(rows.iter().skip(1).any(|r| r.kind == RowKind::Markdown && r.html.contains("2 files found.")), "{rows:?}");
    }

    #[test]
    fn finished_tools_stay_finished() {
        let mut t = Transcript::default();
        let call = json!({"type":"toolCall","id":"c1","name":"task","arguments":{}});
        t.apply(&HarnessEvent::MessageEnd { id: "a1".into(), message: json!({"role":"assistant","content":[call]}) });
        t.apply(&HarnessEvent::ToolStart { call_id: "c1".into(), name: "task".into(), args: json!({}) });
        t.apply(&HarnessEvent::ToolEnd { call_id: "c1".into(), result: json!({"content":[{"type":"text","text":"Spawned 2 background agents"}]}), is_error: false });
        // A background job reports on the call afterwards.
        t.apply(&HarnessEvent::ToolUpdate { call_id: "c1".into(), partial: json!({"content":[{"type":"text","text":"progress"}]}) });
        let rows = t.rows();
        let tool = rows.iter().find(|r| r.kind == RowKind::Tool).unwrap();
        assert_eq!(tool.state, "done");
        assert_eq!(tool.text, "Spawned 2 background agents");
    }

    #[test]
    fn custom_messages_show_as_notices() {
        let mut t = Transcript::default();
        t.apply(&HarnessEvent::MessageEnd {
            id: "x1".into(),
            message: json!({"role":"custom","customType":"async-result","display":true,"content":"<system-notice>\n2 background jobs have completed.\n</system-notice>"}),
        });
        t.apply(&HarnessEvent::MessageEnd { id: "x2".into(), message: json!({"role":"custom","customType":"internal","display":false,"content":"hidden"}) });
        let rows = t.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, RowKind::Notice);
        assert_eq!(rows[0].title, "info");
        assert_eq!(rows[0].text, "2 background jobs have completed.");
    }

    #[test]
    fn streaming_text_updates_only_the_tail() {
        let mut t = Transcript::new();
        t.apply(&HarnessEvent::MessageStart { id: "m1".into(), message: json!({"role":"user","content":"hi"}) });
        t.apply(&HarnessEvent::MessageEnd { id: "m1".into(), message: json!({"role":"user","content":"hi"}) });
        t.apply(&HarnessEvent::MessageStart { id: "m2".into(), message: json!({"role":"assistant","content":[]}) });
        t.apply(&HarnessEvent::BlockDelta { id: "m2".into(), index: 0, kind: BlockKind::Text, delta: "Para one.\n\nPara".into() });
        let a = t.rows();
        assert_eq!(a.len(), 2, "user row + committed paragraph");
        t.apply(&HarnessEvent::BlockDelta { id: "m2".into(), index: 0, kind: BlockKind::Text, delta: " two.\n".into() });
        let b = t.rows();
        let changes = diff_rows(&a, &b);
        assert!(matches!(changes.as_slice(), [RowChange::Insert { at: 2, .. }]), "{changes:?}");
        let mut applied = a.clone();
        apply_changes(&mut applied, changes);
        assert_eq!(applied, b);
    }

    #[test]
    fn tool_rows_track_execution() {
        let mut t = Transcript::new();
        t.apply(&HarnessEvent::MessageStart { id: "m".into(), message: json!({"role":"assistant","content":[]}) });
        t.apply(&HarnessEvent::BlockEnd {
            id: "m".into(),
            index: 0,
            kind: BlockKind::ToolCall,
            content: json!({"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"ls"}}),
        });
        t.apply(&HarnessEvent::ToolStart { call_id: "c1".into(), name: "bash".into(), args: json!({}) });
        assert_eq!(t.rows()[0].state, "running");
        t.apply(&HarnessEvent::ToolEnd { call_id: "c1".into(), result: json!({"content":[{"type":"text","text":"a\nb"}]}), is_error: false });
        let rows = t.rows();
        assert_eq!((rows[0].state.as_str(), rows[0].text.as_str(), rows[0].subtitle.as_str()), ("done", "a\nb", "ls"));
    }

    #[test]
    fn hides_system_info_notices() {
        let mut t = Transcript::new();
        t.apply(&HarnessEvent::Notice { level: NoticeLevel::Info, text: "xd://: mounted mcp__x".into(), system: true });
        assert!(t.rows().is_empty());
        t.apply(&HarnessEvent::Notice { level: NoticeLevel::Warning, text: "MCP server failed".into(), system: true });
        t.apply(&HarnessEvent::Notice { level: NoticeLevel::Info, text: "/context output".into(), system: false });
        assert_eq!(t.rows().len(), 2);
    }

    #[test]
    fn summarizes_tool_arguments() {
        assert_eq!(summarize_args(&json!({"command":"ls -la\nrm x"})), "ls -la…");
        assert_eq!(
            summarize_args(&json!({"tool":"set_node_flags","args":{"node_path":"/obj/a","display":true}})),
            // In the order the model wrote them.
            "set_node_flags  node_path=/obj/a  display=true"
        );
        assert_eq!(summarize_args(&json!({})), "");
    }

    #[test]
    fn runs_of_tools_fold_under_a_summary() {
        let mut t = Transcript::new();
        let calls = json!({"role":"assistant","content":[
            {"type":"thinking","thinking":"Look first."},
            {"type":"toolCall","id":"c1","name":"read","arguments":{"path":"a.rs"}},
            {"type":"toolCall","id":"c2","name":"Edit","arguments":{"path":"a.rs"}},
            {"type":"toolCall","id":"c3","name":"bash","arguments":{"command":"cargo test"}},
            {"type":"toolCall","id":"c4","name":"bash","arguments":{"command":"cargo build"}}]});
        t.load_messages(&[
            json!({"role":"user","content":"go"}),
            calls,
            json!({"role":"toolResult","toolCallId":"c1","content":[{"type":"text","text":"x"}]}),
            json!({"role":"toolResult","toolCallId":"c2","content":[{"type":"text","text":"ok"}]}),
            json!({"role":"toolResult","toolCallId":"c3","content":[{"type":"text","text":"fail"}],"isError":true}),
            json!({"role":"toolResult","toolCallId":"c4","content":[{"type":"text","text":"ok"}]}),
            json!({"role":"assistant","content":[{"type":"text","text":"Done."}]}),
        ]);
        let rows = t.rows();
        let run: Vec<_> = rows.iter().filter(|r| !r.group.is_empty()).collect();
        assert_eq!(run.len(), 5, "{rows:?}");
        assert_eq!(run[0].summary, "Thought, read 1 file, edited 1 file, ran 2 commands · 1 failed");
        assert!(run[1..].iter().all(|r| r.summary.is_empty() && r.group == run[0].key));
        assert!(!run[0].group_open, "a finished run with text after it starts folded");
        // Unchanged rows keep their Arc from one snapshot to the next.
        let again = t.rows();
        assert!(rows.iter().zip(&again).all(|(a, b)| Arc::ptr_eq(a, b)));
    }

    /// History: a run's duration from the messages' own times, and each
    /// call's result.
    #[test]
    fn runs_know_how_long_they_took() {
        let mut t = Transcript::new();
        t.load_messages(&[
            json!({ "role": "user", "content": "fix it", "timestamp": 1000 }),
            json!({ "role": "assistant", "timestamp": 2000, "content": [
                { "type": "thinking", "thinking": "hmm" },
                { "type": "toolCall", "id": "e1", "name": "edit", "arguments": { "path": "a", "oldText": "x", "newText": "y\nz" } },
            ] }),
            json!({ "role": "toolResult", "toolCallId": "e1", "content": [{ "type": "text", "text": "ok" }], "timestamp": 2500 }),
            json!({ "role": "assistant", "timestamp": 3000, "content": [{ "type": "toolCall", "id": "b1", "name": "bash", "arguments": { "command": "cargo test" } }] }),
            json!({ "role": "toolResult", "toolCallId": "b1", "content": [{ "type": "text", "text": "test result: ok. 9 passed" }], "timestamp": 5000 }),
            json!({ "role": "assistant", "timestamp": 6000, "content": [{ "type": "text", "text": "Done." }] }),
        ]);
        let rows = t.rows();
        let run: Vec<_> = rows.iter().filter(|r| !r.group.is_empty()).collect();
        assert_eq!(run.len(), 3);
        assert_eq!(run[0].run_ms, 4000);
        assert_eq!((run[1].result.as_str(), run[1].diff.as_str()), ("+2 −1", "-x\n+y\n+z"));
        assert_eq!((run[2].result.as_str(), run[2].time, run[2].ended), ("9 passed", 3000, 5000));
        assert_eq!(rows[0].time, 1000);
    }

    #[test]
    fn diff_handles_middle_replacement() {
        let mk = |k: &str, t: &str| Arc::new(Row { key: k.into(), kind: RowKind::Markdown, html: t.into(), ..Row::default() });
        let old = vec![mk("a", "1"), mk("b", "1"), mk("c", "1"), mk("z", "1")];
        let new = vec![mk("a", "1"), mk("x", "1"), mk("z", "2")];
        let mut applied = old.clone();
        apply_changes(&mut applied, diff_rows(&old, &new));
        assert_eq!(applied, new);
    }
}
