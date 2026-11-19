//! `locate`: where the code that does something is, from a description.
//!
//! A lexical pass ranks the project's files by the query's words (rarer
//! words count more), cuts the best into windows of lines and keeps the
//! windows richest in those words; one low-effort request to the session's
//! model then scores how likely each window is to implement what was
//! described. The tool returns the scored line ranges, strongest first.
//! Offered only when `HARLO_AGENT_LOCATE=on`, until measured.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Value, json};

pub const NAME: &str = "locate";

/// Files whose text is cut into windows.
const FILES: usize = 12;
/// Lines per window, and how far the next one starts.
const WINDOW: usize = 80;
const STRIDE: usize = 60;
/// Windows per file and in all that the judge scores.
const PER_FILE: usize = 3;
const JUDGED: usize = 24;
/// Lines of a window shown to the judge, at most.
const SHOWN_LINES: usize = 28;
/// Files bigger than this aren't read.
const MAX_FILE: u64 = 1 << 20;
/// Hits returned, at most, and the least score that's one.
const HITS: usize = 8;
const LEAST: f64 = 0.3;

/// Whether the tool is offered (opt-in).
pub fn on() -> bool {
    std::env::var("HARLO_AGENT_LOCATE").is_ok_and(|v| v.trim() == "on")
}

pub fn spec() -> Value {
    json!({
        "type": "function", "name": NAME, "strict": false,
        "description": "Find where the code that does something is, from a description in plain words (e.g. \"where expired sessions are cleaned up\"): returns scored line ranges, strongest first; read the ranges you need. \
                        For when you don't know where to look; for a known name or string, use grep.",
        "parameters": { "type": "object", "properties": {
            "query": { "type": "string", "description": "What the code does, in plain words" },
            "keywords": { "type": "array", "items": { "type": "string" }, "description": "Words likely in that code (names, terms), if you can guess some" },
            "path": { "type": "string", "description": "A folder to look in (the whole project by default)" },
        }, "required": ["query"], "additionalProperties": false },
    })
}

/// The instructions of the judging request.
pub const INSTRUCTIONS: &str = "You score code passages. For each passage, give the probability (0 to 1) that it implements the behaviour described. \
    Reply with one line per passage, `id score`, and nothing else.";

/// A window of a file the judge scores.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub shown: String,
}

/// The words looked for: the given keywords, and the query's own words of
/// four letters or more (not common ones), each also split at case changes
/// and underscores; lowercased, without repeats.
pub fn words(query: &str, keywords: &[String]) -> Vec<String> {
    const COMMON: &[&str] = &["where", "what", "when", "which", "that", "this", "with", "from", "into", "code", "does", "done", "find", "make", "made", "gets", "they", "them", "there", "their", "each", "have", "happens", "handled", "handles"];
    let mut out: Vec<String> = Vec::new();
    let mut add = |w: &str| {
        let w = w.to_lowercase();
        if w.len() >= 3 && !COMMON.contains(&w.as_str()) && !out.contains(&w) {
            out.push(w);
        }
    };
    for k in keywords {
        add(k.trim());
        for part in split_name(k) {
            add(&part);
        }
    }
    for w in query.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| w.len() >= 4) {
        add(w);
        for part in split_name(w) {
            add(&part);
        }
    }
    out
}

