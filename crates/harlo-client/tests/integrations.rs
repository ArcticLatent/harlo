//! Integrations against a private server: connecting Claude Agent through
//! Claude Code's own sign-in, signing a provider in with a key (and the
//! answers its form needs), and removing the connections again.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and a stand-in path for `claude`, never the shared service or the
//! real Claude Code login.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Answer, Client, FieldKind, Integration, SignInMethod};
use support::{Sandbox, WAIT};

/// The server's integration for Claude subscriptions.
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

/// Every integration, once the server has loaded them.
async fn integrations(client: &Client) -> Vec<Integration> {
    tokio::time::timeout(WAIT, async {
        loop {
            let list = client.integrations().await.expect("an answer");
            if list.len() > 100 {
                return list;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("OpenCode lists its integrations")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_provider_signs_in_with_a_key_and_is_removed() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let all = integrations(&client).await;
    let ai302 = all.iter().find(|i| i.id == "302ai").expect("302.AI");
    assert_eq!(ai302.name, "302.AI");
    assert!(matches!(ai302.methods[0], SignInMethod::Key { .. }));
    assert!(
        ai302
            .methods
            .iter()
            .any(|m| matches!(m, SignInMethod::Env { names } if names == &["302AI_API_KEY"]))
    );
    let azure = all.iter().find(|i| i.id == "azure").expect("Azure");
    let form = azure.methods[0].form();
    assert_eq!(form[0].key, "resourceName");
    assert!(form[0].required);
    assert!(matches!(form[0].kind, FieldKind::Text { .. }));

    client
        .connect_key("302ai", "sk-scratch", &[])
        .await
        .expect("signed in");
    let signed = client
        .integration("302ai")
        .await
        .expect("an answer")
        .expect("302.AI");
    let connection = &signed.connections[0];
    assert_eq!(connection.method.as_deref(), Some("key"));
    let providers = client
        .providers(&sandbox.project_str())
        .await
        .expect("providers");
    assert!(providers.iter().any(|p| p.id == "302ai"), "{providers:?}");

    // A form answer the server needs, and what it says without it.
    let refused = client
        .connect_key("azure", "k", &[])
        .await
        .expect_err("needs its resource name");
    assert_eq!(
        refused.server_message(),
        Some("Missing required form field: resourceName")
    );
    client
        .connect_key(
            "azure",
            "k",
            &[("resourceName".into(), Answer::Text("scratch".into()))],
        )
        .await
        .expect("signed in");

    let credential = connection.credential.clone().expect("a credential");
    client
        .remove_credential(&credential)
        .await
        .expect("removed");
    let removed = client
        .integration("302ai")
        .await
        .expect("an answer")
        .expect("302.AI");
    assert!(removed.connections.is_empty());
}
