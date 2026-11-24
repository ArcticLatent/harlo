//! Stats count the sessions, prompts, steps, tokens and tool calls the
//! server stored, by day and by model.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Delivery, EventKind, Prompt};
use support::{Sandbox, until};

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_with_a_tool_is_counted() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    assert_eq!(client.stats(None).await.expect("stats").sessions, 0);
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    let mut events = client.events().await.expect("events");
    client
        .prompt(
            &session.id,
            &Prompt::new(
                r#"tool read {"path":"a.txt"} ;; say read it"#,
                Delivery::Steer,
            ),
        )
        .await
        .expect("prompt");
    until(&mut events, |kind| match kind {
        EventKind::Done { .. } => Some(()),
        _ => None,
    })
    .await;

    let stats = client.stats(None).await.expect("stats");
    assert_eq!(
        (stats.sessions, stats.subagents, stats.prompts, stats.steps),
        (1, 0, 1, 2)
    );
    assert!(stats.tokens.input > 0, "{stats:?}");
    assert_eq!((stats.active_days, stats.streak), (1, 1));
    assert_eq!(stats.activity.len(), 1);
    assert_eq!(stats.models.len(), 1, "{stats:?}");

    // From a start after the session, nothing.
    let later = client
        .stats(Some(
            u64::try_from(harlo_engine::now()).expect("positive") + 1_000,
        ))
        .await
        .expect("stats");
    assert_eq!(later.sessions, 0);
}
