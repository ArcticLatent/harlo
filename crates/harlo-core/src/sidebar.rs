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
    /// A project's Settled shelf (its sessions follow when it's open).
    Shelf,
}

impl SidebarKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Session => "session",
            Self::OtherGroup => "group",
            Self::More => "more",
            Self::Shelf => "shelf",
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
    /// Session on a Settled shelf: why ("merged", "closed", "quiet").
    pub settled: String,
    /// Session another one started (orchestration), shown under it: that
    /// session's title, and how deep ([`nest_helpers`]).
    pub helper_of: String,
    pub nest: u32,
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
    /// Project ids whose Settled shelf is open.
    pub shelves_open: &'a [i64],
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
    /// On its project's Settled shelf: why.
    settled: String,
}

/// A session of a project, whatever the list shows (archived, on a shelf).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSession {
    /// The live key if it's running, else its file.
    pub key: String,
    pub harness: String,
    pub archived: bool,
    pub running: bool,
    /// In one of the project's worktrees.
    pub worktree: bool,
}

/// Which worktree holds a session, else the deepest project containing its
/// cwd: its branch and its project.
fn owner_of(cwd: &Path, projects: &[Project], worktrees: &[WorktreeRecord]) -> (String, Option<i64>) {
    match worktrees.iter().filter(|w| cwd.starts_with(&w.path)).max_by_key(|w| w.path.components().count()) {
        Some(w) => (w.branch.clone(), Some(w.project_id)),
        None => (String::new(), projects.iter().filter(|p| cwd.starts_with(&p.path)).max_by_key(|p| p.path.components().count()).map(|p| p.id)),
    }
}

/// All a project's sessions (each under the project `build` lists it under).
pub fn project_sessions(id: i64, input: &SidebarInput<'_>) -> Vec<ProjectSession> {
    let ours = |cwd: &Path| owner_of(cwd, input.projects, input.worktrees);
    let mut out = Vec::new();
    for s in input.sessions {
        let (branch, owner) = ours(&s.cwd);
        if owner == Some(id) {
            let live = input.live.iter().find(|l| l.session_file.as_ref() == Some(&s.path));
            out.push(ProjectSession {
                key: live.map_or_else(|| s.path.to_string_lossy().into_owned(), |l| l.key.clone()),
                harness: s.harness.as_str().to_owned(),
                archived: input.meta.get(&s.path).is_some_and(|m| m.archived),
                running: live.is_some(),
                worktree: !branch.is_empty(),
            });
        }
    }
    // (Running with no file yet: new, nothing sent.)
    for l in input.live.iter().filter(|l| !l.session_file.as_ref().is_some_and(|f| input.sessions.iter().any(|s| &s.path == f))) {
        let (branch, owner) = ours(&l.cwd);
        if owner == Some(id) {
            out.push(ProjectSession { key: l.key.clone(), harness: l.harness.clone(), archived: false, running: true, worktree: !branch.is_empty() });
        }
    }
    out
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
        // Archived: hidden, even open, unless it's working or waiting on
        // you (then it goes once it's done).
        let active = live.is_some_and(|l| l.needs_input || matches!(l.status, SessionStatus::Busy | SessionStatus::Starting));
        if meta.archived && !input.show_archived && !active {
            continue;
        }
        let (key, status) = match live {
            Some(l) => (l.key.clone(), live_status(l)),
            None => (s.path.to_string_lossy().into_owned(), String::new()),
        };
        let title = live.filter(|l| !l.name.is_empty()).map(|l| l.name.clone()).unwrap_or_else(|| s.display_title().to_owned());
        // Settled: on the shelf until there's newer work in it (or it's
        // working, waiting on you, or pinned).
        let settled = if !meta.settled_why.is_empty() && !meta.pinned && !active && unix(s.modified) <= meta.settled_at { meta.settled_why.clone() } else { String::new() };
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
            settled,
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
            settled: String::new(),
        });
    }

    if !filter.is_empty() {
        entries.retain(|e| e.title.to_lowercase().contains(&filter));
    }
    entries.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.modified.cmp(&a.modified)));

    // Assign each session to a worktree's project, else to the deepest
    // project containing its cwd.
    for e in entries.iter_mut() {
        (e.branch, e.owner) = owner_of(&e.cwd, input.projects, input.worktrees);
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
    // Each project's rows and when it was last worked in: the project with
    // the latest session goes first (those without any last, as added);
    // "Other sessions" after them all.
    let mut groups: Vec<(i64, Vec<SidebarRow>)> = Vec::new();
    let latest = |list: &[&Entry]| list.iter().map(|e| e.modified).max().unwrap_or(0);
    for p in input.projects {
        let (shelf, sessions): (Vec<&Entry>, Vec<&Entry>) = by_project.remove(&p.id).unwrap_or_default().into_iter().partition(|e| !e.settled.is_empty());
        if filtering && sessions.is_empty() && shelf.is_empty() {
            continue;
        }
        let mut rows = Vec::new();
        let worked = latest(&sessions).max(latest(&shelf));
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
            settled: String::new(),
            helper_of: String::new(),
            nest: 0,
        });
        if !collapsed {
            push_sessions(&mut rows, &sessions, p.id, false, filtering || input.expanded.contains(&p.id));
            if !shelf.is_empty() {
                let open = filtering || input.shelves_open.contains(&p.id);
                rows.push(SidebarRow {
                    kind: SidebarKind::Shelf,
                    key: format!("shelf:{}", p.id),
                    project_id: p.id,
                    title: "Settled".into(),
                    subtitle: String::new(),
                    harness: String::new(),
                    status: String::new(),
                    path: String::new(),
                    modified: shelf[0].modified,
                    collapsed: !open,
                    pinned: false,
                    count: shelf.len(),
                    branch: String::new(),
                    cwd: PathBuf::new(),
                    changes: 0,
                    icon: String::new(),
                    color: String::new(),
                    settled: String::new(),
                    helper_of: String::new(),
                    nest: 0,
                });
                if open {
                    push_sessions(&mut rows, &shelf, p.id, false, true);
                }
            }
        }
        groups.push((worked, rows));
    }
    groups.sort_by_key(|(worked, _)| std::cmp::Reverse(*worked));
    if !other.is_empty() {
        let mut rows = Vec::new();
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
            settled: String::new(),
            helper_of: String::new(),
            nest: 0,
        });
        if !collapsed {
            push_sessions(&mut rows, &other, -1, true, filtering || input.expanded.contains(&-1));
        }
        groups.push((0, rows));
    }
    groups.into_iter().flat_map(|(_, rows)| rows).collect()
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
            settled: e.settled.clone(),
            helper_of: String::new(),
            nest: 0,
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
            settled: String::new(),
            helper_of: String::new(),
            nest: 0,
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

