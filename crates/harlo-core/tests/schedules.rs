//! How schedules' times read: one-offs, intervals, cron schedules in words,
//! and run times against a fixed "now".

// Rust guideline compliant 2026-10-09

use harlo_core::schedules::{When, describe, moment};
use harlo_core::server::AutomationTrigger as Trigger;
use jiff::ZonedDateTime;

/// Noon on Saturday 22 November 2026, in Toronto.
fn noon() -> ZonedDateTime {
    "2026-11-22T12:00[America/Toronto]".parse().expect("a time")
}

/// Milliseconds since the epoch at `time`, a Toronto wall-clock time.
fn ms(time: &str) -> f64 {
    let zoned: ZonedDateTime = format!("{time}[America/Toronto]").parse().expect("a time");
    #[expect(
        clippy::cast_precision_loss,
        reason = "test times are far from 2^53 ms"
    )]
    let ms = zoned.timestamp().as_millisecond() as f64;
    ms
}

#[test]
fn run_times_read_relative_to_today() {
    let now = noon();
    assert_eq!(moment(ms("2026-11-22T14:30"), &now), "today 14:30");
    assert_eq!(moment(ms("2026-11-23T09:00"), &now), "tomorrow 09:00");
    assert_eq!(moment(ms("2026-11-21T08:05"), &now), "yesterday 08:05");
    assert_eq!(moment(ms("2026-11-26T09:00"), &now), "Thu 26 Nov, 09:00");
    assert_eq!(moment(ms("2027-01-04T09:00"), &now), "4 Jan 2027, 09:00");
}

#[test]
fn triggers_read_in_words() {
    let now = noon();
    let words = |trigger| describe(&trigger, &now);
    assert_eq!(
        words(Trigger::Every("30 minutes".into())),
        "Every 30 minutes"
    );
    assert_eq!(
        words(Trigger::Cron("0 9 * * 1-5".into())),
        "Weekdays at 09:00"
    );
    assert_eq!(
        words(Trigger::Cron("30 7 * * *".into())),
        "Every day at 07:30"
    );
    assert_eq!(
        words(Trigger::Cron("0 18 * * 5".into())),
        "Fridays at 18:00"
    );
    assert_eq!(
        words(Trigger::Cron("*/15 * * * *".into())),
        "Cron */15 * * * *"
    );
    // The server keeps a one-off as the instant it worked out.
    assert_eq!(
        words(Trigger::At("2026-11-23T14:00:00.000Z".into())),
        "Once, tomorrow 09:00"
    );
    assert_eq!(
        words(Trigger::At("in 20 minutes".into())),
        "Once, in 20 minutes"
    );
}

#[test]
fn the_form_makes_the_trigger_it_names() {
    assert_eq!(
        When::Once.trigger(" in 5 minutes "),
        Trigger::At("in 5 minutes".into())
    );
    assert_eq!(
        When::Every.trigger("2 hours"),
        Trigger::Every("2 hours".into())
    );
    assert_eq!(
        When::Cron.trigger("0 9 * * *"),
        Trigger::Cron("0 9 * * *".into())
    );
}
