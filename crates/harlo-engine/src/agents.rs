//! The agents a session can run as: Build and Plan to talk to, General and
//! Explore for subagents, each with its own permission rules.
//!
//! Plan changes no files but its plans; switching to it and away from it
//! tells the model so, at the next safe point. The configuration can add
//! agents, or change the built-in ones: entries under `agents`, and
//! Markdown files in the config folders' `agents` folders, their front
//! matter holding the settings and their body the agent's instructions.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde_json::Value;

use crate::config::{ModelRef, model_ref};
use crate::rules::{Effect, Rule, agent_rules};

/// Whom an agent talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A person, in a session.
    Primary,
    /// Another agent, which hands it a task.
    Subagent,
    /// Either.
    All,
}

impl Mode {
    /// OpenCode's word for it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Subagent => "subagent",
            Self::All => "all",
        }
    }

    /// The mode OpenCode's word names.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "primary" => Some(Self::Primary),
            "subagent" => Some(Self::Subagent),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    /// Whether a session can run as it.
    #[must_use]
    pub fn talks_to_people(self) -> bool {
        self != Self::Subagent
    }

    /// Whether other agents can hand it a task.
    #[must_use]
    pub fn takes_tasks(self) -> bool {
        self != Self::Primary
    }
}

/// An agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    /// Its ID, such as `build`.
    pub id: String,
    /// The name shown for it.
    pub name: String,
    /// What it's for, for people and for agents choosing a subagent.
    pub description: String,
    /// Whom it talks to.
    pub mode: Mode,
    /// Its permission rules, applied before the configuration's.
    pub rules: Vec<Rule>,
    /// Its own instructions, put before the project's.
    pub system: Option<String>,
    /// What the model is told when a session switches to it.
    pub entered: Option<String>,
    /// What the model is told when a session switches away from it.
    pub left: Option<String>,
    /// Its colour, `#rrggbb`.
    pub color: Option<String>,
    /// The model it always uses, instead of the session's.
    pub model: Option<ModelRef>,
    /// Whether lists leave it out.
    pub hidden: bool,
}

/// The agent a session runs as when none was chosen.
pub const DEFAULT: &str = "build";

fn rule(action: &str, resource: &str, effect: Effect) -> Rule {
    Rule {
        action: action.to_owned(),
        resource: resource.to_owned(),
        effect,
    }
}

/// The built-in agents. Plan may write plans in `plans` only.
#[must_use]
pub fn builtins(plans: &Path) -> Vec<Agent> {
    let base = agent_rules();
    let plans = plans.to_string_lossy();
    let with = |extra: Vec<Rule>| {
        let mut rules = base.clone();
        rules.extend(extra);
        rules
    };
    vec![
        Agent {
            id: "build".into(),
            name: "Build".into(),
            description: "The default agent. Runs tools as the permission rules allow.".into(),
            mode: Mode::Primary,
            rules: with(vec![rule("question", "*", Effect::Allow)]),
            system: None,
            entered: None,
            left: None,
            color: None,
            model: None,
            hidden: false,
        },
        Agent {
            id: "plan".into(),
            name: "Plan".into(),
            description: "Reads and plans; changes no files.".into(),
            mode: Mode::Primary,
            rules: with(vec![
                rule("question", "*", Effect::Allow),
                rule("edit", "*", Effect::Deny),
                rule("edit", &format!("{plans}/*"), Effect::Allow),
                rule("subagent", "*", Effect::Deny),
            ]),
            system: None,
            entered: Some(format!(
                "<system-update>\nYou are in Plan mode. Work out the plan with the person here in the \
                 conversation. Change no files, and ask no subagent to. Write a plan file only when \
                 the person asks for one, and only in:\n{plans}\n\nYou stay in Plan mode until the \
                 person switches agents. If they ask you to make the changes, tell them to switch \
                 agents first.\n</system-update>"
            )),
            left: Some(
                "<system-update>\nYou are no longer in Plan mode. Its limits, and any Plan mode \
                 instructions earlier in this conversation, no longer apply.\n</system-update>"
                    .into(),
            ),
            color: None,
            model: None,
            hidden: false,
        },
        Agent {
            id: "general".into(),
            name: "General".into(),
            description: "Researches questions and carries out tasks of several steps. Give it \
                          independent pieces of work to run side by side."
                .into(),
            mode: Mode::Subagent,
            rules: with(vec![
                rule("question", "*", Effect::Deny),
                rule("subagent", "*", Effect::Deny),
            ]),
            system: None,
            entered: None,
            left: None,
            color: None,
            model: None,
            hidden: false,
        },
        Agent {
            id: "explore".into(),
            name: "Explore".into(),
            description: "Finds things in the code quickly: files by pattern, code by keyword, \
                          or how something works. Say how thorough to be: quick, medium or very \
                          thorough."
                .into(),
            mode: Mode::Subagent,
            rules: with(vec![
                rule("*", "*", Effect::Deny),
                rule("read", "*", Effect::Allow),
                rule("glob", "*", Effect::Allow),
                rule("grep", "*", Effect::Allow),
                rule("read", "*.env", Effect::Ask),
                rule("read", "*.env.*", Effect::Ask),
                rule("read", "*.env.example", Effect::Allow),
                rule("shell", "*", Effect::Ask),
            ]),
            system: Some(
                "You search and explain code; you never change it. Create, edit, move or delete \
                 no files, temporary ones included, and run only commands that change nothing. \
                 Search in parallel where you can, go as deep as you were asked to, and give file \
                 paths in full in your answer."
                    .into(),
            ),
            entered: None,
            left: None,
            color: None,
            model: None,
            hidden: false,
        },
    ]
}

