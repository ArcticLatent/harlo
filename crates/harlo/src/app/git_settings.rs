//! Settings → Git: git's name and email on commits (shared with the setup
//! guide), whether the server keeps the snapshots undoing a turn needs, and
//! the model that writes commit messages.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_core::config;

use crate::model::ModelChoice;
use crate::task::Task;

use super::sheets::{Event as SheetEvent, Load};
use super::{App, Message};

/// Something that happened in Settings → Git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitEvent {
    /// The server's snapshot setting, as read from its config.
    Snapshots(Result<bool, String>),
    /// Turn the snapshots on or off.
    ToggleSnapshots,
    /// The snapshots were turned on or off, or why they couldn't be.
    SnapshotsSet(Result<bool, String>),
    /// The model that writes commit messages; `None` for the server's
    /// default.
    CommitModel(Option<ModelChoice>),
}

/// Settings → Git's state.
#[derive(Debug, Default)]
pub struct GitSettings {
    /// Whether the server keeps snapshots, once read.
    pub snapshots: Load<bool>,
    /// Why the snapshots couldn't be turned on or off.
    pub error: Option<String>,
}

impl App {
    /// Applies something that happened in Settings → Git.
    pub(super) fn git_event(&mut self, event: GitEvent) -> Task<Message> {
        let folder = self.global_config().map(Path::to_path_buf);
        let git = &mut self.sheets.git_settings;
        match event {
            GitEvent::Snapshots(read) => {
                git.snapshots = match read {
                    Ok(on) => Load::Loaded(on),
                    Err(why) => Load::Failed(why),
                };
            }
            GitEvent::ToggleSnapshots => {
                let (Load::Loaded(on), Some(folder)) = (&git.snapshots, folder) else {
                    return Task::none();
                };
                let on = !*on;
                git.error = None;
                return Task::perform(config::set_snapshots_later(folder, on), move |result| {
                    git_message(GitEvent::SnapshotsSet(
                        result
                            .map(|()| on)
                            .map_err(|e| format!("Couldn't change harlo.json: {e}")),
                    ))
                });
            }
            GitEvent::SnapshotsSet(Ok(on)) => git.snapshots = Load::Loaded(on),
            GitEvent::SnapshotsSet(Err(why)) => git.error = Some(why),
            GitEvent::CommitModel(model) => {
                self.preferences.commit_model =
                    model.map(|m| format!("{}/{}", m.provider_id, m.id));
                return self.save_preferences();
            }
        }
        Task::none()
    }

    /// Reads git's version and name on commits, and the server's snapshot
    /// setting where Harlo may change it, for the setup guide and
    /// Settings → Git.
    pub(super) fn load_git(&mut self) -> Task<Message> {
        self.sheets.git_error = None;
        self.sheets.git_settings.error = None;
        self.sheets.git = Load::Loading;
        let global = self.git_config.clone();
        let setup = Task::perform(async move { global.setup().await }, |setup| {
            Message::Sheet(SheetEvent::Git(setup))
        });
        let setup = Task::batch([setup, self.load_accounts()]);
        let Some(folder) = self.global_config().map(Path::to_path_buf) else {
            self.sheets.git_settings.snapshots = Load::Idle;
            return setup;
        };
        let snapshots = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || config::snapshots(&folder))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|read| read.map_err(|e| e.to_string()))
            },
            |read| git_message(GitEvent::Snapshots(read)),
        );
        Task::batch([setup, snapshots])
    }

    /// The model chosen to write commit messages, named as the catalog
    /// names it.
    #[must_use]
    pub fn commit_model_choice(&self) -> Option<ModelChoice> {
        let model = self.commit_model()?;
        let name = self
            .catalog()
            .and_then(|c| c.model(&model.provider_id, &model.id))
            .map_or_else(|| model.id.clone(), |m| m.name.clone());
        Some(ModelChoice {
            provider_id: model.provider_id,
            id: model.id,
            name,
        })
    }

    /// The model chosen to write commit messages.
    pub(super) fn commit_model(&self) -> Option<harlo_core::workspace::ModelRef> {
        let (provider, model) = self.preferences.commit_model.as_deref()?.split_once('/')?;
        Some(harlo_core::workspace::ModelRef {
            provider_id: provider.to_owned(),
            id: model.to_owned(),
            variant: None,
        })
    }
}

/// `event` as the app's message.
fn git_message(event: GitEvent) -> Message {
    Message::Sheet(SheetEvent::GitSettings(event))
}
