//! Shell commands and background jobs in the window, against a private
//! `harlo-server` with scratch `HOME`/`XDG_*` folders and the scripted
//! model: `!command` in the composer runs it and shows what it printed; a
//! command the agent sent to the background shows above the composer and
//! stops from there. Never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;
use std::time::Duration;

use harlo::app::Message;
use harlo_core::server::{
    Connected, Delivery, Effect, JobStatus, Prompt, Rule, ShellStatus, Update,
};
use harlo_core::workspace::{Event, EventKind};
use opencode_client::Events;
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// The composer's placeholder while a session is open.
const COMPOSER: &str = "Send a message to continue this session…";

/// Passes the server's events for session `id` on to the window, as its
/// event stream would, until one `last` accepts.
fn forward(
    tasks: &mut Tasks,
    ui: &Ui,
    events: &mut Events,
    id: &str,
    last: impl Fn(&EventKind) -> bool,
) {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "no such event in time"
        );
        // The window's own requests keep running meanwhile.
        tasks.until(ui, Duration::from_millis(50), |_| false);
        let Ok(next) = tasks.block_on(async {
            tokio::time::timeout(Duration::from_millis(50), events.next()).await
        }) else {
            continue;
        };
        let event: Event = next.expect("the events go on").expect("an event");
        if event.kind.session_id() != Some(id) {
            continue;
        }
        let done = last(&event.kind);
        ui.send(Message::Server(Update::Event(event)));
        if done {
            return;
        }
    }
}

#[test]
fn a_shell_command_shows_what_it_printed_and_a_background_one_stops() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let client = sandbox.client();
    let id = tasks.block_on(async {
        let session = client
            .create_session(&sandbox.project_str())
            .await
            .expect("session");
        client
            .set_permissions(&session.id, &[Rule::any("shell", Effect::Allow)])
            .await
            .expect("rules");
        session.id
    });
    let mut events = tasks.block_on(client.events()).expect("the event stream");

    // The sandbox's session, in the demo's sidebar.
    let mut snapshot = demo::snapshot();
    let mut session = snapshot.sessions[0].clone();
    session.id.clone_from(&id);
    session.directory = sandbox.project_str();
    session.title = Some("Build".into());
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
    ui.send(Message::OpenSession(id.clone()));

    // `!` runs the rest in the session's folder, and the composer empties.
    ui.fill(COMPOSER, "!printf 'one\\ntwo\\n'; exit 2");
    ui.send(Message::Send);
    assert!(ui.app().composer.as_str().is_empty());
    forward(
        &mut tasks,
        &ui,
        &mut events,
        &id,
        |kind| matches!(kind, EventKind::ShellEnded { run, .. } if run.status == ShellStatus::Exited),
    );
    for words in ["Ran", "printf 'one\\ntwo\\n'; exit 2", "Exit 2"] {
        assert!(ui.shows(words), "{words}");
    }
    assert!(ui.shows("one\ntwo"), "what it printed");
    assert_eq!(ui.app().send_error, None);
    assert_eq!(sandbox.model_requests(), 0, "no model");

    // The agent sends a command to the background.
    tasks
        .block_on(client.prompt(
            &id,
            &Prompt::new(
                r#"tool shell {"command":"sleep 60","background":true} ;; say started"#,
                Delivery::Steer,
            ),
        ))
        .expect("prompt");
    forward(
        &mut tasks,
        &ui,
        &mut events,
        &id,
        |kind| matches!(kind, EventKind::JobUpdated(job) if job.status == JobStatus::Running),
    );
    assert!(ui.shows("sleep 60"));
    assert!(ui.shows("In the background"));
    ui.press("Stop");
    forward(
        &mut tasks,
        &ui,
        &mut events,
        &id,
        |kind| matches!(kind, EventKind::JobUpdated(job) if job.status == JobStatus::Cancelled),
    );
    assert_eq!(ui.app().jobs(&id).count(), 0);
    assert!(!ui.shows("In the background"));
}
