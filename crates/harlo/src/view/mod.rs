//! The window's parts, one module each.

// Rust guideline compliant 2026-10-09

pub mod home;
pub mod panel;
pub mod rails;
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
