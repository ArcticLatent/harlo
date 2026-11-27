//! The stats and settings pages against a private `harlo-server` with
//! scratch `HOME`/`XDG_*` folders, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use i_slint_backend_testing::AccessibleRole;

use harlo::app::{Load, Message};
use harlo::model::Picker;
use harlo_core::server::{Connected, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// The window connected to `sandbox`.
fn window(sandbox: &Sandbox) -> Ui {
    let mut app = demo::new_app();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    ui(app)
}

#[test]
fn settings_and_stats_load_from_the_server() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = window(&sandbox);

    ui.press("Settings");
    ui.press("Providers");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Fake")),
        "the scripted model's provider"
    );
    assert!(ui.shows("Providers"));
    ui.press("Extensions");
    assert!(ui.shows("MCP servers"));
    ui.press("Skills");
    assert!(ui.shows("How to add skills"));

    ui.press("Stats");
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            ui.app().sheets.stats,
            Load::Loaded(_)
        )),
        "the stats load"
    );
    assert!(ui.shows("No sessions in this time."));
    ui.press("All time");
    assert!(tasks.until(&ui, WAIT, |ui| matches!(
        ui.app().sheets.stats,
        Load::Loaded(_)
    )));
}

#[test]
fn the_model_for_new_sessions_is_set_in_harlos_config() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let folder = sandbox.root().join("config/harlo");
    let mut app = demo::new_app();
    // The sandbox's own config folder, never the real one.
    app.config_folder = Some(folder.clone());
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);

    ui.press("Settings");
    ui.press("General");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Fake Coder · fake")),
        "the server's default"
    );
    let mut events = tasks
        .block_on(sandbox.client().events())
        .expect("the server's events");
    ui.press("Change");
    assert_eq!(ui.app().picker, Some(Picker::DefaultModel));
    ui.fill("Filter models", "writer");
    assert!(ui.shows("Fake Writer"), "the models, over Settings");
    ui.press("Fake Writer");
    assert_eq!(ui.app().picker, None);

    // Harlo loads the models again and shows the new default; it hears
    // the server's events as the running app would meanwhile.
    let deadline = std::time::Instant::now() + WAIT;
    while !ui.shows("Fake Writer · fake") && std::time::Instant::now() < deadline {
        let next = tasks.block_on(async {
            tokio::time::timeout(std::time::Duration::from_millis(300), events.next()).await
        });
        if let Ok(Some(Ok(event))) = next {
            ui.send(Message::Server(Update::Event(event)));
        }
        let _ = tasks.until(&ui, std::time::Duration::from_millis(100), |ui| {
            ui.shows("Fake Writer · fake")
        });
    }
    assert!(ui.shows("Fake Writer · fake"), "the new default");
    assert_eq!(
        harlo_core::config::model(&folder).expect("read").as_deref(),
        Some("fake/fake-writer")
    );
    let text = std::fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(text.contains("\"fake\""), "the rest of the config stays");
    assert_eq!(ui.app().sheets.model_error, None);
}

#[test]
fn another_servers_default_model_is_only_shown() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.press("General");
    assert!(ui.shows("Model"));
    assert!(ui.find("Change").is_empty(), "nothing to change here");
}

