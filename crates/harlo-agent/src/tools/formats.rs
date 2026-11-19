//! What `read` shows for files that aren't plain text, and the line ranges
//! a path may carry.
//!
//! A notebook shows its cells (not its JSON), a PDF its text, an archive its
//! files and an SQLite database its schema, each paged like any text file.
//! A path like `a.py:50-80` asks for lines 50–80; `a.py:-40` for the last
//! 40; `a.py:5-9,90-95` for two ranges in one call.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

/// How long a converting program may take.
const CONVERT_TIMEOUT: Duration = Duration::from_secs(30);

/// A notebook cell's output is cut here.
const OUTPUT_CHARS: usize = 2_000;

/// Lines asked for in a path's selector.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Range {
    /// From a line (1-based) to another (inclusive), or to the end.
    Lines(u64, Option<u64>),
    /// The last so many lines.
    Last(u64),
}

/// A path's line ranges (`a.py:50-80`, `a.py:50`, `a.py:50-`, `a.py:50+20`,
/// `a.py:-40`, joined by commas), with the path without them. None when the
/// path has none.
pub fn selector(path: &str) -> Option<(&str, Vec<Range>)> {
    let (file, spec) = path.rsplit_once(':')?;
    if file.is_empty() || spec.is_empty() {
        return None;
    }
    let ranges: Option<Vec<Range>> = spec.split(',').map(|part| range(part.trim())).collect();
    Some((file, ranges?))
}

fn range(part: &str) -> Option<Range> {
    let num = |s: &str| s.parse::<u64>().ok().filter(|n| *n > 0);
    if let Some(n) = part.strip_prefix('-') {
        return num(n).map(Range::Last);
    }
    if let Some((a, n)) = part.split_once('+') {
        let (a, n) = (num(a)?, num(n)?);
        return Some(Range::Lines(a, Some(a + n - 1)));
    }
    match part.split_once('-') {
        Some((a, "")) => num(a).map(|a| Range::Lines(a, None)),
        Some((a, b)) => {
            let (a, b) = (num(a)?, num(b)?);
            (b >= a).then_some(Range::Lines(a, Some(b)))
        }
        None => num(part).map(|a| Range::Lines(a, Some(a))),
    }
}

/// The kinds of file shown as text of their own.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Kind {
    Notebook,
    Pdf,
    Archive,
    Sqlite,
}

/// Which kind `path` is (from its start, `head`, and its name).
pub fn kind(path: &Path, head: &[u8]) -> Option<Kind> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if head.starts_with(b"SQLite format 3\0") {
        Some(Kind::Sqlite)
    } else if head.starts_with(b"%PDF-") {
        Some(Kind::Pdf)
    } else if name.ends_with(".ipynb") {
        Some(Kind::Notebook)
    } else if [".zip", ".jar", ".whl", ".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tar.zst", ".7z"].iter().any(|e| name.ends_with(e)) {
        Some(Kind::Archive)
    } else {
        None
    }
}

/// `path`'s text, as `kind` shows it.
///
/// # Errors
///
/// Why it can't be shown: not valid, or the program that reads it is
/// missing or failed.
pub async fn text(kind: Kind, path: &Path, bytes: &[u8]) -> Result<String, String> {
    let p = path.to_string_lossy().into_owned();
    match kind {
        Kind::Notebook => notebook(bytes),
        Kind::Pdf => run("pdftotext", &["-layout", "-q", &p, "-"]).await,
        Kind::Archive => run("bsdtar", &["-tvf", &p]).await.or(run("tar", &["-tvf", &p]).await),
        Kind::Sqlite => {
            let schema = run("sqlite3", &["-readonly", &p, ".schema"]).await?;
            let tables = run("sqlite3", &["-readonly", &p, "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name"]).await?;
            let mut out = String::from("-- schema\n");
            out.push_str(schema.trim_end());
            out.push_str("\n-- rows per table\n");
            for t in tables.lines().filter(|t| !t.is_empty()) {
                let n = run("sqlite3", &["-readonly", &p, &format!("SELECT count(*) FROM \"{}\"", t.replace('"', "\"\""))]).await.unwrap_or_else(|e| e);
                let _ = writeln!(out, "{t}: {}", n.trim());
            }
            Ok(out)
        }
    }
}

