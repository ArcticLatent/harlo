//! The model catalog: the providers models.dev lists (MIT), their models,
//! limits and prices, and which API each speaks.
//!
//! A snapshot is bundled ([`Catalog::bundled`]); the server keeps a fresher
//! copy when it can fetch one. A provider becomes usable once it has a key
//! ([`Entry::provider`]).

// Rust guideline compliant 2026-10-09

use serde_json::{Map, Value};

use crate::config::{ModelSpec, Price, ProviderSpec};

/// The catalog bundled with the engine, trimmed by
/// `tools/models-dev-snapshot.py`.
const BUNDLED: &str = include_str!("../catalog/models.json");

/// Where models.dev serves the whole catalog.
pub const SOURCE: &str = "https://models.dev/api.json";

/// The API a provider speaks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Wire {
    /// OpenAI-compatible chat completions.
    #[default]
    Chat,
    /// Anthropic's Messages API.
    Messages,
    /// `OpenAI`'s Responses API.
    Responses,
    /// One the engine can't speak yet, by its package name.
    Unsupported(String),
}

impl Wire {
    /// The wire a models.dev or config package name stands for.
    #[must_use]
    pub fn of_package(package: &str) -> Self {
        match package {
            "" | "@ai-sdk/openai-compatible" => Self::Chat,
            p if p.ends_with("/anthropic") && !p.contains("vertex") => Self::Messages,
            "@ai-sdk/openai" | "@ai-sdk/azure" => Self::Responses,
            // These packages wrap an OpenAI-compatible endpoint.
            "@openrouter/ai-sdk-provider"
            | "@ai-sdk/groq"
            | "@ai-sdk/deepinfra"
            | "@ai-sdk/togetherai"
            | "@ai-sdk/cerebras"
            | "@ai-sdk/mistral"
            | "@ai-sdk/perplexity"
            | "@ai-sdk/xai"
            | "@ai-sdk/google"
            | "@ai-sdk/fireworks"
            | "@ai-sdk/deepseek"
            | "@aihubmix/ai-sdk-provider"
            | "venice-ai-sdk-provider"
            | "@ai-sdk/gateway"
            | "@ai-sdk/cohere" => Self::Chat,
            other => Self::Unsupported(other.to_owned()),
        }
    }
}

/// Base URLs for providers whose catalog entry names a package rather than
/// an endpoint.
const KNOWN_BASES: &[(&str, &str)] = &[
    ("anthropic", "https://api.anthropic.com/v1"),
    ("openai", "https://api.openai.com/v1"),
    ("openrouter", "https://openrouter.ai/api/v1"),
    ("groq", "https://api.groq.com/openai/v1"),
    ("deepinfra", "https://api.deepinfra.com/v1/openai"),
    ("togetherai", "https://api.together.xyz/v1"),
    ("cerebras", "https://api.cerebras.ai/v1"),
    ("mistral", "https://api.mistral.ai/v1"),
    ("perplexity", "https://api.perplexity.ai"),
    ("xai", "https://api.x.ai/v1"),
    (
        "google",
        "https://generativelanguage.googleapis.com/v1beta/openai",
    ),
    ("vercel", "https://ai-gateway.vercel.sh/v1"),
    ("cohere", "https://api.cohere.ai/compatibility/v1"),
    ("fireworks-ai", "https://api.fireworks.ai/inference/v1"),
    ("deepseek", "https://api.deepseek.com/v1"),
    ("aihubmix", "https://aihubmix.com/v1"),
    ("venice", "https://api.venice.ai/api/v1"),
    ("azure", "https://{resourceName}.openai.azure.com/openai/v1"),
    (
        "cloudflare-workers-ai",
        "https://api.cloudflare.com/client/v4/accounts/{accountId}/ai/v1",
    ),
    (
        "cloudflare-ai-gateway",
        "https://gateway.ai.cloudflare.com/v1/{accountId}/{gatewayId}/compat",
    ),
];

/// A question a provider's sign-in asks besides the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The answer's key, as the base URL uses it.
    pub key: &'static str,
    /// What's asked.
    pub title: &'static str,
    /// An example answer.
    pub placeholder: &'static str,
}

/// What some providers' sign-ins ask, by provider.
const QUESTIONS: &[(&str, &str, Question)] = &[
    (
        "azure",
        "API key",
        Question {
            key: "resourceName",
            title: "Enter Azure Resource Name",
            placeholder: "e.g. my-models",
        },
    ),
    (
        "cloudflare-workers-ai",
        "API key",
        Question {
            key: "accountId",
            title: "Enter your Cloudflare Account ID",
            placeholder: "e.g. 1234567890abcdef1234567890abcdef",
        },
    ),
    (
        "cloudflare-ai-gateway",
        "Gateway API token",
        Question {
            key: "accountId",
            title: "Enter your Cloudflare Account ID",
            placeholder: "e.g. 1234567890abcdef1234567890abcdef",
        },
    ),
    (
        "cloudflare-ai-gateway",
        "Gateway API token",
        Question {
            key: "gatewayId",
            title: "Enter your Cloudflare AI Gateway ID",
            placeholder: "e.g. my-gateway",
        },
    ),
];

/// One provider in the catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Its ID.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The environment variables that hold its key.
    pub env: Vec<String>,
    /// The API it speaks.
    pub wire: Wire,
    /// Its base URL; `{key}` parts come from its sign-in's answers.
    pub base_url: Option<String>,
    /// Its models.
    pub models: Vec<ModelSpec>,
}

