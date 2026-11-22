//! Side panels make way for the conversation in a narrow window, and come
//! back when there's room, without changing what the user chose.

// Rust guideline compliant 2026-10-09

#[expect(
    dead_code,
    reason = "shared UI helpers; these tests size their own window"
)]
mod support;

use harlo::fit::{self, MAIN_MIN, Shown, Side};
use harlo::{App, Message, Panel, ids};
use iced::widget::Id;
use iced_test::Simulator;
use support::{demo, settings, settle};

/// The context panel's width.
const PANEL: Option<f32> = Some(harlo::style::size::PANEL);

/// Which sides are on screen: (sidebar, panel).
fn sides(shown: Shown) -> (bool, bool) {
    (shown.sidebar, shown.panel)
}

fn both() -> (bool, bool) {
    (true, true)
}

#[test]
fn the_right_panel_makes_way_first_then_the_sidebar() {
    let shown = |width| fit::shown(width, true, PANEL, None);
    assert_eq!(sides(shown(1_588.0)), both());
    assert_eq!(sides(shown(1_100.0)), (true, false));
    assert_eq!(sides(shown(900.0)), (false, false));
    for width in [720.0, 900.0, 1_100.0, 1_588.0] {
        assert!(
            shown(width).main_width(width) >= MAIN_MIN,
            "the conversation keeps its room at {width}"
        );
    }
}

#[test]
fn a_panel_opened_without_room_stays_and_the_other_makes_way() {
    assert_eq!(
        sides(fit::shown(1_100.0, true, PANEL, Some(Side::Panel))),
        (false, true)
    );
    // Alone and still too wide, it stays: the user asked for it.
    assert_eq!(
        sides(fit::shown(800.0, true, None, Some(Side::Sidebar))),
        (true, false)
    );
}

#[test]
fn every_panel_is_as_wide_as_the_others() {
    for panel in [
        Panel::Context,
        Panel::Todos,
        Panel::Changes,
        Panel::Files,
        Panel::Notes,
    ] {
        assert_eq!(Some(panel.width()), PANEL, "{panel:?}");
    }
}

#[test]
fn a_closed_panel_never_comes_back_by_itself() {
    assert_eq!(
        sides(fit::shown(1_588.0, false, None, None)),
        (false, false)
    );
}

/// The app in a window `width` wide, with the context panel open.
fn app_at(width: f32) -> App {
    let mut app = demo::app();
    app.panel = Some(Panel::Context);
    let _ = app.update(Message::Resized(iced::Size::new(width, 900.0)));
    settle(&mut app);
    app
}

fn ui(app: &App, width: f32) -> Simulator<'_, Message> {
    Simulator::with_size(settings(), (width, 900.0), app.view())
}

fn on_screen(app: &App, width: f32, id: &'static str) -> bool {
    ui(app, width).find(Id::new(id)).is_ok()
}

fn click(app: &mut App, width: f32, id: &'static str) {
    let messages: Vec<Message> = {
        let mut ui = ui(app, width);
        ui.click(Id::new(id)).expect("the button");
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = app.update(message);
    }
    settle(app);
}

#[test]
fn panels_hide_for_space_and_return_unchanged() {
    let mut app = app_at(900.0);
    assert!(
        !on_screen(&app, 900.0, ids::SESSION_FILTER),
        "sidebar hidden"
    );
    assert!(!on_screen(&app, 900.0, ids::PANEL_CLOSE), "panel hidden");
    assert!(
        app.sidebar && app.panel == Some(Panel::Context),
        "choices kept"
    );

    let _ = app.update(Message::Resized(iced::Size::new(1_588.0, 900.0)));
    settle(&mut app);
    assert!(on_screen(&app, 1_588.0, ids::SESSION_FILTER));
    assert!(on_screen(&app, 1_588.0, ids::PANEL_CLOSE));
}

#[test]
fn opening_a_hidden_panel_shows_it() {
    let mut app = app_at(1_100.0);
    assert!(on_screen(&app, 1_100.0, ids::SESSION_FILTER));
    assert!(!on_screen(&app, 1_100.0, ids::PANEL_CLOSE));

    // Its rail button brings it back, rather than closing the hidden panel.
    click(&mut app, 1_100.0, Panel::Context.id());
    assert!(on_screen(&app, 1_100.0, ids::PANEL_CLOSE));
    assert!(
        !on_screen(&app, 1_100.0, ids::SESSION_FILTER),
        "the sidebar made way"
    );

    // Pressed again, it closes, and the sidebar is back.
    click(&mut app, 1_100.0, Panel::Context.id());
    assert!(!on_screen(&app, 1_100.0, ids::PANEL_CLOSE));
    assert!(on_screen(&app, 1_100.0, ids::SESSION_FILTER));
    assert_eq!(app.panel, None);
}

#[test]
fn the_sidebar_toggle_shows_a_sidebar_hidden_for_space() {
    let mut app = app_at(900.0);
    click(&mut app, 900.0, ids::SIDEBAR_TOGGLE);
    assert!(on_screen(&app, 900.0, ids::SESSION_FILTER));
    click(&mut app, 900.0, ids::SIDEBAR_TOGGLE);
    assert!(!on_screen(&app, 900.0, ids::SESSION_FILTER));
    assert!(!app.sidebar, "closed by the user this time");
}
