//! Watches a session's prompt-cache hits and says why a request missed.
//!
//! Provider prompt caches reuse the work of a shared prefix across a
//! session's requests. When a request reads much less from the cache than
//! the previous one should have left warm, this classifies the cause —
//! a model or account switch, a changed system prompt or tool set, an
//! edited earlier message, compaction, the provider's daily reset, an idle
//! gap, or an unexplained cold read — and flags the causes that are the
//! harness's own fault. It never changes a request; it only explains the
//! misses that already happened, so they stop being invisible.
//!
//! The idea is from jcode's `kv_cache_monitor`.

// Rust guideline compliant 2026-10-09

use std::hash::{Hash as _, Hasher as _};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// A request whose input is below this (tokens) is too small to expect a
/// cache hit, so a cold read on it isn't reported as a miss.
const FLOOR_INPUT: u64 = 2048;

/// A shortfall below this (tokens) is within a provider's cache-block
/// granularity: noise, not a miss (unless the harness caused it).
const FLOOR_MISS: u64 = 1024;

/// A gap longer than this since the last request leaves the cache cold on
/// its own (providers drop an idle prompt cache after a few minutes).
const EXPIRE: Duration = Duration::from_mins(8);

/// Seconds in a day, for telling which UTC day a request fell on.
const DAY: u64 = 24 * 60 * 60;

/// Why a request read less from the prompt cache than its prefix should
/// have left warm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Miss {
    /// The provider changed, so the previous cache doesn't apply.
    ProviderSwitch,
    /// The model changed.
    ModelSwitch,
    /// The system prompt changed mid-session (the harness's own fault).
    SystemChanged,
    /// The tool set changed mid-session (the harness's own fault).
    ToolsChanged,
    /// An already-sent earlier message changed (the harness's own fault).
    PrefixChanged,
    /// The history shrank: it was compacted.
    Compaction,
    /// The provider reset its prompt cache at 00:00 UTC.
    DailyRollover,
    /// Too long passed since the last request; the cache went cold.
    Expired,
    /// The provider read nothing from the cache, for no clear reason.
    ZeroRead,
    /// The provider read only part of the warm prefix, for no clear reason.
    LowRead,
}

impl Miss {
    /// A stable snake_case identifier, for logs and the wire.
    pub fn id(self) -> &'static str {
        match self {
            Self::ProviderSwitch => "provider_switch",
            Self::ModelSwitch => "model_switch",
            Self::SystemChanged => "system_changed",
            Self::ToolsChanged => "tools_changed",
            Self::PrefixChanged => "prefix_changed",
            Self::Compaction => "compaction",
            Self::DailyRollover => "daily_rollover",
            Self::Expired => "expired",
            Self::ZeroRead => "zero_read",
            Self::LowRead => "low_read",
        }
    }

    /// A short human-readable explanation.
    pub fn detail(self) -> &'static str {
        match self {
            Self::ProviderSwitch => "the provider changed",
            Self::ModelSwitch => "the model changed",
            Self::SystemChanged => "the system prompt changed mid-session",
            Self::ToolsChanged => "the tool set changed mid-session",
            Self::PrefixChanged => "an earlier message changed",
            Self::Compaction => "the context was compacted",
            Self::DailyRollover => "the provider resets its cache at 00:00 UTC",
            Self::Expired => "too long passed since the last request",
            Self::ZeroRead => "the provider read nothing from the cache",
            Self::LowRead => "the provider read only part of the cache",
        }
    }

    /// Whether the harness changed its own cached prefix: these should
    /// essentially never happen, and point to a bug when they do.
    pub fn harness_caused(self) -> bool {
        matches!(self, Self::SystemChanged | Self::ToolsChanged | Self::PrefixChanged)
    }
}

/// One classified miss: its reason and how many tokens went uncached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub reason: Miss,
    /// Tokens the request should have read from the cache but didn't.
    pub missed: u64,
}

/// What one request looked like, kept to compare against the next.
struct Shape {
    model: String,
    provider: String,
    system: u64,
    tools: u64,
    history_len: usize,
    history: u64,
    at: SystemTime,
    input: u64,
}

/// Remembers the last request of a session and classifies the next one's
/// cache miss.
#[derive(Default)]
pub struct Monitor {
    last: Option<Shape>,
}

impl Monitor {
    /// Records a completed request and returns a [`Report`] when it missed
    /// the cache by more than noise, `None` when the cache worked (or there
    /// is no previous request to compare against yet).
    ///
    /// `system` and `tools` are the request's system prompt and tool list;
    /// `history` is the messages sent; `input`/`cached` are the tokens the
    /// provider reported.
    #[expect(clippy::too_many_arguments, reason = "a request's cache-relevant fields; grouping them into a struct would only move the noise")]
    pub fn observe(&mut self, model: &str, provider: &str, system: &str, tools: &Value, history: &[Value], input: u64, cached: u64) -> Option<Report> {
        self.observe_at(model, provider, system, tools, history, input, cached, SystemTime::now())
    }

