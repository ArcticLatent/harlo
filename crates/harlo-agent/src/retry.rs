//! Requests that failed for a passing reason are sent again: a rate limit
//! (not a plan used up), a server error or overload, a connection that
//! didn't open or broke during the reply. [`TRIES`] more times, after 1, 2
//! and 4 seconds, or as long as the server asked (`Retry-After`, up to
//! [`MAX_WAIT`]; asked to wait longer, it stops and says when to try
//! again). A refusal that won't change (a bad request, the sign-in, the
//! context too long, the plan used up) isn't retried.

use std::time::Duration;

use crate::responses::{ApiError, Dropped, Failed};

/// Retries after the first try.
pub const TRIES: u32 = 3;
/// The longest wait a server can ask for and still be retried.
pub const MAX_WAIT: u64 = 300;

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

/// The wait a server asked for, in seconds: `retry-after-ms`, or
/// `Retry-After` as seconds or as a date.
pub fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let text = |k| headers.get(k).and_then(|v| v.to_str().ok()).map(str::trim);
    if let Some(ms) = text("retry-after-ms").and_then(|v| v.parse::<f64>().ok()).filter(|ms| ms.is_finite() && *ms >= 0.0) {
        return Some((ms / 1000.0).ceil() as u64);
    }
    let v = text("retry-after")?;
    v.parse().ok().or_else(|| {
        let at = httpdate::parse_http_date(v).ok()?;
        Some(at.duration_since(std::time::SystemTime::now()).map_or(0, |d| d.as_secs_f64().ceil() as u64))
    })
}

/// What's said when a server asks to wait `secs`, longer than
/// [`MAX_WAIT`]: the request isn't sent again, and when to try.
pub fn not_waiting(secs: u64, now: i64) -> String {
    let wait = crate::responses::span(Duration::from_secs(secs));
    let at = crate::clock::local(now + secs as i64);
    let day = if secs >= 20 * 3600 { format!("{:04}-{:02}-{:02} ", at.year, at.month, at.day) } else { String::new() };
    format!("Not retried: the server asked to wait {wait}; try again after {day}{:02}:{:02}", at.hour, at.minute)
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
        return Some((why, a.retry_after));
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

/// Whether `e` says the plan (or the account's quota) is used up, and
/// when it resets (unix seconds), if it said.
pub fn used_up(e: &anyhow::Error) -> Option<Option<i64>> {
    if let Some(a) = e.downcast_ref::<ApiError>() {
        return USED_UP.contains(&a.code.as_str()).then_some(a.resets_at);
    }
    e.downcast_ref::<Failed>().filter(|f| USED_UP.contains(&f.code.as_str())).map(|_| None)
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
        ApiError { status, message: "refused".into(), code: code.into(), retry_after: after, resets_at: None }.into()
    }

    #[test]
    fn what_is_tried_again() {
        assert_eq!(transient(&api(429, "rate_limit_exceeded", Some(7))), Some(("the server's rate limit", Some(7))));
        assert_eq!(transient(&api(429, "", Some(600))), Some(("the server's rate limit", Some(600))));
        assert!(transient(&api(503, "", None)).is_some());
        assert!(transient(&api(500, "", None)).is_some());
        assert!(transient(&Dropped("closed".into(), serde_json::Value::Null).into()).is_some());
        assert!(transient(&Failed { code: "server_error".into(), message: "x".into(), usage: serde_json::Value::Null }.into()).is_some());
        assert!(transient(&Failed { code: String::new(), message: "Model is overloaded".into(), usage: serde_json::Value::Null }.into()).is_some());
    }

    #[test]
    fn what_is_not() {
        assert!(transient(&api(429, "usage_limit_reached", None)).is_none());
        assert!(transient(&api(429, "insufficient_quota", None)).is_none());
        assert!(transient(&api(400, "", None)).is_none());
        assert!(transient(&api(401, "", None)).is_none());
        assert!(transient(&api(403, "", None)).is_none());
        let long: anyhow::Error = ApiError { status: 400, message: "context_length_exceeded".into(), code: "context_length_exceeded".into(), retry_after: None, resets_at: None }.into();
        assert!(transient(&long).is_none());
        assert!(transient(&Failed { code: "invalid_prompt".into(), message: "no".into(), usage: serde_json::Value::Null }.into()).is_none());
        assert!(transient(&anyhow::anyhow!("something else")).is_none());
    }

    #[test]
    fn the_wait_asked_for() {
        use reqwest::header::{HeaderMap, HeaderValue};
        let h = |pairs: &[(&'static str, &str)]| {
            let mut m = HeaderMap::new();
            for (k, v) in pairs {
                m.insert(*k, HeaderValue::from_str(v).unwrap());
            }
            retry_after(&m)
        };
        assert_eq!(h(&[("retry-after", "600")]), Some(600));
        assert_eq!(h(&[("retry-after-ms", "1500"), ("retry-after", "9")]), Some(2), "the finer one wins");
        let later = httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(120));
        assert!(h(&[("retry-after", &later)]).is_some_and(|s| (118..=121).contains(&s)), "a date");
        assert_eq!(h(&[("retry-after", "Wed, 21 Oct 2015 07:28:00 GMT")]), Some(0), "a date gone by");
        assert_eq!(h(&[("retry-after", "soon")]), None);
        // Too long to wait: said when to try instead.
        let now = 1_794_063_845;
        let said = not_waiting(600, now);
        let at = crate::clock::local(now + 600);
        assert!(said.starts_with("Not retried: the server asked to wait 10 min; try again after ") && said.ends_with(&format!("{:02}:{:02}", at.hour, at.minute)), "{said}");
        assert!(not_waiting(2 * 86_400, now).contains(&format!("{:04}-", at.year)), "a day ahead says the date");
    }

    #[test]
    fn waits_grow() {
        assert_eq!(delay(1, None), Duration::from_secs(1));
        assert_eq!(delay(2, None), Duration::from_secs(2));
        assert_eq!(delay(3, None), Duration::from_secs(4));
        assert_eq!(delay(1, Some(9)), Duration::from_secs(9));
    }
}
