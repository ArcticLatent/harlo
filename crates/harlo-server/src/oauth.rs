//! Browser sign-ins: "Sign in with `ChatGPT`", `OpenAI`'s sign-in for
//! open-source apps, whose tokens let a `ChatGPT` plan pay for requests to
//! the Responses API.
//!
//! The server listens on loopback for the browser's return; when the
//! browser is on another computer, the address it ended on is pasted
//! instead. The first sign-in registers this app and `OpenAI` gives it a
//! client ID, kept and used from then on. Access tokens last an hour: a
//! ticker refreshes them before they run out.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use harlo_engine::{ascending_id, now};
use serde_json::{Value, json};
use sha2::Digest as _;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::server::{Refusal, Server, lock};

/// The method's ID and label, as clients list it.
pub(crate) const CHATGPT: &str = "chatgpt-token-sharing";

/// What clients show for it.
pub(crate) const CHATGPT_LABEL: &str = "Sign in with ChatGPT";

/// `OpenAI`'s sign-in server (`HARLO_OPENAI_AUTH` names a stand-in, for
/// tests).
fn issuer() -> String {
    std::env::var("HARLO_OPENAI_AUTH")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .map_or_else(
            || "https://auth.openai.com".to_owned(),
            |u| u.trim_end_matches('/').to_owned(),
        )
}

/// Where the tokens are used.
const RESOURCE: &str = "https://api.openai.com/v1";

/// The client ID a first sign-in registers from.
const REGISTER: &str = "dynamic_agent_client";

/// The scopes asked for; the last lets the plan pay.
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

/// The scope without which the plan can't be used.
const DIRECT: &str = "chatgpt.tokens.use.direct";

/// The loopback port the browser comes back to (`HARLO_OAUTH_PORT` sets
/// another, for tests).
fn port() -> u16 {
    std::env::var("HARLO_OAUTH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1455)
}

/// The path the browser comes back to.
const CALLBACK: &str = "/auth/callback";

/// How long an attempt waits for the browser.
const WAIT: Duration = Duration::from_mins(10);

/// How long before expiry a token is refreshed.
const MARGIN_MS: i64 = 5 * 60 * 1000;

/// A browser sign-in under way.
pub(crate) struct Attempt {
    /// `pending`, `complete` or `failed`, and why it failed.
    status: (String, Option<String>),
    /// Takes a pasted address or code.
    pasted: mpsc::UnboundedSender<String>,
    /// Stops it.
    cancel: CancellationToken,
    created: i64,
}

/// What came back from the browser.
#[derive(Debug, PartialEq, Eq)]
enum Callback {
    /// The code, and the client ID issued on registration.
    Code(String, Option<String>),
    /// The person declined, with the reason given.
    Declined(String),
    /// For another attempt.
    Stale,
    /// Not the callback.
    Other,
}

/// Where a browser sign-in comes back, and what it connects.
#[derive(Debug, Clone)]
pub(crate) struct Return {
    /// The path of the address the browser comes back to.
    pub(crate) path: String,
    /// What the sign-in connects, as its last page names it.
    pub(crate) what: String,
}

/// Reads the callback address `url` for attempt `state`, which comes back
/// to `path`.
fn read_callback(url: &reqwest::Url, path: &str, state: &str) -> Callback {
    if url.path() != path {
        return Callback::Other;
    }
    let q = |k: &str| {
        url.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(err) = q("error") {
        return Callback::Declined(err);
    }
    if q("state").as_deref() != Some(state) {
        return Callback::Stale;
    }
    match q("code") {
        Some(code) => Callback::Code(code, q("client_id").filter(|c| !c.is_empty())),
        None => Callback::Other,
    }
}

/// `n` random bytes, URL-safe.
fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    let _ = getrandom::fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The claims of a JSON web token, unchecked.
fn claims(jwt: &str) -> Option<Value> {
    let payload = jwt.split('.').nth(1)?;
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?).ok()
}

/// A page for the browser after its return.
fn page(ok: bool, text: &str) -> String {
    let title = if ok { "Signed in" } else { "Not signed in" };
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Harlo</title>\
         <body style=\"font-family:sans-serif;padding:3em\"><h2>{title}</h2><p>{text}</p></body>"
    );
    let status = if ok { "200 OK" } else { "400 Bad Request" };
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Asks `OpenAI`'s token endpoint with `form`.
async fn token_request(form: &[(&str, &str)]) -> Result<Value, (String, String)> {
    let url = format!("{}/api/accounts/oauth/token", issuer());
    let response = reqwest::Client::new()
        .post(url)
        .header("accept", "application/json")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(
            reqwest::Url::parse_with_params("http://form.invalid/", form)
                .ok()
                .and_then(|u| u.query().map(str::to_owned))
                .unwrap_or_default(),
        )
        .timeout(Duration::from_mins(1))
        .send()
        .await
        .map_err(|e| {
            (
                String::new(),
                format!("Couldn't reach OpenAI's sign-in server: {e}"),
            )
        })?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(body);
    }
    let code = body["error"]
        .as_str()
        .or_else(|| body["error"]["code"].as_str())
        .unwrap_or_default()
        .to_owned();
    Err((
        code,
        format!(
            "OpenAI's sign-in server refused ({}): {body}",
            status.as_u16()
        ),
    ))
}

