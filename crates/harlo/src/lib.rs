//! Harlo's window: state, messages and views, built with iced.
//!
//! `main.rs` runs it; the tests drive it headless with `iced_test`.

// Rust guideline compliant 2026-10-09

pub mod app;
pub mod assets;
mod clipboard;
pub mod fit;
pub mod ids;
pub mod motion;
pub mod options;
pub mod style;
pub mod timeline;
mod view;
pub mod widget;

#[doc(inline)]
pub use app::{
    App, ChangesEvent, ChangesLoaded, ContextEvent, ContextLoaded, GitAction, MenuState, Message,
    Panel, Status,
};
#[doc(inline)]
pub use options::Options;
#[doc(inline)]
pub use view::locations::status_text;
#[doc(inline)]
pub use view::pickers::{
    AgentChoice, Folder, MENU_LIST, MENU_ROW, MENU_ROWS, ModelChoice, Picker, ROW_GAP, Variant,
};

/// Opens the window and runs until it closes.
///
/// # Errors
///
/// Returns an error when the window or its graphics can't be created.
pub fn run(options: Options) -> iced::Result {
    let boot = move || {
        let locations = options.locations();
        let location = locations
            .last()
            .cloned()
            .unwrap_or(harlo_core::server::Location::Shared);
        // A missing or unreadable layout starts from the default one.
        let store = harlo_core::layout::Store::user();
        let layout = store
            .as_ref()
            .and_then(|store| store.load().ok())
            .unwrap_or_default();
        let app = App::new(location, locations, layout).notifying();
        match store {
            Some(store) => app.saving_to(store),
            None => app,
        }
    };
    let mut app = iced::application(boot, App::update, App::view)
        .title("Harlo")
        .theme(App::theme)
        .subscription(App::subscription)
        .default_font(style::SANS)
        .window(iced::window::Settings {
            size: iced::Size::new(fit::WINDOW.0, fit::WINDOW.1),
            min_size: Some(iced::Size::new(fit::WINDOW_MIN.0, fit::WINDOW_MIN.1)),
            ..iced::window::Settings::default()
        });
    for font in style::FONTS {
        app = app.font(font);
    }
    app.run()
}
