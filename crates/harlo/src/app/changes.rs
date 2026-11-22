//! The Changes panel: what the open session changed, and the folder's
//! uncommitted changes to stage, commit and push.
//!
//! The session's changes come from OpenCode, which compares the agent's own
//! snapshots. The uncommitted ones come from `git status` when the folder is
//! on this computer, with each file's diff from OpenCode; elsewhere, only
//! OpenCode's list shows, and nothing can be staged.
//!
//! The panel loads when it opens, when it switches to another session or
//! folder, and when something in the folder may have changed: a tool that
//! edited files, a finished run, an undo, the window regaining focus, or a
//! git action from here. A load that's asked for while one is on its way
//! runs once that one lands.

// Rust guideline compliant 2026-10-09

use std::collections::HashSet;
use std::path::Path;

use harlo_core::diff;
use harlo_core::git::{self, Repo};
use harlo_core::server::FileDiff;
use harlo_core::workspace::EventKind;
use iced::Task;

use super::{App, Message, Panel};

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
    fn from_diff(file: &FileDiff) -> Self {
        Self {
            path: file.file.clone(),
            additions: file.additions,
            deletions: file.deletions,
            lines: diff::lines(&file.patch),
            git: None,
        }
    }
}

/// What a load found.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The session's changes, when a session is open.
    pub session: Option<Result<Vec<FileDiff>, String>>,
    /// OpenCode's uncommitted changes.
    pub tree: Result<Vec<FileDiff>, String>,
    /// `git status`, when the folder is on this computer.
    pub status: Option<Result<git::Status, String>>,
}

/// A git action from the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitAction {
    /// Staging files.
    Stage,
    /// Unstaging files.
    Unstage,
    /// Committing what's staged.
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
    /// Show all of a file's diff, by key.
    Whole(String),
    /// Stage these files, by path from the repository's root.
    Stage(Vec<String>),
    /// Unstage these files, by path from the repository's root.
    Unstage(Vec<String>),
    /// The commit message changed.
    Message(String),
    /// Commit what's staged.
    Commit,
    /// Push the branch.
    Push,
    /// A git action finished, with what to say about it or why it failed.
    Ran(GitAction, Result<String, String>),
}

/// Which list a file is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// The session's changes.
    Session,
    /// The folder's uncommitted changes.
    Tree,
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
    /// The session's changes, once loaded.
    pub session: Option<Result<Vec<ChangedFile>, String>>,
    /// The uncommitted changes, once loaded.
    pub tree: Option<Result<Vec<ChangedFile>, String>>,
    /// `git status`, when the folder is on this computer and in a repository.
    pub status: Option<git::Status>,
    /// Files whose diff is open, by key.
    pub open: HashSet<String>,
    /// Files whose whole diff shows, by key.
    pub whole: HashSet<String>,
    /// The commit message being written.
    pub message: String,
    /// The git action on its way.
    pub running: Option<GitAction>,
    /// What the last git action did, or why it failed.
    pub outcome: Option<Result<String, String>>,
}

impl Changes {
    /// The key a file's open and whole state is kept under.
    #[must_use]
    pub fn key(list: List, path: &str) -> String {
        match list {
            List::Session => format!("session:{path}"),
            List::Tree => format!("tree:{path}"),
        }
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
            Event::Loaded(load, loaded) => {
                if changes.loading == Some(load) {
                    changes.loading = None;
                    changes.set(*loaded);
                }
            }
            Event::Toggle(key) => {
                if !changes.open.remove(&key) {
                    changes.open.insert(key);
                }
            }
            Event::Whole(key) => {
                changes.whole.insert(key);
            }
            Event::Message(text) => changes.message = text,
            Event::Stage(paths) => return self.run_git(GitAction::Stage, paths),
            Event::Unstage(paths) => return self.run_git(GitAction::Unstage, paths),
            Event::Commit => return self.run_git(GitAction::Commit, Vec::new()),
            Event::Push => return self.run_git(GitAction::Push, Vec::new()),
            Event::Ran(action, result) => {
                changes.running = None;
                changes.stale = true;
                if action == GitAction::Commit && result.is_ok() {
                    changes.message.clear();
                }
                // Staging speaks for itself in the list; only failures need words.
                if matches!(action, GitAction::Commit | GitAction::Push) || result.is_err() {
                    changes.outcome = Some(result);
                }
            }
        }
        Task::none()
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
        let visible = self.panel == Some(Panel::Changes) && self.shown().panel;
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
        Task::perform(
            async move {
                let session = match session {
                    Some(id) => Some(client.diff(&id).await.map_err(|e| e.to_string())),
                    None => None,
                };
                let tree = client
                    .working_changes(&folder)
                    .await
                    .map_err(|e| e.to_string());
                let status = if local {
                    Some(Repo::new(&folder).status().await.map_err(|e| e.message))
                } else {
                    None
                };
                Loaded {
                    session,
                    tree,
                    status,
                }
            },
            move |loaded| Message::Changes(Event::Loaded(load, Box::new(loaded))),
        )
    }

    /// Runs a git action in the shown folder.
    fn run_git(&mut self, action: GitAction, paths: Vec<String>) -> Task<Message> {
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
        Task::perform(
            async move {
                let repo = Repo::new(&folder);
                let result = match action {
                    GitAction::Stage => repo.stage(&paths).await.map(|()| String::new()),
                    GitAction::Unstage => repo.unstage(&paths).await.map(|()| String::new()),
                    GitAction::Commit => repo
                        .commit(&message)
                        .await
                        .map(|summary| format!("Committed {summary}")),
                    GitAction::Push => repo.push().await.map(|()| match upstream {
                        Some(upstream) => format!("Pushed to {upstream}"),
                        None => "Pushed".to_owned(),
                    }),
                };
                result.map_err(|e| e.message)
            },
            move |result| Message::Changes(Event::Ran(action, result)),
        )
    }
}

impl Changes {
    /// Takes in what a load found.
    fn set(&mut self, loaded: Loaded) {
        self.session = loaded
            .session
            .map(|files| files.map(|files| files.iter().map(ChangedFile::from_diff).collect()));
        let status = match loaded.status {
            Some(Ok(status)) => Some(status),
            // Not a repository, or `git` is missing: the list is read-only.
            Some(Err(_)) | None => None,
        };
        self.tree = Some(loaded.tree.map(|diffs| tree_files(&diffs, status.as_ref())));
        self.status = status;
    }
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
