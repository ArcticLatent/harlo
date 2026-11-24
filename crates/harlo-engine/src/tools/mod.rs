//! The tools a model can call, named and shaped as OpenCode's: `read`,
//! `write`, `edit`, `glob`, `grep` and `shell`.
//!
//! A call is first made into a [`Plan`]: its arguments are checked and,
//! for an edit or a write, the file's new content and diff are worked out.
//! The plan says what permission it needs (its action, resources and a
//! preview), then runs. Errors are written for the model, so it can
//! correct its call.

// Rust guideline compliant 2026-10-09

mod edit_match;
mod search;
mod shell;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use similar::TextDiff;
use tokio_util::sync::CancellationToken;

/// The tools' definitions, as a model is told about them.
#[must_use]
pub fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "read",
            "description": "Reads a text file. Lines come numbered. Use offset and limit for part of a long file.",
            "parameters": { "type": "object", "properties": {
                "path": { "type": "string", "description": "The file, relative to the project's folder or absolute." },
                "offset": { "type": "integer", "description": "The first line to show, from 1." },
                "limit": { "type": "integer", "description": "How many lines to show." }
            }, "required": ["path"] }
        }),
        json!({
            "name": "write",
            "description": "Writes a whole file, creating it or replacing what it held. To change part of a file, use edit.",
            "parameters": { "type": "object", "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            }, "required": ["path", "content"] }
        }),
        json!({
            "name": "edit",
            "description": "Replaces text in a file. oldString must match exactly one place unless replaceAll is true; include enough surrounding lines to make it unique.",
            "parameters": { "type": "object", "properties": {
                "path": { "type": "string" },
                "oldString": { "type": "string" },
                "newString": { "type": "string" },
                "replaceAll": { "type": "boolean" }
            }, "required": ["path", "oldString", "newString"] }
        }),
        json!({
            "name": "glob",
            "description": "Finds files by name pattern, such as \"*.rs\" or \"src/**/*.ts\", most recently changed first. Ignored files are left out.",
            "parameters": { "type": "object", "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string", "description": "The folder to search, relative to the project's folder (default: all of it)." }
            }, "required": ["pattern"] }
        }),
        json!({
            "name": "grep",
            "description": "Searches file contents for a regular expression; lists matching lines with their numbers, by file. Ignored files are left out.",
            "parameters": { "type": "object", "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string", "description": "The folder to search (default: the project's)." },
                "include": { "type": "string", "description": "Only files whose name matches this glob, such as \"*.rs\"." }
            }, "required": ["pattern"] }
        }),
        json!({
            "name": "shell",
            "description": "Runs a shell command in the project's folder, without a terminal or input. Long output keeps its start and end.",
            "parameters": { "type": "object", "properties": {
                "command": { "type": "string" },
                "description": { "type": "string", "description": "What the command does, in a few words." },
                "timeout": { "type": "integer", "description": "Milliseconds before it's stopped (default 120000)." }
            }, "required": ["command"] }
        }),
    ]
}

/// What a call needs permission for, and what to show while asking.
#[derive(Debug, Clone, PartialEq)]
pub struct Permission {
    /// The action: `read`, `edit`, `glob`, `grep` or `shell`.
    pub action: &'static str,
    /// What it acts on: paths relative to the project, or the command.
    pub resources: Vec<String>,
    /// What a person sees before answering, such as the diff.
    pub metadata: Map<String, Value>,
}

/// What a tool returned.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// What the model reads.
    pub text: String,
    /// What people see, such as the diff or the command's output.
    pub metadata: Map<String, Value>,
    /// Files the call changed, relative to the project.
    pub changed: Vec<String>,
}

/// A checked call, ready to ask permission for and run.
#[derive(Debug, Clone)]
pub enum Plan {
    /// Reads a file.
    Read {
        /// The file.
        path: PathBuf,
        /// As the model named it, relative to the project when inside it.
        shown: String,
        /// The first line, from 1.
        offset: usize,
        /// How many lines.
        limit: usize,
    },
    /// Writes a file whole.
    Write {
        /// The file.
        path: PathBuf,
        /// Its name as shown.
        shown: String,
        /// What it held, if it existed.
        before: Option<String>,
        /// What it will hold.
        after: String,
    },
    /// Changes part of a file.
    Edit {
        /// The file.
        path: PathBuf,
        /// Its name as shown.
        shown: String,
        /// What it held when the call was checked.
        before: String,
        /// What it will hold.
        after: String,
        /// How many places changed.
        count: usize,
    },
    /// Finds files by name.
    Glob {
        /// The pattern.
        pattern: String,
        /// The folder searched.
        base: PathBuf,
    },
    /// Searches files' contents.
    Grep {
        /// The regular expression.
        pattern: String,
        /// The folder searched.
        base: PathBuf,
        /// Only files whose name matches this glob.
        include: Option<String>,
    },
    /// Keeps a note for later sessions.
    Remember {
        /// The note.
        draft: crate::memory::Draft,
    },
    /// Searches the project's earlier sessions.
    SearchSessions {
        /// The words.
        query: String,
    },
    /// Runs a command.
    Shell {
        /// The command.
        command: String,
        /// Milliseconds before it's stopped.
        timeout_ms: u64,
    },
}

