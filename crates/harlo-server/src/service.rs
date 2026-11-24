//! The background service: one `harlo-server` per person on a computer,
//! started by whichever client needs it first and left running.
//!
//! A running service records where it listens and its password in
//! `$XDG_STATE_HOME/harlo/service.json` (or `~/.local/state/harlo/…`),
//! readable only by its owner; clients find it there.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long `service start` waits for a new service to answer.
const START_WAIT: Duration = Duration::from_secs(15);

/// A running service, as its registration records it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registration {
    /// Its base URL, such as `http://127.0.0.1:41234`.
    pub url: String,
    /// The password clients send.
    pub password: Option<String>,
    /// The server's version.
    pub version: String,
    /// Its process ID.
    pub pid: u32,
}

/// The folder the service keeps its registration and log in.
#[must_use]
pub fn state_folder() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map_or_else(
            || dirs::home_dir().unwrap_or_default().join(".local/state"),
            PathBuf::from,
        )
        .join("harlo")
}

/// The registration file.
#[must_use]
pub fn registration_path() -> PathBuf {
    state_folder().join("service.json")
}

/// Records this process as the running service.
///
/// # Errors
///
/// When the file can't be written.
pub fn register(registration: &Registration) -> Result<(), String> {
    let path = registration_path();
    let folder = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(folder)
        .map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
    let text = serde_json::to_string_pretty(registration)
        .map_err(|e| format!("Couldn't write the registration: {e}"))?;
    // Written beside and renamed, readable by its owner only: it holds the password.
    let partial = folder.join(format!("service.json.{}", std::process::id()));
    write_private(&partial, &text)
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

/// Writes `text` to `path`, readable and writable by its owner only.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())
}

/// Removes the registration if it's still this process's.
pub fn unregister() {
    let path = registration_path();
    if read().is_some_and(|r| r.pid == std::process::id()) {
        let _ = std::fs::remove_file(path);
    }
}

/// The registration, if there's one that reads.
#[must_use]
pub fn read() -> Option<Registration> {
    let text = std::fs::read_to_string(registration_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// Whether the service `registration` names answers.
async fn answers(registration: &Registration) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    else {
        return false;
    };
    client
        .get(format!("{}/api/info", registration.url))
        .basic_auth("harlo", registration.password.as_deref())
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}

/// The running service, if one answers.
pub async fn running() -> Option<Registration> {
    let registration = read()?;
    answers(&registration).await.then_some(registration)
}

/// A new random password.
fn password() -> Result<String, String> {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| format!("No randomness for a password: {e}"))?;
    Ok(bytes.iter().fold(String::new(), |mut text, b| {
        let _ = write!(text, "{b:02x}");
        text
    }))
}

/// Starts the service unless one already answers, and returns it.
///
/// The new service runs on its own, in its own process group, with a
/// random port and password; its output goes to `server.log` beside the
/// registration.
///
/// # Errors
///
/// When it can't be started, or doesn't answer in time.
pub async fn start() -> Result<Registration, String> {
    if let Some(registration) = running().await {
        return Ok(registration);
    }
    let program =
        std::env::current_exe().map_err(|e| format!("Couldn't find harlo-server itself: {e}"))?;
    let folder = state_folder();
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(folder.join("server.log"))
        .map_err(|e| format!("Couldn't open the log: {e}"))?;
    let errors = log
        .try_clone()
        .map_err(|e| format!("Couldn't open the log: {e}"))?;
    let mut child = {
        use std::os::unix::process::CommandExt as _;
        std::process::Command::new(program)
            .args(["serve", "--port", "0", "--register", "--password-stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(log)
            .stderr(errors)
            .process_group(0)
            .spawn()
            .map_err(|e| format!("Couldn't start the service: {e}"))?
    };
    // The password goes down a pipe, not into the service's environment,
    // where any program of this user could read it for as long as it runs.
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| "Couldn't reach the service to start it.".to_owned())?;
    std::io::Write::write_all(&mut input, format!("{}\n", password()?).as_bytes())
        .map_err(|e| format!("Couldn't start the service: {e}"))?;
    drop(input);
    let pid = child.id();
    // Dropping the handle leaves the process running, which is the aim.
    drop(child);
    let deadline = tokio::time::Instant::now() + START_WAIT;
    while tokio::time::Instant::now() < deadline {
        if let Some(registration) = read().filter(|r| r.pid == pid)
            && answers(&registration).await
        {
            return Ok(registration);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "The service didn't answer within {} s; see {}",
        START_WAIT.as_secs(),
        folder.join("server.log").display()
    ))
}
