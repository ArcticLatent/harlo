//! Where requests go. A model is named `provider/model`:
//!
//! - `chatgpt`: the signed-in ChatGPT plan (Responses API).
//! - `openai`: an OpenAI API key in `OPENAI_API_KEY` (Responses API).
//! - those in `providers.json` (harlo-agent's config folder): OpenAI-compatible
//!   servers, local or online, with chat completions (the default) or the
//!   Responses API:
//!
//! ```json
//! { "providers": [
//!     { "id": "llamacpp", "name": "llama.cpp", "baseUrl": "http://127.0.0.1:8090/v1",
//!       "api": "chat", "apiKey": "…" | "apiKeyEnv": "NAME",
//!       "models": ["qwen3", { "id": "…", "name": "…", "contextWindow": 32768, "reasoning": true }] } ] }
//! ```
//!
//! A provider without `models` lists what its `/models` says. The file is
//! read for each request, so a change to it applies to sessions running.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::Value;

use crate::auth;

/// The plan's provider.
pub const CHATGPT: &str = "chatgpt";
const OPENAI_URL: &str = "https://api.openai.com/v1";
/// Thinking levels a model that reasons takes when it doesn't say.
const REASONING_LEVELS: [&str; 4] = ["minimal", "low", "medium", "high"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    Responses,
    Chat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Key {
    /// The ChatGPT sign-in.
    Plan,
    None,
    Value(String),
    Env(String),
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
}

/// A model one can pick.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub provider: String,
    pub id: String,
    /// Its name for people.
    pub name: String,
    /// Tokens, or 0 when not said.
    pub context_window: u64,
    /// Thinking levels it takes (none: it doesn't reason, or doesn't say).
    pub levels: Vec<String>,
}

impl ModelInfo {
    /// `provider/id`.
    pub fn key(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }
}

impl Provider {
    pub fn is_plan(&self) -> bool {
        self.key == Key::Plan
    }

    /// The bearer token (none for a server without a key). `force`: the
    /// plan's token refreshed even if it looks valid.
    pub async fn token(&self, force: bool) -> Result<Option<String>> {
        match &self.key {
            Key::Plan => Ok(Some(auth::access_token(force).await?)),
            Key::None => Ok(None),
            Key::Value(k) => Ok(Some(k.clone())),
            Key::Env(name) => std::env::var(name).map(Some).map_err(|_| anyhow!("{}'s key isn't set: ${name} is empty", self.name)),
        }
    }

    /// Whether to send `model` a thinking level: the plan's always (its
    /// models all reason); a configured model when it has levels; others
    /// when they're known reasoning models by name.
    pub fn reasons(&self, model: &str) -> bool {
        if self.is_plan() {
            return true;
        }
        if let Some(m) = self.models.iter().find(|m| m.id == model) {
            return !m.levels.is_empty();
        }
        let bare = model.rsplit('/').next().unwrap_or(model);
        self.api == Api::Responses && (bare.starts_with("gpt-5") || bare.starts_with("gpt-6") || (bare.starts_with('o') && bare[1..].starts_with(|c: char| c.is_ascii_digit())))
    }

    /// Its models: as configured, else as its server lists them.
    pub async fn models(&self) -> Result<Vec<ModelInfo>> {
        if !self.models.is_empty() {
            return Ok(self.models.clone());
        }
        let token = self.token(false).await?;
        let body = crate::responses::models_raw(&self.base_url, token.as_deref()).await.with_context(|| format!("{}'s models", self.name))?;
        let models = parse_models(&self.id, &body);
        if models.is_empty() {
            bail!("{} lists no models", self.base_url);
        }
        Ok(models)
    }
}

fn config_file() -> PathBuf {
    auth::config_dir().join("providers.json")
}

/// The providers: the plan, an OpenAI key, then the file's.
pub fn all() -> Result<Vec<Provider>> {
    let mut out = vec![Provider { id: CHATGPT.into(), name: "ChatGPT".into(), base_url: auth::RESOURCE.into(), api: Api::Responses, key: Key::Plan, models: Vec::new() }];
    if std::env::var("OPENAI_API_KEY").is_ok_and(|k| !k.trim().is_empty()) {
        out.push(Provider { id: "openai".into(), name: "OpenAI".into(), base_url: OPENAI_URL.into(), api: Api::Responses, key: Key::Env("OPENAI_API_KEY".into()), models: Vec::new() });
    }
    let path = config_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e).with_context(|| format!("Couldn't read {}", path.display())),
    };
    let file: Value = serde_json::from_str(&text).with_context(|| format!("{} isn't valid JSON", path.display()))?;
    for p in file["providers"].as_array().into_iter().flatten() {
        let provider = parse_provider(p).with_context(|| format!("In {}", path.display()))?;
        // (One of the file's replaces a built-in one of the same name.)
        out.retain(|o| o.id != provider.id);
        out.push(provider);
    }
    Ok(out)
}

