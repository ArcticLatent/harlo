//! Themes: Harlo's default matches the design, and OpenCode's own theme files load.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::{self, Mode, Rgba};

/// The greys and accents measured from the design mockup.
#[test]
fn default_dark_theme_matches_the_mockup() {
    let tokens = theme::parse(harlo_core::DEFAULT_THEME)
        .expect("default theme")
        .tokens(Mode::Dark);
    let expected = [
        ("background-base", tokens.background_base, "#292929"),
        ("background-stronger", tokens.background_stronger, "#252525"),
        ("surface-inset-base", tokens.surface_inset_base, "#2d2d2d"),
        ("surface-raised-base", tokens.surface_raised_base, "#2f2f2f"),
        (
            "surface-raised-strong",
            tokens.surface_raised_strong,
            "#353535",
        ),
        ("surface-base-active", tokens.surface_base_active, "#343434"),
        ("border-weak-base", tokens.border_weak_base, "#383838"),
        ("text-strong", tokens.text_strong, "#d5d5d5"),
        ("text-base", tokens.text_base, "#a5a6a9"),
        ("text-weak", tokens.text_weak, "#8b8c8e"),
        ("text-weaker", tokens.text_weaker, "#69696b"),
        ("icon-base", tokens.icon_base, "#a5a6a9"),
        ("icon-selected", tokens.icon_selected, "#c6dafc"),
        ("icon-success-base", tokens.icon_success_base, "#6ce78a"),
        (
            "text-interactive-base",
            tokens.text_interactive_base,
            "#adcafa",
        ),
    ];
    for (name, actual, want) in expected {
        assert_eq!(actual.to_string(), want, "{name}");
    }
}

/// Without overrides, the derivation alone lands within one step of the mockup's greys.
#[test]
fn derived_greys_are_close_to_the_mockup() {
    let json = r##"{ "id": "t", "name": "T",
        "light": { "palette": { "neutral": "#eeeeee", "ink": "#404040", "primary": "#1159e3",
            "success": "#105223", "warning": "#7a6211", "error": "#980313", "info": "#0c42a8" } },
        "dark": { "palette": { "neutral": "#292929", "ink": "#d5d5d5", "primary": "#c6dafc",
            "success": "#6ce78a", "warning": "#f3c30e", "error": "#ffb1b2", "info": "#adcafa" } } }"##;
    let tokens = theme::parse(json).expect("theme").tokens(Mode::Dark);
    let grey = |c: Rgba| c.r * 255.0;
    for (name, actual, want) in [
        ("background-stronger", tokens.background_stronger, 0x25_u8),
        ("surface-inset-base", tokens.surface_inset_base, 0x2d),
        ("surface-raised-base", tokens.surface_raised_base, 0x2f),
        ("surface-raised-strong", tokens.surface_raised_strong, 0x35),
        ("border-weak-base", tokens.border_weak_base, 0x38),
        ("text-weak", tokens.text_weak, 0x8b),
        ("text-weaker", tokens.text_weaker, 0x69),
    ] {
        assert!(
            (grey(actual) - f32::from(want)).abs() <= 2.0,
            "{name}: {actual} vs {want:#x}"
        );
    }
    // An unset `interactive` falls back to `primary`.
    assert_eq!(tokens.text_interactive_base, tokens.icon_selected);
}

/// OpenCode's theme files load unchanged, with their overrides applied.
#[test]
fn opencode_theme_files_load() {
    let catppuccin =
        theme::parse(include_str!("fixtures/catppuccin-macchiato.json")).expect("catppuccin");
    assert_eq!(
        (catppuccin.id.as_str(), catppuccin.name.as_str()),
        ("catppuccin-macchiato", "Catppuccin Macchiato")
    );
    let dark = catppuccin.tokens(Mode::Dark);
    assert_eq!(dark.background_base.to_string(), "#24273a");
    assert_eq!(dark.text_weak.to_string(), "#b8c0e0", "override applied");
    assert_eq!(
        catppuccin.tokens(Mode::Light).text_strong.to_string(),
        "#4c4f69"
    );

    // OpenCode's own theme uses `var(--v2-…)` references Harlo doesn't model;
    // those are skipped and the tokens Harlo knows still come through.
    let opencode = theme::parse(include_str!("fixtures/oc-2.json")).expect("oc-2");
    let light = opencode.tokens(Mode::Light);
    assert_eq!(light.text_strong.to_string(), "#171717");
    assert_eq!(light.text_weak.to_string(), "#8f8f8f");
}

#[test]
fn references_follow_chains_and_survive_cycles() {
    let json = r##"{ "id": "t", "name": "T",
        "light": { "palette": { "neutral": "#ffffff", "ink": "#000000", "primary": "#0000ff",
            "success": "#00ff00", "warning": "#ffff00", "error": "#ff0000", "info": "#00ffff" },
          "overrides": { "text-strong": "#123456", "text-base": "var(--text-strong)",
            "icon-base": "var(--text-base)", "text-weak": "var(--text-weaker)",
            "text-weaker": "var(--text-weak)", "border-base": "nonsense", "unknown-token": "#ffffff" } },
        "dark": { "palette": { "neutral": "#000000", "ink": "#ffffff", "primary": "#0000ff",
            "success": "#00ff00", "warning": "#ffff00", "error": "#ff0000", "info": "#00ffff" } } }"##;
    let theme = theme::parse(json).expect("theme");
    let tokens = theme.tokens(Mode::Light);
    assert_eq!(tokens.text_base.to_string(), "#123456");
    assert_eq!(tokens.icon_base.to_string(), "#123456", "two-step chain");
    // A cycle and a bad value leave usable colours behind.
    assert_eq!(tokens.text_weak, tokens.text_weaker);
    assert_eq!(
        tokens.border_base,
        theme::parse(json)
            .expect("theme")
            .tokens(Mode::Light)
            .border_base
    );
}

#[test]
fn bad_files_are_reported() {
    let not_json = theme::parse("{").expect_err("not JSON");
    assert!(not_json.is_format(), "{not_json}");

    let bad_color = r##"{ "id": "t", "name": "T",
        "light": { "palette": { "neutral": "white", "primary": "#00f", "success": "#0f0",
            "warning": "#ff0", "error": "#f00", "info": "#0ff" } },
        "dark": { "palette": { "neutral": "#000", "primary": "#00f", "success": "#0f0",
            "warning": "#ff0", "error": "#f00", "info": "#0ff" } } }"##;
    let error = theme::parse(bad_color).expect_err("bad colour");
    assert!(error.is_bad_color());
    assert_eq!(
        error.to_string(),
        r#"the light palette's neutral colour isn't a hex colour: "white""#
    );
}

#[test]
fn hex_colors_parse_in_every_length() {
    let parse = |s: &str| Rgba::parse(s).map(|c| c.to_string());
    assert_eq!(parse("#fff").as_deref(), Some("#ffffff"));
    assert_eq!(parse("#0008").as_deref(), Some("#00000088"));
    assert_eq!(parse("#A1B2C3").as_deref(), Some("#a1b2c3"));
    assert_eq!(parse("#a1b2c380").as_deref(), Some("#a1b2c380"));
    for bad in ["", "fff", "#ff", "#ggg", "#12345", "#+12345"] {
        assert_eq!(parse(bad), None, "{bad:?}");
    }
}
