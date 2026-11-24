//! The engine's error type.

// Rust guideline compliant 2026-10-09

use std::fmt;
use std::time::Duration;

/// Something the engine couldn't do.
///
/// Its text is written for the person reading Harlo: what failed and,
/// where it helps, what to change.
#[derive(Debug)]
pub struct Error {
    kind: Kind,
    message: String,
    /// The HTTP status the model's server refused with.
    status: Option<u16>,
    /// Whether asking again may work.
    retryable: bool,
    /// How long the server asked to wait before asking again.
    wait: Option<Duration>,
}

/// How model servers word a request longer than the model's context,
/// in lower case.
const OVERFLOW: &[&str] = &[
    "context length",
    "context window",
    "context_length_exceeded",
    "context limit",
    "context size",
    "maximum context",
    "prompt is too long",
    "input is too long",
    "too many tokens",
    "too many input tokens",
    "request too large",
    "reduce the length",
];

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
    fn new(kind: Kind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            status: None,
            retryable: false,
            wait: None,
        }
    }

    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self::new(Kind::Config, message)
    }

    pub(crate) fn provider(message: impl Into<String>) -> Self {
        Self::new(Kind::Provider, message)
    }

    /// The model's server couldn't be reached: asking again may work.
    pub(crate) fn unreachable(message: impl Into<String>) -> Self {
        Self {
            retryable: true,
            ..Self::new(Kind::Provider, message)
        }
    }

    /// The model's server refused with `status`; a timeout, a conflict, a
    /// rate limit or a server error may pass when asked again.
    pub(crate) fn refused(status: u16, message: impl Into<String>, wait: Option<Duration>) -> Self {
        Self {
            status: Some(status),
            retryable: matches!(status, 408 | 409 | 429) || status >= 500,
            wait,
            ..Self::new(Kind::Provider, message)
        }
    }

    pub(crate) fn stopped() -> Self {
        Self::new(Kind::Stopped, "Stopped.")
    }

    /// The HTTP status the model's server refused with, if it did.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// Whether asking the model again may work.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        self.retryable
    }

    /// How long the model's server asked to wait before asking again.
    #[must_use]
    pub fn wait(&self) -> Option<Duration> {
        self.wait
    }

    /// Whether the model's server refused the request for being longer
    /// than the model's context.
    ///
    /// Servers say so in words, each its own; this knows the common ones.
    #[must_use]
    pub fn is_overflow(&self) -> bool {
        // (A wrong key or a rate limit can mention tokens too.)
        if self.kind != Kind::Provider || matches!(self.status, Some(401 | 403 | 429)) {
            return false;
        }
        let said = self.message.to_lowercase();
        OVERFLOW.iter().any(|words| said.contains(words))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_longer_than_the_context_is_told_from_other_refusals() {
        let too_long = [
            "prompt is too long: 215000 tokens > 200000 maximum",
            "This model's maximum context length is 128000 tokens.",
            "the request exceeds the available context size",
        ];
        for said in too_long {
            assert!(Error::refused(400, said, None).is_overflow(), "{said}");
        }
        assert!(!Error::refused(400, "Unknown parameter: tools", None).is_overflow());
        assert!(!Error::refused(429, "too many tokens per minute", None).is_overflow());
        assert!(!Error::config("no context length is set").is_overflow());
    }
}
