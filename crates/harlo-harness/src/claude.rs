//! Claude Code as a harness (its stream-json mode): its models, its
//! session files (`~/.claude/projects/<folder>/<id>.jsonl`) and their
//! conversation in pi's message shape. The running session is
//! `claude_run.rs`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::claude_run::{result_content, result_text, to_pi_block};
use crate::event::HarnessKind;
use crate::store::SessionInfo;

/// A Claude model harlo offers. Claude Code has no model listing, so the
/// list is curated, each entry from the Claude Code version that
/// knows it.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    pub id: &'static str,
    pub label: &'static str,
    /// The oldest Claude Code that has it.
    pub min_version: Option<&'static str>,
    /// Only with verified access (shown when Claude Code lists it).
    pub requires_access: bool,
    /// Has the extra-high effort level.
    pub xhigh: bool,
    /// Thinking can be turned off.
    pub thinking_off: bool,
}

const fn m(id: &'static str, label: &'static str, min_version: Option<&'static str>, xhigh: bool, thinking_off: bool) -> Model {
    Model { id, label, min_version, requires_access: false, xhigh, thinking_off }
}

pub const MODELS: &[Model] = &[
    m("claude-opus-5-5", "Opus 5.5", Some("2.1.280"), true, false),
    m("claude-opus-5", "Opus 5", Some("2.1.219"), true, true),
    Model { id: "claude-mythos-5-1", label: "Mythos 5.1", min_version: None, requires_access: true, xhigh: true, thinking_off: false },
    m("claude-fable-5-1", "Fable 5.1", None, true, false),
    m("claude-fable-5", "Fable 5", Some("2.1.169"), true, false),
    m("claude-opus-4-8[1m]", "Opus 4.8 1M", None, true, true),
    m("claude-opus-4-8", "Opus 4.8", None, true, true),
    m("claude-sonnet-5-5", "Sonnet 5.5", Some("2.1.284"), true, false),
    m("claude-sonnet-5", "Sonnet 5", None, true, true),
    m("claude-sonnet-5[1m]", "Sonnet 5 1M", None, true, true),
    m("claude-opus-4-7[1m]", "Opus 4.7 1M", None, true, true),
    m("claude-opus-4-7", "Opus 4.7", None, true, true),
    m("claude-opus-4-6[1m]", "Opus 4.6 1M", None, false, true),
    m("claude-opus-4-6", "Opus 4.6", None, false, true),
    m("claude-sonnet-4-6[1m]", "Sonnet 4.6 1M", None, false, true),
    m("claude-sonnet-4-6", "Sonnet 4.6", None, false, true),
    m("claude-haiku-5-5", "Haiku 5.5", Some("2.1.293"), true, true),
    m("claude-haiku-4-5", "Haiku 4.5", None, false, false),
];

/// "2.1.296 (Claude Code)" → [2, 1, 296].
fn version_parts(v: &str) -> Vec<u32> {
    v.split_whitespace().next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn at_least(version: &str, min: &str) -> bool {
    version_parts(version) >= version_parts(min)
}

/// The models this Claude Code (`version`, as `claude --version` prints it)
/// can run, plus one set in `~/.claude/settings.json` if it isn't among them.
pub fn models(version: Option<&str>, settings: Option<&Path>) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = MODELS
        .iter()
        .filter(|m| !m.requires_access)
        .filter(|m| match (m.min_version, version) {
            (Some(min), Some(v)) => at_least(v, min),
            _ => true,
        })
        .map(|m| (m.id.to_owned(), m.label.to_owned()))
        .collect();
    if let Some(chosen) = settings.and_then(settings_model)
        && !list.iter().any(|(id, _)| *id == chosen)
    {
        list.push((chosen.clone(), chosen));
    }
    list
}

/// The model `settings.json` picks (`model`), if any.
fn settings_model(path: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    v["model"].as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned)
}

/// `$CLAUDE_CONFIG_DIR` or `~/.claude`.
pub fn config_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".claude")))
}

/// Where Claude Code keeps a folder's sessions: every character but
/// letters and digits becomes `-` (long names are cut and get a hash, as
/// Claude Code does).
pub fn project_dir_name(cwd: &Path) -> String {
    let real = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let name: String = real.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    if name.len() <= 200 {
        return name;
    }
    // Java's String.hashCode, in base 36.
    let mut h: i32 = 0;
    for u in real.to_string_lossy().encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u as i32);
    }
    let mut n = (h as i64).unsigned_abs();
    let mut digits = Vec::new();
    while n > 0 {
        digits.push(std::char::from_digit((n % 36) as u32, 36).unwrap());
        n /= 36;
    }
    let hash: String = if digits.is_empty() { "0".into() } else { digits.into_iter().rev().collect() };
    format!("{}-{hash}", &name[..200])
}

/// A session's transcript file (`None` without a Claude config folder).
pub fn session_file(cwd: &Path, session_id: &str) -> Option<PathBuf> {
    Some(config_dir()?.join("projects").join(project_dir_name(cwd)).join(format!("{session_id}.jsonl")))
}

/// Every session file: `(path, modified, size)`.
pub fn list_files() -> Vec<(PathBuf, SystemTime, u64)> {
    let Some(root) = config_dir().map(|d| d.join("projects")) else { return Vec::new() };
    let Ok(dirs) = std::fs::read_dir(&root) else { return Vec::new() };
    let mut out = Vec::new();
    for dir in dirs.flatten().filter(|d| d.path().is_dir()) {
        let Ok(files) = std::fs::read_dir(dir.path()) else { continue };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().is_some_and(|e| e == "jsonl")
                && let Ok(meta) = f.metadata()
                && meta.is_file()
            {
                out.push((path, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len()));
            }
        }
    }
    out
}

/// A user message's text, or None for the lines that aren't the user typing
/// (tool results, meta, interruptions, command output, Claude Code's own
/// notices).
fn user_text(entry: &Value) -> Option<String> {
    if entry["isMeta"] == true || entry["isCompactSummary"] == true || task_notice(entry).is_some() {
        return None;
    }
    let content = &entry["message"]["content"];
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => {
            if parts.iter().any(|p| p["type"] == "tool_result") {
                return None;
            }
            parts.iter().filter(|p| p["type"] == "text").filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n")
        }
        _ => return None,
    };
    let t = text.trim();
    if t.is_empty() || t.starts_with("<local-command") || t.starts_with("[Request interrupted") || t == "No response requested." {
        return None;
    }
    // `/cmd args` written as tags.
    if let Some(cmd) = tag(t, "command-name") {
        let args = tag(t, "command-args").unwrap_or_default();
        return Some(format!("{cmd} {args}").trim().to_owned());
    }
    Some(t.to_owned())
}

