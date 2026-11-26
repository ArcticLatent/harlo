//! Signing in with `ChatGPT` against a stand-in for `OpenAI`'s sign-in
//! server: the browser's return finishes the attempt, the tokens are kept
//! as a credential, and `OpenAI`'s models become usable. A token that has
//! run out is refreshed before a model is asked anything, once.

// Rust guideline compliant 2026-10-09

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use harlo_client::{AttemptStatus, Client, Delivery, Message, Prompt};
use support::{Sandbox, free_port};

/// A JSON web token with `claims`, unsigned.
fn jwt(claims: &serde_json::Value) -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    format!("e30.{}.sig", b64.encode(claims.to_string()))
}

/// Serves a token endpoint that answers for `nonce` on a free port;
/// returns its address.
async fn stand_in(nonce: Arc<Mutex<String>>) -> String {
    stand_in_lasting(nonce, 3600, Arc::default()).await
}

/// A token endpoint as [`stand_in`] serves one, whose sign-in token lasts
/// `first_lasts` seconds. It counts the refreshes asked of it in
/// `refreshes`; a refreshed token lasts an hour.
async fn stand_in_lasting(
    nonce: Arc<Mutex<String>>,
    first_lasts: u64,
    refreshes: Arc<AtomicUsize>,
) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = format!("http://{}", listener.local_addr().expect("address"));
    let app = axum::Router::new().route(
        "/api/accounts/oauth/token",
        axum::routing::post(move |form: String| {
            let nonce = nonce.lock().map(|n| n.clone()).unwrap_or_default();
            let refresh = form.contains("grant_type=refresh_token");
            let number = if refresh {
                refreshes.fetch_add(1, Ordering::SeqCst) + 2
            } else {
                1
            };
            async move {
                // (A real refresh takes a moment: time for a second one
                // to start, were nothing in its way.)
                if refresh {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                axum::Json(serde_json::json!({
                    "access_token": format!("access-{number}"),
                    "refresh_token": format!("refresh-{number}"),
                    "expires_in": if refresh { 3600 } else { first_lasts },
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

/// Signs `client`'s server in to `ChatGPT` through the stand-in at
/// `issuer`, as the browser's return does.
async fn sign_in(client: &Client, issuer: &str, nonce: &Mutex<String>) {
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
    assert!(url.as_str().starts_with(issuer));
    *nonce.lock().expect("nonce") = q("nonce");
    let back = format!(
        "{}?code=c1&state={}&client_id=app_test",
        q("redirect_uri"),
        q("state")
    );
    assert!(
        reqwest::get(&back)
            .await
            .expect("returned")
            .status()
            .is_success()
    );
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let s = client
                .oauth_status("openai", &attempt.id)
                .await
                .expect("status");
            if !matches!(s, AttemptStatus::Pending(_)) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("it ended");
    assert_eq!(status, AttemptStatus::Complete);
}

/// Waits until session `id`'s run has ended.
async fn ended(client: &Client, id: &str) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = client.messages(id).await.expect("messages");
            if messages.iter().any(|m| matches!(m, Message::Idle { .. })) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the run ended");
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
        harlo_client::SignInMethod::OAuth { label, .. } if label == "Sign in with ChatGPT"
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

#[tokio::test(flavor = "multi_thread")]
async fn a_token_that_ran_out_is_refreshed_once_before_a_model_is_asked() {
    let nonce = Arc::new(Mutex::new(String::new()));
    let refreshes = Arc::new(AtomicUsize::new(0));
    // The sign-in's token is as good as run out: it lasts less than the
    // margin a token is refreshed in.
    let issuer = stand_in_lasting(Arc::clone(&nonce), 30, Arc::clone(&refreshes)).await;
    let port = free_port().to_string();
    let sandbox =
        Sandbox::start_with_env(&[("HARLO_OPENAI_AUTH", &issuer), ("HARLO_OAUTH_PORT", &port)])
            .await;
    let client = sandbox.client();
    let project = sandbox.project().to_str().expect("utf-8").to_owned();
    sign_in(&client, &issuer, &nonce).await;
    // The timer that refreshes tokens ran when the server started and
    // comes round again in a minute: nothing has refreshed this one.
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);

    // Two sessions set to work at the same moment, as after a restart.
    let first = client.create_session(&project).await.expect("session");
    let second = client.create_session(&project).await.expect("session");
    let say = |id: String| {
        let client = client.clone();
        async move {
            client
                .prompt(&id, &Prompt::new("say Done.", Delivery::Queue))
                .await
                .expect("prompt");
        }
    };
    tokio::join!(say(first.id.clone()), say(second.id.clone()));
    ended(&client, &first.id).await;
    ended(&client, &second.id).await;

    // Refreshed before either model was asked, and by one request: a
    // refresh token works once.
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    // The sign-in is still there, and the next prompt finds it fresh.
    let signed = client
        .integration("openai")
        .await
        .expect("an answer")
        .expect("OpenAI");
    assert_eq!(signed.connections.len(), 1);
    client
        .prompt(&first.id, &Prompt::new("say Again.", Delivery::Queue))
        .await
        .expect("prompt");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
}
