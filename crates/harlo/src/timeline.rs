//! A session's timeline: its rows, and runs of tool calls folded into groups.
//!
//! Runs of two or more tool calls fold into a group with a one-line summary.
//! A group is open while one of its calls runs, and closed once they're done,
//! unless the user opened or closed it.
//!
//! The window lists [`Timeline::items`] in a virtualized list, so only the
//! rows near the view are built, however long the session.

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::{Duration, Instant};

use harlo_core::diff;
use harlo_core::timeline::{Change, Entry, Live, tools_summary};
use harlo_core::workspace::{EventKind, ToolStatus};

/// How long a new prompt takes to fade in, rising a little.
pub const APPEAR: Duration = Duration::from_millis(220);

/// Something the timeline reports.
#[derive(Debug, Clone)]
pub enum Event {
    /// A link in an answer was clicked.
    Link(String),
    /// A tool group was opened or closed, by its key.
    ToggleGroup(String),
    /// A tool row's diff was shown whole, or cut short again, by the row's key.
    ToggleDiff(String),
    /// Undo was chosen on a prompt: it and everything after it, by its message ID.
    Undo(String),
    /// A subagent's child session was chosen, to open it.
    OpenSession(String),
    /// A file in an edit was clicked, to review it: the tool row's key, and
    /// which of its files.
    Review {
        /// The tool row's key.
        tool: String,
        /// Which of the row's files.
        file: usize,
    },
}

/// Where loading a session's history stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    /// The history is on its way.
    Loading,
    /// The rows are the session's history.
    Ready,
    /// Loading failed, with the reason.
    Failed(String),
}

/// What a working session is doing, for the line above the composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// A prompt is on its way to it.
    Sending,
    /// It's thinking, or hasn't answered yet.
    Thinking,
    /// It's writing an answer.
    Writing,
    /// It's waiting for the user to approve an action, such as `shell`.
    Waiting {
        /// What it wants to do, such as `shell` or `edit`.
        action: String,
        /// What it acts on, such as a command or a path.
        detail: String,
    },
    /// It's running a tool: its name for people, and its main argument.
    Running {
        /// The tool's name for people, such as `Bash`.
        label: String,
        /// The call's main argument, such as a command.
        detail: String,
    },
}

/// One row: its entry, and a tool's diffs parsed once.
#[derive(Debug)]
struct Row {
    entry: Entry,
    /// A tool's changed files as diff lines, parsed once.
    diffs: Vec<Vec<diff::Line>>,
    /// Built from live events rather than loaded.
    live: bool,
    /// A prompt Harlo sent that OpenCode's copy hasn't replaced yet; reloads keep it.
    unsaved: bool,
    /// Changes whenever the entry does, so the window rebuilds only changed rows.
    stamp: u64,
    /// When a prompt that wasn't here before arrived, to fade it in.
    appeared: Option<Instant>,
}

impl Row {
    fn new(entry: Entry, live: bool, stamp: u64) -> Self {
        let diffs = match &entry {
            Entry::Tool { files, .. } => files.iter().map(|f| diff::lines(&f.patch)).collect(),
            _ => Vec::new(),
        };
        Self {
            entry,
            diffs,
            live,
            unsaved: false,
            stamp,
            appeared: None,
        }
    }
}

/// What the timeline shows, in order: rows, and the headers of tool groups.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Row(usize),
    Group {
        /// The first tool's key, which names the group.
        key: String,
        /// The rows in the group.
        rows: Range<usize>,
        open: bool,
        running: bool,
    },
}

/// A session's rows and groups.
#[derive(Debug)]
pub struct Timeline {
    /// Where loading stands.
    pub load: Load,
    rows: Vec<Row>,
    index: HashMap<String, usize>,
    items: Vec<Item>,
    /// Groups the user opened (`true`) or closed (`false`).
    choices: HashMap<String, bool>,
    /// Tool rows whose diffs are shown whole.
    whole: HashSet<String>,
    live: Live,
    busy: bool,
    /// Counts changes to what's shown, so the window rebuilds the list only
    /// when it changed.
    revision: u64,
    /// The last row stamp handed out; stamps are unique within the timeline.
    stamps: u64,
    /// Whether a fade started since [`Timeline::take_fading`] last asked.
    fading: bool,
}

