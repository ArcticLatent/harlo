//! Subagents: the child sessions a session hands work to, how they stand,
//! and the way between a child and its parent.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Rgba;
use harlo_core::workspace::Session;

use super::App;
use crate::model::{ChildState, Crew, CrewAgent};

/// A child session as the parent's view lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Child {
    /// The child session's ID.
    pub id: String,
    /// The agent's name for people, such as `Explore`.
    pub agent: String,
    /// The agent's colour, when it has one.
    pub color: Option<Rgba>,
    /// The task's short label.
    pub task: String,
    /// How it stands now.
    pub state: ChildState,
}

impl App {
    /// How `session`'s subagents stand, and the agents' names and colours,
    /// for the cards in its timeline.
    #[must_use]
    pub fn crew(&self, session: &str) -> Crew {
        let mut crew = Crew::default();
        for child in self.workspace.children(session) {
            crew.children
                .insert(child.id.clone(), self.child_state(&child.id));
        }
        if let Some(catalog) = self
            .workspace
            .session(session)
            .and_then(|s| self.catalogs.get(&s.directory))
        {
            for agent in &catalog.agents {
                crew.agents.insert(
                    agent.id.clone(),
                    CrewAgent {
                        name: agent.name.clone(),
                        color: agent.color.as_deref().and_then(Rgba::parse),
                    },
                );
            }
        }
        crew
    }

    /// The session that handed `session` its work, when it's a subagent's.
    #[must_use]
    pub fn parent_of(&self, session: &str) -> Option<&Session> {
        let parent = self.workspace.session(session)?.parent_id.as_deref()?;
        self.workspace.session(parent)
    }

    /// `session`'s subagents that are working or waiting for the user,
    /// in the order they started.
    #[must_use]
    pub fn active_children(&self, session: &str) -> Vec<Child> {
        let crew = self.crew(session);
        self.workspace
            .children(session)
            .into_iter()
            .filter_map(|child| {
                let state = self.child_state(&child.id);
                if state == ChildState::Idle {
                    return None;
                }
                let id = child.agent.clone().unwrap_or_default();
                let known = crew.agents.get(&id);
                Some(Child {
                    id: child.id.clone(),
                    agent: known.map_or(id, |a| a.name.clone()),
                    color: known.and_then(|a| a.color),
                    task: child.title.clone().unwrap_or_default(),
                    state,
                })
            })
            .collect()
    }

    /// How a child session stands: asking the user comes before working.
    fn child_state(&self, id: &str) -> ChildState {
        if self.workspace.needs_user(id) {
            ChildState::Waiting
        } else if self.workspace.is_busy(id) {
            ChildState::Working
        } else {
            ChildState::Idle
        }
    }
}
