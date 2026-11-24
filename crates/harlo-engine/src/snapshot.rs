//! Snapshots of a project's files around each step, and undoing a step's
//! changes one file at a time.
//!
//! A shadow git repository, kept outside the project, records the project's
//! working tree (ignored files and `.git` left out) as a tree object before
//! and after each step. Undo never resets the tree: each file the step
//! changed is restored three-way, so edits made since by someone else stay,
//! and a file that can't be merged is left alone and reported.
//!
//! What a step "changed" is attributed: a file this session's tools wrote is
//! its own and is restored; one another session wrote meanwhile is that
//! session's and stays. Anything else changed meanwhile has no writer the
//! engine can name (a shell command the step ran, or a person in an editor
//! at the same moment): it's left as it is and reported, never guessed at. A push, a database write or a
//! call to another service can't be undone this way at all.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

/// A project's shadow repository. Clones share one lock, so two sessions
/// in the same folder never record at the same moment.
#[derive(Debug, Clone)]
pub struct Shadow {
    git_dir: PathBuf,
    work: PathBuf,
    lock: std::sync::Arc<tokio::sync::Mutex<()>>,
}

/// What undoing did to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restored {
    /// Put back as it was before the step.
    Restored,
    /// Edited since by someone else; the step's change was taken out and
    /// the later edit kept.
    Merged,
    /// Edited since in a way that clashes with undoing the step: left as
    /// it is.
    Conflict,
    /// Another session changed it during the step: left as it is.
    OtherSession,
    /// Changed during the step by a writer the engine can't name: left as
    /// it is.
    Ambiguous,
    /// Changed while the step ran a shell command, which may or may not
    /// have done it: left as it is.
    DuringCommand,
}

/// One file of an undo: its path, relative to the project, and what
/// happened to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileUndo {
    /// The file.
    pub path: String,
    /// What undoing did.
    pub outcome: Restored,
}

/// How the files a step changed came to change.
#[derive(Debug, Clone, Default)]
pub struct Attribution {
    /// Files this session's tools wrote during the step.
    pub own: Vec<String>,
    /// Files other sessions' tools wrote during the step.
    pub others: Vec<String>,
    /// Whether the step ran a shell command.
    pub ran_command: bool,
}

impl Shadow {
    /// The shadow repository for the project in `work`, kept in `store`
    /// (one folder per project).
    #[must_use]
    pub fn new(store: &Path, work: &Path) -> Self {
        Self {
            git_dir: store.to_path_buf(),
            work: work.to_path_buf(),
            lock: std::sync::Arc::default(),
        }
    }

