//! Agents and steering: a session switched to Plan can't edit, and the
//! model is told it's in Plan mode before the next prompt; switched back,
//! it edits again. A prompt sent to steer a running turn joins it at the
//! next step, within the same run.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Delivery, EventKind, Prompt};
use serde_json::Value;
use support::{Sandbox, until};

const EDIT: &str =
    r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"alpha beta"} ;; say Done."#;

/// The bodies the scripted model was sent, oldest first.
fn requests(sandbox: &Sandbox) -> Vec<Value> {
    support::turn_requests(sandbox.dir.path())
}

/// The text of the request's user messages, in order.
fn user_texts(body: &Value) -> Vec<String> {
    body["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"].as_str().map(str::to_owned))
        .collect()
}

async fn done(events: &mut harlo_client::Events) {
    until(events, |kind| match kind {
        EventKind::Done { .. } => Some(()),
        _ => None,
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn plan_mode_stops_edits_and_the_model_is_told() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let directory = project.to_str().expect("utf-8");
    let agents = client.agents(directory).await.expect("agents");
    let ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["build", "plan", "general", "explore"]);
    assert!(agents[1].read_only && agents[1].primary, "{:?}", agents[1]);

    let mut events = client.events().await.expect("events");
    let session = client.create_session(directory).await.expect("session");
    client.set_agent(&session.id, "plan").await.expect("plan");
    client
        .prompt(&session.id, &Prompt::new(EDIT, Delivery::Queue))
        .await
        .expect("prompt");
    done(&mut events).await;
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "alpha\n",
        "Plan doesn't edit"
    );
    let first = &requests(&sandbox)[0];
    let said = user_texts(first);
    assert!(said[0].contains("You are in Plan mode"), "{said:?}");
    assert_eq!(said[1], EDIT);

    client.set_agent(&session.id, "build").await.expect("build");
    client
        .prompt(&session.id, &Prompt::new(EDIT, Delivery::Queue))
        .await
        .expect("prompt");
    done(&mut events).await;
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "alpha beta\n",
        "Build edits"
    );
    let last = requests(&sandbox).pop().expect("a request");
    assert!(
        user_texts(&last)
            .iter()
            .any(|t| t.contains("no longer in Plan mode")),
        "the model is told Plan mode ended"
    );
    let stored = client.session(&session.id).await.expect("session");
    assert_eq!(stored.agent.as_deref(), Some("build"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_steer_joins_the_running_turn_at_its_next_step() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let mut events = client.events().await.expect("events");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(
            &session.id,
            &Prompt::new(
                r#"pace 200 | say Reading first. | tool read {"path":"a.txt"} ;; say Not steered."#,
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");
    until(&mut events, |kind| match kind {
        EventKind::StepStarted { .. } => Some(()),
        _ => None,
    })
    .await;
    client
        .prompt(&session.id, &Prompt::new("say Steered.", Delivery::Steer))
        .await
        .expect("steer");
    done(&mut events).await;

    let bodies = requests(&sandbox);
    assert_eq!(bodies.len(), 2, "one run of two steps");
    let said = user_texts(&bodies[1]);
    assert_eq!(said.last().map(String::as_str), Some("say Steered."));
    let messages = client.messages(&session.id).await.expect("messages");
    let idles = messages
        .iter()
        .filter(|m| matches!(m, harlo_client::Message::Idle { .. }))
        .count();
    assert_eq!(idles, 1, "one run");
}
