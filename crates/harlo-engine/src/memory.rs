//! Notes: decisions carried from one session to the next.
//!
//! A note is a record with a source: its text, its scope (the project,
//! shared by all its worktrees, or the user), the session and message it
//! came from, and whether a newer note replaced it. The agent keeps notes
//! with the `remember` tool and finds the conversations behind them with
//! `search_sessions`; the notes that still hold are in every session's
//! system prompt, each with its ID so the agent can cite it.
//!
//! Where notes live is the caller's business: it implements [`Memory`].

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Who a note is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Every session of the project, in any of its worktrees.
    Project,
    /// Every session of the user's, in any project.
    User,
}

impl Scope {
    /// The scope's name, as the tools and the API spell it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::User => "user",
        }
    }
}

/// Where a note came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// The session.
    pub session: String,
    /// The message in it that kept the note.
    pub message: String,
    /// The session's title when the note was kept, if it had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// A decision kept between sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Its ID, such as `not_…`.
    pub id: String,
    /// Who it's for.
    pub scope: Scope,
    /// The project it belongs to, for a project note.
    #[serde(rename = "projectID", default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// What was decided.
    pub text: String,
    /// Where it came from.
    pub source: Source,
    /// When it was kept, in Unix milliseconds.
    pub created: i64,
    /// The note that replaced it, if one did.
    #[serde(
        rename = "supersededBy",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub superseded_by: Option<String>,
}

/// A note the agent asks to keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    /// What was decided.
    pub text: String,
    /// Who it's for.
    pub scope: Scope,
    /// The note it replaces, if it replaces one.
    pub replaces: Option<String>,
}

/// Where a session's notes are kept and its project's sessions searched.
pub trait Memory: Send + Sync + std::fmt::Debug {
    /// Keeps `draft`, kept by step `message`; returns the note.
    ///
    /// # Errors
    ///
    /// Fails, with a message for the model, when the note it replaces
    /// doesn't exist or the note can't be stored.
    fn remember(&self, draft: Draft, message: &str) -> Result<Note, String>;

    /// The project's conversations and notes that match `query`, written
    /// for the model.
    ///
    /// # Errors
    ///
    /// Fails, with a message for the model, when the query has no words
    /// or the search fails.
    fn search(&self, query: &str) -> Result<String, String>;

    /// What message `message` of one of the project's conversations says,
    /// with the messages around it, written for the model.
    ///
    /// # Errors
    ///
    /// Fails, with a message for the model, when the project has no such
    /// message or the read fails.
    fn read(&self, message: &str) -> Result<String, String>;
}

/// The memory tools' definitions, as a model is told about them.
#[must_use]
pub fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "remember",
            "description": "Keeps a decision for later sessions: what was decided and why, in a sentence or two. Project notes reach every session of this project, in any worktree; user notes reach all of the person's sessions. Keep only decisions the person made or agreed to. To change a decision, keep the new one with `replaces` set to the old note's ID.",
            "parameters": { "type": "object", "properties": {
                "text": { "type": "string", "description": "The decision and its reason." },
                "scope": { "type": "string", "enum": ["project", "user"], "description": "Default: project." },
                "replaces": { "type": "string", "description": "The ID of the note this one replaces." }
            }, "required": ["text"] }
        }),
        json!({
            "name": "search_sessions",
            "description": "Searches this project's earlier conversations and notes, including replaced notes, to find why something was decided. Lists matching messages by session, with a snippet.",
            "parameters": { "type": "object", "properties": {
                "query": { "type": "string", "description": "Words to look for." },
                "message": { "type": "string", "description": "Instead of searching: the ID of a listed message, to read it whole with the ones around it." }
            } }
        }),
    ]
}

/// Characters of one note the system prompt shows: a decision in a
/// sentence or two fits many times over.
const NOTE_CHARS: usize = 1_000;

/// Bytes of notes the system prompt shows in all, about 2,000 tokens,
/// paid on every request. The newest notes come first to that.
const NOTES_BYTES: usize = 8 * 1024;

/// The system prompt's section for `notes` (those still holding), or
/// nothing when there are none.
///
/// `notes` come oldest first. When they are more than the prompt has room
/// for, the oldest are left out and the section says how many.
#[must_use]
pub fn render(notes: &[Note]) -> String {
    let holding: Vec<&Note> = notes.iter().filter(|n| n.superseded_by.is_none()).collect();
    if holding.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut size = 0;
    for note in holding.iter().rev() {
        let from = note
            .source
            .title
            .as_deref()
            .map_or_else(String::new, |t| format!(", from \"{t}\""));
        let mut text: String = note.text.chars().take(NOTE_CHARS).collect();
        if text.len() < note.text.len() {
            text.push('…');
        }
        let line = format!(
            "- [{}] ({} note{from}) {text}\n",
            note.id,
            note.scope.as_str()
        );
        // (The newest note is shown whatever its size.)
        if !lines.is_empty() && size + line.len() > NOTES_BYTES {
            break;
        }
        size += line.len();
        lines.push(line);
    }
    let mut out = String::from(
        "Notes kept from earlier sessions. Follow them unless the person says otherwise, cite a \
         note by its ID when you rely on it, and use search_sessions to see why one was kept:\n",
    );
    out.extend(lines.iter().rev().map(String::as_str));
    let left_out = holding.len() - lines.len();
    if left_out > 0 {
        let _ = writeln!(
            out,
            "({left_out} older notes aren't shown here; search_sessions finds them.)"
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, text: &str, superseded_by: Option<&str>) -> Note {
        Note {
            id: id.into(),
            scope: Scope::Project,
            project: Some("p".into()),
            text: text.into(),
            source: Source {
                session: "ses_1".into(),
                message: "msg_1".into(),
                title: Some("Database".into()),
            },
            created: 1,
            superseded_by: superseded_by.map(str::to_owned),
        }
    }

    #[test]
    fn the_notes_section_has_a_size_and_keeps_the_newest() {
        // A very long note is cut; it is still shown.
        let long = render(&[note("not_1", &"x".repeat(5_000), None)]);
        assert!(long.len() < NOTE_CHARS + 400, "{}", long.len());
        assert!(long.trim_end().ends_with('…'));

        // More notes than there is room for: the oldest are left out.
        let many: Vec<Note> = (0..100)
            .map(|n| note(&format!("not_{n:03}"), &"decided. ".repeat(40), None))
            .collect();
        let text = render(&many);
        assert!(text.len() < NOTES_BYTES + 400, "{}", text.len());
        assert!(text.contains("[not_099]") && !text.contains("[not_000]"));
        let shown = text.matches("- [not_").count();
        assert!(
            text.ends_with(&format!(
                "({} older notes aren't shown here; search_sessions finds them.)\n",
                100 - shown
            )),
            "{text}"
        );
        // In the order they were kept.
        let (earlier, later) = (text.find("[not_098]"), text.find("[not_099]"));
        assert!(earlier < later, "{earlier:?} {later:?}");
    }

    #[test]
    fn only_notes_that_hold_reach_the_prompt() {
        assert_eq!(render(&[]), "");
        let text = render(&[
            note("not_1", "Use SQLite.", Some("not_2")),
            note("not_2", "Use Postgres: we need replicas.", None),
        ]);
        assert!(text.contains("- [not_2] (project note, from \"Database\") Use Postgres"));
        assert!(!text.contains("not_1"), "{text}");
    }
}
