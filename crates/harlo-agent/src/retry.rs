//! Requests that failed for a passing reason are sent again: a rate limit
//! (not a plan used up), a server error or overload, a connection that
//! didn't open or broke during the reply. [`TRIES`] more times, after 1, 2
//! and 4 seconds, or as long as the server asked (`Retry-After`, up to
//! [`MAX_WAIT`]). A refusal that won't change (a bad request, the sign-in,
//! the context too long, the plan used up) isn't retried.

use std::time::Duration;

use crate::responses::{ApiError, Dropped, Failed};

/// Retries after the first try.
pub const TRIES: u32 = 3;
/// The longest wait a server can ask for and still be retried.
const MAX_WAIT: u64 = 60;

/// Error types that mean the plan (or the account's quota) is used up:
/// waiting a few seconds won't help.
pub const USED_UP: &[&str] = &["usage_limit_reached", "usage_not_included", "insufficient_quota", "subscription_sharing_usage_limit_exceeded"];

/// Codes in a stream's error that are worth another try.
const PASSING: &[&str] = &[
    "server_error",
    "internal_error",
    "rate_limit_exceeded",
    "rate_limit_error",
    "overloaded",
    "overloaded_error",
    "service_unavailable",
    "subscription_sharing_usage_unavailable",
    "subscription_sharing_user_unavailable",
];

/// `Retry-After` in seconds (a number; a date isn't read).
pub fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()
}

/// Why `e` is worth another try (shown to the user), with the wait the
/// server asked for; None when it isn't.
pub fn transient(e: &anyhow::Error) -> Option<(&'static str, Option<u64>)> {
    if crate::compact::overflowed(e) {
        return None;
    }
    if let Some(a) = e.downcast_ref::<ApiError>() {
        if USED_UP.contains(&a.code.as_str()) {
            return None;
        }
        let why = match a.status {
            429 => "the server's rate limit",
            408 => "the request timed out",
            500 | 502 | 504 => "a server error",
            503 | 520..=529 => "the server is busy",
            _ => return None,
        };
        return Some((why, a.retry_after.filter(|s| *s <= MAX_WAIT)));
    }
    if e.downcast_ref::<Dropped>().is_some() {
        return Some(("the connection broke", None));
    }
    if let Some(f) = e.downcast_ref::<Failed>() {
        let m = f.message.to_lowercase();
        if PASSING.contains(&f.code.as_str()) || (f.code.is_empty() && (m.contains("overloaded") || m.contains("try again"))) {
            return Some(("the server had a problem", None));
        }
    }
    None
}

/// How long to wait before retry `n` (1-based). `HARLO_AGENT_RETRY_MS`
/// sets the first wait instead (tests).
pub fn delay(n: u32, asked: Option<u64>) -> Duration {
    if let Some(s) = asked {
        return Duration::from_secs(s.max(1));
    }
    let base = std::env::var("HARLO_AGENT_RETRY_MS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(1000);
    Duration::from_millis(base << (n.saturating_sub(1)).min(6))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(status: u16, code: &str, after: Option<u64>) -> anyhow::Error {
        ApiError { status, message: "refused".into(), code: code.into(), retry_after: after }.into()
    }

    #[test]
    fn what_is_tried_again() {
        assert_eq!(transient(&api(429, "rate_limit_exceeded", Some(7))), Some(("the server's rate limit", Some(7))));
        assert_eq!(transient(&api(429, "", Some(600))), Some(("the server's rate limit", None)));
        assert!(transient(&api(503, "", None)).is_some());
        assert!(transient(&api(500, "", None)).is_some());
        assert!(transient(&Dropped("closed".into()).into()).is_some());
        assert!(transient(&Failed { code: "server_error".into(), message: "x".into() }.into()).is_some());
        assert!(transient(&Failed { code: String::new(), message: "Model is overloaded".into() }.into()).is_some());
    }

    #[test]
    fn what_is_not() {
        assert!(transient(&api(429, "usage_limit_reached", None)).is_none());
        assert!(transient(&api(429, "insufficient_quota", None)).is_none());
        assert!(transient(&api(400, "", None)).is_none());
        assert!(transient(&api(401, "", None)).is_none());
        assert!(transient(&api(403, "", None)).is_none());
        let long: anyhow::Error = ApiError { status: 400, message: "context_length_exceeded".into(), code: "context_length_exceeded".into(), retry_after: None }.into();
        assert!(transient(&long).is_none());
        assert!(transient(&Failed { code: "invalid_prompt".into(), message: "no".into() }.into()).is_none());
        assert!(transient(&anyhow::anyhow!("something else")).is_none());
    }

    #[test]
    fn waits_grow() {
        assert_eq!(delay(1, None), Duration::from_secs(1));
        assert_eq!(delay(2, None), Duration::from_secs(2));
        assert_eq!(delay(3, None), Duration::from_secs(4));
        assert_eq!(delay(1, Some(9)), Duration::from_secs(9));
    }
}
