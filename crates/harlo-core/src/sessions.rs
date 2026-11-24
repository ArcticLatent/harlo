//! What the sidebar lists: projects, their worktrees, and sessions.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

/// Sessions a list shows before "Show N more".
pub const VISIBLE_SESSIONS: usize = 12;

/// A project (a folder OpenCode works in), its worktrees and their sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The project's main folder; it identifies the project in the sidebar.
    pub directory: String,
    /// The name people see.
    pub name: String,
    /// Its mark's colour, one of OpenCode's project colours, if chosen.
    pub color: Option<String>,
    /// The URL of an image chosen as its icon, often a `data:` one.
    pub icon: Option<String>,
    /// Sessions in the main folder, newest first.
    pub sessions: Vec<SessionSummary>,
    /// The project's worktrees, each with its sessions.
    pub worktrees: Vec<Worktree>,
}

/// A worktree: another checkout of a project, in a folder of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// The worktree's folder; it identifies the worktree in the sidebar.
    pub directory: String,
    /// The name people see: the folder's name.
    pub name: String,
    /// Sessions in this worktree, newest first.
    pub sessions: Vec<SessionSummary>,
}

/// One row in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// OpenCode's session ID.
    pub id: String,
    /// The title; empty until the server names the session.
    pub title: String,
    /// The provider of the session's model, such as `anthropic`; `None` when unknown.
    pub maker: Option<String>,
    /// Time since the last change.
    pub age: Duration,
    /// Whether the session is working right now.
    pub busy: bool,
    /// The agent it runs, such as `explore`, when known.
    pub agent: Option<String>,
    /// Whether it waits for the user: an approval or an answer, its
    /// subagents' included.
    pub waiting: bool,
    /// Whether all it waits for are answers to questions, no approvals.
    pub asking: bool,
    /// The sessions it handed work to, oldest first.
    pub children: Vec<SessionSummary>,
}

impl Project {
    /// Returns how many sessions the project has, worktrees included.
    #[must_use]
    pub fn total(&self) -> usize {
        self.sessions.len()
            + self
                .worktrees
                .iter()
                .map(|w| w.sessions.len())
                .sum::<usize>()
    }

    /// Returns how many of the project's sessions are working, worktrees included.
    #[must_use]
    pub fn working(&self) -> usize {
        let busy = |list: &[SessionSummary]| list.iter().filter(|s| s.busy).count();
        busy(&self.sessions)
            + self
                .worktrees
                .iter()
                .map(|w| busy(&w.sessions))
                .sum::<usize>()
    }
}

/// Returns the sessions to list and how many more are hidden.
///
/// A non-empty `filter` keeps titles containing it, ignoring case, and shows
/// every match. Otherwise the first [`VISIBLE_SESSIONS`] are listed unless
/// `expanded`. The sessions may come with more, such as where each runs.
#[must_use]
pub fn visible<'a, T: std::borrow::Borrow<SessionSummary>>(
    sessions: &'a [T],
    filter: &str,
    expanded: bool,
) -> (Vec<&'a T>, usize) {
    let filter = filter.trim().to_lowercase();
    if !filter.is_empty() {
        let found = sessions
            .iter()
            .filter(|s| s.borrow().title.to_lowercase().contains(&filter))
            .collect();
        return (found, 0);
    }
    let shown = if expanded {
        sessions.len()
    } else {
        sessions.len().min(VISIBLE_SESSIONS)
    };
    (
        sessions.iter().take(shown).collect(),
        sessions.len() - shown,
    )
}

/// Returns the last part of a folder path, which names a project or worktree.
#[must_use]
pub fn folder_name(directory: &str) -> &str {
    let trimmed = directory.trim_end_matches('/');
    trimmed
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(trimmed)
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
