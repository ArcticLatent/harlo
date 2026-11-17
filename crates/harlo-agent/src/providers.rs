//! Where requests go. A model is named `provider/model`:
//!
//! - `chatgpt`: the signed-in ChatGPT plan (Responses API).
//! - `openai`: an OpenAI API key in `OPENAI_API_KEY` (Responses API).
//! - those in `providers.json` (harlo-agent's config folder): OpenAI-compatible
//!   servers, local or online, with chat completions (the default) or the
//!   Responses API. The file is laid out as pi's `models.json`, so harlo's
//!   Settings → Model servers writes it as it writes pi's:
//!
//! ```json
//! { "providers": {
//!     "llamacpp": { "name": "llama.cpp", "baseUrl": "http://127.0.0.1:8090/v1",
//!       "api": "openai-completions" | "openai-responses",
//!       "apiKey": "…" | "$NAME" (from the environment) | "none",
//!       "models": ["qwen3", { "id": "…", "name": "…", "contextWindow": 32768,
//!         "reasoning": true, "thinkingLevelMap": { "off": "none", "low": null, "high": "high" },
//!         "cost": { "input": 0.4, "output": 1.6, "cacheRead": 0.1 } }] } } }
//! ```
//!
//! `cost`: dollars per million tokens (`cacheRead`: input the cache
//! served; the input price when it's not given). Without one, a model's
//! price is what its server's list says (OpenRouter's `pricing`, say).
//! `"free": true` on a provider: it charges nothing (a server on your own
//! hardware); unpriced and not free, what it costs is unknown, wherever it
//! is (a local proxy may pass requests on to a paid service).
//!
//! `thinkingLevelMap`: the levels offered and the effort each sends (null:
//! not offered); without it a model that reasons takes the usual levels.
//! A provider without `models` lists what its `/models` says. The file is
//! read for each request, so a change to it applies to sessions running.
//!
//! Built in beside them (`presets.json`, in the same layout): cloud
//! providers that take an API key, each listed once it has one (saved by
//! `harlo-agent key set`, or in its environment variable). An entry in
//! `providers.json` with the same name replaces the built-in one.
//! Fields only harlo reads, in either file:
//!
//! - `thinking`: how the server takes a level: `effort` (`reasoning_effort`,
//!   the default), `reasoning` (`reasoning: { effort }`) or `toggle`
//!   (`thinking: { type }`, with `reasoning_effort`; "none" turns it off).
//! - `keepThinking`: send the model's thinking back (`reasoning_content`),
//!   for servers that ask for it.
//! - `body`: fields added to each request (a provider's, then a model's).
//! - `listModels`: ask the server for its models even with `models` given
//!   (those then add what's known of the ones it lists).
//! - a model's `aliases` (other ids it's listed as), `longCost` (the rates
//!   once a request's input reaches `from` tokens) and `peakCost` (the rates
//!   in `utcHours`, on weekdays when `weekdays`), `costFrom` (rates that
//!   hold from a `date`, UTC) and `maxTokens` (the most a reply may hold).

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::Value;

use crate::auth::{self, Sign};
use crate::chat::Thinking;
use crate::used::{Long, Price, Pricing};

/// The plan's provider.
pub const CHATGPT: &str = "chatgpt";
/// The plan through the Codex CLI's sign-in (opted into).
pub const CODEX: &str = "codex";
/// ChatGPT's backend (`HARLO_AGENT_CHATGPT_BACKEND` sets another, to
/// try it against a stand-in).
pub fn chatgpt_backend() -> String {
    std::env::var("HARLO_AGENT_CHATGPT_BACKEND").ok().map(|u| u.trim().trim_end_matches('/').to_owned()).filter(|u| !u.is_empty()).unwrap_or_else(|| "https://chatgpt.com/backend-api".to_owned())
}
/// The Codex version its models list is asked for as (it lists the models
/// that version takes).
const CODEX_CLIENT_VERSION: &str = "0.167.0";
const OPENAI_URL: &str = "https://api.openai.com/v1";
/// Thinking levels a model that reasons takes when it doesn't say.
const REASONING_LEVELS: [&str; 4] = ["minimal", "low", "medium", "high"];
/// pi's thinking levels, in order (a `thinkingLevelMap`'s keys).
const PI_LEVELS: [&str; 6] = ["off", "minimal", "low", "medium", "high", "xhigh"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    Responses,
    Chat,
    /// Anthropic's Messages API.
    Messages,
    /// Google's Gemini API (`generateContent`).
    Gemini,
}

impl Api {
    /// Its name in `providers.json` and in pi's messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::Responses => "openai-responses",
            Self::Chat => "openai-completions",
            Self::Messages => "anthropic-messages",
            Self::Gemini => "google-generative-ai",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Key {
    /// A ChatGPT sign-in.
    Plan(Sign),
    None,
    Value(String),
    Env(String),
    /// A built-in provider's: the one saved for it, else its environment
    /// variable's.
    Saved { provider: String, env: String },
}

#[derive(Clone, Debug)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api: Api,
    key: Key,
    /// As configured (none: ask the server).
    models: Vec<ModelInfo>,
    /// Said to charge nothing (`"free": true`).
    free: bool,
    /// How it takes a thinking level (chat completions).
    pub thinking: Thinking,
    /// Whether its models' thinking goes back to it.
    pub keep_thinking: bool,
    /// Fields every request to it carries.
    body: Value,
    /// Its models asked of it even when `models` are given.
    list: bool,
    /// Its key goes as `Authorization: Bearer` (Messages API servers take
    /// `x-api-key` otherwise).
    pub bearer: bool,
    /// Where its models of another API are (`routes` in the file: a model's
    /// `api` picks one).
    routes: Vec<(Api, String)>,
    /// Each request carries the session's id in `x-opencode-session`, so a
    /// conversation's requests reach the same cache.
    session_header: bool,
    /// Where its models are listed, when that's not under `base_url`.
    list_url: Option<String>,
}

/// The header OpenCode's servers take a conversation's id in.
pub const OPENCODE_SESSION: &str = "x-opencode-session";

/// A built-in provider, and what Settings shows of it.
#[derive(Clone, Debug)]
pub struct Preset {
    pub provider: Provider,
    /// The environment variable its key may be in.
    pub key_env: String,
    /// Where one makes a key.
    pub key_page: String,
    /// Where its prices are.
    pub price_page: String,
    /// A line Settings shows with it ("" none).
    pub note: String,
    /// How a key is checked: a path to GET (`/models`), or `chat` (a
    /// one-token request: it has nothing free to ask).
    pub check: String,
    /// The provider whose key it uses (its own, or another endpoint's of
    /// the same company).
    pub key_of: String,
}

/// A model's rates at its busiest hours.
#[derive(Clone, Debug, PartialEq)]
pub struct Peak {
    pub price: Price,
    /// Hours (UTC) from and to, the end not included.
    pub hours: Vec<(u32, u32)>,
    /// Only Monday to Friday.
    pub weekdays: bool,
}

/// A model one can pick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelInfo {
    pub provider: String,
    pub id: String,
    /// Its name for people.
    pub name: String,
    /// Tokens, or 0 when not said.
    pub context_window: u64,
    /// Thinking levels it takes (none: it doesn't reason, or doesn't say).
    pub levels: Vec<String>,
    /// The effort each level sends, from a `thinkingLevelMap` ("off" may
    /// send "none"); empty: a level sends its name, and "off" nothing.
    pub efforts: Vec<(String, String)>,
    /// Whether it takes images (None: not said; they're sent).
    pub images: Option<bool>,
    /// Its price, when the file or its server says it.
    pub price: Option<Price>,
    /// Its price at peak hours, when that differs.
    pub peak: Option<Peak>,
    /// Prices that hold from a time on (unix seconds), in order.
    pub later: Vec<(u64, Price)>,
    /// Other ids its server may list it as.
    pub aliases: Vec<String>,
    /// Fields its requests carry.
    pub body: Value,
    /// The most a reply may hold, in tokens (0: not said).
    pub max_tokens: u64,
    /// The API it's asked through, when not its provider's (one of the
    /// provider's `routes`).
    pub api: Option<Api>,
    /// How it takes a thinking level, when not as its provider's do.
    pub thinking: Option<Thinking>,
}

