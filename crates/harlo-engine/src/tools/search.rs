//! `glob` and `grep`: finding files by name and lines by pattern. Both
//! walk the project the way git
//! sees it: `.gitignore`d files, hidden folders and `.git` are left out.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::Path;
use std::time::SystemTime;

use globset::GlobBuilder;
use ignore::WalkBuilder;
use regex::Regex;

/// The most paths `glob` lists.
const MAX_FILES: usize = 100;
/// The most matching lines `grep` lists.
const MAX_MATCHES: usize = 100;
/// The longest line `grep` shows whole.
const MAX_LINE: usize = 300;

/// The files under `base` whose path (relative to `base`) matches
/// `pattern`, most recently changed first.
///
/// # Errors
///
/// When `pattern` isn't a glob.
pub fn glob(pattern: &str, base: &Path, cwd: &Path) -> Result<String, String> {
    let matcher = GlobBuilder::new(pattern)
        .literal_separator(pattern.contains('/'))
        .build()
        .map_err(|e| format!("`{pattern}` isn't a glob pattern: {e}"))?
        .compile_matcher();
    let mut found: Vec<(SystemTime, String)> = Vec::new();
    for entry in WalkBuilder::new(base).build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let relative = path.strip_prefix(base).unwrap_or(path);
        let by_name =
            !pattern.contains('/') && relative.file_name().is_some_and(|n| matcher.is_match(n));
        if by_name || matcher.is_match(relative) {
            let changed = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            found.push((changed, shown(path, cwd)));
        }
    }
    if found.is_empty() {
        return Ok("No files found.".to_owned());
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let more = found.len().saturating_sub(MAX_FILES);
    let mut text: String = found
        .into_iter()
        .take(MAX_FILES)
        .map(|(_, p)| p)
        .collect::<Vec<_>>()
        .join("\n");
    if more > 0 {
        let _ = write!(text, "\n({more} more; narrow the pattern)");
    }
    Ok(text)
}

/// The lines in files under `base` (only those whose name matches
/// `include`, when given) that match the regular expression `pattern`,
/// grouped by file.
///
/// # Errors
///
/// When `pattern` isn't a regular expression or `include` isn't a glob.
pub fn grep(
    pattern: &str,
    base: &Path,
    include: Option<&str>,
    cwd: &Path,
) -> Result<String, String> {
    let regex =
        Regex::new(pattern).map_err(|e| format!("`{pattern}` isn't a regular expression: {e}"))?;
    let include = include
        .map(|g| {
            GlobBuilder::new(g)
                .build()
                .map(|g| g.compile_matcher())
                .map_err(|e| format!("`{g}` isn't a glob pattern: {e}"))
        })
        .transpose()?;
    let mut out = String::new();
    let mut count = 0usize;
    let mut more = false;
    for entry in WalkBuilder::new(base).build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        if let Some(m) = &include
            && !path.file_name().is_some_and(|n| m.is_match(n))
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            // Not text.
            continue;
        };
        let mut header = false;
        for (n, line) in text.lines().enumerate() {
            if !regex.is_match(line) {
                continue;
            }
            if count == MAX_MATCHES {
                more = true;
                break;
            }
            if !header {
                let _ = writeln!(out, "{}:", shown(path, cwd));
                header = true;
            }
            let line: String = line.chars().take(MAX_LINE).collect();
            let _ = writeln!(out, "  {}: {line}", n + 1);
            count += 1;
        }
        if more {
            break;
        }
    }
    if count == 0 {
        return Ok("No matches.".to_owned());
    }
    if more {
        let _ = writeln!(out, "(more matches; narrow the pattern or the folder)");
    }
    Ok(out.trim_end().to_owned())
}

/// `path` relative to the project when it's inside it.
fn shown(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_and_grep_skip_what_git_ignores() {
        let t = tempfile::tempdir().expect("dir");
        let p = t.path();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(p)
            .status()
            .expect("git");
        std::fs::write(p.join(".gitignore"), "target/\n").expect("ignore");
        std::fs::create_dir_all(p.join("src")).expect("src");
        std::fs::create_dir_all(p.join("target")).expect("target");
        std::fs::write(p.join("src/main.rs"), "fn main() {\n    hello();\n}\n").expect("main");
        std::fs::write(p.join("src/lib.rs"), "pub fn hello() {}\n").expect("lib");
        std::fs::write(p.join("target/gen.rs"), "fn hello() {}\n").expect("gen");
        std::fs::write(p.join("notes.txt"), "hello there\n").expect("notes");

        let mut rs: Vec<String> = glob("*.rs", p, p)
            .expect("glob")
            .lines()
            .map(str::to_owned)
            .collect();
        rs.sort();
        assert_eq!(rs, ["src/lib.rs", "src/main.rs"]);
        assert_eq!(glob("src/*.rs", p, p).expect("glob").lines().count(), 2);
        assert_eq!(glob("*.md", p, p).expect("glob"), "No files found.");

        let hits = grep(r"hello\(", p, Some("*.rs"), p).expect("grep");
        assert!(hits.contains("src/main.rs:\n  2:     hello();"), "{hits}");
        assert!(
            hits.contains("src/lib.rs:\n  1: pub fn hello() {}"),
            "{hits}"
        );
        assert!(!hits.contains("target"), "{hits}");
        assert!(!hits.contains("notes.txt"), "{hits}");
        grep("(", p, None, p).expect_err("not a regular expression");
    }
}
