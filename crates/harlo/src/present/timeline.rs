//! The open session's timeline as the window's rows.
//!
//! An answer becomes one row per block, and a tool's diff one row per stretch
//! of lines, so no row grows tall however long the session. Rows are built
//! once per change of their entry (its stamp) and kept; each present replaces
//! only the stretch of the list that differs, so a streaming answer touches
//! its last rows and nothing else.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use harlo_core::diff::{self, LineKind};
use harlo_core::theme::{Rgba, Tokens};
use harlo_core::timeline::{Compaction, Entry, summarize};
use harlo_core::workspace::{Report, ToolStatus};
use lucide_icons::Icon;
use slint::{Model, ModelRc, SharedString, StyledText, VecModel};

use super::glyph;
use crate::markdown::{self, Block, Highlighter};
use crate::model::{ChildState, Crew, duration, gist};
use crate::theme::color;
use crate::timeline::{Load, RowView, Shown, Timeline};
use crate::ui::{Chip, DiffLine, Shape, TimelineRow};

/// Space above a row that starts an entry.
const SPACING: f32 = 14.0;

/// Space above the next block of the same answer.
const BLOCK_SPACING: f32 = 10.0;

/// Space above the first row.
const PAD: f32 = 18.0;

/// Diff lines shown before "… N more lines".
const DIFF_LINES: usize = 12;

/// Diff lines per row, when a diff shows whole.
const DIFF_CHUNK: usize = 40;

/// One shown row and what identifies its content.
#[derive(Clone)]
struct Built {
    /// Changes whenever anything the row shows does.
    id: u64,
    row: TimelineRow,
}

/// One timeline item's rows, as last shown.
struct Item {
    /// The entry's key, or a group's; empty for the closing notice.
    key: String,
    /// Changes whenever anything the item's rows show does.
    look: u64,
    rows: Vec<Built>,
}

/// The timeline's rows, kept between presents.
pub struct Rows {
    model: Rc<VecModel<TimelineRow>>,
    /// The items shown, in order, with their rows.
    items: Vec<Item>,
    /// The session the rows are of.
    session: Option<String>,
    /// Answers' blocks and their rows, by key, so a growing answer rebuilds
    /// only the blocks that changed.
    answers: HashMap<String, Vec<(Block, TimelineRow)>>,
    highlighter: Option<Highlighter>,
}

impl std::fmt::Debug for Rows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rows")
            .field("items", &self.items.len())
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

/// An item the timeline shows now, before its rows are built.
enum Next<'a> {
    Shown(Shown<'a>, f32),
    Notice(String, f32),
}

