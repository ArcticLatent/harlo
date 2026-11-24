//! Loading a folder's catalog from the server.
//!
//! Each test runs its own private `harlo-server` with a scratch home and a
//! scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod support;

use harlo_core::server::catalog;
use support::Sandbox;

#[tokio::test(flavor = "multi_thread")]
async fn the_first_catalog_for_a_new_folder_has_its_agents() {
    let sandbox = Sandbox::start().await;
    // A folder the server hasn't seen: this request also starts its instance.
    // (The server runs in the project folder, whose instance is already up.)
    let fresh = sandbox.root().join("fresh");
    std::fs::create_dir_all(&fresh).expect("a new folder");
    let ok = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&fresh)
        .status()
        .expect("git")
        .success();
    assert!(ok, "git init failed");
    let catalog = catalog(&sandbox.client(), &fresh.to_string_lossy())
        .await
        .expect("the catalog");
    assert!(
        catalog.primary_agents().any(|a| a.id == "build"),
        "agents: {:?}",
        catalog.agents.iter().map(|a| &a.id).collect::<Vec<_>>()
    );
    assert!(!catalog.models.is_empty());
}
