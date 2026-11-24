//! What lets its holder act as the server stays away from the agent: the
//! commands it runs don't inherit the server's password, and no tool
//! reads the server's own files, whatever the rules allow.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Effect, Message, Part, Prompt, Rule};
use support::Sandbox;

/// Sends `script` to `session` and waits until its run is over.
async fn run(client: &Client, session: &str, script: &str) {
    let before = idles(client, session).await;
    client
        .prompt(session, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    for _ in 0..200 {
        if idles(client, session).await > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{session} never finished {script}");
}

async fn idles(client: &Client, session: &str) -> usize {
    client
        .messages(session)
        .await
        .expect("messages")
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
}

/// The error of `session`'s last tool call, if it failed.
async fn last_error(client: &Client, session: &str) -> Option<String> {
    client
        .messages(session)
        .await
        .expect("messages")
        .into_iter()
        .flat_map(|m| match m {
            Message::Assistant { parts, .. } => parts,
            _ => Vec::new(),
        })
        .filter_map(|p| match p {
            Part::Tool(call) => Some(call.error),
            _ => None,
        })
        .next_back()
        .flatten()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_gets_neither_the_password_nor_the_servers_own_files() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    // Nothing asks: the rules allow everything.
    client
        .set_permissions(&session.id, &[Rule::any("*", Effect::Allow)])
        .await
        .expect("trusted");

    run(
        &client,
        &session.id,
        r#"tool shell {"command":"env > env.txt"} ;; say done"#,
    )
    .await;
    let environment = std::fs::read_to_string(project.join("env.txt")).expect("env.txt");
    assert!(environment.contains("HOME="), "{environment}");
    assert!(
        !environment.contains(&sandbox.password),
        "a command inherits no password"
    );

    let database = sandbox.dir.path().join("data/harlo/server.db");
    assert!(database.exists());
    let refused = |error: Option<String>| {
        let error = error.expect("refused");
        assert!(error.contains("Harlo's own files"), "{error}");
    };
    let read = format!(
        r#"tool read {{"path":"{}"}} ;; say done"#,
        database.display()
    );
    run(&client, &session.id, &read).await;
    refused(last_error(&client, &session.id).await);
    let copy = format!(
        r#"tool shell {{"command":"cp {} copy.db"}} ;; say done"#,
        database.display()
    );
    run(&client, &session.id, &copy).await;
    refused(last_error(&client, &session.id).await);
    assert!(!project.join("copy.db").exists());
}
