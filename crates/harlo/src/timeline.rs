//! A session's timeline: rows pinned in stack slots, built only near the viewport.
//!
//! Long sessions have thousands of rows. Building them all each frame stutters,
//! so only items near the viewport are built. Item `i` sits in stack slot
//! `i % SLOTS` at its offset, which keeps its widget state (shaped text) for as
//! long as it stays near the viewport. A `sensor` reports each built item's
//! real height, stored by the row's key; until then an estimate stands in.
//! The scrollable is anchored to the bottom, so the newest rows stay put
//! while heights above them settle. Once the user scrolls up, the row at the
//! top of the view is held in place instead: rows streaming in below, or a
//! group opening in view, don't push what they're reading away.
//!
//! Runs of two or more tool calls fold into a group with a one-line summary.
//! A group is open while one of its calls runs, and closed once they're done,
//! unless the user opened or closed it.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::ops::Range;

use harlo_core::theme::{Rgba, Tokens};
use harlo_core::timeline::{Change, Compaction, Entry, Live, summarize, tools_summary};
use harlo_core::workspace::{EventKind, ToolStatus};
use iced::animation::Animation;
use iced::time::Instant;
use iced::widget::scrollable::AbsoluteOffset;
use iced::widget::{
    Id, button, center, column, container, markdown, row, scrollable, sensor, space, stack, text,
};
use iced::{Center, Element, Fill, Size, Task, padding};
use lucide_icons::Icon;

use crate::motion::{self, Moment};
use crate::style::{self, size};
use crate::view::icon;

/// Stack slots for items near the viewport.
const SLOTS: usize = 64;

/// Pixels above and below the viewport whose items stay built.
const OVERSCAN: f32 = 900.0;

/// Gap between items.
const SPACING: f32 = 14.0;

/// Height of one-line rows: thoughts, tools, notes, group headers.
const LINE: f32 = 24.0;

/// The timeline column's widest size; it narrows with the window.
pub const WIDTH: f32 = 780.0;

/// A guess at the viewport height before the first scroll report.
const FIRST_VIEWPORT: f32 = 1_000.0;

/// Padding above and below the rows.
const PAD: f32 = 18.0;

/// Within this distance of the end, the view follows the newest rows.
const FOLLOW: f32 = 4.0;

/// Scroll reports that may disagree with a requested offset before it's dropped.
const STALE_REPORTS: u8 = 4;

