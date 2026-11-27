//! The session's own tools as a script calls them.
//!
//! A script run by `execute` calls tools as functions, so a search over
//! many files, a filter over what they hold, or the same change in many
//! places takes one step and leaves only the script's answer in the
//! conversation. This module gives scripts `tools.project`: `read`,
//! `grep`, `glob`, `edit`, `write` and `shell`, the engine's own tools.
//!
//! A call from a script is the same call the model would make itself. The
//! turn's rules decide it, with the same sealed folders, the same care for
//! paths outside the project and the same check that a file wasn't changed
//! behind the session's back. Where the rules say to ask, the person is
//! asked and the script waits, with the change shown as it would be for
//! the tool. A refusal fails that call in the script; a refusal without a
//! reason stops the script and the turn, as it does for any call. The
//! files a script changed are reported with its step.
//!
//! The same holds for the tools a script reaches outside the session: an
//! MCP server's, a plugin's, the server's own. Each such call is decided
//! under the name the tool has when the model is offered it on its own,
//! `<namespace>_<tool>`, so a rule that refuses a tool or asks about it
//! holds whether the model calls it or a script does.
//!
//! Two things differ, because a script reads its results where a model
//! would: `read` gives a file's text as it is, without line numbers and
//! not cut at the lines a model is shown, and searches list far more.

// Rust guideline compliant 2026-10-09

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::codemode::{Answer, Call, Catalog, Direct, Namespace, Seen, SessionTools, ToolInfo};
use crate::turn::{CallOf, Gate, ScriptCall, Turn};

/// The namespace scripts reach the session's tools through.
const NAMESPACE: &str = "project";

/// The tools scripts are given: its name, how a script calls it, and what
/// it returns.
const TOOLS: &[(&str, &str, &str)] = &[
    (
        "read",
        "({path: string, offset?: number, limit?: number}): string",
        "A file's text as it is, without line numbers; offset and limit pick lines.",
    ),
    (
        "grep",
        "({pattern: string, path?: string, include?: string, filesOnly?: boolean}): string",
        "Matching lines by file, as the grep tool lists them.",
    ),
    (
        "glob",
        "({pattern: string, path?: string}): string[]",
        "The paths of the files matching a name pattern.",
    ),
    (
        "edit",
        "({path: string, oldString: string, newString: string, replaceAll?: boolean}): string",
        "Replaces text in a file, as the edit tool does.",
    ),
    (
        "write",
        "({path: string, content: string}): string",
        "Writes a whole file.",
    ),
    (
        "shell",
        "({command: string, timeout?: number, background?: boolean}): string",
        "Runs a command in the project's folder; returns what it printed.",
    ),
];

/// The namespace as scripts of `turn` are told of it: the tools its rules
/// don't refuse outright; `None` when they refuse them all.
#[must_use]
pub(crate) fn namespace(turn: &Turn) -> Option<Namespace> {
    let tools: Vec<ToolInfo> = TOOLS
        .iter()
        .filter(|(name, _, _)| crate::turn::permitted(turn, name))
        .map(|(name, signature, description)| ToolInfo {
            name: (*name).to_owned(),
            description: (*description).to_owned(),
            signature: (*signature).to_owned(),
        })
        .collect();
    if tools.is_empty() {
        return None;
    }
    Some(Namespace {
        name: NAMESPACE.to_owned(),
        description: "Your own tools, for a script: search or read many files, filter what \
                      they hold, or make the same change in many places in one step, and \
                      return only what matters. Each call follows the rules of the tool of \
                      that name, and may ask the person."
            .to_owned(),
        tools,
    })
}

/// A turn's catalog with the session's own tools added to it, for one
/// script.
pub(crate) struct WithProject<'a, G: Gate> {
    /// The turn's own catalog.
    inner: &'a dyn Catalog,
    /// The namespace as the script itself has it: what this turn's rules
    /// leave, where scripts are given the session's tools at all.
    listed: Option<Namespace>,
    /// The namespace as this turn's rules leave it, for code the catalog
    /// runs for the script, which is given the tools another way.
    own: Option<Namespace>,
    /// The turn. A script's calls come one at a time; the lock is what
    /// lets each of them change the turn's rules when a person answers
    /// "always".
    turn: tokio::sync::Mutex<&'a mut Turn>,
    /// Whom to ask.
    gate: &'a G,
    /// The `execute` call the script runs for.
    at: CallOf<'a>,
    /// What stops the script: the turn's stop, or a refusal without a
    /// reason.
    stop: &'a CancellationToken,
    /// The files the script's calls changed.
    changed: std::sync::Mutex<Vec<String>>,
    /// Whether a person refused a call without a reason.
    halted: AtomicBool,
}

