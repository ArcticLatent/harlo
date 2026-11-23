//! Colour schemes: every bundled one loads and reads well, each keeps its own
//! colours in their Base16 roles, and Harlo's own match its design.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::{
    self, Contrast, DiffColors, HARLO_DARK, HARLO_LIGHT, Look, Mode, Rgba, Scheme,
};

/// The bundled scheme with this id.
fn scheme(id: &str) -> Scheme {
    theme::bundled()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("{id} is bundled"))
}

#[test]
fn every_bundled_scheme_loads_with_harlos_first() {
    let schemes = theme::bundled();
    assert_eq!(schemes[0].id, HARLO_DARK);
    assert_eq!(schemes[1].id, HARLO_LIGHT);
    assert_eq!(schemes.len(), 40, "Harlo's two and the 38 chosen families");
    let mut ids: Vec<&str> = schemes.iter().map(|s| s.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), schemes.len(), "ids are unique");
    let names: Vec<&str> = schemes[2..].iter().map(|s| s.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|n| n.to_lowercase());
    assert_eq!(names, sorted, "the rest by name");
    for (mode, least) in [(Mode::Dark, 20), (Mode::Light, 10)] {
        let count = schemes.iter().filter(|s| s.mode == mode).count();
        assert!(count >= least, "{count} {mode:?} schemes");
    }
    assert!(
        schemes[2..].iter().all(|s| !s.author.is_empty()),
        "each credits its author"
    );
}

#[test]
fn harlos_schemes_are_its_default_dark_and_light() {
    let dark = theme::harlo(Mode::Dark);
    let light = theme::harlo(Mode::Light);
    assert_eq!((dark.id.as_str(), dark.mode), (HARLO_DARK, Mode::Dark));
    assert_eq!((light.id.as_str(), light.mode), (HARLO_LIGHT, Mode::Light));
    assert!(dark.is_harlo() && light.is_harlo());
    assert!(!scheme("dracula").is_harlo());
}

/// The greys measured from the design mockup.
#[test]
fn harlo_dark_matches_the_mockup() {
    let t = theme::harlo(Mode::Dark).tokens();
    let expected = [
        ("background-base", t.background_base, "#292929"),
        ("background-stronger", t.background_stronger, "#252525"),
        ("surface-raised-base", t.surface_raised_base, "#2f2f2f"),
        ("border-weak-base", t.border_weak_base, "#383838"),
        ("text-strong", t.text_strong, "#d5d5d5"),
        ("text-base", t.text_base, "#a5a6a9"),
        ("text-weak", t.text_weak, "#8b8c8e"),
        ("text-interactive-base", t.text_interactive_base, "#adcafa"),
    ];
    for (name, actual, want) in expected {
        assert_eq!(actual.to_string(), want, "{name}");
    }
}

/// Each colour takes the role Tinted Theming's styling guide gives it.
#[test]
fn a_scheme_keeps_its_colours_in_their_roles() {
    let gruvbox = scheme("gruvbox-dark-medium");
    assert_eq!(gruvbox.mode, Mode::Dark);
    let t = gruvbox.tokens();
    let base = |n: usize| gruvbox.base(n).to_string();
    let roles = [
        ("background", t.background_base, base(0)),
        ("raised", t.surface_raised_base, base(1)),
        ("selection", t.surface_selected, base(2)),
        ("text", t.text_base, base(5)),
        ("variables, errors", t.syntax_variable, base(8)),
        ("constants", t.syntax_constant, base(9)),
        ("bold, types", t.markdown_strong, base(10)),
        ("code, strings", t.markdown_code, base(11)),
        ("quotes", t.markdown_quote, base(12)),
        (
            "headings, links, main action",
            t.surface_brand_base,
            base(13),
        ),
        ("keywords, italics, accent", t.accent_base, base(14)),
    ];
    for (role, actual, want) in roles {
        assert_eq!(actual.to_string(), want, "{role}");
    }
    assert_eq!(
        t.background_base.to_string(),
        "#282828",
        "Gruvbox's own grey"
    );
    assert!(
        t.background_stronger.contrast(Rgba::BLACK) < t.background_base.contrast(Rgba::BLACK),
        "a dark scheme's sidebar is darker than its background"
    );
}

#[test]
fn a_light_scheme_has_dark_text_on_light() {
    let t = scheme("github").tokens();
    assert_eq!(t.background_base.to_string(), "#ffffff");
    assert!(t.text_base.contrast(Rgba::BLACK) < t.background_base.contrast(Rgba::BLACK));
}

/// Nord's comment grey barely differs from its raised background; Harlo
/// lifts it until hints and placeholders read.
#[test]
fn faint_colours_are_lifted_until_they_read() {
    let nord = scheme("nord");
    let t = nord.tokens();
    assert!(
        nord.base(3).contrast(nord.base(1)) < 2.0,
        "the scheme's own grey is too faint"
    );
    assert!(t.text_weaker.contrast(t.surface_inset_base) >= 2.5);
    assert!(t.syntax_comment.contrast(t.surface_inset_base) >= 2.5);
    assert_ne!(t.text_weaker, nord.base(3), "so it was moved");
}

