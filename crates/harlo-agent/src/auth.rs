//! Sign in with ChatGPT (OpenAI's flow for open-source apps): the user's
//! ChatGPT plan pays for requests to the public Responses API.
//!
//! Or, opted into: the Codex CLI's sign-in (its client id), whose requests
//! go to ChatGPT's Codex backend. Not OpenAI's documented way for other
//! apps; it's what gives the plan's usage (percent used and resets). Kept
//! apart (`codex.json`).
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
/// The Codex CLI's sign-in.
const CODEX_ISSUER: &str = "https://auth.openai.com";
pub const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_SCOPE: &str = "openid profile email offline_access";
/// The claim the ChatGPT account's id is in.
const AUTH_CLAIM: &str = "https://api.openai.com/auth";
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
    /// The ChatGPT account (the Codex sign-in's requests name it).
    #[serde(default)]
    pub account_id: String,
}

/// Which sign-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    /// OpenAI's for open-source apps.
    Plan,
    /// The Codex CLI's (opted into).
    Codex,
}

impl Sign {
    fn file(self) -> &'static str {
        match self {
            Sign::Plan => "chatgpt",
            Sign::Codex => "codex",
        }
    }

    /// How `harlo-agent login` names it.
    pub fn login(self) -> &'static str {
        match self {
            Sign::Plan => "harlo-agent login",
            Sign::Codex => "harlo-agent login --codex",
        }
    }
}

