//! What Harlo knows of one server: projects, worktrees and sessions.
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
pub use harlo_client::{
    Event, EventKind, FileDiff, Form, History, InboxItem, Message, ModelRef, Part, Permission,
    Project as ServerProject, Report, Revert, Session, Tokens, ToolCall, ToolStatus,
};

use crate::sessions::{Project, SessionSummary, Worktree, folder_name};

/// The name of the sidebar's group of sessions started without a project.
pub const OTHER_SESSIONS: &str = "Other sessions";

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
    /// Requests waiting for approval in the working sessions.
    pub permissions: Vec<Permission>,
    /// Questions waiting for an answer in the working sessions.
    pub forms: Vec<Form>,
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
    /// A session's inbox: prompts waiting for it, by session ID.
    Inbox(String),
    /// A session's requests waiting for approval, by session ID.
    Permissions(String),
    /// A session's questions waiting for an answer, by session ID.
    Forms(String),
    /// The sessions a session handed work to, by its ID.
    Children(String),
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
    /// A session's inbox, by session ID.
    Inbox(String, Vec<InboxItem>),
    /// A session's requests waiting for approval, by session ID.
    Permissions(String, Vec<Permission>),
    /// A session's questions waiting for an answer, by session ID.
    Forms(String, Vec<Form>),
    /// The sessions a session handed work to, by its ID.
    Children(String, Vec<Session>),
}

/// Projects, worktrees and sessions on one server, in the user's sidebar order.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    projects: HashMap<String, ServerProject>,
    worktrees: HashMap<String, Vec<String>>,
    sessions: HashMap<String, Session>,
    busy: HashSet<String>,
    /// Inboxes of the sessions that asked for theirs, by session ID.
    inboxes: HashMap<String, Vec<InboxItem>>,
    /// Requests waiting for approval, by session ID.
    permissions: HashMap<String, Vec<Permission>>,
    /// Questions waiting for an answer, by session ID.
    forms: HashMap<String, Vec<Form>>,
    /// Project folders in sidebar order; see [`Workspace::order`].
    order: Vec<String>,
    /// Project folders removed from the sidebar, which stay away.
    hidden: Vec<String>,
    /// The folder sessions started without a project run in, usually the
    /// home folder: its sessions are listed apart, as Other sessions.
    other: Option<String>,
}

