//! Settings → Tools for agents' plugins: checking the package plugins for
//! newer versions and updating them, through the server.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Plugin, PluginSource};

use super::sheets::{Event as SheetEvent, Load};
use super::{App, Message};
use crate::task::Task;

/// Something that happened to the plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginEvent {
    /// Check the package plugins for newer versions.
    Check,
    /// The check ended: the plugins as they stand, or why it failed.
    Checked(Result<Vec<Plugin>, String>),
    /// Update the plugin at this place in the list, or every outdated one.
    Update(Option<usize>),
    /// The update ended: what it did, or why it failed.
    Updated(Result<String, String>),
}

/// The plugins' state in Settings.
#[derive(Debug, Default)]
pub struct Plugins {
    /// The list the last check returned, which knows which are outdated;
    /// `None` shows the list loaded with the section.
    pub checked: Option<Vec<Plugin>>,
    /// Whether a check or an update is under way.
    pub busy: bool,
    /// What the last step did.
    pub done: Option<String>,
    /// Why the last step failed.
    pub error: Option<String>,
}

impl App {
    /// The plugins Settings shows: the last check's, or the section's.
    #[must_use]
    pub fn plugin_list(&self) -> Option<&[Plugin]> {
        if let Some(checked) = &self.sheets.plugins.checked {
            return Some(checked);
        }
        match &self.sheets.setup {
            Load::Loaded(setup) => setup.plugins.as_deref().ok(),
            Load::Idle | Load::Loading | Load::Failed(_) => None,
        }
    }

    /// Handles `event` on the plugins.
    pub(super) fn plugin_event(&mut self, event: PluginEvent) -> Task<Message> {
        match event {
            PluginEvent::Check => {
                let Some(client) = self.client.clone() else {
                    return Task::none();
                };
                let folder = self.folder();
                self.start_plugin_step();
                Task::perform(
                    async move {
                        client
                            .check_plugins(&folder)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |result| plugin_message(PluginEvent::Checked(result)),
                )
            }
            PluginEvent::Checked(result) => {
                let page = &mut self.sheets.plugins;
                page.busy = false;
                match result {
                    Ok(list) => {
                        let outdated = list.iter().filter(|p| is_outdated(p)).count();
                        page.done = Some(match outdated {
                            0 => "Every package plugin is up to date.".to_owned(),
                            1 => "One plugin has an update.".to_owned(),
                            n => format!("{n} plugins have updates."),
                        });
                        page.checked = Some(list);
                    }
                    Err(why) => page.error = Some(why),
                }
                Task::none()
            }
            PluginEvent::Update(at) => self.update_plugins(at),
            PluginEvent::Updated(result) => {
                let page = &mut self.sheets.plugins;
                page.busy = false;
                page.checked = None;
                match result {
                    Ok(said) => page.done = Some(said),
                    Err(why) => page.error = Some(why),
                }
                self.load_setup()
            }
        }
    }

    /// Marks a plugin step as started.
    fn start_plugin_step(&mut self) {
        let page = &mut self.sheets.plugins;
        page.busy = true;
        page.done = None;
        page.error = None;
    }

    /// Updates the plugin at `at` in the list, or every outdated one.
    fn update_plugins(&mut self, at: Option<usize>) -> Task<Message> {
        let Some(list) = self.plugin_list() else {
            return Task::none();
        };
        let targets: Vec<String> = match at {
            Some(at) => list.get(at).and_then(target).into_iter().collect(),
            None => list
                .iter()
                .filter(|p| is_outdated(p))
                .filter_map(target)
                .collect(),
        };
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if targets.is_empty() {
            return Task::none();
        }
        let folder = self.folder();
        self.start_plugin_step();
        let said = match targets.as_slice() {
            [one] => format!("Updated {one}."),
            many => format!("Updated {} plugins.", many.len()),
        };
        Task::perform(
            async move {
                client
                    .update_plugins(&folder, &targets)
                    .await
                    .map(|()| said)
                    .map_err(|e| {
                        e.server_message()
                            .map_or_else(|| e.to_string(), ToOwned::to_owned)
                    })
            },
            |result| plugin_message(PluginEvent::Updated(result)),
        )
    }
}

/// Whether `plugin` is a package with a newer version out.
#[must_use]
pub fn is_outdated(plugin: &Plugin) -> bool {
    matches!(plugin.source, PluginSource::Package { outdated: true, .. })
}

/// The package `plugin` is configured as, when it's a package.
fn target(plugin: &Plugin) -> Option<String> {
    match &plugin.source {
        PluginSource::Package { target, .. } => Some(target.clone()),
        _ => None,
    }
}

/// `event` as the app's message.
fn plugin_message(event: PluginEvent) -> Message {
    Message::Sheet(SheetEvent::Plugin(event))
}
