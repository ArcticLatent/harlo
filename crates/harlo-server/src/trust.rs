//! What a project's own config files may change before a person says so.
//!
//! A project's `harlo.jsonc` and `.harlo` folder come with the project: a
//! checkout someone else wrote brings its own. Most of what they set is
//! harmless (a model, instructions, an agent's prompt, a command's text),
//! but some of it acts for the person: rules that let actions run without
//! asking, an agent's included, MCP servers (programs started on the
//! spot), and commands that run shell lines when used. Those wait until the person
//! allows them, asked as an approval of [`ACTION`]; "always" lasts until
//! what was allowed changes.
//!
//! Where requests to a model go and with which key is never a project's to
//! say: its `providers` are left out, allowed or not.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_engine::Ask;
use rusqlite::OptionalExtension as _;
use serde_json::{Map, Value};
use sha2::Digest as _;

use crate::server::{Server, lock};
use crate::store::{Error, Store};

/// The action a project's own settings are approved under.
pub(crate) const ACTION: &str = "project_config";

/// How many characters of the settings' fingerprint the approval names:
/// enough that two sets of settings don't share it by accident.
const KEY_LENGTH: usize = 16;

/// What a project's own config asks for that waits for the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Withheld {
    /// What an approval of it is saved as: the project's folder and a
    /// fingerprint of what waits, so a change asks again.
    pub key: String,
    /// What waits, a line each, for the person to read.
    pub wants: Vec<String>,
    /// The commands whose shell lines wait, by name.
    pub commands: Vec<String>,
}

/// Collects what waits in a project's config.
#[derive(Debug, Default)]
pub(crate) struct Held {
    wants: Vec<String>,
    commands: Vec<String>,
    hash: sha2::Sha256,
}

impl Held {
    /// Notes one thing that waits: `line` for the person, `exact` for the
    /// fingerprint.
    fn note(&mut self, line: String, exact: &[u8]) {
        self.hash.update(line.as_bytes());
        self.hash.update([0]);
        self.hash.update(exact);
        self.hash.update([0]);
        self.wants.push(line);
    }

    /// Takes what waits out of project config document `info`, leaving
    /// what a project may set by itself.
    pub fn take_from(&mut self, info: &mut Value) {
        let Some(document) = info.as_object_mut() else {
            return;
        };
        if let Some(Value::Array(rules)) = document.get_mut("permissions") {
            rules.retain(|rule| {
                if rule["effect"] != "allow" {
                    return true;
                }
                let line = format!(
                    "Run {} on “{}” without asking",
                    rule["action"].as_str().unwrap_or("*"),
                    rule["resource"].as_str().unwrap_or("*")
                );
                self.note(line, rule.to_string().as_bytes());
                false
            });
        }
        if let Some(mcp) = document.remove("mcp") {
            for (name, spec) in mcp["servers"].as_object().into_iter().flatten() {
                let line = match spec["url"].as_str() {
                    Some(url) => format!("Connect to the MCP server “{name}” at {url}"),
                    None => format!("Start the MCP server “{name}”: {}", words(&spec["command"])),
                };
                self.note(line, spec.to_string().as_bytes());
            }
        }
        if let Some(Value::Object(agents)) = document.get_mut("agents") {
            for (id, entry) in agents {
                self.take_agent(id, entry);
            }
        }
        for key in ["instructions", "skills"] {
            if let Some(Value::Array(entries)) = document.get_mut(key) {
                entries.retain(|entry| {
                    let Some(path) = entry.as_str().filter(|p| reaches_out(p)) else {
                        return true;
                    };
                    self.note(format!("Read {key} from {path}"), path.as_bytes());
                    false
                });
            }
        }
        // A command stays, as text; its shell lines wait.
        for (name, entry) in document
            .get("commands")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            self.note_command(name, entry["template"].as_str().unwrap_or_default());
        }
    }

    /// Takes out of agent `id`'s `entry` the rules that let it act without
    /// asking; its instructions and the rules that hold it back stay.
    pub fn take_agent(&mut self, id: &str, entry: &mut Value) {
        let Some(Value::Array(rules)) = entry.get_mut("permissions") else {
            return;
        };
        rules.retain(|rule| {
            if rule["effect"] != "allow" {
                return true;
            }
            let line = format!(
                "Let the agent “{id}” run {} on “{}” without asking",
                rule["action"].as_str().unwrap_or("*"),
                rule["resource"].as_str().unwrap_or("*")
            );
            self.note(line, rule.to_string().as_bytes());
            false
        });
    }

    /// Notes what waits in project config folder `folder`: its agents'
    /// rules that let them act unasked, and its commands' shell lines.
    pub fn take_folder(&mut self, folder: &Path) {
        for (id, mut entry) in harlo_engine::agents::in_folder(folder) {
            self.take_agent(&id, &mut entry);
        }
        for command in harlo_engine::commands::in_folder(folder) {
            self.note_command(&command.name, &command.template);
        }
    }

    /// Notes the shell lines command `name`'s `template` runs when used.
    fn note_command(&mut self, name: &str, template: &str) {
        let lines = harlo_engine::commands::shell_blocks(template);
        if lines.is_empty() {
            return;
        }
        for line in lines {
            self.note(
                format!("Run `{line}` when you use /{name}"),
                line.as_bytes(),
            );
        }
        self.commands.push(name.to_owned());
    }

    /// What waits for the project in `root`; none when nothing does.
    pub fn finish(self, root: &Path) -> Option<Withheld> {
        if self.wants.is_empty() {
            return None;
        }
        let mut hash = self.hash;
        hash.update(root.as_os_str().as_encoded_bytes());
        let print = hex::encode(hash.finalize());
        Some(Withheld {
            key: format!("{} @ {}", root.display(), &print[..KEY_LENGTH]),
            wants: self.wants,
            commands: self.commands,
        })
    }
}

