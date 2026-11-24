//! The open session's menu, against a private `opencode serve` with scratch
//! `HOME`/`XDG_*` folders and the scripted model: forking it whole or from
//! a prompt, exporting it, deleting it after asking, importing it back,
//! archiving it and moving it to a worktree. Never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;
use std::time::Duration;

use harlo::app::{Message, SessionAction};
use harlo::model::Picker;
use harlo_core::server::{Client, Connected, Delivery, Prompt, Update};
use harlo_core::timeline::Entry;
use server::{Sandbox, WAIT, wait_idle};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// The prompts in session `id` on the server, in order.
fn prompts(tasks: &mut Tasks, client: &Client, id: &str) -> Vec<String> {
    tasks
        .block_on(client.messages(id))
        .expect("messages")
        .into_iter()
        .filter_map(|m| match m {
            harlo_core::workspace::Message::User { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

/// What `check` finds on the server once it finds something, while the
/// window's own requests keep running.
fn on_server<T>(tasks: &mut Tasks, ui: &Ui, mut check: impl FnMut(&mut Tasks) -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        tasks.until(ui, Duration::from_millis(100), |_| false);
        if let Some(found) = check(tasks) {
            return found;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "not on the server in time: {:?}",
            ui.app().send_error
        );
    }
}

/// Waits for the window to open a session other than `id`, and returns it.
fn opened_other(tasks: &mut Tasks, ui: &Ui, id: &str) -> String {
    assert!(
        tasks.until(ui, WAIT, |ui| ui
            .app()
            .active
            .as_deref()
            .is_some_and(|a| a != id)),
        "a new session opened: {:?}",
        ui.app().send_error
    );
    ui.app().active.clone().expect("open")
}

/// A session in the sandbox's project that has answered two prompts, open
/// in the window.
fn lexer(tasks: &mut Tasks, sandbox: &Sandbox) -> (Ui, String) {
    let client = sandbox.client();
    let project = sandbox.project_str();
    let id = tasks.block_on(async {
        let session = client.create_session(&project).await.expect("session");
        client
            .set_title(&session.id, "Lexer")
            .await
            .expect("titled");
        for prompt in ["say one", "say two"] {
            client
                .prompt(&session.id, &Prompt::new(prompt, Delivery::Steer))
                .await
                .expect("prompt");
            wait_idle(&client, &session.id).await;
        }
        session.id
    });

    // The sandbox's session, in the demo's sidebar.
    let mut snapshot = demo::snapshot();
    let mut session = snapshot.sessions[0].clone();
    session.id.clone_from(&id);
    session.directory.clone_from(&project);
    session.title = Some("Lexer".into());
    snapshot.sessions.push(session);
    let mut app = demo::new_app();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: client.clone(),
        server: "OpenCode".into(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot,
    }))));
    let ui = ui(app);
    ui.send(Message::OpenSession(id.clone()));
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("say two")),
        "its history"
    );
    (ui, id)
}

#[test]
fn the_session_menu_forks_it_whole_or_from_a_prompt() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let client = sandbox.client();
    let (ui, id) = lexer(&mut tasks, &sandbox);

    // Fork: a new session with the whole history.
    ui.press("Session actions");
    assert_eq!(ui.app().picker, Some(Picker::SessionMenu));
    for item in ["Fork", "Export…", "Import a session…", "Archive", "Delete…"] {
        assert!(ui.shows(item), "{item}");
    }
    ui.press("Fork");
    assert_eq!(ui.app().picker, None);
    let fork = opened_other(&mut tasks, &ui, &id);
    assert_eq!(prompts(&mut tasks, &client, &fork), ["say one", "say two"]);

    // Fork from a prompt: what came before, that prompt in the composer.
    ui.send(Message::OpenSession(id.clone()));
    assert!(tasks.until(&ui, WAIT, |ui| ui.shows("say two")));
    let key = ui
        .app()
        .timeline_entries(&id)
        .into_iter()
        .flatten()
        .find_map(|e| match e {
            Entry::Prompt { key, text, .. } if text == "say two" => Some(key),
            _ => None,
        })
        .expect("the second prompt's row");
    ui.send(Message::SessionMenu(SessionAction::ForkBefore(key)));
    let before = opened_other(&mut tasks, &ui, &id);
    assert_ne!(before, fork);
    assert_eq!(prompts(&mut tasks, &client, &before), ["say one"]);
    assert_eq!(ui.app().composer.as_str(), "say two");
}

#[test]
fn the_session_menu_exports_deletes_imports_archives_and_moves_it() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let client = sandbox.client();
    let (ui, id) = lexer(&mut tasks, &sandbox);
    // Another session to come back to, made from the menu.
    ui.press("Session actions");
    ui.press("Fork");
    let fork = opened_other(&mut tasks, &ui, &id);
    ui.send(Message::OpenSession(id.clone()));

    // Export, as the save dialog would after a file is chosen.
    let file = sandbox.root().join("lexer.json");
    ui.send(Message::SessionMenu(SessionAction::ExportTo(file.clone())));
    assert!(
        tasks.until(&ui, WAIT, |_| file.exists()),
        "saved: {:?}",
        ui.app().send_error
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).expect("read")).expect("JSON");
    assert_eq!(saved["info"]["id"], id.as_str());

    // Delete asks first; keeping it closes the menu.
    ui.press("Session actions");
    ui.press("Delete…");
    assert_eq!(ui.app().picker, Some(Picker::DeleteSession));
    assert!(ui.shows("Gone for every app using this server."));
    assert!(ui.shows("Its subagents' sessions are deleted with it."));
    ui.press("Keep it");
    assert_eq!(ui.app().picker, None);
    assert!(tasks.block_on(client.session(&id)).is_ok(), "kept");
    ui.press("Session actions");
    ui.press("Delete…");
    ui.press("Delete for good");
    on_server(&mut tasks, &ui, |tasks| {
        tasks.block_on(client.session(&id)).is_err().then_some(())
    });

    // Imported back from its file, it opens.
    ui.send(Message::OpenSession(fork.clone()));
    ui.send(Message::SessionMenu(SessionAction::ImportFrom(file)));
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().active.as_deref()
            == Some(id.as_str())),
        "imported: {:?}",
        ui.app().send_error
    );
    assert_eq!(prompts(&mut tasks, &client, &id), ["say one", "say two"]);

    // Archived, it stays on the server.
    ui.press("Session actions");
    ui.press("Archive");
    on_server(&mut tasks, &ui, |tasks| {
        let session = tasks.block_on(client.session(&id)).ok()?;
        session.archived.map(|_| ())
    });

    // Moved to a worktree of its project.
    let worktree = tasks
        .block_on(async {
            let session = client.session(&id).await?;
            client
                .create_worktree(&session.project_id, Some("lexer"), None)
                .await
        })
        .expect("a worktree");
    ui.send(Message::SessionMenu(SessionAction::MoveTo(
        worktree.clone(),
    )));
    let moved = on_server(&mut tasks, &ui, |tasks| {
        let session = tasks.block_on(client.session(&id)).ok()?;
        (session.directory == worktree).then_some(session)
    });
    assert_eq!(
        prompts(&mut tasks, &client, &moved.id),
        ["say one", "say two"]
    );
}
