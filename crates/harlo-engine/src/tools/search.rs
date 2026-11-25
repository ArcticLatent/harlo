//! `glob` and `grep`: finding files by name and lines by pattern. Both
//! walk the project the way git sees it: `.gitignore`d files and the
//! version control's own folder are left out. Other hidden files and
//! folders (`.github`, `.harlo`, dotfiles) are part of the project.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
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
/// The folders version control keeps for itself: never the project's own.
const VERSION_CONTROL: [&str; 4] = [".git", ".jj", ".hg", ".svn"];

/// What a `grep` looks for.
#[derive(Debug, Clone, Copy)]
pub struct Wanted<'a> {
    /// The regular expression.
    pub pattern: &'a str,
    /// Only files whose name matches this glob.
    pub include: Option<&'a str>,
    /// Whether to list the files that have a match, not the lines.
    pub files_only: bool,
}

/// Walks the files under `base`, leaving out whatever is under one of the
/// `skip` paths, which are given with their links followed.
fn walk(base: &Path, skip: &[PathBuf]) -> ignore::Walk {
    let mut walk = WalkBuilder::new(base);
    walk.hidden(false);
    let real = super::real_path(base);
    let skip: Vec<PathBuf> = skip
        .iter()
        .filter(|s| s.starts_with(&real))
        .cloned()
        .collect();
    let base = base.to_path_buf();
    walk.filter_entry(move |entry| {
        if entry.depth() > 0
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| VERSION_CONTROL.contains(&name))
        {
            return false;
        }
        if skip.is_empty() {
            return true;
        }
        let at = entry
            .path()
            .strip_prefix(&base)
            .map_or_else(|_| entry.path().to_path_buf(), |rest| real.join(rest));
        !skip.iter().any(|s| at.starts_with(s))
    });
    walk.build()
}