/// Something the timeline reports.
#[derive(Debug, Clone)]
pub enum Event {
    /// The view scrolled or resized.
    Scrolled {
        /// How far the view's bottom is above the end of the rows.
        from_bottom: f32,
        /// The view's height.
        height: f32,
    },
    /// An item was measured: its key and size.
    Measured(String, Size),
    /// A link in an answer was clicked.
    Link(String),
    /// A tool group was opened or closed, by its key.
    ToggleGroup(String),
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

/// One row: its entry, and the parsed Markdown for answers.
#[derive(Debug)]
struct Row {
    entry: Entry,
    markdown: Option<markdown::Content>,
    /// Built from live events rather than loaded.
    live: bool,
    /// A prompt Harlo sent that OpenCode's copy hasn't replaced yet; reloads keep it.
    unsaved: bool,
}

impl Row {
    fn new(entry: Entry, live: bool) -> Self {
        let markdown = match &entry {
            Entry::Answer { text, .. } => Some(markdown::Content::parse(text)),
            _ => None,
        };
        Self {
            entry,
            markdown,
            live,
            unsaved: false,
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

/// The item held in place while the user reads above the end.
#[derive(Debug, Clone, PartialEq)]
struct Anchor {
    /// The item's height key.
    key: String,
    /// How far the view's top is below the item's top.
    offset: f32,
}

/// An offset the view was asked to scroll to, until the scrollable reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pending {
    from_bottom: f32,
    /// Whether the scroll still needs to be sent.
    unsent: bool,
    /// Reports that disagreed since it was sent.
    stale: u8,
}

/// A session's rows and how far the view is scrolled.
#[derive(Debug)]
pub struct Timeline {
    scroller: Id,
    /// Where loading stands.
    pub load: Load,
    rows: Vec<Row>,
    index: HashMap<String, usize>,
    items: Vec<Item>,
    heights: HashMap<String, f32>,
    /// Groups the user opened (`true`) or closed (`false`).
    choices: HashMap<String, bool>,
    live: Live,
    busy: bool,
    from_bottom: f32,
    viewport: f32,
    anchor: Option<Anchor>,
    pending: Option<Pending>,
    /// Prompts fading in, by row key.
    appear: HashMap<String, Animation<bool>>,
    /// When the last fade-in ends.
    settle: Option<Instant>,
}

impl Timeline {
    /// Creates an empty timeline for a session whose history is loading.
    #[must_use]
    pub fn new(session_id: &str) -> Self {
        Self {
            scroller: Id::from(format!("timeline-{session_id}")),
            load: Load::Loading,
            rows: Vec::new(),
            index: HashMap::new(),
            items: Vec::new(),
            heights: HashMap::new(),
            choices: HashMap::new(),
            live: Live::new(),
            busy: false,
            from_bottom: 0.0,
            viewport: FIRST_VIEWPORT,
            anchor: None,
            pending: None,
            appear: HashMap::new(),
            settle: None,
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
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
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
                _ => Row::new(entry, false),
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
    }

    /// Says whether the session is working, which keeps its last tool group open.
    pub fn set_busy(&mut self, busy: bool) {
        if self.busy != busy {
            self.busy = busy;
            self.regroup();
        }
    }

    /// Applies a live event for this session; returns whether the history should be reloaded.
    ///
    /// A prompt that wasn't here before fades in from `at`.
    pub fn apply(&mut self, kind: &EventKind, time: f64, at: Instant) -> bool {
        let mut reload = false;
        let mut structure = false;
        for change in self.live.apply(kind, time) {
            match change {
                Change::Put(entry) => {
                    if let Some(&index) = self.index.get(entry.key()) {
                        structure |=
                            self.rows[index].entry.is_running_tool() != entry.is_running_tool();
                        // OpenCode's copy replaces a prompt Harlo showed while sending it.
                        self.rows[index] = Row::new(entry, true);
                    } else {
                        if matches!(entry, Entry::Prompt { .. }) {
                            self.fade_in(entry.key(), at);
                        }
                        self.push(entry);
                        structure = true;
                    }
                }
                Change::Extend {
                    key,
                    thought,
                    delta,
                } => match self.index.get(&key).map(|&at| &mut self.rows[at]) {
                    Some(Row {
                        entry: Entry::Answer { text, .. },
                        markdown,
                        ..
                    }) => {
                        text.push_str(&delta);
                        if let Some(markdown) = markdown {
                            markdown.push_str(&delta);
                        }
                    }
                    Some(Row {
                        entry: Entry::Thought { text, .. },
                        ..
                    }) => text.push_str(&delta),
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
        reload
    }

    /// Shows a prompt that's on its way to OpenCode, fading in from `at`.
    ///
    /// It stays, marked as sending, until OpenCode's copy (with the same key)
    /// replaces it, [`Timeline::sent`] marks it sent, or
    /// [`Timeline::unpend`] takes it away.
    pub fn pend(&mut self, entry: Entry, at: Instant) {
        self.fade_in(entry.key(), at);
        if self.index.contains_key(entry.key()) {
            // OpenCode's copy is already here.
            return;
        }
        self.index.insert(entry.key().to_owned(), self.rows.len());
        let mut row = Row::new(entry, true);
        row.unsaved = true;
        self.rows.push(row);
        self.load = Load::Ready;
        self.regroup();
    }

    /// Takes away a pending prompt that couldn't be sent.
    pub fn unpend(&mut self, key: &str) {
        if let Some(&index) = self.index.get(key)
            && self.rows[index].unsaved
        {
            self.rows.remove(index);
            self.appear.remove(key);
            self.reindex();
        }
    }

    /// Scrolls to the newest rows and follows them again.
    pub fn to_end(&mut self) {
        self.anchor = None;
        self.from_bottom = 0.0;
        self.pending = Some(Pending {
            from_bottom: 0.0,
            unsent: true,
            stale: 0,
        });
    }

    /// Returns when the last fade-in ends, if any started.
    #[must_use]
    pub fn settles(&self) -> Option<Instant> {
        self.settle
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

    fn fade_in(&mut self, key: &str, at: Instant) {
        self.appear.insert(key.to_owned(), motion::appear(at));
        let end = at + motion::APPEAR;
        self.settle = Some(self.settle.map_or(end, |settle| settle.max(end)));
    }

    fn push(&mut self, entry: Entry) {
        self.index.insert(entry.key().to_owned(), self.rows.len());
        self.rows.push(Row::new(entry, true));
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
        self.reanchor();
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
            &Event::Scrolled {
                from_bottom: reported,
                height,
            } => {
                self.viewport = height;
                if let Some(pending) = &mut self.pending {
                    // Reports from before the requested scroll took effect
                    // are stale; a few in a row mean it never will.
                    if (reported - pending.from_bottom).abs() < 1.0 {
                        self.pending = None;
                        return;
                    }
                    if pending.unsent || pending.stale < STALE_REPORTS {
                        pending.stale += 1;
                        return;
                    }
                    self.pending = None;
                }
                self.from_bottom = reported;
                self.anchor = self.capture();
            }
            Event::Measured(key, size) => {
                self.heights.insert(key.clone(), size.height);
                self.reanchor();
            }
            Event::ToggleGroup(key) => {
                let open = self
                    .items
                    .iter()
                    .any(|item| matches!(item, Item::Group { key: k, open: true, .. } if k == key));
                // The header the user clicked stays where it is.
                let header = format!("group:{key}");
                if let Some(top) = self.top_of(&header) {
                    self.anchor = Some(Anchor {
                        key: header,
                        offset: self.view_top() - top,
                    });
                }
                self.choices.insert(key.clone(), !open);
                self.regroup();
            }
            Event::Link(_) => {}
        }
    }

    /// Returns how far the view's bottom is above the end of the rows.
    ///
    /// Zero while the view follows the newest rows.
    #[must_use]
    pub fn from_bottom(&self) -> f32 {
        self.from_bottom
    }

    /// Returns a scroll that keeps the held row in place, if one is due.
    ///
    /// Call it after [`Timeline::update`], [`Timeline::set`] and
    /// [`Timeline::apply`], and run the task it returns.
    pub fn scroll<T: Send + 'static>(&mut self) -> Task<T> {
        match &mut self.pending {
            Some(pending) if pending.unsent => {
                pending.unsent = false;
                iced::widget::operation::scroll_to(
                    self.scroller.clone(),
                    AbsoluteOffset {
                        x: None,
                        y: Some(pending.from_bottom),
                    },
                )
            }
            _ => Task::none(),
        }
    }

    /// The rows' total height, without the padding around them.
    fn total(&self) -> f32 {
        let sum: f32 = self
            .items
            .iter()
            .map(|item| self.height(item) + SPACING)
            .sum();
        (sum - SPACING).max(0.0)
    }

    /// Where the view's top is, measured from the first row's top.
    fn view_top(&self) -> f32 {
        self.total() + PAD - self.viewport - self.from_bottom
    }

    /// Where the item with the height key `key` starts, if it's shown.
    fn top_of(&self, key: &str) -> Option<f32> {
        let mut top = 0.0;
        for item in &self.items {
            if self.item_key(item) == key {
                return Some(top);
            }
            top += self.height(item) + SPACING;
        }
        None
    }

    /// The item at the top of the view, unless the view follows the end.
    fn capture(&self) -> Option<Anchor> {
        if self.from_bottom <= FOLLOW {
            return None;
        }
        let view_top = self.view_top();
        let mut top = 0.0;
        for item in &self.items {
            let height = self.height(item);
            if top + height + SPACING > view_top {
                return Some(Anchor {
                    key: self.item_key(item),
                    offset: view_top - top,
                });
            }
            top += height + SPACING;
        }
        None
    }

    /// Scrolls so the held item is where it was, after heights or rows changed.
    fn reanchor(&mut self) {
        let Some(anchor) = &self.anchor else {
            return;
        };
        let Some(top) = self.top_of(&anchor.key) else {
            self.anchor = None;
            return;
        };
        let wanted = top + anchor.offset;
        let from_bottom = (self.total() + PAD - self.viewport - wanted).max(0.0);
        if (from_bottom - self.from_bottom).abs() < 0.5 {
            return;
        }
        self.from_bottom = from_bottom;
        self.pending = Some(Pending {
            from_bottom,
            unsent: true,
            stale: 0,
        });
        if from_bottom <= FOLLOW {
            // Back at the end: follow the newest rows again.
            self.anchor = None;
        }
    }

    /// The key an item's height is stored under.
    fn item_key(&self, item: &Item) -> String {
        match item {
            Item::Row(at) => self.rows[*at].entry.key().to_owned(),
            Item::Group { key, .. } => format!("group:{key}"),
        }
    }

    fn height(&self, item: &Item) -> f32 {
        self.heights
            .get(&self.item_key(item))
            .copied()
            .unwrap_or_else(|| match item {
                Item::Row(at) => estimate(&self.rows[*at].entry),
                Item::Group { .. } => LINE,
            })
    }

    /// The timeline, scrolled with the newest rows at the bottom.
    #[must_use]
    ///
    /// New prompts fade in as of `moment`.
    pub fn view<'a>(&'a self, t: &Tokens, moment: Moment) -> Element<'a, Event> {
        let body: Element<'a, Event> = match (&self.load, self.rows.is_empty()) {
            (Load::Loading, true) => return note(t, "Loading…"),
            (Load::Failed(reason), true) => {
                return note(t, &format!("Couldn't load this session: {reason}"));
            }
            (_, true) => return note(t, "No messages yet."),
            _ => self.slots(t, moment),
        };
        let column = container(body)
            .max_width(WIDTH)
            .padding(padding::vertical(PAD));
        scrollable(container(column).center_x(Fill).padding([0, 24]))
            .id(self.scroller.clone())
            .anchor_bottom()
            // Anchored to the bottom, the offset counts up from the end.
            .on_scroll(|viewport| Event::Scrolled {
                from_bottom: viewport.absolute_offset().y,
                height: viewport.bounds().height,
            })
            .height(Fill)
            .style(style::thin_scroll(t))
            .into()
    }

    /// Pins the items near the viewport at their offsets, item `i` in slot `i % SLOTS`.
    fn slots<'a>(&'a self, t: &Tokens, moment: Moment) -> Element<'a, Event> {
        let heights: Vec<f32> = self.items.iter().map(|item| self.height(item)).collect();
        let total: f32 = heights.iter().map(|h| h + SPACING).sum::<f32>() - SPACING;
        let bottom = total - self.from_bottom;
        let (low, high) = (bottom - self.viewport - OVERSCAN, bottom + OVERSCAN);
        let mut slots: Vec<Option<Element<'a, Event>>> = (0..SLOTS).map(|_| None).collect();
        let mut top = 0.0;
        for (i, (item, height)) in self.items.iter().zip(&heights).enumerate() {
            if top + height >= low && top <= high {
                slots[i % SLOTS] = Some(self.measured(t, item, top, moment));
            }
            top += height + SPACING;
        }
        let mut layers: Vec<Element<'a, Event>> = Vec::with_capacity(SLOTS + 1);
        layers.push(space().width(Fill).height(total.max(0.0)).into());
        layers.extend(
            slots
                .into_iter()
                .map(|slot| slot.unwrap_or_else(|| space().into())),
        );
        stack(layers).width(Fill).into()
    }

