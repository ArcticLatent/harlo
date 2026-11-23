//! Comments on diff lines in the Changes panel: a line's number opens a
//! field under it, each comment shows under its line, and all of a folder's
//! comments go to the agent as one message.
//!
//! A comment keeps its line's text and number rather than its place in the
//! diff, so it stays under its line when the diff loads again, and goes
//! away from view (but not from the message) when the line does.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use harlo_core::diff::{Line, LineKind};

use super::changes::Changes;
use super::{App, Message};
use crate::task::Task;

/// A comment on a line of a changed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineComment {
    /// The folder whose changes it's on.
    pub folder: String,
    /// The line it's on.
    pub on: LineRef,
    /// What it says.
    pub text: String,
}

/// A line of a changed file, as a comment names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRef {
    /// The file's path.
    pub path: String,
    /// Added, removed or unchanged.
    pub kind: LineKind,
    /// Its number: in the new file, or the old one for a removed line.
    pub number: Option<u32>,
    /// Its text.
    pub text: String,
}

impl LineRef {
    /// `line` of the file at `path`.
    #[must_use]
    pub fn new(path: &str, line: &Line) -> Self {
        Self {
            path: path.to_owned(),
            kind: line.kind,
            number: line.number,
            text: line.text(),
        }
    }

    /// Whether this is `line` of the file at `path`.
    #[must_use]
    pub fn is(&self, path: &str, line: &Line) -> bool {
        self.path == path
            && self.kind == line.kind
            && self.number == line.number
            && self.text == line.text()
    }
}

/// The line a comment is being written on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commenting {
    /// The file's key in the panel (see [`Changes::key`]), so the field
    /// shows in one list only.
    pub key: String,
    /// The line.
    pub on: LineRef,
}

impl Changes {
    /// The comments on `folder`'s changes, with their places in the list.
    pub fn comments_in<'a>(
        &'a self,
        folder: &'a str,
    ) -> impl Iterator<Item = (usize, &'a LineComment)> + 'a {
        self.comments
            .iter()
            .enumerate()
            .filter(move |(_, c)| c.folder == folder)
    }

    /// Opens the comment field under the line at `at` in the diff of the
    /// file with `key`; the same line again closes it.
    pub(super) fn comment_on(&mut self, key: &str, at: usize) {
        let Some(file) = self.files().find(|(k, _)| k == key).map(|(_, f)| f) else {
            return;
        };
        let Some(line) = file.lines.get(at).filter(|l| l.kind != LineKind::Gap) else {
            return;
        };
        let commenting = Commenting {
            key: key.to_owned(),
            on: LineRef::new(&file.path, line),
        };
        self.commenting = if self.commenting.as_ref() == Some(&commenting) {
            None
        } else {
            Some(commenting)
        };
    }

    /// Adds the comment being written, unless it's empty.
    pub(super) fn add_comment(&mut self, text: &str) {
        let text = text.trim();
        let Some(folder) = self.folder().map(str::to_owned) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let Some(commenting) = self.commenting.take() else {
            return;
        };
        self.comments.push(LineComment {
            folder,
            on: commenting.on,
            text: text.to_owned(),
        });
    }

    /// Removes the comment at `index`.
    pub(super) fn remove_comment(&mut self, index: usize) {
        if index < self.comments.len() {
            self.comments.remove(index);
        }
    }

    /// Drops the folder's comments.
    pub(super) fn discard_comments(&mut self) {
        let Some(folder) = self.folder().map(str::to_owned) else {
            return;
        };
        self.comments.retain(|c| c.folder != folder);
        self.commenting = None;
    }
}

impl App {
    /// Sends the folder's comments to the agent as one message, after what
    /// the composer holds, the way the composer sends.
    pub(super) fn send_comments(&mut self) -> Task<Message> {
        if self.sending.is_some() {
            return Task::none();
        }
        let Some(folder) = self.changes.folder().map(str::to_owned) else {
            return Task::none();
        };
        let comments: Vec<_> = self
            .changes
            .comments_in(&folder)
            .map(|(_, c)| c.clone())
            .collect();
        if comments.is_empty() {
            return Task::none();
        }
        let message = comments_message(&comments);
        let typed = self.composer.text();
        let text = if typed.trim().is_empty() {
            message
        } else {
            format!("{}\n\n{message}", typed.trim_end())
        };
        self.composer.set(&text);
        let task = self.send(None);
        if self.sending.is_some() {
            self.changes.discard_comments();
        }
        task
    }
}

/// The message holding `comments`: each one's file, line and text.
#[must_use]
pub fn comments_message(comments: &[LineComment]) -> String {
    let mut message = String::from(if comments.len() == 1 {
        "A comment on the changes:\n"
    } else {
        "Comments on the changes:\n"
    });
    for comment in comments {
        let on = &comment.on;
        let place = match (on.kind, on.number) {
            (LineKind::Removed, Some(n)) => format!("line {n} (removed)"),
            (_, Some(n)) => format!("line {n}"),
            (_, None) => "a line".to_owned(),
        };
        let mark = match on.kind {
            LineKind::Added => '+',
            LineKind::Removed => '-',
            LineKind::Context | LineKind::Gap => ' ',
        };
        let _ = write!(
            message,
            "\n{}, {place}:\n```diff\n{mark}{}\n```\n{}\n",
            on.path, on.text, comment.text
        );
    }
    message
}
