//! A stand-in for a remote MCP server that needs OAuth, with its own
//! authorization server: metadata, client registration, an authorization
//! page that sends the browser straight back with a code, a token
//! endpoint, and an MCP endpoint that answers only with the token. Shared
//! by harlo-server's tests and the app's.

// Rust guideline compliant 2026-10-09

use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

/// What the stand-in saw.
#[derive(Default)]
pub struct Seen {
    /// The registrations asked for.
    pub registered: Vec<Value>,
    /// How many times its tools were listed with the token.
    pub authorized_lists: usize,
}

/// The stand-in authorization and MCP server.
#[derive(Clone)]
struct Guard {
    base: String,
    seen: Arc<Mutex<Seen>>,
}

/// The access token the stand-in hands out and wants.
const TOKEN: &str = "tok-1";

async fn resource(State(g): State<Guard>) -> Json<Value> {
    Json(json!({ "resource": format!("{}/mcp", g.base), "authorization_servers": [g.base] }))
}

async fn authority(State(g): State<Guard>) -> Json<Value> {
    let b = &g.base;
    Json(json!({
        "issuer": b,
        "authorization_endpoint": format!("{b}/authorize"),
        "token_endpoint": format!("{b}/token"),
        "registration_endpoint": format!("{b}/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
    }))
}

async fn register(State(g): State<Guard>, Json(body): Json<Value>) -> Response {
    g.seen.lock().expect("seen").registered.push(body.clone());
    let answer = json!({
        "client_id": "harlo-test", "client_name": body["client_name"],
        "redirect_uris": body["redirect_uris"], "token_endpoint_auth_method": "none",
    });
    (StatusCode::CREATED, Json(answer)).into_response()
}

async fn authorize(Query(q): Query<Vec<(String, String)>>) -> Response {
    let get = |k: &str| {
        q.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(get("code_challenge_method"), "S256");
    let back = reqwest::Url::parse_with_params(
        &get("redirect_uri"),
        &[("code", "the-code"), ("state", &get("state"))],
    )
    .expect("redirect");
    (StatusCode::FOUND, [(header::LOCATION, back.to_string())]).into_response()
}

async fn token(body: String) -> Response {
    let form = reqwest::Url::parse(&format!("http://form.invalid/?{body}")).expect("form");
    let get = |k: &str| {
        form.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    if get("grant_type") != "authorization_code"
        || get("code") != "the-code"
        || get("code_verifier").is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_grant" })),
        )
            .into_response();
    }
    Json(json!({
        "access_token": TOKEN, "token_type": "Bearer", "expires_in": 3600,
        "refresh_token": "ref-1",
    }))
    .into_response()
}

async fn mcp(State(g): State<Guard>, headers: HeaderMap, Json(msg): Json<Value>) -> Response {
    let bearer = format!("Bearer {TOKEN}");
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        != Some(&bearer)
    {
        let challenge = format!(
            "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\"",
            g.base
        );
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, challenge)],
        )
            .into_response();
    }
    let Some(id) = msg.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match msg["method"].as_str() {
        Some("initialize") => json!({
            "protocolVersion": msg["params"]["protocolVersion"],
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "guarded", "version": "1" },
        }),
        Some("tools/list") => {
            g.seen.lock().expect("seen").authorized_lists += 1;
            json!({ "tools": [{ "name": "whoami", "description": "Says who.",
                                "inputSchema": { "type": "object" } }] })
        }
        _ => json!({}),
    };
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

/// Starts the stand-in; returns its base URL and what it saw.
pub async fn guard() -> (String, Arc<Mutex<Seen>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("address"));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let state = Guard {
        base: base.clone(),
        seen: Arc::clone(&seen),
    };
    let app = Router::new()
        .route("/.well-known/oauth-protected-resource", get(resource))
        .route("/.well-known/oauth-protected-resource/mcp", get(resource))
        .route("/.well-known/oauth-authorization-server", get(authority))
        .route("/register", post(register))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route(
            "/mcp",
            post(mcp).get(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        )
        .with_state(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, seen)
}

/// What a browser does with sign-in address `url`: goes to the
/// authorization server, then back where it sends it; returns that address
/// and the page there.
pub async fn browse(url: &str) -> (String, String) {
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("browser");
    let sent = browser.get(url).send().await.expect("authorize");
    let back = sent.headers()[reqwest::header::LOCATION]
        .to_str()
        .expect("location")
        .to_owned();
    let page = browser
        .get(&back)
        .send()
        .await
        .expect("callback")
        .text()
        .await
        .expect("page");
    (back, page)
}
