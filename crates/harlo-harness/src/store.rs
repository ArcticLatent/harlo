//! Session files on disk. Neither harness can list sessions over RPC, so
//! the sidebar is built by reading the session JSONL files directly.
//!
//! - pi:  `~/.pi/agent/sessions/--<abs-path>--/<ts>_<uuid>.jsonl`. The first
//!   line is the `session` header; the display name comes from the latest
//!   `session_info` entry.
//! - omp: `~/.omp/agent/sessions/<encoded-cwd>/<ts>_<uuid>.jsonl`. Line 1 is
//!   a fixed-width `title` slot rewritten in place on rename; line 2 is the
//!   header. Legacy files start directly with the header.
//!
//! Only the head and tail of large files are read.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::event::HarnessKind;

const FULL_READ_LIMIT: u64 = 512 * 1024;
const HEAD_BYTES: u64 = 64 * 1024;
const TAIL_BYTES: u64 = 64 * 1024;
const TITLE_CHARS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub harness: HarnessKind,
    pub path: PathBuf,
    pub id: String,
    pub cwd: PathBuf,
    /// Explicit name (omp title slot, pi `session_info`); empty if unnamed.
    pub name: String,
    /// First line of the first user message.
    pub first_message: String,
    /// Header timestamp (ISO 8601) as written by the harness.
    pub created: String,
    pub modified: SystemTime,
    pub size: u64,
    pub parent_session: Option<String>,
}

impl SessionInfo {
    /// Name if set, else the first prompt, else a placeholder.
    pub fn display_title(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else if !self.first_message.is_empty() {
            &self.first_message
        } else {
            "Untitled session"
        }
    }
}

pub fn sessions_root(kind: HarnessKind) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    match kind {
        HarnessKind::Pi => Some(
            std::env::var_os("PI_CODING_AGENT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".pi/agent"))
                .join("sessions"),
        ),
        HarnessKind::Omp => Some(home.join(".omp/agent/sessions")),
        // Their sessions are listed their own way (not pi's file layout).
        HarnessKind::Claude | HarnessKind::OpenCode | HarnessKind::Hermes | HarnessKind::Demo => None,
    }
}

/// Every session file of one harness, with its modification time and size.
/// Cheap (stat only); use it to decide which files need [`read_session`].
pub fn list_files(kind: HarnessKind) -> Vec<(PathBuf, SystemTime, u64)> {
    if kind == HarnessKind::Claude {
        return crate::claude::list_files();
    }
    if kind == HarnessKind::OpenCode {
        return crate::opencode::list_sessions();
    }
    if crate::acp::is_acp(kind) {
        return crate::acp::list_sessions(kind);
    }
    let Some(root) = sessions_root(kind) else { return Vec::new() };
    let Ok(dirs) = std::fs::read_dir(&root) else { return Vec::new() };
    let mut out = Vec::new();
    for dir in dirs.flatten() {
        if !dir.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(dir.path()) else { continue };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            if let Ok(meta) = f.metadata()
                && meta.is_file() {
                    out.push((path, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len()));
                }
        }
    }
    out
}

/// Reads the summary of one session file. Returns `None` for files without
/// a session header (not a session, or still being created).
pub fn read_session(kind: HarnessKind, path: &Path) -> Option<SessionInfo> {
    if kind == HarnessKind::Claude {
        return crate::claude::read_session(path);
    }
    if kind == HarnessKind::OpenCode {
        return crate::opencode::read_session(path);
    }
    if crate::acp::is_acp(kind) {
        return crate::acp::read_session(path);
    }
    let meta = std::fs::metadata(path).ok()?;
    let size = meta.len();
    let (head, tail) = read_parts(path, size)?;
    session_from_text(kind, path, size, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), &head, tail.as_deref())
}

/// A file's text: all of it up to [`FULL_READ_LIMIT`], else its first and
/// last parts (cut to complete lines).
fn read_parts(path: &Path, size: u64) -> Option<(String, Option<String>)> {
    let mut file = File::open(path).ok()?;
    if size <= FULL_READ_LIMIT {
        let mut all = String::new();
        file.read_to_string(&mut all).ok()?;
        return Some((all, None));
    }
    let mut head = vec![0u8; HEAD_BYTES as usize];
    file.read_exact(&mut head).ok()?;
    file.seek(SeekFrom::End(-(TAIL_BYTES as i64))).ok()?;
    let mut tail = vec![0u8; TAIL_BYTES as usize];
    file.read_exact(&mut tail).ok()?;
    Some((lossy_complete_lines(&head, false), Some(lossy_complete_lines(&tail, true))))
}

/// [`read_parts`] with the tail as "" for a small file.
pub(crate) fn head_and_tail(path: &Path, size: u64) -> Option<(String, String)> {
    read_parts(path, size).map(|(h, t)| (h, t.unwrap_or_default()))
}

