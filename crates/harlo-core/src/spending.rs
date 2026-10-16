//! Spending limits (Settings → Usage): a session may cost up to `session`
//! dollars, and harlo's sessions together up to `day` dollars a day (0:
//! no limit). Costs are what the agents report (a session's total so far,
//! after each model call; Claude Code after each turn). Past a limit harlo
//! stops the run and holds further messages until you allow more.

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Limits {
    pub session: f64,
    pub day: f64,
}

impl Limits {
    pub fn parse(text: &str) -> Self {
        let v: Value = serde_json::from_str(text).unwrap_or_default();
        let amount = |k: &str| v[k].as_f64().filter(|a| a.is_finite() && *a > 0.0).unwrap_or(0.0);
        Self { session: amount("session"), day: amount("day") }
    }

    pub fn to_json(&self) -> Value {
        json!({ "session": self.session, "day": self.day })
    }
}

/// What harlo's sessions cost today, and how much more was allowed today.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ledger {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    pub total: f64,
    pub extra: f64,
}

impl Ledger {
    pub fn parse(text: &str) -> Self {
        let v: Value = serde_json::from_str(text).unwrap_or_default();
        Self {
            date: v["date"].as_str().unwrap_or_default().to_owned(),
            total: v["total"].as_f64().unwrap_or(0.0).max(0.0),
            extra: v["extra"].as_f64().unwrap_or(0.0).max(0.0),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "date": self.date, "total": self.total, "extra": self.extra })
    }

    /// A new day starts from nothing. True when it did.
    pub fn roll(&mut self, today: &str) -> bool {
        if self.date == today {
            return false;
        }
        *self = Self { date: today.to_owned(), ..Self::default() };
        true
    }

    pub fn add(&mut self, today: &str, amount: f64) {
        self.roll(today);
        self.total += amount.max(0.0);
    }
}

/// A live session's spending: the cost it reported last (the first report
/// is where it started: a resumed session's past isn't today's), and how
/// much more was allowed past the limit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionSpend {
    pub seen: Option<f64>,
    pub cost: f64,
    pub extra: f64,
}

impl SessionSpend {
    /// Its cost so far: what it added since the last report.
    pub fn observe(&mut self, cost: f64) -> f64 {
        let delta = match self.seen {
            Some(was) if cost > was => cost - was,
            _ => 0.0,
        };
        // (A restarted harness counts from 0 again.)
        if self.seen.is_none_or(|was| cost != was) {
            self.seen = Some(cost);
        }
        self.cost = cost;
        delta
    }
}

/// Over a limit: which, and how far.
#[derive(Debug, Clone, PartialEq)]
pub enum Over {
    Session { cost: f64, limit: f64 },
    Day { total: f64, limit: f64 },
}

impl Over {
    pub fn message(&self) -> String {
        match self {
            Over::Session { cost, limit } => format!("This session has cost ${cost:.2}, over its ${limit:.2} limit."),
            Over::Day { total, limit } => format!("harlo's sessions have cost ${total:.2} today, over the ${limit:.2} daily limit."),
        }
    }
}

/// Whether a session (or the day) is over its limit, counting what was
/// allowed past it.
pub fn session_over(limits: &Limits, s: &SessionSpend) -> Option<Over> {
    let limit = limits.session + s.extra;
    (limits.session > 0.0 && s.cost >= limit).then_some(Over::Session { cost: s.cost, limit })
}

pub fn day_over(limits: &Limits, ledger: &Ledger) -> Option<Over> {
    let limit = limits.day + ledger.extra;
    (limits.day > 0.0 && ledger.total >= limit).then_some(Over::Day { total: ledger.total, limit })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_and_ledger() {
        assert_eq!(Limits::parse(""), Limits::default());
        assert_eq!(Limits::parse(r#"{"session":5,"day":-3}"#), Limits { session: 5.0, day: 0.0 });
        let mut l = Ledger::parse("");
        l.add("2026-10-16", 1.5);
        l.add("2026-10-16", 2.0);
        assert_eq!(l.total, 3.5);
        l.extra = 10.0;
        assert_eq!(Ledger::parse(&l.to_json().to_string()), l);
        assert!(l.roll("2026-10-17") && l.total == 0.0 && l.extra == 0.0, "a new day starts from nothing");
        assert!(!l.roll("2026-10-17"));
    }

    #[test]
    fn sessions_count_from_where_they_started() {
        let mut s = SessionSpend::default();
        assert_eq!(s.observe(12.0), 0.0, "a resumed session's past isn't counted");
        assert_eq!(s.observe(12.5), 0.5);
        assert_eq!(s.observe(12.5), 0.0);
        assert_eq!(s.observe(0.2), 0.0, "a restarted harness starts over");
        assert!((s.observe(0.7) - 0.5).abs() < 1e-9);
        assert_eq!(s.cost, 0.7);
    }

    #[test]
    fn over_limits() {
        let limits = Limits { session: 5.0, day: 20.0 };
        let mut s = SessionSpend { cost: 4.99, ..Default::default() };
        assert_eq!(session_over(&limits, &s), None);
        s.cost = 5.2;
        let over = session_over(&limits, &s).unwrap();
        assert_eq!(over.message(), "This session has cost $5.20, over its $5.00 limit.");
        s.extra = 5.0;
        assert_eq!(session_over(&limits, &s), None, "allowed more");
        assert_eq!(session_over(&Limits::default(), &SessionSpend { cost: 99.0, ..Default::default() }), None, "no limit");
        let mut day = Ledger { date: "d".into(), total: 21.0, extra: 0.0 };
        assert_eq!(day_over(&limits, &day).unwrap().message(), "harlo's sessions have cost $21.00 today, over the $20.00 daily limit.");
        day.extra = 20.0;
        assert_eq!(day_over(&limits, &day), None);
    }
}
