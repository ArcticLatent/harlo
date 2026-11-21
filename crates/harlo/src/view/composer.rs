//! The composer: where prompts are written and sent.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Tokens;
use iced::keyboard::{Key, key::Named};
use iced::widget::{button, column, container, row, space, text, text_editor};
use iced::{Center, Element, Fill, Padding};
use lucide_icons::Icon;

use crate::app::Message;
use crate::ids;
use crate::style::{self, size};
use crate::view::icon;

/// The composer box: the text area, its tools, a hint, and the send button.
///
/// Enter sends and Shift+Enter starts a new line. `error` explains a send
/// that failed, under the box.
pub fn view<'a>(
    t: &Tokens,
    content: &'a text_editor::Content,
    placeholder: &'a str,
    hint: &'a str,
    error: Option<&'a str>,
) -> Element<'a, Message> {
    let editor = text_editor(content)
        .placeholder(placeholder)
        .on_action(Message::Compose)
        .key_binding(|press| {
            let enter = matches!(press.key, Key::Named(Named::Enter));
            if enter
                && !press.modifiers.shift()
                && matches!(press.status, text_editor::Status::Focused { .. })
            {
                Some(text_editor::Binding::Custom(Message::Send))
            } else {
                text_editor::Binding::from_key_press(press)
            }
        })
        .size(size::TEXT)
        .height(56)
        .padding(0)
        .style(style::composer(t));
    let has_text = !content.text().trim().is_empty();
    let send = button(icon(
        Icon::SendHorizontal,
        17.0,
        if has_text {
            t.icon_selected
        } else {
            t.text_weaker
        },
    ))
    .padding(2)
    .on_press_maybe(has_text.then_some(Message::Send))
    .style(style::ghost(t, false));
    let tools = row![
        icon(Icon::CirclePlus, 17.0, t.icon_base),
        icon(Icon::ShieldUser, 17.0, t.icon_base),
        space().width(Fill),
        text(hint).size(11.5).color(style::color(t.text_weak)),
        container(send).id(ids::SEND),
    ]
    .spacing(14)
    .align_y(Center);
    let card = container(column![editor, tools].spacing(10))
        .id(ids::COMPOSER)
        .width(Fill)
        .padding(Padding {
            top: 16.0,
            bottom: 12.0,
            left: 18.0,
            right: 14.0,
        })
        .style(style::boxed(
            t.surface_raised_base,
            Some(t.border_weak_base),
            size::COMPOSER_RADIUS,
        ));
    match error {
        Some(error) => column![
            card,
            text(error)
                .size(size::SMALL)
                .color(style::color(t.icon_critical_base))
        ]
        .spacing(8)
        .into(),
        None => card.into(),
    }
}
