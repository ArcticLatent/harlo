//! The composer: where prompts are written, given attachments, and sent.

// Rust guideline compliant 2026-10-09

use harlo_core::composer::Attached;
use std::collections::HashMap;

use harlo_core::theme::{Rgba, Tokens};
use iced::animation::Animation;
use iced::keyboard::{Key, key::Named};
use iced::widget::{button, column, container, row, scrollable, space, text, text_editor};
use iced::{Center, Element, Fill, Padding, padding};
use lucide_icons::Icon;

use crate::app::{MenuState, Message};
use crate::ids;
use crate::motion::{self, Moment};
use crate::style::{self, size};
use crate::view::{breathing, icon};

/// Menu rows shown before the menu scrolls.
const MENU_ROWS: f32 = 8.0;

/// Height of one menu row.
const MENU_ROW: f32 = 30.0;

/// The narrowest composer whose tool row also holds the pickers.
///
/// The attach and mode icons, the agent, reasoning and model pickers, stop
/// and send need about 590 px with their gaps and the card's padding.
/// Narrower, the pickers get a line of their own instead of being cut off.
pub const INLINE_CONTROLS: f32 = 620.0;

/// What the composer shows.
#[derive(Debug, Clone, Copy)]
pub struct Composer<'a> {
    /// The text being written.
    pub content: &'a text_editor::Content,
    /// Shown while the text is empty.
    pub placeholder: &'a str,
    /// A key hint beside the send button, where there's room for one.
    pub hint: Option<&'a str>,
    /// Why the last send failed.
    pub error: Option<&'a str>,
    /// Files and images going with the prompt.
    pub attached: &'a [Attached],
    /// The open `/` or `@` menu.
    pub menu: Option<&'a MenuState>,
    /// Whether the session is working, which offers Stop.
    pub busy: bool,
    /// Whether the pickers fit on the tool row; if not, they get their own line.
    pub wide: bool,
    /// What's going on with what was sent or added.
    pub feedback: Feedback<'a>,
}

/// Feedback on the composer's input: a send on its way, a paste being read,
/// chips and errors fading in.
#[derive(Debug, Clone, Copy)]
pub struct Feedback<'a> {
    /// The moment motion is drawn at.
    pub moment: Moment,
    /// A send is on its way: the send button says so and waits.
    pub sending: bool,
    /// A pasted image is being read.
    pub pasting: bool,
    /// How far the error has faded in, from 0 to 1.
    pub error_shown: f32,
    /// Attachment chips fading in, by name.
    pub chips: &'a HashMap<String, Animation<bool>>,
}

