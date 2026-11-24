//! Following Harlo's server on another computer over SSH.
//!
//! Harlo reads that computer's server registration, starts the
//! server there when none runs (as on this computer; it never stops it),
//! and opens a tunnel from a local port to it.
//!
//! It runs the system's `ssh`, so the user's SSH config, keys, agent and
//! jump hosts apply as they do in a terminal. A computer is given as what
//! follows `ssh` on the command line: `buildbox`, `me@10.0.0.5`, or
//! `-p 2222 me@host`.

// Rust guideline compliant 2026-10-09

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use harlo_client::{SharedService, parse_registration};

/// Returns the SSH config Harlo offers hosts from: `~/.ssh/config`.
#[must_use]
pub fn config_file() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}

/// Returns the hosts the SSH config at `path` names, in order, each once.
///
/// Patterns with wildcards (`*`, `?`) and negated ones name no single
/// computer and are left out. `Include` lines are followed, relative to
/// the folder `path` sits in. A missing or unreadable file names none.
#[must_use]
pub fn config_hosts(path: &Path) -> Vec<String> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut config = ssh2_config::SshConfig::default();
    // Includes resolve against the home holding `.ssh`.
    if let Some(home) = path.parent().and_then(Path::parent) {
        config = config.home_dir(home);
    }
    let rules = ssh2_config::ParseRule::ALLOW_UNKNOWN_FIELDS
        | ssh2_config::ParseRule::ALLOW_UNSUPPORTED_FIELDS;
    let Ok(config) = config.parse(&mut BufReader::new(file), rules) else {
        return Vec::new();
    };
    let mut hosts: Vec<String> = Vec::new();
    for clause in config.get_hosts().iter().flat_map(|h| &h.pattern) {
        let name = &clause.pattern;
        if clause.negated || name.contains(['*', '?']) || hosts.contains(name) {
            continue;
        }
        hosts.push(name.clone());
    }
    hosts
}

/// How long the tunnel may take to open.
const TUNNEL_WAIT: Duration = Duration::from_secs(20);

/// How long reading the registration or starting the service may take.
const COMMAND_WAIT: Duration = Duration::from_secs(60);

/// The options every `ssh` run gets: never ask in a terminal Harlo doesn't
/// have, and notice a dead connection.
const OPTIONS: [&str; 6] = [
    "-o",
    "BatchMode=yes",
    "-o",
    "ServerAliveInterval=15",
    "-o",
    "ServerAliveCountMax=3",
];

/// Reads the registration of `${XDG_STATE_HOME:-~/.local/state}/harlo/service.json`.
const READ_REGISTRATION: &str =
    r#"cat "${XDG_STATE_HOME:-$HOME/.local/state}/harlo/service.json" 2>/dev/null || true"#;

/// Starts Harlo's server, finding the program where its installer puts it
/// too, as a non-interactive SSH session's `PATH` may lack it.
const START_COMMAND: &str =
    r#"PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"; harlo-server service start"#;

/// `target`'s words, as `ssh` takes them.
///
/// # Errors
///
/// When `target` is empty or its quotes don't close.
pub fn words(target: &str) -> Result<Vec<String>, String> {
    let words = shell_words::split(target).map_err(|e| format!("Couldn't read that: {e}."))?;
    if words.is_empty() {
        return Err("Give a computer to reach, as for ssh.".to_owned());
    }
    Ok(words)
}

/// The computer `target` names: its last word, such as `me@host`.
#[must_use]
pub fn destination(target: &str) -> String {
    shell_words::split(target)
        .ok()
        .and_then(|words| words.last().cloned())
        .unwrap_or_else(|| target.trim().to_owned())
}

