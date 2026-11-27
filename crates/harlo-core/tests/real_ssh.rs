//! Following, over SSH, a server on "another computer" that runs a real
//! model: the tunnel connects, a session is started through it, its live
//! events arrive through it, and the model's work lands in the project.
//!
//! This test starts no server and is skipped unless asked for: it is run by
//! `tools/real/features.py` (the case `ssh`), which starts a private
//! server on a scratch sign-in and names it here through `HARLO_REAL_URL`,
//! `HARLO_REAL_PASSWORD` and `HARLO_REAL_PROJECT`. The other computer is a
//! private `sshd` on a loopback port with scratch keys and folders; the
//! client never reads the user's `~/.ssh` or agent.

// Rust guideline compliant 2026-10-09

mod remote;

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use harlo_client::{Delivery, Prompt};
use harlo_core::server::{Location, Update, follow};
use remote::Remote;

/// How long a real model may take over the one turn asked of it.
const PATIENCE: Duration = Duration::from_secs(240);

fn named(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} isn't set: run tools/real/features.py"))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a server on a real model; tools/real/features.py runs it"]
async fn a_real_models_session_is_run_and_followed_through_a_tunnel() {
    let url = named("HARLO_REAL_URL");
    let project = std::path::PathBuf::from(named("HARLO_REAL_PROJECT"));
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let remote = Remote::start(scratch.path()).expect("this system has an sshd");
    remote.register(&url, &named("HARLO_REAL_PASSWORD"));

    let mut updates = follow(Location::Ssh {
        target: remote.target.clone(),
    });
    let started = Instant::now();
    let connected = loop {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "never connected"
        );
        match tokio::time::timeout(Duration::from_secs(60), updates.next()).await {
            Ok(Some(Update::Connected(connected))) => break connected,
            Ok(Some(Update::Lost { reason, .. })) => panic!("lost: {reason}"),
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => panic!("the connection gave nothing"),
        }
    };
    let client = connected.client.clone();
    let through = client.endpoint().url().to_owned();
    println!(
        "SSH connected to {} {} at {through}",
        connected.server, connected.version
    );
    assert!(through.starts_with("http://127.0.0.1:"), "{through}");
    assert_ne!(through, url, "through the tunnel's own port");

    // A session started and prompted through the tunnel.
    let folder = project.to_str().expect("utf-8");
    let session = client.create_session(folder).await.expect("a session");
    let asked = "Create a file named tunnel.txt in this folder holding the single word through. \
                 Then reply with exactly: TUNNEL-OK";
    client
        .prompt(&session.id, &Prompt::new(asked, Delivery::Queue))
        .await
        .expect("prompted");

    // Its live events come through the tunnel while the model works.
    let mut events = 0_usize;
    let started = Instant::now();
    loop {
        assert!(started.elapsed() < PATIENCE, "the run never ended");
        match tokio::time::timeout(Duration::from_secs(2), updates.next()).await {
            Ok(Some(Update::Event(_))) => events += 1,
            Ok(Some(Update::Lost { reason, .. })) => {
                panic!("lost while the model worked: {reason}")
            }
            Ok(Some(_)) | Err(_) => {}
            Ok(None) => panic!("the connection ended"),
        }
        let busy = client.active().await.expect("the active sessions");
        if events > 0 && !busy.contains(&session.id) && project.join("tunnel.txt").exists() {
            break;
        }
    }
    let messages = client.messages(&session.id).await.expect("the messages");
    let read = format!("{messages:?}");
    println!(
        "SSH events through the tunnel: {events}; messages: {}",
        messages.len()
    );
    assert!(read.contains("TUNNEL-OK"), "{read}");
    let wrote = std::fs::read_to_string(project.join("tunnel.txt")).expect("the file");
    assert_eq!(wrote.trim(), "through");
}