/// The credential tokens `token` make for client `client`.
fn credential(token: &Value, client: &str, email: &str) -> Result<Value, String> {
    let text = |k: &str| {
        token[k]
            .as_str()
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| format!("OpenAI's answer has no {k}."))
    };
    if !text("scope")?.split_whitespace().any(|s| s == DIRECT) {
        return Err(
            "Your ChatGPT plan's use wasn't granted: sign in again and allow it.".to_owned(),
        );
    }
    let seconds = token["expires_in"]
        .as_i64()
        .ok_or_else(|| "OpenAI's answer has no expiry.".to_owned())?;
    Ok(json!({
        "type": "oauth", "methodID": CHATGPT, "access": text("access_token")?,
        "refresh": text("refresh_token")?, "expires": now() + seconds * 1000,
        "metadata": { "clientID": client, "email": email },
    }))
}

/// Everything one sign-in needs to finish.
struct Flow {
    verifier: String,
    state: String,
    nonce: String,
    redirect: String,
    saved: Option<String>,
}

impl Server {
    /// This computer's ID for `OpenAI`, made once and kept.
    fn host_id(&self) -> String {
        if let Ok(Some(id)) = self.store.setting("oauth/openai-host") {
            return id;
        }
        let mut b = [0_u8; 16];
        let _ = getrandom::fill(&mut b);
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        let h = hex::encode(b);
        let id = format!(
            "urn:uuid:{}-{}-{}-{}-{}",
            &h[0..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..32]
        );
        let _ = self.store.put_setting("oauth/openai-host", &id);
        id
    }