/// Where harlo-agent keeps its settings and sign-in.
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".config"));
    base.join("harlo-agent")
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn creds_path(sign: Sign) -> PathBuf {
    config_dir().join(format!("{}.json", sign.file()))
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

pub fn load(sign: Sign) -> Option<Credentials> {
    serde_json::from_str(&fs::read_to_string(creds_path(sign)).ok()?).ok()
}

fn save(sign: Sign, c: &Credentials) -> Result<()> {
    ensure_dir()?;
    write_private(&creds_path(sign), &serde_json::to_string_pretty(c)?)
}

/// The client id OpenAI issued on an earlier sign-in (kept after sign-out).
fn saved_client_id() -> Option<String> {
    let path = config_dir().join("client-id");
    fs::read_to_string(path).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// Forgets the tokens here (the registration's client id stays). The user
/// can disconnect harlo in ChatGPT's settings too.
pub fn sign_out(sign: Sign) -> Result<bool> {
    match fs::remove_file(creds_path(sign)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Signs in in the browser: a listener on 127.0.0.1:1455 takes the
/// callback. `open` gets the URL to show or open.
pub async fn sign_in(open: impl Fn(&str), paste: bool) -> Result<Credentials> {
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
    let (code, issued) = wait_for_callback(&listener, &state, "harlo is connected to your ChatGPT plan. You can close this tab.", paste).await?;
    let client_id = issued.or(saved).ok_or_else(|| anyhow!("OpenAI's answer didn't include this app's client id; try again"))?;
    let token = token_request(TOKEN_URL, &[
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
    save(Sign::Plan, &creds)?;
    Ok(creds)
}

/// Signs in as the Codex CLI does (its client id, PKCE, the same loopback
/// callback), naming harlo as the originator.
pub async fn codex_sign_in(open: impl Fn(&str), paste: bool) -> Result<Credentials> {
    let verifier = random_token();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_token();
    let redirect = format!("http://localhost:{CALLBACK_PORT}{CALLBACK_PATH}");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| anyhow!("Port {CALLBACK_PORT} is in use ({e}): another app's sign-in is probably waiting there. Finish or cancel it and try again."))?;
    let params: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", CODEX_CLIENT_ID),
        ("redirect_uri", &redirect),
        ("scope", CODEX_SCOPE),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", &state),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("originator", AGENT_NAME),
    ];
    let url = reqwest::Url::parse_with_params(&format!("{CODEX_ISSUER}/oauth/authorize"), &params)?;
    open(url.as_str());
    let (code, _) = wait_for_callback(&listener, &state, "harlo is signed in with your ChatGPT account (the Codex sign-in). You can close this tab.", paste).await?;
    let token = token_request(
        &format!("{CODEX_ISSUER}/oauth/token"),
        &[("grant_type", "authorization_code"), ("client_id", CODEX_CLIENT_ID), ("code", &code), ("code_verifier", &verifier), ("redirect_uri", &redirect)],
    )
    .await?;
    let creds = codex_credentials(&token, None)?;
    save(Sign::Codex, &creds)?;
    Ok(creds)
}

/// The Codex sign-in's tokens: its expiry from `expires_in` (else the
/// access token's own), its email and account from the ID token (else the
/// access token, else what was known).
fn codex_credentials(token: &Value, before: Option<&Credentials>) -> Result<Credentials> {
    let s = |k: &str| token[k].as_str().filter(|v| !v.trim().is_empty()).map(str::to_owned);
    let access = s("access_token").ok_or_else(|| anyhow!("OpenAI's answer has no access_token"))?;
    // (A refresh may keep the refresh token.)
    let refresh = s("refresh_token").or_else(|| before.map(|b| b.refresh_token.clone())).ok_or_else(|| anyhow!("OpenAI's answer has no refresh_token"))?;
    let access_claims = jwt_claims(&access).unwrap_or(Value::Null);
    let id_claims = s("id_token").and_then(|t| jwt_claims(&t).ok()).unwrap_or(Value::Null);
    let expires_at = token["expires_in"].as_u64().map(|e| now() + e).or_else(|| access_claims["exp"].as_u64()).unwrap_or(now() + 3600);
    let pick = |f: &dyn Fn(&Value) -> Option<String>| f(&id_claims).or_else(|| f(&access_claims));
    let account_id = pick(&|c| c[AUTH_CLAIM]["chatgpt_account_id"].as_str().filter(|a| !a.is_empty()).map(str::to_owned))
        .or_else(|| before.map(|b| b.account_id.clone()).filter(|a| !a.is_empty()))
        .ok_or_else(|| anyhow!("The sign-in has no ChatGPT account (sign in with a ChatGPT account, not an API organization)"))?;
    let email = pick(&|c| c["email"].as_str().map(str::to_owned).or_else(|| c["https://api.openai.com/profile"]["email"].as_str().map(str::to_owned)))
        .or_else(|| before.map(|b| b.email.clone()))
        .unwrap_or_default();
    let scopes = s("scope").map(|x| x.split_whitespace().map(str::to_owned).collect()).unwrap_or_default();
    Ok(Credentials { client_id: CODEX_CLIENT_ID.to_owned(), access_token: access, refresh_token: refresh, expires_at, scopes, email, account_id })
}

/// What a callback said.
#[derive(Debug, PartialEq)]
enum Callback {
    /// The code, and the client id issued on registration.
    Code(String, Option<String>),
    Declined(String),
    /// For another sign-in (an older link).
    Stale,
    /// Not the callback.
    Other,
}

fn read_callback(url: &reqwest::Url, state: &str) -> Callback {
    if url.path() != CALLBACK_PATH {
        return Callback::Other;
    }
    let q = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());
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

/// The browser's redirect: (code, the client id issued on registration).
/// `paste`: or the address the page ended on, pasted on the input (when
/// the browser is on another computer and its redirect can't reach here).
async fn wait_for_callback(listener: &tokio::net::TcpListener, state: &str, done: &str, paste: bool) -> Result<(String, Option<String>)> {
    use tokio::io::AsyncBufReadExt;
    let mut lines = paste.then(|| tokio::io::BufReader::new(tokio::io::stdin()).lines());
    if paste {
        print!("If the page doesn't say you're signed in, paste the address it ended on: ");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
    loop {
        let pasted = async {
            match lines.as_mut() {
                Some(l) => l.next_line().await,
                None => std::future::pending().await,
            }
        };
        let mut sock = tokio::select! {
            got = listener.accept() => got?.0,
            line = pasted => {
                let Some(line) = line? else {
                    lines = None;
                    continue;
                };
                let Ok(url) = reqwest::Url::parse(line.trim()) else {
                    if !line.trim().is_empty() {
                        println!("That isn't the address the page ended on (it starts with http://).");
                    }
                    continue;
                };
                match read_callback(&url, state) {
                    Callback::Code(code, issued) => return Ok((code, issued)),
                    Callback::Declined(err) => bail!("ChatGPT sign-in was declined ({err})"),
                    Callback::Stale => println!("That address is from another sign-in: use this one's link."),
                    Callback::Other => println!("That isn't the address the page ended on (it ends in {CALLBACK_PATH}?code=…)."),
                }
                continue;
            }
        };
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await.unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]);
        let target = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or_default().to_owned();
        let Ok(url) = reqwest::Url::parse(&format!("http://127.0.0.1{target}")) else { continue };
        let reply = |ok: bool, text: &str| {
            let body = format!("<!doctype html><meta charset=utf-8><title>harlo</title><body style=\"font-family:sans-serif;padding:3em\"><h2>{}</h2><p>{text}</p></body>", if ok { "Signed in" } else { "Not signed in" });
            format!("HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", if ok { "200 OK" } else { "400 Bad Request" }, body.len())
        };
        match read_callback(&url, state) {
            Callback::Other => {
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            }
            Callback::Declined(err) => {
                let _ = sock.write_all(reply(false, "ChatGPT wasn't connected. You can close this tab.").as_bytes()).await;
                bail!("ChatGPT sign-in was declined ({err})");
            }
            Callback::Stale => {
                let _ = sock.write_all(reply(false, "This sign-in link is out of date. Start again from harlo.").as_bytes()).await;
            }
            Callback::Code(code, issued) => {
                let _ = sock.write_all(reply(true, done).as_bytes()).await;
                return Ok((code, issued));
            }
        }
    }
}

async fn token_request(url: &str, form: &[(&str, &str)]) -> Result<Value> {
    let resp = reqwest::Client::new().post(url).header("accept", "application/json").form(form).send().await.context("reaching OpenAI's sign-in server")?;
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
    Ok(Credentials { client_id: client_id.to_owned(), access_token: s("access_token")?, refresh_token: s("refresh_token")?, expires_at: now() + expires_in, scopes, email: email.to_owned(), account_id: String::new() })
}

fn jwt_claims(jwt: &str) -> Result<Value> {
    let payload = jwt.split('.').nth(1).ok_or_else(|| anyhow!("malformed ID token"))?;
    Ok(serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('='))?)?)
}

/// A usable access token: the saved one, or refreshed (one process at a
/// time; another may have refreshed while this one waited). `force`:
/// refresh even if it looks valid (the API refused it).
pub async fn access_token(sign: Sign, force: bool) -> Result<String> {
    Ok(credentials_now(sign, force).await?.access_token)
}

/// The sign-in, its access token usable (refreshed as `access_token`).
pub async fn credentials_now(sign: Sign, force: bool) -> Result<Credentials> {
    let not_in = || anyhow!("Not signed in: run `{}`", sign.login());
    let current = load(sign).ok_or_else(not_in)?;
    if !force && current.expires_at > now() + MARGIN.as_secs() {
        return Ok(current);
    }
    let lock_name = format!("{}.lock", sign.file());
    let lock = tokio::task::spawn_blocking(move || -> Result<File> {
        let f = OpenOptions::new().create(true).truncate(false).write(true).open(ensure_dir()?.join(lock_name))?;
        f.lock()?;
        Ok(f)
    })
    .await??;
    let creds = load(sign).ok_or_else(not_in)?;
    // Refreshed by another process meanwhile.
    if creds.refresh_token != current.refresh_token && creds.expires_at > now() + MARGIN.as_secs() {
        drop(lock);
        return Ok(creds);
    }
    let refreshed = match sign {
        Sign::Plan => token_request(TOKEN_URL, &[("grant_type", "refresh_token"), ("client_id", &creds.client_id), ("refresh_token", &creds.refresh_token), ("resource", RESOURCE)]).await,
        Sign::Codex => token_request(&format!("{CODEX_ISSUER}/oauth/token"), &[("grant_type", "refresh_token"), ("client_id", CODEX_CLIENT_ID), ("refresh_token", &creds.refresh_token)]).await,
    };
    let result = match refreshed {
        Ok(token) => {
            let next = match sign {
                Sign::Plan => credentials(&token, &creds.client_id, &creds.email)?,
                Sign::Codex => codex_credentials(&token, Some(&creds))?,
            };
            save(sign, &next)?;
            Ok(next)
        }
        Err(e) => match e.downcast_ref::<TokenError>() {
            // The sign-in is over: forget it.
            Some(t) if matches!(t.code.as_str(), "invalid_grant" | "invalid_refresh_token" | "token_expired" | "refresh_token_expired" | "refresh_token_invalidated" | "refresh_token_reused") => {
                let _ = sign_out(sign);
                Err(anyhow!("Your ChatGPT sign-in has ended ({}): run `{}` again", t.code, sign.login()))
            }
            _ => Err(e),
        },
    };
    drop(lock);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn jwt(claims: &Value) -> String {
        format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims.to_string()))
    }

    #[test]
    fn a_pasted_callback() {
        let u = |s: &str| reqwest::Url::parse(s).unwrap();
        assert_eq!(read_callback(&u("http://localhost:1455/auth/callback?code=c1&state=s1"), "s1"), Callback::Code("c1".into(), None));
        assert_eq!(read_callback(&u("http://127.0.0.1:1455/auth/callback?code=c1&state=s1&client_id=app_x"), "s1"), Callback::Code("c1".into(), Some("app_x".into())));
        assert_eq!(read_callback(&u("http://localhost:1455/auth/callback?code=c1&state=old"), "s1"), Callback::Stale);
        assert_eq!(read_callback(&u("http://localhost:1455/auth/callback?error=access_denied&state=s1"), "s1"), Callback::Declined("access_denied".into()));
        assert_eq!(read_callback(&u("http://localhost:1455/favicon.ico"), "s1"), Callback::Other);
    }

    #[test]
    fn codex_tokens() {
        let id = jwt(&json!({ "email": "a@b", AUTH_CLAIM: { "chatgpt_account_id": "acct-1" } }));
        let access = jwt(&json!({ "exp": 4_000_000_000_u64 }));
        let c = codex_credentials(&json!({ "access_token": access, "refresh_token": "r1", "id_token": id }), None).unwrap();
        assert_eq!((c.email.as_str(), c.account_id.as_str(), c.expires_at, c.client_id.as_str()), ("a@b", "acct-1", 4_000_000_000, CODEX_CLIENT_ID));
        // A refresh without an ID token or a new refresh token keeps what was known.
        let next = codex_credentials(&json!({ "access_token": jwt(&json!({})), "expires_in": 3600 }), Some(&c)).unwrap();
        assert_eq!((next.refresh_token.as_str(), next.account_id.as_str(), next.email.as_str()), ("r1", "acct-1", "a@b"));
        // No ChatGPT account at all: refused.
        assert!(codex_credentials(&json!({ "access_token": jwt(&json!({})), "refresh_token": "r" }), None).is_err());
        assert_eq!((creds_path(Sign::Plan).file_name().unwrap(), creds_path(Sign::Codex).file_name().unwrap()), ("chatgpt.json".as_ref(), "codex.json".as_ref()));
    }
}
