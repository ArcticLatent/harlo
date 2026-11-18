//! Which repository files the wiki answers for.
//!
//! The files on disk now, not the last commit: code written a minute ago
//! counts. The walk follows the repository's ignore rules (`.gitignore`,
//! `.git/info/exclude`, the global one) and skips hidden folders, so local
//! notes and build output stay out. Of what's left, a file is in scope when
//! the scanner knows its language and it isn't tests, vendored or generated
//! code, a licence or a changelog, or the wiki itself.
//!
//! Code files must each belong to a page (or be left out on purpose);
//! Markdown and TOML files may be cited but needn't belong to one.

// Rust guideline compliant 2026-10-09

use crate::units::{self, Lang};
use std::path::Path;

/// Bigger files are data or generated code, not something a page explains.
const MAX_BYTES: u64 = 1024 * 1024;

/// Whether `path` is test code, by the usual folder and file names.
pub fn is_test(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let dirs = format!("/{path}");
    ["/tests/", "/test/", "/Tests/", "/__tests__/", "/spec/", "/fixtures/", "/testdata/", "UITests/"].iter().any(|d| dirs.contains(d))
        || name.starts_with("test_")
        || ["_test.", ".test.", ".spec.", "Tests.", "Test.swift"].iter().any(|t| name.contains(t))
}

/// Whether a path in the repository is out of the wiki's scope by its name.
fn excluded(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let upper = name.to_ascii_uppercase();
    path.starts_with(".okf/")
        || is_test(path)
        || ["/vendor/", "/third_party/", "/third-party/", "/node_modules/", "/dist/", "/build/"].iter().any(|d| format!("/{path}").contains(d))
        || name.contains(".min.")
        || ["CHANGELOG", "LICENSE", "LICENCE", "NOTICE", "THIRD-PARTY", "CODE_OF_CONDUCT", "AGENTS.MD", "CLAUDE.MD"].iter().any(|p| upper.starts_with(p))
}

/// Whether a file in scope must belong to a page: code, not prose or settings.
pub fn needs_owner(path: &str) -> bool {
    !matches!(units::lang(path), Some(Lang::Markdown | Lang::Toml) | None)
}

/// The files in scope in the repository at `root`, sorted, as paths from it.
pub fn files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let walk = ignore::WalkBuilder::new(root).hidden(true).git_ignore(true).git_exclude(true).git_global(true).require_git(false).build();
    for entry in walk.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let Some(rel) = rel.to_str() else { continue };
        let rel = rel.replace('\\', "/");
        if units::lang(&rel).is_none() || excluded(&rel) || entry.metadata().is_ok_and(|m| m.len() > MAX_BYTES) {
            continue;
        }
        out.push(rel);
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_by_name() {
        assert!(is_test("crates/x/tests/a.rs") && is_test("ios/HarloUITests/A.swift") && is_test("a/b_test.go"));
        assert!(!is_test("crates/x/src/testing.rs"));
        assert!(excluded(".okf/overview.md") && excluded("site/x.min.js") && excluded("CHANGELOG.md") && excluded("web/node_modules/a.js"));
        assert!(!excluded("crates/harlo-okf/src/units.rs") && !excluded("README.md"));
        assert!(needs_owner("a.qml") && !needs_owner("README.md") && !needs_owner("Cargo.toml"));
    }

    #[test]
    fn walks_the_working_tree() {
        let dir = std::env::temp_dir().join(format!("harlo-okf-scope-{}", std::process::id()));
        for (p, t) in [("src/a.rs", "fn a() {}"), ("src/tests/b.rs", ""), ("notes/x.rs", ""), (".hidden/c.rs", ""), ("img.png", ""), ("README.md", "# R")] {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        }
        std::fs::write(dir.join(".ignore"), "notes/\n").unwrap();
        assert_eq!(files(&dir), ["README.md", "src/a.rs"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