    /// [`Monitor::observe`] with the request's time given, for tests.
    #[expect(clippy::too_many_arguments, reason = "mirrors observe, plus the time")]
    fn observe_at(&mut self, model: &str, provider: &str, system: &str, tools: &Value, history: &[Value], input: u64, cached: u64, at: SystemTime) -> Option<Report> {
        let cur = Shape {
            model: model.to_owned(),
            provider: provider.to_owned(),
            system: hash_str(system),
            tools: hash_str(&tools.to_string()),
            history_len: history.len(),
            history: hist_hash(history, history.len()),
            at,
            input,
        };
        let report = self.last.take().and_then(|prev| classify(&prev, &cur, history, cached));
        self.last = Some(cur);
        report
    }
}

/// Classifies `cur` against the previous request `prev`; `history` is the
/// current history (to re-hash `prev`'s prefix), `cached` the tokens read.
fn classify(prev: &Shape, cur: &Shape, history: &[Value], cached: u64) -> Option<Report> {
    // What the previous request's total is now a cacheable prefix, so a
    // healthy cache reads about that much. Everything after it (the new
    // messages and the reply) is legitimately uncached.
    let expected = prev.input;
    let missed = expected.saturating_sub(cached);

    // The cause, most specific first.
    let (reason, harness) = if cur.provider != prev.provider {
        (Miss::ProviderSwitch, false)
    } else if cur.model != prev.model {
        (Miss::ModelSwitch, false)
    } else if cur.system != prev.system {
        (Miss::SystemChanged, true)
    } else if cur.tools != prev.tools {
        (Miss::ToolsChanged, true)
    } else if cur.history_len < prev.history_len {
        (Miss::Compaction, false)
    } else if hist_hash(history, prev.history_len) != prev.history {
        (Miss::PrefixChanged, true)
    } else {
        // The prefix is intact and the model, system and tools are the same,
        // so a warm cache would have read it. If it didn't, the cause is
        // outside the request.
        if missed < max(FLOOR_MISS, expected / 10) || expected < FLOOR_INPUT {
            return None;
        }
        let reason = if rolls_daily(&cur.provider) && utc_day(cur.at) != utc_day(prev.at) {
            Miss::DailyRollover
        } else if cur.at.duration_since(prev.at).unwrap_or_default() > EXPIRE {
            Miss::Expired
        } else if cached == 0 {
            Miss::ZeroRead
        } else {
            Miss::LowRead
        };
        return Some(Report { reason, missed });
    };

    // A harness-caused miss is reported however small; a switch or
    // compaction only when it cost more than noise.
    if !harness && missed < FLOOR_MISS {
        return None;
    }
    Some(Report { reason, missed })
}

/// Whether the provider resets its prompt cache daily at 00:00 UTC.
fn rolls_daily(provider: &str) -> bool {
    let p = provider.to_ascii_lowercase();
    ["chatgpt", "codex", "openai", "anthropic", "claude"].iter().any(|k| p.contains(k))
}

/// Which UTC day a time fell on (days since the epoch).
fn utc_day(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / DAY
}

/// The larger of two token counts.
fn max(a: u64, b: u64) -> u64 {
    if a > b { a } else { b }
}

