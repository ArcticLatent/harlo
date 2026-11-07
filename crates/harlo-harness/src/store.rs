//! Session files on disk. The sidebar is built by reading the session JSONL
//! files directly.
//!
//! Harlo's: `$XDG_DATA_HOME/harlo-agent/sessions/--<abs-path>--/<ts>_<uuid>.jsonl`
//! in pi's session format. The first line is the `session` header; the
//! display name comes from the latest `session_info` entry. (A file may also
//! start with a fixed-width `title` slot, rewritten in place on rename, with
//! the header on line 2.) Claude Code's are read by `claude.rs`.
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
    /// Explicit name (a title slot, or `session_info`); empty if unnamed.
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
        // harlo-agent's, in pi's file layout.
        HarnessKind::Harlo => Some(
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".local/share"))
                .join("harlo-agent/sessions"),
        ),
        // Claude Code's are listed their own way.
        HarnessKind::Claude | HarnessKind::Demo => None,
    }
}

/// Every session file of one harness, with its modification time and size.
/// Cheap (stat only); use it to decide which files need [`read_session`].
pub fn list_files(kind: HarnessKind) -> Vec<(PathBuf, SystemTime, u64)> {
    if kind == HarnessKind::Claude {
        return crate::claude::list_files();
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
            // The latest session_info wins; an absent name clears it.
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
    /// Last selected model id (`modelId`, or `provider/id`).
    pub model: String,
    /// The same with its provider, as harlo names models (`provider/id`;
    /// "" when the file doesn't say).
    pub model_id: String,
    pub thinking: String,
    /// The name it was given ("" if none).
    pub name: String,
}

/// A file this big is worth handing its memory back for, once it's read.
pub const BIG_FILE: u64 = 32 * 1024 * 1024;

/// After reading a file of `size` bytes: what a big one needed goes back to
/// the system (freed memory otherwise stays with the process, gigabytes of it
/// after a huge session).
pub fn after_reading(size: u64) {
    if size >= BIG_FILE {
        give_back();
    }
}

/// Returns freed memory to the system.
pub fn give_back() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        // SAFETY: only returns free memory to the system.
        unsafe { malloc_trim(0) };
    }
}

/// Every line of a file from `from` on, with where it starts, one at a time
/// (a session file can be bigger than anything worth holding at once). A line
/// that isn't text is passed over.
pub(crate) fn for_each_line(path: &Path, from: u64, each: &mut dyn FnMut(u64, &str)) -> std::io::Result<()> {
    use std::io::{BufRead, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(from))?;
    let mut reader = std::io::BufReader::with_capacity(1024 * 1024, file);
    let (mut offset, mut line) = (from, Vec::new());
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        if let Ok(text) = std::str::from_utf8(&line) {
            each(offset, text.trim_end_matches(['\n', '\r']));
        }
        offset += read as u64;
    }
}

/// [`for_each_line`] for text already read.
pub(crate) fn for_each_text_line(text: &str, each: &mut dyn FnMut(u64, &str)) {
    for line in text.lines() {
        each((line.as_ptr() as usize - text.as_ptr() as usize) as u64, line);
    }
}

/// Reads the current branch plus the last model and thinking level.
pub fn read_saved(path: &Path) -> std::io::Result<SavedSession> {
    // (Missing or unreadable: said now. Lines are read twice below.)
    drop(std::fs::File::open(path)?);
    Ok(saved_from(&|each| {
        let _ = for_each_line(path, 0, each);
    }))
}

/// [`read_saved`] for any harness's session file (a long Claude one from
/// its cache).
pub fn read_saved_for(kind: HarnessKind, path: &Path) -> std::io::Result<SavedSession> {
    if kind == HarnessKind::Claude { crate::claude::saved_file_cached(path) } else { read_saved(path) }
}

/// [`parse_saved`] for any harness's session file.
pub fn parse_saved_for(kind: HarnessKind, text: &str) -> SavedSession {
    if kind == HarnessKind::Claude { crate::claude::saved(text) } else { parse_saved(text) }
}

/// [`read_saved`] from the file's text.
pub fn parse_saved(text: &str) -> SavedSession {
    saved_from(&|each| for_each_text_line(text, each))
}

/// What's read of each entry at first: most of a long session isn't on the
/// current branch, or isn't a message.
#[derive(serde::Deserialize)]
struct EntryHead {
    #[serde(rename = "type", default)]
    kind: Value,
    #[serde(default)]
    id: Value,
    #[serde(rename = "parentId", default)]
    parent: Value,
    #[serde(default)]
    name: Value,
    #[serde(rename = "modelId", default)]
    model_id: Value,
    #[serde(default)]
    model: Value,
    #[serde(default)]
    provider: Value,
    #[serde(rename = "thinkingLevel", default)]
    thinking: Value,
}

