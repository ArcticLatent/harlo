//! The configuration the engine reads in place: OpenCode's own files.
//!
//! People who used OpenCode keep their setup: `opencode.json` (or
//! `.jsonc`) in OpenCode's config folder names the providers, their
//! models, the default model and permission rules, in OpenCode 2's keys.
//! The engine reads what it supports and leaves the file as it is.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Error;
use crate::rules::Rule;

/// A model, named by its provider and its own ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRef {
    /// The provider's ID, such as `openrouter`.
    #[serde(rename = "providerID")]
    pub provider: String,
    /// The model's ID at that provider.
    #[serde(rename = "id")]
    pub model: String,
    /// One of the model's variants, such as `high`, when one was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// A model's prices, in US dollars per million tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Price {
    /// The context size from which these prices apply; none for the base
    /// prices.
    pub from: Option<u64>,
    /// Input not read from the cache.
    pub input: f64,
    /// Output, reasoning included.
    pub output: f64,
    /// Input read from the cache.
    pub cache_read: f64,
    /// Input written to the cache.
    pub cache_write: f64,
}

impl Price {
    /// What `tokens` cost at these prices, in US dollars.
    #[must_use]
    pub fn of(&self, tokens: &crate::message::Tokens) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "token counts are far below 2^52; cents are what matter"
        )]
        let dollars = |count: u64, per_million: f64| count as f64 * per_million / 1_000_000.0;
        dollars(tokens.input, self.input)
            + dollars(tokens.output, self.output)
            + dollars(tokens.cache.read, self.cache_read)
            + dollars(tokens.cache.write, self.cache_write)
    }
}

/// A variant of a model: what its requests add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantSpec {
    /// Its ID, such as `high`.
    pub id: String,
    /// Fields merged into the request's body.
    pub body: Map<String, Value>,
    /// Headers added to the request.
    pub headers: Vec<(String, String)>,
}

/// One model a provider offers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelSpec {
    /// Its ID at the provider.
    pub id: String,
    /// The name shown for it.
    pub name: String,
    /// Its context window in tokens, when known.
    pub context: Option<u64>,
    /// The most it writes in one reply, when known.
    pub output: Option<u64>,
    /// Fields merged into every request's body.
    pub body: Map<String, Value>,
    /// Headers added to every request.
    pub headers: Vec<(String, String)>,
    /// Its variants, in the order the file lists them.
    pub variants: Vec<VariantSpec>,
    /// Its prices: the base ones, and any for larger contexts.
    pub prices: Vec<Price>,
}

impl ModelSpec {
    /// What `tokens` of one step cost: at the prices for the step's
    /// context (all its input), or nothing when no prices are known.
    #[must_use]
    pub fn cost(&self, tokens: &crate::message::Tokens) -> f64 {
        let context = tokens.input + tokens.cache.read + tokens.cache.write;
        self.prices
            .iter()
            .filter(|p| p.from.is_none_or(|from| context > from))
            .max_by_key(|p| p.from.unwrap_or(0))
            .map_or(0.0, |p| p.of(tokens))
    }

    /// What a request with `variant` adds: the model's body and headers,
    /// then the variant's over them.
    #[must_use]
    pub fn request_extras(
        &self,
        variant: Option<&str>,
    ) -> (Map<String, Value>, Vec<(String, String)>) {
        let mut body = self.body.clone();
        let mut headers = self.headers.clone();
        if let Some(v) = variant.and_then(|id| self.variants.iter().find(|v| v.id == id)) {
            body.extend(v.body.clone());
            headers.retain(|(name, _)| {
                !v.headers
                    .iter()
                    .any(|(over, _)| over.eq_ignore_ascii_case(name))
            });
            headers.extend(v.headers.clone());
        }
        (body, headers)
    }
}

/// A provider: where its OpenAI-compatible API is and how to sign requests.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderSpec {
    /// Its ID, as models name it.
    pub id: String,
    /// The name shown for it.
    pub name: String,
    /// The API's base URL, up to `/chat/completions`.
    pub base_url: String,
    /// Its key, sent as a bearer token; none for servers that take none.
    pub api_key: Option<String>,
    /// Its models, in the order the file lists them.
    pub models: Vec<ModelSpec>,
}

/// What the engine read from the configuration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    /// The file it came from, if one was found.
    pub source: Option<PathBuf>,
    /// The providers with an OpenAI-compatible API.
    pub providers: Vec<ProviderSpec>,
    /// The model new sessions use, when the file names one.
    pub model: Option<ModelRef>,
    /// Permission rules from the file, applied after the agent's own.
    pub permissions: Vec<Rule>,
    /// Extra instruction files (`instructions`): paths, `*` patterns, URLs.
    pub instructions: Vec<String>,
    /// Extra skill folders (`skills`).
    pub skills: Vec<String>,
    /// Commands (`commands`), by name: `template`, `description`, `agent`
    /// and `model`.
    pub commands: Value,
}

