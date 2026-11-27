//! Settings → Experimental: the server's features that are still being
//! tried, each with a switch where Harlo may change the server's config.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_core::config::{self, Experiment};

use crate::task::Task;

use super::sheets::{Event as SheetEvent, Load};
use super::{App, Message};

/// Something that happened in Settings → Experimental.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExperimentEvent {
    /// The features that are on, as read from the server's config.
    Read(Result<Vec<Experiment>, String>),
    /// Turn a feature on or off.
    Toggle(Experiment),
    /// A feature was turned on or off, or why it couldn't be.
    Set(Result<(Experiment, bool), String>),
}

/// Settings → Experimental's state.
#[derive(Debug, Default)]
pub struct Experiments {
    /// The features that are on, once read.
    pub on: Load<Vec<Experiment>>,
    /// Why a feature couldn't be turned on or off.
    pub error: Option<String>,
}

impl App {
    /// Applies something that happened in Settings → Experimental.
    pub(super) fn experiment_event(&mut self, event: ExperimentEvent) -> Task<Message> {
        let folder = self.global_config().map(Path::to_path_buf);
        let page = &mut self.sheets.experiments;
        match event {
            ExperimentEvent::Read(read) => {
                page.on = match read {
                    Ok(on) => Load::Loaded(on),
                    Err(why) => Load::Failed(why),
                };
            }
            ExperimentEvent::Toggle(experiment) => {
                let (Load::Loaded(now), Some(folder)) = (&page.on, folder) else {
                    return Task::none();
                };
                let on = !now.contains(&experiment);
                page.error = None;
                return Task::perform(
                    config::set_experiment_later(folder, experiment, on),
                    move |result| {
                        experiment_message(ExperimentEvent::Set(
                            result
                                .map(|()| (experiment, on))
                                .map_err(|e| format!("Couldn't change harlo.json: {e}")),
                        ))
                    },
                );
            }
            ExperimentEvent::Set(Ok((experiment, on))) => {
                if let Load::Loaded(now) = &mut page.on {
                    now.retain(|e| *e != experiment);
                    if on {
                        now.push(experiment);
                    }
                }
                // What agents can do changed: the server lists it again.
                self.forget_catalogs();
            }
            ExperimentEvent::Set(Err(why)) => page.error = Some(why),
        }
        Task::none()
    }

    /// Reads which experimental features are on, from the config of the
    /// server on this computer.
    pub(super) fn load_experiments(&mut self) -> Task<Message> {
        self.sheets.experiments.error = None;
        let Some(folder) = self.global_config().map(Path::to_path_buf) else {
            self.sheets.experiments.on = Load::Idle;
            return Task::none();
        };
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || config::experiments(&folder))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|read| read.map_err(|e| e.to_string()))
            },
            |read| experiment_message(ExperimentEvent::Read(read)),
        )
    }
}

/// `event` as the app's message.
fn experiment_message(event: ExperimentEvent) -> Message {
    Message::Sheet(SheetEvent::Experimental(event))
}
