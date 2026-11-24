//! The background service: `service start` starts one and records where
//! it is, a second start finds the same one, and stopping it removes the
//! record.
//!
//! Everything runs under a scratch home; the real service is never seen.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Runs `harlo-server service <what>` with `home` as its home.
fn service(home: &Path, what: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_harlo-server"))
        .args(["service", what])
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("harlo-server runs")
}

/// The registration under `home`, if any.
fn registration(home: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(home.join("state/harlo/service.json")).ok()?;
    serde_json::from_str(&text).ok()
}

#[test]
fn the_service_starts_once_and_unregisters_when_stopped() {
    let home = tempfile::tempdir().expect("scratch");
    let home = home.path();
    assert!(
        !service(home, "status").status.success(),
        "nothing runs yet"
    );

    let started = service(home, "start");
    assert!(started.status.success(), "{started:?}");
    let first = registration(home).expect("registered");
    let pid = first["pid"].as_u64().expect("pid");
    let password = first["password"].as_str().expect("a password");
    assert_eq!(password.len(), 48);
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(home.join("state/harlo/service.json"))
            .expect("file")
            .permissions(),
    );
    assert_eq!(mode & 0o777, 0o600, "only its owner reads the password");

    let again = service(home, "start");
    assert!(again.status.success());
    assert_eq!(
        registration(home).expect("registered")["pid"],
        pid,
        "the same one"
    );
    assert!(service(home, "status").status.success());

    Command::new("kill")
        .arg(pid.to_string())
        .status()
        .expect("kill");
    for _ in 0..100 {
        if registration(home).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(registration(home).is_none(), "the record goes with it");
}
