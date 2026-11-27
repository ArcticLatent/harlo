//! Settings → Memory: the switch, the notes the server proposes, each to
//! keep or dismiss, and the notes the agent kept, each with where it came
//! from; one at a time corrected or forgotten.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use harlo_core::server::{Note as KeptNote, NoteScope, Proposal, ProposalKind};

use super::{field, heading, int, note, pair, switch};
use crate::app::{App, Field, Load, Setting};
use crate::ui::SheetRow;
use crate::window;

/// The kind of row with a name, a line under it and a button.
const LINED: i32 = 13;

/// The section's rows.
pub(super) fn memory_rows(app: &App) -> Vec<SheetRow> {
    let memory = &app.sheets.memory;
    let kept = match &memory.kept {
        Load::Loaded(kept) => kept,
        Load::Failed(why) => return vec![pair("Notes", why, 2)],
        Load::Idle | Load::Loading => return vec![pair("Notes", "Loading…", 3)],
    };
    if !kept.supported {
        return vec![note(
            "This server doesn't keep notes between sessions; a newer Harlo server does.",
        )];
    }
    let mut rows = vec![
        note(
            "The agent keeps decisions you make as notes, with the session they came from. \
             Project notes reach every session of the project, in any worktree.",
        ),
        switch(
            Setting::Notes,
            "Keep notes between sessions",
            "For every session on this server.",
            kept.on,
        ),
    ];
    if let Some(why) = &memory.error {
        rows.push(pair("Couldn't change it", why, 2));
    }
    if !kept.proposals.is_empty() {
        rows.push(heading("Suggested"));
        rows.push(note(
            "From what was said in a session and the agent didn't keep. Nothing here reaches \
             a session until you keep it.",
        ));
        for (index, proposal) in kept.proposals.iter().enumerate() {
            proposal_rows(app, index, proposal, &kept.notes, &mut rows);
        }
    }
    let (project, user): (Vec<_>, Vec<_>) = kept
        .notes
        .iter()
        .enumerate()
        .partition(|(_, n)| n.scope == NoteScope::Project);
    rows.push(heading("This project"));
    note_rows(app, &project, "None yet.", &mut rows);
    rows.push(heading("All your sessions"));
    note_rows(app, &user, "None yet.", &mut rows);
    rows
}

/// What a proposed note is called, by its kind.
fn kind_name(kind: ProposalKind) -> &'static str {
    match kind {
        ProposalKind::Correction => "A correction you gave",
        ProposalKind::Decision => "A decision",
        ProposalKind::Pitfall => "Something that went wrong",
    }
}

/// The rows of proposed note `proposal`, the one at `index`: one row with
/// a Review button, or, while it is looked at, its words to keep, where
/// it comes from, the note it would replace, and Dismiss.
fn proposal_rows(
    app: &App,
    index: usize,
    proposal: &Proposal,
    notes: &[KeptNote],
    rows: &mut Vec<SheetRow>,
) {
    let from = proposal
        .source
        .title
        .as_deref()
        .map_or_else(|| "From a session".to_owned(), |t| format!("From “{t}”"));
    let replaced = proposal.replaces.as_deref().and_then(|old| {
        notes
            .iter()
            .find(|n| n.id == old && n.superseded_by.is_none())
            .map(|n| n.text.as_str())
    });
    let scope = match proposal.scope {
        NoteScope::Project => "for this project",
        NoteScope::User => "for all your sessions",
    };
    if app.sheets.memory.reviewing.as_deref() != Some(proposal.id.as_str()) {
        let mut value = format!("{}\n{from} · {scope}", proposal.text);
        if let Some(old) = replaced {
            let _ = write!(value, " · would replace “{old}”");
        }
        rows.push(SheetRow {
            kind: LINED,
            name: kind_name(proposal.kind).into(),
            hint: if replaced.is_some() {
                "Replaces a note"
            } else {
                ""
            }
            .into(),
            value: value.into(),
            action: "Review".into(),
            setting: window::REVIEW_PROPOSAL,
            choice: int(index),
            ..SheetRow::default()
        });
        return;
    }
    let mut text = field(Field::NoteText, "Note", &proposal.text, "What to remember");
    text.action = "Keep".into();
    text.saves = false;
    rows.push(text);
    let mut source = pair(&from, &format!("“{}”", proposal.excerpt), 3);
    source.action = "Open".into();
    source.setting = window::OPEN_PROPOSAL_SESSION;
    rows.push(source);
    if let Some(old) = replaced {
        rows.push(pair("Keeping it replaces", &format!("“{old}”"), 4));
    }
    let mut dismiss = pair("Dismiss this suggestion", "It won't be suggested again", 3);
    dismiss.action = "Dismiss".into();
    dismiss.setting = window::DISMISS_PROPOSAL;
    dismiss.risky = true;
    rows.push(dismiss);
    let mut cancel = pair("Decide later", "", 3);
    cancel.action = "Cancel".into();
    cancel.setting = window::CANCEL_NOTE;
    rows.push(cancel);
}

/// `notes`, numbered by their place in the loaded list, or `empty`.
fn note_rows(app: &App, notes: &[(usize, &KeptNote)], empty: &str, rows: &mut Vec<SheetRow>) {
    if notes.is_empty() {
        rows.push(note(empty));
        return;
    }
    let all = match &app.sheets.memory.kept {
        Load::Loaded(kept) => kept.notes.as_slice(),
        _ => &[],
    };
    for (index, kept) in notes {
        if app.sheets.memory.editing.as_deref() == Some(kept.id.as_str()) {
            let mut text = field(Field::NoteText, "Note", &kept.text, "What was decided");
            text.action = "Save".into();
            text.saves = false;
            rows.push(text);
            let mut forget = pair("Forget this note", "No session will see it again", 3);
            forget.action = "Forget".into();
            forget.setting = window::FORGET_NOTE;
            forget.risky = true;
            rows.push(forget);
            let mut cancel = pair("Keep it as it was", "", 3);
            cancel.action = "Cancel".into();
            cancel.setting = window::CANCEL_NOTE;
            rows.push(cancel);
            continue;
        }
        let replaced = kept.superseded_by.as_deref().map(|by| {
            all.iter().find(|n| n.id == by).map_or_else(
                || "a forgotten note".to_owned(),
                |n| format!("“{}”", n.text),
            )
        });
        let from = kept
            .source
            .title
            .as_deref()
            .map_or_else(|| "From a session".to_owned(), |t| format!("From “{t}”"));
        let mut value = format!("{}\n{from}", kept.text);
        if let Some(by) = &replaced {
            let _ = write!(value, " · replaced by {by}");
        }
        rows.push(SheetRow {
            kind: LINED,
            name: kept_ago(kept.created).into(),
            hint: if replaced.is_some() { "Replaced" } else { "" }.into(),
            value: value.into(),
            action: "Edit".into(),
            setting: window::EDIT_NOTE,
            choice: int(*index),
            ..SheetRow::default()
        });
    }
}

/// "Kept 3h ago" or "Kept just now", from when a note was kept, in Unix milliseconds.
fn kept_ago(created: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let then = u128::try_from(created).unwrap_or(0);
    let age = Duration::from_millis(u64::try_from(now.saturating_sub(then)).unwrap_or(u64::MAX));
    match harlo_core::sessions::compact_age(age).as_str() {
        "now" => "Kept just now".to_owned(),
        ago => format!("Kept {ago} ago"),
    }
}
