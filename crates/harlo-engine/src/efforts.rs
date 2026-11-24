//! Effort levels: the variants a reasoning model offers when its config
//! names none.
//!
//! A catalog model's levels come from its `reasoning_options`: a list of
//! effort values, a token budget (offered as `high` and `max`), and a
//! toggle that adds `none` where the provider's API can turn reasoning
//! off. A model the config adds on an OpenAI-compatible API, about which
//! nothing is known, offers `low`, `medium` and `high`. Each level's
//! request fields are written in the provider's API's own terms.

// Rust guideline compliant 2026-10-09

use serde_json::{Map, Value, json};

use crate::VariantSpec;

/// Anthropic's thinking budgets for `high` and `max`.
const ANTHROPIC_BUDGETS: (u64, u64) = (16_000, 31_999);

/// How a provider's API asks for reasoning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    /// Anthropic's Messages: adaptive thinking with an effort, or a budget.
    Anthropic,
    /// `OpenAI`'s Responses: `reasoning.effort`.
    Responses,
    /// `OpenRouter`: `reasoning` with an effort, a budget, or on and off.
    OpenRouter,
    /// `DeepSeek`'s own API: `reasoning_effort`, off when not asked for.
    DeepSeek,
    /// A plain OpenAI-compatible API: `reasoning_effort`, which can't
    /// turn reasoning off unless `none` is one of its values.
    Compatible,
    /// Another package over an OpenAI-compatible API: `reasoning_effort`.
    Chat,
}

impl Dialect {
    /// The dialect of models.dev or config package `package`.
    fn of(package: &str) -> Self {
        match package {
            "" | "@ai-sdk/openai-compatible" | "@opencode/ai/providers/openai-compatible" => {
                Self::Compatible
            }
            p if p.ends_with("/anthropic") => Self::Anthropic,
            "@ai-sdk/openai" | "@ai-sdk/azure" => Self::Responses,
            p if p.contains("openrouter") => Self::OpenRouter,
            p if p.ends_with("/deepseek") => Self::DeepSeek,
            _ => Self::Chat,
        }
    }
}

/// One level, before it's written in a dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Level {
    /// Reasoning off.
    Off,
    /// Reasoning on, with no level.
    On,
    /// An effort, such as `high`.
    Effort(String),
    /// A token budget.
    Budget(u64),
}

