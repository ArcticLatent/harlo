//! Notes kept between sessions, and search of earlier sessions.
//!
//! Each turn gets a [`SessionMemory`] for the `search_sessions` tool: it
//! finds the project's earlier conversations by their titles and by what
//! was said in them, reads a message with the ones around it, and reads a
//! session from its start. Asked to look everywhere, it does the same in
//! every project's conversations, which asks the person first.
//!
//! Notes are off until the person turns them on (the `memory` setting).
//! When on, the turn also gets the `remember` tool, searches find notes
//! too, and the system prompt lists the notes that hold for the session's
//! project and for the user. People inspect, correct and forget notes
//! through `/harlo/notes`.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::sync::Arc;

use harlo_engine::memory::{Draft, Find, Lookup, Memory, Note, Scope, Source};
use harlo_engine::{Message, ascending_id, now};

use crate::server::{Refusal, Server};
use crate::stats::day;

/// The setting that turns notes on.
const SETTING: &str = "memory";
/// The most messages a search lists.
const HITS: usize = 20;
/// The most sessions a search lists by title: the newest that match.
const TITLES: usize = 10;
/// How many messages are read from each end of a session read whole.
///
/// A session's start says what was asked and its end what came of it; a
/// longer one is read on by message.
const EDGE: usize = 60;
/// Characters of a session read whole that are shown from its start, and
/// from its end: about 6,000 tokens in all, as much as two large files.
const START_CHARS: usize = 8_000;
const END_CHARS: usize = 16_000;
/// What stands for the middle of a session too long to show whole.
const LEFT_OUT: &str =
    "(Messages between are left out. Search for their words, or read one by its ID, for more.)";
/// How many messages with something said are shown on each side of one
/// read from a search: enough to see what it answered and what came of it.
const AROUND: usize = 2;
/// How many messages are looked through on each side for those: runs' ends
/// and notices between them say nothing.
const NEAR: usize = 12;
/// Characters of the message read that are shown, and of each beside it.
const READ_CHARS: usize = 12_000;
const BESIDE_CHARS: usize = 2_000;

/// `message` as a conversation read back shows it, at most `limit`
/// characters of it; nothing when it says nothing.
fn said(message: &Message, limit: usize) -> Option<String> {
    let text = crate::store::searched(message);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let who = match message {
        Message::User { .. } => "Person",
        Message::Shell(_) => "Command",
        _ => "Agent",
    };
    let shown: String = text.chars().take(limit).collect();
    let cut = if shown.len() < text.len() {
        "\n(cut here)"
    } else {
        ""
    };
    Some(format!("{who} ({}):\n{shown}{cut}", message.id()))
}

impl Server {
    /// Whether notes are on.
    pub(crate) fn memory_on(&self) -> bool {
        self.store.setting(SETTING).ok().flatten().as_deref() == Some("on")
    }

    /// Turns notes on or off.
    pub(crate) fn set_memory(&self, on: bool) -> Result<(), Refusal> {
        self.store
            .put_setting(SETTING, if on { "on" } else { "off" })?;
        self.refresh_prompts();
        Ok(())
    }

    /// The user's notes and `project`'s, replaced ones included.
    pub(crate) fn notes(&self, project: Option<&str>) -> Result<Vec<Note>, Refusal> {
        Ok(self.store.notes(project)?)
    }

    /// Corrects note `id`'s text.
    pub(crate) fn correct_note(&self, id: &str, text: &str) -> Result<Note, Refusal> {
        let mut note = self
            .store
            .note(id)?
            .ok_or_else(|| Refusal::NotFound(format!("No note {id}.")))?;
        if text.trim().is_empty() {
            return Err(Refusal::Bad("A note needs text.".to_owned()));
        }
        text.trim().clone_into(&mut note.text);
        self.store.put_note(&note)?;
        self.refresh_prompts();
        Ok(note)
    }

    /// Forgets note `id`. Notes it replaced stay replaced.
    pub(crate) fn forget_note(&self, id: &str) -> Result<(), Refusal> {
        if self.store.delete_note(id)? {
            self.refresh_prompts();
            Ok(())
        } else {
            Err(Refusal::NotFound(format!("No note {id}.")))
        }
    }

    /// The memory for a turn of `session`: search of earlier sessions,
    /// and notes when they're on.
    pub(crate) fn memory_for(self: &Arc<Self>, session: &crate::store::Session) -> Arc<dyn Memory> {
        Arc::new(SessionMemory {
            server: Arc::clone(self),
            session: session.id.clone(),
            project: session.project_id.clone(),
            title: session.title.clone(),
            notes: self.memory_on(),
        })
    }

    /// The system prompt's notes section for `project`, when notes are on.
    pub(crate) fn notes_prompt(&self, project: &str) -> String {
        if !self.memory_on() {
            return String::new();
        }
        self.store
            .notes(Some(project))
            .map(|notes| harlo_engine::memory::render(&notes))
            .unwrap_or_default()
    }
}

