//! The tools a model can call: `read`,
//! `write`, `edit`, `glob`, `grep` and `shell`.
//!
//! A call is first made into a [`Plan`]: its arguments are checked and,
//! for an edit or a write, the file's new content and diff are worked out.
//! The plan says what permission it needs (its action, resources and a
//! preview), then runs. Errors are written for the model, so it can
//! correct its call.

// Rust guideline compliant 2026-10-09

mod edit_match;
mod outline;
mod search;
pub mod shell;

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value, json};
use similar::TextDiff;
use tokio_util::sync::CancellationToken;

use crate::rules::{Effect, Rule};

/// The tools that change no file of the project, whatever they're called
/// with. A tool not named here may: a command, a script, a subagent, a
/// tool from outside.
const LOOKING: &[&str] = &[
    "read",
    "glob",
    "grep",
    "question",
    "todowrite",
    "skill",
    "remember",
    "search_sessions",
    "webfetch",
    "websearch",
];

/// Whether the tool named `tool` leaves the project's files as they are.
pub(crate) fn only_looks(tool: &str) -> bool {
    LOOKING.contains(&tool)
}

/// The tools' definitions, as a model is told about them.
#[must_use]
pub fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "read",
            "description": "Reads a text file. Each line comes after its number and a tab. Use offset and limit for part of a long file.",
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
                "include": { "type": "string", "description": "Only files whose name matches this glob, such as \"*.rs\"." },
                "filesOnly": { "type": "boolean", "description": "List the files with a match and how many lines match, not the lines." }
            }, "required": ["pattern"] }
        }),
        json!({
            "name": "shell",
            "description": "Runs a shell command in the project's folder, without a terminal or input. Long output keeps its start and end.",
            "parameters": { "type": "object", "properties": {
                "command": { "type": "string" },
                "description": { "type": "string", "description": "What the command does, in a few words." },
                "timeout": { "type": "integer", "description": "Milliseconds before it's stopped (default 120000)." },
                "background": { "type": "boolean", "description": "Run it in the background and carry on, for servers and long builds: its output comes to you as a message when it ends." }
            }, "required": ["command"] }
        }),
    ]
}

/// What a call needs permission for, and what to show while asking.
#[derive(Debug, Clone, PartialEq)]
pub struct Permission {
    /// The action: `read`, `edit`, `glob`, `grep`, `shell`, or a tool's
    /// own name.
    pub action: std::borrow::Cow<'static, str>,
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
    /// Pictures it returns, such as an image file it read.
    pub pictures: Vec<crate::attachment::File>,
}

/// A checked call, ready to ask permission for and run.
#[derive(Debug, Clone)]
pub enum Plan {
    /// Runs a Code Mode script.
    Execute {
        /// The script: an async function's body.
        code: String,
    },
    /// Fetches a web page or file.
    WebFetch {
        /// Its address.
        url: String,
    },
    /// Searches the web.
    WebSearch {
        /// What to find.
        query: String,
    },
    /// Runs a tool a catalog offers on its own, such as an MCP server's.
    External {
        /// The name the model called.
        name: String,
        /// The catalog's namespace for it.
        namespace: String,
        /// Its name there.
        tool: String,
        /// Its input.
        input: Value,
    },
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
        /// Whether what the file declares is listed instead of its text.
        outline: bool,
        /// Whether the text comes as it is, without line numbers and past
        /// the lines and bytes a model is shown; set by
        /// [`Plan::for_script`].
        plain: bool,
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
        /// What the search leaves out, set by [`Plan::sealing`].
        skip: Vec<PathBuf>,
        /// The most files it lists; [`Plan::for_script`] raises it.
        most: usize,
    },
    /// Searches files' contents.
    Grep {
        /// The regular expression.
        pattern: String,
        /// The folder searched.
        base: PathBuf,
        /// Only files whose name matches this glob.
        include: Option<String>,
        /// Whether it lists the files with a match, not the lines.
        files_only: bool,
        /// What the search leaves out, set by [`Plan::sealing`].
        skip: Vec<PathBuf>,
        /// The rules that say which files it may read, set by
        /// [`Plan::guarded`]; none reads them all.
        rules: Option<Vec<(String, Vec<Rule>)>>,
        /// The most files or lines it lists; [`Plan::for_script`] raises
        /// it.
        most: usize,
    },
    /// Keeps a note for later sessions.
    Remember {
        /// The note.
        draft: crate::memory::Draft,
    },
    /// Asks the person; answered through the turn's gate.
    Question {
        /// The questions.
        questions: Vec<crate::question::Question>,
    },
    /// Hands a task to a subagent; started by the turn.
    Subagent {
        /// The task.
        request: crate::subagent::Request,
    },
    /// Writes the session's todo list.
    TodoWrite {
        /// The whole list.
        todos: Vec<crate::todo::Todo>,
    },
    /// Loads a skill's instructions; run by the turn, which knows them.
    Skill {
        /// The skill's ID.
        id: String,
    },
    /// Searches the project's earlier sessions, or reads a message of one.
    SearchSessions {
        /// The words.
        query: String,
        /// The message to read instead of searching.
        message: Option<String>,
    },
    /// Runs a command.
    Shell {
        /// The command.
        command: String,
        /// Milliseconds before it's stopped, when the call names them:
        /// otherwise the usual limit holds, [`SHELL_TIMEOUT_MS`].
        timeout_ms: Option<u64>,
        /// Whether it runs in the background.
        background: bool,
    },
}

