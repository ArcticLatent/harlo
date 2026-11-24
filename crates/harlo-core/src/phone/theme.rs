//! The desktop's theme as the phone app reads it.
//!
//! The phone draws itself in the desktop's colours: a palette for each of
//! the dark and light themes, which of them shows, and the user's avatar.
//! Each palette is built from the colours the desktop resolves its scheme
//! to, named the way the phone app reads them (`surface.background`,
//! `primary.base`, `syntax.base.keyword` and so on).

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};

use crate::theme::{Look, Mode, Rgba, Scheme, Tokens};

/// A theme as the phone app reads it: JSON, as [`theme`] makes it.
pub type Theme = Value;

/// The avatars both the desktop and the phone have, the first the default.
pub const AVATARS: [&str; 6] = [
    "classic",
    "beanie",
    "glasses",
    "headphones",
    "jacket",
    "scarf",
];

/// `color` as `#rrggbb`, laid over `base` first when it's see-through, as
/// the phone takes only opaque colours.
fn hex(color: Rgba, base: Rgba) -> String {
    color.over(base, color.a).to_string()
}

/// The palette the phone draws one variant (dark or light) with.
#[must_use]
pub fn palette(t: &Tokens) -> Value {
    let bg = t.background_base;
    let c = |color: Rgba| hex(color, bg);
    json!({
        "surface": {
            "background": c(bg),
            "foreground": c(t.text_base),
            "muted": c(t.background_stronger),
            "mutedForeground": c(t.text_weak),
            "elevated": c(t.surface_raised_base),
            "subtle": c(t.surface_inset_base),
        },
        "primary": { "base": c(t.surface_brand_base) },
        "chat": {
            "divider": c(t.border_weak_base),
            "userMessageBackground": c(t.surface_raised_base),
        },
        "status": {
            "error": c(t.icon_critical_base),
            "warning": c(t.icon_warning_base),
            "success": c(t.icon_success_base),
            "info": c(t.icon_info_base),
        },
        "markdown": {
            "link": c(t.markdown_link),
            "inlineCode": c(t.markdown_code),
            "inlineCodeBackground": c(t.surface_inset_base),
        },
        "syntax": {
            "base": {
                "background": c(t.surface_inset_base),
                "comment": c(t.syntax_comment),
                "keyword": c(t.syntax_keyword),
                "string": c(t.syntax_string),
                "number": c(t.syntax_constant),
                "function": c(t.syntax_function),
                "variable": c(t.syntax_variable),
                "type": c(t.syntax_type),
                "operator": c(t.syntax_operator),
            },
            "highlights": {
                "diffAdded": c(t.diff_added),
                "diffRemoved": c(t.diff_removed),
                "diffModified": c(t.icon_warning_base),
            },
        },
    })
}

/// A theme's family name: its name without " Dark" or " Light".
fn family(scheme: &Scheme) -> String {
    let name = scheme.name.trim();
    [" Dark", " Light", " dark", " light"]
        .iter()
        .find_map(|end| name.strip_suffix(end))
        .unwrap_or(name)
        .to_owned()
}

/// The theme a phone shows, made from the desktop's choices.
///
/// It holds the `dark` and `light` schemes as `look` resolves them, which
/// shows (`appearance`: `system`, `dark` or `light`), and the user's
/// `avatar`, the default one when it isn't one of [`AVATARS`].
#[must_use]
pub fn theme(dark: &Scheme, light: &Scheme, look: Look, appearance: &str, avatar: &str) -> Value {
    let avatar = AVATARS
        .iter()
        .find(|a| **a == avatar)
        .unwrap_or(&AVATARS[0]);
    json!({
        "family": family(dark),
        "mode": appearance,
        "dark": palette(&dark.tokens_with(look)),
        "light": palette(&light.tokens_with(look)),
        "avatar": avatar,
    })
}

/// Harlo's own theme, following the phone's light or dark, with the
/// default avatar: what a phone gets before a desktop has said.
#[must_use]
pub fn harlo() -> Value {
    theme(
        &crate::theme::harlo(Mode::Dark),
        &crate::theme::harlo(Mode::Light),
        Look::default(),
        "system",
        AVATARS[0],
    )
}

/// `theme` with its avatar set to `avatar`; unchanged when that isn't
/// one of [`AVATARS`].
#[must_use]
pub fn with_avatar(mut theme: Value, avatar: &str) -> Value {
    if AVATARS.contains(&avatar) {
        theme["avatar"] = json!(avatar);
    }
    theme
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harlos_own_theme_has_both_variants_and_the_default_avatar() {
        let t = harlo();
        assert_eq!(t["family"], "Harlo");
        assert_eq!(t["mode"], "system");
        assert_eq!(t["avatar"], "classic");
        for variant in ["dark", "light"] {
            let bg = t[variant]["surface"]["background"]
                .as_str()
                .expect("a colour");
            assert!(bg.starts_with('#') && bg.len() == 7, "{bg}");
            assert!(t[variant]["primary"]["base"].is_string());
        }
        assert_ne!(
            t["dark"]["surface"]["background"],
            t["light"]["surface"]["background"]
        );
    }

    #[test]
    fn only_known_avatars_are_taken() {
        assert_eq!(with_avatar(harlo(), "scarf")["avatar"], "scarf");
        assert_eq!(with_avatar(harlo(), "../x")["avatar"], "classic");
        let dark = crate::theme::harlo(Mode::Dark);
        let light = crate::theme::harlo(Mode::Light);
        assert_eq!(
            theme(&dark, &light, Look::default(), "dark", "nobody")["avatar"],
            "classic"
        );
    }
}
