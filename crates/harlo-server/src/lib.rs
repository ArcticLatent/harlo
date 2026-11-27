//! Harlo's background server: it runs sessions on `harlo-engine` and
//! serves them over HTTP and server-sent events.
//!
//! It reads Harlo's config in place (`$XDG_CONFIG_HOME/harlo/harlo.jsonc`,
//! started once from an earlier OpenCode setup when there is one) and keeps
//! its sessions in one SQLite file.

// Rust guideline compliant 2026-10-09

mod agents;
mod api;
mod approvals;
mod automations;
mod checkpoint;
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
mod mcp_auth;
mod memory;
mod oauth;
mod phone;
mod plugins;
mod projects;
mod proposals;
mod pty;
mod search;
mod server;
pub mod service;
mod shell;
mod stale;
mod stats;
mod store;
mod subagents;
mod transfer;
mod trust;
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
    /// The folder Harlo's config folder is in (`$XDG_CONFIG_HOME`).
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

/// The store setting that says an earlier OpenCode setup was looked for.
const COPIED: &str = "config/copied-from-opencode";

/// Starts Harlo's config from an earlier OpenCode setup, the first time the
/// server starts. A failure is logged and tried again next start.
fn copy_old_config(store: &Store, config_home: &std::path::Path) {
    if store.setting(COPIED).ok().flatten().is_some() {
        return;
    }
    match harlo_engine::copy_from_opencode(config_home) {
        Ok(copied) => {
            if let Some(file) = &copied {
                tracing::info!(name: "config.copied", file = %file.display(), "config copied from an OpenCode setup");
            }
            let done = copied.map_or_else(String::new, |f| f.display().to_string());
            if let Err(e) = store.put_setting(COPIED, &done) {
                tracing::warn!(name: "config.copy.unnoted", error = %e, "couldn't note the config copy");
            }
        }
        Err(e) => {
            tracing::warn!(name: "config.copy.failed", error = %e, "couldn't copy an OpenCode setup's config");
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
    copy_old_config(&store, &places.config_home);
    let config = harlo_engine::Config::load(&places.config_home).map_err(|e| e.to_string())?;
    let phone = PhoneHost::new(places.phone.clone(), places.phone_wanted.clone());
    // What holds the server's password, the keys it keeps and its approvals.
    let beside = |suffix: &str| {
        let mut name = places.database.clone().into_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    let sealed: Vec<PathBuf> = [
        // (An agent that could list a plugin could bring in anything.)
        Some(harlo_engine::config_folder(&places.config_home).join(plugins::LIST)),
        Some(service::registration_path()),
        Some(places.database.clone()),
        Some(beside("-wal")),
        Some(beside("-shm")),
        places.phone.clone(),
        Some(places.phone_wanted.clone()),
    ]
    .into_iter()
    .flatten()
    .map(|path| harlo_engine::tools::real_path(&path))
    .collect();
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
        sealed,
    )))
}
