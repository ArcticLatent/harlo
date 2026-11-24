//! Commands: prompt templates a person runs by name, such as `/review`.
//!
//! They come from the built-ins, the config's `commands`, and Markdown
//! files in `commands` folders: the config folder's, then the project's
//! `.opencode/commands` from its root down to the session's folder. A file's
//! front matter sets its description, agent and model, its body the
//! template; `group/name.md` is the command `group/name`. A later source
//! replaces an earlier command of the same name.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::ModelRef;

/// A command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// What it's run as, without the `/`.
    pub name: String,
    /// What it does, for lists.
    pub description: Option<String>,
    /// The prompt it becomes, with `$ARGUMENTS`, `$1`… and `` !`…` ``.
    pub template: String,
    /// The agent it switches the session to.
    pub agent: Option<String>,
    /// The model it switches the session to.
    pub model: Option<ModelRef>,
}

impl Command {
    /// As OpenCode lists it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut out = json!({ "name": self.name });
        if let Some(description) = &self.description {
            out["description"] = json!(description);
        }
        out
    }
}

/// The built-in commands.
#[must_use]
pub fn builtins() -> Vec<Command> {
    let command = |name: &str, description: &str, template: &str| Command {
        name: name.to_owned(),
        description: Some(description.to_owned()),
        template: template.to_owned(),
        agent: None,
        model: None,
    };
    vec![
        command(
            "init",
            "write or update the project's AGENTS.md",
            "Look through this project and write an AGENTS.md at its root for coding agents \
             that work in it, or bring the existing one up to date. Cover how to build, test \
             and lint it (with the commands for a single test), how the code is laid out, and \
             the conventions that aren't obvious from the code. Keep it short: what an agent \
             needs and would otherwise get wrong. Include rules from existing agent instruction \
             files you find.\n\n$ARGUMENTS",
        ),
        command(
            "review",
            "review changes [commit|branch|pr], defaults to uncommitted",
            "Review these changes: $ARGUMENTS\n\nWith nothing named, review the uncommitted \
             changes (`git diff` and `git diff --cached`, and new files). A commit hash means \
             that commit; a branch name means its changes against the default branch; a pull \
             request number or link means that pull request (with `gh`).\n\nRead the code \
             around each change before judging it. Report real problems first, most serious \
             first: bugs, wrong behaviour on edge cases, security issues, broken conventions. \
             Give the file and line for each, and say why it's wrong. Leave out matters of \
             taste. Change no files.",
        ),
    ]
}

/// Where commands are looked for.
#[derive(Debug, Clone, Copy)]
pub struct Search<'a> {
    /// The session's folder.
    pub workspace: &'a Path,
    /// The project's root, if known.
    pub project_root: Option<&'a Path>,
    /// OpenCode's config folder.
    pub config_dir: &'a Path,
    /// The config's `commands` object.
    pub configured: &'a Value,
}

/// The commands, by name.
#[must_use]
pub fn discover(search: &Search<'_>) -> Vec<Command> {
    let mut found: BTreeMap<String, Command> = builtins()
        .into_iter()
        .map(|c| (c.name.clone(), c))
        .collect();
    for (name, entry) in search.configured.as_object().into_iter().flatten() {
        let Some(template) = entry["template"].as_str() else {
            continue;
        };
        found.insert(
            name.clone(),
            Command {
                name: name.clone(),
                description: entry["description"].as_str().map(str::to_owned),
                template: template.to_owned(),
                agent: entry["agent"].as_str().map(str::to_owned),
                model: crate::config::model_ref(&entry["model"]),
            },
        );
    }
    let mut folders = vec![search.config_dir.to_path_buf()];
    let mut project = vec![search.workspace.to_path_buf()];
    if let Some(root) = search.project_root {
        let mut at = search.workspace;
        while let Some(up) = at.parent().filter(|up| up.starts_with(root)) {
            project.push(up.to_path_buf());
            at = up;
        }
    }
    folders.extend(project.into_iter().rev().map(|f| f.join(".opencode")));
    for folder in folders {
        // The singular folder is the older name.
        for kind in ["command", "commands"] {
            let root = folder.join(kind);
            for file in markdown_files(&root) {
                if let Some(command) = read(&root, &file) {
                    found.insert(command.name.clone(), command);
                }
            }
        }
    }
    found.into_values().collect()
}

/// The Markdown files under `root`, sorted.
fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .hidden(true)
        .build()
        .flatten()
        .map(ignore::DirEntry::into_path)
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();
    files
}

#[derive(Debug, Default, Deserialize)]
struct Front {
    description: Option<String>,
    agent: Option<String>,
    model: Option<String>,
}