impl ModelInfo {
    /// `provider/id`.
    pub fn key(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }

    /// Its price at `unix` (seconds): at peak hours, or the latest set
    /// from before then, or its own.
    pub fn price_at(&self, unix: u64) -> Option<Price> {
        let base = self.later.iter().rev().find(|(from, _)| *from <= unix).map(|(_, p)| *p).or(self.price);
        self.peak.as_ref().filter(|p| p.covers(unix)).map(|p| p.price).or(base)
    }
}

/// Seconds from 1970 to the start of `date` (`YYYY-MM-DD`, UTC).
fn unix_date(date: &str) -> Option<u64> {
    let mut it = date.trim().splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // (Days from the civil date: March-based years, so leap days come last.)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    u64::try_from((era * 146_097 + doe - 719_468) * 86_400).ok()
}

impl Provider {
    /// A ChatGPT plan's (either sign-in).
    pub fn is_plan(&self) -> bool {
        matches!(self.key, Key::Plan(_))
    }

    /// Whether its server searches the web itself (OpenAI's Responses
    /// API's `web_search` tool: the plan's and OpenAI's own;
    /// `HARLO_AGENT_WEB_SEARCH=off` leaves it out).
    pub fn searches_web(&self) -> bool {
        self.api == Api::Responses
            && (self.is_plan() || self.id == "openai")
            && std::env::var("HARLO_AGENT_WEB_SEARCH").map_or(true, |v| v.trim() != "off")
    }

    /// Whether its server is on this computer or its network: a loopback or
    /// private address, or a `.local`, `.lan` or dotless name.
    #[cfg(test)]
    pub fn is_local(&self) -> bool {
        let rest = self.base_url.split_once("://").map_or(self.base_url.as_str(), |(_, r)| r);
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let host = match host.strip_prefix('[') {
            Some(v6) => v6.split(']').next().unwrap_or_default(),
            None => host.rsplit_once(':').filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit())).map_or(host, |(h, _)| h),
        }
        .to_ascii_lowercase();
        match host.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(a)) => a.is_loopback() || a.is_private() || a.is_link_local(),
            Ok(std::net::IpAddr::V6(a)) => a.is_loopback() || (a.segments()[0] & 0xfe00) == 0xfc00 || (a.segments()[0] & 0xffc0) == 0xfe80,
            Err(_) => host == "localhost" || [".localhost", ".local", ".lan", ".internal", ".home.arpa"].iter().any(|s| host.ends_with(s)) || (!host.is_empty() && !host.contains('.')),
        }
    }

    /// What `model` (its id here) costs, by what the file says of it (the
    /// server's list's price, when known, as `listed`).
    pub fn pricing(&self, model: &str, listed: Option<Price>) -> Pricing {
        if self.is_plan() {
            return Pricing::Plan;
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let price = self.known(model).and_then(|m| m.price_at(now));
        match price.or(listed) {
            Some(p) => Pricing::Priced(p),
            None if self.free => Pricing::Local,
            None => Pricing::Unknown,
        }
    }

    /// Which sign-in, for a plan's.
    pub fn sign(&self) -> Option<Sign> {
        match self.key {
            Key::Plan(s) => Some(s),
            _ => None,
        }
    }

    /// Headers its requests carry besides the token: the Codex backend's
    /// (the account, and harlo as the originator). Read after `token`, which
    /// refreshes the sign-in.
    pub fn headers(&self) -> Vec<(&'static str, String)> {
        if self.sign() != Some(Sign::Codex) {
            return Vec::new();
        }
        let account = auth::load(Sign::Codex).map(|c| c.account_id).unwrap_or_default();
        vec![("chatgpt-account-id", account), ("originator", "harlo".to_owned()), ("OpenAI-Beta", "responses=experimental".to_owned())]
    }

    /// Where its models are listed.
    fn models_url(&self) -> String {
        if let Some(url) = &self.list_url {
            return url.clone();
        }
        match () {
            () if self.sign() == Some(Sign::Codex) => format!("{}/models?client_version={CODEX_CLIENT_VERSION}", self.base_url),
            // (Twenty a page otherwise.)
            () if self.api == Api::Messages => format!("{}/models?limit=1000", self.base_url),
            () if self.api == Api::Gemini => format!("{}/models?pageSize=1000", self.base_url),
            () => format!("{}/models", self.base_url),
        }
    }

    /// How `key` goes with a request to it: as a header of its own (a
    /// Messages API server's `x-api-key`), or else as a bearer token.
    pub fn key_header(&self, key: Option<&str>) -> Vec<(&'static str, String)> {
        match key {
            Some(k) if self.api == Api::Messages && !self.bearer => vec![("x-api-key", k.to_owned()), ("anthropic-version", "2023-06-01".to_owned())],
            Some(k) if self.api == Api::Gemini => vec![("x-goog-api-key", k.to_owned())],
            _ => Vec::new(),
        }
    }

    /// The most a reply from `model` may hold (a Messages API request
    /// says it): what's configured, else 32,000.
    pub fn max_tokens(&self, model: &str) -> u64 {
        self.known(model).map(|m| m.max_tokens).filter(|n| *n > 0).unwrap_or(32_000)
    }

    /// The bearer token (none for a server without a key). `force`: the
    /// plan's token refreshed even if it looks valid.
    pub async fn token(&self, force: bool) -> Result<Option<String>> {
        match &self.key {
            Key::Plan(sign) => Ok(Some(auth::access_token(*sign, force).await?)),
            Key::None => Ok(None),
            Key::Value(k) => Ok(Some(k.clone())),
            Key::Env(name) => std::env::var(name).map(Some).ok().ok_or_else(|| anyhow!("{}'s key isn't set: ${name} is empty", self.name)),
            Key::Saved { provider, env } => crate::keys::get(provider)
                .or_else(|| std::env::var(env).ok().filter(|k| !k.trim().is_empty()))
                .map(Some)
                .ok_or_else(|| anyhow!("No key for {}: add one in harlo's Settings → Providers (or set ${env})", self.name)),
        }
    }

    /// Where its key comes from: `saved` (keys.json), `environment`, or
    /// `none` (a built-in provider's; others say `set`).
    pub fn key_source(&self) -> &'static str {
        match &self.key {
            Key::Saved { provider, env } => {
                if crate::keys::get(provider).is_some() {
                    "saved"
                } else if std::env::var(env).is_ok_and(|k| !k.trim().is_empty()) {
                    "environment"
                } else {
                    "none"
                }
            }
            _ => "set",
        }
    }

    /// The fields a request for `model` adds to the body (the provider's,
    /// then the model's over them).
    pub fn extra(&self, model: &str) -> Value {
        let mut out = if self.body.is_object() { self.body.clone() } else { serde_json::json!({}) };
        if let Some(m) = self.known(model) {
            crate::chat::overlay(&mut out, &m.body);
        }
        out
    }

    /// Whether to send `model` a thinking level: the plan's always (its
    /// models all reason); a configured model when it has levels; others
    /// when they're known reasoning models by name.
    pub fn reasons(&self, model: &str) -> bool {
        if self.is_plan() {
            return true;
        }
        if let Some(m) = self.known(model) {
            return !m.levels.is_empty();
        }
        // (OpenRouter's take a level whatever they are: it leaves it out
        // for those that don't reason.)
        if self.api == Api::Chat && self.thinking == Thinking::Reasoning {
            return true;
        }
        let bare = model.rsplit('/').next().unwrap_or(model);
        self.api == Api::Responses && (bare.starts_with("gpt-5") || bare.starts_with("gpt-6") || (bare.starts_with('o') && bare[1..].starts_with(|c: char| c.is_ascii_digit())))
    }

    /// The effort to send `model` at thinking level `level` (none: off),
    /// if any.
    pub fn effort(&self, model: &str, level: Option<&str>) -> Option<String> {
        if !self.reasons(model) {
            return None;
        }
        let level = level.unwrap_or("off");
        match self.known(model).filter(|m| !m.efforts.is_empty()) {
            Some(m) => m.efforts.iter().find(|(l, _)| l == level).map(|(_, e)| e.clone()),
            None => (level != "off").then(|| level.to_owned()),
        }
    }

    /// The models configured (known before its server is asked).
    pub fn configured(&self) -> &[ModelInfo] {
        &self.models
    }

    /// What's configured of `model` (by its id or an alias).
    fn known(&self, model: &str) -> Option<&ModelInfo> {
        self.models.iter().find(|m| m.id == model || m.aliases.iter().any(|a| a == model))
    }

    /// As `model` is asked of it: through the model's own API at that
    /// API's address (a route), and with its own way of thinking.
    #[must_use]
    pub fn for_model(mut self, model: &str) -> Self {
        let (api, thinking) = self.known(model).map_or((None, None), |m| (m.api, m.thinking));
        if let Some((api, base)) = api.and_then(|a| self.routes.iter().find(|(r, _)| *r == a).cloned()) {
            self.api = api;
            self.base_url = base;
        }
        if let Some(t) = thinking {
            self.thinking = t;
        }
        self
    }

    /// The headers that tie a request to its conversation (`session`: its
    /// id), for a provider that asks for them.
    pub fn session_headers(&self, session: &str) -> Vec<(&'static str, String)> {
        if self.session_header && !session.is_empty() { vec![(OPENCODE_SESSION, session.to_owned())] } else { Vec::new() }
    }

    /// Its models: as configured, else as its server lists them (with
    /// what's configured of them, when it's asked anyway; what's
    /// configured when it can't say).
    pub async fn models(&self) -> Result<Vec<ModelInfo>> {
        if !self.models.is_empty() && !self.list {
            return Ok(self.models.clone());
        }
        let token = self.token(false).await?;
        let own = self.key_header(token.as_deref());
        let (token, headers) = if own.is_empty() { (token, self.headers()) } else { (None, own) };
        let listed = crate::responses::models_raw(&self.models_url(), token.as_deref(), &headers).await.with_context(|| format!("{}'s models", self.name)).map(|body| parse_models(&self.id, &body));
        match listed {
            Ok(models) if !models.is_empty() => Ok(models.into_iter().map(|m| self.with_known(m)).collect()),
            _ if !self.models.is_empty() => Ok(self.models.clone()),
            Ok(_) => bail!("{} lists no models", self.base_url),
            Err(e) => Err(e),
        }
    }

    /// A listed model with what's configured of it (that wins; what the
    /// list says fills what it leaves out).
    fn with_known(&self, listed: ModelInfo) -> ModelInfo {
        let Some(k) = self.known(&listed.id) else { return listed };
        ModelInfo {
            id: listed.id.clone(),
            context_window: if k.context_window > 0 { k.context_window } else { listed.context_window },
            images: k.images.or(listed.images),
            price: k.price.or(listed.price),
            levels: if k.levels.is_empty() { listed.levels.clone() } else { k.levels.clone() },
            efforts: if k.levels.is_empty() { listed.efforts.clone() } else { k.efforts.clone() },
            max_tokens: if k.max_tokens > 0 { k.max_tokens } else { listed.max_tokens },
            ..k.clone()
        }
    }
}

