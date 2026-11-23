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
    /// Whether other agents can hand it work in a child session.
    pub subagent: bool,
    /// Its colour as `#rrggbb`, when one is configured.
    pub color: Option<String>,
    /// Whether its rules stop it from editing files, as for planning.
    pub read_only: bool,
    /// The model it always uses, as provider ID and model ID, when it's
    /// set to one; otherwise it uses the session's.
    pub model: Option<(String, String)>,
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
        let mode = value["mode"].as_str().unwrap_or("primary");
        Some(Self {
            name: optional(value, "name").unwrap_or_else(|| id.clone()),
            description: optional(value, "description"),
            primary: mode != "subagent",
            subagent: mode != "primary",
            color: optional(value, "color").filter(|c| is_hex_colour(c)),
            read_only: edits_denied(&value["permissions"]),
            model: value["model"]["providerID"]
                .as_str()
                .zip(value["model"]["id"].as_str())
                .map(|(provider, id)| (provider.to_owned(), id.to_owned())),
            id,
        })
    }
}

/// Whether `colour` is written `#rrggbb`.
fn is_hex_colour(colour: &str) -> bool {
    colour.len() == 7
        && colour.starts_with('#')
        && colour[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether an agent's permission rules deny editing files everywhere.
///
/// The last rule for editing that covers every file decides, as in OpenCode,
/// where the last matching rule wins. Rules for some files only (such as the
/// plan agent's own plan files) are exceptions, not its general stance. An
/// action matches when it's `edit` or `*`.
fn edits_denied(rules: &Value) -> bool {
    rules
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .find(|rule| {
            matches!(rule["action"].as_str(), Some("edit" | "*"))
                && matches!(rule["resource"].as_str(), Some("*" | "**"))
        })
        .is_some_and(|rule| rule["effect"] == "deny")
}

/// A slash command, such as `review`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The name typed after `/`.
    pub name: String,
    /// What it does.
    pub description: Option<String>,
    /// The agent that runs it, when it names one.
    ///
    /// The command list doesn't say; [`Client::setup`](crate::Client::setup) does.
    pub agent: Option<String>,
    /// Whether it runs as a subagent, in a child session of its own.
    pub subagent: bool,
}

impl Command {
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let name = text(value, "name");
        (!name.is_empty()).then(|| Self {
            description: optional(value, "description"),
            name,
            agent: None,
            subagent: false,
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
