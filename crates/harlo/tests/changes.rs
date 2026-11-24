//! The Changes panel: the session's and the folder's changes, opening a
//! diff, committing and pushing, ✦ writing the message, comments on diff
//! lines, reloading when files change, and an edit opening its turn.
//!
//! Loads and git actions are fed in as their results; the git commands
//! themselves are tested in `harlo-core`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{
    ChangedFile, ChangesEvent, ChangesLoaded, GitAction, LineComment, LineRef, Message, Panel,
    clean_message, comments_message, commit_prompt,
};
use harlo::timeline::Event as TimelineEvent;
use harlo_core::diff::LineKind;
use harlo_core::git::{FileState, Status};
use harlo_core::server::{FileDiff, Update};
use harlo_core::timeline::Entry;
use harlo_core::workspace::{Event, EventKind, ToolStatus};
use slint::Model as _;
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
        "@@ -1,1 +1,2 @@\n-import os\n+import socket\n+PORT = 9877\n",
        2,
        1,
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
    // The rest opens in the maximized panel, on this file.
    ui.press("20 more lines");
    assert_eq!(ui.app().maximized, Some(harlo::app::Panel::Changes));
    assert_eq!(ui.app().changes.selected.as_deref(), Some("tree:long.txt"));
    assert!(ui.shows("line 45"), "all of it");
}