    /// Starts browser sign-in `method` to integration `id`.
    ///
    /// # Errors
    ///
    /// When there's no such method, or the port the browser comes back to
    /// is taken.
    pub(crate) async fn begin_oauth(
        self: &Arc<Self>,
        id: &str,
        method: &str,
    ) -> Result<Value, Refusal> {
        if id.starts_with(crate::mcp_auth::PREFIX) {
            return self.begin_mcp_oauth(id).await;
        }
        if id != "openai" || method != CHATGPT {
            return Err(Refusal::NotFound(format!(
                "Integration method not found: {method}"
            )));
        }
        let port = port();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|e| {
                Refusal::Bad(format!(
                    "Port {port} is in use ({e}): another sign-in is probably waiting there. \
                     Finish or cancel it and try again."
                ))
            })?;
        let saved = self.store.setting("oauth/openai-client").ok().flatten();
        let flow = Flow {
            verifier: random_token(),
            state: random_token(),
            nonce: random_token(),
            redirect: format!("http://127.0.0.1:{port}{CALLBACK}"),
            saved,
        };
        let challenge = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(flow.verifier.as_bytes()));
        let host = self.host_id();
        let mut params: Vec<(&str, &str)> = vec![
            ("client_id", flow.saved.as_deref().unwrap_or(REGISTER)),
            ("ext_agent_host_id", &host),
            ("response_type", "code"),
            ("redirect_uri", &flow.redirect),
            ("resource", RESOURCE),
            ("scope", SCOPE),
            ("state", &flow.state),
            ("nonce", &flow.nonce),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ];
        if flow.saved.is_none() {
            params.push(("agent_name_hint", "harlo"));
        }
        let url = reqwest::Url::parse_with_params(
            &format!("{}/api/accounts/authorize", issuer()),
            &params,
        )
        .map_err(|e| Refusal::Bad(e.to_string()))?;
        let back = Return {
            path: CALLBACK.to_owned(),
            what: "your ChatGPT plan".to_owned(),
        };
        let server = Arc::clone(self);
        let (attempt, created) = self.track(move |taken| async move {
            let (code, issued) = wait(&listener, taken, &flow.state, &back).await?;
            server.finish_sign_in(&flow, &code, issued).await
        });
        Ok(json!({
            "attemptID": attempt, "url": url.as_str(), "mode": "auto", "paste": true,
            "instructions": "Sign in in the browser. If it's on another computer, paste the \
                             address its last page ended on.",
            "time": { "created": created, "expires": created + 600_000 },
        }))
    }

    /// Trades the browser's code for tokens and keeps them.
    async fn finish_sign_in(
        &self,
        flow: &Flow,
        code: &str,
        issued: Option<String>,
    ) -> Result<(), String> {
        let client = issued.or_else(|| flow.saved.clone()).ok_or_else(|| {
            "OpenAI's answer didn't include this app's client ID; try again.".to_owned()
        })?;
        let token = token_request(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client),
            ("code", code),
            ("code_verifier", &flow.verifier),
            ("redirect_uri", &flow.redirect),
            ("resource", RESOURCE),
        ])
        .await
        .map_err(|(_, why)| why)?;
        // The ID token comes straight from the token endpoint over TLS, so
        // its signature isn't checked (OpenID Connect allows that here);
        // its nonce, audience and expiry are.
        let id = claims(token["id_token"].as_str().unwrap_or_default())
            .ok_or_else(|| "OpenAI's answer has no ID token.".to_owned())?;
        if id["nonce"].as_str() != Some(flow.nonce.as_str()) {
            return Err("The sign-in's nonce doesn't match: try again.".to_owned());
        }
        let ours = match &id["aud"] {
            Value::String(a) => a == &client,
            Value::Array(a) => a.iter().any(|v| v.as_str() == Some(client.as_str())),
            _ => false,
        };
        if !ours || id["exp"].as_i64().is_some_and(|e| e * 1000 < now()) {
            return Err(
                "The sign-in's ID token isn't for this app or has expired: try again.".to_owned(),
            );
        }
        let value = credential(&token, &client, id["email"].as_str().unwrap_or_default())?;
        let _ = self.store.put_setting("oauth/openai-client", &client);
        self.store
            .put_credential("openai", "ChatGPT", &value)
            .map_err(|e| e.to_string())?;
        self.integrations_changed();
        Ok(())
    }

    /// Runs browser sign-in `run` as a new attempt, which ends when `run`
    /// does, is cancelled, or takes too long; returns the attempt's ID and
    /// when it began. `run` is handed what's pasted for it.
    pub(crate) fn track<F>(
        self: &Arc<Self>,
        run: impl FnOnce(mpsc::UnboundedReceiver<String>) -> F,
    ) -> (String, i64)
    where
        F: Future<Output = Result<(), String>> + Send + 'static,
    {
        let attempt = ascending_id("att");
        let (pasted, taken) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let created = now();
        lock(&self.attempts).insert(
            attempt.clone(),
            Attempt {
                status: ("pending".to_owned(), None),
                pasted,
                cancel: cancel.clone(),
                created,
            },
        );
        let running = run(taken);
        let (server, key) = (Arc::clone(self), attempt.clone());
        tokio::spawn(async move {
            let ended = tokio::select! {
                got = running => got,
                () = cancel.cancelled() => Err("The sign-in was cancelled.".to_owned()),
                () = tokio::time::sleep(WAIT) => Err("The sign-in took too long; start it again.".to_owned()),
            };
            if let Some(a) = lock(&server.attempts).get_mut(&key) {
                a.status = match ended {
                    Ok(()) => ("complete".to_owned(), None),
                    Err(why) => ("failed".to_owned(), Some(why)),
                };
            }
        });
        (attempt, created)
    }

    /// How sign-in attempt `attempt` stands.
    ///
    /// # Errors
    ///
    /// When there's no such attempt.
    pub(crate) fn oauth_status(&self, attempt: &str) -> Result<Value, Refusal> {
        let attempts = lock(&self.attempts);
        let a = attempts
            .get(attempt)
            .ok_or_else(|| Refusal::NotFound(format!("There's no sign-in attempt {attempt}.")))?;
        let mut status = json!({
            "status": a.status.0,
            "time": { "created": a.created, "expires": a.created + 600_000 },
        });
        if let Some(why) = &a.status.1 {
            status["message"] = json!(why);
        }
        Ok(status)
    }

    /// Hands attempt `attempt` the address the browser ended on, or its
    /// code.
    ///
    /// # Errors
    ///
    /// When there's no such attempt, or it's over.
    pub(crate) fn complete_oauth(&self, attempt: &str, code: &str) -> Result<(), Refusal> {
        let attempts = lock(&self.attempts);
        let a = attempts
            .get(attempt)
            .ok_or_else(|| Refusal::NotFound(format!("There's no sign-in attempt {attempt}.")))?;
        a.pasted
            .send(code.trim().to_owned())
            .map_err(|_over| Refusal::Bad("That sign-in is over.".to_owned()))
    }

    /// Cancels attempt `attempt`.
    pub(crate) fn cancel_oauth(&self, attempt: &str) {
        if let Some(a) = lock(&self.attempts).remove(attempt) {
            a.cancel.cancel();
        }
    }

    /// Refreshes the `ChatGPT` tokens about to run out; forgets a sign-in
    /// `OpenAI` ended.
    async fn refresh_tokens(&self) {
        let Ok(credentials) = self.store.credentials() else {
            return;
        };
        for c in credentials {
            let value = &c["value"];
            if value["type"] != "oauth"
                || value["methodID"] != CHATGPT
                || value["expires"]
                    .as_i64()
                    .is_some_and(|e| e > now() + MARGIN_MS)
            {
                continue;
            }
            let client = value["metadata"]["clientID"].as_str().unwrap_or_default();
            let refreshed = token_request(&[
                ("grant_type", "refresh_token"),
                ("client_id", client),
                (
                    "refresh_token",
                    value["refresh"].as_str().unwrap_or_default(),
                ),
                ("resource", RESOURCE),
            ])
            .await;
            let email = value["metadata"]["email"].as_str().unwrap_or_default();
            let integration = c["integrationID"].as_str().unwrap_or_default();
            let label = c["label"].as_str().unwrap_or_default();
            match refreshed.map(|t| credential(&t, client, email)) {
                Ok(Ok(next)) => {
                    let _ = self.store.put_credential(integration, label, &next);
                }
                Err((code, _))
                    if code.starts_with("invalid_")
                        || code.contains("expired")
                        || code.contains("reused") =>
                {
                    tracing::warn!(name: "oauth.ended", code = %code, "OpenAI ended the ChatGPT sign-in");
                    let _ = self
                        .store
                        .delete_credential(c["id"].as_str().unwrap_or_default());
                    self.integrations_changed();
                }
                Ok(Err(why)) | Err((_, why)) => {
                    tracing::warn!(name: "oauth.refresh.failed", error = %why, "trying again in a minute");
                }
            }
        }
    }

    /// Starts refreshing sign-in tokens every minute.
    pub fn start_refreshing(self: &Arc<Self>) {
        let server = Arc::clone(self);
        tokio::spawn(async move {
            let mut every = tokio::time::interval(Duration::from_mins(1));
            loop {
                every.tick().await;
                server.refresh_tokens().await;
            }
        });
    }
}

