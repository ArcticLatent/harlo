//! Terminals in the strip under the center: a real shell that answers what's
//! typed into the window, its colours, scrolling back, selecting, and the
//! tab closing when the shell exits.
//!
//! The shells are `/bin/sh` in a scratch folder outside the repository,
//! never the user's own shell or its startup files.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::Path;
use std::time::Duration;

use alacritty_terminal::term::TermMode;
use harlo::app::{App, Message, TerminalEvent};
use harlo::term::{self, Key, Mods, Palette};
use harlo::ui::{Logic, State};
use slint::ComponentHandle as _;
use slint::Model as _;
use slint::platform::{Key as SlintKey, PointerEventButton, WindowEvent};
use slint::{LogicalPosition, SharedString};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// How long a shell gets to answer.
const ANSWER: Duration = Duration::from_secs(10);

/// The demo app, with terminals running `/bin/sh` in `folder`.
fn app(folder: &Path) -> App {
    demo::app().terminal_shell("/bin/sh", folder.to_string_lossy())
}

/// The shown terminal's visible lines.
fn lines(ui: &Ui) -> Vec<String> {
    ui.app()
        .terminals
        .active()
        .map(|shell| shell.screen.text())
        .unwrap_or_default()
}

/// Whether the shown terminal has a line that is exactly `text`.
fn has_line(ui: &Ui, text: &str) -> bool {
    lines(ui).iter().any(|line| line == text)
}

/// Clicks into the terminal, so keys go to it.
fn click_in(ui: &Ui) {
    let screen = ui
        .find("Terminal screen")
        .into_iter()
        .next()
        .expect("the terminal");
    let at = screen.absolute_position();
    let size = screen.size();
    let position = LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0);
    let button = PointerEventButton::Left;
    let window = ui.window.window();
    window.dispatch_event(WindowEvent::PointerPressed { position, button });
    window.dispatch_event(WindowEvent::PointerReleased { position, button });
}

/// Presses and lets go of the key whose text is `text`.
fn key(ui: &Ui, text: impl Into<SharedString>) {
    let text = text.into();
    let window = ui.window.window();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

/// Types `text` into the window, a key at a time; `\n` is Enter.
fn type_text(ui: &Ui, text: &str) {
    for c in text.chars() {
        key(ui, SharedString::from(c.to_string()));
    }
}

/// Holds Control while pressing `c`.
fn control(ui: &Ui, c: &str) {
    let window = ui.window.window();
    let ctrl = SharedString::from(SlintKey::Control);
    window.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
    key(ui, c);
    window.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
}

/// A window with a shell started from the rail and sized to the strip.
fn started(folder: &Path) -> (Ui, Tasks) {
    let ui = ui(app(folder));
    let mut tasks = Tasks::new();
    ui.press("Terminal");
    assert!(ui.app().terminals_shown());
    assert!(
        tasks.until(&ui, ANSWER, |ui| !lines(ui).concat().is_empty()),
        "the shell prompts"
    );
    click_in(&ui);
    (ui, tasks)
}

#[test]
fn a_shell_answers_what_is_typed_into_the_window() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let (ui, mut tasks) = started(scratch.path());
    type_text(&ui, "echo $((6*7))\n");
    assert!(
        tasks.until(&ui, ANSWER, |ui| has_line(ui, "42")),
        "{:?}",
        lines(&ui)
    );
    assert!(ui.shows("42"), "the window draws it");

    // The shell knows the strip's size, not the size it started at.
    type_text(&ui, "stty size\n");
    let size = |ui: &Ui| {
        lines(ui).iter().find_map(|line| {
            let (rows, columns) = line.split_once(' ')?;
            Some((rows.parse::<usize>().ok()?, columns.parse::<usize>().ok()?))
        })
    };
    assert!(tasks.until(&ui, ANSWER, |ui| size(ui).is_some()));
    let (rows, columns) = size(&ui).expect("the size");
    assert!(rows < 24, "the strip is shorter: {rows}");
    assert!(columns > 80, "and wider: {columns}");
    let shown = ui.window.global::<State>().get_term_lines().row_count();
    assert_eq!(shown, rows, "the window draws every line");
}

#[test]
fn control_c_interrupts_and_exit_closes_the_tab() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let (ui, mut tasks) = started(scratch.path());
    type_text(&ui, "sleep 30\n");
    control(&ui, "c");
    type_text(&ui, "echo do''ne\n");
    assert!(
        tasks.until(&ui, ANSWER, |ui| has_line(ui, "done")),
        "not waiting 30 seconds: {:?}",
        lines(&ui)
    );

    type_text(&ui, "exit\n");
    assert!(
        tasks.until(&ui, ANSWER, |ui| ui.app().terminals.shells().is_empty()),
        "the tab closes with the shell"
    );
    assert!(!ui.app().terminals_shown());
    assert_eq!(ui.app().send_error, None, "an exit isn't an error");
}

#[test]
fn the_view_scrolls_back_and_typing_returns_to_the_end() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let (ui, mut tasks) = started(scratch.path());
    type_text(&ui, "seq 1 300\n");
    assert!(tasks.until(&ui, ANSWER, |ui| has_line(ui, "300")));

    ui.send(Message::Terminal(TerminalEvent::Scroll(100)));
    assert!(!has_line(&ui, "300"), "scrolled back");
    assert!(ui.window.global::<State>().get_term_scrolled());

    type_text(&ui, "x");
    assert!(has_line(&ui, "300") || lines(&ui).iter().any(|l| l.ends_with('x')));
    assert!(!ui.window.global::<State>().get_term_scrolled());
}

