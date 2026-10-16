//! Settings → Providers: whether each agent (Claude, OpenCode, Pi, Oh My
//! Pi) is installed, its version and its models. Checked on demand: Claude's
//! models are a curated list, OpenCode's come from a private server, pi's
//! and omp's from a short RPC run with no session (no prompt is sent, so
//! nothing is spent).

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

/// "2.1.296 (Claude Code)", "opencode v2.0.25", "omp/18.8.9" → the number.
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
/// tokio runtime for pi and omp).
pub fn check(kind: HarnessKind, configured: &str, runtime: &tokio::runtime::Handle) -> Status {
    let Some(program) = program(kind, configured) else {
        return Status { error: (!configured.trim().is_empty()).then(|| format!("{configured} isn't there")), ..Default::default() };
    };
    let version = version(&program);
    let mut status = Status { program: Some(program.clone()), version: version.clone(), ..Default::default() };
    let scratch = std::env::temp_dir();
    let models = match kind {
        HarnessKind::Claude => Ok(harlo_harness::claude::models(version.as_deref(), harlo_harness::claude::config_dir().map(|d| d.join("settings.json")).as_deref())),
        HarnessKind::OpenCode => match version.as_deref().and_then(harlo_harness::opencode::version_number) {
            Some(v) if !harlo_harness::opencode::supported(&v) => Err(format!("OpenCode {v} is too old: harlo needs 2.0.20 or later")),
            _ => harlo_harness::opencode::Server::start(&program, &scratch)
                .and_then(|server| harlo_harness::opencode::models(&server, &scratch))
                .map_err(|e| format!("{e:#}")),
        },
        HarnessKind::Pi | HarnessKind::Omp => runtime.block_on(rpc_models(kind, program.clone(), scratch)),
        HarnessKind::Demo => Ok(Vec::new()),
    };
    match models {
        Ok(m) => status.models = m,
        Err(e) => status.error = Some(e),
    }
    // Claude's models only when Claude Code is signed in (its own answer;
    // one it can't give leaves them).
    if kind == HarnessKind::Claude && claude_signed_out(&program) {
        status.models.clear();
        status.error = Some("Not signed in. Sign in under Accounts.".to_owned());
    }
    status.version = status.version.as_deref().map(version_number);
    status
}

/// `claude auth status` says it isn't signed in (an API key in the
/// environment counts as signed in).
fn claude_signed_out(program: &Path) -> bool {
    let Ok(mut child) = std::process::Command::new(program)
        .args(["auth", "status", "--json"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while child.try_wait().ok().flatten().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    let mut out = String::new();
    use std::io::Read;
    let _ = child.stdout.take().map(|mut o| o.read_to_string(&mut out));
    serde_json::from_str::<Value>(&out).is_ok_and(|v| v["loggedIn"].as_bool() == Some(false))
}

/// pi / omp in RPC mode with no session: their model list.
async fn rpc_models(kind: HarnessKind, program: PathBuf, cwd: PathBuf) -> Result<Vec<(String, String)>, String> {
    let cfg = HarnessConfig { kind, program: Some(program), cwd, session: SessionTarget::Ephemeral, extra_args: vec![], env: vec![], remote: None };
    let (harness, _events) = Harness::spawn(cfg).map_err(|e| format!("{e:#}"))?;
    let answer = tokio::time::timeout(Duration::from_secs(25), harness.request(json!({ "type": "get_available_models" }))).await;
    harness.kill();
    let data: Value = match answer {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => return Err(format!("{e:#}")),
        Err(_) => return Err("it didn't answer in time".to_owned()),
    };
    Ok(data["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            let provider = m["provider"].as_str().unwrap_or_default();
            let full = if provider.is_empty() { id.to_owned() } else { format!("{provider}/{id}") };
            Some((full, m["name"].as_str().unwrap_or(id).to_owned()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_numbers() {
        assert_eq!(super::version_number("2.1.296 (Claude Code)"), "2.1.296");
        assert_eq!(super::version_number("opencode v2.0.25"), "2.0.25");
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
    #[ignore]
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
