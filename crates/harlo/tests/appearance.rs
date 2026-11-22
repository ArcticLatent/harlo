//! Settings → Appearance: Harlo's and OpenCode's themes as cards, light or
//! dark, and the window taking the chosen colours at once.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, Message, Section, SheetEvent};
use harlo::ui::{State, Theme};
use harlo_core::layout::{Appearance, Preferences};
use harlo_core::theme::Mode;
use slint::ComponentHandle as _;
use slint::Model as _;
use support::{Ui, demo, ui};

/// Settings open at Appearance.
fn appearance(app: App) -> Ui {
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::Appearance)));
    ui
}

/// The tokens of the bundled theme `id` in `mode`.
fn tokens(id: &str, mode: Mode) -> harlo_core::theme::Tokens {
    harlo_core::theme::bundled()
        .into_iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| panic!("{id} is bundled"))
        .tokens(mode)
}

/// The window's background colour now.
fn background(ui: &Ui) -> slint::Color {
    ui.window.global::<Theme>().get_background_base()
}

#[test]
fn the_themes_show_as_cards_with_harlos_chosen() {
    let ui = appearance(demo::app());
    assert!(ui.shows("Light or dark"));
    // Cards below the fold aren't on screen, so the list is read whole.
    let names: Vec<String> = ui
        .window
        .global::<State>()
        .get_theme_cards()
        .iter()
        .map(|card| card.name.to_string())
        .collect();
    assert_eq!(names[0], "Harlo", "Harlo's own first");
    for name in ["OpenCode", "Dracula", "Nord", "Tokyonight", "Solarized"] {
        assert!(names.iter().any(|n| n == name), "{name} has a card");
    }
    assert!(ui.shows("Dracula"), "on screen");
    let chosen: Vec<_> = ui
        .find("Harlo")
        .into_iter()
        .filter(|e| e.accessible_checked() == Some(true))
        .collect();
    assert_eq!(chosen.len(), 1, "Harlo's own is the one chosen");
    assert_eq!(ui.button("Dark").accessible_checked(), Some(false));
    assert_eq!(ui.button("System").accessible_checked(), Some(true));
}

#[test]
fn choosing_a_theme_recolours_the_window_and_is_saved() {
    let ui = appearance(demo::app());
    let before = background(&ui);
    ui.press("Dracula");
    assert_eq!(ui.app().preferences.theme, "dracula");
    assert_eq!(ui.app().tokens, tokens("dracula", Mode::Dark));
    assert_ne!(background(&ui), before, "the window takes its colours");
    assert_eq!(
        background(&ui),
        harlo::theme::color(tokens("dracula", Mode::Dark).background_base)
    );
    assert_eq!(ui.button("Dracula").accessible_checked(), Some(true));

    ui.press("Light");
    assert_eq!(ui.app().preferences.appearance, Appearance::Light);
    assert_eq!(ui.app().tokens, tokens("dracula", Mode::Light));
    assert_eq!(ui.button("Light").accessible_checked(), Some(true));
}

#[test]
fn system_follows_the_desktop() {
    let ui = appearance(demo::app());
    ui.send(Message::SystemDark(false));
    assert_eq!(ui.app().tokens, tokens("harlo", Mode::Light));
    ui.send(Message::SystemDark(true));
    assert_eq!(ui.app().tokens, tokens("harlo", Mode::Dark));

    // A variant chosen outright stays, whatever the desktop does.
    ui.press("Dark");
    ui.send(Message::SystemDark(false));
    assert_eq!(ui.app().tokens, tokens("harlo", Mode::Dark));
}

#[test]
fn a_saved_choice_comes_back_and_a_missing_theme_falls_back() {
    let saved = demo::app().with_preferences(Preferences {
        theme: "nord".to_owned(),
        appearance: Appearance::Light,
        ..Preferences::default()
    });
    assert_eq!(saved.tokens, tokens("nord", Mode::Light));

    let gone = demo::app().with_preferences(Preferences {
        theme: "no-such-theme".to_owned(),
        ..Preferences::default()
    });
    assert_eq!(gone.theme().id, "harlo");
    assert_eq!(gone.tokens, tokens("harlo", Mode::Dark));
}

#[test]
fn search_finds_themes_by_name() {
    let ui = ui(demo::app());
    ui.send(Message::Sheet(SheetEvent::Open(Section::General)));
    ui.send(Message::Sheet(SheetEvent::Query("gruv".into())));
    assert!(ui.shows("Themes"));
    assert!(ui.shows("Gruvbox"));
    assert!(!ui.shows("Dracula"), "only the themes found");
    ui.press("Gruvbox");
    assert_eq!(ui.app().preferences.theme, "gruvbox");

    ui.send(Message::Sheet(SheetEvent::Query("light or".into())));
    assert!(ui.shows("Light or dark"), "the choice is found too");
    assert!(!ui.shows("Gruvbox"));
}

#[test]
fn the_shade_behind_settings_is_fainter_on_a_light_theme() {
    let ui = appearance(demo::app());
    let scrim = |ui: &Ui| ui.window.global::<Theme>().get_scrim();
    let dark = scrim(&ui);
    assert_eq!(
        dark,
        harlo::theme::color(harlo::theme::scrim(&tokens("harlo", Mode::Dark)))
    );
    ui.press("Light");
    let light = scrim(&ui);
    assert!(
        light.alpha() < dark.alpha(),
        "{light:?} over a light window, {dark:?} over a dark one"
    );
}
