//! Settings → Appearance: a dark and a light theme, chosen from cards, light
//! or dark, and the window taking the chosen colours at once.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, Message, Section, SheetEvent};
use harlo::ui::{State, Theme, ThemeCard};
use harlo_core::layout::{Appearance, Preferences};
use harlo_core::theme::{HARLO_DARK, HARLO_LIGHT, Tokens};
use slint::ComponentHandle as _;
use slint::Model as _;
use support::{Ui, demo, ui};

/// Settings open at Appearance.
fn appearance(app: App) -> Ui {
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::Appearance)));
    ui
}

/// The tokens of the bundled theme `id`.
fn tokens(id: &str) -> Tokens {
    harlo_core::theme::bundled()
        .into_iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| panic!("{id} is bundled"))
        .tokens()
}

/// The window's background colour now.
fn background(ui: &Ui) -> slint::Color {
    ui.window.global::<Theme>().get_background_base()
}

/// The names on the cards of `cards`, read whole: cards below the fold
/// aren't on screen.
fn names(cards: &slint::ModelRc<ThemeCard>) -> Vec<String> {
    cards.iter().map(|card| card.name.to_string()).collect()
}

/// The names of the light theme cards that show as chosen.
fn chosen_light(ui: &Ui) -> Vec<String> {
    ui.window
        .global::<State>()
        .get_light_theme_cards()
        .iter()
        .filter(|card| card.chosen)
        .map(|card| card.name.to_string())
        .collect()
}

/// Picks theme `id` as its card would, for cards below the fold.
fn pick(ui: &Ui, id: &str) {
    ui.send(Message::Sheet(SheetEvent::Theme(id.into())));
}

#[test]
fn only_the_windows_mode_shows_its_themes_with_harlos_chosen() {
    let ui = appearance(demo::app());
    assert!(ui.shows("Light or dark"));
    assert_eq!(ui.button("System").accessible_checked(), Some(true));
    let state = ui.window.global::<State>();

    // The desktop is dark, so only the dark themes show.
    assert!(ui.shows("Dark themes"));
    assert!(!ui.shows("Light themes"));
    let dark = names(&state.get_dark_theme_cards());
    assert_eq!(dark[0], "Harlo Dark", "Harlo's own first");
    for name in ["Dracula", "Nord", "Tokyo Night", "Gruvbox Dark", "One Dark"] {
        assert!(dark.iter().any(|n| n == name), "{name} is dark");
    }
    assert!(dark.iter().all(|n| !n.contains("Light")), "{dark:?}");
    assert_eq!(state.get_light_theme_cards().row_count(), 0);
    assert_eq!(ui.button("Harlo Dark").accessible_checked(), Some(true));
    assert_eq!(ui.button("Dracula").accessible_checked(), Some(false));

    // Light shows the light themes instead, at the top.
    ui.press("Light");
    assert!(ui.shows("Light themes"));
    assert!(!ui.shows("Dark themes"));
    assert_eq!(state.get_dark_theme_cards().row_count(), 0);
    let light = names(&state.get_light_theme_cards());
    assert_eq!(light[0], "Harlo Light", "Harlo's own first");
    for name in ["GitHub Light", "Solarized Light", "Catppuccin Latte"] {
        assert!(light.iter().any(|n| n == name), "{name} is light");
    }
    assert_eq!(ui.button("Harlo Light").accessible_checked(), Some(true));
    ui.press("Solarized Light");
    assert_eq!(ui.app().preferences.light_theme, "solarized-light");
    assert_eq!(chosen_light(&ui), ["Solarized Light"]);
}

#[test]
fn choosing_a_theme_recolours_the_window_and_is_saved() {
    let ui = appearance(demo::app());
    let before = background(&ui);
    ui.press("Dracula");
    assert_eq!(ui.app().preferences.dark_theme, "dracula");
    assert_eq!(ui.app().preferences.light_theme, HARLO_LIGHT, "kept");
    assert_eq!(ui.app().tokens, tokens("dracula"));
    assert_ne!(background(&ui), before, "the window takes its colours");
    assert_eq!(
        background(&ui),
        harlo::theme::color(tokens("dracula").background_base)
    );
    assert_eq!(ui.button("Dracula").accessible_checked(), Some(true));
    assert_eq!(ui.button("Harlo Dark").accessible_checked(), Some(false));

    // Light shows the light theme, and Dark brings Dracula back.
    ui.press("Light");
    assert_eq!(ui.app().preferences.appearance, Appearance::Light);
    assert_eq!(ui.app().tokens, tokens(HARLO_LIGHT));
    ui.press("Dark");
    assert_eq!(ui.app().tokens, tokens("dracula"));
}

#[test]
fn a_fixed_appearance_turns_to_the_theme_picked() {
    let ui = appearance(demo::app());
    ui.press("Dark");
    pick(&ui, "github");
    assert_eq!(ui.app().preferences.light_theme, "github");
    assert_eq!(chosen_light(&ui), ["GitHub Light"]);
    assert_eq!(
        ui.app().preferences.appearance,
        Appearance::Light,
        "so the pick shows"
    );
    assert_eq!(ui.app().tokens, tokens("github"));
}

