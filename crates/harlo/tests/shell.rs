//! The window shell: what it shows on start, and how its controls respond.
//!
//! Each test renders the real view headless, clicks or types into it, and
//! feeds the resulting messages back through `App::update`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::{App, Message, Panel, ids};
use iced::widget::Id;
use std::time::Duration;
use support::{demo, interact, shows, ui};

use harlo::{Status, status_text};
use harlo_core::layout::Layout;
use harlo_core::server::{Location, Update};
use harlo_core::timeline::Entry;
use harlo_core::workspace::{Event as ServerEvent, EventKind, Snapshot, ToolStatus};
use iced::{Event, Point, mouse};
use iced_test::Error;

#[test]
fn start_shows_the_design_content() -> Result<(), Error> {
    let app = demo::app();
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
        "Enter to start · Shift+Enter for a new line",
        "Context",
        "No session selected.",
    ] {
        let _ = ui.find(text)?;
    }
    for id in [harlo::ids::FOLDER, harlo::ids::MODEL, harlo::ids::ATTACH] {
        let _ = ui.find(iced::widget::Id::new(id))?;
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
    let app = demo::app();
    assert!(shows(
        &app,
        "Write this project's specs: SPEC.md files that hold its intent between sessions"
    ));
}

#[test]
fn show_more_lists_every_session_then_fewer_again() {
    let mut app = demo::app();
    let messages = interact(&mut app, |ui| {
        ui.click("Show 19 more").expect("the show-more button");
    });
    assert!(
        matches!(messages.as_slice(), [Message::ToggleMore(folder)] if folder == demo::HOME),
        "{messages:?}"
    );
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
    let mut app = demo::app();
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
    let mut app = demo::app();
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
    let mut app = demo::app();
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
    support::settle(&mut app);
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::PANEL_CLOSE))
            .expect("the close button");
    });
    assert_eq!(app.panel, None);
    assert!(!shows(&app, "No session selected."));
}

