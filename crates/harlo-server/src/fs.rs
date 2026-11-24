//! The project's files, as the `/api/fs` routes serve them: finding
//! by fuzzy path (the composer's `@` menu), listing a folder (the Files
//! panel), reading and writing a file. Paths are relative to the session's
//! folder, and none may lead outside it.

// Rust guideline compliant 2026-10-09

use std::path::{Component, Path, PathBuf};

use frizbee::{Config, Matcher};
use ignore::WalkBuilder;
use serde_json::{Value, json};

/// The most entries a find walks, so a huge folder stays quick.
const WALK_LIMIT: usize = 50_000;

/// One entry: its path relative to the folder, and
/// whether it's a file or a folder.
fn entry(path: &str, folder: bool) -> Value {
    json!({ "path": path, "type": if folder { "directory" } else { "file" } })
}

/// The files and folders under `root` whose path matches `query`, best
/// first, at most `limit`; only files or only folders when `kind` says.
/// Ignored files are left out, as git sees them.
#[must_use]
pub fn find(root: &Path, query: &str, kind: Option<&str>, limit: usize) -> Vec<Value> {
    let mut paths: Vec<(String, bool)> = Vec::new();
    for found in WalkBuilder::new(root).build().flatten().take(WALK_LIMIT) {
        let Ok(relative) = found.path().strip_prefix(root) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let folder = found.file_type().is_some_and(|t| t.is_dir());
        if kind.is_some_and(|k| (k == "directory") != folder) {
            continue;
        }
        let mut path = relative.to_string_lossy().into_owned();
        if folder {
            path.push('/');
        }
        paths.push((path, folder));
    }
    let query = query.trim();
    if query.is_empty() {
        paths.sort();
        return paths
            .into_iter()
            .take(limit)
            .map(|(p, f)| entry(&p, f))
            .collect();
    }
    let haystacks: Vec<&str> = paths.iter().map(|(p, _)| p.as_str()).collect();
    let mut matcher = Matcher::new(query, &Config::default());
    let mut ranked = matcher.match_list(&haystacks);
    // Best score first; among equals, the shorter path.
    ranked.sort_by(|a, b| {
        b.score.cmp(&a.score).then_with(|| {
            haystacks[a.index as usize]
                .len()
                .cmp(&haystacks[b.index as usize].len())
        })
    });
    ranked
        .into_iter()
        .take(limit)
        .filter_map(|m| paths.get(m.index as usize))
        .map(|(p, f)| entry(p, *f))
        .collect()
}

/// The direct children of `path` (relative to `root`, or `root` itself
/// when empty): folders first, then files, each by name. `.git` is left
/// out.
///
/// # Errors
///
/// When `path` leads outside `root` or isn't a readable folder.
pub fn list(root: &Path, path: &str) -> Result<Vec<Value>, String> {
    let folder = inside(root, path)?;
    let read = std::fs::read_dir(&folder).map_err(|e| format!("Couldn't list {path}: {e}"))?;
    let mut children: Vec<(bool, String)> = read
        .flatten()
        .filter(|e| e.file_name() != ".git")
        .map(|e| {
            let is_folder = e.file_type().is_ok_and(|t| t.is_dir());
            let name = e.file_name().to_string_lossy().into_owned();
            (is_folder, name)
        })
        .collect();
    children.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let prefix = path.trim_matches('/');
    Ok(children
        .into_iter()
        .map(|(is_folder, name)| {
            let mut relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if is_folder {
                relative.push('/');
            }
            entry(&relative, is_folder)
        })
        .collect())
}

/// `path` under `root`, refusing one that leads outside it (through `..`
/// or an absolute path).
///
/// # Errors
///
/// When it would lead outside.
pub fn inside(root: &Path, path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(path.trim_start_matches('/'));
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("{path} isn't inside the project's folder."));
    }
    Ok(root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_ranks_paths_and_list_orders_folders_first() {
        let t = tempfile::tempdir().expect("dir");
        let root = t.path();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .status()
            .expect("git");
        std::fs::write(root.join(".gitignore"), "target/\n").expect("ignore");
        std::fs::create_dir_all(root.join("src/app")).expect("src");
        std::fs::create_dir_all(root.join("target")).expect("target");
        std::fs::write(root.join("src/app/main.rs"), "").expect("main");
        std::fs::write(root.join("src/lib.rs"), "").expect("lib");
        std::fs::write(root.join("target/main.rs"), "").expect("gen");
        std::fs::write(root.join("b.txt"), "").expect("b");

        let found = find(root, "main", None, 10);
        assert_eq!(found[0]["path"], "src/app/main.rs", "{found:?}");
        assert!(
            !found
                .iter()
                .any(|e| e["path"].as_str().is_some_and(|p| p.starts_with("target")))
        );
        let folders = find(root, "app", Some("directory"), 10);
        assert_eq!(
            folders[0],
            json!({ "path": "src/app/", "type": "directory" })
        );

        let top = list(root, "").expect("list");
        let names: Vec<&str> = top.iter().filter_map(|e| e["path"].as_str()).collect();
        assert_eq!(names, ["src/", "target/", ".gitignore", "b.txt"]);
        assert_eq!(list(root, "src").expect("src")[0]["path"], "src/app/");
        inside(root, "../etc/passwd").expect_err("outside");
        inside(root, "/etc/passwd")
            .map(|p| assert!(p.starts_with(root)))
            .expect("made relative");
    }
}
