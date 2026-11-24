//! The web tools: `webfetch` reads a page as Markdown, and `websearch`
//! appears once a search service has a key and asks it (a stand-in here).

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use opencode_client::{Delivery, Prompt};
use support::Sandbox;

/// Serves a page and a search service's answers on a free port.
async fn stand_in() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let page = "<html><head><title>Notes</title></head><body><nav>menu</nav>\
                <h1>Release notes</h1><p>Version 9 ships on Friday.</p></body></html>";
    let app = axum::Router::new()
        .route(
            "/page",
            axum::routing::get(move || async move { axum::response::Html(page) }),
        )
        .route(
            "/search",
            axum::routing::post(|headers: axum::http::HeaderMap| async move {
                let keyed = headers
                    .get("authorization")
                    .is_some_and(|v| v == "Bearer tv-key");
                axum::Json(
                    serde_json::json!({ "results": if keyed { vec![serde_json::json!({
                    "url": "https://example.com/v9", "title": "Version 9",
                    "content": "Version 9 is out.",
                })] } else { Vec::new() } }),
                )
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await });
    at
}

/// The tool results the agent's requests carried.
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
    sandbox
        .client()
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
async fn pages_are_fetched_and_searches_need_a_key() {
    let at = stand_in().await;
    let sandbox = Sandbox::start_with_env(&[("HARLO_WEBSEARCH_BASE", &at)]).await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let session = client.create_session(&folder).await.expect("session");

    let fetch = serde_json::json!({ "url": format!("{at}/page") });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool webfetch {fetch} ;; say read"),
        "Version 9 ships on Friday.",
    )
    .await;
    let all = results(&sandbox);
    assert!(
        all.contains("# Release notes") && all.contains(": Notes]"),
        "{all}"
    );
    assert!(!all.contains("menu"));
    let offered = |sandbox: &Sandbox| {
        support::turn_requests(sandbox.dir.path())
            .last()
            .map(|r| r["tools"].to_string())
            .unwrap_or_default()
    };
    assert!(!offered(&sandbox).contains("websearch"), "no key yet");

    let services = client.web_search(&folder).await.expect("services");
    assert!(services.iter().any(|s| s.id == "tavily"));
    client
        .connect_key("tavily", "tv-key", &[])
        .await
        .expect("signed in");
    run_until(
        &sandbox,
        &session.id,
        r#"tool websearch {"query":"version 9"} ;; say found"#,
        "Version 9 is out.",
    )
    .await;
    assert!(results(&sandbox).contains("(https://example.com/v9)"));
}
