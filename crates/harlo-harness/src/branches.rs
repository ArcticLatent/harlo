//! Branches of a session file. pi and omp sessions are append-only trees
//! of entries (`id` / `parentId`); editing an earlier message starts a new
//! branch, and the last written entry marks the current one. These read
//! every branch and can copy one into a new session file.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct BranchInfo {
    /// Id of the branch's last entry.
    pub leaf: String,
    pub current: bool,
    /// Messages from the root to the leaf.
    pub messages: usize,
    /// The branch's last user message, first line.
    pub last_user: String,
    /// The first user message that is not on the current branch (empty for
    /// the current branch itself).
    pub diverges_at: String,
    /// Timestamp of the leaf entry (ISO 8601).
    pub timestamp: String,
}

struct Tree {
    header: Option<Value>,
    entries: Vec<Value>,
    index: HashMap<String, usize>,
}

fn load(path: &Path) -> std::io::Result<Tree> {
    Ok(parse(&std::fs::read_to_string(path)?))
}

fn parse(text: &str) -> Tree {
    let mut tree = Tree { header: None, entries: Vec::new(), index: HashMap::new() };
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
        if entry["type"] == "session" {
            tree.header = Some(entry);
            continue;
        }
        if let Some(id) = entry["id"].as_str() {
            tree.index.insert(id.to_owned(), tree.entries.len());
            tree.entries.push(entry);
        }
    }
    tree
}

impl Tree {
    /// Entry indices from the root to `leaf`.
    fn path(&self, leaf: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut cursor = Some(leaf);
        while let Some(i) = cursor {
            if out.len() > self.entries.len() {
                break; // a cycle
            }
            out.push(i);
            cursor = self.entries[i]["parentId"].as_str().and_then(|p| self.index.get(p).copied());
        }
        out.reverse();
        out
    }
}

fn user_text(entry: &Value) -> Option<String> {
    let m = &entry["message"];
    if entry["type"] != "message" || m["role"] != "user" {
        return None;
    }
    let text = match &m["content"] {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    };
    let line = crate::file_blocks::title_text(&text);
    Some(line.chars().take(120).collect())
}

/// Every branch that contains a user message, newest first.
pub fn branches(path: &Path) -> std::io::Result<Vec<BranchInfo>> {
    Ok(branches_in(&load(path)?))
}

/// `branches` of a session file's text (a remote host's, read over SSH).
pub fn branches_text(text: &str) -> Vec<BranchInfo> {
    branches_in(&parse(text))
}

fn branches_in(tree: &Tree) -> Vec<BranchInfo> {
    let parents: HashSet<&str> = tree.entries.iter().filter_map(|e| e["parentId"].as_str()).collect();
    let current_leaf = tree.entries.len().checked_sub(1);
    let current_path: HashSet<usize> = current_leaf.map(|l| tree.path(l).into_iter().collect()).unwrap_or_default();
    let mut out = Vec::new();
    for (i, entry) in tree.entries.iter().enumerate() {
        let Some(id) = entry["id"].as_str() else { continue };
        if parents.contains(id) {
            continue;
        }
        let path = tree.path(i);
        let users: Vec<(usize, String)> = path.iter().filter_map(|&j| user_text(&tree.entries[j]).map(|t| (j, t))).collect();
        let Some((_, last_user)) = users.last().cloned() else { continue };
        let current = Some(i) == current_leaf;
        out.push(BranchInfo {
            leaf: id.to_owned(),
            current,
            messages: path.iter().filter(|&&j| tree.entries[j]["type"] == "message").count(),
            last_user,
            diverges_at: if current { String::new() } else { users.iter().find(|(j, _)| !current_path.contains(j)).map(|(_, t)| t.clone()).unwrap_or_default() },
            timestamp: entry["timestamp"].as_str().unwrap_or_default().to_owned(),
        });
    }
    out.sort_by(|a, b| b.current.cmp(&a.current).then(b.timestamp.cmp(&a.timestamp)));
    out
}

/// Entry ids of the user messages on the current branch, in order. (omp's
/// `get_branch_messages` lists the user messages of every branch.)
pub fn current_user_entries(path: &Path) -> std::io::Result<Vec<String>> {
    Ok(current_user_entries_text(&std::fs::read_to_string(path)?))
}

/// `current_user_entries` of a session file's text.
pub fn current_user_entries_text(text: &str) -> Vec<String> {
    let tree = parse(text);
    let Some(leaf) = tree.entries.len().checked_sub(1) else { return Vec::new() };
    tree
        .path(leaf)
        .into_iter()
        .filter(|&j| user_text(&tree.entries[j]).is_some())
        .filter_map(|j| tree.entries[j]["id"].as_str().map(str::to_owned))
        .collect()
}