/// How much of a session file [`session_from_text`] needs: all of it up to
/// this size, else its first and last [`PART_BYTES`].
pub const FULL_READ_LIMIT_BYTES: u64 = FULL_READ_LIMIT;
pub const PART_BYTES: u64 = HEAD_BYTES;

/// [`read_session`] from text already read (a remote file): the whole
/// file, or its head and tail (each cut to complete lines).
pub fn session_from_text(kind: HarnessKind, path: &Path, size: u64, modified: SystemTime, head: &str, tail: Option<&str>) -> Option<SessionInfo> {
    if kind == HarnessKind::Claude {
        return crate::claude::session_from_text(path, size, modified, head, tail.unwrap_or_default());
    }
    let mut info = SessionInfo {
        harness: kind,
        path: path.to_owned(),
        id: String::new(),
        cwd: PathBuf::new(),
        name: String::new(),
        first_message: String::new(),
        created: String::new(),
        modified,
        size,
        parent_session: None,
    };
    let mut have_header = false;
    let mut title_slot = String::new();

    let lines = head.lines().chain(tail.unwrap_or_default().lines());
    for line in lines {
        let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
        match entry["type"].as_str() {
            Some("title") => title_slot = entry["title"].as_str().unwrap_or_default().trim().to_owned(),
            Some("session") if !have_header => {
                have_header = true;
                info.id = entry["id"].as_str().unwrap_or_default().to_owned();
                info.cwd = PathBuf::from(entry["cwd"].as_str().unwrap_or_default());
                info.created = entry["timestamp"].as_str().unwrap_or_default().to_owned();
                info.parent_session = entry["parentSession"].as_str().map(str::to_owned);
                if let Some(t) = entry["title"].as_str() {
                    info.name = t.trim().to_owned();
                }
            }
            // pi: the latest session_info wins; an absent name clears it.
            Some("session_info") => info.name = entry["name"].as_str().unwrap_or_default().trim().to_owned(),
            Some("title_change") => {
                if let Some(t) = entry["title"].as_str() {
                    info.name = t.trim().to_owned();
                }
            }
            Some("message") if info.first_message.is_empty() && entry["message"]["role"] == "user" => {
                info.first_message = first_line(&message_text(&entry["message"]["content"]), TITLE_CHARS);
            }
            _ => {}
        }
    }
    if !title_slot.is_empty() {
        info.name = title_slot;
    }
    have_header.then_some(info)
}

/// Messages on the session's current branch, oldest first, in the same
/// `AgentMessage` shape that `get_messages` returns.
///
/// Session files are trees (`id`/`parentId`); the current branch is the path
/// from the last entry back to the root. Compaction and branch summaries are
/// returned as `compactionSummary` / `branchSummary` messages.
pub fn read_messages(path: &Path) -> std::io::Result<Vec<Value>> {
    Ok(read_saved(path)?.messages)
}

/// What a saved session shows before its harness runs.
#[derive(Debug, Default)]
pub struct SavedSession {
    pub messages: Vec<Value>,
    /// Last selected model id (`modelId`, or omp's `provider/id`).
    pub model: String,
    pub thinking: String,
    /// The name it was given ("" if none).
    pub name: String,
}

/// Reads the current branch plus the last model and thinking level.
pub fn read_saved(path: &Path) -> std::io::Result<SavedSession> {
    Ok(parse_saved(&std::fs::read_to_string(path)?))
}

/// [`parse_saved`] for any harness's session file.
pub fn parse_saved_for(kind: HarnessKind, text: &str) -> SavedSession {
    if kind == HarnessKind::Claude { crate::claude::saved(text) } else { parse_saved(text) }
}

/// [`read_saved`] from the file's text.
pub fn parse_saved(text: &str) -> SavedSession {
    let mut entries: Vec<Value> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let (mut model, mut thinking, mut name) = (String::new(), String::new(), String::new());
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
        match entry["type"].as_str() {
            // pi: the latest session_info wins.
            Some("session_info") => name = entry["name"].as_str().unwrap_or_default().trim().to_owned(),
            Some("model_change") => {
                model = entry["modelId"].as_str().or_else(|| entry["model"].as_str()).unwrap_or_default().to_owned();
            }
            Some("thinking_level_change") => {
                thinking = entry["thinkingLevel"].as_str().unwrap_or_default().to_owned();
            }
            _ => {}
        }
        if let Some(id) = entry["id"].as_str().filter(|_| entry["type"] != "session") {
            index.insert(id.to_owned(), entries.len());
            entries.push(entry);
        }
    }
    let mut branch = Vec::new();
    let mut cursor = entries.len().checked_sub(1);
    let mut guard = 0;
    while let Some(i) = cursor {
        branch.push(i);
        cursor = entries[i]["parentId"].as_str().and_then(|p| index.get(p).copied());
        guard += 1;
        if guard > entries.len() {
            break; // cycle
        }
    }
    branch.reverse();
    let messages = branch
        .into_iter()
        .filter_map(|i| {
            let e = &entries[i];
            match e["type"].as_str() {
                Some("message") => Some(e["message"].clone()),
                // omp's own messages (e.g. background job results).
                Some("custom_message") => Some(serde_json::json!({
                    "role": "custom", "customType": e["customType"], "content": e["content"], "display": e["display"].as_bool().unwrap_or(true),
                })),
                Some("compaction") => Some(serde_json::json!({"role": "compactionSummary", "summary": e["summary"]})),
                Some("branch_summary") => Some(serde_json::json!({"role": "branchSummary", "summary": e["summary"]})),
                _ => None,
            }
        })
        .collect();
    SavedSession { messages, model, thinking, name }
}

