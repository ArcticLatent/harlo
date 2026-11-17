//! Harlo's sessions on another machine.
//!
//! harlo puts its agent there (a static build, so it runs whatever C library
//! the machine has; once per build of it) and runs it over SSH as the other
//! agents run there. Its sign-in, settings and sessions are that machine's own,
//! in harlo-agent's folders there.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use harlo_harness::HarnessKind;
use serde_json::Value;

use crate::remote::{self, Host, HostInfo};
use crate::remote_helper::{ARCHES, Helper};

static AGENT: Helper = Helper::new("harlo-agent", true, "agent", "--version");

/// Whether Harlo's sessions on other machines get this computer's provider
/// keys (Settings → Models).
static SHARE_KEYS: AtomicBool = AtomicBool::new(true);

/// The keys each machine was last given, by a hash of them (given again
/// only when they change).
static SHARED: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

/// harlo-agent on `host_name`, put there first if this build of it isn't.
///
/// Its path there (blocking).
///
/// # Errors
///
/// When the machine can't be reached, there's no build for its kind, or it
/// can't be written there.
pub fn program_on(host_name: &str) -> Result<String> {
    let host = remote::host(host_name).with_context(|| format!("unknown machine {host_name}"))?;
    AGENT.on(&host)
}

/// Why Harlo can't run on a machine its check found (None: it can).
pub fn not_there(info: &HostInfo) -> Option<String> {
    if info.os != "Linux" {
        return Some(format!("Harlo runs on Linux machines, not {} ones, so far.", if info.os.is_empty() { "these" } else { &info.os }));
    }
    if !ARCHES.contains(&info.arch.as_str()) {
        return Some(format!("Harlo runs on x86_64 and ARM (aarch64) machines, not {} ones.", info.arch));
    }
    AGENT.local(&info.arch).is_none().then(|| format!("This harlo has no copy of its agent for {} machines (harlo's releases have one).", info.arch))
}

/// For harlo-harness: the program a session of `kind` runs on `host_name`
/// (Harlo's: the copy put there; the others': None, that machine's PATH
/// finds them).
///
/// # Errors
///
/// Why Harlo's agent can't be put on that machine.
pub fn remote_program(host_name: &str, kind: HarnessKind) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    if kind != HarnessKind::Harlo {
        return Ok(None);
    }
    let program = program_on(host_name).with_context(|| format!("Harlo can't start on {host_name}"))?;
    if SHARE_KEYS.load(Ordering::Relaxed)
        && let Some(host) = remote::host(host_name)
        && let Err(e) = share_keys(&host, &program, &local_keys())
    {
        tracing::event!(name: "agent_host.keys.failed", tracing::Level::WARN, server.address = %host_name, error.message = %format_args!("{e:#}"), "provider keys to {{server.address}}: {{error.message}}");
    }
    Ok(Some(program))
}

/// Turns giving other machines this computer's provider keys on or off.
pub fn set_share_keys(on: bool) {
    SHARE_KEYS.store(on, Ordering::Relaxed);
}

/// The provider keys saved on this computer (harlo-agent's `keys.json`).
fn local_keys() -> BTreeMap<String, String> {
    let Some(file) = crate::model_servers::file(HarnessKind::Harlo).map(|f| f.with_file_name("keys.json")) else { return BTreeMap::new() };
    std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// The keys of `local` that the machine's agent has none for, from its
/// `providers --json` (a provider it has its own key for keeps it).
fn keys_to_share(local: &BTreeMap<String, String>, there: &Value) -> Vec<(String, String)> {
    there
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["key"] == "none" && p["replaced"] != true)
        .filter_map(|p| {
            let id = p["id"].as_str()?;
            local.get(id).filter(|k| !k.trim().is_empty()).map(|k| (id.to_owned(), k.clone()))
        })
        .collect()
}

