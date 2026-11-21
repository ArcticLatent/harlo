//! The sidebar: logo, location, new session, filter, then projects and their sessions.

// Rust guideline compliant 2026-10-09

use harlo_core::server::Location;
use harlo_core::sessions::{
    Project, SessionSummary, VISIBLE_SESSIONS, Worktree, compact_age, visible,
};
use harlo_core::theme::{Rgba, Tokens};
use iced::widget::{
    Column, button, center, column, container, row, rule, scrollable, space, svg, text, text_input,
};
use iced::{Center, Element, Fill, Padding};
use lucide_icons::Icon;

use crate::app::{App, Message, Status};
use crate::assets::Assets;
use crate::ids;
use crate::motion::Moment;
use crate::style::{self, size};
use crate::view::{breathing, icon};
use crate::widget::ellipsis;

/// Space between the sidebar's edge and its content.
const INSET: f32 = 14.0;

/// Height of a project heading.
const PROJECT_ROW: f32 = 36.0;

/// Height of a worktree heading.
const WORKTREE_ROW: f32 = 32.0;

/// Height of a session row.
const SESSION_ROW: f32 = 42.0;

/// Side of a session's maker logo tile.
const MAKER_TILE: f32 = 26.0;

/// How far worktree groups are indented under their project.
const WORKTREE_INDENT: f32 = 12.0;

/// The whole sidebar.
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let header = column![
        brand(t, &app.assets),
        space().height(6),
        location(t, &app.location, &app.status),
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

    let body: Element<'_, Message> = if app.projects.is_empty() {
        notice(t, empty_text(&app.status))
    } else {
        let mut groups = column![].padding(Padding {
            top: 0.0,
            bottom: 12.0,
            left: INSET,
            right: INSET,
        });
        for project in &app.projects {
            groups = groups.push(project_group(app, project));
        }
        scrollable(groups)
            .height(Fill)
            .style(style::thin_scroll(t))
            .into()
    };

    let content = container(column![header, body])
        .width(size::SIDEBAR)
        .height(Fill)
        .style(style::fill(t.background_stronger));
    row![
        content,
        rule::vertical(1).style(style::divider(t.border_weak_base))
    ]
    .into()
}

/// What the list says when it has no projects to show.
fn empty_text(status: &Status) -> String {
    match status {
        Status::Connecting { .. } => "Connecting to OpenCode…".to_owned(),
        Status::Lost { reason, .. } => format!("Can't connect: {reason}."),
        Status::Connected { .. } => "No projects yet. Add a folder with + Project.".to_owned(),
    }
}

fn notice(t: &Tokens, message: String) -> Element<'static, Message> {
    container(
        text(message)
            .size(size::LABEL)
            .color(style::color(t.text_weak)),
    )
    .padding([12.0, INSET + 4.0])
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
        .on_press(Message::AddProject)
        .style(style::chip(t));
    row![mark, space().width(6), wordmark, space().width(Fill), add]
        .height(40)
        .align_y(Center)
        .into()
}

/// The colour of the connection dot.
pub fn status_color(t: &Tokens, status: &Status) -> Rgba {
    match status {
        Status::Connected { .. } => t.icon_success_base,
        Status::Connecting { .. } => t.icon_warning_base,
        Status::Lost { .. } => t.icon_critical_base,
    }
}

/// The icon for a location: this computer, or another server.
pub fn location_icon(location: &Location) -> Icon {
    match location {
        Location::Shared => Icon::Monitor,
        Location::Server { .. } => Icon::Server,
    }
}

/// The location picker's button: which server Harlo follows, and how that's going.
fn location(t: &Tokens, location: &Location, status: &Status) -> Element<'static, Message> {
    let dot = container(space().width(7).height(7)).style(style::boxed(
        status_color(t, status),
        None,
        4.0,
    ));
    let label = row![
        dot,
        icon(location_icon(location), 13.0, t.icon_base),
        text(location.label())
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
        space().width(Fill),
        icon(Icon::ChevronDown, 13.0, t.icon_base),
    ]
    .spacing(7)
    .height(Fill)
    .align_y(Center);
    let picker = button(label)
        .width(Fill)
        .height(31)
        .padding([0, 10])
        .on_press(Message::ToggleLocations)
        .style(style::chip(t));
    container(picker).id(ids::LOCATION).into()
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
        .on_press(Message::NewSession(None))
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

/// A project heading, its sessions, then its worktrees.
fn project_group<'a>(app: &'a App, project: &'a Project) -> Element<'a, Message> {
    let t = &app.tokens;
    let filtering = !app.filter.trim().is_empty();
    let main = list(app, &project.directory, &project.sessions);
    let worktrees: Vec<Element<'a, Message>> = project
        .worktrees
        .iter()
        .filter_map(|w| worktree_group(app, w))
        .collect();
    if filtering && main.is_none() && worktrees.is_empty() {
        return column![].into();
    }

    let mut counts = row![].spacing(5).align_y(Center);
    if project.working() > 0 {
        let working = t.text_interactive_base;
        counts = counts.push(breathing(
            icon(Icon::Loader, 12.0, working).style(app.moment().breathing(working)),
        ));
    }
    let heading = row![
        icon(Icon::Folder, 14.0, t.icon_base),
        ellipsis(project.name.as_str())
            .font(style::SANS_SEMIBOLD)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
        counts.push(
            text(project.total())
                .size(size::SMALL)
                .color(style::color(t.text_weak))
        ),
        icon(Icon::Plus, 15.0, t.icon_base),
    ]
    .spacing(9)
    .height(PROJECT_ROW)
    .padding([0, 4])
    .align_y(Center);

    let mut group = column![heading];
    if let Some(main) = main {
        group = group.push(main);
    }
    group = group.extend(worktrees);
    group.push(space().height(13)).into()
}

