//! A tool panel: its body in a dock pane, the buttons at the right end of
//! its pane's header, and the panel maximized over the sidebar and dock.

// Rust guideline compliant 2026-10-09

use iced::widget::{button, center, column, container, row, rule, space, text};
use iced::{Center, Element, Fill};
use lucide_icons::Icon;

use crate::app::{App, ChangesEvent, ContextEvent, FilesEvent, Message, Panel, TerminalEvent};
use crate::ids;
use crate::style::{self, size};
use crate::view::icon;

/// The height of the maximized panel's header.
const HEADER: f32 = 56.0;

/// A tool panel's body, as a dock pane shows it.
pub fn view(app: &App, panel: Panel) -> Element<'_, Message> {
    let t = &app.tokens;
    let body: Element<'_, Message> = match panel {
        Panel::Changes => crate::view::changes::body(app),
        Panel::Context => crate::view::context::body(app),
        Panel::Notes => crate::view::notes::body(app),
        Panel::Files => crate::view::files::body(app),
        Panel::Todos => crate::view::todos::body(app),
    };
    container(body)
        .width(Fill)
        .height(Fill)
        .style(style::fill(t.background_base))
        .into()
}

/// The message that loads a panel again, where it has one.
fn refresh(app: &App, panel: Panel) -> Option<Message> {
    match panel {
        Panel::Context if app.active.is_some() => Some(Message::Context(ContextEvent::Refresh)),
        Panel::Changes => Some(Message::Changes(ChangesEvent::Refresh)),
        Panel::Files => Some(Message::Files(FilesEvent::Refresh)),
        Panel::Context | Panel::Todos | Panel::Notes => None,
    }
}

/// A small round button with an icon.
fn small<'a>(app: &App, glyph: Icon, message: Message) -> Element<'a, Message> {
    let t = &app.tokens;
    button(center(icon(glyph, 13.0, t.icon_base)))
        .width(26)
        .height(26)
        .padding(0)
        .on_press(message)
        .style(style::round(t))
        .into()
}

/// The buttons at the right end of a tool's pane header: Refresh, where the
/// panel has it, and Maximize.
pub fn actions(app: &App, panel: Panel) -> Element<'_, Message> {
    let mut buttons = row![].spacing(2).align_y(Center);
    if let Some(message) = refresh(app, panel) {
        buttons = buttons.push(small(app, Icon::RefreshCw, message));
    }
    buttons = buttons.push(small(app, Icon::Maximize2, Message::Maximize(panel)));
    container(buttons)
        .height(size::TAB_BAR)
        .padding([0, 6])
        .align_y(Center)
        .style(style::fill(app.tokens.background_base))
        .into()
}

/// The button at the right end of a terminal pane's header: another terminal.
pub fn terminal_actions(app: &App) -> Element<'_, Message> {
    container(small(
        app,
        Icon::Plus,
        Message::Terminal(TerminalEvent::New),
    ))
    .height(size::TAB_BAR)
    .padding([0, 6])
    .align_y(Center)
    .style(style::fill(app.tokens.background_base))
    .into()
}

/// A tool panel over the whole space between the rails, with its name and
/// a Restore button (Escape does the same).
pub fn maximized(app: &App, panel: Panel) -> Element<'_, Message> {
    let t = &app.tokens;
    let mut header = row![
        text(panel.title())
            .font(style::SANS_MEDIUM)
            .size(size::TITLE)
            .color(style::color(t.text_strong)),
        space().width(Fill),
    ]
    .spacing(6)
    .align_y(Center)
    .padding([0, 18])
    .height(HEADER);
    if let Some(message) = refresh(app, panel) {
        header = header.push(small(app, Icon::RefreshCw, message));
    }
    let key = container(
        text("Esc")
            .size(size::SMALL)
            .color(style::color(t.text_weak)),
    )
    .padding([1, 6])
    .style(style::boxed(
        t.surface_inset_base,
        Some(t.border_weak_base),
        4.0,
    ));
    header = header.push(
        container(
            button(
                row![
                    text("Restore")
                        .size(size::LABEL)
                        .color(style::color(t.text_strong)),
                    key
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding([5, 10])
            .on_press(Message::Restore)
            .style(style::ghost(t, false)),
        )
        .id(ids::PANEL_RESTORE),
    );
    let body = match panel {
        Panel::Changes => crate::view::changes::maximized(app),
        _ => view(app, panel),
    };
    container(column![
        header,
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
        body,
    ])
    .width(Fill)
    .height(Fill)
    .style(style::fill(t.background_base))
    .into()
}
