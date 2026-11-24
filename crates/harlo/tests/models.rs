//! Settings → Models: which models the model menus offer. Recent ones show
//! until the user chooses; a switch shows or hides each, and the menus
//! follow.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::{Duration, UNIX_EPOCH};

use harlo::app::{Message, Section, SheetEvent};
use harlo::model::Picker;
use harlo_core::server::{Catalog, Model};
use i_slint_backend_testing::AccessibleRole;
use support::{Ui, demo, ui};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn folder() -> String {
    demo::snapshot()
        .sessions
        .into_iter()
        .find(|s| s.id == PI_UPDATE)
        .map(|s| s.directory)
        .expect("a demo session")
}

/// Milliseconds `days` before the demo's clock.
fn days_ago(days: u64) -> i64 {
    let at = demo::now() - Duration::from_hours(24 * days);
    i64::try_from(
        at.duration_since(UNIX_EPOCH)
            .expect("after 1970")
            .as_millis(),
    )
    .expect("a date")
}

fn model(id: &str, name: &str, family: &str, days: Option<u64>) -> Model {
    Model {
        provider_id: "openai".into(),
        id: id.into(),
        name: name.into(),
        variants: Vec::new(),
        images: false,
        context: None,
        family: Some(family.into()),
        released: days.map(days_ago),
        free: false,
    }
}

/// The window with "pi update" open, its folder offering a new model, the
/// one it superseded, and an undated one.
fn window() -> Ui {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let catalog = Catalog {
        models: vec![
            model("gpt-5", "Old Model", "gpt", Some(120)),
            model("gpt-5.1", "New Model", "gpt", Some(10)),
            model("custom", "Plain Model", "custom", None),
        ],
        ..Catalog::default()
    };
    let _ = app.update(Message::Catalog(folder(), Ok(Box::new(catalog))));
    ui(app)
}

/// Whether the model menu offers `name`.
fn offered(ui: &Ui, name: &str) -> bool {
    ui.send(Message::TogglePicker(Picker::Model));
    let shown = ui
        .find(name)
        .iter()
        .any(|e| e.accessible_role() != Some(AccessibleRole::Checkbox));
    ui.send(Message::ClosePicker);
    shown
}

#[test]
fn recent_models_show_and_a_switch_shows_or_hides_each() {
    let ui = window();
    assert!(offered(&ui, "New Model"), "the newest of its family");
    assert!(offered(&ui, "Plain Model"), "undated");
    assert!(!offered(&ui, "Old Model"), "superseded");

    ui.send(Message::Sheet(SheetEvent::Open(Section::Models)));
    let switch = |name: &str| {
        ui.find(name)
            .into_iter()
            .find(|e| e.accessible_role() == Some(AccessibleRole::Checkbox))
            .unwrap_or_else(|| panic!("a switch for {name}"))
    };
    assert_eq!(switch("Old Model").accessible_checked(), Some(false));
    assert_eq!(switch("New Model").accessible_checked(), Some(true));
    switch("Old Model").invoke_accessible_default_action();
    switch("New Model").invoke_accessible_default_action();
    assert_eq!(switch("Old Model").accessible_checked(), Some(true));
    assert_eq!(
        ui.app().preferences.models.get("openai/gpt-5.1"),
        Some(&false),
        "kept as chosen"
    );

    // The search narrows the list.
    ui.fill("Search models", "plain");
    assert!(ui.shows("Plain Model"));
    assert!(!ui.shows("Old Model"));
    ui.send(Message::Sheet(SheetEvent::Close));

    assert!(offered(&ui, "Old Model"), "shown by choice");
    assert!(!offered(&ui, "New Model"), "hidden by choice");
}
