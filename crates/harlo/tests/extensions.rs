//! Settings → Extensions: MCP servers, plugins and skills, a tab each.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{Message, Section, SheetEvent};
use harlo_core::server::{Catalog, Skill};
use support::{demo, ui};

/// "pi update" in the demo data.
const PI_UPDATE: &str = "ses_demo_03";

#[test]
fn the_skills_tab_lists_the_folders_skills_and_how_to_add_more() {
    let mut app = demo::app();
    let _ = app.update(Message::OpenSession(PI_UPDATE.into()));
    let folder = demo::snapshot()
        .sessions
        .into_iter()
        .find(|s| s.id == PI_UPDATE)
        .map(|s| s.directory)
        .expect("a demo session");
    let catalog = Catalog {
        skills: vec![Skill {
            id: "release-notes".into(),
            name: "release-notes".into(),
            description: Some("Write the release notes from the changelog.".into()),
        }],
        ..Catalog::default()
    };
    let _ = app.update(Message::Catalog(folder, Ok(Box::new(catalog))));
    let ui = ui(app);
    ui.send(Message::Sheet(SheetEvent::Open(Section::Extensions)));
    assert!(ui.shows("MCP servers"), "the first tab");
    assert!(!ui.shows("release-notes"));

    ui.press("Skills");
    assert!(ui.shows("release-notes"));
    assert!(ui.shows("Write the release notes from the changelog."));
    ui.press("How to add skills");
    assert!(
        ui.messages().iter().any(
            |m| matches!(m, Message::OpenLink(url) if url.contains("opencode.ai/docs/skills"))
        ),
        "OpenCode's page on skills"
    );
}