impl Timeline {
    /// Creates an empty timeline for a session whose history is loading.
    #[must_use]
    pub fn new(_session_id: &str) -> Self {
        Self {
            load: Load::Loading,
            rows: Vec::new(),
            index: HashMap::new(),
            items: Vec::new(),
            choices: HashMap::new(),
            whole: HashSet::new(),
            live: Live::new(),
            busy: false,
            revision: 0,
            stamps: 0,
            fading: false,
        }
    }

    /// Returns how many rows the timeline has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Returns whether the timeline has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Returns the rows' entries, oldest first.
    #[must_use]
    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &Entry> {
        self.rows.iter().map(|row| &row.entry)
    }

    /// Replaces the rows with a fresh history.
    ///
    /// Rows whose key and content are unchanged keep their parsed Markdown;
    /// measured heights are kept by key, so a reload doesn't make it jump.
    /// While the session works, streamed rows the history doesn't have yet
    /// stay at the end: OpenCode stores them once they're done.
    pub fn set(&mut self, entries: Vec<Entry>) {
        let old = std::mem::take(&mut self.rows);
        let mut in_flight = Vec::new();
        let mut by_key: HashMap<String, Row> = HashMap::with_capacity(old.len());
        let loaded: std::collections::HashSet<&str> = entries.iter().map(Entry::key).collect();
        for row in old {
            let unsaved = (self.busy && row.live) || row.unsaved;
            if unsaved && !loaded.contains(row.entry.key()) {
                in_flight.push(row);
            } else {
                by_key.insert(row.entry.key().to_owned(), row);
            }
        }
        for entry in entries {
            let row = match by_key.remove(entry.key()) {
                Some(row) if row.entry == entry => row,
                _ => Row::new(entry, false, self.stamp()),
            };
            self.rows.push(row);
        }
        self.rows.extend(in_flight);
        self.load = Load::Ready;
        self.reindex();
    }

    /// Records a failed load; rows already shown stay.
    pub fn fail(&mut self, reason: String) {
        self.load = Load::Failed(reason);
        self.revision += 1;
    }

    /// Whether the session is working.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// Says whether the session is working, which keeps its last tool group open.
    pub fn set_busy(&mut self, busy: bool) {
        if self.busy != busy {
            self.busy = busy;
            self.regroup();
        }
    }

    /// Applies a live event for this session; returns whether the history should be reloaded.
    pub fn apply(&mut self, kind: &EventKind, time: f64) -> bool {
        let mut reload = false;
        let mut structure = false;
        for change in self.live.apply(kind, time) {
            match change {
                Change::Put(entry) => {
                    if let Some(&index) = self.index.get(entry.key()) {
                        structure |=
                            self.rows[index].entry.is_running_tool() != entry.is_running_tool();
                        // OpenCode's copy replaces a prompt Harlo showed while sending it;
                        // a fade under way carries on.
                        let stamp = self.stamp();
                        let appeared = self.rows[index].appeared;
                        self.rows[index] = Row::new(entry, true, stamp);
                        self.rows[index].appeared = appeared;
                    } else {
                        let prompt = matches!(entry, Entry::Prompt { .. });
                        self.push(entry);
                        if prompt {
                            self.fade_in();
                        }
                        structure = true;
                    }
                }
                Change::Extend {
                    key,
                    thought,
                    delta,
                } => match self.index.get(&key).copied() {
                    Some(at)
                        if matches!(
                            self.rows[at].entry,
                            Entry::Answer { .. } | Entry::Thought { .. }
                        ) =>
                    {
                        let stamp = self.stamp();
                        let row = &mut self.rows[at];
                        if let Entry::Answer { text, .. } | Entry::Thought { text, .. } =
                            &mut row.entry
                        {
                            text.push_str(&delta);
                        }
                        row.stamp = stamp;
                    }
                    Some(_) => reload = true,
                    None => {
                        let entry = if thought {
                            Entry::Thought { key, text: delta }
                        } else {
                            Entry::Answer { key, text: delta }
                        };
                        self.push(entry);
                        structure = true;
                    }
                },
                Change::Reload => reload = true,
            }
        }
        if structure {
            self.regroup();
        }
        self.revision += 1;
        reload
    }

