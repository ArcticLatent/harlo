//! The theme's tokens, pushed into the UI's `Theme` global.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::{self, Mode, Rgba, Tokens};
use slint::{Color, ComponentHandle};

use crate::ui::{AppWindow, Theme};

/// The tokens of Harlo's own dark scheme, the default.
#[must_use]
pub fn default_tokens() -> Tokens {
    theme::harlo(Mode::Dark).tokens()
}

/// Converts a theme colour to Slint's.
#[must_use]
pub fn color(c: Rgba) -> Color {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=1 first, so the scaled value fits a u8"
    )]
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::from_argb_u8(channel(c.a), channel(c.r), channel(c.g), channel(c.b))
}

/// Sets every colour of `window`'s theme from `t`, the tokens of the theme
/// `theme_id`.
///
/// Harlo's own themes keep the logo's colours; any other tints it with the
/// theme's brand and accent colours.
pub fn apply(window: &AppWindow, t: &Tokens, theme_id: &str) {
    let g = window.global::<Theme>();
    g.set_own_logo(theme_id == theme::HARLO_DARK || theme_id == theme::HARLO_LIGHT);
    g.set_background_base(color(t.background_base));
    g.set_background_stronger(color(t.background_stronger));
    g.set_surface_inset_base(color(t.surface_inset_base));
    g.set_surface_raised_base(color(t.surface_raised_base));
    g.set_surface_raised_strong(color(t.surface_raised_strong));
    g.set_surface_base_hover(color(t.surface_base_hover));
    g.set_surface_base_active(color(t.surface_base_active));
    g.set_surface_brand_base(color(t.surface_brand_base));
    g.set_border_weaker_base(color(t.border_weaker_base));
    g.set_border_weak_base(color(t.border_weak_base));
    g.set_border_base(color(t.border_base));
    g.set_text_strong(color(t.text_strong));
    g.set_text_base(color(t.text_base));
    g.set_text_weak(color(t.text_weak));
    g.set_text_weaker(color(t.text_weaker));
    g.set_text_interactive_base(color(t.text_interactive_base));
    g.set_icon_base(color(t.icon_base));
    g.set_icon_quiet(color(quiet_icon(t)));
    g.set_icon_selected(color(t.icon_selected));
    g.set_icon_success_base(color(t.icon_success_base));
    g.set_icon_warning_base(color(t.icon_warning_base));
    g.set_icon_critical_base(color(t.icon_critical_base));
    g.set_icon_info_base(color(t.icon_info_base));
    g.set_surface_brand_hover(color(t.surface_brand_hover));
    g.set_surface_interactive_base(color(t.surface_interactive_base));
    g.set_surface_interactive_hover(color(t.surface_interactive_hover));
    g.set_surface_interactive_weak(color(t.surface_interactive_weak));
    g.set_text_on_brand_base(color(t.text_on_brand_base));
    g.set_text_on_interactive_base(color(t.text_on_interactive_base));
    g.set_border_interactive_base(color(t.border_interactive_base));
    g.set_border_interactive_hover(color(t.border_interactive_hover));
    g.set_border_interactive_focus(color(t.border_interactive_focus));
    g.set_brand_ink(color(ink(t, t.surface_brand_base)));
    g.set_accent_ink(color(ink(t, t.accent_base)));
    g.set_link_ink(color(ink(t, t.markdown_link)));
    g.set_heading_ink(color(ink(t, t.markdown_heading)));
    g.set_quote_ink(color(ink(t, t.markdown_quote)));
    g.set_chip_hover(color(
        t.surface_base_hover
            .over(t.surface_raised_strong, t.surface_base_hover.a),
    ));
    g.set_menu_edge(color(edge(t, t.surface_raised_strong)));
    g.set_composer_edge(color(
        t.text_interactive_base.over(t.surface_raised_base, EDGE),
    ));
    g.set_selection(color(t.text_interactive_base.alpha(SELECTION)));
    g.set_selected_tint(color(
        t.surface_interactive_weak
            .over(t.background_stronger, SELECTED_TINT),
    ));
    g.set_added_line(color(
        t.icon_success_base.over(t.surface_inset_base, PATCH_TINT),
    ));
    g.set_removed_line(color(
        t.icon_critical_base.over(t.surface_inset_base, PATCH_TINT),
    ));
    g.set_primary_off(color(
        t.surface_brand_base
            .over(t.surface_raised_strong, PRIMARY_OFF),
    ));
    g.set_warning_edge(color(
        t.icon_warning_base
            .over(t.surface_raised_base, WAITING_EDGE),
    ));
    g.set_question_edge(color(
        t.text_interactive_base
            .over(t.surface_raised_base, WAITING_EDGE),
    ));
    g.set_scrim(color(scrim(t)));
}