/// The files under `base` whose path (relative to `base`) matches
/// `pattern`, most recently changed first.
///
/// # Errors
///
/// When `pattern` isn't a glob.
pub fn glob(pattern: &str, base: &Path, cwd: &Path, skip: &[PathBuf]) -> Result<String, String> {
    let matcher = GlobBuilder::new(pattern)
        .literal_separator(pattern.contains('/'))
        .build()
        .map_err(|e| format!("`{pattern}` isn't a glob pattern: {e}"))?
        .compile_matcher();
    let mut found: Vec<(SystemTime, String)> = Vec::new();
    for entry in walk(base, skip).flatten() {
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

/// The lines in files under `base` that match what's `wanted`, grouped by
/// file; or, asked for files only, each file with a match and how many
/// lines of it match.
///
/// A file under a searched folder is read only when `may_read` says so of
/// its path as shown; the result says how many were left out. A file
/// searched by itself is the caller's to decide.
///
/// # Errors
///
/// When `pattern` isn't a regular expression or `include` isn't a glob.
pub fn grep(
    wanted: Wanted<'_>,
    base: &Path,
    cwd: &Path,
    skip: &[PathBuf],
    may_read: Option<&dyn Fn(&str) -> bool>,
) -> Result<String, String> {
    let Wanted {
        pattern,
        include,
        files_only,
    } = wanted;
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
    // Files not read, whether or not they have a match: counting only
    // those that do would tell what is in them.
    let mut left_out = 0usize;
    for entry in walk(base, skip).flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        if let Some(m) = &include
            && !path.file_name().is_some_and(|n| m.is_match(n))
        {
            continue;
        }
        if entry.depth() > 0
            && let Some(may_read) = may_read
            && !may_read(&shown(path, cwd))
        {
            left_out += 1;
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            // Not text.
            continue;
        };
        if files_only {
            let lines = text.lines().filter(|line| regex.is_match(line)).count();
            if lines == 0 {
                continue;
            }
            if count == MAX_FILES {
                more = true;
                break;
            }
            let _ = writeln!(out, "{} ({lines})", shown(path, cwd));
            count += 1;
            continue;
        }
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
            let _ = writeln!(out, "{}\t{line}", n + 1);
            count += 1;
        }
        if more {
            break;
        }
    }
    if count == 0 {
        out.push_str("No matches.\n");
    }
    if more {
        let _ = writeln!(out, "(more matches; narrow the pattern or the folder)");
    }
    if left_out > 0 {
        let _ = writeln!(
            out,
            "({left_out} not searched: reading them needs approval. Read one to ask for it.)"
        );
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

    /// A search for the lines matching `pattern`.
    fn lines<'a>(pattern: &'a str, include: Option<&'a str>) -> Wanted<'a> {
        Wanted {
            pattern,
            include,
            files_only: false,
        }
    }

    #[test]
    fn hidden_files_are_searched_and_version_control_is_not() {
        let t = tempfile::tempdir().expect("dir");
        let p = t.path();
        for dir in [".github/workflows", ".git/hooks", "src/.jj"] {
            std::fs::create_dir_all(p.join(dir)).expect("folder");
        }
        std::fs::write(p.join(".github/workflows/ci.yml"), "run: cargo test\n").expect("ci");
        std::fs::write(p.join(".editorconfig"), "indent = cargo\n").expect("dotfile");
        std::fs::write(p.join(".git/hooks/pre-commit"), "cargo fmt\n").expect("hook");
        std::fs::write(p.join("src/.jj/op"), "cargo\n").expect("op");

        let mut files: Vec<String> = glob("*", p, p, &[])
            .expect("glob")
            .lines()
            .map(str::to_owned)
            .collect();
        files.sort();
        assert_eq!(files, [".editorconfig", ".github/workflows/ci.yml"]);
        let hits = grep(lines("cargo", None), p, p, &[], None).expect("grep");
        assert!(hits.contains(".github/workflows/ci.yml:"), "{hits}");
        assert!(hits.contains(".editorconfig:"), "{hits}");
        assert!(
            !hits.contains("pre-commit") && !hits.contains(".jj"),
            "{hits}"
        );
    }

    #[test]
    fn grep_lists_only_the_files_when_asked() {
        let t = tempfile::tempdir().expect("dir");
        let p = t.path();
        std::fs::write(p.join("a.rs"), "fn one() {}\nfn two() {}\nlet x = 1;\n").expect("a");
        std::fs::write(p.join("b.rs"), "fn three() {}\n").expect("b");
        std::fs::write(p.join("c.rs"), "let y = 2;\n").expect("c");
        let files = Wanted {
            pattern: r"fn \w+",
            include: None,
            files_only: true,
        };
        let mut found: Vec<String> = grep(files, p, p, &[], None)
            .expect("grep")
            .lines()
            .map(str::to_owned)
            .collect();
        found.sort();
        assert_eq!(found, ["a.rs (2)", "b.rs (1)"]);
        let none = Wanted {
            pattern: "nowhere",
            ..files
        };
        assert_eq!(grep(none, p, p, &[], None).expect("grep"), "No matches.");
    }

    #[test]
    fn grep_reads_only_the_files_it_may() {
        let t = tempfile::tempdir().expect("dir");
        let p = t.path();
        std::fs::write(p.join("notes.txt"), "KEY in the notes\n").expect("notes");
        std::fs::write(p.join("prod.env"), "KEY=secret\n").expect("env");
        std::fs::write(p.join("test.env"), "nothing here\n").expect("env");
        let not_env = |shown: &str| !crate::rules::glob("*.env", shown);

        let hits = grep(lines("KEY", None), p, p, &[], Some(&not_env)).expect("grep");
        assert!(hits.contains("notes.txt:"), "{hits}");
        assert!(!hits.contains("secret"), "{hits}");
        // Both are counted, the one without a match too.
        assert!(hits.contains("(2 not searched"), "{hits}");
        let none = grep(lines("secret", None), p, p, &[], Some(&not_env)).expect("grep");
        assert!(none.starts_with("No matches.\n(2 not searched"), "{none}");

        // A file searched by itself was decided by the caller.
        let own = grep(
            lines("KEY", None),
            &p.join("prod.env"),
            p,
            &[],
            Some(&not_env),
        )
        .expect("grep");
        assert!(own.contains("KEY=secret"), "{own}");
    }

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

        let mut rs: Vec<String> = glob("*.rs", p, p, &[])
            .expect("glob")
            .lines()
            .map(str::to_owned)
            .collect();
        rs.sort();
        assert_eq!(rs, ["src/lib.rs", "src/main.rs"]);
        assert_eq!(
            glob("src/*.rs", p, p, &[]).expect("glob").lines().count(),
            2
        );
        assert_eq!(glob("*.md", p, p, &[]).expect("glob"), "No files found.");

        let hits = grep(lines(r"hello\(", Some("*.rs")), p, p, &[], None).expect("grep");
        assert!(hits.contains("src/main.rs:\n2\t    hello();"), "{hits}");
        assert!(hits.contains("src/lib.rs:\n1\tpub fn hello() {}"), "{hits}");
        assert!(!hits.contains("target"), "{hits}");
        assert!(!hits.contains("notes.txt"), "{hits}");
        grep(lines("(", None), p, p, &[], None).expect_err("not a regular expression");
    }
}