/// One session's memory: notes it keeps name it as their source.
#[derive(Debug)]
pub(crate) struct SessionMemory {
    server: Arc<Server>,
    session: String,
    project: String,
    title: Option<String>,
    /// Whether notes are on.
    notes: bool,
}

impl Memory for SessionMemory {
    fn notes(&self) -> bool {
        self.notes
    }

    fn look(&self, lookup: &Lookup) -> Result<String, String> {
        match &lookup.find {
            Find::Words(query) => self.search(query, lookup.everywhere),
            Find::Message(id) => self.read(id, lookup.everywhere),
            Find::Session(id) => self.session(id, lookup.everywhere),
        }
    }

    fn remember(&self, draft: Draft, message: &str) -> Result<Note, String> {
        if !self.notes {
            return Err("Notes are off.".to_owned());
        }
        let store = &self.server.store;
        let replaced = match &draft.replaces {
            Some(id) => {
                let old = store
                    .note(id)
                    .map_err(|e| e.to_string())?
                    .filter(|n| n.project.is_none() || n.project.as_deref() == Some(&self.project))
                    .ok_or_else(|| format!("There is no note {id} to replace."))?;
                Some(old)
            }
            None => None,
        };
        let note = Note {
            id: ascending_id("not"),
            scope: draft.scope,
            project: (draft.scope == Scope::Project).then(|| self.project.clone()),
            text: draft.text.trim().to_owned(),
            source: Source {
                session: self.session.clone(),
                message: message.to_owned(),
                title: self.title.clone(),
            },
            created: now(),
            superseded_by: None,
        };
        store.put_note(&note).map_err(|e| e.to_string())?;
        if let Some(mut old) = replaced {
            old.superseded_by = Some(note.id.clone());
            store.put_note(&old).map_err(|e| e.to_string())?;
        }
        Ok(note)
    }
}

impl SessionMemory {
    /// The session `id` when this memory may read it: one of the project,
    /// or any with `everywhere`.
    ///
    /// # Errors
    ///
    /// Fails, with a message for the model, when there's none, or it is
    /// another project's and `everywhere` wasn't asked.
    fn readable(&self, id: &str, everywhere: bool) -> Result<crate::store::Session, String> {
        let found = self.server.store.session(id).map_err(|e| e.to_string())?;
        match found {
            Some(session) if everywhere || session.project_id == self.project => Ok(session),
            Some(_) => Err(format!(
                "Session {id} is another project's. Ask again with `everywhere` set to read it."
            )),
            None => Err(format!("There is no session {id}.")),
        }
    }

    /// The sessions whose titles have every word of `words`, newest
    /// first: this project's, or every project's; never the asking one,
    /// nor a subagent's.
    fn titled(&self, words: &[String], everywhere: bool) -> Result<Vec<String>, String> {
        let mut sessions = self.server.store.sessions().map_err(|e| e.to_string())?;
        sessions.retain(|s| {
            let title = s.title.as_deref().unwrap_or_default().to_lowercase();
            s.id != self.session
                && s.parent_id.is_none()
                && (everywhere || s.project_id == self.project)
                && words.iter().all(|w| title.contains(w.as_str()))
        });
        sessions.sort_by_key(|s| std::cmp::Reverse(s.time.updated));
        Ok(sessions
            .iter()
            .take(TITLES)
            .map(|s| {
                let folder = if everywhere {
                    format!(", folder {}", s.location.directory)
                } else {
                    String::new()
                };
                format!(
                    "- \"{}\" ({}){folder}, last active {}",
                    s.title.as_deref().unwrap_or("untitled"),
                    s.id,
                    day(s.time.updated)
                )
            })
            .collect())
    }

