//! Settings → Server's other computers: adding one follows Harlo's server
//! there over SSH, and removing it goes back. The "other computer" is a
//! private `sshd` on a loopback port (scratch keys, configs and `HOME`),
//! and its server a private `harlo-server serve`; nothing reaches the
//! user's `~/.ssh`, agent or background service.

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
const HINT: &str = "Computer";

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
    assert!(
        !ui.app()
            .locations
            .iter()
            .any(|l| matches!(l, Location::Ssh { .. })),
        "both of its servers go"
    );
}

#[test]
fn the_ssh_configs_hosts_are_offered_and_one_click_follows() {
    let scratch = tempfile::tempdir().expect("a scratch home");
    let ssh = scratch.path().join(".ssh");
    std::fs::create_dir_all(&ssh).expect("the folder");
    std::fs::write(
        ssh.join("config"),
        "Host buildbox\n    User me\nHost nano\nHost *\n    ServerAliveInterval 30\n",
    )
    .expect("a scratch config");
    let mut app = demo::new_app();
    app.ssh_config = Some(ssh.join("config"));
    app.preferences.computers = vec!["buildbox".into()];
    let ui = ui(app);
    let mut tasks = Tasks::new();
    ui.send(Message::Sheet(SheetEvent::Open(Section::Server)));
    assert!(
        tasks.until(&ui, Duration::from_secs(5), |ui| ui.shows("nano")),
        "the config's hosts"
    );
    assert!(!ui.shows("Follow: buildbox"), "added already");
    assert!(!ui.shows("*"), "patterns name no computer");

    ui.press("Follow: nano");
    // Following would run `ssh`; the test stops short of it.
    drop(ui.driver.take_tasks());
    assert_eq!(ui.app().preferences.computers, ["buildbox", "nano"]);
    assert_eq!(
        ui.app().location,
        Location::Ssh {
            target: "nano".into(),
        },
        "Harlo's server there, as chosen"
    );
    assert!(!ui.shows("Follow: nano"));
}
