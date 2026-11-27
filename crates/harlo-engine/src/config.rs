//! The configuration the engine reads in place: Harlo's config file.
//!
//! `harlo.jsonc` (or `harlo.json`) in Harlo's config folder names the
//! providers, their models, the default model and permission rules; a
//! project adds its own (see [`project_config_files`]). The engine reads
//! what it supports and leaves the file as it is.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Error;
use crate::rules::Rule;

/// The name of Harlo's config folder in `$XDG_CONFIG_HOME`.
pub const CONFIG_FOLDER: &str = "harlo";

/// The config file's names, the one with comments first: in the config
/// folder, and in a project's folders.
pub const CONFIG_FILES: [&str; 2] = ["harlo.jsonc", "harlo.json"];

/// The folder in a project that holds its own config, skills, commands and
/// agents.
pub const PROJECT_FOLDER: &str = ".harlo";

/// Returns Harlo's config folder under `config_home` (usually
/// `$XDG_CONFIG_HOME`).
#[must_use]
pub fn config_folder(config_home: &Path) -> PathBuf {
    config_home.join(CONFIG_FOLDER)
}

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

impl ModelRef {
    /// The model a config or request value names: `"provider/model#variant"`
    /// (the variant optional) or `{ "providerID", "model" or "id", "variant" }`.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Self> {
        model_ref(value)
    }
}

/// A model's prices, in US dollars per million tokens.
/// When a session's old tool results are cleared for the model.
///
/// Only ever as a session is taken up again after a rest: in the middle
/// of a turn the provider reads the conversation from its cache, and
/// clearing would have it charged as new.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Prune {
    /// Once the provider's cache has let go of the conversation.
    #[default]
    Cold,
    /// After this many milliseconds of rest.
    After(i64),
    /// Never.
    Never,
}

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
            + dollars(tokens.cache.write, self.written())
    }
}

impl Price {
    /// The price of input written to the cache: the input price when the
    /// model lists none, since a write is never free where it's counted.
    fn written(&self) -> f64 {
        if self.cache_write > 0.0 {
            self.cache_write
        } else {
            self.input
        }
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
    /// The API it speaks.
    pub wire: crate::catalog::Wire,
    /// Whether the system prompt opens the conversation, where the
    /// provider's cache keeps it, on models that would otherwise charge
    /// it whole with every request (`experimental.cachedInstructions`).
    pub system_first: bool,
}

/// What the configuration says of the sandbox agents' commands run in
/// (`sandbox`): `false` turns it off; an object can turn it off
/// (`enabled`) and name more folders (`writable`, `hidden`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandboxConfig {
    /// Whether commands are confined (on unless turned off).
    pub enabled: bool,
    /// More folders commands may change, as written (`~/` for the home
    /// folder).
    pub writable: Vec<String>,
    /// More places commands can't read, as written.
    pub hidden: Vec<String>,
}

