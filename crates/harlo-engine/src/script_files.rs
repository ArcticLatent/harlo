//! The project's files as a script reaches them.
//!
//! A script run by `execute` calls tools as functions, so a search over
//! many files, or a filter over what they hold, takes one step and leaves
//! only its answer in the conversation. This module gives scripts
//! `tools.files`: `read`, `grep` and `glob`, the engine's own tools for
//! looking, which change nothing.
//!
//! A script can't stop to ask a person. Each call is decided by the
//! turn's rules as the tool itself would be, and runs only where they
//! allow it outright. Where they would ask, or refuse, the call fails and
//! says to use the tool itself, which can ask.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::codemode::{Answer, Call, Catalog, Direct, Namespace, Seen, ToolInfo};
use crate::rules::{Effect, Rule, decide};
use crate::tools::Plan;
use crate::turn::Turn;

/// The namespace scripts reach files through.
const NAMESPACE: &str = "files";

/// The largest file a script reads whole, in bytes: well inside the
/// memory a script may use, and past any source file.
const READ_BYTES: u64 = 4 * 1024 * 1024;

/// The namespace, as scripts are told of it.
#[must_use]
pub(crate) fn namespace() -> Namespace {
    let tool = |name: &str, signature: &str, description: &str| ToolInfo {
        name: name.to_owned(),
        description: description.to_owned(),
        signature: signature.to_owned(),
    };
    Namespace {
        name: NAMESPACE.to_owned(),
        description: "The project's files, to read and search without their text entering \
                      the conversation. Only what the read, grep and glob tools may do \
                      without asking."
            .to_owned(),
        tools: vec![
            tool(
                "read",
                "({path: string, offset?: number, limit?: number}): string",
                "A file's text, without line numbers; offset and limit pick lines.",
            ),
            tool(
                "grep",
                "({pattern: string, path?: string, include?: string, filesOnly?: boolean}): string",
                "Matching lines by file, as the grep tool lists them.",
            ),
            tool(
                "glob",
                "({pattern: string, path?: string}): string[]",
                "The paths of the files matching a name pattern.",
            ),
        ],
    }
}

/// A turn's catalog with the project's files added to it.
#[derive(Debug)]
pub(crate) struct WithFiles<'a> {
    /// The turn's own catalog.
    pub inner: &'a dyn Catalog,
    /// The turn, for its folder and its rules.
    pub turn: &'a Turn,
    /// What stops the turn.
    pub stop: &'a CancellationToken,
}

impl WithFiles<'_> {
    /// Runs file tool `tool` with `args`, where the turn's rules allow it
    /// without asking.
    async fn file_call(&self, tool: &str, args: Value) -> Result<Value, String> {
        if !matches!(tool, "read" | "grep" | "glob") {
            return Err(format!("There's no tool {tool} in tools.{NAMESPACE}."));
        }
        let Value::Object(input) = args else {
            return Err(format!("tools.{NAMESPACE}.{tool} takes an object."));
        };
        let turn = self.turn;
        let plan = Plan::new(tool, &input, &turn.cwd)?
            .sealing(&turn.sealed)?
            .guarded(&turn.rules)
            .for_script();
        self.allowed(tool, &plan)?;
        match plan {
            Plan::Read {
                path,
                shown,
                offset,
                limit,
            } => {
                let size = tokio::fs::metadata(&path).await.map_or(0, |m| m.len());
                if size > READ_BYTES {
                    return Err(format!(
                        "{shown} is {} MB, too large for a script to read whole. Search it \
                         with tools.{NAMESPACE}.grep.",
                        size / (1024 * 1024)
                    ));
                }
                let text = tokio::fs::read_to_string(&path)
                    .await
                    .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
                // All of it, unless the script picked lines.
                if !input.contains_key("offset") && !input.contains_key("limit") {
                    return Ok(json!(text));
                }
                let lines: Vec<&str> = text
                    .lines()
                    .skip(offset.saturating_sub(1))
                    .take(if input.contains_key("limit") {
                        limit
                    } else {
                        usize::MAX
                    })
                    .collect();
                Ok(json!(lines.join("\n")))
            }
            plan => {
                let found = plan.run(&turn.cwd, self.stop).await?.text;
                if tool != "glob" {
                    return Ok(json!(found));
                }
                // The paths alone: the tool's words for "none" and "more"
                // aren't files.
                let paths: Vec<&str> = found
                    .lines()
                    .filter(|l| *l != "No files found." && !l.starts_with('('))
                    .collect();
                Ok(json!(paths))
            }
        }
    }

    /// Whether the turn's rules allow `plan`, a call of `tool`, without
    /// asking; the error says what they wouldn't.
    fn allowed(&self, tool: &str, plan: &Plan) -> Result<(), String> {
        let turn = self.turn;
        let layers: Vec<(&str, &[Rule])> = turn
            .rules
            .iter()
            .map(|(n, r)| (n.as_str(), r.as_slice()))
            .collect();
        let skills: Vec<&std::path::Path> =
            turn.skills.iter().filter_map(|s| s.path.parent()).collect();
        let asked = plan
            .outside(&turn.cwd, &skills)
            .into_iter()
            .chain(plan.reads(&turn.cwd))
            .chain([plan.permission()]);
        for permission in asked {
            for resource in &permission.resources {
                if decide(&layers, &permission.action, resource).effect != Effect::Allow {
                    return Err(format!(
                        "A script may not {} {resource}: the rules ask the person first, or \
                         refuse. Call the {tool} tool itself for it.",
                        permission.action
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Catalog for WithFiles<'_> {
    fn namespaces(&self) -> Vec<Namespace> {
        let mut all = vec![namespace()];
        all.extend(self.inner.namespaces());
        all
    }

    fn direct(&self) -> Vec<Direct> {
        self.inner.direct()
    }

    fn call<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Answer<'a> {
        if namespace == NAMESPACE {
            return Box::pin(self.file_call(tool, args));
        }
        self.inner.call(namespace, tool, args)
    }

    fn call_seen<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Seen<'a> {
        self.inner.call_seen(namespace, tool, args)
    }

    fn progress(&self, message: &str, call: &str, calls: &[Call]) {
        self.inner.progress(message, call, calls);
    }
}
