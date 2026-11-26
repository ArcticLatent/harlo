//! Plugins: packages from a Git repository that add to what agents have.
//!
//! A plugin is a folder with a `plugin.json` beside the things it brings:
//! `skills`, `commands` and `agents` folders, read as the config folder's
//! are, and the MCP servers its `plugin.json` names. A person adds one by
//! its address; the server fetches it at one commit and keeps it under
//! its data folder, by name and commit. Nothing is fetched or updated
//! unless a person asks.
//!
//! The plugins added are listed in `plugins.json` in the config folder,
//! with the commit each is kept at, so the file can travel to another
//! computer and bring the same plugins there.
//!
//! What acts for the person waits for a yes: MCP servers (programs started
//! on the spot), agents' rules that let them act unasked, and commands'
//! shell lines. The yes is kept with a fingerprint of what was asked, on
//! this computer only, so an update that asks for something else asks
//! again. Until then nothing of the plugin is used.
//!
//! A person's own skills, commands, agents and MCP servers win over a
//! plugin's of the same name, and a plugin can't change a built-in agent.
//!
//! A plugin may also bring code of its own, which gives agents tools and
//! runs commands; [`code`] has how it runs and what it may reach.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::server::{Refusal, Server, lock};

pub(crate) mod code;
mod fetch;
mod gallery;

/// The file a plugin's folder must have.
const MANIFEST: &str = "plugin.json";

/// The file in the config folder that lists the plugins added.
pub(crate) const LIST: &str = "plugins.json";

/// The store setting a plugin's approval is kept under, before its name.
const APPROVED: &str = "plugin/approved/";

/// The most characters of a plugin's name.
const NAME_LENGTH: usize = 64;

/// A plugin as `plugins.json` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    /// Where it comes from: an `https://` address or a folder.
    src: String,
    /// The tag or branch asked for; none follows the default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    /// The commit it's kept at.
    commit: String,
    /// Whether it's in use.
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if hands a reference"
)]
fn is_yes(value: &bool) -> bool {
    *value
}

/// `plugins.json`, whole.
#[derive(Debug, Default, Serialize, Deserialize)]
struct List {
    /// The plugins, by name.
    #[serde(default)]
    plugins: BTreeMap<String, Entry>,
}

/// What a plugin's `plugin.json` says.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    /// Its name; without one, the last part of its address.
    name: Option<String>,
    /// What it's for, in a line.
    description: Option<String>,
    /// Its own version, shown to people.
    version: Option<String>,
    /// The oldest Harlo it works with.
    min_harlo: Option<String>,
    /// MCP servers, by name, as the config's `mcp.servers` has them.
    #[serde(default)]
    mcp: Map<String, Value>,
    /// The file its code is in; `plugin.js` unless named.
    main: Option<String>,
    /// The tools its code gives agents.
    #[serde(default)]
    tools: Vec<code::ToolSpec>,
    /// The commands its code runs.
    #[serde(default)]
    commands: Vec<code::CommandSpec>,
    /// What its code runs for with no one calling: `turn.ended`.
    #[serde(default)]
    events: Vec<String>,
    /// What a person fills in for it, such as a token.
    #[serde(default)]
    settings: Vec<code::SettingSpec>,
    /// What its code may reach.
    #[serde(default)]
    permissions: code::Permissions,
}

impl Manifest {
    /// What it says of its code.
    fn declared(&self) -> code::Declared<'_> {
        code::Declared {
            main: self.main.as_deref(),
            tools: &self.tools,
            commands: &self.commands,
            events: &self.events,
            settings: &self.settings,
            permissions: &self.permissions,
        }
    }
}

/// Where a plugin stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Standing {
    /// In use.
    Active,
    /// Turned off by the person.
    Off,
    /// What it asks for hasn't been allowed.
    Waiting,
    /// Listed, but not fetched on this computer.
    Missing,
    /// Unusable, and why.
    Failed(String),
}

impl Standing {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Off => "off",
            Self::Waiting => "waiting",
            Self::Missing => "missing",
            Self::Failed(_) => "failed",
        }
    }
}

