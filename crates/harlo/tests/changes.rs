//! The Changes panel: the session's and the folder's changes, opening a
//! diff, committing and pushing, reloading when files change, and an edit
//! opening its turn.
//!
//! Loads and git actions are fed in as their results; the git commands
//! themselves are tested in `harlo-core`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{ChangesEvent, ChangesLoaded, GitAction, Message, Panel};
use harlo::timeline::Event as TimelineEvent;
use harlo_core::git::{FileState, Status};
use harlo_core::server::{FileDiff, Update};
use harlo_core::timeline::Entry;
use harlo_core::workspace::{Event, EventKind, ToolStatus};
use support::{Ui, WINDOW, demo, ui_at};

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

/// The window with "pi update" open and the Changes panel showing `loaded`.
fn panel(loaded: ChangesLoaded) -> Ui {
    panel_at(loaded, WINDOW)
}

/// [`panel`], in a window `size` big.
fn panel_at(loaded: ChangesLoaded, size: (f32, f32)) -> Ui {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let ui = ui_at(app, size);
    ui.open("Changes");
    // Opening it starts the first load.
    ui.send(Message::Changes(ChangesEvent::Loaded(1, Box::new(loaded))));
    ui
}

fn event(kind: EventKind) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind,
    }))
}

fn sent(ui: &Ui, wanted: impl Fn(&ChangesEvent) -> bool) -> bool {
    ui.messages()
        .iter()
        .any(|m| matches!(m, Message::Changes(event) if wanted(event)))
}

#[test]
fn the_panel_lists_the_session_and_uncommitted_files() {
    let ui = panel(found(Some(status(
        vec![
            state("src/check.py", true, false),
            state("notes.md", false, true),
        ],
        2,
    ))));
    assert!(ui.shows("This session"));
    assert!(ui.shows("Uncommitted"));
    assert!(ui.shows("check.py"), "the file's name");
    assert!(ui.shows("src"), "and its folder");
    assert!(ui.shows("notes.md"));
    assert!(ui.shows("main"), "the branch");
    assert!(
        !ui.shows("Stage all"),
        "no staging: a commit takes them all"
    );
    assert!(ui.shows("Push 2 commits"));
    assert!(ui.shows("to origin/main"));
    // Diffs stay closed until asked for.
    assert!(!ui.shows("PORT = 9877"));
}

#[test]
fn a_file_opens_to_its_diff() {
    let ui = panel(found(Some(status(vec![state("notes.md", false, true)], 0))));
    ui.press("notes.md");
    assert!(ui.shows("My notes"));
    ui.press("notes.md");
    assert!(!ui.shows("My notes"));
}

#[test]
fn a_long_diff_shows_its_start_then_all_of_it() {
    let patch = (0..60).fold(String::from("@@ -0,0 +1,60 @@\n"), |out, i| {
        out + &format!("+line {i}\n")
    });
    let loaded = ChangesLoaded::new(
        None,
        Some(Ok(Vec::new())),
        Ok(vec![diff("long.txt", &patch, 60, 0)]),
        Some(Ok(status(vec![state("long.txt", false, true)], 0))),
    );
    // Tall enough for the whole diff's start to be on screen.
    let ui = panel_at(loaded, (WINDOW.0, 1400.0));
    ui.press("long.txt");
    assert!(ui.shows("line 0"));
    assert!(!ui.shows("line 45"), "cut short");
    ui.press("20 more lines");
    assert!(!ui.shows("20 more lines"));
    assert!(ui.app().changes.whole.contains("tree:long.txt"));
}

#[test]
fn a_message_commits_every_uncommitted_file() {
    let ui = panel(found(Some(status(
        vec![
            state("src/check.py", false, true),
            state("notes.md", false, true),
        ],
        0,
    ))));
    ui.press("Commit");
    assert!(
        !sent(&ui, |e| matches!(e, ChangesEvent::Commit)),
        "nothing to commit without a message"
    );

    ui.fill("Commit message", "Use port 9877");
    assert_eq!(ui.app().changes.message, "Use port 9877");
    ui.press("Commit");
    assert!(sent(&ui, |e| matches!(e, ChangesEvent::Commit)));
    assert!(ui.shows("Committing…"));

    ui.send(Message::Changes(ChangesEvent::Ran(
        GitAction::Commit,
        Ok("Committed 1a2b3c4 Use port 9877".into()),
    )));
    assert!(ui.shows("Committed 1a2b3c4 Use port 9877"));
    assert!(
        ui.app().changes.message.is_empty(),
        "the message is used up"
    );
}