/// Claude Code's word that a background task ended (a command, a
/// subagent), written as a user message: its summary, said as a notice.
fn task_notice(entry: &Value) -> Option<String> {
    let content = &entry["message"]["content"];
    let text = match content {
        Value::String(s) => s.as_str(),
        Value::Array(parts) => parts.iter().find(|p| p["type"] == "text").and_then(|p| p["text"].as_str())?,
        _ => return None,
    };
    let t = text.trim();
    if entry["origin"]["kind"] != "task-notification" && !t.starts_with("<task-notification>") {
        return None;
    }
    let summary = tag(t, "summary").filter(|s| !s.is_empty()).map(str::to_owned);
    Some(summary.unwrap_or_else(|| match tag(t, "status") {
        Some(status) if !status.is_empty() => format!("A background task {status}."),
        _ => "A background task ended.".to_owned(),
    }))
}

fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim())
}

/// The sidebar's view of a session file (from its start and end).
pub fn read_session(path: &Path) -> Option<SessionInfo> {
    let meta = std::fs::metadata(path).ok()?;
    let size = meta.len();
    let (head, tail) = crate::store::head_and_tail(path, size)?;
    let mut info = session_from_text(path, size, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), &head, &tail)?;
    // Its folder was moved or renamed: where it last ran, if that's still there.
    if !info.cwd.is_dir()
        && let Some(cwd) = last_folder(if tail.is_empty() { &head } else { &tail })
    {
        info.cwd = cwd;
    }
    Some(info)
}

/// The latest folder in a transcript's text that's still there, or the top of
/// the repository it's in (the session's own folder: a command can leave it in
/// one further down).
fn last_folder(tail: &str) -> Option<PathBuf> {
    let cwd = tail.lines().rev().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|e| e["isSidechain"] != true).filter_map(|e| e["cwd"].as_str().map(PathBuf::from)).find(|c| c.is_dir())?;
    Some(cwd.ancestors().find(|d| d.join(".git").exists()).unwrap_or(&cwd).to_path_buf())
}

/// Whether a transcript is the Claude extension for pi's: it ran Claude Code
/// for a pi session, whose copy this is (the conversation is pi's, not a
/// Claude Code session of its own). Its file doesn't name it, so this goes by how it runs: a history
/// it wrote in itself has its library's message ids; else Claude Code was
/// started through the SDK, bypassing permissions, with none of its own tools
/// and only the extension's. (The quotes can't match text in a message: there
/// they're escaped.)
fn pi_extensions_copy(head: &str) -> bool {
    if head.contains(r#""id":"msg_syn_"#) {
        return true;
    }
    let first = head.lines().filter(|l| l.contains(r#""type":"user""#)).find_map(|l| serde_json::from_str::<Value>(l).ok());
    first.is_some_and(|e| e["entrypoint"] == "sdk-ts" && e["turnOrigin"] == "sdk" && e["permissionMode"] == "bypassPermissions")
        && (head.contains(r#""mcp__custom-tools__"#) || !head.contains("deferred_tools_delta"))
}

/// In a session's `extra_args`: the thinking level it starts on (not Claude's
/// own option: the level is turned into those).
pub const THINKING_ARG: &str = "--harlo-thinking=";

/// The thinking levels of a model (`claude-…`, or `anthropic/claude-…`).
pub fn thinking_levels(model: &str) -> Vec<&'static str> {
    let model = model.strip_prefix("anthropic/").unwrap_or(model);
    let m = MODELS.iter().find(|m| m.id == model);
    let mut levels = Vec::new();
    if m.is_none_or(|m| m.thinking_off) {
        levels.push("off");
    }
    levels.extend(["low", "medium", "high"]);
    if m.is_none_or(|m| m.xhigh) {
        levels.push("xhigh");
    }
    levels.push("max");
    levels
}

/// The effort Claude Code runs at when it isn't told one: its settings' (""
/// if they don't say).
pub fn default_effort() -> String {
    let settings = config_dir().and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok());
    let settings = settings.and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or_default();
    settings["effortLevel"].as_str().unwrap_or_default().to_owned()
}

/// `read_session` from a transcript's start and end (a remote host's, read
/// over SSH).
pub fn session_from_text(path: &Path, size: u64, modified: SystemTime, head: &str, tail: &str) -> Option<SessionInfo> {
    if pi_extensions_copy(head) {
        return None;
    }
    let id = path.file_stem()?.to_string_lossy().into_owned();
    let (mut cwd, mut first, mut created, mut name) = (None, String::new(), String::new(), String::new());
    for line in head.lines() {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        if e["isSidechain"] == true {
            continue;
        }
        if cwd.is_none() {
            cwd = e["cwd"].as_str().map(PathBuf::from);
        }
        if created.is_empty() {
            created = e["timestamp"].as_str().unwrap_or_default().to_owned();
        }
        if first.is_empty() && e["type"] == "user" {
            first = user_text(&e).map(|t| crate::file_blocks::title_text(&t).chars().take(100).collect()).unwrap_or_default();
        }
    }
    // The latest title: one the user gave, else Claude's.
    let (mut custom, mut ai) = (String::new(), String::new());
    for line in head.lines().chain(tail.lines()) {
        if !(line.contains("custom-title") || line.contains("ai-title")) {
            continue;
        }
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(t) = e["customTitle"].as_str() {
            custom = t.to_owned();
        }
        if let Some(t) = e["aiTitle"].as_str() {
            ai = t.to_owned();
        }
    }
    if !custom.is_empty() {
        name = custom;
    } else if !ai.is_empty() {
        name = ai;
    }
    // A file with no conversation (only settings lines): not a session.
    if first.is_empty() && name.is_empty() {
        return None;
    }
    Some(SessionInfo {
        harness: HarnessKind::Claude,
        path: path.to_path_buf(),
        id,
        cwd: cwd?,
        name,
        first_message: first,
        created,
        // (Not before the file's time: a clock that ran ahead.)
        modified: last_activity(if tail.is_empty() { head } else { tail }).map_or(modified, |t| t.min(modified)),
        size,
        parent_session: None,
    })
}

/// When the conversation last changed: its newest entry's time. Claude Code
/// also writes lines with no time that aren't the conversation (its costs,
/// as it quits; titles), which change the file's time.
fn last_activity(text: &str) -> Option<SystemTime> {
    text.lines().rev().filter(|l| l.contains(r#""timestamp":""#)).find_map(|l| {
        let e = serde_json::from_str::<Value>(l).ok()?;
        let t = chrono::DateTime::parse_from_rfc3339(e["timestamp"].as_str()?).ok()?;
        let ms = u64::try_from(t.timestamp_millis()).ok()?;
        Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ms))
    })
}