/// Runs `command` on `target` with the shell there; gives what it printed.
async fn run(target: &str, command: &str) -> Result<String, String> {
    let mut ssh = tokio::process::Command::new("ssh");
    ssh.args(OPTIONS)
        .args(words(target)?)
        .arg("--")
        .arg(format!("sh -c '{}'", command.replace('\'', r"'\''")))
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(COMMAND_WAIT, ssh.output())
        .await
        .map_err(|_elapsed| format!("{} didn't answer in time.", destination(target)))?
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "There's no ssh on this computer.".to_owned(),
            _ => format!("Couldn't run ssh: {e}"),
        })?;
    // 255 is ssh's own failure; anything else is the command's.
    if output.status.code() == Some(255) {
        return Err(ssh_error(&output.stderr, target));
    }
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "On {}: {}",
            destination(target),
            said.trim().lines().last().unwrap_or("it failed")
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Why `ssh` couldn't reach `target`, from what it printed.
fn ssh_error(stderr: &[u8], target: &str) -> String {
    let said = String::from_utf8_lossy(stderr);
    let line = said
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty() && !l.starts_with("Warning: Permanently added"))
        .unwrap_or("it gave no reason");
    format!("Couldn't reach {} over SSH: {line}", destination(target))
}

/// Harlo's server registered on `target`, if any.
///
/// # Errors
///
/// When `target` can't be reached, or its registration doesn't read.
pub async fn registered(target: &str) -> Result<Option<SharedService>, String> {
    let text = run(target, READ_REGISTRATION).await?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    parse_registration(&text)
        .map(Some)
        .map_err(|e| format!("{}'s service.json doesn't read: {e}", destination(target)))
}

/// Starts Harlo's server on `target` when none runs.
///
/// # Errors
///
/// When `target` can't be reached or the service doesn't start.
pub async fn start_service(target: &str) -> Result<(), String> {
    run(target, START_COMMAND).await.map(|_| ())
}

/// A tunnel from a port on this computer to a server on another; it closes
/// when dropped.
#[derive(Debug)]
pub struct Tunnel {
    /// The `ssh` that holds it open.
    ssh: tokio::process::Child,
    /// The local end, such as `http://127.0.0.1:50123`.
    url: String,
}

impl Tunnel {
    /// Opens a tunnel over `target` to `url`, an address as seen on that
    /// computer (such as `http://127.0.0.1:4096`), and waits until its
    /// local end takes connections.
    ///
    /// # Errors
    ///
    /// When `url` has no host and port, or `ssh` can't open the tunnel.
    pub async fn open(target: &str, url: &str) -> Result<Self, String> {
        let remote = url
            .split_once("://")
            .map_or(url, |(_, rest)| rest)
            .split('/')
            .next()
            .unwrap_or_default();
        if remote.rsplit_once(':').is_none() {
            return Err(format!(
                "The service on {} has no port: {url}",
                destination(target)
            ));
        }
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .map_err(|e| format!("No free port for the tunnel: {e}"))?;
        let mut ssh = tokio::process::Command::new("ssh")
            .args(OPTIONS)
            .args(["-o", "ExitOnForwardFailure=yes", "-N", "-L"])
            .arg(format!("127.0.0.1:{port}:{remote}"))
            .args(words(target)?)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Couldn't run ssh: {e}"))?;
        let started = Instant::now();
        loop {
            if let Some(status) = ssh.try_wait().map_err(|e| e.to_string())? {
                let mut said = Vec::new();
                if let Some(mut stderr) = ssh.stderr.take() {
                    let _ = tokio::io::AsyncReadExt::read_to_end(&mut stderr, &mut said).await;
                }
                return Err(if said.is_empty() {
                    format!("The tunnel to {} closed ({status}).", destination(target))
                } else {
                    ssh_error(&said, target)
                });
            }
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                return Ok(Self {
                    ssh,
                    url: format!("http://127.0.0.1:{port}"),
                });
            }
            if started.elapsed() > TUNNEL_WAIT {
                return Err(format!(
                    "The tunnel to {} didn't open in time.",
                    destination(target)
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The local end, such as `http://127.0.0.1:50123`.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Whether `ssh` has ended, so the tunnel is gone.
    pub fn closed(&mut self) -> bool {
        !matches!(self.ssh.try_wait(), Ok(None))
    }
}