/// Puts each session another one started (`parent_of`: key → its
/// parent's key) right under its parent, when they're in the same part of
/// the list (one project's sessions, or its Settled shelf), marked with the
/// parent's title and indented a step deeper. The rest keep their order.
pub fn nest_helpers(rows: &mut Vec<SidebarRow>, parent_of: &HashMap<String, String>) {
    // Each row's part of the list: what non-session rows came before it.
    let mut part = 0usize;
    let parts: Vec<usize> = rows
        .iter()
        .map(|r| {
            if r.kind != SidebarKind::Session {
                part += 1;
            }
            part
        })
        .collect();
    let at: HashMap<&str, usize> = rows.iter().enumerate().filter(|(_, r)| r.kind == SidebarKind::Session).map(|(i, r)| (r.key.as_str(), i)).collect();
    // Who is shown under whom (in their order in the list).
    let mut under: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut nested = vec![false; rows.len()];
    for (i, r) in rows.iter().enumerate().filter(|(_, r)| r.kind == SidebarKind::Session) {
        if let Some(&p) = parent_of.get(&r.key).and_then(|p| at.get(p.as_str()))
            && p != i
            && parts[p] == parts[i]
        {
            under.entry(p).or_default().push(i);
            nested[i] = true;
        }
    }
    if under.is_empty() {
        return;
    }
    fn place(i: usize, depth: u32, rows: &[SidebarRow], under: &HashMap<usize, Vec<usize>>, done: &mut [bool], order: &mut Vec<(usize, u32)>) {
        if std::mem::replace(&mut done[i], true) {
            return;
        }
        order.push((i, depth));
        for &c in under.get(&i).into_iter().flatten() {
            place(c, depth + 1, rows, under, done, order);
        }
    }
    let mut done = vec![false; rows.len()];
    let mut order = Vec::with_capacity(rows.len());
    for i in 0..rows.len() {
        if !nested[i] {
            place(i, 0, rows, &under, &mut done, &mut order);
        }
    }
    // (A loop of parents: left where they were.)
    for i in 0..rows.len() {
        if !done[i] {
            place(i, 0, rows, &under, &mut done, &mut order);
        }
    }
    let titles: HashMap<usize, String> = under.keys().map(|&p| (p, rows[p].title.clone())).collect();
    let parent_at: HashMap<usize, usize> = under.iter().flat_map(|(&p, cs)| cs.iter().map(move |&c| (c, p))).collect();
    let mut old: Vec<Option<SidebarRow>> = std::mem::take(rows).into_iter().map(Some).collect();
    for (i, depth) in order {
        let mut row = old[i].take().expect("each row placed once");
        row.nest = depth;
        row.helper_of = parent_at.get(&i).and_then(|p| titles.get(p)).cloned().unwrap_or_default();
        rows.push(row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harlo_harness::HarnessKind;
    use std::time::Duration;

    fn row(kind: SidebarKind, key: &str) -> SidebarRow {
        SidebarRow {
            kind, key: key.into(), project_id: 1, title: format!("T {key}"), subtitle: String::new(), harness: String::new(), status: String::new(),
            path: String::new(), modified: 0, collapsed: false, pinned: false, count: 0, branch: String::new(), cwd: PathBuf::new(), changes: 0,
            icon: String::new(), color: String::new(), settled: String::new(), helper_of: String::new(), nest: 0,
        }
    }

    #[test]
    fn helpers_show_under_their_parent() {
        use SidebarKind::*;
        let mut rows = vec![row(Project, "p"), row(Session, "child"), row(Session, "other"), row(Session, "lead"), row(Session, "grandchild"), row(Shelf, "s"), row(Session, "settled-child")];
        let parents: HashMap<String, String> = [("child", "lead"), ("grandchild", "child"), ("settled-child", "lead"), ("orphan", "gone")].map(|(a, b)| (a.to_owned(), b.to_owned())).into();
        nest_helpers(&mut rows, &parents);
        let shown: Vec<(&str, u32, &str)> = rows.iter().map(|r| (r.key.as_str(), r.nest, r.helper_of.as_str())).collect();
        assert_eq!(shown, [
            ("p", 0, ""), ("other", 0, ""), ("lead", 0, ""), ("child", 1, "T lead"), ("grandchild", 2, "T child"),
            // On the shelf, its parent isn't: it stays.
            ("s", 0, ""), ("settled-child", 0, ""),
        ]);
    }

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
    fn finds_all_a_projects_sessions() {
        let projects = [project(1, "/w"), project(2, "/w/app")];
        let worktrees = [WorktreeRecord { path: PathBuf::from("/data/trees/w/feat"), project_id: 1, branch: "feat".into(), base: "main".into(), removed: false }];
        let sessions = [
            session("/s/plain", "/w/src", "plain", 10),
            session("/s/archived", "/w", "archived", 20),
            session("/s/running", "/w", "running", 30),
            session("/s/tree", "/data/trees/w/feat", "in a worktree", 40),
            session("/s/nested", "/w/app", "the nested project's", 50),
            session("/s/stray", "/elsewhere", "no project's", 60),
        ];
        let meta = HashMap::from([(PathBuf::from("/s/archived"), SessionMeta { archived: true, ..Default::default() })]);
        let live = |key: &str, cwd: &str, file: Option<&str>| LiveInfo {
            key: key.into(),
            harness: "pi".into(),
            cwd: PathBuf::from(cwd),
            session_file: file.map(PathBuf::from),
            name: String::new(),
            first_message: String::new(),
            status: SessionStatus::Idle,
            needs_input: false,
            unread: false,
            started_at: SystemTime::now(),
        };
        let live = [live("live:1", "/w", Some("/s/running")), live("live:2", "/w", None), live("live:3", "/elsewhere", None)];
        let input = SidebarInput { projects: &projects, worktrees: &worktrees, sessions: &sessions, live: &live, meta: &meta, filter: "hidden by a filter", show_archived: false, other_collapsed: false, expanded: &[], shelves_open: &[] };
        let got = project_sessions(1, &input);
        let keys: Vec<&str> = got.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["/s/plain", "/s/archived", "live:1", "/s/tree", "live:2"]);
        assert!(got[1].archived && !got[0].archived);
        assert!(got[2].running && got[4].running && !got[0].running);
        assert!(got[3].worktree && !got[0].worktree);
        // The nested project's are its own; removing it leaves the others.
        assert_eq!(project_sessions(2, &input).iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["/s/nested"]);
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
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[], shelves_open: &[] });
        let shape: Vec<(SidebarKind, &str)> = rows.iter().map(|r| (r.kind, r.title.as_str())).collect();
        // The project worked in last first ("in w" is newer than "in app");
        // "Other sessions" after the projects, however new.
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
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &live, meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[], shelves_open: &[] });
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
        let rows = build(&SidebarInput { projects: &projects, worktrees: &worktrees, sessions: &sessions, live: &[], meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[], shelves_open: &[] });
        let shape: Vec<(&str, &str)> = rows.iter().map(|r| (r.title.as_str(), r.branch.as_str())).collect();
        assert_eq!(shape, [("w", ""), ("in worktree", "feat"), ("in main", "")]);
    }

    #[test]
    fn settled_sessions_go_on_their_projects_shelf() {
        let projects = [project(1, "/w")];
        let sessions = [session("/s/a", "/w", "done", 100), session("/s/b", "/w", "current", 50), session("/s/c", "/w", "worked in again", 10)];
        let now = unix(SystemTime::now());
        let mut meta = HashMap::new();
        meta.insert(PathBuf::from("/s/a"), SessionMeta { settled_why: "merged".into(), settled_at: now - 100, ..Default::default() });
        meta.insert(PathBuf::from("/s/c"), SessionMeta { settled_why: "quiet".into(), settled_at: now - 1000, ..Default::default() });
        let rows = |open: &[i64], filter: &str| {
            build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter, show_archived: false, other_collapsed: false, expanded: &[], shelves_open: open })
                .into_iter()
                .map(|r| (r.kind, r.title, r.count, r.settled))
                .collect::<Vec<_>>()
        };
        let row = |k, t: &str, c, s: &str| (k, t.to_owned(), c, s.to_owned());
        assert_eq!(rows(&[], ""), [
            row(SidebarKind::Project, "w", 2, ""),
            row(SidebarKind::Session, "worked in again", 0, ""),
            row(SidebarKind::Session, "current", 0, ""),
            row(SidebarKind::Shelf, "Settled", 1, ""),
        ]);
        assert_eq!(rows(&[1], "")[4], row(SidebarKind::Session, "done", 0, "merged"));
        // Searching finds them on the shelf.
        assert_eq!(rows(&[], "don"), [row(SidebarKind::Project, "w", 0, ""), row(SidebarKind::Shelf, "Settled", 1, ""), row(SidebarKind::Session, "done", 0, "merged")]);
    }

    #[test]
    fn archived_and_filtered_sessions_are_hidden() {
        let projects = [project(1, "/w")];
        let sessions = [session("/s/a", "/w", "alpha", 1), session("/s/b", "/w", "beta", 2)];
        let mut meta = HashMap::new();
        meta.insert(PathBuf::from("/s/a"), SessionMeta { archived: true, ..Default::default() });
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[], shelves_open: &[] });
        assert_eq!(rows.iter().filter(|r| r.kind == SidebarKind::Session).count(), 1);
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter: "zzz", show_archived: true, other_collapsed: false, expanded: &[], shelves_open: &[] });
        assert!(rows.is_empty());
    }

    #[test]
    fn archived_open_sessions_stay_only_while_they_need_you() {
        let projects = [project(1, "/w")];
        let sessions = [session("/s/a", "/w", "alpha", 1)];
        let mut meta = HashMap::new();
        meta.insert(PathBuf::from("/s/a"), SessionMeta { archived: true, ..Default::default() });
        let shown = |status, needs_input| {
            let live = [LiveInfo {
                key: "live:1".into(),
                harness: "pi".into(),
                cwd: PathBuf::from("/w"),
                session_file: Some(PathBuf::from("/s/a")),
                name: String::new(),
                first_message: String::new(),
                status,
                needs_input,
                unread: false,
                started_at: SystemTime::now(),
            }];
            build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &live, meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[], shelves_open: &[] })
                .iter()
                .any(|r| r.kind == SidebarKind::Session)
        };
        assert!(!shown(SessionStatus::Idle, false), "open but done: archived");
        assert!(shown(SessionStatus::Busy, false), "still working");
        assert!(shown(SessionStatus::Idle, true), "waiting on you");
    }

    #[test]
    fn long_lists_get_a_more_row() {
        let projects = [project(1, "/w")];
        let sessions: Vec<_> = (0..20).map(|i| session(&format!("/s/{i}"), "/w", "x", i)).collect();
        let meta = HashMap::new();
        let rows = build(&SidebarInput { projects: &projects, worktrees: &[], sessions: &sessions, live: &[], meta: &meta, filter: "", show_archived: false, other_collapsed: false, expanded: &[], shelves_open: &[] });
        assert_eq!(rows.len(), 1 + SESSIONS_PER_PROJECT + 1);
        assert_eq!(rows.last().unwrap().kind, SidebarKind::More);
    }
}
