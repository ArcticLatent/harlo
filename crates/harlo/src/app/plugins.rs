//! Settings → Extensions → Plugins: the packages added to Harlo's server,
//! each bringing skills, commands, agents and MCP servers.
//!
//! Every step is the server's: adding one fetches it there, and what it
//! asks for waits there for a yes. A change reaches every client of the
//! server.
//!
//! The plugins people have published are listed only once Browse is
//! pressed: the server asks GitHub then, and not before.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Client, Error, Plugin, PluginState, PublishedPlugin};

use super::sheets::Load;
use super::{App, Message};
use crate::task::Task;

/// Settings → Extensions → Plugins' state.
#[derive(Debug, Default)]
pub struct Plugins {
    /// The plugins the server listed.
    pub list: Load<Vec<Plugin>>,
    /// The plugins people have published; idle until they're asked for.
    pub published: Load<Vec<PublishedPlugin>>,
    /// The plugin whose page shows, by name; none shows the list.
    pub shown: Option<String>,
    /// Whether a step is on its way.
    pub busy: bool,
    /// Whether Remove was pressed once and waits to be pressed again.
    pub removing: bool,
    /// What the last step did.
    pub done: Option<String>,
    /// Why the last step failed.
    pub error: Option<String>,
}

impl Plugins {
    /// The plugin whose page shows.
    #[must_use]
    pub fn plugin(&self) -> Option<&Plugin> {
        let Load::Loaded(list) = &self.list else {
            return None;
        };
        let shown = self.shown.as_deref()?;
        list.iter().find(|plugin| plugin.name == shown)
    }
}

/// Something that happened in Settings → Extensions → Plugins.
#[derive(Debug, Clone)]
pub enum PluginEvent {
    /// The server's plugins came, or couldn't.
    Loaded(Result<Vec<Plugin>, String>),
    /// Add the plugin at this address or folder.
    Add(String),
    /// Show the page of the plugin at this place in the list.
    Show(usize),
    /// Back to the list.
    Back,
    /// Allow what the shown plugin asks for.
    Approve,
    /// Move the shown plugin to its newest commit.
    Update,
    /// Fetch the shown plugin, listed but missing on the server's computer.
    Install,
    /// Turn the shown plugin on (`true`) or off.
    Switch(bool),
    /// Remove the shown plugin, asking once.
    Remove,
    /// Fill in the shown plugin's setting at this place with this text.
    Fill(usize, String),
    /// Clear the shown plugin's setting at this place.
    Clear(usize),
    /// List the plugins people have published.
    Browse,
    /// The published plugins came, or couldn't.
    Published(Result<Vec<PublishedPlugin>, String>),
    /// Add the published plugin at this place in their list.
    AddPublished(usize),
    /// A step ended: what to say, or why it failed.
    Done(Result<String, String>),
}

impl App {
    /// Takes an event of Settings → Extensions → Plugins.
    pub(super) fn plugin_event(&mut self, event: PluginEvent) -> Task<Message> {
        let page = &mut self.sheets.plugins;
        match event {
            PluginEvent::Loaded(result) => {
                page.list = match result {
                    Ok(list) => Load::Loaded(list),
                    Err(why) => Load::Failed(why),
                };
                // A plugin removed from another client has no page.
                if page.shown.is_some() && page.plugin().is_none() {
                    page.shown = None;
                }
                Task::none()
            }
            PluginEvent::Add(src) => self.add_plugin(&src),
            PluginEvent::Browse => self.browse_plugins(),
            PluginEvent::Published(result) => {
                page.published = match result {
                    Ok(list) => Load::Loaded(list),
                    Err(why) => Load::Failed(why),
                };
                Task::none()
            }
            PluginEvent::AddPublished(at) => {
                let src = match &page.published {
                    Load::Loaded(list) => list.get(at).map(|plugin| plugin.src.clone()),
                    _ => None,
                };
                src.map_or_else(Task::none, |src| self.add_plugin(&src))
            }
            PluginEvent::Show(index) => {
                if let Load::Loaded(list) = &page.list {
                    page.shown = list.get(index).map(|plugin| plugin.name.clone());
                }
                page.removing = false;
                page.done = None;
                page.error = None;
                Task::none()
            }
            PluginEvent::Back => {
                page.shown = None;
                page.removing = false;
                page.done = None;
                page.error = None;
                Task::none()
            }
            PluginEvent::Approve => self.shown_plugin_step(|client, name| async move {
                client.approve_plugin(&name).await?;
                Ok(format!("{name} is in use."))
            }),
            PluginEvent::Update => self.update_shown_plugin(),
            PluginEvent::Install => self.shown_plugin_step(|client, name| async move {
                client.install_plugin(&name).await?;
                Ok(format!("Fetched {name}."))
            }),
            PluginEvent::Switch(on) => self.shown_plugin_step(move |client, name| async move {
                client.set_plugin_enabled(&name, on).await?;
                Ok(format!("Turned {name} {}.", if on { "on" } else { "off" }))
            }),
            PluginEvent::Remove => self.remove_shown_plugin(),
            PluginEvent::Fill(at, text) => self.fill_shown_plugin(at, Some(text)),
            PluginEvent::Clear(at) => self.fill_shown_plugin(at, None),
            PluginEvent::Done(result) => {
                page.busy = false;
                match result {
                    Ok(said) => {
                        page.done = Some(said);
                        page.error = None;
                    }
                    Err(why) => {
                        page.error = Some(why);
                        page.done = None;
                    }
                }
                // What a plugin brings shows in the menus and the server's
                // MCP list at once.
                Task::batch([self.load_plugins(), self.load_setup()])
            }
        }
    }