/// `agents` with the configuration's agents applied.
///
/// They come from the merged config's `agents` entries, then the Markdown
/// files in each of `folders`' `agents` (or `agent`) folder, later ones
/// winning. An entry for a known agent changes the settings it names;
/// `disabled: true` removes it.
#[must_use]
pub fn configured(mut agents: Vec<Agent>, entries: &Value, folders: &[PathBuf]) -> Vec<Agent> {
    let mut found: Vec<(String, Value)> = entries
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, entry)| (id.clone(), entry.clone()))
        .collect();
    for folder in folders {
        for kind in ["agent", "agents"] {
            let Ok(dir) = std::fs::read_dir(folder.join(kind)) else {
                continue;
            };
            let mut files: Vec<PathBuf> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "md"))
                .collect();
            files.sort();
            found.extend(files.iter().filter_map(|f| from_file(f)));
        }
    }
    for (id, entry) in found {
        if entry["disabled"] == true {
            agents.retain(|a| a.id != id);
            continue;
        }
        if let Some(agent) = agents.iter_mut().find(|a| a.id == id) {
            apply(agent, &entry);
        } else {
            let mut agent = Agent {
                name: id.clone(),
                id,
                description: String::new(),
                mode: Mode::All,
                rules: agent_rules(),
                system: None,
                entered: None,
                left: None,
                color: None,
                model: None,
                hidden: false,
            };
            apply(&mut agent, &entry);
            agents.push(agent);
        }
    }
    agents
}

/// The agent in Markdown file `file`: its ID, and its settings with the
/// body as `system`.
fn from_file(file: &Path) -> Option<(String, Value)> {
    let id = file.file_stem()?.to_str()?.to_owned();
    let source = std::fs::read_to_string(file).ok()?;
    let parsed = Matter::<YAML>::new().parse::<Value>(&source).ok()?;
    let mut entry = parsed
        .data
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let body = parsed.content.trim();
    if !body.is_empty() {
        entry["system"] = Value::String(body.to_owned());
    }
    Some((id, entry))
}

/// Sets what `entry` names on `agent`.
fn apply(agent: &mut Agent, entry: &Value) {
    let text = |key: &str| entry[key].as_str().map(str::to_owned);
    if let Some(name) = text("name") {
        agent.name = name;
    }
    if let Some(description) = text("description") {
        agent.description = description;
    }
    if let Some(mode) = entry["mode"].as_str().and_then(Mode::parse) {
        agent.mode = mode;
    }
    if let Some(system) = text("system") {
        agent.system = Some(system);
    }
    if let Some(color) = text("color").filter(|c| {
        c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|d| d.is_ascii_hexdigit())
    }) {
        agent.color = Some(color);
    }
    if let Some(model) = model_ref(&entry["model"]) {
        agent.model = Some(model);
    }
    if let Some(hidden) = entry["hidden"].as_bool() {
        agent.hidden = hidden;
    }
    agent.rules.extend(
        entry["permissions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| serde_json::from_value::<Rule>(r.clone()).ok()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::decide;

    #[test]
    fn plan_writes_only_plans_and_explore_only_reads() {
        let agents = builtins(Path::new("/data/plans"));
        let effect = |id: &str, action: &str, resource: &str| {
            let agent = agents.iter().find(|a| a.id == id).expect("agent");
            decide(&[("agent", &agent.rules)], action, resource).effect
        };
        assert_eq!(effect("build", "edit", "a.txt"), Effect::Allow);
        assert_eq!(effect("plan", "edit", "a.txt"), Effect::Deny);
        assert_eq!(effect("plan", "edit", "/data/plans/next.md"), Effect::Allow);
        assert_eq!(effect("plan", "shell", "ls"), Effect::Ask);
        assert_eq!(effect("explore", "edit", "a.txt"), Effect::Deny);
        assert_eq!(effect("explore", "grep", "fn main"), Effect::Allow);
        assert_eq!(effect("explore", "read", ".env"), Effect::Ask);
        assert_eq!(effect("general", "question", "Which?"), Effect::Deny);
    }

    #[test]
    fn the_configuration_adds_and_changes_agents() {
        let dir = tempfile::tempdir().expect("dir");
        let folder = dir.path().join(".opencode");
        std::fs::create_dir_all(folder.join("agents")).expect("mkdir");
        std::fs::write(
            folder.join("agents/reviewer.md"),
            "---\ndescription: Reviews changes\nmode: all\ncolor: \"#ff6b6b\"\npermissions:\n  \
             - action: edit\n    resource: \"*\"\n    effect: deny\n---\nReview the changes.\n",
        )
        .expect("write");
        let entries = serde_json::json!({
            "explore": { "disabled": true },
            "build": { "color": "#00ff00" },
        });
        let agents = configured(builtins(Path::new("/p")), &entries, &[folder]);
        let ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["build", "plan", "general", "reviewer"]);
        assert_eq!(agents[0].color.as_deref(), Some("#00ff00"));
        let reviewer = &agents[3];
        assert_eq!(reviewer.mode, Mode::All);
        assert_eq!(reviewer.system.as_deref(), Some("Review the changes."));
        let edit = decide(&[("agent", &reviewer.rules)], "edit", "a.txt").effect;
        assert_eq!(edit, Effect::Deny);
    }
}