/// Gives the machine's agent the keys of `local` (this computer's) it
/// lacks, each into its own private file there (`key set`, the key on its
/// input, so it shows in no command line). Once per change of them.
fn share_keys(host: &Host, program: &str, local: &BTreeMap<String, String>) -> Result<()> {
    use std::hash::{Hash, Hasher};
    if local.is_empty() {
        return Ok(());
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    local.hash(&mut hasher);
    let hash = hasher.finish();
    if SHARED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().and_then(|m| m.get(&host.name)) == Some(&hash) {
        return Ok(());
    }
    let agent = remote::quote(program);
    let listed = remote::run_ok(host, &format!("{agent} providers --json </dev/null"), Duration::from_secs(30))?;
    let there: Value = serde_json::from_str(listed.trim()).context("its agent didn't list its providers")?;
    for (id, key) in keys_to_share(local, &there) {
        remote::run_ok_with_input(host, &format!("{agent} key set {} >/dev/null", remote::quote(&id)), key.as_bytes(), Duration::from_secs(30))?;
    }
    SHARED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_or_insert_with(HashMap::new).insert(host.name.clone(), hash);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_gets_the_keys_it_lacks() {
        let local: BTreeMap<String, String> = [("anthropic", "sk-ant"), ("gemini", "AIza"), ("xai", "xai-1"), ("deepseek", "sk-ds")].iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect();
        let there = serde_json::json!([
            { "id": "anthropic", "key": "none" },
            // Its own key (saved or in its environment) stays.
            { "id": "gemini", "key": "saved" },
            { "id": "xai", "key": "environment" },
            // One its providers.json sets up isn't the preset any more.
            { "id": "deepseek", "key": "none", "replaced": true },
            { "id": "mistral", "key": "none" },
        ]);
        assert_eq!(keys_to_share(&local, &there), [("anthropic".to_owned(), "sk-ant".to_owned())]);
    }

    #[test]
    fn which_machines_get_harlo() {
        let info = |os: &str, arch: &str| HostInfo { os: os.into(), arch: arch.into(), ..Default::default() };
        assert!(not_there(&info("Darwin", "arm64")).unwrap().contains("Linux"));
        assert!(not_there(&info("Linux", "armv7l")).unwrap().contains("armv7l"));
        // (Whether this build has the copy depends on what's been built.)
        let here = not_there(&info("Linux", std::env::consts::ARCH));
        assert_eq!(here.is_none(), AGENT.local(std::env::consts::ARCH).is_some());
    }

    /// harlo-agent on a real SSH server (not run by default; a static build
    /// first: `cargo build -p harlo-agent --target x86_64-unknown-linux-musl`):
    /// `HARLO_TEST_SSH="$(scripts/test-sshd.sh start /tmp/x)" cargo test -p harlo-core -- --ignored harlo_agent_over_ssh`.
    /// Its settings, sign-in and sessions there are a scratch folder's (set
    /// through the SSH environment); harlo's cache there gets the copy.
    #[test]
    #[ignore = "needs a real SSH host"]
    fn harlo_agent_over_ssh() {
        use std::time::Duration;
        let dir = crate::scratch::Scratch::new(std::env::temp_dir().join(format!("harlo-agent-host-{}", std::process::id())));
        let args = remote::parse_ssh_command(&std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH")).unwrap();
        let mut ssh = vec!["-o".to_owned(), format!("SetEnv=XDG_CONFIG_HOME={0}/config XDG_DATA_HOME={0}/data", dir.display())];
        ssh.extend(args);
        let host = remote::Host { name: "t".into(), ssh };
        remote::set_hosts(vec![host.clone()]);
        let info = remote::check(&host).unwrap();
        assert_eq!(not_there(&info), None, "{info:?}");
        assert!(info.has("harlo"));

        // Put there, and run.
        let started = std::time::Instant::now();
        let path = program_on("t").unwrap();
        println!("put there in {:?}: {path}", started.elapsed());
        assert!(path.contains("/.cache/harlo/harlo-agent-"), "{path}");
        let version = remote::run_ok(&host, &format!("{} --version", remote::quote(&path)), Duration::from_secs(30)).unwrap();
        assert!(version.starts_with("harlo-agent "), "{version}");
        // Again: the same, not sent again.
        let when = || std::fs::metadata(&path).unwrap().modified().unwrap();
        let before = when();
        let started = std::time::Instant::now();
        assert_eq!(program_on("t").unwrap(), path);
        println!("again in {:?}", started.elapsed());
        assert_eq!(when(), before);

        // A session there, started as harlo starts one (this computer's
        // own keys stay here: they're given below, stand-ins).
        set_share_keys(false);
        harlo_harness::set_remote_program(remote_program);
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let cfg = harlo_harness::HarnessConfig {
                kind: HarnessKind::Harlo,
                program: None,
                cwd: remote::join("t", &proj.to_string_lossy()),
                session: harlo_harness::SessionTarget::Ephemeral,
                extra_args: vec![],
                env: vec![],
                remote: Some(remote::launch(&host)),
            };
            let (harness, _events) = harlo_harness::Harness::spawn(cfg).unwrap();
            let state = tokio::time::timeout(Duration::from_secs(30), harness.request(serde_json::json!({ "type": "get_state" }))).await.unwrap().unwrap();
            assert!(state.get("model").is_some(), "{state}");
            // (Its sign-in there is the scratch folder's: none.)
            let models = tokio::time::timeout(Duration::from_secs(30), harness.request(serde_json::json!({ "type": "get_available_models" }))).await.unwrap();
            assert!(models.as_ref().is_err_and(|e| e.to_string().contains("Not signed in")), "{models:?}");
            harness.kill();
        });

        // Keys from here: the ones it lacks, in its own private file; one it
        // has stays its own; given once per change.
        let keys_there = dir.join("config/harlo-agent/keys.json");
        std::fs::create_dir_all(keys_there.parent().unwrap()).unwrap();
        std::fs::write(&keys_there, r#"{"gemini":"AIza-its-own"}"#).unwrap();
        let here: BTreeMap<String, String> = [("anthropic", "sk-ant-here"), ("gemini", "AIza-here")].iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect();
        share_keys(&host, &path, &here).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&keys_there).unwrap()).unwrap();
        assert_eq!(saved, serde_json::json!({ "anthropic": "sk-ant-here", "gemini": "AIza-its-own" }));
        let mode = { use std::os::unix::fs::PermissionsExt; std::fs::metadata(&keys_there).unwrap().permissions().mode() & 0o777 };
        assert_eq!(mode, 0o600, "only its user reads it");
        std::fs::remove_file(&keys_there).unwrap();
        share_keys(&host, &path, &here).unwrap();
        assert!(!keys_there.exists(), "the same keys aren't given twice");

        // Its saved sessions there, in the sidebar's listing.
        let sessions = dir.join("data/harlo-agent/sessions/--proj--");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("s.jsonl"),
            format!("{{\"type\":\"session\",\"id\":\"s1\",\"cwd\":\"{}\",\"timestamp\":\"2026-01-01T00:00:00Z\"}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"on the host\"}}]}}}}\n", proj.display()),
        )
        .unwrap();
        let delta = crate::remote_sessions::RemoteScanner::default().poll(&host).unwrap();
        let found = delta.upserts.iter().find(|i| i.path.ends_with("--proj--/s.jsonl")).unwrap_or_else(|| panic!("{:?}", delta.upserts.iter().map(|i| &i.path).collect::<Vec<_>>()));
        assert_eq!(found.harness, HarnessKind::Harlo);
        remote::disconnect(&host);
    }

    /// The same on a real machine (not run by default): `HARLO_TEST_HOST=<ssh
    /// arguments, e.g. an alias> cargo test -p harlo-core -- --ignored harlo_agent_on_a_host`.
    /// harlo-agent's settings, sign-in and sessions there are a scratch
    /// folder's in its /tmp, removed after; harlo's cache there gets the copy.
    #[test]
    #[ignore = "needs a real SSH host"]
    fn harlo_agent_on_a_host() {
        use std::time::Duration;
        let ssh = remote::parse_ssh_command(&std::env::var("HARLO_TEST_HOST").expect("HARLO_TEST_HOST")).unwrap();
        let host = remote::Host { name: "t".into(), ssh };
        remote::set_hosts(vec![host.clone()]);
        let info = remote::check(&host).unwrap();
        println!("{} {}", info.os, info.arch);
        assert_eq!(not_there(&info), None);
        let started = std::time::Instant::now();
        let path = program_on("t").unwrap();
        println!("there in {:?}: {path}", started.elapsed());
        let started = std::time::Instant::now();
        assert_eq!(program_on("t").unwrap(), path);
        println!("again in {:?}", started.elapsed());
        let version = remote::run_ok(&host, &format!("{} --version", remote::quote(&path)), Duration::from_secs(30)).unwrap();
        println!("{}", version.trim());
        assert!(version.starts_with("harlo-agent "));

        let scratch = remote::run_ok(&host, "mktemp -d /tmp/harlo-m5.XXXXXX", Duration::from_secs(30)).unwrap().trim().to_owned();
        assert!(scratch.starts_with("/tmp/harlo-m5."), "{scratch}");
        remote::run_ok(&host, &format!("mkdir {}", remote::quote(&format!("{scratch}/proj"))), Duration::from_secs(30)).unwrap();
        harlo_harness::set_remote_program(remote_program);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let outcome = runtime.block_on(async {
            let cfg = harlo_harness::HarnessConfig {
                kind: HarnessKind::Harlo,
                program: None,
                cwd: remote::join("t", &format!("{scratch}/proj")),
                session: harlo_harness::SessionTarget::Ephemeral,
                extra_args: vec![],
                env: vec![("XDG_CONFIG_HOME".into(), format!("{scratch}/config")), ("XDG_DATA_HOME".into(), format!("{scratch}/data"))],
                remote: Some(remote::launch(&host)),
            };
            let (harness, _events) = harlo_harness::Harness::spawn(cfg).unwrap();
            let state = tokio::time::timeout(Duration::from_secs(30), harness.request(serde_json::json!({ "type": "get_state" }))).await;
            let models = tokio::time::timeout(Duration::from_secs(30), harness.request(serde_json::json!({ "type": "get_available_models" }))).await;
            harness.kill();
            (state, models)
        });
        remote::run_ok(&host, &format!("rm -rf {}", remote::quote(&scratch)), Duration::from_secs(30)).unwrap();
        let (state, models) = outcome;
        let state = state.unwrap().unwrap();
        println!("state: model {}, cwd {}", state["model"], state["cwd"]);
        assert!(state.get("model").is_some(), "{state}");
        assert!(models.unwrap().is_err_and(|e| e.to_string().contains("Not signed in")));
        remote::disconnect(&host);
    }
}
