//! The Changes panel: what the open session changed, and the folder's
//! uncommitted changes to commit and push.
//!
//! The session's changes come from OpenCode, which compares the agent's own
//! snapshots. The uncommitted ones come from `git status` when the folder is
//! on this computer, with each file's diff from OpenCode; elsewhere, only
//! OpenCode's list shows, and nothing can be committed. A commit takes every
//! uncommitted file.
//!
//! Diffs are parsed into lines in the background, with the load, so a large
//! set of changes doesn't hold up the window.
//!
//! A file in an agent's edit opens the panel on that turn's changes, with
//! the file selected and the edited hunk lit: the turn's list comes first,
//! from OpenCode's diff of that turn alone.
//!
//! The panel loads when it opens, when it switches to another session or
//! folder, and when something in the folder may have changed: a tool that
//! edited files, a finished run, an undo, the window regaining focus, or a
//! git action from here. A load that's asked for while one is on its way
//! runs once that one lands.

// Rust guideline compliant 2026-10-09

use std::collections::HashSet;
use std::path::Path;

use crate::task::Task;
use harlo_core::diff;
use harlo_core::git::{self, Repo};
use harlo_core::server::FileDiff;
use harlo_core::workspace::EventKind;

use super::{App, Message, Panel};

/// How many more times to ask OpenCode for the uncommitted diffs when it
/// answers with none while `git status` lists changes.
const DIFF_RETRIES: u32 = 4;

/// The wait before asking again.
const DIFF_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

/// One changed file, with its diff's lines ready to draw.
#[derive(Debug, Clone)]
pub struct ChangedFile {
    /// Path, as the list shows it.
    pub path: String,
    /// Lines added.
    pub additions: u64,
    /// Lines removed.
    pub deletions: u64,
    /// The diff's lines; empty when there's no diff to show.
    pub lines: Vec<diff::Line>,
    /// How it stands in Git, when the folder is on this computer.
    pub git: Option<git::FileState>,
}

impl ChangedFile {
    /// A file from OpenCode's diff of it.
    #[must_use]
    pub fn from_diff(file: &FileDiff) -> Self {
        Self {
            path: file.file.clone(),
            additions: file.additions,
            deletions: file.deletions,
            lines: diff::lines(&file.patch),
            git: None,
        }
    }
}

/// What a load found, with the diffs parsed and ready to draw.
#[derive(Debug, Clone)]
pub struct Loaded {
    turn: Option<Result<Vec<ChangedFile>, String>>,
    session: Option<Result<Vec<ChangedFile>, String>>,
    tree: Result<Vec<ChangedFile>, String>,
    status: Option<git::Status>,
}

impl Loaded {
    /// Parses what a load found: the reviewed turn's changes, the session's,
    /// OpenCode's uncommitted changes, and `git status` when the folder is
    /// on this computer. Without a status, the uncommitted list is OpenCode's
    /// alone and can't be committed.
    #[must_use]
    pub fn new(
        turn: Option<Result<Vec<FileDiff>, String>>,
        session: Option<Result<Vec<FileDiff>, String>>,
        tree: Result<Vec<FileDiff>, String>,
        status: Option<Result<git::Status, String>>,
    ) -> Self {
        let parse = |files: Result<Vec<FileDiff>, String>| {
            files.map(|files| files.iter().map(ChangedFile::from_diff).collect())
        };
        // Not a repository, or `git` is missing: the list is read-only.
        let status = status.and_then(Result::ok);
        Self {
            turn: turn.map(parse),
            session: session.map(parse),
            tree: tree.map(|diffs| tree_files(&diffs, status.as_ref())),
            status,
        }
    }
}

/// A git action from the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitAction {
    /// Committing every uncommitted file.
    Commit,
    /// Pushing to the upstream.
    Push,
}

