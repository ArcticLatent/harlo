//! The sidebar: logo, location, new session, filter, then projects and their sessions.

// Rust guideline compliant 2026-10-09

use harlo_core::sessions::{Project, SessionSummary, compact_age};
use harlo_core::theme::{Rgba, Tokens};
use iced::widget::{
    button, center, column, container, row, rule, scrollable, space, svg, text, text_input,
};
use iced::{Center, Element, Fill, Padding};
use lucide_icons::Icon;

use crate::app::{App, Message};
use crate::assets::Assets;
use crate::ids;
use crate::style::{self, size};
use crate::view::icon;
use crate::widget::ellipsis;

/// Space between the sidebar's edge and its content.
const INSET: f32 = 14.0;

/// Height of a project heading.
const PROJECT_ROW: f32 = 36.0;

/// Height of a session row.
const SESSION_ROW: f32 = 42.0;

/// Side of a session's maker logo tile.
const MAKER_TILE: f32 = 26.0;

/// The whole sidebar.
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let header = column![
        brand(t, &app.assets),
        space().height(6),
        location(t),
        space().height(14),
        new_session(t),
        space().height(12),
        filter(t, &app.filter),
        space().height(7),
    ]
    .padding(Padding {
        top: 10.0,
        bottom: 0.0,
        left: INSET,
        right: INSET,
    });

    let mut groups = column![].padding(Padding {
        top: 0.0,
        bottom: 12.0,
        left: INSET,
        right: INSET,
    });
    for (index, project) in app.projects.iter().enumerate() {
        groups = groups.push(group(
            t,
            &app.assets,
            index,
            project,
            &app.filter,
            app.expanded.contains(&index),
        ));
    }
    let list = scrollable(groups).height(Fill).style(style::thin_scroll(t));

    let body = container(column![header, list])
        .width(size::SIDEBAR)
        .height(Fill)
        .style(style::fill(t.background_stronger));
    row![
        body,
        rule::vertical(1).style(style::divider(t.border_weak_base))
    ]
    .into()
}

/// The logo, the wordmark and `+ Project`.
fn brand(t: &Tokens, assets: &Assets) -> Element<'static, Message> {
    let mark = svg(assets.logo.clone()).width(23).height(24);
    let wordmark = text("Harlo")
        .font(style::SANS_SEMIBOLD)
        .size(size::WORDMARK)
        .color(style::color(t.text_strong));
    let add = button(text("+ Project").size(size::SMALL))
        .padding([5, 9])
        .style(style::chip(t));
    row![mark, space().width(6), wordmark, space().width(Fill), add]
        .height(40)
        .align_y(Center)
        .into()
}

/// The location picker: which OpenCode server Harlo is attached to.
fn location(t: &Tokens) -> Element<'static, Message> {
    let dot =
        container(space().width(7).height(7)).style(style::boxed(t.icon_success_base, None, 4.0));
    let label = row![
        dot,
        icon(Icon::Monitor, 13.0, t.icon_base),
        text("Local")
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
        space().width(Fill),
        icon(Icon::ChevronDown, 13.0, t.icon_base),
    ]
    .spacing(7)
    .height(Fill)
    .align_y(Center);
    button(label)
        .width(Fill)
        .height(31)
        .padding([0, 10])
        .style(style::chip(t))
        .into()
}

fn new_session(t: &Tokens) -> Element<'static, Message> {
    let label = row![
        icon(Icon::MessageSquarePlus, 17.0, t.text_strong),
        text("New session")
            .font(style::SANS_MEDIUM)
            .size(size::TEXT)
            .color(style::color(t.text_strong)),
        space().width(Fill),
        text("Ctrl+N")
            .size(size::SMALL)
            .color(style::color(t.text_weak)),
    ]
    .spacing(10)
    .height(Fill)
    .align_y(Center);
    button(label)
        .width(Fill)
        .height(34)
        .padding([0, 12])
        .style(style::ghost(t, false))
        .into()
}

