//! What the server is set up with: providers, web search, MCP servers,
//! plugins and integrations (the services it signs in to); and the usage
//! statistics across sessions.
//!
//! Only names and states are read. A provider's settings and a credential's
//! value, which hold keys and tokens, are never decoded.

// Rust guideline compliant 2026-10-09

use serde_json::Value;

use crate::Tokens;
use crate::types::count;

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

fn optional(value: &Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn whole(value: &Value) -> u64 {
    count(value.as_f64().unwrap_or_default())
}

/// A model provider the server knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// Its ID, as models name it.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Whether it's turned on.
    pub enabled: bool,
}

impl Provider {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let id = text(value, "id");
        (!id.is_empty()).then(|| Self {
            name: optional(value, "name").unwrap_or_else(|| id.clone()),
            enabled: value["activation"].as_str() != Some("disabled"),
            id,
        })
    }
}

/// A web search service the server can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSearch {
    /// Its ID.
    pub id: String,
    /// Its name.
    pub name: String,
}

impl WebSearch {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let id = text(value, "id");
        (!id.is_empty()).then(|| Self {
            name: optional(value, "name").unwrap_or_else(|| id.clone()),
            id,
        })
    }
}

/// How an MCP server stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpStatus {
    /// Connected and serving.
    Connected,
    /// Connecting.
    Pending,
    /// Turned off.
    Disabled,
    /// Failed, for this reason.
    Failed(String),
    /// Waiting for the user to sign in, for this reason.
    NeedsAuth(String),
    /// A state this client doesn't know, by name.
    Other(String),
}

/// An MCP server the server connects to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    /// Its name in the configuration.
    pub name: String,
    /// The title it gives itself, if any.
    pub title: Option<String>,
    /// How it stands.
    pub status: McpStatus,
}

impl McpServer {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let name = text(value, "name");
        let status = &value["status"];
        let error = text(status, "error");
        let status = match status["status"].as_str().unwrap_or_default() {
            "connected" => McpStatus::Connected,
            "pending" => McpStatus::Pending,
            "disabled" => McpStatus::Disabled,
            "failed" => McpStatus::Failed(error),
            "needs_auth" => McpStatus::NeedsAuth(error),
            other => McpStatus::Other(other.to_owned()),
        };
        (!name.is_empty()).then(|| Self {
            title: optional(value, "title"),
            name,
            status,
        })
    }
}

/// Where a plugin comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    /// Part of OpenCode.
    Builtin,
    /// An installed package.
    Package {
        /// The package, as configured.
        target: String,
        /// The installed version, if known.
        version: Option<String>,
        /// Whether a newer version is out.
        outdated: bool,
    },
    /// A file on the server's computer.
    Local(String),
    /// Added by a program using OpenCode's SDK.
    Sdk,
    /// A source this client doesn't know, by name.
    Other(String),
}

/// A plugin the server loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    /// Its ID.
    pub id: String,
    /// Where it comes from.
    pub source: PluginSource,
    /// Why it failed to load, if it did.
    pub failed: Option<String>,
}

impl Plugin {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let id = text(value, "id");
        let source = &value["source"];
        let source = match source["type"].as_str().unwrap_or_default() {
            "builtin" => PluginSource::Builtin,
            "package" => PluginSource::Package {
                target: text(source, "target"),
                version: optional(source, "version"),
                outdated: source["outdated"].as_bool().unwrap_or(false),
            },
            "local" => PluginSource::Local(text(source, "path")),
            "sdk" => PluginSource::Sdk,
            other => PluginSource::Other(other.to_owned()),
        };
        let state = &value["state"];
        let failed = (state["status"].as_str() == Some("failed"))
            .then(|| optional(state, "error").unwrap_or_else(|| "failed".to_owned()));
        (!id.is_empty()).then_some(Self { id, source, failed })
    }
}

/// Steps on one day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    /// The date, as `YYYY-MM-DD`.
    pub date: String,
    /// The model steps taken that day.
    pub steps: u64,
}

/// One model's share of the usage.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelUsage {
    /// The provider's ID.
    pub provider: String,
    /// The model's ID.
    pub model: String,
    /// The steps it took.
    pub steps: u64,
    /// The tokens it read and wrote.
    pub tokens: Tokens,
    /// What it cost, in US dollars.
    pub cost: f64,
}

/// Usage across every session in a time range.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stats {
    /// The range's start, in milliseconds since the Unix epoch.
    pub from: u64,
    /// Its end.
    pub to: u64,
    /// Sessions started, subagents' left out.
    pub sessions: u64,
    /// Subagent sessions.
    pub subagents: u64,
    /// Prompts sent.
    pub prompts: u64,
    /// Model steps taken.
    pub steps: u64,
    /// Tokens read and written.
    pub tokens: Tokens,
    /// What it cost, in US dollars.
    pub cost: f64,
    /// Days with any step.
    pub active_days: u64,
    /// Days in a row up to the range's end with any step.
    pub streak: u64,
    /// Steps per day, for days with any.
    pub activity: Vec<Day>,
    /// Usage per model, busiest first.
    pub models: Vec<ModelUsage>,
}

