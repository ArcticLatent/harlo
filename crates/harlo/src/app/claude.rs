//! A Claude subscription for OpenCode: Settings → Models checks Claude Code
//! on this computer and OpenCode's Claude Agent integration, and offers the
//! next step.
//!
//! Claude Code keeps the sign-in. Installing it and signing in run in a
//! Harlo terminal, where the user sees and answers them. Connecting and
//! disconnecting go through OpenCode's API and apply to every client of
//! the server. A server on another computer uses that computer's Claude
//! Code, so only the connection is checked from here.
//!
//! While Harlo runs, the plan's limits are read now and then, and a
//! notification says when one is used up, and when one nearly or fully used
//! has reset (with a sound of its own).

// Rust guideline compliant 2026-10-09

use std::ffi::OsString;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use harlo_core::claude::{self, Alert, ClaudeCode, Limit, Login, Watch};
use harlo_core::server::Integration;

use super::{App, Load, Message};
use crate::task::Task;

/// Claude Code's official installer, run in a terminal when the user asks.
const INSTALL: &str = "curl -fsSL https://claude.ai/install.sh | bash";

/// How soon after Harlo starts the plan's limits are first read.
const FIRST_READING: Duration = Duration::from_secs(3);

/// How often the plan's limits are read while Harlo runs.
const READING_EVERY: Duration = Duration::from_mins(10);

/// The sound a limit's reset plays: the sound theme's login tune.
const RESET_SOUND: &str = "desktop-login";

/// How many times the integration is asked for before it's taken as
/// missing: OpenCode loads integrations once a request first needs them, and
/// answers "not found" until it has.
const AGENT_TRIES: u32 = 6;

/// How long to wait between those tries.
const AGENT_PAUSE: Duration = Duration::from_secs(1);

/// The icon of a plan limit's notification.
const LIMIT_ICON: &str = "dialog-information";

/// What a terminal task ends with, so its output stays readable.
const KEEP_OPEN: &str = "printf '\\n%s' 'Press Enter to close this terminal. '; read _";

/// How the Claude subscription stands.
#[derive(Debug, Clone)]
pub struct Claude {
    /// Claude Code on this computer; stays idle for a server elsewhere.
    pub code: Load<ClaudeCode>,
    /// OpenCode's Claude Agent integration, or `None` when the server has
    /// none.
    pub agent: Load<Option<Integration>>,
    /// How much of the plan's limits is used, or `None` when there are no
    /// limits to show; asked once Claude Code is signed in to a plan.
    pub usage: Load<Option<Vec<Limit>>>,
    /// What's being changed on the server, while it is.
    pub busy: Option<Action>,
    /// Why the last change failed.
    pub error: Option<String>,
    /// The `claude` program to check and run.
    program: OsString,
    /// What the plan's limits did, so a change is told once.
    watch: Watch,
}

impl Default for Claude {
    fn default() -> Self {
        Self {
            code: Load::Idle,
            agent: Load::Idle,
            usage: Load::Idle,
            busy: None,
            error: None,
            program: claude::program(),
            watch: Watch::default(),
        }
    }
}

/// Something the user can do about the subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Install Claude Code, in a terminal.
    Install,
    /// Sign Claude Code in, or switch its account, in a terminal.
    SignIn,
    /// Connect OpenCode to Claude Code's sign-in.
    Connect,
    /// Remove OpenCode's connection.
    Disconnect,
    /// Check everything again.
    Check,
}

/// Something that happened to the subscription.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The user asked for this.
    Act(Action),
    /// Claude Code was checked.
    Checked(ClaudeCode),
    /// The integration was read, or couldn't be.
    Agent(Result<Option<Integration>, String>),
    /// A change on the server finished, or failed.
    Changed(Result<(), String>),
    /// The plan's usage was read, or couldn't be.
    Usage(Result<Option<Vec<Limit>>, String>),
    /// Time to check Claude Code and read the plan's limits again.
    Read,
}

impl App {
    /// Uses `program` as Claude Code instead of the one OpenCode would run.
    #[must_use]
    pub fn with_claude_program(mut self, program: impl Into<OsString>) -> Self {
        self.claude.program = program.into();
        self
    }

