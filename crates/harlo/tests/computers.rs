//! Settings → Server's other computers: adding one follows OpenCode's
//! shared service there over SSH, and removing it goes back. The "other
//! computer" is a private `sshd` on a loopback port (scratch keys, configs
//! and `HOME`), and its service a private `opencode serve`; nothing reaches
//! the user's `~/.ssh`, agent or shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-core/tests/remote/mod.rs"]
mod remote;
#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::time::Duration;

use harlo::app::{Message, Section, SheetEvent, Status};
use harlo_core::server::Location;
use remote::Remote;
use server::Sandbox;
use support::tasks::Tasks;
use support::{demo, ui};

/// The field for a computer, by its hint.
const HINT: &str = "me@host, or a name from ~/.ssh/config";

/// How long connecting may take.
const WAIT: Duration = Duration::from_secs(60);

#[test]
fn a_computer_is_added_followed_over_ssh_and_removed() {
    let mut tasks = Tasks::new();
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let Some(remote) = Remote::start(scratch.path()) else {
        return;
    };
    let sandbox = tasks.block_on(Sandbox::start());
    remote.register(&sandbox.url, &sandbox.password);

    // Started with the sandbox itself, never the shared service here.
    let here = Location::Server {
        url: sandbox.url.clone(),
        password: Some(sandbox.password.clone()),
    };
    let mut app = demo::new_app();
    app.location = here.clone();
    app.locations = vec![here.clone()];
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::Server)));
    assert!(ui.shows("Other computers"));

    ui.fill(HINT, "\"unclosed");
    ui.press("Add and follow Computer");
    assert!(ui.shows("Not added"), "a bad target is refused");
    assert!(ui.app().preferences.computers.is_empty());

    ui.fill(HINT, &remote.target);
    ui.press("Add and follow Computer");
    let there = Location::Ssh {
        target: remote.target.clone(),
    };
    assert_eq!(ui.app().location, there);
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            ui.app().status,
            Status::Connected { .. }
        )),
        "connected over SSH: {:?}",
        ui.app().status
    );
    assert_eq!(
        ui.app().preferences.computers,
        std::slice::from_ref(&remote.target)
    );
    assert!(ui.app().locations.contains(&there), "the picker offers it");
    assert!(ui.shows("Following"));
    assert!(ui.shows(&format!("ssh {}", remote.target)), "the address");

    ui.press("Remove");
    assert_eq!(ui.app().location, here, "back to the first location");
    assert!(ui.app().preferences.computers.is_empty());
    assert!(!ui.app().locations.contains(&there));
}
