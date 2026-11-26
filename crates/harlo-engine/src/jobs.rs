//! Background jobs: work a turn starts and doesn't wait for, such as a
//! shell command run in the background. Its result comes back to the
//! session as a message when it ends, which starts the agent again.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

use crate::sandbox::Confined;
use crate::tools::shell::Running;

/// Where a turn's background jobs start.
pub trait Jobs: Send + Sync + std::fmt::Debug {
    /// Starts `command` in `cwd` in the background, for the step
    /// `message`'s call `call`; returns the job's ID.
    ///
    /// # Errors
    ///
    /// With a message for the model when it can't be started.
    fn shell(
        &self,
        message: &str,
        call: &str,
        command: &str,
        cwd: &Path,
        confined: Option<Confined>,
    ) -> Result<String, String>;

    /// How long a command that names no time limit runs before it is
    /// taken over with [`Self::adopt`]; `None` to stop it at the usual
    /// limit instead, as when there is nowhere for it to carry on.
    fn takes_over_after(&self) -> Option<Duration> {
        None
    }

    /// Resolves when a person asks that the step `message`'s call `call`,
    /// a command still running, carry on in the background. Never, where
    /// nobody can ask.
    fn asked_to_move<'a>(&'a self, message: &'a str, call: &'a str) -> Asked<'a> {
        let _ = (message, call);
        Box::pin(std::future::pending())
    }

    /// Takes over `adopted`, a command that outran its time or that a
    /// person asked not to wait for. It carries on as a background job;
    /// returns the job's ID.
    fn adopt(&self, adopted: Adopted<'_>) -> String;
}

/// A person's asking for a command to carry on in the background, on its
/// way.
pub type Asked<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// A command handed over to carry on in the background.
#[derive(Debug)]
pub struct Adopted<'a> {
    /// The step whose call ran it.
    pub message: &'a str,
    /// The call that ran it.
    pub call: &'a str,
    /// The command.
    pub command: &'a str,
    /// The folder it runs in.
    pub cwd: &'a Path,
    /// The command itself, still going.
    pub running: Running,
    /// Whether a person asked for it, rather than its time running out.
    pub asked: bool,
}

/// What the model is told once a command runs in the background.
#[must_use]
pub fn started(id: &str) -> String {
    format!(
        "The command runs in the background (shell ID: {id}). Its output comes to you as a \
         <background-result kind=\"shell\"> message when it ends. Don't wait for it or poll \
         it: carry on with other work, or tell the person what you started and end your reply."
    )
}

/// What the model is told when a command carries on in the background:
/// after `seconds`, having printed `so_far`, because its time ran out or
/// because a person `asked`.
///
/// Stopping it would lose what it did, and the model would run it again
/// from the start. Only its last lines are shown: all it prints comes
/// with its end.
#[must_use]
pub fn moved(id: &str, seconds: u64, asked: bool, so_far: &str) -> String {
    let latest = crate::tools::shell::latest(so_far);
    let mut out = String::new();
    if !latest.is_empty() {
        let _ = writeln!(out, "{latest}");
    }
    let why = if asked {
        format!("The person sent it to the background after {seconds} s, where it carries on")
    } else {
        format!("Still running after {seconds} s, so it carries on in the background")
    };
    let _ = write!(
        out,
        "{why} (shell ID: {id}). Its output comes to you as a \
         <background-result kind=\"shell\"> message when it ends. Don't run it again, wait \
         for it or poll it: carry on with other work, or tell the person it is still running \
         and end your reply."
    );
    out
}

/// The message a session gets when a command ends: its output and how it
/// ended (`completed`, `error` or `cancelled`).
#[must_use]
pub fn report(id: &str, command: &str, state: &str, output: &str, exit: Option<i32>) -> String {
    let escape = |s: &str| s.replace('&', "&amp;").replace('"', "&quot;");
    let mut body = if state == "cancelled" {
        "Cancelled".to_owned()
    } else {
        output.trim_end().to_owned()
    };
    if let Some(code) = exit {
        let _ = write!(body, "\n\nExit code: {code}");
    }
    format!(
        "<background-result kind=\"shell\" id=\"{id}\" state=\"{state}\" command=\"{}\">\n{body}\n</background-result>",
        escape(command)
    )
}