    /// Applies something that happened to the subscription.
    pub(super) fn claude_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Act(Action::Check) => return self.check_claude(),
            Event::Act(Action::Install) => {
                return self.claude_terminal("Install Claude Code", INSTALL);
            }
            Event::Act(Action::SignIn) => {
                // `$0` is Claude Code, passed separately so no quoting is needed.
                return self.claude_terminal("Claude sign-in", "\"$0\" auth login");
            }
            Event::Act(action @ (Action::Connect | Action::Disconnect)) => {
                return self.change_claude(action);
            }
            Event::Checked(found) => {
                let subscribed = matches!(
                    found,
                    ClaudeCode::Installed {
                        login: Login::Subscription(_),
                        ..
                    }
                );
                self.claude.code = Load::Loaded(found);
                if subscribed {
                    return self.read_usage();
                }
                self.claude.usage = Load::Idle;
            }
            Event::Read => return self.check_claude_code(),
            Event::Usage(Ok(found)) => {
                let alerts = found.as_deref().map_or_else(Vec::new, |limits| {
                    self.claude.watch.read(limits, epoch_seconds(self.now))
                });
                self.claude.usage = Load::Loaded(found);
                return self.limit_notices(&alerts);
            }
            Event::Usage(Err(error)) => self.claude.usage = Load::Failed(error),
            Event::Agent(Ok(found)) => self.claude.agent = Load::Loaded(found),
            Event::Agent(Err(error)) => self.claude.agent = Load::Failed(error),
            Event::Changed(result) => {
                self.claude.busy = None;
                self.claude.error = result.err();
                // The server's models change with the connection.
                self.forget_catalogs();
                return Task::batch([self.read_claude_agent(), self.load_setup()]);
            }
        }
        Task::none()
    }

    /// Checks Claude Code, when the server is on this computer, and reads
    /// the integration.
    pub(super) fn check_claude(&mut self) -> Task<Message> {
        self.claude.error = None;
        Task::batch([self.check_claude_code(), self.read_claude_agent()])
    }

    /// Checks Claude Code, when the server is on this computer; a check
    /// that finds it signed in to a plan reads the plan's limits next.
    fn check_claude_code(&mut self) -> Task<Message> {
        if !self.location.is_local() {
            self.claude.code = Load::Idle;
            return Task::none();
        }
        if !matches!(self.claude.code, Load::Loaded(_)) {
            self.claude.code = Load::Loading;
        }
        let program = self.claude.program.clone();
        Task::future(async move { Message::Claude(Event::Checked(claude::check(program).await)) })
    }

    /// Tells the limits that have reset by now, and reads them again.
    pub(super) fn claude_tick(&mut self) -> Task<Message> {
        let alerts = self.claude.watch.tick(epoch_seconds(self.now));
        if alerts.is_empty() {
            return Task::none();
        }
        Task::batch([self.limit_notices(&alerts), self.check_claude_code()])
    }

    /// A desktop notification for each alert.
    fn limit_notices(&self, alerts: &[Alert]) -> Task<Message> {
        if !self.notices.enabled || alerts.is_empty() {
            return Task::none();
        }
        let now = epoch_seconds(self.now);
        let notices: Vec<_> = alerts
            .iter()
            .map(|alert| limit_notice(alert, now))
            .collect();
        Task::future(async move {
            for (summary, body, reset) in notices {
                let mut notice = notify_rust::Notification::new();
                notice
                    .appname("Harlo")
                    .summary(&summary)
                    .body(&body)
                    .icon(LIMIT_ICON);
                if reset {
                    notice.sound_name(RESET_SOUND);
                }
                // Without a notification service there's nothing to show it
                // on; Settings → Models still shows the limits.
                let _ = notice.show_async().await;
            }
        })
        .discard()
    }

    /// Asks Claude Code how much of the plan's limits is used.
    fn read_usage(&mut self) -> Task<Message> {
        if !matches!(self.claude.usage, Load::Loaded(_)) {
            self.claude.usage = Load::Loading;
        }
        let program = self.claude.program.clone();
        Task::future(async move { Message::Claude(Event::Usage(claude::usage(program).await)) })
    }

    /// Reads OpenCode's Claude Agent integration.
    fn read_claude_agent(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.claude.agent = Load::Failed("Not connected to OpenCode yet.".to_owned());
            return Task::none();
        };
        if !matches!(self.claude.agent, Load::Loaded(_)) {
            self.claude.agent = Load::Loading;
        }
        Task::future(async move {
            let mut tries = AGENT_TRIES;
            loop {
                let found = client
                    .integration(claude::INTEGRATION)
                    .await
                    .map_err(|e| e.to_string());
                tries -= 1;
                if !matches!(found, Ok(None)) || tries == 0 {
                    return Message::Claude(Event::Agent(found));
                }
                tokio::time::sleep(AGENT_PAUSE).await;
            }
        })
    }

    /// Runs `script` in a terminal on this computer with Claude Code as
    /// `$0`, then checks again once it ends.
    fn claude_terminal(&mut self, title: &str, script: &str) -> Task<Message> {
        if !self.location.is_local() {
            return Task::none();
        }
        // The terminal opens under the window, so Settings steps aside.
        self.sheets.open = None;
        let program = self.claude.program.to_string_lossy().into_owned();
        self.run_in_terminal(
            title,
            "/bin/sh",
            vec!["-c".to_owned(), format!("{script}; {KEEP_OPEN}"), program],
            Message::Claude(Event::Act(Action::Check)),
        )
    }

    /// Connects OpenCode to Claude Code's sign-in, or removes its
    /// connections.
    fn change_claude(&mut self, action: Action) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if self.claude.busy.is_some() {
            return Task::none();
        }
        let credentials: Vec<String> = match &self.claude.agent {
            Load::Loaded(Some(agent)) => agent
                .connections
                .iter()
                .filter_map(|c| c.credential.clone())
                .collect(),
            _ => Vec::new(),
        };
        self.claude.busy = Some(action);
        self.claude.error = None;
        Task::future(async move {
            let result = if action == Action::Connect {
                client
                    .connect_external(claude::INTEGRATION, claude::LOGIN_METHOD)
                    .await
            } else {
                let mut result = Ok(());
                for id in credentials {
                    result = result.and(client.remove_credential(&id).await);
                }
                result
            };
            Message::Claude(Event::Changed(result.map_err(|e| e.to_string())))
        })
    }
}

