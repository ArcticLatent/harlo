//! M6 endpoints against a private server: what a session's model sees, its
//! running usage, and each model's context limit.
//!
//! Each test runs its own `opencode serve` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Delivery, EventKind, ModelRef, Prompt, Tokens};
use support::{Sandbox, WAIT, wait_idle};

/// The scripted model reports these for every step.
const STEP: Tokens = Tokens {
    input: 10,
    output: 5,
    reasoning: 0,
    cache_read: 0,
    cache_write: 0,
};

/// A step's tokens and the session's totals arrive as events and in the
/// session's context, along with the model and agent the step used.
#[tokio::test(flavor = "multi_thread")]
async fn usage_and_context_follow_the_steps() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    assert_eq!(session.tokens, Tokens::default(), "nothing used yet");

    let mut events = client.events().await.expect("events");
    client
        .prompt(&session.id, &Prompt::new("say hello", Delivery::Steer))
        .await
        .expect("prompt");
    let (step, total) = tokio::time::timeout(WAIT, async {
        let (mut step, mut total) = (None, None);
        while let Some(event) = events.next().await {
            match event.expect("event").kind {
                EventKind::StepEnded {
                    session_id, tokens, ..
                } if session_id == session.id => step = Some(tokens),
                EventKind::UsageUpdated {
                    session_id, tokens, ..
                } if session_id == session.id => total = Some(tokens),
                _ => {}
            }
            if let (Some(step), Some(total)) = (step, total) {
                return (step, total);
            }
        }
        panic!("event stream ended");
    })
    .await
    .expect("usage was announced");
    assert_eq!(step, STEP);
    assert!(total.total() >= STEP.total());
    wait_idle(&client, &session.id).await;

    let read = client.session(&session.id).await.expect("session");
    assert!(read.tokens.total() >= STEP.total(), "{:?}", read.tokens);
    let context = client.context(&session.id).await.expect("context");
    assert!(context.messages >= 2, "the prompt and the answer");
    assert_eq!(context.last_step, Some(STEP));
    assert_eq!(
        context.model,
        Some(ModelRef {
            provider_id: "fake".into(),
            id: "fake-coder".into(),
            variant: None,
        })
    );
    assert_eq!(context.agent.as_deref(), Some("build"));
    assert!(context.loaded.is_empty());
}

/// A subfolder's `AGENTS.md`, loaded once the agent reads a file there,
/// shows among the session's loaded instructions.
#[tokio::test(flavor = "multi_thread")]
async fn instructions_loaded_on_the_way_are_listed() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let sub = sandbox.project().join("sub");
    std::fs::create_dir_all(&sub).expect("subfolder");
    std::fs::write(sub.join("AGENTS.md"), "Be brief here.\n").expect("AGENTS.md");
    std::fs::write(sub.join("x.txt"), "x\n").expect("x.txt");
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let script = r#"tool read {"path":"sub/x.txt"} ;; say read it"#;
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    wait_idle(&client, &session.id).await;
    let context = client.context(&session.id).await.expect("context");
    let expected = sub.join("AGENTS.md");
    assert!(
        context
            .loaded
            .iter()
            .any(|p| std::path::Path::new(p) == expected),
        "{:?}",
        context.loaded
    );
}

/// Models carry their context window's size, when configured.
#[tokio::test(flavor = "multi_thread")]
async fn models_carry_their_context_limit() {
    let sandbox = Sandbox::start().await;
    let models = sandbox
        .client()
        .models(&sandbox.project_str())
        .await
        .expect("models");
    let fake = models
        .iter()
        .find(|m| m.provider_id == "fake" && m.id == "fake-coder")
        .expect("the scripted model");
    assert_eq!(fake.context, Some(128_000));
}
