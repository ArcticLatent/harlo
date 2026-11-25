//! A long session's history, read a stretch at a time: the end when the
//! session opens, and what came before as the reader goes back to it.
//!
//! The timeline's own part is tested as it is; the scrolling in the real
//! window, headless.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::Message;
use harlo::timeline::{Event as TimelineEvent, Stretch, Timeline};
use harlo_core::timeline::Entry;
use i_slint_backend_testing::AccessibleRole;
use support::{Ui, demo, ui};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn prompt(n: usize) -> Entry {
    Entry::Prompt {
        key: format!("msg_p{n}"),
        text: format!("Question number {n}"),
        attachments: Vec::new(),
        pictures: Vec::new(),
        skills: Vec::new(),
        agent: None,
    }
}

fn answer(n: usize) -> Entry {
    Entry::Answer {
        key: format!("msg_a{n}/text/0"),
        text: "A paragraph long enough to wrap onto a second line of the \
               transcript, so rows differ in height. "
            .repeat(1 + n % 4),
    }
}

/// Questions `from..to`, each with its answer.
fn questions(from: usize, to: usize) -> Vec<Entry> {
    (from..to).flat_map(|n| [prompt(n), answer(n)]).collect()
}

/// A stretch with history before it, starting at `earlier`.
fn after(earlier: &str, entries: Vec<Entry>) -> Stretch {
    Stretch {
        entries,
        earlier: Some(earlier.to_owned()),
        opened_by: None,
    }
}

fn keys(timeline: &Timeline) -> Vec<&str> {
    timeline.entries().map(Entry::key).collect()
}

#[test]
fn earlier_history_goes_above_the_rows_until_there_is_none() {
    let mut timeline = Timeline::new("ses");
    timeline.set_end(after("at-4", questions(4, 6)));
    assert!(timeline.has_earlier());
    assert_eq!(timeline.fetch_earlier().as_deref(), Some("at-4"));
    // On its way: asking again starts nothing.
    assert!(!timeline.has_earlier());
    assert_eq!(timeline.fetch_earlier(), None);

    timeline.add_earlier("at-4", after("at-2", questions(2, 4)));
    assert_eq!(timeline.fetch_earlier().as_deref(), Some("at-2"));
    timeline.add_earlier("at-2", questions(0, 2).into());
    let expected = questions(0, 6);
    let expected: Vec<&str> = expected.iter().map(Entry::key).collect();
    assert_eq!(keys(&timeline), expected);
    assert!(!timeline.has_earlier(), "the start is reached");
    assert_eq!(timeline.fetch_earlier(), None);
}

#[test]
fn a_prompt_answered_in_the_next_stretch_gets_its_agent() {
    let mut timeline = Timeline::new("ses");
    // The stretch opens with the answer to a prompt before it.
    timeline.set_end(Stretch {
        entries: vec![answer(1)],
        earlier: Some("at-1".to_owned()),
        opened_by: Some("plan".to_owned()),
    });
    timeline.add_earlier("at-1", vec![prompt(0), answer(0), prompt(1)].into());
    let agents: Vec<Option<&str>> = timeline
        .entries()
        .filter_map(|entry| match entry {
            Entry::Prompt { agent, .. } => Some(agent.as_deref()),
            _ => None,
        })
        .collect();
    // (The first prompt's answer named no agent here; the last one's is
    // in the rows that were already shown.)
    assert_eq!(agents, [None, Some("plan")]);
}

#[test]
fn a_reload_keeps_what_was_read_from_before_it() {
    let mut timeline = Timeline::new("ses");
    timeline.set_end(after("at-4", questions(4, 6)));
    timeline.add_earlier("at-4", after("at-2", questions(2, 4)));

    // The end again, one question on: it joins the rows shown.
    timeline.set_end(after("at-5", questions(5, 7)));
    let expected = questions(2, 7);
    let expected: Vec<&str> = expected.iter().map(Entry::key).collect();
    assert_eq!(keys(&timeline), expected);
    // What came before the rows still starts where it did.
    assert_eq!(timeline.fetch_earlier().as_deref(), Some("at-2"));

    // An end that joins nothing shown is all there is, with its own start;
    // the stretch asked for before it is dropped when it comes.
    timeline.set_end(after("at-40", questions(40, 42)));
    timeline.add_earlier("at-2", questions(0, 2).into());
    let expected = questions(40, 42);
    let expected: Vec<&str> = expected.iter().map(Entry::key).collect();
    assert_eq!(keys(&timeline), expected);
    assert_eq!(timeline.fetch_earlier().as_deref(), Some("at-40"));
}

/// Whether the timeline shows a row reading `text`, not counting the rail's
/// mark for a prompt of the same words.
fn in_view(ui: &Ui, text: &str) -> bool {
    ui.find(text)
        .iter()
        .any(|e| e.accessible_role() != Some(AccessibleRole::Button))
}

/// Lets the list lay its rows out, as the event loop's next turns would.
fn settle(ui: &Ui, shown: &str) {
    for _ in 0..5 {
        let _ = in_view(ui, shown);
        ui.wait(50);
    }
}

/// The questions whose prompts are in view.
fn prompts_in_view(ui: &Ui) -> Vec<usize> {
    (0..120)
        .filter(|n| in_view(ui, &format!("Question number {n}")))
        .collect()
}

/// A question whose prompt is in the middle of the view: whole, so the
/// pointer can rest on it.
fn mid_view(ui: &Ui) -> usize {
    let shown = prompts_in_view(ui);
    *shown.get(shown.len() / 2).expect("a prompt in view")
}

#[test]
fn going_back_to_the_start_asks_for_what_came_before_and_keeps_the_place() {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::Recent(
        PI_UPDATE.into(),
        Ok(after("at-60", questions(60, 120))),
    ));
    let ui = ui(app);
    settle(&ui, "Question number 119");
    assert!(in_view(&ui, "Question number 119"), "it opens at its end");
    let asked = |ui: &Ui| {
        ui.messages()
            .iter()
            .any(|m| matches!(m, Message::Timeline(_, TimelineEvent::NearStart)))
    };
    assert!(!asked(&ui), "nothing is asked for at the end");

    // Back to the first row shown.
    for _ in 0..200 {
        if in_view(&ui, "Question number 60") {
            break;
        }
        let shown = mid_view(&ui);
        ui.wheel(&format!("Question number {shown}"), 1);
    }
    assert!(in_view(&ui, "Question number 60"), "the first row shown");
    let before = prompts_in_view(&ui);
    assert!(asked(&ui), "what came before is asked for");

    // It comes, above the rows: the reader is where they were.
    ui.send(Message::Earlier(
        PI_UPDATE.into(),
        "at-60".into(),
        Ok(questions(0, 60).into()),
    ));
    settle(&ui, "Question number 60");
    assert_eq!(prompts_in_view(&ui), before, "the place is kept");
    assert_eq!(ui.app().timeline_len(PI_UPDATE), Some(240));
    // And going back goes on, into what came.
    let _ = ui.messages();
    for _ in 0..40 {
        if in_view(&ui, "Question number 57") {
            break;
        }
        let shown = mid_view(&ui);
        ui.wheel(&format!("Question number {shown}"), 1);
    }
    assert!(in_view(&ui, "Question number 57"), "earlier rows show");
    assert!(
        !asked(&ui),
        "with the start read, nothing more is asked for"
    );
}