/// What's read of every line of a transcript first: most of a long one
/// (Claude Code's records of its requests, hooks' output) is never shown.
#[derive(serde::Deserialize)]
struct LineHead {
    #[serde(rename = "type", default)]
    kind: Value,
    #[serde(rename = "isSidechain", default)]
    sidechain: Value,
    #[serde(rename = "agentId", default)]
    agent_id: Value,
    #[serde(rename = "customTitle", default)]
    custom_title: Value,
    #[serde(rename = "aiTitle", default)]
    ai_title: Value,
}

/// Where to read a growing transcript on from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resume {
    /// The file offset of a user message's line.
    pub offset: u64,
    /// The message index that line starts at.
    pub index: i64,
    /// The bytes before `offset`: another text there means it was rewritten.
    pub check: Vec<u8>,
}

/// Messages of a transcript from `first_index` on.
#[derive(Debug, Default)]
pub struct Part {
    pub messages: Vec<Value>,
    pub first_index: i64,
    pub resume: Resume,
}

const RESUME_CHECK_BYTES: u64 = 64;

/// The conversation so far, a line at a time.
#[derive(Default)]
struct HistoryReader {
    out: Vec<Value>,
    last_assistant: Option<String>,
    subagent: Option<bool>,
    custom: String,
    ai: String,
    /// The last user line and how many messages came before it. A user line
    /// ends what came before (an assistant's blocks join across lines), so
    /// reading on from it reads what reading from the start would.
    resume: (u64, usize),
}

impl HistoryReader {
    fn line(&mut self, offset: u64, line: &str) {
        let Ok(head) = serde_json::from_str::<LineHead>(line) else { return };
        let subagent = *self.subagent.get_or_insert_with(|| head.sidechain == true && head.agent_id.is_string());
        if let Some(t) = head.custom_title.as_str() {
            self.custom = t.to_owned();
        }
        if let Some(t) = head.ai_title.as_str() {
            self.ai = t.to_owned();
        }
        if head.sidechain == true && !subagent {
            return;
        }
        match head.kind.as_str() {
            Some("user") => {
                self.resume = (offset, self.out.len());
                self.last_assistant = None;
                let Ok(e) = serde_json::from_str::<Value>(line) else { return };
                let content = &e["message"]["content"];
                if let Some(parts) = content.as_array()
                    && parts.iter().any(|p| p["type"] == "tool_result")
                {
                    for p in parts.iter().filter(|p| p["type"] == "tool_result") {
                        self.out.push(json!({ "role": "toolResult", "toolCallId": p["tool_use_id"], "content": result_content(&p["content"]), "isError": p["is_error"].as_bool().unwrap_or(false) }));
                    }
                } else if let Some(text) = task_notice(&e) {
                    self.out.push(json!({ "role": "custom", "customType": "task-notification", "display": true, "content": text }));
                } else if let Some(text) = user_text(&e) {
                    self.out.push(json!({ "role": "user", "content": [{ "type": "text", "text": text }] }));
                }
            }
            Some("assistant") => {
                let Ok(e) = serde_json::from_str::<Value>(line) else { return };
                let id = e["message"]["id"].as_str().unwrap_or_default().to_owned();
                let blocks: Vec<Value> = e["message"]["content"].as_array().into_iter().flatten().filter(|b| b["type"] != "redacted_thinking").map(to_pi_block).collect();
                let usage = usage_of(&e["message"]["usage"]);
                match (self.last_assistant.as_deref(), self.out.last_mut()) {
                    (Some(prev), Some(m)) if prev == id && !id.is_empty() => {
                        if let Some(c) = m["content"].as_array_mut() {
                            c.extend(blocks);
                        }
                        // Each part says the reply's use so far.
                        if !usage.is_null() {
                            m["usage"] = usage;
                        }
                    }
                    _ => {
                        let mut m = json!({ "role": "assistant", "content": blocks, "model": e["message"]["model"] });
                        if !usage.is_null() {
                            m["usage"] = usage;
                        }
                        self.out.push(m);
                        self.last_assistant = Some(id);
                    }
                }
            }
            _ => {}
        }
    }

    fn saved(self) -> crate::store::SavedSession {
        let messages = self.out;
        let model = messages.iter().rev().find_map(|m| m["model"].as_str().filter(|s| !s.is_empty())).map(|m| format!("anthropic/{m}")).unwrap_or_default();
        crate::store::SavedSession { messages, model, model_id: String::new(), thinking: String::new(), name: if self.custom.is_empty() { self.ai } else { self.custom } }
    }
}

/// The conversation in a transcript, in pi's message shape (as
/// `get_messages` returns), oldest first. Subagents' and meta lines are
/// left out (a subagent's own transcript, all sidechain, is read whole);
/// one message's per-block lines are joined.
pub fn history(text: &str) -> Vec<Value> {
    let mut reader = HistoryReader::default();
    crate::store::for_each_text_line(text, &mut |offset, line| reader.line(offset, line));
    reader.out
}

/// What a saved session shows before Claude runs: its conversation and model.
pub fn saved(text: &str) -> crate::store::SavedSession {
    let mut reader = HistoryReader::default();
    crate::store::for_each_text_line(text, &mut |offset, line| reader.line(offset, line));
    reader.saved()
}

/// [`saved`] of a file, holding a line of it at a time.
pub fn saved_file(path: &Path) -> std::io::Result<crate::store::SavedSession> {
    let mut reader = HistoryReader::default();
    crate::store::for_each_line(path, 0, &mut |offset, line| reader.line(offset, line))?;
    Ok(reader.saved())
}

