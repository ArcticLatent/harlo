//! The narrow icon rails on the window's left and right edges.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Tokens;
use iced::widget::{button, center, column, container, image, row, rule, space};
use iced::{Element, Fill, Padding};
use lucide_icons::Icon;

use crate::app::{Message, Panel};
use crate::assets::Assets;
use crate::ids;
use crate::style::{self, size};
use crate::view::icon;

/// Gap between rail buttons.
const GAP: f32 = 8.0;

/// Space above the first and below the last rail item.
const EDGE: f32 = 11.0;

/// The left rail: the sidebar toggle, then machines, shortcuts, stats, settings and you.
pub fn left(t: &Tokens, assets: &Assets) -> Element<'static, Message> {
    let avatar = image(assets.avatar.clone())
        .width(32)
        .height(32)
        .border_radius(16);
    let items = column![
        rail_button(
            t,
            Icon::PanelLeft,
            ids::SIDEBAR_TOGGLE,
            false,
            Some(Message::ToggleSidebar)
        ),
        space().height(Fill),
        // The phone link shows as on, as in the design; the iPhone milestone wires it up.
        rail_button(t, Icon::Smartphone, "rail-phone", true, None),
        rail_button(t, Icon::Keyboard, "rail-shortcuts", false, None),
        rail_button(t, Icon::ChartColumn, "rail-stats", false, None),
        rail_button(t, Icon::Settings, "rail-settings", false, None),
        center(avatar).height(size::RAIL_BUTTON + GAP),
    ]
    .spacing(GAP)
    .align_x(iced::Center);
    rail(t, Edge::Left, items)
}

/// The right rail: one button per panel, and the terminal at the bottom.
pub fn right(t: &Tokens, open: Option<Panel>) -> Element<'static, Message> {
    let mut items = column![].spacing(GAP).align_x(iced::Center);
    for panel in Panel::ALL {
        let glyph = match panel {
            Panel::Context => Icon::Clock,
            Panel::Todos => Icon::ListChecks,
            Panel::Changes => Icon::GitBranch,
            Panel::Files => Icon::Files,
            Panel::Notes => Icon::FileText,
        };
        items = items.push(rail_button(
            t,
            glyph,
            panel.id(),
            open == Some(panel),
            Some(Message::OpenPanel(panel)),
        ));
    }
    let items = items.push(space().height(Fill)).push(rail_button(
        t,
        Icon::SquareTerminal,
        "rail-terminal",
        false,
        None,
    ));
    rail(t, Edge::Right, items)
}

/// Which window edge a rail sits on; its divider goes on the inner side.
#[derive(Clone, Copy)]
enum Edge {
    Left,
    Right,
}

fn rail<'a>(
    t: &Tokens,
    edge: Edge,
    items: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let inner = size::RAIL - 1.0;
    let body = container(items)
        .width(inner)
        .height(Fill)
        .padding(Padding {
            top: EDGE,
            bottom: EDGE,
            left: 0.0,
            right: 0.0,
        })
        .center_x(inner)
        .style(style::fill(t.background_stronger));
    let line = rule::vertical(1).style(style::divider(t.border_weak_base));
    match edge {
        Edge::Left => row![body, line],
        Edge::Right => row![line, body],
    }
    .into()
}

/// A square icon button with a widget ID; `None` leaves it inert until its milestone.
fn rail_button(
    t: &Tokens,
    glyph: Icon,
    id: &'static str,
    selected: bool,
    on_press: Option<Message>,
) -> Element<'static, Message> {
    let color = if selected {
        t.icon_selected
    } else {
        t.icon_base
    };
    let button = button(center(icon(glyph, size::RAIL_ICON, color)))
        .width(size::RAIL_BUTTON)
        .height(size::RAIL_BUTTON)
        .padding(0)
        .on_press_maybe(on_press)
        .style(style::ghost(t, selected));
    container(button).id(id).into()
}
