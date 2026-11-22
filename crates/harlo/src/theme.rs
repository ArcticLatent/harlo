//! The theme's tokens, pushed into the UI's `Theme` global.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::{self, Mode, Rgba, Tokens};
use slint::{Color, ComponentHandle};

use crate::ui::{AppWindow, Theme};

/// The bundled default theme's dark tokens.
///
/// # Panics
///
/// Panics if the bundled default theme doesn't parse. `harlo-core`'s tests
/// check that it does, so this would be a build mistake.
#[must_use]
pub fn default_tokens() -> Tokens {
    theme::parse(harlo_core::DEFAULT_THEME)
        .expect("the bundled theme is valid")
        .tokens(Mode::Dark)
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

/// Sets every colour of `window`'s theme from `t`.
pub fn apply(window: &AppWindow, t: &Tokens) {
    let g = window.global::<Theme>();
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
    g.set_icon_weak_base(color(t.icon_weak_base));
    g.set_icon_selected(color(t.icon_selected));
    g.set_icon_success_base(color(t.icon_success_base));
    g.set_icon_warning_base(color(t.icon_warning_base));
    g.set_icon_critical_base(color(t.icon_critical_base));
    g.set_icon_info_base(color(t.icon_info_base));
    g.set_chip_hover(color(
        t.surface_base_hover
            .over(t.surface_raised_strong, t.surface_base_hover.a),
    ));
    g.set_menu_edge(color(edge(t, t.surface_raised_strong)));
    g.set_composer_edge(color(
        t.text_interactive_base.over(t.surface_raised_base, EDGE),
    ));
    g.set_selection(color(t.text_interactive_base.alpha(SELECTION)));
    g.set_added_line(color(
        t.icon_success_base.over(t.surface_inset_base, PATCH_TINT),
    ));
    g.set_removed_line(color(
        t.icon_critical_base.over(t.surface_inset_base, PATCH_TINT),
    ));
    g.set_primary_hover(color(
        t.background_base.over(t.surface_brand_base, PRIMARY_HOVER),
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
}

/// How much of the background the main button's fill takes under the pointer.
const PRIMARY_HOVER: f32 = 0.18;

/// How much of its colour a main button keeps while it can't be pressed.
const PRIMARY_OFF: f32 = 0.4;

/// How much of its accent a waiting card's outline takes.
const WAITING_EDGE: f32 = 0.45;

/// How much colour an added or removed diff line's background takes.
const PATCH_TINT: f32 = 0.16;

/// How much of the strong text colour an outline mixes into its surface.
const EDGE: f32 = 0.12;

/// How opaque selected text's highlight is.
const SELECTION: f32 = 0.3;

/// An outline for a card of colour `surface`.
fn edge(t: &Tokens, surface: Rgba) -> Rgba {
    t.text_strong.over(surface, EDGE)
}
