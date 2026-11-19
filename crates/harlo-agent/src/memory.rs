//! Memory: notes kept between sessions (`HARLO_AGENT_MEMORY=on`).
//!
//! The `memory` tool adds a note (a fact or a lesson: a command that
//! works, a pitfall, a convention) or removes one. Notes are kept per
//! project (its top folder) or for every project (`scope: user`), in
//! `$XDG_DATA_HOME/harlo-agent/memory/`, and the next sessions get them in
//! their instructions, so they're cached with them. Each file holds at
//! most [`MAX_NOTES`] notes: a full one takes a removal first. Off by
//! default: the notes and the tool cost every request.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const NAME: &str = "memory";
/// Notes in one file, at most.
pub const MAX_NOTES: usize = 40;
/// Of one note, at most (characters).
const NOTE_CHARS: usize = 300;

/// Writes one at a time (several calls in a batch run at once).
static WRITING: Mutex<()> = Mutex::new(());

/// Whether sessions keep notes (`HARLO_AGENT_MEMORY=on`).
pub fn on() -> bool {
    std::env::var("HARLO_AGENT_MEMORY").as_deref() == Ok("on")
}

/// The tool's definition.
pub fn spec() -> Value {
    json!({
        "type": "function", "name": NAME, "strict": false,
        "description": "Keeps a note for later sessions: something non-obvious you learned that would save them work (a command that works, a pitfall, a convention). Not what the code, its docs or git already say. Remove a note that turned out wrong.",
        "parameters": {
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["add", "remove"] },
                "text": { "type": "string", "description": "The note, in a line (remove: part of it)" },
                "scope": { "type": "string", "enum": ["project", "user"], "description": "project (default), or user: for every project" },
            },
            "required": ["action", "text"], "additionalProperties": false,
        },
    })
}

/// Where notes are kept: `$XDG_DATA_HOME/harlo-agent/memory` (or
/// `~/.local/share/…`).
fn root() -> Option<PathBuf> {
    crate::session::root().and_then(|s| s.parent().map(|p| p.join("memory")))
}

/// The file of `scope`'s notes for the project at `cwd`, under `root`.
fn file(root: &Path, cwd: &Path, scope: &str) -> PathBuf {
    if scope == "user" { root.join("user.md") } else { crate::session::dir_for(root, &crate::rules::top(cwd)).with_extension("md") }
}

fn notes_in(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| l.strip_prefix("- ")).map(str::to_owned).collect()
}

/// The notes for a session at `cwd`, for its instructions (None: none).
pub fn loaded(cwd: &Path) -> Option<String> {
    loaded_in(&root().filter(|_| on())?, cwd)
}

fn loaded_in(root: &Path, cwd: &Path) -> Option<String> {
    let mut out = String::new();
    for (scope, heading) in [("user", "For every project"), ("project", "For this project")] {
        let notes = notes_in(&file(root, cwd, scope));
        if !notes.is_empty() {
            let _ = writeln!(out, "{heading}:");
            for n in notes {
                let _ = writeln!(out, "- {n}");
            }
        }
    }
    (!out.is_empty()).then(|| format!("\n<memory>\nNotes kept in earlier sessions (check one against the code before relying on it; remove it with memory if it's wrong):\n{out}</memory>\n"))
}

/// Runs a call: its result, and whether it failed.
pub fn call(args: &Value, cwd: &Path) -> (String, bool) {
    match root() {
        Some(root) => call_in(&root, args, cwd),
        None => ("There's no home folder to keep notes in.".to_owned(), true),
    }
}

fn call_in(root: &Path, args: &Value, cwd: &Path) -> (String, bool) {
    let text: String = args["text"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return ("Give the note in text.".to_owned(), true);
    }
    let scope = if args["scope"] == "user" { "user" } else { "project" };
    let path = file(root, cwd, scope);
    let _held = WRITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut notes = notes_in(&path);
    let message = match args["action"].as_str() {
        Some("add") => {
            if text.chars().count() > NOTE_CHARS {
                return (format!("A note is a line: at most {NOTE_CHARS} characters."), true);
            }
            if notes.iter().any(|n| n.eq_ignore_ascii_case(&text)) {
                return ("That note is already kept.".to_owned(), false);
            }
            if notes.len() >= MAX_NOTES {
                return (format!("The {scope} notes are full ({MAX_NOTES}): remove one that matters less first."), true);
            }
            notes.push(text);
            format!("Kept ({scope}, {} of {MAX_NOTES}).", notes.len())
        }
        Some("remove") => {
            let lower = text.to_lowercase();
            let before = notes.len();
            notes.retain(|n| !n.to_lowercase().contains(&lower));
            if notes.len() == before {
                return (format!("No {scope} note has \"{text}\" in it."), true);
            }
            format!("Removed {}.", before - notes.len())
        }
        _ => return ("action is add or remove.".to_owned(), true),
    };
    let body: String = notes.iter().map(|n| format!("- {n}\n")).collect();
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| {
        let tmp = path.with_extension("md.tmp");
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &path)
    });
    match written {
        Ok(()) => (message, false),
        Err(e) => (format!("Couldn't keep it: {e}"), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_kept_per_project_and_for_every_one() {
        let dir = std::env::temp_dir().join(format!("harlo-memory-{}", std::process::id()));
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let root = dir.join("memory");
        let call = |args: Value, cwd: &Path| call_in(&root, &args, cwd);
        let loaded = |cwd: &Path| loaded_in(&root, cwd);
        assert_eq!(call(json!({ "action": "add", "text": "Tests  run with\nmake check." }), &a), ("Kept (project, 1 of 40).".to_owned(), false));
        assert!(!call(json!({ "action": "add", "text": "tests run with make check." }), &a).1);
        assert!(!call(json!({ "action": "add", "text": "Answer briefly.", "scope": "user" }), &a).1);
        let shown = loaded(&a).unwrap();
        assert!(shown.contains("For every project:\n- Answer briefly.\nFor this project:\n- Tests run with make check.\n"), "{shown}");
        assert!(!loaded(&b).unwrap().contains("make check"));
        assert!(call(json!({ "action": "remove", "text": "nothing like it" }), &a).1);
        assert_eq!(call(json!({ "action": "remove", "text": "MAKE CHECK" }), &a).0, "Removed 1.");
        for i in 0..MAX_NOTES {
            let _ = call(json!({ "action": "add", "text": format!("note {i}") }), &b);
        }
        assert!(call(json!({ "action": "add", "text": "one more" }), &b).0.contains("full"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