impl Rows {
    /// An empty timeline.
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: Rc::new(VecModel::default()),
            items: Vec::new(),
            session: None,
            answers: HashMap::new(),
            highlighter: None,
        }
    }

    /// The model the window lists.
    #[must_use]
    pub fn model(&self) -> ModelRc<TimelineRow> {
        ModelRc::from(Rc::clone(&self.model))
    }

    /// Shows `timeline`, of session `session`, or nothing.
    ///
    /// Only the items that changed since the last call are built again, and
    /// only the rows that differ change in the model.
    pub fn show(&mut self, t: &Tokens, session: Option<(&str, &Timeline, &Crew)>) {
        let Some((id, timeline, crew)) = session else {
            self.clear();
            return;
        };
        if self.session.as_deref() != Some(id) {
            self.clear();
            self.session = Some(id.to_owned());
        }
        let next = next_items(timeline);
        let looks: Vec<u64> = next.iter().map(|item| item_look(item, crew)).collect();

        // The items alike at both ends stay; the stretch between is rebuilt.
        let prefix = self
            .items
            .iter()
            .zip(&looks)
            .take_while(|(old, look)| old.look == **look)
            .count();
        let room = self.items.len().min(looks.len()) - prefix;
        let suffix = self
            .items
            .iter()
            .rev()
            .zip(looks.iter().rev())
            .take(room)
            .take_while(|(old, look)| old.look == **look)
            .count();
        let row_start: usize = self.items[..prefix].iter().map(|i| i.rows.len()).sum();
        let old: Vec<Item> = self
            .items
            .drain(prefix..self.items.len() - suffix)
            .collect();
        let old_ids: Vec<u64> = old
            .iter()
            .flat_map(|i| i.rows.iter().map(|b| b.id))
            .collect();
        let mut reusable: HashMap<String, Item> =
            old.into_iter().map(|i| (i.key.clone(), i)).collect();

        let mut fresh = Vec::with_capacity(looks.len() - prefix - suffix);
        let end = looks.len() - suffix;
        for (item, &look) in next.into_iter().zip(&looks).take(end).skip(prefix) {
            let key = match &item {
                Next::Shown(Shown::Row(view), _) => view.entry.key(),
                Next::Shown(Shown::Group { key, .. }, _) => key,
                Next::Notice(..) => "",
            };
            let rows = match reusable.remove(key) {
                Some(old) if old.look == look => old.rows,
                _ => self.item_rows(t, item, crew),
            };
            fresh.push(Item {
                key: key.to_owned(),
                look,
                rows,
            });
        }
        // Answers no longer shown let their blocks go.
        for gone in reusable.keys() {
            if !fresh.iter().any(|i| &i.key == gone) {
                self.answers.remove(gone);
            }
        }
        let new_rows: Vec<Built> = fresh.iter().flat_map(|i| i.rows.iter().cloned()).collect();
        self.splice(row_start, &old_ids, new_rows);
        let tail = self.items.split_off(prefix);
        self.items.extend(fresh);
        self.items.extend(tail);
    }

    fn clear(&mut self) {
        self.session = None;
        self.items.clear();
        self.answers.clear();
        self.model.set_vec(Vec::new());
    }

    /// The rows of one item.
    fn item_rows(&mut self, t: &Tokens, item: Next<'_>, crew: &Crew) -> Vec<Built> {
        match item {
            Next::Notice(text, gap) => {
                let mut row = blank(Shape::Notice, "", gap);
                row.label = text.into();
                vec![built(row)]
            }
            Next::Shown(
                Shown::Group {
                    key,
                    summary,
                    open,
                    running,
                },
                gap,
            ) => {
                let mut row = blank(Shape::Group, key, gap);
                row.label = summary.into();
                row.open = open;
                row.working = running;
                vec![built(row)]
            }
            Next::Shown(Shown::Row(view), gap) => {
                let key = view.entry.key();
                let rows = if let Entry::Answer { text, .. } = view.entry {
                    let highlighter = self.highlighter.get_or_insert_with(Highlighter::new);
                    let blocks = self.answers.entry(key.to_owned()).or_default();
                    answer_rows(key, text, gap, highlighter, blocks)
                } else {
                    entry_rows(t, &view, gap, crew)
                };
                rows.into_iter()
                    .map(|row| built(with_nesting(row, view.nested)))
                    .collect()
            }
        }
    }

    /// Replaces the model's rows from `at` that `old` names with `rows`,
    /// changing only those that differ.
    fn splice(&self, at: usize, old: &[u64], rows: Vec<Built>) {
        if old.is_empty() && at == 0 && self.model.row_count() == 0 {
            self.model
                .set_vec(rows.into_iter().map(|b| b.row).collect::<Vec<_>>());
            return;
        }
        let ids: Vec<u64> = rows.iter().map(|b| b.id).collect();
        let prefix = old.iter().zip(&ids).take_while(|(a, b)| a == b).count();
        let room = old.len().min(ids.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(ids.iter().rev())
            .take(room)
            .take_while(|(a, b)| a == b)
            .count();
        let gone = old.len() - prefix - suffix;
        let came = ids.len() - prefix - suffix;
        let start = at + prefix;
        let mut fresh = rows.into_iter().skip(prefix).take(came).map(|b| b.row);
        // Rows in both change in place; the rest come or go.
        let both = gone.min(came);
        for row in start..start + both {
            if let Some(data) = fresh.next() {
                self.model.set_row_data(row, data);
            }
        }
        for (offset, data) in fresh.enumerate() {
            self.model.insert(start + both + offset, data);
        }
        for _ in both..gone {
            self.model.remove(start + both);
        }
    }
}