/// Solarized's selection grey is a mid grey; as every button's fill it
/// would make the window heavy, so buttons take a fainter one.
#[test]
fn button_fills_stay_quiet() {
    let solarized = scheme("solarized-light");
    let t = solarized.tokens();
    assert!(solarized.base(2).contrast(solarized.base(0)) > 2.0);
    assert!(t.surface_raised_strong.contrast(t.background_base) <= 1.7);
    assert_eq!(t.border_weak_base, solarized.base(2), "outlines keep it");
    let gruvbox = scheme("gruvbox-dark-medium");
    assert_eq!(
        gruvbox.tokens().surface_raised_strong,
        gruvbox.base(2),
        "a quiet selection grey is used as it is"
    );
}

#[test]
fn every_bundled_scheme_reads() {
    for scheme in theme::bundled() {
        let t = scheme.tokens();
        let backs = [
            ("background", t.background_base),
            ("sidebar", t.background_stronger),
            ("field", t.surface_inset_base),
        ];
        for (back, color) in backs {
            let checks = [
                ("text", t.text_base, 4.5),
                ("secondary text", t.text_weak, 3.0),
                ("placeholder", t.text_weaker, 2.5),
                ("icon", t.icon_base, 3.0),
            ];
            for (what, fore, least) in checks {
                let contrast = fore.contrast(color);
                assert!(
                    contrast >= least,
                    "{}: {what} on the {back} reads {contrast:.2}",
                    scheme.id
                );
            }
        }
        let on_brand = t.text_on_brand_base.contrast(t.surface_brand_base);
        assert!(
            on_brand >= 3.0,
            "{}: the send button's icon reads {on_brand:.2}",
            scheme.id
        );
    }
}

#[test]
fn bad_files_are_reported() {
    let error = theme::parse("not json").expect_err("not a list");
    assert!(error.is_format());
    let missing =
        r##"[{"id": "x", "name": "X", "variant": "dark", "palette": {"base00": "#000000"}}]"##;
    let error = theme::parse(missing).expect_err("fifteen colours missing");
    assert!(error.is_bad_color());
    assert!(error.to_string().contains("base01"), "{error}");
    let palette: String = (0..16)
        .map(|n| {
            format!(
                "\"base{n:02X}\": \"{}\"",
                if n == 7 { "#zz" } else { "#123456" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let bad =
        format!(r#"[{{"id": "x", "name": "X", "variant": "light", "palette": {{{palette}}}}}]"#);
    let error = theme::parse(&bad).expect_err("a bad colour");
    assert!(error.is_bad_color());
    assert!(error.to_string().contains("base07"), "{error}");
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
fn every_token_has_a_name() {
    let t = theme::harlo(Mode::Dark).tokens();
    for name in theme::Tokens::names() {
        assert!(t.get(name).is_some(), "{name}");
    }
    assert_eq!(t.get("text-base"), Some(t.text_base));
    assert_eq!(t.get("nothing"), None);
}

#[test]
fn more_contrast_lifts_text_in_every_scheme() {
    let more = Look {
        contrast: Contrast::More,
        ..Look::default()
    };
    for scheme in theme::bundled() {
        let (standard, lifted) = (scheme.tokens(), scheme.tokens_with(more));
        let back = lifted.background_base;
        for (what, fore, least) in [
            ("text", lifted.text_base, 7.0),
            ("secondary text", lifted.text_weak, 4.5),
            ("placeholder", lifted.text_weaker, 3.5),
        ] {
            let contrast = fore.contrast(back);
            assert!(
                contrast >= least,
                "{}: {what} reads {contrast:.2}",
                scheme.id
            );
        }
        assert!(
            lifted.text_weak.contrast(back) >= standard.text_weak.contrast(back),
            "{}: never fainter",
            scheme.id
        );
        assert_eq!(
            lifted.background_base, standard.background_base,
            "same background"
        );
    }
}

#[test]
fn diffs_take_green_and_red_or_blue_and_orange() {
    let gruvbox = scheme("gruvbox-dark-medium");
    let standard = gruvbox.tokens();
    assert_eq!(standard.diff_added, gruvbox.base(11), "green");
    assert_eq!(standard.diff_removed, gruvbox.base(8), "red");
    let blue_orange = gruvbox.tokens_with(Look {
        diff: DiffColors::BlueOrange,
        ..Look::default()
    });
    assert_eq!(blue_orange.diff_added, gruvbox.base(13), "blue");
    assert_eq!(blue_orange.diff_removed, gruvbox.base(9), "orange");
}
