//! MCP servers named in a project's config: listed with their status, and
//! their tools called by an agent, through Code Mode or on their own.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Delivery, McpStatus, Prompt};
use support::Sandbox;

/// A stdio MCP server with one tool, `echo`.
const ECHO: &str = r#"import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "echo", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "echo", "description": "Says the text back.",
                             "inputSchema": {"type": "object", "required": ["text"],
                                             "properties": {"text": {"type": "string"}}}}]}
    elif method == "tools/call":
        said = msg["params"]["arguments"]["text"]
        result = {"content": [{"type": "text", "text": "echo: " + said}]}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": msg["id"],
                          "error": {"code": -32601, "message": "no"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
"#;

/// The text of the tool results the agent's requests carried.
fn results(sandbox: &Sandbox) -> String {
    support::turn_requests(sandbox.dir.path())
        .iter()
        .flat_map(|r| r["messages"].as_array().cloned().unwrap_or_default())
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Sends `script` and waits until a tool result holds `wanted`.
async fn run_until(sandbox: &Sandbox, session: &str, script: &str, wanted: &str) {
    let client = sandbox.client();
    client
        .prompt(session, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..200 {
        if results(sandbox).contains(wanted) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no result with {wanted:?}: {}", results(sandbox));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_projects_servers_connect_and_their_tools_answer() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let echo = sandbox.dir.path().join("echo.py");
    std::fs::write(&echo, ECHO).expect("echo.py");
    let command = serde_json::json!(["python3", "-I", echo]);
    let config = serde_json::json!({ "mcp": { "servers": {
        "probe": { "type": "local", "command": command },
        "plain": { "type": "local", "command": command, "codemode": false },
        "off": { "type": "local", "command": command, "disabled": true },
    } } });
    std::fs::write(project.join("opencode.json"), config.to_string()).expect("config");

    let mut listed = Vec::new();
    for _ in 0..200 {
        listed = client.mcp_servers(&folder).await.expect("listed");
        if listed
            .iter()
            .filter(|s| s.status == McpStatus::Connected)
            .count()
            == 2
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let status = |name: &str| {
        listed
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.status.clone())
    };
    assert_eq!(status("probe"), Some(McpStatus::Connected), "{listed:?}");
    assert_eq!(status("plain"), Some(McpStatus::Connected));
    assert_eq!(status("off"), Some(McpStatus::Disabled));

    let session = client.create_session(&folder).await.expect("session");
    let code = serde_json::json!({ "code": "return await tools.probe.echo({text: 'hi'})" });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool execute {code} ;; say done"),
        "echo: hi",
    )
    .await;
    // Not through Code Mode: offered on its own.
    let offered = support::turn_requests(sandbox.dir.path())
        .last()
        .map(|r| r["tools"].to_string())
        .unwrap_or_default();
    assert!(offered.contains("plain_echo"), "{offered}");
    assert!(!offered.contains("probe_echo"));
    run_until(
        &sandbox,
        &session.id,
        r#"tool plain_echo {"text":"yo"} ;; say ok"#,
        "echo: yo",
    )
    .await;

    // Turned off until connected again.
    client.disconnect_mcp(&folder, "probe").await.expect("off");
    let listed = client.mcp_servers(&folder).await.expect("listed");
    assert!(
        listed
            .iter()
            .any(|s| s.name == "probe" && s.status == McpStatus::Disabled)
    );
    client.connect_mcp(&folder, "probe").await.expect("on");
    client
        .connect_mcp(&folder, "missing")
        .await
        .expect_err("no such server");
}