/// A `glob` or `grep` call's plan, for `pattern`.
fn search_call(name: &str, pattern: String, input: &Map<String, Value>, cwd: &Path) -> Plan {
    let base = input
        .get("path")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map_or_else(|| cwd.to_path_buf(), |p| place(p, cwd).0);
    if name == "glob" {
        return Plan::Glob {
            pattern,
            base,
            skip: Vec::new(),
            most: search::MAX_FILES,
        };
    }
    Plan::Grep {
        pattern,
        base,
        skip: Vec::new(),
        rules: None,
        most: search::MAX_FILES,
        include: input
            .get("include")
            .and_then(Value::as_str)
            .filter(|i| !i.is_empty())
            .map(str::to_owned),
        files_only: input
            .get("filesOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// A `shell` call's plan.
fn shell_call(input: &Map<String, Value>) -> Result<Plan, String> {
    let command = input
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| "shell needs `command`, as text.".to_owned())?;
    Ok(Plan::Shell {
        command: command.to_owned(),
        timeout_ms: input
            .get("timeout")
            .and_then(Value::as_u64)
            .filter(|t| *t > 0),
        background: input
            .get("background")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Reads `path` (shown as `shown`): a picture whole, text as numbered
/// lines from `offset`, at most `limit` of them.
async fn read(path: &Path, shown: &str, offset: usize, limit: usize) -> Result<Output, String> {
    if let Some(mime) = picture_type(path) {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let picture = crate::attachment::File::new(
            &bytes,
            mime,
            json!({ "type": "uri", "uri": format!("file://{}", path.display()) }),
            name,
        );
        return Ok(Output {
            text: format!("Read the picture {shown}; it follows."),
            metadata: Map::new(),
            changed: Vec::new(),
            pictures: vec![picture],
        });
    }
    // (A file this large is a log or data: not to be loaded to show a page.)
    let size = tokio::fs::metadata(path).await.map_or(0, |m| m.len());
    if size > READ_FILE_BYTES {
        return Err(format!(
            "{shown} is {} MB, too large to read. Search it with grep, or take the part you \
             need with a command.",
            size / (1024 * 1024)
        ));
    }
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
    Ok(Output {
        text: numbered(&content, offset, limit),
        metadata: Map::new(),
        changed: Vec::new(),
        pictures: Vec::new(),
    })
}

/// The text of the file at `path` as it is, for a script: `limit` lines
/// from line `offset`.
async fn read_plain(
    path: &Path,
    shown: &str,
    offset: usize,
    limit: usize,
) -> Result<Output, String> {
    let size = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Couldn't read {shown}: {e}"))?
        .len();
    if size > SCRIPT_READ_BYTES {
        return Err(format!(
            "{shown} is {} MB, too large for a script to read. Search it with grep.",
            size / (1024 * 1024)
        ));
    }
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
    let text = if offset <= 1 && limit == usize::MAX {
        content
    } else {
        let lines: Vec<&str> = content.lines().skip(offset - 1).take(limit).collect();
        lines.join("\n")
    };
    Ok(Output {
        text,
        metadata: Map::new(),
        changed: Vec::new(),
        pictures: Vec::new(),
    })
}

/// What the file at `path` declares, for a `read` asked for its outline;
/// for a file that has none, how long it is and why not.
async fn outlined(path: PathBuf, shown: String) -> Result<Output, String> {
    let size = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("Couldn't read {shown}: {e}"))?
        .len();
    let text = if !outline::reads(&path) {
        format!(
            "{shown} has no outline: only {} files have one. Read it, or search it with grep.",
            outline::LANGUAGES
        )
    } else if size > outline::MAX_BYTES {
        format!("{shown} is too large to outline. Search it with grep.")
    } else {
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
        // (Off the async threads: a large file takes a moment to read.)
        tokio::task::spawn_blocking(move || {
            let lines = content.lines().count();
            outline::of(&path, &content).map_or_else(
                || format!("{shown}: {lines} lines. It declares no functions or types.\n"),
                |listed| {
                    format!(
                        "{shown}: {lines} lines. What it declares, after the lines each \
                         spans:\n{listed}"
                    )
                },
            )
        })
        .await
        .map_err(|e| format!("The outline stopped: {e}"))?
    };
    Ok(Output {
        text,
        metadata: Map::new(),
        changed: Vec::new(),
        pictures: Vec::new(),
    })
}

/// Adds the `outline` argument to the `read` tool among `definitions`.
pub(crate) fn offer_outline(definitions: &mut [Value]) {
    let Some(read) = definitions.iter_mut().find(|d| d["name"] == "read") else {
        return;
    };
    if let Some(description) = read["description"].as_str() {
        read["description"] = json!(format!(
            "{description} With outline, lists what a source file declares and the lines \
             each function and type spans, instead of its text: do that first for a long \
             file, then read only the part you need."
        ));
    }
    read["parameters"]["properties"]["outline"] = json!({
        "type": "boolean",
        "description": "List the file's functions and types with their lines, not its text."
    });
}

/// The picture type of the file at `path`, judged by its extension, when
/// it's a picture the model can see.
fn picture_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// Lines a read shows when no limit is given.
const READ_LINES: usize = 2000;
/// Bytes one read shows at most, whatever its line limit: about 25,000
/// tokens. A file of ordinary lines reaches the line limit first.
const READ_BYTES: usize = 100 * 1024;
/// The largest file a read loads, in bytes.
const READ_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Bytes of a result from a tool outside the engine (an MCP server's) the
/// model is shown; the whole of a longer one is saved for it to read.
const RESULT_BYTES: usize = 32 * 1024;
/// The default command timeout.
pub(crate) const SHELL_TIMEOUT_MS: u64 = 120_000;

/// The most files or lines a search lists for a script.
///
/// A script filters what it finds and returns little of it, so it may see
/// far more than the hundred a model reads at once; this still keeps a
/// search of everything from filling the script's memory.
const SCRIPT_MOST: usize = 5_000;
/// The largest file a script reads, in bytes: well inside the memory a
/// script may use, and past any source file.
const SCRIPT_READ_BYTES: u64 = 4 * 1024 * 1024;

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
                    outline: input
                        .get("outline")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    plain: false,
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
            "glob" | "grep" => Ok(search_call(name, text("pattern")?, input, cwd)),
            "remember" => remember(input),
            "question" => Ok(Self::Question {
                questions: crate::question::parse(input)?,
            }),
            "skill" => Ok(Self::Skill { id: text("id")? }),
            "subagent" => Ok(Self::Subagent {
                request: crate::subagent::parse(input)?,
            }),
            "todowrite" => Ok(Self::TodoWrite {
                todos: crate::todo::parse(input)?,
            }),
            "search_sessions" => {
                let message = input
                    .get("message")
                    .and_then(Value::as_str)
                    .filter(|m| !m.is_empty())
                    .map(str::to_owned);
                let query = match &message {
                    Some(_) => String::new(),
                    None => text("query")?,
                };
                Ok(Self::SearchSessions { query, message })
            }
            "shell" => shell_call(input),
            "webfetch" => text("url").map(|url| Self::WebFetch { url }),
            "websearch" => text("query").map(|query| Self::WebSearch { query }),
            "execute" => Ok(Self::Execute {
                code: text("code")?,
            }),
            other => Err(format!(
                "There is no tool called `{other}`. The tools are read, write, edit, glob, grep and shell."
            )),
        }
    }

    /// Whether the call only looks, so it can run beside others that do.
    ///
    /// A read, a search of files or of the web, and a fetch: none changes
    /// what another finds, whichever ends first.
    #[must_use]
    pub(crate) fn together(&self) -> bool {
        matches!(
            self,
            Self::Read { .. }
                | Self::Glob { .. }
                | Self::Grep { .. }
                | Self::WebFetch { .. }
                | Self::WebSearch { .. }
        )
    }

    /// What the call needs permission for.
    #[must_use]
    pub fn permission(&self) -> Permission {
        match self {
            Self::Read { shown, .. } => Permission {
                action: "read".into(),
                resources: vec![shown.clone()],
                metadata: Map::new(),
            },
            Self::Write {
                shown,
                before,
                after,
                ..
            } => Permission {
                action: "edit".into(),
                resources: vec![shown.clone()],
                metadata: files_metadata(shown, before.as_deref(), after),
            },
            Self::Edit {
                shown,
                before,
                after,
                ..
            } => Permission {
                action: "edit".into(),
                resources: vec![shown.clone()],
                metadata: files_metadata(shown, Some(before), after),
            },
            Self::Glob { pattern, .. } => Permission {
                action: "glob".into(),
                resources: vec![pattern.clone()],
                metadata: Map::new(),
            },
            Self::Grep { pattern, .. } => Permission {
                action: "grep".into(),
                resources: vec![pattern.clone()],
                metadata: Map::new(),
            },
            Self::Remember { draft } => Permission {
                action: match draft.scope {
                    crate::memory::Scope::Project => "remember".into(),
                    crate::memory::Scope::User => crate::rules::REMEMBER_USER.into(),
                },
                resources: vec![draft.text.clone()],
                metadata: Map::new(),
            },
            Self::Question { questions } => Permission {
                action: "question".into(),
                resources: questions.iter().map(|q| q.question.clone()).collect(),
                metadata: Map::new(),
            },
            Self::Subagent { request } => Permission {
                action: "subagent".into(),
                resources: vec![request.agent.clone()],
                metadata: Map::new(),
            },
            Self::TodoWrite { .. } => Permission {
                action: "todowrite".into(),
                resources: vec!["*".to_owned()],
                metadata: Map::new(),
            },
            Self::Skill { id } => Permission {
                action: "skill".into(),
                resources: vec![id.clone()],
                metadata: Map::new(),
            },
            Self::SearchSessions { query, message } => Permission {
                action: "search_sessions".into(),
                resources: vec![message.clone().unwrap_or_else(|| query.clone())],
                metadata: Map::new(),
            },
            Self::Shell { command, .. } => Permission {
                action: "shell".into(),
                resources: vec![command.clone()],
                metadata: Map::new(),
            },
            Self::Execute { .. } => Permission {
                action: "execute".into(),
                resources: vec!["*".to_owned()],
                metadata: Map::new(),
            },
            Self::WebFetch { url } => Permission {
                action: "webfetch".into(),
                resources: vec![url.clone()],
                metadata: Map::new(),
            },
            Self::WebSearch { query } => Permission {
                action: "websearch".into(),
                resources: vec![query.clone()],
                metadata: Map::new(),
            },
            Self::External { name, .. } => Permission {
                action: name.clone().into(),
                resources: vec!["*".to_owned()],
                metadata: Map::new(),
            },
        }
    }

    /// The call, kept away from the files in `sealed`: Harlo's own, such as
    /// where its server keeps its password, given with their links followed.
    ///
    /// No rule opens them. A search elsewhere leaves them out. A command is
    /// refused when it names one, which stops the plain case and no more:
    /// a command can spell a path in ways this doesn't see.
    ///
    /// # Errors
    ///
    /// Fails, with a message for the model, when the call reads, changes
    /// or names one of them.
    pub fn sealing(mut self, sealed: &[PathBuf]) -> Result<Self, String> {
        if sealed.is_empty() {
            return Ok(self);
        }
        let under = |path: &Path| {
            let real = real_path(path);
            sealed.iter().any(|s| real.starts_with(s))
        };
        let refused = match &mut self {
            Self::Read { path, .. } | Self::Write { path, .. } | Self::Edit { path, .. } => {
                under(path)
            }
            Self::Glob { base, skip, .. } | Self::Grep { base, skip, .. } => {
                *skip = sealed.to_vec();
                under(base)
            }
            Self::Shell { command, .. } => sealed.iter().any(|s| names(command, s)),
            _ => false,
        };
        if refused {
            Err(
                "Refused: that is one of Harlo's own files, which tools don't read or change."
                    .to_owned(),
            )
        } else {
            Ok(self)
        }
    }

    /// What the call needs permission for besides its own action, because
    /// it reaches outside project folder `cwd`; none when it stays inside.
    ///
    /// Symbolic links and `..` are followed first, so a link in the project
    /// to a file elsewhere counts as outside. A call that only reads may
    /// read in the `readable` folders without it.
    #[must_use]
    pub fn outside(&self, cwd: &Path, readable: &[&Path]) -> Option<Permission> {
        let (path, reads) = match self {
            Self::Read { path, .. } => (path, true),
            Self::Glob { base, .. } | Self::Grep { base, .. } => (base, true),
            Self::Write { path, .. } | Self::Edit { path, .. } => (path, false),
            _ => return None,
        };
        let real = real_path(path);
        if real.starts_with(real_path(cwd)) {
            return None;
        }
        if reads
            && readable
                .iter()
                .any(|open| real.starts_with(real_path(open)))
        {
            return None;
        }
        let searched = matches!(self, Self::Glob { .. } | Self::Grep { .. }) && real.is_dir();
        let resource = if searched {
            format!("{}/*", real.display())
        } else {
            real.display().to_string()
        };
        Some(Permission {
            action: crate::rules::OUTSIDE.into(),
            resources: vec![resource],
            // The preview of what would change, for the person asked.
            metadata: self.permission().metadata,
        })
    }

    /// The call as a script makes it, with `input`: a search lists as much
    /// as a script may see, and a read gives the file's text as it is, all
    /// of it unless the script picked lines. Any other call, unchanged.
    ///
    /// A script parses and filters what it gets and returns little of it,
    /// so it isn't held to what a model reads at once.
    #[must_use]
    pub fn for_script(mut self, input: &Map<String, Value>) -> Self {
        match &mut self {
            Self::Glob { most, .. } | Self::Grep { most, .. } => *most = SCRIPT_MOST,
            Self::Read {
                plain,
                limit,
                outline: false,
                ..
            } => {
                *plain = true;
                if !input.contains_key("limit") {
                    *limit = usize::MAX;
                }
            }
            _ => {}
        }
        self
    }

    /// A content search that reads only the files `rules` let be read
    /// without asking; any other call, unchanged.
    ///
    /// A search can't stop to ask about each file it passes, so the files
    /// that reading would ask about, or is refused for, are left out of it.
    #[must_use]
    pub fn guarded(mut self, rules: &[(String, Vec<Rule>)]) -> Self {
        if let Self::Grep { rules: guard, .. } = &mut self {
            *guard = Some(rules.to_vec());
        }
        self
    }

    /// The reading a content search of one file amounts to, in project
    /// folder `cwd`: it is decided as reading that file is.
    #[must_use]
    pub fn reads(&self, cwd: &Path) -> Option<Permission> {
        let Self::Grep { base, .. } = self else {
            return None;
        };
        if !base.is_file() {
            return None;
        }
        let shown = base
            .strip_prefix(cwd)
            .map_or_else(|_| base.display().to_string(), |p| p.display().to_string());
        Some(Permission {
            action: "read".into(),
            resources: vec![shown],
            metadata: Map::new(),
        })
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
                outline,
                plain,
            } => match (outline, plain) {
                (true, _) => outlined(path, shown).await,
                (false, true) => read_plain(&path, &shown, offset, limit).await,
                (false, false) => read(&path, &shown, offset, limit).await,
            },
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
                    pictures: Vec::new(),
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
                    pictures: Vec::new(),
                })
            }
            Self::Glob { .. } | Self::Grep { .. } => self.search(cwd).await,
            Self::Remember { .. } | Self::SearchSessions { .. } => {
                Err("Notes aren't kept on this server.".to_owned())
            }
            Self::Question { .. } => Err("There's no one to ask here.".to_owned()),
            Self::Execute { .. } => Err("Code Mode isn't available here.".to_owned()),
            Self::External { name, .. } => Err(format!("There is no tool called `{name}` here.")),
            Self::WebFetch { url } => crate::web::fetch(&url, &crate::web::saved_pages()).await,
            Self::WebSearch { .. } => Err("No web search service is set up.".to_owned()),
            Self::Skill { id } => Err(format!("There's no skill {id}.")),
            Self::Subagent { .. } => Err("Subagents can't start here.".to_owned()),
            Self::TodoWrite { todos } => Ok(Output {
                text: crate::todo::summary(&todos),
                metadata: json!({ "todos": todos, "truncated": false })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                changed: Vec::new(),
                pictures: Vec::new(),
            }),
            Self::Shell {
                command,
                timeout_ms,
                ..
            } => Ok(shell::run(&command, cwd, timeout_ms.unwrap_or(SHELL_TIMEOUT_MS), stop).await),
        }
    }
}

