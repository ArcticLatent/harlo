//! Goals against a private server: `/goal` sets one, the server reports
//! its changes, and the reviewer's verdict settles it.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use opencode_client::{Delivery, EventKind, Goal, GoalStatus, Message};
use support::{Sandbox, WAIT};

/// Waits for session `id`'s goal to change to one `test` accepts.
async fn changed(
    events: &mut opencode_client::Events,
    id: &str,
    test: impl Fn(Option<&Goal>) -> bool,
) -> Option<Goal> {
    tokio::time::timeout(WAIT, async {
        while let Some(event) = events.next().await {
            if let Ok(event) = event
                && let EventKind::GoalChanged { session_id, goal } = event.kind
                && session_id == id
                && test(goal.as_ref())
            {
                return goal;
            }
        }
        panic!("the events ended")
    })
    .await
    .expect("the goal changed")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_is_set_reviewed_reached_and_cleared() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project_str();
    let session = client.create_session(&project).await.expect("session");
    let id = session.id.as_str();
    assert_eq!(client.goal(id, &project).await.expect("read"), None);
    let mut events = client.events().await.expect("the event stream");

    client
        .run_command(id, "goal", "Write the release notes", Delivery::Queue)
        .await
        .expect("set");
    let active = changed(&mut events, id, |g| {
        g.is_some_and(|g| g.status == GoalStatus::Active)
    })
    .await
    .expect("a goal");
    assert_eq!(active.objective, "Write the release notes");
    assert_eq!(active.max_rounds, 50);

    // The scripted reviewer finds it met.
    let reached = changed(&mut events, id, |g| {
        g.is_some_and(|g| g.status == GoalStatus::Complete)
    })
    .await
    .expect("a goal");
    assert_eq!(reached.id, active.id);
    assert_eq!(
        reached.reason.as_deref(),
        Some("The work is done and shown.")
    );
    assert_eq!(
        client.goal(id, &project).await.expect("read"),
        Some(reached)
    );

    // Its start shows as one note; the prompt behind it is the agent's.
    let messages = client.messages(id).await.expect("messages");
    assert!(
        messages.iter().any(
            |m| matches!(m, Message::Note { text, .. } if text == "Goal: Write the release notes")
        ),
        "{messages:#?}"
    );

    client
        .run_command(id, "goal", "clear", Delivery::Queue)
        .await
        .expect("cleared");
    assert_eq!(changed(&mut events, id, |g| g.is_none()).await, None);
    assert_eq!(client.goal(id, &project).await.expect("read"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_goal_pauses_with_why_and_resumes() {
    let sandbox = Sandbox::start().await;
    std::fs::write(
        sandbox.root().join("scripts.json"),
        serde_json::json!({
            "<reviewer>": r#"{"verdict":"blocked","reason":"It needs the release date."}"#
        })
        .to_string(),
    )
    .expect("the reviewer's verdict");
    let client = sandbox.client();
    let project = sandbox.project_str();
    let session = client.create_session(&project).await.expect("session");
    let id = session.id.as_str();
    let mut events = client.events().await.expect("the event stream");

    client
        .run_command(id, "goal", "Ship it", Delivery::Queue)
        .await
        .expect("set");
    let paused = changed(&mut events, id, |g| {
        g.is_some_and(|g| g.status == GoalStatus::Paused)
    })
    .await
    .expect("a goal");
    assert_eq!(paused.reason.as_deref(), Some("It needs the release date."));

    std::fs::remove_file(sandbox.root().join("scripts.json")).expect("met from now on");
    client
        .run_command(id, "goal", "resume", Delivery::Queue)
        .await
        .expect("resumed");
    let resumed = changed(&mut events, id, |g| {
        g.is_some_and(|g| g.status == GoalStatus::Active)
    })
    .await
    .expect("a goal");
    assert_eq!(resumed.rounds, 1, "the resume is its first round");
    let reached = changed(&mut events, id, |g| {
        g.is_some_and(|g| g.status == GoalStatus::Complete)
    })
    .await
    .expect("a goal");
    assert_eq!(reached.id, paused.id);
    // Pausing a goal that isn't active fails.
    let _refused = client
        .run_command(id, "goal", "pause", Delivery::Queue)
        .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        client
            .goal(id, &project)
            .await
            .expect("read")
            .map(|g| g.status),
        Some(GoalStatus::Complete)
    );
}