    fn search(&self, query: &str, everywhere: bool) -> Result<String, String> {
        let words: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect();
        if words.is_empty() {
            return Err("search_sessions needs words to look for.".to_owned());
        }
        let store = &self.server.store;
        let mut out = String::new();
        let notes = if self.notes {
            store
                .notes(Some(&self.project))
                .map_err(|e| e.to_string())?
        } else {
            Vec::new()
        };
        let matching: Vec<&Note> = notes
            .iter()
            .filter(|n| {
                let text = n.text.to_lowercase();
                words.iter().any(|w| text.contains(w.as_str()))
            })
            .collect();
        if !matching.is_empty() {
            out.push_str("Notes:\n");
            for n in matching {
                let state = n
                    .superseded_by
                    .as_deref()
                    .map_or_else(String::new, |by| format!(" (replaced by {by})"));
                let from = n
                    .source
                    .title
                    .as_deref()
                    .map_or_else(String::new, |t| format!(", kept in \"{t}\""));
                let _ = writeln!(
                    out,
                    "- [{}]{state} {}{from} ({}, message {})",
                    n.id, n.text, n.source.session, n.source.message
                );
            }
        }
        let titled = self.titled(&words, everywhere)?;
        if !titled.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("Sessions by title:\n");
            for line in titled {
                let _ = writeln!(out, "{line}");
            }
        }
        let project = (!everywhere).then_some(self.project.as_str());
        // (Not the asking session's own messages: the question just
        // asked has the very words, and the model has it already.)
        let hits = store
            .search(project, Some(&self.session), &words, HITS)
            .map_err(|e| e.to_string())?;
        // Grouped by session, in the order sessions first match.
        let mut sessions: Vec<(&crate::store::Hit, Vec<String>)> = Vec::new();
        for hit in &hits {
            let line = format!("  {}: {}", hit.message, hit.snippet.replace('\n', " "));
            match sessions
                .iter_mut()
                .find(|(first, _)| first.session == hit.session)
            {
                Some((_, lines)) => lines.push(line),
                None => sessions.push((hit, vec![line])),
            }
        }
        if !sessions.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("Conversations:\n");
            for (first, lines) in sessions {
                let folder = match (&first.directory, everywhere) {
                    (Some(directory), true) => format!(", folder {directory}"),
                    _ => String::new(),
                };
                let _ = writeln!(
                    out,
                    "Session \"{}\" ({}){folder}:",
                    first.title.as_deref().unwrap_or("untitled"),
                    first.session
                );
                for line in lines {
                    let _ = writeln!(out, "{line}");
                }
            }
        }
        if out.is_empty() {
            return Ok("Nothing found.".to_owned());
        }
        Ok(out.trim_end().to_owned())
    }

    fn read(&self, message: &str, everywhere: bool) -> Result<String, String> {
        let store = &self.server.store;
        let (session, asked) = store
            .message(message)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("There is no message {message}."))?;
        let owner = self.readable(&session, everywhere)?;
        let beside = |newest_first: bool| -> Result<Vec<String>, String> {
            let (page, _) = store
                .message_page(&session, newest_first, Some(message), NEAR)
                .map_err(|e| e.to_string())?;
            Ok(page
                .iter()
                .filter_map(|m| said(m, BESIDE_CHARS))
                .take(AROUND)
                .collect())
        };
        let mut parts = beside(true)?;
        parts.reverse();
        parts
            .push(said(&asked, READ_CHARS).unwrap_or_else(|| format!("({message} says nothing.)")));
        parts.extend(beside(false)?);
        Ok(format!(
            "Session \"{}\" ({session}):\n\n{}",
            owner.title.as_deref().unwrap_or("untitled"),
            parts.join("\n\n")
        ))
    }

    /// Session `id` from its start: what was said at its start and at its
    /// end, with what lies between left out when it is long.
    fn session(&self, id: &str, everywhere: bool) -> Result<String, String> {
        let store = &self.server.store;
        let owner = self.readable(id, everywhere)?;
        let (first, more) = store
            .message_page(id, false, None, EDGE)
            .map_err(|e| e.to_string())?;
        let mut last = if more {
            store
                .message_page(id, true, None, EDGE)
                .map_err(|e| e.to_string())?
                .0
        } else {
            Vec::new()
        };
        last.reverse();
        // (A session of fewer than two pages: its end overlaps its start.)
        last.retain(|m| !first.iter().any(|f| f.id() == m.id()));
        let start: Vec<String> = first.iter().filter_map(|m| said(m, BESIDE_CHARS)).collect();
        let end: Vec<String> = last.iter().filter_map(|m| said(m, BESIDE_CHARS)).collect();
        // The start: the first messages that fit.
        let mut room = START_CHARS + if more { 0 } else { END_CHARS };
        let mut shown = 0;
        for part in &start {
            if part.len() > room && shown > 0 {
                break;
            }
            room = room.saturating_sub(part.len());
            shown += 1;
        }
        // The end: the last messages that fit, of those not yet shown.
        let rest: Vec<&String> = start[shown..].iter().chain(&end).collect();
        let mut room = END_CHARS;
        let mut from = rest.len();
        while from > 0 && rest[from - 1].len() <= room {
            room -= rest[from - 1].len();
            from -= 1;
        }
        // (Two full pages that share no message may have more between.)
        let left_out = from > 0 || (more && last.len() == EDGE);
        let mut parts: Vec<&str> = start[..shown].iter().map(String::as_str).collect();
        if left_out {
            parts.push(LEFT_OUT);
        }
        parts.extend(rest[from..].iter().map(|p| p.as_str()));
        Ok(format!(
            "Session \"{}\" ({id}), folder {}, last active {}:\n\n{}",
            owner.title.as_deref().unwrap_or("untitled"),
            owner.location.directory,
            day(owner.time.updated),
            parts.join("\n\n")
        ))
    }
}
