//! A real session's history, turned into timeline rows.
//!
//! Runs against a private `opencode serve` with scratch `HOME`/`XDG_*` folders
//! and a scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod support;

use harlo_core::server::{Delivery, message_id};
use harlo_core::timeline::{Entry, entries};
use harlo_core::workspace::ToolStatus;
use support::{Sandbox, wait_for, wait_idle};

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_run_becomes_prompt_tool_and_answer_rows() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("a new session");
    let script = r#"tool glob {"pattern":"*"} ;; say Found **the files**."#;
    client
        .prompt(&session.id, &message_id(), script, Delivery::Queue)
        .await
        .expect("the prompt is accepted");
    wait_for("the run to start", || async {
        let messages = client.messages(&session.id).await.ok()?;
        (messages.len() > 1).then_some(())
    })
    .await;
    wait_idle(&client, &session.id).await;

    let messages = client.messages(&session.id).await.expect("the history");
    let rows = entries(&messages);
    let prompt = rows
        .iter()
        .position(|row| matches!(row, Entry::Prompt { text, .. } if text == script))
        .unwrap_or_else(|| panic!("the prompt row: {rows:#?}"));
    let tool = rows
        .iter()
        .position(|row| {
            matches!(row, Entry::Tool { label, detail, status: ToolStatus::Completed, took: Some(_), .. }
                if label == "Glob" && detail == "*")
        })
        .unwrap_or_else(|| panic!("a finished Glob row with its pattern: {rows:#?}"));
    let answer = rows
        .iter()
        .position(|row| matches!(row, Entry::Answer { text, .. } if text.contains("**the files**")))
        .unwrap_or_else(|| panic!("the Markdown answer: {rows:#?}"));
    assert!(prompt < tool && tool < answer, "oldest first: {rows:#?}");

    // Keys are stable, so reloading keeps the same rows.
    let again = entries(&client.messages(&session.id).await.expect("the history"));
    assert_eq!(rows, again);
}
