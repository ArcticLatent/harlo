//! The Changes panel: its lists flattened into rows, the selected file's
//! diff when maximized, and its foot with Commit and Push.
//!
//! Every row's height is set here, so where a row sits is known without
//! the window laying it out: that's how the panel scrolls to the lit hunk.

// Rust guideline compliant 2026-10-09

use std::rc::Rc;

use harlo_core::diff::{Line, LineKind};
use slint::{SharedString, VecModel};

use super::{
    sync,
    timeline::{diff_line, digits},
};
use crate::app::{App, ChangedFile, Changes, GitAction, List, Panel};
use crate::file_icons;
use crate::ui::{ChangeRow, ChangesFoot, DiffLine, State};

/// The most lines a file's diff shows under it before "N more lines".
const DIFF_LINES: usize = 40;

/// Row heights, and the gaps above them.
mod height {
    /// The space above the first row.
    pub const TOP: f32 = 16.0;
    /// The space above a section after the first.
    pub const SECTION_GAP: f32 = 22.0;
    /// A section's title.
    pub const TITLE: f32 = 24.0;
    /// The space between a title and what follows.
    pub const UNDER_TITLE: f32 = 6.0;
    /// The turn's question, and a note.
    pub const NOTE: f32 = 22.0;
    /// A file.
    pub const FILE: f32 = 30.0;
    /// The space between files.
    pub const FILE_GAP: f32 = 2.0;
    /// A diff line; `Column.diff-line` in the UI.
    pub const LINE: f32 = 19.0;
    /// "N more lines".
    pub const MORE: f32 = 26.0;
    /// A comment under its line.
    pub const COMMENT: f32 = 28.0;
    /// The field for a comment.
    pub const COMMENT_FIELD: f32 = 44.0;
    /// The space between two hunks of the maximized diff.
    pub const HUNK_GAP: f32 = 12.0;
    /// How far above the revealed row the view stops.
    pub const MARGIN: f32 = 24.0;
}

/// Kinds of row, as the UI numbers them.
mod kind {
    pub const TITLE: i32 = 0;
    pub const ASKED: i32 = 1;
    pub const NOTE: i32 = 2;
    pub const FAILURE: i32 = 3;
    pub const FILE: i32 = 4;
    pub const LINE: i32 = 5;
    pub const MORE: i32 = 6;
    pub const COMMENT: i32 = 8;
    pub const COMMENT_FIELD: i32 = 9;
}

/// The panel's lists, kept for the window's lifetime.
pub struct Lists {
    rows: Rc<VecModel<ChangeRow>>,
    diff: Rc<VecModel<ChangeRow>>,
}

impl std::fmt::Debug for Lists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lists").finish_non_exhaustive()
    }
}

impl Lists {
    /// Gives the window the panel's lists.
    pub fn new(state: &State<'_>) -> Self {
        let lists = Self {
            rows: Rc::new(VecModel::default()),
            diff: Rc::new(VecModel::default()),
        };
        state.set_change_rows(Rc::clone(&lists.rows).into());
        state.set_change_diff(Rc::clone(&lists.diff).into());
        lists
    }

    /// Fills the panel, when it's on screen.
    pub fn show(&self, app: &App, state: &State<'_>) {
        if !app.panel_shown(Panel::Changes) {
            return;
        }
        let changes = &app.changes;
        let wide = app.maximized == Some(Panel::Changes);
        state.set_changes_note(if changes.is_loading() {
            "Loading changes…".into()
        } else {
            SharedString::new()
        });
        let mut list = Builder::new(app, wide);
        list.sections(app.active.is_some());
        state.set_change_rows_y(list.reveal.unwrap_or(0.0));
        sync(&self.rows, list.rows);
        state.set_changes_foot(foot(app));
        state.set_commit_message(changes.message.as_str().into());

        let selected = wide.then(|| changes.selected_file()).flatten();
        state.set_change_path(selected.map_or_else(SharedString::new, |f| f.path.as_str().into()));
        let (rows, y) = match (selected, changes.selected.as_deref()) {
            (Some(file), Some(key)) => whole_diff(changes, key, file),
            _ => (Vec::new(), 0.0),
        };
        state.set_change_diff_y(y);
        sync(&self.diff, rows);
    }
}

