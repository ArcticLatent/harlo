//! Sign in with ChatGPT (OpenAI's flow for open-source apps): the user's
//! ChatGPT plan pays for requests to the public Responses API.
//!
//! The first sign-in registers this app (`dynamic_agent_client`); OpenAI
//! answers with a client id that's kept and used from then on. Each machine
//! has its own host id. Access tokens last an hour and refresh tokens rotate
//! on every refresh, so refreshes are serialized across processes (helpers
//! share one sign-in) with a file lock, and the file is re-read under it.

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const AUTHORIZE_URL: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
pub const RESOURCE: &str = "https://api.openai.com/v1";
const REGISTER_CLIENT: &str = "dynamic_agent_client";
const AGENT_NAME: &str = "harlo";
const DIRECT_SCOPE: &str = "chatgpt.tokens.use.direct";
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const CALLBACK_PORT: u16 = 1455;
const CALLBACK_PATH: &str = "/auth/callback";
/// Refreshed this long before it expires, so no request starts with a
/// token about to run out.
const MARGIN: Duration = Duration::from_secs(180);

/// What's kept of a sign-in (`chatgpt.json`, only the user can read it).
#[derive(Serialize, Deserialize, Clone)]
pub struct Credentials {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: u64,
    pub scopes: Vec<String>,
    /// From the ID token, to show who's signed in.
    #[serde(default)]
    pub email: String,
}

/// Where harlo-agent keeps its settings and sign-in.
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".config"));
    base.join("harlo-agent")
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn creds_path() -> PathBuf {
    config_dir().join("chatgpt.json")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("the system's random source");
    b
}

fn random_token() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes::<32>())
}

fn ensure_dir() -> Result<PathBuf> {
    let dir = config_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}

/// Writes `text` to `path` readable only by the user, replacing it whole.
fn write_private(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp).with_context(|| format!("writing {}", tmp.display()))?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// This machine's id for OpenAI (`urn:uuid:…`), made once and kept.
fn host_id() -> Result<String> {
    let path = ensure_dir()?.join("host-id");
    if let Ok(id) = fs::read_to_string(&path) {
        let id = id.trim();
        if id.starts_with("urn:uuid:") {
            return Ok(id.to_owned());
        }
    }
    let mut b = random_bytes::<16>();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let id = format!("urn:uuid:{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]);
    write_private(&path, &id)?;
    Ok(id)
}

pub fn load() -> Option<Credentials> {
    serde_json::from_str(&fs::read_to_string(creds_path()).ok()?).ok()
}

fn save(c: &Credentials) -> Result<()> {
    ensure_dir()?;
    write_private(&creds_path(), &serde_json::to_string_pretty(c)?)
}

/// The client id OpenAI issued on an earlier sign-in (kept after sign-out).
fn saved_client_id() -> Option<String> {
    let path = config_dir().join("client-id");
    fs::read_to_string(path).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// Forgets the tokens here (the registration's client id stays). The user
/// can disconnect harlo in ChatGPT's settings too.
pub fn sign_out() -> Result<bool> {
    match fs::remove_file(creds_path()) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Signs in in the browser: a listener on 127.0.0.1:1455 takes the
/// callback. `open` gets the URL to show or open.
pub async fn sign_in(open: impl Fn(&str)) -> Result<Credentials> {
    let host = host_id()?;
    let saved = saved_client_id();
    let verifier = random_token();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_token();
    let nonce = random_token();
    let redirect = format!("http://127.0.0.1:{CALLBACK_PORT}{CALLBACK_PATH}");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| anyhow!("Port {CALLBACK_PORT} is in use ({e}): another app's sign-in is probably waiting there. Finish or cancel it and try again."))?;
    let mut params: Vec<(&str, &str)> = vec![
        ("client_id", saved.as_deref().unwrap_or(REGISTER_CLIENT)),
        ("ext_agent_host_id", &host),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        ("resource", RESOURCE),
        ("scope", SCOPE),
        ("state", &state),
        ("nonce", &nonce),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ];
    if saved.is_none() {
        params.push(("agent_name_hint", AGENT_NAME));
    }
    let url = reqwest::Url::parse_with_params(AUTHORIZE_URL, &params)?;
    open(url.as_str());
    let (code, issued) = wait_for_callback(&listener, &state).await?;
    let client_id = issued.or(saved).ok_or_else(|| anyhow!("OpenAI's answer didn't include this app's client id; try again"))?;
    let token = token_request(&[
        ("grant_type", "authorization_code"),
        ("client_id", &client_id),
        ("code", &code),
        ("code_verifier", &verifier),
        ("redirect_uri", &redirect),
        ("resource", RESOURCE),
    ])
    .await?;
    // The ID token comes straight from the token endpoint over TLS, so its
    // signature isn't checked here (OpenID Connect allows that for this
    // flow); its nonce, audience and expiry are.
    let id = token["id_token"].as_str().ok_or_else(|| anyhow!("OpenAI's answer has no ID token"))?;
    let claims = jwt_claims(id)?;
    if claims["nonce"].as_str() != Some(nonce.as_str()) {
        bail!("The sign-in's nonce doesn't match: try again");
    }
    let aud_ok = match &claims["aud"] {
        Value::String(a) => a == &client_id,
        Value::Array(a) => a.iter().any(|v| v.as_str() == Some(&client_id)),
        _ => false,
    };
    if !aud_ok || claims["exp"].as_u64().is_some_and(|e| e < now()) {
        bail!("The sign-in's ID token isn't for this app or has expired: try again");
    }
    let creds = credentials(&token, &client_id, claims["email"].as_str().unwrap_or_default())?;
    write_private(&ensure_dir()?.join("client-id"), &client_id)?;
    save(&creds)?;
    Ok(creds)
}

/// The browser's redirect: (code, the client id issued on registration).
async fn wait_for_callback(listener: &tokio::net::TcpListener, state: &str) -> Result<(String, Option<String>)> {
    loop {
        let (mut sock, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await.unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]);
        let target = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or_default().to_owned();
        let Ok(url) = reqwest::Url::parse(&format!("http://127.0.0.1{target}")) else { continue };
        if url.path() != CALLBACK_PATH {
            let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            continue;
        }
        let q = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());
        let reply = |ok: bool, text: &str| {
            let body = format!("<!doctype html><meta charset=utf-8><title>harlo</title><body style=\"font-family:sans-serif;padding:3em\"><h2>{}</h2><p>{text}</p></body>", if ok { "Signed in" } else { "Not signed in" });
            format!("HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", if ok { "200 OK" } else { "400 Bad Request" }, body.len())
        };
        if let Some(err) = q("error") {
            let _ = sock.write_all(reply(false, "ChatGPT wasn't connected. You can close this tab.").as_bytes()).await;
            bail!("ChatGPT sign-in was declined ({err})");
        }
        if q("state").as_deref() != Some(state) {
            let _ = sock.write_all(reply(false, "This sign-in link is out of date. Start again from harlo.").as_bytes()).await;
            continue;
        }
        let Some(code) = q("code") else { continue };
        let _ = sock.write_all(reply(true, "harlo is connected to your ChatGPT plan. You can close this tab.").as_bytes()).await;
        return Ok((code, q("client_id").filter(|c| !c.is_empty())));
    }
}