#[test]
fn a_long_line_wraps_inside_the_panel() {
    let long = "word ".repeat(80);
    let patch = format!("@@ -0,0 +1,1 @@\n+{long}\n");
    let loaded = ChangesLoaded::new(
        None,
        Some(Ok(Vec::new())),
        Ok(vec![diff("wide.txt", &patch, 1, 0)]),
        Some(Ok(status(vec![state("wide.txt", false, true)], 0))),
    );
    let ui = panel(loaded);
    ui.press("wide.txt");
    ui.wait(50);
    let columns = ui.app().changes.columns[0];
    assert!(columns > 8, "the panel measured its width: {columns}");
    let rows = slint::ComponentHandle::global::<harlo::ui::State>(&ui.window).get_change_rows();
    let pieces: Vec<_> = rows.iter().filter(|row| row.line.kind == 1).collect();
    assert!(pieces.len() > 1, "the line wraps");
    assert!(
        pieces
            .iter()
            .all(|p| p.line.text.chars().count() <= columns)
    );
    assert_eq!(
        pieces
            .iter()
            .map(|p| p.line.text.to_string())
            .collect::<String>(),
        long,
        "nothing is lost"
    );
    assert!(pieces[1..].iter().all(|p| p.line.number.is_empty()));
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
fn the_sparkle_writes_the_message_for_the_folder_it_was_asked_for() {
    let ui = panel(found(Some(status(
        vec![
            state("src/check.py", false, true),
            state("notes.md", false, true),
        ],
        0,
    ))));
    drop(ui.driver.take_tasks());
    ui.press("Write the message");
    assert!(sent(&ui, |e| matches!(e, ChangesEvent::WriteMessage)));
    assert_eq!(ui.driver.take_tasks().len(), 1, "a request to the server");
    assert!(ui.shows("Writing…"));
    ui.press("Writing the message…");
    assert!(ui.driver.take_tasks().is_empty(), "one at a time");

    let folder = ui.app().changes.folder().expect("a folder").to_owned();
    ui.send(Message::Changes(ChangesEvent::MessageWritten(
        "/elsewhere".into(),
        Ok("Wrong folder".into()),
    )));
    assert!(
        ui.app().changes.message.is_empty(),
        "another folder's message"
    );
    ui.send(Message::Changes(ChangesEvent::MessageWritten(
        folder.clone(),
        Ok("Use port 9877".into()),
    )));
    assert_eq!(ui.app().changes.message, "Use port 9877");
    assert!(ui.shows("Write the message"));

    ui.send(Message::Changes(ChangesEvent::MessageWritten(
        folder,
        Err("Couldn't write a message: no model".into()),
    )));
    assert!(ui.shows("Couldn't write a message: no model"));
}

#[test]
fn the_prompt_holds_the_diffs_and_names_what_doesnt_fit() {
    let small = ChangedFile::from_diff(&diff(
        "src/check.py",
        "@@ -1,1 +1,2 @@\n import socket\n+PORT = 9877\n",
        1,
        0,
    ));
    let prompt = commit_prompt(std::slice::from_ref(&small));
    assert!(prompt.starts_with("Write a git commit message"));
    assert!(
        prompt.contains("--- src/check.py (+1 -0)\n import socket\n+PORT = 9877\n"),
        "{prompt}"
    );

    let huge_patch = format!("@@ -0,0 +1,3000 @@\n{}", "+a line of text\n".repeat(3000));
    let huge = ChangedFile::from_diff(&diff("big.txt", &huge_patch, 3000, 0));
    let prompt = commit_prompt(&[small, huge]);
    assert!(prompt.len() < 25_000, "{}", prompt.len());
    assert!(prompt.contains("+PORT = 9877"));
    assert!(prompt.ends_with("Also changed, too large to show: big.txt\n"));
}

#[test]
fn a_reply_loses_its_fences_and_quotes() {
    assert_eq!(clean_message("  Fix the port\n\n"), "Fix the port");
    assert_eq!(
        clean_message("```text\nFix the port\n\nIt was wrong.\n```"),
        "Fix the port\n\nIt was wrong."
    );
    assert_eq!(clean_message("\"Fix the port\""), "Fix the port");
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
            pictures: Vec::new(),
            skills: Vec::new(),
            agent: None,
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
            pictures: Vec::new(),
            error: None,
            script: None,
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

/// Writes `text` as a comment on the line with the number button `line`.
fn comment(ui: &Ui, line: &str, text: &str) {
    ui.press(line);
    ui.fill("Comment on this line", text);
    ui.press("Add");
}

#[test]
fn a_line_number_takes_a_comment_and_send_sends_them_as_one_message() {
    let ui = panel(found(Some(status(vec![state("notes.md", false, true)], 0))));
    ui.press("notes.md");
    assert!(!ui.shows("Send to the agent"), "nothing to send yet");

    comment(&ui, "Comment on line 1", "Say whose notes");
    assert!(ui.shows("Say whose notes"), "under its line");
    assert!(!ui.shows("Comment on this line"), "the field closes");
    assert!(ui.shows("1 comment"));
    // A second comment, on the same line.
    comment(&ui, "Comment on line 1", "And date them");
    assert!(ui.shows("2 comments"));

    ui.press("Send to the agent");
    assert!(ui.app().is_sending(), "sent the way the composer sends");
    assert!(
        ui.app().changes.comments.is_empty(),
        "and gone from the panel"
    );
    assert!(!ui.shows("Send to the agent"));
    assert!(ui.app().composer.text().is_empty());
}

#[test]
fn the_composers_text_goes_first() {
    let ui = panel(found(Some(status(vec![state("notes.md", false, true)], 0))));
    ui.press("notes.md");
    comment(&ui, "Comment on line 1", "Say whose notes");
    ui.send(Message::Compose(harlo::text::Edit {
        text: "Please fix these".into(),
        cursor: 16,
        anchor: 16,
    }));
    assert!(ui.shows("Sent as one message after what's in the composer."));
    ui.press("Send to the agent");
    // A failed send puts the whole message back in the composer.
    ui.send(Message::Sent(Err("refused".into())));
    let text = ui.app().composer.text();
    assert!(
        text.starts_with("Please fix these\n\nA comment on the changes:\n\nnotes.md, line 1:"),
        "{text}"
    );
    assert!(text.ends_with("Say whose notes\n"), "{text}");
}

#[test]
fn a_comment_stays_with_its_line_and_can_be_removed_or_discarded() {
    let ui = panel(found(Some(status(
        vec![state("src/check.py", false, true)],
        0,
    ))));
    // The session's list has it first.
    ui.button("src/check.py").invoke_accessible_default_action();
    ui.press("Comment on line 2");
    // The same number again closes the field.
    ui.press("Comment on line 2");
    assert!(!ui.shows("Comment on this line"));
    ui.press("Comment on line 2");
    ui.press("Cancel the comment");
    assert!(!ui.shows("Comment on this line"));

    comment(&ui, "Comment on line 2", "Read the port from the config");
    comment(&ui, "Comment on line 1", "Unused import");
    // A reload keeps them under their lines.
    ui.send(Message::Changes(ChangesEvent::Refresh));
    ui.send(Message::Changes(ChangesEvent::Loaded(
        2,
        Box::new(found(Some(status(
            vec![state("src/check.py", false, true)],
            0,
        )))),
    )));
    assert!(ui.shows("Read the port from the config"));

    ui.press("Remove the comment");
    assert!(ui.shows("1 comment"));
    ui.press("Discard");
    assert!(!ui.shows("Read the port from the config") && !ui.shows("Unused import"));
    assert!(ui.app().changes.comments.is_empty());
}

#[test]
fn the_maximized_diff_takes_comments_too() {
    let ui = panel(found(Some(status(vec![state("notes.md", false, true)], 0))));
    ui.press("Maximize");
    ui.button("notes.md").invoke_accessible_default_action();
    comment(&ui, "Comment on line 1", "Say whose notes");
    assert!(ui.shows("Say whose notes"));
    assert!(ui.shows("1 comment"));
}

#[test]
fn the_message_names_each_file_and_line() {
    let line = |path: &str, kind, number, text: &str| LineRef {
        path: path.into(),
        kind,
        number: Some(number),
        text: text.into(),
    };
    let comments = [
        LineComment {
            folder: "/p".into(),
            on: line("src/check.py", LineKind::Added, 2, "PORT = 9877"),
            text: "Read it from the config".into(),
        },
        LineComment {
            folder: "/p".into(),
            on: line("src/old.py", LineKind::Removed, 7, "import os"),
            text: "Still needed".into(),
        },
    ];
    assert_eq!(
        comments_message(&comments),
        "Comments on the changes:\n\
         \nsrc/check.py, line 2:\n```diff\n+PORT = 9877\n```\nRead it from the config\n\
         \nsrc/old.py, line 7 (removed):\n```diff\n-import os\n```\nStill needed\n"
    );
}

#[test]
fn only_changed_lines_show_with_a_break_where_unchanged_ones_are_left_out() {
    let patch =
        "@@ -1,5 +1,5 @@\n-first old\n+first new\n kept one\n kept two\n-last old\n+last new\n";
    let loaded = ChangesLoaded::new(
        None,
        Some(Ok(vec![diff("src/two.py", patch, 2, 2)])),
        Ok(vec![]),
        None,
    );
    let ui = panel(loaded);
    ui.button("src/two.py").invoke_accessible_default_action();
    assert!(ui.shows("first new") && ui.shows("last old"));
    assert!(!ui.shows("kept one") && !ui.shows("kept two"));
}
