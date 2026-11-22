//! The Files panel and the editor tab: the folder's tree, opening a file,
//! editing and saving, closing with unsaved changes, and changes made on
//! disk meanwhile.
//!
//! The demo app's requests never run, so these tests hand it what the server
//! would send: listings, file contents, save results and change reports.

// Rust guideline compliant 2026-10-09

mod support;

use std::collections::HashSet;
use std::path::Path;

use harlo::app::{FilesEvent, Message, OpenFile};
use harlo_core::server::{FileChange, FileMatch, Update};
use harlo_core::workspace::{Event, EventKind};
use i_slint_backend_testing::AccessibleRole;
use slint::ComponentHandle as _;
use slint::LogicalPosition;
use slint::platform::{PointerEventButton, WindowEvent};
use support::{Ui, demo, ui};

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

/// The window with "pi update" open, the Files panel showing its folder's
/// root, and that folder's path.
fn with_tree() -> (Ui, String) {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let ui = ui(app);
    ui.press("Files");
    let root = ui.app().file_tree().0;
    ui.send(files(FilesEvent::Listed {
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
    (ui, root)
}

fn key(root: &str, path: &str) -> String {
    Path::new(root).join(path).to_string_lossy().into_owned()
}

/// Opens `path` and hands it `text` as its contents; returns its key.
fn open(ui: &Ui, root: &str, path: &str, text: &str) -> String {
    ui.send(files(FilesEvent::Open(path.into())));
    let key = key(root, path);
    ui.send(files(FilesEvent::Read {
        key: key.clone(),
        bytes: Ok(text.as_bytes().to_vec()),
    }));
    key
}

/// Types `text` into the editor showing `path`, in place of its text.
fn type_text(ui: &Ui, path: &str, text: &str) {
    ui.fill(path, text);
}

fn dirty(ui: &Ui) -> bool {
    ui.app().shown_file().is_some_and(OpenFile::is_dirty)
}

#[test]
fn the_tree_lists_the_folder_and_opens_folders_in_place() {
    let (ui, root) = with_tree();
    for name in ["src", "target", "README.md", "logo.png"] {
        assert!(ui.shows(name), "{name}");
    }
    assert!(!ui.shows(".git"), "Git's own folder is left out");

    ui.press("src");
    ui.send(files(FilesEvent::Listed {
        root,
        folder: "src".into(),
        entries: Ok(vec![entry("src/main.rs", false)]),
        ignored: HashSet::new(),
    }));
    assert!(ui.shows("main.rs"));
    ui.press("src");
    assert!(!ui.shows("main.rs"), "closed again");
}

#[test]
fn clicking_a_file_opens_it_in_the_editor_tab() {
    let (ui, root) = with_tree();
    ui.press("README.md");
    assert_eq!(ui.app().fs.shown, Some(key(&root, "README.md")));
    assert!(ui.shows("Opening…") || ui.shows("Opening the file…"));
    ui.send(files(FilesEvent::Read {
        key: key(&root, "README.md"),
        bytes: Ok(b"# Demo\n".to_vec()),
    }));
    assert_eq!(ui.tabs("README.md").len(), 1, "its tab");
    let field = ui
        .find("README.md")
        .into_iter()
        .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("the editor");
    assert_eq!(field.accessible_value().as_deref(), Some("# Demo\n"));
}

#[test]
fn a_file_is_edited_and_saved() {
    let (ui, root) = with_tree();
    let key = open(&ui, &root, "README.md", "# Demo\n");
    type_text(&ui, "README.md", "# Demo\nHi\n");
    assert_eq!(ui.app().file_text(&key).as_deref(), Some("# Demo\nHi\n"));
    assert!(ui.shows("Unsaved changes"));

    ui.press("Save");
    assert!(ui.shows("Saving…"));
    // A save that fails says why, and keeps the changes.
    ui.send(files(FilesEvent::Saved {
        key: key.clone(),
        text: "# Demo\nHi\n".into(),
        result: Err("disk full".into()),
    }));
    assert!(ui.shows("Couldn't save: disk full"));
    assert!(dirty(&ui));

    // A save that lands clears the mark.
    ui.send(files(FilesEvent::Saved {
        key,
        text: "# Demo\nHi\n".into(),
        result: Ok(()),
    }));
    assert!(!ui.shows("Unsaved changes"));
    assert!(!dirty(&ui));
}

#[test]
fn closing_with_unsaved_changes_asks_first() {
    let (ui, root) = with_tree();
    let _ = open(&ui, &root, "README.md", "# Demo\n");
    type_text(&ui, "README.md", "x");

    ui.send(Message::CloseActiveTab);
    let asked = "Save your changes to README.md before closing?";
    assert!(ui.shows(asked));
    ui.press("Cancel");
    assert!(!ui.shows(asked));

    ui.send(Message::CloseActiveTab);
    ui.press("Don't save");
    assert!(ui.app().shown_file().is_none(), "the session is back");
    assert!(ui.app().fs.open.is_empty());

    // A file without changes closes straight away.
    let key = open(&ui, &root, "README.md", "# Demo\n");
    ui.send(files(FilesEvent::Close(key)));
    assert!(ui.app().fs.open.is_empty());
}

#[test]
fn a_change_on_disk_reloads_or_asks() {
    let (ui, root) = with_tree();
    let key = open(&ui, &root, "README.md", "one\n");

    // No unsaved changes: the new text simply takes over.
    ui.send(files(FilesEvent::Reread {
        key: key.clone(),
        bytes: Ok(b"two\n".to_vec()),
    }));
    assert_eq!(ui.app().file_text(&key).as_deref(), Some("two\n"));

    // With unsaved changes, the tab asks.
    type_text(&ui, "README.md", "mine two\n");
    ui.send(files(FilesEvent::Reread {
        key: key.clone(),
        bytes: Ok(b"three\n".to_vec()),
    }));
    assert!(ui.shows("This file changed on disk while you had unsaved changes."));
    assert!(ui.app().file_text(&key).is_some_and(|t| t.contains("mine")));
    ui.press("Load the new version");
    assert_eq!(ui.app().file_text(&key).as_deref(), Some("three\n"));
    assert!(!dirty(&ui));

    // Deleted while edited: it says saving puts it back.
    type_text(&ui, "README.md", "again three\n");
    ui.send(files(FilesEvent::Reread {
        key,
        bytes: Err("not found".into()),
    }));
    assert!(ui.shows("This file was deleted on disk. Saving puts it back."));
}

#[test]
fn files_change_reports_are_followed() {
    let (ui, root) = with_tree();
    let key = open(&ui, &root, "README.md", "one\n");
    // A report for an open file only asks to read it again: the tab keeps
    // its text until the new contents arrive.
    ui.send(Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind: EventKind::FileChanged {
            directory: root,
            file: key.clone(),
            change: FileChange::Changed,
        },
    })));
    assert_eq!(ui.app().file_text(&key).as_deref(), Some("one\n"));
}

