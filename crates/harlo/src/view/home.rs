//! The main area before a session is open: "What are we working on?" and the composer.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Tokens;
use iced::widget::{button, column, container, row, space, text, text_editor};
use iced::{Center, Element, Fill, Padding};
use lucide_icons::Icon;

use crate::app::Message;
use crate::ids;
use crate::style::{self, size};
use crate::view::icon;

/// The composer's widest size; it narrows with the window.
const COMPOSER_WIDTH: f32 = 760.0;

/// The empty state, centred in the main area.
pub fn view<'a>(t: &Tokens, composer: &'a text_editor::Content) -> Element<'a, Message> {
    let question = text("What are we working on?")
        .size(size::HEADLINE)
        .color(style::color(t.text_strong));
    let pickers = row![
        picker(t, Icon::House, "Home folder"),
        picker(t, Icon::Sparkles, "Default model")
    ]
    .spacing(5);
    let body = column![
        question,
        space().height(22),
        pickers,
        space().height(14),
        composer_box(t, composer)
    ]
    .align_x(Center)
    .max_width(COMPOSER_WIDTH);
    container(body)
        .width(Fill)
        .height(Fill)
        // Optically centred: a little above the middle, as in the design.
        .padding(Padding {
            top: 0.0,
            bottom: 82.0,
            left: 24.0,
            right: 24.0,
        })
        .center(Fill)
        .style(style::fill(t.background_base))
        .into()
}

fn picker<'a>(t: &Tokens, glyph: Icon, label: &'a str) -> Element<'a, Message> {
    let content = row![
        icon(glyph, 14.0, t.text_interactive_base),
        text(label)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
        icon(Icon::ChevronDown, 13.0, t.icon_base),
    ]
    .spacing(8)
    .align_y(Center);
    button(content)
        .padding([6, 12])
        .style(style::chip(t))
        .into()
}

fn composer_box<'a>(t: &Tokens, content: &'a text_editor::Content) -> Element<'a, Message> {
    let editor = text_editor(content)
        .placeholder("Ask, or describe what to build.")
        .on_action(Message::Compose)
        .size(size::TEXT)
        .height(56)
        .padding(0)
        .style(style::composer(t));
    let tools = row![
        icon(Icon::CirclePlus, 17.0, t.icon_base),
        icon(Icon::ShieldUser, 17.0, t.icon_base),
        space().width(Fill),
        text("Enter to start · Shift+Enter for a new line")
            .size(11.5)
            .color(style::color(t.text_weak)),
        icon(Icon::SendHorizontal, 17.0, t.text_weaker),
    ]
    .spacing(14)
    .align_y(Center);
    container(column![editor, tools].spacing(10))
        .id(ids::COMPOSER)
        .width(Fill)
        .padding(Padding {
            top: 16.0,
            bottom: 12.0,
            left: 18.0,
            right: 16.0,
        })
        .style(style::boxed(
            t.surface_raised_base,
            Some(t.border_weak_base),
            size::COMPOSER_RADIUS,
        ))
        .into()
}