impl Entry {
    /// What its key sign-in asks besides the key, and the method's label.
    #[must_use]
    pub fn questions(&self) -> (Option<&'static str>, Vec<&'static Question>) {
        let mine: Vec<_> = QUESTIONS.iter().filter(|(id, ..)| *id == self.id).collect();
        (
            mine.first().map(|(_, label, _)| *label),
            mine.iter().map(|(.., q)| q).collect(),
        )
    }

    /// The provider, signed in with `key` and the sign-in's `answers`; none
    /// when it has no base URL or an answer it needs is missing.
    #[must_use]
    pub fn provider(
        &self,
        key: Option<String>,
        answers: &Map<String, Value>,
    ) -> Option<ProviderSpec> {
        let mut base = self.base_url.clone()?;
        for (name, value) in answers {
            if let Some(value) = value.as_str() {
                base = base.replace(&format!("{{{name}}}"), value);
            }
        }
        if base.contains('{') {
            return None;
        }
        Some(ProviderSpec {
            id: self.id.clone(),
            name: self.name.clone(),
            base_url: base.trim_end_matches('/').to_owned(),
            api_key: key,
            models: self.models.clone(),
            wire: self.wire.clone(),
        })
    }
}

/// The providers and their models.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    /// The providers, by ID.
    pub entries: Vec<Entry>,
}

/// Catalog model `m` of a provider on package `package`, as a
/// [`ModelSpec`].
fn model(id: &str, m: &Value, package: &str) -> ModelSpec {
    let cost = &m["cost"];
    let number = |k: &str| cost[k].as_f64().unwrap_or(0.0);
    ModelSpec {
        id: id.to_owned(),
        name: m["name"].as_str().unwrap_or(id).to_owned(),
        context: m["limit"]["context"].as_u64(),
        output: m["limit"]["output"].as_u64(),
        prices: if cost.is_object() {
            vec![Price {
                from: None,
                input: number("input"),
                output: number("output"),
                cache_read: number("cache_read"),
                cache_write: number("cache_write"),
            }]
        } else {
            Vec::new()
        },
        variants: crate::efforts::catalog(package, m),
        ..ModelSpec::default()
    }
}

impl Catalog {
    /// The catalog bundled with the engine.
    #[must_use]
    pub fn bundled() -> Self {
        Self::parse(BUNDLED).unwrap_or_default()
    }

    /// Reads a catalog in models.dev's shape, whole or trimmed.
    ///
    /// # Errors
    ///
    /// When `text` isn't a JSON object of providers.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let providers = value
            .as_object()
            .ok_or_else(|| "The catalog isn't a JSON object.".to_owned())?;
        let mut entries: Vec<Entry> = providers
            .iter()
            .map(|(id, p)| {
                let mut models: Vec<ModelSpec> = p["models"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(mid, m)| model(mid, m, p["npm"].as_str().unwrap_or_default()))
                    .collect();
                models.sort_by(|a, b| a.id.cmp(&b.id));
                Entry {
                    id: id.clone(),
                    name: p["name"].as_str().unwrap_or(id).to_owned(),
                    env: p["env"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect(),
                    wire: Wire::of_package(p["npm"].as_str().unwrap_or_default()),
                    base_url: p["api"].as_str().map(str::to_owned).or_else(|| {
                        KNOWN_BASES
                            .iter()
                            .find(|(known, _)| known == id)
                            .map(|(_, base)| (*base).to_owned())
                    }),
                    models,
                }
            })
            .collect();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        if entries.is_empty() {
            return Err("The catalog lists no providers.".to_owned());
        }
        Ok(Self { entries })
    }

    /// Provider `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_catalog_reads_with_wires_and_bases() {
        let catalog = Catalog::bundled();
        assert!(catalog.entries.len() > 100);
        let anthropic = catalog.get("anthropic").expect("Anthropic");
        assert_eq!(anthropic.wire, Wire::Messages);
        assert_eq!(anthropic.env, ["ANTHROPIC_API_KEY"]);
        assert!(!anthropic.models.is_empty());
        assert_eq!(catalog.get("openai").expect("OpenAI").wire, Wire::Responses);
        let ai302 = catalog.get("302ai").expect("302.AI");
        assert_eq!(ai302.wire, Wire::Chat);
        let spec = ai302
            .provider(Some("k".into()), &Map::new())
            .expect("signed in");
        assert_eq!(spec.base_url, "https://api.302.ai/v1");
        for (id, base) in [
            ("vercel", "https://ai-gateway.vercel.sh/v1"),
            ("cohere", "https://api.cohere.ai/compatibility/v1"),
        ] {
            let entry = catalog.get(id).expect("listed");
            assert_eq!(entry.wire, Wire::Chat, "{id}");
            let spec = entry
                .provider(Some("k".into()), &Map::new())
                .expect("signed in");
            assert_eq!(spec.base_url, base);
        }

        let azure = catalog.get("azure").expect("Azure");
        assert_eq!(azure.questions().1[0].key, "resourceName");
        assert!(azure.provider(Some("k".into()), &Map::new()).is_none());
        let mut answers = Map::new();
        answers.insert("resourceName".into(), "mine".into());
        let spec = azure
            .provider(Some("k".into()), &answers)
            .expect("answered");
        assert_eq!(spec.base_url, "https://mine.openai.azure.com/openai/v1");
    }
}