/// A name's parts: at underscores and lower-to-upper case changes.
fn split_name(name: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c == '_' || c == '-' || c == '.' || (c.is_uppercase() && prev_lower) {
            if cur.len() >= 3 {
                parts.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
        if c.is_alphanumeric() {
            cur.push(c);
        }
        prev_lower = c.is_lowercase();
    }
    if cur.len() >= 3 && cur.len() < name.len() {
        parts.push(cur);
    }
    parts
}

/// How often each of `words` is in `lower` (a lowercased text), each count
/// capped so one word repeated doesn't swamp the rest.
fn counts(lower: &str, words: &[String]) -> Vec<usize> {
    words.iter().map(|w| lower.matches(w.as_str()).count().min(20)).collect()
}

/// The windows worth judging: of `files` (path relative to the project,
/// its text), the best by the words' weight (rarer words weigh more), cut
/// into windows, the richest of each kept (the path counting in both).
pub fn windows(files: &[(String, String)], words: &[String]) -> Vec<Window> {
    if words.is_empty() {
        return Vec::new();
    }
    // (A file's path counts as part of its text: names are strong signs.)
    let lowered: Vec<String> = files.iter().map(|(p, t)| format!("{p}\n{t}").to_lowercase()).collect();
    let per_file: Vec<Vec<usize>> = lowered.iter().map(|t| counts(t, words)).collect();
    #[expect(clippy::cast_precision_loss, reason = "file counts, far below 2^52")]
    let idf: Vec<f64> = (0..words.len())
        .map(|i| {
            let df = per_file.iter().filter(|c| c[i] > 0).count();
            ((files.len() as f64 + 1.0) / (df as f64 + 1.0)).ln() + 1.0
        })
        .collect();
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let score = |c: &[usize]| -> f64 { c.iter().zip(&idf).map(|(n, w)| if *n > 0 { (1.0 + (*n as f64).ln()) * w } else { 0.0 }).sum() };
    let mut ranked: Vec<(usize, f64)> = per_file.iter().enumerate().map(|(i, c)| (i, score(c))).filter(|(_, s)| *s > 0.0).collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut all: Vec<(f64, Window)> = Vec::new();
    for &(i, _) in ranked.iter().take(FILES) {
        let (path, text) = &files[i];
        let lines: Vec<&str> = text.lines().collect();
        let mut mine: Vec<(f64, Window)> = Vec::new();
        let mut start = 0;
        while start < lines.len() {
            let end = (start + WINDOW).min(lines.len());
            let chunk = format!("{path}\n{}", lines[start..end].join("\n")).to_lowercase();
            let s = score(&counts(&chunk, words));
            if s > 0.0 {
                mine.push((s, Window { path: path.clone(), start: start + 1, end, shown: sketch(&lines[start..end], start, words) }));
            }
            if end == lines.len() {
                break;
            }
            start += STRIDE;
        }
        mine.sort_by(|a, b| b.0.total_cmp(&a.0));
        all.extend(mine.into_iter().take(PER_FILE));
    }
    all.sort_by(|a, b| b.0.total_cmp(&a.0));
    all.into_iter().take(JUDGED).map(|(_, w)| w).collect()
}

/// A window's lines shown to the judge: the ones with the words or that
/// define something, numbered, at most [`SHOWN_LINES`].
fn sketch(lines: &[&str], first: usize, words: &[String]) -> String {
    const DEFINES: &[&str] = &["fn ", "def ", "class ", "func ", "function ", "impl ", "struct ", "interface ", "trait ", "enum "];
    let mut out = String::new();
    let mut shown = 0;
    for (i, l) in lines.iter().enumerate() {
        let lower = l.to_lowercase();
        if words.iter().any(|w| lower.contains(w.as_str())) || DEFINES.iter().any(|d| lower.trim_start().starts_with(d) || lower.contains(&format!(" {d}"))) {
            let line: String = l.trim_end().chars().take(160).collect();
            let _ = writeln!(out, "{}: {line}", first + i + 1);
            shown += 1;
            if shown == SHOWN_LINES {
                break;
            }
        }
    }
    out
}

/// The judging request's one message: the description, then each window.
pub fn question(query: &str, windows: &[Window]) -> String {
    let mut q = format!("Behaviour: {query}\n\nPassages:\n");
    for (i, w) in windows.iter().enumerate() {
        let _ = write!(q, "\n[{}] {}:{}-{}\n{}", i + 1, w.path, w.start, w.end, w.shown);
    }
    q
}

/// The judge's scores (`id score` lines) as hits, strongest first: each
/// window's range and score, with its first shown line.
pub fn hits(reply: &str, windows: &[Window]) -> String {
    let mut scored: Vec<(f64, &Window)> = reply
        .lines()
        .filter_map(|l| {
            let mut parts = l.trim().trim_start_matches('[').split(|c: char| c.is_whitespace() || c == ']' || c == ':').filter(|p| !p.is_empty());
            let id: usize = parts.next()?.parse().ok()?;
            let score: f64 = parts.next()?.parse().ok()?;
            Some((score.clamp(0.0, 1.0), windows.get(id.checked_sub(1)?)?))
        })
        .filter(|(s, _)| *s >= LEAST)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    if scored.is_empty() {
        return "Nothing scored as likely: try other words (keywords: names you'd expect), or grep.".into();
    }
    let mut out = String::new();
    for (s, w) in scored.into_iter().take(HITS) {
        let first = w.shown.lines().next().and_then(|l| l.split_once(": ")).map_or("", |(_, t)| t.trim());
        let _ = writeln!(out, "{}:{}-{}  {s:.2}  {first}", w.path, w.start, w.end);
    }
    out.trim_end().to_owned()
}

/// The project's text files under `root` (gitignore respected, through
/// `rg --files`), with paths relative to `cwd`.
pub async fn files(cwd: &Path, root: &Path) -> Vec<(String, String)> {
    let listed = tokio::process::Command::new("rg").arg("--files").arg(root).current_dir(cwd).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await;
    let Ok(listed) = listed else { return Vec::new() };
    let cwd = cwd.to_path_buf();
    let names: Vec<String> = String::from_utf8_lossy(&listed.stdout).lines().map(str::to_owned).collect();
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for name in names {
            let full = cwd.join(&name);
            if std::fs::metadata(&full).map_or(true, |m| m.len() > MAX_FILE) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&full) else { continue };
            if bytes[..bytes.len().min(8192)].contains(&0) {
                continue;
            }
            let rel = full.strip_prefix(&cwd).map_or_else(|_| name.clone(), |p| p.to_string_lossy().into_owned());
            out.push((rel, String::from_utf8_lossy(&bytes).into_owned()));
        }
        out
    })
    .await
    .unwrap_or_default()
}