    fn measured<'a>(
        &'a self,
        t: &Tokens,
        item: &'a Item,
        top: f32,
        moment: Moment,
    ) -> Element<'a, Event> {
        let mut rise = 0.0;
        let (key, content): (&'a str, Element<'a, Event>) = match item {
            Item::Row(at) => {
                let row = &self.rows[*at];
                let shown = self
                    .appear
                    .get(row.entry.key())
                    .map_or(1.0, |appear| motion::shown(appear, moment.now));
                rise = (1.0 - shown) * motion::RISE;
                let view = row_view(t, row, shown);
                let view = if self.in_open_group(*at) {
                    container(view).padding(padding::left(22)).into()
                } else {
                    view
                };
                (row.entry.key(), view)
            }
            Item::Group {
                key,
                rows,
                open,
                running,
            } => (
                key.as_str(),
                self.group_header(t, key, rows.clone(), *open, *running),
            ),
        };
        let shown = self.item_key(item);
        let resized = shown.clone();
        let content = sensor(content)
            .key_ref(key)
            .anticipate(OVERSCAN * 2.0)
            .on_show(move |size| Event::Measured(shown.clone(), size))
            .on_resize(move |size| Event::Measured(resized.clone(), size));
        crate::widget::place(container(content).width(Fill), top + rise).into()
    }

    fn in_open_group(&self, at: usize) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item, Item::Group { rows, open: true, .. } if rows.contains(&at)))
    }

    fn group_header<'a>(
        &self,
        t: &Tokens,
        key: &str,
        rows: Range<usize>,
        open: bool,
        running: bool,
    ) -> Element<'a, Event> {
        let names = self.rows[rows].iter().filter_map(|row| match &row.entry {
            Entry::Tool { name, .. } => Some(name.as_str()),
            _ => None,
        });
        let summary = tools_summary(names);
        let marker = if running {
            icon(Icon::Loader, 13.0, t.text_interactive_base)
        } else if open {
            icon(Icon::ChevronDown, 13.0, t.icon_base)
        } else {
            icon(Icon::ChevronRight, 13.0, t.icon_base)
        };
        let content = row![
            container(marker).width(14).center_x(14),
            text(summary)
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        ]
        .spacing(9)
        .align_y(Center)
        .height(LINE);
        button(content)
            .padding(0)
            .width(Fill)
            .on_press(Event::ToggleGroup(key.to_owned()))
            .style(style::ghost(t, false))
            .into()
    }
}

