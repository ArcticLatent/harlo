//! Following a real OpenCode server: connect, snapshot, live events, fetches.
//!
//! Runs against a private `opencode serve` with scratch `HOME`/`XDG_*` folders,
//! never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod support;

use std::time::{Duration, SystemTime};

use futures_util::StreamExt;
use harlo_core::server::{self, Location, Update};
use harlo_core::workspace::Workspace;
use opencode_client::EventKind;
use support::{Sandbox, WAIT};

/// Waits for the next update that `pick` accepts.
async fn next<T>(updates: &mut server::Updates, mut pick: impl FnMut(Update) -> Option<T>) -> T {
    tokio::time::timeout(WAIT, async {
        loop {
            let update = updates.next().await.expect("the stream keeps going");
            if let Some(found) = pick(update) {
                return found;
            }
        }
    })
    .await
    .expect("the expected update arrives in time")
}

#[tokio::test(flavor = "multi_thread")]
async fn follow_loads_a_snapshot_and_applies_live_sessions() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let before = client
        .create_session(&sandbox.project_str())
        .await
        .expect("create a session");

    let mut updates = server::follow(Location::Server {
        url: sandbox.url.clone(),
        password: Some(sandbox.password.clone()),
    });
    let connected = next(&mut updates, |u| match u {
        Update::Connected(c) => Some(c),
        _ => None,
    })
    .await;
    assert_eq!(connected.version, opencode_client::OPENCODE_VERSION);

    let mut ws = Workspace::new(Vec::new());
    ws.resync(connected.snapshot.clone());
    let sidebar = ws.sidebar(SystemTime::now());
    assert_eq!(sidebar.len(), 1, "{sidebar:?}");
    assert_eq!(sidebar[0].name, "project");
    assert_eq!(sidebar[0].sessions[0].id, before.id);
    assert!(
        sidebar[0].worktrees.is_empty(),
        "the main checkout isn't a worktree"
    );

    // Another client makes a session; the event names it and a fetch completes it.
    let after = client
        .create_session(&sandbox.project_str())
        .await
        .expect("create a second session");
    let fetches = next(&mut updates, |u| match u {
        Update::Event(e) => match &e.kind {
            EventKind::SessionCreated { session_id } if *session_id == after.id => {
                Some(ws.apply(&e.kind, e.time))
            }
            _ => None,
        },
        _ => None,
    })
    .await;
    for fetch in fetches {
        let result = server::fetch(&connected.client, fetch)
            .await
            .expect("fetch");
        let _ = ws.fetched(result);
    }
    let ids: Vec<String> = ws.sidebar(SystemTime::now())[0]
        .sessions
        .iter()
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(ids, [after.id.clone(), before.id.clone()], "newest first");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_password_is_reported_and_retried() {
    let sandbox = Sandbox::start().await;
    let mut updates = server::follow(Location::Server {
        url: sandbox.url.clone(),
        password: Some("wrong".to_owned()),
    });
    let (reason, retry_in) = next(&mut updates, |u| match u {
        Update::Lost { reason, retry_in } => Some((reason, retry_in)),
        _ => None,
    })
    .await;
    assert_eq!(reason, "the server rejected the password");
    assert_eq!(retry_in, Duration::from_secs(1));
    let attempt = next(&mut updates, |u| match u {
        Update::Connecting { attempt } => Some(attempt),
        _ => None,
    })
    .await;
    assert_eq!(attempt, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_server_reconnects_with_a_fresh_snapshot() {
    let sandbox = Sandbox::start().await;
    let location = Location::Server {
        url: sandbox.url.clone(),
        password: Some(sandbox.password.clone()),
    };
    let mut updates = server::follow(location);
    let () = next(&mut updates, |u| {
        matches!(u, Update::Connected(_)).then_some(())
    })
    .await;

    // Stop the server under the connection, then start it again on the same port.
    let mut sandbox = sandbox;
    sandbox.restart_server().await;
    let reason = next(&mut updates, |u| match u {
        Update::Lost { reason, .. } => Some(reason),
        _ => None,
    })
    .await;
    assert!(!reason.is_empty());
    let created = sandbox
        .client()
        .create_session(&sandbox.project_str())
        .await
        .expect("create a session while Harlo is away");
    let connected = next(&mut updates, |u| match u {
        Update::Connected(c) => Some(c),
        _ => None,
    })
    .await;
    assert!(
        connected
            .snapshot
            .sessions
            .iter()
            .any(|s| s.id == created.id),
        "the session made while disconnected arrives with the resync"
    );
}

#[test]
fn location_debug_hides_the_password() {
    let location = Location::Server {
        url: "http://127.0.0.1:1".into(),
        password: Some("s3cret".into()),
    };
    let shown = format!("{location:?}");
    assert!(!shown.contains("s3cret"), "{shown}");
    assert_eq!(location.label(), "127.0.0.1:1");
    assert_eq!(Location::Engine.label(), "Local");
    assert_eq!(Location::Shared.label(), "Local · OpenCode");
}
