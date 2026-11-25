//! A local repository's state, and staging, committing and pushing its changes.
//!
//! These run the `git` command in the session's folder rather than a Git
//! library: commits then go through the repository's own hooks and signing
//! settings, and pushes use the credential helpers and SSH setup `git`
//! already has. They need the folder on this computer, so Harlo offers them
//! only when it is. Reading what changed works anywhere, through the server
//! (see `harlo_client::Client::working_changes`).
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileState {
    /// Path relative to the repository's root.
    pub path: String,
    /// Some of its changes are staged for the next commit.
    pub staged: bool,
    /// Some of its changes aren't staged.
    pub unstaged: bool,
    /// Git's letter for the change: `M` modified, `A` added, `D` deleted,
    /// `R` renamed, `C` copied, `T` type changed, `U` in conflict, `?`
    /// untracked. The unstaged change's when there is one.
    pub code: char,
    /// The path it was renamed or copied from, relative to the root.
    pub from: Option<String>,
}

/// A branch, as [`Repo::branches`] lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// Its short name, such as `main` or `origin/main`.
    pub name: String,
    /// The branch it pushes to, such as `origin/main`; empty when none.
    pub upstream: String,
    /// Commits on it that its upstream doesn't have.
    pub ahead: u32,
    /// When its last commit was made, in seconds since the Unix epoch.
    pub date: i64,
}

/// A repository's branches, the most recently committed to first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Branches {
    /// The branch checked out; `None` when `HEAD` is detached.
    pub current: Option<String>,
    /// The local branches.
    pub local: Vec<Branch>,
    /// The remote-tracking branches, such as `origin/main`.
    pub remote: Vec<Branch>,
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

impl Error {
    /// Whether git refused to commit because it doesn't know the author's
    /// name or email.
    ///
    /// Setting `user.name` and `user.email` (see [`Identity`]) fixes it.
    #[must_use]
    pub fn needs_identity(&self) -> bool {
        [
            "Author identity unknown",
            "Please tell me who you are",
            "empty ident name",
        ]
        .iter()
        .any(|said| self.message.contains(said))
    }
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
        // Written while the answer is read: with many paths, git fills its
        // output before it has taken them all in.
        let stdin = child.stdin.take();
        let write = async move {
            if let Some(mut stdin) = stdin {
                // Outside a repository git ends without reading, and the
                // write fails; how git ended says so, below.
                let _ = stdin.write_all(&input).await;
            }
        };
        let (output, ()) = tokio::join!(child.wait_with_output(), write);
        let output = output.map_err(cant_run)?;
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

    /// Lists the repository's local and remote-tracking branches.
    ///
    /// # Errors
    ///
    /// Fails when the folder isn't in a Git repository or `git` can't run.
    pub async fn branches(&self) -> Result<Branches, Error> {
        let out = self
            .git(&[
                "for-each-ref",
                "--sort=-committerdate",
                "--format=%(refname)%00%(refname:short)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(committerdate:unix)",
                "refs/heads",
                "refs/remotes",
            ])
            .await?;
        let current = self.git(&["branch", "--show-current"]).await?;
        let mut branches = Branches {
            current: Some(current.trim().to_owned()).filter(|c| !c.is_empty()),
            ..Branches::default()
        };
        for line in out.lines() {
            let mut fields = line.split('\0');
            let (Some(full), Some(name)) = (fields.next(), fields.next()) else {
                continue;
            };
            let upstream = fields.next().unwrap_or_default().to_owned();
            let ahead = fields
                .next()
                .unwrap_or_default()
                .split(", ")
                .find_map(|part| part.strip_prefix("ahead "))
                .and_then(|n| n.parse().ok())
                .unwrap_or_default();
            let date = fields
                .next()
                .and_then(|d| d.trim().parse().ok())
                .unwrap_or_default();
            let branch = Branch {
                name: name.to_owned(),
                upstream,
                ahead,
                date,
            };
            if full.starts_with("refs/heads/") {
                branches.local.push(branch);
            } else if !full.ends_with("/HEAD") {
                branches.remote.push(branch);
            }
        }
        Ok(branches)
    }

