//! Harlo's server on this computer: following it starts one when none
//! runs, and finds the same one next time.
//!
//! This test binary runs under a scratch home and state folder with no
//! desktop bus, set before anything starts, so the server it starts never
//! sees the real setup. It uses `target/debug/harlo-server`, and skips when
//! that isn't built.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;
use std::time::Duration;

use futures_util::StreamExt as _;
use harlo_core::server::{Location, Update, follow};

/// The built server, if there is one.
fn built_server() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/harlo-server");
    path.is_file().then_some(path)
}

/// Follows Harlo's server until it connects; its URL and what it says it is.
async fn connect() -> (String, String) {
    let mut updates = follow(Location::Engine);
    loop {
        match tokio::time::timeout(Duration::from_secs(60), updates.next()).await {
            Ok(Some(Update::Connected(connected))) => {
                return (
                    connected.client.endpoint().url().to_owned(),
                    connected.server.clone(),
                );
            }
            Ok(Some(Update::Lost { reason, .. })) => panic!("lost: {reason}"),
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => panic!("nothing happened"),
        }
    }
}

#[test]
#[expect(
    clippy::semicolon_outside_block,
    reason = "the unsafe block sets several variables, each its own statement"
)]
fn harlos_server_is_started_once_and_found_again() {
    let Some(server) = built_server() else {
        eprintln!("skipped: build harlo-server first");
        return;
    };
    let home = tempfile::tempdir().expect("a scratch home");
    let root = home.path();
    let state = root.join("state");
    // SAFETY: this is the binary's only test, and it sets these before
    // starting the runtime or any other thread.
    unsafe {
        std::env::set_var("HOME", root);
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_STATE_HOME", &state);
        std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent");
        std::env::set_var("HARLO_SERVER_BIN", &server);
    }
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let (first, what) = runtime.block_on(connect());
    assert!(what.contains("Harlo"), "{what}");
    let registration: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(state.join("harlo/service.json")).expect("registered"),
    )
    .expect("json");
    let (again, _) = runtime.block_on(connect());
    assert_eq!(first, again, "the same server");
    let pid = registration["pid"].as_u64().expect("pid");
    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .expect("kill");
    // It's gone before its scratch home is.
    for _ in 0..100 {
        if !state.join("harlo/service.json").exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the server didn't stop");
}