#[test]
fn the_composer_accepts_text() {
    let mut app = demo::app();
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

#[test]
fn before_connecting_the_sidebar_says_so() {
    let mut app = App::new(Location::Shared, vec![Location::Shared], Layout::default());
    assert!(shows(&app, "Connecting to OpenCode…"));

    let _ = app.update(Message::Server(Update::Lost {
        reason: "can't reach the server".into(),
        retry_in: Duration::from_secs(4),
    }));
    assert!(shows(&app, "Can't connect: can't reach the server."));

    let _ = app.update(demo::connected(Snapshot::default()));
    assert!(shows(&app, "No projects yet. Add a folder with + Project."));
}

#[test]
fn the_location_picker_shows_servers_and_the_connection() {
    let mut app = demo::app();
    let _ = interact(&mut app, |ui| {
        ui.click(Id::new(ids::LOCATION))
            .expect("the location button");
    });
    assert!(app.picking_location);
    assert!(shows(
        &app,
        &format!("Connected · OpenCode {}", harlo_core::OPENCODE_VERSION)
    ));
    assert!(shows(
        &app,
        "OpenCode's background service on this computer"
    ));

    // Clicking outside the card closes it.
    let _ = interact(&mut app, |ui| {
        ui.click(Point::new(900.0, 700.0)).expect("the backdrop");
    });
    assert!(!app.picking_location);
}

#[test]
fn picking_another_server_switches_and_reconnects() {
    let server = Location::Server {
        url: "http://127.0.0.1:4096".into(),
        password: None,
    };
    let mut app = App::new(
        Location::Shared,
        vec![Location::Shared, server.clone()],
        demo::layout(),
    );
    let _ = app.update(demo::connected(demo::snapshot()));
    let _ = app.update(Message::ToggleLocations);
    let _ = interact(&mut app, |ui| {
        ui.click("127.0.0.1:4096").expect("the other server");
    });
    assert_eq!(app.location, server);
    assert_eq!(app.status, Status::Connecting { attempt: 1 });
    // The sidebar keeps the user's folders but drops the old server's sessions.
    assert!(!shows(&app, "hi how are you"));
    assert!(shows(&app, "ArcticLatent.github.io"));
}

#[test]
fn status_text_covers_every_state() {
    let pinned = harlo_core::OPENCODE_VERSION;
    assert_eq!(
        status_text(&Status::Connecting { attempt: 1 }),
        "Connecting…"
    );
    assert_eq!(
        status_text(&Status::Connecting { attempt: 3 }),
        "Reconnecting (attempt 3)…"
    );
    assert_eq!(
        status_text(&Status::Connected {
            version: "9.9.9".into()
        }),
        format!("Connected · OpenCode 9.9.9. Harlo is tested with {pinned}.")
    );
    assert_eq!(
        status_text(&Status::Lost {
            reason: "the server rejected the password".into(),
            retry_in: Duration::from_secs(2),
        }),
        "Can't connect: the server rejected the password. Trying again in 2 s."
    );
}

#[test]
fn live_events_update_the_sidebar() {
    let mut app = demo::app();
    let event = |kind| {
        Message::Server(Update::Event(ServerEvent {
            id: "evt".into(),
            seq: None,
            time: 1_795_000_000_000.0,
            kind,
        }))
    };
    let _ = app.update(event(EventKind::SessionRenamed {
        session_id: "ses_demo_03".into(),
        title: "Renamed while open".into(),
    }));
    assert!(shows(&app, "Renamed while open"));
    assert!(!shows(&app, "pi update"));

    let _ = app.update(event(EventKind::SessionDeleted {
        session_id: "ses_demo_00".into(),
    }));
    assert!(!shows(&app, "hi how are you"));
    assert!(shows(&app, "30"), "the count follows");
    assert!(shows(&app, "Show 18 more"));
}

#[test]
fn clicking_a_session_opens_it() {
    let mut app = demo::app();
    let messages = interact(&mut app, |ui| {
        ui.click("pi update").expect("the session row");
    });
    assert!(
        matches!(messages.as_slice(), [Message::OpenSession(id)] if id == "ses_demo_03"),
        "{messages:?}"
    );
    assert_eq!(app.active.as_deref(), Some("ses_demo_03"));
    assert_eq!(app.active_title().as_deref(), Some("pi update"));
}

#[test]
fn worktrees_group_their_sessions_under_the_project() {
    let mut snapshot = demo::snapshot();
    let worktree = "/home/burce/.worktrees/feature-x";
    snapshot
        .worktrees
        .insert("p_home".into(), vec![demo::HOME.into(), worktree.into()]);
    snapshot.sessions[0].directory = worktree.into();
    snapshot.sessions[0].title = Some("Working in the worktree".into());
    let mut app = App::new(Location::Shared, vec![Location::Shared], demo::layout());
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(demo::connected(snapshot));
    assert!(shows(&app, "feature-x"));
    assert!(shows(&app, "Working in the worktree"));

    // The filter keeps the worktree heading when it has matches.
    let _ = app.update(Message::Filter("worktree".into()));
    assert!(shows(&app, "feature-x"));
    assert!(!shows(&app, "hi how are you"));
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
    let mut app = demo::app();
    let snapshot = ui(&app).snapshot(&app.theme())?;
    assert!(snapshot.matches_image(&path)?);

    // The same screen with the location picker open.
    let _ = app.update(Message::ToggleLocations);
    let snapshot = ui(&app).snapshot(&app.theme())?;
    assert!(snapshot.matches_image(path.with_file_name(format!("{stem}-picker.png")))?);

    // An open session among five tabs, with a history like the design's.
    let _ = app.update(Message::ToggleLocations);
    for id in ["00", "01", "02", "03", "04"] {
        let _ = app.update(Message::OpenSession(format!("ses_demo_{id}")));
    }
    let _ = app.update(Message::History("ses_demo_04".into(), Ok(design_history())));
    // The design shows its tool group opened.
    let _ = app.update(Message::Timeline(
        "ses_demo_04".into(),
        harlo::timeline::Event::ToggleGroup("m2/2".into()),
    ));
    let snapshot = ui(&app).snapshot(&app.theme())?;
    assert!(snapshot.matches_image(path.with_file_name(format!("{stem}-session.png")))?);
    Ok(())
}

/// A history like the one in the session design.
fn design_history() -> Vec<Entry> {
    let tool = |key: &str, name: &str, detail: &str, secs: f64| Entry::Tool {
        key: key.into(),
        name: name.into(),
        label: harlo_core::timeline::tool_label(name),
        detail: detail.into(),
        status: ToolStatus::Completed,
        took: Some(Duration::from_secs_f64(secs)),
    };
    vec![
        Entry::Prompt {
            key: "m1".into(),
            text:
                "Write this project's specs: SPEC.md files that hold its intent between sessions. \
                   Start with the connectivity check."
                    .into(),
            attachments: Vec::new(),
            skills: Vec::new(),
        },
        Entry::Thought {
            key: "m2/0".into(),
            text: "**Updating README connectivity notes**".into(),
        },
        Entry::Answer {
            key: "m2/1".into(),
            text: "I'll change the check to port `9877`, rename its output key to \
                   `Blender9877_tcp`, and update the docs. I'll leave historical results \
                   and the Blender addon configuration unchanged."
                .into(),
        },
        tool("m2/2", "edit", "SPEC.md", 15.0),
        tool("m2/3", "edit", "README.md", 15.0),
        tool("m2/4", "bash", "python3 - <<'PY'", 17.0),
        tool("m2/5", "todowrite", "4 items", 3.7),
        Entry::Answer {
            key: "m2/6".into(),
            text: "Changed `tests/check_connections.py` to check Blender on **127.0.0.1:9877** \
                   and report `Blender9877_tcp`.\n\nUpdated `tests/SPEC.md`, root `SPEC.md`, \
                   and `README.md`. Historical results remain unchanged.\n\nMocked \
                   reachable/unreachable checks passed; no live services were contacted. \
                   This does **not** reconfigure the Blender addon's listening port."
                .into(),
        },
    ]
}