    /// Switches to branch `name`, and says what Git did.
    ///
    /// With `remote`, `name` is a remote-tracking branch such as
    /// `origin/fix`, and a local branch tracking it is made (`fix`). With
    /// `stash`, uncommitted changes, untracked files among them, are
    /// stashed first.
    ///
    /// # Errors
    ///
    /// Fails when the name is empty or starts with `-`, the stash or the
    /// switch fails, or `git` can't run.
    pub async fn switch(&self, name: &str, remote: bool, stash: bool) -> Result<String, Error> {
        if name.is_empty() || name.starts_with('-') {
            return Err(Error {
                message: format!("“{name}” isn't a branch."),
            });
        }
        let mut said = Vec::new();
        if stash {
            let note = format!("Harlo: before switching to {name}");
            let (out, err) = self
                .run(&["stash", "push", "--include-untracked", "-m", &note])
                .await?;
            said.push(format!("{}{}", out.trim(), err.trim()));
        }
        let (out, err) = if remote {
            self.run(&["switch", "--track", name]).await?
        } else {
            self.run(&["switch", name]).await?
        };
        said.push(format!("{}{}", out.trim(), err.trim()));
        Ok(said
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    /// Returns the unified diff of `path` (relative to the repository's
    /// root) against the last commit, staged and unstaged changes
    /// together; empty when it has none.
    ///
    /// # Errors
    ///
    /// Fails when the folder isn't in a Git repository or `git` can't run.
    pub async fn diff_file(&self, path: &str) -> Result<String, Error> {
        let spec = format!(":(top,literal){path}");
        self.git(&["diff", "HEAD", "--no-color", "--no-ext-diff", "--", &spec])
            .await
    }

    /// Returns the repository's root folder.
    ///
    /// # Errors
    ///
    /// Fails when the folder isn't in a Git repository or `git` can't run.
    pub async fn root(&self) -> Result<PathBuf, Error> {
        let out = self.git(&["rev-parse", "--show-toplevel"]).await?;
        Ok(PathBuf::from(out.trim_end_matches(['\n', '\r'])))
    }

    /// Runs `git` with `args` and returns what it printed.
    async fn git(&self, args: &[&str]) -> Result<String, Error> {
        self.run(args).await.map(|(out, _)| out)
    }

    /// Runs `git` with `args` and returns what it printed to its output and
    /// to its errors, which some commands use for what they did.
    async fn run(&self, args: &[&str]) -> Result<(String, String), Error> {
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
            return Ok((
                String::from_utf8_lossy(&output.stdout).into_owned(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
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
                let mut file = changed(rest, RENAMED_FIELDS);
                // A rename's original path follows as its own record.
                file.from = records.next().map(str::to_owned);
                status.files.push(file);
            }
            "u" => status.files.push(FileState {
                code: 'U',
                ..changed(rest, UNMERGED_FIELDS)
            }),
            "?" => status.files.push(FileState {
                path: rest.to_owned(),
                staged: false,
                unstaged: true,
                code: '?',
                from: None,
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
        code: if tree == '.' { index } else { tree },
        from: None,
    }
}

/// Git on this computer: its version, and who it says made commits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Setup {
    /// The version, such as `2.47.1`, or `None` when `git` isn't installed.
    pub version: Option<String>,
    /// The name on commits, empty when none is set.
    pub name: String,
    /// The email on commits, empty when none is set.
    pub email: String,
}

/// Who git says made a commit: the name or the email.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    /// `user.name`.
    Name,
    /// `user.email`.
    Email,
}

impl Identity {
    /// Its key in git's config.
    fn key(self) -> &'static str {
        match self {
            Self::Name => "user.name",
            Self::Email => "user.email",
        }
    }
}

/// Git's settings for the user, in their global config (`~/.gitconfig`,
/// or where `GIT_CONFIG_GLOBAL` points).
///
/// # Examples
///
/// ```no_run
/// # async fn run() {
/// let setup = harlo_core::git::Global::new().setup().await;
/// println!("git {:?}, commits by {}", setup.version, setup.name);
/// # }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Global {
    env: Vec<(String, String)>,
}

impl Global {
    /// The user's own global config.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an environment variable for every `git` command run, such as
    /// `GIT_CONFIG_GLOBAL` for a scratch config.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Reads git's version and the name and email on commits; anything
    /// that can't be read is left empty.
    pub async fn setup(&self) -> Setup {
        let Ok(version) = self.git(&["--version"]).await else {
            return Setup::default();
        };
        // An unset key makes `git config` fail: nothing is set.
        let read = |identity: Identity| async move {
            self.git(&["config", "--global", "--get", identity.key()])
                .await
                .map(|value| value.trim().to_owned())
                .unwrap_or_default()
        };
        let (name, email) = futures_util::join!(read(Identity::Name), read(Identity::Email));
        Setup {
            version: Some(
                version
                    .trim()
                    .trim_start_matches("git version")
                    .trim()
                    .to_owned(),
            ),
            name,
            email,
        }
    }

    /// Sets the name or email on commits; an empty `value` removes it.
    ///
    /// # Errors
    ///
    /// Fails when `git` can't run or can't write its config.
    pub async fn set(&self, identity: Identity, value: &str) -> Result<(), Error> {
        let value = value.trim();
        if value.is_empty() {
            // Already unset is fine: git exits 5 for a key it doesn't have.
            return match self
                .git(&["config", "--global", "--unset-all", identity.key()])
                .await
            {
                Err(e) if e.message.is_empty() => Ok(()),
                other => other.map(drop),
            };
        }
        self.git(&["config", "--global", identity.key(), value])
            .await
            .map(drop)
    }

    /// Runs `git` with `args`, outside any repository, and returns what it
    /// printed.
    async fn git(&self, args: &[&str]) -> Result<String, Error> {
        let output = Command::new("git")
            .args(args)
            .current_dir(std::env::temp_dir())
            .env("GIT_TERMINAL_PROMPT", "0")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| Error {
                message: format!("Couldn't run git: {e}"),
            })?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(Error {
                message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            })
        }
    }
}