/// A notebook's cells: each one's kind and source, and a code cell's text
/// outputs (cut short).
fn notebook(bytes: &[u8]) -> Result<String, String> {
    let nb: Value = serde_json::from_slice(bytes).map_err(|e| format!("not a valid notebook ({e})"))?;
    let joined = |v: &Value| match v {
        Value::Array(parts) => parts.iter().filter_map(Value::as_str).collect::<String>(),
        Value::String(s) => s.clone(),
        _ => String::new(),
    };
    let mut out = String::new();
    for (i, cell) in nb["cells"].as_array().into_iter().flatten().enumerate() {
        let kind = cell["cell_type"].as_str().unwrap_or("code");
        let _ = writeln!(out, "# cell {} [{kind}]", i + 1);
        out.push_str(joined(&cell["source"]).trim_end());
        out.push('\n');
        for o in cell["outputs"].as_array().into_iter().flatten() {
            let text = if o["text"].is_null() { joined(&o["data"]["text/plain"]) } else { joined(&o["text"]) };
            let text = if o["output_type"] == "error" { format!("{}: {}", o["ename"].as_str().unwrap_or("Error"), o["evalue"].as_str().unwrap_or_default()) } else { text };
            if text.trim().is_empty() {
                continue;
            }
            let cut: String = text.chars().take(OUTPUT_CHARS).collect();
            out.push_str("# output\n");
            out.push_str(cut.trim_end());
            if cut.len() < text.len() {
                out.push_str("\n[output cut short]");
            }
            out.push('\n');
        }
    }
    if out.is_empty() { Err("a notebook with no cells".into()) } else { Ok(out) }
}

/// A program's output, or why there is none.
async fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let cmd = tokio::process::Command::new(program).args(args).stdin(std::process::Stdio::null()).kill_on_drop(true).output();
    match tokio::time::timeout(CONVERT_TIMEOUT, cmd).await {
        Err(_) => Err(format!("{program} took longer than {} s", CONVERT_TIMEOUT.as_secs())),
        Ok(Err(e)) => Err(format!("{program} isn't available ({e})")),
        Ok(Ok(o)) if !o.status.success() => Err(format!("{program} failed: {}", String::from_utf8_lossy(&o.stderr).trim())),
        Ok(Ok(o)) => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_name_lines_and_leave_other_paths_alone() {
        assert_eq!(selector("a.py:50-80"), Some(("a.py", vec![Range::Lines(50, Some(80))])));
        assert_eq!(selector("a.py:7"), Some(("a.py", vec![Range::Lines(7, Some(7))])));
        assert_eq!(selector("a.py:50-"), Some(("a.py", vec![Range::Lines(50, None)])));
        assert_eq!(selector("a.py:50+3"), Some(("a.py", vec![Range::Lines(50, Some(52))])));
        assert_eq!(selector("a.py:-40"), Some(("a.py", vec![Range::Last(40)])));
        assert_eq!(selector("src/a.rs:5-9, 90-95"), Some(("src/a.rs", vec![Range::Lines(5, Some(9)), Range::Lines(90, Some(95))])));
        for plain in ["a.py", "C:", "x:y", "a.py:0", "a.py:9-3", "a.py:", "notes:draft"] {
            assert_eq!(selector(plain), None, "{plain}");
        }
    }

    #[test]
    fn a_notebook_shows_its_cells_and_outputs() {
        let nb = br##"{"cells":[{"cell_type":"markdown","source":["# Title\n","Intro"]},
            {"cell_type":"code","source":"x = 1\nprint(x)","outputs":[{"output_type":"stream","text":["1\n"]}]},
            {"cell_type":"code","source":"1/0","outputs":[{"output_type":"error","ename":"ZeroDivisionError","evalue":"division by zero","traceback":["-"]}]}]}"##;
        let text = notebook(nb).unwrap();
        assert_eq!(text, "# cell 1 [markdown]\n# Title\nIntro\n# cell 2 [code]\nx = 1\nprint(x)\n# output\n1\n# cell 3 [code]\n1/0\n# output\nZeroDivisionError: division by zero\n");
        notebook(b"{").unwrap_err();
    }

    #[test]
    fn kinds_come_from_the_start_or_the_name() {
        assert_eq!(kind(Path::new("x.db"), b"SQLite format 3\0rest"), Some(Kind::Sqlite));
        assert_eq!(kind(Path::new("paper"), b"%PDF-1.7"), Some(Kind::Pdf));
        assert_eq!(kind(Path::new("a.ipynb"), b"{"), Some(Kind::Notebook));
        assert_eq!(kind(Path::new("dist.tar.gz"), b"\x1f\x8b"), Some(Kind::Archive));
        assert_eq!(kind(Path::new("a.py"), b"import x"), None);
    }

    #[tokio::test]
    async fn an_sqlite_database_shows_its_schema_and_rows() {
        let dir = std::env::temp_dir().join(format!("harlo-formats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("t.db");
        let made = std::process::Command::new("sqlite3").arg(&db).arg("CREATE TABLE users(id INTEGER, name TEXT); INSERT INTO users VALUES (1,'a'),(2,'b');").status();
        if made.is_ok_and(|s| s.success()) {
            let text = text(Kind::Sqlite, &db, b"").await.unwrap();
            assert!(text.contains("CREATE TABLE users(id INTEGER, name TEXT);") && text.contains("users: 2"), "{text}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
