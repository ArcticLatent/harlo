//! The folders the sandbox reads its settings from and keeps hidden.

// Rust guideline compliant 2026-11-19

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".config"))
}

/// The sandbox's extras: `~/.config/harlo/sandbox.json`, else the one
/// harlo's former agent read (`~/.config/harlo-agent/sandbox.json`).
pub fn sandbox_file() -> PathBuf {
    let file = config_home().join("harlo/sandbox.json");
    if file.is_file() { file } else { old_agent_config().join("sandbox.json") }
}

/// harlo's former agent's settings folder: hidden (it may hold keys).
pub fn old_agent_config() -> PathBuf {
    config_home().join("harlo-agent")
}

/// harlo's former agent's sessions: hidden.
pub fn old_agent_sessions() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("harlo-agent/sessions"))
}

/// This process's user id.
pub fn uid() -> u32 {
    // SAFETY: getuid(2) takes no arguments, always succeeds and touches no
    // memory of this process.
    unsafe { libc::getuid() }
}
