//! Harnesses (pi, omp, Claude Code, OpenCode) and the translation of their
//! protocols into one normalized event vocabulary.

mod demo;
pub mod branches;
pub mod claude;
mod claude_run;
pub mod event;
pub mod file_blocks;
pub mod framing;
pub mod opencode;
mod opencode_run;
pub mod process;
pub mod protocol_log;
pub mod store;

pub use event::{BlockKind, HarnessEvent, HarnessKind, NoticeLevel, Translator};
pub use process::{Harness, HarnessConfig, RemoteLaunch, SendMode, SessionTarget, image_from_file, on_host, resolve_program};
pub use opencode_run::shutdown as stop_opencode;
/// A remote host's OpenCode: its sessions, a saved one, its prompts.
pub use opencode_run::{
    remote_saved as opencode_remote_saved, remote_sessions as opencode_remote_sessions, remote_subagents as opencode_remote_subagents,
    remote_user_entries as opencode_remote_user_entries,
};
pub use store::SessionInfo;

/// harlo's MCP bridge (`<harlo> --mcp-bridge`), given to the agents harlo
/// starts here (Claude with each run, OpenCode with its server). Unset: no
/// bridge (tests, other users of this crate).
static MCP_BRIDGE: std::sync::OnceLock<(std::path::PathBuf, std::path::PathBuf)> = std::sync::OnceLock::new();

/// `harlo`: the executable; `socket`: where it listens.
pub fn set_mcp_bridge(harlo: std::path::PathBuf, socket: std::path::PathBuf) {
    let _ = MCP_BRIDGE.set((harlo, socket));
}

/// Orchestration is on (Settings → Orchestration): sessions get the bridge.
static ORCHESTRATION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_orchestration(on: bool) {
    ORCHESTRATION.store(on, std::sync::atomic::Ordering::SeqCst);
}

/// On a remote host: puts the bridge there and forwards harlo's socket,
/// giving the command its agents run (`harlo_core::orchestration`).
pub type RemoteBridge = fn(&str) -> Option<Vec<String>>;
static REMOTE_BRIDGE: std::sync::OnceLock<RemoteBridge> = std::sync::OnceLock::new();

pub fn set_remote_bridge(f: RemoteBridge) {
    let _ = REMOTE_BRIDGE.set(f);
}

/// The bridge an agent runs for harlo's tools, here or on its host, while
/// orchestration is on.
pub(crate) fn mcp_bridge_for(remote: Option<&RemoteLaunch>) -> Option<Vec<String>> {
    match remote {
        None => mcp_bridge(),
        Some(r) if ORCHESTRATION.load(std::sync::atomic::Ordering::SeqCst) => REMOTE_BRIDGE.get()?(&r.host),
        Some(_) => None,
    }
}

/// The bridge's command line here (`[harlo, --mcp-bridge, --socket, path]`),
/// while orchestration is on.
pub(crate) fn mcp_bridge() -> Option<Vec<String>> {
    if !ORCHESTRATION.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    MCP_BRIDGE.get().map(|(exe, socket)| vec![exe.to_string_lossy().into_owned(), "--mcp-bridge".into(), "--socket".into(), socket.to_string_lossy().into_owned()])
}