async fn token_request(form: &[(&str, &str)]) -> Result<Value> {
    let resp = reqwest::Client::new().post(TOKEN_URL).header("accept", "application/json").form(form).send().await.context("reaching OpenAI's sign-in server")?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let code = body["error"].as_str().or_else(|| body["error"]["code"].as_str()).unwrap_or_default().to_owned();
        return Err(TokenError { status: status.as_u16(), code, body: body.to_string() }.into());
    }
    Ok(body)
}

/// A refused token request.
#[derive(Debug)]
pub struct TokenError {
    pub status: u16,
    pub code: String,
    body: String,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OpenAI's sign-in server refused ({}): {}", self.status, self.body)
    }
}

impl std::error::Error for TokenError {}

fn credentials(token: &Value, client_id: &str, email: &str) -> Result<Credentials> {
    let s = |k: &str| token[k].as_str().filter(|v| !v.trim().is_empty()).map(str::to_owned).ok_or_else(|| anyhow!("OpenAI's answer has no {k}"));
    let scopes: Vec<String> = s("scope")?.split_whitespace().map(str::to_owned).collect();
    if !scopes.iter().any(|x| x == DIRECT_SCOPE) {
        bail!("ChatGPT plan usage wasn't granted (sign in again and allow harlo to use your plan)");
    }
    let expires_in = token["expires_in"].as_u64().ok_or_else(|| anyhow!("OpenAI's answer has no expiry"))?;
    Ok(Credentials { client_id: client_id.to_owned(), access_token: s("access_token")?, refresh_token: s("refresh_token")?, expires_at: now() + expires_in, scopes, email: email.to_owned() })
}

fn jwt_claims(jwt: &str) -> Result<Value> {
    let payload = jwt.split('.').nth(1).ok_or_else(|| anyhow!("malformed ID token"))?;
    Ok(serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('='))?)?)
}

/// A usable access token: the saved one, or refreshed (one process at a
/// time; another may have refreshed while this one waited). `force`:
/// refresh even if it looks valid (the API refused it).
pub async fn access_token(force: bool) -> Result<String> {
    let current = load().ok_or_else(|| anyhow!("Not signed in: run `harlo-agent login`"))?;
    if !force && current.expires_at > now() + MARGIN.as_secs() {
        return Ok(current.access_token);
    }
    let lock = tokio::task::spawn_blocking(|| -> Result<File> {
        let f = OpenOptions::new().create(true).truncate(false).write(true).open(ensure_dir()?.join("chatgpt.lock"))?;
        f.lock()?;
        Ok(f)
    })
    .await??;
    let creds = load().ok_or_else(|| anyhow!("Not signed in: run `harlo-agent login`"))?;
    // Refreshed by another process meanwhile.
    if creds.refresh_token != current.refresh_token && creds.expires_at > now() + MARGIN.as_secs() {
        drop(lock);
        return Ok(creds.access_token);
    }
    let refreshed = token_request(&[("grant_type", "refresh_token"), ("client_id", &creds.client_id), ("refresh_token", &creds.refresh_token), ("resource", RESOURCE)]).await;
    let result = match refreshed {
        Ok(token) => {
            let next = credentials(&token, &creds.client_id, &creds.email)?;
            save(&next)?;
            Ok(next.access_token)
        }
        Err(e) => match e.downcast_ref::<TokenError>() {
            // The sign-in is over: forget it.
            Some(t) if matches!(t.code.as_str(), "invalid_grant" | "invalid_refresh_token" | "token_expired" | "refresh_token_expired" | "refresh_token_invalidated" | "refresh_token_reused") => {
                let _ = sign_out();
                Err(anyhow!("Your ChatGPT sign-in has ended ({}): run `harlo-agent login` again", t.code))
            }
            _ => Err(e),
        },
    };
    drop(lock);
    result
}
