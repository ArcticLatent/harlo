//! Schedules: OpenCode's automations that send a prompt to a session, as
//! Harlo names and describes them. The server keeps and runs them (see
//! [`crate::server::Automation`]); this is how their times read.

// Rust guideline compliant 2026-10-09

use jiff::tz::TimeZone;
use jiff::{Timestamp, ZonedDateTime};

use crate::server::AutomationTrigger as Trigger;

/// How a new schedule's time is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum When {
    /// Once, at a time or after a while.
    #[default]
    Once,
    /// Again and again, a while apart.
    Every,
    /// On a cron schedule.
    Cron,
}

impl When {
    /// Every kind, in the order the form lists them.
    pub const ALL: [Self; 3] = [Self::Once, Self::Every, Self::Cron];

    /// Its name in the form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Once => "Once",
            Self::Every => "Every",
            Self::Cron => "Cron",
        }
    }

    /// What the time field shows while empty.
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            Self::Once => "in 20 minutes, or 2026-12-01T09:00",
            Self::Every => "30 minutes, 2 hours or 1 day",
            Self::Cron => "0 9 * * 1-5",
        }
    }

    /// What the time field means.
    #[must_use]
    pub fn help(self) -> &'static str {
        match self {
            Self::Once => "A time on the server's clock, or how long from now.",
            Self::Every => {
                "Intervals must be at least one minute. The first run is one interval from now."
            }
            Self::Cron => {
                "Minute, hour, day of the month, month and day of the week, on the server's clock: \
                 0 9 * * 1-5 is weekdays at 09:00."
            }
        }
    }

    /// The trigger for `time`, as the server reads it.
    #[must_use]
    pub fn trigger(self, time: &str) -> Trigger {
        let time = time.trim().to_owned();
        match self {
            Self::Once => Trigger::At(time),
            Self::Every => Trigger::Every(time),
            Self::Cron => Trigger::Cron(time),
        }
    }
}

/// When `trigger` runs, in words, with times shown in `zone` against
/// `now`.
#[must_use]
pub fn describe(trigger: &Trigger, now: &ZonedDateTime) -> String {
    match trigger {
        Trigger::At(at) => match at.parse::<Timestamp>() {
            Ok(instant) => format!("Once, {}", moment_at(instant, now)),
            Err(_) => format!("Once, {at}"),
        },
        // "Every day", not "Every 1 day".
        Trigger::Every(every) => match every.strip_prefix("1 ") {
            Some(unit) => format!("Every {unit}"),
            None => format!("Every {every}"),
        },
        Trigger::Cron(cron) => cron_words(cron).unwrap_or_else(|| format!("Cron {cron}")),
        Trigger::Event { server, event } => format!("When {server} sends {event}"),
    }
}

/// When `trigger` runs, in words, in this computer's time zone.
#[must_use]
pub fn describe_now(trigger: &Trigger) -> String {
    describe(trigger, &now())
}

/// A time `ms` milliseconds after the Unix epoch, against `now`: `today
/// 14:30`, `tomorrow 09:00`, `Mon 24 Nov, 09:00`, or with the year when
/// it isn't this one.
#[must_use]
pub fn moment(ms: f64, now: &ZonedDateTime) -> String {
    // Milliseconds since 1970 fit an i64 for any real date.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a real date's milliseconds fit an i64"
    )]
    let millis = ms.round() as i64;
    match Timestamp::from_millisecond(millis) {
        Ok(instant) => moment_at(instant, now),
        Err(_) => String::new(),
    }
}

/// [`moment`] in this computer's time zone.
#[must_use]
pub fn moment_now(ms: f64) -> String {
    moment(ms, &now())
}

/// `instant` as [`moment`] words it.
fn moment_at(instant: Timestamp, now: &ZonedDateTime) -> String {
    let at = instant.to_zoned_datetime(now.time_zone().clone());
    let clock = at.strftime("%H:%M");
    let (day, today) = (at.date(), now.date());
    if day == today {
        format!("today {clock}")
    } else if today.tomorrow().is_ok_and(|t| t == day) {
        format!("tomorrow {clock}")
    } else if today.yesterday().is_ok_and(|y| y == day) {
        format!("yesterday {clock}")
    } else if day.year() == today.year() {
        format!("{}, {clock}", at.strftime("%a %-d %b"))
    } else {
        format!("{}, {clock}", at.strftime("%-d %b %Y"))
    }
}

/// The time now, in this computer's time zone (UTC when it can't be read).
fn now() -> ZonedDateTime {
    Timestamp::now().to_zoned_datetime(TimeZone::try_system().unwrap_or(TimeZone::UTC))
}

/// Common cron schedules in words: a fixed minute and hour every day, on
/// weekdays, or on one day of the week.
fn cron_words(cron: &str) -> Option<String> {
    /// Days of the week, as cron numbers them from Sunday.
    const DAYS: [&str; 7] = [
        "Sundays",
        "Mondays",
        "Tuesdays",
        "Wednesdays",
        "Thursdays",
        "Fridays",
        "Saturdays",
    ];
    let fields: Vec<&str> = cron.split_whitespace().collect();
    let [minute, hour, "*", "*", weekday] = fields.as_slice() else {
        return None;
    };
    let minute: u8 = minute.parse().ok().filter(|m| *m < 60)?;
    let hour: u8 = hour.parse().ok().filter(|h| *h < 24)?;
    let clock = format!("{hour:02}:{minute:02}");
    let days = match *weekday {
        "*" => "Every day".to_owned(),
        "1-5" => "Weekdays".to_owned(),
        "0,6" | "6,0" => "Weekends".to_owned(),
        day => {
            let n: usize = day.parse().ok()?;
            (*DAYS.get(n % 7)?).to_owned()
        }
    };
    Some(format!("{days} at {clock}"))
}