/// Builds the side list's rows, and where the selected one sits.
struct Builder<'a> {
    changes: &'a Changes,
    /// Maximized: files select rather than open.
    wide: bool,
    rows: Vec<ChangeRow>,
    /// How far down the next row starts.
    y: f32,
    /// Where to scroll to show the selection.
    reveal: Option<f32>,
}

impl<'a> Builder<'a> {
    fn new(app: &'a App, wide: bool) -> Self {
        Self {
            changes: &app.changes,
            wide,
            rows: Vec::new(),
            y: 0.0,
            reveal: None,
        }
    }

    /// The turn under review, the session's changes when one is open, and
    /// the uncommitted ones.
    fn sections(&mut self, session: bool) {
        let changes = self.changes;
        if let Some(review) = &changes.review {
            self.title("This turn", String::new(), false, true);
            let asked = review.asked.lines().next().unwrap_or_default();
            self.push(kind::ASKED, height::NOTE, 0.0, |row| {
                row.text = asked.into();
            });
            self.files(
                List::Turn,
                changes.turn.as_ref(),
                "This turn didn't change any files.",
            );
        }
        if session {
            let count = match &changes.session {
                Some(Ok(files)) if !files.is_empty() => files_count(files.len()),
                _ => String::new(),
            };
            self.title("This session", count, false, false);
            self.files(
                List::Session,
                changes.session.as_ref(),
                "The agent hasn't changed any files.",
            );
        }
        let branch = changes
            .status
            .as_ref()
            .and_then(|s| s.branch.clone())
            .unwrap_or_default();
        let has_branch = !branch.is_empty();
        self.title("Uncommitted", branch, has_branch, false);
        self.files(List::Tree, changes.tree.as_ref(), "Nothing to commit.");
    }

    /// Adds a row of `kind`, `tall` high with `gap` above, filled by `fill`.
    fn push(&mut self, kind: i32, tall: f32, gap: f32, fill: impl FnOnce(&mut ChangeRow)) {
        let mut row = ChangeRow {
            kind,
            height: gap + tall,
            gap,
            ..ChangeRow::default()
        };
        fill(&mut row);
        self.y += row.height;
        self.rows.push(row);
    }

    /// A section's title, with what it holds or its branch beside it.
    fn title(&mut self, text: &str, aside: String, branch: bool, closes: bool) {
        let gap = if self.rows.is_empty() {
            height::TOP
        } else {
            height::SECTION_GAP
        };
        self.push(kind::TITLE, height::TITLE, gap, |row| {
            row.text = text.into();
            row.aside = aside.into();
            row.branch = branch;
            row.closes = closes;
        });
    }

    /// A list's files, or why there are none.
    fn files(
        &mut self,
        list: List,
        files: Option<&'a Result<Vec<ChangedFile>, String>>,
        empty: &str,
    ) {
        match files {
            Some(Ok(files)) if files.is_empty() => self.note(kind::NOTE, empty),
            Some(Ok(files)) => {
                for (index, file) in files.iter().enumerate() {
                    let gap = if index == 0 {
                        height::UNDER_TITLE
                    } else {
                        height::FILE_GAP
                    };
                    self.file(list, file, gap);
                }
            }
            Some(Err(reason)) => self.note(kind::FAILURE, &format!("Couldn't load them: {reason}")),
            None => {}
        }
    }

    /// A line of text in the panel: a note, or why something failed.
    ///
    /// A note is one line high, so a text with line breaks, such as a
    /// server's error message, is joined into one line.
    fn note(&mut self, kind: i32, text: &str) {
        let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        self.push(kind, height::NOTE, height::UNDER_TITLE, |row| {
            row.text = line.into();
        });
    }

