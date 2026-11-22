//! Claude subscriptions through OpenCode's Claude Agent integration.
//!
//! OpenCode's `claude-agent` integration runs Claude Code (`claude`) through
//! Anthropic's Agent SDK, so a Claude subscription pays for its models. Claude
//! Code keeps the sign-in; OpenCode only records that it should use it.
//!
//! This module checks the Claude Code on this computer: whether it's
//! installed, and which account it's signed in to (`claude auth status`).
//! Connecting and disconnecting go through OpenCode's API
//! ([`Client::connect_external`](crate::server::Client::connect_external)).

// Rust guideline compliant 2026-10-09

use std::ffi::OsString;
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;

/// OpenCode's integration for Claude subscriptions.
pub const INTEGRATION: &str = "claude-agent";

/// The integration's method that uses Claude Code's own sign-in.
pub const LOGIN_METHOD: &str = "claude-login";

/// The variable OpenCode reads for the `claude` program to run.
pub const PROGRAM_VARIABLE: &str = "OPENCODE_CLAUDE_PATH";

/// How long a `claude` check may take before it counts as failed.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

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