/// The composer box: chips, the text area, its tools, and send or stop.
///
/// Enter sends and Shift+Enter starts a new line; while the session works,
/// Enter queues and Ctrl+Enter steers. With a menu open, the arrow keys move
/// through it, Enter or Tab picks, and Escape closes it. `controls` (pickers)
/// sit at the right of the tool row, or on a line above it when the composer
/// isn't [`Composer::wide`].
pub fn view<'a>(
    t: &Tokens,
    composer: Composer<'a>,
    controls: Option<Element<'a, Message>>,
    shield: Element<'a, Message>,
) -> Element<'a, Message> {
    let menu_open = composer.menu.is_some_and(|m| !m.items.is_empty());
    let editor = text_editor(composer.content)
        .placeholder(composer.placeholder)
        .on_action(Message::Compose)
        .key_binding(move |press| binding(&press, menu_open))
        .size(size::TEXT)
        .height(56)
        .padding(0)
        .style(style::composer(t));
    let feedback = composer.feedback;
    let ready = !composer.content.text().trim().is_empty() || !composer.attached.is_empty();
    // While a send is on its way the button turns into a pulsing loader and
    // waits; a second press would send twice.
    let glyph: Element<'static, Message> = if feedback.sending {
        let color = t.icon_selected;
        breathing(icon(Icon::Loader, 17.0, color).style(feedback.moment.breathing(color)))
    } else if ready {
        icon(Icon::SendHorizontal, 17.0, t.icon_selected).into()
    } else {
        icon(Icon::SendHorizontal, 17.0, t.text_weaker).into()
    };
    let send = button(glyph)
        .padding(2)
        .on_press_maybe((ready && !feedback.sending).then_some(Message::Send))
        .style(style::ghost(t, false));
    // Icons are `'static`; making each control an element lets it join the borrowed row.
    let send: Element<'static, Message> = container(send).id(ids::SEND).into();
    let attach = button(icon(Icon::CirclePlus, 17.0, t.icon_base))
        .padding(2)
        .on_press(Message::Attach)
        .style(style::ghost(t, false));
    let mut tools = row![
        container(attach).id(ids::ATTACH),
        shield,
        space().width(Fill),
    ]
    .spacing(14)
    .align_y(Center);
    if let Some(hint) = composer.hint {
        tools = tools.push(text(hint).size(11.5).color(style::color(t.text_weak)));
    }
    let mut own_line = None;
    if let Some(controls) = controls {
        if composer.wide {
            tools = tools.push(controls);
        } else {
            own_line = Some(controls);
        }
    }
    if composer.busy {
        let stop: Element<'static, Message> = container(
            button(icon(Icon::Square, 14.0, t.icon_base))
                .padding(4)
                .on_press(Message::Stop)
                .style(style::chip(t)),
        )
        .id(ids::STOP)
        .into();
        tools = tools.push(stop);
    }
    tools = tools.push(send);

    // Optional parts keep their place in the tree, empty when unused: a
    // widget's state follows its position, so the editor must not move when
    // a chip or the menu appears, or it loses its focus and cursor.
    let attached: Element<'a, Message> = if composer.attached.is_empty() && !feedback.pasting {
        space().into()
    } else {
        container(attachments(t, composer.attached, feedback))
            .padding(padding::bottom(10))
            .into()
    };
    // The editor stays first in its column, wherever the pickers go.
    let mut body = column![editor].spacing(10);
    if let Some(controls) = own_line {
        body = body.push(controls);
    }
    let inside = column![attached, body.push(tools)];
    let card = card(t, inside);
    let menu: Element<'a, Message> = match composer.menu.filter(|m| !m.items.is_empty()) {
        Some(menu) => container(menu_view(t, menu))
            .padding(padding::bottom(8))
            .into(),
        None => space().into(),
    };
    let mut out = column![menu, card];
    if let Some(error) = composer.error {
        // It fades in, rising a little, so a new error catches the eye.
        let shown = feedback.error_shown;
        out = out.push(
            container(
                text(error)
                    .size(size::SMALL)
                    .color(style::color(motion::fade(t.icon_critical_base, shown))),
            )
            .padding(padding::top(8.0 + (1.0 - shown) * motion::RISE / 2.0)),
        );
    }
    out.into()
}

/// The composer's raised box around `inside`.
fn card<'a>(t: &Tokens, inside: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(inside)
        .id(ids::COMPOSER)
        .width(Fill)
        .padding(Padding {
            top: 16.0,
            bottom: 12.0,
            left: 18.0,
            right: 14.0,
        })
        .style(style::lifted(
            t.surface_raised_base,
            style::composer_edge(t),
            size::COMPOSER_RADIUS,
        ))
        .into()
}

/// Maps a key press in the focused composer to what it does.
fn binding(
    press: &text_editor::KeyPress,
    menu_open: bool,
) -> Option<text_editor::Binding<Message>> {
    use text_editor::Binding;
    if !matches!(press.status, text_editor::Status::Focused { .. }) {
        return Binding::from_key_press(press.clone());
    }
    let modifiers = press.modifiers;
    match press.key.as_ref() {
        Key::Named(Named::ArrowUp) if menu_open => Some(Binding::Custom(Message::MenuMove(-1))),
        Key::Named(Named::ArrowDown) if menu_open => Some(Binding::Custom(Message::MenuMove(1))),
        Key::Named(Named::Enter | Named::Tab) if menu_open && !modifiers.shift() => {
            Some(Binding::Custom(Message::MenuPick(None)))
        }
        Key::Named(Named::Escape) if menu_open => Some(Binding::Custom(Message::MenuClose)),
        Key::Named(Named::Enter) if modifiers.command() => Some(Binding::Custom(Message::Steer)),
        Key::Named(Named::Enter) if !modifiers.shift() => Some(Binding::Custom(Message::Send)),
        // An image on the clipboard is attached; text, if any, is pasted as usual.
        Key::Character("v") if modifiers.command() && !modifiers.shift() => {
            Some(Binding::Sequence(vec![
                Binding::Custom(Message::PasteImage),
                Binding::Paste,
            ]))
        }
        _ => Binding::from_key_press(press.clone()),
    }
}