/// Something the panel reported, or a load or git action that finished.
#[derive(Debug, Clone)]
pub enum Event {
    /// Load again.
    Refresh,
    /// A load finished: which one, and what it found.
    Loaded(u64, Box<Loaded>),
    /// Show or hide a file's diff, by key (see [`Changes::key`]).
    Toggle(String),
    /// Show all of a file's diff, by key, in the maximized panel.
    Whole(String),
    /// Select a file, by key: the maximized panel shows its diff.
    Select(String),
    /// Light a hunk of the selected file, by its place in the diff.
    Hunk(usize),
    /// How many characters of code fit on a diff's line: in the panel's
    /// diffs, or the maximized panel's diff when `whole`.
    Columns {
        /// Whether it's the maximized panel's diff.
        whole: bool,
        /// The characters that fit.
        columns: usize,
    },
    /// Stop reviewing a turn.
    EndReview,
    /// The commit message changed.
    Message(String),
    /// Commit every uncommitted file.
    Commit,
    /// Push the branch.
    Push,
    /// Have the commit-message model write the message.
    WriteMessage,
    /// A message was written for the folder, or why it wasn't.
    MessageWritten(String, Result<String, String>),
    /// A git action finished, with what to say about it or why it failed.
    Ran(GitAction, Result<String, String>),
    /// Open or close the comment field under a line: the file's key and
    /// the line's place in its diff.
    CommentOn(String, usize),
    /// Add the comment being written.
    AddComment(String),
    /// Close the comment field.
    CancelComment,
    /// Remove a comment, by its place in the list.
    RemoveComment(usize),
    /// Send the folder's comments to the agent as one message.
    SendComments,
    /// Drop the folder's comments.
    DiscardComments,
}

/// Which list a file is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// The changes of the turn being reviewed.
    Turn,
    /// The session's changes.
    Session,
    /// The folder's uncommitted changes.
    Tree,
}

/// A turn under review: the user's message that started it, and where to look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// The session.
    pub session: String,
    /// The user's message that started the turn.
    pub from: String,
    /// What the user asked, to name the turn.
    pub asked: String,
    /// The file the review opened on.
    pub path: String,
    /// A line the edit changed, to find its hunk in the turn's diff.
    pub line: Option<String>,
}

/// The Changes panel's state.
#[derive(Debug, Default)]
pub struct Changes {
    /// The session and folder shown.
    shown: Option<(Option<String>, String)>,
    /// Whether the panel was on screen at the last update.
    visible: bool,
    /// Something changed since the last load.
    stale: bool,
    /// The load on its way, by number.
    loading: Option<u64>,
    loads: u64,
    /// The turn being reviewed, if one is.
    pub review: Option<Review>,
    /// The reviewed turn's changes, once loaded.
    pub turn: Option<Result<Vec<ChangedFile>, String>>,
    /// The session's changes, once loaded.
    pub session: Option<Result<Vec<ChangedFile>, String>>,
    /// The uncommitted changes, once loaded.
    pub tree: Option<Result<Vec<ChangedFile>, String>>,
    /// `git status`, when the folder is on this computer and in a repository.
    pub status: Option<git::Status>,
    /// The selected file, by key: the maximized panel shows its diff.
    pub selected: Option<String>,
    /// The lit hunk of the selected file, by its place in the diff.
    pub hunk: Option<usize>,
    /// Files whose diff is open, by key.
    pub open: HashSet<String>,
    /// Files whose whole diff shows, by key.
    pub whole: HashSet<String>,
    /// The commit message being written.
    pub message: String,
    /// The git action on its way.
    pub running: Option<GitAction>,
    /// Whether a commit message is being written.
    pub writing: bool,
    /// What the last git action did, or why it failed.
    pub outcome: Option<Result<String, String>>,
    /// Comments on diff lines, waiting to be sent.
    pub comments: Vec<super::LineComment>,
    /// The line a comment is being written on.
    pub commenting: Option<super::Commenting>,
    /// How many characters fit on a line of the panel's diffs, then of the
    /// maximized panel's diff; longer lines wrap. 0 until measured.
    pub columns: [usize; 2],
}

impl Changes {
    /// The key a file's open and whole state is kept under.
    #[must_use]
    pub fn key(list: List, path: &str) -> String {
        match list {
            List::Turn => format!("turn:{path}"),
            List::Session => format!("session:{path}"),
            List::Tree => format!("tree:{path}"),
        }
    }

    /// Every loaded file with its key, in the order the panel lists them.
    pub fn files(&self) -> impl Iterator<Item = (String, &ChangedFile)> {
        [
            (List::Turn, &self.turn),
            (List::Session, &self.session),
            (List::Tree, &self.tree),
        ]
        .into_iter()
        .flat_map(|(list, files)| {
            files
                .iter()
                .flatten()
                .flatten()
                .map(move |f| (Self::key(list, &f.path), f))
        })
    }

    /// The selected file, if it's loaded.
    #[must_use]
    pub fn selected_file(&self) -> Option<&ChangedFile> {
        let selected = self.selected.as_deref()?;
        self.files()
            .find(|(key, _)| key == selected)
            .map(|(_, f)| f)
    }

