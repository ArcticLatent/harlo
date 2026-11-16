//! The error of this crate: starting an agent, talking to it, framing.

// Rust guideline compliant 2026-10-09

use std::backtrace::{Backtrace, BacktraceStatus};
use std::fmt;

/// Why an agent couldn't be started, asked or read.
///
/// The `is_*` methods tell the cases apart. The message says what failed,
/// followed by its causes (`what: cause: cause`), as error chains print
/// them; the causes are part of the message, so a chain prints them once.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    backtrace: Backtrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    /// The agent's program couldn't be found or started.
    Start,
    /// The session has no running agent to ask.
    NotRunning,
    /// The agent ended before it answered.
    Exited,
    /// The agent answered a command with an error.
    Refused,
    /// A queued message that was already sent can't be changed.
    AlreadySent,
    /// A picture couldn't be read or isn't a kind agents take.
    Image,
    /// The agent's output broke the protocol (framing, chunks).
    Protocol,
    /// The conversation so far couldn't be read whole.
    History,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), backtrace: Backtrace::capture() }
    }

    /// The agent's program couldn't be found or started.
    #[must_use]
    pub fn is_start(&self) -> bool {
        self.kind == ErrorKind::Start
    }

    /// The session has no running agent, or it ended before answering.
    #[must_use]
    pub fn is_gone(&self) -> bool {
        matches!(self.kind, ErrorKind::NotRunning | ErrorKind::Exited)
    }

    /// The agent answered the command with an error; the message is its own.
    #[must_use]
    pub fn is_refused(&self) -> bool {
        matches!(self.kind, ErrorKind::Refused | ErrorKind::AlreadySent)
    }

    /// A picture couldn't be read or isn't PNG, JPEG, GIF or WebP.
    #[must_use]
    pub fn is_image(&self) -> bool {
        self.kind == ErrorKind::Image
    }

    /// The agent's output broke the protocol.
    #[must_use]
    pub fn is_protocol(&self) -> bool {
        self.kind == ErrorKind::Protocol
    }

    /// The conversation so far couldn't be read whole.
    #[must_use]
    pub fn is_history(&self) -> bool {
        self.kind == ErrorKind::History
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        if self.backtrace.status() == BacktraceStatus::Captured {
            write!(f, "\n{}", self.backtrace)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// Turns a failure into an [`Error`] of a kind, its cause kept in the message.
pub(crate) trait Context<T> {
    /// `what`, then `: ` and the cause (its whole chain).
    fn or_error(self, kind: ErrorKind, what: impl FnOnce() -> String) -> Result<T, Error>;
}

impl<T, E: std::error::Error> Context<T> for Result<T, E> {
    fn or_error(self, kind: ErrorKind, what: impl FnOnce() -> String) -> Result<T, Error> {
        self.map_err(|e| Error::new(kind, format!("{}: {}", what(), chain(&e))))
    }
}

impl<T> Context<T> for Option<T> {
    fn or_error(self, kind: ErrorKind, what: impl FnOnce() -> String) -> Result<T, Error> {
        self.ok_or_else(|| Error::new(kind, what()))
    }
}

/// An error's text with its causes (`a: b: c`).
pub(crate) fn chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        text.push_str(": ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    text
}