    /// A file's row, and its diff under it when it's open.
    fn file(&mut self, list: List, file: &ChangedFile, gap: f32) {
        let changes = self.changes;
        let key = Changes::key(list, &file.path);
        let selected = changes.selected.as_deref() == Some(key.as_str());
        let open = !self.wide && changes.open.contains(&key) && !file.lines.is_empty();
        let (folder, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));
        let icon = file_icons::icon(name);
        // A file deleted outside OpenCode's view has nothing to count.
        let deleted = file.git.is_none() && file.lines.is_empty() && file.additions == 0;
        if selected && (self.wide || changes.hunk.is_none_or(|h| h == 0) || !open) {
            self.reveal = Some(self.y);
        }
        let wide = self.wide;
        self.push(kind::FILE, height::FILE, gap, |row| {
            row.key = key.as_str().into();
            row.text = name.into();
            row.aside = folder.into();
            // A plain file shows a plain page.
            row.icon = if file_icons::plain(name) {
                SharedString::new()
            } else {
                icon.glyph.to_string().into()
            };
            row.added = count(file.additions);
            row.removed = count(file.deletions);
            row.counts = !deleted;
            row.open = open;
            row.chevron = !wide;
            row.selected = selected && wide;
            row.clickable = !file.lines.is_empty();
        });
        if !open {
            return;
        }
        let whole = changes.whole.contains(&key);
        let width = digits(&file.lines);
        let lines = changed_lines(&file.lines);
        let total = lines
            .iter()
            .filter(|l| matches!(l, Shown::Line(..)))
            .count();
        let lit_hunk = selected
            .then_some(changes.hunk)
            .flatten()
            .filter(|&h| h > 0);
        let mut shown = 0;
        for line in lines {
            if !whole && shown == DIFF_LINES {
                break;
            }
            match line {
                Shown::Break { hunk, starts } => {
                    if starts && lit_hunk == Some(hunk) {
                        self.reveal = Some(self.y);
                    }
                    self.push(kind::LINE, height::LINE, 0.0, |row| {
                        row.key = key.as_str().into();
                        row.line = DiffLine {
                            kind: 3,
                            ..DiffLine::default()
                        };
                        row.at = -1;
                    });
                }
                Shown::Line(at, line) => {
                    shown += 1;
                    for (piece, part) in wrapped(
                        diff_line(line, width),
                        text_columns(changes.columns[0], width),
                    )
                    .into_iter()
                    .enumerate()
                    {
                        self.push(kind::LINE, height::LINE, 0.0, |row| {
                            row.key = key.as_str().into();
                            row.line = part;
                            row.at = int(at);
                            row.clickable = piece == 0 && commentable(line);
                        });
                    }
                    for mut row in under_line(changes, &key, &file.path, line) {
                        row.line.digits = width;
                        self.y += row.height;
                        self.rows.push(row);
                    }
                }
            }
        }
        if shown < total {
            let more = total - shown;
            self.push(kind::MORE, height::MORE, 0.0, |row| {
                row.key = key.as_str().into();
                row.line.digits = width;
                row.text = format!("{more} more lines").into();
            });
        }
    }
}

/// The selected file's whole diff, hunk by hunk with a space between, and
/// where its lit hunk starts. `key` is the file's.
fn whole_diff(changes: &Changes, key: &str, file: &ChangedFile) -> (Vec<ChangeRow>, f32) {
    let mut rows = Vec::new();
    let mut y = 0.0;
    let mut reveal = 0.0;
    // The hunk's place, counting only hunks with lines, and whether the
    // current one has any yet.
    let mut hunk = 0;
    let mut started = false;
    // Unchanged lines skipped inside a hunk: a space before the next change.
    let mut skipped = false;
    let width = digits(&file.lines);
    for (at, line) in file.lines.iter().enumerate() {
        if line.kind == LineKind::Gap {
            if started {
                hunk += 1;
                started = false;
            }
            skipped = false;
            continue;
        }
        if line.kind == LineKind::Context {
            skipped = started;
            continue;
        }
        let selected = changes.hunk == Some(hunk);
        let gap = match (started, rows.is_empty()) {
            (true, _) if skipped => height::HUNK_GAP,
            (true, _) => 0.0,
            (false, true) => height::TOP,
            (false, false) => height::HUNK_GAP,
        };
        skipped = false;
        if !started && selected {
            reveal = y + gap;
        }
        started = true;
        for (piece, part) in wrapped(
            diff_line(line, width),
            text_columns(changes.columns[1], width),
        )
        .into_iter()
        .enumerate()
        {
            // The space before a change goes above its first piece.
            let gap = if piece == 0 { gap } else { 0.0 };
            y += height::LINE + gap;
            rows.push(ChangeRow {
                kind: kind::LINE,
                height: height::LINE + gap,
                gap,
                key: key.into(),
                line: part,
                hunk: int(hunk),
                at: int(at),
                clickable: piece == 0 && commentable(line),
                selected,
                ..ChangeRow::default()
            });
        }
        for mut row in under_line(changes, key, &file.path, line) {
            row.line.digits = width;
            y += row.height;
            rows.push(row);
        }
    }
    (rows, (reveal - height::MARGIN).max(0.0))
}