#[test]
fn system_shows_the_theme_for_the_desktops_mode() {
    let ui = appearance(demo::app());
    ui.press("Nord");
    pick(&ui, "solarized-light");
    assert_eq!(
        ui.app().preferences.appearance,
        Appearance::System,
        "System stays"
    );
    ui.send(Message::SystemDark(false));
    assert_eq!(ui.app().tokens, tokens("solarized-light"));
    ui.send(Message::SystemDark(true));
    assert_eq!(ui.app().tokens, tokens("nord"));

    // A mode chosen outright stays, whatever the desktop does.
    ui.press("Dark");
    ui.send(Message::SystemDark(false));
    assert_eq!(ui.app().tokens, tokens("nord"));
}

#[test]
fn a_saved_choice_comes_back_and_a_missing_theme_falls_back() {
    let saved = demo::app().with_preferences(Preferences {
        light_theme: "nord-light".to_owned(),
        appearance: Appearance::Light,
        ..Preferences::default()
    });
    assert_eq!(saved.tokens, tokens("nord-light"));

    let gone = demo::app().with_preferences(Preferences {
        dark_theme: "no-such-theme".to_owned(),
        appearance: Appearance::Dark,
        ..Preferences::default()
    });
    assert_eq!(gone.theme().id, HARLO_DARK);
    assert_eq!(gone.tokens, tokens(HARLO_DARK));

    // A light theme saved as the dark one doesn't turn the window light.
    let mixed = demo::app().with_preferences(Preferences {
        dark_theme: "github".to_owned(),
        appearance: Appearance::Dark,
        ..Preferences::default()
    });
    assert_eq!(mixed.theme().id, HARLO_DARK);
}

#[test]
fn search_finds_themes_by_name() {
    let ui = ui(demo::app());
    ui.send(Message::Sheet(SheetEvent::Open(Section::General)));
    ui.send(Message::Sheet(SheetEvent::Query("gruv".into())));
    assert!(ui.shows("Dark themes"));
    assert!(ui.shows("Gruvbox Dark"));
    assert!(
        !ui.shows("Gruvbox Light"),
        "only the window's mode's themes"
    );
    assert!(!ui.shows("Light themes"));
    assert!(!ui.shows("Dracula"), "only the themes found");
    ui.press("Gruvbox Dark");
    assert_eq!(ui.app().preferences.dark_theme, "gruvbox-dark-medium");

    ui.send(Message::Sheet(SheetEvent::Query("latte".into())));
    assert!(
        ui.shows("No setting matches “latte”."),
        "a light theme isn't found while the window is dark"
    );

    ui.send(Message::Sheet(SheetEvent::Query("light or".into())));
    assert!(ui.shows("Light or dark"), "the choice is found too");
    assert!(!ui.shows("Gruvbox Dark"));
}

#[test]
fn about_credits_the_schemes_and_their_licence() {
    let ui = ui(demo::app());
    ui.send(Message::Sheet(SheetEvent::Open(Section::About)));
    assert!(ui.shows("Colour themes"));
    assert!(
        ui.shows("Copyright (c) 2022 Tinted Theming (https://github.com/tinted-theming)"),
        "the licence's notice"
    );
}

#[test]
fn the_shade_behind_settings_is_fainter_on_a_light_theme() {
    let ui = appearance(demo::app());
    let scrim = |ui: &Ui| ui.window.global::<Theme>().get_scrim();
    let dark = scrim(&ui);
    assert_eq!(
        dark,
        harlo::theme::color(harlo::theme::scrim(&tokens(HARLO_DARK)))
    );
    ui.press("Light");
    let light = scrim(&ui);
    assert!(
        light.alpha() < dark.alpha(),
        "{light:?} over a light window, {dark:?} over a dark one"
    );
}

#[test]
fn accents_and_secondary_icons_read_in_every_theme() {
    let mut faint = Vec::new();
    for theme in harlo_core::theme::bundled() {
        let t = theme.tokens();
        let inks = [
            ("brand", harlo::theme::ink(&t, t.surface_brand_base)),
            ("accent", harlo::theme::ink(&t, t.accent_base)),
            ("link", harlo::theme::ink(&t, t.markdown_link)),
            ("heading", harlo::theme::ink(&t, t.markdown_heading)),
            ("quote", harlo::theme::ink(&t, t.markdown_quote)),
            ("code", harlo::theme::ink(&t, t.markdown_code)),
        ];
        for (name, ink) in inks {
            for background in [t.background_base, t.background_stronger] {
                let contrast = ink.contrast(background);
                if contrast < 3.0 {
                    faint.push(format!("{} {name}: {contrast:.2}", theme.id));
                }
            }
        }
        // Secondary icons stay quieter than text, but visible.
        let quiet = harlo::theme::quiet_icon(&t).contrast(t.background_stronger);
        if quiet < 2.0 {
            faint.push(format!("{} quiet icon: {quiet:.2}", theme.id));
        }
    }
    assert!(faint.is_empty(), "too faint:\n{}", faint.join("\n"));
}

#[test]
fn a_readable_accent_keeps_its_colour() {
    let t = tokens("dracula");
    assert_eq!(
        harlo::theme::ink(&t, t.surface_brand_base),
        t.surface_brand_base,
        "Dracula's blue reads on its dark background as it is"
    );
}
