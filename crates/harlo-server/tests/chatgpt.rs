//! Signing in with `ChatGPT` against a stand-in for `OpenAI`'s sign-in
//! server: the browser's return finishes the attempt, the tokens are kept
//! as a credential, and `OpenAI`'s models become usable.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::{Arc, Mutex};

use base64::Engine as _;
use opencode_client::AttemptStatus;
use support::{Sandbox, free_port};

/// A JSON web token with `claims`, unsigned.
fn jwt(claims: &serde_json::Value) -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    format!("e30.{}.sig", b64.encode(claims.to_string()))
}

/// Serves a token endpoint that answers for `nonce` on a free port;
/// returns its address.
async fn stand_in(nonce: Arc<Mutex<String>>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let app = axum::Router::new().route(
        "/api/accounts/oauth/token",
        axum::routing::post(move || {
            let nonce = nonce.lock().map(|n| n.clone()).unwrap_or_default();
            async move {
                axum::Json(serde_json::json!({
                    "access_token": "access-1", "refresh_token": "refresh-1", "expires_in": 3600,
                    "scope": "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
                    "id_token": jwt(&serde_json::json!({
                        "nonce": nonce, "aud": "app_test", "email": "a@example.com",
                    })),
                }))
            }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await });
    at
}

#[tokio::test(flavor = "multi_thread")]
async fn the_browser_return_signs_chatgpt_in() {
    let nonce = Arc::new(Mutex::new(String::new()));
    let issuer = stand_in(Arc::clone(&nonce)).await;
    let port = free_port().to_string();
    let sandbox =
        Sandbox::start_with_env(&[("HARLO_OPENAI_AUTH", &issuer), ("HARLO_OAUTH_PORT", &port)])
            .await;
    let client = sandbox.client();
    let project = sandbox.project();
    let openai = client
        .integration("openai")
        .await
        .expect("an answer")
        .expect("OpenAI");
    assert!(openai.methods.iter().any(|m| matches!(
        m,
        opencode_client::SignInMethod::OAuth { label, .. } if label == "Sign in with ChatGPT"
    )));

    let attempt = client
        .begin_oauth("openai", "chatgpt-token-sharing", &[])
        .await
        .expect("begun");
    let url = reqwest::Url::parse(&attempt.url).expect("URL");
    let q = |k: &str| {
        url.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    assert!(url.as_str().starts_with(&issuer));
    assert_eq!(q("client_id"), "dynamic_agent_client");
    *nonce.lock().expect("nonce") = q("nonce");

    // The browser comes back with the code and the client ID issued.
    let back = format!(
        "{}?code=c1&state={}&client_id=app_test",
        q("redirect_uri"),
        q("state")
    );
    let page = reqwest::get(&back).await.expect("returned");
    assert!(page.status().is_success());
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let s = client
                .oauth_status("openai", &attempt.id)
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
        .integration("openai")
        .await
        .expect("an answer")
        .expect("OpenAI");
    assert_eq!(signed.connections[0].method.as_deref(), Some("oauth"));
    let providers = client
        .providers(project.to_str().expect("utf-8"))
        .await
        .expect("providers");
    assert!(providers.iter().any(|p| p.id == "openai"), "{providers:?}");
}