/// Lines a read shows when no limit is given.
const READ_LINES: usize = 2000;
/// The default command timeout.
const SHELL_TIMEOUT_MS: u64 = 120_000;

impl Plan {
    /// Checks a call to tool `name` with `input`, in project folder `cwd`.
    ///
    /// # Errors
    ///
    /// Fails, with a message for the model, when the tool doesn't exist,
    /// an argument is missing or wrong, or an edit can't be placed.
    pub fn new(name: &str, input: &Map<String, Value>, cwd: &Path) -> Result<Self, String> {
        let text = |key: &str| -> Result<String, String> {
            input
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("{name} needs `{key}`, as text."))
        };
        let number = |key: &str| input.get(key).and_then(Value::as_u64);
        match name {
            "read" => {
                let (path, shown) = place(&text("path")?, cwd);
                let offset = number("offset")
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(1)
                    .max(1);
                let limit = number("limit")
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(READ_LINES);
                Ok(Self::Read {
                    path,
                    shown,
                    offset,
                    limit,
                })
            }
            "write" => {
                let (path, shown) = place(&text("path")?, cwd);
                let before = std::fs::read_to_string(&path).ok();
                Ok(Self::Write {
                    path,
                    shown,
                    before,
                    after: text("content")?,
                })
            }
            "edit" => {
                let (path, shown) = place(&text("path")?, cwd);
                let before = std::fs::read_to_string(&path).map_err(|e| {
                    format!("Couldn't read {shown}: {e}. To create a file, use write.")
                })?;
                let (old, new) = (text("oldString")?, text("newString")?);
                let all = input
                    .get("replaceAll")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let (after, count) = edited(&before, &old, &new, all)?;
                Ok(Self::Edit {
                    path,
                    shown,
                    before,
                    after,
                    count,
                })
            }
            "glob" | "grep" => {
                let pattern = text("pattern")?;
                let base = input
                    .get("path")
                    .and_then(Value::as_str)
                    .filter(|p| !p.is_empty())
                    .map_or_else(|| cwd.to_path_buf(), |p| place(p, cwd).0);
                Ok(if name == "glob" {
                    Self::Glob { pattern, base }
                } else {
                    Self::Grep {
                        pattern,
                        base,
                        include: input
                            .get("include")
                            .and_then(Value::as_str)
                            .filter(|i| !i.is_empty())
                            .map(str::to_owned),
                    }
                })
            }
            "remember" => remember(input),
            "search_sessions" => Ok(Self::SearchSessions {
                query: text("query")?,
            }),
            "shell" => Ok(Self::Shell {
                command: text("command")?,
                timeout_ms: number("timeout")
                    .filter(|t| *t > 0)
                    .unwrap_or(SHELL_TIMEOUT_MS),
            }),
            other => Err(format!(
                "There is no tool called `{other}`. The tools are read, write, edit, glob, grep and shell."
            )),
        }
    }

    /// What the call needs permission for.
    #[must_use]
    pub fn permission(&self) -> Permission {
        match self {
            Self::Read { shown, .. } => Permission {
                action: "read",
                resources: vec![shown.clone()],
                metadata: Map::new(),
            },
            Self::Write {
                shown,
                before,
                after,
                ..
            } => Permission {
                action: "edit",
                resources: vec![shown.clone()],
                metadata: files_metadata(shown, before.as_deref(), after),
            },
            Self::Edit {
                shown,
                before,
                after,
                ..
            } => Permission {
                action: "edit",
                resources: vec![shown.clone()],
                metadata: files_metadata(shown, Some(before), after),
            },
            Self::Glob { pattern, .. } => Permission {
                action: "glob",
                resources: vec![pattern.clone()],
                metadata: Map::new(),
            },
            Self::Grep { pattern, .. } => Permission {
                action: "grep",
                resources: vec![pattern.clone()],
                metadata: Map::new(),
            },
            Self::Remember { draft } => Permission {
                action: "remember",
                resources: vec![draft.text.clone()],
                metadata: Map::new(),
            },
            Self::SearchSessions { query } => Permission {
                action: "search_sessions",
                resources: vec![query.clone()],
                metadata: Map::new(),
            },
            Self::Shell { command, .. } => Permission {
                action: "shell",
                resources: vec![command.clone()],
                metadata: Map::new(),
            },
        }
    }

    /// Runs the call in project folder `cwd`.
    ///
    /// # Errors
    ///
    /// Fails, with a message for the model, when the file can't be read or
    /// written, an edited file changed since the call was checked and the
    /// edit no longer fits, or the command can't start.
    pub async fn run(self, cwd: &Path, stop: &CancellationToken) -> Result<Output, String> {
        match self {
            Self::Read {
                path,
                shown,
                offset,
                limit,
            } => {
                let content = tokio::fs::read_to_string(&path)
                    .await
                    .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
                Ok(Output {
                    text: numbered(&content, offset, limit),
                    metadata: Map::new(),
                    changed: Vec::new(),
                })
            }
            Self::Write {
                path,
                shown,
                before,
                after,
            } => {
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| format!("Couldn't make {}: {e}", parent.display()))?;
                }
                tokio::fs::write(&path, &after)
                    .await
                    .map_err(|e| format!("Couldn't write {shown}: {e}"))?;
                let verb = if before.is_some() { "Wrote" } else { "Created" };
                Ok(Output {
                    text: format!("{verb} {shown}"),
                    metadata: files_metadata(&shown, before.as_deref(), &after),
                    changed: vec![shown],
                })
            }
            Self::Edit {
                path,
                shown,
                before,
                after,
                count,
            } => {
                // (Someone else may have written it since: the edit must still
                // fit what's there now.)
                let now = tokio::fs::read_to_string(&path)
                    .await
                    .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
                if now != before {
                    return Err(format!(
                        "{shown} changed since the edit was checked. Read it again and redo the edit."
                    ));
                }
                tokio::fs::write(&path, &after)
                    .await
                    .map_err(|e| format!("Couldn't write {shown}: {e}"))?;
                let places = if count == 1 {
                    "1 replacement".to_owned()
                } else {
                    format!("{count} replacements")
                };
                Ok(Output {
                    text: format!("Edited {shown} ({places})"),
                    metadata: files_metadata(&shown, Some(&before), &after),
                    changed: vec![shown],
                })
            }
            Self::Glob { .. } | Self::Grep { .. } => self.search(cwd).await,
            Self::Remember { .. } | Self::SearchSessions { .. } => {
                Err("Notes aren't kept on this server.".to_owned())
            }
            Self::Shell {
                command,
                timeout_ms,
            } => Ok(shell::run(&command, cwd, timeout_ms, stop).await),
        }
    }
}

