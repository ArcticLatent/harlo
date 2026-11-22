//! Integrations against a private server: connecting Claude Agent through
//! Claude Code's own sign-in, and removing the connection again.
//!
//! Each test runs its own `opencode serve` with scratch `HOME`/`XDG_*`
//! folders and a stand-in path for `claude`, never the shared service or the
//! real Claude Code login.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Client, Integration};
use support::{Sandbox, WAIT};

/// OpenCode's integration for Claude subscriptions.
const CLAUDE_AGENT: &str = "claude-agent";

/// Its method that uses Claude Code's sign-in.
const CLAUDE_LOGIN: &str = "claude-login";

/// Claude Agent, once the server has loaded its integrations, which it does
/// in the background after it starts.
async fn claude_agent(client: &Client) -> Integration {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Some(found) = client.integration(CLAUDE_AGENT).await.expect("an answer") {
                return found;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("OpenCode has Claude Agent")
}

#[tokio::test(flavor = "multi_thread")]
async fn claude_agent_connects_and_disconnects() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();

    let fresh = claude_agent(&client).await;
    assert_eq!(fresh.name, "Claude Agent");
    assert!(
        fresh.connections.iter().all(|c| c.credential.is_none()),
        "nothing stored in a fresh server: {:?}",
        fresh.connections
    );

    client
        .connect_external(CLAUDE_AGENT, CLAUDE_LOGIN)
        .await
        .expect("connected");
    let connected = claude_agent(&client).await;
    let credential = connected
        .connections
        .iter()
        .find_map(|c| c.credential.clone())
        .expect("a stored connection");
    assert_eq!(connected.connections[0].label, "Claude Agent");

    client
        .remove_credential(&credential)
        .await
        .expect("removed");
    let removed = claude_agent(&client).await;
    assert!(removed.connections.iter().all(|c| c.credential.is_none()));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_integration_is_none() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    claude_agent(&client).await;
    let missing = client
        .integration("no-such-service")
        .await
        .expect("an answer");
    assert_eq!(missing, None);
}
