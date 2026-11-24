//! Harlo's server is the one on this computer: the picker offers it, a
//! computer added in Settings brings its own, and Settings → Server offers
//! to bring the server's old sessions over.
//!
//! Nothing here connects: the app is built without following a server.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, Message, Section, SheetEvent, imported_text};
use harlo::options::Options;
use harlo_core::import::Imported;
use harlo_core::layout::Preferences;
use harlo_core::server::Location;
use support::{demo, ui};

#[test]
fn harlos_server_is_offered_with_each_computer_and_opencodes_sessions_import() {
    let options = Options::parse(Vec::<String>::new(), None).expect("no arguments");
    assert_eq!(options.locations(), [Location::Local]);

    // A computer added in Settings brings its server.
    let app = App::new(Location::Local, options.locations(), demo::layout()).with_preferences(
        Preferences {
            computers: vec!["buildbox".into()],
            ..Preferences::default()
        },
    );
    assert_eq!(
        app.locations,
        [
            Location::Local,
            Location::Ssh {
                target: "buildbox".into()
            }
        ]
    );

    let mut app = demo::new_app();
    app.location = Location::Local;
    app.locations = vec![Location::Local];
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::Server)));
    assert!(!ui.shows("Open on"));
    assert!(ui.shows("Harlo's own server. It keeps running when Harlo closes."));
    // The server's sessions can be brought over. Never pressed here: that
    // reaches the servers on this computer.
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
    app.location = Location::Local;
    app.status = harlo::app::Status::Connected {
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::General)));
    assert!(!ui.shows("Chosen by OpenCode"));
    assert!(
        ui.shows("Chosen by Harlo's server"),
        "a model left to the server is the server's choice"
    );
}
