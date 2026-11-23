//! A server snapshot that reproduces the design mockup, for UI tests.

// Rust guideline compliant 2026-10-09

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use harlo::app::{App, Message};
use harlo_core::layout::Layout;
use harlo_core::server::{Client, Connected, Endpoint, Location, Update};
use harlo_core::workspace::{ModelRef, ServerProject, Session, Snapshot, Tokens};

const HOUR: u64 = 3_600;
const DAY: u64 = 24 * HOUR;

/// The moment the tests treat as now.
pub fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_795_000_000)
}

/// The project folder holding the mockup's sessions.
pub const HOME: &str = "/home/burce";

/// Folders the user added to the sidebar themselves, in order.
pub fn order() -> Vec<String> {
    vec![
        "/home/burce/code/ArcticLatent.github.io".to_owned(),
        "/home/burce/code/harlo".to_owned(),
    ]
}

fn project(id: &str, directory: &str) -> ServerProject {
    ServerProject {
        id: id.to_owned(),
        directory: directory.to_owned(),
        name: None,
        git: true,
        color: None,
        icon: None,
        start: None,
    }
}

/// The projects and sessions from the design mockup.
pub fn snapshot() -> Snapshot {
    let recent: [(&str, &str, u64); 12] = [
        ("hi how are you", "openai", 4 * HOUR),
        ("how are you", "anthropic", DAY),
        ("how are you", "openai", DAY + 60),
        ("pi update", "openai", 10 * DAY),
        (
            "Write this project's specs: SPEC.md files that hold its intent between sessions",
            "openai",
            20 * DAY,
        ),
        ("how are you", "openai", 31 * DAY),
        ("/tmp/pi-clipboard-649dc2f0-1e4b.png", "openai", 32 * DAY),
        (
            "I am using gnome 51 unstable and the dock is gone",
            "openai",
            33 * DAY,
        ),
        (
            "i have installed pi-lens extension but it doesn't load",
            "openai",
            34 * DAY,
        ),
        (
            "You are a senior Houdini programmer. Review this VEX",
            "openai",
            35 * DAY,
        ),
        (
            "You are a senior Houdini programmer. Port this HDA",
            "openai",
            36 * DAY,
        ),
        (
            "You are a senior Houdini programmer. Write a SOP",
            "zai-coding-plan",
            37 * DAY,
        ),
    ];
    let older = (0..19_u64).map(|i| {
        (
            format!("Older session {}", i + 1),
            "openai",
            (40 + 3 * i) * DAY,
        )
    });
    let now_ms = 1_795_000_000_000.0;
    #[expect(clippy::cast_precision_loss, reason = "ages fit easily in an f64")]
    let sessions = recent
        .into_iter()
        .map(|(title, maker, age)| (title.to_owned(), maker, age))
        .chain(older)
        .enumerate()
        .map(|(i, (title, maker, age))| Session {
            id: format!("ses_demo_{i:02}"),
            title: Some(title),
            directory: HOME.to_owned(),
            project_id: "p_home".to_owned(),
            parent_id: None,
            agent: Some("build".to_owned()),
            model: Some(ModelRef {
                provider_id: maker.to_owned(),
                id: "model".to_owned(),
                variant: None,
            }),
            created: now_ms - (age as f64) * 1000.0,
            updated: now_ms - (age as f64) * 1000.0,
            revert: None,
            permissions: Vec::new(),
            cost: 0.0,
            tokens: Tokens::default(),
        })
        .collect();
    Snapshot {
        projects: vec![
            project("p_arctic", "/home/burce/code/ArcticLatent.github.io"),
            project("p_harlo", "/home/burce/code/harlo"),
            project("p_home", HOME),
        ],
        worktrees: [
            ("p_arctic", "/home/burce/code/ArcticLatent.github.io"),
            ("p_harlo", "/home/burce/code/harlo"),
            ("p_home", HOME),
        ]
        .into_iter()
        .map(|(id, dir)| (id.to_owned(), vec![dir.to_owned()]))
        .collect(),
        sessions,
        active: Vec::new(),
        permissions: Vec::new(),
        forms: Vec::new(),
    }
}

/// The update a connection sends once it has loaded `snapshot`.
///
/// Its client points at a port nothing listens on; the UI tests never call it.
pub fn connected(snapshot: Snapshot) -> Message {
    Message::Server(Update::Connected(Arc::new(Connected {
        client: Client::new(Endpoint::new("http://127.0.0.1:9", None)),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot,
    })))
}

/// The default layout with the mockup's project folders.
pub fn layout() -> Layout {
    Layout {
        order: order(),
        ..Layout::default()
    }
}

/// A new app on the shared service, with the mockup's layout. Harlo's own
/// tools stay off: offered, they'd attach for the real home folder on the
/// test's server. Their own test turns them on with a scratch home.
pub fn new_app() -> App {
    let mut app = App::new(Location::Shared, vec![Location::Shared], layout());
    app.preferences.own_tools = false;
    // Never the real speech models' folder.
    app.voice.models = None;
    app
}

/// An app connected to the mockup's server, with the clock at [`now`].
pub fn app() -> App {
    let mut app = new_app();
    let _ = app.update(Message::Tick(now()));
    let _ = app.update(connected(snapshot()));
    app
}
