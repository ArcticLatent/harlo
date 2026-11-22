//! Terminals in the dock: the rail button opens a shell below the
//! conversation, hides and shows the shells, and closing a tab ends one.
//!
//! The shells are `/bin/sh` in a scratch folder outside the repository, never
//! the user's own shell or its startup files.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use harlo::{App, DockKey, Message, TerminalEvent, ids};
use harlo_core::layout::DockNode;
use harlo_core::server::Location;
use iced::widget::Id;
use iced_term::BackendCommand;
use support::{demo, interact, shows};

/// How long a shell gets to answer.
const ANSWER: Duration = Duration::from_secs(10);

/// The demo app, with terminals running `/bin/sh` in `folder`.
fn app(folder: &Path) -> App {
    let mut app = App::new(Location::Shared, vec![Location::Shared], demo::layout())
        .terminal_shell("/bin/sh", folder.to_string_lossy());
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(demo::connected(demo::snapshot()));
    app
}

/// Clicks the rail's terminal button.
fn toggle(app: &mut App) {
    let _ = interact(app, |ui| {
        ui.click(Id::new(ids::RAIL_TERMINAL))
            .expect("the terminal button");
    });
}

/// The terminal tabs in the dock.
fn docked(app: &App) -> Vec<DockKey> {
    app.dock
        .keys()
        .into_iter()
        .filter(|key| matches!(key, DockKey::Terminal(_)))
        .collect()
}

/// Types `text` into terminal `id`.
fn type_into(app: &mut App, id: u64, text: &str) {
    let call = iced_term::Event::BackendCall(id, BackendCommand::Write(text.as_bytes().to_vec()));
    let _ = app.update(Message::Terminal(TerminalEvent::Backend(call)));
}

/// Waits until terminal `id` shows a line with `text`.
fn wait_for(app: &App, id: u32, text: &str) -> bool {
    let until = Instant::now() + ANSWER;
    while Instant::now() < until {
        let shell = app.terminals.get(id).expect("the terminal");
        if shell.term.text().iter().any(|line| line.contains(text)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn the_rail_button_opens_a_shell_below_the_conversation() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let mut app = app(scratch.path());
    toggle(&mut app);
    assert!(app.terminals_shown());
    assert_eq!(docked(&app), [DockKey::Terminal(0)]);

    // A split of the center and the bottom dock, the center keeping most.
    let Some(DockNode::Split { parts, .. }) = app.dock.saved() else {
        panic!("a split layout");
    };
    let Some(DockNode::Split {
        across: false,
        shares,
        parts: column,
    }) = parts.first()
    else {
        panic!("the center split above the terminals: {parts:?}");
    };
    let names: Vec<_> = column
        .iter()
        .map(|part| match part {
            DockNode::Tabs { name, .. } => name.clone(),
            DockNode::Split { .. } => None,
        })
        .collect();
    assert_eq!(
        names,
        [Some(harlo::CENTER.into()), Some(harlo::BOTTOM.into())]
    );
    assert_eq!(shares, &[680, 320]);

    // The shell runs in the folder.
    type_into(&mut app, 0, "echo $((40 + 2)); pwd\r");
    assert!(wait_for(&app, 0, "42"), "the shell answers");
    let folder = scratch.path().canonicalize().expect("the folder");
    assert!(wait_for(&app, 0, &folder.to_string_lossy()));
}

#[test]
fn hidden_shells_keep_running_and_come_back() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let mut app = app(scratch.path());
    toggle(&mut app);
    let _ = app.update(Message::Terminal(TerminalEvent::New));
    assert_eq!(docked(&app), [DockKey::Terminal(0), DockKey::Terminal(1)]);
    type_into(&mut app, 0, "echo before\r");
    assert!(wait_for(&app, 0, "before"));

    toggle(&mut app);
    assert!(!app.terminals_shown());
    assert!(docked(&app).is_empty());
    assert_eq!(app.terminals.shells().len(), 2, "still running");
    type_into(&mut app, 0, "echo while hidden\r");
    assert!(wait_for(&app, 0, "while hidden"));

    toggle(&mut app);
    assert_eq!(docked(&app), [DockKey::Terminal(0), DockKey::Terminal(1)]);
    assert!(
        app.dock.is_shown(DockKey::Terminal(1)),
        "the last one in front"
    );
}

#[test]
fn closing_a_terminal_ends_its_shell() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let mut app = app(scratch.path());
    toggle(&mut app);
    let _ = app.update(Message::Terminal(TerminalEvent::New));
    let _ = app.update(Message::CloseDocked(DockKey::Terminal(1)));
    assert_eq!(docked(&app), [DockKey::Terminal(0)]);
    assert!(app.terminals.get(1).is_none());
    assert!(app.dock.is_shown(DockKey::Terminal(0)));
}

#[test]
fn no_shell_runs_until_asked() {
    let app = demo::app();
    assert!(app.terminals.shells().is_empty());
    assert!(docked(&app).is_empty());
    assert!(!app.terminals_shown());
}

#[test]
fn a_terminal_tab_is_named_after_its_folder() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let mut app = app(scratch.path());
    toggle(&mut app);
    let name = scratch
        .path()
        .file_name()
        .expect("a folder name")
        .to_string_lossy();
    assert!(shows(&app, &name));
}
