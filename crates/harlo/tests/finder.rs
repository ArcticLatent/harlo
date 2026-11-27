//! The finder: Ctrl+K opens one field that finds a session, a page of
//! Settings or something Harlo does by a few letters of its name.
//!
//! The demo's sessions; nothing real is reached.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{Message, Section, Sheet, finder_rows};
use harlo::keys::KeyPress;
use harlo::model::Picker;
use harlo::window::window_key;
use support::{Ui, demo, ui};

/// The finder's field, by what it says while empty.
const FIELD: &str = "Find a session, a setting or something to do";

/// Presses a key anywhere in the window, with Ctrl held when `command`.
fn key(ui: &Ui, text: &str, command: bool) {
    let press = KeyPress::new(text, command, false, false);
    let message = window_key(&ui.app(), &press);
    ui.send(message.unwrap_or_else(|| panic!("{text:?} does nothing here")));
}

/// The names the finder lists, best first.
fn listed(ui: &Ui) -> Vec<String> {
    finder_rows(&ui.app())
        .into_iter()
        .map(|row| row.item.label)
        .collect()
}

#[test]
fn ctrl_k_lists_the_newest_sessions_then_what_harlo_does() {
    let ui = ui(demo::app());
    key(&ui, "k", true);
    assert_eq!(ui.app().picker, Some(Picker::Finder));
    assert!(ui.shows(FIELD));

    let names = listed(&ui);
    assert_eq!(names[0], "hi how are you", "the newest session first");
    assert_eq!(names[3], "pi update");
    assert_eq!(names[8], "New session", "eight sessions, then actions");
    assert!(names.contains(&"Settings: Appearance".to_owned()));
    assert!(names.contains(&"Changes panel".to_owned()));
    assert!(ui.shows("Type a few letters of a name. Start with > to leave sessions out."));

    // The same key closes it, and so does Escape.
    key(&ui, "k", true);
    assert_eq!(ui.app().picker, None);
    key(&ui, "k", true);
    key(&ui, "\u{1b}", false);
    assert_eq!(ui.app().picker, None);
}

#[test]
fn a_few_letters_find_a_session_and_enter_opens_it() {
    let ui = ui(demo::app());
    key(&ui, "k", true);
    ui.fill(FIELD, "pi upd");
    assert_eq!(listed(&ui)[0], "pi update");
    // Every session is searched, not only the newest few.
    ui.fill(FIELD, "older 17");
    assert_eq!(listed(&ui)[0], "Older session 17");

    key(&ui, "\n", false);
    let app = ui.app();
    assert_eq!(app.picker, None);
    assert_eq!(app.active.as_deref(), Some("ses_demo_28"));
}

#[test]
fn letters_apart_find_a_page_of_settings_and_so_do_the_words_for_it() {
    let ui = ui(demo::app());
    key(&ui, "k", true);
    ui.fill(FIELD, "stapp");
    assert_eq!(listed(&ui)[0], "Settings: Appearance");
    // The letters typed are marked in the name.
    let marked = finder_rows(&ui.app()).remove(0).marked;
    assert_eq!(marked.len(), 5, "{marked:?}");

    ui.fill(FIELD, "mcp");
    assert_eq!(listed(&ui)[0], "Settings: Extensions");
    ui.press("Settings: Extensions");
    assert_eq!(ui.app().picker, None);
    assert_eq!(ui.app().sheets.open, Some(Sheet::Settings));
    assert_eq!(ui.app().sheets.section, Section::Extensions);

    // From Settings, another page of it is one more search away.
    key(&ui, "k", true);
    ui.fill(FIELD, "shortcuts");
    key(&ui, "\n", false);
    assert_eq!(ui.app().sheets.open, Some(Sheet::Settings));
    assert_eq!(ui.app().sheets.section, Section::Shortcuts);
    assert!(
        ui.shows("Ctrl+K"),
        "the Shortcuts page lists the finder's key"
    );

    // A session found from there closes Settings to show it.
    key(&ui, "k", true);
    ui.fill(FIELD, "pi update");
    key(&ui, "\n", false);
    let app = ui.app();
    assert_eq!(app.sheets.open, None);
    assert_eq!(app.active.as_deref(), Some("ses_demo_03"));
}

#[test]
fn a_leading_mark_leaves_sessions_out_and_any_text_can_search_settings() {
    let ui = ui(demo::app());
    key(&ui, "k", true);
    ui.fill(FIELD, "how");
    assert!(listed(&ui).iter().any(|name| name == "how are you"));
    ui.fill(FIELD, "> how");
    let names = listed(&ui);
    assert!(
        !names.iter().any(|name| name.contains("how are you")),
        "{names:?}"
    );
    assert!(names.contains(&"Show or hide the sidebar".to_owned()));

    ui.fill(FIELD, "glass");
    let names = listed(&ui);
    assert_eq!(
        names.last().map(String::as_str),
        Some("Search Settings for “glass”")
    );
    ui.press("Search Settings for “glass”");
    let app = ui.app();
    assert_eq!(app.picker, None);
    assert_eq!(app.sheets.open, Some(Sheet::Settings));
    assert_eq!(app.sheets.query, "glass");
}

#[test]
fn the_arrow_keys_move_and_a_row_does_what_it_says() {
    let ui = ui(demo::app());
    let shown = ui.app().sidebar;
    key(&ui, "k", true);
    ui.fill(FIELD, "> sidebar");
    assert_eq!(listed(&ui)[0], "Show or hide the sidebar");
    // Down and back up again stays on the first row.
    key(&ui, "\u{F701}", false);
    assert_eq!(ui.app().picker_index, 1);
    key(&ui, "\u{F700}", false);
    key(&ui, "\n", false);
    assert_eq!(ui.app().sidebar, !shown);

    key(&ui, "k", true);
    ui.fill(FIELD, "new session in harlo");
    key(&ui, "\n", false);
    let app = ui.app();
    assert_eq!(app.active, None);
    assert_eq!(app.new_folder.as_deref(), Some("/home/burce/code/harlo"));
    assert!(matches!(
        harlo::app::shortcut(&KeyPress::new("k", true, false, false)),
        Some(Message::TogglePicker(Picker::Finder))
    ));
}