fn note<'a>(t: &Tokens, message: &str) -> Element<'a, Event> {
    center(
        text(message.to_owned())
            .size(size::LABEL)
            .color(style::color(t.text_weak)),
    )
    .into()
}

/// A first guess at a row's height, before it's measured.
#[expect(
    clippy::cast_precision_loss,
    reason = "line counts fit easily in an f32"
)]
fn estimate(entry: &Entry) -> f32 {
    let lines = |text: &str| {
        text.lines()
            .map(|line| line.chars().count() / 95 + 1)
            .sum::<usize>() as f32
    };
    match entry {
        Entry::Prompt {
            text,
            attachments,
            skills,
            ..
        } => {
            let chips = if attachments.is_empty() && skills.is_empty() {
                0.0
            } else {
                32.0
            };
            lines(text) * 22.0 + 26.0 + chips
        }
        Entry::Answer { text, .. } => lines(text) * 24.0,
        Entry::Thought { .. }
        | Entry::Tool { .. }
        | Entry::Stopped { .. }
        | Entry::Compaction { .. }
        | Entry::Note { .. } => LINE,
    }
}

/// Markdown settings for answers, from the theme tokens.
fn markdown_settings(t: &Tokens) -> markdown::Settings {
    let mut settings = markdown::Settings::with_text_size(
        size::ANSWER,
        markdown::Style {
            font: style::SANS,
            inline_code_highlight: iced::advanced::text::Highlight {
                background: style::color(t.surface_raised_strong).into(),
                border: iced::border::rounded(3),
            },
            inline_code_padding: padding::left(2).right(2),
            inline_code_color: style::color(t.icon_success_base.over(t.text_base, 0.7)),
            inline_code_font: style::MONO,
            code_block_font: style::MONO,
            link_color: style::color(t.text_interactive_base),
        },
    );
    settings.code_size = size::CODE.into();
    settings.h1_size = (size::ANSWER * 1.5).into();
    settings.h2_size = (size::ANSWER * 1.3).into();
    settings.h3_size = (size::ANSWER * 1.15).into();
    settings
}

