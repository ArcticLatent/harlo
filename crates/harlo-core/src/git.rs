//! A local repository's state, and staging, committing and pushing its changes.
//!
//! These run the `git` command in the session's folder rather than a Git
//! library: commits then go through the repository's own hooks and signing
//! settings, and pushes use the credential helpers and SSH setup `git`
//! already has. They need the folder on this computer, so Harlo offers them
//! only when it is. Reading what changed works anywhere, through OpenCode
//! (see `opencode_client::Client::working_changes`).
//!
//! `git` never prompts: `GIT_TERMINAL_PROMPT=0` makes a push that needs a
//! password fail with a message instead of waiting on a terminal nobody sees.

// Rust guideline compliant 2026-10-09

use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;

/// A file with changes, and how much of them is staged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    /// Path relative to the repository's root.
    pub path: String,
    /// Some of its changes are staged for the next commit.
    pub staged: bool,
    /// Some of its changes aren't staged.
    pub unstaged: bool,
}

/// A repository's branch and changed files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// The current branch; `None` when `HEAD` is detached.
    pub branch: Option<String>,
    /// The branch it pushes to, such as `origin/main`, if it has one.
    pub upstream: Option<String>,
    /// Commits here that the upstream doesn't have.
    pub ahead: u32,
    /// Commits on the upstream not here.
    pub behind: u32,
    /// Changed and untracked files, in `git`'s order.
    pub files: Vec<FileState>,
    /// Where the folder sits in the repository, such as `app/`; empty at its root.
    pub prefix: String,
}

impl Status {
    /// Returns whether anything is staged for the next commit.
    #[must_use]
    pub fn has_staged(&self) -> bool {
        self.files.iter().any(|f| f.staged)
    }

    /// Returns how `path` stands, if it has changes.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&FileState> {
        self.files.iter().find(|f| f.path == path)
    }
}

/// A `git` command that couldn't run or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// What `git` said, or why it couldn't run.
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// A folder in a Git repository, to run `git` in.
///
/// # Examples
///
/// ```no_run
/// # async fn run() -> Result<(), harlo_core::git::Error> {
/// let repo = harlo_core::git::Repo::new("/home/me/project");
/// let status = repo.status().await?;
/// println!("{} changed files", status.files.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    dir: PathBuf,
    env: Vec<(String, String)>,
}

impl Repo {
    /// Creates a handle on the repository that holds `dir`.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            env: Vec::new(),
        }
    }

    /// Sets an environment variable for every `git` command run here.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Reads the repository's branch and changed files.
    ///
    /// # Errors
    ///
    /// Fails when the folder isn't in a Git repository or `git` can't run.
    pub async fn status(&self) -> Result<Status, Error> {
        let out = self
            .git(&[
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=all",
            ])
            .await?;
        let prefix = self.git(&["rev-parse", "--show-prefix"]).await?;
        Ok(Status {
            prefix: prefix.trim().to_owned(),
            ..parse_status(&out)
        })
    }

    /// Stages `paths`, as they are now, for the next commit.
    ///
    /// Paths are relative to the repository's root, as [`Status`] gives them.
    ///
    /// # Errors
    ///
    /// Fails when a path can't be staged or `git` can't run.
    pub async fn stage(&self, paths: &[String]) -> Result<(), Error> {
        self.with_paths(&["add", "--all"], paths).await
    }

    /// Takes `paths` out of the next commit, keeping their changes in the files.
    ///
    /// Paths are relative to the repository's root, as [`Status`] gives them.
    ///
    /// # Errors
    ///
    /// Fails when a path can't be unstaged or `git` can't run.
    pub async fn unstage(&self, paths: &[String]) -> Result<(), Error> {
        // `reset` works before the first commit too, where `restore --staged` doesn't.
        self.with_paths(&["reset", "--quiet"], paths).await
    }

    /// Runs `git` with `args` on `paths`, each taken from the root as it's written.
    async fn with_paths(&self, args: &[&str], paths: &[String]) -> Result<(), Error> {
        let specs: Vec<String> = paths
            .iter()
            .map(|path| format!(":(top,literal){path}"))
            .collect();
        let mut all = args.to_vec();
        all.push("--");
        all.extend(specs.iter().map(String::as_str));
        self.git(&all).await.map(drop)
    }

    /// Commits what's staged, with `message`, and returns its short hash and subject.
    ///
    /// The repository's hooks and signing settings apply.
    ///
    /// # Errors
    ///
    /// Fails when nothing is staged, a hook refuses, or `git` can't run.
    pub async fn commit(&self, message: &str) -> Result<String, Error> {
        self.git(&["commit", "--quiet", "--message", message])
            .await?;
        let summary = self.git(&["log", "-1", "--format=%h %s"]).await?;
        Ok(summary.trim().to_owned())
    }

    /// Stages `paths` as they are now and commits them with whatever else
    /// is staged, with `message`; returns the commit's short hash and subject.
    ///
    /// Paths are relative to the repository's root, as [`Status`] gives them.
    /// Given every file `git status` lists, it commits all uncommitted changes.
    ///
    /// # Errors
    ///
    /// Fails when a path can't be staged, nothing is left to commit, a hook
    /// refuses, or `git` can't run.
    pub async fn commit_all(&self, paths: &[String], message: &str) -> Result<String, Error> {
        self.stage(paths).await?;
        self.commit(message).await
    }

    /// Pushes the current branch to its upstream.
    ///
    /// # Errors
    ///
    /// Fails when the branch has no upstream, the remote refuses, credentials
    /// are needed that `git` can't supply without asking, or `git` can't run.
    pub async fn push(&self) -> Result<(), Error> {
        self.git(&["push", "--quiet"]).await.map(drop)
    }

    /// Returns which of `paths` Git ignores.
    ///
    /// Paths are relative to the folder this handle was made for. Outside a
    /// repository, nothing is ignored.
    ///
    /// # Errors
    ///
    /// Fails when `git` can't run.
    pub async fn ignored(&self, paths: &[String]) -> Result<HashSet<String>, Error> {
        use tokio::io::AsyncWriteExt;
        if paths.is_empty() {
            return Ok(HashSet::new());
        }
        let cant_run = |e: std::io::Error| Error {
            message: format!("Couldn't run git: {e}"),
        };
        let mut child = Command::new("git")
            .args(["check-ignore", "-z", "--stdin"])
            .current_dir(&self.dir)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(cant_run)?;
        let mut input = Vec::new();
        for path in paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&input).await.map_err(cant_run)?;
        }
        let output = child.wait_with_output().await.map_err(cant_run)?;
        // 0: some are ignored; 1: none are; anything else: not a repository.
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Ok(HashSet::new());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// Runs `git` with `args` and returns what it printed.
    async fn git(&self, args: &[&str]) -> Result<String, Error> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| Error {
                message: format!("Couldn't run git: {e}"),
            })?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        let said = String::from_utf8_lossy(&output.stderr);
        let message = match said.trim() {
            "" => String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            said => said.to_owned(),
        };
        Err(Error { message })
    }
}

