//! Harnesses (pi, omp, Claude Code, OpenCode) and the translation of their
//! protocols into one normalized event vocabulary.

mod demo;
pub mod acp;
mod acp_run;
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
pub use acp::shutdown as stop_acp;
/// A remote host's OpenCode: its sessions, a saved one, its prompts. And
/// deleting a session (here or there).
pub use opencode_run::{
    delete_session as opencode_delete_session, remote_saved as opencode_remote_saved, remote_sessions as opencode_remote_sessions,
    remote_subagents as opencode_remote_subagents, remote_user_entries as opencode_remote_user_entries,
};
pub use store::SessionInfo;

/// Whether a model's server (its `baseUrl`) is this computer or the local
/// network, a model you run yourself, rather than a provider's service:
/// loopback, private and link-local addresses, Tailscale's, and `.local`,
/// `.lan`, `.home.arpa` and `.ts.net` names.
pub fn is_local_url(url: &str) -> bool {
    use std::net::IpAddr;
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = match host_port.strip_prefix('[') {
        Some(h) => h.split(']').next().unwrap_or(""),
        None => host_port.rsplit_once(':').map_or(host_port, |(h, _)| h),
    }
    .to_ascii_lowercase();
    if host == "localhost" || [".localhost", ".local", ".lan", ".home.arpa", ".ts.net"].iter().any(|s| host.ends_with(s)) {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            let [a, b, ..] = ip.octets();
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified() || (a == 100 && (64..128).contains(&b))
        }
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unspecified() || ip.segments()[0] & 0xfe00 == 0xfc00 || ip.segments()[0] & 0xffc0 == 0xfe80,
        Err(_) => false,
    }
}

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

#[cfg(test)]
mod tests {
    #[test]
    fn local_servers() {
        use super::is_local_url;
        for url in ["http://127.0.0.1:8080/v1", "http://localhost:11434", "http://192.168.1.20:1234/v1", "http://10.0.0.5/v1", "http://[::1]:8080", "http://100.101.102.103:8080", "http://gpu-box.local:8000/v1", "http://box.tail1234.ts.net/v1"] {
            assert!(is_local_url(url), "{url}");
        }
        for url in ["https://api.openai.com/v1", "https://opencode.ai/inference/openai/v1", "https://8.8.8.8/v1", "http://100.200.1.1", ""] {
            assert!(!is_local_url(url), "{url}");
        }
    }
}
