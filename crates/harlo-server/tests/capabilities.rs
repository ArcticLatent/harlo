//! Client tools: a client hosts an MCP server over a WebSocket, and its
//! tools reach agents through Code Mode while the socket stays open, each
//! call saying which session made it.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use harlo_client::{Delivery, Prompt, ToolHost, ToolRun, ToolSpec};
use support::Sandbox;

/// One tool, `whoami`, which answers with the calling session.
struct Whoami {
    calls: Arc<Mutex<Vec<Option<String>>>>,
}

impl ToolHost for Whoami {
    fn tools(&self) -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "whoami".to_owned(),
            description: "Says which session called.".to_owned(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }]
    }

    fn call(&self, _name: String, _input: serde_json::Value, session: Option<String>) -> ToolRun {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(session.clone());
        }
        Box::pin(async move { Ok(format!("called from {}", session.unwrap_or_default())) })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn client_tools_reach_agents_with_the_calling_session() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let host = Arc::new(Whoami {
        calls: Arc::clone(&calls),
    });
    let hosting = {
        let (client, folder) = (client.clone(), folder.clone());
        tokio::spawn(async move { client.host_tools(&folder, "harlo", host).await })
    };

    let session = client.create_session(&folder).await.expect("session");
    let code = serde_json::json!({ "code": "return await tools.harlo.whoami({})" });
    // The first prompt can come before the tools are listed.
    let mut answered = false;
    for _ in 0..40 {
        client
            .prompt(
                &session.id,
                &Prompt::new(format!("tool execute {code} ;; say done"), Delivery::Queue),
            )
            .await
            .expect("prompt");
        tokio::time::sleep(Duration::from_millis(250)).await;
        if calls.lock().is_ok_and(|c| !c.is_empty()) {
            answered = true;
            break;
        }
    }
    assert!(answered, "the agent reached the client's tool");
    let first = calls.lock().expect("calls")[0].clone();
    assert_eq!(first.as_deref(), Some(session.id.as_str()));

    let info = reqwest::Client::new()
        .get(format!("{}/api/info", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("info")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    assert_eq!(info["capabilities"]["clientTools"], true);
    hosting.abort();
}
