//! A new session is named from its first message: at once with the
//! message's first line, then with the title the model gives.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Delivery, Prompt};
use harlo_engine::generate::title_prompt;
use support::Sandbox;

#[tokio::test(flavor = "multi_thread")]
async fn the_model_names_a_new_session() {
    let sandbox = Sandbox::start().await;
    let first = "Fix the login bug in the auth module, please";
    let scripts = serde_json::json!({ title_prompt(first): "say \"Login bug in auth.\"" });
    std::fs::write(sandbox.dir.path().join("scripts.json"), scripts.to_string()).expect("scripts");
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(&session.id, &Prompt::new(first, Delivery::Queue))
        .await
        .expect("prompt");
    let mut title = None;
    for _ in 0..100 {
        title = client.session(&session.id).await.expect("session").title;
        if title.as_deref() == Some("Login bug in auth") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never named by the model: {title:?}");
}

/// The requests that asked the scripted model for a title.
fn title_requests(sandbox: &Sandbox) -> Vec<serde_json::Value> {
    std::fs::read_to_string(sandbox.dir.path().join("model.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .map(|r| r["body"].clone())
        .filter(|b| {
            b["messages"][0]["content"]
                .as_str()
                .is_some_and(|s| s.starts_with(harlo_engine::generate::TITLE_SYSTEM))
        })
        .collect()
}

/// Changes the sandbox's own config as `change` says.
fn configure(sandbox: &Sandbox, change: impl FnOnce(&mut serde_json::Value)) {
    let path = sandbox.dir.path().join("config/harlo/harlo.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("JSON");
    change(&mut config);
    std::fs::write(&path, config.to_string()).expect("config");
}

/// Sends `first` to a new session and waits for the title request.
async fn first_title_request(sandbox: &Sandbox, first: &str) -> serde_json::Value {
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(&session.id, &Prompt::new(first, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..100 {
        if let Some(request) = title_requests(sandbox).pop() {
            return request;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no title was asked for");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_title_is_asked_of_the_small_model_when_there_is_one() {
    let sandbox = Sandbox::start().await;
    configure(&sandbox, |config| {
        config["small_model"] = "fake/fake-small".into();
        config["providers"]["fake"]["models"]["fake-small"] =
            serde_json::json!({ "name": "Fake Small" });
    });
    let request = first_title_request(&sandbox, "say Hello.").await;
    assert_eq!(request["model"], "fake-small");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_title_is_asked_without_the_sessions_long_thought() {
    let sandbox = Sandbox::start().await;
    configure(&sandbox, |config| {
        config["model"] = "fake/fake-coder#high".into();
        config["providers"]["fake"]["models"]["fake-coder"]["variants"] =
            serde_json::json!([{ "id": "high", "body": { "reasoning_effort": "high" } }]);
    });
    let request = first_title_request(&sandbox, "say Hello.").await;
    assert_eq!(request["model"], "fake-coder");
    assert!(request.get("reasoning_effort").is_none(), "{request}");
    // The session's own steps keep it.
    for _ in 0..100 {
        let steps = support::turn_requests(sandbox.dir.path());
        if let Some(step) = steps.last() {
            assert_eq!(step["reasoning_effort"], "high", "{step}");
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the session never ran");
}