/// The bytes just before `offset`, to recognise that place later.
fn check_at(path: &Path, offset: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let start = offset.saturating_sub(RESUME_CHECK_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; (offset - start) as usize];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// The messages of a transcript from where `resume` left off (they only ever
/// get added to), or all of them if it isn't there any more.
pub fn read_part(path: &Path, resume: Option<&Resume>) -> std::io::Result<Part> {
    let resume = resume.filter(|r| check_at(path, r.offset).is_ok_and(|check| check == r.check));
    let (from, first_index) = resume.map_or((0, 0), |r| (r.offset, r.index));
    let mut reader = HistoryReader { resume: (from, 0), ..Default::default() };
    crate::store::for_each_line(path, from, &mut |offset, line| reader.line(offset, line))?;
    let (offset, count) = reader.resume;
    Ok(Part { messages: reader.out, first_index, resume: Resume { offset, index: first_index + count as i64, check: check_at(path, offset)? } })
}

/// What's in a transcript's cache: its messages up to `index` (those before
/// its last prompt, which nothing added later changes), one a line in the
/// `.jsonl` next to it, `bytes` of it.
#[derive(serde::Serialize, serde::Deserialize)]
struct CacheMeta {
    version: u32,
    path: PathBuf,
    offset: u64,
    index: usize,
    check: Vec<u8>,
    bytes: u64,
    subagent: bool,
    custom: String,
    ai: String,
}

/// Bumped whenever what `HistoryReader` makes of a line changes.
const CACHE_VERSION: u32 = 2;
/// A cache not written to for this long goes.
const CACHE_KEPT: std::time::Duration = std::time::Duration::from_secs(30 * 86400);
/// One reader or writer of the caches at a time.
static CACHE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A reply's token use, as pi's messages say it (Null if it has none).
fn usage_of(u: &Value) -> Value {
    if !u.is_object() {
        return Value::Null;
    }
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let (input, write, read, output) = (n("input_tokens"), n("cache_creation_input_tokens"), n("cache_read_input_tokens"), n("output_tokens"));
    json!({ "input": input, "cacheWrite": write, "cacheRead": read, "output": output, "totalTokens": input + write + read + output })
}

/// The context the last reply sent (what its next request starts from).
pub fn context_used(messages: &[Value]) -> u64 {
    messages
        .iter()
        .rev()
        .find_map(|m| {
            let u = m.get("usage").filter(|_| m["role"] == "assistant")?;
            Some(["input", "cacheWrite", "cacheRead"].iter().filter_map(|k| u[*k].as_u64()).sum::<u64>()).filter(|n| *n > 0)
        })
        .unwrap_or(0)
}

/// Each model's window and where Claude compacts it, as Claude last said
/// (so a session shows them before Claude runs).
fn models_file() -> Option<PathBuf> {
    cache_dir().and_then(|d| d.parent().map(|p| p.join("claude-models.json")))
}

/// What Claude said last for `model` (`claude-…`, or `anthropic/claude-…`):
/// its window and where it compacts (0: it won't).
pub fn known_context(model: &str) -> Option<(u64, u64)> {
    let model = model.strip_prefix("anthropic/").unwrap_or(model);
    let known: Value = serde_json::from_str(&std::fs::read_to_string(models_file()?).ok()?).ok()?;
    let m = &known[model];
    Some((m["window"].as_u64().filter(|w| *w > 0)?, m["compactAt"].as_u64()?))
}

/// Keeps what Claude said for `model`, for [`known_context`].
pub fn remember_context(model: &str, window: u64, compact_at: u64) {
    let model = model.strip_prefix("anthropic/").unwrap_or(model);
    let Some(path) = models_file() else { return };
    if model.is_empty() || window == 0 || known_context(model) == Some((window, compact_at)) {
        return;
    }
    let mut known: Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
    known[model] = json!({ "window": window, "compactAt": compact_at });
    let tmp = path.with_extension("json.tmp");
    if path.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::write(&tmp, known.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Where transcripts' caches are kept.
pub fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|d| d.join("harlo/history"))
}

/// [`saved_file`], quicker for a long transcript: what an earlier read made
/// of it is kept in the cache, so only what Claude added since is read
/// (it only adds; a transcript that changed otherwise is read again).
pub fn saved_file_cached(path: &Path) -> std::io::Result<crate::store::SavedSession> {
    match cache_dir() {
        Some(dir) => saved_cached_in(path, &dir, crate::store::BIG_FILE),
        None => saved_file(path),
    }
}

/// A transcript's cache: what it holds, and its messages.
fn cache_files(dir: &Path, path: &Path) -> (PathBuf, PathBuf) {
    // (A stable name: FNV-1a of the path.)
    let hash = path.as_os_str().as_encoded_bytes().iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100_0000_01b3));
    (dir.join(format!("{hash:016x}.json")), dir.join(format!("{hash:016x}.jsonl")))
}

/// A deleted transcript's cache goes with it.
pub fn forget_cached(path: &Path) {
    let Some(dir) = cache_dir() else { return };
    let _one = CACHE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (meta, lines) = cache_files(&dir, path);
    let _ = std::fs::remove_file(meta);
    let _ = std::fs::remove_file(lines);
}

fn saved_cached_in(path: &Path, dir: &Path, from_size: u64) -> std::io::Result<crate::store::SavedSession> {
    if std::fs::metadata(path)?.len() < from_size {
        return saved_file(path);
    }
    let _one = CACHE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (meta_path, lines_path) = cache_files(dir, path);
    let mut reader = HistoryReader::default();
    let mut cached = (0, 0);
    let meta = std::fs::read(&meta_path).ok().and_then(|b| serde_json::from_slice::<CacheMeta>(&b).ok()).filter(|m| {
        m.version == CACHE_VERSION && m.path == path && check_at(path, m.offset).is_ok_and(|c| c == m.check)
    });
    if let Some(m) = meta {
        let mut out = Vec::with_capacity(m.index);
        let read = crate::store::for_each_line(&lines_path, 0, &mut |offset, line| {
            if offset < m.bytes
                && let Ok(v) = serde_json::from_str(line)
            {
                out.push(v);
            }
        });
        if read.is_ok() && out.len() == m.index {
            reader = HistoryReader { out, subagent: Some(m.subagent), custom: m.custom, ai: m.ai, resume: (m.offset, m.index), ..Default::default() };
            cached = (m.index, m.bytes);
        }
    }
    crate::store::for_each_line(path, reader.resume.0, &mut |offset, line| reader.line(offset, line))?;
    // What's settled now goes into the cache too.
    let (offset, settled) = reader.resume;
    if settled > cached.0
        && let Err(e) = write_cache(dir, &meta_path, &lines_path, &reader, cached, path, offset)
    {
        tracing::warn!("caching {}: {e}", path.display());
    }
    Ok(reader.saved())
}

