//! Claude subscriptions through OpenCode's Claude Agent integration.
//!
//! OpenCode's `claude-agent` integration runs Claude Code (`claude`) through
//! Anthropic's Agent SDK, so a Claude subscription pays for its models. Claude
//! Code keeps the sign-in; OpenCode only records that it should use it.
//!
//! This module checks the Claude Code on this computer: whether it's
//! installed, which account it's signed in to (`claude auth status`), and how
//! much of the plan's limits is used ([`usage`], the answer Claude Code's own
//! `/usage` shows, asked over the stream protocol OpenCode also uses).
//! Connecting and disconnecting go through OpenCode's API
//! ([`Client::connect_external`](crate::server::Client::connect_external)).

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::Command;

/// OpenCode's integration for Claude subscriptions.
pub const INTEGRATION: &str = "claude-agent";

/// The integration's method that uses Claude Code's own sign-in.
pub const LOGIN_METHOD: &str = "claude-login";

/// The variable OpenCode reads for the `claude` program to run.
pub const PROGRAM_VARIABLE: &str = "OPENCODE_CLAUDE_PATH";

/// How long a `claude` check may take before it counts as failed.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

/// How long asking for the plan's usage may take; it goes to Claude's servers.
const USAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Variables of a Claude Code session Harlo may run inside, which a `claude`
/// it starts must not take as its own.
const SESSION_VARIABLES: [&str; 6] = [
    "CLAUDECODE",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
];

/// How often a check tries again while `claude` is busy being written.
const BUSY_TRIES: u32 = 20;

/// How long it waits between those tries.
const BUSY_PAUSE: Duration = Duration::from_millis(50);

/// The `claude` program OpenCode runs: the one [`PROGRAM_VARIABLE`] names,
/// else `claude` on the `PATH`.
#[must_use]
pub fn program() -> OsString {
    std::env::var_os(PROGRAM_VARIABLE)
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| OsString::from("claude"))
}

/// How Claude Code stands on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeCode {
    /// There's no `claude` to run.
    Missing,
    /// It's installed, at this version, and signed in as `login` says.
    Installed {
        /// Its version, such as `2.1.330`.
        version: String,
        /// Who it's signed in as.
        login: Login,
    },
    /// It's there but couldn't be checked.
    Failed(String),
}

/// Who Claude Code is signed in as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Login {
    /// Not signed in.
    SignedOut,
    /// Signed in with a Claude subscription, which pays for Claude Agent.
    Subscription(Account),
    /// Signed in some other way, such as an Anthropic Console account billed
    /// per use; `method` is Claude Code's name for it.
    Other {
        /// How it's signed in.
        method: String,
    },
}

/// The Claude account Claude Code is signed in to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Account {
    /// Its email address.
    pub email: Option<String>,
    /// Its organization's name.
    pub organization: Option<String>,
    /// Its plan, such as `max` or `pro`.
    pub plan: Option<String>,
}

/// `claude auth status`'s JSON, as far as Harlo reads it.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Status {
    logged_in: bool,
    auth_method: Option<String>,
    email: Option<String>,
    org_name: Option<String>,
    subscription_type: Option<String>,
}

impl Status {
    fn login(self) -> Login {
        let method = self.auth_method.unwrap_or_default();
        if !self.logged_in || method == "none" {
            return Login::SignedOut;
        }
        if method != "claude.ai" {
            return Login::Other { method };
        }
        let known = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
        Login::Subscription(Account {
            email: known(self.email),
            organization: known(self.org_name),
            plan: known(self.subscription_type),
        })
    }
}

