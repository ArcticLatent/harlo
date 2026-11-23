//! The composer's `/` menu: typing `/` offers the folder's commands over
//! the field, a click puts one in, and Escape closes it for that word.
//!
//! The catalog is fed in as the server would send it; the matching rules
//! are tested in `harlo-core`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::Message;
use harlo::text::Edit;
use harlo_core::server::{Catalog, Command};
use support::{Ui, demo, ui};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn folder() -> String {
    demo::snapshot()
        .sessions
        .into_iter()
        .find(|s| s.id == PI_UPDATE)
        .map(|s| s.directory)
        .expect("a demo session")
}

fn command(name: &str, description: &str, subagent: bool) -> Command {
    Command {
        name: name.into(),
        description: Some(description.into()),
        agent: None,
        subagent,
    }
}

/// The window with "pi update" open and two commands in its folder.
fn chatting() -> Ui {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let catalog = Catalog {
        commands: vec![
            command("review", "Review what changed", true),
            command("release", "Cut a release", false),
        ],
        ..Catalog::default()
    };
    let _ = app.update(Message::Catalog(folder(), Ok(Box::new(catalog))));
    ui(app)
}

#[test]
fn typing_a_slash_offers_commands_and_a_click_puts_one_in() {
    let ui = chatting();
    assert!(!ui.shows("/review"));

    ui.send(Message::Compose(Edit::at("/re", 3)));
    assert!(ui.shows("/review"), "the menu over the field");
    assert!(ui.shows("/release"));
    assert!(ui.shows("Review what changed · subagent"), "how it runs");

    ui.press("/release");
    assert!(ui.app().menu.is_none(), "picked, it closes");
    assert!(ui.app().composer.as_str().starts_with("/release"));
    assert!(!ui.shows("Review what changed · subagent"));
}

#[test]
fn escape_closes_the_menu_until_the_word_changes() {
    let ui = chatting();
    ui.send(Message::Compose(Edit::at("/", 1)));
    assert!(ui.shows("/review"));
    ui.send(Message::MenuClose);
    assert!(!ui.shows("/review"));
    ui.send(Message::Compose(Edit::at("/r", 2)));
    assert!(!ui.shows("/review"), "closed for that word");
    ui.send(Message::Compose(Edit::at("", 0)));
    ui.send(Message::Compose(Edit::at("/", 1)));
    assert!(ui.shows("/review"), "a new word opens it again");
}