/// A row's view. A prompt is drawn `shown` faded in (0 to 1).
fn row_view<'a>(t: &Tokens, row: &'a Row, shown: f32) -> Element<'a, Event> {
    match &row.entry {
        Entry::Prompt {
            text: prompt,
            attachments,
            skills,
            ..
        } => {
            let fade = |c: Rgba| motion::fade(c, shown);
            let mut body = column![].spacing(8);
            // A prompt can be only attachments, such as a pasted screenshot.
            if !prompt.is_empty() {
                body = body.push(
                    text(prompt.as_str())
                        .size(size::ANSWER)
                        .color(style::color(fade(t.text_strong))),
                );
            }
            if !attachments.is_empty() || !skills.is_empty() {
                body = body.push(chips(t, attachments, skills, shown));
            }
            container(body)
                .width(Fill)
                .padding([11, 16])
                .style(style::boxed(
                    fade(t.surface_raised_base),
                    Some(fade(t.border_weak_base)),
                    size::COMPOSER_RADIUS,
                ))
                .into()
        }
        Entry::Answer { .. } => {
            let items = row
                .markdown
                .as_ref()
                .map_or(&[][..], markdown::Content::items);
            markdown::view(items, markdown_settings(t)).map(Event::Link)
        }
        Entry::Thought { text, .. } => line(
            t,
            icon(Icon::Circle, 8.0, t.icon_weak_base),
            "Thought".to_owned(),
            summarize(text),
            None,
            false,
        ),
        Entry::Tool {
            label,
            detail,
            status,
            took,
            ..
        } => tool_row(t, label, detail, *status, *took),
        Entry::Stopped { outcome, .. } => {
            let what = if outcome == "interrupted" {
                "Stopped"
            } else {
                "The run failed"
            };
            line(
                t,
                icon(Icon::CircleAlert, 13.0, t.icon_warning_base),
                what.to_owned(),
                String::new(),
                None,
                false,
            )
        }
        Entry::Note { text, warning, .. } => {
            let marker = if *warning {
                icon(Icon::CircleAlert, 13.0, t.icon_warning_base)
            } else {
                icon(Icon::Sparkles, 13.0, t.icon_base)
            };
            line(t, marker, text.clone(), String::new(), None, false)
        }
        Entry::Compaction { state, .. } => compaction_row(t, state),
    }
}

