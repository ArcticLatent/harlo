//! Settings → Tools for agents' MCP servers: connecting and disconnecting
//! them on the server, and adding, turning off and removing them in
//! OpenCode's global config, from the catalog or by hand.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use harlo_core::config::{self, McpEntry, McpRun};
use harlo_core::mcp_catalog::OFFERED;

use crate::task::Task;

use super::sheets::{Event as SheetEvent, Field, Load};
use super::{App, Message};

/// How long OpenCode takes to notice a config change and start or stop a
/// server, before its status is asked again.
const SETTLE: Duration = Duration::from_millis(1500);

/// What Extensions' MCP servers tab shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum McpView {
    /// The servers, with the rest of the section.
    #[default]
    List,
    /// One server's page, by name.
    Shown(String),
    /// The catalog and the form for a server of one's own.
    Add,
}

/// Something that happened to the MCP servers.
#[derive(Debug, Clone, PartialEq)]
pub enum McpEvent {
    /// The servers in the global config were read, or couldn't be.
    Saved(Result<Vec<McpEntry>, String>),
    /// Show the page of the server with this name.
    Show(String),
    /// Back to the list.
    Back,
    /// Show the catalog and the form.
    Add,
    /// Add the catalog's server at this place.
    AddOffered(usize),
    /// Add the server the form describes.
    Save,
    /// Connect the shown server.
    Connect,
    /// Disconnect the shown server.
    Disconnect,
    /// Turn the shown server off (`true`) or on in the config.
    TurnOff(bool),
    /// Have agents reach the shown server's tools through scripts (`true`)
    /// or as tools listed one by one.
    Scripts(bool),
    /// Remove the shown server from the config, asking once.
    Remove,
    /// A step ended: what to say, or why it failed.
    Done(Result<String, String>),
    /// Ask how the servers stand again.
    Refresh,
}

/// The MCP servers' pages.
#[derive(Debug, Default)]
pub struct McpServers {
    /// What shows.
    pub view: McpView,
    /// The servers OpenCode's global config on this computer sets up;
    /// [`Load::Idle`] when Harlo can't change it.
    pub saved: Load<Vec<McpEntry>>,
    /// The new server's name.
    pub name: String,
    /// Whether the new server is a program (`true`) or an address.
    pub local: bool,
    /// The new server's program and arguments, as typed.
    pub command: String,
    /// The new server's address.
    pub url: String,
    /// Whether a step is under way.
    pub busy: bool,
    /// Why the last step failed.
    pub error: Option<String>,
    /// What the last step did.
    pub done: Option<String>,
    /// Whether Remove has asked once.
    pub removing: bool,
}

impl McpServers {
    /// The global config's entry named `name`.
    #[must_use]
    pub fn saved(&self, name: &str) -> Option<&McpEntry> {
        match &self.saved {
            Load::Loaded(list) => list.iter().find(|entry| entry.name == name),
            Load::Idle | Load::Loading | Load::Failed(_) => None,
        }
    }

    /// The shown server's name.
    #[must_use]
    pub fn shown(&self) -> Option<&str> {
        match &self.view {
            McpView::Shown(name) => Some(name),
            McpView::List | McpView::Add => None,
        }
    }
}

