//! Harlo's own updates in Settings → General: never looked for when built
//! from source; for a package, a new release found, downloaded, installed
//! and restarted into, or why not; and the server left on the old version.
//!
//! Nothing here reaches the network: the window's tasks don't run, and
//! what they would report is sent as messages.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::Arc;

use harlo::app::{
    App, Message, Section, ServerRestart, Setting, SheetEvent, UpdateEvent, UpdatePhase, VERSION,
};
use harlo_core::download::Download;
use harlo_core::server::{Client, Connected, Endpoint, Update};
use harlo_core::updates::{Asset, InstallError, InstallKind, Release};
use support::{Ui, demo, ui};

/// A release far newer than this Harlo.
const NEXT: &str = "99.0.0";

fn release(version: &str) -> Release {
    Release {
        version: version.into(),
        assets: vec![Asset {
            name: format!("harlo_{version}-1_amd64.deb"),
            sha256: "ab".repeat(32),
            download_url: format!("http://127.0.0.1:9/harlo_{version}-1_amd64.deb"),
        }],
    }
}

/// The demo installed from a Debian package, Settings open on General.
fn packaged() -> Ui {
    let mut app = demo::app();
    app.updates.kind = Some(InstallKind::Deb);
    settings(app)
}

/// `app` with Settings open on General.
fn settings(app: App) -> Ui {
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::General)));
    ui
}

/// The update event the last button pressed sent.
fn pressed(ui: &Ui) -> Option<UpdateEvent> {
    ui.messages().into_iter().rev().find_map(|m| match m {
        Message::Update(event) => Some(event),
        _ => None,
    })
}

fn update(ui: &Ui, event: UpdateEvent) {
    ui.send(Message::Update(event));
}

#[test]
fn a_harlo_built_from_source_never_looks() {
    let ui = settings(demo::app());
    assert!(ui.shows("Built from source, so it updates with its code."));
    assert!(!ui.shows("Check for updates"));
    update(&ui, UpdateEvent::Check { asked: false });
    update(&ui, UpdateEvent::Check { asked: true });
    assert_eq!(ui.app().updates.phase, UpdatePhase::Idle);
}

#[test]
fn a_new_release_is_offered_once_and_installed() {
    let ui = packaged();
    assert!(ui.shows("Check for updates"));
    ui.press("Check now");
    assert_eq!(pressed(&ui), Some(UpdateEvent::Check { asked: true }));
    update(&ui, UpdateEvent::Check { asked: true });
    assert!(ui.shows("Checking…"));

    update(&ui, UpdateEvent::Checked(Ok(release(NEXT))));
    assert!(ui.shows(&format!("Harlo {NEXT} is out")));
    assert!(ui.shows(&format!("You have {VERSION}.")));
    assert!(ui.shows("What's new"));
    // Announced once: the version is remembered.
    assert_eq!(ui.app().preferences.update_announced, NEXT);
    update(
        &ui,
        UpdateEvent::Notes {
            version: NEXT.into(),
            text: "• Faster.\n• Calmer.".into(),
        },
    );
    assert!(ui.shows("• Faster.\n• Calmer."));

    ui.press("Update");
    assert_eq!(pressed(&ui), Some(UpdateEvent::Install));
    update(&ui, UpdateEvent::Install);
    assert!(ui.shows("Downloading the update… 0%"));
    update(&ui, UpdateEvent::Downloaded(Download::Progress(0.42)));
    assert!(ui.shows("Downloading the update… 42%"));
    update(&ui, UpdateEvent::Installed(Ok(())));
    assert!(ui.shows(&format!("Harlo {NEXT} is installed")));
    assert!(ui.shows("Restart Harlo to use it."));
    let _ = ui.driver.take_tasks();
    ui.press("Restart");
    assert_eq!(pressed(&ui), Some(UpdateEvent::Restart));
    // Starting the new Harlo is left to a task, which this window doesn't
    // run: pressing the button here starts no program.
    assert_eq!(ui.driver.take_tasks().len(), 1);
    // While it waits to restart, a timed look doesn't start over.
    update(&ui, UpdateEvent::Check { asked: false });
    assert!(matches!(ui.app().updates.phase, UpdatePhase::Installed(_)));
}