fn write_cache(dir: &Path, meta_path: &Path, lines_path: &Path, reader: &HistoryReader, cached: (usize, u64), path: &Path, offset: u64) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    prune_caches(dir);
    // Its own words: only you can read it, as Claude's file.
    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(lines_path)?;
    file.set_len(cached.1)?;
    let mut out = std::io::BufWriter::with_capacity(1024 * 1024, std::io::Seek::seek(&mut file, std::io::SeekFrom::End(0)).map(|_| file)?);
    let mut bytes = cached.1;
    for m in &reader.out[cached.0..reader.resume.1] {
        let line = m.to_string();
        out.write_all(line.as_bytes())?;
        out.write_all(b"\n")?;
        bytes += line.len() as u64 + 1;
    }
    out.flush()?;
    let meta = CacheMeta {
        version: CACHE_VERSION,
        path: path.to_owned(),
        offset,
        index: reader.resume.1,
        check: check_at(path, offset)?,
        bytes,
        subagent: reader.subagent.unwrap_or(false),
        custom: reader.custom.clone(),
        ai: reader.ai.clone(),
    };
    let tmp = meta_path.with_extension("json.tmp");
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?.write_all(&serde_json::to_vec(&meta)?)?;
    std::fs::rename(tmp, meta_path)
}

/// Drops the caches of transcripts that are gone, and those not written to
/// for a month.
fn prune_caches(dir: &Path) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|age| age > CACHE_KEPT));
        let gone = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<CacheMeta>(&b).ok()).is_none_or(|m| !m.path.exists());
        if old || gone {
            let _ = std::fs::remove_file(p.with_extension("jsonl"));
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// The ids (`uuid`) of the prompts in a transcript, in the order harlo
/// shows them (as `history` reads them).
pub fn user_entries(path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let _ = crate::store::for_each_line(path, 0, &mut |_, line| user_entry(line, &mut out));
    out
}

/// `user_entries` from a transcript's text.
pub fn user_entries_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    text.lines().for_each(|line| user_entry(line, &mut out));
    out
}

fn user_entry(line: &str, out: &mut Vec<String>) {
    let Ok(head) = serde_json::from_str::<LineHead>(line) else { return };
    if head.kind != "user" || head.sidechain == true {
        return;
    }
    let Ok(e) = serde_json::from_str::<Value>(line) else { return };
    if user_text(&e).is_some() {
        out.extend(e["uuid"].as_str().map(str::to_owned));
    }
}

/// A fresh id, as Claude Code writes them.
pub fn new_uuid() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// A new session with the conversation before the prompt `entry` (as the
/// SDK's `forkSession`: the lines copied with fresh ids, beside the
/// original). Returns its id and the prompt's text.
pub fn fork_before(path: &Path, entry: &str) -> std::io::Result<(String, String)> {
    let text = std::fs::read_to_string(path)?;
    let Some((id, out, prompt)) = fork_text(&text, entry) else { return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "that message isn't in the session")) };
    std::fs::write(path.with_file_name(format!("{id}.jsonl")), out)?;
    Ok((id, prompt))
}

/// `fork_before` on a transcript's text: the new session's id, its text and
/// the prompt's text (None: that prompt isn't there).
pub fn fork_text(text: &str, entry: &str) -> Option<(String, String, String)> {
    let id = new_uuid();
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut out = String::new();
    let mut prompt = None;
    for line in text.lines() {
        let Ok(mut e) = serde_json::from_str::<Value>(line) else { continue };
        if e["uuid"] == entry {
            prompt = user_text(&e);
            break;
        }
        if let Some(obj) = e.as_object_mut() {
            for key in ["uuid", "parentUuid", "logicalParentUuid", "leafUuid"] {
                if let Some(old) = obj.get(key).and_then(Value::as_str).map(str::to_owned) {
                    let new = ids.entry(old).or_insert_with(new_uuid).clone();
                    obj.insert(key.to_owned(), json!(new));
                }
            }
            if obj.contains_key("sessionId") {
                obj.insert("sessionId".into(), json!(id));
            }
        }
        out.push_str(&e.to_string());
        out.push('\n');
    }
    Some((id, out, prompt?))
}

/// The subagents a saved session started (as harlo's dock lists them):
/// Claude Code keeps each one's transcript in `<session>/subagents/` with a
/// `.meta.json` naming the Task / Agent call. Nothing runs in a saved
/// session: the call's result says how it went (an error: failed; a
/// background launch: the transcript's end decides), else "ended".
pub fn subagents(session_file: &Path) -> Vec<Value> {
    let dir = session_file.with_extension("").join("subagents");
    let mut metas = Vec::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(id) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_prefix("agent-")).and_then(|n| n.strip_suffix(".meta.json")) {
                let Some(meta) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
                let transcript = path.with_file_name(format!("agent-{id}.jsonl"));
                let ended_well = std::fs::read_to_string(&transcript)
                    .ok()
                    .and_then(|t| t.lines().rev().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|e| e["type"] == "assistant"))
                    .is_some_and(|e| e["message"]["stop_reason"] == "end_turn" || e["message"]["stop_reason"].is_null());
                metas.push(SubagentMeta { agent_id: id.to_owned(), meta, transcript: transcript.is_file().then_some(transcript), ended_well });
            }
        }
    }
    if metas.is_empty() {
        return Vec::new();
    }
    subagents_read(metas, |each| {
        let _ = crate::store::for_each_line(session_file, 0, each);
    })
}

/// A subagent a transcript's folder records: its `.meta.json`, its
/// transcript (if any) and whether that ended with Claude's reply.
pub struct SubagentMeta {
    pub agent_id: String,
    pub meta: Value,
    pub transcript: Option<PathBuf>,
    pub ended_well: bool,
}

/// The dock's entries for `metas`, linked to the calls in the parent's
/// transcript (`text`), in the order they were made.
/// [`subagents_from`] with the parent's transcript in a file (read a line
/// at a time).
pub fn subagents_from_file(path: &Path, metas: Vec<SubagentMeta>) -> Vec<Value> {
    subagents_read(metas, |each| {
        let _ = crate::store::for_each_line(path, 0, each);
    })
}

pub fn subagents_from(text: &str, metas: Vec<SubagentMeta>) -> Vec<Value> {
    subagents_read(metas, |each| crate::store::for_each_text_line(text, each))
}

