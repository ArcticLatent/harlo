//! An open session: the tab bar, its header, the timeline and the composer.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Tokens;
use iced::widget::{button, center, column, container, row, rule, space, text};
use iced::{Center, Element, Fill, Length, Padding};
use lucide_icons::Icon;

use crate::app::{App, Message, Panel};
use crate::style::{self, size};
use crate::timeline::{self, Timeline};
use crate::view::{composer, icon};
use crate::widget::ellipsis;

/// The open session's view.
pub fn view<'a>(app: &'a App, id: &'a str, timeline: &'a Timeline) -> Element<'a, Message> {
    let t = &app.tokens;
    let owned = id.to_owned();
    let body = timeline
        .view(t)
        .map(move |event| Message::Timeline(owned.clone(), event));
    let composer = container(composer::view(
        t,
        &app.composer,
        "Send a message to continue this session…",
        "Enter to send · Shift+Enter for a new line",
        app.send_error.as_deref(),
    ))
    .max_width(timeline::WIDTH)
    .padding(Padding {
        top: 4.0,
        bottom: 16.0,
        left: 0.0,
        right: 0.0,
    });
    let content = column![
        tabs(app),
        header(app, id),
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
        body,
        container(composer).center_x(Fill).padding([0, 24]),
    ];
    container(content)
        .width(Fill)
        .height(Fill)
        .style(style::fill(t.background_base))
        .into()
}

/// The tab bar: one tab per open session.
pub fn tabs(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let mut bar = row![].spacing(2).padding([6, 10]).align_y(Center);
    for id in &app.tabs {
        let active = app.active.as_deref() == Some(id.as_str());
        bar = bar.push(tab(t, id, &app.title(id), active));
    }
    column![
        container(bar).height(size::TAB_BAR).width(Fill),
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
    ]
    .into()
}

fn tab<'a>(t: &Tokens, id: &str, title: &str, active: bool) -> Element<'a, Message> {
    let label = if title.is_empty() {
        "Untitled session".to_owned()
    } else {
        title.to_owned()
    };
    let mut content = row![
        icon(
            Icon::Bot,
            13.0,
            if active {
                t.icon_selected
            } else {
                t.icon_weak_base
            }
        ),
        ellipsis(label)
            .size(size::LABEL)
            .color(style::color(if active {
                t.text_strong
            } else {
                t.text_base
            })),
    ]
    .spacing(7)
    .align_y(Center);
    if active {
        let close = button(center(icon(Icon::X, 12.0, t.icon_base)))
            .width(20)
            .height(20)
            .padding(0)
            .on_press(Message::CloseTab(id.to_owned()))
            .style(style::ghost(t, false));
        content = content.push(container(close).id(crate::ids::TAB_CLOSE));
    }
    let t = *t;
    let tab = button(content.height(Fill))
        .width(Length::Fill)
        .height(30)
        .padding([0, 10])
        .on_press(Message::OpenSession(id.to_owned()))
        .style(move |theme, status| {
            let mut style = style::ghost(&t, false)(theme, status);
            if active {
                style.background = Some(style::color(t.surface_raised_base).into());
            }
            style
        });
    container(tab).max_width(size::TAB).into()
}

/// The session's title, then its agent and model, and a link to its changes.
fn header<'a>(app: &'a App, id: &str) -> Element<'a, Message> {
    let t = &app.tokens;
    let title = app.title(id);
    let title = if title.is_empty() {
        "Untitled session".to_owned()
    } else {
        title
    };
    let (agent, model) = app.agent_and_model(id);
    let mut facts = row![icon(Icon::Bot, 13.0, t.icon_weak_base)]
        .spacing(7)
        .align_y(Center);
    if let Some(agent) = agent {
        facts = facts.push(
            text(agent)
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    if let Some(model) = model {
        facts = facts.push(
            text(model)
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    let changes = button(
        text("Changes")
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .padding([5, 9])
    .on_press(Message::OpenPanel(Panel::Changes))
    .style(style::ghost(t, false));
    let titles = column![
        ellipsis(title)
            .font(style::SANS_SEMIBOLD)
            .size(size::SESSION_TITLE)
            .color(style::color(t.text_strong)),
        facts,
    ]
    .spacing(6);
    let inner = container(
        row![titles, space().width(16), changes]
            .align_y(Center)
            .width(Fill),
    )
    .max_width(timeline::WIDTH);
    container(inner).padding([14, 24]).center_x(Fill).into()
}
