//! The window shell: what it shows on start, and how its controls respond.
//!
//! Each test renders the real view headless, clicks or types into it, and
//! feeds the resulting messages back through `App::update`.

// Rust guideline compliant 2026-10-09

use std::borrow::Cow;

use harlo::{App, Message, Panel, demo, ids, style};
use iced::widget::Id;
use iced::{Event, mouse};
use iced_test::{Error, Simulator};

/// The window size the design was drawn at.
const WINDOW: (f32, f32) = (1588.0, 947.0);

fn settings() -> iced::Settings {
    iced::Settings {
        fonts: style::FONTS
            .iter()
            .map(|font| Cow::Borrowed(*font))
            .collect(),
        default_font: style::SANS,
        ..iced::Settings::default()
    }
}

fn ui(app: &App) -> Simulator<'_, Message> {
    Simulator::with_size(settings(), WINDOW, app.view())
}

/// Runs `interact` against the current view, then applies the messages it produced.
fn interact(app: &mut App, interact: impl FnOnce(&mut Simulator<'_, Message>)) -> Vec<Message> {
    let messages: Vec<Message> = {
        let mut ui = ui(app);
        interact(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages.clone() {
        let _ = app.update(message);
    }
    messages
}

fn shows(app: &App, text: &str) -> bool {
    ui(app).find(text).is_ok()
}

#[test]
fn start_shows_the_design_content() -> Result<(), Error> {
    let app = App::new(demo::projects());
    let mut ui = ui(&app);
    for text in [
        "Harlo",
        "+ Project",
        "Local",
        "New session",
        "Ctrl+N",
        "ArcticLatent.github.io",
        "harlo",
        "burce",
        "31",
        "hi how are you",
        "4h",
        "Show 19 more",
        "What are we working on?",
        "Home folder",
        "Default model",
        "Enter to start · Shift+Enter for a new line",
        "Context",
        "No session selected.",
    ] {
        let _ = ui.find(text)?;
    }
    // Only the first twelve sessions are listed.
    let _ = ui
        .find("Older session 1")
        .expect_err("hidden until expanded");
    Ok(())
}

#[test]
fn long_titles_keep_their_full_text() {
    // The row shortens the title on screen; the full title is what the view reports.
    let app = App::new(demo::projects());
    assert!(shows(
        &app,
        "Write this project's specs: SPEC.md files that hold its intent between sessions"
    ));
}

#[test]
fn show_more_lists_every_session_then_fewer_again() {
    let mut app = App::new(demo::projects());
    let messages = interact(&mut app, |ui| {
        ui.click("Show 19 more").expect("the show-more button");
    });
    assert!(matches!(messages.as_slice(), [Message::ToggleMore(2)]));
    assert!(shows(&app, "Older session 19"));
    assert!(shows(&app, "Show fewer"));

    // The button is now below the fold; scroll the list to reach it.
    let _ = interact(&mut app, |ui| {
        ui.point_at((200.0, 500.0));
        let _ = ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels {
                x: 0.0,
                y: -2_000.0,
            },
        })]);
        ui.click("Show fewer").expect("the show-fewer button");
    });
    assert!(!shows(&app, "Older session 19"));
    assert!(shows(&app, "Show 19 more"));
}

#[test]
fn the_filter_narrows_the_list() {
    let mut app = App::new(demo::projects());
    let messages = interact(&mut app, |ui| {
        ui.click(Id::new(ids::SESSION_FILTER)).expect("the filter");
        let _ = ui.typewrite("H");
    });
    assert!(
        matches!(messages.as_slice(), [Message::Filter(text)] if text == "H"),
        "{messages:?}"
    );

    let _ = app.update(Message::Filter("houdini".into()));
    assert!(shows(
        &app,
        "You are a senior Houdini programmer. Write a SOP"
    ));
    assert!(!shows(&app, "hi how are you"));
    // Projects without a match drop out; the one with matches stays.
    assert!(!shows(&app, "harlo"));
    assert!(shows(&app, "burce"));
}

#[test]
fn the_sidebar_toggles() {
    let mut app = App::new(demo::projects());
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::SIDEBAR_TOGGLE))
            .expect("the sidebar toggle");
    });
    assert!(!app.sidebar);
    assert!(!shows(&app, "New session"));
    assert!(shows(&app, "What are we working on?"));

    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::SIDEBAR_TOGGLE))
            .expect("the sidebar toggle");
    });
    assert!(shows(&app, "New session"));
}

#[test]
fn the_right_rail_switches_and_closes_panels() {
    let mut app = App::new(demo::projects());
    assert_eq!(app.panel, Some(Panel::Context));

    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(Panel::Todos.id()))
            .expect("the todos button");
    });
    assert_eq!(app.panel, Some(Panel::Todos));
    assert!(shows(&app, "Todos"));
    assert!(!shows(&app, "Context"));

    // The open panel's button closes it.
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(Panel::Todos.id()))
            .expect("the todos button");
    });
    assert_eq!(app.panel, None);

    let _ = app.update(Message::OpenPanel(Panel::Notes));
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::PANEL_CLOSE))
            .expect("the close button");
    });
    assert_eq!(app.panel, None);
    assert!(!shows(&app, "No session selected."));
}

#[test]
fn the_composer_accepts_text() {
    let mut app = App::new(demo::projects());
    let messages = interact(&mut app, |ui| {
        ui.click(Id::new(ids::COMPOSER)).expect("the composer");
        let _ = ui.typewrite("hi");
    });
    assert!(
        messages.iter().all(|m| matches!(m, Message::Compose(_))),
        "{messages:?}"
    );
    assert_eq!(app.composer.text().trim_end(), "hi");
}

/// Renders the start screen for comparing with the design by eye.
///
/// Run with `HARLO_SNAPSHOT=/path/shot.png cargo test -p harlo -- --ignored snapshot`;
/// the image lands beside that path, named with the renderer, such as `shot-wgpu.png`.
#[test]
#[ignore = "writes an image for a visual check; run on demand"]
fn snapshot() -> Result<(), Error> {
    let Some(path) = std::env::var_os("HARLO_SNAPSHOT").map(std::path::PathBuf::from) else {
        return Ok(());
    };
    // Replace earlier images rather than comparing against them.
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let folder = path.parent().unwrap_or(std::path::Path::new("."));
    for entry in std::fs::read_dir(folder)?.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{stem}-"))
        {
            std::fs::remove_file(entry.path())?;
        }
    }
    let app = App::new(demo::projects());
    let snapshot = ui(&app).snapshot(&app.theme())?;
    assert!(snapshot.matches_image(&path)?);
    Ok(())
}