#[test]
fn a_harlo_that_cant_be_started_again_says_why() {
    let ui = packaged();
    update(&ui, UpdateEvent::Installed(Ok(())));
    update(
        &ui,
        UpdateEvent::Relaunching(Err("No such file or directory".into())),
    );
    assert!(matches!(ui.app().updates.phase, UpdatePhase::Failed(_)));
    assert!(ui.shows("Couldn't start the new Harlo: No such file or directory"));
}

#[test]
fn a_failed_install_says_why_and_how_to_do_it_by_hand() {
    let ui = packaged();
    update(&ui, UpdateEvent::Checked(Ok(release(NEXT))));
    update(&ui, UpdateEvent::Install);
    let command = "sudo apt-get install -y --allow-downgrades \"/c/harlo.deb\"";
    update(
        &ui,
        UpdateEvent::Installed(Err(InstallError {
            message: "pkexec didn't install it: Not authorized".into(),
            manual: Some(command.into()),
        })),
    );
    assert!(ui.shows("Not updated"));
    assert!(ui.shows("pkexec didn't install it: Not authorized"));
    assert!(ui.shows("Install it by hand"));
    assert!(ui.shows(command));
    assert!(ui.button("Copy").accessible_enabled().unwrap_or(true));
    ui.press("Try again");
    assert_eq!(pressed(&ui), Some(UpdateEvent::Check { asked: true }));
}

#[test]
fn nothing_new_and_failed_looks() {
    let ui = packaged();
    // An older or the same release: this one is the newest.
    update(&ui, UpdateEvent::Check { asked: true });
    update(&ui, UpdateEvent::Checked(Ok(release("0.0.1"))));
    assert!(ui.shows("This is the newest version."));
    assert!(ui.app().preferences.update_announced.is_empty());
    // A timed look that fails stays quiet; one asked for says why.
    update(&ui, UpdateEvent::Check { asked: false });
    update(&ui, UpdateEvent::Checked(Err("offline".into())));
    assert_eq!(ui.app().updates.phase, UpdatePhase::Idle);
    update(&ui, UpdateEvent::Check { asked: true });
    update(
        &ui,
        UpdateEvent::Checked(Err("Couldn't reach Harlo's release manifest".into())),
    );
    assert!(ui.shows("Couldn't reach Harlo's release manifest"));
}

#[test]
fn the_switch_stops_timed_looks_but_not_check_now() {
    let ui = packaged();
    ui.send(Message::Sheet(SheetEvent::Switch(Setting::CheckUpdates)));
    assert!(!ui.app().preferences.check_updates);
    update(&ui, UpdateEvent::Check { asked: false });
    assert_eq!(ui.app().updates.phase, UpdatePhase::Idle);
    update(&ui, UpdateEvent::Check { asked: true });
    assert_eq!(ui.app().updates.phase, UpdatePhase::Checking);
}

#[test]
fn a_server_left_on_the_old_version_can_be_restarted() {
    let ui = packaged();
    assert!(!ui.shows("Harlo's server"));
    ui.send(Message::Server(Update::Connected(Arc::new(Connected {
        client: Client::new(Endpoint::new("http://127.0.0.1:9", None)),
        server: "Harlo".into(),
        version: "0.0.1".into(),
        snapshot: demo::snapshot(),
    }))));
    assert!(ui.shows("Still runs 0.0.1. Restarting it stops the sessions it's running."));
    ui.press("Restart server");
    assert_eq!(pressed(&ui), Some(UpdateEvent::RestartServer));
    update(&ui, UpdateEvent::RestartServer);
    assert!(ui.shows("Restarting…"));
    update(
        &ui,
        UpdateEvent::ServerRestarted(Err("The server didn't stop in time.".into())),
    );
    assert_eq!(
        ui.app().updates.server,
        ServerRestart::Failed("The server didn't stop in time.".into())
    );
    assert!(ui.shows("Not restarted"));
}
