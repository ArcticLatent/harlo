//! Reading a repository's status, and staging, committing and pushing, with
//! the real `git` in scratch folders.
//!
//! Each `git` gets a scratch `HOME` and no system config, so the user's own
//! settings, hooks and signing never apply.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_core::git::{FileState, Repo, parse_status};

/// A repository in its own scratch folder, cut off from the user's settings.
struct Scratch {
    _dir: tempfile::TempDir,
    work: std::path::PathBuf,
    remote: std::path::PathBuf,
    home: std::path::PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("scratch folder");
        let work = dir.path().join("work");
        let remote = dir.path().join("remote.git");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&work).expect("work folder");
        std::fs::create_dir_all(&home).expect("home folder");
        let scratch = Self {
            _dir: dir,
            work,
            remote,
            home,
        };
        scratch.run(
            scratch.work.as_path(),
            &["init", "--quiet", "--initial-branch=main"],
        );
        scratch.run(
            scratch.work.as_path(),
            &[
                "init",
                "--quiet",
                "--bare",
                "--initial-branch=main",
                scratch.remote.to_str().expect("utf-8 path"),
            ],
        );
        scratch
    }

    /// The repository as Harlo runs `git` in it.
    fn repo(&self) -> Repo {
        let home = self.home.to_str().expect("utf-8 path");
        Repo::new(&self.work)
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
    }

    /// Runs `git` directly, with the same isolation, to set things up.
    fn run(&self, dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .status()
            .expect("git runs")
            .success();
        assert!(ok, "git {args:?}");
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.work.join(name), text).expect("write file");
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

fn state(path: &str, staged: bool, unstaged: bool) -> FileState {
    FileState {
        path: path.to_owned(),
        staged,
        unstaged,
    }
}

#[test]
fn status_reads_branch_counts_and_each_kind_of_entry() {
    let out = [
        "# branch.oid 0123abcd",
        "# branch.head main",
        "# branch.upstream origin/main",
        "# branch.ab +2 -1",
        "1 M. N... 100644 100644 100644 aaaa bbbb staged.rs",
        "1 .M N... 100644 100644 100644 aaaa aaaa with space.md",
        "2 RM N... 100644 100644 100644 aaaa bbbb R90 new name.rs",
        "old name.rs",
        "u UU N... 100644 100644 100644 100644 aaaa bbbb cccc both.rs",
        "? untracked.txt",
        "",
    ]
    .join("\0");
    let status = parse_status(&out);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!((status.ahead, status.behind), (2, 1));
    assert_eq!(
        status.files,
        vec![
            state("staged.rs", true, false),
            state("with space.md", false, true),
            state("new name.rs", true, true),
            state("both.rs", true, true),
            state("untracked.txt", false, true),
        ]
    );
}

#[test]
fn a_detached_head_has_no_branch() {
    let status = parse_status("# branch.oid 0123\0# branch.head (detached)\0");
    assert_eq!(status.branch, None);
    assert!(status.files.is_empty());
}

#[test]
fn stage_commit_and_push_in_a_real_repository() {
    let scratch = Scratch::new();
    let repo = scratch.repo();
    let rt = runtime();
    scratch.write("a.txt", "one\n");
    scratch.write("b.txt", "two\n");

    let status = rt.block_on(repo.status()).expect("status");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(!status.has_staged());
    assert_eq!(status.files.len(), 2);

    rt.block_on(repo.stage(&["a.txt".to_owned()]))
        .expect("stage");
    let status = rt.block_on(repo.status()).expect("status");
    assert_eq!(status.file("a.txt"), Some(&state("a.txt", true, false)));
    assert_eq!(status.file("b.txt"), Some(&state("b.txt", false, true)));

    // Before the first commit, unstaging still works.
    rt.block_on(repo.unstage(&["a.txt".to_owned()]))
        .expect("unstage");
    assert!(!rt.block_on(repo.status()).expect("status").has_staged());

    rt.block_on(repo.stage(&["a.txt".to_owned()]))
        .expect("stage");
    let summary = rt.block_on(repo.commit("Add a")).expect("commit");
    assert!(summary.ends_with(" Add a"), "{summary}");
    let status = rt.block_on(repo.status()).expect("status");
    assert_eq!(status.files, vec![state("b.txt", false, true)]);

    let remote = scratch.remote.to_str().expect("utf-8 path");
    scratch.run(&scratch.work, &["remote", "add", "origin", remote]);
    scratch.run(&scratch.work, &["push", "--quiet", "-u", "origin", "main"]);
    scratch.write("a.txt", "one\nmore\n");
    rt.block_on(repo.stage(&["a.txt".to_owned()]))
        .expect("stage");
    rt.block_on(repo.commit("More a")).expect("commit");
    let status = rt.block_on(repo.status()).expect("status");
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!((status.ahead, status.behind), (1, 0));

    rt.block_on(repo.push()).expect("push");
    let status = rt.block_on(repo.status()).expect("status");
    assert_eq!((status.ahead, status.behind), (0, 0));
}

#[test]
fn failures_carry_what_git_said() {
    let scratch = Scratch::new();
    let rt = runtime();
    let err = rt
        .block_on(scratch.repo().commit("Nothing"))
        .expect_err("nothing staged");
    assert!(!err.message.is_empty());
    // No upstream to push to.
    let err = rt.block_on(scratch.repo().push()).expect_err("no upstream");
    assert!(!err.message.is_empty());

    let outside = tempfile::tempdir().expect("scratch folder");
    let err = rt
        .block_on(
            Repo::new(outside.path())
                .env(
                    "GIT_CEILING_DIRECTORIES",
                    outside
                        .path()
                        .parent()
                        .and_then(Path::to_str)
                        .unwrap_or("/"),
                )
                .status(),
        )
        .expect_err("not a repository");
    assert!(err.message.contains("not a git repository"), "{err}");
}

#[test]
fn a_subfolder_stages_by_paths_from_the_root() {
    let scratch = Scratch::new();
    let rt = runtime();
    std::fs::create_dir_all(scratch.work.join("app")).expect("subfolder");
    scratch.write("app/[x].txt", "brackets\n");
    scratch.write("top.txt", "top\n");
    let repo = scratch.repo();
    let inner = Repo::new(scratch.work.join("app"));
    let inner = [
        ("HOME", scratch.home.to_str().expect("utf-8 path")),
        ("GIT_CONFIG_NOSYSTEM", "1"),
    ]
    .into_iter()
    .fold(inner, |repo, (k, v)| repo.env(k, v));
    let status = rt.block_on(inner.status()).expect("status");
    assert_eq!(status.prefix, "app/");
    assert_eq!(rt.block_on(repo.status()).expect("status").prefix, "");
    // Root-relative, and taken literally rather than as a pattern.
    rt.block_on(inner.stage(&["app/[x].txt".to_owned(), "top.txt".to_owned()]))
        .expect("stage");
    let status = rt.block_on(repo.status()).expect("status");
    assert_eq!(
        status.file("app/[x].txt"),
        Some(&state("app/[x].txt", true, false))
    );
    assert_eq!(status.file("top.txt"), Some(&state("top.txt", true, false)));
}
