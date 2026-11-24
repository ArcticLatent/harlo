//! Two servers on this computer: Harlo's and OpenCode's are both in the
//! picker, so sessions on either stay reachable, and Settings → Server
//! only chooses which one Harlo opens on.
//!
//! Nothing here connects: the app is built without following a server.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, Message, Section, SettingChoice, SheetEvent, imported_text};
use harlo::options::Options;
use harlo_core::import::Imported;
use harlo_core::layout::{Backend, Preferences};
use harlo_core::server::Location;
use support::{demo, ui};

#[test]
fn both_servers_are_offered_and_settings_choose_where_harlo_opens() {
    let options = Options::parse(Vec::<String>::new(), None).expect("no arguments");
    assert_eq!(options.locations(), [Location::Engine, Location::Shared]);
    assert_eq!(
        Preferences::default().backend,
        Backend::Harlo,
        "Harlo's by default"
    );
    assert_eq!(Location::here(Backend::OpenCode), Location::Shared);

    // A computer added in Settings brings both of its servers.
    let app = App::new(Location::Engine, options.locations(), demo::layout()).with_preferences(
        Preferences {
            computers: vec!["buildbox".into()],
            ..Preferences::default()
        },
    );
    assert_eq!(app.locations.len(), 4);
    assert!(app.locations.contains(&Location::Ssh {
        target: "buildbox".into(),
        backend: Backend::OpenCode,
    }));

    let mut app = demo::new_app();
    app.location = Location::Engine;
    app.locations = vec![Location::Engine, Location::Shared];
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::Server)));
    assert!(ui.shows("Open on"));
    assert!(ui.shows("Harlo's own server. It keeps running when Harlo closes."));
    ui.send(Message::Sheet(SheetEvent::Choose(
        SettingChoice::Backend,
        1,
    )));
    assert_eq!(ui.app().preferences.backend, Backend::OpenCode);
    assert_eq!(ui.app().location, Location::Engine, "the open server stays");
    assert!(
        ui.app().locations.contains(&Location::Engine),
        "its sessions stay reachable"
    );
    // On Harlo's server, OpenCode's sessions can be brought over. Never
    // pressed here: that reaches the servers on this computer.
    assert!(ui.shows("Sessions made on OpenCode"));
    assert!(ui.shows("Import all"));
}

#[test]
fn an_import_says_what_it_did() {
    let said = |copied, already, failed: &[&str]| {
        imported_text(&Ok(Imported {
            copied,
            already,
            failed: failed.iter().map(|f| (*f).to_owned()).collect(),
        }))
    };
    assert_eq!(said(0, 0, &[]), "Nothing new to bring over.");
    assert_eq!(said(1, 0, &[]), "Brought 1 session over.");
    assert_eq!(
        said(3, 2, &["ses_x: gone"]),
        "Brought 3 sessions over; 2 already here; 1 didn't come over (ses_x: gone)."
    );
    assert_eq!(
        imported_text(&Err("OpenCode isn't running".into())),
        "Not imported: OpenCode isn't running"
    );
}

#[test]
fn settings_name_the_server_harlo_is_on() {
    let mut app = demo::new_app();
    app.location = Location::Engine;
    app.status = harlo::app::Status::Connected {
        server: "Harlo".into(),
        version: "0.1.0".into(),
    };
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::General)));
    assert!(!ui.shows("Chosen by OpenCode"));
    assert!(
        ui.shows("Chosen by Harlo's server"),
        "a model left to the server is the server's choice"
    );
}
