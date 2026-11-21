//! What Harlo knows of one OpenCode server: projects, worktrees and sessions.
//!
//! A [`Workspace`] is rebuilt from a [`Snapshot`] on every (re)connection and
//! then kept current by live events. Events that only name a session or
//! project ask for a follow-up [`Fetch`]; the caller runs it and hands the
//! result back with [`Workspace::fetched`]. Applying an event or a fetch twice
//! is harmless, so events buffered during a resync can simply be replayed.

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[doc(inline)]
pub use opencode_client::{
    Event, EventKind, Message, ModelRef, Part, Project as ServerProject, Session, ToolCall,
    ToolStatus,
};

use crate::sessions::{Project, SessionSummary, Worktree, folder_name};

/// Everything the sidebar needs, loaded in one go after connecting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// Every project the server knows.
    pub projects: Vec<ServerProject>,
    /// Each git project's folders (main checkout and worktrees), by project ID.
    pub worktrees: HashMap<String, Vec<String>>,
    /// Every top-level session that isn't archived.
    pub sessions: Vec<Session>,
    /// IDs of sessions working right now.
    pub active: Vec<String>,
}

/// Something the workspace needs loaded to follow an event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Fetch {
    /// One session, by ID.
    Session(String),
    /// The project list.
    Projects,
    /// One project's worktrees, by project ID.
    Worktrees(String),
}

/// The result of a [`Fetch`].
#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    /// The session, or `None` when it no longer exists.
    Session(String, Option<Box<Session>>),
    /// The project list.
    Projects(Vec<ServerProject>),
    /// A project's folders, by project ID.
    Worktrees(String, Vec<String>),
}

/// Projects, worktrees and sessions on one server, in the user's sidebar order.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    projects: HashMap<String, ServerProject>,
    worktrees: HashMap<String, Vec<String>>,
    sessions: HashMap<String, Session>,
    busy: HashSet<String>,
    /// Project folders in sidebar order; see [`Workspace::order`].
    order: Vec<String>,
}

impl Workspace {
    /// Creates an empty workspace whose sidebar lists `order` first.
    #[must_use]
    pub fn new(order: Vec<String>) -> Self {
        Self {
            order,
            ..Self::default()
        }
    }

    /// Replaces everything with a fresh snapshot, keeping the sidebar order.
    ///
    /// Projects with sessions that aren't in the order yet are added to it.
    pub fn resync(&mut self, snapshot: Snapshot) {
        self.projects = snapshot
            .projects
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();
        self.worktrees = snapshot.worktrees;
        self.sessions = snapshot
            .sessions
            .into_iter()
            .filter(|s| s.parent_id.is_none())
            .map(|s| (s.id.clone(), s))
            .collect();
        self.busy = snapshot.active.into_iter().collect();
        self.adopt_projects();
    }

    /// Applies a live event and returns what must be loaded to complete it.
    pub fn apply(&mut self, kind: &EventKind, time: f64) -> Vec<Fetch> {
        match kind {
            EventKind::SessionCreated { session_id }
            | EventKind::SessionMoved { session_id }
            | EventKind::SessionUnarchived { session_id } => {
                vec![Fetch::Session(session_id.clone())]
            }
            EventKind::SessionRenamed { session_id, title } => {
                if let Some(s) = self.sessions.get_mut(session_id) {
                    s.title = Some(title.clone());
                }
                Vec::new()
            }
            EventKind::SessionDeleted { session_id }
            | EventKind::SessionArchived { session_id } => {
                self.sessions.remove(session_id);
                self.busy.remove(session_id);
                Vec::new()
            }
            EventKind::ModelSelected { session_id, model } => {
                if let Some(s) = self.sessions.get_mut(session_id) {
                    s.model = Some(model.clone());
                }
                Vec::new()
            }
            EventKind::Busy { session_id } => {
                self.busy.insert(session_id.clone());
                self.touch(session_id, time);
                Vec::new()
            }
            EventKind::Done { session_id, .. } => {
                self.busy.remove(session_id);
                self.touch(session_id, time);
                Vec::new()
            }
            EventKind::ProjectUpdated { .. } => vec![Fetch::Projects],
            EventKind::WorktreesChanged { project_id } => {
                vec![Fetch::Worktrees(project_id.clone())]
            }
            _ => Vec::new(),
        }
    }

    /// Applies the result of a [`Fetch`] and returns any further loads needed.
    pub fn fetched(&mut self, result: Fetched) -> Vec<Fetch> {
        match result {
            Fetched::Session(id, None) => {
                self.sessions.remove(&id);
                self.busy.remove(&id);
                Vec::new()
            }
            Fetched::Session(_, Some(session)) => self.upsert(*session),
            Fetched::Projects(projects) => {
                let fetches = projects
                    .iter()
                    .filter(|p| p.git && !self.worktrees.contains_key(&p.id))
                    .map(|p| Fetch::Worktrees(p.id.clone()))
                    .collect();
                self.projects = projects.into_iter().map(|p| (p.id.clone(), p)).collect();
                self.adopt_projects();
                fetches
            }
            Fetched::Worktrees(project_id, folders) => {
                self.worktrees.insert(project_id, folders);
                Vec::new()
            }
        }
    }

