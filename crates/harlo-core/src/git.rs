//! Git through the `git` CLI, so the user's config, credentials, hooks and
//! signing all apply. Status uses porcelain v2; diffs are parsed into hunks
//! so single hunks can be staged, unstaged or reverted with `git apply`.
//!
//! Operations on one repository are serialized: concurrent git commands
//! race for `index.lock`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    /// Repository top level.
    pub root: PathBuf,
    /// Branch name; empty when detached.
    pub branch: String,
    /// Short commit id of HEAD; empty before the first commit.
    pub head: String,
    pub upstream: String,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileChange>,
    /// Number of stash entries.
    pub stashes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Path relative to the repository root.
    pub path: String,
    /// Original path for renames and copies.
    pub orig_path: Option<String>,
    /// Porcelain status letters, `.` for unchanged (e.g. `M`, `A`, `D`, `R`).
    pub index: char,
    pub worktree: char,
    pub untracked: bool,
    pub conflicted: bool,
}

impl FileChange {
    pub fn staged(&self) -> bool {
        !self.untracked && self.index != '.'
    }

    pub fn unstaged(&self) -> bool {
        self.untracked || self.worktree != '.'
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    /// `\ No newline at end of file`
    NoNewline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// `@@ -a,b +c,d @@ context`
    pub header: String,
    pub lines: Vec<DiffLine>,
    /// The hunk exactly as git printed it (header and lines), for patches.
    raw: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    /// `diff --git`, index, mode and `---`/`+++` lines, for patches.
    header: String,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
    pub added: u32,
    pub removed: u32,
}

impl FileDiff {
    /// A patch containing only hunk `index`.
    pub fn hunk_patch(&self, index: usize) -> Option<String> {
        let hunk = self.hunks.get(index)?;
        Some(format!("{}{}", self.header, hunk.raw))
    }
}

fn locks() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(Default::default)
}

fn repo_lock(root: &Path) -> Arc<Mutex<()>> {
    locks().lock().unwrap().entry(root.to_owned()).or_default().clone()
}

/// A git command in a folder: run here, or on the folder's host over SSH
/// for an `ssh://host/…` folder (arguments that are `ssh://` paths become
/// that machine's paths).
struct GitCmd {
    dir: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    input: Option<Vec<u8>>,
}

impl GitCmd {
    fn arg(&mut self, a: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        let a = a.as_ref().to_string_lossy().into_owned();
        self.args.push(if a.starts_with("ssh://") { harlo_harness::on_host(Path::new(&a)) } else { a });
        self
    }
    fn args<I: IntoIterator<Item = S>, S: AsRef<std::ffi::OsStr>>(&mut self, args: I) -> &mut Self {
        for a in args {
            self.arg(a);
        }
        self
    }
    fn env(&mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        self.env.push((key.to_owned(), value.as_ref().to_string_lossy().into_owned()));
        self
    }
    fn input(&mut self, bytes: &[u8]) -> &mut Self {
        self.input = Some(bytes.to_vec());
        self
    }
    fn output(&self) -> Result<Output> {
        const ENV: [(&str, &str); 3] = [("GIT_TERMINAL_PROMPT", "0"), ("GIT_OPTIONAL_LOCKS", "0"), ("LC_ALL", "C")];
        if let Some((name, dir)) = crate::remote::split(&self.dir) {
            let host = crate::remote::host(&name).with_context(|| format!("unknown remote host {name}"))?;
            let q = crate::remote::quote;
            let env: Vec<String> = ENV.iter().map(|(k, v)| format!("{k}={v}")).chain(self.env.iter().map(|(k, v)| format!("{k}={}", q(v)))).collect();
            let script = format!("cd {} && {} git {}", q(&dir), env.join(" "), self.args.iter().map(|a| q(a)).collect::<Vec<_>>().join(" "));
            // Long for push / pull over a slow link.
            return crate::remote::run_with_input(&host, &script, self.input.as_deref(), std::time::Duration::from_secs(600));
        }
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.dir).args(&self.args);
        for (k, v) in ENV {
            cmd.env(k, v);
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        let Some(input) = &self.input else {
            cmd.stdin(Stdio::null());
            return cmd.output().context("running git (is it installed?)");
        };
        use std::io::Write;
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().context("running git (is it installed?)")?;
        child.stdin.take().expect("piped").write_all(input)?;
        Ok(child.wait_with_output()?)
    }
}

fn git(dir: &Path) -> GitCmd {
    GitCmd { dir: dir.to_owned(), args: Vec::new(), env: Vec::new(), input: None }
}