impl Plan {
    /// Runs a `glob` or `grep` off the async threads.
    async fn search(self, cwd: &Path) -> Result<Output, String> {
        let cwd = cwd.to_path_buf();
        let text = tokio::task::spawn_blocking(move || match self {
            Self::Glob { pattern, base } => search::glob(&pattern, &base, &cwd),
            Self::Grep {
                pattern,
                base,
                include,
            } => search::grep(&pattern, &base, include.as_deref(), &cwd),
            _ => Err("not a search".to_owned()),
        })
        .await
        .map_err(|e| format!("The search stopped: {e}"))??;
        Ok(Output {
            text,
            metadata: Map::new(),
            changed: Vec::new(),
        })
    }
}

/// Checks a `remember` call.
fn remember(input: &Map<String, Value>) -> Result<Plan, String> {
    let text = input
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "remember needs `text`, as text.".to_owned())?;
    let scope = match input.get("scope").and_then(Value::as_str) {
        None | Some("project") => crate::memory::Scope::Project,
        Some("user") => crate::memory::Scope::User,
        Some(other) => {
            return Err(format!(
                "remember's scope is project or user, not `{other}`."
            ));
        }
    };
    Ok(Plan::Remember {
        draft: crate::memory::Draft {
            text,
            scope,
            replaces: input
                .get("replaces")
                .and_then(Value::as_str)
                .filter(|r| !r.is_empty())
                .map(str::to_owned),
        },
    })
}

/// The file `named` and how to show it: relative to `cwd` when inside it.
fn place(named: &str, cwd: &Path) -> (PathBuf, String) {
    let path = if Path::new(named).is_absolute() {
        PathBuf::from(named)
    } else {
        cwd.join(named)
    };
    let shown = path
        .strip_prefix(cwd)
        .map_or_else(|_| path.display().to_string(), |p| p.display().to_string());
    (path, shown)
}