    /// Shows a prompt that's on its way to OpenCode.
    ///
    /// It stays, marked as sending, until OpenCode's copy (with the same key)
    /// replaces it, [`Timeline::sent`] marks it sent, or
    /// [`Timeline::unpend`] takes it away.
    pub fn pend(&mut self, entry: Entry) {
        if self.index.contains_key(entry.key()) {
            // OpenCode's copy is already here.
            return;
        }
        self.index.insert(entry.key().to_owned(), self.rows.len());
        let mut row = Row::new(entry, true, self.stamp());
        row.unsaved = true;
        self.rows.push(row);
        self.fade_in();
        self.load = Load::Ready;
        self.regroup();
    }

    /// Fades in the last row, a prompt that just arrived.
    fn fade_in(&mut self) {
        if let Some(row) = self.rows.last_mut() {
            row.appeared = Some(Instant::now());
            self.fading = true;
        }
    }

    /// Whether a fade started since the last call, so the window shows
    /// the row once more when it ends, without its fade.
    pub fn take_fading(&mut self) -> bool {
        std::mem::take(&mut self.fading)
    }

    /// Takes away a pending prompt that couldn't be sent.
    pub fn unpend(&mut self, key: &str) {
        if let Some(&index) = self.index.get(key)
            && self.rows[index].unsaved
        {
            self.rows.remove(index);
            self.reindex();
        }
    }

    /// Returns the text of the prompt with this key, if it's shown.
    #[must_use]
    pub fn prompt_text(&self, key: &str) -> Option<&str> {
        match &self.rows.get(*self.index.get(key)?)?.entry {
            Entry::Prompt { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Returns the newest stored prompt's key, which is its message ID.
    #[must_use]
    pub fn last_prompt(&self) -> Option<&str> {
        self.rows.iter().rev().find_map(|row| match &row.entry {
            Entry::Prompt { key, .. } if !row.unsaved => Some(key.as_str()),
            _ => None,
        })
    }

    /// Returns what the session is doing in its current run, judged from
    /// its newest rows: a running tool, an answer being written, or
    /// thinking (which covers the wait before anything arrives).
    #[must_use]
    pub fn activity(&self) -> Activity {
        let run = self
            .rows
            .iter()
            .rev()
            .take_while(|row| !matches!(row.entry, Entry::Prompt { .. }));
        let mut last = None;
        for row in run {
            if let Entry::Tool {
                label,
                detail,
                status: ToolStatus::Streaming | ToolStatus::Running,
                ..
            } = &row.entry
            {
                return Activity::Running {
                    label: label.clone(),
                    detail: detail.clone(),
                };
            }
            last.get_or_insert(&row.entry);
        }
        match last {
            Some(Entry::Answer { .. }) => Activity::Writing,
            _ => Activity::Thinking,
        }
    }

    fn push(&mut self, entry: Entry) {
        self.index.insert(entry.key().to_owned(), self.rows.len());
        let stamp = self.stamp();
        self.rows.push(Row::new(entry, true, stamp));
    }

    /// A fresh row stamp.
    fn stamp(&mut self) -> u64 {
        self.stamps += 1;
        self.stamps
    }

    fn reindex(&mut self) {
        self.index = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.entry.key().to_owned(), i))
            .collect();
        self.regroup();
    }

    /// Rebuilds the items: rows, with runs of tool calls folded into groups.
    fn regroup(&mut self) {
        let mut items = Vec::with_capacity(self.rows.len());
        let mut at = 0;
        while at < self.rows.len() {
            let Some(run) = self.tool_run(at) else {
                items.push(Item::Row(at));
                at += 1;
                continue;
            };
            let key = self.rows[run.start].entry.key().to_owned();
            let running = self.rows[run.clone()]
                .iter()
                .any(|row| row.entry.is_running_tool())
                || (self.busy && run.end == self.rows.len());
            let open = self.choices.get(&key).copied().unwrap_or(running);
            items.push(Item::Group {
                key,
                rows: run.clone(),
                open,
                running,
            });
            if open {
                items.extend(run.clone().map(Item::Row));
            }
            at = run.end;
        }
        self.items = items;
        self.revision += 1;
    }

    /// The run of rows starting at `at` that folds into a group, if any.
    ///
    /// A run starts and ends with a tool call, may hold thoughts between
    /// calls, and needs at least two calls.
    fn tool_run(&self, at: usize) -> Option<Range<usize>> {
        if !matches!(self.rows[at].entry, Entry::Tool { .. }) {
            return None;
        }
        let mut last_tool = at;
        let mut tools = 1;
        for (i, row) in self.rows.iter().enumerate().skip(at + 1) {
            match row.entry {
                Entry::Tool { .. } => {
                    tools += 1;
                    last_tool = i;
                }
                Entry::Thought { .. } => {}
                _ => break,
            }
        }
        (tools >= 2).then(|| at..last_tool + 1)
    }

    /// Applies a timeline event.
    pub fn update(&mut self, event: &Event) {
        match event {
            Event::ToggleGroup(key) => {
                let open = self
                    .items
                    .iter()
                    .any(|item| matches!(item, Item::Group { key: k, open: true, .. } if k == key));
                self.choices.insert(key.clone(), !open);
                self.regroup();
            }
            Event::ToggleDiff(key) => {
                if !self.whole.remove(key) {
                    self.whole.insert(key.clone());
                }
                self.revision += 1;
            }
            // The app acts on these.
            Event::Link(_) | Event::Undo(_) | Event::OpenSession(_) | Event::Review { .. } => {}
        }
    }

    /// Counts changes to what's shown.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// What the timeline shows, in order.
    #[must_use]
    pub fn items(&self) -> Vec<Shown<'_>> {
        // The open group the rows that follow belong to, if any.
        let mut group: Option<Range<usize>> = None;
        self.items
            .iter()
            .map(|item| match item {
                Item::Row(at) => {
                    let row = &self.rows[*at];
                    let key = row.entry.key();
                    Shown::Row(RowView {
                        entry: &row.entry,
                        stamp: row.stamp,
                        diffs: &row.diffs,
                        whole: self.whole.contains(key),
                        // A run must end first, and a prompt must be stored to be undone.
                        undo: !self.busy && !row.unsaved,
                        nested: group.as_ref().is_some_and(|rows| rows.contains(at)),
                        sending: row.unsaved,
                        fresh: row.appeared.is_some_and(|at| at.elapsed() < APPEAR),
                    })
                }
                Item::Group {
                    key,
                    rows,
                    open,
                    running,
                } => {
                    group = open.then(|| rows.clone());
                    let names = self.rows[rows.clone()]
                        .iter()
                        .filter_map(|row| match &row.entry {
                            Entry::Tool { name, status, .. } => Some((name.as_str(), *status)),
                            _ => None,
                        });
                    Shown::Group {
                        key,
                        summary: tools_summary(names),
                        open: *open,
                        running: *running,
                    }
                }
            })
            .collect()
    }

