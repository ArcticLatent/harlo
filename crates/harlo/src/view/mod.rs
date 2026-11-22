//! The window's parts, one module each.

// Rust guideline compliant 2026-10-09

pub mod changes;
pub mod composer;
pub mod context;
pub mod diff;
pub mod dock;
pub mod editor;
pub mod files;
pub mod home;
pub mod locations;
pub mod notes;
pub mod panel;
pub mod pickers;
pub mod rails;
pub mod session;
pub mod sidebar;
pub mod subagents;
pub mod todos;
pub mod waiting;

use harlo_core::theme::Rgba;
use iced::Element;
use iced::widget::Text;
use lucide_icons::Icon;

/// A Lucide icon at the given size and colour.
/// Wraps an indicator that breathes (see `Moment::breathing`) so it keeps
/// redrawing while it's on screen.
pub fn breathing<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    crate::widget::breathe(content, crate::motion::BREATH_FRAME).into()
}

pub fn icon(glyph: Icon, size: f32, color: Rgba) -> Text<'static> {
    glyph
        .widget()
        .size(size)
        .color(crate::style::color(color))
        .line_height(1.0)
}

/// A file's type icon in its colour, faded when `dim`.
pub fn file_icon(path: &str, size: f32, dim: bool) -> Text<'static> {
    let icon = crate::file_icons::icon(path);
    let mut color = icon.iced_color();
    if dim {
        color.a = DIM_ICON;
    }
    Text::new(icon.glyph.to_string())
        .font(crate::file_icons::FONT)
        .size(size)
        .color(color)
        .line_height(1.0)
}

/// How opaque a faded file icon is, as for an entry Git ignores.
pub const DIM_ICON: f32 = 0.45;

/// Formats how long something took: `850ms`, `3.7s`, `15s`, `2m 5s`.
#[must_use]
pub fn duration(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    match ms {
        0..1_000 => format!("{ms}ms"),
        1_000..10_000 => format!("{:.1}s", d.as_secs_f64()),
        10_000..60_000 => format!("{}s", d.as_secs()),
        _ => format!("{}m {}s", d.as_secs() / 60, d.as_secs() % 60),
    }
}
