//! What the sidebar lists: projects and their sessions.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

/// Sessions a project shows before "Show N more".
pub const VISIBLE_SESSIONS: usize = 12;

/// A project (a folder OpenCode works in) and its sessions, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The folder name people see.
    pub name: String,
    /// Sessions working right now.
    pub working: usize,
    /// Every session, newest first.
    pub sessions: Vec<SessionSummary>,
}

/// One row in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// OpenCode's session ID.
    pub id: String,
    /// The title, or the first prompt when it has none.
    pub title: String,
    /// Who made the model the session uses, as a provider logo ID such as
    /// `anthropic`; `None` when unknown.
    pub maker: Option<String>,
    /// Time since the last change.
    pub age: Duration,
}

impl Project {
    /// Returns the sessions to list and how many more are hidden.
    ///
    /// A non-empty `filter` keeps titles containing it, ignoring case, and
    /// shows every match. Otherwise the first [`VISIBLE_SESSIONS`] are listed
    /// unless `expanded`.
    #[must_use]
    pub fn visible(&self, filter: &str, expanded: bool) -> (Vec<&SessionSummary>, usize) {
        let filter = filter.trim().to_lowercase();
        if !filter.is_empty() {
            let found = self
                .sessions
                .iter()
                .filter(|s| s.title.to_lowercase().contains(&filter))
                .collect();
            return (found, 0);
        }
        let shown = if expanded {
            self.sessions.len()
        } else {
            self.sessions.len().min(VISIBLE_SESSIONS)
        };
        (
            self.sessions.iter().take(shown).collect(),
            self.sessions.len() - shown,
        )
    }
}

/// Formats a session's age the way the sidebar shows it: `now`, `4m`, `4h`, `10d`, `1mo`, `2y`.
#[must_use]
pub fn compact_age(age: Duration) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    // Months and years are approximate; the label only needs the right order of magnitude.
    const MONTH: u64 = 30 * DAY;
    const YEAR: u64 = 365 * DAY;
    let secs = age.as_secs();
    match secs {
        s if s < MINUTE => "now".to_owned(),
        s if s < HOUR => format!("{}m", s / MINUTE),
        s if s < DAY => format!("{}h", s / HOUR),
        s if s < MONTH => format!("{}d", s / DAY),
        s if s < YEAR => format!("{}mo", s / MONTH),
        s => format!("{}y", s / YEAR),
    }
}
