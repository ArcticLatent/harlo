//! The docked main area: the conversation anchored in the center, tools
//! around it, moving around from the keyboard, maximizing a tool, and the
//! arrangement kept across restarts.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::Path;

use harlo::{App, DockKey, DockMove, FilesEvent, Message, Panel, ids};
use harlo_core::server::Location;
use iced::keyboard::{self, Key, Modifiers, key};
use iced::widget::Id;
use iced_code_editor::Message as EditorMessage;
use iced_dock::Direction;
use support::{demo, interact, shows};

const PI_UPDATE: &str = "ses_demo_03";
const HI: &str = "ses_demo_00";

/// The demo app with two sessions open, `HI` current.
fn two_open() -> App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let _ = app.update(Message::OpenSession(HI.into()));
    let _ = app.update(Message::History(HI.into(), Ok(Vec::new())));
    app
}

/// Draws the window once, as a frame on screen would: moving between
/// panes goes by where they're drawn.
fn draw(app: &App) {
    let mut ui = support::ui(app);
    let _ = ui.snapshot(&iced::Theme::Dark);
}

/// The sessions in the center, in tab order.
fn center(app: &App) -> Vec<DockKey> {
    app.dock.center()
}

/// Escape, as the keyboard reports it.
fn escape() -> keyboard::Event {
    let key = Key::Named(key::Named::Escape);
    keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        text: None,
    }
}

#[test]
fn the_start_screen_is_a_tab_until_a_session_opens() {
    let mut app = demo::app();
    assert!(app.dock.has(DockKey::Home));
    let _ = app.update(Message::OpenSession(HI.into()));
    assert!(!app.dock.has(DockKey::Home), "a session takes its place");
    let _ = app.update(Message::NewSession(None));
    assert!(app.dock.has(DockKey::Home));
    assert!(app.dock.is_shown(DockKey::Home));
}

#[test]
fn a_new_session_tab_opens_after_the_current_one() {
    let mut app = two_open();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let third = "ses_demo_05";
    let _ = app.update(Message::OpenSession(third.into()));
    let keys: Vec<DockKey> = [PI_UPDATE, third, HI]
        .iter()
        .map(|id| app.dock.session_key(id))
        .collect();
    let documents: Vec<DockKey> = app
        .dock
        .keys()
        .into_iter()
        .filter(|key| matches!(key, DockKey::Session(_)))
        .collect();
    assert_eq!(documents, keys);
}

#[test]
fn the_conversation_stays_in_one_central_pane() {
    let mut app = two_open();
    let hi = app.dock.session_key(HI);
    let pi = app.dock.session_key(PI_UPDATE);
    draw(&app);
    // A session can't split off or be carried out of the center.
    let _ = app.update(Message::DockMove(DockMove::Split(Direction::Right)));
    draw(&app);
    let _ = app.update(Message::DockMove(DockMove::Carry(Direction::Right)));
    assert_eq!(center(&app), [pi, hi]);
    assert!(app.dock.is_shown(hi));
    assert!(!app.dock.is_shown(pi), "one session on screen at a time");
    assert_eq!(app.active.as_deref(), Some(HI));
}

#[test]
fn the_keyboard_moves_focus_between_the_center_and_the_dock() {
    let mut app = two_open();
    draw(&app);
    let _ = app.update(Message::DockMove(DockMove::Focus(Direction::Right)));
    assert_eq!(app.dock.focused(), Some(DockKey::Panel(Panel::Changes)));
    assert_eq!(
        app.active.as_deref(),
        Some(HI),
        "the composer still writes to it"
    );

    // A tool can't be carried into the center.
    draw(&app);
    let _ = app.update(Message::DockMove(DockMove::Carry(Direction::Left)));
    assert!(!center(&app).contains(&DockKey::Panel(Panel::Changes)));
    assert!(app.panel_shown(Panel::Changes));

    draw(&app);
    let _ = app.update(Message::DockMove(DockMove::Focus(Direction::Left)));
    assert_eq!(app.dock.focused(), Some(app.dock.session_key(HI)));
}