/// `before` with `old` replaced: in one place (matched with care), or in
/// every exact place when `all`.
fn edited(before: &str, old: &str, new: &str, all: bool) -> Result<(String, usize), String> {
    if all {
        let count = before.matches(old).count();
        if old.is_empty() || count == 0 {
            return Err(
                "The old text isn't in the file. Read it and copy the text to replace exactly."
                    .to_owned(),
            );
        }
        return Ok((before.replace(old, new), count));
    }
    let edit = edit_match::Edit {
        old: old.to_owned(),
        new: new.to_owned(),
    };
    edit_match::apply(before, &[edit]).map(|(after, _)| (after, 1))
}

/// The diff of one file, as OpenCode reports it in `metadata.files`.
fn files_metadata(shown: &str, before: Option<&str>, after: &str) -> Map<String, Value> {
    let old = before.unwrap_or_default();
    let diff = TextDiff::from_lines(old, after);
    let (mut additions, mut deletions) = (0, 0);
    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Insert => additions += 1,
            similar::ChangeTag::Delete => deletions += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    let patch = diff.unified_diff().header(shown, shown).to_string();
    let status = if before.is_some() {
        "modified"
    } else {
        "added"
    };
    let mut metadata = Map::new();
    metadata.insert(
        "files".to_owned(),
        json!([{ "file": shown, "patch": patch, "status": status, "additions": additions, "deletions": deletions }]),
    );
    metadata
}

/// Lines `offset`.. of `content`, at most `limit`, each with its number.
fn numbered(content: &str, offset: usize, limit: usize) -> String {
    let total = content.lines().count();
    let mut out = String::new();
    for (i, line) in content.lines().enumerate().skip(offset - 1).take(limit) {
        let _ = writeln!(out, "{:>5}: {line}", i + 1);
    }
    let shown_to = (offset - 1 + limit).min(total);
    if shown_to < total {
        let _ = writeln!(
            out,
            "({} more lines; read on with offset {})",
            total - shown_to,
            shown_to + 1
        );
    }
    if out.is_empty() {
        out.push_str("(The file is empty.)\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }

    #[tokio::test]
    async fn an_edit_shows_its_diff_before_it_runs_and_refuses_a_changed_file() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("a.txt"), "alpha\n").expect("write");
        let plan = Plan::new(
            "edit",
            &input(json!({ "path": "a.txt", "oldString": "alpha", "newString": "omega" })),
            dir.path(),
        )
        .expect("plan");
        let ask = plan.permission();
        assert_eq!(
            (ask.action, ask.resources.as_slice()),
            ("edit", ["a.txt".to_owned()].as_slice())
        );
        let file = &ask.metadata["files"][0];
        assert_eq!(
            (file["additions"].as_u64(), file["deletions"].as_u64()),
            (Some(1), Some(1))
        );
        assert!(
            file["patch"]
                .as_str()
                .is_some_and(|p| p.contains("-alpha\n+omega")),
            "{file}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).expect("read"),
            "alpha\n",
            "nothing written yet"
        );

        let stop = CancellationToken::new();
        let out = plan.clone().run(dir.path(), &stop).await.expect("ran");
        assert_eq!(out.text, "Edited a.txt (1 replacement)");
        assert_eq!(out.changed, ["a.txt"]);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).expect("read"),
            "omega\n"
        );
        // Run again: the file isn't what was checked any more.
        assert!(
            plan.run(dir.path(), &stop)
                .await
                .is_err_and(|e| e.contains("changed since"))
        );
    }

    #[test]
    fn bad_calls_say_what_to_fix() {
        let dir = tempfile::tempdir().expect("dir");
        assert!(
            Plan::new(
                "edit",
                &input(json!({ "path": "nope.txt", "oldString": "a", "newString": "b" })),
                dir.path()
            )
            .is_err_and(|e| e.contains("To create a file, use write"))
        );
        assert!(
            Plan::new("read", &input(json!({})), dir.path())
                .is_err_and(|e| e.contains("needs `path`"))
        );
        assert!(
            Plan::new("bash", &input(json!({})), dir.path())
                .is_err_and(|e| e.contains("no tool called `bash`"))
        );
    }

    #[test]
    fn reads_are_numbered_and_say_what_is_left() {
        assert_eq!(
            numbered("a\nb\nc\n", 2, 1),
            "    2: b\n(1 more lines; read on with offset 3)\n"
        );
    }
}