/// Waits for the browser's return to `back` on `listener`, or a pasted
/// address on `pasted`; returns the code and any client ID issued.
pub(crate) async fn wait(
    listener: &tokio::net::TcpListener,
    mut pasted: mpsc::UnboundedReceiver<String>,
    state: &str,
    back: &Return,
) -> Result<(String, Option<String>), String> {
    loop {
        let mut socket = tokio::select! {
            got = listener.accept() => match got {
                Ok((socket, _)) => socket,
                Err(_) => continue,
            },
            line = pasted.recv() => {
                let Some(line) = line else { std::future::pending::<()>().await; continue };
                // A bare code, as some pages show it, or the address.
                let parsed = reqwest::Url::parse(&line).map_or_else(
                    |_bare| Callback::Code(line.clone(), None),
                    |u| read_callback(&u, &back.path, state),
                );
                match parsed {
                    Callback::Code(code, issued) => return Ok((code, issued)),
                    Callback::Declined(why) => return Err(format!("The sign-in was declined ({why}).")),
                    Callback::Stale | Callback::Other => continue,
                }
            }
        };
        let mut buf = vec![0_u8; 8192];
        let n = socket.read(&mut buf).await.unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]);
        let target = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or_default()
            .to_owned();
        let Ok(url) = reqwest::Url::parse(&format!("http://127.0.0.1{target}")) else {
            continue;
        };
        match read_callback(&url, &back.path, state) {
            Callback::Other => {
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            }
            Callback::Declined(why) => {
                let _ = socket
                    .write_all(
                        page(
                            false,
                            &format!(
                                "Harlo wasn't connected to {}. You can close this tab.",
                                back.what
                            ),
                        )
                        .as_bytes(),
                    )
                    .await;
                return Err(format!("The sign-in was declined ({why})."));
            }
            Callback::Stale => {
                let _ = socket
                    .write_all(
                        page(
                            false,
                            "This sign-in link is out of date. Start again from Harlo.",
                        )
                        .as_bytes(),
                    )
                    .await;
            }
            Callback::Code(code, issued) => {
                let _ = socket
                    .write_all(
                        page(
                            true,
                            &format!(
                                "Harlo is connected to {}. You can close this tab.",
                                back.what
                            ),
                        )
                        .as_bytes(),
                    )
                    .await;
                return Ok((code, issued));
            }
        }
    }
}