impl Config {
    /// Reads OpenCode's configuration from its folder under `config_home`
    /// (usually `$XDG_CONFIG_HOME`), or an empty one when there is none.
    ///
    /// # Errors
    ///
    /// Fails when the file exists but can't be read or parsed.
    pub fn load(config_home: &Path) -> Result<Self, Error> {
        let folder = config_home.join("opencode");
        for name in ["opencode.json", "opencode.jsonc"] {
            let path = folder.join(name);
            match std::fs::read_to_string(&path) {
                Ok(text) => return Self::parse(&text, Some(path)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(Error::config(format!(
                        "Couldn't read {}: {e}",
                        path.display()
                    )));
                }
            }
        }
        Ok(Self::default())
    }

    /// Reads a configuration from the text of an OpenCode config file.
    ///
    /// # Errors
    ///
    /// Fails when `text` isn't JSON (comments and trailing commas allowed).
    pub fn parse(text: &str, source: Option<PathBuf>) -> Result<Self, Error> {
        let shown = source.as_deref().map_or_else(
            || "the configuration".to_owned(),
            |p| p.display().to_string(),
        );
        let value: Value =
            jsonc_parser::parse_to_serde_value(text, &jsonc_parser::ParseOptions::default())
                .map_err(|e| Error::config(format!("Couldn't parse {shown}: {e}")))?;
        let providers = value["providers"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(id, p)| provider(id, p))
            .collect();
        let permissions = value["permissions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| serde_json::from_value(r.clone()).ok())
            .collect();
        Ok(Self {
            source,
            providers,
            model: model_ref(&value["model"]),
            permissions,
            instructions: strings(&value["instructions"]),
            skills: strings(&value["skills"]),
            commands: value["commands"].clone(),
        })
    }

    /// The provider with ID `id`.
    #[must_use]
    pub fn provider(&self, id: &str) -> Option<&ProviderSpec> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// The model new sessions use: the one the file names, else the first
    /// model of the first provider.
    #[must_use]
    pub fn default_model(&self) -> Option<ModelRef> {
        self.model.clone().or_else(|| {
            self.providers.iter().find_map(|p| {
                p.models.first().map(|m| ModelRef {
                    provider: p.id.clone(),
                    model: m.id.clone(),
                    variant: None,
                })
            })
        })
    }

    /// The provider and model `model` names, when both are configured.
    #[must_use]
    pub fn resolve(&self, model: &ModelRef) -> Option<(&ProviderSpec, &ModelSpec)> {
        let provider = self.provider(&model.provider)?;
        let spec = provider.models.iter().find(|m| m.id == model.model)?;
        Some((provider, spec))
    }
}

/// A provider entry, when it has a base URL to send to.
fn provider(id: &str, p: &Value) -> Option<ProviderSpec> {
    let settings = &p["settings"];
    let base_url = settings["baseURL"]
        .as_str()
        .or_else(|| p["options"]["baseURL"].as_str())?;
    let api_key = settings["apiKey"]
        .as_str()
        .or_else(|| p["options"]["apiKey"].as_str())
        .map(str::to_owned);
    let models = p["models"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(model, m)| ModelSpec {
            id: model.clone(),
            name: m["name"].as_str().unwrap_or(model).to_owned(),
            context: m["limit"]["context"].as_u64(),
            output: m["limit"]["output"].as_u64(),
            body: m["body"].as_object().cloned().unwrap_or_default(),
            headers: headers_of(&m["headers"]),
            variants: m["variants"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| {
                    Some(VariantSpec {
                        id: v["id"].as_str()?.to_owned(),
                        body: v["body"].as_object().cloned().unwrap_or_default(),
                        headers: headers_of(&v["headers"]),
                    })
                })
                .collect(),
            prices: m["cost"].as_array().map_or_else(
                // (A single object is read as the base prices.)
                || price(&m["cost"]).into_iter().collect(),
                |list| list.iter().filter_map(price).collect(),
            ),
        })
        .collect();
    Some(ProviderSpec {
        id: id.to_owned(),
        name: p["name"].as_str().unwrap_or(id).to_owned(),
        base_url: base_url.trim_end_matches('/').to_owned(),
        api_key,
        models,
    })
}

/// Prices from a config entry: `input`, `output`, `cache.read` and
/// `cache.write`, with `tier.size` for larger contexts.
fn price(v: &Value) -> Option<Price> {
    let number = |v: &Value| v.as_f64().unwrap_or(0.0);
    v.is_object().then(|| Price {
        from: v["tier"]["size"].as_u64(),
        input: number(&v["input"]),
        output: number(&v["output"]),
        cache_read: number(&v["cache"]["read"]),
        cache_write: number(&v["cache"]["write"]),
    })
}

/// A JSON object of header names and values.
fn headers_of(v: &Value) -> Vec<(String, String)> {
    v.as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
        .collect()
}

/// The strings in a JSON array; none when it isn't one.
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// `"provider/model#variant"` (the variant optional) or
/// `{ "providerID", "model", "variant" }`.
pub(crate) fn model_ref(v: &Value) -> Option<ModelRef> {
    if let Some(s) = v.as_str() {
        let (provider, model) = s.split_once('/')?;
        let (model, variant) = match model.split_once('#') {
            Some((model, variant)) => (model, Some(variant.to_owned())),
            None => (model, None),
        };
        return Some(ModelRef {
            provider: provider.to_owned(),
            model: model.to_owned(),
            variant,
        });
    }
    let provider = v["providerID"].as_str()?;
    let model = v["model"].as_str().or_else(|| v["id"].as_str())?;
    Some(ModelRef {
        provider: provider.to_owned(),
        model: model.to_owned(),
        variant: v["variant"].as_str().map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Effect;

    #[test]
    fn reads_opencode_2_keys_with_comments() {
        let text = r#"{
            // The sandbox's.
            "model": { "providerID": "fake", "model": "fake-coder" },
            "permissions": [ { "action": "shell", "resource": "*", "effect": "ask" } ],
            "providers": {
                "fake": {
                    "name": "Fake",
                    "settings": { "baseURL": "http://127.0.0.1:9/v1/", "apiKey": "k" },
                    "models": { "fake-coder": {
                        "name": "Fake Coder", "limit": { "context": 128000, "output": 8192 },
                        "body": { "temperature": 0.2 }, "headers": { "X-Team": "a" },
                        "cost": [
                            { "input": 3, "output": 15, "cache": { "read": 0.3, "write": 3.75 } },
                            { "tier": { "type": "context", "size": 200000 }, "input": 6, "output": 22.5,
                              "cache": { "read": 0.6, "write": 7.5 } }
                        ],
                        "variants": [
                            { "id": "high", "body": { "reasoning_effort": "high" } },
                            { "id": "low", "body": { "reasoning_effort": "low" }, "headers": { "X-Team": "b" } }
                        ]
                    } },
                },
                "nourl": { "models": {} }
            }
        }"#;
        let config = Config::parse(text, None).expect("parsed");
        assert_eq!(
            config.providers.len(),
            1,
            "a provider without a URL is left out"
        );
        let fake = config.provider("fake").expect("fake");
        assert_eq!(fake.base_url, "http://127.0.0.1:9/v1");
        assert_eq!(fake.models[0].context, Some(128_000));
        let model = config.default_model().expect("a default");
        assert_eq!(
            (model.provider.as_str(), model.model.as_str()),
            ("fake", "fake-coder")
        );
        let (_, spec) = config.resolve(&model).expect("resolved");
        assert_eq!(config.permissions[0].effect, Effect::Ask);
        let (body, headers) = spec.request_extras(Some("low"));
        assert_eq!(
            Value::Object(body),
            serde_json::json!({ "temperature": 0.2, "reasoning_effort": "low" })
        );
        assert_eq!(headers.last(), Some(&("X-Team".to_owned(), "b".to_owned())));
        assert_eq!(spec.request_extras(None).0.len(), 1, "no variant");

        let step = |input, read| crate::message::Tokens {
            input,
            output: 1_000_000,
            reasoning: 0,
            cache: crate::message::Cache { read, write: 0 },
        };
        // A million output tokens, plus the input at its tier's price.
        assert!(
            (spec.cost(&step(1_000_000, 0)) - 6.0 - 22.5).abs() < 1e-9,
            "large context"
        );
        assert!(
            (spec.cost(&step(100_000, 0)) - 0.3 - 15.0).abs() < 1e-9,
            "base prices"
        );
        assert!(
            (spec.cost(&step(0, 100_000)) - 0.03 - 15.0).abs() < 1e-9,
            "cache reads"
        );
    }

    #[test]
    fn a_string_model_and_a_missing_file() {
        let config = Config::parse(r#"{ "model": "a/b" }"#, None).expect("parsed");
        assert_eq!(config.model.map(|m| m.model), Some("b".to_owned()));
        let empty = tempfile::tempdir().expect("dir");
        assert_eq!(
            Config::load(empty.path()).expect("loaded"),
            Config::default()
        );
    }
}
