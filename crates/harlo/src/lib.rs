//! Harlo's window: state, messages and views, built with iced.
//!
//! `main.rs` runs it; the tests drive it headless with `iced_test`.

// Rust guideline compliant 2026-10-09

pub mod app;
pub mod assets;
pub mod demo;
pub mod ids;
pub mod style;
mod view;
pub mod widget;

#[doc(inline)]
pub use app::{App, Message, Panel};

/// Opens the window and runs until it closes.
///
/// # Errors
///
/// Returns an error when the window or its graphics can't be created.
pub fn run() -> iced::Result {
    let mut app = iced::application(|| App::new(demo::projects()), App::update, App::view)
        .title("Harlo")
        .theme(App::theme)
        .default_font(style::SANS)
        .window_size((1588.0, 947.0));
    for font in style::FONTS {
        app = app.font(font);
    }
    app.run()
}
