//! A staged undo's restores, and the user's edits they would overwrite.

// Rust guideline compliant 2026-10-09

use harlo_client::{FileDiff, Revert};
use harlo_core::undo::restores;

fn file(name: &str, patch: &str) -> FileDiff {
    FileDiff {
        file: name.into(),
        patch: patch.into(),
        additions: 0,
        deletions: 0,
        status: "modified".into(),
    }
}

/// The agent changed `two` to `THREE`; the user then added a line.
fn staged() -> Revert {
    Revert {
        message_id: "msg_1".into(),
        files: vec![file(
            "u.txt",
            "@@ -1,3 +1,2 @@\n one\n-THREE\n-user line\n+two\n",
        )],
    }
}

#[test]
fn an_undo_names_the_users_own_lines_it_overwrites() {
    let agent = [file("u.txt", "@@ -1,2 +1,2 @@\n one\n-two\n+THREE\n")];
    let shown = restores(&staged(), Some(&agent));
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].lost, Some(1), "only the user's line is lost");
}

#[test]
fn an_undo_of_only_the_agents_changes_loses_nothing() {
    let revert = Revert {
        message_id: "msg_1".into(),
        files: vec![file("u.txt", "@@ -1,2 +1,2 @@\n one\n-THREE\n+two\n")],
    };
    let agent = [file("u.txt", "@@ -1,2 +1,2 @@\n one\n-two\n+THREE\n")];
    assert_eq!(restores(&revert, Some(&agent))[0].lost, Some(0));
}

#[test]
fn without_the_agents_changes_nothing_can_be_said() {
    assert_eq!(restores(&staged(), None)[0].lost, None);
}

#[test]
fn a_line_the_user_changed_counts_once() {
    // The agent changed `a` to `b`; the user then changed `c` to `c!`.
    let revert = Revert {
        message_id: "msg_1".into(),
        files: vec![file("f.txt", "@@ -1,2 +1,2 @@\n-b\n-c!\n+a\n+c\n")],
    };
    let agent = [file("f.txt", "@@ -1,2 +1,2 @@\n-a\n+b\n c\n")];
    assert_eq!(restores(&revert, Some(&agent))[0].lost, Some(1));
}