    async fn git(&self, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let mut cmd = Command::new("git");
        cmd.arg("--git-dir")
            .arg(&self.git_dir)
            .arg("--work-tree")
            .arg(&self.work)
            .args(["-c", "core.autocrlf=false", "-c", "core.quotepath=false"])
            .args(args)
            .current_dir(&self.work)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("couldn't run git: {e}"))?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            use tokio::io::AsyncWriteExt as _;
            stdin
                .write_all(input)
                .await
                .map_err(|e| format!("couldn't write to git: {e}"))?;
        }
        let out = child
            .wait_with_output()
            .await
            .map_err(|e| format!("git failed: {e}"))?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
        }
    }

    /// Records the project's files as they are now; returns the snapshot's
    /// ID.
    ///
    /// # Errors
    ///
    /// When git can't be run or the shadow repository can't be made.
    pub async fn track(&self) -> Result<String, String> {
        let _one_at_a_time = self.lock.lock().await;
        if !self.git_dir.join("HEAD").exists() {
            std::fs::create_dir_all(&self.git_dir)
                .map_err(|e| format!("couldn't make {}: {e}", self.git_dir.display()))?;
            let made = Command::new("git")
                .args(["init", "-q", "--bare"])
                .arg(&self.git_dir)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .stdin(Stdio::null())
                .output()
                .await
                .map_err(|e| format!("couldn't run git: {e}"))?;
            if !made.status.success() {
                return Err(String::from_utf8_lossy(&made.stderr).trim().to_owned());
            }
            // The project's own repository never goes into a snapshot.
            let exclude = self.git_dir.join("info");
            let _ = std::fs::create_dir_all(&exclude);
            let _ = std::fs::write(exclude.join("exclude"), ".git\n");
        }
        self.git(&["add", "-A", "--", "."], None).await?;
        let tree = self.git(&["write-tree"], None).await?;
        Ok(String::from_utf8_lossy(&tree).trim().to_owned())
    }

    /// The files that differ between snapshots `from` and `to`.
    ///
    /// # Errors
    ///
    /// When either isn't a snapshot.
    pub async fn changed(&self, from: &str, to: &str) -> Result<Vec<String>, String> {
        let out = self
            .git(
                &[
                    "diff-tree",
                    "-r",
                    "--name-only",
                    "--no-renames",
                    "-z",
                    from,
                    to,
                ],
                None,
            )
            .await?;
        Ok(out
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect())
    }

    /// The changes from snapshot `from` to `to` as a unified diff, with
    /// `a/` and `b/` paths.
    ///
    /// # Errors
    ///
    /// When either isn't a snapshot.
    pub async fn patch(&self, from: &str, to: &str) -> Result<String, String> {
        let out = self
            .git(
                &[
                    "diff-tree",
                    "-p",
                    "-r",
                    "--no-color",
                    "--no-renames",
                    "--no-ext-diff",
                    from,
                    to,
                ],
                None,
            )
            .await?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// File `path` as snapshot `tree` has it; `None` when it had none.
    async fn blob(&self, tree: &str, path: &str) -> Option<Vec<u8>> {
        self.git(&["cat-file", "blob", &format!("{tree}:{path}")], None)
            .await
            .ok()
    }

    /// Undoes what the step between snapshots `start` and `end` did to the
    /// project's files, as `who` attributes it. Each file is restored on its
    /// own; nothing else is touched.
    ///
    /// # Errors
    ///
    /// When a snapshot can't be read.
    pub async fn undo(
        &self,
        start: &str,
        end: &str,
        who: &Attribution,
    ) -> Result<Vec<FileUndo>, String> {
        let mut out = Vec::new();
        for path in self.changed(start, end).await? {
            let outcome = if who.others.contains(&path) && !who.own.contains(&path) {
                Restored::OtherSession
            } else if who.own.contains(&path) {
                self.restore(start, end, &path).await
            } else if who.ran_command {
                Restored::DuringCommand
            } else {
                Restored::Ambiguous
            };
            out.push(FileUndo { path, outcome });
        }
        Ok(out)
    }

    /// Restores `path` three-way: from what the step left (`end`) back to
    /// what it found (`start`), on top of the file as it is now.
    async fn restore(&self, start: &str, end: &str, path: &str) -> Restored {
        let before = self.blob(start, path).await;
        let after = self.blob(end, path).await;
        let full = self.work.join(path);
        let now = std::fs::read(&full).ok();
        if now == after {
            return match &before {
                Some(bytes) => {
                    if let Some(dir) = full.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    if std::fs::write(&full, bytes).is_ok() {
                        Restored::Restored
                    } else {
                        Restored::Conflict
                    }
                }
                None => {
                    if std::fs::remove_file(&full).is_ok() {
                        Restored::Restored
                    } else {
                        Restored::Conflict
                    }
                }
            };
        }
        // Changed since: take the step's change out of the file as it is.
        let (Some(now), Some(after), Some(before)) = (now, after, before) else {
            return Restored::Conflict;
        };
        match merge(&now, &after, &before).await {
            Some(merged) if std::fs::write(&full, &merged).is_ok() => Restored::Merged,
            _ => Restored::Conflict,
        }
    }
}

/// `current` with the change from `base` to `other` applied, when the two
/// don't clash.
async fn merge(current: &[u8], base: &[u8], other: &[u8]) -> Option<Vec<u8>> {
    let dir = tempfile_dir()?;
    let write = |name: &str, bytes: &[u8]| {
        let p = dir.join(name);
        std::fs::write(&p, bytes).ok().map(|()| p)
    };
    let (c, b, o) = (
        write("current", current)?,
        write("base", base)?,
        write("other", other)?,
    );
    let out = Command::new("git")
        .args(["merge-file", "-p", "--"])
        .args([&c, &b, &o])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out?;
    // 0: clean; a positive count: conflicts; negative: an error.
    (out.status.code() == Some(0)).then_some(out.stdout)
}