/// The current branch from `lines` (which calls back with each line and where
/// it starts): the tree first, then only the messages on that branch.
fn saved_from(lines: &dyn Fn(&mut dyn FnMut(u64, &str))) -> SavedSession {
    let mut entries: Vec<(u64, Option<String>)> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let (mut model, mut thinking, mut name) = (String::new(), String::new(), String::new());
    let mut model_id = String::new();
    lines(&mut |offset, line| {
        let Ok(entry) = serde_json::from_str::<EntryHead>(line) else { return };
        match entry.kind.as_str() {
            // The latest session_info wins.
            Some("session_info") => name = entry.name.as_str().unwrap_or_default().trim().to_owned(),
            Some("model_change") => {
                model = entry.model_id.as_str().or_else(|| entry.model.as_str()).unwrap_or_default().to_owned();
                // `provider` + `modelId`, or `model` as `provider/id`.
                model_id = match entry.provider.as_str() {
                    Some(p) if !p.is_empty() && !model.is_empty() => format!("{p}/{model}"),
                    _ => model.clone(),
                };
            }
            Some("thinking_level_change") => {
                thinking = entry.thinking.as_str().unwrap_or_default().to_owned();
            }
            _ => {}
        }
        if let Some(id) = entry.id.as_str().filter(|_| entry.kind != "session") {
            index.insert(id.to_owned(), entries.len());
            entries.push((offset, entry.parent.as_str().map(str::to_owned)));
        }
    });
    let mut branch = Vec::new();
    let mut cursor = entries.len().checked_sub(1);
    let mut guard = 0;
    while let Some(i) = cursor {
        branch.push(i);
        cursor = entries[i].1.as_deref().and_then(|p| index.get(p).copied());
        guard += 1;
        if guard > entries.len() {
            break; // cycle
        }
    }
    branch.reverse();
    let places: std::collections::HashMap<u64, usize> = branch.iter().enumerate().map(|(place, &i)| (entries[i].0, place)).collect();
    let mut found: Vec<Option<Value>> = vec![None; branch.len()];
    lines(&mut |offset, line| {
        let Some(&place) = places.get(&offset) else { return };
        let Ok(mut e) = serde_json::from_str::<Value>(line) else { return };
        found[place] = match e["type"].as_str() {
            Some("message") => Some(e["message"].take()),
            // Custom messages (e.g. background job results).
            Some("custom_message") => Some(serde_json::json!({
                "role": "custom", "customType": e["customType"], "content": e["content"], "display": e["display"].as_bool().unwrap_or(true),
            })),
            Some("compaction") => Some(serde_json::json!({"role": "compactionSummary", "summary": e["summary"]})),
            Some("branch_summary") => Some(serde_json::json!({"role": "branchSummary", "summary": e["summary"]})),
            _ => None,
        };
    });
    let messages: Vec<Value> = found.into_iter().flatten().collect();
    // No model change in it: the model of its last reply.
    if !model_id.contains('/') {
        let last = messages.iter().rev().find(|m| m["role"] == "assistant" && m["provider"].is_string() && m["model"].is_string());
        if let Some(m) = last {
            model_id = format!("{}/{}", m["provider"].as_str().unwrap_or_default(), m["model"].as_str().unwrap_or_default());
        }
    }
    SavedSession { messages, model, model_id, thinking, name }
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

    #[test]
    fn a_saved_session_knows_its_models_provider() {
        let pi = r#"{"type":"model_change","id":"a","parentId":null,"provider":"qwen3-4b","modelId":"Qwen3-4B-UD-Q4_K_XL"}"#;
        let s = parse_saved(pi);
        assert_eq!((s.model.as_str(), s.model_id.as_str()), ("Qwen3-4B-UD-Q4_K_XL", "qwen3-4b/Qwen3-4B-UD-Q4_K_XL"));
        let omp = r#"{"type":"model_change","id":"a","parentId":null,"model":"llama.cpp/Qwen3-4B-UD-Q4_K_XL"}"#;
        assert_eq!(parse_saved(omp).model_id, "llama.cpp/Qwen3-4B-UD-Q4_K_XL");
        // No model change: its last reply's.
        let reply = r#"{"type":"message","id":"m","parentId":null,"message":{"role":"assistant","content":[],"provider":"qwen3-4b","model":"Qwen3-4B"}}"#;
        assert_eq!(parse_saved(reply).model_id, "qwen3-4b/Qwen3-4B");
    }

    fn write(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let mut f = File::create(&path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        path
    }

    /// A folder removed when the test ends (passed or failed).
    struct TmpDir(PathBuf);

    impl std::ops::Deref for TmpDir {
        type Target = PathBuf;
        fn deref(&self) -> &PathBuf {
            &self.0
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmp_dir(tag: &str) -> TmpDir {
        let dir = std::env::temp_dir().join(format!("harlo-store-test-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TmpDir(dir)
    }

    #[test]
    fn reads_a_session() {
        let dir = tmp_dir("pi");
        let path = write(&dir, "a.jsonl", &[
            r#"{"type":"session","version":3,"id":"abc","timestamp":"2026-10-07T23:51:20.693Z","cwd":"/home/u/proj"}"#,
            r#"{"type":"message","id":"1","parentId":null,"message":{"role":"system","content":"sys"}}"#,
            r#"{"type":"message","id":"2","parentId":"1","message":{"role":"user","content":[{"type":"text","text":"\n  Fix the build\nplease"}]}}"#,
            r#"{"type":"session_info","id":"3","parentId":"2","name":"Build fix"}"#,
        ]);
        let info = read_session(HarnessKind::Harlo, &path).unwrap();
        assert_eq!(info.id, "abc");
        assert_eq!(info.cwd, PathBuf::from("/home/u/proj"));
        assert_eq!(info.first_message, "Fix the build");
        assert_eq!(info.display_title(), "Build fix");
    }

    #[test]
    fn reads_a_title_slot() {
        let dir = tmp_dir("slot");
        let path = write(&dir, "b.jsonl", &[
            r#"{"type":"title","v":1,"title":"Houdini setup","updatedAt":"x","pad":"   "}"#,
            r#"{"type":"session","version":3,"id":"def","timestamp":"t","cwd":"/tmp"}"#,
            r#"{"type":"message","id":"1","message":{"role":"user","content":"hello"}}"#,
        ]);
        let info = read_session(HarnessKind::Harlo, &path).unwrap();
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
        assert!(read_session(HarnessKind::Harlo, &path).is_none());
    }
}