impl<G: Gate> std::fmt::Debug for WithProject<'_, G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WithProject")
            .field("inner", &self.inner)
            .field("at", &self.at)
            .finish_non_exhaustive()
    }
}

impl<'a, G: Gate> WithProject<'a, G> {
    /// `inner` with the tools of `turn` added, for the script of call
    /// `at`, which `stop` stops; `gate` is asked where the rules say to.
    pub(crate) fn new(
        inner: &'a dyn Catalog,
        turn: &'a mut Turn,
        gate: &'a G,
        at: CallOf<'a>,
        stop: &'a CancellationToken,
    ) -> Self {
        let own = namespace(turn);
        Self {
            inner,
            listed: own.clone().filter(|_| turn.script_tools),
            own,
            turn: tokio::sync::Mutex::new(turn),
            gate,
            at,
            stop,
            changed: std::sync::Mutex::new(Vec::new()),
            halted: AtomicBool::new(false),
        }
    }

    /// The files the script's calls changed, and whether a person refused
    /// one of them without a reason.
    pub(crate) fn ended(self) -> (Vec<String>, bool) {
        let changed = self.changed.into_inner().unwrap_or_default();
        (changed, self.halted.load(Ordering::Relaxed))
    }

    /// Lets a script's call of the catalog's tool `tool` in `namespace`
    /// through, or says why not: the turn's rules decide it as they would
    /// the model's own call of that tool, and a person is asked where
    /// they say to.
    async fn admitted(&self, namespace: &str, tool: &str, args: &Value) -> Result<(), String> {
        let mut turn = self.turn.lock().await;
        let decided = crate::turn::script_outside(
            &mut turn, namespace, tool, args, self.at, self.gate, self.stop,
        )
        .await;
        match decided {
            Ok(()) => Ok(()),
            Err(ScriptCall::Failed(why)) => Err(why),
            Err(ScriptCall::Halted(why)) => {
                self.halted.store(true, Ordering::Relaxed);
                self.stop.cancel();
                Err(why)
            }
        }
    }

    /// Runs tool `tool` of those in `offered` with `args`, as the model's
    /// own call of it would run.
    async fn own_call(
        &self,
        offered: Option<&Namespace>,
        tool: &str,
        args: Value,
    ) -> Result<Value, String> {
        let offered = offered.is_some_and(|listed| listed.tools.iter().any(|t| t.name == tool));
        if !offered {
            return Err(format!("There's no tool {tool} in tools.{NAMESPACE}."));
        }
        let Value::Object(input) = args else {
            return Err(format!("tools.{NAMESPACE}.{tool} takes an object."));
        };
        let mut turn = self.turn.lock().await;
        let ran =
            crate::turn::script_call(&mut turn, tool, &input, self.at, self.gate, self.stop).await;
        match ran {
            Ok(out) => {
                if let Ok(mut changed) = self.changed.lock() {
                    changed.extend(out.changed);
                }
                if tool != "glob" {
                    return Ok(json!(out.text));
                }
                // The paths alone: the tool's words for "none" and "more"
                // aren't files.
                let paths: Vec<&str> = out
                    .text
                    .lines()
                    .filter(|l| *l != "No files found." && !l.starts_with('('))
                    .collect();
                Ok(json!(paths))
            }
            Err(ScriptCall::Failed(why)) => Err(why),
            Err(ScriptCall::Halted(why)) => {
                self.halted.store(true, Ordering::Relaxed);
                self.stop.cancel();
                Err(why)
            }
        }
    }
}

impl<G: Gate> SessionTools for WithProject<'_, G> {
    fn call<'a>(&'a self, tool: &'a str, args: Value) -> Answer<'a> {
        Box::pin(self.own_call(self.own.as_ref(), tool, args))
    }
}

impl<G: Gate> Catalog for WithProject<'_, G> {
    fn namespaces(&self) -> Vec<Namespace> {
        let mut all: Vec<Namespace> = self.listed.iter().cloned().collect();
        all.extend(self.inner.namespaces());
        all
    }

    fn direct(&self) -> Vec<Direct> {
        self.inner.direct()
    }

    fn call<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Answer<'a> {
        if namespace == NAMESPACE {
            return Box::pin(self.own_call(self.listed.as_ref(), tool, args));
        }
        Box::pin(async move {
            self.admitted(namespace, tool, &args).await?;
            self.inner.call_within(namespace, tool, args, self).await
        })
    }

    fn call_seen<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Seen<'a> {
        self.inner.call_seen(namespace, tool, args)
    }

    fn progress(&self, message: &str, call: &str, calls: &[Call]) {
        self.inner.progress(message, call, calls);
    }
}
