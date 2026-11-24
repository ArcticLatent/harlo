//! OpenCode's global config file on this computer, edited as OpenCode's docs
//! describe: `opencode.jsonc` or `opencode.json` in its config folder.
//!
//! The running server watches the file and reloads it, so a change applies
//! to every client of the server at once. Edits keep the rest of the file,
//! comments and layout included.

// Rust guideline compliant 2026-10-09

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstInputValue, CstRootNode};

/// The names of the global file, the one OpenCode prefers first.
const NAMES: [&str; 2] = ["opencode.jsonc", "opencode.json"];

/// The key naming the model new sessions use, as `provider/model`.
const MODEL: &str = "model";

/// The key that turns off the snapshots undo uses, when `false`.
const SNAPSHOTS: &str = "snapshots";

/// The object holding OpenCode's experimental switches.
const EXPERIMENTAL: &str = "experimental";

/// OpenCode's switch for automations, inside `experimental`.
const AUTOMATIONS: &str = "automations";

/// The object holding MCP settings, and its object of servers by name.
const MCP: &str = "mcp";
/// The servers inside [`MCP`].
const SERVERS: &str = "servers";
/// The object of providers OpenCode adds to its catalog, by ID.
const PROVIDERS: &str = "providers";
/// The package OpenCode's docs give for a server with the `OpenAI` API.
pub const OPENAI_COMPATIBLE: &str = "@opencode/ai/providers/openai-compatible";
/// What a new global file starts with: the schema OpenCode's docs give.
const NEW_FILE: &str = "{\n  \"$schema\": \"https://opencode.ai/config.json\"\n}\n";

/// The name of the file a change is written to before it replaces the real one.
const PENDING: &str = ".opencode.json.harlo";

/// Why the global config couldn't be read or changed.
#[derive(Debug)]
pub struct Error {
    /// The file concerned.
    path: PathBuf,
    /// What went wrong.
    kind: Kind,
}

/// What went wrong with the file.
#[derive(Debug)]
enum Kind {
    /// It couldn't be read or written.
    Io(io::Error),
    /// It isn't valid JSONC.
    Parse(String),
    /// Its top level isn't an object.
    NotObject,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.display();
        match &self.kind {
            Kind::Io(e) => write!(f, "{path}: {e}"),
            Kind::Parse(e) => write!(f, "{path} isn't valid JSON: {e}"),
            Kind::NotObject => write!(f, "{path} doesn't hold a settings object"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            Kind::Io(e) => Some(e),
            Kind::Parse(_) | Kind::NotObject => None,
        }
    }
}

/// OpenCode's global config folder on this computer, such as
/// `~/.config/opencode`.
///
/// Returns `None` when neither the config folder nor the home folder can be
/// found.
#[must_use]
pub fn folder() -> Option<PathBuf> {
    let config = dirs::config_dir().or_else(|| Some(dirs::home_dir()?.join(".config")))?;
    Some(config.join("opencode"))
}

/// The global file in `folder`: `opencode.jsonc` when there is one, since
/// OpenCode prefers it, otherwise `opencode.json`, which may not exist yet.
#[must_use]
pub fn file(folder: &Path) -> PathBuf {
    NAMES
        .iter()
        .map(|name| folder.join(name))
        .find(|path| path.exists())
        .unwrap_or_else(|| folder.join(NAMES[1]))
}

/// The model new sessions use by default, as `provider/model`, if the
/// global file in `folder` names one.
///
/// # Errors
///
/// Fails when the file exists but can't be read or isn't valid JSONC.
pub fn model(folder: &Path) -> Result<Option<String>, Error> {
    let path = file(folder);
    let Some(root) = parse(&path)? else {
        return Ok(None);
    };
    let value = root
        .object_value()
        .and_then(|object| object.get(MODEL))
        .and_then(|prop| prop.value())
        .and_then(|value| value.as_string_lit())
        .and_then(|text| text.decoded_value().ok());
    Ok(value)
}

/// Sets the model new sessions use, as `provider/model`, in the global file
/// in `folder`; `None` removes the setting so OpenCode picks again.
///
/// The rest of the file stays as it was. A missing file or folder is
/// created.
///
/// # Errors
///
/// Fails when the file can't be read or written, isn't valid JSONC, or
/// doesn't hold an object.
pub fn set_model(folder: &Path, model: Option<&str>) -> Result<(), Error> {
    set(folder, MODEL, model.map(CstInputValue::from))
}

