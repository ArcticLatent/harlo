//! Side questions against a private server: answered from a session's
//! context, leaving its history as it was.
//!
//! Each test runs its own `opencode serve` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Delivery, Prompt};
use support::{Sandbox, wait_idle};

/// What the model is told before a side question, as Harlo sends it.
const PREAMBLE: &str = "The user is asking a quick side question about the conversation so far. \
                        Answer directly and concisely in markdown from what you already know. \
                        Do not call any tools and do not take any actions.";

#[tokio::test(flavor = "multi_thread")]
async fn a_side_question_is_answered_without_joining_the_conversation() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
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
    let before = client.messages(&session.id).await.expect("messages").len();

    let answer = client
        .ask_aside(
            &session.id,
            &format!("{PREAMBLE}\n\nsay It returns **tokens**."),
        )
        .await
        .expect("answered");
    assert_eq!(answer, "It returns **tokens**.");

    // The conversation reached the model with the question after it.
    let log = std::fs::read_to_string(sandbox.root().join("model.jsonl")).expect("the log");
    let asked = log.lines().last().expect("a request");
    assert!(asked.contains("The lexer returns tokens."), "{asked}");
    assert!(asked.contains("quick side question"), "{asked}");

    // The history is as it was.
    let after = client.messages(&session.id).await.expect("messages").len();
    assert_eq!(after, before);

    // A session that doesn't exist fails.
    let _missing = client
        .ask_aside("ses_missing", "say hi")
        .await
        .expect_err("no such session");
}