/// What the engine read from the configuration.
#[derive(Debug, Clone, Default, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a switch of its own in the configuration file"
)]
pub struct Config {
    /// The file it came from, if one was found.
    pub source: Option<PathBuf>,
    /// The providers with an OpenAI-compatible API.
    pub providers: Vec<ProviderSpec>,
    /// The model new sessions use, when the file names one.
    pub model: Option<ModelRef>,
    /// A cheaper model for small jobs such as naming a session
    /// (`small_model`), when the file names one.
    pub small_model: Option<ModelRef>,
    /// Whether the system prompt opens the conversation on models that
    /// charge a request's own instructions whole every time
    /// (`experimental.cachedInstructions`, off unless turned on).
    pub cached_instructions: bool,
    /// Whether scripts may call the session's own tools: read, search,
    /// edit, write and run commands, each under the tool's own rules
    /// (`experimental.scriptTools`, off unless turned on: measured on
    /// small jobs it cost 12% more prompt tokens with a shell and 35%
    /// more without one, and with a shell no script used it).
    pub script_tools: bool,
    /// Whether `read` can list what a source file declares instead of its
    /// text (`experimental.outline`, on unless turned off: over four jobs
    /// in long files it halved the tokens sent, with the same work done).
    pub outline: bool,
    /// Permission rules from the file, applied after the agent's own.
    pub permissions: Vec<Rule>,
    /// Extra instruction files (`instructions`): paths, `*` patterns, URLs.
    pub instructions: Vec<String>,
    /// Whether agents keep a todo list (`experimental.todo`, off unless
    /// turned on: over six runs of a prompt asking for three changes it
    /// took four more steps and half more prompt tokens, with the same
    /// work done).
    pub todo: bool,
    /// Extra skill folders (`skills`).
    pub skills: Vec<String>,
    /// Commands (`commands`), by name: `template`, `description`, `agent`
    /// and `model`.
    pub commands: Value,
    /// Agents (`agents`), by ID: new ones, or changes to built-in ones.
    pub agents: Value,
    /// Whether steps are recorded so undo can put files back
    /// (`snapshots`, on unless turned off).
    pub snapshots: bool,
    /// Whether automations may be made (`automations`, on unless turned
    /// off).
    pub automations: bool,
    /// The context size, in tokens, at which a session is summarized
    /// (`compaction.limit`); none leaves it to the engine.
    pub compact_at: Option<u64>,
    /// When a rested session's old tool results are cleared as it's taken
    /// up again (`compaction.prune`: `false` for never, a number for that
    /// many seconds of rest; otherwise once the provider's cache has let
    /// go of the conversation).
    pub prune: Prune,
    /// MCP servers (`mcp.servers`), by name, as the file has them.
    pub mcp: Value,
    /// The sandbox agents' commands run in (`sandbox`).
    pub sandbox: SandboxConfig,
    /// What it says of web search: the service it names
    /// (`websearch.provider`), an ID or `random`, or `false` when it turns
    /// web search off (`"websearch": false`).
    pub websearch: Option<Value>,
}

impl Config {
    /// Reads Harlo's configuration from its folder under `config_home`
    /// (usually `$XDG_CONFIG_HOME`), or an empty one when there is none.
    ///
    /// # Errors
    ///
    /// Fails when the file exists but can't be read or parsed.
    pub fn load(config_home: &Path) -> Result<Self, Error> {
        let folder = config_folder(config_home);
        for name in CONFIG_FILES {
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
        // (No file is an empty one: what is on unless turned off stays on.)
        Ok(Self::from_value(&Value::Null, None))
    }

    /// Reads a configuration from the text of a config file.
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
        Ok(Self::from_value(&value, source))
    }

