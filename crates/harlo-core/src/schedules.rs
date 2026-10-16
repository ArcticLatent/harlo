//! Scheduled prompts: a prompt that starts a new session in a project on
//! chosen days at a time, every N minutes, or once. They run while harlo
//! does; a run missed while it was closed can be caught up at startup.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Timelike};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repeat {
    /// On the days in `days` (bit 0 = Monday … bit 6 = Sunday) at `hour:minute`.
    Days { days: u8, hour: u32, minute: u32 },
    /// Every `minutes` minutes, counted from the last run (or from saving).
    Every { minutes: u32 },
    /// Once, at this time (unix seconds).
    Once { at: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub id: i64,
    pub title: String,
    pub prompt: String,
    /// -1: the home folder.
    pub project_id: i64,
    /// "" = the project's (or the default) harness.
    pub harness: String,
    /// "" = the harness's default model.
    pub model: String,
    pub repeat: Repeat,
    pub enabled: bool,
    /// Run once at startup when a run was missed while harlo was closed.
    pub catch_up: bool,
    pub last_run: Option<i64>,
    pub next_run: Option<i64>,
}

const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

impl Repeat {
    /// The first run strictly after `after`, if any.
    pub fn next_after(&self, after: DateTime<Local>) -> Option<DateTime<Local>> {
        match *self {
            Repeat::Days { days, hour, minute } => {
                if days & 0x7f == 0 {
                    return None;
                }
                let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
                (0..8).find_map(|offset| {
                    let date = after.date_naive() + Duration::days(offset);
                    let weekday = date.weekday().num_days_from_monday();
                    if days & (1 << weekday) == 0 {
                        return None;
                    }
                    // A time skipped by a clock change does not exist that day.
                    let at = Local.from_local_datetime(&date.and_time(time)).earliest()?;
                    (at > after).then_some(at)
                })
            }
            Repeat::Every { minutes } => (minutes > 0).then(|| after + Duration::minutes(minutes as i64)),
            Repeat::Once { at } => {
                let at = Local.timestamp_opt(at, 0).single()?;
                (at > after).then_some(at)
            }
        }
    }

    /// "Weekdays at 09:00", "Every day at 08:30", "Every 2 hours", …
    pub fn describe(&self) -> String {
        match *self {
            Repeat::Days { days, hour, minute } => {
                let when = match days & 0x7f {
                    0x7f => "Every day".to_owned(),
                    0x1f => "Weekdays".to_owned(),
                    0x60 => "Weekends".to_owned(),
                    d => (0..7).filter(|i| d & (1 << i) != 0).map(|i| DAY_NAMES[i]).collect::<Vec<_>>().join(", "),
                };
                format!("{when} at {hour:02}:{minute:02}")
            }
            Repeat::Every { minutes } if minutes % 60 == 0 && minutes >= 60 => {
                let h = minutes / 60;
                if h == 1 { "Every hour".to_owned() } else { format!("Every {h} hours") }
            }
            Repeat::Every { minutes } => {
                if minutes == 1 { "Every minute".to_owned() } else { format!("Every {minutes} minutes") }
            }
            Repeat::Once { at } => match Local.timestamp_opt(at, 0).single() {
                Some(t) => format!("Once, {}", t.format("%a %Y-%m-%d %H:%M")),
                None => "Once".to_owned(),
            },
        }
    }

    pub fn to_json(&self) -> Value {
        match *self {
            Repeat::Days { days, hour, minute } => json!({ "kind": "days", "days": days, "hour": hour, "minute": minute }),
            Repeat::Every { minutes } => json!({ "kind": "every", "minutes": minutes }),
            Repeat::Once { at } => json!({ "kind": "once", "at": at }),
        }
    }

    pub fn from_json(v: &Value) -> Option<Repeat> {
        let n = |k: &str| v[k].as_i64();
        Some(match v["kind"].as_str()? {
            "days" => Repeat::Days { days: n("days")? as u8, hour: n("hour")?.clamp(0, 23) as u32, minute: n("minute")?.clamp(0, 59) as u32 },
            "every" => Repeat::Every { minutes: n("minutes")?.max(1) as u32 },
            "once" => Repeat::Once { at: n("at")? },
            _ => return None,
        })
    }
}