/// Whether OpenCode keeps snapshots of a project's files before each turn,
/// so undoing a turn can put them back: on unless the global file in
/// `folder` turns them off.
///
/// # Errors
///
/// Fails when the file exists but can't be read or isn't valid JSONC.
pub fn snapshots(folder: &Path) -> Result<bool, Error> {
    let path = file(folder);
    let Some(root) = parse(&path)? else {
        return Ok(true);
    };
    let off = root
        .object_value()
        .and_then(|object| object.get(SNAPSHOTS))
        .and_then(|prop| prop.value())
        .and_then(|value| value.as_boolean_lit())
        .is_some_and(|value| !value.value());
    Ok(!off)
}

/// Turns OpenCode's snapshots on or off in the global file in `folder`.
/// On, the default, removes the setting.
///
/// # Errors
///
/// Fails as [`set_model`] does.
pub fn set_snapshots(folder: &Path, on: bool) -> Result<(), Error> {
    set(
        folder,
        SNAPSHOTS,
        (!on).then_some(CstInputValue::from(false)),
    )
}

/// Sets the snapshots as [`set_snapshots`] does, on a blocking thread.
///
/// # Errors
///
/// Returns the errors of [`set_snapshots`], or an error when the thread is
/// cancelled.
pub async fn set_snapshots_later(folder: PathBuf, on: bool) -> Result<(), Error> {
    later(folder, move |folder| set_snapshots(folder, on)).await
}

/// Whether OpenCode runs automations (Harlo's schedules): the top-level
/// `automations` in the global file in `folder`, on unless set to `false`.
///
/// # Errors
///
/// Fails when the file exists but can't be read or isn't valid JSONC.
pub fn automations(folder: &Path) -> Result<bool, Error> {
    let path = file(folder);
    let Some(root) = parse(&path)? else {
        return Ok(true);
    };
    let off = root
        .object_value()
        .and_then(|object| object.get(AUTOMATIONS))
        .and_then(|prop| prop.value())
        .and_then(|value| value.as_boolean_lit())
        .is_some_and(|value| !value.value());
    Ok(!off)
}

/// Turns OpenCode's automations on or off in the global file in `folder`.
/// On, the default, removes the setting; off writes `"automations": false`.
/// Either way the `experimental.automations` of older releases goes, since
/// OpenCode now warns about it.
///
/// # Errors
///
/// Fails as [`set_model`] does, or when `experimental` isn't an object.
pub fn set_automations(folder: &Path, on: bool) -> Result<(), Error> {
    set_at(folder, &[EXPERIMENTAL, AUTOMATIONS], None)?;
    set_at(
        folder,
        &[AUTOMATIONS],
        (!on).then_some(CstInputValue::from(false)),
    )
}

/// Sets the automations as [`set_automations`] does, on a blocking thread.
///
/// # Errors
///
/// Returns the errors of [`set_automations`], or an error when the thread
/// is cancelled.
pub async fn set_automations_later(folder: PathBuf, on: bool) -> Result<(), Error> {
    later(folder, move |folder| set_automations(folder, on)).await
}

/// How OpenCode runs an MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpRun {
    /// A program on the server's computer, spoken to over its input and
    /// output.
    Local {
        /// The program and its arguments.
        command: Vec<String>,
        /// Environment variables it gets, by name.
        environment: Vec<(String, String)>,
    },
    /// A server reached over HTTP.
    Remote {
        /// Its address.
        url: String,
        /// Headers sent with each request, such as a key.
        headers: Vec<(String, String)>,
    },
}

/// An MCP server in the global file: `mcp.servers.<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpEntry {
    /// Its name, which OpenCode puts before its tools' names.
    pub name: String,
    /// How it runs.
    pub run: McpRun,
    /// Whether it's turned off, so OpenCode doesn't start it.
    pub disabled: bool,
}

impl McpEntry {
    /// Reads `value`, an entry of `mcp.servers`, named `name`.
    fn from_json(name: &str, value: &serde_json::Value) -> Option<Self> {
        let pairs = |key: &str| -> Vec<(String, String)> {
            value[key]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        };
        let run = match value["type"].as_str()? {
            "local" => McpRun::Local {
                command: value["command"]
                    .as_array()?
                    .iter()
                    .filter_map(|part| part.as_str().map(str::to_owned))
                    .collect(),
                environment: pairs("environment"),
            },
            "remote" => McpRun::Remote {
                url: value["url"].as_str()?.to_owned(),
                headers: pairs("headers"),
            },
            _ => return None,
        };
        Some(Self {
            name: name.to_owned(),
            run,
            disabled: value["disabled"].as_bool().unwrap_or(false),
        })
    }