/// Converts bytes to text, dropping a partial first line (tail reads) or
/// partial last line (head reads).
pub fn lossy_complete_lines(bytes: &[u8], drop_first: bool) -> String {
    let text = String::from_utf8_lossy(bytes);
    if drop_first {
        text.split_once('\n').map(|(_, rest)| rest.to_owned()).unwrap_or_default()
    } else {
        text.rsplit_once('\n').map(|(start, _)| start.to_owned()).unwrap_or_default()
    }
}

fn message_text(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_owned();
    }
    content
        .as_array()
        .and_then(|blocks| blocks.iter().find_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten()))
        .unwrap_or_default()
        .to_owned()
}

fn first_line(s: &str, max: usize) -> String {
    // Attached files (`<file>` blocks) don't make a title.
    let line = crate::file_blocks::title_text(s);
    let line = line.as_str();
    let mut out: String = line.chars().take(max).collect();
    if out.len() < line.len() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let mut f = File::create(&path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        path
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("harlo-store-test-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_pi_session() {
        let dir = tmp_dir("pi");
        let path = write(&dir, "a.jsonl", &[
            r#"{"type":"session","version":3,"id":"abc","timestamp":"2026-10-07T23:51:20.693Z","cwd":"/home/u/proj"}"#,
            r#"{"type":"message","id":"1","parentId":null,"message":{"role":"system","content":"sys"}}"#,
            r#"{"type":"message","id":"2","parentId":"1","message":{"role":"user","content":[{"type":"text","text":"\n  Fix the build\nplease"}]}}"#,
            r#"{"type":"session_info","id":"3","parentId":"2","name":"Build fix"}"#,
        ]);
        let info = read_session(HarnessKind::Pi, &path).unwrap();
        assert_eq!(info.id, "abc");
        assert_eq!(info.cwd, PathBuf::from("/home/u/proj"));
        assert_eq!(info.first_message, "Fix the build");
        assert_eq!(info.display_title(), "Build fix");
    }

    #[test]
    fn reads_omp_title_slot() {
        let dir = tmp_dir("omp");
        let path = write(&dir, "b.jsonl", &[
            r#"{"type":"title","v":1,"title":"Houdini setup","updatedAt":"x","pad":"   "}"#,
            r#"{"type":"session","version":3,"id":"def","timestamp":"t","cwd":"/tmp"}"#,
            r#"{"type":"message","id":"1","message":{"role":"user","content":"hello"}}"#,
        ]);
        let info = read_session(HarnessKind::Omp, &path).unwrap();
        assert_eq!((info.name.as_str(), info.first_message.as_str()), ("Houdini setup", "hello"));
    }

    #[test]
    fn follows_the_current_branch() {
        let dir = tmp_dir("branch");
        let path = write(&dir, "d.jsonl", &[
            r#"{"type":"session","id":"s","cwd":"/w"}"#,
            r#"{"type":"message","id":"a","parentId":null,"message":{"role":"user","content":"root"}}"#,
            r#"{"type":"message","id":"b","parentId":"a","message":{"role":"assistant","content":"abandoned"}}"#,
            r#"{"type":"message","id":"c","parentId":"a","message":{"role":"assistant","content":"kept"}}"#,
            r#"{"type":"compaction","id":"d","parentId":"c","summary":"sum"}"#,
        ]);
        let msgs = read_messages(&path).unwrap();
        let roles: Vec<_> = msgs.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["user", "assistant", "compactionSummary"]);
        assert_eq!(msgs[1]["content"], "kept");
    }

    #[test]
    fn rejects_files_without_header() {
        let dir = tmp_dir("bad");
        let path = write(&dir, "c.jsonl", &[r#"{"type":"message"}"#]);
        assert!(read_session(HarnessKind::Pi, &path).is_none());
    }
}
