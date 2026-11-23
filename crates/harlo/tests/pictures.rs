//! Pictures: thumbnails under prompts and under the tools that returned
//! them, in the composer, the session's grid, and the viewer.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::Arc;
use std::time::Duration;

use harlo::app::{Message, PictureEvent, Thumb};
use harlo::task::{Action, Effect};
use harlo_core::server::{Attachment, Picture};
use harlo_core::timeline::Entry;
use harlo_core::workspace::ToolStatus;
use slint::platform::WindowEvent;
use slint::{ComponentHandle as _, Model as _};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

const PI_UPDATE: &str = "ses_demo_03";
const WAIT: Duration = Duration::from_secs(10);

/// A PNG `width` by `height` pixels, all one colour.
fn png(width: u32, height: u32) -> Vec<u8> {
    let picture = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 60, 40, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    picture
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("a PNG");
    out.into_inner()
}

/// `data` as a picture named `name`, as the client reads one.
fn picture(name: &str, data: &[u8]) -> Picture {
    Attachment::inline("image/png", data, name)
        .picture()
        .expect("a picture")
}

/// Presses Escape.
fn escape(ui: &Ui) {
    let text = slint::SharedString::from(slint::platform::Key::Escape);
    let window = ui.window.window();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

/// The window with "pi update" open: a prompt with a picture, and a tool
/// that returned one.
fn with_pictures() -> Ui {
    shown(pictures_app())
}

/// The window showing `app`, then the history with pictures, whose
/// thumbnails the window's tasks decode.
fn shown(app: harlo::app::App) -> Ui {
    let ui = ui(app);
    ui.send(Message::History(PI_UPDATE.into(), Ok(history())));
    ui
}

/// "pi update" open, its history not yet in.
fn pictures_app() -> harlo::app::App {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    app
}

/// A prompt with a picture, and a tool that returned one.
fn history() -> Vec<Entry> {
    vec![
        Entry::Prompt {
            key: "msg_1".into(),
            text: "What's wrong here?".into(),
            attachments: vec!["shot.png".into()],
            pictures: vec![picture("shot.png", &png(30, 20))],
            skills: Vec::new(),
            agent: None,
        },
        Entry::Tool {
            key: "msg_2/tool/0".into(),
            name: "read".into(),
            label: "Read".into(),
            detail: "plot.png".into(),
            status: ToolStatus::Completed,
            took: None,
            files: Vec::new(),
            todos: None,
            pictures: vec![picture("plot.png", &png(60, 20))],
        },
    ]
}

/// Runs tasks until every thumbnail is decoded.
fn decoded(ui: &Ui, tasks: &mut Tasks) {
    let ready = |ui: &Ui| {
        let app = ui.app();
        !app.pictures.thumbs.is_empty()
            && app
                .pictures
                .thumbs
                .values()
                .all(|t| matches!(t, Thumb::Ready(_)))
    };
    assert!(tasks.until(ui, WAIT, ready), "thumbnails decode");
}

#[test]
fn prompts_and_tools_show_their_pictures_as_thumbnails() {
    let ui = with_pictures();
    let mut tasks = Tasks::new();
    decoded(&ui, &mut tasks);
    for name in ["shot.png", "plot.png"] {
        assert!(!ui.find(&format!("Open {name}")).is_empty(), "{name}");
    }
    assert_eq!(
        ui.button("Open shot.png")
            .accessible_description()
            .as_deref(),
        Some("You sent it")
    );
    assert_eq!(
        ui.button("Open plot.png")
            .accessible_description()
            .as_deref(),
        Some("The agent sent it")
    );
    assert!(
        !ui.shows("shot.png"),
        "a picture shows as itself, not as a file chip"
    );
    // The thumbnails were filled in after the rows were built.
    let state = ui.window.global::<harlo::ui::State>();
    let thumbs = state.get_thumbs();
    let sizes: Vec<_> = (0..thumbs.row_count())
        .filter_map(|i| thumbs.row_data(i))
        .map(|image| (image.size().width, image.size().height))
        .collect();
    assert!(sizes.contains(&(30, 20)), "{sizes:?}");
    assert!(sizes.contains(&(60, 20)), "{sizes:?}");
}

#[test]
fn the_viewer_shows_a_picture_and_turns_and_zooms_it() {
    let ui = with_pictures();
    let mut tasks = Tasks::new();
    decoded(&ui, &mut tasks);
    ui.press("Open plot.png");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("60 × 20")),
        "the whole picture decodes and its size shows"
    );
    assert!(ui.shows("The agent sent"));
    assert!(ui.shows("plot.png"));
    assert!(
        ui.shows("Zoom 100%"),
        "a small picture opens at its own size"
    );

    ui.press("Zoom in");
    assert!(ui.shows("Zoom 125%"));
    ui.press("Zoom out");
    ui.press("Zoom out");
    assert!(ui.shows("Zoom 80%"));
    ui.press("Actual size");
    assert!(ui.shows("Zoom 100%"));

    ui.press("Turn right");
    assert!(ui.shows("20 × 60"), "turned, it's tall");
    ui.press("Turn left");
    assert!(ui.shows("60 × 20"));

    ui.press("Close the picture");
    assert!(ui.app().pictures.viewing.is_none());
    assert!(ui.find("Picture viewer").is_empty());
}

