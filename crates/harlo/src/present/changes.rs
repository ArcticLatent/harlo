//! The Changes panel: its lists flattened into rows, the selected file's
//! diff when maximized, and its foot with Commit and Push.
//!
//! Every row's height is set here, so where a row sits is known without
//! the window laying it out: that's how the panel scrolls to the lit hunk.

// Rust guideline compliant 2026-10-09

use std::rc::Rc;

use harlo_core::diff::{Line, LineKind};
use slint::{SharedString, VecModel};

use super::{sync, timeline::diff_line};
use crate::app::{App, ChangedFile, Changes, GitAction, List, Panel};
use crate::file_icons;
use crate::ui::{ChangeRow, ChangesFoot, State};

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
        state.set_changes_foot(foot(changes));
        state.set_commit_message(changes.message.as_str().into());

        let selected = wide.then(|| changes.selected_file()).flatten();
        state.set_change_path(selected.map_or_else(SharedString::new, |f| f.path.as_str().into()));
        let (rows, y) = selected.map_or_else(|| (Vec::new(), 0.0), |f| whole_diff(f, changes.hunk));
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
            row.icon = icon.glyph.to_string().into();
            row.icon_color = icon.slint_color();
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
        let shown = if whole {
            file.lines.len()
        } else {
            file.lines.len().min(DIFF_LINES)
        };
        let lit_hunk = selected
            .then_some(changes.hunk)
            .flatten()
            .filter(|&h| h > 0);
        let mut hunk = 0;
        for line in &file.lines[..shown] {
            if line.kind == LineKind::Gap {
                hunk += 1;
                if lit_hunk == Some(hunk) {
                    self.reveal = Some(self.y);
                }
            }
            self.push(kind::LINE, height::LINE, 0.0, |row| {
                row.key = key.as_str().into();
                row.line = diff_line(line);
            });
        }
        if shown < file.lines.len() {
            let more = file.lines.len() - shown;
            self.push(kind::MORE, height::MORE, 0.0, |row| {
                row.key = key.as_str().into();
                row.text = format!("{more} more lines").into();
            });
        }
    }
}

/// The selected file's whole diff, hunk by hunk with a space between, and
/// where its lit hunk starts.
fn whole_diff(file: &ChangedFile, lit: Option<usize>) -> (Vec<ChangeRow>, f32) {
    let mut rows = Vec::new();
    let mut y = 0.0;
    let mut reveal = 0.0;
    for (index, hunk) in hunks(&file.lines).into_iter().enumerate() {
        let selected = lit == Some(index);
        for (at, line) in hunk.iter().enumerate() {
            let gap = match (at, index) {
                (0, 0) => height::TOP,
                (0, _) => height::HUNK_GAP,
                _ => 0.0,
            };
            if at == 0 && selected {
                reveal = y + gap;
            }
            y += height::LINE + gap;
            rows.push(ChangeRow {
                kind: kind::LINE,
                height: height::LINE + gap,
                gap,
                line: diff_line(line),
                hunk: i32::try_from(index).unwrap_or(i32::MAX),
                selected,
                ..ChangeRow::default()
            });
        }
    }
    (rows, (reveal - height::MARGIN).max(0.0))
}

/// A diff's hunks: its lines between the gaps.
fn hunks(lines: &[Line]) -> Vec<&[Line]> {
    lines
        .split(|line| line.kind == LineKind::Gap)
        .filter(|hunk| !hunk.is_empty())
        .collect()
}

/// The foot: Commit when there's something to commit here, Push when the
/// branch is ahead, and how the last git action went.
fn foot(changes: &Changes) -> ChangesFoot {
    let files = match &changes.tree {
        Some(Ok(files)) => files.len(),
        _ => 0,
    };
    let mut foot = ChangesFoot::default();
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
