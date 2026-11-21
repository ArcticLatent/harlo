//! Where a session's timeline stays while rows change: at the end while the
//! user follows it, and on what they're reading once they've scrolled up.

// Rust guideline compliant 2026-10-09

use harlo::timeline::{Event, Timeline};
use harlo_core::timeline::Entry;
use iced::Size;
use iced::time::Instant;

const ROW: f32 = 100.0;
const SPACING: f32 = 14.0;
const VIEW: f32 = 600.0;

fn answer(i: usize) -> Entry {
    Entry::Answer {
        key: format!("a{i}"),
        text: format!("Answer {i}"),
    }
}

/// A timeline of `n` measured answers, `ROW` tall each.
fn timeline(n: usize) -> (Timeline, Vec<Entry>) {
    let mut timeline = Timeline::new("ses_scroll");
    let entries: Vec<Entry> = (0..n).map(answer).collect();
    timeline.set(entries.clone());
    for i in 0..n {
        timeline.update(&Event::Measured(format!("a{i}"), Size::new(700.0, ROW)));
    }
    (timeline, entries)
}

/// Reports a scroll, as the scrollable does, and lets any requested scroll settle.
fn scroll(timeline: &mut Timeline, from_bottom: f32) {
    timeline.update(&Event::Scrolled {
        from_bottom,
        height: VIEW,
    });
}

/// Sends the requested scroll and reports it back, as the scrollable would.
fn settle(timeline: &mut Timeline) {
    let _ = timeline.scroll::<()>();
    let at = timeline.from_bottom();
    scroll(timeline, at);
}

#[test]
fn rows_arriving_below_keep_a_scrolled_up_view_in_place() {
    let (mut timeline, mut entries) = timeline(30);
    scroll(&mut timeline, 1_000.0);

    // A new answer streams in below the view.
    entries.push(answer(30));
    timeline.set(entries);
    timeline.update(&Event::Measured("a30".into(), Size::new(700.0, 400.0)));
    assert!(
        (timeline.from_bottom() - (1_000.0 + 400.0 + SPACING)).abs() < 0.5,
        "the view moved away from the end by what was added: {}",
        timeline.from_bottom()
    );

    // A stale report from before the requested scroll doesn't undo it.
    scroll(&mut timeline, 1_000.0);
    assert!((timeline.from_bottom() - 1_414.0).abs() < 0.5);
    settle(&mut timeline);
    assert!((timeline.from_bottom() - 1_414.0).abs() < 0.5);
}

#[test]
fn rows_settling_above_the_view_dont_move_it() {
    let (mut timeline, _) = timeline(30);
    scroll(&mut timeline, 1_000.0);
    // The first answer turns out taller once it's laid out again.
    timeline.update(&Event::Measured("a0".into(), Size::new(700.0, ROW + 50.0)));
    assert!(
        (timeline.from_bottom() - 1_000.0).abs() < 0.5,
        "measured from the end, the view is where it was"
    );
}

#[test]
fn a_view_at_the_end_follows_new_rows() {
    let (mut timeline, mut entries) = timeline(30);
    scroll(&mut timeline, 0.0);
    entries.push(answer(30));
    timeline.set(entries);
    timeline.update(&Event::Measured("a30".into(), Size::new(700.0, 400.0)));
    assert!(
        timeline.from_bottom().abs() < 0.5,
        "the newest row is in view"
    );
}

#[test]
fn scrolling_back_to_the_end_follows_again() {
    let (mut timeline, mut entries) = timeline(30);
    scroll(&mut timeline, 500.0);
    scroll(&mut timeline, 0.0);
    entries.push(answer(30));
    timeline.set(entries);
    assert!(timeline.from_bottom().abs() < 0.5);
}

#[test]
fn sending_a_prompt_scrolls_back_to_the_end() {
    let (mut timeline, _) = timeline(30);
    scroll(&mut timeline, 1_000.0);
    timeline.pend(
        Entry::Prompt {
            key: "msg_new".into(),
            text: "Next".into(),
            attachments: Vec::new(),
            skills: Vec::new(),
        },
        Instant::now(),
    );
    timeline.to_end();
    assert!(
        timeline.from_bottom().abs() < 0.5,
        "the new prompt is in view"
    );
    assert_eq!(timeline.entries().count(), 31);

    timeline.unpend("msg_new");
    assert_eq!(
        timeline.entries().count(),
        30,
        "a failed send takes it back"
    );
}
