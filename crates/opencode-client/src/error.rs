//! The error type returned by every client call.

// Rust guideline compliant 2026-10-09

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

/// HTTP status OpenCode uses when the target no longer exists.
///
/// Returned, for example, when replying to a permission request that another
/// client already answered (`PermissionNotFoundError`).
const NOT_FOUND: u16 = 404;

/// HTTP status OpenCode uses when the target was already settled.
///
/// Returned, for example, when answering a form another client already
/// answered (`FormAlreadySettledError`).
const CONFLICT: u16 = 409;

/// HTTP status for a missing or wrong server password.
const UNAUTHORIZED: u16 = 401;

#[derive(Debug)]
enum Kind {
    /// The server could not be reached, or the connection broke.
    Transport(reqwest::Error),
    /// The server answered with an error status.
    Status {
        status: u16,
        tag: Option<String>,
        message: String,
        /// Whether retrying with `force` would do it, such as removing a
        /// worktree with changes.
        force: bool,
    },
    /// The server's answer did not match the expected shape.
    Decode(serde_json::Error),
    /// A local file (such as the service registration) could not be read.
    Io(std::io::Error),
    /// Something else, described in words.
    Other(String),
}

/// A failed call to an OpenCode server.
///
/// Use the `is_*` helpers to react to specific outcomes: a request that was
/// already answered elsewhere ([`Error::is_settled`]), a wrong password
/// ([`Error::is_unauthorized`]) or an unreachable server ([`Error::is_transport`]).
///
/// Its text is one sentence, fit to show people: where the error was
/// created is kept apart, in [`Error::backtrace`].
#[derive(Debug)]
pub struct Error {
    kind: Kind,
    backtrace: Backtrace,
}

impl Error {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            backtrace: Backtrace::capture(),
        }
    }

    pub(crate) fn status(status: u16, body: &str) -> Self {
        let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
        let tag = parsed
            .as_ref()
            .and_then(|v| v["_tag"].as_str())
            .map(str::to_owned);
        // Some errors keep their words and details under `data`.
        let message = parsed
            .as_ref()
            .and_then(|v| {
                v["message"]
                    .as_str()
                    .or_else(|| v["data"]["message"].as_str())
            })
            .map_or_else(|| body.chars().take(300).collect(), str::to_owned);
        let force = parsed
            .as_ref()
            .is_some_and(|v| v["data"]["forceRequired"].as_bool() == Some(true));
        Self::new(Kind::Status {
            status,
            tag,
            message,
            force,
        })
    }

    pub(crate) fn other(message: impl Into<String>) -> Self {
        Self::new(Kind::Other(message.into()))
    }

    /// Returns where the error was created, when backtraces are enabled.
    ///
    /// `RUST_BACKTRACE` and `RUST_LIB_BACKTRACE` turn them on; otherwise
    /// it's empty.
    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }

    /// Returns the HTTP status when the server answered with an error.
    #[must_use]
    pub fn http_status(&self) -> Option<u16> {
        match self.kind {
            Kind::Status { status, .. } => Some(status),
            _ => None,
        }
    }

    /// Returns OpenCode's error tag, such as `PermissionNotFoundError`.
    #[must_use]
    pub fn tag(&self) -> Option<&str> {
        match &self.kind {
            Kind::Status { tag, .. } => tag.as_deref(),
            _ => None,
        }
    }

    /// Whether the target was already answered, cancelled or removed elsewhere.
    ///
    /// A client that shows a stale approval or question should drop it and not
    /// retry when this is true.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        matches!(self.http_status(), Some(NOT_FOUND | CONFLICT))
    }

    /// Whether the server would do it with `force`, such as removing a
    /// worktree that has changes.
    #[must_use]
    pub fn needs_force(&self) -> bool {
        matches!(self.kind, Kind::Status { force: true, .. })
    }

    /// Whether the server rejected the password.
    #[must_use]
    pub fn is_unauthorized(&self) -> bool {
        self.http_status() == Some(UNAUTHORIZED)
    }

    /// Whether the server could not be reached or the connection broke.
    #[must_use]
    pub fn is_transport(&self) -> bool {
        matches!(self.kind, Kind::Transport(_))
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::Transport(e) => write!(f, "could not reach the OpenCode server: {e}")?,
            Kind::Status {
                status,
                tag,
                message,
                ..
            } => {
                write!(f, "the OpenCode server answered {status}")?;
                if let Some(tag) = tag {
                    write!(f, " ({tag})")?;
                }
                write!(f, ": {message}")?;
            }
            Kind::Decode(e) => write!(f, "unexpected answer from the OpenCode server: {e}")?,
            Kind::Io(e) => write!(f, "could not read an OpenCode file: {e}")?,
            Kind::Other(message) => write!(f, "{message}")?,
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            Kind::Transport(e) => Some(e),
            Kind::Decode(e) => Some(e),
            Kind::Io(e) => Some(e),
            Kind::Status { .. } | Kind::Other(_) => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::new(Kind::Transport(e))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::new(Kind::Decode(e))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new(Kind::Io(e))
    }
}