    /// Selects the first file when none that's loaded is.
    fn select_some(&mut self) {
        if self.selected_file().is_none() {
            let first = self.files().next().map(|(key, _)| key);
            self.selected = first;
            self.hunk = None;
        }
    }

    /// The folder shown.
    #[must_use]
    pub fn folder(&self) -> Option<&str> {
        self.shown.as_ref().map(|(_, folder)| folder.as_str())
    }

    /// Returns whether the first load hasn't landed yet.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.loading.is_some() && self.tree.is_none()
    }
}

impl App {
    /// Applies something the Changes panel reported.
    pub(super) fn changes_action(&mut self, event: Event) -> Task<Message> {
        let changes = &mut self.changes;
        match event {
            Event::Refresh => changes.stale = true,
            Event::Columns { whole, columns } => changes.columns[usize::from(whole)] = columns,
            Event::Loaded(load, loaded) => {
                if changes.loading == Some(load) {
                    changes.loading = None;
                    let reviewing = changes.turn.is_none() && changes.review.is_some();
                    changes.set(*loaded);
                    if reviewing {
                        changes.land_review();
                        return self.scroll_to_hunk();
                    }
                }
            }
            Event::Toggle(key) => {
                if !changes.open.remove(&key) {
                    changes.open.insert(key.clone());
                }
                if changes.selected.as_ref() != Some(&key) {
                    changes.selected = Some(key);
                    changes.hunk = None;
                }
            }
            Event::Select(key) => {
                if changes.selected.as_ref() != Some(&key) {
                    changes.selected = Some(key);
                    changes.hunk = None;
                    return self.scroll_to_hunk();
                }
            }
            Event::Hunk(hunk) => changes.hunk = Some(hunk),
            Event::EndReview => {
                changes.review = None;
                changes.turn = None;
                changes.select_some();
            }
            // Too long for the panel: the maximized panel shows all of it.
            Event::Whole(key) => {
                if changes.selected.as_ref() != Some(&key) {
                    changes.selected = Some(key);
                    changes.hunk = None;
                }
                if self.panel == Some(Panel::Changes) && self.maximized.is_none() {
                    return self.arrange(Message::Maximize(Panel::Changes));
                }
                return self.scroll_to_hunk();
            }
            Event::Message(text) => changes.message = text,
            Event::Commit => return self.run_git(GitAction::Commit),
            Event::Push => return self.run_git(GitAction::Push),
            Event::WriteMessage => return self.write_commit_message(),
            Event::MessageWritten(folder, written) => {
                self.commit_message_written(&folder, written);
            }
            Event::Ran(action, result) => {
                changes.running = None;
                changes.stale = true;
                if action == GitAction::Commit && result.is_ok() {
                    changes.message.clear();
                }
                changes.outcome = Some(result);
            }
            Event::CommentOn(key, at) => changes.comment_on(&key, at),
            Event::AddComment(text) => changes.add_comment(&text),
            Event::CancelComment => changes.commenting = None,
            Event::RemoveComment(index) => changes.remove_comment(index),
            Event::SendComments => return self.send_comments(),
            Event::DiscardComments => changes.discard_comments(),
        }
        Task::none()
    }

    /// Opens the panel on an agent's edit: the turn it was made in, the file
    /// it changed, and its hunk. `tool` is the edit's row key in `session`'s
    /// timeline, and `file` which of its files.
    pub(super) fn review_edit(&mut self, session: &str, tool: &str, file: usize) -> Task<Message> {
        let Some(timeline) = self.timelines.get(session) else {
            return Task::none();
        };
        let mut asked = None;
        let mut edit = None;
        for entry in timeline.entries() {
            match entry {
                harlo_core::timeline::Entry::Prompt { key, text, .. } => {
                    asked = Some((key.clone(), text.clone()));
                }
                harlo_core::timeline::Entry::Tool { key, files, .. } if key == tool => {
                    edit = files.get(file).cloned();
                    break;
                }
                _ => {}
            }
        }
        let (Some((from, asked)), Some(edit)) = (asked, edit) else {
            return Task::none();
        };
        let line = diff::lines(&edit.patch)
            .into_iter()
            .find(|l| matches!(l.kind, diff::LineKind::Added | diff::LineKind::Removed))
            .map(|l| l.text());
        self.changes.review = Some(Review {
            session: session.to_owned(),
            from,
            asked,
            path: edit.file,
            line,
        });
        self.changes.turn = None;
        self.changes.stale = true;
        let shown = self.panel_shown(Panel::Changes);
        let task = self.update(Message::OpenPanel(Panel::Changes));
        if shown {
            // Already on screen: it loads on the next update.
            return Task::batch([task, self.follow_changes()]);
        }
        task
    }