    /// The entry as the file holds it.
    fn input(&self) -> CstInputValue {
        let text = |s: &str| CstInputValue::String(s.to_owned());
        let pairs = |list: &[(String, String)]| {
            CstInputValue::Object(list.iter().map(|(k, v)| (k.clone(), text(v))).collect())
        };
        let mut fields = Vec::new();
        match &self.run {
            McpRun::Local {
                command,
                environment,
            } => {
                fields.push(("type".to_owned(), text("local")));
                fields.push((
                    "command".to_owned(),
                    CstInputValue::Array(command.iter().map(|part| text(part)).collect()),
                ));
                if !environment.is_empty() {
                    fields.push(("environment".to_owned(), pairs(environment)));
                }
            }
            McpRun::Remote { url, headers } => {
                fields.push(("type".to_owned(), text("remote")));
                fields.push(("url".to_owned(), text(url)));
                if !headers.is_empty() {
                    fields.push(("headers".to_owned(), pairs(headers)));
                }
            }
        }
        if self.disabled {
            fields.push(("disabled".to_owned(), CstInputValue::Bool(true)));
        }
        CstInputValue::Object(fields)
    }
}

/// The MCP servers the global file in `folder` sets up, in its order.
/// Entries OpenCode wouldn't read are left out.
///
/// # Errors
///
/// Fails when the file exists but can't be read or isn't valid JSONC.
pub fn mcp_servers(folder: &Path) -> Result<Vec<McpEntry>, Error> {
    let path = file(folder);
    let Some(root) = parse(&path)? else {
        return Ok(Vec::new());
    };
    let Some(servers) = root
        .object_value()
        .and_then(|object| object.object_value(MCP))
        .and_then(|object| object.object_value(SERVERS))
    else {
        return Ok(Vec::new());
    };
    Ok(servers
        .properties()
        .into_iter()
        .filter_map(|prop| {
            let name = prop.name()?.decoded_value().ok()?;
            let value = prop.value()?.to_serde_value()?;
            McpEntry::from_json(&name, &value)
        })
        .collect())
}

/// Adds `entry` to the global file in `folder`, or replaces the server of
/// that name. OpenCode starts it when it reloads the file.
///
/// # Errors
///
/// Fails as [`set_model`] does, or when `mcp` or its `servers` isn't an
/// object.
pub fn set_mcp_server(folder: &Path, entry: &McpEntry) -> Result<(), Error> {
    set_at(folder, &[MCP, SERVERS, &entry.name], Some(entry.input()))
}

/// Removes MCP server `name` from the global file in `folder`, and `mcp`
/// with it when nothing else is left there.
///
/// # Errors
///
/// Fails as [`set_model`] does.
pub fn remove_mcp_server(folder: &Path, name: &str) -> Result<(), Error> {
    set_at(folder, &[MCP, SERVERS, name], None)
}

/// Turns MCP server `name` in the global file in `folder` off, or on again,
/// which removes `disabled`. A server the file doesn't have is left alone.
///
/// # Errors
///
/// Fails as [`set_model`] does.
pub fn set_mcp_disabled(folder: &Path, name: &str, disabled: bool) -> Result<(), Error> {
    if !mcp_servers(folder)?.iter().any(|entry| entry.name == name) {
        return Ok(());
    }
    set_at(
        folder,
        &[MCP, SERVERS, name, "disabled"],
        disabled.then_some(CstInputValue::Bool(true)),
    )
}

/// Reads the MCP servers as [`mcp_servers`] does, on a blocking thread.
///
/// # Errors
///
/// Returns the errors of [`mcp_servers`], or an error when the thread is
/// cancelled.
pub async fn mcp_servers_later(folder: PathBuf) -> Result<Vec<McpEntry>, Error> {
    let path = file(&folder);
    tokio::task::spawn_blocking(move || mcp_servers(&folder))
        .await
        .map_err(|join| Error {
            path,
            kind: Kind::Io(io::Error::other(join.to_string())),
        })?
}

