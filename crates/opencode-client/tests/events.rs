//! Server events decoded as Harlo uses them, from the JSON the server sends.

// Rust guideline compliant 2026-10-09

use opencode_client::{Event, EventKind};

#[test]
fn a_branch_change_names_its_folder_and_branch() {
    let event = Event::parse(
        r#"{"id":"evt_1","created":1,"type":"vcs.branch.updated",
            "location":{"directory":"/work/fix-tabs"},"data":{"branch":"fix-tabs"}}"#,
    )
    .expect("an event");
    assert!(matches!(
        event.kind,
        EventKind::BranchChanged { ref directory, branch: Some(ref branch) }
            if directory == "/work/fix-tabs" && branch == "fix-tabs"
    ));
    // Off any branch, such as a detached checkout.
    let event = Event::parse(
        r#"{"id":"evt_2","created":1,"type":"vcs.branch.updated",
            "location":{"directory":"/work/fix-tabs"},"data":{}}"#,
    )
    .expect("an event");
    assert!(matches!(
        event.kind,
        EventKind::BranchChanged { branch: None, .. }
    ));
}
