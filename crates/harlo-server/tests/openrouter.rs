//! Signing in with `OpenRouter` against a stand-in for its site: the
//! browser comes back to the server with a code, which is traded (with the
//! PKCE verifier) for a key, kept as `OpenRouter`'s key credential.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::{Arc, Mutex};

use base64::Engine as _;
use harlo_client::AttemptStatus;
use sha2::Digest as _;
use support::Sandbox;

/// Serves `OpenRouter`'s key exchange on a free port, handing out a key
/// only for the verifier whose challenge `challenge` holds; returns its
/// address.
async fn stand_in(challenge: Arc<Mutex<String>>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let app = axum::Router::new().route(
        "/api/v1/auth/keys",
        axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
            let challenge = challenge.lock().map(|c| c.clone()).unwrap_or_default();
            async move {
                let verifier = body["code_verifier"].as_str().unwrap_or_default();
                let matches = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(sha2::Sha256::digest(verifier.as_bytes()))
                    == challenge;
                if body["code"] == "c1" && body["code_challenge_method"] == "S256" && matches {
                    (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({ "key": "sk-or-test" })),
                    )
                } else {
                    (
                        axum::http::StatusCode::FORBIDDEN,
                        axum::Json(serde_json::json!({ "error": "Invalid code or code_verifier" })),
                    )
                }
            }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await });
    at
}

#[tokio::test(flavor = "multi_thread")]
async fn the_browser_return_signs_openrouter_in_with_a_key() {
    let challenge = Arc::new(Mutex::new(String::new()));
    let site = stand_in(Arc::clone(&challenge)).await;
    let sandbox = Sandbox::start_with_env(&[("HARLO_OPENROUTER_AUTH", &site)]).await;
    let client = sandbox.client();
    let openrouter = client
        .integration("openrouter")
        .await
        .expect("an answer")
        .expect("OpenRouter");
    assert!(openrouter.methods.iter().any(|m| matches!(
        m,
        harlo_client::SignInMethod::OAuth { label, .. } if label == "Sign in with OpenRouter"
    )));

    let attempt = client
        .begin_oauth("openrouter", "openrouter-sign-in", &[])
        .await
        .expect("begun");
    let url = reqwest::Url::parse(&attempt.url).expect("URL");
    let q = |k: &str| {
        url.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    assert_eq!(
        url.as_str().split('?').next(),
        Some(format!("{site}/auth").as_str())
    );
    assert_eq!(q("code_challenge_method"), "S256");
    assert!(q("callback_url").starts_with("http://localhost:"));
    *challenge.lock().expect("challenge") = q("code_challenge");

    // The browser comes back with the code and the attempt's state.
    let back = format!("{}?code=c1&state={}", q("callback_url"), q("state"));
    let page = reqwest::get(&back).await.expect("returned");
    assert!(page.status().is_success());
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let s = client
                .oauth_status("openrouter", &attempt.id)
                .await
                .expect("status");
            if !matches!(s, AttemptStatus::Pending(_)) {
                return s;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("it ended");
    assert_eq!(status, AttemptStatus::Complete);

    let signed = client
        .integration("openrouter")
        .await
        .expect("an answer")
        .expect("OpenRouter");
    assert_eq!(signed.connections.len(), 1);
    assert_eq!(signed.connections[0].method.as_deref(), Some("key"));
    let providers = client
        .providers(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("providers");
    assert!(
        providers.iter().any(|p| p.id == "openrouter"),
        "{providers:?}"
    );
}
