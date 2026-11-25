//! Instruction files under a session that is going: the system prompt it
//! began with stays as it was, so a provider's cache of the conversation
//! still holds, and what changed in the files reaches the model as a
//! message with the next prompt.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Message, Prompt};
use support::Sandbox;

/// How many turns of `session` have ended.
async fn idles(client: &Client, session: &str) -> usize {
    client
        .messages(session)
        .await
        .expect("messages")
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
}

/// Sends `script` and waits until the session is idle again.
async fn turn(client: &Client, session: &str, script: &str) {
    let before = idles(client, session).await;
    client
        .prompt(session, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..400 {
        if idles(client, session).await > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the turn never ended");
}

/// The model request that answered `prompt`.
fn request_for(sandbox: &Sandbox, prompt: &str) -> serde_json::Value {
    support::turn_requests(sandbox.dir.path())
        .into_iter()
        .find(|r| r["messages"].to_string().contains(prompt))
        .unwrap_or_else(|| panic!("no request answered {prompt:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edited_instruction_file_reaches_a_going_session_as_a_message() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let rules = project.join("AGENTS.md");
    std::fs::write(&rules, "Answer in lowercase.\n").expect("AGENTS.md");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    turn(&client, &session.id, "say First.").await;

    // The file changes under the session.
    std::fs::write(&rules, "Answer in capitals.\n").expect("AGENTS.md");
    turn(&client, &session.id, "say Second.").await;

    // The conversation opens as it did, so what a provider cached of it
    // still holds.
    let first = request_for(&sandbox, "say First.");
    let second = request_for(&sandbox, "say Second.");
    let began = first["messages"][0]["content"].as_str().expect("a prompt");
    assert!(began.contains("Answer in lowercase."), "{began}");
    assert_eq!(second["messages"][0], first["messages"][0]);
    assert_eq!(second["tools"], first["tools"]);

    // What changed comes ahead of the prompt it arrived with.
    let said = second["messages"].to_string();
    let update = said
        .find("The instruction files changed")
        .expect("an update");
    assert!(said.contains("Answer in capitals."), "{said}");
    assert!(
        update < said.find("say Second.").expect("the prompt"),
        "{said}"
    );

    // Once told, a session isn't told again.
    turn(&client, &session.id, "say Third.").await;
    let third = request_for(&sandbox, "say Third.");
    assert_eq!(third["messages"][0], first["messages"][0]);
    assert_eq!(
        third["messages"]
            .to_string()
            .matches("The instruction files changed")
            .count(),
        1
    );

    // A new session reads the file as it is now.
    let other = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("another session");
    turn(&client, &other.id, "say Elsewhere.").await;
    let fresh = request_for(&sandbox, "say Elsewhere.");
    let system = fresh["messages"][0]["content"].as_str().expect("a prompt");
    assert!(system.contains("Answer in capitals."), "{system}");
    assert!(
        !fresh["messages"]
            .to_string()
            .contains("The instruction files changed")
    );
}