/// A listed plugin as it is on this computer.
#[derive(Debug)]
struct Seen {
    name: String,
    entry: Entry,
    /// Its folder.
    root: PathBuf,
    standing: Standing,
    manifest: Manifest,
    /// What it asks for that waits for a yes, a line each.
    asks: Vec<String>,
    /// The fingerprint of what it asks; none when it asks nothing.
    key: Option<String>,
}

/// A plugin in use: what a folder's configuration takes from it.
#[derive(Debug, Clone)]
pub(crate) struct Loaded {
    /// Its folder, whose `skills` and `commands` are read.
    pub root: PathBuf,
    /// Its MCP servers, by name.
    pub mcp: Map<String, Value>,
    /// Its agents' entries, by ID.
    pub agents: Vec<(String, Value)>,
    /// Its code, if it has any.
    pub code: Option<Arc<code::Program>>,
}

/// The plugins in use, as last worked out.
#[derive(Debug, Default)]
pub(crate) struct Cache {
    /// What was worked out, and `plugins.json`'s change time then.
    held: Option<(Option<SystemTime>, Arc<Vec<Loaded>>)>,
    /// Why a plugin's code last failed when it ran with no one calling,
    /// by plugin: a person sees it nowhere else.
    faults: std::collections::HashMap<String, String>,
    /// The published plugins, as GitHub last listed them.
    gallery: gallery::Kept,
}

/// Whether `name` can name a plugin: lower-case letters, digits, `-`, `_`
/// and `.`, starting with a letter or digit. It names a folder.
fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.len() <= NAME_LENGTH
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_.".contains(c))
        && !name.contains("..")
}