fn tool_row<'a>(
    t: &Tokens,
    label: &str,
    detail: &str,
    status: ToolStatus,
    took: Option<std::time::Duration>,
) -> Element<'a, Event> {
    let (glyph, color) = match status {
        ToolStatus::Completed => (Icon::Dot, t.icon_base),
        ToolStatus::Failed => (Icon::X, t.icon_critical_base),
        ToolStatus::Running | ToolStatus::Streaming => (Icon::Loader, t.text_interactive_base),
    };
    line(
        t,
        icon(glyph, 14.0, color),
        label.to_owned(),
        detail.to_owned(),
        took.map(crate::view::duration),
        true,
    )
}

fn compaction_row<'a>(t: &Tokens, state: &Compaction) -> Element<'a, Event> {
    let (marker, label, detail) = match state {
        Compaction::Running => (
            icon(Icon::Loader, 13.0, t.text_interactive_base),
            "Summarizing the conversation…",
            String::new(),
        ),
        Compaction::Failed(error) => (
            icon(Icon::CircleAlert, 13.0, t.icon_warning_base),
            "Couldn't summarize the conversation",
            error.clone(),
        ),
        Compaction::Done(_) => (
            icon(Icon::Archive, 13.0, t.icon_base),
            "Summarized the conversation to free up context",
            String::new(),
        ),
    };
    line(t, marker, label.to_owned(), detail, None, false)
}

/// Attachment and skill names as small chips.
fn chips<'a>(
    t: &Tokens,
    attachments: &'a [String],
    skills: &'a [String],
    shown: f32,
) -> Element<'a, Event> {
    let fade = |c: Rgba| motion::fade(c, shown);
    let mut list = row![].spacing(6);
    let named = attachments
        .iter()
        .map(|name| (Icon::Paperclip, name))
        .chain(skills.iter().map(|name| (Icon::Sparkles, name)));
    for (glyph, name) in named {
        list = list.push(
            container(
                row![
                    icon(glyph, 11.0, fade(t.icon_base)),
                    text(name.as_str())
                        .size(size::SMALL)
                        .color(style::color(fade(t.text_base))),
                ]
                .spacing(5)
                .align_y(Center),
            )
            .padding([3, 8])
            .style(style::boxed(
                fade(t.surface_raised_strong),
                None,
                size::RADIUS,
            )),
        );
    }
    list.wrap().into()
}

/// A one-line row: a marker, a label, its detail and a time on the right.
fn line<'a, M: Into<Element<'a, Event>>>(
    t: &Tokens,
    marker: M,
    label: String,
    detail: String,
    took: Option<String>,
    mono: bool,
) -> Element<'a, Event> {
    let mut content = row![container(marker.into()).width(14).center_x(14)]
        .spacing(9)
        .align_y(Center);
    // The row is one line tall: whichever text is last shortens to fit.
    if detail.is_empty() {
        content = content.push(
            crate::widget::ellipsis(label)
                .size(size::SMALL)
                .font(style::SANS_MEDIUM)
                .color(style::color(t.text_base)),
        );
    } else {
        content = content
            .push(
                text(label)
                    .size(size::SMALL)
                    .font(style::SANS_MEDIUM)
                    .color(style::color(t.text_base)),
            )
            .push(
                crate::widget::ellipsis(detail)
                    .size(if mono { size::CODE } else { size::SMALL })
                    .font(if mono { style::MONO } else { style::SANS })
                    .color(style::color(t.text_weak)),
            );
    }
    if let Some(took) = took {
        content = content.push(
            text(took)
                .size(size::SMALL)
                .color(style::color(t.text_weaker)),
        );
    }
    column![content].height(LINE).into()
}