    /// Reads a configuration from a config document, or several
    /// [`merge_config`]d into one.
    #[must_use]
    pub fn from_value(value: &Value, source: Option<PathBuf>) -> Self {
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
        let cached_instructions = value["experimental"]["cachedInstructions"]
            .as_bool()
            .unwrap_or(false);
        let mut providers: Vec<ProviderSpec> = providers;
        for provider in &mut providers {
            provider.system_first = cached_instructions;
        }
        Self {
            source,
            providers,
            cached_instructions,
            script_tools: value["experimental"]["scriptTools"] == true,
            outline: value["experimental"]["outline"].as_bool().unwrap_or(true),
            model: model_ref(&value["model"]),
            small_model: model_ref(&value["small_model"]),
            permissions,
            instructions: strings(&value["instructions"]),
            skills: strings(&value["skills"]),
            todo: value["experimental"]["todo"].as_bool().unwrap_or(false),
            commands: value["commands"].clone(),
            agents: value["agents"].clone(),
            snapshots: value["snapshots"].as_bool().unwrap_or(true),
            automations: value["automations"]
                .as_bool()
                .or_else(|| value["experimental"]["automations"].as_bool())
                .unwrap_or(true),
            compact_at: value["compaction"]["limit"]
                .as_u64()
                .filter(|limit| *limit > 0),
            prune: match &value["compaction"]["prune"] {
                Value::Bool(false) => Prune::Never,
                seconds => seconds
                    .as_i64()
                    .map_or(Prune::Cold, |s| Prune::After(s.saturating_mul(1_000))),
            },
            mcp: value["mcp"]["servers"].clone(),
            sandbox: SandboxConfig {
                enabled: value["sandbox"]
                    .as_bool()
                    .or_else(|| value["sandbox"]["enabled"].as_bool())
                    .unwrap_or(true),
                writable: strings(&value["sandbox"]["writable"]),
                hidden: strings(&value["sandbox"]["hidden"]),
            },
            websearch: if value["websearch"] == false {
                Some(Value::Bool(false))
            } else {
                Some(value["websearch"]["provider"].clone()).filter(|v| !v.is_null())
            },
        }
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

/// The config files a project adds over the global one, in order.
///
/// In each folder from the project's root down to `workspace` they are its
/// `harlo.jsonc` (or `harlo.json`), then its `.harlo/harlo.jsonc` (or
/// `.json`).
#[must_use]
pub fn project_config_files(workspace: &Path, root: Option<&Path>) -> Vec<PathBuf> {
    let mut folders = vec![workspace.to_path_buf()];
    if let Some(root) = root {
        let mut at = workspace;
        while let Some(up) = at.parent().filter(|up| up.starts_with(root)) {
            folders.push(up.to_path_buf());
            at = up;
        }
    }
    folders.reverse();
    let mut files = Vec::new();
    for folder in folders {
        for dir in [folder.clone(), folder.join(PROJECT_FOLDER)] {
            let file = CONFIG_FILES
                .iter()
                .map(|n| dir.join(n))
                .find(|p| p.is_file());
            files.extend(file);
        }
    }
    files
}

/// Reads the config document in `file`, comments allowed.
///
/// # Errors
///
/// When it can't be read or isn't JSON.
pub fn read_config_document(file: &Path) -> Result<Value, Error> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| Error::config(format!("Couldn't read {}: {e}", file.display())))?;
    jsonc_parser::parse_to_serde_value(&text, &jsonc_parser::ParseOptions::default())
        .map_err(|e| Error::config(format!("Couldn't parse {}: {e}", file.display())))
}

/// The config folder an earlier OpenCode setup kept, in `$XDG_CONFIG_HOME`.
const OPENCODE_FOLDER: &str = "opencode";

/// That setup's config file names, the one with comments first.
const OPENCODE_FILES: [&str; 2] = ["opencode.jsonc", "opencode.json"];

/// What else of that setup's folder is copied: the global instructions and
/// the folders of commands, agents and skills.
const OPENCODE_EXTRAS: [&str; 6] = [
    "AGENTS.md",
    "command",
    "commands",
    "agent",
    "agents",
    "skills",
];

/// The package name that setup wrote for an OpenAI-compatible server, and
/// the one the engine reads.
const OLD_COMPATIBLE: (&str, &str) = (
    "@opencode/ai/providers/openai-compatible",
    "@ai-sdk/openai-compatible",
);

