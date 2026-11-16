//! Scheduled prompts that start a new session in a project.
//!
//! On chosen days at a time, every N minutes, or once. They run while harlo
//! does; a run missed while it was closed can be caught up at startup.

// Rust guideline compliant 2026-10-09

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
    /// Without a project: the machine whose home folder it runs in ("" this
    /// one), the one shown when it was made.
    pub host: String,
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
                if days.trailing_zeros() >= 7 {
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
            Repeat::Every { minutes } => (minutes > 0).then(|| after + Duration::minutes(i64::from(minutes))),
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
            "days" => Repeat::Days { days: u8::try_from(n("days")?).unwrap_or(0), hour: u32::try_from(n("hour")?.clamp(0, 23)).unwrap_or(0), minute: u32::try_from(n("minute")?.clamp(0, 59)).unwrap_or(0) },
            "every" => Repeat::Every { minutes: u32::try_from(n("minutes")?.max(1)).unwrap_or(u32::MAX) },
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
            "id": self.id, "title": self.title, "prompt": self.prompt, "projectId": self.project_id, "host": self.host,
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
            host: s("host"),
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

    /// A schedule from the schedule dialog's form, timed from `now`.
    ///
    /// `previous` is the schedule it edits, if any: its machine and last run
    /// are kept. A new one without a project runs on `host`, the machine
    /// shown when it was made ("" this one). An empty title is the prompt's
    /// first line, shortened.
    ///
    /// # Errors
    ///
    /// What to correct, for the dialog: no prompt, a time or date that isn't
    /// one, no days, an interval under a minute, a time already passed, or
    /// no choice of when it runs.
    pub fn from_form(form: &Value, previous: Option<&Schedule>, host: &str, now: DateTime<Local>) -> Result<Schedule, &'static str> {
        let prompt = form["prompt"].as_str().unwrap_or_default().trim().to_owned();
        if prompt.is_empty() {
            return Err("Write the prompt to send.");
        }
        let r = &form["repeat"];
        let repeat = match r["kind"].as_str().unwrap_or_default() {
            "days" => {
                let (hour, minute) = parse_time(r["time"].as_str().unwrap_or_default()).ok_or("Use a time like 09:00.")?;
                let days = u8::try_from(r["days"].as_u64().unwrap_or(0) & 0x7f).unwrap_or(0);
                if days == 0 {
                    return Err("Pick at least one day.");
                }
                Repeat::Days { days, hour, minute }
            }
            "every" => {
                let minutes = r["minutes"].as_u64().unwrap_or(0);
                if minutes < 1 {
                    return Err("Use an interval of at least a minute.");
                }
                Repeat::Every { minutes: u32::try_from(minutes).unwrap_or(u32::MAX) }
            }
            "once" => {
                let at = parse_local(r["at"].as_str().unwrap_or_default()).ok_or("Use a date and time like 2026-10-13 09:00.")?;
                if at <= now.timestamp() {
                    return Err("That time has passed.");
                }
                Repeat::Once { at }
            }
            _ => return Err("Choose when it runs."),
        };
        let title = form["title"].as_str().unwrap_or_default().trim().to_owned();
        let mut schedule = Schedule {
            id: form["id"].as_i64().unwrap_or(0),
            title: if title.is_empty() { prompt.lines().next().unwrap_or_default().chars().take(48).collect() } else { title },
            prompt,
            project_id: form["projectId"].as_i64().unwrap_or(-1),
            host: previous.map_or_else(|| host.to_owned(), |p| p.host.clone()),
            harness: form["harness"].as_str().unwrap_or_default().to_owned(),
            model: form["model"].as_str().unwrap_or_default().trim().to_owned(),
            repeat,
            enabled: form["enabled"].as_bool().unwrap_or(true),
            catch_up: form["catchUp"].as_bool().unwrap_or(true),
            last_run: previous.and_then(|p| p.last_run),
            next_run: None,
        };
        schedule.reschedule(now);
        Ok(schedule)
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
        Schedule { id: 1, title: "t".into(), prompt: "p".into(), project_id: -1, host: String::new(), harness: String::new(), model: String::new(), repeat, enabled: true, catch_up: true, last_run: None, next_run: None }
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

    #[test]
    fn the_dialogs_form_becomes_a_schedule() {
        let now = at(2026, 10, 12, 8, 0);
        let form = json!({ "prompt": " Review the open PRs\nand say which\n", "projectId": 3, "repeat": { "kind": "days", "time": "09:00", "days": 0x1f } });
        let s = Schedule::from_form(&form, None, "nas", now).unwrap();
        assert_eq!((s.title.as_str(), s.prompt.as_str(), s.project_id, s.host.as_str()), ("Review the open PRs", "Review the open PRs\nand say which", 3, "nas"));
        assert_eq!(s.next_run, Some(at(2026, 10, 12, 9, 0).timestamp()));
        assert!(s.enabled && s.catch_up);

        let previous = Schedule { host: String::new(), last_run: Some(7), ..s.clone() };
        let edited = Schedule::from_form(&json!({ "id": 4, "prompt": "p", "title": " T ", "repeat": { "kind": "every", "minutes": 30 } }), Some(&previous), "nas", now).unwrap();
        assert_eq!((edited.id, edited.title.as_str(), edited.host.as_str(), edited.last_run), (4, "T", "", Some(7)));
        assert_eq!(edited.repeat, Repeat::Every { minutes: 30 });
    }

    #[test]
    fn a_form_says_what_to_correct() {
        let now = at(2026, 10, 12, 8, 0);
        let with = |repeat: Value| Schedule::from_form(&json!({ "prompt": "p", "repeat": repeat }), None, "", now).unwrap_err();
        assert_eq!(Schedule::from_form(&json!({ "prompt": "  " }), None, "", now).unwrap_err(), "Write the prompt to send.");
        assert_eq!(with(json!({ "kind": "days", "time": "9am", "days": 1 })), "Use a time like 09:00.");
        assert_eq!(with(json!({ "kind": "days", "time": "09:00", "days": 0x80 })), "Pick at least one day.");
        assert_eq!(with(json!({ "kind": "every", "minutes": 0 })), "Use an interval of at least a minute.");
        assert_eq!(with(json!({ "kind": "once", "at": "2026-10-12 07:00" })), "That time has passed.");
        assert_eq!(with(json!({})), "Choose when it runs.");
    }
}
