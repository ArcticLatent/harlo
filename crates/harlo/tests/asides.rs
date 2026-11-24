//! Side questions: `/btw` in the composer asks one about the open session,
//! and the Side questions panel shows it and its answer, against a private
//! `harlo-server` with scratch `HOME`/`XDG_*` folders and the scripted
//! model. Never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use harlo::app::{Answer, Message, Panel};
use harlo_core::server::{Connected, Delivery, Prompt, Update};
use server::{Sandbox, WAIT, wait_idle};
use support::tasks::Tasks;
use support::{demo, ui};

/// The composer's placeholder while a session is open.
const COMPOSER: &str = "Send a message to continue this session…";

#[test]
fn btw_asks_about_the_session_and_its_panel_shows_the_answer() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let client = sandbox.client();
    let id = tasks.block_on(async {
        let session = client
            .create_session(&sandbox.project_str())
            .await
            .expect("session");
        client
            .prompt(
                &session.id,
                &Prompt::new("say The lexer returns tokens.", Delivery::Steer),
            )
            .await
            .expect("prompt");
        wait_idle(&client, &session.id).await;
        session.id
    });
    let before = tasks
        .block_on(client.messages(&id))
        .expect("messages")
        .len();

    // The sandbox's session, in the demo's sidebar.
    let mut snapshot = demo::snapshot();
    let mut session = snapshot.sessions[0].clone();
    session.id.clone_from(&id);
    session.title = Some("Lexer".into());
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

    // An empty question asks for one.
    ui.fill(COMPOSER, "/btw ");
    ui.send(Message::Send);
    assert_eq!(
        ui.app().send_error.as_deref(),
        Some("Add a question after /btw.")
    );

    ui.fill(COMPOSER, "/btw say It returns **tokens**.");
    ui.send(Message::Send);
    assert!(ui.app().panel_shown(Panel::Asides), "its panel opens");
    assert!(
        ui.app().composer.as_str().is_empty(),
        "the composer empties"
    );
    assert_eq!(ui.app().asides_waiting(), 1);
    ui.wait(support::SLIDE_MS);
    assert!(ui.shows("say It returns **tokens**."), "the question");
    assert!(ui.shows("Answering…"));

    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().asides_waiting() == 0),
        "answered"
    );
    assert_eq!(
        ui.app().asides(&id)[0].answer,
        Answer::Given("It returns **tokens**.".into())
    );
    assert!(!ui.shows("Answering…"));

    // The conversation is as it was.
    let after = tasks
        .block_on(client.messages(&id))
        .expect("messages")
        .len();
    assert_eq!(after, before);
}

#[test]
fn a_question_that_fails_says_why_and_retries() {
    // The demo's client reaches no server.
    let mut tasks = Tasks::new();
    let ui = ui(demo::app());
    ui.send(Message::OpenSession("ses_demo_00".into()));
    ui.fill(COMPOSER, "/btw what port does it use?");
    ui.send(Message::Send);
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().asides_waiting() == 0),
        "it gave up"
    );
    let failed = |ui: &support::Ui| {
        matches!(&ui.app().asides("ses_demo_00")[0].answer, Answer::Failed(why)
            if why.starts_with("Couldn't answer that question"))
    };
    assert!(failed(&ui));
    ui.wait(support::SLIDE_MS);
    assert!(ui.shows("Retry"));

    ui.press("Retry");
    assert_eq!(ui.app().asides_waiting(), 1, "asked again");
    assert!(tasks.until(&ui, WAIT, failed));

    // Another session has none of them.
    ui.send(Message::OpenSession("ses_demo_01".into()));
    assert!(ui.shows(
        "No side questions yet. Type /btw and a question to ask about this session without \
         adding to it."
    ));
}
