//! Which servers run on this computer, so their folders can be used directly.

// Rust guideline compliant 2026-10-09

use harlo_core::server::Location;

fn at(url: &str) -> Location {
    Location::Server {
        url: url.into(),
        password: None,
    }
}

#[test]
fn loopback_addresses_are_local_and_others_are_not() {
    assert!(Location::Local.is_local());
    for url in [
        "http://127.0.0.1:4096",
        "http://localhost:4096/",
        "http://[::1]:4096",
        "127.0.0.2:80",
    ] {
        assert!(at(url).is_local(), "{url}");
    }
    for url in [
        "http://192.168.1.20:4096",
        "https://box.example.com",
        "http://localhost.example.com:4096",
        "http://[2001:db8::1]:4096",
    ] {
        assert!(!at(url).is_local(), "{url}");
    }
}