/// Reads the command in `file` under `root`.
fn read(root: &Path, file: &Path) -> Option<Command> {
    let source = std::fs::read_to_string(file).ok()?;
    let relative = file.strip_prefix(root).ok()?.with_extension("");
    let name = relative
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?
        .join("/");
    let parsed = Matter::<YAML>::new().parse::<Front>(&source).ok();
    let (front, body) = match parsed {
        Some(p) => (p.data.unwrap_or_default(), p.content),
        None => (Front::default(), source),
    };
    Some(Command {
        name,
        description: front.description,
        template: body.trim().to_owned(),
        agent: front.agent,
        model: front
            .model
            .as_deref()
            .and_then(|m| crate::config::model_ref(&json!(m))),
    })
}

/// `template` with `arguments` in place of `$ARGUMENTS` and `$1`, `$2`….
///
/// Quotes group words, the highest-numbered placeholder takes the rest,
/// and missing ones are empty. With no placeholder, the arguments follow
/// after a blank line.
#[must_use]
pub fn fill(template: &str, arguments: &str) -> String {
    let arguments = arguments.trim();
    let words = shell_words::split(arguments)
        .unwrap_or_else(|_| arguments.split_whitespace().map(str::to_owned).collect());
    let highest = (1..=9)
        .rev()
        .find(|n| template.contains(&format!("${n}")))
        .unwrap_or(0);
    let has_all = template.contains("$ARGUMENTS");
    if highest == 0 && !has_all {
        return if arguments.is_empty() {
            template.to_owned()
        } else {
            format!("{template}\n\n{arguments}")
        };
    }
    let mut out = template.replace("$ARGUMENTS", arguments);
    for n in (1..=highest).rev() {
        let value = if n == highest {
            words
                .get(n - 1..)
                .map(|rest| rest.join(" "))
                .unwrap_or_default()
        } else {
            words.get(n - 1).cloned().unwrap_or_default()
        };
        out = out.replace(&format!("${n}"), &value);
    }
    out
}

/// How long a template's command may run.
const SHELL_TIMEOUT: Duration = Duration::from_secs(30);

/// `text` with each `` !`command` `` replaced by what the command printed,
/// run in `cwd`.
pub async fn run_shell_blocks(text: &str, cwd: &Path) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("!`") {
        let Some(len) = rest[at + 2..].find('`') else {
            break;
        };
        out.push_str(&rest[..at]);
        let command = &rest[at + 2..at + 2 + len];
        out.push_str(shell_output(command, cwd).await.trim_end());
        rest = &rest[at + 2 + len + 1..];
    }
    out.push_str(rest);
    out
}

async fn shell_output(command: &str, cwd: &Path) -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let run = tokio::process::Command::new(shell)
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(SHELL_TIMEOUT, run).await {
        Ok(Ok(output)) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            text
        }
        Ok(Err(e)) => format!("(couldn't run `{command}`: {e})"),
        Err(_) => {
            let mut text = String::new();
            let _ = write!(text, "(`{command}` took longer than {SHELL_TIMEOUT:?})");
            text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_fill_their_places() {
        assert_eq!(fill("Hi $ARGUMENTS.", "Bob Ann"), "Hi Bob Ann.");
        assert_eq!(
            fill("Compare $1 with $2.", "api stable branch"),
            "Compare api with stable branch."
        );
        assert_eq!(fill("Ask $1 about $2", "\"Ann Lee\""), "Ask Ann Lee about ");
        assert_eq!(fill("Plain.", "extra words"), "Plain.\n\nextra words");
        assert_eq!(fill("Plain.", ""), "Plain.");
    }

    #[tokio::test]
    async fn shell_blocks_are_replaced_by_their_output() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("f.txt"), "inside\n").expect("f");
        let text = run_shell_blocks("See: !`cat f.txt` done", dir.path()).await;
        assert_eq!(text, "See: inside done");
    }

    #[test]
    fn files_and_config_add_commands() {
        let dir = tempfile::tempdir().expect("dir");
        let project = dir.path().join("p");
        let folder = project.join(".opencode/commands/team");
        std::fs::create_dir_all(&folder).expect("mkdir");
        std::fs::write(
            folder.join("ship.md"),
            "---\ndescription: Ships it\nagent: plan\n---\nShip $1.\n",
        )
        .expect("write");
        let configured = json!({ "review": { "template": "Mine: $ARGUMENTS" } });
        let commands = discover(&Search {
            workspace: &project,
            project_root: Some(&project),
            config_dir: &dir.path().join("config"),
            configured: &configured,
        });
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["init", "review", "team/ship"]);
        assert_eq!(
            commands[1].template, "Mine: $ARGUMENTS",
            "config replaces a built-in"
        );
        assert_eq!(commands[2].agent.as_deref(), Some("plan"));
        assert_eq!(commands[2].template, "Ship $1.");
    }
}
