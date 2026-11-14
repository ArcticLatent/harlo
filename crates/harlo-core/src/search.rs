//! Full-text search over every session's messages: an SQLite FTS5 index
//! (`search.db` next to harlo's database), kept up to date from the session
//! files. Hits name the message's position on the session's current branch,
//! which is how the transcript keys saved rows (`h<index>`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use anyhow::Result;
use harlo_harness::claude::{Part, Resume};
use harlo_harness::store::{BIG_FILE, after_reading};
use rusqlite::{Connection, OptionalExtension, params};
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

/// How often one big file that's being written to is read again.
const BIG_FILE_INTERVAL: Duration = Duration::from_secs(60);

pub struct SearchIndex {
    conn: Connection,
    /// When each big file was last read.
    big_reads: HashMap<String, Instant>,
}

/// A second connection to the index, for searching while it's being
/// updated (the index is in WAL mode: a search sees what's committed and
/// never waits for the update).
pub struct SearchReader {
    conn: Connection,
}

impl SearchReader {
    /// The index at `path` (open it with [`SearchIndex::open`] first).
    pub fn open(path: &Path) -> Result<SearchReader> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA query_only = 1;")?;
        conn.busy_timeout(Duration::from_millis(200))?;
        Ok(SearchReader { conn })
    }

    /// Best matches first.
    pub fn search(&self, input: &str, limit: usize) -> Result<Vec<Hit>> {
        // (Tests: a slow disk or a big index.)
        if let Some(ms) = std::env::var("HARLO_TEST_SLOW_SEARCH_MS").ok().and_then(|v| v.parse::<u64>().ok()) {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
        search_in(&self.conn, input, limit)
    }
}

/// How many files the indexer reads before it commits them, so searches
/// see them as it goes; and the longest it holds them.
const BATCH_FILES: usize = 20;
const BATCH_TIME: Duration = Duration::from_millis(500);

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
    let words: Vec<String> = query_words(input).map(|w| format!("\"{w}\"*")).collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn query_words(input: &str) -> impl Iterator<Item = String> {
    input
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.').collect::<String>())
        .filter(|w| !w.is_empty())
}

/// Which of a message's blocks a search for `input` found: the first with the
/// most of its words (0 if none has any).
pub fn best_block<S: AsRef<str>>(blocks: &[S], input: &str) -> usize {
    let words: Vec<String> = query_words(input).map(|w| w.to_lowercase()).collect();
    let mut best = (0, 0);
    for (i, block) in blocks.iter().enumerate() {
        let text = block.as_ref().to_lowercase();
        let found = words.iter().filter(|w| text.contains(w.as_str())).count();
        if found > best.1 {
            best = (i, found);
        }
    }
    best.0
}

