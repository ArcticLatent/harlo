//! Harnesses (pi, omp, Claude Code, OpenCode) and the translation of their
//! protocols into one normalized event vocabulary.

mod demo;
pub mod acp;
mod acp_run;
mod ask_bridge;
pub mod branches;
pub mod claude;
mod claude_run;
pub mod event;
pub mod file_blocks;
pub mod framing;
pub mod mirror;
pub mod opencode;
mod opencode_run;
mod permission_bridge;
pub mod process;
pub mod protocol_log;
pub mod store;

pub use event::{BlockKind, HarnessEvent, HarnessKind, NoticeLevel, Translator};
pub use process::{Harness, HarnessConfig, RemoteLaunch, SendMode, SessionTarget, image_from_file, on_host, resolve_program};
pub use opencode_run::shutdown as stop_opencode;
pub use opencode_run::shutdown_on as stop_opencode_on;
pub use opencode_run::reload_if_idle as reload_opencode_if_idle;
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
/// The thinking level of `available` nearest `want` (helpers are given one
/// level for every agent; a model names its own): `want` itself when it's
/// there or nothing's known.
pub fn nearest_level(want: &str, available: &[String]) -> String {
    const ORDER: [&str; 8] = ["off", "minimal", "low", "medium", "high", "xhigh", "max", "maximum"];
    let rank = |l: &str| ORDER.iter().position(|o| o.eq_ignore_ascii_case(l));
    let known: Vec<&String> = available.iter().filter(|l| rank(l).is_some()).collect();
    let Some(w) = rank(want) else { return want.to_owned() };
    if known.is_empty() || available.iter().any(|l| l.eq_ignore_ascii_case(want)) {
        return want.to_owned();
    }
    known.into_iter().min_by_key(|l| (rank(l).unwrap() as i32 - w as i32).abs() * 2 + i32::from(rank(l).unwrap() < w)).map_or_else(|| want.to_owned(), |l| l.to_string())
}

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

/// Before a session starts on a remote host: what harlo forwards there
/// (this computer's model servers, `harlo_core::model_servers`).
pub type RemoteStart = fn(&str);
static REMOTE_START: std::sync::OnceLock<RemoteStart> = std::sync::OnceLock::new();

pub fn set_remote_start(f: RemoteStart) {
    let _ = REMOTE_START.set(f);
}

/// The bridge an agent runs for harlo's tools, here or on its host, while
/// orchestration is on. (Called as each session starts: on a host, what
/// [`set_remote_start`] set runs first.)
pub(crate) fn mcp_bridge_for(remote: Option<&RemoteLaunch>) -> Option<Vec<String>> {
    if let (Some(r), Some(start)) = (remote, REMOTE_START.get()) {
        start(&r.host);
    }
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
    fn a_thinking_level_maps_to_the_nearest_a_model_has() {
        use super::nearest_level;
        let v = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(nearest_level("high", &v(&["low", "medium", "high"])), "high");
        assert_eq!(nearest_level("high", &v(&["low", "max"])), "max");
        assert_eq!(nearest_level("medium", &v(&["low", "high"])), "high");
        assert_eq!(nearest_level("low", &v(&["minimal", "high"])), "minimal");
        assert_eq!(nearest_level("high", &v(&[])), "high");
        assert_eq!(nearest_level("high", &v(&["fast", "smart"])), "high");
    }

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
