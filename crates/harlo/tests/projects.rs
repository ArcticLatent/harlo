//! Settings → Projects: a project's name and colour (the server's), the
//! model its new sessions start with (Harlo's), its worktrees, and removing
//! it from the sidebar.
//!
//! The demo's client points at nothing: requests are dropped, and the
//! server's answers fed in. Making, removing and changing projects against
//! a real server is tested in `opencode-client`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{Field, Message, ProjectEvent, Section, SettingChoice, Sheet, SheetEvent};
use harlo::model::ModelChoice;
use harlo_core::workspace::{Fetched, ServerProject};

use support::{Ui, demo, ui};

/// The demo's project that the tests change.
const HARLO: &str = "/home/burce/code/harlo";

/// The demo, with `harlo`'s settings opened from its sidebar button.
fn harlo_settings() -> Ui {
    let ui = ui(demo::app());
    ui.press("Settings for harlo");
    ui
}

/// The demo's projects as the server answers once `change` is made to
/// `harlo`.
fn answered(ui: &Ui, change: impl FnOnce(&mut ServerProject)) {
    let mut projects = demo::snapshot().projects;
    if let Some(harlo) = projects.iter_mut().find(|p| p.directory == HARLO) {
        change(harlo);
    }
    ui.send(Message::Fetched(Ok(Fetched::Projects(projects))));
}

fn project_event(ui: &Ui, event: ProjectEvent) {
    ui.send(Message::Sheet(SheetEvent::Project(event)));
}

#[test]
fn the_sidebar_opens_a_projects_settings() {
    let ui = harlo_settings();
    let app = ui.app();
    assert_eq!(app.sheets.open, Some(Sheet::Settings));
    assert_eq!(app.sheets.section, Section::Projects);
    assert_eq!(app.sheets.projects.shown.as_deref(), Some(HARLO));
    drop(app);
    assert!(ui.shows("On every app"));
    assert!(ui.shows("Worktrees"));

    // Back to the list, and into another.
    ui.press("All projects");
    assert_eq!(ui.app().sheets.projects.shown, None);
    assert!(ui.shows("Choose a project to change its settings."));
    ui.press("ArcticLatent.github.io");
    assert_eq!(
        ui.app().sheets.projects.shown.as_deref(),
        Some("/home/burce/code/ArcticLatent.github.io")
    );
}

#[test]
fn a_name_and_colour_go_to_the_server_and_show_once_it_says_so() {
    let ui = harlo_settings();
    drop(ui.driver.take_tasks());

    ui.send(Message::Sheet(SheetEvent::Text(
        Field::ProjectName,
        " Harlo app ".into(),
    )));
    assert_eq!(ui.driver.take_tasks().len(), 1, "a request to the server");
    ui.press("Colour: Blue");
    assert_eq!(ui.driver.take_tasks().len(), 1, "a request to the server");

    answered(&ui, |p| {
        p.name = Some("Harlo app".to_owned());
        p.color = Some("blue".to_owned());
    });
    let app = ui.app();
    let harlo = app
        .projects
        .iter()
        .find(|p| p.directory == HARLO)
        .expect("the project");
    assert_eq!(harlo.name, "Harlo app");
    assert_eq!(harlo.color.as_deref(), Some("blue"));
    // Blue is the seventh colour, after "None".
    assert_eq!(app.chosen(SettingChoice::ProjectColor), 7);
    drop(app);
    assert!(ui.shows("Harlo app"));

    // A refusal says why.
    project_event(&ui, ProjectEvent::Saved(Err("Couldn't save it: no".into())));
    assert!(ui.shows("Couldn't save it: no"));
}