/// A hit's snippet as styled text: its matches in `<b>`, markdown's
/// backticks and `**` left out (the text reads as it does in the session).
fn snippet_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 16);
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next();
            continue;
        }
        match c {
            '`' => {}
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

/// Deletes `path`'s messages from `from` on, by their rows.
fn forget(conn: &Connection, path: &str, from: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM messages WHERE rowid IN (SELECT row FROM message_rows WHERE path = ?1 AND idx >= ?2)", params![path, from])?;
    conn.execute("DELETE FROM message_rows WHERE path = ?1 AND idx >= ?2", params![path, from])?;
    Ok(())
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
        let has_resume = conn.prepare("SELECT 1 FROM pragma_table_info('files') WHERE name = 'resume_offset'")?.exists([])?;
        if !has_resume {
            conn.execute_batch(
                "ALTER TABLE files ADD COLUMN resume_offset INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE files ADD COLUMN resume_index INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE files ADD COLUMN resume_check BLOB;",
            )?;
        }
        // (Which rows of `messages` each file has: deleting a file's rows
        // by `path` would scan the whole full-text table, once per file.)
        let has_rows = |c: &Connection| c.prepare("SELECT 1 FROM sqlite_master WHERE name = 'message_rows'")?.exists([]);
        if !has_rows(&conn)? {
            let tx = rusqlite::Transaction::new_unchecked(&conn, rusqlite::TransactionBehavior::Immediate)?;
            if !has_rows(&tx)? {
                tx.execute_batch(
                    "CREATE TABLE message_rows (row INTEGER PRIMARY KEY, path TEXT NOT NULL, idx INTEGER NOT NULL);
                     CREATE INDEX message_rows_path ON message_rows (path, idx);
                     INSERT INTO message_rows (row, path, idx) SELECT rowid, path, idx FROM messages;",
                )?;
            }
            tx.commit()?;
        }
        Ok(SearchIndex { conn, big_reads: HashMap::new() })
    }

    /// Re-indexes session files that changed since last time and forgets
    /// ones that are gone. Returns how many files were (re)indexed.
    pub fn update(&mut self, files: &[PathBuf]) -> Result<usize> {
        let stamped: Vec<(PathBuf, i64, i64)> = files.iter().filter_map(|f| stamp(f).map(|(m, s)| (f.clone(), m, s))).collect();
        self.update_with(&stamped, |f| harlo_harness::store::read_messages(f).unwrap_or_default())
    }

    /// `update` for sessions anywhere: each with the stamp it has (a remote
    /// host's), read by `read` when it changed.
    pub fn update_with(&mut self, files: &[(PathBuf, i64, i64)], read: impl Fn(&Path) -> Vec<Value>) -> Result<usize> {
        self.update_parts(files, |p, _| Part { messages: read(p), ..Default::default() })
    }

    /// `update_with` where `read` may read on from where a file was left
    /// (it's given that place; what it returns replaces the messages from
    /// its `first_index` on).
    pub fn update_parts(&mut self, files: &[(PathBuf, i64, i64)], read: impl Fn(&Path, Option<&Resume>) -> Part) -> Result<usize> {
        self.update_batched(files, read, || {})
    }

    /// `update_parts`, committing as it goes ([`BATCH_FILES`] files, or
    /// [`BATCH_TIME`]): `committed` is called after each commit that
    /// changed something.
    pub fn update_batched(&mut self, files: &[(PathBuf, i64, i64)], read: impl Fn(&Path, Option<&Resume>) -> Part, mut committed: impl FnMut()) -> Result<usize> {
        let wanted: HashSet<&Path> = files.iter().map(|p| p.0.as_path()).collect();
        let known: HashMap<String, (i64, i64)> = {
            let mut stmt = self.conn.prepare("SELECT path, modified, size FROM files")?;
            stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?.collect::<rusqlite::Result<_>>()?
        };
        let mut tx = self.conn.transaction()?;
        let mut pending = 0;
        for path in known.keys().filter(|p| !wanted.contains(Path::new(p))) {
            forget(&tx, path, 0)?;
            tx.execute("DELETE FROM files WHERE path = ?1", params![path])?;
            pending += 1;
        }
        let mut indexed = 0;
        let mut biggest = 0;
        let mut since = Instant::now();
        for (file, modified, size) in files {
            let (modified, size) = (*modified, *size);
            let key = file.to_string_lossy();
            if known.get(key.as_ref()) == Some(&(modified, size)) {
                continue;
            }
            // (What's read so far, committed: searches see it.)
            if pending > 0 && (pending >= BATCH_FILES || since.elapsed() >= BATCH_TIME) {
                tx.commit()?;
                committed();
                tx = self.conn.transaction()?;
                pending = 0;
                since = Instant::now();
            }
            // (A big one that's being written to: it's caught up on later.)
            if size as u64 >= BIG_FILE {
                if self.big_reads.get(key.as_ref()).is_some_and(|at| at.elapsed() < BIG_FILE_INTERVAL) {
                    continue;
                }
                self.big_reads.insert(key.to_string(), Instant::now());
                biggest = biggest.max(size as u64);
            }
            let was_known = known.contains_key(key.as_ref());
            let resume = if !was_known {
                None
            } else {
                tx.query_row("SELECT resume_offset, resume_index, resume_check FROM files WHERE path = ?1 AND resume_offset > 0", params![key], |r| {
                    Ok(Resume { offset: r.get::<_, i64>(0)? as u64, index: r.get(1)?, check: r.get::<_, Option<Vec<u8>>>(2)?.unwrap_or_default() })
                })
                .optional()?
            };
            let part = read(file, resume.as_ref());
            if was_known {
                forget(&tx, &key, part.first_index)?;
            }
            for (i, m) in part.messages.iter().enumerate() {
                if let Some((role, text)) = message_text(m) {
                    let idx = part.first_index + i as i64;
                    tx.execute("INSERT INTO messages (text, path, idx, role) VALUES (?1, ?2, ?3, ?4)", params![text, key, idx, role])?;
                    tx.execute("INSERT INTO message_rows (row, path, idx) VALUES (?1, ?2, ?3)", params![tx.last_insert_rowid(), key, idx])?;
                }
            }
            tx.execute(
                "INSERT INTO files (path, modified, size, resume_offset, resume_index, resume_check) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(path) DO UPDATE SET modified = excluded.modified, size = excluded.size,
                     resume_offset = excluded.resume_offset, resume_index = excluded.resume_index, resume_check = excluded.resume_check",
                params![key, modified, size, part.resume.offset as i64, part.resume.index, part.resume.check],
            )?;
            indexed += 1;
            pending += 1;
        }
        tx.commit()?;
        if pending > 0 {
            committed();
        }
        after_reading(biggest);
        Ok(indexed)
    }

    /// Best matches first.
    pub fn search(&self, input: &str, limit: usize) -> Result<Vec<Hit>> {
        search_in(&self.conn, input, limit)
    }
}