/// Parses `git status --porcelain=v2 --branch -z`.
#[must_use]
pub fn parse_status(out: &str) -> Status {
    let mut status = Status::default();
    let mut records = out.split('\0');
    while let Some(record) = records.next() {
        let (kind, rest) = record.split_once(' ').unwrap_or((record, ""));
        match kind {
            "#" => branch_header(&mut status, rest),
            "1" => status.files.push(changed(rest, ORDINARY_FIELDS)),
            "2" => {
                status.files.push(changed(rest, RENAMED_FIELDS));
                // A rename's original path follows as its own record.
                let _ = records.next();
            }
            "u" => status.files.push(changed(rest, UNMERGED_FIELDS)),
            "?" => status.files.push(FileState {
                path: rest.to_owned(),
                staged: false,
                unstaged: true,
            }),
            _ => {}
        }
    }
    status
}

/// Fields between the flags and the path of a changed entry: submodule,
/// three modes and two hashes.
const ORDINARY_FIELDS: usize = 6;

/// Fields between the flags and the path of a renamed or copied entry: an
/// ordinary entry's, plus the similarity score.
const RENAMED_FIELDS: usize = 7;

/// Fields between the flags and the path of an unmerged entry: submodule,
/// four modes and three hashes.
const UNMERGED_FIELDS: usize = 8;

/// Reads a `# branch.…` header into `status`.
fn branch_header(status: &mut Status, header: &str) {
    let mut words = header.split(' ');
    match (words.next(), words.next()) {
        (Some("branch.head"), Some(head)) if head != "(detached)" => {
            status.branch = Some(head.to_owned());
        }
        (Some("branch.upstream"), Some(upstream)) => {
            status.upstream = Some(upstream.to_owned());
        }
        (Some("branch.ab"), Some(ahead)) => {
            status.ahead = ahead.trim_start_matches('+').parse().unwrap_or_default();
            status.behind = words
                .next()
                .map(|b| b.trim_start_matches('-'))
                .and_then(|b| b.parse().ok())
                .unwrap_or_default();
        }
        _ => {}
    }
}

/// Reads a changed entry: its `XY` flags, then `fields` more fields, then
/// the path (which may hold spaces).
fn changed(rest: &str, fields: usize) -> FileState {
    let mut parts = rest.splitn(fields + 2, ' ');
    let mut flags = parts.next().unwrap_or("..").chars();
    let path = parts.nth(fields).unwrap_or_default().to_owned();
    let index = flags.next().unwrap_or('.');
    let tree = flags.next().unwrap_or('.');
    FileState {
        path,
        staged: index != '.',
        unstaged: tree != '.',
    }
}
