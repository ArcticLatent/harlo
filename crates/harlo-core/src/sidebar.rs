//! Builds the flat sidebar row list: project headers, their sessions, and
//! an "Other sessions" group for sessions outside every project.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use harlo_harness::SessionInfo;

use crate::db::{Project, SessionMeta, WorktreeRecord};
use crate::session::SessionStatus;

/// Sessions shown per project before "Show more".
pub const SESSIONS_PER_PROJECT: usize = 12;

/// A session with a running harness process.
#[derive(Debug, Clone)]
pub struct LiveInfo {
    /// `live:<n>`; stable for the life of the process.
    pub key: String,
    pub harness: String,
    pub cwd: PathBuf,
    /// Known once the harness reports it (`get_state`).
    pub session_file: Option<PathBuf>,
    pub name: String,
    pub first_message: String,
    pub status: SessionStatus,
    pub needs_input: bool,
    pub unread: bool,
    pub started_at: SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarKind {
    Project,
    Session,
    OtherGroup,
    More,
}

impl SidebarKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Session => "session",
            Self::OtherGroup => "group",
            Self::More => "more",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarRow {
    pub kind: SidebarKind,
    /// Project: `project:<id>`. Session: live key if running, else the file path.
    pub key: String,
    pub project_id: i64,
    pub title: String,
    /// Session: cwd (other group) or empty. Project: path.
    pub subtitle: String,
    pub harness: String,
    /// busy | input | unread | live | starting | ""
    pub status: String,
    pub path: String,
    /// Unix seconds of last activity.
    pub modified: i64,
    pub collapsed: bool,
    pub pinned: bool,
    pub count: usize,
    /// Worktree branch for sessions that run in a worktree.
    pub branch: String,
    /// Session: its working directory.
    pub cwd: PathBuf,
    /// Session: uncommitted changed files in its repository (filled in by
    /// the caller; `build` leaves it 0).
    pub changes: usize,
    /// Project: its icon and colour (see `db::Project`).
    pub icon: String,
    pub color: String,
}

pub struct SidebarInput<'a> {
    pub projects: &'a [Project],
    /// Worktrees of the projects; sessions inside one belong to its project.
    pub worktrees: &'a [WorktreeRecord],
    pub sessions: &'a [SessionInfo],
    pub live: &'a [LiveInfo],
    pub meta: &'a HashMap<PathBuf, SessionMeta>,
    pub filter: &'a str,
    pub show_archived: bool,
    pub other_collapsed: bool,
    /// Project ids (and -1 for the other group) whose full list is shown.
    pub expanded: &'a [i64],
}

struct Entry {
    branch: String,
    owner: Option<i64>,
    key: String,
    path: Option<PathBuf>,
    cwd: PathBuf,
    title: String,
    harness: String,
    status: String,
    modified: i64,
    pinned: bool,
}