    /// Selects a file when the panel is maximized with none selected, and
    /// brings its lit hunk into view.
    pub(super) fn on_maximize(&mut self, tool: Panel) -> Task<Message> {
        if tool != Panel::Changes {
            return Task::none();
        }
        // It may have been hidden behind another tab, and out of date.
        let load = self.follow_changes();
        self.changes.select_some();
        Task::batch([load, self.scroll_to_hunk()])
    }

    /// Scrolls the selected file's lit hunk into view: in the maximized
    /// panel's diff, or in the panel's list.
    pub(super) fn scroll_to_hunk(&self) -> Task<Message> {
        let list = if self.maximized == Some(Panel::Changes) {
            crate::ids::CHANGES_DIFF
        } else {
            crate::ids::CHANGES_LIST
        };
        Task::effect(crate::task::Effect::Reveal(list))
    }

    /// Marks the panel stale when `event` may have changed what it shows.
    pub(super) fn changes_event(&mut self, event: &EventKind) {
        let touches = match event {
            EventKind::ToolEnded { files, .. } => !files.is_empty(),
            EventKind::Done { .. }
            | EventKind::RevertStaged { .. }
            | EventKind::RevertCleared { .. }
            | EventKind::RevertCommitted { .. } => true,
            _ => false,
        };
        let Some((_, folder)) = &self.changes.shown else {
            return;
        };
        let here = event
            .session_id()
            .and_then(|id| self.workspace.session(id))
            .is_some_and(|s| s.directory == *folder);
        if touches && here {
            self.changes.stale = true;
        }
    }

    /// Marks the panel stale, as when the window regains focus.
    pub(super) fn changes_stale(&mut self) {
        self.changes.stale = true;
    }

    /// Loads the panel when it's on screen and out of date.
    pub(super) fn follow_changes(&mut self) -> Task<Message> {
        let visible = self.panel_shown(Panel::Changes);
        let opened = visible && !self.changes.visible;
        self.changes.visible = visible;
        if !visible {
            return Task::none();
        }
        let shown = (self.active.clone(), self.folder());
        if self.changes.shown.as_ref() != Some(&shown) {
            self.changes = super::changes::Changes {
                shown: Some(shown.clone()),
                visible,
                // A commit message in progress belongs to the folder.
                message: if self.changes.shown.as_ref().map(|s| &s.1) == Some(&shown.1) {
                    std::mem::take(&mut self.changes.message)
                } else {
                    String::new()
                },
                // A review asked for before the panel caught up stays.
                review: self
                    .changes
                    .review
                    .take()
                    .filter(|r| shown.0.as_ref() == Some(&r.session)),
                stale: true,
                ..Default::default()
            };
        }
        if opened {
            self.changes.stale = true;
        }
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if !self.changes.stale || self.changes.loading.is_some() {
            return Task::none();
        }
        self.changes.stale = false;
        self.changes.loads += 1;
        let load = self.changes.loads;
        self.changes.loading = Some(load);
        let (session, folder) = shown;
        let local = self.location.is_local() && Path::new(&folder).is_dir();
        let review = self
            .changes
            .review
            .clone()
            .filter(|r| session.as_ref() == Some(&r.session));
        Task::perform(
            async move {
                let turn = match review {
                    Some(review) => Some(
                        client
                            .diff_since(&review.session, &review.from, None)
                            .await
                            .map_err(|e| e.to_string()),
                    ),
                    None => None,
                };
                let session = match session {
                    Some(id) => Some(client.diff(&id).await.map_err(|e| e.to_string())),
                    None => None,
                };
                let status = if local {
                    Some(Repo::new(&folder).status().await.map_err(|e| e.message))
                } else {
                    None
                };
                let changed = matches!(&status, Some(Ok(s)) if !s.files.is_empty());
                let mut tree = client
                    .working_changes(&folder)
                    .await
                    .map_err(|e| e.to_string());
                // OpenCode answers a folder it's still starting up for with
                // no diffs at all; git knows better, so ask again.
                for _ in 0..DIFF_RETRIES {
                    if !changed || !matches!(&tree, Ok(diffs) if diffs.is_empty()) {
                        break;
                    }
                    tokio::time::sleep(DIFF_RETRY_DELAY).await;
                    tree = client
                        .working_changes(&folder)
                        .await
                        .map_err(|e| e.to_string());
                }
                // Parsing a large set of diffs takes a while; not on the
                // window's thread.
                tokio::task::spawn_blocking(move || Loaded::new(turn, session, tree, status))
                    .await
                    .unwrap_or_else(|e| Loaded::new(None, None, Err(e.to_string()), None))
            },
            move |loaded| Message::Changes(Event::Loaded(load, Box::new(loaded))),
        )
    }