impl Peak {
    /// Whether `unix` (seconds) is in its hours.
    fn covers(&self, unix: u64) -> bool {
        // (1970-01-01 was a Thursday: day 0 is Thursday.)
        let weekday = (unix / 86_400 + 3) % 7;
        let hour = u32::try_from((unix % 86_400) / 3_600).unwrap_or(0);
        (!self.weekdays || weekday < 5) && self.hours.iter().any(|(from, to)| (*from..*to).contains(&hour))
    }
}

impl Provider {
    /// One with nothing set (to fill in).
    fn bare() -> Self {
        Self { id: String::new(), name: String::new(), base_url: String::new(), api: Api::Chat, key: Key::None, models: Vec::new(), free: false, thinking: Thinking::Effort, keep_thinking: false, body: Value::Null, list: false, bearer: false, routes: Vec::new(), session_header: false, list_url: None }
    }
}

/// The built-in providers, keyed or not.
pub fn presets() -> Vec<Preset> {
    let file: Value = serde_json::from_str(include_str!("presets.json")).expect("presets.json is valid JSON");
    let s = |p: &Value, k: &str| p[k].as_str().unwrap_or_default().to_owned();
    // (`HARLO_AGENT_PRESETS_URL` sends each to `<it>/<id>` instead, to try
    // them against a stand-in.)
    let stand_in = std::env::var("HARLO_AGENT_PRESETS_URL").ok().map(|u| u.trim().trim_end_matches('/').to_owned()).filter(|u| !u.is_empty());
    file["providers"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, p)| {
            let mut provider = parse_provider(id, p).expect("presets.json's providers are valid");
            if let Some(url) = &stand_in {
                provider.base_url = format!("{url}/{id}");
                for (api, base) in &mut provider.routes {
                    *base = format!("{url}/{id}/{}", api.name());
                }
                provider.list_url = None;
            }
            let key_of = p["keyOf"].as_str().unwrap_or(id).to_owned();
            provider.key = Key::Saved { provider: key_of.clone(), env: s(p, "keyEnv") };
            Preset { provider, key_env: s(p, "keyEnv"), key_page: s(p, "keyPage"), price_page: s(p, "pricePage"), note: s(p, "note"), check: p["check"].as_str().unwrap_or("/models").to_owned(), key_of }
        })
        .collect()
}

impl Preset {
    /// Whether its provider takes `key`: what it said when it did.
    ///
    /// Asks something free (its models, or the key's own details) where it
    /// has that; otherwise a one-token request to its cheapest model.
    ///
    /// # Errors
    ///
    /// When the key is refused, the server can't be reached, or it answers
    /// with an error.
    pub async fn check_key(&self, key: &str) -> Result<String> {
        let p = &self.provider;
        let req = if self.check == "chat" {
            let cheapest = p.models.iter().min_by(|a, b| a.price.map_or(f64::MAX, |x| x.input).total_cmp(&b.price.map_or(f64::MAX, |x| x.input))).with_context(|| format!("{} has no model to try", p.name))?;
            let body = serde_json::json!({ "model": cheapest.id, "messages": [{ "role": "user", "content": "Hi" }], "max_tokens": 1, "stream": false });
            let path = if p.api == Api::Messages { "messages" } else { "chat/completions" };
            crate::responses::client().post(format!("{}/{path}", p.base_url)).json(&body)
        } else {
            crate::responses::client().get(format!("{}{}", p.base_url, self.check))
        };
        let own = p.key_header(Some(key));
        let req = if own.is_empty() { req.bearer_auth(key) } else { own.into_iter().fold(req, |r, (k, v)| r.header(k, v)) };
        let resp = req.timeout(Duration::from_secs(20)).send().await.map_err(|e| anyhow!("Couldn't reach {}: {e}", p.base_url))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(format!("{} took the key", p.name));
        }
        let text = crate::responses::refusal_text(resp).await;
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let said = body["error"]["message"].as_str().or_else(|| body["error"].as_str()).or_else(|| body["message"].as_str()).or_else(|| body["detail"].as_str()).map_or_else(|| text.chars().take(200).collect(), str::to_owned);
        match status.as_u16() {
            401 | 403 => bail!("{} refused the key ({status}): {said}", p.name),
            _ => bail!("{} answered {status}: {said}", p.name),
        }
    }
}

/// The names of the providers in `providers.json` (each replaces a
/// built-in one of the same name).
pub fn file_ids() -> Vec<String> {
    let file: Value = std::fs::read_to_string(config_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    file["providers"].as_object().map(|o| o.keys().map(|k| k.trim().to_owned()).collect()).unwrap_or_default()
}

fn config_file() -> PathBuf {
    auth::config_dir().join("providers.json")
}

/// The providers: the plan, an OpenAI key, the built-in ones with a key,
/// then the file's.
pub fn all() -> Result<Vec<Provider>> {
    let mut out = vec![
        Provider { id: CHATGPT.into(), name: "ChatGPT".into(), base_url: auth::RESOURCE.into(), api: Api::Responses, key: Key::Plan(Sign::Plan), ..Provider::bare() },
        Provider { id: CODEX.into(), name: "ChatGPT (Codex sign-in)".into(), base_url: format!("{}/codex", chatgpt_backend()), api: Api::Responses, key: Key::Plan(Sign::Codex), ..Provider::bare() },
    ];
    if std::env::var("OPENAI_API_KEY").is_ok_and(|k| !k.trim().is_empty()) {
        out.push(Provider { id: "openai".into(), name: "OpenAI".into(), base_url: OPENAI_URL.into(), api: Api::Responses, key: Key::Env("OPENAI_API_KEY".into()), ..Provider::bare() });
    }
    out.extend(presets().into_iter().map(|p| p.provider).filter(|p| p.key_source() != "none"));
    let path = config_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e).with_context(|| format!("Couldn't read {}", path.display())),
    };
    let file: Value = serde_json::from_str(&text).with_context(|| format!("{} isn't valid JSON", path.display()))?;
    for (id, p) in file["providers"].as_object().into_iter().flatten() {
        let provider = parse_provider(id, p).with_context(|| format!("In {}", path.display()))?;
        // (One of the file's replaces a built-in one of the same name.)
        out.retain(|o| o.id != provider.id);
        out.push(provider);
    }
    Ok(out)
}

