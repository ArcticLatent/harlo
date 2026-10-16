//! Full-text search over every session's messages: an SQLite FTS5 index
//! (`search.db` next to harlo's database), kept up to date from the session
//! files. Hits name the message's position on the session's current branch,
//! which is how the transcript keys saved rows (`h<index>`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::Value;

/// One matching message.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub path: PathBuf,
    /// Position on the session's current branch.
    pub index: i64,
    pub role: String,
    /// Rich text: the matching passage, matches in bold.
    pub snippet: String,
}

pub struct SearchIndex {
    conn: Connection,
}

pub fn default_path() -> PathBuf {
    crate::db::default_path().with_file_name("search.db")
}

fn stamp(path: &Path) -> Option<(i64, i64)> {
    let m = std::fs::metadata(path).ok()?;
    let modified = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64;
    Some((modified, m.len() as i64))
}

/// The searchable text of a message: what the user wrote (without attached
/// file contents or harlo's notes) or what the assistant answered.
fn message_text(message: &Value) -> Option<(String, String)> {
    let role = message["role"].as_str()?;
    if role != "user" && role != "assistant" {
        return None;
    }
    let content = &message["content"];
    let mut text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    if role == "user" {
        let (_, rest) = crate::attach::split_blocks(&text);
        let (rest, _) = crate::starters::strip(&rest);
        let rest = harlo_harness::file_blocks::strip_ask_first(rest).0;
        text = harlo_harness::file_blocks::strip_plan(rest).0.to_owned();
    }
    let text = text.trim().to_owned();
    (!text.is_empty()).then(|| (role.to_owned(), text))
}