    /// Runs a git action in the shown folder.
    fn run_git(&mut self, action: GitAction) -> Task<Message> {
        let Some((_, folder)) = self.changes.shown.clone() else {
            return Task::none();
        };
        let message = self.changes.message.trim().to_owned();
        if self.changes.running.is_some() || (action == GitAction::Commit && message.is_empty()) {
            return Task::none();
        }
        self.changes.running = Some(action);
        self.changes.outcome = None;
        let upstream = self
            .changes
            .status
            .as_ref()
            .and_then(|s| s.upstream.clone());
        let paths: Vec<String> = self
            .changes
            .status
            .iter()
            .flat_map(|s| s.files.iter().map(|f| f.path.clone()))
            .collect();
        Task::perform(
            async move {
                let repo = Repo::new(&folder);
                let result = match action {
                    GitAction::Commit => repo
                        .commit_all(&paths, &message)
                        .await
                        .map(|summary| format!("Committed {summary}")),
                    GitAction::Push => repo.push().await.map(|()| match upstream {
                        Some(upstream) => format!("Pushed to {upstream}"),
                        None => "Pushed".to_owned(),
                    }),
                };
                result.map_err(|e| {
                    if e.needs_identity() {
                        "Git needs your name and email for commits. Set them in \
                         Settings → Git, then commit again."
                            .to_owned()
                    } else {
                        e.message
                    }
                })
            },
            move |result| Message::Changes(Event::Ran(action, result)),
        )
    }
}

impl Changes {
    /// Takes in what a load found.
    fn set(&mut self, loaded: Loaded) {
        self.turn = loaded.turn;
        self.session = loaded.session;
        self.tree = Some(loaded.tree);
        self.status = loaded.status;
        self.select_some();
    }

    /// Selects the reviewed file in the turn's list, opens its diff and
    /// lights the hunk holding the edited line.
    fn land_review(&mut self) {
        let Some(review) = &self.review else {
            return;
        };
        let Some(file) = self
            .turn
            .iter()
            .flatten()
            .flatten()
            .find(|f| f.path == review.path || f.path.ends_with(&format!("/{}", review.path)))
        else {
            return;
        };
        let key = Self::key(List::Turn, &file.path);
        let hunk = review
            .line
            .as_deref()
            .and_then(|line| hunk_of(&file.lines, line))
            .unwrap_or(0);
        self.open.insert(key.clone());
        self.whole.insert(key.clone());
        self.selected = Some(key);
        self.hunk = Some(hunk);
    }
}

/// The hunk (counting the gaps between them) holding a changed line with `text`.
#[must_use]
pub fn hunk_of(lines: &[diff::Line], text: &str) -> Option<usize> {
    let mut hunk = 0;
    for line in lines {
        match line.kind {
            diff::LineKind::Gap => hunk += 1,
            diff::LineKind::Added | diff::LineKind::Removed if line.text() == text => {
                return Some(hunk);
            }
            _ => {}
        }
    }
    None
}

/// The uncommitted files: `git status`'s, with OpenCode's diffs, or
/// OpenCode's alone when there's no status.
fn tree_files(diffs: &[FileDiff], status: Option<&git::Status>) -> Vec<ChangedFile> {
    let Some(status) = status else {
        return diffs.iter().map(ChangedFile::from_diff).collect();
    };
    status
        .files
        .iter()
        .map(|state| {
            // OpenCode's paths may be from the folder rather than the root.
            let local = state.path.strip_prefix(status.prefix.as_str());
            let diff = diffs
                .iter()
                .find(|d| d.file == state.path || Some(d.file.as_str()) == local);
            let mut file = diff.map_or_else(
                || ChangedFile {
                    path: state.path.clone(),
                    additions: 0,
                    deletions: 0,
                    lines: Vec::new(),
                    git: None,
                },
                ChangedFile::from_diff,
            );
            file.path.clone_from(&state.path);
            file.git = Some(state.clone());
            file
        })
        .collect()
}
