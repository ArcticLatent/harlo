//! Shared helpers for the UI tests.

// Rust guideline compliant 2026-10-09

pub mod demo;

use std::borrow::Cow;
use std::time::Duration;

use harlo::{App, Message, style};
use iced_test::Simulator;

/// Longer than any one-off motion takes.
const MOTION_SETTLED: Duration = Duration::from_secs(1);

/// The window size the design was drawn at.
pub const WINDOW: (f32, f32) = (1588.0, 947.0);

/// The app's fonts, as the real window loads them.
pub fn settings() -> iced::Settings {
    iced::Settings {
        fonts: style::FONTS
            .iter()
            .map(|font| Cow::Borrowed(*font))
            .collect(),
        default_font: style::SANS,
        ..iced::Settings::default()
    }
}

/// A headless simulator of the app's current view.
pub fn ui(app: &App) -> Simulator<'_, Message> {
    Simulator::with_size(settings(), WINDOW, app.view())
}

/// Runs `interact` against the current view, then applies the messages it produced.
pub fn interact(app: &mut App, interact: impl FnOnce(&mut Simulator<'_, Message>)) -> Vec<Message> {
    let messages: Vec<Message> = {
        let mut ui = ui(app);
        interact(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages.clone() {
        let _ = app.update(message);
    }
    settle(app);
    messages
}

/// Lets all motion finish, as if a second had passed.
pub fn settle(app: &mut App) {
    let _ = app.update(Message::Frame(app.moment().now + MOTION_SETTLED));
}

/// Returns whether the app's view shows `text`.
pub fn shows(app: &App, text: &str) -> bool {
    ui(app).find(text).is_ok()
}
