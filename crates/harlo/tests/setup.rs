//! The setup guide: its steps, the name and email on commits, OpenCode's
//! installer, and that it's done with once finished or skipped.
//!
//! Git's setup is fed in as its result; reading and writing git's config
//! is tested against scratch configs in `harlo-core`. Nothing here runs the
//! installer or touches the user's git config: the tasks that would are
//! dropped.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo::app::{Field, Message, Sheet, SheetEvent, Step};
use harlo_core::git::{Identity, Setup};
use harlo_core::server::Update;

use support::{Ui, demo, ui};

/// The guide, opened, with git's setup as `git` reads it.
fn guide(git: Setup) -> Ui {
    let ui = ui(demo::app());
    ui.send(Message::Sheet(SheetEvent::OpenSetup));
    // Its loads would read the real git config: they're dropped.
    drop(ui.driver.take_tasks());
    ui.send(Message::Sheet(SheetEvent::Git(git)));
    ui
}

/// Git as installed, with nothing set.
fn fresh_git() -> Setup {
    Setup {
        version: Some("2.47.1".to_owned()),
        name: String::new(),
        email: String::new(),
    }
}

#[test]
fn the_steps_go_on_and_back_and_finishing_is_remembered() {
    let ui = guide(fresh_git());
    assert_eq!(ui.app().sheets.open, Some(Sheet::Setup));
    assert!(ui.shows("Set up Harlo"));
    assert!(ui.shows("OpenCode runs the agents, in a background service on this computer. Harlo shows them, the work they do and what they ask you."));
    assert!(ui.shows(&format!(
        "Connected · OpenCode {}",
        harlo_core::OPENCODE_VERSION
    )));
    assert!(!ui.shows("Back"), "nothing before the first step");

    ui.press("Next");
    assert_eq!(ui.app().sheets.step, Step::Providers);
    assert!(ui.shows("New sessions"), "the model for new sessions");
    ui.press("Back");
    assert_eq!(ui.app().sheets.step, Step::OpenCode);

    // The list jumps to any step.
    ui.press("Theme");
    assert_eq!(ui.app().sheets.step, Step::Theme);
    assert!(ui.shows("Finish"));
    assert!(ui.shows("Light or dark: System"), "the scheme's choice");
    assert!(!ui.app().preferences.setup_done, "not done while it's open");

    ui.press("Finish");
    assert_eq!(ui.app().sheets.open, None);
    assert!(
        ui.app().preferences.setup_done,
        "it won't open on its own again"
    );
}

#[test]
fn skipping_or_closing_is_done_with_it_too() {
    for how in ["Skip setup", "Close"] {
        let ui = guide(fresh_git());
        ui.press(how);
        assert_eq!(ui.app().sheets.open, None, "{how}");
        assert!(ui.app().preferences.setup_done, "{how}");
    }
}

#[test]
fn settings_opens_it_again() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.press("General");
    ui.press("Open");
    assert_eq!(ui.app().sheets.open, Some(Sheet::Setup));
    assert_eq!(ui.app().sheets.step, Step::OpenCode);
}

#[test]
fn the_name_and_email_on_commits_are_saved() {
    let ui = guide(fresh_git());
    ui.press("Git");
    assert!(ui.shows("2.47.1"));
    assert!(ui.shows("Your name on commits"));

    ui.enter("Name", "Ada Lovelace");
    // No button: the field says it saved, for a moment.
    assert!(ui.find("Save Name").is_empty());
    assert!(ui.shows("Saved"));
    ui.wait(2_000);
    assert!(!ui.shows("Saved"));
    let asked = ui.messages();
    assert!(
        asked.iter().any(|m| matches!(
            m,
            Message::Sheet(SheetEvent::Text(Field::Identity(Identity::Name), name))
                if name == "Ada Lovelace"
        )),
        "{asked:?}"
    );
    // The save would write the real config: it's dropped, and its result
    // fed in.
    assert_eq!(ui.driver.take_tasks().len(), 1, "one save");
    ui.send(Message::Sheet(SheetEvent::IdentitySet(
        Identity::Name,
        Ok("Ada Lovelace".to_owned()),
    )));
    let saved = match &ui.app().sheets.git {
        harlo::app::Load::Loaded(setup) => setup.name.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(saved, "Ada Lovelace");

    // Saving what's there already does nothing.
    ui.send(Message::Sheet(SheetEvent::Text(
        Field::Identity(Identity::Name),
        " Ada Lovelace ".to_owned(),
    )));
    assert!(ui.driver.take_tasks().is_empty(), "nothing to save");

    // Leaving a field saves it too.
    ui.focus("Email");
    ui.fill("Email", "ada@example.invalid");
    ui.focus("Name");
    assert!(
        ui.messages().iter().any(|m| matches!(
            m,
            Message::Sheet(SheetEvent::Text(Field::Identity(Identity::Email), email))
                if email == "ada@example.invalid"
        )),
        "saved on leaving"
    );
    assert_eq!(ui.driver.take_tasks().len(), 1, "one save");

    // A failed save says why.
    ui.send(Message::Sheet(SheetEvent::IdentitySet(
        Identity::Email,
        Err("Couldn't save it: could not lock config file".to_owned()),
    )));
    assert!(ui.shows("Couldn't save it: could not lock config file"));
}

#[test]
fn without_git_it_says_how_to_get_it() {
    let ui = guide(Setup::default());
    ui.press("Git");
    assert!(ui.shows("Not found"));
    assert!(!ui.shows("Your name on commits"));
}

#[test]
fn an_unreachable_service_offers_opencodes_installer() {
    let ui = guide(fresh_git());
    assert!(
        !ui.shows("Install OpenCode"),
        "connected: nothing to install"
    );
    ui.send(Message::Server(Update::Lost {
        reason: "OpenCode isn't installed (no `opencode` on PATH)".to_owned(),
        retry_in: Duration::from_secs(5),
    }));
    assert!(ui.shows("OpenCode isn't installed (no `opencode` on PATH)"));
    assert!(ui.shows("Install OpenCode"));
    assert!(ui.shows("curl -fsSL https://opencode.ai/v2/install | bash"));
    // Pressing it would run the installer for real; that the button is
    // there is enough here.
    assert!(ui.shows("Install"));
}
