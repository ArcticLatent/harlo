//! Forking, moving, deleting, exporting and importing sessions against a
//! private server.
//!
//! Each test runs its own `opencode serve` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Client, Delivery, Message, Prompt};
use support::{Sandbox, wait_idle};

/// A session in the sandbox's project that has answered `prompts`.
async fn talked(client: &Client, sandbox: &Sandbox, prompts: &[&str]) -> String {
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    client
        .set_title(&session.id, "Lexer")
        .await
        .expect("titled");
    for prompt in prompts {
        client
            .prompt(&session.id, &Prompt::new(*prompt, Delivery::Steer))
            .await
            .expect("prompt");
        wait_idle(client, &session.id).await;
    }
    session.id
}

/// The prompts in session `id`, in order.
async fn prompts(client: &Client, id: &str) -> Vec<String> {
    client
        .messages(id)
        .await
        .expect("messages")
        .into_iter()
        .filter_map(|m| match m {
            Message::User { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fork_copies_the_history_or_what_came_before_a_message() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let id = talked(&client, &sandbox, &["say one", "say two"]).await;

    let whole = client.fork_session(&id, None).await.expect("forked");
    assert_ne!(whole.id, id);
    assert_eq!(prompts(&client, &whole.id).await, ["say one", "say two"]);

    let second = client
        .messages(&id)
        .await
        .expect("messages")
        .into_iter()
        .find(|m| matches!(m, Message::User { text, .. } if text == "say two"))
        .expect("the second prompt");
    let before = client
        .fork_session(&id, Some(second.id()))
        .await
        .expect("forked");
    assert_eq!(prompts(&client, &before.id).await, ["say one"]);
    assert_eq!(
        prompts(&client, &id).await,
        ["say one", "say two"],
        "untouched"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_moves_to_a_worktree_and_is_deleted_for_good() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let id = talked(&client, &sandbox, &["say one"]).await;
    let session = client.session(&id).await.expect("the session");
    let worktree = client
        .create_worktree(&session.project_id, Some("lexer"), None)
        .await
        .expect("a worktree");

    client.move_session(&id, &worktree).await.expect("moved");
    let moved = client.session(&id).await.expect("the session");
    assert_eq!(moved.directory, worktree);
    assert_eq!(
        prompts(&client, &id).await,
        ["say one"],
        "its history goes along"
    );

    client.delete_session(&id).await.expect("deleted");
    assert!(client.session(&id).await.is_err(), "gone");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exported_session_imports_where_it_isnt_yet() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let id = talked(&client, &sandbox, &["say one", "say two"]).await;
    let transcript = client.export_session(&id).await.expect("exported");
    let parsed: serde_json::Value = serde_json::from_str(&transcript).expect("JSON");
    assert_eq!(parsed["info"]["id"], id.as_str());

    // The server already has it.
    let refused = client
        .import_session(&transcript, &sandbox.project_str())
        .await
        .expect_err("already there");
    assert!(!refused.to_string().is_empty());

    // Once it's gone, it comes back whole, as it was.
    client.delete_session(&id).await.expect("deleted");
    let imported = client
        .import_session(&transcript, &sandbox.project_str())
        .await
        .expect("imported");
    assert_eq!(imported.id, id);
    assert_eq!(imported.title.as_deref(), Some("Lexer"));
    assert_eq!(imported.directory, sandbox.project_str());
    assert_eq!(prompts(&client, &id).await, ["say one", "say two"]);
}
