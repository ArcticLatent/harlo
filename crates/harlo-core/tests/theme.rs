//! Themes: Harlo's default matches the design, OpenCode's own theme files load,
//! and every bundled theme gives both variants.

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

/// Backgrounds come from the neutral colour as OpenCode's do: a dark one
/// sinks, and a mid grey lifts most of the way to white for light.
#[test]
fn backgrounds_follow_opencodes_greys() {
    let carbonfox = theme::bundled()
        .into_iter()
        .find(|t| t.id == "carbonfox")
        .expect("carbonfox");
    // Its light neutral is #8e8e8e; OpenCode shows it nearly white.
    let light = carbonfox.tokens(Mode::Light);
    assert!(light.background_base.r > 0.9, "{}", light.background_base);
    assert!(
        light.text_strong.r < 0.3,
        "dark text: {}",
        light.text_strong
    );
    // A dark neutral sinks below itself.
    let dracula = theme::bundled()
        .into_iter()
        .find(|t| t.id == "dracula")
        .expect("dracula");
    let dark = dracula.tokens(Mode::Dark);
    let neutral = Rgba::parse("#282a36").expect("dracula's neutral");
    assert!(
        dark.background_base.r < neutral.r,
        "{}",
        dark.background_base
    );
    assert!(dark.text_strong.r > 0.8, "light text: {}", dark.text_strong);
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
    assert_eq!(dark.text_weak.to_string(), "#b8c0e0", "override applied");
    assert_ne!(
        dark.background_base,
        catppuccin.tokens(Mode::Light).background_base
    );

    // OpenCode's own theme uses `var(--v2-…)` references Harlo doesn't model;
    // those are skipped and the tokens Harlo knows still come through.
    let opencode = theme::parse(include_str!("fixtures/oc-2.json")).expect("oc-2");
    let light = opencode.tokens(Mode::Light);
    assert!(light.text_strong.r < 0.2, "{}", light.text_strong);
    assert!(light.background_base.r > 0.9, "{}", light.background_base);
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

#[test]
fn every_bundled_theme_loads_with_harlo_first() {
    let themes = theme::bundled();
    assert_eq!(themes[0].id, "harlo", "Harlo's own theme leads");
    assert!(
        themes.len() >= 37,
        "Harlo's and OpenCode's: {}",
        themes.len()
    );
    let names: Vec<&str> = themes[1..].iter().map(|t| t.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|name| name.to_lowercase());
    assert_eq!(names, sorted, "OpenCode's by name");
    for id in [
        "oc-2",
        "dracula",
        "nord",
        "tokyonight",
        "github",
        "solarized",
    ] {
        assert!(themes.iter().any(|t| t.id == id), "{id} is bundled");
    }
    for t in &themes {
        let (light, dark) = (t.tokens(Mode::Light), t.tokens(Mode::Dark));
        assert_ne!(
            light.background_base, dark.background_base,
            "{} has two variants",
            t.id
        );
    }
}

#[test]
fn code_colours_come_from_the_theme() {
    let dracula = theme::bundled()
        .into_iter()
        .find(|t| t.id == "dracula")
        .expect("dracula");
    let dark = dracula.tokens(Mode::Dark);
    // Set by the theme.
    assert_eq!(dark.syntax_keyword.to_string(), "#ff79c6");
    assert_eq!(dark.syntax_string.to_string(), "#f1fa8c");
    assert_eq!(dark.syntax_comment.to_string(), "#6272a4");
    // Not set: punctuation follows its weak text, as OpenCode's does.
    assert_eq!(dark.syntax_punctuation, dark.text_weak);

    // A theme without code colours takes them from its palette by role.
    let plain = theme::parse(
        r##"{"id": "p", "name": "P",
            "light": {"palette": {"neutral": "#ffffff", "ink": "#000000", "primary": "#0000ff",
              "success": "#00ff00", "warning": "#ffff00", "error": "#ff0000", "info": "#00ffff"}},
            "dark": {"palette": {"neutral": "#000000", "ink": "#ffffff", "primary": "#0000ff",
              "accent": "#ff00ff", "success": "#00ff00", "warning": "#ffff00", "error": "#ff0000",
              "info": "#00ffff"},
              "overrides": {"text-weak": "#123456"}}}"##,
    )
    .expect("a theme");
    let dark = plain.tokens(Mode::Dark);
    // Keywords and constants share the accent; strings, types and the rest
    // each have their own.
    assert_eq!(dark.syntax_keyword, dark.syntax_constant);
    assert_ne!(dark.syntax_keyword, dark.syntax_string);
    assert_ne!(dark.syntax_string, dark.syntax_type);
    assert!(
        dark.syntax_string.g > dark.syntax_string.r,
        "green: {}",
        dark.syntax_string
    );
    assert_eq!(
        dark.syntax_comment.to_string(),
        "#123456",
        "follows text-weak"
    );
    assert_eq!(dark.syntax_operator.to_string(), "#123456");
    let light = plain.tokens(Mode::Light);
    assert_eq!(
        light.syntax_keyword, light.syntax_property,
        "no accent: info"
    );
    assert_eq!(light.syntax_operator, light.text_base);
}

/// OpenCode's own theme code, run on every bundled theme at the pinned
/// version (tools/opencode-theme-reference.sh), gives exactly these colours.
#[test]
fn bundled_themes_match_opencode_exactly() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/opencode-resolved.json"))
            .expect("the reference is JSON");
    let reference = reference.as_object().expect("themes by id");
    let themes = theme::bundled();
    assert_eq!(reference.len(), themes.len() - 1, "every OpenCode theme");
    let mut wrong = Vec::new();
    for (id, modes) in reference {
        let theme = themes
            .iter()
            .find(|t| &t.id == id)
            .unwrap_or_else(|| panic!("{id} is bundled"));
        for (mode, name) in [(Mode::Light, "light"), (Mode::Dark, "dark")] {
            let tokens = theme.tokens(mode);
            for (token, want) in modes[name].as_object().expect("tokens") {
                let got = tokens.get(token).map(|c| c.to_string());
                if got.as_deref() != want.as_str() {
                    wrong.push(format!("{id} {name} {token}: {got:?}, OpenCode {want}"));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
