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

/// Sets `key` to `value` at the top of the global file in `folder`, or
/// removes it for `None`, keeping the rest of the file. A missing file or
/// folder is created.
fn set(folder: &Path, key: &str, value: Option<CstInputValue>) -> Result<(), Error> {
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
    match (object.get(key), value) {
        (Some(prop), Some(value)) => prop.set_value(value),
        (Some(prop), None) => prop.remove(),
        (None, Some(value)) => {
            object.append(key, value);
        }
        (None, None) => return Ok(()),
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
