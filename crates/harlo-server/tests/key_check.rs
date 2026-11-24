//! Checking a provider key before it's kept, against a stand-in provider
//! on loopback: a key it refuses isn't kept and says why, one it takes is
//! kept, each sent the way the provider's API takes keys.

// Rust guideline compliant 2026-10-09

mod support;

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse as _;
use support::Sandbox;

/// The one key the stand-in takes.
const GOOD: &str = "good-key";

/// Serves `/<provider>/models` on a free port, taking `GOOD` as a bearer
/// token or as Anthropic's `x-api-key`; returns its address.
async fn stand_in() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let app = axum::Router::new().route(
        "/{provider}/models",
        axum::routing::get(|headers: HeaderMap| async move {
            let said = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
            let bearer = said("authorization") == Some(&format!("Bearer {GOOD}"));
            let anthropic = said("x-api-key") == Some(GOOD) && said("anthropic-version").is_some();
            if bearer || anthropic {
                axum::Json(serde_json::json!({ "data": [] })).into_response()
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({ "error": { "message": "Invalid API key" } })),
                )
                    .into_response()
            }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await });
    at
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_key_is_not_kept_and_a_good_one_is() {
    let base = stand_in().await;
    let sandbox = Sandbox::start_with_env(&[("HARLO_KEY_CHECK_BASE", &base)]).await;
    let client = sandbox.client();

    // A bearer key (an OpenAI-compatible provider).
    let refused = client
        .connect_key("302ai", "wrong-key", &[])
        .await
        .expect_err("refused");
    assert!(
        refused
            .to_string()
            .contains("302.AI refused this key: Invalid API key"),
        "{refused}"
    );
    let kept = client
        .integration("302ai")
        .await
        .expect("an answer")
        .expect("302.AI");
    assert!(kept.connections.is_empty(), "nothing kept");
    client.connect_key("302ai", GOOD, &[]).await.expect("taken");

    // Anthropic's key goes in its own header.
    let refused = client
        .connect_key("anthropic", "wrong-key", &[])
        .await
        .expect_err("refused");
    assert!(
        refused.to_string().contains("Anthropic refused this key"),
        "{refused}"
    );
    client
        .connect_key("anthropic", GOOD, &[])
        .await
        .expect("taken");

    for id in ["302ai", "anthropic"] {
        let signed = client
            .integration(id)
            .await
            .expect("an answer")
            .expect("listed");
        assert_eq!(signed.connections.len(), 1, "{id}");
        assert_eq!(signed.connections[0].method.as_deref(), Some("key"));
    }
}