/// Checks the Claude Code at `program`: its version, then who it's signed
/// in as.
pub async fn check(program: OsString) -> ClaudeCode {
    let version = match run(&program, &["--version"]).await {
        Ok(out) => out,
        Err(Run::Missing) => return ClaudeCode::Missing,
        Err(Run::Failed(why)) => return ClaudeCode::Failed(why),
    };
    let version = version
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    // It exits with an error while signed out, and still prints the status.
    let status = match run(&program, &["auth", "status", "--json"]).await {
        Ok(out) => out,
        Err(Run::Missing) => return ClaudeCode::Missing,
        Err(Run::Failed(why)) => return ClaudeCode::Failed(why),
    };
    match serde_json::from_str::<Status>(&status) {
        Ok(status) => ClaudeCode::Installed {
            version,
            login: status.login(),
        },
        Err(_) => ClaudeCode::Failed("Claude Code's sign-in status couldn't be read.".to_owned()),
    }
}

/// Why running `claude` didn't give an answer.
enum Run {
    Missing,
    Failed(String),
}

/// Runs `program` with `args`, returning what it printed, whether it
/// succeeded or not.
async fn run(program: &OsString, args: &[&str]) -> Result<String, Run> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut busy_tries = BUSY_TRIES;
    loop {
        let output = match tokio::time::timeout(CHECK_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Run::Missing);
            }
            // Still being written, as while it's installed or updated, or
            // open in a process another thread is starting: try again soon.
            Ok(Err(error))
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy && busy_tries > 0 =>
            {
                busy_tries -= 1;
                tokio::time::sleep(BUSY_PAUSE).await;
                continue;
            }
            Ok(Err(error)) => return Err(Run::Failed(error.to_string())),
            Err(_) => return Err(Run::Failed("Claude Code didn't answer.".to_owned())),
        };
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
}

/// One of the plan's usage limits, such as the current session's or the
/// week's, as Claude's servers report it.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    /// The kind of meter, such as `session`, `weekly_all` or `weekly_scoped`.
    pub kind: String,
    /// The group it's shown under, such as `session` or `weekly`.
    pub group: String,
    /// The model it's for, when it's for one, such as `Fable`.
    pub model: Option<String>,
    /// How much of it is used, from 0 to 100.
    pub percent: f64,
    /// When it resets, in seconds since the Unix epoch.
    pub resets_at: Option<i64>,
    /// How the servers grade it.
    pub severity: Severity,
    /// Whether it's the one to show when only one is shown.
    pub active: bool,
}

/// How Claude's servers grade a limit's use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Plenty left.
    Normal,
    /// Getting close.
    Warning,
    /// At or near the limit.
    Critical,
}

