//! The engine's error type.

// Rust guideline compliant 2026-10-09

use std::fmt;

/// Something the engine couldn't do.
///
/// Its text is written for the person reading Harlo: what failed and,
/// where it helps, what to change.
#[derive(Debug)]
pub struct Error {
    kind: Kind,
    message: String,
}

/// What sort of failure an [`Error`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The configuration couldn't be read or doesn't name what's needed.
    Config,
    /// The model's server refused the request or the stream broke.
    Provider,
    /// The turn was stopped.
    Stopped,
}

impl Error {
    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Config,
            message: message.into(),
        }
    }

    pub(crate) fn provider(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Provider,
            message: message.into(),
        }
    }

    pub(crate) fn stopped() -> Self {
        Self {
            kind: Kind::Stopped,
            message: "Stopped.".to_owned(),
        }
    }

    /// Whether the turn ended because it was stopped, not because it failed.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.kind == Kind::Stopped
    }

    /// Whether the model's server refused the request or its stream broke.
    #[must_use]
    pub fn is_provider(&self) -> bool {
        self.kind == Kind::Provider
    }

    /// Whether the configuration was missing or wrong.
    #[must_use]
    pub fn is_config(&self) -> bool {
        self.kind == Kind::Config
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}
