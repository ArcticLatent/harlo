//! What a folder's OpenCode configuration sets up beyond what the API lists:
//! which agent each command runs, and whether the todo tool is on.
//!
//! `/api/command` names commands but not how they run. The configuration
//! does: a `commands` map in each configuration file, and a Markdown file
//! with front matter per command in each configuration folder's `command/`
//! or `commands/`. `/api/config` lists those files and folders in the order
//! OpenCode merges them, later ones winning.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::Path;

use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::Deserialize;
use serde_json::Value;

/// The folders under a configuration folder that hold command files.
const COMMAND_FOLDERS: [&str; 2] = ["command", "commands"];

/// How deep command files can nest, as `group/name.md`, before a search stops.
const MAX_DEPTH: usize = 8;

/// What a folder's configuration sets up for its commands and tools.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Setup {
    /// Whether the agent has the todo tool (`experimental.todo`).
    pub todo_tool: bool,
    /// How each command runs, by name.
    pub commands: HashMap<String, CommandSetup>,
}

/// How a command runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandSetup {
    /// The agent that runs it, when it names one.
    pub agent: Option<String>,
    /// Whether it runs as a subagent, in a child session of its own.
    pub subagent: bool,
}

/// A command's settings, as a configuration file or front matter writes them.
#[derive(Debug, Default, Deserialize)]
struct Written {
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    subagent: Option<bool>,
    /// The deprecated name for `subagent`.
    #[serde(default)]
    subtask: Option<bool>,
}

impl From<Written> for CommandSetup {
    fn from(written: Written) -> Self {
        Self {
            agent: written.agent.filter(|a| !a.trim().is_empty()),
            subagent: written.subagent.or(written.subtask).unwrap_or(false),
        }
    }
}

impl Setup {
    /// Builds the setup from `/api/config`'s entries, reading command files
    /// from the folders it lists.
    ///
    /// Files that can't be read or parsed are left out: OpenCode reports
    /// those itself, and the command still runs as OpenCode decides.
    pub(crate) fn from_entries(entries: &Value) -> Self {
        let mut setup = Self::default();
        for entry in entries.as_array().into_iter().flatten() {
            match entry["type"].as_str() {
                Some("document") => setup.add_document(&entry["info"]),
                Some("directory") => {
                    if let Some(path) = entry["path"].as_str() {
                        setup.add_folder(Path::new(path));
                    }
                }
                _ => {}
            }
        }
        setup
    }

    fn add_document(&mut self, info: &Value) {
        if let Some(todo) = info["experimental"]["todo"].as_bool() {
            self.todo_tool = todo;
        }
        for (name, command) in info["commands"].as_object().into_iter().flatten() {
            if let Ok(written) = Written::deserialize(command) {
                self.commands.insert(name.clone(), written.into());
            }
        }
    }

    fn add_folder(&mut self, folder: &Path) {
        for sub in COMMAND_FOLDERS {
            let root = folder.join(sub);
            let mut files = Vec::new();
            markdown_files(&root, 0, &mut files);
            files.sort();
            for file in files {
                let Some(name) = command_name(&root, &file) else {
                    continue;
                };
                if let Some(setup) = front_matter(&file) {
                    self.commands.insert(name, setup);
                }
            }
        }
    }
}

/// Collects the `.md` files under `dir`, searching at most [`MAX_DEPTH`] deep.
fn markdown_files(dir: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && depth < MAX_DEPTH {
            markdown_files(&path, depth + 1, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// The name a command file gives its command: its path under `root`
/// without `.md`, such as `group/name`.
fn command_name(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?.with_extension("");
    let parts: Vec<&str> = relative
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<_>>()?;
    Some(parts.join("/"))
}

/// How a command file's front matter says it runs.
fn front_matter(file: &Path) -> Option<CommandSetup> {
    let source = std::fs::read_to_string(file).ok()?;
    let parsed = Matter::<YAML>::new().parse::<Written>(&source).ok()?;
    Some(parsed.data.unwrap_or_default().into())
}
