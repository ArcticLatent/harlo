//! Settings → Server's "Import all" reaches the import and shows what came
//! of it.
//!
//! This binary runs with a scratch home and state folder, no desktop bus,
//! an empty `PATH` and no `harlo-server`, set before anything starts: the
//! import finds neither server on "this computer", so it can only say so;
//! the real servers are never reached or started.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo::app::{Message, Section, SheetEvent};
use harlo_core::server::Location;
use support::tasks::Tasks;
use support::{demo, ui};

#[test]
#[expect(
    clippy::semicolon_outside_block,
    reason = "the unsafe block sets several variables, each its own statement"
)]
fn import_all_says_why_nothing_came_over() {
    let home = tempfile::tempdir().expect("a scratch home");
    let root = home.path();
    std::fs::create_dir_all(root.join("bin")).expect("an empty bin");
    // SAFETY: this is the binary's only test, and it sets these before
    // starting anything that reads them.
    unsafe {
        std::env::set_var("HOME", root);
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_STATE_HOME", root.join("state"));
        std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent");
        std::env::set_var("PATH", root.join("bin"));
        std::env::set_var("HARLO_SERVER_BIN", root.join("bin/harlo-server"));
    }
    let mut app = demo::new_app();
    app.location = Location::Engine;
    let ui = ui(app);
    let mut tasks = Tasks::new();
    ui.send(Message::Sheet(SheetEvent::Open(Section::Server)));
    ui.press("Import all");
    assert!(
        tasks.until(&ui, Duration::from_secs(20), |ui| ui
            .app()
            .sheets
            .import
            .as_deref()
            .is_some_and(|s| s.starts_with("Not imported"))),
        "{:?}",
        ui.app().sheets.import
    );
    let said = ui.app().sheets.import.clone().unwrap_or_default();
    assert!(said.contains("OpenCode isn't installed"), "{said}");
    assert!(ui.shows(&said));
}
