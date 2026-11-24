//! M0 acceptance tests: a prompt sent again with its ID runs once.
//!
//! Each test runs its own private Harlo server (see `support`).

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Message, Part, Prompt, message_id};
use support::{Sandbox, wait_for};

/// Counts finished runs in a session's timeline.
async fn idles(client: &Client, session_id: &str) -> usize {
    let messages = client.messages(session_id).await.expect("messages");
    messages
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
}

/// Waits until the session has finished `count` runs and isn't working.
async fn wait_runs(client: &Client, session_id: &str, count: usize) {
    wait_for("the run to finish", || async {
        let active = client.active().await.ok()?;
        let done = idles(client, session_id).await >= count;
        (done && !active.iter().any(|id| id == session_id)).then_some(())
    })
    .await;
}

fn texts(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::Assistant { parts, .. } => Some(parts.iter().filter_map(|p| match p {
                Part::Text(t) => Some(t.clone()),
                _ => None,
            })),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Sending a prompt again with the same ID stores it and answers it once.
#[tokio::test(flavor = "multi_thread")]
async fn retry_with_same_id_runs_once() {
    let sandbox = Sandbox::start().await;
    let harlo = sandbox.client();
    let session = harlo
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let id = message_id();
    harlo
        .prompt(
            &session.id,
            &Prompt {
                id: id.clone(),
                ..Prompt::new("say once", Delivery::Steer)
            },
        )
        .await
        .expect("first send");
    harlo
        .prompt(
            &session.id,
            &Prompt {
                id: id.clone(),
                ..Prompt::new("say once", Delivery::Steer)
            },
        )
        .await
        .expect("retry");
    wait_runs(&harlo, &session.id, 1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let messages = harlo.messages(&session.id).await.expect("messages");
    let users = messages
        .iter()
        .filter(|m| matches!(m, Message::User { .. }))
        .count();
    assert_eq!(users, 1, "one user message: {messages:#?}");
    assert_eq!(texts(&messages), ["once"]);
}
