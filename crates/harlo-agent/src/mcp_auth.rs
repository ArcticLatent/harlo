//! Signing in to MCP servers at an address (OAuth, as MCP specifies it):
//! `harlo-agent mcp login <name>`.
//!
//! - Finding where: the server's 401 names its resource metadata
//!   (`WWW-Authenticate: Bearer resource_metadata=…`), else the well-known
//!   address beside it; that names the authorization server, whose own
//!   metadata gives its endpoints (a server without any: its origin's
//!   `/authorize`, `/token` and `/register`, as older servers have it).
//! - This app is registered with it once per server (dynamic client
//!   registration), unless the entry gives `oauth.clientId` (and
//!   `clientSecret`).
//! - The browser opens the sign-in (PKCE), which returns to the same
//!   loopback address as the ChatGPT sign-in (so harlo's forward to a host
//!   works for it too), or the address it ended on is pasted.
//! - Tokens are kept in `mcp-auth.json` (only the user can read it) by the
//!   server's address, refreshed before they expire (under a file lock:
//!   helpers share them), and sent as a bearer token.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::auth;

/// A token this close to expiring is refreshed first (s).
const EARLY: u64 = 60;

/// A server's sign-in.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Token {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    /// Unix seconds (0: not said).
    #[serde(default)]
    pub expires_at: u64,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub resource: String,
}

fn path() -> PathBuf {
    auth::config_dir().join("mcp-auth.json")
}

fn read_all() -> BTreeMap<String, Token> {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_all(all: &BTreeMap<String, Token>) -> Result<()> {
    auth::ensure_dir()?;
    auth::write_private(&path(), &serde_json::to_string_pretty(all)?)
}

/// The file lock refreshes and sign-ins take (another process may be
/// refreshing the same token: refresh tokens can be used once).
async fn locked() -> Result<File> {
    tokio::task::spawn_blocking(|| -> Result<File> {
        let f = OpenOptions::new().create(true).truncate(false).write(true).open(auth::ensure_dir()?.join("mcp-auth.lock"))?;
        f.lock()?;
        Ok(f)
    })
    .await?
}

/// The bearer token for `url`, refreshed when it's about to expire (or
/// `force`); None when there's no sign-in, or it couldn't be refreshed.
pub async fn bearer(url: &str, force: bool) -> Option<String> {
    let t = read_all().remove(url)?;
    if !force && (t.expires_at == 0 || t.expires_at > auth::now() + EARLY) {
        return Some(t.access_token);
    }
    if t.refresh_token.is_empty() {
        return (!force).then_some(t.access_token);
    }
    let _lock = locked().await.ok()?;
    // (Another process may have refreshed it meanwhile.)
    let mut all = read_all();
    let now = all.get(url)?.clone();
    if now.access_token != t.access_token && now.expires_at > auth::now() + EARLY {
        return Some(now.access_token);
    }
    let mut form = vec![("grant_type", "refresh_token"), ("refresh_token", now.refresh_token.as_str()), ("client_id", now.client_id.as_str())];
    if !now.resource.is_empty() {
        form.push(("resource", now.resource.as_str()));
    }
    if !now.client_secret.is_empty() {
        form.push(("client_secret", now.client_secret.as_str()));
    }
    let got = token_request(&now.token_endpoint, &form).await.ok()?;
    let fresh = Token {
        access_token: got["access_token"].as_str()?.to_owned(),
        refresh_token: got["refresh_token"].as_str().map(str::to_owned).unwrap_or(now.refresh_token.clone()),
        expires_at: got["expires_in"].as_u64().map_or(0, |s| auth::now() + s),
        ..now
    };
    all.insert(url.to_owned(), fresh.clone());
    write_all(&all).ok()?;
    Some(fresh.access_token)
}

/// Forgets the sign-in for `url`: whether there was one.
pub fn forget(url: &str) -> Result<bool> {
    let mut all = read_all();
    let had = all.remove(url).is_some();
    if had {
        write_all(&all)?;
    }
    Ok(had)
}

async fn token_request(url: &str, form: &[(&str, &str)]) -> Result<Value> {
    let resp = crate::responses::client().post(url).header("accept", "application/json").form(form).send().await.with_context(|| format!("reaching {url}"))?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        bail!("{url} refused ({status}): {}", body["error_description"].as_str().or_else(|| body["error"].as_str()).unwrap_or("no reason given"));
    }
    if body["access_token"].as_str().is_none_or(str::is_empty) {
        bail!("{url} answered with no access token");
    }
    Ok(body)
}

