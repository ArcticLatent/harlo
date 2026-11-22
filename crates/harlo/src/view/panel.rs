//! The right panel. Each panel shows an empty state until its content lands.

// Rust guideline compliant 2026-10-09

use iced::widget::{button, center, column, container, row, rule, space, text};
use iced::{Center, Element, Fill};
use lucide_icons::Icon;

use crate::app::{App, ChangesEvent, ContextEvent, FilesEvent, Message, Panel};
use crate::ids;
use crate::style::{self, size};
use crate::view::icon;

/// The open panel, with a divider on its left edge.
pub fn view(app: &App, panel: Panel) -> Element<'_, Message> {
    let t = &app.tokens;
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
    let refresh = match panel {
        Panel::Context if app.active.is_some() => Some(Message::Context(ContextEvent::Refresh)),
        Panel::Changes => Some(Message::Changes(ChangesEvent::Refresh)),
        Panel::Files => Some(Message::Files(FilesEvent::Refresh)),
        Panel::Context | Panel::Todos | Panel::Notes => None,
    };
    if let Some(refresh) = refresh {
        header = header.push(
            button(center(icon(Icon::RefreshCw, 14.0, t.icon_base)))
                .width(30)
                .height(30)
                .padding(0)
                .on_press(refresh)
                .style(style::round(t)),
        );
    }
    let header = header.push(container(close).id(ids::PANEL_CLOSE));
    let body: Element<'_, Message> = if panel == Panel::Changes {
        crate::view::changes::body(app)
    } else if panel == Panel::Context {
        crate::view::context::body(app)
    } else if panel == Panel::Notes {
        crate::view::notes::body(app)
    } else if panel == Panel::Files {
        crate::view::files::body(app)
    } else if panel == Panel::Todos {
        crate::view::todos::body(app)
    } else {
        container(
            text(if app.active.is_some() {
                "Nothing to show for this session yet."
            } else {
                "No session selected."
            })
            .size(size::LABEL)
            .color(style::color(t.text_base)),
        )
        .padding([20, 18])
        .into()
    };
    let content = column![
        header,
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
        body
    ];
    row![
        rule::vertical(1).style(style::divider(t.border_weak_base)),
        content
    ]
    .width(panel.width() + 1.0)
    .height(Fill)
    .into()
}