/// Takes out of project config document `info` what no project may set:
/// where a model's requests go, and with which key.
pub(crate) fn drop_providers(info: &mut Value, file: &Path) {
    let dropped = info.as_object_mut().and_then(|d| d.remove("providers"));
    if dropped.is_some() {
        tracing::warn!(
            name: "config.project.providers.ignored",
            path = %file.display(),
            "providers are set in Harlo's own config, not in {{path}}"
        );
    }
}

/// Whether instruction or skill `entry` is read from outside the project.
fn reaches_out(entry: &str) -> bool {
    entry.starts_with('~')
        || entry.contains("://")
        || Path::new(entry).is_absolute()
        || Path::new(entry)
            .components()
            .any(|c| c == std::path::Component::ParentDir)
}

/// A command's words, as one line.
fn words(command: &Value) -> String {
    command
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

impl Store {
    /// Whether an approval of `action` on exactly `resource` is saved, in
    /// any project.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub(crate) fn approved(&self, action: &str, resource: &str) -> Result<bool, Error> {
        self.conn()
            .query_row(
                "SELECT 1 FROM approval WHERE action = ?1 AND resource = ?2 LIMIT 1",
                [action, resource],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
    }
}

impl Server {
    /// Whether the person allowed the project settings saved as `key`:
    /// for now, or always.
    pub(crate) fn trusted(&self, key: &str) -> bool {
        if let Some(answer) = lock(&self.trust).get(key) {
            return *answer;
        }
        self.store.approved(ACTION, key).unwrap_or(false)
    }

    /// Asks the person, in session `session`, whether the project's own
    /// settings in `held` apply; an answer already given stands.
    ///
    /// "Once" and a refusal last until the server restarts; "always" is
    /// saved with the project's approvals. Returns whether they apply.
    pub(crate) async fn ask_trust(
        self: &std::sync::Arc<Self>,
        session: &str,
        directory: &str,
        held: &Withheld,
        stop: &tokio_util::sync::CancellationToken,
    ) -> bool {
        use harlo_engine::{Answer, Gate as _};
        if lock(&self.trust).contains_key(&held.key) {
            return self.trusted(&held.key);
        }
        let ask = Ask {
            id: harlo_engine::ascending_id("per"),
            message: String::new(),
            call: String::new(),
            action: ACTION.to_owned(),
            resources: held.wants.clone(),
            save: vec![held.key.clone()],
            metadata: Map::new(),
            why: "This project's own config files ask for these. Until you allow them, they \
                  stay off and the session carries on without them."
                .to_owned(),
        };
        let hooks = crate::server::Hooks::new(self, session, directory);
        let answer = tokio::select! {
            a = hooks.ask(ask) => a,
            () = stop.cancelled() => return false,
        };
        let allowed = matches!(answer, Answer::Once | Answer::Always);
        lock(&self.trust).insert(held.key.clone(), allowed);
        allowed
    }
}