/// Adds or replaces a server as [`set_mcp_server`] does, on a blocking
/// thread.
///
/// # Errors
///
/// Returns the errors of [`set_mcp_server`], or an error when the thread is
/// cancelled.
pub async fn set_mcp_server_later(folder: PathBuf, entry: McpEntry) -> Result<(), Error> {
    later(folder, move |folder| set_mcp_server(folder, &entry)).await
}

/// Removes a server as [`remove_mcp_server`] does, on a blocking thread.
///
/// # Errors
///
/// Returns the errors of [`remove_mcp_server`], or an error when the thread
/// is cancelled.
pub async fn remove_mcp_server_later(folder: PathBuf, name: String) -> Result<(), Error> {
    later(folder, move |folder| remove_mcp_server(folder, &name)).await
}

/// Turns a server off or on as [`set_mcp_disabled`] does, on a blocking
/// thread.
///
/// # Errors
///
/// Returns the errors of [`set_mcp_disabled`], or an error when the thread
/// is cancelled.
pub async fn set_mcp_disabled_later(
    folder: PathBuf,
    name: String,
    disabled: bool,
) -> Result<(), Error> {
    later(folder, move |folder| {
        set_mcp_disabled(folder, &name, disabled)
    })
    .await
}

/// A model server on this computer, added to OpenCode as a provider that
/// speaks the `OpenAI` API: `providers.<id>`, with its models listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProvider {
    /// Its ID, which goes before its models' IDs.
    pub id: String,
    /// Its name for people.
    pub name: String,
    /// Its address, ending in `/v1`.
    pub base_url: String,
    /// Its models: ID, name, and context in tokens.
    pub models: Vec<(String, String, u32)>,
}

impl LocalProvider {
    /// The entry as the file holds it.
    fn input(&self) -> CstInputValue {
        let text = |s: &str| CstInputValue::String(s.to_owned());
        let models = self
            .models
            .iter()
            .map(|(id, name, context)| {
                let mut fields = vec![("name".to_owned(), text(name))];
                if *context > 0 {
                    fields.push((
                        "limit".to_owned(),
                        CstInputValue::Object(vec![(
                            "context".to_owned(),
                            CstInputValue::Number(context.to_string()),
                        )]),
                    ));
                }
                (id.clone(), CstInputValue::Object(fields))
            })
            .collect();
        CstInputValue::Object(vec![
            ("name".to_owned(), text(&self.name)),
            ("package".to_owned(), text(OPENAI_COMPATIBLE)),
            (
                "settings".to_owned(),
                CstInputValue::Object(vec![("baseURL".to_owned(), text(&self.base_url))]),
            ),
            ("models".to_owned(), CstInputValue::Object(models)),
        ])
    }
}

/// Whether the global file in `folder` has provider `id`.
///
/// # Errors
///
/// Fails when the file exists but can't be read or isn't valid JSONC.
pub fn has_provider(folder: &Path, id: &str) -> Result<bool, Error> {
    let Some(root) = parse(&file(folder))? else {
        return Ok(false);
    };
    Ok(root
        .object_value()
        .and_then(|object| object.object_value(PROVIDERS))
        .and_then(|providers| providers.get(id))
        .is_some())
}

/// Adds `provider` to the global file in `folder`, or replaces the one with
/// its ID. OpenCode lists its models when it reloads the file.
///
/// # Errors
///
/// Fails as [`set_model`] does, or when `providers` isn't an object.
pub fn set_provider(folder: &Path, provider: &LocalProvider) -> Result<(), Error> {
    set_at(folder, &[PROVIDERS, &provider.id], Some(provider.input()))
}

/// Removes provider `id` from the global file in `folder`.
///
/// # Errors
///
/// Fails as [`set_model`] does.
pub fn remove_provider(folder: &Path, id: &str) -> Result<(), Error> {
    set_at(folder, &[PROVIDERS, id], None)
}

/// Adds or replaces a provider as [`set_provider`] does, on a blocking
/// thread.
///
/// # Errors
///
/// Returns the errors of [`set_provider`], or an error when the thread is
/// cancelled.
pub async fn set_provider_later(folder: PathBuf, provider: LocalProvider) -> Result<(), Error> {
    later(folder, move |folder| set_provider(folder, &provider)).await
}

/// Removes a provider as [`remove_provider`] does, on a blocking thread.
///
/// # Errors
///
/// Returns the errors of [`remove_provider`], or an error when the thread is
/// cancelled.
pub async fn remove_provider_later(folder: PathBuf, id: String) -> Result<(), Error> {
    later(folder, move |folder| remove_provider(folder, &id)).await
}