impl Plan {
    /// Runs a `glob` or `grep` off the async threads.
    async fn search(self, cwd: &Path) -> Result<Output, String> {
        let cwd = cwd.to_path_buf();
        let text = tokio::task::spawn_blocking(move || match self {
            Self::Glob {
                pattern,
                base,
                skip,
                most,
            } => search::glob(&pattern, &base, &cwd, &skip, most),
            Self::Grep {
                pattern,
                base,
                include,
                files_only,
                skip,
                rules,
                most,
            } => {
                let layers: Option<Vec<(&str, &[Rule])>> = rules.as_ref().map(|rules| {
                    rules
                        .iter()
                        .map(|(name, rules)| (name.as_str(), rules.as_slice()))
                        .collect()
                });
                let allowed = |shown: &str| {
                    layers.as_ref().is_none_or(|layers| {
                        crate::rules::decide(layers, "read", shown).effect == Effect::Allow
                    })
                };
                let guard: Option<&dyn Fn(&str) -> bool> = layers
                    .is_some()
                    .then_some(&allowed as &dyn Fn(&str) -> bool);
                let wanted = search::Wanted {
                    pattern: &pattern,
                    include: include.as_deref(),
                    files_only,
                    most,
                };
                search::grep(wanted, &base, &cwd, &skip, guard)
            }
            _ => Err("not a search".to_owned()),
        })
        .await
        .map_err(|e| format!("The search stopped: {e}"))??;
        Ok(Output {
            text,
            metadata: Map::new(),
            changed: Vec::new(),
            pictures: Vec::new(),
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

/// Whether `command` names `path`, in full or from the home folder (`~/…`).
fn names(command: &str, path: &Path) -> bool {
    let full = path.display().to_string();
    if command.contains(&full) {
        return true;
    }
    std::env::var_os("HOME")
        .and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
        .is_some_and(|rest| {
            let rest = rest.display().to_string();
            ["~/", "$HOME/", "${HOME}/"]
                .iter()
                .any(|home| command.contains(&format!("{home}{rest}")))
        })
}

/// `path` with its symbolic links and `..` followed, as far as it exists.
///
/// The part that doesn't exist yet is kept as written: a file about to be
/// created is placed by the folders that do exist.
#[must_use]
pub fn real_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    let mut exists = true;
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
                // What follows may exist again, and be a link.
                exists = true;
            }
            other => {
                out.push(other);
                if exists {
                    match out.canonicalize() {
                        Ok(real) => out = real,
                        Err(_) => exists = false,
                    }
                }
            }
        }
    }
    out
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

/// The diff of one file, as a tool reports it in `metadata.files`.
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
///
/// A very long line is cut short, and the lines stop at [`READ_BYTES`]
/// if that comes before `limit`; what follows says where to read on.
fn numbered(content: &str, offset: usize, limit: usize) -> String {
    let total = content.lines().count();
    let mut out = String::new();
    let mut shown_to = offset - 1;
    for (i, line) in content.lines().enumerate().skip(offset - 1).take(limit) {
        let line = shell::short_line(line);
        // (The first line asked for is always shown.)
        if !out.is_empty() && out.len() + line.len() > READ_BYTES {
            break;
        }
        // A number and a tab, as `cat -n` writes them and no wider:
        // padding and a colon cost three more tokens on every line.
        let _ = writeln!(out, "{}\t{line}", i + 1);
        shown_to = i + 1;
    }
    if shown_to < total && !out.is_empty() {
        let _ = writeln!(
            out,
            "({} more lines; read on with offset {})",
            total - shown_to,
            shown_to + 1
        );
    }
    if out.is_empty() {
        out.push_str(if total == 0 {
            "(The file is empty.)\n"
        } else {
            "(No lines there: the file is shorter.)\n"
        });
    }
    out
}

/// A result from a tool outside the engine, as the model is shown it.
///
/// One longer than [`RESULT_BYTES`] is saved whole under `spill`, and the
/// model gets its start and where the rest is.
pub(crate) fn outside_result(value: &Value, spill: &Path) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        // (Without the spaces and line breaks: they cost tokens.)
        other => other.to_string(),
    };
    if text.len() <= RESULT_BYTES {
        return text;
    }
    let mut end = RESULT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    // Saved in lines, so it can be read in parts and searched.
    let whole = match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| text.clone()),
    };
    let saved = std::fs::create_dir_all(spill).ok().and_then(|()| {
        let path = spill.join(format!("{}.txt", crate::id::ascending_id("result")));
        std::fs::write(&path, &whole).ok().map(|()| path)
    });
    let rest = match saved {
        Some(path) => format!(
            "the whole result is saved in {}: read it in parts or search it with grep",
            path.display()
        ),
        None => "the rest couldn't be saved: ask the tool for less at a time".to_owned(),
    };
    format!(
        "{}\n… ({} of {} bytes shown; {rest})",
        &text[..end],
        end,
        text.len()
    )
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

    #[cfg(unix)]
    #[test]
    fn a_read_is_bounded_in_bytes_as_well_as_lines() {
        // A minified file: one line far past any limit.
        let minified = format!("{}\nlast\n", "x".repeat(300_000));
        let shown = numbered(&minified, 1, READ_LINES);
        assert!(shown.len() < 3_000, "{}", shown.len());
        assert!(shown.contains("more characters on this line)"));
        assert!(shown.ends_with("2\tlast\n"));

        // Many long lines: it stops at the size limit and says where to go on.
        let wide = format!("{}\n", "y".repeat(1_000)).repeat(150);
        let shown = numbered(&wide, 1, READ_LINES);
        assert!(shown.len() <= READ_BYTES + 100, "{}", shown.len());
        let lines = shown.lines().filter(|l| l.contains("\ty")).count();
        assert!((90..150).contains(&lines), "{lines}");
        assert!(
            shown.ends_with(&format!(
                "({} more lines; read on with offset {})\n",
                150 - lines,
                lines + 1
            )),
            "{}",
            &shown[shown.len() - 80..]
        );
        // Reading on from there reaches the end.
        let rest = numbered(&wide, lines + 1, READ_LINES);
        assert!(rest.contains("\n150\ty") && !rest.contains("more lines"));
        assert_eq!(
            numbered("a\n", 5, 10),
            "(No lines there: the file is shorter.)\n"
        );
    }

    #[test]
    fn a_long_result_from_outside_is_cut_and_saved_whole() {
        let dir = tempfile::tempdir().expect("scratch");
        assert_eq!(outside_result(&json!("hi"), dir.path()), "hi");
        assert_eq!(
            outside_result(&json!({ "a": [1, 2] }), dir.path()),
            r#"{"a":[1,2]}"#
        );
        assert_eq!(std::fs::read_dir(dir.path()).expect("list").count(), 0);

        let rows: Vec<Value> = (0..5_000).map(|n| json!({ "row": n })).collect();
        let shown = outside_result(&json!({ "rows": rows }), dir.path());
        assert!(shown.len() < RESULT_BYTES + 300, "{}", shown.len());
        assert!(shown.starts_with(r#"{"rows":[{"row":0}"#));
        let saved = std::fs::read_dir(dir.path())
            .expect("list")
            .next()
            .expect("a file")
            .expect("entry")
            .path();
        assert!(shown.contains(&saved.display().to_string()), "{shown}");
        let whole = std::fs::read_to_string(saved).expect("saved");
        assert!(whole.contains("\"row\": 4999"));
        assert!(whole.lines().count() > 5_000, "in lines, to read in parts");
    }

    #[test]
    fn links_and_parent_folders_dont_hide_a_file_outside_the_project() {
        let dir = tempfile::tempdir().expect("dir");
        let project = dir.path().join("project");
        let secrets = dir.path().join("secrets");
        std::fs::create_dir_all(project.join("src")).expect("project");
        std::fs::create_dir_all(&secrets).expect("secrets");
        std::fs::write(secrets.join("key"), "k\n").expect("key");
        std::fs::write(project.join("src/a.rs"), "a\n").expect("a.rs");
        std::os::unix::fs::symlink(&secrets, project.join("link")).expect("link");
        let real = secrets.canonicalize().expect("real");
        let outside = |name: &str, args: Value| {
            Plan::new(name, &input(args), &project)
                .expect("plan")
                .outside(&project, &[])
                .map(|p| (p.action.to_string(), p.resources))
        };
        let asks = |resource: String| Some((crate::rules::OUTSIDE.to_owned(), vec![resource]));

        assert_eq!(outside("read", json!({ "path": "src/a.rs" })), None);
        assert_eq!(outside("read", json!({ "path": "src/../src/a.rs" })), None);
        assert_eq!(
            outside("write", json!({ "path": "new/b.rs", "content": "b" })),
            None
        );
        let key = real.join("key").display().to_string();
        assert_eq!(
            outside("read", json!({ "path": "link/key" })),
            asks(key.clone())
        );
        assert_eq!(
            outside("read", json!({ "path": "../secrets/key" })),
            asks(key.clone())
        );
        assert_eq!(
            outside("read", json!({ "path": secrets.join("key") })),
            asks(key)
        );
        // Through folders that don't exist yet, then a link.
        assert_eq!(
            outside(
                "write",
                json!({ "path": "new/../link/made", "content": "x" })
            ),
            asks(real.join("made").display().to_string())
        );
        assert_eq!(
            outside("grep", json!({ "pattern": "k", "path": "link" })),
            asks(format!("{}/*", real.display()))
        );

        // A call that only reads may read where it's told it may.
        let reading =
            Plan::new("read", &input(json!({ "path": "link/key" })), &project).expect("plan");
        assert_eq!(reading.outside(&project, &[secrets.as_path()]), None);
        let writing = Plan::new(
            "write",
            &input(json!({ "path": "link/key", "content": "x" })),
            &project,
        )
        .expect("plan");
        assert!(writing.outside(&project, &[secrets.as_path()]).is_some());
    }

    #[test]
    fn no_call_reads_changes_or_names_a_sealed_file() {
        let dir = tempfile::tempdir().expect("dir");
        let project = dir.path().join("project");
        let own = dir.path().join("own");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(&own).expect("own");
        std::fs::write(own.join("service.json"), "password\n").expect("secret");
        std::fs::write(own.join("server.log"), "started\n").expect("log");
        let sealed = [real_path(&own.join("service.json"))];
        let plan = |name: &str, args: Value| {
            Plan::new(name, &input(args), &project)
                .expect("plan")
                .sealing(&sealed)
        };
        let secret = own.join("service.json");

        plan("read", json!({ "path": secret })).expect_err("sealed");
        plan("write", json!({ "path": secret, "content": "x" })).expect_err("sealed");
        plan("read", json!({ "path": "../own/service.json" })).expect_err("sealed");
        plan("read", json!({ "path": own.join("server.log") })).expect("not sealed");
        let cat = format!("cat {}", secret.display());
        plan("shell", json!({ "command": cat })).expect_err("names it");
        plan("shell", json!({ "command": "ls" })).expect("names nothing");

        // A search around it leaves it out.
        let found = |name: &str, pattern: &str| {
            let plan = plan(name, json!({ "pattern": pattern, "path": own })).expect("allowed");
            match &plan {
                Plan::Glob {
                    pattern,
                    base,
                    skip,
                    most,
                } => search::glob(pattern, base, &project, skip, *most),
                Plan::Grep {
                    pattern,
                    base,
                    skip,
                    ..
                } => {
                    let wanted = search::Wanted {
                        pattern,
                        include: None,
                        files_only: false,
                        most: search::MAX_FILES,
                    };
                    search::grep(wanted, base, &project, skip, None)
                }
                other => panic!("{other:?} isn't a search"),
            }
            .expect("search")
        };
        let files = found("glob", "*");
        assert!(
            files.contains("server.log") && !files.contains("service.json"),
            "{files}"
        );
        assert_eq!(found("grep", "password"), "No matches.");
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
            ("edit".into(), ["a.txt".to_owned()].as_slice())
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
            "2\tb\n(1 more lines; read on with offset 3)\n"
        );
    }
}
