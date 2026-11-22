//! The Changes panel: the session's and the folder's changes, opening a
//! diff, committing and pushing, and reloading when files change.
//!
//! Loads and git actions are fed in as their results; the git commands
//! themselves are tested in `harlo-core`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::{App, ChangesEvent, ChangesLoaded, GitAction, Message, Panel, ids};
use harlo_core::git::{FileState, Status};
use harlo_core::server::{FileDiff, Update};
use harlo_core::timeline::Entry;
use harlo_core::workspace::{Event, EventKind, ToolStatus};
use iced::widget::Id;
use support::{demo, interact, shows};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn diff(file: &str, patch: &str, additions: u64, deletions: u64) -> FileDiff {
    FileDiff {
        file: file.into(),
        patch: patch.into(),
        additions,
        deletions,
        status: "modified".into(),
    }
}

fn state(path: &str, staged: bool, unstaged: bool) -> FileState {
    FileState {
        path: path.into(),
        staged,
        unstaged,
    }
}

fn status(files: Vec<FileState>, ahead: u32) -> Status {
    Status {
        branch: Some("main".into()),
        upstream: Some("origin/main".into()),
        ahead,
        behind: 0,
        files,
        prefix: String::new(),
    }
}

/// The session's edit to `check.py`, and an untracked `notes.md`.
fn found(status: Option<Status>) -> ChangesLoaded {
    found_with(None, status)
}

/// [`found`], with the reviewed turn's changes.
fn found_with(turn: Option<Vec<FileDiff>>, status: Option<Status>) -> ChangesLoaded {
    let check = diff(
        "src/check.py",
        "@@ -1,1 +1,2 @@\n import socket\n+PORT = 9877\n",
        1,
        0,
    );
    ChangesLoaded::new(
        turn.map(Ok),
        Some(Ok(vec![check.clone()])),
        Ok(vec![
            check,
            diff("notes.md", "@@ -0,0 +1 @@\n+My notes\n", 1, 0),
        ]),
        status.map(Ok),
    )
}

/// The demo app with "pi update" open and the Changes panel showing `loaded`.
fn panel(loaded: ChangesLoaded) -> App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let _ = app.update(Message::OpenPanel(Panel::Changes));
    // Opening it starts the first load.
    let _ = app.update(Message::Changes(ChangesEvent::Loaded(1, Box::new(loaded))));
    support::settle(&mut app);
    app
}

fn event(kind: EventKind) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind,
    }))
}

#[test]
fn the_panel_lists_the_session_and_uncommitted_files() {
    let app = panel(found(Some(status(
        vec![
            state("src/check.py", true, false),
            state("notes.md", false, true),
        ],
        2,
    ))));
    assert!(shows(&app, "This session"));
    assert!(shows(&app, "Uncommitted"));
    assert!(shows(&app, "check.py"), "the file's name");
    assert!(shows(&app, "src"), "and its folder");
    assert!(shows(&app, "notes.md"));
    assert!(shows(&app, "main"), "the branch");
    assert!(
        !shows(&app, "Stage all"),
        "no staging: a commit takes them all"
    );
    assert!(shows(&app, "Push 2 commits"));
    assert!(shows(&app, "to origin/main"));
    // Diffs stay closed until asked for.
    assert!(!shows(&app, "PORT = 9877"));
}

#[test]
fn a_file_opens_to_its_diff() {
    let mut app = panel(found(Some(status(vec![state("notes.md", false, true)], 0))));
    let _ = interact(&mut app, |ui| {
        let _ = ui.click("notes.md");
    });
    assert!(shows(&app, "My notes"));
    let _ = interact(&mut app, |ui| {
        let _ = ui.click("notes.md");
    });
    assert!(!shows(&app, "My notes"));
}

#[test]
fn a_message_commits_every_uncommitted_file() {
    let mut app = panel(found(Some(status(
        vec![
            state("src/check.py", false, true),
            state("notes.md", false, true),
        ],
        0,
    ))));
    let sent = interact(&mut app, |ui| {
        let _ = ui.click("Commit");
    });
    assert!(
        !sent
            .iter()
            .any(|m| matches!(m, Message::Changes(ChangesEvent::Commit))),
        "nothing to commit without a message: {sent:?}"
    );

    let _ = interact(&mut app, |ui| {
        let _ = ui.click(Id::new(ids::COMMIT_MESSAGE));
        let _ = ui.typewrite("Use port 9877");
    });
    assert_eq!(app.changes.message, "Use port 9877");
    let sent = interact(&mut app, |ui| {
        let _ = ui.click("Commit");
    });
    assert!(
        sent.iter()
            .any(|m| matches!(m, Message::Changes(ChangesEvent::Commit))),
        "{sent:?}"
    );
    assert!(shows(&app, "Committing…"));

    let _ = app.update(Message::Changes(ChangesEvent::Ran(
        GitAction::Commit,
        Ok("Committed 1a2b3c4 Use port 9877".into()),
    )));
    assert!(shows(&app, "Committed 1a2b3c4 Use port 9877"));
    assert!(app.changes.message.is_empty(), "the message is used up");
}

