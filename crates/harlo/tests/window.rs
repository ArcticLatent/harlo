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
    theme::apply(&window, &tokens);
    assert_eq!(
        window.global::<ui::Theme>().get_background_base(),
        theme::color(tokens.background_base)
    );
    let buttons: Vec<_> =
        ElementHandle::find_by_accessible_label(&window, "Toggle sidebar").collect();
    assert_eq!(buttons.len(), 1);
}
