//! Harlo's server is the one on this computer: the picker offers it, a
//! computer added in Settings brings its own, and Settings → Server offers
//! to bring the server's old sessions over.
//!
//! Nothing here connects: the app is built without following a server.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, Message, Section, SheetEvent};
use harlo::options::Options;
use harlo_core::layout::Preferences;
use harlo_core::server::Location;
use support::{demo, ui};

#[test]
fn harlos_server_is_offered_with_each_computer() {
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
