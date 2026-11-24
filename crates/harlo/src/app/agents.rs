//! Settings → Agents: The server's agents, and the approvals it saved when
//! someone chose "Allow always".

// Rust guideline compliant 2026-10-09

use harlo_core::server::SavedApproval;

use crate::task::Task;

use super::sheets::{Event as SheetEvent, Load};
use super::{App, Message};

/// Something that happened on the saved approvals.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalEvent {
    /// The server listed them, or couldn't.
    Loaded(Result<Vec<SavedApproval>, String>),
    /// Remove the approval at this place in the list.
    Remove(usize),
    /// Removing one ended.
    Removed(Result<(), String>),
}

/// The saved approvals Settings → Agents shows.
#[derive(Debug, Default)]
pub struct Approvals {
    /// Every saved approval, by project, then by what it allows.
    pub list: Load<Vec<SavedApproval>>,
    /// Why the last removal failed.
    pub error: Option<String>,
    /// Whether a removal is under way.
    pub busy: bool,
}

impl App {
    /// Handles `event` on the saved approvals.
    pub(super) fn approval_event(&mut self, event: ApprovalEvent) -> Task<Message> {
        let page = &mut self.sheets.approvals;
        match event {
            ApprovalEvent::Loaded(result) => {
                page.list = match result {
                    Ok(mut list) => {
                        list.sort_by(|a, b| {
                            (&a.project_id, &a.action, &a.resource).cmp(&(
                                &b.project_id,
                                &b.action,
                                &b.resource,
                            ))
                        });
                        Load::Loaded(list)
                    }
                    Err(why) => Load::Failed(why),
                };
            }
            ApprovalEvent::Remove(index) => {
                let id = match &page.list {
                    Load::Loaded(list) => list.get(index).map(|a| a.id.clone()),
                    Load::Idle | Load::Loading | Load::Failed(_) => None,
                };
                let (Some(id), Some(client)) = (id, self.client.clone()) else {
                    return Task::none();
                };
                page.busy = true;
                page.error = None;
                return Task::perform(
                    async move {
                        client
                            .remove_saved_approval(&id)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |result| approval_message(ApprovalEvent::Removed(result)),
                );
            }
            ApprovalEvent::Removed(result) => {
                page.busy = false;
                page.error = result.err();
                return self.load_approvals();
            }
        }
        Task::none()
    }

    /// Lists the saved approvals again.
    pub(super) fn load_approvals(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.sheets.approvals.list = Load::Idle;
            return Task::none();
        };
        if !matches!(self.sheets.approvals.list, Load::Loaded(_)) {
            self.sheets.approvals.list = Load::Loading;
        }
        Task::perform(
            async move { client.saved_approvals().await.map_err(|e| e.to_string()) },
            |result| approval_message(ApprovalEvent::Loaded(result)),
        )
    }

    /// The name of the project with server ID `id`, as the sidebar shows
    /// it, or its folder.
    #[must_use]
    pub fn project_name_by_id(&self, id: &str) -> Option<String> {
        let project = self.workspace.project_by_id(id)?;
        let shown = self
            .projects
            .iter()
            .find(|p| p.directory == project.directory)
            .map(|p| p.name.clone());
        Some(
            shown
                .or_else(|| project.name.clone())
                .unwrap_or_else(|| project.directory.clone()),
        )
    }
}

/// `event` as the app's message.
fn approval_message(event: ApprovalEvent) -> Message {
    Message::Sheet(SheetEvent::Approval(event))
}