#[test]
fn a_selection_is_copied_as_text() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let (ui, mut tasks) = started(scratch.path());
    type_text(&ui, "echo alpha beta\n");
    assert!(tasks.until(&ui, ANSWER, |ui| has_line(ui, "alpha beta")));
    let line = lines(&ui)
        .iter()
        .position(|l| l == "alpha beta")
        .expect("the answer");

    // A drag from the first cell's left half to the fifth cell's right half.
    ui.send(Message::Terminal(TerminalEvent::SelectFrom {
        line,
        column: 0.2,
        words: false,
    }));
    ui.send(Message::Terminal(TerminalEvent::SelectTo(line, 4.8)));
    let copied = ui.window.global::<Logic>().invoke_terminal_selection();
    assert_eq!(copied, "alpha");

    // A double click takes the word.
    ui.send(Message::Terminal(TerminalEvent::SelectFrom {
        line,
        column: 7.5,
        words: true,
    }));
    ui.send(Message::Terminal(TerminalEvent::SelectTo(line, 7.5)));
    let copied = ui.window.global::<Logic>().invoke_terminal_selection();
    assert_eq!(copied, "beta");

    // Typing drops it.
    type_text(&ui, " ");
    assert_eq!(ui.window.global::<Logic>().invoke_terminal_selection(), "");
}

#[test]
fn colours_come_from_the_theme() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let (ui, mut tasks) = started(scratch.path());
    type_text(
        &ui,
        "printf '\\033[31mred\\033[0m \\033[1mbold\\033[0m\\n'\n",
    );
    assert!(tasks.until(&ui, ANSWER, |ui| has_line(ui, "red bold")));
    let app = ui.app();
    let palette = Palette::new(&app.tokens);
    let view = app
        .terminals
        .active()
        .expect("the terminal")
        .screen
        .view(&palette);
    let runs: Vec<_> = view.lines.iter().flatten().collect();
    let red = runs.iter().find(|r| r.text == "red").expect("the red run");
    let t = app.tokens;
    let critical = t.icon_critical_base;
    assert!(
        (red.style.fg.r - critical.r).abs() < 0.02 && (red.style.fg.g - critical.g).abs() < 0.02,
        "red is the theme's critical colour"
    );
    let bold = runs
        .iter()
        .find(|r| r.text == "bold")
        .expect("the bold run");
    assert!(bold.style.bold);
    assert!(!red.style.bold);
}

#[test]
fn the_rail_button_hides_and_shows_the_running_shell() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let (ui, mut tasks) = started(scratch.path());
    type_text(&ui, "echo kept\n");
    assert!(tasks.until(&ui, ANSWER, |ui| has_line(ui, "kept")));
    ui.press("Hide terminals");
    assert!(!ui.app().terminals_shown());
    ui.press("Terminal");
    assert!(ui.app().terminals_shown());
    assert_eq!(ui.app().terminals.shells().len(), 1, "the same shell");
    assert!(
        tasks.until(&ui, ANSWER, |ui| ui.shows("kept")),
        "its screen is drawn again"
    );
}

// Keys, as the terminal sends them.

fn sent(key: &Key, mods: Mods) -> Vec<u8> {
    term::encode(key, mods, TermMode::empty()).expect("the key sends something")
}

const CTRL: Mods = Mods {
    ctrl: true,
    alt: false,
    shift: false,
};

#[test]
fn keys_send_what_shells_expect() {
    let none = Mods::default();
    assert_eq!(sent(&Key::Text("c".into()), CTRL), [3]);
    assert_eq!(sent(&Key::Text("C".into()), CTRL), [3], "shifted too");
    assert_eq!(
        sent(&Key::Text("x".into()), Mods { alt: true, ..none }),
        b"\x1bx"
    );
    assert_eq!(sent(&Key::Enter, none), b"\r");
    assert_eq!(sent(&Key::Backspace, none), [0x7f]);
    assert_eq!(
        sent(
            &Key::Tab,
            Mods {
                shift: true,
                ..none
            }
        ),
        b"\x1b[Z"
    );
    assert_eq!(sent(&Key::Up, none), b"\x1b[A");
    assert_eq!(sent(&Key::Up, CTRL), b"\x1b[1;5A");
    assert_eq!(
        term::encode(&Key::Up, none, TermMode::APP_CURSOR),
        Some(b"\x1bOA".to_vec()),
        "a full-screen program's cursor keys"
    );
    assert_eq!(sent(&Key::F(1), none), b"\x1bOP");
    assert_eq!(sent(&Key::F(5), none), b"\x1b[15~");
    assert_eq!(sent(&Key::Delete, none), b"\x1b[3~");
    assert_eq!(sent(&Key::Text("é".into()), none), "é".as_bytes());
}

#[test]
fn the_windows_key_text_names_the_keys() {
    let named = |key: SlintKey| Key::from_text(&SharedString::from(key));
    assert_eq!(named(SlintKey::UpArrow), Some(Key::Up));
    assert_eq!(named(SlintKey::Return), Some(Key::Enter));
    assert_eq!(named(SlintKey::F12), Some(Key::F(12)));
    assert_eq!(named(SlintKey::PageDown), Some(Key::PageDown));
    assert_eq!(
        named(SlintKey::Shift),
        None,
        "a modifier alone sends nothing"
    );
    assert_eq!(named(SlintKey::Menu), None);
    assert_eq!(Key::from_text("a"), Some(Key::Text("a".into())));
}