/// A fresh private folder for merge inputs.
fn tempfile_dir() -> Option<PathBuf> {
    let dir = std::env::temp_dir().join(format!("harlo-merge-{}", crate::ascending_id("m")));
    std::fs::create_dir_all(&dir).ok().map(|()| dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_init(dir: &Path) {
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .status()
            .expect("git");
    }

    fn put(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).expect("write");
    }

    fn get(dir: &Path, name: &str) -> String {
        std::fs::read_to_string(dir.join(name)).unwrap_or_default()
    }

    fn own(files: &[&str]) -> Attribution {
        Attribution {
            own: files.iter().map(|f| (*f).to_owned()).collect(),
            ..Attribution::default()
        }
    }

    #[tokio::test]
    async fn uncommitted_work_from_before_the_step_stays() {
        let t = tempfile::tempdir().expect("dir");
        let work = t.path().join("p");
        std::fs::create_dir_all(&work).expect("dir");
        git_init(&work);
        put(&work, ".gitignore", "target/\n");
        put(&work, "a.txt", "mine, not committed\n");
        std::fs::create_dir_all(work.join("target")).expect("dir");
        put(&work, "target/big", "ignored\n");
        let shadow = Shadow::new(&t.path().join("shadow"), &work);
        let start = shadow.track().await.expect("start");
        put(&work, "a.txt", "the agent's\n");
        put(&work, "new.txt", "made by the agent\n");
        put(&work, "target/big", "ignored, changed\n");
        let end = shadow.track().await.expect("end");

        let done = shadow
            .undo(&start, &end, &own(&["a.txt", "new.txt"]))
            .await
            .expect("undo");
        assert_eq!(get(&work, "a.txt"), "mine, not committed\n");
        assert!(!work.join("new.txt").exists());
        assert_eq!(
            get(&work, "target/big"),
            "ignored, changed\n",
            "ignored files untouched"
        );
        assert!(
            done.iter().all(|f| f.outcome == Restored::Restored),
            "{done:?}"
        );
    }

    #[tokio::test]
    async fn a_later_edit_is_kept_or_reported() {
        let t = tempfile::tempdir().expect("dir");
        let work = t.path().join("p");
        std::fs::create_dir_all(&work).expect("dir");
        put(&work, "a.txt", "one\ntwo\nthree\nfour\nfive\n");
        put(&work, "b.txt", "x\n");
        let shadow = Shadow::new(&t.path().join("shadow"), &work);
        let start = shadow.track().await.expect("start");
        put(&work, "a.txt", "one\nTWO\nthree\nfour\nfive\n");
        put(&work, "b.txt", "y\n");
        let end = shadow.track().await.expect("end");
        // Afterwards someone else edits both: a.txt elsewhere, b.txt the
        // same line.
        put(&work, "a.txt", "one\nTWO\nthree\nfour\nFIVE\n");
        put(&work, "b.txt", "z\n");

        let done = shadow
            .undo(&start, &end, &own(&["a.txt", "b.txt"]))
            .await
            .expect("undo");
        assert_eq!(
            get(&work, "a.txt"),
            "one\ntwo\nthree\nfour\nFIVE\n",
            "the later edit stays"
        );
        assert_eq!(get(&work, "b.txt"), "z\n", "a clash is left alone");
        let outcome = |p: &str| done.iter().find(|f| f.path == p).map(|f| f.outcome.clone());
        assert_eq!(outcome("a.txt"), Some(Restored::Merged));
        assert_eq!(outcome("b.txt"), Some(Restored::Conflict));
    }

    #[tokio::test]
    async fn an_edit_during_the_step_by_another_writer_is_kept() {
        let t = tempfile::tempdir().expect("dir");
        let work = t.path().join("p");
        std::fs::create_dir_all(&work).expect("dir");
        put(&work, "a.txt", "a\n");
        put(&work, "b.txt", "b\n");
        put(&work, "c.txt", "c\n");
        let shadow = Shadow::new(&t.path().join("shadow"), &work);
        let start = shadow.track().await.expect("start");
        put(&work, "a.txt", "a by this session\n");
        put(&work, "b.txt", "b by another session\n");
        put(&work, "c.txt", "c by someone unknown\n");
        let end = shadow.track().await.expect("end");

        let who = Attribution {
            own: vec!["a.txt".to_owned()],
            others: vec!["b.txt".to_owned()],
            ran_command: false,
        };
        let done = shadow.undo(&start, &end, &who).await.expect("undo");
        assert_eq!(get(&work, "a.txt"), "a\n");
        assert_eq!(get(&work, "b.txt"), "b by another session\n");
        assert_eq!(get(&work, "c.txt"), "c by someone unknown\n");
        let outcome = |p: &str| done.iter().find(|f| f.path == p).map(|f| f.outcome.clone());
        assert_eq!(outcome("b.txt"), Some(Restored::OtherSession));
        assert_eq!(outcome("c.txt"), Some(Restored::Ambiguous));
    }
}
