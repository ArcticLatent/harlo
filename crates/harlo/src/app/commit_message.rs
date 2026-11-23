//! ✦ in the Changes panel: a commit message written from the uncommitted
//! files' diffs by the model chosen in Settings → Git, through OpenCode's
//! one-off text generation (no tools, and no session kept).

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use harlo_core::diff::LineKind;

use super::changes::{ChangedFile, Event};
use super::{App, Message};
use crate::task::Task;

/// The most diff text a prompt holds; past it, files are only named.
const MOST_DIFF: usize = 24_000;

/// What the model is asked, before the diffs.
const ASK: &str = "Write a git commit message for the changes below. Reply with the message \
                   alone: a summary line of at most 72 characters in the imperative mood, then, \
                   only if the change needs explaining, a blank line and a short body wrapped at \
                   72 characters. No code fences, quotes or commentary.";

impl App {
    /// Asks the commit-message model for a message for every uncommitted
    /// file in the Changes panel.
    pub(super) fn write_commit_message(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let Some(folder) = self.changes.folder().map(str::to_owned) else {
            return Task::none();
        };
        let Some(Ok(files)) = &self.changes.tree else {
            return Task::none();
        };
        if files.is_empty() || self.changes.writing || self.changes.running.is_some() {
            return Task::none();
        }
        let prompt = commit_prompt(files);
        let model = self.commit_model();
        self.changes.writing = true;
        self.changes.outcome = None;
        Task::perform(
            async move {
                client
                    .generate_text(&prompt, model.as_ref())
                    .await
                    .map(|text| clean_message(&text))
                    .map_err(|e| format!("Couldn't write a message: {e}"))
            },
            move |written| Message::Changes(Event::MessageWritten(folder, written)),
        )
    }

    /// Takes a written message, unless the panel moved to another folder.
    pub(super) fn commit_message_written(&mut self, folder: &str, written: Result<String, String>) {
        self.changes.writing = false;
        if self.changes.folder() != Some(folder) {
            return;
        }
        match written {
            Ok(message) if message.is_empty() => {
                self.changes.outcome = Some(Err("The model wrote nothing.".to_owned()));
            }
            Ok(message) => self.changes.message = message,
            Err(why) => self.changes.outcome = Some(Err(why)),
        }
    }
}

/// The prompt asking for a commit message for `files`: each one's diff,
/// up to [`MOST_DIFF`] characters in all, then the rest by name.
#[must_use]
pub fn commit_prompt(files: &[ChangedFile]) -> String {
    let mut prompt = format!("{ASK}\n");
    let mut left_out = Vec::new();
    for file in files {
        let mut part = format!(
            "\n--- {} (+{} -{})\n",
            file.path, file.additions, file.deletions
        );
        if file.lines.is_empty() {
            part.push_str("(no diff to show)\n");
        }
        for line in &file.lines {
            let mark = match line.kind {
                LineKind::Added => '+',
                LineKind::Removed => '-',
                LineKind::Context => ' ',
                LineKind::Gap => {
                    part.push_str("…\n");
                    continue;
                }
            };
            let _ = writeln!(part, "{mark}{}", line.text());
        }
        if prompt.len() + part.len() > MOST_DIFF {
            left_out.push(file.path.as_str());
        } else {
            prompt.push_str(&part);
        }
    }
    if !left_out.is_empty() {
        let _ = write!(
            prompt,
            "\nAlso changed, too large to show: {}\n",
            left_out.join(", ")
        );
    }
    prompt
}

/// A model's reply as a commit message: without fences or quotes around
/// it, or blank lines at its ends.
#[must_use]
pub fn clean_message(reply: &str) -> String {
    let mut text = reply.trim();
    if let Some(inner) = text.strip_prefix("```") {
        // Past the fence's language name, up to the closing fence.
        let inner = inner.split_once('\n').map_or("", |(_, rest)| rest);
        text = inner.trim_end().strip_suffix("```").unwrap_or(inner).trim();
    }
    for quote in ['"', '\'', '`'] {
        if let Some(inner) = text.strip_prefix(quote).and_then(|t| t.strip_suffix(quote)) {
            text = inner.trim();
        }
    }
    text.to_owned()
}