/// A worktree heading and its sessions; `None` when a filter hides them all.
fn worktree_group<'a>(app: &'a App, worktree: &'a Worktree) -> Option<Element<'a, Message>> {
    let t = &app.tokens;
    let filtering = !app.filter.trim().is_empty();
    let sessions = list(app, &worktree.directory, &worktree.sessions);
    if filtering && sessions.is_none() {
        return None;
    }
    let heading = row![
        icon(Icon::GitBranch, 13.0, t.icon_weak_base),
        ellipsis(worktree.name.as_str())
            .font(style::SANS_MEDIUM)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
        text(worktree.sessions.len())
            .size(size::SMALL)
            .color(style::color(t.text_weak)),
        icon(Icon::Plus, 14.0, t.icon_base),
    ]
    .spacing(9)
    .height(WORKTREE_ROW)
    .padding([0, 4])
    .align_y(Center);
    let mut group = column![heading];
    if let Some(sessions) = sessions {
        group = group.push(sessions);
    }
    Some(
        container(group)
            .padding(Padding {
                left: WORKTREE_INDENT,
                ..Padding::ZERO
            })
            .into(),
    )
}

/// The session rows of one folder, with "Show N more"; `None` when there are none to show.
fn list<'a>(
    app: &'a App,
    folder: &'a str,
    sessions: &'a [SessionSummary],
) -> Option<Element<'a, Message>> {
    let t = &app.tokens;
    let filtering = !app.filter.trim().is_empty();
    let expanded = app.expanded.contains(folder);
    let (shown, hidden) = visible(sessions, &app.filter, expanded);
    if shown.is_empty() {
        return None;
    }
    let mut rows: Column<'a, Message> = column![];
    for session in shown {
        let selected = app.active.as_deref() == Some(session.id.as_str());
        rows = rows.push(session_row(t, &app.assets, session, selected, app.moment()));
    }
    if hidden > 0 || (expanded && !filtering && sessions.len() > VISIBLE_SESSIONS) {
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
        .on_press(Message::ToggleMore(folder.to_owned()))
        .style(style::ghost(t, false));
        rows = rows.push(container(more).padding(Padding {
            left: 16.0,
            ..Padding::ZERO
        }));
    }
    Some(rows.into())
}

fn session_row<'a>(
    t: &Tokens,
    assets: &Assets,
    session: &'a SessionSummary,
    selected: bool,
    moment: Moment,
) -> Element<'a, Message> {
    let title = if session.title.is_empty() {
        ellipsis("Untitled session")
            .size(size::TEXT)
            .color(style::color(t.text_weaker))
    } else {
        ellipsis(session.title.as_str())
            .size(size::TEXT)
            .color(style::color(if selected {
                t.text_strong
            } else {
                t.text_base
            }))
    };
    let when: Element<'a, Message> = if session.busy {
        // It pulses while the session works.
        let working = t.text_interactive_base;
        breathing(icon(Icon::Loader, 13.0, working).style(moment.breathing(working)))
    } else {
        text(compact_age(session.age))
            .size(size::SMALL)
            .color(style::color(t.text_weak))
            .into()
    };
    let content = row![maker(t, assets, session.maker.as_deref()), title, when]
        .spacing(11)
        .height(Fill)
        .align_y(Center);
    button(content)
        .width(Fill)
        .height(SESSION_ROW)
        .padding([0, 4])
        .on_press(Message::OpenSession(session.id.clone()))
        .style(style::ghost(t, selected))
        .into()
}

/// The tile showing who made a session's model.
fn maker(t: &Tokens, assets: &Assets, provider: Option<&str>) -> Element<'static, Message> {
    let logo = provider.and_then(|id| assets.provider(id));
    // Each maker's own colour; others use the text colour.
    let tone = match logo.map(|(key, _)| key) {
        Some("anthropic") => Rgba::parse("#e99b73"),
        Some("zai") => Rgba::parse("#849cff"),
        _ => None,
    }
    .unwrap_or(t.text_base);
    let glyph: Element<'static, Message> = match logo {
        Some((_, handle)) => svg(handle.clone())
            .width(15)
            .height(15)
            .style(style::tint(tone))
            .into(),
        None => icon(Icon::Bot, 14.0, tone).into(),
    };
    container(center(glyph))
        .width(MAKER_TILE)
        .height(MAKER_TILE)
        .style(style::boxed(
            tone.over(t.background_stronger, 0.18),
            None,
            5.0,
        ))
        .into()
}
