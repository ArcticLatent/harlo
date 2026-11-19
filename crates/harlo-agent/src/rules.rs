//! Rules for some files: other agents' rule files scoped by globs, sent
//! with the first tool result that touches a file they're for.
//!
//! Cursor's (`.cursor/rules/*.mdc` with `globs:` and no `alwaysApply:
//! true`) and Claude Code's (`.claude/rules/*.md` with `paths:`), in the
//! project's top folder. A rule comes once, as a folder's AGENTS.md does
//! (see `Tools::nested_rules`), and costs nothing until a matching file
//! is read or changed. A glob without a `/` matches a file's name in any
//! folder; `**` matches any folders, `*` and `?` within a name, and
//! `{a,b}` either.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

/// A rule for the files its globs match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobRule {
    /// Its file, from the project's top folder.
    pub shown: String,
    pub globs: Vec<String>,
    pub body: String,
}

impl GlobRule {
    /// Whether it's for the file at `rel` (from the top folder).
    pub fn matches(&self, rel: &str) -> bool {
        self.globs.iter().any(|g| glob(g, rel))
    }
}

/// The project's top folder: the repository's, else `cwd`.
pub fn top(cwd: &Path) -> PathBuf {
    cwd.ancestors().find(|d| d.join(".git").exists()).unwrap_or(cwd).to_path_buf()
}

/// A rule file's frontmatter and body (no frontmatter: empty, the whole).
pub fn frontmatter(text: &str) -> (&str, &str) {
    match text.strip_prefix("---\n").and_then(|r| r.split_once("\n---")) {
        Some((h, b)) => (h, b.split_once('\n').map_or("", |(_, b)| b)),
        None => ("", text),
    }
}

/// A frontmatter field's value (quotes trimmed; None when empty).
pub fn field(head: &str, key: &str) -> Option<String> {
    head.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().trim_matches('"').trim_matches('\'').to_owned())).filter(|v| !v.is_empty())
}

/// A field's globs: `a, b`, `["a", "b"]`, or a YAML list below the key.
fn globs(head: &str, key: &str) -> Vec<String> {
    let split = |v: &str| -> Vec<String> { v.trim_matches(['[', ']']).split(',').map(|g| g.trim().trim_matches('"').trim_matches('\'').to_owned()).filter(|g| !g.is_empty()).collect() };
    if let Some(v) = field(head, key) {
        return split(&v);
    }
    let mut lines = head.lines().skip_while(|l| l.split_once(':').is_none_or(|(k, _)| k.trim() != key)).skip(1);
    let mut out = Vec::new();
    while let Some(item) = lines.next().and_then(|l| l.trim().strip_prefix("- ")) {
        out.extend(split(item));
    }
    out
}

/// The glob-scoped rules in the project's top folder `top`, by file name.
pub fn glob_rules(top: &Path) -> Vec<GlobRule> {
    let mut out = Vec::new();
    for (folder, key) in [(".cursor/rules", "globs"), (".claude/rules", "paths")] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(top.join(folder)).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "mdc" || e == "md")).collect();
        files.sort();
        for p in files {
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let (head, body) = frontmatter(&text);
            if field(head, "alwaysApply").as_deref() == Some("true") {
                continue;
            }
            let globs = globs(head, key);
            if globs.is_empty() || body.trim().is_empty() {
                continue;
            }
            let shown = p.strip_prefix(top).unwrap_or(&p).display().to_string();
            out.push(GlobRule { shown, globs, body: body.trim().to_owned() });
        }
    }
    out
}

/// Whether `path` (from the top folder, `/` between folders) matches `pattern`.
pub fn glob(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim().trim_start_matches("./");
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    // (No folder in it: a name anywhere.)
    let pattern = if pattern.contains('/') { pattern.to_owned() } else { format!("**/{pattern}") };
    expand(&pattern).iter().any(|p| {
        let parts: Vec<&str> = p.split('/').collect();
        let names: Vec<&str> = path.split('/').collect();
        segments(&parts, &names)
    })
}

/// A pattern with its `{a,b}` choices spelled out.
fn expand(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else { return vec![pattern.to_owned()] };
    let Some(close) = pattern[open..].find('}').map(|c| open + c) else { return vec![pattern.to_owned()] };
    pattern[open + 1..close].split(',').flat_map(|choice| expand(&format!("{}{choice}{}", &pattern[..open], &pattern[close + 1..]))).collect()
}

fn segments(parts: &[&str], names: &[&str]) -> bool {
    match parts.split_first() {
        None => names.is_empty(),
        Some((&"**", rest)) => (0..=names.len()).any(|skip| segments(rest, &names[skip..])),
        Some((part, rest)) => names.split_first().is_some_and(|(name, names)| name_matches(part.as_bytes(), name.as_bytes()) && segments(rest, names)),
    }
}

fn name_matches(p: &[u8], n: &[u8]) -> bool {
    match p.split_first() {
        None => n.is_empty(),
        Some((b'*', rest)) => (0..=n.len()).any(|skip| name_matches(rest, &n[skip..])),
        Some((b'?', rest)) => !n.is_empty() && name_matches(rest, &n[1..]),
        Some((c, rest)) => n.first() == Some(c) && name_matches(rest, &n[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_as_rule_files_mean_them() {
        assert!(glob("*.ts", "src/web/app.ts"));
        assert!(glob("src/**/*.ts", "src/app.ts") && glob("src/**/*.ts", "src/a/b/app.ts"));
        assert!(!glob("src/**/*.ts", "lib/app.ts"));
        assert!(glob("**/*.{ts,tsx}", "ui/View.tsx") && !glob("**/*.{ts,tsx}", "ui/View.jsx"));
        assert!(glob("/db/migrations/*", "db/migrations/001.sql") && !glob("db/migrations/*", "db/migrations/old/001.sql"));
        assert!(glob("Makefile", "tools/Makefile") && glob("test_?.py", "tests/test_a.py"));
    }

    #[test]
    fn scoped_rule_files_are_read_from_both_formats() {
        let dir = std::env::temp_dir().join(format!("harlo-rules-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".cursor/rules")).unwrap();
        std::fs::create_dir_all(dir.join(".claude/rules")).unwrap();
        std::fs::write(dir.join(".cursor/rules/api.mdc"), "---\ndescription: API\nglobs: src/api/**/*.ts, *.proto\n---\nValidate every input.\n").unwrap();
        std::fs::write(dir.join(".cursor/rules/all.mdc"), "---\nalwaysApply: true\nglobs: *.ts\n---\nAlways.\n").unwrap();
        std::fs::write(dir.join(".cursor/rules/asked.mdc"), "---\ndescription: Only when asked\n---\nAsked.\n").unwrap();
        std::fs::write(dir.join(".claude/rules/db.md"), "---\npaths:\n  - \"db/**/*.sql\"\n  - migrations/*\n---\nNever edit an applied migration.\n").unwrap();
        let rules = glob_rules(&dir);
        assert_eq!(rules.len(), 2, "{rules:?}");
        assert_eq!((rules[0].shown.as_str(), rules[0].globs.clone(), rules[0].body.as_str()), (".cursor/rules/api.mdc", vec!["src/api/**/*.ts".to_owned(), "*.proto".to_owned()], "Validate every input."));
        assert_eq!(rules[1].globs, vec!["db/**/*.sql".to_owned(), "migrations/*".to_owned()]);
        assert!(rules[0].matches("src/api/v1/users.ts") && rules[0].matches("proto/x.proto") && !rules[0].matches("src/web/a.ts"));
        assert!(rules[1].matches("db/schema/a.sql"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
