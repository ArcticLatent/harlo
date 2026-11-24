//! A capability `harlo-server` doesn't have yet says so plainly: client
//! tools are refused with "unavailable", so a client stops asking and
//! never takes an empty success for registered tools.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::Arc;

use opencode_client::{ToolHost, ToolRun, ToolSpec};
use support::Sandbox;

struct NoTools;

impl ToolHost for NoTools {
    fn tools(&self) -> Vec<ToolSpec> {
        Vec::new()
    }

    fn call(&self, _name: String, _input: serde_json::Value, _session: Option<String>) -> ToolRun {
        Box::pin(async { Err("no tools".to_owned()) })
    }
}

#[tokio::test]
async fn client_tools_are_refused_as_unavailable() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let refused = client
        .host_tools(project.to_str().expect("utf-8"), "harlo", Arc::new(NoTools))
        .await
        .expect_err("refused");
    assert!(refused.is_unavailable(), "{refused}");
    assert!(!refused.is_settled() && !refused.is_transport());
    assert!(
        refused
            .server_message()
            .is_some_and(|m| m.contains("doesn't offer")),
        "{refused}"
    );

    let info = reqwest::Client::new()
        .get(format!("{}/api/info", sandbox.url()))
        .basic_auth("opencode", Some(&sandbox.password))
        .send()
        .await
        .expect("info")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    assert_eq!(info["capabilities"]["clientTools"], false);
}