/// `resource_metadata="…"` out of a `WWW-Authenticate` header.
fn resource_metadata(header: &str) -> Option<String> {
    let at = header.find("resource_metadata=")? + "resource_metadata=".len();
    let rest = &header[at..];
    let v = match rest.strip_prefix('"') {
        Some(q) => q.split('"').next()?,
        None => rest.split([',', ' ']).next()?,
    };
    (!v.is_empty()).then(|| v.to_owned())
}

/// `https://h:1/a/b` → (`https://h:1`, `/a/b`).
fn split_origin(url: &str) -> (String, String) {
    let start = url.find("://").map_or(0, |i| i + 3);
    match url[start..].find('/') {
        Some(i) => (url[..start + i].to_owned(), url[start + i..].trim_end_matches('/').to_owned()),
        None => (url.to_owned(), String::new()),
    }
}

async fn get_json(url: &str) -> Option<Value> {
    let resp = crate::responses::client().get(url).header("Accept", "application/json").send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().await.ok().filter(Value::is_object)
}

/// Where the server says to sign in: (its resource metadata, if any).
async fn discover_resource(url: &str, kind_sse: bool, headers: &[(String, String)]) -> Result<Option<Value>> {
    let client = crate::responses::client();
    let mut req = if kind_sse {
        client.get(url).header("Accept", "text/event-stream")
    } else {
        client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "harlo-agent", "version": env!("CARGO_PKG_VERSION") } } }).to_string())
    };
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = tokio::time::timeout(std::time::Duration::from_secs(30), req.send()).await.map_err(|_| anyhow!("{url} didn't answer"))?.with_context(|| format!("reaching {url}"))?;
    let status = resp.status().as_u16();
    if status != 401 && status != 403 {
        bail!("{url} doesn't ask for a sign-in (it answered HTTP {status})");
    }
    let named = resp.headers().get_all("www-authenticate").iter().filter_map(|v| v.to_str().ok()).find_map(resource_metadata);
    if let Some(m) = named
        && let Some(v) = get_json(&m).await
    {
        return Ok(Some(v));
    }
    let (origin, path) = split_origin(url);
    for at in [format!("{origin}/.well-known/oauth-protected-resource{path}"), format!("{origin}/.well-known/oauth-protected-resource")] {
        if let Some(v) = get_json(&at).await {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// An authorization server's metadata (its endpoints).
async fn server_metadata(issuer: &str) -> Value {
    let (origin, path) = split_origin(issuer);
    let mut tries = vec![format!("{origin}/.well-known/oauth-authorization-server{path}"), format!("{origin}/.well-known/openid-configuration{path}")];
    if !path.is_empty() {
        tries.push(format!("{origin}{path}/.well-known/openid-configuration"));
    }
    for at in tries {
        if let Some(v) = get_json(&at).await
            && v["authorization_endpoint"].is_string()
            && v["token_endpoint"].is_string()
        {
            return v;
        }
    }
    // (An older server: its own origin's addresses.)
    json!({ "authorization_endpoint": format!("{origin}/authorize"), "token_endpoint": format!("{origin}/token"), "registration_endpoint": format!("{origin}/register") })
}

/// What a sign-in to an MCP server needs from its entry.
pub struct Server<'a> {
    pub name: &'a str,
    pub url: &'a str,
    pub sse: bool,
    pub headers: &'a [(String, String)],
    /// `oauth.clientId` / `clientSecret` / `scopes` from the entry.
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub scopes: Option<String>,
}

/// Signs in to `s` in the browser (`open` shows the link); keeps the token.
pub async fn login(s: &Server<'_>, open: impl Fn(&str), paste: bool) -> Result<()> {
    let resource_meta = discover_resource(s.url, s.sse, s.headers).await?;
    let issuer = resource_meta
        .as_ref()
        .and_then(|m| m["authorization_servers"].as_array()?.first()?.as_str().map(str::to_owned))
        .unwrap_or_else(|| split_origin(s.url).0);
    let meta = server_metadata(&issuer).await;
    let authorize = meta["authorization_endpoint"].as_str().unwrap_or_default().to_owned();
    let token_endpoint = meta["token_endpoint"].as_str().unwrap_or_default().to_owned();
    // The server's canonical address, which tokens are asked for.
    let resource = resource_meta.as_ref().and_then(|m| m["resource"].as_str().map(str::to_owned)).unwrap_or_else(|| s.url.to_owned());
    let scope = s.scopes.clone().or_else(|| {
        let listed: Vec<&str> = resource_meta.as_ref()?["scopes_supported"].as_array()?.iter().filter_map(Value::as_str).collect();
        (!listed.is_empty()).then(|| listed.join(" "))
    });
    let redirect = format!("http://127.0.0.1:{}{}", auth::CALLBACK_PORT, auth::CALLBACK_PATH);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", auth::CALLBACK_PORT))
        .await
        .map_err(|e| anyhow!("Port {} is in use ({e}): another sign-in is probably waiting there. Finish or cancel it and try again.", auth::CALLBACK_PORT))?;
    // This app, registered with it (once per server).
    let known = read_all().remove(s.url).filter(|t| t.token_endpoint == token_endpoint);
    let (client_id, client_secret) = match (&s.client_id, known) {
        (Some(id), _) => (id.clone(), s.client_secret.clone().unwrap_or_default()),
        (None, Some(t)) => (t.client_id, t.client_secret),
        (None, None) => {
            let Some(register) = meta["registration_endpoint"].as_str() else {
                bail!("{} doesn't let apps register themselves: give the client id it issued you as oauth.clientId in the server's entry", s.name);
            };
            let body = json!({
                "client_name": "Harlo", "redirect_uris": [redirect], "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"], "token_endpoint_auth_method": "none",
            });
            let resp = crate::responses::client().post(register).json(&body).send().await.with_context(|| format!("reaching {register}"))?;
            let status = resp.status();
            let got: Value = resp.json().await.unwrap_or(Value::Null);
            let id = got["client_id"].as_str().filter(|_| status.is_success()).ok_or_else(|| anyhow!("{register} didn't register Harlo ({status}): {got}"))?;
            (id.to_owned(), got["client_secret"].as_str().unwrap_or_default().to_owned())
        }
    };
    let verifier = auth::random_token();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = auth::random_token();
    let mut params: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", &client_id),
        ("redirect_uri", &redirect),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", &state),
        ("resource", &resource),
    ];
    if let Some(sc) = &scope {
        params.push(("scope", sc));
    }
    let url = reqwest::Url::parse_with_params(&authorize, &params).with_context(|| format!("{authorize} isn't an address"))?;
    open(url.as_str());
    let (code, _) = auth::wait_for_callback(&listener, &state, s.name, &format!("Harlo is signed in to {}. You can close this tab.", s.name), paste).await?;
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("client_id", client_id.as_str()),
        ("code_verifier", verifier.as_str()),
        ("resource", resource.as_str()),
    ];
    if !client_secret.is_empty() {
        form.push(("client_secret", client_secret.as_str()));
    }
    let got = token_request(&token_endpoint, &form).await?;
    let token = Token {
        access_token: got["access_token"].as_str().unwrap_or_default().to_owned(),
        refresh_token: got["refresh_token"].as_str().unwrap_or_default().to_owned(),
        expires_at: got["expires_in"].as_u64().map_or(0, |s| auth::now() + s),
        client_id,
        client_secret,
        token_endpoint,
        resource,
    };
    let _lock = locked().await?;
    let mut all = read_all();
    all.insert(s.url.to_owned(), token);
    write_all(&all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_challenge_and_addresses() {
        assert_eq!(resource_metadata(r#"Bearer realm="x", resource_metadata="https://h/.well-known/oauth-protected-resource/mcp""#).as_deref(), Some("https://h/.well-known/oauth-protected-resource/mcp"));
        assert_eq!(resource_metadata("Bearer resource_metadata=https://h/m, error=x").as_deref(), Some("https://h/m"));
        assert_eq!(resource_metadata("Bearer realm=x"), None);
        assert_eq!(split_origin("https://h:1/a/b/"), ("https://h:1".into(), "/a/b".into()));
        assert_eq!(split_origin("http://h"), ("http://h".into(), String::new()));
    }
}