impl App {
    /// Handles `event` on the MCP servers.
    pub(super) fn mcp_event(&mut self, event: McpEvent) -> Task<Message> {
        let page = &mut self.sheets.mcp;
        match event {
            McpEvent::Saved(result) => {
                page.saved = match result {
                    Ok(list) => Load::Loaded(list),
                    Err(why) => Load::Failed(why),
                };
            }
            McpEvent::Show(name) => {
                page.view = McpView::Shown(name);
                page.error = None;
                page.done = None;
                page.removing = false;
            }
            McpEvent::Back => {
                page.view = McpView::List;
                page.error = None;
                page.done = None;
            }
            McpEvent::Add => {
                page.view = McpView::Add;
                page.error = None;
                page.done = None;
                page.local = true;
            }
            McpEvent::AddOffered(index) => {
                if let Some(offered) = OFFERED.get(index) {
                    let said = format!("Added {}.", offered.title);
                    return self.write_entry(offered.entry(), said);
                }
            }
            McpEvent::Save => return self.save_own(),
            McpEvent::Connect | McpEvent::Disconnect => {
                let connect = event == McpEvent::Connect;
                return self.connect_mcp(connect);
            }
            McpEvent::TurnOff(off) => return self.turn_off_mcp(off),
            McpEvent::Scripts(on) => return self.mcp_scripts(on),
            McpEvent::Remove => return self.remove_mcp(),
            McpEvent::Done(result) => {
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
                return Task::batch([self.load_mcp(), self.load_setup(), settle()]);
            }
            McpEvent::Refresh => {
                // Only while someone is looking.
                if self.sheets.open == Some(super::Sheet::Settings)
                    && self.sheets.section == super::Section::Extensions
                {
                    return self.load_setup();
                }
            }
        }
        Task::none()
    }

    /// Takes text typed into the form for a server of one's own.
    pub(super) fn mcp_text(&mut self, field: Field, value: &str) -> Task<Message> {
        let page = &mut self.sheets.mcp;
        let slot = match field {
            Field::McpName => &mut page.name,
            Field::McpCommand => &mut page.command,
            Field::McpUrl => &mut page.url,
            _ => return Task::none(),
        };
        value.clone_into(slot);
        Task::none()
    }

    /// Reads the servers in the global config, when Harlo may change it.
    pub(super) fn load_mcp(&mut self) -> Task<Message> {
        let Some(folder) = self.global_config().map(ToOwned::to_owned) else {
            self.sheets.mcp.saved = Load::Idle;
            return Task::none();
        };
        if !matches!(self.sheets.mcp.saved, Load::Loaded(_)) {
            self.sheets.mcp.saved = Load::Loading;
        }
        Task::perform(
            async move {
                config::mcp_servers_later(folder)
                    .await
                    .map_err(|e| e.to_string())
            },
            |result| mcp_message(McpEvent::Saved(result)),
        )
    }

