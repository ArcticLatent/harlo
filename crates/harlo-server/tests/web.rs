//! The web tools: `webfetch` reads a page as Markdown, and `websearch`
//! asks a search service (stand-ins here), with a key or without; the
//! first search asks which.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::Arc;
use std::time::Duration;

use harlo_client::{Delivery, Prompt};
use support::Sandbox;

/// What the stand-in search services saw: (service, keyed) per search.
type Seen = Arc<std::sync::Mutex<Vec<(String, bool)>>>;

/// One result named after `service`, as the services' MCP tools return it.
fn mcp_answer(tool: &str, service: &str) -> serde_json::Value {
    let (url, title) = ("https://example.com/v9", format!("Version 9 via {service}"));
    let result = match tool {
        "web_search_exa" => serde_json::json!({ "content": [{ "type": "text",
            "text": format!("Title: {title}\nURL: {url}\nText: Version 9 is out.") }] }),
        "web_search" => serde_json::json!({ "content": [], "structuredContent": {
            "results": [{ "url": url, "title": title, "excerpts": ["Version 9 is out."] }] } }),
        "firecrawl_search" => serde_json::json!({ "content": [{ "type": "text",
            "text": serde_json::json!({ "success": true, "data": { "web": [
                { "url": url, "title": title, "description": "Version 9 is out." }] } })
                .to_string() }] }),
        _ => serde_json::json!({ "content": [{ "type": "text",
            "text": serde_json::json!({ "results": [
                { "url": url, "title": title, "snippet": "Version 9 is out." }] })
                .to_string() }] }),
    };
    serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result })
}

