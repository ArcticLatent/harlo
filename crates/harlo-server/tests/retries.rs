//! A model request that fails in a way that may pass is asked again, and
//! the step goes on; one that can't pass fails at once.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Delivery, EventKind, Prompt};
use support::{Sandbox, until};

#[tokio::test(flavor = "multi_thread")]
async fn a_busy_model_is_asked_again() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    let mut events = client.events().await.expect("events");
    client
        .prompt(
            &session.id,
            &Prompt::new("flaky 1 503 busy | say recovered.", Delivery::Steer),
        )
        .await
        .expect("prompt");
    let (attempt, error) = until(&mut events, |kind| match kind {
        EventKind::RetryScheduled { attempt, error, .. } => Some((attempt, error)),
        _ => None,
    })
    .await;
    assert_eq!(attempt, 2);
    assert!(error.contains("busy"), "{error}");
    let outcome = until(&mut events, |kind| match kind {
        EventKind::Done { outcome, .. } => Some(outcome),
        _ => None,
    })
    .await;
    assert_eq!(outcome, "succeeded");
    let requests = support::turn_requests(sandbox.dir.path());
    assert_eq!(requests.len(), 2, "the first try and the retry");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_request_fails_without_a_retry() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    let mut events = client.events().await.expect("events");
    client
        .prompt(
            &session.id,
            &Prompt::new("fail 400 bad request", Delivery::Steer),
        )
        .await
        .expect("prompt");
    let retried = until(&mut events, |kind| match kind {
        EventKind::RetryScheduled { .. } => Some(true),
        EventKind::Done { .. } => Some(false),
        _ => None,
    })
    .await;
    assert!(!retried, "a bad request isn't asked again");
    assert_eq!(support::turn_requests(sandbox.dir.path()).len(), 1);
}