#[test]
fn cycling_goes_through_the_focused_panes_tabs() {
    let mut app = two_open();
    let _ = app.update(Message::CycleTab { back: false });
    assert_eq!(app.active.as_deref(), Some(PI_UPDATE), "it wraps around");
    let _ = app.update(Message::CycleTab { back: true });
    assert_eq!(app.active.as_deref(), Some(HI));
}

#[test]
fn a_dirty_file_asks_before_its_tab_closes() {
    let mut app = two_open();
    let root = app.file_tree().0;
    let _ = app.update(Message::Files(FilesEvent::Open("notes.txt".into())));
    assert!(app.dock.has(DockKey::Editor));
    let key = Path::new(&root)
        .join("notes.txt")
        .to_string_lossy()
        .into_owned();
    let _ = app.update(Message::Files(FilesEvent::Read {
        key: key.clone(),
        bytes: Ok(b"one\n".to_vec()),
    }));
    let _ = app.update(Message::Files(FilesEvent::Editor(
        key,
        EditorMessage::Paste("two ".into()),
    )));
    let _ = app.update(Message::CloseDocked(DockKey::Editor));
    assert!(app.dock.has(DockKey::Editor), "kept until the user answers");
    assert!(shows(&app, "Don't save"));
}

#[test]
fn the_arrangement_comes_back_after_a_restart() {
    let mut app = two_open();
    let _ = app.update(Message::OpenPanel(Panel::Context));
    let _ = app.update(Message::CloseDocked(DockKey::Panel(Panel::Notes)));
    let saved = app.layout();
    let arrangement = app.dock.saved();
    assert!(arrangement.is_some());

    let mut again = App::new(Location::Shared, vec![Location::Shared], saved);
    let _ = again.update(Message::Tick(demo::now()));
    let _ = again.update(demo::connected(demo::snapshot()));
    assert_eq!(again.dock.saved(), arrangement);
    assert_eq!(again.active.as_deref(), Some(HI));
    assert!(again.panel_shown(Panel::Context));
    assert!(again.panel_open(Panel::Changes));
    assert!(!again.panel_open(Panel::Notes));

    // A restored tool stays when the rail opens another.
    let _ = again.update(Message::OpenPanel(Panel::Todos));
    assert!(again.panel_open(Panel::Context));
}

#[test]
fn sessions_gone_while_away_leave_the_arrangement() {
    let app = two_open();
    let saved = app.layout();
    // The server no longer has the session on the left.
    let mut snapshot = demo::snapshot();
    snapshot.sessions.retain(|s| s.id != PI_UPDATE);
    let mut again = App::new(Location::Shared, vec![Location::Shared], saved);
    let _ = again.update(Message::Tick(demo::now()));
    let _ = again.update(demo::connected(snapshot));
    assert_eq!(again.tabs, [HI]);
    let pi = again.dock.session_key(PI_UPDATE);
    let hi = again.dock.session_key(HI);
    assert!(!again.dock.has(pi));
    assert!(again.dock.is_shown(hi));
}

#[test]
fn a_maximized_tool_restores_the_window_as_it_was() {
    let mut app = two_open();
    let _ = app.update(Message::OpenPanel(Panel::Notes));
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.typewrite("half a re");
    });
    let arrangement = app.dock.saved();

    let _ = app.update(Message::Maximize(Panel::Changes));
    assert_eq!(app.maximized, Some(Panel::Changes));
    assert!(app.panel_shown(Panel::Changes));
    assert!(!app.panel_shown(Panel::Notes), "covered");
    assert!(shows(&app, "Restore"));

    // Escape restores it, and only while something is maximized.
    let restore = harlo::app::restore_key(escape()).expect("Escape restores");
    let _ = app.update(restore);
    assert_eq!(app.maximized, None);
    assert!(app.panel_shown(Panel::Notes));
    assert_eq!(app.dock.saved(), arrangement, "every pane where it was");
    assert_eq!(app.active.as_deref(), Some(HI));
    assert_eq!(app.composer.text().trim_end(), "half a re");
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.typewrite("ply");
    });
    assert_eq!(app.composer.text().trim_end(), "half a reply");
}
