//! Motion: a prompt that just went out and a newly attached file fade in;
//! what was already there doesn't move. The sidebar's slide is tested in
//! `shell.rs`.
//!
//! The fades themselves run in the window; these tests check what's marked
//! to fade and that the mark goes once the fade is over.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::PathBuf;
use std::time::Duration;

use harlo::app::Message;
use harlo::ui::{Shape, State};
use harlo_core::timeline::Entry;
use slint::{ComponentHandle as _, Model as _};
use support::{Ui, demo, ui};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

/// The session composer's label.
const COMPOSER: &str = "Send a message to continue this session · Tab switches agents";

/// The timeline's prompts, each with whether it's marked to fade in.
fn prompts(ui: &Ui) -> Vec<(String, bool)> {
    let rows = ui.window.global::<State>().get_timeline();
    (0..rows.row_count())
        .filter_map(|i| rows.row_data(i))
        .filter(|row| row.shape == Shape::Prompt)
        .map(|row| (row.label.to_string(), row.fresh))
        .collect()
}

/// The attachment chips, each with whether it's marked to fade in.
fn chips(ui: &Ui) -> Vec<(String, bool)> {
    let chips = ui.window.global::<State>().get_attached();
    (0..chips.row_count())
        .filter_map(|i| chips.row_data(i))
        .map(|chip| (chip.label.to_string(), chip.fresh))
        .collect()
}

fn owned(rows: &[(&str, bool)]) -> Vec<(String, bool)> {
    rows.iter().map(|(s, b)| ((*s).to_owned(), *b)).collect()
}

#[test]
fn a_sent_prompt_fades_in_and_the_history_stays_still() {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(
        PI_UPDATE.into(),
        Ok(vec![Entry::Prompt {
            key: "msg_p1".into(),
            text: "Change the port".into(),
            attachments: Vec::new(),
            skills: Vec::new(),
        }]),
    ));
    let ui = ui(app);
    assert_eq!(prompts(&ui), owned(&[("Change the port", false)]));

    ui.fill(COMPOSER, "Now the host");
    ui.send(Message::Send);
    assert_eq!(
        prompts(&ui),
        owned(&[("Change the port", false), ("Now the host", true)])
    );

    // Once the fade is over, the window shows the prompt still.
    std::thread::sleep(harlo::timeline::APPEAR + Duration::from_millis(30));
    ui.send(Message::Settled);
    assert_eq!(
        prompts(&ui),
        owned(&[("Change the port", false), ("Now the host", false)])
    );
}

#[test]
fn a_new_attachment_fades_in_and_can_be_removed() {
    let ui = ui(demo::app());
    ui.send(Message::Attached(vec![PathBuf::from("/scratch/notes.txt")]));
    assert_eq!(chips(&ui), owned(&[("notes.txt", true)]));

    ui.send(Message::Attached(vec![PathBuf::from("/scratch/plan.md")]));
    assert_eq!(
        chips(&ui),
        owned(&[("notes.txt", false), ("plan.md", true)]),
        "only the new one fades in"
    );

    ui.press("Remove notes.txt");
    assert_eq!(chips(&ui), owned(&[("plan.md", false)]));
    assert_eq!(ui.app().draft.attached.len(), 1);
    assert!(!ui.shows("notes.txt"));
}
