//! The Context panel: how full the open session's context window is, what
//! the session has used and cost, and the instructions its agent was given.
//!
//! The totals follow the session's usage events. The window's fill, the
//! model and agent, and instructions loaded on the way come from the server's
//! view of the session's context, loaded when the panel is shown for a
//! session and again after each step, compaction or change of model. The
//! instruction files given from the start are worked out here, for folders
//! on this computer (see [`harlo_core::instructions`]).

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use crate::task::Task;
use harlo_core::instructions::{self, Places};
use harlo_core::server::{SessionContext, Tokens};
use harlo_core::workspace::EventKind;

use super::{App, Message, Panel};

/// What a load found.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// What the model sees now.
    pub context: Result<SessionContext, String>,
    /// The instruction files given from the start; `None` when the folder
    /// isn't on this computer.
    pub files: Option<Vec<instructions::File>>,
    /// This computer's home folder, to show paths under it with `~`.
    pub home: Option<PathBuf>,
    /// The project's root, to show paths inside it from there.
    pub root: Option<PathBuf>,
}

/// Something the panel reported, or a load that finished.
#[derive(Debug, Clone)]
pub enum Event {
    /// Load again.
    Refresh,
    /// A load finished: which one, and what it found.
    Loaded(u64, Box<Loaded>),
}

/// The Context panel's state.
#[derive(Debug, Default)]
pub struct ContextView {
    /// The session shown.
    shown: Option<String>,
    /// Whether the panel was on screen at the last update.
    visible: bool,
    /// Something changed since the last load.
    stale: bool,
    /// The load on its way, by number.
    loading: Option<u64>,
    loads: u64,
    /// What the last load found.
    pub loaded: Option<Loaded>,
}

/// What the Context panel shows for the open session.
#[derive(Debug, Clone)]
pub struct Facts<'a> {
    /// The session's cost so far, in US dollars.
    pub cost: f64,
    /// Every token the session has used so far.
    pub tokens: Tokens,
    /// What the model sees now, once loaded.
    pub context: Option<&'a SessionContext>,
    /// Why loading it failed.
    pub error: Option<&'a str>,
    /// The model's name for people.
    pub model: Option<String>,
    /// How many tokens the model's context window holds, if known.
    pub limit: Option<u64>,
    /// The agent the session runs.
    pub agent: Option<String>,
    /// The instruction files given from the start; `None` until loaded, or
    /// when the folder isn't on this computer.
    pub files: Option<&'a [instructions::File]>,
    /// Whether the first load is still on its way.
    pub loading: bool,
    /// The home folder, to show paths under it with `~`; empty when unknown.
    pub home: &'a Path,
    /// The project's root, to show paths inside it from there; empty when unknown.
    pub root: &'a Path,
}

impl App {
    /// Returns what the Context panel shows, when a session is open.
    #[must_use]
    pub fn context_facts(&self) -> Option<Facts<'_>> {
        let session = self.workspace.session(self.active.as_deref()?)?;
        let loaded = self.context.loaded.as_ref();
        let context = loaded.and_then(|l| l.context.as_ref().ok());
        // The model and agent of the latest step, or the session's choice.
        let model = context
            .and_then(|c| c.model.clone())
            .or_else(|| self.current_model());
        let catalog = self.catalogs.get(&session.directory);
        let known = model
            .as_ref()
            .and_then(|m| catalog?.model(&m.provider_id, &m.id));
        let agent = context
            .and_then(|c| c.agent.clone())
            .or_else(|| self.current_agent());
        // The agent by name, when the folder's catalog knows it.
        let agent = agent.map(|id| {
            catalog
                .and_then(|c| c.agents.iter().find(|a| a.id == id))
                .map_or(id, |a| a.name.clone())
        });
        Some(Facts {
            cost: session.cost,
            tokens: session.tokens,
            context,
            error: loaded.and_then(|l| l.context.as_ref().err().map(String::as_str)),
            model: known
                .map(|m| m.name.clone())
                .or_else(|| model.map(|m| m.id)),
            limit: known.and_then(|m| m.context),
            agent,
            files: loaded.and_then(|l| l.files.as_deref()),
            loading: loaded.is_none(),
            home: loaded
                .and_then(|l| l.home.as_deref())
                .unwrap_or(Path::new("")),
            root: loaded
                .and_then(|l| l.root.as_deref())
                .unwrap_or(Path::new("")),
        })
    }

    /// Applies something the Context panel reported.
    pub(super) fn context_action(&mut self, event: Event) {
        let view = &mut self.context;
        match event {
            Event::Refresh => view.stale = true,
            Event::Loaded(load, loaded) => {
                if view.loading == Some(load) {
                    view.loading = None;
                    view.loaded = Some(*loaded);
                }
            }
        }
    }

    /// Marks the panel stale when `event` changed what the shown session's model sees.
    pub(super) fn context_event(&mut self, event: &EventKind) {
        let changes = matches!(
            event,
            EventKind::StepEnded { .. }
                | EventKind::Done { .. }
                | EventKind::CompactionEnded { .. }
                | EventKind::ModelSelected { .. }
                | EventKind::AgentSelected { .. }
                | EventKind::RevertCommitted { .. }
        );
        if changes
            && event.session_id().is_some()
            && event.session_id() == self.context.shown.as_deref()
        {
            self.context.stale = true;
        }
    }

    /// Loads the panel when it's on screen and out of date.
    pub(super) fn follow_context(&mut self) -> Task<Message> {
        let visible = self.panel_shown(Panel::Context);
        let opened = visible && !self.context.visible;
        self.context.visible = visible;
        if !visible {
            return Task::none();
        }
        let Some(session) = self.active.clone() else {
            self.context = ContextView {
                visible,
                ..ContextView::default()
            };
            return Task::none();
        };
        if self.context.shown.as_deref() != Some(session.as_str()) {
            self.context = ContextView {
                shown: Some(session.clone()),
                visible,
                stale: true,
                ..ContextView::default()
            };
        }
        if opened {
            self.context.stale = true;
        }
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if !self.context.stale || self.context.loading.is_some() {
            return Task::none();
        }
        self.context.stale = false;
        self.context.loads += 1;
        let load = self.context.loads;
        self.context.loading = Some(load);
        let places = self.instruction_places(&session);
        Task::perform(
            async move {
                let context = client.context(&session).await.map_err(|e| e.to_string());
                // A few file checks; quick enough not to need a thread of their own.
                let root = places.as_ref().map(|places| places.project.clone());
                let files = places.map(|places| instructions::files(&places));
                Loaded {
                    context,
                    files,
                    home: instructions::home(),
                    root,
                }
            },
            move |loaded| Message::Context(Event::Loaded(load, Box::new(loaded))),
        )
    }

    /// Where to look for `session`'s instruction files, when its folder is on this computer.
    fn instruction_places(&self, session: &str) -> Option<Places> {
        let folder = &self.workspace.session(session)?.directory;
        if !self.location.is_local() || !Path::new(folder).is_dir() {
            return None;
        }
        // The project's root, or the worktree's when the session runs in one.
        let root = self.projects.iter().find_map(|project| {
            if project.sessions.iter().any(|s| s.id == session) {
                return Some(project.directory.as_str());
            }
            project
                .worktrees
                .iter()
                .find(|w| w.sessions.iter().any(|s| s.id == session))
                .map(|w| w.directory.as_str())
        });
        Places::here(folder, root.unwrap_or(folder))
    }
}