fn filter<'a>(t: &Tokens, value: &'a str) -> Element<'a, Message> {
    text_input("Filter sessions", value)
        .id(ids::SESSION_FILTER)
        .on_input(Message::Filter)
        .icon(text_input::Icon {
            font: iced::Font::with_name("lucide"),
            code_point: Icon::Search.unicode(),
            size: Some(15.0.into()),
            spacing: 9.0,
            side: text_input::Side::Left,
        })
        .size(size::TEXT)
        .padding([9, 12])
        .style(style::filter(t))
        .into()
}

/// A project heading and its sessions.
fn group<'a>(
    t: &Tokens,
    assets: &Assets,
    index: usize,
    project: &'a Project,
    filter: &str,
    expanded: bool,
) -> Element<'a, Message> {
    let (sessions, hidden) = project.visible(filter, expanded);
    let filtering = !filter.trim().is_empty();
    if filtering && sessions.is_empty() {
        return column![].into();
    }

    let mut counts = row![].spacing(5).align_y(Center);
    if project.working > 0 {
        counts = counts.push(icon(Icon::Loader, 12.0, t.icon_weak_base));
    }
    let heading = row![
        icon(Icon::Folder, 14.0, t.icon_base),
        ellipsis(project.name.as_str())
            .font(style::SANS_SEMIBOLD)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
        counts.push(
            text(project.sessions.len())
                .size(size::SMALL)
                .color(style::color(t.text_weak))
        ),
        icon(Icon::Plus, 15.0, t.icon_base),
    ]
    .spacing(9)
    .height(PROJECT_ROW)
    .padding([0, 4])
    .align_y(Center);

    let mut rows = column![heading];
    for session in sessions {
        rows = rows.push(session_row(t, assets, session));
    }
    if hidden > 0
        || (expanded
            && !filtering
            && project.sessions.len() > harlo_core::sessions::VISIBLE_SESSIONS)
    {
        let label = if hidden > 0 {
            format!("Show {hidden} more")
        } else {
            "Show fewer".to_owned()
        };
        let more = button(
            text(label)
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        )
        .padding([6, 8])
        .on_press(Message::ToggleMore(index))
        .style(style::ghost(t, false));
        rows = rows.push(container(more).padding(Padding {
            left: 16.0,
            ..Padding::ZERO
        }));
    }
    rows.push(space().height(13)).into()
}

fn session_row<'a>(
    t: &Tokens,
    assets: &Assets,
    session: &'a SessionSummary,
) -> Element<'a, Message> {
    let content = row![
        maker(t, assets, session.maker.as_deref()),
        ellipsis(session.title.as_str())
            .size(size::TEXT)
            .color(style::color(t.text_base)),
        text(compact_age(session.age))
            .size(size::SMALL)
            .color(style::color(t.text_weak)),
    ]
    .spacing(11)
    .align_y(Center);
    container(content)
        .height(SESSION_ROW)
        .padding([0, 4])
        .align_y(Center)
        .into()
}

/// The tile showing who made a session's model.
fn maker(t: &Tokens, assets: &Assets, id: Option<&str>) -> Element<'static, Message> {
    // Each maker's own colour; unknown makers use the text colour.
    let tone = match id {
        Some("anthropic") => Rgba::parse("#e99b73"),
        Some("zai") => Rgba::parse("#849cff"),
        _ => None,
    }
    .unwrap_or(t.text_base);
    let logo: Element<'static, Message> = match id.and_then(|id| assets.provider(id)) {
        Some(handle) => svg(handle.clone())
            .width(15)
            .height(15)
            .style(style::tint(tone))
            .into(),
        None => icon(Icon::Bot, 14.0, tone).into(),
    };
    container(center(logo))
        .width(MAKER_TILE)
        .height(MAKER_TILE)
        .style(style::boxed(
            tone.over(t.background_stronger, 0.18),
            None,
            5.0,
        ))
        .into()
}