/// Starts Harlo's config from an earlier OpenCode setup, when Harlo has no
/// config file yet.
///
/// Copies `opencode.jsonc` (or `.json`) from OpenCode's config folder under
/// `config_home` as `harlo.jsonc` (or `.json`), with its providers, models,
/// default model and rules, and copies the folder's `AGENTS.md` and its
/// commands, agents and skills beside it. The file's `$schema` line is left
/// out and the old package name for OpenAI-compatible servers is updated;
/// the original folder stays as it is. Returns the file written, or `None`
/// when Harlo already has a config file or there's none to copy.
///
/// # Errors
///
/// When a file can't be read or written.
pub fn copy_from_opencode(config_home: &Path) -> Result<Option<PathBuf>, Error> {
    let folder = config_folder(config_home);
    if CONFIG_FILES.iter().any(|n| folder.join(n).exists()) {
        return Ok(None);
    }
    let old = config_home.join(OPENCODE_FOLDER);
    let Some((index, source)) = OPENCODE_FILES
        .iter()
        .map(|n| old.join(n))
        .enumerate()
        .find(|(_, p)| p.is_file())
    else {
        return Ok(None);
    };
    let failed = |path: &Path, e: std::io::Error| {
        Error::config(format!("Couldn't copy {}: {e}", path.display()))
    };
    let text = std::fs::read_to_string(&source).map_err(|e| failed(&source, e))?;
    let mut copy = String::with_capacity(text.len());
    for line in text.lines() {
        if !(line.trim_start().starts_with("\"$schema\"") && line.contains("opencode.ai")) {
            copy.push_str(&line.replace(OLD_COMPATIBLE.0, OLD_COMPATIBLE.1));
            copy.push('\n');
        }
    }
    std::fs::create_dir_all(&folder).map_err(|e| failed(&folder, e))?;
    let target = folder.join(CONFIG_FILES[index]);
    std::fs::write(&target, copy).map_err(|e| failed(&target, e))?;
    for extra in OPENCODE_EXTRAS {
        let (from, to) = (old.join(extra), folder.join(extra));
        if !to.exists() {
            copy_tree(&from, &to).map_err(|e| failed(&from, e))?;
        }
    }
    Ok(Some(target))
}

/// Copies the file or folder `from` to `to`, folders whole; symbolic links
/// and anything missing are skipped.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let Ok(kind) = std::fs::symlink_metadata(from).map(|m| m.file_type()) else {
        return Ok(());
    };
    if kind.is_file() {
        std::fs::copy(from, to).map(drop)
    } else if kind.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        Ok(())
    }
}

/// Lays config document `over` on `base`: objects merge key by key, the
/// lists of rules, instructions and skills add up, and anything else in
/// `over` replaces what `base` had.
pub fn merge_config(base: &mut Value, over: &Value) {
    merge_at(base, over, "");
}

fn merge_at(base: &mut Value, over: &Value, key: &str) {
    match (base, over) {
        (Value::Object(base), Value::Object(over)) => {
            for (k, v) in over {
                merge_at(base.entry(k.clone()).or_insert(Value::Null), v, k);
            }
        }
        (Value::Array(base), Value::Array(over))
            if matches!(key, "permissions" | "instructions" | "skills") =>
        {
            base.extend(over.iter().cloned());
        }
        (base, over) => *base = over.clone(),
    }
}

