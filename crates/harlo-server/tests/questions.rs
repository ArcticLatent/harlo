//! The agent's questions, as forms: a `question` call creates a form a
//! person answers, once, and the agent reads the answers; a dismissed
//! form stops the turn.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Answer, Delivery, EventKind, Prompt};
use serde_json::Value;
use support::{Sandbox, until};

const ASK: &str = r#"tool question {"questions":[{"question":"Which colour?","header":"Colour","options":[{"label":"Red","description":"warm"},{"label":"Blue"}]},{"question":"Which sizes?","multiple":true,"options":[{"label":"S"},{"label":"L"}]}]} ;; say Got it."#;

/// The session's assistant content parts, as stored.
async fn parts(sandbox: &Sandbox, session: &str) -> Vec<Value> {
    let messages: Value = reqwest::Client::new()
        .get(format!("{}/api/session/{session}/message", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("messages")
        .json()
        .await
        .expect("JSON");
    messages["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["type"] == "assistant")
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .collect()
}

/// What the session's question call returned, or `error: …`.
async fn question_result(sandbox: &Sandbox, session: &str) -> String {
    let parts = parts(sandbox, session).await;
    let call = parts
        .iter()
        .find(|p| p["type"] == "tool" && p["name"] == "question")
        .expect("the question call");
    call["state"]["content"][0]["text"].as_str().map_or_else(
        || {
            format!(
                "error: {}",
                call["state"]["error"]["message"]
                    .as_str()
                    .unwrap_or_default()
            )
        },
        str::to_owned,
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_is_answered_once_and_the_agent_reads_the_answers() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let mut events = client.events().await.expect("events");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(&session.id, &Prompt::new(ASK, Delivery::Queue))
        .await
        .expect("prompt");
    let form = until(&mut events, |kind| match kind {
        EventKind::FormCreated(f) => Some(f),
        _ => None,
    })
    .await;
    assert_eq!(form.fields.len(), 2);
    assert_eq!(form.fields[0].title.as_deref(), Some("Colour"));
    assert_eq!(form.fields[0].description.as_deref(), Some("Which colour?"));
    let listed = client.forms(&session.id).await.expect("forms");
    assert_eq!(listed.len(), 1);
    let in_folder = client
        .forms_in(project.to_str().expect("utf-8"))
        .await
        .expect("forms in the folder");
    assert_eq!(in_folder.len(), 1);

    let answers = [
        ("q0".to_owned(), Answer::Text("Blue".into())),
        ("q1".to_owned(), Answer::List(vec!["S".into(), "L".into()])),
    ];
    let (first, second) = tokio::join!(
        client.reply_form(&session.id, &form.id, &answers),
        client.reply_form(&session.id, &form.id, &answers),
    );
    first.expect("answered");
    second.expect("the same answer again");
    let other = client
        .reply_form(&session.id, &form.id, &answers[..1])
        .await
        .expect_err("already answered");
    assert!(other.is_settled(), "{other}");

    until(&mut events, |kind| match kind {
        EventKind::Done { .. } => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(
        question_result(&sandbox, &session.id).await,
        "User has answered your questions: \"Which colour?\"=\"Blue\", \"Which sizes?\"=\"S, L\". \
         You can now continue with the user's answers in mind."
    );
    assert!(client.forms(&session.id).await.expect("forms").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dismissed_question_stops_the_turn() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let mut events = client.events().await.expect("events");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(&session.id, &Prompt::new(ASK, Delivery::Queue))
        .await
        .expect("prompt");
    let form = until(&mut events, |kind| match kind {
        EventKind::FormCreated(f) => Some(f),
        _ => None,
    })
    .await;
    client
        .cancel_form(&session.id, &form.id)
        .await
        .expect("dismissed");
    until(&mut events, |kind| match kind {
        EventKind::Done { .. } => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(
        question_result(&sandbox, &session.id).await,
        "error: The user dismissed this question"
    );
    let said: Vec<String> = parts(&sandbox, &session.id)
        .await
        .iter()
        .filter(|p| p["type"] == "text")
        .filter_map(|p| p["text"].as_str().map(str::to_owned))
        .collect();
    assert!(!said.iter().any(|t| t.contains("Got it.")), "{said:?}");
}
