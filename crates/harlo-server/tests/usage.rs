//! What providers tell of an account's use, against a stand-in for their
//! sites on loopback: a key's limits and amounts are read with the key
//! the server keeps, one reading serves the clients that ask together,
//! and a provider that refuses a request over a limit counts as limited
//! until it answers again.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::{Arc, Mutex};

use axum::http::HeaderMap;
use harlo_client::{Delivery, EventKind, Prompt};
use serde_json::json;
use support::{Sandbox, until};

/// Serves `OpenRouter`'s key and `DeepSeek`'s balance on a free port, and
/// a model list for the check of a new key. Returns its address and the
/// requests for use it got, each as its path and the key it came with.
async fn stand_in() -> (String, Arc<Mutex<Vec<(String, String)>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let keep = |seen: &Arc<Mutex<Vec<(String, String)>>>, path: &str, headers: &HeaderMap| {
        let key = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        seen.lock().expect("seen").push((path.to_owned(), key));
    };
    let (keys, balances) = (Arc::clone(&seen), Arc::clone(&seen));
    let app = axum::Router::new()
        .route(
            "/{provider}/models",
            axum::routing::get(|| async { axum::Json(json!({ "data": [] })) }),
        )
        .route(
            "/openrouter/key",
            axum::routing::get(move |headers: HeaderMap| {
                keep(&keys, "key", &headers);
                async {
                    axum::Json(json!({ "data": {
                        "limit": 100, "limit_remaining": 20, "limit_reset": "monthly",
                        "usage": 400.0, "usage_monthly": 80.0,
                    }}))
                }
            }),
        )
        .route(
            "/deepseek/user/balance",
            axum::routing::get(move |headers: HeaderMap| {
                keep(&balances, "balance", &headers);
                async {
                    axum::Json(json!({ "is_available": true, "balance_infos": [
                        { "currency": "USD", "total_balance": "4.20" },
                    ]}))
                }
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await });
    (at, seen)
}

/// Sends `script` to the scripted model in `session` and returns how the
/// turn ended.
async fn turn(client: &harlo_client::Client, session: &str, script: &str) -> String {
    let mut events = client.events().await.expect("events");
    client
        .prompt(session, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    until(&mut events, |kind| match kind {
        EventKind::Done { outcome, .. } => Some(outcome),
        _ => None,
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_keys_use_is_read_with_the_key_and_shared_for_a_while() {
    let (site, seen) = stand_in().await;
    let sandbox =
        Sandbox::start_with_env(&[("HARLO_USAGE_BASE", &site), ("HARLO_KEY_CHECK_BASE", &site)])
            .await;
    let client = sandbox.client();
    assert_eq!(
        client.provider_usage().await.expect("an answer"),
        Vec::new(),
        "nothing to tell before a sign-in"
    );

    client
        .connect_key("openrouter", "or-key", &[])
        .await
        .expect("signed in");
    client
        .connect_key("deepseek", "ds-key", &[])
        .await
        .expect("signed in");
    // A sign-in is read at once, however fresh the reading before it.
    let usage = client.provider_usage().await.expect("an answer");
    assert_eq!(usage.len(), 2, "{usage:?}");
    let of = |id: &str| {
        usage
            .iter()
            .find(|u| u.provider == id)
            .unwrap_or_else(|| panic!("no {id} in {usage:?}"))
    };
    let router = of("openrouter");
    assert_eq!(router.limits.len(), 1, "{router:?}");
    assert_eq!(router.limits[0].label, "Key limit");
    assert!(
        (router.limits[0].percent - 80.0).abs() < 0.001,
        "{router:?}"
    );
    assert_eq!(router.limits[0].period.as_deref(), Some("monthly"));
    assert!(!router.reached && router.error.is_none(), "{router:?}");
    let deep = of("deepseek");
    let amount = deep.amount.as_ref().expect("a balance");
    assert_eq!(
        (amount.label.as_str(), amount.currency.as_str()),
        ("Balance", "USD")
    );
    assert!((amount.value - 4.2).abs() < 0.001, "{deep:?}");

    // Each went with its own key, and asking again at once asks no one.
    let asked = seen.lock().expect("seen").clone();
    assert!(
        asked.contains(&("key".to_owned(), "Bearer or-key".to_owned())),
        "{asked:?}"
    );
    assert!(
        asked.contains(&("balance".to_owned(), "Bearer ds-key".to_owned())),
        "{asked:?}"
    );
    client.provider_usage().await.expect("an answer");
    assert_eq!(
        seen.lock().expect("seen").len(),
        asked.len(),
        "kept for a while"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_provider_refusing_over_a_limit_is_limited_until_it_answers() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");

    // Any other failure says nothing of a limit.
    assert_eq!(
        turn(&client, &session.id, "fail 400 bad request").await,
        "failed"
    );
    assert_eq!(
        client.provider_usage().await.expect("an answer"),
        Vec::new()
    );

    let spent = "fail 402 You have reached your usage limit.";
    assert_eq!(turn(&client, &session.id, spent).await, "failed");
    let usage = client.provider_usage().await.expect("an answer");
    assert_eq!(usage.len(), 1, "{usage:?}");
    assert!(usage[0].provider == "fake" && usage[0].reached, "{usage:?}");

    assert_eq!(turn(&client, &session.id, "say back").await, "succeeded");
    assert_eq!(
        client.provider_usage().await.expect("an answer"),
        Vec::new()
    );
}
