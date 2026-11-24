//! A session's goal above its composer, against a private `harlo-server`
//! with scratch `HOME`/`XDG_*` folders and the scripted model: read when
//! the session opens, followed as it changes, and resumed and dismissed
//! from its line. Never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;
use std::time::Duration;

use harlo::app::Message;
use harlo_core::server::{Client, Connected, Delivery, Goal, GoalStatus, Update};
use harlo_core::workspace::{Event, EventKind};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// Session `id`'s goal on the server, once `test` accepts it.
fn on_server(
    tasks: &mut Tasks,
    ui: &Ui,
    client: &Client,
    id: &str,
    project: &str,
    test: impl Fn(Option<&Goal>) -> bool,
) -> Option<Goal> {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        // The window's own requests keep running meanwhile.
        tasks.until(ui, Duration::from_millis(100), |_| false);
        let goal = tasks.block_on(client.goal(id, project)).expect("the goal");
        if test(goal.as_ref()) {
            return goal;
        }
        assert!(std::time::Instant::now() < deadline, "the goal is {goal:?}");
    }
}

/// The server's word that session `id`'s goal is now `goal`, as the event
/// stream would bring it.
fn changed(id: &str, goal: Option<Goal>) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt_goal".into(),
        seq: None,
        time: 1.0,
        kind: EventKind::GoalChanged {
            session_id: id.into(),
            goal,
        },
    }))
}

#[test]
fn a_goal_shows_above_the_composer_and_resumes_and_dismisses_from_there() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let reviewer = sandbox.root().join("scripts.json");
    std::fs::write(
        &reviewer,
        serde_json::json!({
            "<reviewer>": r#"{"verdict":"blocked","reason":"It needs the release date."}"#
        })
        .to_string(),
    )
    .expect("the reviewer's verdict");
    let client = sandbox.client();
    let project = sandbox.project_str();
    let id = tasks.block_on(async {
        let session = client.create_session(&project).await.expect("session");
        client
            .run_command(&session.id, "goal", "Ship the release", Delivery::Queue)
            .await
            .expect("set");
        session.id
    });

    // The sandbox's session, in the demo's sidebar.
    let mut snapshot = demo::snapshot();
    let mut session = snapshot.sessions[0].clone();
    session.id.clone_from(&id);
    session.directory.clone_from(&project);
    session.title = Some("Release".into());
    snapshot.sessions.push(session);
    let mut app = demo::new_app();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: client.clone(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot,
    }))));
    let ui = ui(app);
    let paused = on_server(&mut tasks, &ui, &client, &id, &project, |g| {
        g.is_some_and(|g| g.status == GoalStatus::Paused)
    });

    // Read as the session opens, with why it paused.
    ui.send(Message::OpenSession(id.clone()));
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().goal(&id).is_some()
            && ui.shows("Goal: Ship the release")),
        "the goal and its note in the timeline"
    );
    assert_eq!(ui.app().goal(&id), paused.as_ref());
    for words in [
        "Ship the release",
        "Paused",
        "It needs the release date.",
        "Resume",
    ] {
        assert!(ui.shows(words), "{words}");
    }

    // Resumed, the scripted reviewer finds it met.
    std::fs::remove_file(&reviewer).expect("met from now on");
    ui.press("Resume");
    let reached = on_server(&mut tasks, &ui, &client, &id, &project, |g| {
        g.is_some_and(|g| g.status == GoalStatus::Complete)
    });
    ui.send(changed(&id, reached));
    assert!(ui.shows("Reached"));
    assert!(ui.shows("The work is done and shown."));
    assert!(!ui.shows("Clear goal"), "nothing left to stop");

    // Dismissed, it goes.
    ui.press("Dismiss");
    let gone = on_server(&mut tasks, &ui, &client, &id, &project, |g| g.is_none());
    ui.send(changed(&id, gone));
    assert_eq!(ui.app().goal(&id), None);
    assert!(!ui.shows("Reached"));
}