/// Best matches first, on either connection.
fn search_in(conn: &Connection, input: &str, limit: usize) -> Result<Vec<Hit>> {
    let Some(query) = fts_query(input) else { return Ok(Vec::new()) };
    let mut stmt = conn.prepare(
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
    fn snippets_leave_out_markdown() {
        assert_eq!(snippet_html("the raw `\u{2}returnTo\u{3}` and **bold** 2*3 <x>"), "the raw <b>returnTo</b> and bold 2*3 &lt;x&gt;");
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

    #[test]
    fn a_search_never_waits_for_the_indexer() {
        let dir = std::env::temp_dir().join(format!("harlo-search-batches-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let files: Vec<PathBuf> = (0..45).map(|i| session(&dir, &format!("s{i}.jsonl"), &[("user", &format!("zebra number {i}"))])).collect();
        let db = dir.join("search.db");
        let mut index = SearchIndex::open(&db).unwrap();
        let reader = SearchReader::open(&db).unwrap();
        let stamped: Vec<(PathBuf, i64, i64)> = files.iter().map(|f| stamp(f).map(|(m, s)| (f.clone(), m, s)).unwrap()).collect();
        let seen = std::cell::RefCell::new(Vec::new());
        let mut commits = 0;
        index
            .update_batched(
                &stamped,
                |p, _| {
                    // (Mid-update: the reader answers at once, with what's committed.)
                    let started = Instant::now();
                    seen.borrow_mut().push(reader.search("zebra", 100).unwrap().len());
                    assert!(started.elapsed() < Duration::from_millis(150), "a search waited {:?}", started.elapsed());
                    Part { messages: harlo_harness::store::read_messages(p).unwrap_or_default(), ..Default::default() }
                },
                || commits += 1,
            )
            .unwrap();
        let seen = seen.into_inner();
        assert_eq!((seen[0], seen[20], seen[40]), (0, 20, 40), "batches of 20 seen as they're committed");
        assert_eq!(commits, 3);
        assert_eq!(reader.search("zebra", 100).unwrap().len(), 45);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hit_lands_on_the_block_with_its_words() {
        let blocks = ["I fixed the quit crash.", "If it crashes again, run coredumpctl", "run COREDUMPCTL info with its id"];
        assert_eq!(best_block(&blocks, "coredumpctl info"), 2, "the most of its words, in any case");
        assert_eq!(best_block(&blocks, "coredumpctl"), 1, "the first of equals");
        assert_eq!(best_block(&blocks, "nothing here"), 0);
        assert_eq!(best_block(&blocks, ""), 0);
    }

    /// A Claude transcript that grows: what was there isn't read again, and
    /// hits still name the message's place in the whole conversation.
    #[test]
    fn indexes_only_what_was_added() {
        use harlo_harness::claude::read_part;
        let dir = std::env::temp_dir().join(format!("harlo-search-parts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let line = |kind: &str, id: &str, text: &str| match kind {
            "user" => format!(r#"{{"type":"user","message":{{"role":"user","content":"{text}"}}}}"#) + "\n",
            _ => format!(r#"{{"type":"assistant","message":{{"id":"{id}","model":"m","content":[{{"type":"text","text":"{text}"}}]}}}}"#) + "\n",
        };
        let path = dir.join("s.jsonl");
        let mut text = line("user", "", "walrus") + &line("assistant", "a", "narwhal") + &line("user", "", "puffin");
        std::fs::write(&path, &text).unwrap();
        let stamped = |size: usize, at: i64| vec![(path.clone(), at, size as i64)];
        let mut index = SearchIndex::open(&dir.join("search.db")).unwrap();
        let read_from = std::cell::Cell::new(None);
        let read = |p: &Path, r: Option<&Resume>| {
            read_from.set(Some(r.map_or(0, |r| r.offset)));
            read_part(p, r).unwrap()
        };
        index.update_parts(&stamped(text.len(), 1), read).unwrap();
        assert_eq!(read_from.get(), Some(0));
        assert_eq!(index.search("puffin", 10).unwrap()[0].index, 2);

        text += &line("assistant", "b", "ptarmigan");
        text += &line("user", "", "lemming");
        std::fs::write(&path, &text).unwrap();
        index.update_parts(&stamped(text.len(), 2), read).unwrap();
        assert!(read_from.get().unwrap() > 0, "read on from the last prompt");
        for (word, at) in [("walrus", 0), ("narwhal", 1), ("puffin", 2), ("ptarmigan", 3), ("lemming", 4)] {
            let hits = index.search(word, 10).unwrap();
            assert_eq!(hits.len(), 1, "{word} once");
            assert_eq!(hits[0].index, at, "{word}");
        }

        // Another conversation in its place: read whole.
        std::fs::write(&path, line("user", "", "albatross")).unwrap();
        index.update_parts(&stamped(1, 3), read).unwrap();
        assert!(index.search("walrus", 10).unwrap().is_empty());
        assert_eq!(index.search("albatross", 10).unwrap()[0].index, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opens_an_index_from_before_it_read_on() {
        let dir = std::env::temp_dir().join(format!("harlo-search-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("search.db");
        Connection::open(&db).unwrap().execute_batch("CREATE TABLE files (path TEXT PRIMARY KEY, modified INTEGER NOT NULL, size INTEGER NOT NULL); INSERT INTO files VALUES ('/x', 1, 2);").unwrap();
        drop(SearchIndex::open(&db).unwrap());
        let mut index = SearchIndex::open(&db).unwrap();
        let unchanged = index.update_parts(&[(PathBuf::from("/x"), 1, 2)], |_, _| panic!("unchanged")).unwrap();
        assert_eq!(unchanged, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_index_from_before_its_row_table_still_forgets() {
        let dir = std::env::temp_dir().join(format!("harlo-search-rows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("search.db");
        Connection::open(&db)
            .unwrap()
            .execute_batch(
                "CREATE TABLE files (path TEXT PRIMARY KEY, modified INTEGER NOT NULL, size INTEGER NOT NULL);
                 CREATE VIRTUAL TABLE messages USING fts5(text, path UNINDEXED, idx UNINDEXED, role UNINDEXED, tokenize = 'unicode61 remove_diacritics 2');
                 INSERT INTO files VALUES ('/a', 1, 1), ('/b', 1, 1);
                 INSERT INTO messages VALUES ('walrus', '/a', 0, 'user'), ('narwhal', '/b', 0, 'user');",
            )
            .unwrap();
        let mut index = SearchIndex::open(&db).unwrap();
        let said = |word: &'static str| move |_: &Path, _: Option<&Resume>| Part { messages: vec![serde_json::json!({"role": "user", "content": word})], ..Default::default() };
        // /a changed, /b gone: their old rows go.
        index.update_parts(&[(PathBuf::from("/a"), 2, 2)], said("puffin")).unwrap();
        assert!(index.search("walrus", 10).unwrap().is_empty());
        assert!(index.search("narwhal", 10).unwrap().is_empty());
        assert_eq!(index.search("puffin", 10).unwrap().len(), 1);
        // And rows written since go too.
        index.update_parts(&[(PathBuf::from("/a"), 3, 3)], said("lemming")).unwrap();
        assert!(index.search("puffin", 10).unwrap().is_empty());
        index.update_parts(&[], said("x")).unwrap();
        assert!(index.search("lemming", 10).unwrap().is_empty());
        let rows: i64 = index.conn.query_row("SELECT count(*) FROM message_rows", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
