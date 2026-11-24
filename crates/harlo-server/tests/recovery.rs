//! Recovering from a server that dies mid-answer: what the model had
//! streamed is kept, the step is marked as stopped, the run is over, and
//! the session answers its next prompt.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use serde_json::Value;
use support::Sandbox;

/// `GET path` on the sandbox's server, as JSON.
async fn get(sandbox: &Sandbox, path: &str) -> Value {
    reqwest::Client::new()
        .get(format!("{}{path}", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("request")
        .json::<Value>()
        .await
        .expect("json")
}

/// `POST path` with `body` on the sandbox's server.
async fn post(sandbox: &Sandbox, path: &str, body: &Value) {
    let response = reqwest::Client::new()
        .post(format!("{}{path}", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .json(body)
        .send()
        .await
        .expect("request");
    assert!(response.status().is_success(), "{}", response.status());
}

async fn messages(sandbox: &Sandbox, session: &str) -> Vec<Value> {
    get(sandbox, &format!("/api/session/{session}/message")).await["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The text of an assistant message.
fn text(message: &Value) -> String {
    message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["text"].as_str())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_cut_short_by_a_crash_keeps_its_text_and_ends() {
    let mut sandbox = Sandbox::start().await;
    let project = sandbox.project();
    let created = sandbox
        .client()
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    let id = created.id;

    // A slow answer streams for a while, then the server is killed.
    post(
        &sandbox,
        &format!("/api/session/{id}/prompt"),
        &serde_json::json!({ "text": "pace 40 | long 6" }),
    )
    .await;
    let mut streamed = String::new();
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(step) = messages(&sandbox, &id).await.get(1) {
            streamed = text(step);
            if streamed.contains("Section 1") {
                break;
            }
        }
    }
    assert!(
        streamed.contains("Section 0"),
        "saved while streaming: {streamed:?}"
    );
    sandbox.restart().await;

    // What had been saved stays, and the step is over, as stopped.
    let after = messages(&sandbox, &id).await;
    assert_eq!(after.len(), 2, "{after:?}");
    let step = &after[1];
    assert!(text(step).starts_with(&streamed), "{step}");
    assert_eq!(step["error"]["type"], "aborted", "{step}");
    assert!(step["time"]["completed"].is_i64(), "{step}");
    let active = get(&sandbox, "/api/session/active").await;
    assert!(
        active["data"]
            .as_object()
            .is_none_or(serde_json::Map::is_empty),
        "{active}"
    );
    let session = get(&sandbox, &format!("/api/session/{id}")).await;
    let time = &session["data"]["time"];
    assert!(time["idle"].as_i64() >= time["busy"].as_i64(), "{session}");

    // The session answers its next prompt.
    post(
        &sandbox,
        &format!("/api/session/{id}/prompt"),
        &serde_json::json!({ "text": "say Back again." }),
    )
    .await;
    let mut answered = false;
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if messages(&sandbox, &id)
            .await
            .iter()
            .any(|m| text(m) == "Back again.")
        {
            answered = true;
            break;
        }
    }
    assert!(answered, "{:?}", messages(&sandbox, &id).await);
}
