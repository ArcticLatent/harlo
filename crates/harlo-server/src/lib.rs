//! Harlo's background server: it runs sessions on `harlo-engine` and
//! serves them over HTTP and server-sent events.
//!
//! For now it answers the part of OpenCode 2's API that Harlo uses for a
//! turn, in OpenCode's shapes, so Harlo can attach to either server. It
//! reads OpenCode's config in place and keeps its own sessions in one
//! SQLite file.

// Rust guideline compliant 2026-10-09

mod agents;
mod api;
mod approvals;
mod automations;
mod claude;
mod code;
mod commands;
mod credentials;
mod folder;
mod forms;
mod fs;
mod git;
mod goals;
mod mcp;
mod memory;
mod oauth;
mod phone;
mod projects;
mod pty;
mod server;
pub mod service;
mod shell;
mod stale;
mod stats;
mod store;
mod subagents;
mod transfer;
mod undo;
mod watch;

use std::path::PathBuf;
use std::sync::Arc;

pub use api::router;
pub use phone::PhoneHost;
pub use server::Server;
pub use store::Store;

/// Where the server reads its configuration and keeps its data.
#[derive(Debug, Clone)]
pub struct Places {
    /// The folder OpenCode's config folder is in (`$XDG_CONFIG_HOME`).
    pub config_home: PathBuf,
    /// The SQLite file.
    pub database: PathBuf,
    /// Where phone access keeps its key and paired phones.
    pub phone: Option<PathBuf>,
    /// Where the wanted phone access is kept.
    pub phone_wanted: PathBuf,
}

impl Places {
    /// The usual places: `$XDG_CONFIG_HOME` (or `~/.config`) and
    /// `$XDG_DATA_HOME/harlo/server.db` (or under `~/.local/share`).
    #[must_use]
    pub fn from_env() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map_or_else(|| home.join(".config"), PathBuf::from);
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map_or_else(|| home.join(".local/share"), PathBuf::from);
        Self {
            config_home,
            database: data_home.join("harlo/server.db"),
            phone: harlo_core::phone::folder(),
            phone_wanted: data_home.join("harlo/phone.json"),
        }
    }
}

/// Opens the store and reads the configuration.
///
/// # Errors
///
/// Fails, with a message for a person, when the data folder can't be made,
/// the store can't be opened, or the config file can't be parsed.
pub fn open(places: &Places, password: Option<String>) -> Result<Arc<Server>, String> {
    if let Some(folder) = places.database.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
    }
    let store = Store::open(&places.database)
        .map_err(|e| format!("Couldn't open {}: {e}", places.database.display()))?;
    let config = harlo_engine::Config::load(&places.config_home).map_err(|e| e.to_string())?;
    let phone = PhoneHost::new(places.phone.clone(), places.phone_wanted.clone());
    Ok(Arc::new(Server::new(
        store,
        config,
        &places.config_home,
        password,
        phone,
        places
            .database
            .parent()
            .map_or_else(|| PathBuf::from("snapshot"), |d| d.join("snapshot")),
    )))
}