    /// Adds or updates a session; subagent sessions are ignored.
    pub fn upsert(&mut self, session: Session) -> Vec<Fetch> {
        if session.parent_id.is_some() {
            return Vec::new();
        }
        let unknown_project = !self.projects.contains_key(&session.project_id);
        self.sessions.insert(session.id.clone(), session);
        if unknown_project {
            return vec![Fetch::Projects];
        }
        self.adopt_projects();
        Vec::new()
    }

    /// Adds a project folder to the end of the sidebar, if it isn't listed.
    pub fn add_project(&mut self, directory: String) {
        if !self.order.contains(&directory) {
            self.order.push(directory);
        }
    }

    /// Removes a project folder from the sidebar.
    ///
    /// It comes back when one of its sessions next changes.
    pub fn remove_project(&mut self, directory: &str) {
        self.order.retain(|d| d != directory);
    }

    /// Returns the project folders in sidebar order, for saving.
    #[must_use]
    pub fn order(&self) -> &[String] {
        &self.order
    }

    /// Returns a session by ID.
    #[must_use]
    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    /// Returns whether a session is working right now.
    #[must_use]
    pub fn is_busy(&self, id: &str) -> bool {
        self.busy.contains(id)
    }

    /// Builds the sidebar: projects in order, each with its sessions and worktrees.
    ///
    /// Ages are measured from `now`. Sessions are listed newest first.
    #[must_use]
    pub fn sidebar(&self, now: SystemTime) -> Vec<Project> {
        let now_ms = now
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
        let by_directory: HashMap<&str, &ServerProject> = self
            .projects
            .values()
            .map(|p| (p.directory.as_str(), p))
            .collect();

        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_by(|a, b| b.updated.total_cmp(&a.updated).then(a.id.cmp(&b.id)));

        self.order
            .iter()
            .map(|directory| {
                let server = by_directory.get(directory.as_str()).copied();
                let mut project = Project {
                    directory: directory.clone(),
                    name: server
                        .and_then(|p| p.name.clone())
                        .unwrap_or_else(|| folder_name(directory).to_owned()),
                    sessions: Vec::new(),
                    worktrees: Vec::new(),
                };
                let Some(server) = server else {
                    return project;
                };
                let folders = self.worktrees.get(&server.id);
                for folder in folders.into_iter().flatten() {
                    if folder != directory {
                        project.worktrees.push(Worktree {
                            directory: folder.clone(),
                            name: folder_name(folder).to_owned(),
                            sessions: Vec::new(),
                        });
                    }
                }
                for session in sessions.iter().filter(|s| s.project_id == server.id) {
                    let row = self.summary(session, now_ms);
                    let dir = session.directory.as_str();
                    // Worktrees first: one may sit inside the main folder.
                    let worktree = project
                        .worktrees
                        .iter()
                        .position(|w| dir == w.directory || inside(dir, &w.directory));
                    let index = match worktree {
                        Some(index) => index,
                        None if dir == directory || inside(dir, directory) => {
                            project.sessions.push(row);
                            continue;
                        }
                        None => {
                            // A worktree the server no longer lists: still show its sessions.
                            project.worktrees.push(Worktree {
                                directory: dir.to_owned(),
                                name: folder_name(dir).to_owned(),
                                sessions: Vec::new(),
                            });
                            project.worktrees.len() - 1
                        }
                    };
                    project.worktrees[index].sessions.push(row);
                }
                project
            })
            .collect()
    }

    fn summary(&self, session: &Session, now_ms: f64) -> SessionSummary {
        let age_ms = (now_ms - session.updated).max(0.0);
        SessionSummary {
            id: session.id.clone(),
            title: session.title.clone().unwrap_or_default(),
            maker: session.model.as_ref().map(|m| m.provider_id.clone()),
            age: Duration::from_secs_f64(age_ms / 1000.0),
            busy: self.busy.contains(&session.id),
        }
    }

    fn touch(&mut self, session_id: &str, time: f64) {
        if let Some(s) = self.sessions.get_mut(session_id) {
            s.updated = s.updated.max(time);
        }
    }

    /// Appends projects that have sessions but aren't in the sidebar yet, newest first.
    fn adopt_projects(&mut self) {
        let mut newest: HashMap<&str, f64> = HashMap::new();
        for session in self.sessions.values() {
            let Some(project) = self.projects.get(&session.project_id) else {
                continue;
            };
            let entry = newest.entry(project.directory.as_str()).or_insert(0.0);
            *entry = entry.max(session.updated);
        }
        let mut missing: Vec<(&str, f64)> = newest
            .into_iter()
            .filter(|(dir, _)| !self.order.iter().any(|d| d == dir))
            .collect();
        missing.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
        let missing: Vec<String> = missing.into_iter().map(|(d, _)| d.to_owned()).collect();
        self.order.extend(missing);
    }
}

/// Whether `path` is inside the folder `parent`.
fn inside(path: &str, parent: &str) -> bool {
    path.strip_prefix(parent.trim_end_matches('/'))
        .is_some_and(|rest| rest.starts_with('/'))
}
