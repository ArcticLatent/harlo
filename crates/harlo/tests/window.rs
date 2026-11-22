//! The window opens with the theme and the rail.

// Rust guideline compliant 2026-10-09

use harlo::{theme, ui};
use i_slint_backend_testing::ElementHandle;
use slint::ComponentHandle;

#[test]
fn the_window_shows_its_rail_in_the_theme() {
    i_slint_backend_testing::init_no_event_loop();
    let window = ui::AppWindow::new().expect("the window opens");
    let tokens = theme::default_tokens();
    theme::apply(&window, &tokens, harlo_core::theme::HARLO_DARK);
    assert_eq!(
        window.global::<ui::Theme>().get_background_base(),
        theme::color(tokens.background_base)
    );
    let buttons: Vec<_> =
        ElementHandle::find_by_accessible_label(&window, "Toggle sidebar").collect();
    assert_eq!(buttons.len(), 1);
}

#[test]
fn only_harlos_own_themes_keep_the_logos_colours() {
    i_slint_backend_testing::init_no_event_loop();
    let window = ui::AppWindow::new().expect("the window opens");
    let tokens = theme::default_tokens();
    theme::apply(&window, &tokens, harlo_core::theme::HARLO_DARK);
    assert!(
        window.global::<ui::Theme>().get_own_logo(),
        "Harlo's colours"
    );
    theme::apply(&window, &tokens, harlo_core::theme::HARLO_LIGHT);
    assert!(
        window.global::<ui::Theme>().get_own_logo(),
        "and the light one"
    );
    theme::apply(&window, &tokens, "dracula");
    assert!(
        !window.global::<ui::Theme>().get_own_logo(),
        "tinted with the theme's brand"
    );
    assert_eq!(
        window.global::<ui::Theme>().get_surface_brand_hover(),
        theme::color(tokens.surface_brand_hover),
        "the accent colours reach the window"
    );
}
