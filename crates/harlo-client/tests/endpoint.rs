//! Finding the shared the server service and keeping its password out of logs.
//!
//! These tests read registration files written into scratch folders only.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_client::{Endpoint, opencode_registration_path, read_registration};

#[test]
fn registration_path_prefers_xdg_state_home() {
    let path = opencode_registration_path(Some(Path::new("/x/state")), Path::new("/home/u"));
    assert_eq!(path, Path::new("/x/state/opencode/service.json"));
}

#[test]
fn registration_path_falls_back_to_home() {
    let path = opencode_registration_path(None, Path::new("/home/u"));
    assert_eq!(
        path,
        Path::new("/home/u/.local/state/opencode/service.json")
    );
}

#[test]
fn missing_registration_means_no_service() {
    let dir = tempfile::tempdir().expect("scratch folder");
    let found = read_registration(&dir.path().join("service.json"))
        .expect("a missing file is not an error");
    assert!(found.is_none());
}

#[test]
fn registration_is_read_and_ignores_extra_fields() {
    let dir = tempfile::tempdir().expect("scratch folder");
    let file = dir.path().join("service.json");
    let json = r#"{"id":"svc","version":"2.3.2","url":"http://127.0.0.1:49374/","pid":42,"password":"s3cret","extra":true}"#;
    std::fs::write(&file, json).expect("write registration");

    let service = read_registration(&file).expect("read").expect("a service");
    assert_eq!(service.version.as_deref(), Some("2.3.2"));
    assert_eq!(
        service.endpoint.url(),
        "http://127.0.0.1:49374",
        "trailing slash trimmed"
    );
}

#[test]
fn malformed_registration_is_an_error() {
    let dir = tempfile::tempdir().expect("scratch folder");
    let file = dir.path().join("service.json");
    std::fs::write(&file, "{ not json").expect("write registration");
    let _error = read_registration(&file).expect_err("malformed JSON is reported");
}

#[test]
fn debug_output_hides_the_password() {
    let endpoint = Endpoint::new("http://127.0.0.1:1", Some("s3cret".to_owned()));
    let shown = format!("{endpoint:?}");
    assert!(!shown.contains("s3cret"), "{shown}");
    assert!(shown.contains("http://127.0.0.1:1"), "{shown}");
}