pub fn build(input: &SidebarInput<'_>) -> Vec<SidebarRow> {
    let filter = input.filter.trim().to_lowercase();
    let mut entries: Vec<Entry> = Vec::new();
    let mut live_by_file: HashMap<&Path, &LiveInfo> = HashMap::new();
    for l in input.live {
        if let Some(f) = &l.session_file {
            live_by_file.insert(f.as_path(), l);
        }
    }

    for s in input.sessions {
        let meta = input.meta.get(&s.path).cloned().unwrap_or_default();
        let live = live_by_file.remove(s.path.as_path());
        if meta.archived && !input.show_archived && live.is_none() {
            continue;
        }
        let (key, status) = match live {
            Some(l) => (l.key.clone(), live_status(l)),
            None => (s.path.to_string_lossy().into_owned(), String::new()),
        };
        let title = live.filter(|l| !l.name.is_empty()).map(|l| l.name.clone()).unwrap_or_else(|| s.display_title().to_owned());
        entries.push(Entry {
            key,
            path: Some(s.path.clone()),
            cwd: s.cwd.clone(),
            title,
            harness: s.harness.as_str().to_owned(),
            status,
            modified: unix(s.modified),
            pinned: meta.pinned,
            branch: String::new(),
            owner: None,
        });
    }
    // Live sessions whose file is not on disk yet (new, nothing sent).
    for l in input.live {
        let on_disk = l.session_file.as_ref().is_some_and(|f| input.sessions.iter().any(|s| &s.path == f));
        if on_disk {
            continue;
        }
        let title = if !l.name.is_empty() {
            l.name.clone()
        } else if !l.first_message.is_empty() {
            l.first_message.clone()
        } else {
            "New session".to_owned()
        };
        entries.push(Entry {
            key: l.key.clone(),
            path: l.session_file.clone(),
            cwd: l.cwd.clone(),
            title,
            harness: l.harness.clone(),
            status: live_status(l),
            modified: unix(l.started_at),
            pinned: false,
            branch: String::new(),
            owner: None,
        });
    }

    if !filter.is_empty() {
        entries.retain(|e| e.title.to_lowercase().contains(&filter));
    }
    entries.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.modified.cmp(&a.modified)));

    // Assign each session to a worktree's project, else to the deepest
    // project containing its cwd.
    for e in entries.iter_mut() {
        if let Some(w) = input
            .worktrees
            .iter()
            .filter(|w| e.cwd.starts_with(&w.path))
            .max_by_key(|w| w.path.components().count())
        {
            e.branch = w.branch.clone();
            e.owner = Some(w.project_id);
        } else {
            e.owner = input
                .projects
                .iter()
                .filter(|p| e.cwd.starts_with(&p.path))
                .max_by_key(|p| p.path.components().count())
                .map(|p| p.id);
        }
    }
    let mut by_project: HashMap<i64, Vec<&Entry>> = HashMap::new();
    let mut other: Vec<&Entry> = Vec::new();
    for e in &entries {
        match e.owner.filter(|id| input.projects.iter().any(|p| p.id == *id)) {
            Some(id) => by_project.entry(id).or_default().push(e),
            None => other.push(e),
        }
    }

    let filtering = !filter.is_empty();
    let mut rows = Vec::new();
    for p in input.projects {
        let sessions = by_project.remove(&p.id).unwrap_or_default();
        if filtering && sessions.is_empty() {
            continue;
        }
        let collapsed = p.collapsed && !filtering;
        rows.push(SidebarRow {
            kind: SidebarKind::Project,
            key: format!("project:{}", p.id),
            project_id: p.id,
            title: p.name.clone(),
            subtitle: if crate::remote::is_remote(&p.path) { crate::remote::display(&p.path) } else { p.path.to_string_lossy().into_owned() },
            harness: p.harness.clone(),
            status: aggregate_status(&sessions),
            path: p.path.to_string_lossy().into_owned(),
            modified: sessions.first().map_or(0, |e| e.modified),
            collapsed,
            pinned: false,
            count: sessions.len(),
            branch: String::new(),
            cwd: PathBuf::new(),
            changes: 0,
            icon: p.icon.clone(),
            color: p.color.clone(),
        });
        if !collapsed {
            push_sessions(&mut rows, &sessions, p.id, false, filtering || input.expanded.contains(&p.id));
        }
    }
    if !other.is_empty() {
        let collapsed = input.other_collapsed && !filtering;
        rows.push(SidebarRow {
            kind: SidebarKind::OtherGroup,
            key: "group:other".into(),
            project_id: -1,
            title: "Other sessions".into(),
            subtitle: String::new(),
            harness: String::new(),
            status: aggregate_status(&other),
            path: String::new(),
            modified: other.first().map_or(0, |e| e.modified),
            collapsed,
            pinned: false,
            count: other.len(),
            branch: String::new(),
            cwd: PathBuf::new(),
            changes: 0,
            icon: String::new(),
            color: String::new(),
        });
        if !collapsed {
            push_sessions(&mut rows, &other, -1, true, filtering || input.expanded.contains(&-1));
        }
    }
    rows
}

fn push_sessions(rows: &mut Vec<SidebarRow>, sessions: &[&Entry], project_id: i64, show_cwd: bool, all: bool) {
    // Running sessions are always listed, even past the limit.
    let limit = if all { usize::MAX } else { SESSIONS_PER_PROJECT };
    let mut hidden = 0;
    for (i, e) in sessions.iter().enumerate() {
        if i >= limit && e.status.is_empty() {
            hidden += 1;
            continue;
        }
        rows.push(SidebarRow {
            kind: SidebarKind::Session,
            key: e.key.clone(),
            project_id,
            title: e.title.clone(),
            subtitle: if show_cwd { tilde(&e.cwd) } else { String::new() },
            harness: e.harness.clone(),
            status: e.status.clone(),
            path: e.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            modified: e.modified,
            collapsed: false,
            pinned: e.pinned,
            count: 0,
            branch: e.branch.clone(),
            cwd: e.cwd.clone(),
            changes: 0,
            icon: String::new(),
            color: String::new(),
        });
    }
    if hidden > 0 {
        rows.push(SidebarRow {
            kind: SidebarKind::More,
            key: format!("more:{project_id}"),
            project_id,
            title: format!("Show {hidden} more"),
            subtitle: String::new(),
            harness: String::new(),
            status: String::new(),
            path: String::new(),
            modified: 0,
            collapsed: false,
            pinned: false,
            count: hidden,
            branch: String::new(),
            cwd: PathBuf::new(),
            changes: 0,
            icon: String::new(),
            color: String::new(),
        });
    }
}

/// Sidebar status of a live session: input | busy | starting | unread | live | "".
pub fn live_status(l: &LiveInfo) -> String {
    if l.needs_input {
        "input"
    } else if l.status == SessionStatus::Busy {
        "busy"
    } else if l.status == SessionStatus::Starting {
        "starting"
    } else if l.unread {
        "unread"
    } else if matches!(l.status, SessionStatus::Exited | SessionStatus::Offline | SessionStatus::Loading) {
        ""
    } else {
        "live"
    }
    .into()
}

