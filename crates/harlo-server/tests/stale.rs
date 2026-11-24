//! An agent's edit to a file that changed since it read it is refused,
//! naming the session that changed it; after reading it again, the edit
//! goes through.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use opencode_client::{Client, Delivery, Message, Part, Prompt};
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
async fn an_edit_after_another_sessions_change_is_refused_until_read_again() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let a = client.create_session(&folder).await.expect("a");
    let b = client.create_session(&folder).await.expect("b");
    let edit = |new: &str| {
        format!(
            r#"tool edit {{"path":"a.txt","oldString":"alpha","newString":"{new}"}} ;; say done"#
        )
    };

    run(&client, &a.id, r#"tool read {"path":"a.txt"} ;; say read"#).await;
    // B never read it: its edit only has to match.
    run(&client, &b.id, &edit("alpha beta")).await;
    assert_eq!(last_error(&client, &b.id).await, None);
    let b_title = client
        .session(&b.id)
        .await
        .expect("b")
        .title
        .expect("named");

    run(&client, &a.id, &edit("ALPHA")).await;
    let why = last_error(&client, &a.id).await.expect("refused");
    assert!(why.contains(&format!("\"{b_title}\"")), "{why}");
    let text = std::fs::read_to_string(sandbox.project().join("a.txt")).expect("a.txt");
    assert!(text.contains("alpha beta"), "B's change stays: {text}");

    // Read again, the edit goes through.
    run(
        &client,
        &a.id,
        &format!(r#"tool read {{"path":"a.txt"}} ;; {}"#, edit("ALPHA")),
    )
    .await;
    assert_eq!(last_error(&client, &a.id).await, None);
    let text = std::fs::read_to_string(sandbox.project().join("a.txt")).expect("a.txt");
    assert!(text.contains("ALPHA beta"), "{text}");
}