/// A git command's stdout in `dir` (here or on its host), "" on failure.
pub fn output_in(dir: &Path, args: &[&str]) -> String {
    let mut cmd = git(dir);
    cmd.args(args);
    cmd.output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// A path git printed, on the same machine as `dir` (`ssh://` when remote).
fn on_same_host(dir: &Path, printed: &str) -> PathBuf {
    match crate::remote::host_of(dir) {
        Some(host) => crate::remote::join(&host, printed),
        None => PathBuf::from(printed),
    }
}

fn run(cmd: GitCmd) -> Result<Output> {
    cmd.output()
}

fn run_ok(cmd: GitCmd) -> Result<String> {
    let out = run(cmd)?;
    if !out.status.success() {
        // Some failures (e.g. "nothing to commit") are reported on stdout.
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let msg = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
        bail!("{}", if msg.is_empty() { "git failed" } else { msg });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Counts uncommitted changed files for many working directories at once,
/// running `git status` once per repository. Repository roots are cached.
#[derive(Default)]
pub struct ChangeCounter {
    roots: HashMap<PathBuf, PathBuf>,
    /// Folders of repositories (not one themselves): what's in them, and
    /// when that was looked at.
    nested: HashMap<PathBuf, (std::time::Instant, Vec<PathBuf>)>,
}

impl ChangeCounter {
    /// Changed files (as the Changes panel counts them) per directory in
    /// `dirs` that is inside a repository, except repositories in `ignored`
    /// (by top-level path). Others are left out.
    pub fn count(&mut self, dirs: &[PathBuf], ignored: &HashSet<PathBuf>) -> HashMap<PathBuf, usize> {
        let mut per_root: HashMap<PathBuf, Option<usize>> = HashMap::new();
        let mut out = HashMap::new();
        for dir in dirs {
            // A remote folder counts only while its host is connected (a
            // count never waits for a connection).
            let remote = crate::remote::host_of(dir);
            if let Some(name) = &remote {
                if !crate::remote::host(name).is_some_and(|h| crate::remote::connected(&h)) {
                    continue;
                }
            } else if !dir.is_dir() {
                continue;
            }
            // Only found roots are cached: a folder can become a repository.
            let root = match self.roots.get(dir) {
                Some(r) if remote.is_some() || r.is_dir() => r.clone(),
                _ => match repo_root(dir) {
                    Some(r) => {
                        self.roots.insert(dir.clone(), r.clone());
                        r
                    }
                    // A folder of repositories: all of theirs.
                    None => {
                        let fresh = self.nested.get(dir).is_some_and(|(at, _)| at.elapsed() < std::time::Duration::from_secs(120));
                        if !fresh {
                            self.nested.insert(dir.clone(), (std::time::Instant::now(), nested_repos(dir)));
                        }
                        let repos = self.nested[dir].1.clone();
                        if repos.is_empty() {
                            continue;
                        }
                        let n: usize = repos
                            .iter()
                            .filter(|r| !ignored.contains(*r))
                            .filter_map(|r| *per_root.entry(r.clone()).or_insert_with(|| status(r).ok().map(|s| s.files.len())))
                            .sum();
                        out.insert(dir.clone(), n);
                        continue;
                    }
                },
            };
            if ignored.contains(&root) {
                continue;
            }
            let n = *per_root.entry(root.clone()).or_insert_with(|| status(&root).ok().map(|s| s.files.len()));
            if let Some(n) = n {
                out.insert(dir.clone(), n);
            }
        }
        out
    }
}

/// Folders that never hold a repository worth showing (and are slow to
/// walk).
const SKIP_DIRS: [&str; 10] = ["node_modules", "target", ".cache", ".venv", "venv", "__pycache__", "dist", "build", ".git", ".tox"];

/// How deep under a folder to look for repositories, and how many to take.
pub const NESTED_DEPTH: usize = 3;
pub const MAX_NESTED: usize = 60;

/// Whether `dir` may be a folder of repositories: not the file system's
/// top, a home folder or one just under the top (`/`, `/home/me`, `/srv`),
/// which would mean searching and watching far too much.
pub fn may_hold_repos(dir: &Path) -> bool {
    let on_host = crate::remote::split(dir).map(|(_, p)| PathBuf::from(p)).unwrap_or_else(|| dir.to_owned());
    let depth = on_host.components().filter(|c| matches!(c, std::path::Component::Normal(_))).count();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    depth > 2 || (depth == 2 && !on_host.starts_with("/home") && !on_host.starts_with("/Users")) && home.as_deref() != Some(on_host.as_path())
}

/// The git repositories inside `dir` (a folder that isn't one itself, such
/// as a folder of a team's repositories), here or on its host: their top
/// folders, by name, at most `MAX_NESTED`, up to `NESTED_DEPTH` levels down.
/// A repository's own folders (submodules) aren't searched.
pub fn nested_repos(dir: &Path) -> Vec<PathBuf> {
    if !may_hold_repos(dir) {
        return Vec::new();
    }
    let mut found = match crate::remote::split(dir).and_then(|(h, p)| Some((h.clone(), crate::remote::host(&h)?, p))) {
        Some((name, host, path)) => {
            let q = crate::remote::quote;
            let skip = SKIP_DIRS.iter().filter(|d| **d != ".git").map(|d| format!("-name {}", q(d))).collect::<Vec<_>>().join(" -o ");
            // `.git` folders (or files, in worktrees), not looking inside them.
            let script = format!("find {} -maxdepth {} \\( {skip} \\) -prune -o -name .git -print -prune 2>/dev/null | head -n {}", q(&path), NESTED_DEPTH + 1, MAX_NESTED * 4);
            let out = crate::remote::run(&host, &script, std::time::Duration::from_secs(30)).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
            let mut roots: Vec<String> = out.lines().filter_map(|l| l.strip_suffix("/.git")).map(str::to_owned).collect();
            roots.sort();
            // Not one inside another.
            let mut kept: Vec<String> = Vec::new();
            for r in roots {
                if !kept.iter().any(|k| r.starts_with(&format!("{k}/"))) {
                    kept.push(r);
                }
            }
            kept.into_iter().map(|r| crate::remote::join(&name, &r)).collect()
        }
        None => {
            let mut out = Vec::new();
            walk_for_repos(dir, 0, &mut out);
            out
        }
    };
    found.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    found.truncate(MAX_NESTED);
    found
}

fn walk_for_repos(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth >= NESTED_DEPTH || out.len() >= MAX_NESTED {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut subdirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
        .map(|e| e.path())
        .collect();
    subdirs.sort();
    for d in subdirs {
        if d.join(".git").exists() {
            out.push(d);
        } else {
            walk_for_repos(&d, depth + 1, out);
        }
        if out.len() >= MAX_NESTED {
            return;
        }
    }
}

/// The repository containing `dir`, or `None` if it is not in one.
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    let mut cmd = git(dir);
    cmd.args(["rev-parse", "--show-toplevel"]);
    let out = run(cmd).ok()?;
    out.status
        .success()
        .then(|| on_same_host(dir, String::from_utf8_lossy(&out.stdout).trim()))
}

pub fn status(root: &Path) -> Result<GitStatus> {
    let lock = repo_lock(root);
    let _guard = lock.lock().unwrap();
    let mut cmd = git(root);
    cmd.args(["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"]);
    let out = run_ok(cmd)?;
    let mut st = parse_status(&out);
    st.root = root.to_owned();
    let mut count = git(root);
    count.args(["rev-list", "--walk-reflogs", "--count", "refs/stash", "--"]);
    st.stashes = run(count).ok().filter(|o| o.status.success()).and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0);
    Ok(st)
}

fn parse_status(out: &str) -> GitStatus {
    let mut st = GitStatus::default();
    let mut records = out.split('\0').peekable();
    while let Some(rec) = records.next() {
        if rec.is_empty() {
            continue;
        }
        if let Some(h) = rec.strip_prefix("# ") {
            let (key, value) = h.split_once(' ').unwrap_or((h, ""));
            match key {
                "branch.oid" if value != "(initial)" => st.head = value.chars().take(8).collect(),
                "branch.head" if value != "(detached)" => st.branch = value.to_owned(),
                "branch.upstream" => st.upstream = value.to_owned(),
                "branch.ab" => {
                    for part in value.split(' ') {
                        if let Some(n) = part.strip_prefix('+') {
                            st.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            st.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let mut fields = rec.splitn(2, ' ');
        let kind = fields.next().unwrap_or_default();
        let rest = fields.next().unwrap_or_default();
        match kind {
            // 1 XY sub mH mI mW hH hI path
            "1" => {
                let f: Vec<&str> = rest.splitn(8, ' ').collect();
                if f.len() == 8 {
                    st.files.push(change(f[0], f[7], None, false));
                }
            }
            // 2 XY sub mH mI mW hH hI Xscore path \0 origPath
            "2" => {
                let f: Vec<&str> = rest.splitn(9, ' ').collect();
                if f.len() == 9 {
                    let orig = records.next().map(str::to_owned);
                    st.files.push(change(f[0], f[8], orig, false));
                }
            }
            // u XY sub m1 m2 m3 mW h1 h2 h3 path
            "u" => {
                let f: Vec<&str> = rest.splitn(10, ' ').collect();
                if f.len() == 10 {
                    st.files.push(change(f[0], f[9], None, true));
                }
            }
            "?" => st.files.push(FileChange {
                path: rest.to_owned(),
                orig_path: None,
                index: '.',
                worktree: '?',
                untracked: true,
                conflicted: false,
            }),
            _ => {}
        }
    }
    st.files.sort_by(|a, b| a.path.cmp(&b.path));
    st
}

fn change(xy: &str, path: &str, orig_path: Option<String>, conflicted: bool) -> FileChange {
    let mut c = xy.chars();
    FileChange {
        path: path.to_owned(),
        orig_path,
        index: c.next().unwrap_or('.'),
        worktree: c.next().unwrap_or('.'),
        untracked: false,
        conflicted,
    }
}

/// Diff of one file: staged (index vs HEAD) or unstaged (worktree vs
/// index). Untracked files diff against an empty file.
pub fn diff_file(root: &Path, path: &str, staged: bool, untracked: bool) -> Result<FileDiff> {
    diff_with_context(root, path, staged, untracked, 3)
}

/// A file's diff with the whole file as context: every line, in order (for
/// the side-by-side view, `crate::splitdiff`).
pub fn diff_file_full(root: &Path, path: &str, staged: bool, untracked: bool) -> Result<FileDiff> {
    diff_with_context(root, path, staged, untracked, 100_000_000)
}

fn diff_with_context(root: &Path, path: &str, staged: bool, untracked: bool, context: u32) -> Result<FileDiff> {
    let lock = repo_lock(root);
    let _guard = lock.lock().unwrap();
    let mut cmd = git(root);
    cmd.args(["-c", "core.quotePath=false", "diff", "--no-color", "--no-ext-diff"]).arg(format!("-U{context}"));
    if untracked {
        // Exits with 1 when the files differ, which is the normal case.
        cmd.args(["--no-index", "--", "/dev/null", path]);
        let out = run(cmd)?;
        if out.status.code().unwrap_or(2) > 1 {
            bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        return Ok(parse_diff(path, &String::from_utf8_lossy(&out.stdout)));
    }
    if staged {
        cmd.arg("--cached");
    }
    cmd.args(["--", path]);
    let out = run_ok(cmd)?;
    Ok(parse_diff(path, &out))
}

fn parse_diff(path: &str, text: &str) -> FileDiff {
    let mut diff = FileDiff { path: path.to_owned(), ..Default::default() };
    let mut current: Option<Hunk> = None;
    let (mut old_no, mut new_no) = (0u32, 0u32);
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if body.starts_with("@@") {
            if let Some(h) = current.take() {
                diff.hunks.push(h);
            }
            let (o, n) = parse_hunk_header(body);
            old_no = o;
            new_no = n;
            current = Some(Hunk { header: body.to_owned(), lines: Vec::new(), raw: line.to_owned() });
            continue;
        }
        let Some(hunk) = current.as_mut() else {
            if body.starts_with("Binary files") || body.starts_with("GIT binary patch") {
                diff.binary = true;
            }
            diff.header.push_str(line);
            continue;
        };
        hunk.raw.push_str(line);
        let (kind, text) = match body.chars().next() {
            Some('+') => (LineKind::Added, &body[1..]),
            Some('-') => (LineKind::Removed, &body[1..]),
            Some('\\') => (LineKind::NoNewline, body),
            _ => (LineKind::Context, body.get(1..).unwrap_or("")),
        };
        let (o, n) = match kind {
            LineKind::Context => {
                old_no += 1;
                new_no += 1;
                (Some(old_no - 1), Some(new_no - 1))
            }
            LineKind::Added => {
                new_no += 1;
                diff.added += 1;
                (None, Some(new_no - 1))
            }
            LineKind::Removed => {
                old_no += 1;
                diff.removed += 1;
                (Some(old_no - 1), None)
            }
            LineKind::NoNewline => (None, None),
        };
        hunk.lines.push(DiffLine { kind, old_no: o, new_no: n, text: text.to_owned() });
    }
    if let Some(h) = current {
        diff.hunks.push(h);
    }
    diff
}

#[cfg(test)]
pub(crate) fn parse_diff_for_tests(path: &str, text: &str) -> FileDiff {
    parse_diff(path, text)
}

/// Start line numbers from `@@ -a,b +c,d @@`.
fn parse_hunk_header(h: &str) -> (u32, u32) {
    let mut parts = h.split_whitespace().skip(1);
    let num = |s: Option<&str>| {
        s.and_then(|s| s.get(1..))
            .and_then(|s| s.split(',').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(1)
    };
    (num(parts.next()), num(parts.next()))
}

fn apply_patch(root: &Path, patch: &str, args: &[&str]) -> Result<()> {
    let lock = repo_lock(root);
    let _guard = lock.lock().unwrap();
    let mut cmd = git(root);
    cmd.arg("apply").args(args).arg("-").input(patch.as_bytes());
    let out = cmd.output().context("running git apply")?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// Stages one hunk of an unstaged diff.
pub fn stage_hunk(root: &Path, diff: &FileDiff, index: usize) -> Result<()> {
    let patch = diff.hunk_patch(index).context("no such hunk")?;
    apply_patch(root, &patch, &["--cached"])
}

/// Unstages one hunk of a staged diff.
pub fn unstage_hunk(root: &Path, diff: &FileDiff, index: usize) -> Result<()> {
    let patch = diff.hunk_patch(index).context("no such hunk")?;
    apply_patch(root, &patch, &["--cached", "--reverse"])
}

/// Discards one hunk of an unstaged diff from the working tree.
pub fn revert_hunk(root: &Path, diff: &FileDiff, index: usize) -> Result<()> {
    let patch = diff.hunk_patch(index).context("no such hunk")?;
    apply_patch(root, &patch, &["--reverse"])
}

fn locked(root: &Path, args: &[&str]) -> Result<String> {
    let lock = repo_lock(root);
    let _guard = lock.lock().unwrap();
    let mut cmd = git(root);
    cmd.args(args);
    run_ok(cmd)
}

pub fn stage_file(root: &Path, path: &str) -> Result<()> {
    locked(root, &["add", "--", path]).map(drop)
}

pub fn unstage_file(root: &Path, path: &str) -> Result<()> {
    locked(root, &["restore", "--staged", "--", path]).map(drop)
}

/// Discards unstaged changes to a tracked file. Untracked files are not
/// touched here; the caller moves them to the trash.
pub fn revert_file(root: &Path, path: &str) -> Result<()> {
    locked(root, &["restore", "--worktree", "--", path]).map(drop)
}

pub fn stage_all(root: &Path) -> Result<()> {
    locked(root, &["add", "--all"]).map(drop)
}

/// Commits what is staged. Returns git's summary line.
pub fn commit(root: &Path, message: &str) -> Result<String> {
    if message.trim().is_empty() {
        bail!("The commit message is empty.");
    }
    let out = locked(root, &["commit", "--message", message])?;
    Ok(out.lines().next().unwrap_or_default().to_owned())
}

/// Pushes the current branch (setting its upstream on the first push).
pub fn push(root: &Path, set_upstream: bool) -> Result<String> {
    let args: &[&str] = if set_upstream { &["push", "--set-upstream", "origin", "HEAD"] } else { &["push"] };
    let lock = repo_lock(root);
    let _guard = lock.lock().unwrap();
    let mut cmd = git(root);
    cmd.args(args);
    let out = run(cmd)?;
    let text = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    if !out.status.success() {
        bail!("{text}");
    }
    Ok(text.lines().last().unwrap_or("Pushed").to_owned())
}

/// What a pull request from the current branch needs: its remote's address,
/// the remote's default branch, the branch's name there, and its commits'
/// subjects since the default branch (newest first).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RequestBase {
    pub remote_url: String,
    pub base: String,
    pub head: String,
    pub subjects: Vec<String>,
}

pub fn request_base(root: &Path) -> Option<RequestBase> {
    let upstream = output_in(root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
    let (remote, head) = upstream.trim().split_once('/')?;
    let remote_url = output_in(root, &["remote", "get-url", remote]).trim().to_owned();
    if remote_url.is_empty() {
        return None;
    }
    // The remote's HEAD, else a main or master it has.
    let head_ref = output_in(root, &["symbolic-ref", "--short", &format!("refs/remotes/{remote}/HEAD")]);
    let base = match head_ref.trim().split_once('/') {
        Some((_, b)) if !b.is_empty() => b.to_owned(),
        _ => ["main", "master"]
            .into_iter()
            .find(|b| !output_in(root, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{remote}/{b}")]).trim().is_empty())
            .unwrap_or("main")
            .to_owned(),
    };
    let subjects = output_in(root, &["log", "--format=%s", "-n", "30", &format!("{remote}/{base}..HEAD")]).lines().map(str::to_owned).filter(|l| !l.trim().is_empty()).collect();
    Some(RequestBase { remote_url, base, head: head.to_owned(), subjects })
}

pub fn pull(root: &Path) -> Result<String> {
    let out = locked(root, &["pull", "--ff-only"])?;
    Ok(out.lines().last().unwrap_or("Up to date").to_owned())
}

/// Combined diff of what a commit would contain (staged, or everything
/// when nothing is staged), for generating a commit message.
pub fn diff_for_message(root: &Path, max_bytes: usize) -> Result<String> {
    let staged = locked(root, &["diff", "--cached", "--no-color", "--no-ext-diff", "--stat", "-p"])?;
    let mut text = if staged.trim().is_empty() {
        locked(root, &["diff", "--no-color", "--no-ext-diff", "--stat", "-p"])?
    } else {
        staged
    };
    if text.len() > max_bytes {
        let cut = (0..=max_bytes).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
        text.truncate(cut);
        text.push_str("\n[diff truncated]\n");
    }
    Ok(text)
}

// ---- branches and stashes --------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    /// Local name (`main`), or `origin/feature` for a remote-only branch.
    pub name: String,
    pub remote: bool,
    pub upstream: String,
    /// Commits not pushed to the upstream.
    pub ahead: u32,
    /// Unix time of the last commit.
    pub date: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Branches {
    /// Most recently committed first.
    pub local: Vec<BranchInfo>,
    /// Remote branches without a local branch of the same name.
    pub remote: Vec<BranchInfo>,
    /// Empty when detached.
    pub current: String,
}

pub fn branches(root: &Path) -> Result<Branches> {
    let format = "%(refname)%00%(refname:short)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(committerdate:unix)";
    let out = locked(root, &["for-each-ref", "--sort=-committerdate", &format!("--format={format}"), "refs/heads", "refs/remotes"])?;
    let current = locked(root, &["branch", "--show-current"])?.trim().to_owned();
    let mut b = Branches { current, ..Default::default() };
    let mut remote = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split('\0').collect();
        let [full, short, upstream, track, date] = f[..] else { continue };
        let ahead = track.split(", ").find_map(|p| p.strip_prefix("ahead ")).and_then(|n| n.parse().ok()).unwrap_or(0);
        let info = BranchInfo { name: short.to_owned(), remote: full.starts_with("refs/remotes/"), upstream: upstream.to_owned(), ahead, date: date.parse().unwrap_or(0) };
        if !info.remote {
            b.local.push(info);
        } else if !full.ends_with("/HEAD") && short.contains('/') {
            remote.push(info);
        }
    }
    let local: HashSet<String> = b.local.iter().map(|l| l.name.clone()).collect();
    let tracked: HashSet<String> = b.local.iter().map(|l| l.upstream.clone()).collect();
    b.remote = remote
        .into_iter()
        .filter(|r| !tracked.contains(&r.name) && !local.contains(r.name.split_once('/').map_or("", |(_, n)| n)))
        .collect();
    Ok(b)
}

/// Switches to `name`: a local branch, a remote one (`origin/x` creates a
/// tracking `x`), or a new branch from HEAD when `create`. With `stash`,
/// uncommitted changes (untracked files too) are stashed first.
pub fn switch_branch(root: &Path, name: &str, create: bool, remote: bool, stash: bool) -> Result<String> {
    if create && !valid_branch_name(root, name) {
        bail!("\"{name}\" is not a valid branch name.");
    }
    let target = if remote { name.split_once('/').map_or(name, |(_, n)| n) } else { name };
    if stash {
        locked(root, &["stash", "push", "--include-untracked", "--message", &format!("harlo: before switching to {target}")])?;
    }
    let args: Vec<&str> = if create {
        vec!["switch", "--create", name]
    } else if remote {
        vec!["switch", "--track", name]
    } else {
        vec!["switch", name]
    };
    locked(root, &args)?;
    Ok(if stash { format!("Switched to {target}; your changes are in the stash.") } else { format!("Switched to {target}.") })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    /// `n` in `stash@{n}`.
    pub index: usize,
    /// The stash's message, without git's `On <branch>:` prefix.
    pub message: String,
    pub branch: String,
    pub date: i64,
}

pub fn stashes(root: &Path) -> Result<Vec<StashEntry>> {
    let out = locked(root, &["stash", "list", "--format=%gd%x00%gs%x00%ct"])?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\0').collect();
            let [refname, subject, date] = f[..] else { return None };
            let index = refname.strip_prefix("stash@{")?.strip_suffix('}')?.parse().ok()?;
            // "On main: message" or "WIP on main: 1234abc subject"
            let rest = subject.strip_prefix("WIP on ").or_else(|| subject.strip_prefix("On ")).unwrap_or(subject);
            let (branch, message) = rest.split_once(": ").unwrap_or(("", rest));
            Some(StashEntry { index, message: message.to_owned(), branch: branch.to_owned(), date: date.parse().unwrap_or(0) })
        })
        .collect())
}

/// Stashes all uncommitted changes, untracked files included.
pub fn stash_push(root: &Path, message: &str) -> Result<String> {
    let mut args = vec!["stash", "push", "--include-untracked"];
    if !message.trim().is_empty() {
        args.extend(["--message", message.trim()]);
    }
    let out = locked(root, &args)?;
    if out.contains("No local changes to save") {
        bail!("No local changes to save.");
    }
    Ok("Changes stashed.".into())
}

/// Applies `stash@{index}`; `pop` also drops it when it applied cleanly.
pub fn stash_apply(root: &Path, index: usize, pop: bool) -> Result<String> {
    let entry = format!("stash@{{{index}}}");
    locked(root, &["stash", if pop { "pop" } else { "apply" }, &entry])?;
    Ok(if pop { format!("Restored and dropped {entry}.") } else { format!("Applied {entry}.") })
}

pub fn stash_drop(root: &Path, index: usize) -> Result<String> {
    let entry = format!("stash@{{{index}}}");
    locked(root, &["stash", "drop", &entry])?;
    Ok(format!("Dropped {entry}."))
}

// ---- worktrees -------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// Short branch name; empty when detached.
    pub branch: String,
    pub head: String,
    /// The repository's main checkout.
    pub is_main: bool,
    pub locked: bool,
    /// Its folder is gone; `git worktree prune` would drop it.
    pub prunable: bool,
}

/// All worktrees of the repository at `root`, main checkout first.
pub fn list_worktrees(root: &Path) -> Result<Vec<Worktree>> {
    let out = locked(root, &["worktree", "list", "--porcelain"])?;
    let mut list = parse_worktrees(&out);
    for w in &mut list {
        w.path = on_same_host(root, &w.path.to_string_lossy());
    }
    Ok(list)
}

fn parse_worktrees(out: &str) -> Vec<Worktree> {
    let mut list = Vec::new();
    for block in out.split("\n\n") {
        let mut wt = Worktree {
            path: PathBuf::new(),
            branch: String::new(),
            head: String::new(),
            is_main: list.is_empty(),
            locked: false,
            prunable: false,
        };
        for line in block.lines() {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "worktree" => wt.path = PathBuf::from(value),
                "HEAD" => wt.head = value.chars().take(8).collect(),
                "branch" => wt.branch = value.strip_prefix("refs/heads/").unwrap_or(value).to_owned(),
                "locked" => wt.locked = true,
                "prunable" => wt.prunable = true,
                _ => {}
            }
        }
        if !wt.path.as_os_str().is_empty() {
            list.push(wt);
        }
    }
    list
}

/// Local branches, most recently committed first, and the current branch.
pub fn local_branches(root: &Path) -> Result<(Vec<String>, String)> {
    let out = locked(root, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/heads"])?;
    let current = locked(root, &["branch", "--show-current"])?.trim().to_owned();
    Ok((out.lines().map(str::to_owned).collect(), current))
}

/// Checks a branch name the way git does (`git check-ref-format --branch`).
pub fn valid_branch_name(root: &Path, name: &str) -> bool {
    let mut cmd = git(root);
    cmd.args(["check-ref-format", "--branch", name]);
    run(cmd).is_ok_and(|o| o.status.success())
}

/// Adds a worktree at `path` on `branch`. A missing branch is created from
/// `base`; an existing one is checked out.
pub fn create_worktree(root: &Path, path: &Path, branch: &str, base: &str) -> Result<()> {
    if !valid_branch_name(root, branch) {
        bail!("\"{branch}\" is not a valid branch name.");
    }
    if exists(path) {
        bail!("{} already exists.", crate::remote::display(path));
    }
    if let Some(parent) = path.parent() {
        make_dirs(parent)?;
    }
    let (branches, _) = local_branches(root)?;
    // (As the repository's machine sees it.)
    let path_s = harlo_harness::on_host(path);
    if branches.iter().any(|b| b == branch) {
        locked(root, &["worktree", "add", &path_s, branch])?;
    } else {
        locked(root, &["worktree", "add", "-b", branch, &path_s, base])?;
    }
    Ok(())
}

// ---- turn snapshots: undoing what an agent did in one turn ---------------

/// Where the snapshots taken before each turn live (`<prefix><unix ms>`):
/// hidden commits of the whole working tree, untracked files included
/// (ignored ones left out), off every branch.
pub const TURN_REFS: &str = "refs/harlo/turns/";
/// Where the state just before an undo lives (so an undo can be undone).
pub const UNDO_REFS: &str = "refs/harlo/undo/";
/// How many turn snapshots a repository keeps (the newest), and for how long.
const TURNS_KEPT: usize = 200;
const TURNS_DAYS: i64 = 30;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Before a turn starts: the repository's files, as a snapshot to undo the
/// turn to. None outside a git repository (or if git fails).
pub fn snapshot_turn(dir: &Path) -> Option<String> {
    let Some(root) = repo_root(dir) else { return snapshot_folder_turn(dir) };
    // Not a home folder kept in git (dotfiles): far too much to snapshot.
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if !crate::remote::is_remote(&root) && (root == Path::new("/") || home.is_some_and(|h| h == root)) {
        return None;
    }
    let id = snapshot_worktree(&root, &format!("{TURN_REFS}{}", now_ms()), "harlo: before a turn").ok()?;
    prune_turns(&root);
    Some(id)
}

/// A folder of repositories: each one's files snapshotted under the same
/// time (in parallel); the turn is `@<ms>` (see `folder_undo_plan`).
fn snapshot_folder_turn(dir: &Path) -> Option<String> {
    let repos = nested_repos(dir);
    if repos.is_empty() {
        return None;
    }
    let ms = now_ms();
    let refname = format!("{TURN_REFS}{ms}");
    let done = std::thread::scope(|scope| {
        let handles: Vec<_> = repos
            .iter()
            .map(|root| {
                let refname = &refname;
                scope.spawn(move || {
                    let ok = snapshot_worktree(root, refname, "harlo: before a turn").is_ok();
                    prune_turns(root);
                    ok
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join()).filter(|r| matches!(r, Ok(true))).count()
    });
    (done > 0).then(|| format!("@{ms}"))
}

/// The repositories a session's folder is about: the one it's in, or (a
/// folder of repositories) those inside it.
pub fn folder_repos(dir: &Path) -> Vec<PathBuf> {
    repo_root(dir).map(|r| vec![r]).unwrap_or_else(|| nested_repos(dir))
}

/// A folder's turn snapshots, oldest first: its repository's, or (a folder
/// of repositories) each time any of them was snapshotted, as `@<ms>`.
pub fn folder_turn_snapshots(dir: &Path) -> Vec<(i64, String)> {
    if let Some(root) = repo_root(dir) {
        return turn_snapshots(&root);
    }
    let mut times: Vec<i64> = nested_repos(dir).iter().flat_map(|r| turn_snapshots(r).into_iter().map(|(t, _)| t)).collect();
    times.sort();
    times.dedup();
    times.into_iter().map(|t| (t, format!("@{t}"))).collect()
}

/// For a folder of repositories: each repository with its snapshot from
/// the turn `@<ms>`, and its path in the folder.
fn folder_turn(dir: &Path, ms: &str) -> Vec<(PathBuf, String, String)> {
    let Ok(ms) = ms.parse::<i64>() else { return Vec::new() };
    nested_repos(dir)
        .into_iter()
        .filter_map(|r| {
            let snap = turn_snapshots(&r).into_iter().find(|(t, _)| *t == ms)?.1;
            let rel = r.strip_prefix(dir).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            Some((r, snap, rel))
        })
        .collect()
}

/// Several repositories' plans as one: paths with their repository's place
/// in the folder.
fn merge_plans(parts: Vec<(String, UndoPlan)>) -> UndoPlan {
    let mut plan = UndoPlan::default();
    for (rel, p) in parts {
        let at = |f: String| if rel.is_empty() { f } else { format!("{rel}/{f}") };
        plan.restore.extend(p.restore.into_iter().map(at));
        plan.remove.extend(p.remove.into_iter().map(at));
        plan.head_moved |= p.head_moved;
    }
    plan
}

/// `undo_plan` for a session's folder (one repository, or several: `@<ms>`).
pub fn folder_undo_plan(dir: &Path, snapshot: &str) -> Result<UndoPlan> {
    match snapshot.strip_prefix('@') {
        Some(ms) => folder_turn(dir, ms).into_iter().map(|(r, snap, rel)| Ok((rel, undo_plan(&r, &snap)?))).collect::<Result<Vec<_>>>().map(merge_plans),
        None => undo_plan(&repo_root(dir).context("not a git repository")?, snapshot),
    }
}

/// `undo_turn` for a session's folder (one repository, or several).
pub fn folder_undo_turn(dir: &Path, snapshot: &str) -> Result<UndoPlan> {
    match snapshot.strip_prefix('@') {
        Some(ms) => folder_turn(dir, ms).into_iter().map(|(r, snap, rel)| Ok((rel, undo_turn(&r, &snap)?))).collect::<Result<Vec<_>>>().map(merge_plans),
        None => undo_turn(&repo_root(dir).context("not a git repository")?, snapshot),
    }
}

/// A repository's turn snapshots, oldest first: `(unix ms, commit)`.
pub fn turn_snapshots(root: &Path) -> Vec<(i64, String)> {
    let mut out: Vec<(i64, String)> = output_in(root, &["for-each-ref", "--format=%(refname) %(objectname)", TURN_REFS])
        .lines()
        .filter_map(|l| {
            let (name, id) = l.split_once(' ')?;
            Some((name.strip_prefix(TURN_REFS)?.parse().ok()?, id.to_owned()))
        })
        .collect();
    out.sort();
    out
}

/// Old snapshots go (the newest `TURNS_KEPT`, a month at most).
fn prune_turns(root: &Path) {
    let all = turn_snapshots(root);
    let cutoff = now_ms() - TURNS_DAYS * 86_400_000;
    let keep_from = all.len().saturating_sub(TURNS_KEPT);
    let gone: String = all.iter().enumerate().filter(|(i, (ms, _))| *i < keep_from || *ms < cutoff).map(|(_, (ms, _))| format!("delete {TURN_REFS}{ms}\n")).collect();
    if !gone.is_empty() {
        let mut cmd = git(root);
        cmd.args(["update-ref", "--stdin"]).input(gone.as_bytes());
        let _ = run(cmd);
    }
}

/// Which snapshot each prompt of a session started from: for each prompt's
/// time (unix ms, oldest first), the newest snapshot taken in the two
/// minutes before it (and after the prompt before it); None if none was.
pub fn match_turns(snapshots: &[(i64, String)], prompts: &[i64]) -> Vec<Option<String>> {
    let mut previous = i64::MIN;
    prompts
        .iter()
        .map(|&t| {
            // (A prompt with no time can't be matched.)
            let found = (t > 0).then(|| snapshots.iter().rev().find(|(ms, _)| *ms <= t + 10_000 && *ms >= t - 120_000 && *ms > previous).map(|(_, id)| id.clone())).flatten();
            if t > 0 {
                previous = t;
            }
            found
        })
        .collect()
}

/// What undoing to a snapshot does: files to bring back (changed or
/// deleted since), files to remove (new since), and whether commits were
/// made since (they stay; undo puts back files).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct UndoPlan {
    pub restore: Vec<String>,
    pub remove: Vec<String>,
    pub head_moved: bool,
}

/// The working tree as a tree object (a scratch index; the real one is
/// left alone), and HEAD.
fn worktree_tree(dir: &Path) -> Result<(String, String)> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!("harlo-tree-{}-{n}-{}.index", std::process::id(), now_ms());
    let index = if crate::remote::is_remote(dir) { PathBuf::from("/tmp").join(&name) } else { std::env::temp_dir().join(&name) };
    let with_index = |args: &[&str]| {
        let mut cmd = git(dir);
        cmd.env("GIT_INDEX_FILE", &index).args(args);
        run_ok(cmd)
    };
    let head = output_in(dir, &["rev-parse", "--verify", "-q", "HEAD"]).trim().to_owned();
    let result = (|| {
        // Seeded from the real index (its file stamps): only changed files
        // are read again. Else from HEAD.
        if !seed_index(dir, &index) && !head.is_empty() {
            with_index(&["read-tree", "HEAD"])?;
        }
        with_index(&["add", "-A"])?;
        Ok::<_, anyhow::Error>(with_index(&["write-tree"])?.trim().to_owned())
    })();
    match crate::remote::host_of(dir).and_then(|h| crate::remote::host(&h)) {
        Some(host) => {
            let _ = crate::remote::run(&host, &format!("rm -f {}", crate::remote::quote(&index.to_string_lossy())), std::time::Duration::from_secs(20));
        }
        None => {
            let _ = std::fs::remove_file(&index);
        }
    }
    Ok((result?, head))
}

pub fn undo_plan(root: &Path, snapshot: &str) -> Result<UndoPlan> {
    let (now, head) = worktree_tree(root)?;
    let mut cmd = git(root);
    cmd.args(["diff", "--name-status", "--no-renames", "-z", snapshot, &now]);
    let out = run_ok(cmd)?;
    let mut plan = UndoPlan::default();
    let mut parts = out.split('\0').filter(|p| !p.is_empty());
    while let (Some(status), Some(path)) = (parts.next(), parts.next()) {
        match status.chars().next() {
            Some('A') => plan.remove.push(path.to_owned()),
            Some(_) => plan.restore.push(path.to_owned()),
            None => {}
        }
    }
    let parent = output_in(root, &["rev-parse", "--verify", "-q", &format!("{snapshot}^")]).trim().to_owned();
    plan.head_moved = !parent.is_empty() && parent != head;
    Ok(plan)
}

/// Puts the files back as they were in a snapshot (the index and commits
/// are left alone). The state before goes to `UNDO_REFS` first, so this can
/// be undone too. What it did.
pub fn undo_turn(root: &Path, snapshot: &str) -> Result<UndoPlan> {
    let lock = repo_lock(root);
    let _guard = lock.lock().unwrap();
    let plan = undo_plan(root, snapshot)?;
    if plan.restore.is_empty() && plan.remove.is_empty() {
        return Ok(plan);
    }
    snapshot_worktree(root, &format!("{UNDO_REFS}{}", now_ms()), "harlo: before an undo")?;
    if !plan.restore.is_empty() {
        let mut list = plan.restore.join("\0").into_bytes();
        list.push(0);
        let mut cmd = git(root);
        cmd.args(["restore", &format!("--source={snapshot}"), "--worktree", "--pathspec-from-file=-", "--pathspec-file-nul"]).input(&list);
        run_ok(cmd)?;
    }
    // (New since the snapshot: kept in the one just taken, so they go.)
    for path in &plan.remove {
        let full = root.join(path);
        match crate::remote::split(&full) {
            Some((name, f)) => {
                let host = crate::remote::host(&name).with_context(|| format!("unknown remote host {name}"))?;
                crate::remote::run(&host, &format!("rm -f -- {}", crate::remote::quote(&f)), std::time::Duration::from_secs(20))?;
            }
            None => std::fs::remove_file(&full).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).with_context(|| format!("removing {path}"))?,
        }
    }
    Ok(plan)
}

/// Saves everything in a worktree (tracked, staged, unstaged and untracked
/// but not ignored files) as a commit under `refname`, leaving its real
/// index, HEAD and branch untouched. Returns the commit id.
pub fn snapshot_worktree(dir: &Path, refname: &str, message: &str) -> Result<String> {
    // The scratch index lives where the repository is (one each: several
    // repositories are snapshotted at once).
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!("harlo-snapshot-{}-{n}-{}.index", std::process::id(), refname.replace('/', "_"));
    let index = if crate::remote::is_remote(dir) { PathBuf::from("/tmp").join(&name) } else { std::env::temp_dir().join(&name) };
    let with_index = |args: &[&str]| {
        let mut cmd = git(dir);
        cmd.env("GIT_INDEX_FILE", &index).args(args);
        run_ok(cmd)
    };
    let head = run_ok({
        let mut c = git(dir);
        c.args(["rev-parse", "--verify", "-q", "HEAD"]);
        c
    })
    .map(|h| h.trim().to_owned())
    .unwrap_or_default();
    let result = (|| {
        if !head.is_empty() {
            with_index(&["read-tree", "HEAD"])?;
        }
        with_index(&["add", "-A"])?;
        let tree = with_index(&["write-tree"])?.trim().to_owned();
        let mut commit = git(dir);
        commit.args(["commit-tree", &tree, "-m", message]);
        if !head.is_empty() {
            commit.args(["-p", &head]);
        }
        commit
            .env("GIT_AUTHOR_NAME", "harlo")
            .env("GIT_AUTHOR_EMAIL", "harlo@localhost")
            .env("GIT_COMMITTER_NAME", "harlo")
            .env("GIT_COMMITTER_EMAIL", "harlo@localhost");
        let id = run_ok(commit)?.trim().to_owned();
        let mut update = git(dir);
        update.args(["update-ref", refname, &id]);
        run_ok(update)?;
        Ok(id)
    })();
    match crate::remote::host_of(dir).and_then(|h| crate::remote::host(&h)) {
        Some(host) => {
            let _ = crate::remote::run(&host, &format!("rm -f {}", crate::remote::quote(&index.to_string_lossy())), std::time::Duration::from_secs(20));
        }
        None => {
            let _ = std::fs::remove_file(&index);
        }
    }
    result
}

/// Copies the repository's index to `to` (on its host when it's remote).
fn seed_index(dir: &Path, to: &Path) -> bool {
    let real = output_in(dir, &["rev-parse", "--path-format=absolute", "--git-path", "index"]).trim().to_owned();
    if real.is_empty() {
        return false;
    }
    match crate::remote::host_of(dir).and_then(|h| crate::remote::host(&h)) {
        Some(host) => {
            let q = crate::remote::quote;
            crate::remote::run(&host, &format!("test -f {0} && cp {0} {1}", q(&real), q(&to.to_string_lossy())), std::time::Duration::from_secs(20)).is_ok_and(|o| o.status.success())
        }
        None => Path::new(&real).is_file() && std::fs::copy(&real, to).is_ok(),
    }
}

/// `path` exists (on its host, over SSH, when it's remote).
fn exists(path: &Path) -> bool {
    match crate::remote::split(path).and_then(|(h, p)| Some((crate::remote::host(&h)?, p))) {
        Some((host, p)) => crate::remote::run(&host, &format!("test -e {}", crate::remote::quote(&p)), std::time::Duration::from_secs(20)).is_ok_and(|o| o.status.success()),
        None => path.exists(),
    }
}

/// Creates a folder and its parents (on its host when it's remote).
fn make_dirs(dir: &Path) -> Result<()> {
    match crate::remote::split(dir).and_then(|(h, p)| Some((crate::remote::host(&h)?, p))) {
        Some((host, p)) => crate::remote::run_ok(&host, &format!("mkdir -p {}", crate::remote::quote(&p)), std::time::Duration::from_secs(20)).map(drop),
        None => std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display())),
    }
}

/// Removes a worktree folder (refusing with uncommitted changes unless
/// `force`) and prunes stale entries.
pub fn remove_worktree(root: &Path, path: &Path, force: bool) -> Result<()> {
    let path_s = harlo_harness::on_host(path);
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path_s);
    if exists(path) {
        locked(root, &args)?;
    }
    locked(root, &["worktree", "prune"])?;
    Ok(())
}

/// Deletes a local branch; without `force`, only if it is merged.
pub fn delete_branch(root: &Path, branch: &str, force: bool) -> Result<()> {
    locked(root, &["branch", if force { "-D" } else { "-d" }, branch]).map(drop)
}

/// What finishing a worktree would lose or carry over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeSummary {
    /// Files with uncommitted changes (including untracked).
    pub dirty: usize,
    /// Commits on the worktree's branch that `base` lacks.
    pub ahead: u32,
    /// Commits on `base` that the branch lacks.
    pub behind: u32,
}

pub fn worktree_summary(path: &Path, base: &str) -> Result<WorktreeSummary> {
    let st = status(path)?;
    let mut summary = WorktreeSummary { dirty: st.files.len(), ..Default::default() };
    if !base.is_empty() {
        let out = locked(path, &["rev-list", "--left-right", "--count", &format!("{base}...HEAD")])?;
        let mut nums = out.split_whitespace().map(|n| n.parse().unwrap_or(0));
        summary.behind = nums.next().unwrap_or(0);
        summary.ahead = nums.next().unwrap_or(0);
    }
    Ok(summary)
}

/// Merges `branch` into the branch checked out in `main` (the repository's
/// main checkout), which must be clean. On conflicts the merge is aborted
/// and nothing changes.
pub fn merge_branch(main: &Path, branch: &str) -> Result<String> {
    let st = status(main)?;
    if !st.files.is_empty() {
        bail!("The main checkout has uncommitted changes; commit or stash them first.");
    }
    let lock = repo_lock(main);
    let _guard = lock.lock().unwrap();
    let mut cmd = git(main);
    cmd.args(["merge", "--no-ff", "--no-edit", branch]);
    let out = run(cmd)?;
    if out.status.success() {
        return Ok(format!("Merged {branch} into {}", if st.branch.is_empty() { "HEAD" } else { &st.branch }));
    }
    let mut abort = git(main);
    abort.args(["merge", "--abort"]);
    let _ = run(abort);
    let detail = String::from_utf8_lossy(&out.stdout);
    bail!("Merging {branch} hit conflicts, so it was aborted.\n{}", detail.trim());
}

/// A folder name for a branch: `feature/login fix` → `feature-login-fix`.
pub fn branch_slug(branch: &str) -> String {
    let slug: String = branch
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '_' { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() { "worktree".into() } else { slug }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_turn_is_undone_to_its_snapshot() {
        let root = repo("turns");
        std::fs::write(root.join("c.txt"), "keep me").unwrap();
        sh(&root, &["add", "."]);
        sh(&root, &["commit", "-q", "-m", "c"]);
        // Yours, before the turn (uncommitted): it stays.
        std::fs::write(root.join("a.txt"), "two").unwrap();
        std::fs::write(root.join("notes.md"), "mine").unwrap();
        let snap = snapshot_turn(&root).expect("a snapshot");
        assert_eq!(turn_snapshots(&root).len(), 1);
        // The agent's turn: an edit, a new file, a deleted one.
        std::fs::write(root.join("a.txt"), "three").unwrap();
        std::fs::write(root.join("b.txt"), "new").unwrap();
        std::fs::remove_file(root.join("c.txt")).unwrap();
        let plan = undo_plan(&root, &snap).unwrap();
        let mut restore = plan.restore.clone();
        restore.sort();
        assert_eq!(restore, ["a.txt", "c.txt"]);
        assert_eq!(plan.remove, ["b.txt"]);
        assert!(!plan.head_moved);
        undo_turn(&root, &snap).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "two");
        assert_eq!(std::fs::read_to_string(root.join("c.txt")).unwrap(), "keep me");
        assert_eq!(std::fs::read_to_string(root.join("notes.md")).unwrap(), "mine");
        assert!(!root.join("b.txt").exists());
        // The state before the undo is kept (an undo can be undone).
        assert_eq!(output_in(&root, &["for-each-ref", "--format=%(refname)", UNDO_REFS]).lines().count(), 1);
        assert!(undo_plan(&root, &snap).unwrap().restore.is_empty());
        // A commit since: noted (it stays).
        std::fs::write(root.join("a.txt"), "four").unwrap();
        sh(&root, &["commit", "-qam", "agent"]);
        assert!(undo_plan(&root, &snap).unwrap().head_moved);
        let _ = std::fs::remove_dir_all(&root);
        // Outside git: no snapshot.
        let plain = std::env::temp_dir().join(format!("harlo-plain-{}", std::process::id()));
        std::fs::create_dir_all(&plain).unwrap();
        assert!(snapshot_turn(&plain).is_none());
        let _ = std::fs::remove_dir_all(&plain);
    }

    #[test]
    fn a_turn_is_undone_across_a_folder_of_repositories() {
        let top = std::env::temp_dir().join(format!("harlo-multi-undo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        std::fs::create_dir_all(top.join("group")).unwrap();
        std::fs::rename(repo("mu-a"), top.join("a")).unwrap();
        std::fs::rename(repo("mu-b"), top.join("group/b")).unwrap();
        // Staged work in a: the snapshot keeps it (the real index seeds it).
        std::fs::write(top.join("a/staged.txt"), "mine").unwrap();
        sh(&top.join("a"), &["add", "staged.txt"]);
        let snap = snapshot_turn(&top).expect("a snapshot of the folder");
        assert!(snap.starts_with('@'));
        assert_eq!(folder_turn_snapshots(&top).len(), 1);
        assert_eq!(turn_snapshots(&top.join("group/b")).len(), 1, "every repository has it");
        // The agent's turn, in both.
        std::fs::write(top.join("a/a.txt"), "changed").unwrap();
        std::fs::write(top.join("group/b/new.txt"), "new").unwrap();
        let plan = folder_undo_plan(&top, &snap).unwrap();
        assert_eq!((plan.restore.clone(), plan.remove.clone()), (vec!["a/a.txt".to_owned()], vec!["group/b/new.txt".to_owned()]));
        folder_undo_turn(&top, &snap).unwrap();
        assert!(std::fs::read_to_string(top.join("a/a.txt")).unwrap().starts_with("line 1"));
        assert!(!top.join("group/b/new.txt").exists());
        assert_eq!(std::fs::read_to_string(top.join("a/staged.txt")).unwrap(), "mine", "work from before the turn stays");
        assert!(folder_undo_plan(&top, &snap).unwrap().restore.is_empty());
        let _ = std::fs::remove_dir_all(&top);
    }

    #[test]
    fn prompts_match_the_snapshot_just_before_them() {
        let snaps = vec![(1_000, "a".to_owned()), (60_000, "b".to_owned()), (61_000, "c".to_owned()), (500_000, "d".to_owned())];
        // A prompt at 62 s: the newest before it (c); at 400 s: none in the two minutes before;
        // one at 0 (unknown time): none.
        assert_eq!(match_turns(&snaps, &[2_000, 62_000, 400_000, 501_000]), [Some("a".into()), Some("c".into()), None, Some("d".into())]);
        assert_eq!(match_turns(&snaps, &[0]), [None]);
    }

    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("harlo-git-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        sh(&dir, &["init", "-q", "-b", "main"]);
        sh(&dir, &["config", "user.name", "t"]);
        sh(&dir, &["config", "user.email", "t@t"]);
        sh(&dir, &["config", "commit.gpgsign", "false"]);
        let lines: String = (1..=30).map(|i| format!("line {i}\n")).collect();
        std::fs::write(dir.join("a.txt"), lines).unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn finds_the_repositories_in_a_folder() {
        let top = std::env::temp_dir().join(format!("harlo-nested-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        std::fs::create_dir_all(top.join("group/deep")).unwrap();
        std::fs::create_dir_all(top.join("node_modules/pkg/.git")).unwrap();
        std::fs::create_dir_all(top.join("plain")).unwrap();
        for (name, at) in [("pipeline", top.join("pipeline")), ("tools", top.join("group/tools")), ("lib", top.join("group/deep/lib")), ("far", top.join("a/b/c/far"))] {
            std::fs::create_dir_all(&at).unwrap();
            let r = repo(&format!("nested-{name}"));
            std::fs::rename(&r, &at).unwrap();
        }
        // A repository's own sub-repository isn't listed.
        let inner = repo("nested-inner");
        std::fs::rename(&inner, top.join("pipeline/vendor")).unwrap();
        let found: Vec<String> = nested_repos(&top).iter().map(|p| p.strip_prefix(&top).unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(found, ["group/deep/lib", "group/tools", "pipeline"], "three levels down, heavy folders and sub-repositories left out");
        for top_level in ["/", "/home/someone", "/Users/someone", "/srv", "/root"] {
            assert!(!may_hold_repos(Path::new(top_level)), "{top_level}");
        }
        assert!(may_hold_repos(Path::new("/home/me/Projects")) && may_hold_repos(Path::new("/srv/team/repos")) && may_hold_repos(Path::new("/opt/repos")));
        assert!(!may_hold_repos(Path::new("ssh://box/home/me")) && may_hold_repos(Path::new("ssh://box/home/me/team-repos")));
        let _ = std::fs::remove_dir_all(&top);
    }

    #[test]
    fn lists_and_switches_branches() {
        let dir = repo("branches");
        sh(&dir, &["branch", "feature"]);
        let b = branches(&dir).unwrap();
        assert_eq!(b.current, "main");
        let names: Vec<&str> = b.local.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"main") && names.contains(&"feature"), "{names:?}");
        assert!(b.remote.is_empty());

        // Dirty tree: stash, switch, and the change waits in the stash.
        std::fs::write(dir.join("a.txt"), "edited\n").unwrap();
        std::fs::write(dir.join("new.txt"), "untracked\n").unwrap();
        switch_branch(&dir, "feature", false, false, true).unwrap();
        assert_eq!(branches(&dir).unwrap().current, "feature");
        assert!(status(&dir).unwrap().files.is_empty());
        let st = stashes(&dir).unwrap();
        assert_eq!(st.len(), 1);
        assert_eq!(st[0].branch, "main");
        assert_eq!(st[0].message, "harlo: before switching to feature");
        assert_eq!(status(&dir).unwrap().stashes, 1);

        switch_branch(&dir, "topic/new", true, false, false).unwrap();
        assert_eq!(branches(&dir).unwrap().current, "topic/new");
        assert!(switch_branch(&dir, "bad..name", true, false, false).is_err());
    }

    #[test]
    fn switches_to_a_remote_branch() {
        let origin = repo("remote-origin");
        sh(&origin, &["branch", "feature"]);
        let clone = std::env::temp_dir().join(format!("harlo-git-remote-clone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&clone);
        sh(&std::env::temp_dir(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
        let b = branches(&clone).unwrap();
        let remote: Vec<&str> = b.remote.iter().map(|r| r.name.as_str()).collect();
        // origin/main is tracked by main; origin/HEAD is skipped.
        assert_eq!(remote, ["origin/feature"]);
        switch_branch(&clone, "origin/feature", false, true, false).unwrap();
        let b = branches(&clone).unwrap();
        assert_eq!(b.current, "feature");
        assert!(b.remote.is_empty());
        assert_eq!(b.local.iter().find(|l| l.name == "feature").unwrap().upstream, "origin/feature");
    }

    #[test]
    fn stashes_apply_pop_and_drop() {
        let dir = repo("stash");
        assert!(stash_push(&dir, "nothing").is_err());
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        stash_push(&dir, "first").unwrap();
        std::fs::write(dir.join("b.txt"), "two\n").unwrap();
        stash_push(&dir, "").unwrap();
        let list = stashes(&dir).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].message, "first");
        assert_eq!(list[1].branch, "main");
        // Newest first: stash@{0} is the untitled one (git's "WIP on" form).
        assert!(list[0].message.contains("init"), "{:?}", list[0]);

        stash_apply(&dir, 1, false).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "one\n");
        assert_eq!(stashes(&dir).unwrap().len(), 2);
        sh(&dir, &["checkout", "--", "a.txt"]);
        stash_apply(&dir, 0, true).unwrap();
        assert!(dir.join("b.txt").exists());
        assert_eq!(stashes(&dir).unwrap().len(), 1);
        stash_drop(&dir, 0).unwrap();
        assert!(stashes(&dir).unwrap().is_empty());
    }

    #[test]
    fn snapshots_a_worktree_without_touching_it() {
        let dir = repo("snapshot");
        std::fs::write(dir.join("a.txt"), "edited\n").unwrap();
        std::fs::write(dir.join("new.txt"), "untracked\n").unwrap();
        let before = status(&dir).unwrap().files.len();
        let id = snapshot_worktree(&dir, "refs/harlo/runs/test/a", "harlo run snapshot").unwrap();
        assert_eq!(status(&dir).unwrap().files.len(), before, "the worktree is unchanged");
        let show = |spec: &str| {
            let out = Command::new("git").current_dir(&dir).args(["show", spec]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        assert_eq!(show(&format!("{id}:new.txt")), "untracked\n");
        assert_eq!(show("refs/harlo/runs/test/a:a.txt"), "edited\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn counts_changes_per_directory() {
        let dir = repo("count");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
        std::fs::write(dir.join("sub/new.txt"), "x").unwrap();
        let outside = std::env::temp_dir().join(format!("harlo-git-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        let mut counter = ChangeCounter::default();
        let dirs = [dir.clone(), dir.join("sub"), outside.clone(), dir.join("missing")];
        let none = HashSet::new();
        let counts = counter.count(&dirs, &none);
        assert_eq!(counts.get(&dir), Some(&2));
        assert_eq!(counts.get(&dir.join("sub")), Some(&2), "a subfolder counts its whole repository");
        assert!(!counts.contains_key(&outside) && !counts.contains_key(&dir.join("missing")));
        let root = repo_root(&dir).unwrap();
        let ignored = counter.count(&dirs, &HashSet::from([root]));
        assert!(ignored.is_empty(), "an ignored repository is not counted, from any folder");
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-q", "-m", "all"]);
        assert_eq!(counter.count(&dirs, &none).get(&dir), Some(&0));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn parses_status_records() {
        let out = "# branch.oid 0123456789abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
                   1 .M N... 100644 100644 100644 aaa bbb src/a.rs\0\
                   2 R. N... 100644 100644 100644 aaa bbb R100 new name.rs\0old.rs\0\
                   ? notes.md\0";
        let st = parse_status(out);
        assert_eq!((st.branch.as_str(), st.ahead, st.behind, st.head.as_str()), ("main", 2, 1, "01234567"));
        assert_eq!(st.files.len(), 3);
        let renamed = st.files.iter().find(|f| f.path == "new name.rs").unwrap();
        assert_eq!((renamed.index, renamed.orig_path.as_deref()), ('R', Some("old.rs")));
        assert!(st.files.iter().any(|f| f.path == "notes.md" && f.untracked && f.unstaged()));
    }

    #[test]
    fn stages_and_reverts_single_hunks() {
        let dir = repo("hunks");
        let mut text = std::fs::read_to_string(dir.join("a.txt")).unwrap();
        text = text.replace("line 2\n", "line 2 changed\n").replace("line 28\n", "line 28 changed\n");
        std::fs::write(dir.join("a.txt"), &text).unwrap();
        std::fs::write(dir.join("new.txt"), "fresh\n").unwrap();

        let st = status(&dir).unwrap();
        assert_eq!(st.files.len(), 2);
        let diff = diff_file(&dir, "a.txt", false, false).unwrap();
        assert_eq!(diff.hunks.len(), 2);
        assert_eq!((diff.added, diff.removed), (2, 2));
        let added = diff.hunks[0].lines.iter().find(|l| l.kind == LineKind::Added).unwrap();
        assert_eq!((added.new_no, added.text.as_str()), (Some(2), "line 2 changed"));

        // Stage only the first hunk.
        stage_hunk(&dir, &diff, 0).unwrap();
        let staged = diff_file(&dir, "a.txt", true, false).unwrap();
        assert_eq!(staged.hunks.len(), 1);
        assert!(staged.hunks[0].header.starts_with("@@ -1,"));
        let unstaged = diff_file(&dir, "a.txt", false, false).unwrap();
        assert_eq!(unstaged.hunks.len(), 1);

        // Revert the remaining worktree hunk, then unstage the staged one.
        revert_hunk(&dir, &unstaged, 0).unwrap();
        assert!(!std::fs::read_to_string(dir.join("a.txt")).unwrap().contains("line 28 changed"));
        unstage_hunk(&dir, &staged, 0).unwrap();
        assert!(diff_file(&dir, "a.txt", true, false).unwrap().hunks.is_empty());

        let untracked = diff_file(&dir, "new.txt", false, true).unwrap();
        assert_eq!(untracked.added, 1);

        stage_all(&dir).unwrap();
        assert!(commit(&dir, "two changes").unwrap().contains("two changes"));
        assert!(status(&dir).unwrap().files.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn worktree_lifecycle() {
        let dir = repo("wt");
        let wt_path = dir.with_file_name(format!("{}-trees", dir.file_name().unwrap().to_string_lossy())).join("feature-x");
        let _ = std::fs::remove_dir_all(wt_path.parent().unwrap());

        assert!(!valid_branch_name(&dir, "bad..name"));
        create_worktree(&dir, &wt_path, "feature/x", "main").unwrap();
        let wts = list_worktrees(&dir).unwrap();
        assert_eq!(wts.len(), 2);
        assert!(wts[0].is_main && wts[0].branch == "main");
        assert_eq!((wts[1].branch.as_str(), wts[1].path.as_path()), ("feature/x", wt_path.canonicalize().unwrap().as_path()));
        assert!(create_worktree(&dir, &wt_path, "feature/y", "main").is_err(), "existing folder");

        // Work in the worktree: one commit plus an uncommitted file.
        std::fs::write(wt_path.join("a.txt"), "rewritten\n").unwrap();
        sh(&wt_path, &["commit", "-q", "-am", "feature work"]);
        std::fs::write(wt_path.join("scratch.txt"), "wip\n").unwrap();
        let s = worktree_summary(&wt_path, "main").unwrap();
        assert_eq!((s.dirty, s.ahead, s.behind), (1, 1, 0));

        // Removing refuses while dirty unless forced; merging brings the commit over.
        assert!(remove_worktree(&dir, &wt_path, false).is_err());
        assert!(merge_branch(&dir, "feature/x").unwrap().contains("feature/x"));
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "rewritten\n");
        remove_worktree(&dir, &wt_path, true).unwrap();
        assert_eq!(list_worktrees(&dir).unwrap().len(), 1);
        delete_branch(&dir, "feature/x", false).unwrap();
        assert!(!local_branches(&dir).unwrap().0.contains(&"feature/x".to_owned()));

        let _ = std::fs::remove_dir_all(wt_path.parent().unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merge_conflicts_are_aborted() {
        let dir = repo("conflict");
        sh(&dir, &["checkout", "-q", "-b", "other"]);
        std::fs::write(dir.join("a.txt"), "other side\n").unwrap();
        sh(&dir, &["commit", "-q", "-am", "other"]);
        sh(&dir, &["checkout", "-q", "main"]);
        std::fs::write(dir.join("a.txt"), "main side\n").unwrap();
        sh(&dir, &["commit", "-q", "-am", "main"]);
        let err = merge_branch(&dir, "other").unwrap_err().to_string();
        assert!(err.contains("aborted"), "{err}");
        assert!(status(&dir).unwrap().files.is_empty(), "the merge left no changes behind");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn slugs_branch_names() {
        assert_eq!(branch_slug("feature/login fix"), "feature-login-fix");
        assert_eq!(branch_slug("///"), "worktree");
    }

    #[test]
    fn non_repo_has_no_root() {
        let dir = std::env::temp_dir().join(format!("harlo-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // /tmp is not inside a repository on a normal system.
        if repo_root(&std::env::temp_dir()).is_none() {
            assert!(repo_root(&dir).is_none());
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