#[test]
fn one_editor_tab_takes_each_file_opened() {
    let (ui, root) = with_tree();
    let readme = open(&ui, &root, "README.md", "# Demo\n");
    ui.send(files(FilesEvent::Open("logo.png".into())));
    let picture = key(&root, "logo.png");
    assert_eq!(ui.app().fs.open.len(), 1, "the picture took README's tab");
    assert!(!ui.app().fs.open.iter().any(|f| f.key == readme));
    ui.send(files(FilesEvent::Read {
        key: picture.clone(),
        bytes: Ok(vec![0x89, b'P', b'N', b'G']),
    }));
    assert!(
        ui.app().file_text(&picture).is_none(),
        "a picture isn't edited"
    );

    // The tabs cycle: the session, the file, and round again.
    ui.send(Message::OpenSession(PI_UPDATE.into()));
    assert_eq!(ui.app().fs.shown, None, "the session shows");
    ui.send(Message::CycleTab { back: false });
    assert_eq!(ui.app().fs.shown, Some(picture));
    ui.send(Message::CycleTab { back: false });
    assert_eq!(ui.app().fs.shown, None);

    // A file that isn't text says so.
    let blob = open(&ui, &root, "data.bin", "a\0b");
    assert!(ui.shows("This file isn't text, so it can't be edited here."));
    ui.send(Message::CloseActiveTab);
    assert!(!ui.app().fs.open.iter().any(|f| f.key == blob));
}

#[test]
fn opening_another_file_asks_about_unsaved_changes() {
    let (ui, root) = with_tree();
    let readme = open(&ui, &root, "README.md", "# Demo\n");
    type_text(&ui, "README.md", "x");

    ui.send(files(FilesEvent::Open("notes.txt".into())));
    assert!(ui.shows("Save your changes to README.md before opening notes.txt?"));
    assert_eq!(
        ui.app().fs.shown,
        Some(readme.clone()),
        "the changes stay in view"
    );

    // Cancel keeps it; asking again and not saving opens the other file.
    ui.press("Cancel");
    assert!(ui.app().fs.open.iter().all(|f| f.then_open.is_none()));
    ui.send(files(FilesEvent::Open("notes.txt".into())));
    ui.press("Don't save");
    let notes = key(&root, "notes.txt");
    assert_eq!(ui.app().fs.shown, Some(notes.clone()));
    assert_eq!(ui.app().fs.open.len(), 1);

    // Saving first works too: the other file opens once the save lands.
    ui.send(files(FilesEvent::Read {
        key: notes.clone(),
        bytes: Ok(b"n\n".to_vec()),
    }));
    type_text(&ui, "notes.txt", "n y\n");
    ui.send(files(FilesEvent::Open("README.md".into())));
    ui.press("Save first");
    assert_eq!(
        ui.app().fs.shown,
        Some(notes.clone()),
        "waiting for the save"
    );
    ui.send(files(FilesEvent::Saved {
        key: notes,
        text: "n y\n".into(),
        result: Ok(()),
    }));
    assert_eq!(ui.app().fs.shown, Some(readme));
}

#[test]
fn a_middle_click_closes_a_tab() {
    let (ui, root) = with_tree();
    let _ = open(&ui, &root, "README.md", "# Demo\n");
    let tab = ui.tabs("README.md").into_iter().next().expect("its tab");
    let at = tab.absolute_position();
    let size = tab.size();
    let position = LogicalPosition::new(at.x + size.width / 3.0, at.y + size.height / 2.0);
    let window = ui.window.window();
    let button = PointerEventButton::Middle;
    for event in [
        WindowEvent::PointerMoved { position },
        WindowEvent::PointerPressed { position, button },
        WindowEvent::PointerReleased { position, button },
    ] {
        window.dispatch_event(event);
    }
    assert!(ui.app().fs.open.is_empty());
    assert!(ui.tabs("README.md").is_empty());
}