/// FTS5 query for what the user typed: every word must appear (as a prefix).
fn fts_query(input: &str) -> Option<String> {
    let words: Vec<String> = input
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.').collect::<String>())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"*", w.replace('"', "")))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn snippet_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 16);
    for c in raw.chars() {
        match c {
            '\u{2}' => out.push_str("<b>"),
            '\u{3}' => out.push_str("</b>"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

impl SearchIndex {
    pub fn open(path: &Path) -> Result<SearchIndex> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS files (path TEXT PRIMARY KEY, modified INTEGER NOT NULL, size INTEGER NOT NULL);
             CREATE VIRTUAL TABLE IF NOT EXISTS messages USING fts5(
                 text, path UNINDEXED, idx UNINDEXED, role UNINDEXED,
                 tokenize = 'unicode61 remove_diacritics 2');",
        )?;
        Ok(SearchIndex { conn })
    }

    /// Re-indexes session files that changed since last time and forgets
    /// ones that are gone. Returns how many files were (re)indexed.
    pub fn update(&mut self, files: &[PathBuf]) -> Result<usize> {
        let stamped: Vec<(PathBuf, i64, i64)> = files.iter().filter_map(|f| stamp(f).map(|(m, s)| (f.clone(), m, s))).collect();
        self.update_with(&stamped, |f| harlo_harness::store::read_messages(f).unwrap_or_default())
    }

    /// `update` for sessions anywhere: each with the stamp it has (a remote
    /// host's, an OpenCode database row's), read by `read` when it changed.
    pub fn update_with(&mut self, files: &[(PathBuf, i64, i64)], read: impl Fn(&Path) -> Vec<Value>) -> Result<usize> {
        let wanted: HashSet<&Path> = files.iter().map(|p| p.0.as_path()).collect();
        let known: Vec<(String, i64, i64)> = {
            let mut stmt = self.conn.prepare("SELECT path, modified, size FROM files")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
        };
        let tx = self.conn.transaction()?;
        for (path, _, _) in known.iter().filter(|(p, _, _)| !wanted.contains(Path::new(p))) {
            tx.execute("DELETE FROM messages WHERE path = ?1", params![path])?;
            tx.execute("DELETE FROM files WHERE path = ?1", params![path])?;
        }
        let mut indexed = 0;
        for (file, modified, size) in files {
            let (modified, size) = (*modified, *size);
            let key = file.to_string_lossy();
            if known.iter().any(|(p, m, s)| *p == key && *m == modified && *s == size) {
                continue;
            }
            let messages = read(file);
            tx.execute("DELETE FROM messages WHERE path = ?1", params![key])?;
            for (i, m) in messages.iter().enumerate() {
                if let Some((role, text)) = message_text(m) {
                    tx.execute("INSERT INTO messages (text, path, idx, role) VALUES (?1, ?2, ?3, ?4)", params![text, key, i as i64, role])?;
                }
            }
            tx.execute(
                "INSERT INTO files (path, modified, size) VALUES (?1, ?2, ?3)
                 ON CONFLICT(path) DO UPDATE SET modified = excluded.modified, size = excluded.size",
                params![key, modified, size],
            )?;
            indexed += 1;
        }
        tx.commit()?;
        Ok(indexed)
    }

    /// Best matches first.
    pub fn search(&self, input: &str, limit: usize) -> Result<Vec<Hit>> {
        let Some(query) = fts_query(input) else { return Ok(Vec::new()) };
        let mut stmt = self.conn.prepare(
            "SELECT path, idx, role, snippet(messages, 0, char(2), char(3), '…', 16)
             FROM messages WHERE messages MATCH ?1 ORDER BY bm25(messages) LIMIT ?2",
        )?;
        let hits = stmt
            .query_map(params![query, limit as i64], |r| {
                Ok(Hit {
                    path: PathBuf::from(r.get::<_, String>(0)?),
                    index: r.get(1)?,
                    role: r.get(2)?,
                    snippet: snippet_html(&r.get::<_, String>(3)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(dir: &Path, name: &str, messages: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        let mut lines = vec![serde_json::json!({"type":"session","version":3,"id":name,"timestamp":"2026-10-11T12:00:00.000Z","cwd":"/tmp"}).to_string()];
        let mut parent = Value::Null;
        for (i, (role, text)) in messages.iter().enumerate() {
            let id = format!("m{i}");
            lines.push(serde_json::json!({"type":"message","id":id,"parentId":parent,"timestamp":"2026-10-11T12:00:01.000Z","message":{"role":role,"content":[{"type":"text","text":text}]}}).to_string());
            parent = Value::String(id);
        }
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    #[test]
    fn indexes_and_finds_messages() {
        let dir = std::env::temp_dir().join(format!("harlo-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = session(&dir, "a.jsonl", &[("user", "Why does the cache need a lock?"), ("assistant", "Two workers write the cache at once.")]);
        let b = session(&dir, "b.jsonl", &[("user", "<file name=\"/x/secret.md\">\nlockfile content\n</file>\nRename the parser"), ("assistant", "Renamed it.")]);
        let mut index = SearchIndex::open(&dir.join("search.db")).unwrap();
        assert_eq!(index.update(&[a.clone(), b.clone()]).unwrap(), 2);
        assert_eq!(index.update(&[a.clone(), b.clone()]).unwrap(), 0, "unchanged files are skipped");

        let hits = index.search("cache lock", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].path.clone(), hits[0].index, hits[0].role.as_str()), (a.clone(), 0, "user"));
        assert!(hits[0].snippet.contains("<b>cache</b>"), "{}", hits[0].snippet);
        assert_eq!(index.search("work", 10).unwrap()[0].index, 1, "words match as prefixes");
        assert!(index.search("lockfile", 10).unwrap().is_empty(), "attached file contents are not indexed");
        assert_eq!(index.search("parser", 10).unwrap()[0].path, b);

        std::fs::remove_file(&b).unwrap();
        index.update(&[a]).unwrap();
        assert!(index.search("parser", 10).unwrap().is_empty(), "deleted sessions drop out");
        assert!(index.search("  \"\" ", 10).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
