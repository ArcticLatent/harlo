//! The right panel. Each panel shows an empty state until its content lands.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Tokens;
use iced::widget::{button, center, column, container, row, rule, space, text};
use iced::{Center, Element, Fill};
use lucide_icons::Icon;

use crate::app::{Message, Panel};
use crate::ids;
use crate::style::{self, size};
use crate::view::icon;

/// The open panel, with a divider on its left edge.
///
/// `session_open` says whether a session is shown, which changes the empty state.
pub fn view(t: &Tokens, panel: Panel, session_open: bool) -> Element<'static, Message> {
    let close = button(center(icon(Icon::X, 14.0, t.icon_base)))
        .width(30)
        .height(30)
        .padding(0)
        .on_press(Message::ClosePanel)
        .style(style::round(t));
    let mut header = row![
        text(panel.title())
            .font(style::SANS_SEMIBOLD)
            .size(size::TITLE)
            .color(style::color(t.text_strong))
    ]
    .spacing(12)
    .align_y(Center)
    .height(74)
    .padding([0, 18]);
    header = header.push(space().width(Fill));
    if panel == Panel::Context {
        header = header.push(icon(Icon::RefreshCw, 15.0, t.icon_base));
    }
    let header = header.push(container(close).id(ids::PANEL_CLOSE));
    let body = text(if session_open {
        "Nothing to show for this session yet."
    } else {
        "No session selected."
    })
    .size(size::LABEL)
    .color(style::color(t.text_base));
    let content = column![
        header,
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
        container(body).padding([20, 18])
    ];
    row![
        rule::vertical(1).style(style::divider(t.border_weak_base)),
        content
    ]
    .width(size::PANEL + 1.0)
    .height(Fill)
    .into()
}