    /// Adds the server the form describes, checking it first.
    fn save_own(&mut self) -> Task<Message> {
        let page = &mut self.sheets.mcp;
        let name = page.name.trim().to_owned();
        let problem = if name.is_empty() {
            Some("Give it a name first.".to_owned())
        } else if name.contains(char::is_whitespace) {
            Some("A name has no spaces; OpenCode puts it before its tools' names.".to_owned())
        } else if page.saved(&name).is_some() {
            Some(format!("There's already a server named {name}."))
        } else {
            None
        };
        if let Some(problem) = problem {
            page.error = Some(problem);
            return Task::none();
        }
        let run = if page.local {
            match shell_words::split(page.command.trim()) {
                Ok(command) if !command.is_empty() => McpRun::Local {
                    command,
                    environment: Vec::new(),
                },
                Ok(_) => {
                    page.error = Some("Type the command that starts it first.".to_owned());
                    return Task::none();
                }
                Err(why) => {
                    page.error = Some(format!("The command doesn't read right: {why}."));
                    return Task::none();
                }
            }
        } else {
            let url = page.url.trim();
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                page.error = Some("Type its address, starting with https://.".to_owned());
                return Task::none();
            }
            McpRun::Remote {
                url: url.to_owned(),
                headers: Vec::new(),
            }
        };
        page.name.clear();
        page.command.clear();
        page.url.clear();
        // It shows in the list, connecting.
        page.view = McpView::List;
        let said = format!("Added {name}.");
        self.write_entry(
            McpEntry {
                name,
                run,
                disabled: false,
                codemode: true,
            },
            said,
        )
    }

    /// Writes `entry` to the global config, then says `said`.
    fn write_entry(&mut self, entry: McpEntry, said: String) -> Task<Message> {
        let Some(folder) = self.global_config().map(ToOwned::to_owned) else {
            return Task::none();
        };
        let page = &mut self.sheets.mcp;
        page.busy = true;
        page.error = None;
        page.done = None;
        Task::perform(
            async move {
                config::set_mcp_server_later(folder, entry)
                    .await
                    .map(|()| said)
                    .map_err(|e| e.to_string())
            },
            |result| mcp_message(McpEvent::Done(result)),
        )
    }

    /// Connects or disconnects the shown server on the server, leaving the
    /// config as it is.
    fn connect_mcp(&mut self, connect: bool) -> Task<Message> {
        let (Some(client), Some(name)) = (
            self.client.clone(),
            self.sheets.mcp.shown().map(ToOwned::to_owned),
        ) else {
            return Task::none();
        };
        let folder = self.folder();
        let page = &mut self.sheets.mcp;
        page.busy = true;
        page.error = None;
        page.done = None;
        Task::perform(
            async move {
                let result = if connect {
                    client.connect_mcp(&folder, &name).await
                } else {
                    client.disconnect_mcp(&folder, &name).await
                };
                result
                    .map(|()| {
                        if connect {
                            "Connecting."
                        } else {
                            "Disconnected."
                        }
                        .to_owned()
                    })
                    .map_err(|e| {
                        e.server_message()
                            .map_or_else(|| e.to_string(), ToOwned::to_owned)
                    })
            },
            |result| mcp_message(McpEvent::Done(result)),
        )
    }

    /// Turns the shown server off or on in the global config.
    fn turn_off_mcp(&mut self, off: bool) -> Task<Message> {
        let (Some(folder), Some(name)) = (
            self.global_config().map(ToOwned::to_owned),
            self.sheets.mcp.shown().map(ToOwned::to_owned),
        ) else {
            return Task::none();
        };
        let page = &mut self.sheets.mcp;
        page.busy = true;
        page.error = None;
        let said = if off { "Turned off." } else { "Turned on." }.to_owned();
        Task::perform(
            async move {
                config::set_mcp_disabled_later(folder, name, off)
                    .await
                    .map(|()| said)
                    .map_err(|e| e.to_string())
            },
            |result| mcp_message(McpEvent::Done(result)),
        )
    }

    /// Has agents reach the shown server's tools through Code Mode scripts,
    /// or as tools listed one by one, in the global config.
    fn mcp_scripts(&mut self, on: bool) -> Task<Message> {
        let (Some(folder), Some(name)) = (
            self.global_config().map(ToOwned::to_owned),
            self.sheets.mcp.shown().map(ToOwned::to_owned),
        ) else {
            return Task::none();
        };
        let page = &mut self.sheets.mcp;
        page.busy = true;
        page.error = None;
        let said = if on {
            "Its tools are reached through scripts."
        } else {
            "Its tools are listed one by one."
        }
        .to_owned();
        Task::perform(
            async move {
                config::set_mcp_codemode_later(folder, name, on)
                    .await
                    .map(|()| said)
                    .map_err(|e| e.to_string())
            },
            |result| mcp_message(McpEvent::Done(result)),
        )
    }

    /// Removes the shown server from the global config, once asked twice.
    fn remove_mcp(&mut self) -> Task<Message> {
        let (Some(folder), Some(name)) = (
            self.global_config().map(ToOwned::to_owned),
            self.sheets.mcp.shown().map(ToOwned::to_owned),
        ) else {
            return Task::none();
        };
        let page = &mut self.sheets.mcp;
        if !page.removing {
            page.removing = true;
            return Task::none();
        }
        page.removing = false;
        page.busy = true;
        page.error = None;
        page.view = McpView::List;
        let said = format!("Removed {name}.");
        Task::perform(
            async move {
                config::remove_mcp_server_later(folder, name)
                    .await
                    .map(|()| said)
                    .map_err(|e| e.to_string())
            },
            |result| mcp_message(McpEvent::Done(result)),
        )
    }
}

/// Asks how the servers stand once OpenCode has had time to act on a
/// change.
pub(super) fn settle() -> Task<Message> {
    Task::future(async {
        tokio::time::sleep(SETTLE).await;
        mcp_message(McpEvent::Refresh)
    })
}

/// `event` as the app's message.
fn mcp_message(event: McpEvent) -> Message {
    Message::Sheet(SheetEvent::Mcp(event))
}