/// Sets `key` to `value` at the top of the global file in `folder`, or
/// removes it for `None`, keeping the rest of the file. A missing file or
/// folder is created.
fn set(folder: &Path, key: &str, value: Option<CstInputValue>) -> Result<(), Error> {
    set_at(folder, &[key], value)
}

/// Sets the key at `keys`, a path of nested objects, to `value` in the
/// global file in `folder`, or removes it for `None` along with an object
/// on the way that it leaves empty. The rest of the file stays as it was;
/// a missing file, folder or object on the way is created.
fn set_at(folder: &Path, keys: &[&str], value: Option<CstInputValue>) -> Result<(), Error> {
    let Some((key, outer)) = keys.split_last() else {
        return Ok(());
    };
    let path = file(folder);
    let root = match parse(&path)? {
        Some(root) => root,
        None => CstRootNode::parse(NEW_FILE, &ParseOptions::default()).map_err(|e| Error {
            path: path.clone(),
            kind: Kind::Parse(e.to_string()),
        })?,
    };
    let object = match root.value() {
        None => root.object_value_or_set(),
        Some(_) => root.object_value().ok_or_else(|| Error {
            path: path.clone(),
            kind: Kind::NotObject,
        })?,
    };
    let not_object = || Error {
        path: path.clone(),
        kind: Kind::NotObject,
    };
    // The objects on the way, outermost first.
    let mut objects = vec![object];
    for name in outer {
        let Some(parent) = objects.last() else {
            break;
        };
        let next = if value.is_some() {
            parent.object_value_or_create(name).ok_or_else(not_object)?
        } else {
            match parent.object_value(name) {
                Some(next) => next,
                None => return Ok(()),
            }
        };
        objects.push(next);
    }
    let Some(object) = objects.last() else {
        return Ok(());
    };
    match (object.get(key), value) {
        (Some(prop), Some(value)) => prop.set_value(value),
        (Some(prop), None) => prop.remove(),
        (None, Some(value)) => {
            object.append(key, value);
        }
        (None, None) => return Ok(()),
    }
    // Objects the removal left empty go too, innermost first: each is the
    // value of a property of the one before it.
    for (index, name) in outer.iter().enumerate().rev() {
        let (Some(parent), Some(object)) = (objects.get(index), objects.get(index + 1)) else {
            break;
        };
        if !object.properties().is_empty() {
            break;
        }
        if let Some(prop) = parent.get(name) {
            prop.remove();
        }
    }
    write(folder, &path, &root.to_string())
}

/// Runs `change` on the global file in `folder` on a blocking thread.
async fn later(
    folder: PathBuf,
    change: impl FnOnce(&Path) -> Result<(), Error> + Send + 'static,
) -> Result<(), Error> {
    let path = file(&folder);
    tokio::task::spawn_blocking(move || change(&folder))
        .await
        .map_err(|join| Error {
            path,
            kind: Kind::Io(io::Error::other(join.to_string())),
        })?
}

/// Sets the model as [`set_model`] does, on a blocking thread.
///
/// # Errors
///
/// Returns the errors of [`set_model`], or an error when the thread is
/// cancelled.
pub async fn set_model_later(folder: PathBuf, model: Option<String>) -> Result<(), Error> {
    later(folder, move |folder| set_model(folder, model.as_deref())).await
}

/// Parses the file at `path`, or `None` when there's no file.
fn parse(path: &Path) -> Result<Option<CstRootNode>, Error> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(Error {
                path: path.to_owned(),
                kind: Kind::Io(e),
            });
        }
    };
    CstRootNode::parse(&text, &ParseOptions::default())
        .map(Some)
        .map_err(|e| Error {
            path: path.to_owned(),
            kind: Kind::Parse(e.to_string()),
        })
}

/// Replaces the file at `path` with `text` in one step, so the server never
/// reads half of it, keeping the old file's permissions.
fn write(folder: &Path, path: &Path, text: &str) -> Result<(), Error> {
    let failed = |e: io::Error| Error {
        path: path.to_owned(),
        kind: Kind::Io(e),
    };
    fs::create_dir_all(folder).map_err(failed)?;
    let pending = folder.join(PENDING);
    fs::write(&pending, text).map_err(failed)?;
    if let Ok(meta) = fs::metadata(path) {
        fs::set_permissions(&pending, meta.permissions()).map_err(failed)?;
    }
    fs::rename(&pending, path).map_err(failed)
}