#[test]
fn settings_experimental_turns_the_servers_features_on_and_off_in_harlos_config() {
    use harlo_core::config::{self, Experiment};

    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let folder = sandbox.root().join("config/harlo");
    let mut app = demo::new_app();
    // The sandbox's own config, never the real one.
    app.config_folder = Some(folder.clone());
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);
    let switch = |ui: &Ui, name: &str| {
        ui.find(name)
            .into_iter()
            .find(|e| e.accessible_role() == Some(AccessibleRole::Checkbox))
            .expect("the switch")
    };

    ui.press("Settings");
    ui.press("Experimental");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Todo list")),
        "the features, once the config is read"
    );
    // The server's defaults: only outlines.
    assert_eq!(
        ui.app().sheets.experiments.on,
        Load::Loaded(vec![Experiment::Outline])
    );
    for name in [
        "File outlines",
        "Session tools in scripts",
        "Cached instructions",
    ] {
        assert!(ui.shows(name), "{name} has its switch");
    }
    assert!(!ui.app().todo_tool(), "no todo tool yet");

    switch(&ui, "Todo list").invoke_accessible_default_action();
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            &ui.app().sheets.experiments.on,
            Load::Loaded(on) if on.contains(&Experiment::Todo)
        )),
        "the todo list turned on"
    );
    let text = std::fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(text.contains("\"todo\": true"), "{text}");
    assert!(text.contains("\"fake\""), "the rest of the config stays");
    // The server reads the change, and agents here get the tool.
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().todo_tool()),
        "the server offers the todo tool"
    );

    // One that is on by default is turned off by name.
    switch(&ui, "File outlines").invoke_accessible_default_action();
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            &ui.app().sheets.experiments.on,
            Load::Loaded(on) if !on.contains(&Experiment::Outline)
        )),
        "outlines turned off"
    );
    assert_eq!(
        config::experiments(&folder).expect("read"),
        [Experiment::Todo]
    );

    // Back to the defaults, the settings go.
    switch(&ui, "Todo list").invoke_accessible_default_action();
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            &ui.app().sheets.experiments.on,
            Load::Loaded(on) if !on.contains(&Experiment::Todo)
        )),
        "the todo list turned off"
    );
    switch(&ui, "File outlines").invoke_accessible_default_action();
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            &ui.app().sheets.experiments.on,
            Load::Loaded(on) if on.contains(&Experiment::Outline)
        )),
        "outlines back on"
    );
    let text = std::fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(!text.contains("experimental"), "{text}");
    assert!(
        tasks.until(&ui, WAIT, |ui| !ui.app().todo_tool()),
        "the todo tool is gone again"
    );
}

#[test]
fn settings_git_reads_scratch_git_and_turns_snapshots_off_in_harlos_config() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let folder = sandbox.root().join("config/harlo");
    let gitconfig = sandbox.root().join("home/.gitconfig");
    std::fs::write(&gitconfig, "[user]\n\tname = Scratch Person\n").expect("a git config");
    let mut app = demo::new_app();
    // The sandbox's own configs, never the real ones.
    app.config_folder = Some(folder.clone());
    app.git_config = harlo_core::git::Global::new()
        .env("GIT_CONFIG_GLOBAL", gitconfig.to_string_lossy())
        .env("GIT_CONFIG_NOSYSTEM", "1");
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);

    ui.press("Settings");
    ui.press("Git");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Keep snapshots")
            && matches!(&ui.app().sheets.git, Load::Loaded(git) if git.name == "Scratch Person")),
        "git's name and the server's snapshots"
    );
    assert!(
        ui.shows("The server's default"),
        "no commit-message model yet"
    );
    assert!(matches!(
        ui.app().sheets.git_settings.snapshots,
        Load::Loaded(true)
    ));

    ui.find("Keep snapshots")
        .into_iter()
        .find(|e| e.accessible_role() == Some(AccessibleRole::Checkbox))
        .expect("the switch")
        .invoke_accessible_default_action();
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            ui.app().sheets.git_settings.snapshots,
            Load::Loaded(false)
        )),
        "turned off"
    );
    assert!(!harlo_core::config::snapshots(&folder).expect("read"));
    let text = std::fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(text.contains("\"fake\""), "the rest of the config stays");

    // The commit-message model is Harlo's own.
    assert!(
        tasks.until(&ui, WAIT, |ui| !ui.app().models.is_empty()),
        "the models"
    );
    ui.press("Change");
    assert_eq!(ui.app().picker, Some(Picker::CommitModel));
    ui.fill("Filter models", "writer");
    ui.press("Fake Writer");
    assert_eq!(ui.app().picker, None);
    assert_eq!(
        ui.app().preferences.commit_model.as_deref(),
        Some("fake/fake-writer")
    );
    assert!(ui.shows("Fake Writer · fake"));
    ui.press("Use the default");
    assert_eq!(ui.app().preferences.commit_model, None);
}