/// A provider entry, when it has a base URL to send to.
fn provider(id: &str, p: &Value) -> Option<ProviderSpec> {
    let settings = &p["settings"];
    let package = p["package"]
        .as_str()
        .or_else(|| p["npm"].as_str())
        .unwrap_or_default();
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
            // Those its entry names, or the levels its API offers.
            variants: m["variants"].as_array().map_or_else(
                || crate::efforts::configured(package),
                |list| {
                    list.iter()
                        .filter_map(|v| {
                            Some(VariantSpec {
                                id: v["id"].as_str()?.to_owned(),
                                body: v["body"].as_object().cloned().unwrap_or_default(),
                                headers: headers_of(&v["headers"]),
                            })
                        })
                        .collect()
                },
            ),
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
        wire: crate::catalog::Wire::of_package(package),
        system_first: false,
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
    fn reads_its_keys_with_comments() {
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
        let none = Config::load(empty.path()).expect("loaded");
        assert_eq!(none, Config::from_value(&Value::Null, None));
        assert!(none.outline && !none.script_tools && none.model.is_none());
        let changed = Config::parse(
            r#"{ "experimental": { "outline": false, "scriptTools": true } }"#,
            None,
        )
        .expect("parsed");
        assert!(!changed.outline && changed.script_tools);
    }

    #[test]
    fn the_sandbox_is_on_unless_turned_off_and_takes_more_folders() {
        let sandbox = |text: &str| Config::parse(text, None).expect("parsed").sandbox;
        assert!(sandbox("{}").enabled);
        assert!(!sandbox(r#"{ "sandbox": false }"#).enabled);
        assert!(!sandbox(r#"{ "sandbox": { "enabled": false } }"#).enabled);
        let more =
            sandbox(r#"{ "sandbox": { "writable": ["~/notes"], "hidden": ["/srv/keys"] } }"#);
        assert!(more.enabled);
        assert_eq!(
            (more.writable, more.hidden),
            (vec!["~/notes".to_owned()], vec!["/srv/keys".to_owned()])
        );
    }

    #[test]
    fn an_opencode_setup_is_copied_once() {
        let home = tempfile::tempdir().expect("dir");
        let old = home.path().join("opencode");
        std::fs::create_dir_all(old.join("commands/team")).expect("folders");
        std::fs::write(
            old.join("opencode.jsonc"),
            "{\n  \"$schema\": \"https://opencode.ai/config.json\",\n  // Mine.\n  \"model\": \"local/coder\",\n  \"providers\": { \"local\": { \"package\": \"@opencode/ai/providers/openai-compatible\",\n    \"settings\": { \"baseURL\": \"http://127.0.0.1:9/v1/\" }, \"models\": { \"coder\": {} } } }\n}\n",
        )
        .expect("config");
        std::fs::write(old.join("AGENTS.md"), "Be brief.\n").expect("rules");
        std::fs::write(old.join("commands/team/review.md"), "Review.\n").expect("command");

        let copied = copy_from_opencode(home.path()).expect("copied");
        let target = home.path().join("harlo/harlo.jsonc");
        assert_eq!(copied.as_deref(), Some(target.as_path()));
        let text = std::fs::read_to_string(&target).expect("read");
        assert!(
            text.contains("// Mine.") && !text.contains("$schema"),
            "{text}"
        );
        let config = Config::load(home.path()).expect("loaded");
        assert_eq!(config.model.map(|m| m.model), Some("coder".to_owned()));
        assert!(text.contains("\"@ai-sdk/openai-compatible\""), "{text}");
        assert_eq!(config.providers[0].wire, crate::catalog::Wire::Chat);
        let harlo = home.path().join("harlo");
        assert!(harlo.join("AGENTS.md").is_file());
        assert!(harlo.join("commands/team/review.md").is_file());

        // Harlo's own file is never replaced.
        std::fs::write(&target, "{}\n").expect("edited");
        assert_eq!(copy_from_opencode(home.path()).expect("checked"), None);
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "{}\n");
    }

    #[test]
    fn old_tool_results_are_cleared_once_cold_unless_told_otherwise() {
        let prune = |text: &str| Config::parse(text, None).expect("config").prune;
        assert_eq!(prune("{}"), Prune::Cold);
        assert_eq!(prune(r#"{ "compaction": { "prune": true } }"#), Prune::Cold);
        assert_eq!(
            prune(r#"{ "compaction": { "prune": false } }"#),
            Prune::Never
        );
        assert_eq!(
            prune(r#"{ "compaction": { "prune": 90 } }"#),
            Prune::After(90_000)
        );
    }

    #[test]
    fn a_cache_write_without_a_price_costs_what_input_does() {
        let tokens = crate::message::Tokens {
            cache: crate::message::Cache {
                read: 0,
                write: 1_000_000,
            },
            ..Default::default()
        };
        let unlisted = Price {
            input: 2.0,
            ..Default::default()
        };
        assert!((unlisted.of(&tokens) - 2.0).abs() < 1e-9);
        let listed = Price {
            input: 2.0,
            cache_write: 2.5,
            ..Default::default()
        };
        assert!((listed.of(&tokens) - 2.5).abs() < 1e-9);
    }
}
