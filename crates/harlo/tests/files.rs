//! The Files panel and editor tabs: the folder's tree, opening files in
//! tabs beside the sessions, editing and saving, closing with unsaved
//! changes, and changes made on disk meanwhile.
//!
//! The demo app's requests never run, so these tests hand it what the server
//! would send: listings, file contents, save results and change reports.

// Rust guideline compliant 2026-10-09

mod support;

use std::collections::HashSet;
use std::path::Path;

use harlo::app::OpenFile;
use harlo::{App, FilesEvent, Message, Panel};
use harlo_core::server::{FileChange, FileMatch, Update};
use harlo_core::workspace::{Event, EventKind};
use iced_code_editor::Message as EditorMessage;
use support::{demo, interact, shows};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

fn entry(path: &str, directory: bool) -> FileMatch {
    FileMatch {
        path: path.into(),
        directory,
    }
}

fn files(event: FilesEvent) -> Message {
    Message::Files(event)
}

/// The demo app with "pi update" open, the Files panel showing its folder's
/// root, and that folder's path.
fn with_tree() -> (App, String) {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    if app.panel != Some(Panel::Files) {
        let _ = app.update(Message::OpenPanel(Panel::Files));
    }
    let root = app.file_tree().0;
    let _ = app.update(files(FilesEvent::Listed {
        root: root.clone(),
        folder: String::new(),
        entries: Ok(vec![
            entry("src", true),
            entry("target", true),
            entry(".git", true),
            entry("README.md", false),
            entry("logo.png", false),
        ]),
        ignored: HashSet::from(["target".to_owned()]),
    }));
    (app, root)
}

/// Opens `path` and hands it `text` as its contents; returns its key.
fn open(app: &mut App, root: &str, path: &str, text: &str) -> String {
    let _ = app.update(files(FilesEvent::Open(path.into())));
    let key = Path::new(root).join(path).to_string_lossy().into_owned();
    let _ = app.update(files(FilesEvent::Read {
        key: key.clone(),
        bytes: Ok(text.as_bytes().to_vec()),
    }));
    key
}

/// Edits the file by pasting `text` at the cursor, which (unlike typing)
/// doesn't need the editor to have been clicked first.
fn type_text(app: &mut App, key: &str, text: &str) {
    let _ = app.update(files(FilesEvent::Editor(
        key.into(),
        EditorMessage::Paste(text.into()),
    )));
}

#[test]
fn the_tree_lists_the_folder_and_opens_folders_in_place() {
    let (mut app, root) = with_tree();
    for name in ["src", "target", "README.md", "logo.png"] {
        assert!(shows(&app, name), "{name}");
    }
    assert!(!shows(&app, ".git"), "Git's own folder is left out");

    let _ = app.update(files(FilesEvent::Toggle("src".into())));
    let _ = app.update(files(FilesEvent::Listed {
        root,
        folder: "src".into(),
        entries: Ok(vec![entry("src/main.rs", false)]),
        ignored: HashSet::new(),
    }));
    assert!(shows(&app, "main.rs"));
    let _ = app.update(files(FilesEvent::Toggle("src".into())));
    assert!(!shows(&app, "main.rs"), "closed again");
}

#[test]
fn clicking_a_file_opens_it() {
    let (mut app, root) = with_tree();
    let messages = interact(&mut app, |ui| {
        let _ = ui.click("README.md");
    });
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Files(FilesEvent::Open(path)) if path == "README.md")),
        "{messages:?}"
    );
    let key = Path::new(&root)
        .join("README.md")
        .to_string_lossy()
        .into_owned();
    assert_eq!(app.fs.shown, Some(key));
}

#[test]
fn a_file_opens_in_a_tab_and_saves() {
    let (mut app, root) = with_tree();
    let key = open(&mut app, &root, "README.md", "# Demo\n");
    assert_eq!(app.shown_file().map(|f| f.key.as_str()), Some(key.as_str()));
    assert!(shows(&app, "README.md"), "its tab");
    assert_eq!(app.file_text(&key).as_deref(), Some("# Demo\n"));

    type_text(&mut app, &key, "Hi");
    let edited = app.file_text(&key).expect("text");
    assert_ne!(edited, "# Demo\n");
    assert!(shows(&app, "Unsaved changes"));

    let _ = app.update(files(FilesEvent::Save(key.clone())));
    assert!(shows(&app, "Saving…"));
    // A save that fails says why, and keeps the changes.
    let _ = app.update(files(FilesEvent::Saved {
        key: key.clone(),
        text: edited.clone(),
        result: Err("disk full".into()),
    }));
    assert!(shows(&app, "Couldn't save: disk full"));
    assert!(app.shown_file().is_some_and(OpenFile::is_dirty));

    // A save that lands clears the mark.
    let _ = app.update(files(FilesEvent::Saved {
        key: key.clone(),
        text: edited,
        result: Ok(()),
    }));
    assert!(!shows(&app, "Unsaved changes"));
    assert!(!app.shown_file().is_some_and(OpenFile::is_dirty));
}

