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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
}

impl ModelSpec {
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
            instructions: value["instructions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
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

/// A JSON object of header names and values.
fn headers_of(v: &Value) -> Vec<(String, String)> {
    v.as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
        .collect()
}

/// `"provider/model"` or `{ "providerID", "model" }`.
fn model_ref(v: &Value) -> Option<ModelRef> {
    if let Some(s) = v.as_str() {
        let (provider, model) = s.split_once('/')?;
        return Some(ModelRef {
            provider: provider.to_owned(),
            model: model.to_owned(),
            variant: None,
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