    /// Adds the plugin at `src`, an address or a folder on the server's
    /// computer.
    fn add_plugin(&mut self, src: &str) -> Task<Message> {
        let src = src.trim().to_owned();
        if src.is_empty() {
            return Task::none();
        }
        self.plugin_step(move |client| async move {
            let plugin = client.add_plugin(&src, None).await?;
            Ok(if plugin.asks.is_empty() {
                format!("Added {}.", plugin.name)
            } else {
                format!(
                    "Added {}. It waits for you to allow what it asks.",
                    plugin.name
                )
            })
        })
    }

    /// Asks the server for the plugins people have published.
    fn browse_plugins(&mut self) -> Task<Message> {
        let page = &mut self.sheets.plugins;
        let Some(client) = self.client.clone() else {
            page.published = Load::Failed("Not connected to the server yet.".to_owned());
            return Task::none();
        };
        if matches!(page.published, Load::Loading) {
            return Task::none();
        }
        page.published = Load::Loading;
        Task::perform(
            async move { client.published_plugins().await.map_err(|e| e.to_string()) },
            |result| Message::Plugins(PluginEvent::Published(result)),
        )
    }

    /// Moves the shown plugin to the newest commit of what it follows.
    fn update_shown_plugin(&mut self) -> Task<Message> {
        self.shown_plugin_step(|client, name| async move {
            let before = client.plugins().await?;
            let was = before
                .iter()
                .find(|plugin| plugin.name == name)
                .map(|plugin| plugin.commit.clone());
            let now = client.update_plugin(&name).await?;
            Ok(if was.as_deref() == Some(now.commit.as_str()) {
                format!("{name} is up to date.")
            } else if now.state == PluginState::Waiting {
                format!("Updated {name}. It asks for something new, so it waits for you.")
            } else {
                format!("Updated {name}.")
            })
        })
    }

    /// Fills in the shown plugin's setting at place `at` with `text`, or
    /// clears it.
    ///
    /// A secret's field is always empty, so nothing typed into it changes
    /// nothing; what isn't a secret is cleared by emptying its field.
    fn fill_shown_plugin(&mut self, at: usize, text: Option<String>) -> Task<Message> {
        let Some(setting) = self
            .sheets
            .plugins
            .plugin()
            .and_then(|plugin| plugin.settings.get(at))
            .cloned()
        else {
            return Task::none();
        };
        let text = text.map(|text| text.trim().to_owned());
        if setting.secret && text.as_deref() == Some("") {
            return Task::none();
        }
        let text = text.filter(|text| !text.is_empty());
        if !setting.secret && text == setting.value {
            return Task::none();
        }
        self.shown_plugin_step(move |client, name| async move {
            client
                .fill_plugin(&name, &setting.key, text.as_deref())
                .await?;
            Ok(if text.is_some() {
                format!("Saved {}.", setting.label)
            } else {
                format!("Cleared {}.", setting.label)
            })
        })
    }

    /// Removes the shown plugin from the server, once asked twice.
    fn remove_shown_plugin(&mut self) -> Task<Message> {
        let page = &mut self.sheets.plugins;
        if page.busy || page.shown.is_none() {
            return Task::none();
        }
        if !page.removing {
            page.removing = true;
            return Task::none();
        }
        let task = self.shown_plugin_step(|client, name| async move {
            client.remove_plugin(&name).await?;
            Ok(format!("Removed {name}."))
        });
        self.sheets.plugins.shown = None;
        task
    }

    /// Loads the server's plugins.
    pub(super) fn load_plugins(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.sheets.plugins.list = Load::Failed("Not connected to the server yet.".to_owned());
            return Task::none();
        };
        if !matches!(self.sheets.plugins.list, Load::Loaded(_)) {
            self.sheets.plugins.list = Load::Loading;
        }
        Task::perform(
            async move { client.plugins().await.map_err(|e| e.to_string()) },
            |result| Message::Plugins(PluginEvent::Loaded(result)),
        )
    }

    /// Takes a step on the shown plugin, unless one is on its way.
    fn shown_plugin_step<F, Fut>(&mut self, make: F) -> Task<Message>
    where
        F: FnOnce(Client, String) -> Fut + Send + 'static,
        Fut: Future<Output = Result<String, Error>> + Send + 'static,
    {
        let Some(name) = self.sheets.plugins.shown.clone() else {
            return Task::none();
        };
        self.plugin_step(move |client| make(client, name))
    }

    /// Takes a step on the server's plugins, unless one is on its way;
    /// its outcome comes as [`PluginEvent::Done`].
    fn plugin_step<F, Fut>(&mut self, make: F) -> Task<Message>
    where
        F: FnOnce(Client) -> Fut + Send + 'static,
        Fut: Future<Output = Result<String, Error>> + Send + 'static,
    {
        let page = &mut self.sheets.plugins;
        let Some(client) = self.client.clone() else {
            page.error = Some("Not connected to the server yet.".to_owned());
            return Task::none();
        };
        if page.busy {
            return Task::none();
        }
        page.busy = true;
        page.removing = false;
        page.done = None;
        page.error = None;
        Task::perform(
            async move { make(client).await.map_err(|e| e.to_string()) },
            |result| Message::Plugins(PluginEvent::Done(result)),
        )
    }
}