/// Tokens as the stats count them.
fn tokens(value: &Value) -> Tokens {
    Tokens {
        input: whole(&value["input"]),
        output: whole(&value["output"]),
        reasoning: whole(&value["reasoning"]),
        cache_read: whole(&value["cache"]["read"]),
        cache_write: whole(&value["cache"]["write"]),
    }
}

impl Stats {
    pub(crate) fn from_json(value: &Value) -> Self {
        let mut models: Vec<ModelUsage> = value["models"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|m| ModelUsage {
                        provider: text(&m["model"], "providerID"),
                        model: text(&m["model"], "id"),
                        steps: whole(&m["steps"]),
                        tokens: tokens(&m["tokens"]),
                        cost: m["cost"].as_f64().unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        models.sort_by_key(|m| std::cmp::Reverse(m.steps));
        Self {
            from: whole(&value["range"]["from"]),
            to: whole(&value["range"]["to"]),
            sessions: whole(&value["sessions"]),
            subagents: whole(&value["subagents"]),
            prompts: whole(&value["prompts"]),
            steps: whole(&value["steps"]),
            tokens: tokens(&value["tokens"]),
            cost: value["cost"].as_f64().unwrap_or_default(),
            active_days: whole(&value["activeDays"]),
            streak: whole(&value["streak"]),
            activity: value["activity"]
                .as_array()
                .map(|days| {
                    days.iter()
                        .map(|d| Day {
                            date: text(d, "date"),
                            steps: whole(&d["steps"]),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            models,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_read_from_the_servers_answer() {
        let answer = serde_json::json!({
            "range": { "from": 1_795_338_328_527_u64, "to": 1_795_340_639_401_u64 },
            "sessions": 3, "subagents": 1, "prompts": 5, "steps": 9,
            "tokens": { "input": 10, "output": 5, "reasoning": 0,
                        "cache": { "read": 100, "write": 2 } },
            "cost": 0.25,
            "activeDays": 2, "streak": 1,
            "activity": [{ "date": "2026-11-21", "steps": 4 }, { "date": "2026-11-22", "steps": 5 }],
            "models": [
                { "model": { "id": "small", "providerID": "fake" }, "steps": 2,
                  "tokens": { "input": 1, "output": 1, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
                  "cost": 0.05 },
                { "model": { "id": "big", "providerID": "fake" }, "steps": 7,
                  "tokens": { "input": 9, "output": 4, "reasoning": 0, "cache": { "read": 100, "write": 2 } },
                  "cost": 0.2 },
            ],
        });
        let stats = Stats::from_json(&answer);
        assert_eq!(stats.sessions, 3);
        assert_eq!(stats.tokens.cache_read, 100);
        assert_eq!(stats.activity.len(), 2);
        assert_eq!(stats.models[0].model, "big", "busiest first");
        assert!((stats.cost - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn a_provider_reads_without_its_settings() {
        let answer = serde_json::json!({
            "id": "fake", "name": "Fake", "activation": "enabled",
            "settings": { "apiKey": "secret" },
        });
        let provider = Provider::from_json(&answer).expect("a provider");
        assert!(!format!("{provider:?}").contains("secret"));
    }
}

/// A service the server can sign in to, such as a model provider, and the
/// connections it has to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Integration {
    /// Its ID, such as `claude-agent`.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its connections: stored credentials and environment variables.
    pub connections: Vec<Connection>,
}

/// One way the server is connected to an integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    /// The stored credential's ID; `None` for an environment variable.
    pub credential: Option<String>,
    /// The credential's label, or the variable's name.
    pub label: String,
    /// Why it needs signing in again, when it does.
    pub needs_sign_in: Option<String>,
}

impl Integration {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let id = value["id"].as_str().filter(|s| !s.is_empty())?.to_owned();
        let connections = value["connections"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Connection::from_json)
            .collect();
        Some(Self {
            name: optional(value, "name").unwrap_or_else(|| id.clone()),
            id,
            connections,
        })
    }
}

impl Connection {
    fn from_json(value: &Value) -> Option<Self> {
        let (credential, label) = match value["type"].as_str()? {
            "credential" => (Some(text(value, "id")), text(value, "label")),
            "env" => (None, text(value, "name")),
            _ => return None,
        };
        let status = &value["status"];
        let needs_sign_in =
            (status["status"].as_str() == Some("needs_auth")).then(|| text(status, "message"));
        Some(Self {
            credential,
            label,
            needs_sign_in,
        })
    }
}