/// The attempts under way, by ID.
pub(crate) type Attempts = std::sync::Mutex<HashMap<String, Attempt>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_callback_reads() {
        let u = |s: &str| reqwest::Url::parse(s).expect("URL");
        assert_eq!(
            read_callback(
                &u("http://127.0.0.1:1455/auth/callback?code=c1&state=s1&client_id=app_x"),
                CALLBACK,
                "s1"
            ),
            Callback::Code("c1".into(), Some("app_x".into()))
        );
        assert_eq!(
            read_callback(
                &u("http://localhost:1455/auth/callback?code=c1&state=old"),
                CALLBACK,
                "s1"
            ),
            Callback::Stale
        );
        assert_eq!(
            read_callback(
                &u("http://localhost:1455/auth/callback?error=access_denied&state=s1"),
                CALLBACK,
                "s1"
            ),
            Callback::Declined("access_denied".into())
        );
        assert_eq!(
            read_callback(&u("http://localhost:1455/favicon.ico"), CALLBACK, "s1"),
            Callback::Other
        );
    }

    #[test]
    fn tokens_need_the_plan_scope() {
        let token = json!({ "access_token": "a", "refresh_token": "r", "expires_in": 3600, "scope": "openid" });
        credential(&token, "app", "e").unwrap_err();
        let token = json!({ "access_token": "a", "refresh_token": "r", "expires_in": 3600, "scope": SCOPE });
        let kept = credential(&token, "app", "e").expect("kept");
        assert_eq!(
            (kept["access"].as_str(), kept["methodID"].as_str()),
            (Some("a"), Some(CHATGPT))
        );
    }
}