fn parse_provider(p: &Value) -> Result<Provider> {
    let id = p["id"].as_str().map(str::trim).filter(|s| !s.is_empty()).context("a provider has no id")?.to_owned();
    if id.contains('/') {
        bail!("the provider id {id} has a /");
    }
    let base_url = p["baseUrl"].as_str().map(|s| s.trim().trim_end_matches('/').to_owned()).filter(|s| !s.is_empty()).with_context(|| format!("{id} has no baseUrl"))?;
    let api = match p["api"].as_str().unwrap_or("chat") {
        "chat" | "openai-completions" => Api::Chat,
        "responses" | "openai-responses" => Api::Responses,
        other => bail!("{id}'s api is {other}: it's chat or responses"),
    };
    let key = match (p["apiKey"].as_str().filter(|k| !k.is_empty()), p["apiKeyEnv"].as_str().filter(|k| !k.is_empty())) {
        (Some(k), _) => Key::Value(k.to_owned()),
        (None, Some(env)) => Key::Env(env.to_owned()),
        (None, None) => Key::None,
    };
    let models = p["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let (mid, name) = match m {
                Value::String(s) => (s.clone(), s.clone()),
                _ => {
                    let mid = m["id"].as_str()?.to_owned();
                    (mid.clone(), m["name"].as_str().unwrap_or(&mid).to_owned())
                }
            };
            let levels = match &m["levels"] {
                Value::Array(l) => l.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
                _ if m["reasoning"] == true => REASONING_LEVELS.iter().map(|l| l.to_string()).collect(),
                _ => Vec::new(),
            };
            Some(ModelInfo { provider: id.clone(), id: mid, name, context_window: m["contextWindow"].as_u64().unwrap_or(0), levels })
        })
        .collect();
    Ok(Provider { name: p["name"].as_str().filter(|s| !s.is_empty()).unwrap_or(&id).to_owned(), id, base_url, api, key, models })
}

/// A models list as a server sends it: a plan's (`models`, only those to
/// list) or an OpenAI-style one (`data`).
pub fn parse_models(provider: &str, body: &Value) -> Vec<ModelInfo> {
    let window = |m: &Value| {
        ["context_window", "contextWindow", "context_length", "max_context_length"].iter().find_map(|k| m[*k].as_u64()).or_else(|| m["meta"]["n_ctx"].as_u64()).unwrap_or(0)
    };
    let levels = |m: &Value| -> Vec<String> {
        m["supported_reasoning_levels"].as_array().into_iter().flatten().filter_map(|l| l["effort"].as_str().or_else(|| l.as_str()).map(str::to_owned)).collect()
    };
    let info = |m: &Value, id: &str, name: &str| ModelInfo { provider: provider.to_owned(), id: id.to_owned(), name: name.to_owned(), context_window: window(m), levels: levels(m) };
    // (The plan's list has slugs; llama.cpp sends an Ollama-style `models`
    // beside its `data`.)
    if let Some(list) = body["models"].as_array().filter(|l| body["data"].is_null() || l.iter().any(|m| m["slug"].is_string())) {
        return list
            .iter()
            .filter(|m| m["visibility"].as_str().is_none_or(|v| v == "list"))
            .filter_map(|m| {
                let id = m["slug"].as_str().or_else(|| m["id"].as_str())?;
                Some(info(m, id, m["display_name"].as_str().unwrap_or(id)))
            })
            .collect();
    }
    body["data"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str().map(|id| info(m, id, id))).collect()
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
    Ok((provider, id.to_owned()))
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
    let signed_in = auth::load().is_some();
    let asks = providers.iter().filter(|p| !p.is_plan() || signed_in).map(|p| async move {
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
    if !signed_in && models.is_empty() {
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
        let p = parse_provider(&json!({ "id": "llamacpp", "name": "llama.cpp", "baseUrl": "http://127.0.0.1:8090/v1/", "models": ["a", { "id": "b", "name": "Bee", "contextWindow": 8192, "reasoning": true }] })).unwrap();
        assert_eq!((p.base_url.as_str(), p.api, p.key.clone()), ("http://127.0.0.1:8090/v1", Api::Chat, Key::None));
        assert_eq!(p.models[0], ModelInfo { provider: "llamacpp".into(), id: "a".into(), name: "a".into(), context_window: 0, levels: vec![] });
        assert_eq!((p.models[1].name.as_str(), p.models[1].context_window, p.models[1].levels.len()), ("Bee", 8192, 4));
        let p = parse_provider(&json!({ "id": "x", "baseUrl": "https://x/v1", "api": "responses", "apiKeyEnv": "X_KEY" })).unwrap();
        assert_eq!((p.api, p.key, p.name.as_str()), (Api::Responses, Key::Env("X_KEY".into()), "x"));
        assert!(parse_provider(&json!({ "id": "a/b", "baseUrl": "u" })).is_err());
        assert!(parse_provider(&json!({ "id": "a" })).is_err());
        assert!(parse_provider(&json!({ "id": "a", "baseUrl": "u", "api": "soap" })).is_err());
    }

    #[test]
    fn models_lists_and_names() {
        let plan = parse_models(CHATGPT, &json!({ "models": [{ "slug": "m1", "display_name": "M1", "context_window": 500000, "supported_reasoning_levels": [{ "effort": "low" }] }, { "slug": "hidden", "visibility": "hide" }] }));
        assert_eq!(plan, vec![ModelInfo { provider: CHATGPT.into(), id: "m1".into(), name: "M1".into(), context_window: 500000, levels: vec!["low".into()] }]);
        let api = parse_models("llamacpp", &json!({ "models": [{ "name": "qwen", "model": "qwen" }], "data": [{ "id": "qwen", "meta": { "n_ctx": 32768 } }] }));
        assert_eq!((api[0].key(), api[0].context_window), ("llamacpp/qwen".into(), 32768));
        assert_eq!(full("gpt-x"), "chatgpt/gpt-x");
        assert_eq!(full("chatgpt/gpt-x"), "chatgpt/gpt-x");
        // (Unless the machine running the test has an OpenAI key.)
        if std::env::var_os("OPENAI_API_KEY").is_none() {
            assert_eq!(full("openai/gpt-x"), "chatgpt/gpt-x", "the plan's old name");
        }
        assert_eq!(full("nowhere/org/model"), "nowhere/org/model");
    }
}
