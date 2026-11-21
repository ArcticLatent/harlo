//! The location picker: which OpenCode server Harlo follows, and how the connection stands.

// Rust guideline compliant 2026-10-09

use harlo_core::server::Location;
use iced::widget::{button, column, container, mouse_area, opaque, row, rule, space, text};
use iced::{Center, Element, Fill, Padding};
use lucide_icons::Icon;

use crate::app::{App, Message, Status};
use crate::style::{self, size};
use crate::view::icon;
use crate::view::sidebar::{location_icon, status_color};

/// Where the card sits: under the location button, aligned with the sidebar's content.
const LEFT: f32 = size::RAIL + 14.0;
const TOP: f32 = 91.0;
const WIDTH: f32 = size::SIDEBAR - 28.0;

/// Describes the connection for the picker.
#[must_use]
pub fn status_text(status: &Status) -> String {
    match status {
        Status::Connecting { attempt: 1 } => "Connecting…".to_owned(),
        Status::Connecting { attempt } => format!("Reconnecting (attempt {attempt})…"),
        Status::Connected { version } if version == harlo_core::OPENCODE_VERSION => {
            format!("Connected · OpenCode {version}")
        }
        Status::Connected { version } => format!(
            "Connected · OpenCode {version}. Harlo is tested with {}.",
            harlo_core::OPENCODE_VERSION
        ),
        Status::Lost { reason, retry_in } => format!(
            "Can't connect: {reason}. Trying again in {} s.",
            retry_in.as_secs().max(1)
        ),
    }
}

/// What a location is, under its name.
fn detail(location: &Location) -> &'static str {
    match location {
        Location::Shared => "OpenCode's background service on this computer",
        Location::Server { .. } => "Server given with --server",
    }
}

/// The open picker, over the window; clicking outside it closes it.
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let mut items = column![].spacing(2);
    for location in &app.locations {
        let current = *location == app.location;
        let check = if current {
            icon(Icon::Check, 14.0, t.icon_selected).into()
        } else {
            Element::from(space().width(14))
        };
        let label = column![
            text(location.label())
                .size(size::LABEL)
                .color(style::color(t.text_strong)),
            text(detail(location))
                .size(11.5)
                .color(style::color(t.text_weak)),
        ]
        .spacing(2);
        let content = row![
            check,
            icon(location_icon(location), 14.0, t.icon_base),
            label
        ]
        .spacing(9)
        .align_y(Center);
        items = items.push(
            button(content)
                .width(Fill)
                .padding([7, 8])
                .on_press(Message::PickLocation(location.clone()))
                .style(style::ghost(t, false)),
        );
    }

    let dot = container(space().width(7).height(7)).style(style::boxed(
        status_color(t, &app.status),
        None,
        4.0,
    ));
    let status = row![
        container(dot).padding(Padding {
            top: 5.0,
            ..Padding::ZERO
        }),
        text(status_text(&app.status))
            .size(size::SMALL)
            .color(style::color(t.text_base)),
    ]
    .spacing(8);

    let card = container(
        column![
            items,
            rule::horizontal(1).style(style::divider(style::edge(t, t.surface_raised_strong))),
            container(status).padding([4, 8]),
        ]
        .spacing(6),
    )
    .width(WIDTH)
    .padding(6)
    .style(style::menu_card(t));

    let placed = container(opaque(card)).padding(Padding {
        top: TOP,
        left: LEFT,
        ..Padding::ZERO
    });
    mouse_area(container(placed).width(Fill).height(Fill))
        .on_press(Message::ToggleLocations)
        .into()
}