fn parse_provider(id: &str, p: &Value) -> Result<Provider> {
    let id = id.trim().to_owned();
    if id.is_empty() || id.contains('/') {
        bail!("the provider name {id:?} is empty or has a /");
    }
    let base_url = p["baseUrl"].as_str().map(|s| s.trim().trim_end_matches('/').to_owned()).filter(|s| !s.is_empty()).with_context(|| format!("{id} has no baseUrl"))?;
    let api = api_named(&id, p["api"].as_str().unwrap_or("openai-completions"))?;
    let routes = p["routes"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, url)| {
            let url = url.as_str().map(|u| u.trim().trim_end_matches('/').to_owned()).filter(|u| !u.is_empty()).with_context(|| format!("{id}'s route for {name} has no address"))?;
            Ok((api_named(&id, name)?, url))
        })
        .collect::<Result<Vec<_>>>()?;
    let session_header = match p["sessionHeader"].as_str() {
        None => false,
        Some(OPENCODE_SESSION) => true,
        Some(other) => bail!("{id}'s sessionHeader is {other}: only {OPENCODE_SESSION} is known"),
    };
    let key = match p["apiKey"].as_str().map(str::trim).unwrap_or_default() {
        "" | "none" => Key::None,
        k => match k.strip_prefix('$').map(|n| n.trim_start_matches('{').trim_end_matches('}')) {
            Some(var) if !var.is_empty() => Key::Env(var.to_owned()),
            _ => Key::Value(k.to_owned()),
        },
    };
    // (A model's own API is one the provider routes; its thinking, a known
    // way.)
    for m in p["models"].as_array().into_iter().flatten() {
        if let Some(a) = m["api"].as_str()
            && let on = api_named(&id, a)?
            && on != api
            && !routes.iter().any(|(r, _)| *r == on)
        {
            bail!("{id}'s model {} is on {a}, which {id} has no route for", m["id"].as_str().unwrap_or_default());
        }
        if let Some(t) = m["thinking"].as_str() {
            thinking_named(&id, t)?;
        }
    }
    let models = p["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| parse_model(&id, m))
        .collect::<Vec<_>>();
    let thinking = thinking_named(&id, p["thinking"].as_str().unwrap_or("effort"))?;
    Ok(Provider {
        name: p["name"].as_str().filter(|s| !s.is_empty()).unwrap_or(&id).to_owned(),
        id,
        base_url,
        api,
        key,
        models,
        free: p["free"] == true,
        thinking,
        keep_thinking: p["keepThinking"] == true,
        body: p["body"].clone(),
        list: p["listModels"] == true,
        bearer: p["bearer"] == true,
        routes,
        session_header,
        list_url: p["modelsUrl"].as_str().map(|u| u.trim().to_owned()).filter(|u| !u.is_empty()),
    })
}

