//! Fonts, sizes and widget styles, all drawn from the theme's [`Tokens`].
//!
//! Views pass the current `Tokens` into these helpers; each returns a style
//! closure that captures the colours it needs. The `iced::Theme` built by
//! [`iced_theme`] only supplies defaults for widgets nothing here styles.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::{Rgba, Tokens};
use iced::widget::{button, container, rule, scrollable, svg, text_editor, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, font};

/// Geist, the interface font.
pub const SANS: Font = Font::with_name("Geist");

/// Geist Medium, for labels that need a little more weight.
pub const SANS_MEDIUM: Font = Font {
    weight: font::Weight::Medium,
    ..SANS
};

/// Geist `SemiBold`, for headings.
pub const SANS_SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..SANS
};

/// Fonts bundled with the app, loaded at start.
pub const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    lucide_icons::LUCIDE_FONT_BYTES,
];

/// Sizes in logical pixels, measured from the design.
pub mod size {
    /// Width of the left and right rails.
    pub const RAIL: f32 = 56.0;
    /// Width of the sidebar.
    pub const SIDEBAR: f32 = 288.0;
    /// Width of the right panel.
    pub const PANEL: f32 = 260.0;
    /// A square rail button.
    pub const RAIL_BUTTON: f32 = 36.0;
    /// Rail icons.
    pub const RAIL_ICON: f32 = 18.0;
    /// Body text.
    pub const TEXT: f32 = 14.0;
    /// Secondary text.
    pub const SMALL: f32 = 12.5;
    /// Project names and hints.
    pub const LABEL: f32 = 13.0;
    /// The wordmark beside the logo.
    pub const WORDMARK: f32 = 27.0;
    /// Panel titles.
    pub const TITLE: f32 = 19.0;
    /// The empty state's question.
    pub const HEADLINE: f32 = 26.0;
    /// Corner radius of buttons and fields.
    pub const RADIUS: f32 = 6.0;
    /// Corner radius of the composer.
    pub const COMPOSER_RADIUS: f32 = 8.0;
}

/// Converts a theme colour to iced's.
#[must_use]
pub fn color(c: Rgba) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// Builds the `iced::Theme` that supplies defaults, from the tokens.
#[must_use]
pub fn iced_theme(name: &str, t: &Tokens) -> Theme {
    Theme::custom(
        name.to_owned(),
        iced::theme::Palette {
            background: color(t.background_base),
            text: color(t.text_strong),
            primary: color(t.surface_brand_base),
            success: color(t.icon_success_base),
            warning: color(t.icon_warning_base),
            danger: color(t.icon_critical_base),
        },
    )
}

/// A plain background in the given colour.
pub fn fill(c: Rgba) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color(c).into()),
        ..container::Style::default()
    }
}

/// A rounded, filled box, optionally outlined.
pub fn boxed(
    fill: Rgba,
    outline: Option<Rgba>,
    radius: f32,
) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color(fill).into()),
        border: Border {
            radius: radius.into(),
            width: f32::from(u8::from(outline.is_some())),
            color: outline.map_or(Color::TRANSPARENT, color),
        },
        ..container::Style::default()
    }
}

/// A button with no fill until hovered, for rail icons and list rows.
pub fn ghost(
    t: &Tokens,
    selected: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style + use<> {
    let t = *t;
    move |_, status| {
        let background = match status {
            _ if selected => Some(t.surface_base_active),
            button::Status::Hovered => Some(t.surface_base_hover),
            button::Status::Pressed => Some(t.surface_base_active),
            button::Status::Active | button::Status::Disabled => None,
        };
        button::Style {
            background: background.map(|c| Background::Color(color(c))),
            text_color: color(if selected {
                t.icon_selected
            } else {
                t.text_base
            }),
            border: Border {
                radius: size::RADIUS.into(),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// A filled button, such as `+ Project` and the empty state's pickers.
pub fn chip(t: &Tokens) -> impl Fn(&Theme, button::Status) -> button::Style + use<> {
    let t = *t;
    move |_, status| {
        let background = match status {
            button::Status::Hovered | button::Status::Pressed => {
                t.surface_base_hover.over(t.surface_raised_strong, 1.0)
            }
            button::Status::Active | button::Status::Disabled => t.surface_raised_strong,
        };
        button::Style {
            background: Some(color(background).into()),
            text_color: color(t.text_strong),
            border: Border {
                radius: size::RADIUS.into(),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// A round button on a filled circle, such as a panel's close button.
pub fn round(t: &Tokens) -> impl Fn(&Theme, button::Status) -> button::Style + use<> {
    let t = *t;
    move |theme, status| button::Style {
        border: Border {
            radius: 999.0.into(),
            ..Border::default()
        },
        ..chip(&t)(theme, status)
    }
}

/// The session filter.
pub fn filter(t: &Tokens) -> impl Fn(&Theme, text_input::Status) -> text_input::Style + use<> {
    let t = *t;
    move |_, status| text_input::Style {
        background: color(t.surface_inset_base).into(),
        border: Border {
            radius: size::RADIUS.into(),
            width: 1.0,
            color: color(match status {
                text_input::Status::Focused { .. } => t.border_base,
                _ => t.surface_inset_base,
            }),
        },
        icon: color(t.icon_weak_base),
        placeholder: color(t.text_weaker),
        value: color(t.text_strong),
        selection: color(t.text_interactive_base.alpha(0.3)),
    }
}

/// The composer's text area, which sits inside its own box.
pub fn composer(t: &Tokens) -> impl Fn(&Theme, text_editor::Status) -> text_editor::Style + use<> {
    let t = *t;
    move |_, _| text_editor::Style {
        background: Color::TRANSPARENT.into(),
        border: Border::default(),
        placeholder: color(t.text_weak),
        value: color(t.text_strong),
        selection: color(t.text_interactive_base.alpha(0.3)),
    }
}

/// A one-pixel divider.
pub fn divider(c: Rgba) -> impl Fn(&Theme) -> rule::Style {
    move |_| rule::Style {
        color: color(c),
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

/// A scrollable with a thin scroller that is only visible against the rail.
pub fn thin_scroll(t: &Tokens) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style + use<> {
    let t = *t;
    move |theme, status| {
        let mut style = scrollable::default(theme, status);
        let scroller = match status {
            scrollable::Status::Hovered {
                is_vertical_scrollbar_hovered: true,
                ..
            }
            | scrollable::Status::Dragged { .. } => t.text_weaker,
            _ => t.border_weak_base,
        };
        style.vertical_rail.background = None;
        style.vertical_rail.border = Border::default();
        style.vertical_rail.scroller.background = color(scroller).into();
        style.vertical_rail.scroller.border = Border {
            radius: 4.0.into(),
            ..Border::default()
        };
        style
    }
}

/// Tints a single-colour SVG.
pub fn tint(c: Rgba) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |_, _| svg::Style {
        color: Some(color(c)),
    }
}