/// What the timeline shows now, each item with the space above it, then
/// a note on how loading went.
fn next_items(timeline: &Timeline) -> Vec<Next<'_>> {
    let mut next: Vec<Next<'_>> = timeline
        .items()
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let gap = match &item {
                Shown::Row(view) if view.nested => 4.0,
                _ if index == 0 => PAD,
                _ => SPACING,
            };
            Next::Shown(item, gap)
        })
        .collect();
    match &timeline.load {
        Load::Loading if next.is_empty() => {
            next.push(Next::Notice("Loading the session…".into(), 40.0));
        }
        Load::Failed(reason) => {
            let gap = if next.is_empty() { 40.0 } else { SPACING };
            next.push(Next::Notice(
                format!("Couldn't load the session: {reason}"),
                gap,
            ));
        }
        _ => {}
    }
    next
}

/// What an item's rows depend on, as one number.
fn item_look(item: &Next<'_>, crew: &Crew) -> u64 {
    let mut hasher = DefaultHasher::new();
    match item {
        Next::Shown(Shown::Row(view), gap) => {
            0_u8.hash(&mut hasher);
            view.entry.key().hash(&mut hasher);
            look(view, *gap, crew).hash(&mut hasher);
        }
        Next::Shown(
            Shown::Group {
                key,
                summary,
                open,
                running,
            },
            gap,
        ) => {
            1_u8.hash(&mut hasher);
            (key, summary, open, running).hash(&mut hasher);
            gap.to_bits().hash(&mut hasher);
        }
        Next::Notice(text, gap) => {
            2_u8.hash(&mut hasher);
            text.hash(&mut hasher);
            gap.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

impl Default for Rows {
    fn default() -> Self {
        Self::new()
    }
}

/// A row with its content ID.
fn built(row: TimelineRow) -> Built {
    let mut hasher = DefaultHasher::new();
    // What a row shows: its kind, key, words and state. Its text is covered
    // by the stamp its key's entry got, folded into `label`/`detail` or the
    // block index below.
    (row.shape as i32).hash(&mut hasher);
    row.key.as_str().hash(&mut hasher);
    row.label.as_str().hash(&mut hasher);
    row.detail.as_str().hash(&mut hasher);
    row.note.as_str().hash(&mut hasher);
    row.more.as_str().hash(&mut hasher);
    row.gap.to_bits().hash(&mut hasher);
    (row.open, row.working, row.undo, row.sending, row.nested).hash(&mut hasher);
    (row.added, row.removed, row.file, row.level).hash(&mut hasher);
    raw(&row.text).hash(&mut hasher);
    row.lines.row_count().hash(&mut hasher);
    Built {
        id: hasher.finish(),
        row,
    }
}

/// Styled text's characters, for telling rows apart.
fn raw(text: &StyledText) -> String {
    format!("{text:?}")
}

/// What an entry's rows depend on besides the entry: its stamp, how it's
/// shown, and for a subagent how its child stands.
fn look(view: &RowView<'_>, gap: f32, crew: &Crew) -> u64 {
    let mut hasher = DefaultHasher::new();
    view.stamp.hash(&mut hasher);
    (view.whole, view.undo, view.nested, view.sending).hash(&mut hasher);
    gap.to_bits().hash(&mut hasher);
    if let Entry::Subagent {
        child: Some(child), ..
    } = view.entry
    {
        crew.children
            .get(child)
            .copied()
            .map(state_number)
            .hash(&mut hasher);
    }
    hasher.finish()
}

fn state_number(state: ChildState) -> u8 {
    match state {
        ChildState::Working => 0,
        ChildState::Waiting => 1,
        ChildState::Idle => 2,
    }
}

/// A row of `shape` for the entry `key`, with nothing else set.
fn blank(shape: Shape, key: &str, gap: f32) -> TimelineRow {
    TimelineRow {
        shape,
        key: key.into(),
        gap,
        ..TimelineRow::default()
    }
}

/// Parses Markdown the window draws, or shows it as plain text if it can't.
fn styled(markup: &str) -> StyledText {
    StyledText::from_markdown(markup).unwrap_or_else(|_| StyledText::from_plain_text(markup))
}

/// An entry's rows.
fn entry_rows(t: &Tokens, view: &RowView<'_>, gap: f32, crew: &Crew) -> Vec<TimelineRow> {
    let key = view.entry.key();
    let nested = view.nested;
    let with = |mut row: TimelineRow| {
        row.nested = nested;
        row
    };
    match view.entry {
        Entry::Prompt {
            text,
            attachments,
            skills,
            ..
        } => {
            let mut row = blank(Shape::Prompt, key, gap);
            row.label = text.as_str().into();
            row.text = styled(&markdown::escaped(text));
            row.undo = view.undo;
            row.sending = view.sending;
            let chips: Vec<Chip> = attachments
                .iter()
                .map(|name| (Icon::Paperclip, name))
                .chain(skills.iter().map(|name| (Icon::Sparkles, name)))
                .map(|(icon, name)| Chip {
                    glyph: glyph(icon),
                    glyph_color: color(t.icon_base),
                    label: name.as_str().into(),
                    ..Chip::default()
                })
                .collect();
            row.chips = ModelRc::new(VecModel::from(chips));
            vec![row]
        }
        // The caller builds answers, block by block.
        Entry::Answer { .. } => Vec::new(),
        Entry::Thought { text, .. } => {
            let mut row = line(t, key, gap, Icon::Circle, t.icon_weak_base);
            row.label = "Thought".into();
            row.detail = summarize(text).into();
            vec![with(row)]
        }
        Entry::Tool { .. } => tool_rows(t, view, gap).into_iter().map(with).collect(),
        Entry::Stopped { outcome, .. } => {
            let mut row = line(t, key, gap, Icon::CircleAlert, t.icon_warning_base);
            row.label = if outcome == "interrupted" {
                "Stopped"
            } else {
                "The run failed"
            }
            .into();
            vec![with(row)]
        }
        Entry::Note { text, warning, .. } => {
            let mut row = if *warning {
                line(t, key, gap, Icon::CircleAlert, t.icon_warning_base)
            } else {
                line(t, key, gap, Icon::Sparkles, t.icon_base)
            };
            row.label = text.as_str().into();
            vec![with(row)]
        }
        Entry::Compaction { state, .. } => vec![with(compaction_row(t, key, gap, state))],
        Entry::Subagent {
            agent,
            task,
            child,
            status,
            ..
        } => {
            let mut row = blank(Shape::Subagent, key, gap);
            row.label = crew.name(agent).into();
            row.detail = task.as_str().into();
            row.glyph_color = color(crew.color(agent).unwrap_or(t.text_interactive_base));
            row.child = child.as_deref().unwrap_or_default().into();
            let state = child.as_ref().and_then(|id| crew.children.get(id)).copied();
            let (note, note_color, working) = standing(t, *status, state);
            row.note = note.into();
            row.note_color = color(note_color);
            row.working = working;
            vec![with(row)]
        }
        Entry::Report { report, .. } => vec![with(report_row(t, key, gap, crew, report))],
    }
}

/// The row for summarizing the conversation.
fn compaction_row(t: &Tokens, key: &str, gap: f32, state: &Compaction) -> TimelineRow {
    match state {
        Compaction::Running => {
            let mut row = line(t, key, gap, Icon::Loader, t.text_interactive_base);
            row.label = "Summarizing the conversation…".into();
            row.working = true;
            row
        }
        Compaction::Failed(error) => {
            let mut row = line(t, key, gap, Icon::CircleAlert, t.icon_warning_base);
            row.label = "Couldn't summarize the conversation".into();
            row.detail = error.as_str().into();
            row
        }
        Compaction::Done(_) => {
            let mut row = line(t, key, gap, Icon::Archive, t.icon_base);
            row.label = "Summarized the conversation to free up context".into();
            row
        }
    }
}

/// A one-line row with its marker.
fn line(t: &Tokens, key: &str, gap: f32, icon: Icon, tone: Rgba) -> TimelineRow {
    let mut row = blank(Shape::Line, key, gap);
    row.glyph = glyph(icon);
    row.glyph_color = color(tone);
    row.note_color = color(t.text_weaker);
    row
}

/// Marks a row as inside an open group, or not.
fn with_nesting(mut row: TimelineRow, nested: bool) -> TimelineRow {
    row.nested = nested;
    row
}

/// An answer's rows: one per block.
///
/// `blocks` holds the answer's blocks and rows from last time; a block that
/// hasn't changed keeps its row, so a growing answer parses and colours only
/// its newest blocks.
fn answer_rows(
    key: &str,
    text: &str,
    gap: f32,
    highlighter: &Highlighter,
    blocks: &mut Vec<(Block, TimelineRow)>,
) -> Vec<TimelineRow> {
    let fresh = markdown::blocks(text);
    let mut kept = Vec::with_capacity(fresh.len());
    for (index, block) in fresh.into_iter().enumerate() {
        let gap = if index == 0 { gap } else { BLOCK_SPACING };
        let row = match blocks.get(index) {
            Some((old, row)) if *old == block => {
                let mut row = row.clone();
                row.gap = gap;
                row
            }
            _ => block_row(key, index, &block, gap, highlighter),
        };
        kept.push((block, row));
    }
    let rows = kept.iter().map(|(_, row)| row.clone()).collect();
    *blocks = kept;
    rows
}

/// The row for block `index` of an answer.
fn block_row(
    key: &str,
    index: usize,
    block: &Block,
    gap: f32,
    highlighter: &Highlighter,
) -> TimelineRow {
    let mut row = match block {
        Block::Text(source) => {
            let mut row = blank(Shape::Text, key, gap);
            row.text = styled(source);
            row
        }
        Block::Heading { level, text } => {
            let mut row = blank(Shape::Heading, key, gap);
            row.text = styled(&format!("**{}**", text.trim_matches('*')));
            row.level = i32::from(*level);
            row
        }
        Block::Code { language, text } => {
            let mut row = blank(Shape::Code, key, gap);
            row.text = styled(&highlighter.markup(language, text));
            row
        }
        Block::Table(text) => {
            let mut row = blank(Shape::Code, key, gap);
            row.text = styled(&markdown::escaped(text));
            row
        }
        Block::Quote(source) => {
            let mut row = blank(Shape::Quote, key, gap);
            row.text = styled(source);
            row
        }
        Block::Rule => blank(Shape::Rule, key, gap),
    };
    // Blocks of one answer differ by their place.
    row.file = i32::try_from(index).unwrap_or(i32::MAX);
    row
}

/// A tool's row, then its diffs.
fn tool_rows(t: &Tokens, view: &RowView<'_>, gap: f32) -> Vec<TimelineRow> {
    let Entry::Tool {
        key,
        label,
        detail,
        status,
        took,
        files,
        ..
    } = view.entry
    else {
        return Vec::new();
    };
    let (icon, tone, working) = match status {
        ToolStatus::Completed => (Icon::Dot, t.icon_base, false),
        ToolStatus::Failed => (Icon::X, t.icon_critical_base, false),
        ToolStatus::Running | ToolStatus::Streaming => {
            (Icon::Loader, t.text_interactive_base, true)
        }
    };
    let mut head = line(t, key, gap, icon, tone);
    head.label = label.as_str().into();
    head.detail = detail.as_str().into();
    head.mono = true;
    head.working = working;
    head.note = took.map(duration).unwrap_or_default().into();
    if !files.is_empty() {
        head.counts = true;
        head.added = count(files.iter().map(|f| f.additions).sum());
        head.removed = count(files.iter().map(|f| f.deletions).sum());
    }
    let mut rows = vec![head];
    let limit = if view.whole { usize::MAX } else { DIFF_LINES };
    for (index, (file, lines)) in files.iter().zip(view.diffs).enumerate() {
        let hidden = lines.len().saturating_sub(limit);
        let shown = &lines[..lines.len().min(limit)];
        let chunks: Vec<&[diff::Line]> = if shown.is_empty() {
            vec![&[]]
        } else {
            shown.chunks(DIFF_CHUNK).collect()
        };
        let last = chunks.len() - 1;
        for (at, chunk) in chunks.into_iter().enumerate() {
            let mut row = blank(Shape::Diff, key, if at == 0 { 6.0 } else { 0.0 });
            row.file = i32::try_from(index).unwrap_or(i32::MAX);
            row.top = at == 0;
            row.bottom = at == last;
            // A single file is named on the tool's row; several get headers.
            if at == 0 && files.len() > 1 {
                row.label = file.file.as_str().into();
                row.added = count(file.additions);
                row.removed = count(file.deletions);
            }
            row.lines = ModelRc::new(VecModel::from(
                chunk.iter().map(diff_line).collect::<Vec<_>>(),
            ));
            if at == last {
                row.more = if hidden > 0 {
                    format!("… {hidden} more lines").into()
                } else if view.whole && lines.len() > DIFF_LINES {
                    "Show less".into()
                } else {
                    SharedString::new()
                };
            }
            // Each piece is told apart by its place.
            row.detail = format!("{index}:{at}").into();
            rows.push(row);
        }
    }
    rows
}

pub(super) fn diff_line(line: &diff::Line) -> DiffLine {
    let (mark, kind) = match line.kind {
        LineKind::Added => ("+", 1),
        LineKind::Removed => ("−", 2),
        LineKind::Context => ("", 0),
        LineKind::Gap => ("", 3),
    };
    DiffLine {
        number: line
            .number
            .map(|n| n.to_string())
            .unwrap_or_default()
            .into(),
        mark: mark.into(),
        text: line.text().replace('\t', "    ").into(),
        kind,
    }
}

/// A count as the UI's `int`.
fn count(n: u64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// What a subagent's card says about where it is, its colour, and whether
/// it breathes.
fn standing(
    t: &Tokens,
    status: ToolStatus,
    state: Option<ChildState>,
) -> (&'static str, Rgba, bool) {
    match (status, state) {
        (ToolStatus::Failed, _) => ("Couldn't start", t.icon_critical_base, false),
        (ToolStatus::Streaming | ToolStatus::Running, _) => {
            ("Starting…", t.text_interactive_base, true)
        }
        (ToolStatus::Completed, Some(ChildState::Working)) => {
            ("Working", t.text_interactive_base, true)
        }
        (ToolStatus::Completed, Some(ChildState::Waiting)) => {
            ("Needs you", t.icon_warning_base, false)
        }
        (ToolStatus::Completed, Some(ChildState::Idle)) => ("Done", t.text_weak, false),
        (ToolStatus::Completed, None) => ("", t.text_weak, false),
    }
}

/// The row for a subagent's report, with its first lines below.
fn report_row(t: &Tokens, key: &str, gap: f32, crew: &Crew, report: &Report) -> TimelineRow {
    let agent = crew.name(&report.agent);
    let mut row = blank(Shape::Report, key, gap);
    if report.state == "completed" {
        row.glyph = glyph(Icon::CornerDownLeft);
        row.glyph_color = color(t.icon_base);
        row.label = format!("{agent} reported back").into();
    } else {
        row.glyph = glyph(Icon::CircleAlert);
        row.glyph_color = color(t.icon_warning_base);
        row.label = format!("{agent} stopped: {}", report.state).into();
    }
    row.detail = report.task.as_str().into();
    row.child = report.child.as_str().into();
    let gist = gist(&report.text);
    if !gist.is_empty() {
        row.text = styled(&markdown::escaped(&gist));
        row.note = gist.into();
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_of(rows: &Rows) -> Vec<TimelineRow> {
        (0..rows.model.row_count())
            .filter_map(|i| rows.model.row_data(i))
            .collect()
    }

    fn row(label: &str) -> Built {
        let mut row = blank(Shape::Line, label, 0.0);
        row.label = label.into();
        built(row)
    }

    #[test]
    fn splice_changes_only_the_stretch_that_differs() {
        let rows = Rows::new();
        let ids = |list: &[Built]| list.iter().map(|b| b.id).collect::<Vec<_>>();
        let first = ["a", "b", "c", "d"].map(row).to_vec();
        rows.splice(0, &[], first.clone());
        let second = ["a", "x", "y", "c", "d"].map(row).to_vec();
        rows.splice(0, &ids(&first), second.clone());
        let labels: Vec<String> = rows_of(&rows).iter().map(|r| r.label.to_string()).collect();
        assert_eq!(labels, ["a", "x", "y", "c", "d"]);
        // A stretch inside: rows 1.. are replaced, row 0 stays.
        let third = ["d"].map(row).to_vec();
        rows.splice(1, &ids(&second[1..]), third.clone());
        let labels: Vec<String> = rows_of(&rows).iter().map(|r| r.label.to_string()).collect();
        assert_eq!(labels, ["a", "d"]);
        rows.splice(1, &ids(&third), ["d", "e"].map(row).to_vec());
        let labels: Vec<String> = rows_of(&rows).iter().map(|r| r.label.to_string()).collect();
        assert_eq!(labels, ["a", "d", "e"]);
    }
}