/// Asks for the plan's limits shortly after Harlo starts, then now and then.
pub(super) fn watch_usage() -> Task<Message> {
    let readings = futures_util::stream::unfold(FIRST_READING, |wait| async move {
        tokio::time::sleep(wait).await;
        Some((Message::Claude(Event::Read), READING_EVERY))
    });
    Task::stream(readings)
}

/// A limit's notification: its summary, its body, and whether it's a reset.
fn limit_notice(alert: &Alert, now: i64) -> (String, String, bool) {
    match alert {
        Alert::UsedUp(limit) => {
            let body = limit.resets_at.map_or_else(
                || "Claude models wait until it resets.".to_owned(),
                |at| format!("It resets {}.", resets_in(at - now)),
            );
            (format!("{} is used up", limit_name(limit)), body, false)
        }
        Alert::Reset(limit) => (
            format!("{} has reset", limit_name(limit)),
            "You can use it again.".to_owned(),
            true,
        ),
    }
}

/// A limit in a notification: "Claude's weekly limit".
fn limit_name(limit: &Limit) -> String {
    let what = match (limit.kind.as_str(), limit.model.as_deref()) {
        ("session", _) => "session".to_owned(),
        (_, Some(model)) => format!("{} {model}", limit.group),
        _ => limit.group.clone(),
    };
    format!("Claude's {what} limit")
}

/// `time` in whole seconds since the Unix epoch.
pub(crate) fn epoch_seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// When something `seconds` from now happens, briefly: "in 2 h 14 min".
pub(crate) fn resets_in(seconds: i64) -> String {
    if seconds <= 60 {
        return "now".to_owned();
    }
    let minutes = seconds / 60;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    if days > 0 {
        let unit = if days == 1 { "day" } else { "days" };
        if hours > 0 {
            format!("in {days} {unit} {hours} h")
        } else {
            format!("in {days} {unit}")
        }
    } else if hours > 0 {
        format!("in {hours} h {minutes} min")
    } else {
        format!("in {minutes} min")
    }
}

#[cfg(test)]
mod tests {
    use harlo_core::claude::Severity;

    use super::*;

    /// A limit of `kind` in `group`, for `model`, used up, resetting at
    /// `resets_at`.
    fn limit(kind: &str, group: &str, model: Option<&str>, resets_at: Option<i64>) -> Limit {
        Limit {
            kind: kind.into(),
            group: group.into(),
            model: model.map(str::to_owned),
            percent: 100.0,
            resets_at,
            severity: Severity::Critical,
            active: true,
        }
    }

    #[test]
    fn notices_name_the_limit_and_when_it_resets() {
        let session = limit("session", "session", None, Some(2 * 3600 + 14 * 60 + 5));
        assert_eq!(
            limit_notice(&Alert::UsedUp(session), 0),
            (
                "Claude's session limit is used up".to_owned(),
                "It resets in 2 h 14 min.".to_owned(),
                false
            )
        );
        let fable = limit("weekly_scoped", "weekly", Some("Fable"), None);
        assert_eq!(
            limit_notice(&Alert::UsedUp(fable.clone()), 0).1,
            "Claude models wait until it resets."
        );
        assert_eq!(
            limit_notice(&Alert::Reset(fable), 0),
            (
                "Claude's weekly Fable limit has reset".to_owned(),
                "You can use it again.".to_owned(),
                true
            )
        );
        let week = limit("weekly_all", "weekly", None, None);
        assert_eq!(
            limit_notice(&Alert::Reset(week), 0).0,
            "Claude's weekly limit has reset"
        );
    }
}
