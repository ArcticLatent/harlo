//! The window's parts, one module each.

// Rust guideline compliant 2026-10-09

pub mod composer;
pub mod home;
pub mod locations;
pub mod panel;
pub mod rails;
pub mod session;
pub mod sidebar;

use harlo_core::theme::Rgba;
use iced::widget::Text;
use lucide_icons::Icon;

/// A Lucide icon at the given size and colour.
pub fn icon(glyph: Icon, size: f32, color: Rgba) -> Text<'static> {
    glyph
        .widget()
        .size(size)
        .color(crate::style::color(color))
        .line_height(1.0)
}

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