/// `level`'s request fields in `dialect`.
fn body(dialect: Dialect, level: &Level) -> Map<String, Value> {
    let adaptive = json!({ "type": "adaptive", "display": "summarized" });
    let fields = match (dialect, level) {
        (Dialect::Anthropic, Level::Off) => json!({ "thinking": { "type": "disabled" } }),
        (Dialect::Anthropic, Level::On) => json!({ "thinking": adaptive }),
        (Dialect::Anthropic, Level::Effort(e)) => {
            json!({ "thinking": adaptive, "output_config": { "effort": e } })
        }
        (Dialect::Anthropic, Level::Budget(n)) => {
            json!({ "thinking": { "type": "enabled", "budget_tokens": n } })
        }
        (Dialect::Responses, Level::Effort(e)) => {
            json!({ "reasoning": { "effort": e, "summary": "auto" } })
        }
        (Dialect::OpenRouter, Level::Off) => json!({ "reasoning": { "enabled": false } }),
        (Dialect::OpenRouter, Level::On) => json!({ "reasoning": { "enabled": true } }),
        (Dialect::OpenRouter, Level::Effort(e)) => json!({ "reasoning": { "effort": e } }),
        (Dialect::OpenRouter, Level::Budget(n)) => json!({ "reasoning": { "max_tokens": n } }),
        (Dialect::DeepSeek | Dialect::Compatible | Dialect::Chat, Level::Effort(e)) => {
            json!({ "reasoning_effort": e })
        }
        // Nothing to send: the API's default is the level.
        _ => json!({}),
    };
    match fields {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// The variant `id` for `level` in `dialect`.
fn variant(dialect: Dialect, id: &str, level: &Level) -> VariantSpec {
    VariantSpec {
        id: id.to_owned(),
        body: body(dialect, level),
        headers: Vec::new(),
    }
}

/// The levels a model the config adds offers on package `package`, when
/// its entry names none.
#[must_use]
pub fn configured(package: &str) -> Vec<VariantSpec> {
    let dialect = Dialect::of(package);
    if dialect != Dialect::Compatible {
        return Vec::new();
    }
    ["low", "medium", "high"]
        .iter()
        .map(|e| variant(dialect, e, &Level::Effort((*e).to_owned())))
        .collect()
}

/// The levels catalog model `m` offers on package `package`.
#[must_use]
pub fn catalog(package: &str, m: &Value) -> Vec<VariantSpec> {
    if m["reasoning"].as_bool() != Some(true) {
        return Vec::new();
    }
    let dialect = Dialect::of(package);
    let Some(options) = m["reasoning_options"].as_array() else {
        return configured(package);
    };
    let find = |kind: &str| options.iter().find(|o| o["type"] == kind);
    let efforts: Vec<String> = find("effort")
        .and_then(|o| o["values"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|v| *v != "default")
        .map(str::to_owned)
        .collect();
    let mut out = Vec::new();
    let can_turn_off = dialect != Dialect::Compatible;
    if find("toggle").is_some() && can_turn_off && !efforts.iter().any(|e| e == "none") {
        out.push(variant(dialect, "none", &Level::Off));
    }
    if !efforts.is_empty() {
        out.extend(
            efforts
                .iter()
                .map(|e| variant(dialect, e, &Level::Effort(e.clone()))),
        );
    } else if let Some(budget) = find("budget_tokens") {
        let (high, max) = budgets(dialect, budget, m["limit"]["output"].as_u64());
        out.push(variant(dialect, "high", &Level::Budget(high)));
        out.push(variant(dialect, "max", &Level::Budget(max)));
    } else if find("toggle").is_some() {
        out.push(variant(dialect, "thinking", &Level::On));
    }
    out
}

/// The budgets of `high` and `max` for budget option `budget`, on a model
/// that writes at most `output` tokens.
fn budgets(dialect: Dialect, budget: &Value, output: Option<u64>) -> (u64, u64) {
    let below_output = |n: u64| output.map_or(n, |o| n.min(o.saturating_sub(1)));
    if dialect == Dialect::Anthropic {
        return (
            below_output(ANTHROPIC_BUDGETS.0),
            below_output(ANTHROPIC_BUDGETS.1),
        );
    }
    match (budget["max"].as_u64(), output) {
        (Some(max), _) => (max / 2, max),
        (None, Some(output)) => (output / 2, output.saturating_sub(1)),
        (None, None) => ANTHROPIC_BUDGETS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(variants: &[VariantSpec]) -> Vec<&str> {
        variants.iter().map(|v| v.id.as_str()).collect()
    }

    #[test]
    fn efforts_come_from_the_catalog_in_each_apis_terms() {
        let gpt = json!({ "reasoning": true, "reasoning_options": [
            { "type": "effort", "values": ["none", "low", "high"] }] });
        let v = catalog("@ai-sdk/openai", &gpt);
        assert_eq!(ids(&v), ["none", "low", "high"]);
        assert_eq!(
            Value::Object(v[1].body.clone()),
            json!({ "reasoning": { "effort": "low", "summary": "auto" } })
        );

        let sonnet = json!({ "reasoning": true, "reasoning_options": [
            { "type": "toggle" }, { "type": "effort", "values": ["low", "max"] },
            { "type": "budget_tokens", "min": 1024 }] });
        let v = catalog("@ai-sdk/anthropic", &sonnet);
        assert_eq!(ids(&v), ["none", "low", "max"]);
        assert_eq!(
            Value::Object(v[0].body.clone()),
            json!({ "thinking": { "type": "disabled" } })
        );
        assert_eq!(v[2].body["output_config"], json!({ "effort": "max" }));
        assert_eq!(v[2].body["thinking"]["type"], "adaptive");
    }

    #[test]
    fn budgets_and_toggles() {
        let haiku = json!({ "reasoning": true, "limit": { "output": 64_000 },
            "reasoning_options": [{ "type": "toggle" }, { "type": "budget_tokens" }] });
        let v = catalog("@ai-sdk/anthropic", &haiku);
        assert_eq!(ids(&v), ["none", "high", "max"]);
        assert_eq!(v[2].body["thinking"]["budget_tokens"], 31_999);

        let qwen = json!({ "reasoning": true, "limit": { "output": 65_536 },
            "reasoning_options": [{ "type": "toggle" }, { "type": "budget_tokens" }] });
        let v = catalog("@openrouter/ai-sdk-provider", &qwen);
        assert_eq!(v[1].body["reasoning"]["max_tokens"], 32_768);
        assert_eq!(v[2].body["reasoning"]["max_tokens"], 65_535);

        let hy = json!({ "reasoning": true, "reasoning_options": [{ "type": "toggle" }] });
        let v = catalog("@openrouter/ai-sdk-provider", &hy);
        assert_eq!(ids(&v), ["none", "thinking"]);

        // A plain OpenAI-compatible API can't turn reasoning off.
        let flash = json!({ "reasoning": true, "reasoning_options": [
            { "type": "toggle" }, { "type": "effort", "values": ["low", "default", "max"] }] });
        assert_eq!(
            ids(&catalog("@ai-sdk/openai-compatible", &flash)),
            ["low", "max"]
        );
    }

    #[test]
    fn models_without_reasoning_offer_none_and_configured_ones_three() {
        assert!(catalog("@ai-sdk/openai", &json!({ "reasoning": false })).is_empty());
        let v = configured("@opencode/ai/providers/openai-compatible");
        assert_eq!(ids(&v), ["low", "medium", "high"]);
        assert_eq!(v[2].body["reasoning_effort"], "high");
        assert!(configured("@ai-sdk/anthropic").is_empty());
    }
}