/// How many requests to propose notes `sandbox`'s scripted model answered.
fn note_looks(sandbox: &Sandbox) -> usize {
    std::fs::read_to_string(sandbox.root().join("model.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|l| l.contains("Propose notes from this stretch of a session."))
        .count()
}

/// Sends `text` to `session` on `sandbox`'s server, allows what its agent
/// asks about, and waits until the session was read for notes `looks`
/// times in all.
async fn turn_until_looked(sandbox: &Sandbox, session: &str, text: &str, looks: usize) {
    use harlo_core::server::{Delivery, PermissionReply, Prompt};

    let client = sandbox.client();
    client
        .prompt(session, &Prompt::new(text, Delivery::Queue))
        .await
        .expect("prompt");
    server::wait_for("the session to be read for notes", || async {
        for asked in client.permissions(session).await.ok()? {
            client
                .reply_permission(session, &asked.id, PermissionReply::Once, None)
                .await
                .ok()?;
        }
        (note_looks(sandbox) >= looks).then_some(())
    })
    .await;
}

#[test]
fn settings_memory_shows_proposed_notes_to_keep_changed_or_dismiss() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let client = sandbox.client();
    tasks.block_on(client.set_notes_on(true)).expect("notes on");
    let session = tasks
        .block_on(client.create_session(&sandbox.project_str()))
        .expect("session");

    // A note for all sessions, kept by the agent; then the person says
    // otherwise, and the agent keeps nothing.
    tasks.block_on(turn_until_looked(
        &sandbox,
        &session.id,
        r#"tool remember {"text":"Answer at length.","scope":"user"} ;; say Kept."#,
        1,
    ));
    let old = tasks.block_on(client.notes(None)).expect("notes");
    assert_eq!(old.len(), 1, "{old:?}");
    std::fs::write(
        sandbox.root().join("scripts.json"),
        serde_json::json!({
            "Keep your answers short from now on.": r#"tool read {"path":"a.txt"} ;; say Short it is."#,
            "<notes>": format!(
                r#"[{{"text":"Keep answers short.","kind":"correction","scope":"user","message":"{{person}}","replaces":"{}"}},
                    {{"text":"Reading a.txt needs no question first.","kind":"pitfall","scope":"user","message":"{{person}}"}}]"#,
                old[0].id
            ),
        })
        .to_string(),
    )
    .expect("the scripts");
    tasks.block_on(turn_until_looked(
        &sandbox,
        &session.id,
        "Keep your answers short from now on.",
        2,
    ));
    tasks.block_on(server::wait_for("the proposals", || async {
        (client.note_proposals(None).await.ok()?.len() == 2).then_some(())
    }));

    let ui = window(&sandbox);
    ui.press("Settings");
    ui.press("Memory");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Suggested")),
        "the proposals, once loaded"
    );
    // Each says what it is, where it comes from and what it would replace.
    assert!(ui.shows("A correction you gave"));
    assert!(ui.shows("Replaces a note"));
    assert!(ui.shows("Something that went wrong"));
    let title = session.title.clone().unwrap_or_default();
    let shown = ui.app().sheets.memory.kept.clone();
    let Load::Loaded(shown) = shown else {
        panic!("the page isn't loaded");
    };
    let from = shown.proposals[0].source.title.clone().unwrap_or(title);
    assert!(
        ui.shows(&format!(
            "Keep answers short.\nFrom “{from}” · for all your sessions · would replace “Answer at length.”"
        )),
        "the first proposal's line"
    );

    // Looked at: its words in a field, the message it comes from, the
    // note it replaces. Kept with the words changed.
    ui.press("Review: A correction you gave");
    assert!(ui.shows("“Keep your answers short from now on.”"));
    assert!(ui.shows("Keeping it replaces"));
    assert!(ui.shows("“Answer at length.”"));
    ui.fill("Note", "Keep answers short: a few lines.");
    ui.press("Keep Note");
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            &ui.app().sheets.memory.kept,
            Load::Loaded(kept) if kept.proposals.len() == 1 && kept.notes.len() == 2
        )),
        "kept as a note"
    );
    let notes = tasks.block_on(client.notes(None)).expect("notes");
    let new = notes
        .iter()
        .find(|n| n.text == "Keep answers short: a few lines.")
        .expect("the new note");
    assert_eq!(new.source.session, session.id);
    assert_eq!(new.source.message, shown.proposals[0].source.message);
    let replaced = notes.iter().find(|n| n.id == old[0].id).expect("the old");
    assert_eq!(replaced.superseded_by.as_deref(), Some(new.id.as_str()));
    assert!(ui.shows("Replaced"), "the old note says it was replaced");

    // The other is dismissed, and the section goes.
    ui.press("Review: Something that went wrong");
    ui.press("Dismiss");
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            &ui.app().sheets.memory.kept,
            Load::Loaded(kept) if kept.proposals.is_empty()
        )),
        "dismissed"
    );
    assert!(!ui.shows("Suggested"));
    assert_eq!(
        tasks.block_on(client.notes(None)).expect("notes").len(),
        2,
        "a dismissed proposal is no note"
    );
}
