//! Git patch transport for local and remote helper checkouts.

// Rust guideline compliant 2026-10-09

/// Reads a transferable patch between two Git trees without decoding its contents.
///
/// The patch is the same whatever the user's Git settings say about
/// diffs: no colour, no external diff or text conversion, no renames, and
/// the `a/` and `b/` prefixes [`apply`] expects (`diff.noprefix` would
/// drop them).
///
/// # Errors
///
/// Returns the local or remote Git runner's error if diffing fails.
pub fn between<E>(base: &str, tree: &str, git: impl FnOnce(&[&str]) -> Result<Vec<u8>, E>) -> Result<Vec<u8>, E> {
    git(&[
        "diff",
        "--binary",
        "--full-index",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        base,
        tree,
    ])
}

/// Applies or checks a patch without allowing Git to rewrite its whitespace.
///
/// `source` names the patch file, or `-` for a runner supplying the bytes
/// on standard input. `options` can select files, reverse, or check only.
///
/// # Errors
///
/// Returns the runner's error if Git rejects the patch.
pub fn apply<T, E>(source: &str, options: &[&str], git: impl FnOnce(&[&str]) -> Result<T, E>) -> Result<(), E> {
    let mut args = vec!["apply", "--whitespace=nowarn"];
    args.extend_from_slice(options);
    args.push(source);
    git(&args).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::{Command, Output};

    /// Git in `dir`, with only the repository's own settings.
    fn git(dir: &Path, args: &[&str]) -> Output {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        out
    }

    #[test]
    fn the_users_diff_settings_dont_change_the_patch() {
        let dir = std::env::temp_dir().join(format!("harlo-worktree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        for (key, value) in [("user.email", "t@t"), ("user.name", "t"), ("diff.noprefix", "true"), ("color.diff", "always"), ("color.ui", "always")] {
            git(&dir, &["config", key, value]);
        }
        std::fs::write(dir.join("f.txt"), "one\n").unwrap();
        git(&dir, &["add", "f.txt"]);
        git(&dir, &["commit", "-qm", "base"]);
        let base = String::from_utf8(git(&dir, &["rev-parse", "HEAD"]).stdout).unwrap().trim().to_owned();
        let exact = b"one\ntrailing  \nnon-UTF-8: \xff\n";
        std::fs::write(dir.join("f.txt"), exact).unwrap();
        git(&dir, &["add", "f.txt"]);
        let tree = String::from_utf8(git(&dir, &["write-tree"]).stdout).unwrap().trim().to_owned();
        git(&dir, &["reset", "-q", "--hard"]);

        let patch = between(&base, &tree, |args| Ok::<_, ()>(git(&dir, args).stdout)).unwrap();
        assert!(patch.starts_with(b"diff --git a/f.txt b/f.txt\n"), "{}", String::from_utf8_lossy(&patch));
        let file = dir.join("p.patch");
        std::fs::write(&file, &patch).unwrap();
        let path = file.to_str().unwrap();
        apply(path, &["--check"], |args| Ok::<_, ()>(git(&dir, args))).unwrap();
        apply(path, &[], |args| Ok::<_, ()>(git(&dir, args))).unwrap();
        assert_eq!(std::fs::read(dir.join("f.txt")).unwrap(), exact);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