/// "2026-10-13 09:00" (local time) as unix seconds.
pub fn parse_local(text: &str) -> Option<i64> {
    let (date, time) = text.trim().split_once(' ')?;
    let date = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
    let time = NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()?;
    Local.from_local_datetime(&date.and_time(time)).earliest().map(|t| t.timestamp())
}

/// "09:00" as (hour, minute).
pub fn parse_time(text: &str) -> Option<(u32, u32)> {
    let t = NaiveTime::parse_from_str(text.trim(), "%H:%M").ok()?;
    Some((t.hour(), t.minute()))
}

impl Schedule {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id, "title": self.title, "prompt": self.prompt, "projectId": self.project_id,
            "harness": self.harness, "model": self.model, "repeat": self.repeat.to_json(),
            "enabled": self.enabled, "catchUp": self.catch_up, "lastRun": self.last_run, "nextRun": self.next_run,
        })
    }

    pub fn from_json(v: &Value) -> Option<Schedule> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Some(Schedule {
            id: v["id"].as_i64().unwrap_or(0),
            title: s("title"),
            prompt: s("prompt"),
            project_id: v["projectId"].as_i64().unwrap_or(-1),
            harness: s("harness"),
            model: s("model"),
            repeat: Repeat::from_json(&v["repeat"])?,
            enabled: v["enabled"].as_bool().unwrap_or(true),
            catch_up: v["catchUp"].as_bool().unwrap_or(true),
            last_run: v["lastRun"].as_i64(),
            next_run: v["nextRun"].as_i64(),
        })
    }

    /// Recomputes `next_run` from now (after saving or editing).
    pub fn reschedule(&mut self, now: DateTime<Local>) {
        let from = match self.repeat {
            // Intervals count from the last run.
            Repeat::Every { .. } => self.last_run.and_then(|t| Local.timestamp_opt(t, 0).single()).filter(|t| *t <= now).unwrap_or(now),
            _ => now,
        };
        self.next_run = self.enabled.then(|| self.repeat.next_after(from)).flatten().map(|t| t.timestamp());
        if let Some(n) = self.next_run
            && n <= now.timestamp()
        {
            self.next_run = self.repeat.next_after(now).map(|t| t.timestamp());
        }
    }

    /// Due at `now`?
    pub fn is_due(&self, now: DateTime<Local>) -> bool {
        self.enabled && self.next_run.is_some_and(|n| n <= now.timestamp())
    }

    /// After a run started at `now`: record it and plan the next one.
    /// A one-off schedule switches itself off.
    pub fn ran(&mut self, now: DateTime<Local>) {
        self.last_run = Some(now.timestamp());
        if matches!(self.repeat, Repeat::Once { .. }) {
            self.enabled = false;
            self.next_run = None;
        } else {
            self.next_run = self.repeat.next_after(now).map(|t| t.timestamp());
        }
    }

    /// At startup: a run missed while harlo was closed. Returns true when it
    /// should run now (catch-up); otherwise the schedule moves on.
    pub fn missed_at_startup(&mut self, now: DateTime<Local>) -> bool {
        if !self.is_due(now) {
            return false;
        }
        if self.catch_up {
            return true;
        }
        if matches!(self.repeat, Repeat::Once { .. }) {
            self.enabled = false;
            self.next_run = None;
        } else {
            self.next_run = self.repeat.next_after(now).map(|t| t.timestamp());
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).earliest().unwrap()
    }

    #[test]
    fn days_at_a_time() {
        // 2026-10-12 is a Monday.
        let weekdays = Repeat::Days { days: 0x1f, hour: 9, minute: 0 };
        assert_eq!(weekdays.next_after(at(2026, 10, 12, 8, 0)), Some(at(2026, 10, 12, 9, 0)));
        assert_eq!(weekdays.next_after(at(2026, 10, 12, 9, 0)), Some(at(2026, 10, 13, 9, 0)), "strictly after");
        // Friday evening → Monday.
        assert_eq!(weekdays.next_after(at(2026, 10, 16, 18, 0)), Some(at(2026, 10, 19, 9, 0)));
        let sunday = Repeat::Days { days: 1 << 6, hour: 20, minute: 30 };
        assert_eq!(sunday.next_after(at(2026, 10, 12, 10, 0)), Some(at(2026, 10, 18, 20, 30)));
        assert_eq!(Repeat::Days { days: 0, hour: 9, minute: 0 }.next_after(at(2026, 10, 12, 8, 0)), None);
    }

    #[test]
    fn intervals_and_once() {
        let every = Repeat::Every { minutes: 90 };
        assert_eq!(every.next_after(at(2026, 10, 12, 8, 0)), Some(at(2026, 10, 12, 9, 30)));
        let once = Repeat::Once { at: at(2026, 10, 13, 9, 0).timestamp() };
        assert_eq!(once.next_after(at(2026, 10, 12, 8, 0)), Some(at(2026, 10, 13, 9, 0)));
        assert_eq!(once.next_after(at(2026, 10, 14, 8, 0)), None);
    }

    #[test]
    fn describes() {
        assert_eq!(Repeat::Days { days: 0x1f, hour: 9, minute: 0 }.describe(), "Weekdays at 09:00");
        assert_eq!(Repeat::Days { days: 0x7f, hour: 8, minute: 30 }.describe(), "Every day at 08:30");
        assert_eq!(Repeat::Days { days: 0b101, hour: 10, minute: 5 }.describe(), "Mon, Wed at 10:05");
        assert_eq!(Repeat::Every { minutes: 120 }.describe(), "Every 2 hours");
        assert_eq!(Repeat::Every { minutes: 60 }.describe(), "Every hour");
        assert_eq!(Repeat::Every { minutes: 45 }.describe(), "Every 45 minutes");
        assert!(Repeat::Once { at: at(2026, 10, 13, 9, 0).timestamp() }.describe().starts_with("Once, Tue 2026-10-13 09:00"));
    }

    fn schedule(repeat: Repeat) -> Schedule {
        Schedule { id: 1, title: "t".into(), prompt: "p".into(), project_id: -1, harness: String::new(), model: String::new(), repeat, enabled: true, catch_up: true, last_run: None, next_run: None }
    }

    #[test]
    fn runs_reschedules_and_catches_up() {
        let now = at(2026, 10, 12, 8, 0);
        let mut s = schedule(Repeat::Days { days: 0x7f, hour: 9, minute: 0 });
        s.reschedule(now);
        assert_eq!(s.next_run, Some(at(2026, 10, 12, 9, 0).timestamp()));
        assert!(!s.is_due(now));
        assert!(s.is_due(at(2026, 10, 12, 9, 0)));
        s.ran(at(2026, 10, 12, 9, 0));
        assert_eq!(s.next_run, Some(at(2026, 10, 13, 9, 0).timestamp()));

        // Closed over the next run: catch up once, or skip ahead.
        let later = at(2026, 10, 13, 12, 0);
        assert!(s.clone().missed_at_startup(later));
        s.catch_up = false;
        assert!(!s.missed_at_startup(later));
        assert_eq!(s.next_run, Some(at(2026, 10, 14, 9, 0).timestamp()));

        // A one-off turns itself off after running.
        let mut once = schedule(Repeat::Once { at: at(2026, 10, 12, 9, 0).timestamp() });
        once.reschedule(now);
        once.ran(at(2026, 10, 12, 9, 0));
        assert!(!once.enabled && once.next_run.is_none());

        // Intervals count from the last run; disabled ones have no next run.
        let mut every = schedule(Repeat::Every { minutes: 30 });
        every.last_run = Some(at(2026, 10, 12, 7, 50).timestamp());
        every.reschedule(now);
        assert_eq!(every.next_run, Some(at(2026, 10, 12, 8, 20).timestamp()));
        every.enabled = false;
        every.reschedule(now);
        assert_eq!(every.next_run, None);
    }

    #[test]
    fn json_round_trip_and_parsing() {
        let mut s = schedule(Repeat::Days { days: 0x1f, hour: 9, minute: 15 });
        s.last_run = Some(5);
        assert_eq!(Schedule::from_json(&s.to_json()), Some(s));
        assert_eq!(parse_time("09:15"), Some((9, 15)));
        assert_eq!(parse_time("25:00"), None);
        assert_eq!(parse_local("2026-10-13 09:00"), Some(at(2026, 10, 13, 9, 0).timestamp()));
        assert_eq!(parse_local("tomorrow"), None);
    }
}