#[test]
fn a_failed_push_says_what_git_said() {
    let mut app = panel(found(Some(status(Vec::new(), 1))));
    let sent = interact(&mut app, |ui| {
        let _ = ui.click("Push 1 commit");
    });
    assert!(
        sent.iter()
            .any(|m| matches!(m, Message::Changes(ChangesEvent::Push))),
        "{sent:?}"
    );
    assert!(shows(&app, "Pushing…"));
    let _ = app.update(Message::Changes(ChangesEvent::Ran(
        GitAction::Push,
        Err("! [rejected] main -> main (fetch first)".into()),
    )));
    assert!(shows(&app, "! [rejected] main -> main (fetch first)"));
    assert!(shows(&app, "Push 1 commit"), "and it can be tried again");
}

#[test]
fn without_git_here_the_list_is_read_only() {
    let app = panel(found(None));
    assert!(shows(&app, "notes.md"));
    assert!(shows(
        &app,
        "Committing needs the folder on this computer, in a Git repository."
    ));
    assert!(!shows(&app, "Commit"));
}

#[test]
fn a_finished_run_reloads_and_a_late_answer_is_ignored() {
    let mut app = panel(found(Some(status(vec![state("notes.md", false, true)], 0))));
    let _ = app.update(event(EventKind::Done {
        session_id: PI_UPDATE.into(),
        outcome: "completed".into(),
    }));
    let fresh = ChangesLoaded::new(
        None,
        Some(Ok(Vec::new())),
        Ok(vec![diff("fresh.rs", "@@ -0,0 +1 @@\n+new\n", 1, 0)]),
        Some(Ok(status(vec![state("fresh.rs", false, true)], 0))),
    );
    // An answer to the first load, arriving late, changes nothing.
    let stale = ChangesLoaded::new(
        None,
        Some(Ok(Vec::new())),
        Ok(Vec::new()),
        Some(Ok(status(Vec::new(), 0))),
    );
    let _ = app.update(Message::Changes(ChangesEvent::Loaded(1, Box::new(stale))));
    assert!(shows(&app, "notes.md"));
    let _ = app.update(Message::Changes(ChangesEvent::Loaded(2, Box::new(fresh))));
    assert!(shows(&app, "fresh.rs"));
    assert!(shows(&app, "The agent hasn't changed any files."));
}

#[test]
fn an_edit_opens_its_turn_on_the_hunk_it_changed() {
    let mut app = panel(found(None));
    // The edit changed the second of the file's hunks this turn.
    let edit = diff(
        "src/check.py",
        "@@ -40,1 +40,1 @@\n-PORT = 80\n+PORT = 9877\n",
        1,
        1,
    );
    let turn = diff(
        "src/check.py",
        "@@ -1,1 +1,1 @@\n-import os\n+import socket\n@@ -40,1 +40,1 @@\n-PORT = 80\n+PORT = 9877\n",
        2,
        2,
    );
    let history = vec![
        Entry::Prompt {
            key: "msg_1".into(),
            text: "Pin the port".into(),
            attachments: Vec::new(),
            skills: Vec::new(),
        },
        Entry::Tool {
            key: "msg_2/0".into(),
            name: "edit".into(),
            label: "Edit".into(),
            detail: "src/check.py".into(),
            status: ToolStatus::Completed,
            took: None,
            files: vec![edit],
            todos: None,
        },
    ];
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(history)));
    let _ = app.update(Message::Timeline(
        PI_UPDATE.into(),
        harlo::timeline::Event::Review {
            tool: "msg_2/0".into(),
            file: 0,
        },
    ));
    let review = app.changes.review.as_ref().expect("a review of that turn");
    assert_eq!(review.from, "msg_1");
    assert_eq!(review.path, "src/check.py");

    // The turn's changes arrive.
    let loaded = found_with(Some(vec![turn]), None);
    let _ = app.update(Message::Changes(ChangesEvent::Loaded(2, Box::new(loaded))));
    assert_eq!(app.changes.selected.as_deref(), Some("turn:src/check.py"));
    assert_eq!(
        app.changes.hunk,
        Some(1),
        "the hunk it changed, after a gap"
    );
    assert!(shows(&app, "This turn"));
    assert!(shows(&app, "Pin the port"));
    assert!(app.panel_shown(Panel::Changes));
}
