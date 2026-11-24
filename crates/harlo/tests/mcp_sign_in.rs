//! Signing in to a remote MCP server from Settings → Extensions, against a
//! private server with scratch `HOME`/`XDG_*` folders (OpenCode, or
//! harlo-server with `HARLO_OPENCODE_BIN`) and a stand-in for the MCP
//! server and its authorization server. No browser opens: the test follows
//! the sign-in page's address itself.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-server/tests/support/oauth_guard.rs"]
mod oauth_guard;
#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;
use std::time::Duration;

use harlo::app::{Load, Message, SignInAttempt as Attempt};
use harlo_core::server::{Connected, McpStatus, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{demo, ui};

/// A port nothing listens on yet.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port")
}

#[test]
fn a_remote_server_signs_in_from_its_page() {
    let mut tasks = Tasks::new();
    let (base, seen) = tasks.block_on(oauth_guard::guard());
    let sandbox = tasks.block_on(Sandbox::start());
    let callback = free_port();
    let config = serde_json::json!({ "mcp": { "servers": {
        "guarded": { "type": "remote", "url": format!("{base}/mcp"),
                     "oauth": { "callbackPort": callback } },
    } } });
    std::fs::write(sandbox.project().join("opencode.json"), config.to_string()).expect("config");
    let client = sandbox.client();
    let folder = sandbox.project_str();
    let waited = tasks.block_on(async {
        tokio::time::timeout(WAIT, async {
            while !client.mcp_servers(&folder).await.is_ok_and(|l| {
                l.iter()
                    .any(|s| s.name == "guarded" && matches!(s.status, McpStatus::NeedsAuth(_)))
            }) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
    });
    assert!(waited.is_ok(), "the server says it needs a sign-in");

    let mut app = demo::new_app();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "OpenCode".into(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(folder.clone());
    let ui = ui(app);
    ui.press("Settings");
    ui.press("Extensions");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("guarded")),
        "the server's row"
    );
    ui.press("guarded");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Needs signing in")),
        "it needs a sign-in"
    );
    ui.press("Sign in");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Open the sign-in page")),
        "its sign-in page"
    );
    ui.press("Open the sign-in page");
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            ui.app().sheets.providers.attempt,
            Some(Attempt::OAuth(_))
        )),
        "the sign-in started: {:?}",
        ui.app().sheets.providers.error
    );
    let Some(Attempt::OAuth(attempt)) = ui.app().sheets.providers.attempt.clone() else {
        unreachable!("checked above");
    };
    assert!(ui.shows(&attempt.url), "the page's address shows");
    let (_, page) = tasks.block_on(oauth_guard::browse(&attempt.url));
    assert!(!page.is_empty());
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().sheets.providers.attempt.is_none()
            && ui.app().sheets.providers.done.is_some()),
        "the sign-in ends: {:?}",
        ui.app().sheets.providers.error
    );

    // The server connects with the token.
    let connected = tasks.block_on(async {
        tokio::time::timeout(WAIT, async {
            while !client.mcp_servers(&folder).await.is_ok_and(|l| {
                l.iter()
                    .any(|s| s.name == "guarded" && s.status == McpStatus::Connected)
            }) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
    });
    assert!(connected.is_ok(), "it connects");
    assert!(seen.lock().expect("seen").authorized_lists >= 1);
    ui.press("Extensions");
    ui.press("guarded");
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            &ui.app().sheets.setup,
            Load::Loaded(_)
        ) && ui.shows("Connected")),
        "its page says so"
    );
}