/// The messages from the root to `leaf` (as `read_saved` shows them).
pub fn branch_messages(path: &Path, leaf: &str) -> std::io::Result<Vec<Value>> {
    let tree = load(path)?;
    let Some(&i) = tree.index.get(leaf) else { return Ok(Vec::new()) };
    Ok(tree.path(i).into_iter().filter(|&j| tree.entries[j]["type"] == "message").map(|j| tree.entries[j]["message"].clone()).collect())
}

/// Writes the branch ending at `leaf` as a new session file in the same
/// folder (header copied, a fresh id, `parentSession` naming the original).
/// Returns the new file's path.
pub fn write_branch(path: &Path, leaf: &str) -> std::io::Result<PathBuf> {
    write_branch_in(path, leaf, path.parent().unwrap_or(Path::new(".")))
}

/// Like `write_branch`, into `dir` (a temporary preview, say).
pub fn write_branch_in(path: &Path, leaf: &str, dir: &Path) -> std::io::Result<PathBuf> {
    let (name, text) = branch_file(&std::fs::read_to_string(path)?, leaf, &path.to_string_lossy())?;
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(name);
    std::fs::write(&dest, text)?;
    Ok(dest)
}

/// The branch ending at `leaf` of a session file's text, as a new session
/// file: its name and text (`original`: the parent session's path).
pub fn branch_file(text: &str, leaf: &str, original: &str) -> std::io::Result<(String, String)> {
    let tree = parse(text);
    let Some(&i) = tree.index.get(leaf) else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such entry"));
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let id = format!("{:016x}{:08x}", now.as_millis(), now.subsec_nanos());
    let stamp = chrono_like(now.as_secs(), now.subsec_millis());
    let mut header = tree.header.clone().unwrap_or_else(|| json!({ "type": "session", "version": 3 }));
    header["id"] = json!(id);
    header["timestamp"] = json!(stamp);
    header["parentSession"] = json!(original);
    let mut lines = vec![header.to_string()];
    lines.extend(tree.path(i).into_iter().map(|j| tree.entries[j].to_string()));
    Ok((format!("{}_{id}.jsonl", stamp.replace([':', '.'], "-")), lines.join("\n") + "\n"))
}

/// `2026-10-11T12:34:56.789Z` from Unix seconds (UTC), without a date crate.
pub(crate) fn chrono_like(secs: u64, millis: u32) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, parent: Option<&str>, role: &str, text: &str, ts: &str) -> String {
        json!({"type":"message","id":id,"parentId":parent,"timestamp":ts,"message":{"role":role,"content":[{"type":"text","text":text}]}}).to_string()
    }

    #[test]
    fn lists_and_copies_branches() {
        let dir = std::env::temp_dir().join(format!("harlo-branches-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        // u1 → a1 → u2 → a2 (old branch); then u2' edited from a1 → a2' (current).
        let lines = [
            json!({"type":"session","version":3,"id":"orig","timestamp":"2026-10-11T10:00:00.000Z","cwd":"/tmp"}).to_string(),
            msg("u1", None, "user", "Add a cache", "2026-10-11T10:00:01Z"),
            msg("a1", Some("u1"), "assistant", "Which kind?", "2026-10-11T10:00:02Z"),
            msg("u2", Some("a1"), "user", "An LRU one", "2026-10-11T10:00:03Z"),
            msg("a2", Some("u2"), "assistant", "Done (LRU).", "2026-10-11T10:00:04Z"),
            msg("u3", Some("a1"), "user", "A TTL one", "2026-10-11T10:01:00Z"),
            msg("a3", Some("u3"), "assistant", "Done (TTL).", "2026-10-11T10:01:01Z"),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        let list = branches(&path).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].current && list[0].leaf == "a3" && list[0].last_user == "A TTL one" && list[0].diverges_at.is_empty());
        assert_eq!((list[1].leaf.as_str(), list[1].diverges_at.as_str(), list[1].messages), ("a2", "An LRU one", 4));

        assert_eq!(current_user_entries(&path).unwrap(), ["u1", "u3"], "only the current branch");
        let old = branch_messages(&path, "a2").unwrap();
        assert_eq!(old.len(), 4);
        assert_eq!(old[3]["content"][0]["text"], "Done (LRU).");

        let copy = write_branch(&path, "a2").unwrap();
        let saved = crate::store::read_saved(&copy).unwrap();
        assert_eq!(saved.messages.len(), 4, "the copy's current branch is the chosen one");
        let header: Value = serde_json::from_str(std::fs::read_to_string(&copy).unwrap().lines().next().unwrap()).unwrap();
        assert_eq!(header["parentSession"], path.to_string_lossy().as_ref());
        assert_ne!(header["id"], "orig");
        assert_eq!(chrono_like(0, 5), "1970-01-01T00:00:00.005Z");
        assert_eq!(chrono_like(1_760_183_696, 0), "2025-10-11T11:54:56.000Z");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
