//! What a server offers in a folder: models, agents, commands, skills, files, MCP resources.

// Rust guideline compliant 2026-10-09

use serde_json::Value;

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

fn optional(value: &Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}

/// A model a provider offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// Provider ID, such as `anthropic`.
    pub provider_id: String,
    /// Model ID within the provider.
    pub id: String,
    /// Name for people, such as `Claude Opus`.
    pub name: String,
    /// Reasoning levels the model accepts, such as `low` and `high`.
    pub variants: Vec<String>,
    /// Whether the model reads images.
    pub images: bool,
    /// How many tokens its context window holds, if known.
    pub context: Option<u64>,
}

impl Model {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        if value["enabled"] == false {
            return None;
        }
        let id = text(value, "id");
        let provider_id = text(value, "providerID");
        if id.is_empty() || provider_id.is_empty() {
            return None;
        }
        Some(Self {
            name: optional(value, "name").unwrap_or_else(|| id.clone()),
            variants: value["variants"]
                .as_array()
                .map(|v| {
                    v.iter()
                        .filter_map(|x| x["id"].as_str())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            images: value["capabilities"]["input"]
                .as_array()
                .is_some_and(|input| input.iter().any(|i| i == "image")),
            context: value["limit"]["context"].as_u64().filter(|n| *n > 0),
            provider_id,
            id,
        })
    }
}

/// An agent, such as `build` or `plan`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    /// Agent ID.
    pub id: String,
    /// Name for people.
    pub name: String,
    /// What it's for.
    pub description: Option<String>,
    /// Whether a session can run it; otherwise it's only called by other agents.
    pub primary: bool,
}

impl Agent {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        if value["hidden"] == true {
            return None;
        }
        let id = text(value, "id");
        if id.is_empty() {
            return None;
        }
        Some(Self {
            name: optional(value, "name").unwrap_or_else(|| id.clone()),
            description: optional(value, "description"),
            primary: value["mode"] != "subagent",
            id,
        })
    }
}

/// A slash command, such as `review`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The name typed after `/`.
    pub name: String,
    /// What it does.
    pub description: Option<String>,
}

impl Command {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let name = text(value, "name");
        (!name.is_empty()).then(|| Self {
            description: optional(value, "description"),
            name,
        })
    }
}

/// A skill the agent can load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// Skill ID.
    pub id: String,
    /// Name for people.
    pub name: String,
    /// When to use it.
    pub description: Option<String>,
}

impl Skill {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let id = text(value, "id");
        (!id.is_empty()).then(|| Self {
            name: optional(value, "name").unwrap_or_else(|| id.clone()),
            description: optional(value, "description"),
            id,
        })
    }
}

/// A resource an MCP server offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResource {
    /// The MCP server's name.
    pub server: String,
    /// The resource's name.
    pub name: String,
    /// Its URI, which a prompt attaches.
    pub uri: String,
    /// What it holds.
    pub description: Option<String>,
}

impl McpResource {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let uri = text(value, "uri");
        (!uri.is_empty()).then(|| Self {
            server: text(value, "server"),
            name: optional(value, "name").unwrap_or_else(|| uri.clone()),
            description: optional(value, "description"),
            uri,
        })
    }
}

/// A file or folder found by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatch {
    /// Path relative to the searched folder.
    pub path: String,
    /// Whether it's a folder.
    pub directory: bool,
}

impl FileMatch {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let path = text(value, "path");
        (!path.is_empty() && path != "/").then(|| Self {
            directory: value["type"] == "directory",
            path,
        })
    }
}