#[test]
fn worktrees_list_and_one_with_changes_needs_forcing() {
    let ui = harlo_settings();
    let worktree = "/home/burce/code/harlo-feature";
    ui.send(Message::Fetched(Ok(Fetched::Worktrees(
        "p_harlo".into(),
        vec![HARLO.into(), worktree.into()],
    ))));
    assert!(ui.shows("harlo-feature"));
    drop(ui.driver.take_tasks());

    ui.press("Remove");
    assert_eq!(ui.driver.take_tasks().len(), 1, "a request to the server");
    project_event(
        &ui,
        ProjectEvent::WorktreeRemoved(
            worktree.into(),
            Err(("the worktree has changes".into(), true)),
        ),
    );
    assert!(ui.shows("It has changes; removing it throws them away."));
    ui.press("Remove anyway");
    assert!(
        ui.messages().iter().any(|m| matches!(
            m,
            Message::Sheet(SheetEvent::Project(ProjectEvent::RemoveWorktree {
                directory,
                force: true,
            })) if directory == worktree
        )),
        "forced"
    );

    // Making one that can't be made says why.
    project_event(
        &ui,
        ProjectEvent::WorktreeMade(Err("Couldn't make it: the name is taken".into())),
    );
    assert!(ui.shows("Couldn't make it: the name is taken"));
}

#[test]
fn a_projects_model_starts_its_new_sessions_only() {
    let ui = harlo_settings();
    let model = ModelChoice {
        provider_id: "fake".into(),
        id: "fake-coder".into(),
        name: "Fake Coder".into(),
    };
    project_event(&ui, ProjectEvent::Model(Some(model)));
    assert_eq!(
        ui.app()
            .preferences
            .project_models
            .get(HARLO)
            .map(String::as_str),
        Some("fake/fake-coder")
    );
    // Named by its ID: the demo has no catalog that names it.
    assert!(ui.shows("fake-coder · fake"));

    let started = |folder: &str| {
        ui.send(Message::NewSession(Some(folder.to_owned())));
        ui.app().current_model().map(|m| m.id)
    };
    assert_eq!(started(HARLO).as_deref(), Some("fake-coder"));
    assert_ne!(started(demo::HOME).as_deref(), Some("fake-coder"));

    ui.press("Use the default");
    assert!(!ui.app().preferences.project_models.contains_key(HARLO));
}

#[test]
fn removing_a_project_hides_it_and_can_archive_its_sessions() {
    let ui = ui(demo::app());
    ui.press("Settings for burce");
    drop(ui.driver.take_tasks());
    ui.press("Its sessions: Archive them");
    // The button ends the page.
    ui.wheel("Its sessions", -10);
    ui.press("Remove");

    let gone = |ui: &Ui| !ui.app().projects.iter().any(|p| p.directory == demo::HOME);
    assert!(gone(&ui));
    assert_eq!(ui.app().hidden_projects(), [demo::HOME.to_owned()]);
    // The tests keep no layout; the one task archives its sessions.
    assert_eq!(ui.driver.take_tasks().len(), 1, "its sessions archived");
    assert!(ui.shows("1 removed from the sidebar; + Project with its folder brings one back."));

    // Its sessions don't bring it back; adding its folder does.
    ui.send(demo::connected(demo::snapshot()));
    assert!(gone(&ui));
    ui.send(Message::ProjectChosen(Some(demo::HOME.into())));
    assert!(!gone(&ui));
    assert!(ui.app().hidden_projects().is_empty());
}

#[test]
fn an_icon_image_is_kept_small() {
    use base64::Engine as _;
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let wide = scratch.path().join("wide.png");
    image::RgbaImage::from_pixel(200, 100, image::Rgba([200, 30, 30, 255]))
        .save(&wide)
        .expect("a picture");
    let drawn = scratch.path().join("mark.svg");
    std::fs::write(
        &drawn,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#36c"/></svg>"##,
    )
    .expect("a drawing");

    for (file, size) in [(&wide, (64, 32)), (&drawn, (64, 64))] {
        let url = harlo::app::icon_url(file).expect("an icon");
        let data = url
            .strip_prefix("data:image/png;base64,")
            .expect("a PNG data URL");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .expect("base64");
        let picture = image::load_from_memory(&bytes).expect("a PNG");
        assert_eq!((picture.width(), picture.height()), size, "{file:?}");
    }

    let text = scratch.path().join("notes.png");
    std::fs::write(&text, "not a picture").expect("a file");
    let refused = harlo::app::icon_url(&text).expect_err("not a picture");
    assert!(refused.starts_with("That isn't an image"), "{refused}");
}