#[test]
fn closing_with_unsaved_changes_asks_first() {
    let (mut app, root) = with_tree();
    let key = open(&mut app, &root, "README.md", "# Demo\n");
    type_text(&mut app, &key, "x");

    let _ = app.update(Message::CloseActiveTab);
    assert!(shows(
        &app,
        "Save your changes to README.md before closing?"
    ));
    let _ = app.update(files(FilesEvent::KeepOpen(key.clone())));
    assert!(!shows(
        &app,
        "Save your changes to README.md before closing?"
    ));

    let _ = app.update(Message::CloseActiveTab);
    let _ = app.update(files(FilesEvent::Discard(key)));
    assert!(app.shown_file().is_none(), "the session is back");
    assert!(app.fs.open.is_empty());

    // A file without changes closes straight away.
    let key = open(&mut app, &root, "README.md", "# Demo\n");
    let _ = app.update(files(FilesEvent::Close(key)));
    assert!(app.fs.open.is_empty());
}

#[test]
fn a_change_on_disk_reloads_or_asks() {
    let (mut app, root) = with_tree();
    let key = open(&mut app, &root, "README.md", "one\n");

    // No unsaved changes: the new text simply takes over.
    let _ = app.update(files(FilesEvent::Reread {
        key: key.clone(),
        bytes: Ok(b"two\n".to_vec()),
    }));
    assert_eq!(app.file_text(&key).as_deref(), Some("two\n"));

    // With unsaved changes, the tab asks.
    type_text(&mut app, &key, "mine ");
    let _ = app.update(files(FilesEvent::Reread {
        key: key.clone(),
        bytes: Ok(b"three\n".to_vec()),
    }));
    assert!(shows(
        &app,
        "This file changed on disk while you had unsaved changes."
    ));
    assert!(app.file_text(&key).is_some_and(|t| t.contains("mine")));
    let _ = app.update(files(FilesEvent::Reload(key.clone())));
    assert_eq!(app.file_text(&key).as_deref(), Some("three\n"));
    assert!(!app.shown_file().is_some_and(OpenFile::is_dirty));

    // Deleted while edited: it says saving puts it back.
    type_text(&mut app, &key, "again ");
    let _ = app.update(files(FilesEvent::Reread {
        key: key.clone(),
        bytes: Err("not found".into()),
    }));
    assert!(shows(
        &app,
        "This file was deleted on disk. Saving puts it back."
    ));
}

#[test]
fn files_change_reports_are_followed() {
    let (mut app, root) = with_tree();
    let key = open(&mut app, &root, "README.md", "one\n");
    // A report for an open file only asks to read it again: the tab keeps
    // its text until the new contents arrive.
    let _ = app.update(Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind: EventKind::FileChanged {
            directory: root,
            file: key.clone(),
            change: FileChange::Changed,
        },
    })));
    assert_eq!(app.file_text(&key).as_deref(), Some("one\n"));
}

#[test]
fn file_tabs_sit_beside_the_sessions() {
    let (mut app, root) = with_tree();
    let readme = open(&mut app, &root, "README.md", "# Demo\n");
    let picture = {
        let _ = app.update(files(FilesEvent::Open("logo.png".into())));
        Path::new(&root)
            .join("logo.png")
            .to_string_lossy()
            .into_owned()
    };
    assert!(shows(&app, "Opening…"));
    let _ = app.update(files(FilesEvent::Read {
        key: picture.clone(),
        bytes: Ok(vec![0x89, b'P', b'N', b'G']),
    }));
    assert!(app.file_text(&picture).is_none(), "a picture isn't edited");

    // The tabs cycle: session, README, logo, and round again.
    let shown = |app: &App| app.fs.shown.clone();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    assert_eq!(shown(&app), None, "the session shows");
    let _ = app.update(Message::CycleTab { back: false });
    assert_eq!(shown(&app), Some(readme.clone()));
    let _ = app.update(Message::CycleTab { back: false });
    assert_eq!(shown(&app), Some(picture.clone()));
    let _ = app.update(Message::CycleTab { back: true });
    assert_eq!(shown(&app), Some(readme));

    // A file that isn't text says so.
    let blob = open(&mut app, &root, "data.bin", "a\0b");
    assert!(shows(
        &app,
        "This file isn't text, so it can't be edited here."
    ));
    let _ = app.update(Message::CloseActiveTab);
    assert!(!app.fs.open.iter().any(|f| f.key == blob));
}
