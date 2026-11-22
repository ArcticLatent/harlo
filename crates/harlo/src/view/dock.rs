//! The main area: each tab's content, its title and icon, the buttons in a
//! tool's header, and the dock drawn in the theme's colours.
//!
//! A tool alone in its pane shows a header rather than a tab: a grip to
//! drag it by, its name, and no close button (its rail icon closes it).
//! Terminals always have a close button: closing one ends its shell.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Tokens;
use iced::{Border, Color, Element};
use iced_dock::{
    CloseButtonStyle, DockBackgroundStyle, DockStyle, DropOverlayStyle, SplitterStyle, TabBarStyle,
    TabIcon, TabStyle, TabTooltipStyle, WindowStyle,
};
use lucide_icons::Icon;

use crate::app::{App, DockKey, Message, OpenFile, TerminalEvent};
use crate::style::{self, size};
use crate::view;

/// The font Lucide's glyphs are in.
const LUCIDE: iced::Font = iced::Font::with_name("lucide");

/// The narrowest a pane gets.
const MIN_PANE: f32 = 220.0;

/// The space around a terminal's text.
const TERMINAL_PADDING: [u16; 2] = [6, 12];

/// How far in from a pane's edge a drop splits it, as a share of its size.
const EDGE: f32 = 0.34;

/// The dock, with each tab's content.
pub fn view(app: &App) -> Element<'_, Message> {
    let tokens = app.tokens;
    iced_dock::dock()
        .state(app.dock.state())
        .on_event(Message::Dock)
        .on_close_requested(Message::CloseDocked)
        .content(move |key| content(app, key))
        .tab_title(move |key| Some(title(app, key)))
        .tab_icon(move |key| Some(tab_icon(app, key)))
        .modified(move |key| key == DockKey::Editor && app.fs.open.iter().any(OpenFile::is_dirty))
        .tab_tooltip(move |key| tooltip(app, key))
        .tab_closable(move |key, count| {
            key.is_document() || matches!(key, DockKey::Terminal(_)) || count > 1
        })
        .pane_actions(move |key| match key {
            DockKey::Panel(panel) => Some(view::panel::actions(app, panel)),
            DockKey::Terminal(_) => Some(view::panel::terminal_actions(app)),
            _ => None,
        })
        // Tabs join a group through its tab bar; a pane's middle takes nothing.
        .content_fill(false)
        .drop_edge_fraction(EDGE)
        .style(move |_| style(&tokens))
        .tab_font(style::SANS)
        .tab_text_size(size::LABEL)
        .tab_bar_height(size::TAB_BAR)
        .tab_padding([0.0, 12.0])
        .tab_accent_height(2.0)
        .close_button_size(18.0)
        .splitter_gap(1.0)
        .pane_padding(0.0)
        .min_pane_width(MIN_PANE)
        .build()
        .into()
}

/// What a tab shows.
fn content(app: &App, key: DockKey) -> Element<'_, Message> {
    match key {
        DockKey::Home => view::home::view(app),
        DockKey::Session(_) => match app
            .docked_session(key)
            .and_then(|id| Some((id, app.timeline(id)?)))
        {
            Some((id, timeline)) => view::session::view(app, id, timeline),
            None => quiet(app, "Opening the session…"),
        },
        DockKey::Editor => match app.fs.open.first() {
            Some(file) => view::editor::view(app, file),
            None => quiet(app, "No file is open."),
        },
        // The maximized panel draws over the dock; its pane waits underneath.
        DockKey::Panel(panel) if app.maximized == Some(panel) => quiet(app, ""),
        DockKey::Panel(panel) => view::panel::view(app, panel),
        DockKey::Terminal(id) => match app.terminals.get(id) {
            Some(shell) => iced::widget::container(
                iced_term::TerminalView::show(&shell.term)
                    .map(|event| Message::Terminal(TerminalEvent::Backend(event))),
            )
            .padding(TERMINAL_PADDING)
            .style(style::fill(app.tokens.background_base))
            .into(),
            None => quiet(app, "The terminal has closed."),
        },
    }
}

/// A tab's title.
fn title(app: &App, key: DockKey) -> String {
    match key {
        DockKey::Home => "New session".to_owned(),
        DockKey::Session(_) => app
            .docked_session(key)
            .map(|id| app.title(id))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Untitled session".to_owned()),
        DockKey::Editor => app
            .fs
            .open
            .first()
            .map_or_else(|| "Editor".to_owned(), |f| f.name().to_owned()),
        DockKey::Panel(panel) => panel.title().to_owned(),
        DockKey::Terminal(id) => app
            .terminals
            .get(id)
            .map_or_else(|| "Terminal".to_owned(), |s| s.title.clone()),
    }
}

