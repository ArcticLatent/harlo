//! Claude Code as a harness (its stream-json mode): its models, its
//! session files (`~/.claude/projects/<folder>/<id>.jsonl`) and their
//! conversation in pi's message shape. The running session is
//! `claude_run.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::claude_run::{result_text, to_pi_block};
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
/// (tool results, meta, interruptions, command output).
fn user_text(entry: &Value) -> Option<String> {
    if entry["isMeta"] == true || entry["isCompactSummary"] == true {
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
    session_from_text(path, size, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), &head, &tail)
}

/// `read_session` from a transcript's start and end (a remote host's, read
/// over SSH).
pub fn session_from_text(path: &Path, size: u64, modified: SystemTime, head: &str, tail: &str) -> Option<SessionInfo> {
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
        modified,
        size,
        parent_session: None,
    })
}

/// The conversation in a transcript, in pi's message shape (as
/// `get_messages` returns), oldest first. Subagents' and meta lines are
/// left out (a subagent's own transcript, all sidechain, is read whole);
/// one message's per-block lines are joined.
pub fn history(text: &str) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut last_assistant: Option<String> = None;
    let subagent = text.lines().find_map(|l| serde_json::from_str::<Value>(l).ok()).is_some_and(|e| e["isSidechain"] == true && e["agentId"].is_string());
    for line in text.lines() {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        if e["isSidechain"] == true && !subagent {
            continue;
        }
        match e["type"].as_str() {
            Some("user") => {
                last_assistant = None;
                let content = &e["message"]["content"];
                if let Some(parts) = content.as_array()
                    && parts.iter().any(|p| p["type"] == "tool_result")
                {
                    for p in parts.iter().filter(|p| p["type"] == "tool_result") {
                        out.push(json!({ "role": "toolResult", "toolCallId": p["tool_use_id"], "content": [{ "type": "text", "text": result_text(&p["content"]) }], "isError": p["is_error"].as_bool().unwrap_or(false) }));
                    }
                } else if let Some(text) = user_text(&e) {
                    out.push(json!({ "role": "user", "content": [{ "type": "text", "text": text }] }));
                }
            }
            Some("assistant") => {
                let id = e["message"]["id"].as_str().unwrap_or_default().to_owned();
                let blocks: Vec<Value> = e["message"]["content"].as_array().into_iter().flatten().filter(|b| b["type"] != "redacted_thinking").map(to_pi_block).collect();
                match (last_assistant.as_deref(), out.last_mut()) {
                    (Some(prev), Some(m)) if prev == id && !id.is_empty() => {
                        if let Some(c) = m["content"].as_array_mut() {
                            c.extend(blocks);
                        }
                    }
                    _ => {
                        out.push(json!({ "role": "assistant", "content": blocks, "model": e["message"]["model"] }));
                        last_assistant = Some(id);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// What a saved session shows before Claude runs: its conversation and model.
pub fn saved(text: &str) -> crate::store::SavedSession {
    let messages = history(text);
    let model = messages.iter().rev().find_map(|m| m["model"].as_str().filter(|s| !s.is_empty())).map(|m| format!("anthropic/{m}")).unwrap_or_default();
    // The name: one you gave (custom-title), else Claude's (ai-title).
    let (mut custom, mut ai) = (String::new(), String::new());
    for line in text.lines().filter(|l| l.contains("custom-title") || l.contains("ai-title")) {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(t) = e["customTitle"].as_str() {
            custom = t.to_owned();
        }
        if let Some(t) = e["aiTitle"].as_str() {
            ai = t.to_owned();
        }
    }
    crate::store::SavedSession { messages, model, thinking: String::new(), name: if custom.is_empty() { ai } else { custom } }
}

/// The ids (`uuid`) of the prompts in a transcript, in the order harlo
/// shows them (as `history` reads them).
pub fn user_entries(path: &Path) -> Vec<String> {
    user_entries_text(&std::fs::read_to_string(path).unwrap_or_default())
}

/// `user_entries` from a transcript's text.
pub fn user_entries_text(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| e["type"] == "user" && e["isSidechain"] != true && user_text(e).is_some())
        .filter_map(|e| e["uuid"].as_str().map(str::to_owned))
        .collect()
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
    subagents_from(&std::fs::read_to_string(session_file).unwrap_or_default(), metas)
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
pub fn subagents_from(text: &str, metas: Vec<SubagentMeta>) -> Vec<Value> {
    // The calls' results, and the order they were made in.
    let (mut results, mut order): (HashMap<String, (String, bool)>, Vec<String>) = Default::default();
    for line in text.lines().filter(|l| l.contains("tool_use") || l.contains("tool_result")) {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        for b in e["message"]["content"].as_array().into_iter().flatten() {
            match b["type"].as_str() {
                Some("tool_use") => order.extend(b["id"].as_str().map(str::to_owned)),
                Some("tool_result") => {
                    if let Some(id) = b["tool_use_id"].as_str() {
                        results.insert(id.to_owned(), (result_text(&b["content"]), b["is_error"] == true));
                    }
                }
                _ => {}
            }
        }
    }
    let mut out: Vec<Value> = metas
        .into_iter()
        .map(|m| {
            let call = m.meta["toolUseId"].as_str().map(str::to_owned).unwrap_or_else(|| m.agent_id.clone());
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
        ]
        .join("\n");
        let h = history(&text);
        assert_eq!(h.len(), 4, "{h:#?}");
        assert_eq!(h[0], json!({"role":"user","content":[{"type":"text","text":"Run cat notes.txt"}]}));
        assert_eq!(h[1]["content"].as_array().unwrap().len(), 2, "thinking and the tool call joined");
        assert_eq!(h[1]["content"][1]["type"], "toolCall");
        assert_eq!(h[2]["role"], "toolResult");
        assert_eq!(h[3]["content"][0]["text"], "hello");
        assert_eq!(saved(&text).model, "anthropic/claude-haiku-4-5");

        let dir = std::env::temp_dir().join(format!("harlo-claude-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, &text).unwrap();
        let info = read_session(&path).unwrap();
        assert_eq!((info.id.as_str(), info.cwd.as_path(), info.name.as_str(), info.first_message.as_str()), ("s1", Path::new("/w"), "Read the notes", "Run cat notes.txt"));
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