/// The name a plugin from `src` gets when its `plugin.json` gives none:
/// the last part of the address, without `.git`.
fn name_from(src: &str) -> String {
    let last = src
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    last.trim_end_matches(".git")
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Reads the `plugin.json` in `root`.
///
/// # Errors
///
/// Fails, with a line for a person, when there's none or it isn't valid.
fn manifest(root: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(root.join(MANIFEST))
        .map_err(|_missing| format!("It has no {MANIFEST}, so it isn't a Harlo plugin."))?;
    serde_json::from_str(&text).map_err(|e| format!("Its {MANIFEST} can't be read: {e}"))
}

/// Reads the `plugin.json` of the plugin fetched into `root`, before it's
/// kept.
///
/// # Errors
///
/// Fails, with a line for a person, when it isn't a plugin this Harlo can
/// use: one that is used already stays at the commit that works.
fn usable(root: &Path) -> Result<Manifest, String> {
    let manifest = manifest(root)?;
    let fault = manifest
        .declared()
        .fault(root)
        .or_else(|| too_old(manifest.min_harlo.as_deref()));
    match fault {
        Some(why) => Err(why),
        None => Ok(manifest),
    }
}

/// Why this Harlo can't use a plugin that needs `min` or newer; none when
/// it can.
fn too_old(min: Option<&str>) -> Option<String> {
    let min = min?;
    let needed = semver::Version::parse(min.trim_start_matches('v')).ok()?;
    let have = semver::Version::parse(env!("CARGO_PKG_VERSION")).ok()?;
    (have < needed).then(|| format!("It needs Harlo {needed} or newer; this is {have}."))
}

/// `mcp` with each server that's a program started in `root`, unless it
/// names a folder itself.
fn servers_in(mcp: &Map<String, Value>, root: &Path) -> Map<String, Value> {
    mcp.iter()
        .map(|(name, spec)| {
            let mut spec = spec.clone();
            if spec["url"].is_null() && spec["cwd"].is_null() && spec.is_object() {
                spec["cwd"] = json!(root);
            }
            (name.clone(), spec)
        })
        .collect()
}

impl Server {
    /// Where plugins are kept: `<data>/plugins/<name>/<commit>`.
    fn plugins_dir(&self) -> PathBuf {
        self.snapshots.with_file_name("plugins")
    }

    /// The file that lists the plugins added.
    fn plugins_file(&self) -> PathBuf {
        self.config_dir.join(LIST)
    }

    /// The plugins `plugins.json` lists; none when there's no file.
    ///
    /// # Errors
    ///
    /// Fails when the file is there but can't be read as a list.
    fn plugin_list(&self) -> Result<List, Refusal> {
        let file = self.plugins_file();
        match std::fs::read_to_string(&file) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| Refusal::Bad(format!("{} can't be read: {e}", file.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(List::default()),
            Err(e) => Err(Refusal::Store(format!(
                "{} can't be read: {e}",
                file.display()
            ))),
        }
    }

    /// Changes the list: reads `plugins.json`, has `change` alter it, and
    /// writes it, then tells clients the plugins changed.
    ///
    /// One change at a time: two at once would each write the list as it
    /// read it, and the first would be lost.
    ///
    /// # Errors
    ///
    /// When the list can't be read or written, or `change` refuses; the
    /// file is then as it was.
    fn change_plugins<T>(
        &self,
        change: impl FnOnce(&mut List) -> Result<T, Refusal>,
    ) -> Result<T, Refusal> {
        let made = {
            let mut cache = lock(&self.plugins);
            let mut list = self.plugin_list()?;
            let made = change(&mut list)?;
            self.write_plugin_list(&list)?;
            cache.held = None;
            made
        };
        self.plugins_changed();
        Ok(made)
    }

    /// Writes `list` as `plugins.json`, whole or not at all.
    fn write_plugin_list(&self, list: &List) -> Result<(), Refusal> {
        let file = self.plugins_file();
        let failed =
            |e: std::io::Error| Refusal::Store(format!("{} can't be written: {e}", file.display()));
        let text = serde_json::to_string_pretty(list).unwrap_or_default();
        std::fs::create_dir_all(&self.config_dir).map_err(failed)?;
        let fresh = file.with_extension("json.new");
        std::fs::write(&fresh, text + "\n").map_err(failed)?;
        std::fs::rename(&fresh, &file).map_err(failed)
    }

    /// Forgets what was worked out and tells clients the plugins changed.
    fn plugins_changed(&self) {
        lock(&self.plugins).held = None;
        self.emit("plugin.updated", json!({}), None, None);
        // What plugins bring shows in these lists.
        self.emit("mcp.updated", json!({}), None, None);
    }

    /// Plugin `name` as it is on this computer.
    fn see(&self, name: &str, entry: &Entry) -> Seen {
        let root = self.plugins_dir().join(name).join(&entry.commit);
        let mut seen = Seen {
            name: name.to_owned(),
            entry: entry.clone(),
            root,
            standing: Standing::Missing,
            manifest: Manifest::default(),
            asks: Vec::new(),
            key: None,
        };
        if !valid_name(name) || !fetch::is_commit(&entry.commit) {
            seen.standing = Standing::Failed(format!("{LIST} names it wrongly."));
            return seen;
        }
        if !seen.root.is_dir() {
            return seen;
        }
        match manifest(&seen.root) {
            Ok(manifest) => seen.manifest = manifest,
            Err(why) => {
                seen.standing = Standing::Failed(why);
                return seen;
            }
        }
        if let Some(why) = seen.manifest.declared().fault(&seen.root) {
            seen.standing = Standing::Failed(why);
            return seen;
        }
        // What acts for the person, worded as a project's own is. (The
        // servers as written: the folder they start in has the commit in
        // its name, and a new commit alone asks nothing new.)
        let mut held = crate::trust::Held::default();
        held.take_from(&mut json!({ "mcp": { "servers": seen.manifest.mcp } }));
        held.take_folder(&seen.root);
        for (line, exact) in seen.manifest.declared().asks() {
            held.note(line, &exact);
        }
        // (With where it comes from: another source under the name asks
        // again.)
        if let Some(waits) = held.finish(Path::new(&format!("{name} from {}", entry.src))) {
            seen.asks = waits.wants;
            seen.key = Some(waits.key);
        }
        let approved = seen.key.as_ref().is_none_or(|key| {
            self.store
                .setting(&format!("{APPROVED}{name}"))
                .ok()
                .flatten()
                .as_ref()
                == Some(key)
        });
        seen.standing = if let Some(why) = too_old(seen.manifest.min_harlo.as_deref()) {
            Standing::Failed(why)
        } else if !entry.enabled {
            Standing::Off
        } else if !approved {
            Standing::Waiting
        } else {
            Standing::Active
        };
        seen
    }

    /// The plugins in use, in name order.
    ///
    /// Worked out once and kept until `plugins.json` or an approval
    /// changes: a plugin's folder never changes under its commit.
    pub(crate) fn plugins_in_use(&self) -> Arc<Vec<Loaded>> {
        let stamp = std::fs::metadata(self.plugins_file())
            .and_then(|m| m.modified())
            .ok();
        if let Some((at, loaded)) = &lock(&self.plugins).held
            && *at == stamp
        {
            return Arc::clone(loaded);
        }
        let list = self.plugin_list().unwrap_or_else(|why| {
            tracing::warn!(name: "plugins.list.unreadable", error = %why, "no plugins are used");
            List::default()
        });
        let builtin = |id: &str| self.agents.iter().any(|a| a.id == id);
        let loaded: Vec<Loaded> = list
            .plugins
            .iter()
            .map(|(name, entry)| self.see(name, entry))
            .filter(|seen| seen.standing == Standing::Active)
            .map(|seen| Loaded {
                code: seen
                    .manifest
                    .declared()
                    .program(
                        &seen.name,
                        &seen.root,
                        seen.manifest.description.as_deref(),
                        seen.manifest.version.as_deref(),
                    )
                    .map(Arc::new),
                mcp: servers_in(&seen.manifest.mcp, &seen.root),
                // New agents only: a plugin neither changes nor removes
                // one that's built in.
                agents: harlo_engine::agents::in_folder(&seen.root)
                    .into_iter()
                    .filter(|(id, entry)| !builtin(id) && entry["disabled"] != true)
                    .collect(),
                root: seen.root,
            })
            .collect();
        let loaded = Arc::new(loaded);
        lock(&self.plugins).held = Some((stamp, Arc::clone(&loaded)));
        loaded
    }

    /// The code of the plugin in use whose tools scripts reach under
    /// `namespace`.
    pub(crate) fn plugin_program(&self, namespace: &str) -> Option<Arc<code::Program>> {
        self.plugin_programs()
            .into_iter()
            .find(|program| program.namespace == namespace)
    }

    /// The code of the plugins in use.
    pub(crate) fn plugin_programs(&self) -> Vec<Arc<code::Program>> {
        self.plugins_in_use()
            .iter()
            .filter_map(|plugin| plugin.code.clone())
            .collect()
    }

    /// The plugins added, as clients list them.
    ///
    /// # Errors
    ///
    /// When `plugins.json` can't be read.
    pub(crate) fn plugins(&self) -> Result<Vec<Value>, Refusal> {
        Ok(self
            .plugin_list()?
            .plugins
            .iter()
            .map(|(name, entry)| self.shown(&self.see(name, entry)))
            .collect())
    }

    /// Plugin `name` as clients see it.
    fn plugin(&self, name: &str) -> Result<Value, Refusal> {
        let list = self.plugin_list()?;
        let entry = list
            .plugins
            .get(name)
            .ok_or_else(|| Refusal::NotFound(format!("There's no plugin {name}.")))?;
        Ok(self.shown(&self.see(name, entry)))
    }

    /// A plugin as clients see it.
    fn shown(&self, seen: &Seen) -> Value {
        let mut plugin = shown(seen);
        plugin["settings"] = json!(code::settings_shown(
            self,
            &seen.name,
            &seen.manifest.settings
        ));
        if let Some(why) = lock(&self.plugins).faults.get(&seen.name) {
            plugin["eventError"] = json!(why);
        }
        plugin
    }

    /// Fills in settings of plugin `name`: text by key, or null to clear
    /// one.
    ///
    /// # Errors
    ///
    /// When there's no such plugin, it names no such setting, or a value
    /// isn't text it can keep.
    pub(crate) fn fill_plugin(
        &self,
        name: &str,
        changes: &Map<String, Value>,
    ) -> Result<Value, Refusal> {
        let list = self.plugin_list()?;
        let entry = list
            .plugins
            .get(name)
            .ok_or_else(|| Refusal::NotFound(format!("There's no plugin {name}.")))?;
        let seen = self.see(name, entry);
        code::fill(self, name, &seen.manifest.settings, changes).map_err(Refusal::Bad)?;
        self.emit("plugin.updated", json!({}), None, None);
        Ok(self.shown(&seen))
    }

    /// Runs the code of the plugins that asked to when a turn of session
    /// `session` ends as `outcome`, without waiting for it.
    ///
    /// What the code returns goes nowhere. Why it failed is kept for the
    /// plugin's page, until it next runs through.
    pub(crate) fn plugins_turn_ended(
        self: &Arc<Self>,
        session: &crate::store::Session,
        outcome: &str,
        error: Option<&str>,
    ) {
        for program in self.plugin_programs() {
            if !program.events.iter().any(|e| e == code::TURN_ENDED) {
                continue;
            }
            let server = Arc::clone(self);
            let event = json!({
                "outcome": outcome,
                "error": error,
                "session": {
                    "id": session.id,
                    "directory": session.location.directory,
                    "title": session.title,
                    "parentID": session.parent_id,
                },
            });
            let (id, directory) = (session.id.clone(), session.location.directory.clone());
            tokio::spawn(async move {
                let within = code::Within {
                    session: Some((&id, &directory)),
                    tools: None,
                };
                let ran = program
                    .call(&server, code::Kind::Event, code::TURN_ENDED, event, within)
                    .await;
                let changed = {
                    let mut plugins = lock(&server.plugins);
                    match &ran {
                        Ok(_) => plugins.faults.remove(&program.plugin).is_some(),
                        Err(why) => {
                            plugins.faults.insert(program.plugin.clone(), why.clone())
                                != Some(why.clone())
                        }
                    }
                };
                if let Err(why) = &ran {
                    tracing::warn!(name: "plugins.event.failed", plugin = %program.plugin, error = %why, "a plugin's code failed when a turn ended");
                }
                if changed {
                    server.emit("plugin.updated", json!({}), None, None);
                }
            });
        }
    }

    /// Adds the plugin at `src`, at `version` (a tag, branch or commit) or
    /// its default branch: fetches it and lists it. What it asks for
    /// still waits for [`Self::approve_plugin`].
    ///
    /// # Errors
    ///
    /// When the address isn't one a plugin comes from, it can't be
    /// fetched, it isn't a plugin, or one of its name is there already.
    pub(crate) async fn add_plugin(
        &self,
        src: &str,
        version: Option<&str>,
    ) -> Result<Value, Refusal> {
        let src = fetch::source(src).map_err(Refusal::Bad)?;
        let version = version.map(str::trim).filter(|v| !v.is_empty());
        let commit = fetch::resolve(&src, version).await.map_err(Refusal::Bad)?;
        let dir = self.plugins_dir();
        let fetched = fetch::fetch(&src, &commit, &dir)
            .await
            .map_err(Refusal::Bad)?;
        let named = usable(fetched.path()).map(|m| m.name.unwrap_or_else(|| name_from(&src)));
        let name = match named {
            Ok(name) if valid_name(&name) => name,
            Ok(name) => {
                return Err(Refusal::Bad(format!(
                    "\"{name}\" can't name a plugin: lower-case letters, digits, - _ and . only."
                )));
            }
            Err(why) => return Err(Refusal::Bad(why)),
        };
        self.change_plugins(|list| {
            if list.plugins.contains_key(&name) {
                return Err(Refusal::Exists(format!(
                    "A plugin named {name} is there already. Update it, or remove it first."
                )));
            }
            fetched
                .keep(&dir.join(&name).join(&commit))
                .map_err(Refusal::Store)?;
            list.plugins.insert(
                name.clone(),
                Entry {
                    src,
                    version: version.map(str::to_owned),
                    commit,
                    enabled: true,
                },
            );
            Ok(())
        })?;
        self.plugin(&name)
    }

    /// Fetches plugin `name` at the commit `plugins.json` lists, where
    /// it's listed but not on this computer.
    ///
    /// # Errors
    ///
    /// When there's no such plugin, or it can't be fetched.
    pub(crate) async fn install_plugin(&self, name: &str) -> Result<Value, Refusal> {
        let list = self.plugin_list()?;
        let entry = list
            .plugins
            .get(name)
            .filter(|_| valid_name(name))
            .ok_or_else(|| Refusal::NotFound(format!("There's no plugin {name}.")))?;
        let dir = self.plugins_dir();
        let at = dir.join(name).join(&entry.commit);
        if !at.is_dir() {
            let src = fetch::source(&entry.src).map_err(Refusal::Bad)?;
            if !fetch::is_commit(&entry.commit) {
                return Err(Refusal::Bad(format!("{LIST} names no commit for {name}.")));
            }
            let fetched = fetch::fetch(&src, &entry.commit, &dir)
                .await
                .map_err(Refusal::Bad)?;
            usable(fetched.path()).map_err(Refusal::Bad)?;
            fetched.keep(&at).map_err(Refusal::Store)?;
            self.plugins_changed();
        }
        self.plugin(name)
    }

    /// Moves plugin `name` to the newest commit of the version it was
    /// added at; what it asks for then, if it changed, waits again.
    ///
    /// # Errors
    ///
    /// When there's no such plugin, or the newer one can't be fetched or
    /// isn't a plugin.
    pub(crate) async fn update_plugin(&self, name: &str) -> Result<Value, Refusal> {
        let gone = || Refusal::NotFound(format!("There's no plugin {name}."));
        let entry = self
            .plugin_list()?
            .plugins
            .get(name)
            .filter(|_| valid_name(name))
            .cloned()
            .ok_or_else(gone)?;
        let src = fetch::source(&entry.src).map_err(Refusal::Bad)?;
        let commit = fetch::resolve(&src, entry.version.as_deref())
            .await
            .map_err(Refusal::Bad)?;
        let dir = self.plugins_dir();
        if commit == entry.commit && dir.join(name).join(&commit).is_dir() {
            return self.plugin(name);
        }
        let fetched = fetch::fetch(&src, &commit, &dir)
            .await
            .map_err(Refusal::Bad)?;
        usable(fetched.path()).map_err(Refusal::Bad)?;
        // (It may have been removed while the newer one was fetched.)
        let replaced = self.change_plugins(|list| {
            let listed = list.plugins.get_mut(name).ok_or_else(gone)?;
            fetched
                .keep(&dir.join(name).join(&commit))
                .map_err(Refusal::Store)?;
            Ok(std::mem::replace(&mut listed.commit, commit.clone()))
        })?;
        if replaced != commit && fetch::is_commit(&replaced) {
            // The one it replaces goes; a failure leaves a folder nothing
            // reads.
            let _ = std::fs::remove_dir_all(dir.join(name).join(&replaced));
        }
        self.plugin(name)
    }

    /// Allows what plugin `name` asks for now.
    ///
    /// # Errors
    ///
    /// When there's no such plugin, or the store fails.
    pub(crate) fn approve_plugin(&self, name: &str) -> Result<Value, Refusal> {
        let list = self.plugin_list()?;
        let entry = list
            .plugins
            .get(name)
            .ok_or_else(|| Refusal::NotFound(format!("There's no plugin {name}.")))?;
        if let Some(key) = self.see(name, entry).key {
            self.store.put_setting(&format!("{APPROVED}{name}"), &key)?;
            self.plugins_changed();
        }
        self.plugin(name)
    }

    /// Turns plugin `name` on or off.
    ///
    /// # Errors
    ///
    /// When there's no such plugin, or the list can't be written.
    pub(crate) fn enable_plugin(&self, name: &str, enabled: bool) -> Result<Value, Refusal> {
        self.change_plugins(|list| {
            let entry = list
                .plugins
                .get_mut(name)
                .ok_or_else(|| Refusal::NotFound(format!("There's no plugin {name}.")))?;
            entry.enabled = enabled;
            Ok(())
        })?;
        self.plugin(name)
    }

    /// Removes plugin `name`: its listing, its folder and its approval.
    ///
    /// # Errors
    ///
    /// When there's no such plugin, or the list can't be written.
    pub(crate) fn remove_plugin(&self, name: &str) -> Result<(), Refusal> {
        self.change_plugins(|list| match list.plugins.remove(name) {
            Some(_) => Ok(()),
            None => Err(Refusal::NotFound(format!("There's no plugin {name}."))),
        })?;
        if valid_name(name) {
            // A failure leaves a folder nothing reads.
            let _ = std::fs::remove_dir_all(self.plugins_dir().join(name));
        }
        self.store.put_setting(&format!("{APPROVED}{name}"), "")?;
        code::forget(self, name);
        lock(&self.plugins).faults.remove(name);
        Ok(())
    }
}

/// A plugin as clients see it, but for what the server keeps for it.
fn shown(seen: &Seen) -> Value {
    let mut plugin = json!({
        "name": seen.name,
        "src": seen.entry.src,
        "commit": seen.entry.commit,
        "enabled": seen.entry.enabled,
        "state": seen.standing.as_str(),
        "asks": seen.asks,
        "skills": 0, "commands": 0, "agents": 0,
        "mcp": seen.manifest.mcp.keys().collect::<Vec<_>>(),
        "tools": seen.manifest.tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
        "events": seen.manifest.events,
    });
    if let Some(version) = &seen.entry.version {
        plugin["version"] = json!(version);
    }
    if let Some(description) = &seen.manifest.description {
        plugin["description"] = json!(description);
    }
    if let Some(release) = &seen.manifest.version {
        plugin["release"] = json!(release);
    }
    if let Standing::Failed(why) = &seen.standing {
        plugin["error"] = json!(why);
    }
    if seen.root.is_dir() {
        plugin["skills"] = json!(harlo_engine::skills::in_folder(&seen.root).len());
        plugin["commands"] = json!(
            harlo_engine::commands::in_folder(&seen.root).len() + seen.manifest.commands.len()
        );
        plugin["agents"] = json!(harlo_engine::agents::in_folder(&seen.root).len());
    }
    plugin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_folder_safe() {
        assert!(valid_name("review-tools"));
        assert!(valid_name("a1_b.c"));
        for bad in ["", "Review", "-x", "a/b", "..", "a..b", "a b", ".hidden"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        assert!(!valid_name(&"a".repeat(NAME_LENGTH + 1)));
        assert_eq!(
            name_from("https://github.com/Someone/Harlo-Review.git"),
            "harlo-review"
        );
        assert_eq!(name_from("/home/me/my plugin/"), "my-plugin");
    }

    #[test]
    fn a_newer_harlo_is_asked_for_in_words() {
        assert_eq!(too_old(None), None);
        assert_eq!(too_old(Some("0.0.1")), None);
        let why = too_old(Some("v999.0.0")).expect("too old");
        assert!(why.starts_with("It needs Harlo 999.0.0 or newer"), "{why}");
        // A version that isn't one holds nothing back.
        assert_eq!(too_old(Some("soon")), None);
    }

    #[test]
    fn a_plugins_programs_start_in_its_folder() {
        let mcp = json!({
            "local": { "type": "local", "command": ["node", "server.js"] },
            "placed": { "type": "local", "command": ["x"], "cwd": "/elsewhere" },
            "remote": { "type": "remote", "url": "https://example.com/mcp" },
        });
        let servers = servers_in(mcp.as_object().expect("object"), Path::new("/plugins/p/c"));
        assert_eq!(servers["local"]["cwd"], "/plugins/p/c");
        assert_eq!(servers["placed"]["cwd"], "/elsewhere");
        assert!(servers["remote"]["cwd"].is_null());
    }
}
