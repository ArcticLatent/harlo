//! Following OpenCode on another computer over SSH, against a private
//! `sshd` on a loopback port. Its sessions get a scratch `HOME` and state
//! folder, no desktop bus, and a stand-in `opencode`; the client uses its
//! own config, key and known hosts, never the user's `~/.ssh` or agent. The
//! "other computer's" service is a private `opencode serve`.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod support;

mod remote;

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use harlo_core::server::{Location, Update, follow};
use harlo_core::ssh;
use remote::{Remote, free_port, registration};
use support::Sandbox;

/// Follows `location` until it connects or is lost; gives the connected
/// server's URL, or why it was lost.
async fn connect(location: Location) -> Result<String, String> {
    let mut updates = follow(location);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        match tokio::time::timeout(Duration::from_secs(60), updates.next()).await {
            Ok(Some(Update::Connected(connected))) => {
                return Ok(connected.client.endpoint().url().to_owned());
            }
            Ok(Some(Update::Lost { reason, .. })) => return Err(reason),
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
    Err("nothing happened".to_owned())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_registered_service_is_followed_through_a_tunnel() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let Some(remote) = Remote::start(scratch.path()) else {
        return;
    };
    let sandbox = Sandbox::start().await;
    assert!(
        ssh::registered(&remote.target)
            .await
            .expect("reached")
            .is_none(),
        "nothing registered yet"
    );
    remote.register(&sandbox.url, &sandbox.password);
    let service = ssh::registered(&remote.target)
        .await
        .expect("reached")
        .expect("registered");
    assert_eq!(service.endpoint.url(), sandbox.url);

    let url = connect(Location::Ssh {
        target: remote.target.clone(),
    })
    .await
    .expect("connected");
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    assert_ne!(url, sandbox.url, "through the tunnel's own port");
    assert_eq!(
        Location::Ssh {
            target: remote.target.clone()
        }
        .label(),
        "harlo-remote"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_service_is_started_there_when_none_runs() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let Some(remote) = Remote::start(scratch.path()) else {
        return;
    };
    let sandbox = Sandbox::start().await;
    // The stand-in registers the sandbox, as `opencode service start` would
    // register the service it starts.
    let stand_in = format!(
        "#!/bin/sh\n[ \"$1 $2\" = \"service start\" ] || exit 2\n\
         mkdir -p \"$XDG_STATE_HOME/opencode\"\n\
         printf '%s' '{}' > \"$XDG_STATE_HOME/opencode/service.json\"\n",
        registration(&sandbox.url, &sandbox.password)
    );
    let program = remote.root.join("bin/opencode");
    std::fs::write(&program, stand_in).expect("the stand-in");
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&program)
        .status()
        .expect("chmod");

    connect(Location::Ssh {
        target: remote.target.clone(),
    })
    .await
    .expect("connected");
    assert!(remote.root.join("state/opencode/service.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_computer_says_why() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let Some(remote) = Remote::start(scratch.path()) else {
        return;
    };
    let closed = free_port();
    let why = connect(Location::Ssh {
        target: format!("-p {closed} {}", remote.target),
    })
    .await
    .expect_err("not reached");
    assert!(
        why.contains("Couldn't reach harlo-remote over SSH"),
        "{why}"
    );
}