/// The most urgent status among a group's sessions.
fn aggregate_status(sessions: &[&Entry]) -> String {
    for s in ["input", "busy", "unread"] {
        if sessions.iter().any(|e| e.status == s) {
            return s.into();
        }
    }
    String::new()
}

fn unix(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The path with the home folder shown as `~` (a remote one as `host:~/…`).
pub fn tilde(p: &Path) -> String {
    if crate::remote::is_remote(p) {
        return crate::remote::display(p);
    }
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(rest) = p.strip_prefix(&home) {
            return if rest.as_os_str().is_empty() { "~".into() } else { format!("~/{}", rest.display()) };
        }
    p.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use harlo_harness::HarnessKind;
    use std::time::Duration;

    fn session(path: &str, cwd: &str, title: &str, age_secs: u64) -> SessionInfo {
        SessionInfo {
            harness: HarnessKind::Pi,
            path: PathBuf::from(path),
            id: path.into(),
            cwd: PathBuf::from(cwd),
            name: String::new(),
            first_message: title.into(),
            created: String::new(),
            modified: SystemTime::now() - Duration::from_secs(age_secs),
            size: 1,
            parent_session: None,
        }
    }

    fn project(id: i64, path: &str) -> Project {
        Project { id, path: PathBuf::from(path), name: path.rsplit('/').next().unwrap().into(), harness: "pi".into(), collapsed: false, position: id, setup: String::new(), icon: String::new(), color: String::new() }
    }

    #[test]
    fn groups_sessions_under_deepest_project() {
        let projects = [project(1, "/w"), project(2, "/w/app")];
        let sessions = [
            session("/s/a", "/w/app/src", "in app", 10),
            session("/s/b", "/w", "in w", 5),
            session("/s/c", "/elsewhere", "stray", 1),
        ];
        let meta = HashMap::new();
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[] });
        let shape: Vec<(SidebarKind, &str)> = rows.iter().map(|r| (r.kind, r.title.as_str())).collect();
        assert_eq!(shape, [
            (SidebarKind::Project, "w"),
            (SidebarKind::Session, "in w"),
            (SidebarKind::Project, "app"),
            (SidebarKind::Session, "in app"),
            (SidebarKind::OtherGroup, "Other sessions"),
            (SidebarKind::Session, "stray"),
        ]);
    }

    #[test]
    fn live_session_replaces_its_file_row() {
        let projects = [project(1, "/w")];
        let sessions = [session("/s/a", "/w", "old title", 100)];
        let live = [LiveInfo {
            key: "live:1".into(),
            harness: "pi".into(),
            cwd: PathBuf::from("/w"),
            session_file: Some(PathBuf::from("/s/a")),
            name: String::new(),
            first_message: String::new(),
            status: SessionStatus::Busy,
            needs_input: false,
            unread: false,
            started_at: SystemTime::now(),
        }];
        let meta = HashMap::new();
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &live, meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[] });
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[1].key.as_str(), rows[1].status.as_str()), ("live:1", "busy"));
        assert_eq!(rows[0].status, "busy", "project shows the most urgent child status");
    }

    #[test]
    fn worktree_sessions_join_their_project() {
        let projects = [project(1, "/w")];
        let worktrees = [WorktreeRecord { path: PathBuf::from("/data/trees/w/feat"), project_id: 1, branch: "feat".into(), base: "main".into(), removed: false }];
        let sessions = [session("/s/a", "/data/trees/w/feat/src", "in worktree", 1), session("/s/b", "/w", "in main", 2)];
        let meta = HashMap::new();
        let rows = build(&SidebarInput { projects: &projects, worktrees: &worktrees, sessions: &sessions, live: &[], meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[] });
        let shape: Vec<(&str, &str)> = rows.iter().map(|r| (r.title.as_str(), r.branch.as_str())).collect();
        assert_eq!(shape, [("w", ""), ("in worktree", "feat"), ("in main", "")]);
    }

    #[test]
    fn archived_and_filtered_sessions_are_hidden() {
        let projects = [project(1, "/w")];
        let sessions = [session("/s/a", "/w", "alpha", 1), session("/s/b", "/w", "beta", 2)];
        let mut meta = HashMap::new();
        meta.insert(PathBuf::from("/s/a"), SessionMeta { archived: true, ..Default::default() });
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[] });
        assert_eq!(rows.iter().filter(|r| r.kind == SidebarKind::Session).count(), 1);
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter: "zzz", show_archived: true, other_collapsed: false, expanded: &[] });
        assert!(rows.is_empty());
    }

    #[test]
    fn long_lists_get_a_more_row() {
        let projects = [project(1, "/w")];
        let sessions: Vec<_> = (0..20).map(|i| session(&format!("/s/{i}"), "/w", "x", i)).collect();
        let meta = HashMap::new();
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[] });
        assert_eq!(rows.len(), 1 + SESSIONS_PER_PROJECT + 1);
        assert_eq!(rows.last().unwrap().kind, SidebarKind::More);
    }
}
