//! Signing in to a remote MCP server that needs OAuth: it shows as needing
//! a sign-in, with an integration; the browser sign-in registers Harlo,
//! trades the code for tokens and connects it; signing out asks again.

// Rust guideline compliant 2026-10-09

#[path = "support/oauth_guard.rs"]
mod oauth_guard;
mod support;

use std::time::Duration;

use oauth_guard::{browse, guard};
use opencode_client::{AttemptStatus, McpStatus};
use serde_json::json;
use support::Sandbox;

/// The status of server `name` in `folder`, once `done` says it's settled.
async fn status_until(
    client: &opencode_client::Client,
    folder: &str,
    name: &str,
    done: impl Fn(&McpStatus) -> bool,
) -> opencode_client::McpServer {
    let mut last = None;
    for _ in 0..300 {
        let listed = client.mcp_servers(folder).await.expect("listed");
        last = listed.into_iter().find(|s| s.name == name);
        if last.as_ref().is_some_and(|s| done(&s.status)) {
            return last.expect("found");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{name} never settled: {last:?}");
}

/// How sign-in `attempt` to `id` ended.
async fn settled(client: &opencode_client::Client, id: &str, attempt: &str) -> AttemptStatus {
    let mut status = AttemptStatus::Pending(None);
    for _ in 0..100 {
        status = client.oauth_status(id, attempt).await.expect("status");
        if status != AttemptStatus::Pending(None) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    status
}

#[tokio::test(flavor = "multi_thread")]
async fn a_remote_server_signs_in_in_the_browser_and_connects() {
    let (base, seen) = guard().await;
    let mut sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let callback = support::free_port();
    let config = json!({ "mcp": { "servers": {
        "guarded": { "type": "remote", "url": format!("{base}/mcp"),
                     "oauth": { "callbackPort": callback } },
    } } });
    std::fs::write(project.join("opencode.json"), config.to_string()).expect("config");

    let server = status_until(&client, &folder, "guarded", |s| {
        matches!(s, McpStatus::NeedsAuth(_))
    })
    .await;
    let id = server.integration.expect("an integration signs it in");
    assert!(id.starts_with("mcp_"), "{id}");
    let integration = client
        .integration(&id)
        .await
        .expect("read")
        .expect("listed");
    assert_eq!(integration.name, "guarded");
    assert!(integration.connections.is_empty());
    let Some(opencode_client::SignInMethod::OAuth { id: method, .. }) =
        integration.methods.first().cloned()
    else {
        panic!("a browser sign-in: {:?}", integration.methods);
    };

    let attempt = client
        .begin_oauth(&id, &method, &[])
        .await
        .expect("sign-in begins");
    assert!(
        attempt.url.starts_with(&format!("{base}/authorize?")),
        "{}",
        attempt.url
    );
    let (back, page) = browse(&attempt.url).await;
    assert!(
        back.starts_with(&format!("http://127.0.0.1:{callback}/mcp/oauth/callback?")),
        "{back}"
    );
    assert!(page.contains("Harlo is connected to guarded"), "{page}");
    assert_eq!(
        settled(&client, &id, &attempt.id).await,
        AttemptStatus::Complete
    );
    let registered = seen.lock().expect("seen").registered.clone();
    assert_eq!(registered.len(), 1, "Harlo registered itself once");
    assert_eq!(registered[0]["client_name"], "Harlo");

    status_until(&client, &folder, "guarded", |s| *s == McpStatus::Connected).await;
    assert!(
        seen.lock().expect("seen").authorized_lists >= 1,
        "its tools were listed with the token"
    );
    let integration = client
        .integration(&id)
        .await
        .expect("read")
        .expect("listed");
    assert!(
        integration
            .connections
            .iter()
            .any(|c| c.credential.is_some()),
        "the sign-in is kept"
    );

    // The sign-in is kept: after a restart it connects with no browser.
    sandbox.restart().await;
    let client = sandbox.client();
    status_until(&client, &folder, "guarded", |s| *s == McpStatus::Connected).await;
    let integration = client
        .integration(&id)
        .await
        .expect("read")
        .expect("listed");
    let credential = integration
        .connections
        .first()
        .and_then(|c| c.credential.clone())
        .expect("still kept");

    // Signing out asks for a sign-in again.
    client
        .remove_credential(&credential)
        .await
        .expect("signed out");
    status_until(&client, &folder, "guarded", |s| {
        matches!(s, McpStatus::NeedsAuth(_))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_with_sign_in_turned_off_has_no_integration() {
    let (base, _) = guard().await;
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let config = json!({ "mcp": { "servers": {
        "keyed": { "type": "remote", "url": format!("{base}/mcp"), "oauth": false },
    } } });
    std::fs::write(project.join("opencode.json"), config.to_string()).expect("config");
    let server = status_until(&client, &folder, "keyed", |s| *s != McpStatus::Pending).await;
    assert_eq!(server.integration, None);
    assert!(
        matches!(server.status, McpStatus::NeedsAuth(_)),
        "{server:?}"
    );
}