/// The glyph before a tab's title.
fn tab_icon(app: &App, key: DockKey) -> TabIcon {
    let t = &app.tokens;
    let lucide = |glyph: Icon| TabIcon {
        glyph: glyph.unicode(),
        font: LUCIDE,
        active: style::color(t.icon_selected),
        inactive: style::color(t.icon_weak_base),
    };
    match key {
        DockKey::Home => lucide(Icon::Plus),
        DockKey::Session(_) => lucide(Icon::Bot),
        // A tool alone in its pane has a header: the grip says it can be moved.
        DockKey::Panel(_) if app.dock.tab_count(key) == 1 => lucide(Icon::GripVertical),
        DockKey::Panel(panel) => lucide(view::rails::glyph(panel)),
        DockKey::Terminal(_) => lucide(Icon::SquareTerminal),
        DockKey::Editor => {
            let icon = crate::file_icons::icon(app.fs.open.first().map_or("", |f| f.path.as_str()));
            let color = icon.iced_color();
            TabIcon {
                glyph: icon.glyph,
                font: crate::file_icons::FONT,
                active: color,
                inactive: Color {
                    a: view::DIM_ICON,
                    ..color
                },
            }
        }
    }
}

/// The text shown when hovering a tab: a file's path, a session's folder.
fn tooltip(app: &App, key: DockKey) -> Option<String> {
    match key {
        DockKey::Editor => app.fs.open.first().map(|f| f.path.clone()),
        DockKey::Session(_) => app
            .docked_session(key)
            .and_then(|id| app.session_folder(id)),
        DockKey::Terminal(id) => app.terminals.get(id).map(|s| s.folder.clone()),
        DockKey::Home | DockKey::Panel(_) => None,
    }
}

/// A line of quiet text, for a tab with nothing to show yet.
fn quiet<'a>(app: &App, words: &'a str) -> Element<'a, Message> {
    let t = &app.tokens;
    iced::widget::container(
        iced::widget::text(words)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .padding([20, 18])
    .width(iced::Fill)
    .height(iced::Fill)
    .style(style::fill(t.background_base))
    .into()
}

/// The dock in the theme's colours: flat panes split by thin lines, the
/// focused pane outlined.
fn style(t: &Tokens) -> DockStyle {
    let line = style::color(t.border_weak_base);
    let accent = style::color(t.text_interactive_base);
    let faint = |c: Color, a: f32| Color { a, ..c };
    DockStyle {
        background: DockBackgroundStyle { color: line },
        window: WindowStyle {
            background: style::color(t.background_base),
            // As thick as the focus outline, so the outline isn't covered.
            border: Border {
                width: 1.0,
                color: style::color(t.background_base),
                radius: 0.0.into(),
            },
            focused_border: Some(Border {
                width: 1.0,
                color: faint(accent, 0.28),
                radius: 0.0.into(),
            }),
        },
        tab_bar: TabBarStyle {
            background: style::color(t.background_base),
            close_button: CloseButtonStyle {
                text_color: style::color(t.icon_base),
                background: Color::TRANSPARENT,
                hovered_background: style::color(t.surface_base_hover),
                hovered_text: style::color(t.text_strong),
                border_radius: 4.0,
            },
            separator: Some(style::color(t.border_weaker_base)),
            scrollbar_track: Color::TRANSPARENT,
            scrollbar_thumb: style::color(t.border_base),
            scrollbar_thumb_hovered: style::color(t.icon_base),
            scrollbar_thumb_border: Color::TRANSPARENT,
        },
        tab: TabStyle {
            border_radius: 0.0,
            inactive_background: style::color(t.background_base),
            inactive_text: style::color(t.text_weak),
            hovered_background: style::color(t.surface_base_hover),
            hovered_text: style::color(t.text_base),
            pressed_background: style::color(t.surface_base_active),
            pressed_text: style::color(t.text_strong),
            active_background: style::color(t.surface_raised_base),
            active_text: style::color(t.text_strong),
            active_accent: accent,
            modified_background: None,
        },
        splitter: SplitterStyle {
            idle_color: Color::TRANSPARENT,
            hover_color: accent,
            drag_color: accent,
        },
        drop_overlay: DropOverlayStyle {
            color: faint(accent, 0.18),
            blocked_color: faint(style::color(t.icon_critical_base), 0.18),
            border_width: 1.5,
            border_color: accent,
            blocked_border_color: style::color(t.icon_critical_base),
            insert_marker_min_alpha: 0.65,
        },
        tooltip: TabTooltipStyle {
            background: style::color(t.surface_raised_strong),
            text_color: style::color(t.text_base),
            border_color: line,
            border_width: 1.0,
            border_radius: 6.0,
            padding: [4.0, 8.0],
        },
    }
}
