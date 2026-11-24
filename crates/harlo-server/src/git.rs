//! What the server asks git: a folder's project and its uncommitted
//! changes.

// Rust guideline compliant 2026-10-09

use std::process::Stdio;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::process::Command;

/// The ID of the project `directory` belongs to: its repository's first
/// commit, so every worktree of a repository shares it; for a folder
/// outside git, a hash of its path.
pub async fn project_id(directory: &str) -> String {
    let root = git(directory, &["rev-list", "--max-parents=0", "HEAD"]).await;
    if let Some(first) = root
        .as_deref()
        .and_then(|out| out.lines().next())
        .filter(|l| !l.is_empty())
    {
        return first.to_owned();
    }
    let digest = Sha256::digest(directory.as_bytes());
    hex::encode(digest)[..40].to_owned()
}

/// Whether `directory` is in a git repository.
pub async fn is_repo(directory: &str) -> bool {
    git(directory, &["rev-parse", "--is-inside-work-tree"])
        .await
        .is_some_and(|o| o.trim() == "true")
}

/// The uncommitted changes in `directory`, one entry per file, as
/// OpenCode reports them: `{ file, patch, additions, deletions, status }`.
/// With `only`, just those files.
pub async fn changes(directory: &str, only: Option<&[String]>) -> Vec<Value> {
    let mut args = vec!["diff", "--no-color", "--no-ext-diff", "HEAD", "--"];
    if let Some(files) = only {
        if files.is_empty() {
            return Vec::new();
        }
        args.extend(files.iter().map(String::as_str));
    }
    let mut out: Vec<Value> = git(directory, &args)
        .await
        .map(|text| split(&text))
        .unwrap_or_default();
    let untracked = git(directory, &["ls-files", "--others", "--exclude-standard"])
        .await
        .unwrap_or_default();
    for file in untracked
        .lines()
        .filter(|f| only.is_none_or(|o| o.iter().any(|x| x == f)))
    {
        let Ok(output) = Command::new("git")
            .args([
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-index",
                "--",
                "/dev/null",
                file,
            ])
            .current_dir(directory)
            .stdin(Stdio::null())
            .output()
            .await
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        out.extend(split(&text));
    }
    out
}

/// `git diff` output, split into one entry per file.
fn split(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for chunk in text.split("diff --git ").skip(1) {
        let patch = format!("diff --git {chunk}");
        let file = chunk
            .lines()
            .find_map(|l| l.strip_prefix("+++ b/"))
            .or_else(|| chunk.lines().find_map(|l| l.strip_prefix("--- a/")))
            .unwrap_or_else(|| {
                chunk
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .split(" b/")
                    .last()
                    .unwrap_or_default()
            })
            .to_owned();
        let status = if chunk.contains("\nnew file mode") {
            "added"
        } else if chunk.contains("\ndeleted file mode") {
            "deleted"
        } else {
            "modified"
        };
        let body = chunk.lines().skip_while(|l| !l.starts_with("@@"));
        let (mut additions, mut deletions) = (0, 0);
        for line in body {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
        }
        out.push(json!({ "file": file, "patch": patch, "additions": additions, "deletions": deletions, "status": status }));
    }
    out
}

/// What `git args` prints in `directory`, when it succeeds.
async fn git(directory: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn changes_cover_edited_and_new_files() {
        let dir = tempfile::tempdir().expect("dir");
        let d = dir.path().to_str().expect("utf-8").to_owned();
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&d)
                .output()
                .expect("git");
        };
        run(&["init", "-q"]);
        std::fs::write(dir.path().join("a.txt"), "alpha\n").expect("write");
        run(&["add", "."]);
        run(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "init",
        ]);
        let id = project_id(&d).await;
        assert_eq!(id.len(), 40);
        assert!(is_repo(&d).await);

        std::fs::write(dir.path().join("a.txt"), "omega\n").expect("write");
        std::fs::write(dir.path().join("new.txt"), "one\ntwo\n").expect("write");
        let all = changes(&d, None).await;
        assert_eq!(all.len(), 2, "{all:?}");
        assert_eq!(all[0]["file"], "a.txt");
        assert_eq!(
            (all[0]["additions"].as_u64(), all[0]["deletions"].as_u64()),
            (Some(1), Some(1))
        );
        assert!(
            all[0]["patch"]
                .as_str()
                .is_some_and(|p| p.contains("-alpha\n+omega"))
        );
        assert_eq!(
            (all[1]["file"].as_str(), all[1]["status"].as_str()),
            (Some("new.txt"), Some("added"))
        );
        assert_eq!(all[1]["additions"].as_u64(), Some(2));
        let only = changes(&d, Some(&["a.txt".to_owned()])).await;
        assert_eq!(only.len(), 1);
    }
}