/// A server's time in whole milliseconds.
#[must_use]
pub fn millis(time: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "milliseconds since 1970 fit an i64 for any real date"
    )]
    let whole = time.round() as i64;
    whole
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
            .map(|s| (s.id.clone(), s))
            .collect();
        self.busy = snapshot.active.into_iter().collect();
        // Inboxes may have changed while away; whoever shows one asks again.
        self.inboxes.clear();
        // What waits for the user is replaced: requests answered elsewhere
        // go, and ones asked while away appear.
        self.permissions.clear();
        for asked in snapshot.permissions {
            add(
                &mut self.permissions,
                asked.session_id.clone(),
                asked,
                |p| &p.id,
            );
        }
        self.forms.clear();
        for asked in snapshot.forms {
            add(&mut self.forms, asked.session_id.clone(), asked, |f| &f.id);
        }
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
                    Vec::new()
                } else {
                    // Renamed before its creation's fetch came back, which
                    // may have been answered before the rename: fetch again.
                    vec![Fetch::Session(session_id.clone())]
                }
            }
            EventKind::SessionDeleted { session_id }
            | EventKind::SessionArchived { session_id } => {
                self.sessions.remove(session_id);
                self.busy.remove(session_id);
                self.inboxes.remove(session_id);
                Vec::new()
            }
            EventKind::AgentSelected { session_id, agent } => {
                if let Some(s) = self.sessions.get_mut(session_id) {
                    s.agent = Some(agent.clone());
                }
                Vec::new()
            }
            EventKind::InboxAdded { session_id, item } => {
                if let Some(inbox) = self.inboxes.get_mut(session_id) {
                    inbox.retain(|i| i.id != item.id);
                    inbox.push(item.clone());
                }
                Vec::new()
            }
            EventKind::InboxRemoved {
                session_id,
                item_id,
                ..
            } => {
                if let Some(inbox) = self.inboxes.get_mut(session_id) {
                    inbox.retain(|i| i.id != *item_id);
                }
                Vec::new()
            }
            EventKind::InboxDeliveryChanged {
                session_id,
                item_id,
                delivery,
            } => {
                if let Some(item) = self
                    .inboxes
                    .get_mut(session_id)
                    .and_then(|inbox| inbox.iter_mut().find(|i| i.id == *item_id))
                {
                    item.delivery = *delivery;
                }
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
            EventKind::PermissionAsked(_)
            | EventKind::PermissionReplied { .. }
            | EventKind::FormCreated(_)
            | EventKind::FormSettled { .. } => {
                self.apply_waiting(kind);
                Vec::new()
            }
            EventKind::SessionPermissions { .. }
            | EventKind::UsageUpdated { .. }
            | EventKind::RevertStaged { .. }
            | EventKind::RevertCleared { .. }
            | EventKind::RevertCommitted { .. } => {
                self.apply_session_state(kind);
                Vec::new()
            }
            EventKind::Done { session_id, .. } => {
                // A run that ended asks for nothing more.
                self.permissions.remove(session_id);
                self.forms.remove(session_id);
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

    /// Applies a change to a session's own rules, usage or staged undo.
    fn apply_session_state(&mut self, kind: &EventKind) {
        let Some(session) = kind.session_id().and_then(|id| self.sessions.get_mut(id)) else {
            return;
        };
        match kind {
            EventKind::SessionPermissions { rules, .. } => session.permissions.clone_from(rules),
            EventKind::UsageUpdated { cost, tokens, .. } => {
                session.cost = *cost;
                session.tokens = *tokens;
            }
            EventKind::RevertStaged { revert, .. } => session.revert = Some(revert.clone()),
            EventKind::RevertCleared { .. } | EventKind::RevertCommitted { .. } => {
                session.revert = None;
            }
            _ => {}
        }
    }

    /// Applies an event about what waits for the user: approvals and questions.
    fn apply_waiting(&mut self, kind: &EventKind) {
        match kind {
            EventKind::PermissionAsked(asked) => add(
                &mut self.permissions,
                asked.session_id.clone(),
                Permission::clone(asked),
                |p| &p.id,
            ),
            EventKind::PermissionReplied {
                session_id,
                request_id,
            } => self.settle_permission(session_id, request_id),
            EventKind::FormCreated(asked) => add(
                &mut self.forms,
                asked.session_id.clone(),
                asked.clone(),
                |f| &f.id,
            ),
            EventKind::FormSettled {
                session_id,
                form_id,
            } => self.settle_form(session_id, form_id),
            _ => {}
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
            Fetched::Inbox(session_id, items) => {
                self.inboxes.insert(session_id, items);
                Vec::new()
            }
            Fetched::Permissions(session_id, waiting) => {
                self.permissions.insert(session_id, waiting);
                Vec::new()
            }
            Fetched::Forms(session_id, waiting) => {
                self.forms.insert(session_id, waiting);
                Vec::new()
            }
            Fetched::Children(_, children) => children
                .into_iter()
                .flat_map(|child| self.upsert(child))
                .collect(),
        }
    }

    /// Adds or updates a session, a subagent's child session included.
    ///
    /// An archived one leaves the lists instead: a session archived just
    /// after it was made can arrive from its fetch after the archive event.
    pub fn upsert(&mut self, session: Session) -> Vec<Fetch> {
        if session.archived.is_some() {
            self.sessions.remove(&session.id);
            self.busy.remove(&session.id);
            self.inboxes.remove(&session.id);
            return Vec::new();
        }
        // Two fetches of one session can come back out of order; the
        // older answer doesn't replace the newer.
        if self
            .sessions
            .get(&session.id)
            .is_some_and(|known| known.updated > session.updated)
        {
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

    /// Adds a project folder to the end of the sidebar, if it isn't listed,
    /// bringing back one that was hidden.
    pub fn add_project(&mut self, directory: String) {
        self.hidden.retain(|d| *d != directory);
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

    /// Removes a project folder from the sidebar for good: unlike
    /// [`Workspace::remove_project`], its sessions don't bring it back. Only
    /// adding it again does.
    pub fn hide_project(&mut self, directory: &str) {
        self.remove_project(directory);
        if !self.hidden.iter().any(|d| d == directory) {
            self.hidden.push(directory.to_owned());
        }
    }

    /// Sets the folder sessions started without a project run in: they're
    /// listed last as Other sessions, never as a project or hidden.
    pub fn set_other_folder(&mut self, folder: Option<String>) {
        if self.other == folder {
            return;
        }
        if let Some(folder) = &folder {
            self.order.retain(|d| d != folder);
        }
        self.other = folder;
        self.adopt_projects();
    }

    /// Sets the project folders kept out of the sidebar, as saved.
    pub fn set_hidden(&mut self, hidden: Vec<String>) {
        self.order.retain(|d| !hidden.contains(d));
        self.hidden = hidden;
    }

    /// Returns the project folders kept out of the sidebar, for saving.
    #[must_use]
    pub fn hidden(&self) -> &[String] {
        &self.hidden
    }

    /// Returns the server's project whose main folder is `directory`.
    #[must_use]
    pub fn project(&self, directory: &str) -> Option<&ServerProject> {
        self.projects.values().find(|p| p.directory == directory)
    }

    /// Returns the server's project with ID `id`.
    #[must_use]
    pub fn project_by_id(&self, id: &str) -> Option<&ServerProject> {
        self.projects.get(id)
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

    /// The sessions with an answer from after `since` (milliseconds since
    /// the Unix epoch): their last run ended later, by itself.
    ///
    /// A subagent's session isn't one (its run ends inside its parent's,
    /// which says so), nor is a session at work again or one the user
    /// stopped.
    pub fn answered_since(&self, since: i64) -> impl Iterator<Item = &str> {
        self.sessions.values().filter_map(move |session| {
            let told = matches!(session.outcome.as_deref(), Some("succeeded" | "failed"));
            let later = session.idle.is_some_and(|at| millis(at) > since);
            (told && later && session.parent_id.is_none() && !self.busy.contains(&session.id))
                .then_some(session.id.as_str())
        })
    }

    /// When the newest run of any session here ended, in milliseconds
    /// since the Unix epoch; 0 when none has run.
    #[must_use]
    pub fn last_ended(&self) -> i64 {
        self.sessions
            .values()
            .filter_map(|session| session.idle.map(millis))
            .max()
            .unwrap_or(0)
    }

    /// Returns a session's waiting prompts, oldest first.
    ///
    /// Empty until the inbox is loaded with [`Fetch::Inbox`]; from then on
    /// events keep it current.
    #[must_use]
    pub fn inbox(&self, id: &str) -> &[InboxItem] {
        self.inboxes.get(id).map_or(&[], Vec::as_slice)
    }

    /// Returns a session's requests waiting for approval, oldest first.
    ///
    /// Requests asked while Harlo is attached arrive as events; older ones
    /// are loaded with [`Fetch::Permissions`].
    #[must_use]
    pub fn permissions(&self, id: &str) -> &[Permission] {
        self.permissions.get(id).map_or(&[], Vec::as_slice)
    }

    /// Returns a session's questions waiting for an answer, oldest first.
    ///
    /// Questions asked while Harlo is attached arrive as events; older ones
    /// are loaded with [`Fetch::Forms`].
    #[must_use]
    pub fn forms(&self, id: &str) -> &[Form] {
        self.forms.get(id).map_or(&[], Vec::as_slice)
    }

    /// Returns whether a session waits for the user: an approval or an answer.
    #[must_use]
    pub fn needs_user(&self, id: &str) -> bool {
        !self.permissions(id).is_empty() || !self.forms(id).is_empty()
    }

    /// Whether session `id` waits only for answers to questions: some are
    /// open and no approval is.
    #[must_use]
    pub fn only_asks(&self, id: &str) -> bool {
        self.permissions(id).is_empty() && !self.forms(id).is_empty()
    }

    /// Forgets an approval request, answered here or elsewhere.
    pub fn settle_permission(&mut self, session_id: &str, request_id: &str) {
        if let Some(waiting) = self.permissions.get_mut(session_id) {
            waiting.retain(|p| p.id != request_id);
        }
    }

    /// Forgets a question, answered or dismissed here or elsewhere.
    pub fn settle_form(&mut self, session_id: &str, form_id: &str) {
        if let Some(waiting) = self.forms.get_mut(session_id) {
            waiting.retain(|f| f.id != form_id);
        }
    }

    /// Records a session's staged undo, or that none is staged.
    ///
    /// The server announces the same; this shows it without waiting for that.
    pub fn set_revert(&mut self, session_id: &str, revert: Option<Revert>) {
        if let Some(s) = self.sessions.get_mut(session_id) {
            s.revert = revert;
        }
    }

    /// Returns whether a session is working right now.
    #[must_use]
    pub fn is_busy(&self, id: &str) -> bool {
        self.busy.contains(id)
    }

    /// Returns the sessions `id` handed work to, in the order it started them.
    #[must_use]
    pub fn children(&self, id: &str) -> Vec<&Session> {
        let mut children: Vec<&Session> = self
            .sessions
            .values()
            .filter(|s| s.parent_id.as_deref() == Some(id))
            .collect();
        children.sort_by(|a, b| a.created.total_cmp(&b.created).then(a.id.cmp(&b.id)));
        children
    }

    /// Builds the sidebar: projects in order, each with its sessions and worktrees.
    ///
    /// Ages are measured from `now`. Top-level sessions are listed newest
    /// first, each with the sessions it handed work to beneath it.
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

        let mut sessions: Vec<&Session> = self
            .sessions
            .values()
            .filter(|s| s.parent_id.is_none())
            .collect();
        sessions.sort_by(|a, b| b.updated.total_cmp(&a.updated).then(a.id.cmp(&b.id)));

        let other = self.other.as_deref();
        let mut projects: Vec<Project> =
            self.order
                .iter()
                .filter(|directory| Some(directory.as_str()) != other)
                .map(|directory| {
                    let server = by_directory.get(directory.as_str()).copied();
                    let mut project = Project {
                        directory: directory.clone(),
                        name: server
                            .and_then(|p| p.name.clone())
                            .unwrap_or_else(|| folder_name(directory).to_owned()),
                        color: server.and_then(|p| p.color.clone()),
                        icon: server.and_then(|p| p.icon.clone()),
                        sessions: Vec::new(),
                        worktrees: Vec::new(),
                        other: false,
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
                    for session in sessions.iter().filter(|s| {
                        s.project_id == server.id && Some(s.directory.as_str()) != other
                    }) {
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
                .collect();
        if let Some(folder) = other {
            let loose: Vec<SessionSummary> = sessions
                .iter()
                .filter(|s| s.directory == folder)
                .map(|s| self.summary(s, now_ms))
                .collect();
            if !loose.is_empty() {
                projects.push(Project {
                    directory: folder.to_owned(),
                    name: OTHER_SESSIONS.to_owned(),
                    color: None,
                    icon: None,
                    sessions: loose,
                    worktrees: Vec::new(),
                    other: true,
                });
            }
        }
        projects
    }

    fn summary(&self, session: &Session, now_ms: f64) -> SessionSummary {
        let age_ms = (now_ms - session.updated).max(0.0);
        let children: Vec<SessionSummary> = self
            .children(&session.id)
            .into_iter()
            .map(|child| self.summary(child, now_ms))
            .collect();
        SessionSummary {
            id: session.id.clone(),
            title: session.title.clone().unwrap_or_default(),
            maker: session.model.as_ref().map(|m| m.provider_id.clone()),
            agent: session.agent.clone(),
            age: Duration::from_secs_f64(age_ms / 1000.0),
            busy: self.busy.contains(&session.id),
            // A subagent asking for the user needs them as much as its parent.
            waiting: self.needs_user(&session.id) || children.iter().any(|c| c.waiting),
            asking: self.only_asks(&session.id) && children.iter().all(|c| !c.waiting || c.asking),
            children,
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
        // Subagents' sessions sit under their parents and don't reorder projects.
        for session in self.sessions.values().filter(|s| s.parent_id.is_none()) {
            let Some(project) = self.projects.get(&session.project_id) else {
                continue;
            };
            let entry = newest.entry(project.directory.as_str()).or_insert(0.0);
            *entry = entry.max(session.updated);
        }
        let mut missing: Vec<(&str, f64)> = newest
            .into_iter()
            .filter(|(dir, _)| Some(*dir) != self.other.as_deref())
            .filter(|(dir, _)| !self.order.iter().chain(&self.hidden).any(|d| d == dir))
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

/// Adds `item` to `session`'s list in `map`, unless one with its ID is there.
fn add<T>(map: &mut HashMap<String, Vec<T>>, session: String, item: T, id: impl Fn(&T) -> &String) {
    let list = map.entry(session).or_default();
    if !list.iter().any(|other| id(other) == id(&item)) {
        list.push(item);
    }
}