/// The characters left for a line's text when `columns` fit its number and
/// text together and its numbers have `digits` digits; 0 while unmeasured.
fn text_columns(columns: usize, digits: i32) -> usize {
    if columns == 0 {
        return 0;
    }
    columns
        .saturating_sub(usize::try_from(digits).unwrap_or(0))
        .max(8)
}

/// `line` in pieces of at most `columns` characters, so a long line wraps
/// rather than running off the panel; the pieces after the first have no
/// number or mark. A piece breaks after a space when one is in its last
/// third. `columns` 0 leaves the line whole.
fn wrapped(line: DiffLine, columns: usize) -> Vec<DiffLine> {
    let chars: Vec<char> = line.text.chars().collect();
    if columns == 0 || chars.len() <= columns {
        return vec![line];
    }
    let mut pieces = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + columns).min(chars.len());
        if end < chars.len()
            && let Some(space) = chars[start..end].iter().rposition(|c| c.is_whitespace())
            && space + 1 > columns * 2 / 3
        {
            end = start + space + 1;
        }
        let first = start == 0;
        pieces.push(DiffLine {
            number: if first {
                line.number.clone()
            } else {
                SharedString::new()
            },
            mark: if first {
                line.mark.clone()
            } else {
                SharedString::new()
            },
            text: chars[start..end].iter().collect::<String>().into(),
            kind: line.kind,
            digits: line.digits,
        });
        start = end;
    }
    pieces
}

/// A row of a file's diff in the list: a changed line, by its place in the
/// file's lines, or a break where unchanged lines are left out.
enum Shown<'a> {
    Line(usize, &'a Line),
    /// A break; `hunk` counts the gaps before it, and `starts` says it's
    /// where hunk `hunk` starts rather than unchanged lines inside one.
    Break {
        hunk: usize,
        starts: bool,
    },
}

/// The added and removed lines of `lines`, with one break wherever
/// unchanged lines or a gap between hunks sit between two of them.
fn changed_lines(lines: &[Line]) -> Vec<Shown<'_>> {
    let mut shown = Vec::new();
    let mut hunk = 0;
    let mut pending = false;
    let mut starts = false;
    for (at, line) in lines.iter().enumerate() {
        match line.kind {
            LineKind::Gap => {
                hunk += 1;
                pending = true;
                starts = true;
            }
            LineKind::Context => pending = true,
            LineKind::Added | LineKind::Removed => {
                if pending && !shown.is_empty() {
                    shown.push(Shown::Break { hunk, starts });
                }
                pending = false;
                starts = false;
                shown.push(Shown::Line(at, line));
            }
        }
    }
    shown
}

/// Whether a comment can go on `line`: one with a number.
fn commentable(line: &Line) -> bool {
    line.kind != LineKind::Gap && line.number.is_some()
}

/// What shows under `line` of the file at `path`, listed under `key`: its
/// comments, then the field for one when it's being commented on.
fn under_line(changes: &Changes, key: &str, path: &str, line: &Line) -> Vec<ChangeRow> {
    let Some(folder) = changes.folder() else {
        return Vec::new();
    };
    let mut rows: Vec<_> = changes
        .comments_in(folder)
        .filter(|(_, c)| c.on.is(path, line))
        .map(|(index, comment)| ChangeRow {
            kind: kind::COMMENT,
            height: height::COMMENT,
            key: key.into(),
            text: comment
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .into(),
            at: int(index),
            ..ChangeRow::default()
        })
        .collect();
    if changes
        .commenting
        .as_ref()
        .is_some_and(|c| c.key == key && c.on.is(path, line))
    {
        rows.push(ChangeRow {
            kind: kind::COMMENT_FIELD,
            height: height::COMMENT_FIELD,
            key: key.into(),
            ..ChangeRow::default()
        });
    }
    rows
}

