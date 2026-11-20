//! Settings → Agents: each agent's install, version and models.
//!
//! Whether pi is installed. Checked on demand: its models
//! come from a short RPC run with no session (no prompt is sent, so nothing is
//! spent).

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::time::Duration;

use harlo_harness::{Harness, HarnessConfig, HarnessKind, SessionTarget};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub program: Option<PathBuf>,
    pub version: Option<String>,
    /// `(id, name)`; ids as the harness takes them.
    pub models: Vec<(String, String)>,
    /// The ids of models served on this computer or the local network.
    pub local: Vec<String>,
    pub error: Option<String>,
}

/// The program to run: the configured one, else the one found.
pub fn program(kind: HarnessKind, configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        return p.is_file().then_some(p);
    }
    harlo_harness::resolve_program(kind)
}

/// "2.1.296 (Claude Code)", "harlo-agent v2.0.25", "omp/18.8.9" → the number.
pub fn version_number(line: &str) -> String {
    line.split(|c: char| c.is_whitespace() || c == '/')
        .map(|w| w.trim_start_matches('v'))
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(line)
        .to_owned()
}

/// `program --version`'s first line (5 s at most).
pub fn version(program: &Path) -> Option<String> {
    let mut child = std::process::Command::new(program)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(30)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    use std::io::Read;
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    out.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_owned)
}

/// Checks one provider (blocking; run it off the UI thread, inside the
/// tokio runtime for its harness).
pub fn check(kind: HarnessKind, configured: &str, runtime: &tokio::runtime::Handle) -> Status {
    let Some(program) = program(kind, configured) else {
        return Status { error: (!configured.trim().is_empty()).then(|| format!("{configured} isn't there")), ..Default::default() };
    };
    let version = version(&program);
    let mut status = Status { program: Some(program.clone()), version: version.clone(), ..Default::default() };
    let scratch = std::env::temp_dir();
    let models = match kind {
        HarnessKind::Pi => runtime.block_on(rpc_models(kind, program.clone(), scratch)),
        HarnessKind::Demo => Ok(Vec::new()),
    };
    match models {
        Ok(m) => {
            status.local = m.iter().filter(|(_, _, local)| *local).map(|(id, _, _)| id.clone()).collect();
            status.models = m.into_iter().map(|(id, name, _)| (id, name)).collect();
        }
        Err(e) => status.error = Some(format!("{e:#}")),
    }
    status.version = status.version.as_deref().map(version_number);
    status
}

/// The agent in RPC mode with no session: its model list.
async fn rpc_models(kind: HarnessKind, program: PathBuf, cwd: PathBuf) -> anyhow::Result<Vec<(String, String, bool)>> {
    models_of(HarnessConfig { kind, program: Some(program), cwd, session: SessionTarget::Ephemeral, extra_args: vec![], env: vec![], remote: None }).await
}

/// What an agent offers on a remote host, asked there with no session (it
/// has its own logins). Each entry: `provider/id`, name, served locally.
///
/// # Errors
///
/// When the host can't be reached, or the agent there can't be started or
/// doesn't answer.
pub async fn host_models(kind: HarnessKind, remote: harlo_harness::RemoteLaunch, cwd: PathBuf) -> anyhow::Result<Vec<(String, String, bool)>> {
    models_of(HarnessConfig { kind, program: None, cwd, session: SessionTarget::Ephemeral, extra_args: vec![], env: vec![], remote: Some(remote) }).await
}

async fn models_of(cfg: HarnessConfig) -> anyhow::Result<Vec<(String, String, bool)>> {
    let (harness, _events) = Harness::spawn(cfg).map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let answer = tokio::time::timeout(Duration::from_secs(25), harness.request(json!({ "type": "get_available_models" }))).await;
    harness.kill();
    let data: Value = match answer {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => return Err(anyhow::anyhow!("{e:#}")),
        Err(_) => return Err(anyhow::Error::msg("it didn't answer in time")),
    };
    Ok(data["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            let provider = m["provider"].as_str().unwrap_or_default();
            let full = if provider.is_empty() { id.to_owned() } else { format!("{provider}/{id}") };
            Some((full, m["name"].as_str().unwrap_or(id).to_owned(), m["baseUrl"].as_str().is_some_and(harlo_harness::is_local_url)))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_numbers() {
        assert_eq!(super::version_number("2.1.296 (Claude Code)"), "2.1.296");
        assert_eq!(super::version_number("harlo-agent v2.0.25"), "2.0.25");
        assert_eq!(super::version_number("omp/18.8.9"), "18.8.9");
        assert_eq!(super::version_number("1.0.4"), "1.0.4");
    }
}

#[cfg(test)]
mod this_computer {
    /// Every provider on this computer (no prompts are sent):
    /// `cargo test -p harlo-core providers -- --ignored --nocapture`
    /// (`HARLO_CHECK=pi:5`: only pi, five times).
    #[test]
    #[ignore = "spends real model plans"]
    fn checks_each() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let only = std::env::var("HARLO_CHECK").unwrap_or_default();
        let (only, times) = only.split_once(':').map_or((only.as_str(), 1), |(k, n)| (k, n.parse().unwrap_or(1)));
        for kind in harlo_harness::HarnessKind::PROVIDERS.into_iter().filter(|k| only.is_empty() || k.as_str() == only) {
            for _ in 0..times {
                let started = std::time::Instant::now();
                let s = super::check(kind, "", rt.handle());
                println!("{:9} {:?} v{:?} models={} err={:?} ({:?})", kind.as_str(), s.program, s.version, s.models.len(), s.error, started.elapsed());
            }
        }
    }
}