/// Secondary icons: the theme's weak icon colour when it reads against the
/// background, else its icon colour faded towards the background.
///
/// Some schemes' faint grey barely differs from the background, too faint
/// for icons people need to see.
#[must_use]
pub fn quiet_icon(t: &Tokens) -> Rgba {
    if t.icon_weak_base.contrast(t.background_base) >= QUIET_CONTRAST {
        t.icon_weak_base
    } else {
        t.icon_base.over(t.background_base, QUIET_ICON)
    }
}

/// `accent` as text, icons or a thin line: itself where it reads against
/// both backgrounds, else mixed towards the strongest text until it does.
///
/// Some themes' brand or interactive colour is nearly as light as their
/// light background; used as is, a project's name in it would vanish.
#[must_use]
pub fn ink(t: &Tokens, accent: Rgba) -> Rgba {
    let reads = |c: Rgba| {
        c.contrast(t.background_base) >= INK_CONTRAST
            && c.contrast(t.background_stronger) >= INK_CONTRAST
    };
    std::iter::once(accent)
        .chain((1..=INK_STEPS).map(|step| {
            let keep = 1.0 - f32::from(step) / f32::from(INK_STEPS);
            accent.over(t.text_strong, keep)
        }))
        .find(|&c| reads(c))
        .unwrap_or(t.text_strong)
}

/// The shade over the window behind a dialog. Black at the dark themes'
/// strength would turn a light window grey, so a light theme's is fainter.
#[must_use]
pub fn scrim(t: &Tokens) -> Rgba {
    let Rgba { r, g, b, .. } = t.background_base;
    let light = 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5;
    Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: if light { SCRIM_LIGHT } else { SCRIM_DARK },
    }
}

/// How dark the shade behind a dialog is on a dark theme.
const SCRIM_DARK: f32 = 0.4;

/// How much of the interactive colour's weak fill tints the selected item:
/// the fill itself is loud as a row's background on dark themes.
const SELECTED_TINT: f32 = 0.5;

/// The contrast against the background a theme's weak icon colour needs
/// to be used as is: WCAG's minimum for graphics people must see.
const QUIET_CONTRAST: f64 = 3.0;

/// How much of the icon colour a secondary icon keeps, faded towards the
/// background, when the theme's weak icon colour is too faint.
const QUIET_ICON: f32 = 0.7;

/// The contrast an accent colour needs against the backgrounds to be used
/// as ink: WCAG's minimum for icons and large text.
const INK_CONTRAST: f64 = 3.0;

/// How many steps an accent takes towards the strongest text when it's too
/// faint to read; each keeps a tenth less of the accent.
const INK_STEPS: u8 = 10;

/// How dark the shade behind a dialog is on a light theme.
const SCRIM_LIGHT: f32 = 0.14;

/// How much of its colour a main button keeps while it can't be pressed.
const PRIMARY_OFF: f32 = 0.4;

/// How much of its accent a waiting card's outline takes.
const WAITING_EDGE: f32 = 0.45;

/// How much colour an added or removed diff line's background takes.
const PATCH_TINT: f32 = 0.16;

/// How much of the strong text colour an outline mixes into its surface.
const EDGE: f32 = 0.12;

/// How opaque selected text's highlight is.
pub(crate) const SELECTION: f32 = 0.3;

/// An outline for a card of colour `surface`.
fn edge(t: &Tokens, surface: Rgba) -> Rgba {
    t.text_strong.over(surface, EDGE)
}
