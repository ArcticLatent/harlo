//! Undoing turns: what a staged undo restores, and which of the user's own
//! edits that restore overwrites.
//!
//! OpenCode undoes a turn by putting every file the agent touched back to
//! how it was before the turn. Edits the user made to those files since are
//! overwritten with it. Staging the undo with files applies the restore
//! right away, but cancelling puts every file back, so staging is safe.
//! Its patch shows what the restore changes on disk.
//!
//! To tell the user's lines from the agent's, the restore is compared with
//! the agent's own changes over the same turns (from the session diff, which
//! compares the agent's snapshots). A line the restore removes that the
//! agent never added, or brings back that the agent never removed, is one of
//! the user's edits being lost.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;

use opencode_client::{FileDiff, Revert};

use crate::diff::{self, LineKind};

/// One file a staged undo puts back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restore {
    /// What putting it back changes, compared with the file right now.
    pub file: FileDiff,
    /// How many of the user's own changed lines it overwrites.
    ///
    /// `None` when the agent's changes aren't known, such as for an undo
    /// staged by another client.
    pub lost: Option<usize>,
}

/// The files a staged undo restores, each with the user's edits it overwrites.
///
/// `agent` is what the agent changed over the turns being undone, if known.
#[must_use]
pub fn restores(revert: &Revert, agent: Option<&[FileDiff]>) -> Vec<Restore> {
    revert
        .files
        .iter()
        .map(|file| Restore {
            file: file.clone(),
            lost: agent.map(|agent| {
                let theirs = agent.iter().find(|a| a.file == file.file);
                lost(file, theirs)
            }),
        })
        .collect()
}

/// Counts the user's lines a restore changes: ones the agent's changes don't explain.
fn lost(restore: &FileDiff, agent: Option<&FileDiff>) -> usize {
    // What the agent added, the restore removes; what it removed comes back.
    let mut added: HashMap<String, usize> = HashMap::new();
    let mut removed: HashMap<String, usize> = HashMap::new();
    if let Some(agent) = agent {
        for line in diff::lines(&agent.patch) {
            match line.kind {
                LineKind::Added => *added.entry(line.text()).or_default() += 1,
                LineKind::Removed => *removed.entry(line.text()).or_default() += 1,
                LineKind::Context | LineKind::Gap => {}
            }
        }
    }
    // A line the user changed shows on both sides; it counts once.
    let (mut gone, mut back) = (0, 0);
    for line in diff::lines(&restore.patch) {
        let (explained, unexplained) = match line.kind {
            LineKind::Removed => (&mut added, &mut gone),
            LineKind::Added => (&mut removed, &mut back),
            LineKind::Context | LineKind::Gap => continue,
        };
        match explained.get_mut(&line.text()) {
            Some(count) if *count > 0 => *count -= 1,
            _ => *unexplained += 1,
        }
    }
    gone.max(back)
}