/// `subagents_from` with the parent's transcript read through `lines`.
fn subagents_read(metas: Vec<SubagentMeta>, lines: impl FnOnce(&mut dyn FnMut(u64, &str))) -> Vec<Value> {
    #[derive(serde::Deserialize)]
    struct Line {
        #[serde(default)]
        message: Value,
    }
    let call_of = |m: &SubagentMeta| m.meta["toolUseId"].as_str().map(str::to_owned).unwrap_or_else(|| m.agent_id.clone());
    let calls: HashSet<String> = metas.iter().map(call_of).collect();
    // The calls' results, and the order they were made in.
    let (mut results, mut order): (HashMap<String, (String, bool)>, Vec<String>) = Default::default();
    lines(&mut |_, line| {
        if !line.contains("tool_use") && !line.contains("tool_result") {
            return;
        }
        let Ok(e) = serde_json::from_str::<Line>(line) else { return };
        for b in e.message["content"].as_array().into_iter().flatten() {
            match b["type"].as_str() {
                Some("tool_use") => order.extend(b["id"].as_str().filter(|id| calls.contains(*id)).map(str::to_owned)),
                Some("tool_result") => {
                    if let Some(id) = b["tool_use_id"].as_str().filter(|id| calls.contains(*id)) {
                        results.insert(id.to_owned(), (result_text(&b["content"]), b["is_error"] == true));
                    }
                }
                _ => {}
            }
        }
    });
    let mut out: Vec<Value> = metas
        .into_iter()
        .map(|m| {
            let call = call_of(&m);
            let status = match results.get(&call) {
                Some((_, true)) => "failed",
                Some((text, false)) if text.starts_with("Async agent launched") => if m.ended_well { "completed" } else { "ended" },
                Some(_) => "completed",
                None => "ended",
            };
            let mut entry = json!({
                "id": call, "parentToolCallId": call, "name": m.meta["description"].as_str().unwrap_or("Subagent"),
                "agent": m.meta["agentType"].as_str().unwrap_or("general-purpose"), "status": status,
                "canCancel": false, "canSteer": false,
            });
            if let Some(t) = m.transcript {
                entry["sessionFile"] = json!(t.to_string_lossy());
            }
            entry
        })
        .collect();
    out.sort_by_key(|e| order.iter().position(|o| e["id"] == o.as_str()).unwrap_or(usize::MAX));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_reply_keeps_its_context_use() {
        let text = [
            r#"{"type":"user","message":{"role":"user","content":"hi"}}"#,
            r#"{"type":"assistant","message":{"id":"a","model":"claude-opus-5-5","content":[{"type":"thinking","thinking":"hm"}],"usage":{"input_tokens":3,"cache_creation_input_tokens":100,"cache_read_input_tokens":20000,"output_tokens":1}}}"#,
            r#"{"type":"assistant","message":{"id":"a","model":"claude-opus-5-5","content":[{"type":"text","text":"hello"}],"usage":{"input_tokens":3,"cache_creation_input_tokens":100,"cache_read_input_tokens":20000,"output_tokens":40}}}"#,
        ]
        .join("\n");
        let saved = saved(&text);
        let reply = saved.messages.iter().find(|m| m["role"] == "assistant").unwrap();
        assert_eq!(reply["usage"]["output"], 40, "the reply's last say");
        assert_eq!(context_used(&saved.messages), 20_103);
        assert_eq!(context_used(&saved.messages[..1]), 0);
    }

    #[test]
    fn quitting_isnt_activity() {
        let head = [
            r#"{"type":"user","cwd":"/p","timestamp":"2026-11-04T03:20:00.000Z","message":{"role":"user","content":"hi"}}"#,
            r#"{"type":"assistant","timestamp":"2026-11-04T03:21:40.500Z","message":{"content":[{"type":"text","text":"hello"}]}}"#,
            // (What Claude Code writes as it quits: no time.)
            r#"{"type":"cost-state","sessionId":"s","totalCostUSD":0.1}"#,
        ]
        .join("\n");
        let file_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_793_800_000);
        let info = session_from_text(Path::new("/p/s.jsonl"), 0, file_time, &head, "").unwrap();
        assert_eq!(info.modified, SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_793_762_500_500), "the reply's time, not the file's");
        // No times at all: the file's.
        let bare = r#"{"type":"user","cwd":"/p","message":{"role":"user","content":"hi"}}"#;
        assert_eq!(session_from_text(Path::new("/p/s.jsonl"), 0, file_time, bare, "").unwrap().modified, file_time);
    }

    #[test]
    fn models_by_version() {
        let all = models(Some("2.1.296 (Claude Code)"), None);
        assert_eq!(all.len(), 17, "every model but the access-only one");
        assert!(all.iter().any(|(id, _)| id == "claude-haiku-5-5"));
        let older = models(Some("2.1.200"), None);
        assert!(!older.iter().any(|(id, _)| id == "claude-opus-5-5" || id == "claude-haiku-5-5"));
        assert!(older.iter().any(|(id, _)| id == "claude-opus-4-8"));
    }

    #[test]
    fn folder_names() {
        assert_eq!(project_dir_name(Path::new("/nonexistent/a b/c_d.e")), "-nonexistent-a-b-c-d-e");
        let long = format!("/nonexistent/{}", "x".repeat(250));
        let name = project_dir_name(Path::new(&long));
        assert!(name.len() > 200 && name.starts_with("-nonexistent-xxx"));
    }

    /// A transcript: prompts, tool rounds, an assistant message on two lines,
    /// a subagent's aside and Claude Code's own records.
    fn transcript() -> String {
        let mut lines = Vec::new();
        for i in 0..5 {
            lines.push(format!(r#"{{"type":"user","uuid":"u{i}","isSidechain":false,"message":{{"role":"user","content":"prompt {i}"}}}}"#));
            lines.push(format!(r#"{{"type":"api-request","id":"r{i}","body":"{}"}}"#, "x".repeat(200)));
            lines.push(format!(r#"{{"type":"assistant","isSidechain":false,"message":{{"id":"a{i}","model":"claude-opus-5-5","content":[{{"type":"thinking","thinking":"hm"}}]}}}}"#));
            lines.push(format!(r#"{{"type":"assistant","isSidechain":false,"message":{{"id":"a{i}","model":"claude-opus-5-5","content":[{{"type":"tool_use","id":"t{i}","name":"Read","input":{{}}}}]}}}}"#));
            lines.push(format!(r#"{{"type":"assistant","isSidechain":true,"agentId":"x","message":{{"id":"s{i}","content":[{{"type":"text","text":"aside"}}]}}}}"#));
            lines.push(format!(r#"{{"type":"user","isSidechain":false,"message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t{i}","content":"done {i}"}}]}}}}"#));
            lines.push(format!(r#"{{"type":"assistant","isSidechain":false,"message":{{"id":"b{i}","model":"claude-opus-5-5","content":[{{"type":"text","text":"answer {i}"}}]}}}}"#));
        }
        lines.join("\n") + "\n"
    }

    fn temp_file(name: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("harlo-claude-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_file_reads_as_its_text_does() {
        let text = transcript();
        let path = temp_file("file", &text);
        let (file, whole) = (saved_file(&path).unwrap(), saved(&text));
        assert_eq!(file.messages, whole.messages);
        assert_eq!(file.model, whole.model);
        assert_eq!(file.name, whole.name);
        assert_eq!(whole.messages.len(), 20, "a prompt, a tool call, its result and an answer, five times");
        assert_eq!(user_entries(&path), user_entries_text(&text));
        assert_eq!(user_entries(&path), ["u0", "u1", "u2", "u3", "u4"]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Reading on from anywhere it could have stopped gives what reading the
    /// grown file from the start gives, wherever the file was cut.
    #[test]
    fn reads_on_from_where_it_left_off() {
        let text = transcript();
        let path = temp_file("part", "");
        let all = history(&text);
        for cut in text.char_indices().map(|(i, _)| i).step_by(37) {
            std::fs::write(&path, &text[..cut]).unwrap();
            let first = read_part(&path, None).unwrap();
            assert_eq!(first.first_index, 0);
            std::fs::write(&path, &text).unwrap();
            let next = read_part(&path, Some(&first.resume)).unwrap();
            assert_eq!(next.first_index, first.resume.index);
            let mut joined = first.messages[..first.resume.index as usize].to_vec();
            joined.extend(next.messages);
            assert_eq!(joined, all, "cut at {cut}");
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Opening a long transcript from its cache shows what reading all of it
    /// does: as it grows from wherever it was cut, after it was rewritten,
    /// and with its cache damaged.
    #[test]
    fn a_cached_transcript_reads_as_the_whole_file_does() {
        let text = transcript().replacen("\n", "\n{\"type\":\"custom-title\",\"customTitle\":\"Named\"}\n", 1);
        let path = temp_file("cached", "");
        let dir = path.parent().unwrap().join("cache");
        let same = |a: &crate::store::SavedSession, b: &crate::store::SavedSession| a.messages == b.messages && a.model == b.model && a.name == b.name;
        for cut in text.char_indices().map(|(i, _)| i).step_by(53) {
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::write(&path, &text[..cut]).unwrap();
            assert!(same(&saved_cached_in(&path, &dir, 0).unwrap(), &saved(&text[..cut])), "cut at {cut}");
            std::fs::write(&path, &text).unwrap();
            assert!(same(&saved_cached_in(&path, &dir, 0).unwrap(), &saved(&text)), "grown from {cut}");
            assert!(same(&saved_cached_in(&path, &dir, 0).unwrap(), &saved(&text)), "again from {cut}");
        }
        assert_eq!(saved_cached_in(&path, &dir, 0).unwrap().name, "Named");
        // (Claude only adds to a transcript; these are what could still happen.)
        let rewritten = text.replace("aside", "Aside").replace("answer", "Answer");
        std::fs::write(&path, &rewritten).unwrap();
        assert!(same(&saved_cached_in(&path, &dir, 0).unwrap(), &saved(&rewritten)), "rewritten");
        let shorter = &rewritten[..rewritten.len() / 3];
        std::fs::write(&path, shorter).unwrap();
        assert!(same(&saved_cached_in(&path, &dir, 0).unwrap(), &saved(shorter)), "cut shorter");
        std::fs::write(&path, &rewritten).unwrap();
        assert!(same(&saved_cached_in(&path, &dir, 0).unwrap(), &saved(&rewritten)), "grown again");
        for f in std::fs::read_dir(&dir).unwrap().flatten().filter(|f| f.path().extension().is_some_and(|x| x == "jsonl")) {
            std::fs::write(f.path(), "not json\n").unwrap();
        }
        assert!(same(&saved_cached_in(&path, &dir, 0).unwrap(), &saved(&rewritten)), "damaged");
        // A transcript that's gone takes its cache with it.
        let other = path.with_file_name("t.jsonl");
        std::fs::write(&other, &text).unwrap();
        saved_cached_in(&other, &dir, 0).unwrap();
        std::fs::remove_file(&other).unwrap();
        std::fs::write(&path, text.clone() + &text).unwrap();
        saved_cached_in(&path, &dir, 0).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "only the one transcript's");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_rewritten_transcript_is_read_again_from_the_start() {
        let text = transcript();
        let path = temp_file("rewritten", &text);
        let part = read_part(&path, None).unwrap();
        assert!(part.resume.offset > 0);

        std::fs::write(&path, text.replace("aside", "Aside")).unwrap();
        let again = read_part(&path, Some(&part.resume)).unwrap();
        assert_eq!(again.first_index, 0);
        assert_eq!(again.messages.len(), part.messages.len());

        std::fs::write(&path, "").unwrap();
        let again = read_part(&path, Some(&part.resume)).unwrap();
        assert_eq!((again.first_index, again.messages.len()), (0, 0), "a shorter file too");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn subagents_keep_only_their_own_calls() {
        let text = format!(
            "{}\n{}\n{}\n{}\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"other","name":"Read"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"task","name":"Task"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"other","content":"x","is_error":true}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"task","content":"fine"}]}}"#,
        );
        let meta = SubagentMeta { agent_id: "a".into(), meta: json!({ "toolUseId": "task" }), transcript: None, ended_well: true };
        let list = subagents_from(&text, vec![meta]);
        assert_eq!((list[0]["id"].as_str(), list[0]["status"].as_str()), (Some("task"), Some("completed")));
    }

    #[test]
    fn leaves_out_the_pi_extensions_copies() {
        let user = |entry: &str, origin: &str, mode: &str| format!(r#"{{"type":"user","entrypoint":"{entry}","turnOrigin":"{origin}","permissionMode":"{mode}","cwd":"/w","timestamp":"2026-11-01T08:00:00Z","isSidechain":false,"message":{{"role":"user","content":"how are you"}}}}"#);
        let listed = |text: &str| session_from_text(Path::new("/p/s.jsonl"), 0, SystemTime::UNIX_EPOCH, text, "").is_some();
        let tools = r#"{"type":"attachment","attachment":{"type":"deferred_tools_delta","addedNames":["WebFetch"]}}"#;
        let its_tools = r#"{"type":"attachment","attachment":{"type":"deferred_tools_delta","addedNames":["mcp__custom-tools__read"]}}"#;

        // Started by the extension, without its tools listed or with them.
        assert!(!listed(&user("sdk-ts", "sdk", "bypassPermissions")));
        assert!(!listed(&format!("{}\n{its_tools}", user("sdk-ts", "sdk", "bypassPermissions"))));
        // History it wrote: its library says "cli", with made-up ids.
        let written = r#"{"type":"assistant","entrypoint":"cli","isSidechain":false,"message":{"id":"msg_syn_0123","role":"assistant","content":[]}}"#;
        assert!(!listed(&format!("{}\n{written}", user("cli", "human", "default"))));

        // harlo's own Claude, bypassing permissions or not, and in a terminal.
        assert!(listed(&format!("{}\n{tools}", user("sdk-ts", "human", "bypassPermissions"))));
        assert!(listed(&format!("{}\n{tools}", user("sdk-ts", "sdk", "default"))));
        assert!(listed(&format!("{}\n{tools}", user("sdk-ts", "sdk", "bypassPermissions"))));
        assert!(listed(&user("cli", "human", "auto")));
        // A message about these names is only a message.
        let about = r#"{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"text","text":"\"id\":\"msg_syn_ and \"mcp__custom-tools__"}]}}"#;
        assert!(listed(&format!("{}\n{tools}\n{about}", user("sdk-ts", "human", "bypassPermissions"))));
    }

    #[test]
    fn reads_a_transcript() {
        let text = [
            r#"{"type":"permission-mode","permissionMode":"default","sessionId":"s1"}"#,
            r#"{"type":"user","cwd":"/w","timestamp":"2026-10-13T08:00:00Z","isSidechain":false,"message":{"role":"user","content":"Run cat notes.txt"}}"#,
            r#"{"type":"assistant","isSidechain":false,"message":{"id":"m1","model":"claude-haiku-4-5","role":"assistant","content":[{"type":"thinking","thinking":"ok"}]}}"#,
            r#"{"type":"assistant","isSidechain":false,"message":{"id":"m1","model":"claude-haiku-4-5","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cat notes.txt"}}]}}"#,
            r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"hello"}]}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"id":"x","role":"assistant","content":[{"type":"text","text":"subagent"}]}}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>…"}}"#,
            r#"{"type":"assistant","isSidechain":false,"message":{"id":"m2","model":"claude-haiku-4-5","role":"assistant","content":[{"type":"text","text":"hello"}]}}"#,
            r#"{"type":"ai-title","aiTitle":"Read the notes","sessionId":"s1"}"#,
            r#"{"type":"user","uuid":"n1","isSidechain":false,"origin":{"kind":"task-notification"},"message":{"role":"user","content":"<task-notification>\n<task-id>b1</task-id>\n<status>stopped</status>\n<summary>Background shell command didn't finish before the previous session ended</summary>\n</task-notification>"}}"#,
        ]
        .join("\n");
        let h = history(&text);
        assert_eq!(h.len(), 5, "{h:#?}");
        // Claude Code's notice that a background task ended isn't the user's.
        assert_eq!(h[4], json!({"role":"custom","customType":"task-notification","display":true,"content":"Background shell command didn't finish before the previous session ended"}));
        assert!(!user_entries_text(&text).contains(&"n1".to_owned()), "and not one of their prompts");
        assert_eq!(h[0], json!({"role":"user","content":[{"type":"text","text":"Run cat notes.txt"}]}));
        assert_eq!(h[1]["content"].as_array().unwrap().len(), 2, "thinking and the tool call joined");
        assert_eq!(h[1]["content"][1]["type"], "toolCall");
        assert_eq!(h[2]["role"], "toolResult");
        assert_eq!(h[3]["content"][0]["text"], "hello");
        assert_eq!(saved(&text).model, "anthropic/claude-haiku-4-5");

        // A picture a tool returned (Read of a .png) comes as an image block,
        // not the word "[image]"; text beside it stays text.
        let read = [
            r#"{"type":"assistant","isSidechain":false,"message":{"id":"m3","role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"/w/a.png"}}]}}"#,
            r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":[{"type":"text","text":"a picture"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}]}]}}"#,
        ]
        .join("\n");
        let h = history(&read);
        let result = h.iter().find(|m| m["role"] == "toolResult").unwrap();
        assert_eq!(result["content"], json!([{"type":"text","text":"a picture"},{"type":"image","data":"iVBORw0KGgo=","mimeType":"image/png"}]));

        let dir = std::env::temp_dir().join(format!("harlo-claude-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, &text).unwrap();
        let info = read_session(&path).unwrap();
        assert_eq!((info.id.as_str(), info.cwd.as_path(), info.name.as_str(), info.first_message.as_str()), ("s1", Path::new("/w"), "Read the notes", "Run cat notes.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn thinking_levels_go_by_the_model_however_its_named() {
        assert_eq!(thinking_levels("anthropic/claude-haiku-4-5"), thinking_levels("claude-haiku-4-5"));
        assert!(!thinking_levels("anthropic/claude-haiku-4-5").contains(&"xhigh"));
        assert!(thinking_levels("").contains(&"high"), "a model not known yet");
    }

    #[test]
    fn a_moved_session_runs_where_it_last_was() {
        let dir = std::env::temp_dir().join(format!("harlo-claude-moved-{}", std::process::id()));
        let inner = dir.join("repo/crates/app");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::create_dir(dir.join("repo/.git")).unwrap();
        let line = |cwd: &Path, side: bool| format!(r#"{{"type":"user","isSidechain":{side},"cwd":"{}","message":{{"role":"user","content":"hi"}}}}"#, cwd.display());
        let path = dir.join("s1.jsonl");
        let gone = dir.join("old-name");
        let write = |lines: &[String]| std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        // Still there: as it started, wherever it went since.
        write(&[line(&dir, false), line(&inner, false)]);
        assert_eq!(read_session(&path).unwrap().cwd, dir);
        // Renamed: the repository it's in now.
        write(&[line(&gone, false), line(&inner, false), line(&gone, true)]);
        assert_eq!(read_session(&path).unwrap().cwd, dir.join("repo"));
        // Nowhere left: as it was.
        write(&[line(&gone, false)]);
        assert_eq!(read_session(&path).unwrap().cwd, gone);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn adds_the_settings_model() {
        let dir = std::env::temp_dir().join(format!("harlo-claude-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"model":"opus"}"#).unwrap();
        let list = models(Some("2.1.296"), Some(&path));
        assert_eq!(list.last().unwrap().0, "opus");
        std::fs::write(&path, r#"{"model":"claude-opus-4-8"}"#).unwrap();
        assert_eq!(models(Some("2.1.296"), Some(&path)).len(), 17);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