/// The foot: the comments waiting to be sent, Commit when there's something
/// to commit here, Push when the branch is ahead, and how the last git
/// action went.
fn foot(app: &App) -> ChangesFoot {
    let changes = &app.changes;
    let files = match &changes.tree {
        Some(Ok(files)) => files.len(),
        _ => 0,
    };
    let mut foot = ChangesFoot::default();
    let comments = changes
        .folder()
        .map_or(0, |folder| changes.comments_in(folder).count());
    if comments > 0 {
        foot.comments = if comments == 1 {
            "1 comment".to_owned()
        } else {
            format!("{comments} comments")
        }
        .into();
        foot.can_send = !app.is_sending();
        if !app.composer.text().trim().is_empty() {
            foot.send_note = "Sent as one message after what's in the composer.".into();
        }
    }
    match &changes.status {
        Some(status) => {
            foot.commit = files > 0;
            foot.ready = !changes.message.trim().is_empty() && changes.running.is_none();
            foot.committing = changes.running == Some(GitAction::Commit);
            foot.writing = changes.writing;
            foot.can_write = files > 0 && !changes.writing && changes.running.is_none();
            if let Some(upstream) = status.upstream.as_deref().filter(|_| status.ahead > 0) {
                foot.push = if changes.running == Some(GitAction::Push) {
                    "Pushing…".to_owned()
                } else if status.ahead == 1 {
                    "Push 1 commit".to_owned()
                } else {
                    format!("Push {} commits", status.ahead)
                }
                .into();
                foot.can_push = changes.running.is_none();
                foot.upstream = format!("to {upstream}").into();
                if status.behind > 0 {
                    foot.behind = format!("({} behind)", status.behind).into();
                }
            }
        }
        None if files > 0 => {
            foot.note = "Committing needs the folder on this computer, in a Git repository.".into();
        }
        None => {}
    }
    match &changes.outcome {
        Some(Ok(said)) => foot.outcome = said.as_str().into(),
        Some(Err(reason)) => {
            foot.outcome = reason.as_str().into();
            foot.failed = true;
        }
        None => {}
    }
    foot
}

/// "1 file" or "3 files".
fn files_count(n: usize) -> String {
    if n == 1 {
        "1 file".to_owned()
    } else {
        format!("{n} files")
    }
}

/// A count as the UI's `int`.
fn count(n: u64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// A place in a list as the UI's `int`.
fn int(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::wrapped;
    use crate::ui::DiffLine;

    fn line(text: &str) -> DiffLine {
        DiffLine {
            number: "7".into(),
            mark: "+".into(),
            text: text.into(),
            kind: 1,
            digits: 1,
        }
    }

    #[test]
    fn a_short_line_or_unknown_width_stays_whole() {
        assert_eq!(wrapped(line("short"), 20).len(), 1);
        assert_eq!(wrapped(line(&"x".repeat(500)), 0).len(), 1);
    }

    #[test]
    fn a_long_line_wraps_after_a_space_and_only_its_first_piece_is_numbered() {
        let pieces = wrapped(line("alpha beta gamma delta epsilon"), 12);
        let texts: Vec<&str> = pieces.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, ["alpha beta ", "gamma delta ", "epsilon"]);
        assert_eq!(pieces[0].number, "7");
        assert_eq!(pieces[0].mark, "+");
        assert!(
            pieces[1..]
                .iter()
                .all(|p| p.number.is_empty() && p.mark.is_empty())
        );
        assert!(
            pieces.iter().all(|p| p.kind == 1),
            "each piece keeps its colour"
        );
    }

    #[test]
    fn a_files_digits_leave_the_rest_of_the_width_to_its_text() {
        use super::text_columns;
        assert_eq!(text_columns(0, 3), 0, "unmeasured stays unwrapped");
        assert_eq!(text_columns(50, 2), 48);
        assert_eq!(text_columns(50, 5), 45);
        assert_eq!(text_columns(10, 5), 8, "never narrower than 8");
    }

    #[test]
    fn the_number_column_fits_the_largest_number() {
        use crate::present::timeline::digits;
        use harlo_core::diff::{Line, LineKind};
        let line = |number| Line {
            kind: LineKind::Added,
            number,
            spans: Vec::new(),
        };
        assert_eq!(digits(&[line(Some(9)), line(Some(58))]), 2);
        assert_eq!(digits(&[line(Some(10_231)), line(None)]), 5);
        assert_eq!(digits(&[line(None)]), 1);
    }

    #[test]
    fn a_line_without_spaces_breaks_at_the_width() {
        let pieces = wrapped(line(&"x".repeat(25)), 10);
        let lengths: Vec<usize> = pieces.iter().map(|p| p.text.chars().count()).collect();
        assert_eq!(lengths, [10, 10, 5]);
    }
}
