//! The sidebar makes way for the main area in a narrow window, and comes
//! back when there's room, without changing what the user chose.

// Rust guideline compliant 2026-10-09

#[expect(
    dead_code,
    reason = "shared UI helpers; these tests size their own window"
)]
mod support;

use harlo::fit::{self, MAIN_MIN};
use harlo::{App, Message, ids};
use iced::widget::Id;
use iced_test::Simulator;
use support::{demo, settings, settle};

#[test]
fn the_sidebar_makes_way_for_the_main_area() {
    let shown = |width| fit::shown(width, true, false).sidebar;
    assert!(shown(1_100.0));
    assert!(!shown(800.0));
    for width in [720.0, 900.0, 1_100.0, 1_588.0] {
        assert!(
            fit::shown(width, true, false).main_width(width) >= MAIN_MIN,
            "the main area keeps its room at {width}"
        );
    }
}

#[test]
fn a_sidebar_opened_without_room_stays() {
    assert!(
        fit::shown(800.0, true, true).sidebar,
        "the user asked for it"
    );
}

#[test]
fn a_closed_sidebar_never_comes_back_by_itself() {
    assert!(!fit::shown(1_588.0, false, false).sidebar);
}

/// The app in a window `width` wide.
fn app_at(width: f32) -> App {
    let mut app = demo::app();
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
fn the_sidebar_hides_for_space_and_returns_unchanged() {
    let mut app = app_at(800.0);
    assert!(
        !on_screen(&app, 800.0, ids::SESSION_FILTER),
        "sidebar hidden"
    );
    assert!(app.sidebar, "the choice is kept");

    let _ = app.update(Message::Resized(iced::Size::new(1_588.0, 900.0)));
    settle(&mut app);
    assert!(on_screen(&app, 1_588.0, ids::SESSION_FILTER));
}

#[test]
fn the_sidebar_toggle_shows_a_sidebar_hidden_for_space() {
    let mut app = app_at(800.0);
    click(&mut app, 800.0, ids::SIDEBAR_TOGGLE);
    assert!(on_screen(&app, 800.0, ids::SESSION_FILTER));
    click(&mut app, 800.0, ids::SIDEBAR_TOGGLE);
    assert!(!on_screen(&app, 800.0, ids::SESSION_FILTER));
    assert!(!app.sidebar, "closed by the user this time");
}
