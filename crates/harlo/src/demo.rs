//! Static sidebar content shown until Harlo lists real sessions (milestone M2).

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use harlo_core::sessions::{Project, SessionSummary};

const HOUR: u64 = 3_600;
const DAY: u64 = 24 * HOUR;

/// The projects and sessions from the design mockup.
#[must_use]
pub fn projects() -> Vec<Project> {
    let recent: [(&str, Option<&str>, u64); 12] = [
        ("hi how are you", Some("openai"), 4 * HOUR),
        ("how are you", Some("anthropic"), DAY),
        ("how are you", Some("openai"), DAY),
        ("pi update", Some("openai"), 10 * DAY),
        (
            "Write this project's specs: SPEC.md files that hold its intent between sessions",
            Some("openai"),
            20 * DAY,
        ),
        ("how are you", Some("openai"), 31 * DAY),
        (
            "/tmp/pi-clipboard-649dc2f0-1e4b.png",
            Some("openai"),
            32 * DAY,
        ),
        (
            "I am using gnome 51 unstable and the dock is gone",
            Some("openai"),
            33 * DAY,
        ),
        (
            "i have installed pi-lens extension but it doesn't load",
            Some("openai"),
            34 * DAY,
        ),
        (
            "You are a senior Houdini programmer. Review this VEX",
            Some("openai"),
            35 * DAY,
        ),
        (
            "You are a senior Houdini programmer. Port this HDA",
            Some("openai"),
            36 * DAY,
        ),
        (
            "You are a senior Houdini programmer. Write a SOP",
            Some("zai"),
            37 * DAY,
        ),
    ];
    let older = (0..19_u64).map(|i| {
        (
            format!("Older session {}", i + 1),
            Some("openai"),
            (40 + 3 * i) * DAY,
        )
    });
    let sessions = recent
        .into_iter()
        .map(|(title, maker, age)| (title.to_owned(), maker, age))
        .chain(older)
        .enumerate()
        .map(|(i, (title, maker, age))| SessionSummary {
            id: format!("ses_demo_{i}"),
            title,
            maker: maker.map(str::to_owned),
            age: Duration::from_secs(age),
        })
        .collect();
    vec![
        Project {
            name: "ArcticLatent.github.io".into(),
            working: 1,
            sessions: Vec::new(),
        },
        Project {
            name: "harlo".into(),
            working: 0,
            sessions: Vec::new(),
        },
        Project {
            name: "burce".into(),
            working: 0,
            sessions,
        },
    ]
}
