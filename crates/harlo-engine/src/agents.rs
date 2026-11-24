//! The agents a session can run as: Build and Plan to talk to, General and
//! Explore for subagents, each with its own permission rules.
//!
//! Plan changes no files but its plans; switching to it and away from it
//! tells the model so, at the next safe point.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use crate::rules::{Effect, Rule, agent_rules};

/// Whom an agent talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A person, in a session.
    Primary,
    /// Another agent, which hands it a task.
    Subagent,
}

impl Mode {
    /// OpenCode's word for it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Subagent => "subagent",
        }
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
        },
    ]
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
}
