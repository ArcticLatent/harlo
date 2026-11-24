//! Settings → Schedules against a private `opencode serve` with scratch
//! `HOME`/`XDG_*` folders: allowing them in the sandbox's OpenCode config,
//! making one, pausing, resuming and deleting it, and a time the server
//! can't read. Never the shared service or the real config.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use i_slint_backend_testing::AccessibleRole;

use slint::ComponentHandle as _;
use slint::platform::{Key, WindowEvent};

use harlo::app::{Load, Message, SchedulePage};
use harlo::model::Picker;
use harlo_core::server::{AutomationStatus, Connected, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// Settings → Schedules, connected to `sandbox` and writing its config.
fn schedules(sandbox: &Sandbox, tasks: &mut Tasks) -> Ui {
    let mut app = demo::new_app();
    app.opencode_folder = Some(sandbox.root().join("config/opencode"));
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);
    ui.press("Settings");
    ui.press("Schedules");
    assert!(
        tasks.until(&ui, WAIT, |ui| {
            let page = &ui.app().sheets.schedules;
            matches!(page.list, Load::Loaded(_)) && matches!(page.allowed, Load::Loaded(_))
        }),
        "the list and the switch"
    );
    ui
}

/// Presses and lets go of Escape in the window.
fn escape(ui: &Ui) {
    let text = slint::SharedString::from(Key::Escape);
    let window = ui.window.window();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

/// Fills a new schedule's form and creates it.
fn create(ui: &Ui, name: &str, when: &str, time: &str) {
    ui.press("New schedule");
    ui.fill("Name", name);
    ui.press(&format!("Runs: {when}"));
    ui.fill("Time", time);
    ui.fill("Prompt", "Check the build.");
    ui.press("Create schedule");
}

fn status(ui: &Ui) -> Option<AutomationStatus> {
    let page = &ui.app().sheets.schedules;
    let SchedulePage::Shown(id) = &page.page else {
        return None;
    };
    page.get(id).map(|a| a.status)
}

#[test]
fn a_schedule_is_allowed_made_paused_resumed_and_deleted() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = schedules(&sandbox, &mut tasks);
    // OpenCode runs automations unless its config says otherwise.
    assert_eq!(ui.app().sheets.schedules.allowed, Load::Loaded(true));
    let switch = || {
        ui.find("Allow schedules")
            .into_iter()
            .find(|e| e.accessible_role() == Some(AccessibleRole::Checkbox))
            .expect("the switch")
            .invoke_accessible_default_action();
    };
    let folder = sandbox.root().join("config/opencode");
    switch();
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().sheets.schedules.allowed
            == Load::Loaded(false)),
        "turned off"
    );
    assert!(!harlo_core::config::automations(&folder).expect("read"));
    assert!(tasks.until(&ui, WAIT, |ui| {
        ui.shows("No schedules yet. Turn on Allow schedules to add one.")
    }));
    assert!(ui.find("New schedule").is_empty(), "not allowed now");

    switch();
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().sheets.schedules.allowed
            == Load::Loaded(true)),
        "allowed"
    );
    assert!(harlo_core::config::automations(&folder).expect("read"));

    create(&ui, "Morning check", "Every", "30 minutes");
    assert_eq!(ui.app().sheets.schedules.draft.prompt, "Check the build.");
    assert!(
        tasks.until(&ui, WAIT, |ui| status(ui) == Some(AutomationStatus::Active)),
        "created: {:?}",
        ui.app().sheets.schedules.error
    );
    assert!(ui.shows("Every 30 minutes"));
    assert!(ui.shows("Check the build."));
    // Its prompt goes to a new session named after it.
    let sessions = tasks
        .block_on(sandbox.client().root_sessions())
        .expect("the sessions");
    assert!(
        sessions
            .iter()
            .any(|s| s.title.as_deref() == Some("Morning check")),
        "{sessions:?}"
    );

    // Escape closes a menu over Settings, not Settings.
    ui.press("All schedules");
    ui.press("New schedule");
    ui.press("Change");
    assert_eq!(ui.app().picker, Some(Picker::ScheduleFolder));
    escape(&ui);
    assert_eq!(ui.app().picker, None);
    // Choosing from a menu closes it.
    ui.press("Change");
    let first = harlo::model::items(&ui.app(), Picker::ScheduleFolder)
        .first()
        .map(|item| item.label.clone())
        .expect("a project");
    ui.press(&first);
    assert_eq!(ui.app().picker, None);
    assert!(ui.app().sheets.open.is_some(), "Settings stays open");
    ui.press("Cancel");
    ui.press("Morning check");

    ui.press("Pause");
    assert!(tasks.until(&ui, WAIT, |ui| status(ui) == Some(AutomationStatus::Paused)));
    assert!(ui.shows("Paused"));
    ui.press("Resume");
    assert!(tasks.until(&ui, WAIT, |ui| status(ui) == Some(AutomationStatus::Active)));

    ui.press("All schedules");
    assert!(ui.shows("Morning check"), "in the list");
    ui.press("Morning check");
    ui.press("Delete");
    assert!(ui.shows("Delete it for every app using this server?"));
    ui.press("Delete for good");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("No schedules yet.")),
        "deleted"
    );
    assert!(
        tasks
            .block_on(sandbox.client().automations())
            .expect("the list")
            .is_empty()
    );
}

#[test]
fn a_time_the_server_cant_read_says_why_and_leaves_no_session() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    harlo_core::config::set_automations(&sandbox.root().join("config/opencode"), true)
        .expect("allowed");
    let ui = schedules(&sandbox, &mut tasks);
    assert_eq!(ui.app().sheets.schedules.allowed, Load::Loaded(true));
    // Nothing filled in yet: the form says what's missing.
    ui.press("New schedule");
    ui.press("Create schedule");
    assert!(ui.shows("Give it a name."));

    ui.fill("Name", "Bad one");
    ui.fill("Time", "tomorrow-ish");
    ui.fill("Prompt", "Check the build.");
    ui.press("Create schedule");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Invalid time: tomorrow-ish")),
        "the server's reason: {:?}",
        ui.app().sheets.schedules.error
    );
    let sessions = tasks
        .block_on(sandbox.client().root_sessions())
        .expect("the sessions");
    assert!(
        !sessions
            .iter()
            .any(|s| s.title.as_deref() == Some("Bad one")),
        "the session made for it is archived: {sessions:?}"
    );
}