/// A model as `providers.json` describes it (a bare id, or an object); None
/// without an id.
fn parse_model(id: &str, m: &Value) -> Option<ModelInfo> {
    let (mid, name) = match m {
        Value::String(s) => (s.clone(), short_name(s)),
        _ => {
            let mid = m["id"].as_str()?.to_owned();
            (mid.clone(), m["name"].as_str().map_or_else(|| short_name(&mid), str::to_owned))
        }
    };
    // (pi's level names, in their order: a JSON object's isn't kept.)
    let efforts: Vec<(String, String)> = match m["thinkingLevelMap"].as_object() {
        Some(map) if m["reasoning"] == true => PI_LEVELS.iter().filter_map(|l| map.get(*l)?.as_str().map(|e| (l.to_string(), e.to_owned()))).collect(),
        _ => Vec::new(),
    };
    let levels = match &m["levels"] {
        _ if !efforts.is_empty() => efforts.iter().map(|(l, _)| l.clone()).collect(),
        Value::Array(l) => l.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        _ if m["reasoning"] == true => REASONING_LEVELS.iter().map(std::string::ToString::to_string).collect(),
        _ => Vec::new(),
    };
    let images = m["input"].as_array().map(|i| i.iter().any(|t| t == "image"));
    let mut price = file_price(&m["cost"]);
    if let (Some(p), Some(long)) = (price.as_mut(), file_price(&m["longCost"])) {
        p.long = m["longCost"]["from"].as_u64().map(|from| Long { from, input: long.input, output: long.output, cached: long.cached, write: long.write });
    }
    let peak = file_price(&m["peakCost"]).map(|price| Peak {
        price,
        hours: m["peakCost"]["utcHours"].as_array().into_iter().flatten().filter_map(|h| Some((u32::try_from(h[0].as_u64()?).ok()?, u32::try_from(h[1].as_u64()?).ok()?))).collect(),
        weekdays: m["peakCost"]["weekdays"] == true,
    });
    let mut later: Vec<(u64, Price)> = m["costFrom"].as_array().into_iter().flatten().filter_map(|c| Some((unix_date(c["date"].as_str()?)?, file_price(c)?))).collect();
    later.sort_by_key(|(from, _)| *from);
    let aliases = m["aliases"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
    // (Its own API and thinking: checked above.)
    let api = m["api"].as_str().map(|a| api_named(id, a));
    let thinking = m["thinking"].as_str().map(|t| thinking_named(id, t));
    Some(ModelInfo {
        provider: id.to_owned(),
        id: mid,
        name,
        context_window: m["contextWindow"].as_u64().unwrap_or(0),
        levels,
        efforts,
        images,
        price,
        peak,
        later,
        aliases,
        body: m["body"].clone(),
        max_tokens: m["maxTokens"].as_u64().unwrap_or(0),
        api: api.and_then(Result::ok),
        thinking: thinking.and_then(Result::ok),
    })
}

/// An API by its name in the file.
fn api_named(id: &str, name: &str) -> Result<Api> {
    Ok(match name {
        "openai-completions" | "chat" => Api::Chat,
        "openai-responses" | "responses" => Api::Responses,
        "anthropic-messages" | "messages" => Api::Messages,
        "google-generative-ai" | "gemini" => Api::Gemini,
        other => bail!("{id}'s api is {other}: it's openai-completions, openai-responses, anthropic-messages or google-generative-ai"),
    })
}

/// A way of taking a thinking level, by its name in the file.
fn thinking_named(id: &str, name: &str) -> Result<Thinking> {
    Ok(match name {
        "effort" => Thinking::Effort,
        "reasoning" => Thinking::Reasoning,
        "toggle" => Thinking::Toggle,
        "claude" => Thinking::Claude,
        other => bail!("{id}'s thinking is {other}: it's effort, reasoning, toggle or claude"),
    })
}

/// A model's `cost` in the file: dollars per million tokens.
fn file_price(cost: &Value) -> Option<Price> {
    let rate = |k: &str| cost[k].as_f64().filter(|r| r.is_finite() && *r >= 0.0);
    let (input, output) = (rate("input")?, rate("output")?);
    Some(Price { write: rate("cacheWrite").unwrap_or(input), ..Price::flat(input, output, rate("cacheRead").unwrap_or(input)) })
}

/// A model's price as its server lists it: OpenRouter's `pricing`,
/// dollars a token as strings (a negative: it varies, so unknown), or
/// Together's, dollars a million tokens as numbers (none at all: not
/// priced by the token).
fn listed_price(pricing: &Value) -> Option<Price> {
    let rate = |k: &str| {
        let v = &pricing[k];
        v.as_str().and_then(|s| s.trim().parse::<f64>().ok()).or_else(|| v.as_f64()).filter(|r| r.is_finite() && *r >= 0.0).map(|r| r * 1e6)
    };
    if let (Some(input), Some(output)) = (rate("prompt"), rate("completion")) {
        return Some(Price::flat(input, output, rate("input_cache_read").unwrap_or(input)));
    }
    let per_million = |k: &str| pricing[k].as_f64().filter(|r| r.is_finite() && *r >= 0.0);
    let (input, output) = (per_million("input")?, per_million("output")?);
    (input > 0.0 || output > 0.0).then(|| Price::flat(input, output, per_million("cached_input").unwrap_or(input)))
}

/// A models list as a server sends it: a plan's (`models`, only those to
/// list) or an OpenAI-style one (`data`).
pub fn parse_models(provider: &str, body: &Value) -> Vec<ModelInfo> {
    let window = |m: &Value| {
        ["context_window", "contextWindow", "context_length", "max_context_length", "max_input_tokens"].iter().find_map(|k| m[*k].as_u64()).or_else(|| m["meta"]["n_ctx"].as_u64()).unwrap_or(0)
    };
    let levels = |m: &Value| -> Vec<String> {
        m["supported_reasoning_levels"].as_array().into_iter().flatten().filter_map(|l| l["effort"].as_str().or_else(|| l.as_str()).map(str::to_owned)).collect()
    };
    // (The plan's `input_modalities`; an Ollama-style list's capabilities.)
    let images = |m: &Value| -> Option<bool> {
        let said = |v: &Value, words: &[&str]| v.as_array().map(|l| l.iter().any(|t| t.as_str().is_some_and(|t| words.contains(&t))));
        said(&m["input_modalities"], &["image"]).or_else(|| said(&m["architecture"]["input_modalities"], &["image"])).or_else(|| said(&m["capabilities"], &["vision", "multimodal"]))
    };
    // (OpenRouter's: the efforts it takes, "none" as off.)
    let efforts = |m: &Value| -> Vec<(String, String)> {
        let said: Vec<&str> = m["reasoning"]["supported_efforts"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        PI_LEVELS
            .iter()
            .filter_map(|l| {
                let e = if *l == "off" { "none" } else { l };
                said.contains(&e).then(|| ((*l).to_owned(), e.to_owned()))
            })
            .collect()
    };
    let info = |m: &Value, id: &str, name: &str| ModelInfo {
        provider: provider.to_owned(),
        id: id.to_owned(),
        name: name.to_owned(),
        context_window: window(m),
        levels: if efforts(m).is_empty() { levels(m) } else { efforts(m).into_iter().map(|(l, _)| l).collect() },
        efforts: efforts(m),
        images: images(m),
        price: listed_price(&m["pricing"]),
        ..ModelInfo::default()
    };
    // (Gemini's: `models/<id>`, the ones to chat with. Speech, music,
    // pictures, robots, computer use and deep research answer
    // generateContent too; a chat model is marked `thinking`.)
    const NOT_CHAT: [&str; 5] = ["image", "banana", "robotics", "computer-use", "deep-research"];
    if let Some(list) = body["models"].as_array().filter(|l| l.iter().any(|m| m["name"].as_str().is_some_and(|n| n.starts_with("models/")))) {
        return list
            .iter()
            .filter(|m| m["supportedGenerationMethods"].as_array().is_none_or(|g| g.iter().any(|x| x == "generateContent")))
            .filter(|m| m["thinking"] == true && m["name"].as_str().is_some_and(|n| !NOT_CHAT.iter().any(|w| n.contains(w))))
            .filter_map(|m| {
                let id = m["name"].as_str()?.strip_prefix("models/")?;
                let mut info = info(m, id, m["displayName"].as_str().unwrap_or(id));
                info.context_window = m["inputTokenLimit"].as_u64().unwrap_or(0);
                info.max_tokens = m["outputTokenLimit"].as_u64().unwrap_or(0);
                Some(info)
            })
            .collect();
    }
    // (The plan's list has slugs; llama.cpp sends an Ollama-style `models`
    // beside its `data`.)
    if let Some(list) = body["models"].as_array().filter(|l| body["data"].is_null() || l.iter().any(|m| m["slug"].is_string())) {
        return list
            .iter()
            .filter(|m| m["visibility"].as_str().is_none_or(|v| v == "list"))
            .filter_map(|m| {
                let id = m["slug"].as_str().or_else(|| m["id"].as_str())?;
                Some(info(m, id, &m["display_name"].as_str().map_or_else(|| short_name(id), str::to_owned)))
            })
            .collect();
    }
    // (llama.cpp's capabilities are in the other list.)
    let beside = |id: &str| body["models"].as_array().into_iter().flatten().find(|m| m["model"] == id || m["name"] == id).and_then(images);
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str().map(|id| ModelInfo { images: images(m).or_else(|| beside(id)), ..info(m, id, &m["display_name"].as_str().map_or_else(|| short_name(id), str::to_owned)) }))
        .collect()
}

/// A model's name when it has none but its id: a file's path (llama.cpp
/// lists models by the path they were loaded from) is named by its file,
/// without `.gguf` and a shard's `-00001-of-00003`; other ids as they are
/// (`org/model` too).
pub fn short_name(id: &str) -> String {
    let gguf = id.len() > 5 && id[id.len() - 5..].eq_ignore_ascii_case(".gguf");
    let path = gguf || id.starts_with('/') || id.starts_with("./") || id.starts_with("~/");
    if !path {
        return id.to_owned();
    }
    let file = id.rsplit(['/', '\\']).next().unwrap_or(id);
    let stem = if file.len() > 5 && file[file.len() - 5..].eq_ignore_ascii_case(".gguf") { &file[..file.len() - 5] } else { file };
    // (A shard: name-00001-of-00003.)
    let b = stem.as_bytes();
    let shard = b.len() > 15 && {
        let t = &b[b.len() - 15..];
        t[0] == b'-' && t[1..6].iter().all(u8::is_ascii_digit) && &t[6..10] == b"-of-" && t[10..].iter().all(u8::is_ascii_digit)
    };
    let name = if shard { &stem[..stem.len() - 15] } else { stem };
    if name.is_empty() { id.to_owned() } else { name.to_owned() }
}

/// A model's provider and its id there. `model` is `provider/id`; a bare
/// id (as sessions before providers kept it) is the plan's, and so is
/// `openai/…` when there's no OpenAI key (it named the plan then).
pub fn resolve(model: &str) -> Result<(Provider, String)> {
    let providers = all()?;
    let find = |id: &str| providers.iter().find(|p| p.id == id).cloned();
    let (provider, id) = match model.split_once('/') {
        Some((p, id)) if find(p).is_some() => (find(p).expect("found"), id),
        Some(("openai", id)) => (find(CHATGPT).expect("built in"), id),
        Some((p, _)) => bail!("The provider {p} isn't set up (models are named provider/model)"),
        None => (find(CHATGPT).expect("built in"), model),
    };
    if id.is_empty() {
        bail!("No model named in {model}");
    }
    Ok((provider.for_model(id), id.to_owned()))
}

/// `provider/id`, from what a session or a command named: a bare id, or
/// `openai/…` that [`resolve`] takes as the plan's, becomes `chatgpt/…`.
/// (Left as it was when its provider isn't set up: using it says so.)
pub fn full(model: &str) -> String {
    match resolve(model) {
        Ok((p, id)) => format!("{}/{id}", p.id),
        Err(_) if model.contains('/') => model.to_owned(),
        Err(_) => format!("{CHATGPT}/{model}"),
    }
}

/// Every provider's models (those that answer; how the others failed).
/// The plan's only when signed in.
pub async fn every_model() -> (Vec<ModelInfo>, Vec<String>) {
    let providers = match all() {
        Ok(p) => p,
        Err(e) => return (Vec::new(), vec![format!("{e:#}")]),
    };
    let signed_in = |s: Sign| auth::load(s).is_some();
    let asks = providers.iter().filter(|p| p.sign().is_none_or(signed_in)).map(|p| async move {
        let r = tokio::time::timeout(Duration::from_secs(8), p.models()).await;
        (p, r.unwrap_or_else(|_| Err(anyhow!("{} didn't answer in time", p.name))))
    });
    let mut models = Vec::new();
    let mut errors = Vec::new();
    for (p, r) in futures_util::future::join_all(asks).await {
        match r {
            Ok(m) => models.extend(m),
            Err(e) => errors.push(format!("{}: {e:#}", p.name)),
        }
    }
    if !signed_in(Sign::Plan) && !signed_in(Sign::Codex) && models.is_empty() {
        errors.insert(0, "Not signed in: run `harlo-agent login`".into());
    }
    (models, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn providers_from_the_file() {
        let p = parse_provider("llamacpp", &json!({ "name": "llama.cpp", "baseUrl": "http://127.0.0.1:8090/v1/", "apiKey": "none", "models": ["a", { "id": "b", "name": "Bee", "contextWindow": 8192, "reasoning": true }] })).unwrap();
        assert_eq!((p.base_url.as_str(), p.api, p.key.clone()), ("http://127.0.0.1:8090/v1", Api::Chat, Key::None));
        assert_eq!(p.models[0], ModelInfo { provider: "llamacpp".into(), id: "a".into(), name: "a".into(), context_window: 0, levels: vec![], efforts: vec![], images: None, price: None, ..ModelInfo::default() });
        assert_eq!((p.models[1].name.as_str(), p.models[1].context_window, p.models[1].levels.len()), ("Bee", 8192, 4));
        assert_eq!((p.effort("a", Some("high")), p.effort("b", Some("high")), p.effort("b", None)), (None, Some("high".into()), None));
        let p = parse_provider("x", &json!({ "baseUrl": "https://x/v1", "api": "openai-responses", "apiKey": "${X_KEY}" })).unwrap();
        assert_eq!((p.api, p.key, p.name.as_str()), (Api::Responses, Key::Env("X_KEY".into()), "x"));
        assert_eq!(parse_provider("x", &json!({ "baseUrl": "u", "apiKey": "sk-1" })).unwrap().key, Key::Value("sk-1".into()));
        parse_provider("a/b", &json!({ "baseUrl": "u" })).unwrap_err();
        parse_provider("a", &json!({})).unwrap_err();
        parse_provider("a", &json!({ "baseUrl": "u", "api": "soap" })).unwrap_err();
    }

    #[test]
    fn what_a_model_costs() {
        let file = parse_provider("gw", &json!({ "baseUrl": "https://gw.example.com/v1", "models": [
            { "id": "priced", "cost": { "input": 0.4, "output": 1.6, "cacheRead": 0.1 } },
            { "id": "no-cache-price", "cost": { "input": 3, "output": 15 } },
            { "id": "half", "cost": { "input": 3 } }, "plain"] })).unwrap();
        assert_eq!(file.pricing("priced", None), Pricing::Priced(Price::flat(0.4, 1.6, 0.1)));
        assert_eq!(file.pricing("no-cache-price", None), Pricing::Priced(Price::flat(3.0, 15.0, 3.0)));
        // Half a price, or none: unknown, unless its server lists one.
        assert_eq!(file.pricing("half", None), Pricing::Unknown);
        assert_eq!(file.pricing("plain", None), Pricing::Unknown);
        let listed = parse_models("openrouter", &json!({ "data": [
            { "id": "a/b", "pricing": { "prompt": "0.000003", "completion": "0.000015", "input_cache_read": "0.0000003" } },
            { "id": "auto", "pricing": { "prompt": "-1", "completion": "-1" } }] }));
        let p = listed[0].price.unwrap();
        assert!((p.input - 3.0).abs() < 1e-9 && (p.output - 15.0).abs() < 1e-9 && (p.cached - 0.3).abs() < 1e-9, "{p:?}");
        assert_eq!(listed[1].price, None, "it varies: unknown");
        assert_eq!(file.pricing("plain", listed[0].price), Pricing::Priced(p));
        // Said to be free: nothing extra; not said, unknown, even on this
        // computer (a proxy may pass it on to a paid service); a plan's: in
        // the plan.
        for url in ["http://127.0.0.1:8090/v1", "http://localhost:11434/v1", "http://192.168.1.20:8000/v1", "http://[::1]:8080/v1", "http://gpu:8000/v1", "http://box.local/v1", "http://10.0.0.5/v1"] {
            let p = parse_provider("l", &json!({ "baseUrl": url })).unwrap();
            assert!(p.is_local(), "{url}");
            assert_eq!(p.pricing("m", None), Pricing::Unknown, "{url}");
            assert_eq!(parse_provider("l", &json!({ "baseUrl": url, "free": true })).unwrap().pricing("m", None), Pricing::Local, "{url}");
        }
        for url in ["https://openrouter.ai/api/v1", "https://8.8.8.8/v1", "https://user@api.example.com:443/v1"] {
            let p = parse_provider("o", &json!({ "baseUrl": url })).unwrap();
            assert!(!p.is_local(), "{url}");
            assert_eq!(p.pricing("m", None), Pricing::Unknown, "{url}");
        }
        // A price still wins.
        let priced = parse_provider("l", &json!({ "baseUrl": "http://127.0.0.1:4000/v1", "free": true, "models": [{ "id": "m", "cost": { "input": 1, "output": 2 } }] })).unwrap();
        assert!(matches!(priced.pricing("m", None), Pricing::Priced(_)));
        let plan = Provider { id: CHATGPT.into(), name: "ChatGPT".into(), base_url: auth::RESOURCE.into(), api: Api::Responses, key: Key::Plan(Sign::Plan), ..Provider::bare() };
        assert_eq!(plan.pricing("gpt", None), Pricing::Plan);
    }

    /// As Settings → Model servers writes it (pi's layout).
    #[test]
    fn thinking_level_maps() {
        let p = parse_provider("gpu", &json!({ "baseUrl": "http://gpu:8000/v1", "api": "openai-completions", "apiKey": "none", "models": [
            { "id": "onoff", "reasoning": true, "thinkingLevelMap": { "off": "none", "minimal": null, "low": null, "medium": null, "high": "high" } },
            { "id": "graded", "reasoning": true, "thinkingLevelMap": { "high": "xhigh", "off": "none", "minimal": null, "low": "low", "medium": "medium" } },
            { "id": "plain", "contextWindow": 4096 } ] })).unwrap();
        assert_eq!(p.models[0].levels, ["off", "high"]);
        assert_eq!(p.models[1].levels, ["off", "low", "medium", "high"], "pi's order, not the file's");
        assert_eq!(p.effort("onoff", None), Some("none".into()), "off sends none");
        assert_eq!(p.effort("onoff", Some("medium")), None, "not offered");
        assert_eq!(p.effort("graded", Some("high")), Some("xhigh".into()));
        assert_eq!((p.models[2].levels.len(), p.effort("plain", Some("high"))), (0, None));
    }

    #[test]
    fn models_lists_and_names() {
        let plan = parse_models(CHATGPT, &json!({ "models": [{ "slug": "m1", "display_name": "M1", "context_window": 500000, "supported_reasoning_levels": [{ "effort": "low" }] }, { "slug": "hidden", "visibility": "hide" }] }));
        assert_eq!(plan, vec![ModelInfo { provider: CHATGPT.into(), id: "m1".into(), name: "M1".into(), context_window: 500000, levels: vec!["low".into()], efforts: vec![], images: None, price: None, ..ModelInfo::default() }]);
        // llama.cpp: ids are the files' paths; named by the file.
        let paths = parse_models("llamacpp", &json!({ "models": [{ "name": "/m/Qwen3-4B-Q4_K_M.gguf" }], "data": [
            { "id": "/home/u/models/Qwen3-4B-Q4_K_M.gguf" },
            { "id": "/m/big/GLM-4.6-Q2_K-00001-of-00003.gguf" },
            { "id": "models/llama.GGUF" },
            { "id": "qwen-alias" },
            { "id": "Qwen/Qwen3-8B" }] }));
        let names: Vec<(&str, &str)> = paths.iter().map(|m| (m.id.as_str(), m.name.as_str())).collect();
        assert_eq!(names, [
            ("/home/u/models/Qwen3-4B-Q4_K_M.gguf", "Qwen3-4B-Q4_K_M"),
            ("/m/big/GLM-4.6-Q2_K-00001-of-00003.gguf", "GLM-4.6-Q2_K"),
            ("models/llama.GGUF", "llama"),
            ("qwen-alias", "qwen-alias"),
            ("Qwen/Qwen3-8B", "Qwen/Qwen3-8B"),
        ]);
        let p = parse_provider("llamacpp", &json!({ "baseUrl": "http://127.0.0.1:8090/v1", "models": ["/m/a.gguf", { "id": "/m/b.gguf" }, { "id": "/m/c.gguf", "name": "Cee" }] })).unwrap();
        assert_eq!(p.models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["a", "b", "Cee"]);
        assert_eq!((short_name("a.gguf"), short_name(".gguf"), short_name("/")), ("a".into(), ".gguf".into(), "/".into()));
        let api = parse_models("llamacpp", &json!({ "models": [{ "name": "qwen", "model": "qwen" }], "data": [{ "id": "qwen", "meta": { "n_ctx": 32768 } }] }));
        assert_eq!((api[0].key(), api[0].context_window), ("llamacpp/qwen".into(), 32768));
        assert_eq!(api[0].images, None);
        let api = parse_models("llamacpp", &json!({ "models": [{ "name": "q", "model": "q", "capabilities": ["completion", "multimodal"] }, { "name": "t", "model": "t", "capabilities": ["completion"] }], "data": [{ "id": "q" }, { "id": "t" }, { "id": "u" }] }));
        assert_eq!(api.iter().map(|m| m.images).collect::<Vec<_>>(), [Some(true), Some(false), None]);
        let plan = parse_models(CODEX, &json!({ "models": [{ "slug": "a", "input_modalities": ["text", "image"] }, { "slug": "b", "input_modalities": ["text"] }] }));
        assert_eq!((plan[0].images, plan[1].images), (Some(true), Some(false)));
        let p = parse_provider("x", &json!({ "baseUrl": "u", "models": [{ "id": "v", "input": ["text", "image"] }, { "id": "t", "input": ["text"] }, "s"] })).unwrap();
        assert_eq!(p.models.iter().map(|m| m.images).collect::<Vec<_>>(), [Some(true), Some(false), None]);
        assert_eq!(full("gpt-x"), "chatgpt/gpt-x");
        assert_eq!(full("chatgpt/gpt-x"), "chatgpt/gpt-x");
        // (Unless the machine running the test has an OpenAI key.)
        if std::env::var_os("OPENAI_API_KEY").is_none() {
            assert_eq!(full("openai/gpt-x"), "chatgpt/gpt-x", "the plan's old name");
        }
        assert_eq!(full("nowhere/org/model"), "nowhere/org/model");
    }

    /// Every built-in provider reads, with a key page and a known way to
    /// take thinking; none is listed without a key.
    #[test]
    fn opencode_go_asks_each_model_its_own_way() {
        let go = presets().into_iter().find(|p| p.provider.id == "opencode-go").expect("a preset").provider;
        let on = |model: &str| {
            let p = go.clone().for_model(model);
            (p.api, p.base_url, p.thinking)
        };
        let at = |path: &str| format!("https://opencode.ai/inference/go/{path}");
        assert_eq!(on("glm-5.3"), (Api::Chat, at("openai/v1"), Thinking::Effort));
        assert_eq!(on("claude-haiku-5-5"), (Api::Messages, at("anthropic/v1"), Thinking::Claude));
        assert_eq!(on("minimax-m3"), (Api::Messages, at("anthropic/v1"), Thinking::Effort));
        assert_eq!(on("gemini-4-argon"), (Api::Gemini, at("google/v1beta"), Thinking::Effort));
        assert_eq!(on("gpt-6-luna"), (Api::Responses, at("openai/v1"), Thinking::Effort));
        assert_eq!(on("a-listed-one"), (Api::Chat, at("openai/v1"), Thinking::Effort), "one only its list names: chat");
        assert_eq!(go.models_url(), at("v1/models"));
        // The conversation's id with each request; only for it.
        assert_eq!(go.session_headers("s1"), vec![(OPENCODE_SESSION, "s1".to_owned())]);
        assert!(go.session_headers("").is_empty());
        let xai = presets().into_iter().find(|p| p.provider.id == "xai").expect("a preset").provider;
        assert!(xai.session_headers("s1").is_empty());
        // A model on an API the provider has no route for: the file's mistake.
        let bad = json!({ "baseUrl": "https://x/v1", "models": [{ "id": "m", "api": "anthropic-messages" }] });
        parse_provider("x", &bad).unwrap_err();
        let bad = json!({ "baseUrl": "https://x/v1", "sessionHeader": "x-other" });
        parse_provider("x", &bad).unwrap_err();
    }

    #[test]
    fn the_built_in_providers() {
        let presets = presets();
        let ids: Vec<&str> = presets.iter().map(|p| p.provider.id.as_str()).collect();
        assert_eq!(ids.len(), 16, "{ids:?}");
        for p in &presets {
            assert!(p.key_page.starts_with("https://") && p.price_page.starts_with("https://") && p.provider.base_url.starts_with("https://"), "{}", p.provider.id);
            assert!(!p.key_env.is_empty() && (p.check == "chat" || p.check.starts_with('/')), "{}", p.provider.id);
            assert!(matches!(p.provider.key, Key::Saved { .. }), "{}", p.provider.id);
            for m in &p.provider.models {
                assert!(m.levels.iter().all(|l| PI_LEVELS.contains(&l.as_str())), "{}: {:?}", m.id, m.levels);
            }
        }
        let by = |id: &str| presets.iter().find(|p| p.provider.id == id).unwrap().provider.clone();
        assert_eq!((by("xai").api, by("deepseek").thinking, by("openrouter").thinking), (Api::Responses, Thinking::Toggle, Thinking::Reasoning));
        let ds = by("deepseek");
        assert!(ds.keep_thinking && !by("groq").keep_thinking);
        assert_eq!((ds.effort("deepseek-v4-pro", None), ds.effort("deepseek-v4-pro", Some("xhigh"))), (Some("none".into()), Some("max".into())));
        assert_eq!(by("minimax").extra("MiniMax-M3"), json!({ "reasoning_split": true }));
        assert_eq!(by("groq").extra("qwen/qwen3.8-27b"), json!({ "reasoning_format": "parsed" }));
        assert_eq!(by("groq").extra("openai/gpt-oss-120b"), json!({}));
        // (OpenRouter's take a level, whatever the model.)
        assert_eq!(by("openrouter").effort("any/model", Some("low")), Some("low".into()));
        assert_eq!(by("mistral").known("mistral-large-4").map(|m| m.id.as_str()), Some("mistral-large-4-0"), "by its alias");
        // Claude: off only where the model takes it, each its own way; cache
        // writes priced; a long reply room.
        let claude = by("anthropic");
        assert_eq!((claude.api, claude.thinking, claude.max_tokens("claude-opus-5-5")), (Api::Messages, Thinking::Claude, 64_000));
        assert_eq!((claude.effort("claude-opus-5-5", None), claude.effort("claude-sonnet-5-5", None), claude.effort("claude-haiku-5-5", None)), (None, Some("between_tools".into()), Some("disabled".into())));
        assert_eq!(claude.known("claude-opus-5-5").and_then(|m| m.price).map(|p| p.write), Some(5.0));
        // The Anthropic endpoints share their company's key.
        let shared = |id: &str| presets.iter().find(|p| p.provider.id == id).map(|p| (p.key_of.clone(), p.provider.key.clone()));
        assert_eq!(shared("minimax-anthropic"), Some(("minimax".into(), Key::Saved { provider: "minimax".into(), env: "MINIMAX_API_KEY".into() })));
        assert_eq!(shared("deepseek-anthropic").map(|(k, _)| k), Some("deepseek".into()));
        assert!(by("minimax-anthropic").bearer && !by("deepseek-anthropic").bearer && !claude.bearer);
        assert_eq!(by("minimax-anthropic").effort("MiniMax-M3", Some("high")), Some("adaptive".into()));
        assert_eq!(by("deepseek-anthropic").effort("deepseek-flash", None), Some("disabled".into()));
        // Gemini: its key as x-goog-api-key, its levels by name, the price
        // from the day it changes.
        let g = by("gemini");
        assert_eq!((g.api, g.key_header(Some("k")), g.effort("gemini-4-carbon", Some("high"))), (Api::Gemini, vec![("x-goog-api-key", "k".to_owned())], Some("high".into())));
        let carbon = g.known("gemini-4-carbon").unwrap();
        let dec1 = unix_date("2026-12-01").unwrap();
        assert_eq!((dec1, carbon.price_at(dec1 - 1).map(|p| p.input), carbon.price_at(dec1).map(|p| p.input)), (1_796_083_200, Some(2.0), Some(4.0)));
        assert_eq!((unix_date("2026-11-16"), unix_date("2024-02-29"), unix_date("2026-13-01")), (Some(1_794_787_200), Some(1_709_164_800), None));
        let listed = parse_models("gemini", &json!({ "models": [
            { "name": "models/gemini-4-carbon", "displayName": "Gemini 4 Carbon", "inputTokenLimit": 1_048_576, "outputTokenLimit": 65_536, "supportedGenerationMethods": ["generateContent", "countTokens"], "thinking": true },
            { "name": "models/text-embedding-5", "supportedGenerationMethods": ["embedContent"] },
            { "name": "models/gemini-3.8-flash-tts", "supportedGenerationMethods": ["generateContent", "countTokens"] },
            { "name": "models/lyria-3.5", "supportedGenerationMethods": ["generateContent", "countTokens"] },
            { "name": "models/gemini-3.1-flash-image", "supportedGenerationMethods": ["generateContent"], "thinking": true },
            { "name": "models/gemini-robotics-er-2-preview", "supportedGenerationMethods": ["generateContent"], "thinking": true },
            { "name": "models/deep-research-preview-04-2026", "supportedGenerationMethods": ["generateContent"], "thinking": true },
        ] }));
        assert_eq!(listed.len(), 1, "only the chat model: {listed:?}");
        assert_eq!((listed[0].id.as_str(), listed[0].name.as_str(), listed[0].context_window, listed[0].max_tokens), ("gemini-4-carbon", "Gemini 4 Carbon", 1_048_576, 65_536));
        // Anthropic's models list.
        let listed = parse_models("anthropic", &json!({ "data": [{ "type": "model", "id": "claude-opus-5-5", "display_name": "Claude Opus 5.5", "max_input_tokens": 1_000_000, "max_tokens": 128_000 }] }));
        assert_eq!((listed[0].name.as_str(), listed[0].context_window), ("Claude Opus 5.5", 1_000_000));
        assert_eq!(claude.with_known(listed[0].clone()).price.map(|p| p.input), Some(4.0));
    }

    #[test]
    fn peak_and_long_input_prices() {
        let ds = presets().into_iter().find(|p| p.provider.id == "deepseek").unwrap().provider;
        let peak = ds.known("deepseek-flash").unwrap().peak.clone().unwrap();
        // 2026-11-16 was a Monday.
        let monday = 1_794_787_200;
        assert!(peak.covers(monday + 2 * 3600) && peak.covers(monday + 9 * 3600 + 3599));
        assert!(!peak.covers(monday + 4 * 3600) && !peak.covers(monday + 12 * 3600));
        assert!(!peak.covers(monday + 5 * 86_400 + 2 * 3600), "not on a Saturday");
        let grok = presets().into_iter().find(|p| p.provider.id == "xai").unwrap().provider;
        let price = grok.known("grok-4.7").unwrap().price.unwrap();
        let cost = |input: u64| {
            let u = crate::used::Used { requests: 1, input, output: 1_000_000, ..crate::used::Used::default() };
            u.priced(&Pricing::Priced(price)).cost
        };
        assert!((cost(100_000) - (0.2 + 6.0)).abs() < 1e-9, "{}", cost(100_000));
        assert!((cost(200_000) - (0.8 + 12.0)).abs() < 1e-9, "the whole request at the long rates: {}", cost(200_000));
    }

    /// What a server lists, with what's known of its models over it.
    #[test]
    fn listed_models_with_what_is_known() {
        let p = parse_provider("t", &json!({ "baseUrl": "https://t/v1", "listModels": true, "models": [{ "id": "known", "aliases": ["known-alias"], "contextWindow": 1000, "cost": { "input": 1, "output": 2 }, "reasoning": true, "thinkingLevelMap": { "high": "max" } }] })).unwrap();
        let listed = parse_models("t", &json!({ "data": [
            { "id": "known-alias", "context_length": 5000, "pricing": { "input": 0.5, "output": 0.9 } },
            { "id": "new", "pricing": { "input": 0.2, "output": 0.4, "cached_input": 0.02 } },
            { "id": "dedicated", "pricing": { "hourly": 3.2, "input": 0, "output": 0 } }] }));
        let merged: Vec<ModelInfo> = listed.into_iter().map(|m| p.with_known(m)).collect();
        assert_eq!((merged[0].id.as_str(), merged[0].context_window, merged[0].levels.clone()), ("known-alias", 1000, vec!["high".to_owned()]));
        assert_eq!(merged[0].price, Some(Price::flat(1.0, 2.0, 1.0)), "the known price wins");
        assert_eq!(merged[1].price, Some(Price::flat(0.2, 0.4, 0.02)), "Together's list, per million");
        assert_eq!(merged[2].price, None, "not priced by the token");
        // OpenRouter's efforts and modalities.
        let or = parse_models("openrouter", &json!({ "data": [{ "id": "a/b", "architecture": { "input_modalities": ["text", "image"] }, "reasoning": { "supported_efforts": ["none", "low", "high", "max"] } }] }));
        assert_eq!((or[0].images, or[0].levels.clone()), (Some(true), vec!["off".to_owned(), "low".into(), "high".into()]));
        assert_eq!(or[0].efforts[0], ("off".to_owned(), "none".to_owned()));
        parse_provider("x", &json!({ "baseUrl": "u", "thinking": "loud" })).unwrap_err();
    }

    /// A key is checked against a stand-in server: taken, refused, or
    /// tried with one token where nothing free can be asked.
    #[tokio::test]
    async fn a_key_is_checked() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let lower = req.to_ascii_lowercase();
                let good = req.contains("Bearer good") || (lower.contains("x-api-key: good") && lower.contains("anthropic-version: 2023-06-01") && !lower.contains("authorization"));
                let (status, body) = match (good, req.starts_with("POST /v1/chat/completions") || req.starts_with("POST /v1/messages")) {
                    (true, true) if req.contains("\"max_tokens\":1") && req.contains("\"model\":\"cheap\"") => ("200 OK", "{}"),
                    (true, false) if req.starts_with("GET /v1/models") => ("200 OK", "{\"data\":[]}"),
                    (true, _) => ("400 Bad Request", "{\"error\":{\"message\":\"unexpected\"}}"),
                    (false, _) => ("401 Unauthorized", "{\"error\":{\"message\":\"Invalid API key\"}}"),
                };
                let _ = sock.write_all(format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await;
            }
        });
        let provider = parse_provider("s", &json!({ "name": "Stand-in", "baseUrl": base, "models": [{ "id": "dear", "cost": { "input": 5, "output": 5 } }, { "id": "cheap", "cost": { "input": 0.1, "output": 0.1 } }] })).unwrap();
        let get = Preset { provider: provider.clone(), key_env: "S_KEY".into(), key_page: String::new(), price_page: String::new(), note: String::new(), check: "/models".into(), key_of: "s".into() };
        assert_eq!(get.check_key("good").await.unwrap(), "Stand-in took the key");
        let e = format!("{:#}", get.check_key("bad").await.unwrap_err());
        assert!(e.contains("refused the key") && e.contains("Invalid API key"), "{e}");
        let chat = Preset { check: "chat".into(), ..get.clone() };
        chat.check_key("good").await.unwrap();
        // A Messages API server's: the key as x-api-key, the request to /messages.
        let messages = Preset { provider: Provider { api: Api::Messages, ..provider.clone() }, ..get.clone() };
        messages.check_key("good").await.unwrap();
        Preset { check: "chat".into(), ..messages.clone() }.check_key("good").await.unwrap();
        assert!(format!("{:#}", messages.check_key("bad").await.unwrap_err()).contains("refused the key"));
        // One that takes it as a bearer token.
        Preset { provider: Provider { bearer: true, ..messages.provider.clone() }, check: "chat".into(), ..get }.check_key("good").await.unwrap();
    }
}
