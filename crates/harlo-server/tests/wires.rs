//! A turn on a provider that speaks Anthropic's Messages API and on one
//! that speaks `OpenAI`'s Responses API: reasoning, a tool call and the
//! answer come through, and the model's own reasoning goes back to it
//! signed or encrypted in the next request.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Delivery, EventKind, Message, ModelRef, Part, Prompt, ToolStatus};
use support::{Sandbox, until};

/// Adds providers `fake-messages` and `fake-responses`, the scripted model
/// behind the two APIs, to the sandbox's config.
fn add_wires(root: &std::path::Path) {
    let file = root.join("config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("config")).expect("JSON");
    let fake = config["providers"]["fake"].clone();
    for (id, package) in [
        ("fake-messages", "@ai-sdk/anthropic"),
        ("fake-responses", "@ai-sdk/openai"),
    ] {
        let mut provider = fake.clone();
        provider["package"] = serde_json::json!(package);
        config["providers"][id] = provider;
    }
    std::fs::write(&file, config.to_string()).expect("config");
}

#[tokio::test(flavor = "multi_thread")]
async fn both_apis_carry_reasoning_calls_and_answers() {
    let sandbox = Sandbox::start().await;
    let project = sandbox.project();
    let root = project.parent().expect("root").to_path_buf();
    add_wires(&root);
    let client = sandbox.client();
    for (provider, path, sent_back) in [
        ("fake-messages", "/v1/messages", "fake-signature"),
        ("fake-responses", "/v1/responses", "fake-encrypted"),
    ] {
        let session = client
            .create_session(project.to_str().expect("utf-8"))
            .await
            .expect("session");
        client
            .set_model(
                &session.id,
                &ModelRef {
                    provider_id: provider.into(),
                    id: "fake-coder".into(),
                    variant: None,
                },
            )
            .await
            .expect("model");
        let mut events = client.events().await.expect("events");
        client
            .prompt(
                &session.id,
                &Prompt::new(
                    r#"think plan it | tool read {"path":"a.txt"} ;; say read it"#,
                    Delivery::Steer,
                ),
            )
            .await
            .expect("prompt");
        until(&mut events, |kind| match kind {
            EventKind::Done { session_id, .. } if session_id == session.id => Some(()),
            _ => None,
        })
        .await;

        let steps: Vec<Vec<Part>> = client
            .messages(&session.id)
            .await
            .expect("messages")
            .into_iter()
            .filter_map(|m| match m {
                Message::Assistant { parts, .. } => Some(parts),
                _ => None,
            })
            .collect();
        assert_eq!(steps.len(), 2, "{provider}: {steps:#?}");
        assert!(
            matches!(&steps[0][0], Part::Reasoning(t) if t == "plan it"),
            "{provider}"
        );
        assert!(
            matches!(&steps[0][1], Part::Tool(c) if c.name == "read" && c.status == ToolStatus::Completed),
            "{provider}: {:?}",
            steps[0]
        );
        assert!(
            matches!(&steps[1][0], Part::Text(t) if t == "read it"),
            "{provider}"
        );

        let log = std::fs::read_to_string(root.join("model.jsonl")).expect("log");
        // The request after the call, which carries its result (the
        // session's title is asked for on the side).
        let after_call = log
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|r| r["path"] == path)
            .map(|r| r["body"].to_string())
            .find(|b| b.contains("tool_result") || b.contains("function_call_output"))
            .expect("the request after the call");
        assert!(
            after_call.contains(sent_back),
            "{provider} sends its reasoning back: {after_call}"
        );
    }
}