/// Serves a page and stand-ins for the search services on a free port.
/// The services in `limited` answer 429.
async fn stand_in(limited: &'static [&'static str]) -> (String, Seen) {
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse as _;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let page = "<html><head><title>Notes</title></head><body><nav>menu</nav>\
                <h1>Release notes</h1><p>Version 9 ships on Friday.</p></body></html>";
    let seen = Seen::default();
    let (tavily, mcp) = (Arc::clone(&seen), Arc::clone(&seen));
    let mcp = move |headers: HeaderMap, body: axum::Json<serde_json::Value>| {
        let seen = Arc::clone(&mcp);
        async move {
            let tool = body["params"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let service = match tool.as_str() {
                "web_search_exa" => "exa",
                "web_search" => "parallel",
                "firecrawl_search" => "firecrawl",
                _ => "tinyfish",
            };
            if service == "tinyfish" && headers.get("x-tinyfish-access-mode").is_none() {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            seen.lock().expect("seen").push((service.to_owned(), false));
            if limited.contains(&service) {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("retry-after", "120")],
                    "slow down",
                )
                    .into_response();
            }
            axum::Json(mcp_answer(&tool, service)).into_response()
        }
    };
    let app = axum::Router::new()
        .route(
            "/page",
            axum::routing::get(move || async move { axum::response::Html(page) }),
        )
        .route("/mcp", axum::routing::post(mcp.clone()))
        .route("/v2/mcp", axum::routing::post(mcp))
        .route(
            "/search",
            axum::routing::post(move |headers: HeaderMap| {
                let seen = Arc::clone(&tavily);
                async move {
                    let keyed = headers
                        .get("authorization")
                        .is_some_and(|v| v == "Bearer tv-key");
                    let keyless = headers
                        .get("x-tavily-access-mode")
                        .is_some_and(|v| v == "keyless");
                    if !keyed && !keyless {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    seen.lock()
                        .expect("seen")
                        .push(("tavily".to_owned(), keyed));
                    if limited.contains(&"tavily") {
                        return (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response();
                    }
                    axum::Json(serde_json::json!({ "results": [{
                        "url": "https://example.com/v9", "title": "Version 9 via tavily",
                        "content": "Version 9 is out.",
                    }] }))
                    .into_response()
                }
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await });
    (at, seen)
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

/// The tools offered in the model request that carried `text`, once there
/// is one.
async fn offered_with(sandbox: &Sandbox, text: &str) -> String {
    for _ in 0..200 {
        if let Some(request) = support::turn_requests(sandbox.dir.path())
            .into_iter()
            .rev()
            .find(|r| r["messages"].to_string().contains(text))
        {
            return request["tools"].to_string();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no request with {text:?}");
}

/// Session `session`'s first waiting form, once there is one.
async fn form(sandbox: &Sandbox, session: &str) -> harlo_client::Form {
    for _ in 0..200 {
        let forms = sandbox.client().forms(session).await.expect("forms");
        if let Some(form) = forms.into_iter().next() {
            return form;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no form");
}

/// Answers `form`'s one question with `value`.
async fn reply(sandbox: &Sandbox, session: &str, form: &harlo_client::Form, value: &str) {
    let key = form.fields[0].key.clone();
    sandbox
        .client()
        .reply_form(
            session,
            &form.id,
            &[(key, harlo_client::Answer::Text(value.into()))],
        )
        .await
        .expect("answered");
}

/// Sends `script` with no wait.
async fn send(sandbox: &Sandbox, session: &str, script: &str) {
    sandbox
        .client()
        .prompt(session, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
}

/// What the stand-ins saw, once they saw more than `count` searches.
async fn seen_after(seen: &Seen, count: usize) -> Vec<(String, bool)> {
    for _ in 0..200 {
        let now = seen.lock().expect("seen").clone();
        if now.len() > count {
            // (A search moving on from limited services sends a few.)
            tokio::time::sleep(Duration::from_millis(300)).await;
            return seen.lock().expect("seen").clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no search after {count}");
}

const SEARCH: &str = r#"tool websearch {"query":"version 9"} ;; say found"#;

#[tokio::test(flavor = "multi_thread")]
async fn pages_are_fetched_and_the_first_search_asks_which_service() {
    let (at, seen) = stand_in(&[]).await;
    let sandbox = Sandbox::start_with_env(&[("HARLO_WEBSEARCH_BASE", &at)]).await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let session = client.create_session(&folder).await.expect("session");

    // A page is fetched once the person allowed its site.
    let fetch = serde_json::json!({ "url": format!("{at}/page") });
    let mut events = client.events().await.expect("events");
    send(
        &sandbox,
        &session.id,
        &format!("tool webfetch {fetch} ;; say read"),
    )
    .await;
    let asked = support::until(&mut events, |kind| match kind {
        harlo_client::EventKind::PermissionAsked(p) => Some(*p),
        _ => None,
    })
    .await;
    assert_eq!(asked.action, "webfetch");
    assert_eq!(asked.save, [format!("{at}/*")]);
    client
        .reply_permission(
            &session.id,
            &asked.id,
            harlo_client::PermissionReply::Always,
            None,
        )
        .await
        .expect("allowed");
    run_until(
        &sandbox,
        &session.id,
        "say and then",
        "Version 9 ships on Friday.",
    )
    .await;
    let all = results(&sandbox);
    assert!(
        all.contains("# Release notes") && all.contains(": Notes]"),
        "{all}"
    );
    assert!(!all.contains("menu"));
    assert!(
        offered_with(&sandbox, "say read")
            .await
            .contains("websearch"),
        "searching needs no key"
    );
    let services = client.web_search(&folder).await.expect("services");
    let ids: Vec<&str> = services.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["exa", "firecrawl", "parallel", "tavily", "tinyfish"]);

    // The first search asks: allow any, choose one, or turn it off.
    send(&sandbox, &session.id, SEARCH).await;
    let first = form(&sandbox, &session.id).await;
    assert_eq!(first.title, "Web Search");
    reply(&sandbox, &session.id, &first, "choose").await;
    let second = form(&sandbox, &session.id).await;
    assert_eq!(second.title, "Choose a web search provider");
    reply(&sandbox, &session.id, &second, "tavily").await;
    assert_eq!(seen_after(&seen, 0).await, [("tavily".to_owned(), false)]);
    run_until(&sandbox, &session.id, "say waited", "Version 9 via tavily").await;
    assert!(results(&sandbox).contains("(https://example.com/v9)"));

    // The choice is kept, and a key, once there is one, goes with it.
    client
        .connect_key("tavily", "tv-key", &[])
        .await
        .expect("signed in");
    send(&sandbox, &session.id, SEARCH).await;
    let seen = seen_after(&seen, 1).await;
    assert_eq!(seen[1], ("tavily".to_owned(), true));
    assert!(client.forms(&session.id).await.expect("forms").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn at_random_a_limited_service_hands_the_search_on() {
    let (at, seen) = stand_in(&["exa", "firecrawl", "parallel", "tavily"]).await;
    let sandbox = Sandbox::start_with_env(&[("HARLO_WEBSEARCH_BASE", &at)]).await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let session = client.create_session(&folder).await.expect("session");

    send(&sandbox, &session.id, SEARCH).await;
    let first = form(&sandbox, &session.id).await;
    reply(&sandbox, &session.id, &first, "allow").await;
    run_until(
        &sandbox,
        &session.id,
        "say waited",
        "Version 9 via tinyfish",
    )
    .await;
    let tries = seen.lock().expect("seen").clone();
    assert_eq!(tries.last().map(|t| t.0.as_str()), Some("tinyfish"));
    let limited: std::collections::HashSet<&str> = tries.iter().map(|t| t.0.as_str()).collect();
    assert_eq!(limited.len(), tries.len(), "each tried once: {tries:?}");

    // The limited services are left alone; the session keeps TinyFish.
    send(&sandbox, &session.id, SEARCH).await;
    let after = seen_after(&seen, tries.len()).await;
    assert_eq!(after[tries.len()..], [("tinyfish".to_owned(), false)]);
    assert!(client.forms(&session.id).await.expect("forms").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn web_search_turned_off_leaves_the_tool_out() {
    let (at, seen) = stand_in(&[]).await;
    let sandbox = Sandbox::start_with_env(&[("HARLO_WEBSEARCH_BASE", &at)]).await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let session = client.create_session(&folder).await.expect("session");

    // A config can turn it off.
    let docs = sandbox.project().join("docs");
    std::fs::create_dir_all(&docs).expect("folder");
    std::fs::write(docs.join("harlo.json"), r#"{ "websearch": false }"#).expect("config");
    let other = client
        .create_session(docs.to_str().expect("utf-8"))
        .await
        .expect("session");
    send(&sandbox, &other.id, "say in docs").await;
    assert!(
        !offered_with(&sandbox, "say in docs")
            .await
            .contains("websearch")
    );

    // So can the answer to the first search.
    send(&sandbox, &session.id, SEARCH).await;
    let first = form(&sandbox, &session.id).await;
    reply(&sandbox, &session.id, &first, "disable").await;
    run_until(&sandbox, &session.id, "say waited", "turned off").await;
    send(&sandbox, &session.id, "say after").await;
    assert!(
        !offered_with(&sandbox, "say after")
            .await
            .contains("websearch")
    );
    assert!(seen.lock().expect("seen").is_empty());
}
