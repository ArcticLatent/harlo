//! A Claude subscription for OpenCode: Settings → Models checks Claude Code
//! on this computer and OpenCode's Claude Agent integration, and offers the
//! next step.
//!
//! Claude Code keeps the sign-in. Installing it and signing in run in a
//! Harlo terminal, where the user sees and answers them. Connecting and
//! disconnecting go through OpenCode's API and apply to every client of
//! the server. A server on another computer uses that computer's Claude
//! Code, so only the connection is checked from here.

// Rust guideline compliant 2026-10-09

use std::ffi::OsString;

use harlo_core::claude::{self, ClaudeCode};
use harlo_core::server::Integration;

use super::{App, Load, Message};
use crate::task::Task;

/// Claude Code's official installer, run in a terminal when the user asks.
const INSTALL: &str = "curl -fsSL https://claude.ai/install.sh | bash";

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
    /// What's being changed on the server, while it is.
    pub busy: Option<Action>,
    /// Why the last change failed.
    pub error: Option<String>,
    /// The `claude` program to check and run.
    program: OsString,
}

impl Default for Claude {
    fn default() -> Self {
        Self {
            code: Load::Idle,
            agent: Load::Idle,
            busy: None,
            error: None,
            program: claude::program(),
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
            Event::Checked(found) => self.claude.code = Load::Loaded(found),
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
        let code = if self.location.is_local() {
            if !matches!(self.claude.code, Load::Loaded(_)) {
                self.claude.code = Load::Loading;
            }
            let program = self.claude.program.clone();
            Task::future(
                async move { Message::Claude(Event::Checked(claude::check(program).await)) },
            )
        } else {
            self.claude.code = Load::Idle;
            Task::none()
        };
        Task::batch([code, self.read_claude_agent()])
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
            let found = client
                .integration(claude::INTEGRATION)
                .await
                .map_err(|e| e.to_string());
            Message::Claude(Event::Agent(found))
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