/// The attachments as chips, each with a button to remove it.
///
/// New chips fade in, and a pasted image still being read shows as a
/// pulsing chip at the end.
fn attachments<'a>(
    t: &Tokens,
    attached: &'a [Attached],
    feedback: Feedback<'_>,
) -> Element<'a, Message> {
    let mut list = row![].spacing(6);
    for (index, item) in attached.iter().enumerate() {
        let shown = feedback
            .chips
            .get(&item.name)
            .map_or(1.0, |appear| motion::shown(appear, feedback.moment.now));
        let fade = |c: Rgba| motion::fade(c, shown);
        let glyph = if item.attachment.uri.starts_with("data:image/") {
            Icon::Image
        } else {
            Icon::Paperclip
        };
        let remove = button(icon(Icon::X, 11.0, fade(t.icon_base)))
            .padding(1)
            .on_press(Message::Detach(index))
            .style(style::ghost(t, false));
        list = list.push(
            container(
                row![
                    icon(glyph, 12.0, fade(t.icon_base)),
                    text(item.name.as_str())
                        .size(size::SMALL)
                        .color(style::color(fade(t.text_base))),
                    remove,
                ]
                .spacing(6)
                .align_y(Center),
            )
            .padding(Padding {
                top: 3.0,
                bottom: 3.0,
                left: 8.0,
                right: 4.0,
            })
            .style(style::boxed(
                fade(t.surface_raised_strong),
                None,
                size::RADIUS,
            )),
        );
    }
    if feedback.pasting {
        let color = t.text_weak;
        let breath = feedback.moment;
        list = list.push(
            container(breathing(
                row![
                    icon(Icon::Image, 12.0, color).style(breath.breathing(color)),
                    text("Reading the pasted image…")
                        .size(size::SMALL)
                        .style(breath.breathing(color)),
                ]
                .spacing(6)
                .align_y(Center),
            ))
            .padding([3, 8])
            .style(style::boxed(t.surface_raised_strong, None, size::RADIUS)),
        );
    }
    list.wrap().into()
}

/// The `/` or `@` menu, above the composer.
fn menu_view<'a>(t: &Tokens, menu: &'a MenuState) -> Element<'a, Message> {
    let mut list = column![].spacing(2);
    for (index, item) in menu.items.iter().enumerate() {
        let selected = index == menu.selected;
        let content = row![
            text(item.label.as_str())
                .size(size::LABEL)
                .font(style::SANS_MEDIUM)
                .color(style::color(t.text_strong)),
            crate::widget::ellipsis(item.detail.clone())
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        ]
        .spacing(10)
        .align_y(Center)
        .height(Fill);
        list = list.push(
            button(content)
                .width(Fill)
                .height(MENU_ROW)
                .padding([0, 10])
                .on_press(Message::MenuPick(Some(index)))
                .style(style::menu_row(t, selected)),
        );
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a menu has far fewer rows than an f32 holds"
    )]
    let rows = (menu.items.len() as f32).min(MENU_ROWS);
    let height = rows * (MENU_ROW + 2.0) + 10.0;
    container(
        scrollable(list)
            .id(menu.scroller.clone())
            .style(style::thin_scroll(t)),
    )
    .id(ids::MENU)
    .width(Fill)
    .height(height)
    .padding(5)
    .style(style::menu_card(t))
    .into()
}
