//! Notes kept between sessions, and search of a project's sessions.
//!
//! Off until the person turns it on (the `memory` setting). When on, each
//! turn gets a [`SessionMemory`] for the `remember` and `search_sessions`
//! tools, and the system prompt lists the notes that hold for the
//! session's project and for the user. People inspect, correct and forget
//! notes through `/harlo/notes`.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::sync::Arc;

use harlo_engine::memory::{Draft, Memory, Note, Scope, Source};
use harlo_engine::{ascending_id, now};

use crate::server::{Refusal, Server};

/// The setting that turns notes on.
const SETTING: &str = "memory";
/// The most messages a search lists.
const HITS: usize = 20;

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

    /// The memory for a turn of `session`, when notes are on.
    pub(crate) fn memory_for(
        self: &Arc<Self>,
        session: &crate::store::Session,
    ) -> Option<Arc<dyn Memory>> {
        self.memory_on().then(|| {
            Arc::new(SessionMemory {
                server: Arc::clone(self),
                session: session.id.clone(),
                project: session.project_id.clone(),
                title: session.title.clone(),
            }) as Arc<dyn Memory>
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
}

impl Memory for SessionMemory {
    fn remember(&self, draft: Draft, message: &str) -> Result<Note, String> {
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

    fn search(&self, query: &str) -> Result<String, String> {
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
        let notes = store
            .notes(Some(&self.project))
            .map_err(|e| e.to_string())?;
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
        let hits = store
            .search(&self.project, &words, HITS)
            .map_err(|e| e.to_string())?;
        // Grouped by session, in the order sessions first match.
        let mut sessions: Vec<(&str, Option<&str>, Vec<String>)> = Vec::new();
        for hit in &hits {
            let line = format!("  {}: {}", hit.message, hit.snippet.replace('\n', " "));
            match sessions.iter_mut().find(|(id, ..)| *id == hit.session) {
                Some((.., lines)) => lines.push(line),
                None => sessions.push((&hit.session, hit.title.as_deref(), vec![line])),
            }
        }
        if !sessions.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("Conversations:\n");
            for (id, title, lines) in sessions {
                let _ = writeln!(out, "Session \"{}\" ({id}):", title.unwrap_or("untitled"));
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
}
