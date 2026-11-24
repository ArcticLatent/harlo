//! Background jobs: work a turn starts and doesn't wait for, such as a
//! shell command run in the background. Its result comes back to the
//! session as a message when it ends, which starts the agent again.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::Path;

/// Where a turn's background jobs start.
pub trait Jobs: Send + Sync + std::fmt::Debug {
    /// Starts `command` in `cwd` in the background, for the step
    /// `message`'s call `call`; returns the job's ID.
    ///
    /// # Errors
    ///
    /// With a message for the model when it can't be started.
    fn shell(&self, message: &str, call: &str, command: &str, cwd: &Path)
    -> Result<String, String>;
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