    /// The row with this key's changed files as diff lines, if it's a tool's.
    #[must_use]
    pub fn diffs(&self, key: &str) -> Option<&[Vec<diff::Line>]> {
        self.rows
            .get(*self.index.get(key)?)
            .map(|row| row.diffs.as_slice())
    }

    /// The entry with this key, if it's shown.
    #[must_use]
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.rows.get(*self.index.get(key)?).map(|row| &row.entry)
    }
}

/// An item the timeline shows.
#[derive(Debug, Clone)]
pub enum Shown<'a> {
    /// A row.
    Row(RowView<'a>),
    /// The header of a run of tool calls.
    Group {
        /// The group's key: its first call's.
        key: &'a str,
        /// What its calls did, in a line.
        summary: String,
        /// Whether its calls show below it.
        open: bool,
        /// Whether one of its calls is running.
        running: bool,
    },
}

/// A row as the timeline shows it.
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about one row, read by the window"
)]
pub struct RowView<'a> {
    /// What the row is.
    pub entry: &'a Entry,
    /// Changes whenever the entry does.
    pub stamp: u64,
    /// A tool's changed files as diff lines.
    pub diffs: &'a [Vec<diff::Line>],
    /// Whether its diffs show whole rather than cut short.
    pub whole: bool,
    /// Whether it can be undone, which the window offers under the pointer:
    /// a stored prompt, with no run going.
    pub undo: bool,
    /// Whether it's inside an open group, and indented.
    pub nested: bool,
    /// Whether it's a prompt still on its way to OpenCode.
    pub sending: bool,
    /// Whether it's a prompt that just arrived, which fades in.
    pub fresh: bool,
}