#[test]
fn a_failed_push_says_what_git_said() {
    let ui = panel(found(Some(status(Vec::new(), 1))));
    ui.press("Push 1 commit");
    assert!(sent(&ui, |e| matches!(e, ChangesEvent::Push)));
    assert!(ui.shows("Pushing…"));
    ui.send(Message::Changes(ChangesEvent::Ran(
        GitAction::Push,
        Err("! [rejected] main -> main (fetch first)".into()),
    )));
    assert!(ui.shows("! [rejected] main -> main (fetch first)"));
    assert!(ui.shows("Push 1 commit"), "and it can be tried again");
}

#[test]
fn a_failed_load_says_why_on_one_line() {
    let loaded = ChangesLoaded::new(
        None,
        Some(Err(
            "the OpenCode server answered 500:\nno such session".into()
        )),
        Ok(Vec::new()),
        None,
    );
    let ui = panel(loaded);
    assert!(
        ui.shows("Couldn't load them: the OpenCode server answered 500: no such session"),
        "a message's lines are joined, so none spills over the rows below"
    );
}

#[test]
fn without_git_here_the_list_is_read_only() {
    let ui = panel(found(None));
    assert!(ui.shows("notes.md"));
    assert!(ui.shows("Committing needs the folder on this computer, in a Git repository."));
    assert!(!ui.shows("Commit"));
}

#[test]
fn a_finished_run_reloads_and_a_late_answer_is_ignored() {
    let ui = panel(found(Some(status(vec![state("notes.md", false, true)], 0))));
    ui.send(event(EventKind::Done {
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
    ui.send(Message::Changes(ChangesEvent::Loaded(1, Box::new(stale))));
    assert!(ui.shows("notes.md"));
    ui.send(Message::Changes(ChangesEvent::Loaded(2, Box::new(fresh))));
    assert!(ui.shows("fresh.rs"));
    assert!(ui.shows("The agent hasn't changed any files."));
}

#[test]
fn maximized_the_selected_files_diff_fills_the_right() {
    let ui = panel(found(Some(status(
        vec![
            state("src/check.py", false, true),
            state("notes.md", false, true),
        ],
        0,
    ))));
    ui.press("Maximize");
    assert!(ui.shows("src/check.py"), "the first file, selected");
    assert!(ui.shows("PORT = 9877"), "its diff beside the lists");
    // Picking another file shows its diff instead.
    ui.button("notes.md").invoke_accessible_default_action();
    assert!(ui.shows("My notes"));
    assert!(!ui.shows("PORT = 9877"));
}

#[test]
fn an_edit_opens_its_turn_on_the_hunk_it_changed() {
    let ui = panel(found(None));
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
    ui.send(Message::History(PI_UPDATE.into(), Ok(history)));
    ui.send(Message::Timeline(
        PI_UPDATE.into(),
        TimelineEvent::Review {
            tool: "msg_2/0".into(),
            file: 0,
        },
    ));
    let review = ui
        .app()
        .changes
        .review
        .clone()
        .expect("a review of that turn");
    assert_eq!(review.from, "msg_1");
    assert_eq!(review.path, "src/check.py");

    // The turn's changes arrive.
    let loaded = found_with(Some(vec![turn]), None);
    ui.send(Message::Changes(ChangesEvent::Loaded(2, Box::new(loaded))));
    assert_eq!(
        ui.app().changes.selected.as_deref(),
        Some("turn:src/check.py")
    );
    assert_eq!(
        ui.app().changes.hunk,
        Some(1),
        "the hunk it changed, after a gap"
    );
    assert!(ui.shows("This turn"));
    assert!(ui.shows("Pin the port"));
    assert!(ui.shows("PORT = 9877"), "its diff open whole");
    assert!(ui.app().panel_shown(Panel::Changes));

    ui.press("End review");
    assert!(!ui.shows("This turn"));
}