/// A 64-bit hash of a string.
fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// A 64-bit hash of the first `upto` messages of `history`.
fn hist_hash(history: &[Value], upto: usize) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for m in &history[..upto.min(history.len())] {
        m.to_string().hash(&mut h);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(text: &str) -> Value {
        json!({ "role": "user", "content": text })
    }

    fn history(n: usize) -> Vec<Value> {
        (0..n).map(|i| msg(&format!("m{i}"))).collect()
    }

    #[test]
    fn the_first_request_has_no_baseline_to_miss() {
        let mut m = Monitor::default();
        assert_eq!(m.observe("codex/g", "codex", "sys", &json!([]), &history(2), 5000, 0), None);
    }

    #[test]
    fn a_warm_cache_reports_nothing() {
        let mut m = Monitor::default();
        m.observe("codex/g", "codex", "sys", &json!([]), &history(2), 5000, 0);
        // The prefix (≈5000) is read, plus new uncached input.
        assert_eq!(m.observe("codex/g", "codex", "sys", &json!([]), &history(4), 6000, 5000), None);
    }

    #[test]
    fn a_cold_read_with_no_cause_is_zero_read() {
        let mut m = Monitor::default();
        m.observe("codex/g", "codex", "sys", &json!([]), &history(2), 5000, 4800);
        let r = m.observe("codex/g", "codex", "sys", &json!([]), &history(4), 6000, 0).unwrap();
        assert_eq!(r.reason, Miss::ZeroRead);
        assert_eq!(r.missed, 5000);
        assert!(!r.reason.harness_caused());
    }

    #[test]
    fn a_partial_read_is_low_read() {
        let mut m = Monitor::default();
        m.observe("codex/g", "codex", "sys", &json!([]), &history(2), 5000, 4800);
        let r = m.observe("codex/g", "codex", "sys", &json!([]), &history(4), 6000, 1500).unwrap();
        assert_eq!(r.reason, Miss::LowRead);
        assert_eq!(r.missed, 3500);
    }

    #[test]
    fn a_changed_system_prompt_is_harness_caused() {
        let mut m = Monitor::default();
        m.observe("codex/g", "codex", "sys", &json!([]), &history(2), 5000, 4800);
        let r = m.observe("codex/g", "codex", "SYS-CHANGED", &json!([]), &history(4), 6000, 0).unwrap();
        assert_eq!(r.reason, Miss::SystemChanged);
        assert!(r.reason.harness_caused());
    }

    #[test]
    fn changed_tools_beat_a_cold_read() {
        let mut m = Monitor::default();
        m.observe("codex/g", "codex", "sys", &json!([{ "name": "read" }]), &history(2), 5000, 4800);
        let r = m.observe("codex/g", "codex", "sys", &json!([{ "name": "read" }, { "name": "grep" }]), &history(4), 6000, 0).unwrap();
        assert_eq!(r.reason, Miss::ToolsChanged);
    }

    #[test]
    fn an_edited_earlier_message_is_prefix_changed() {
        let mut m = Monitor::default();
        let mut h = history(3);
        m.observe("codex/g", "codex", "sys", &json!([]), &h, 5000, 4800);
        h[0] = msg("edited");
        h.push(msg("m3"));
        let r = m.observe("codex/g", "codex", "sys", &json!([]), &h, 6000, 0).unwrap();
        assert_eq!(r.reason, Miss::PrefixChanged);
        assert!(r.reason.harness_caused());
    }

    #[test]
    fn a_shorter_history_is_compaction() {
        let mut m = Monitor::default();
        m.observe("codex/g", "codex", "sys", &json!([]), &history(10), 20000, 18000);
        let r = m.observe("codex/g", "codex", "sys", &json!([]), &history(3), 6000, 0).unwrap();
        assert_eq!(r.reason, Miss::Compaction);
    }

    #[test]
    fn a_model_switch_wins_over_the_cold_read_it_causes() {
        let mut m = Monitor::default();
        m.observe("codex/g", "codex", "sys", &json!([]), &history(2), 5000, 4800);
        let r = m.observe("codex/h", "codex", "sys", &json!([]), &history(4), 6000, 0).unwrap();
        assert_eq!(r.reason, Miss::ModelSwitch);
    }

    #[test]
    fn a_long_gap_is_expired_not_zero_read() {
        let mut m = Monitor::default();
        let t0 = UNIX_EPOCH + Duration::from_secs(1_000_000);
        m.observe_at("codex/g", "codex", "sys", &json!([]), &history(2), 5000, 4800, t0);
        let late = t0 + Duration::from_mins(20);
        let r = m.observe_at("codex/g", "codex", "sys", &json!([]), &history(4), 6000, 0, late).unwrap();
        assert_eq!(r.reason, Miss::Expired);
    }

    #[test]
    fn midnight_utc_is_daily_rollover() {
        let mut m = Monitor::default();
        // 00:30 UTC one day, then 00:10 UTC the next (a 23.6 h gap is over
        // EXPIRE, but crossing midnight is the more specific cause).
        let t0 = UNIX_EPOCH + Duration::from_secs(100 * DAY + 1800);
        m.observe_at("codex/g", "codex", "sys", &json!([]), &history(2), 5000, 4800, t0);
        let next = UNIX_EPOCH + Duration::from_secs(101 * DAY + 600);
        let r = m.observe_at("codex/g", "codex", "sys", &json!([]), &history(4), 6000, 0, next).unwrap();
        assert_eq!(r.reason, Miss::DailyRollover);
    }

    #[test]
    fn a_small_request_is_not_a_miss() {
        let mut m = Monitor::default();
        m.observe("codex/g", "codex", "sys", &json!([]), &history(1), 500, 0);
        assert_eq!(m.observe("codex/g", "codex", "sys", &json!([]), &history(2), 900, 0), None);
    }
}