/// Asks the Claude Code at `program` how much of its plan's limits is used.
///
/// Returns `None` when there are no plan limits to show, as when it's signed
/// out or signed in without a subscription.
///
/// # Errors
///
/// Returns why when Claude Code can't be started, doesn't answer, or
/// answers with an error.
pub async fn usage(program: OsString) -> Result<Option<Vec<Limit>>, String> {
    let mut command = Command::new(&program);
    command
        .args([
            "--output-format",
            "stream-json",
            "--verbose",
            "--input-format",
            "stream-json",
            "--tools=",
            "--setting-sources=",
            "--strict-mcp-config",
            "--no-session-persistence",
            "--settings",
            r#"{"disableAllHooks":true}"#,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    for name in SESSION_VARIABLES {
        command.env_remove(name);
    }
    let mut child = spawn(&mut command).await.map_err(|e| e.to_string())?;
    let mut input = child.stdin.take().ok_or("Claude Code took no input")?;
    let output = child.stdout.take().ok_or("Claude Code gave no output")?;
    let asked = async {
        for request in [
            json!({"type": "control_request", "request_id": "init",
                   "request": {"subtype": "initialize"}}),
            json!({"type": "control_request", "request_id": "usage",
                   "request": {"subtype": "get_usage", "skip_behaviors": true}}),
        ] {
            let mut line = request.to_string();
            line.push('\n');
            input
                .write_all(line.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
        }
        input.flush().await.map_err(|e| e.to_string())?;
        let mut lines = BufReader::new(output).lines();
        while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let response = &message["response"];
            if message["type"] != "control_response" || response["request_id"] != "usage" {
                continue;
            }
            if response["subtype"] != "success" {
                let why = response["error"].as_str().unwrap_or("Claude Code refused");
                return Err(why.to_owned());
            }
            return Ok(limits(&response["response"]));
        }
        Err("Claude Code ended without answering.".to_owned())
    };
    let answer = tokio::time::timeout(USAGE_TIMEOUT, asked)
        .await
        .unwrap_or_else(|_| Err("Claude Code didn't answer.".to_owned()));
    // Closing its input ends it; dropping the child kills it if it lingers.
    drop(child);
    answer
}

/// The limits in a `get_usage` answer, or `None` when it has none.
fn limits(answer: &Value) -> Option<Vec<Limit>> {
    if answer["rate_limits_available"] == false {
        return None;
    }
    let rows = answer["rate_limits"]["limits"].as_array()?;
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    Some(
        rows.iter()
            .map(|row| Limit {
                kind: text(&row["kind"]),
                group: text(&row["group"]),
                model: row["scope"]["model"]["display_name"]
                    .as_str()
                    .map(str::to_owned),
                percent: row["percent"].as_f64().unwrap_or_default(),
                resets_at: row["resets_at"]
                    .as_str()
                    .and_then(|t| t.parse::<jiff::Timestamp>().ok())
                    .map(jiff::Timestamp::as_second),
                severity: match row["severity"].as_str() {
                    Some("critical") => Severity::Critical,
                    Some("warning") => Severity::Warning,
                    _ => Severity::Normal,
                },
                active: row["is_active"] == true,
            })
            .collect(),
    )
}

/// Starts `command`, trying again for a moment while its program is busy
/// being written, as while it's installed or updated.
async fn spawn(command: &mut Command) -> std::io::Result<tokio::process::Child> {
    let mut busy_tries = BUSY_TRIES;
    loop {
        match command.spawn() {
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy && busy_tries > 0 =>
            {
                busy_tries -= 1;
                tokio::time::sleep(BUSY_PAUSE).await;
            }
            started => return started,
        }
    }
}

/// How used a limit must be for its reset to be worth telling.
const NEARLY: f64 = 90.0;

/// Something about the plan's limits worth telling the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Alert {
    /// The limit is used up.
    UsedUp(Limit),
    /// The limit, nearly or fully used, has reset.
    Reset(Limit),
}

/// Follows the plan's limits from one reading to the next, and says what's
/// worth telling: a limit used up, and one nearly or fully used resetting.
/// Each is told once.
#[derive(Debug, Clone, Default)]
pub struct Watch {
    /// The limits nearly or fully used, by what they're for, until they reset.
    nearly: BTreeMap<(String, Option<String>), Limit>,
    /// The limits told as used up, by what they're for, while they are.
    used_up: BTreeSet<(String, Option<String>)>,
}

impl Watch {
    /// Takes a new reading at `now` (seconds since the Unix epoch).
    pub fn read(&mut self, limits: &[Limit], now: i64) -> Vec<Alert> {
        // A reset that came between readings is told before the new
        // reading replaces what was watched.
        let mut alerts = self.tick(now);
        for limit in limits {
            let key = (limit.kind.clone(), limit.model.clone());
            if limit.percent < 100.0 {
                self.used_up.remove(&key);
            } else if self.used_up.insert(key.clone()) {
                alerts.push(Alert::UsedUp(limit.clone()));
            }
            if limit.percent >= NEARLY && limit.resets_at.is_some() {
                self.nearly.insert(key, limit.clone());
            } else {
                self.nearly.remove(&key);
            }
        }
        alerts
    }

    /// The limits watched that have reset by `now`.
    pub fn tick(&mut self, now: i64) -> Vec<Alert> {
        let due: Vec<_> = self
            .nearly
            .iter()
            .filter(|(_, limit)| limit.resets_at.is_some_and(|at| at <= now))
            .map(|(key, _)| key.clone())
            .collect();
        due.into_iter()
            .filter_map(|key| {
                self.used_up.remove(&key);
                self.nearly.remove(&key)
            })
            .map(Alert::Reset)
            .collect()
    }
}