/// Why a query can't be looked for, if it can't.
pub fn refused(args: &Value) -> Option<&'static str> {
    args["query"].as_str().is_none_or(|q| q.trim().is_empty()).then_some("Say what the code does in query.")
}

/// The keywords given, as strings.
pub fn keywords(args: &Value) -> Vec<String> {
    args["keywords"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
}

/// How many windows of each file are judged, for tests.
#[cfg(test)]
fn by_path(ws: &[Window]) -> std::collections::HashMap<&str, usize> {
    let mut m = std::collections::HashMap::new();
    for w in ws {
        *m.entry(w.path.as_str()).or_insert(0) += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_come_from_keywords_and_the_query() {
        let w = words("where expired sessions are cleaned up", &["purgeExpired".into(), "session_store".into()]);
        assert_eq!(w, ["purgeexpired", "purge", "expired", "session_store", "session", "store", "sessions", "cleaned"]);
    }

    #[test]
    fn the_richest_windows_of_the_best_files_are_judged() {
        let mut big = String::new();
        for i in 0..300 {
            big.push_str(if i == 250 { "def purge_expired(store):\n" } else { "x = 1\n" });
        }
        let files = vec![
            ("a/sessions.py".to_owned(), big),
            ("b/other.py".to_owned(), "def unrelated():\n    pass\n".to_owned()),
            ("c/notes.md".to_owned(), "sessions sessions sessions\n".to_owned()),
        ];
        let ws = windows(&files, &words("purge expired sessions", &[]));
        assert_eq!(ws[0].path, "a/sessions.py");
        assert!(ws[0].start <= 251 && ws[0].end >= 251 && ws[0].shown.contains("251: def purge_expired(store):"), "{ws:?}");
        assert!(!by_path(&ws).contains_key("b/other.py"));
        let q = question("purge expired sessions", &ws);
        assert!(q.starts_with("Behaviour: purge expired sessions\n\nPassages:\n\n[1] a/sessions.py:"), "{q}");
    }

    #[test]
    fn scores_become_hits_strongest_first() {
        let w = |p: &str, s: usize| Window { path: p.into(), start: s, end: s + 79, shown: format!("{}: def f():\n", s + 3) };
        let ws = vec![w("a.py", 1), w("b.py", 61), w("c.py", 121)];
        let out = hits("1 0.2\n[2] 0.9\n3: 0.55\nnoise\n9 0.99", &ws);
        assert_eq!(out, "b.py:61-140  0.90  def f():\nc.py:121-200  0.55  def f():");
        assert!(hits("1 0.1", &ws).starts_with("Nothing scored as likely"));
    }
}
