//! Code Mode against a private server: a script the model runs with
//! `execute` reports the tools it called, live and in the history.
//!
//! Each test runs its own `opencode serve` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Delivery, EventKind, Message, Part, Prompt, ToolStatus};
use support::{Sandbox, wait_idle};

/// A script that calls one tool from OpenCode's Code Mode catalog.
const SCRIPT: &str =
    "// List the schedules\nconst all = await tools.automation.list({});\nreturn all;";

#[tokio::test(flavor = "multi_thread")]
async fn a_script_reports_the_tools_it_called() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let mut events = client.events().await.expect("the event stream");

    let script = format!(
        "tool execute {} ;; say listed",
        serde_json::json!({ "code": SCRIPT })
    );
    client
        .prompt(&session.id, &Prompt::new(&script, Delivery::Steer))
        .await
        .expect("prompt");
    wait_idle(&client, &session.id).await;

    // The script reports its calls as it runs.
    let heard = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(event) = events.next().await {
            if let Ok(event) = event
                && let EventKind::ToolProgress { calls, .. } = event.kind
                && !calls.is_empty()
            {
                return calls;
            }
        }
        Vec::new()
    })
    .await
    .expect("the script's progress was heard");
    assert_eq!(heard[0].tool, "automation.list");

    let messages = client.messages(&session.id).await.expect("messages");
    let call = messages
        .iter()
        .find_map(|m| match m {
            Message::Assistant { parts, .. } => parts.iter().find_map(|p| match p {
                Part::Tool(call) if call.name == "execute" => Some(call.clone()),
                _ => None,
            }),
            _ => None,
        })
        .expect("the execute call");
    assert_eq!(call.status, ToolStatus::Completed);
    assert_eq!(call.input["code"], SCRIPT);
    assert_eq!(call.calls.len(), 1, "{call:#?}");
    assert_eq!(call.calls[0].tool, "automation.list");
    assert_eq!(call.calls[0].status, ToolStatus::Completed);
}