#[test]
fn the_pictures_link_lists_every_picture_and_escape_closes_in_turn() {
    let ui = with_pictures();
    let mut tasks = Tasks::new();
    decoded(&ui, &mut tasks);
    assert_eq!(
        ui.button("Pictures").accessible_description().as_deref(),
        Some("2")
    );
    ui.press("Pictures");
    assert!(ui.shows("2 in this session"));
    assert_eq!(
        ui.find("Open plot.png").len(),
        2,
        "in the timeline and in the grid"
    );

    // From the grid, the viewer; Escape closes it, then the grid.
    let grid = ui.find("Open shot.png");
    assert!(!grid.is_empty());
    grid.last()
        .expect("in the grid")
        .invoke_accessible_default_action();
    assert!(tasks.until(&ui, WAIT, |ui| ui.shows("30 × 20")));
    assert!(ui.shows("You sent"));
    escape(&ui);
    assert!(ui.app().pictures.viewing.is_none(), "the viewer closes");
    assert!(ui.app().pictures.grid, "the grid stays");
    escape(&ui);
    assert!(!ui.app().pictures.grid, "then the grid closes");
}

#[test]
fn an_empty_session_says_it_has_no_pictures() {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let ui = ui(app);
    ui.press("Pictures");
    assert!(ui.shows("0 in this session"));
    assert!(ui.shows(
        "No pictures in this session yet. Paste one into your message, or ask the agent for one."
    ));
}

#[test]
fn a_pasted_picture_shows_as_a_thumbnail_in_the_composer() {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let _ = app.update(Message::History(PI_UPDATE.into(), Ok(Vec::new())));
    let ui = ui(app);
    let mut tasks = Tasks::new();
    ui.send(Message::Pasted(Some(png(16, 16))));
    decoded(&ui, &mut tasks);
    assert!(!ui.find("Open pasted-image-1.png").is_empty());
    assert!(!ui.find("Remove pasted-image-1.png").is_empty());
    ui.press("Open pasted-image-1.png");
    assert!(tasks.until(&ui, WAIT, |ui| ui.shows("16 × 16")));
    assert!(ui.shows("You sent"));
}

#[test]
fn show_in_folder_saves_the_picture_and_opens_its_folder() {
    let scratch = tempfile::tempdir().expect("scratch folder");
    let mut app = pictures_app();
    app.pictures.folder = Some(scratch.path().to_owned());
    let ui = shown(app);
    let mut tasks = Tasks::new();
    decoded(&ui, &mut tasks);
    ui.press("Open plot.png");
    let _ = ui.driver.take_tasks();
    ui.press("Show in folder");
    // The save runs; the folder is only asked for, never opened here.
    let run = |ui: &Ui| -> Vec<Action<Message>> {
        ui.driver
            .take_tasks()
            .into_iter()
            .filter_map(harlo::task::Task::into_stream)
            .flat_map(|s| tasks.block_on(futures_util::StreamExt::collect::<Vec<_>>(s)))
            .collect()
    };
    let saved = run(&ui)
        .into_iter()
        .find_map(|a| match a {
            Action::Output(m @ Message::Pictures(PictureEvent::Saved(_))) => Some(m),
            _ => None,
        })
        .expect("saved");
    ui.send(saved);
    let path = run(&ui)
        .into_iter()
        .find_map(|a| match a {
            Action::Effect(Effect::ShowInFolder(path)) => Some(path),
            _ => None,
        })
        .expect("its folder is asked for");
    assert!(path.starts_with(scratch.path()), "{path:?}");
    assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("plot.png"));
    let saved = std::fs::read(&path).expect("saved");
    assert_eq!(
        Arc::<[u8]>::from(saved),
        picture("plot.png", &png(60, 20)).data
    );
}
