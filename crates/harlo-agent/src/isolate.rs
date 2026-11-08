//! Worktrees for subagents that edit files side by side (two or more
//! `general` tasks in one call, in a git project), so none overwrites
//! another's work.
//!
//! - The start: a commit of the project's files as they are (staged,
//!   unstaged and untracked; ignored ones left out), made with a temporary
//!   index: the project's HEAD, index and files aren't touched.
//! - Each subagent works in a worktree of that commit, under harlo-agent's
//!   cache folder (writable inside the sandbox), in the same subfolder.
//! - Its changes are its worktree's files against the start, as a patch.
//!   They're applied to the project in the order the tasks were given;
//!   one that doesn't apply cleanly (another changed the same lines) is
//!   left in its worktree, with its patch, for the model to look at.
//!   Nothing is forced.
//! - Not done (the tasks share the folder, as before): a repository with
//!   submodules, a sparse checkout, a folder that isn't in a repository.
//!   Worktrees left over for more than a week are removed.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

/// Worktrees older than this are removed (one kept for a conflict).
const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 3600);

fn git(dir: &Path, args: &[&str], index: Option<&Path>) -> Result<String, String> {
    let mut c = Command::new("git");
    c.arg("-C").arg(dir).args(args).stdin(Stdio::null());
    if let Some(i) = index {
        c.env("GIT_INDEX_FILE", i);
    }
    // (No hooks, no prompts, a fixed author for the start commit.)
    c.env("GIT_TERMINAL_PROMPT", "0").env("GIT_AUTHOR_NAME", "harlo-agent").env("GIT_AUTHOR_EMAIL", "harlo-agent@localhost").env("GIT_COMMITTER_NAME", "harlo-agent").env("GIT_COMMITTER_EMAIL", "harlo-agent@localhost");
    let out = c.output().map_err(|e| format!("git didn't run: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
}

/// The folder worktrees go in.
fn cache_root() -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".cache"));
    cache.join("harlo-agent/worktrees")
}

/// A tree of `dir`'s files as they are (what `git add -A` would stage),
/// written with a temporary index.
fn tree_of(dir: &Path, scratch: &Path) -> Result<String, String> {
    let index = scratch.join(format!("index-{}", rand()));
    let head = git(dir, &["rev-parse", "--verify", "-q", "HEAD"], None).ok();
    let result = (|| {
        match &head {
            Some(h) => git(dir, &["read-tree", h], Some(&index))?,
            None => git(dir, &["read-tree", "--empty"], Some(&index))?,
        };
        git(dir, &["add", "-A", "."], Some(&index))?;
        git(dir, &["write-tree"], Some(&index))
    })();
    let _ = std::fs::remove_file(&index);
    result
}

fn rand() -> String {
    let mut b = [0u8; 4];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Where a call's subagents start from.
pub struct Baseline {
    /// The repository's top folder.
    pub root: PathBuf,
    /// The working directory, within it.
    rel: PathBuf,
    commit: String,
    /// This call's folder for its worktrees and patches.
    folder: PathBuf,
}

/// One subagent's worktree.
pub struct Worktree {
    pub dir: PathBuf,
    /// Where the subagent works (the same subfolder as the session).
    pub cwd: PathBuf,
    name: String,
}

/// What became of a subagent's changes.
pub enum Applied {
    Nothing,
    /// Applied: the files it changed.
    Done(Vec<String>),
    /// Didn't apply: why, and where they are.
    Kept { why: String, dir: PathBuf, patch: PathBuf, files: Vec<String> },
}

impl Baseline {
    /// The start for subagents working in `cwd`, or why they can't have
    /// worktrees there.
    pub fn take(cwd: &Path) -> Result<Baseline, String> {
        Baseline::take_in(cwd, &cache_root())
    }

    fn take_in(cwd: &Path, cache: &Path) -> Result<Baseline, String> {
        let root = PathBuf::from(git(cwd, &["rev-parse", "--show-toplevel"], None).map_err(|_| "not in a git repository".to_owned())?);
        if git(&root, &["config", "--bool", "core.sparseCheckout"], None).is_ok_and(|v| v == "true") {
            return Err("the repository is a sparse checkout".into());
        }
        if git(&root, &["ls-files", "-s"], None).is_ok_and(|l| l.lines().any(|l| l.starts_with("160000 "))) {
            return Err("the repository has submodules".into());
        }
        prune_old(&root, cache);
        let folder = cache.join(format!("{}-{}", std::process::id(), rand()));
        std::fs::create_dir_all(&folder).map_err(|e| format!("can't make {}: {e}", folder.display()))?;
        let tree = tree_of(&root, &folder)?;
        let mut args = vec!["commit-tree", tree.as_str(), "-m", "harlo-agent: where subagents started"];
        let head = git(&root, &["rev-parse", "--verify", "-q", "HEAD"], None).ok();
        if let Some(h) = &head {
            args.extend(["-p", h.as_str()]);
        }
        let commit = git(&root, &args, None)?;
        let rel = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()).strip_prefix(root.canonicalize().unwrap_or_else(|_| root.clone())).map(Path::to_path_buf).unwrap_or_default();
        Ok(Baseline { root, rel, commit, folder })
    }

    /// A worktree for the subagent `name`.
    pub fn worktree(&self, name: &str) -> Result<Worktree, String> {
        let dir = self.folder.join(name);
        git(&self.root, &["worktree", "add", "-q", "--detach", &dir.to_string_lossy(), &self.commit], None)?;
        let cwd = dir.join(&self.rel);
        let _ = std::fs::create_dir_all(&cwd);
        Ok(Worktree { cwd, dir, name: name.to_owned() })
    }

    /// Brings `w`'s changes over to the project.
    pub fn apply(&self, w: &Worktree) -> Applied {
        let tree = match tree_of(&w.dir, &self.folder) {
            Ok(t) => t,
            Err(why) => return Applied::Kept { why: format!("its changes couldn't be read ({why})"), dir: w.dir.clone(), patch: PathBuf::new(), files: Vec::new() },
        };
        let patch = match git(&self.root, &["diff", "--binary", "--full-index", &self.commit, &tree], None) {
            Ok(p) => p,
            Err(why) => return Applied::Kept { why: format!("its changes couldn't be read ({why})"), dir: w.dir.clone(), patch: PathBuf::new(), files: Vec::new() },
        };
        if patch.trim().is_empty() {
            self.remove(w);
            return Applied::Nothing;
        }
        let file = self.folder.join(format!("{}.patch", w.name));
        if let Err(e) = std::fs::write(&file, format!("{patch}\n")) {
            return Applied::Kept { why: format!("its patch couldn't be saved ({e})"), dir: w.dir.clone(), patch: PathBuf::new(), files: Vec::new() };
        }
        let path = file.to_string_lossy().into_owned();
        let files: Vec<String> = git(&self.root, &["apply", "--numstat", &path], None).map(|s| s.lines().filter_map(|l| l.split('\t').nth(2).map(str::to_owned)).collect()).unwrap_or_default();
        match git(&self.root, &["apply", "--check", "--binary", &path], None).and_then(|_| git(&self.root, &["apply", "--binary", &path], None)) {
            Ok(_) => {
                self.remove(w);
                let _ = std::fs::remove_file(&file);
                Applied::Done(files)
            }
            Err(why) => Applied::Kept { why: why.lines().next().unwrap_or("it conflicts").trim_start_matches("error: ").to_owned(), dir: w.dir.clone(), patch: file, files },
        }
    }

    fn remove(&self, w: &Worktree) {
        let _ = git(&self.root, &["worktree", "remove", "--force", &w.dir.to_string_lossy()], None);
    }
}

impl Drop for Baseline {
    /// The call's folder goes when nothing in it was kept.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.folder);
        let _ = git(&self.root, &["worktree", "prune"], None);
    }
}

/// Removes worktrees left for more than [`KEEP_FOR`].
fn prune_old(root: &Path, cache: &Path) {
    let Ok(entries) = std::fs::read_dir(cache) else { return };
    let now = SystemTime::now();
    for e in entries.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| now.duration_since(t).unwrap_or_default() > KEEP_FOR);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    let _ = git(root, &["worktree", "prune"], None);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("harlo-agent-iso-{}-{}", std::process::id(), rand()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.txt"), "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "ignored.txt\n").unwrap();
        for args in [&["init", "-q"][..], &["add", "."], &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "start"]] {
            assert!(Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap().success());
        }
        dir
    }

    #[test]
    fn side_by_side_changes_come_back_in_order() {
        let dir = repo();
        // Uncommitted and untracked work is where they start from.
        std::fs::write(dir.join("src/a.txt"), "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n").unwrap();
        std::fs::write(dir.join("new.txt"), "untracked\n").unwrap();
        std::fs::write(dir.join("ignored.txt"), "ignored\n").unwrap();
        let unix = std::env::temp_dir().join(format!("harlo-agent-iso-cache-{}", std::process::id()));
        let base = Baseline::take_in(&dir.join("src"), &unix).unwrap();
        let (a, b, c) = (base.worktree("a").unwrap(), base.worktree("b").unwrap(), base.worktree("c").unwrap());
        assert!(a.cwd.ends_with("a/src"));
        assert_eq!(std::fs::read_to_string(a.cwd.join("a.txt")).unwrap().lines().next(), Some("ONE"));
        assert!(a.dir.join("new.txt").exists() && !a.dir.join("ignored.txt").exists());
        // a changes the top, b the bottom of the same file, c the same line as a.
        std::fs::write(a.cwd.join("a.txt"), "ONE\nTWO\nthree\nfour\nfive\nsix\nseven\neight\n").unwrap();
        std::fs::write(b.cwd.join("a.txt"), "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\nEIGHT\n").unwrap();
        std::fs::write(b.dir.join("b-made.txt"), "b\n").unwrap();
        std::fs::write(c.cwd.join("a.txt"), "ONE\nzwei\nthree\nfour\nfive\nsix\nseven\neight\n").unwrap();
        assert!(matches!(base.apply(&a), Applied::Done(f) if f == ["src/a.txt"]));
        assert!(matches!(base.apply(&b), Applied::Done(f) if f.contains(&"b-made.txt".to_owned())));
        let kept = base.apply(&c);
        let Applied::Kept { dir: kept_dir, patch, files, .. } = kept else { panic!("c should conflict") };
        assert!(kept_dir.exists() && patch.exists() && files == ["src/a.txt"]);
        assert_eq!(std::fs::read_to_string(dir.join("src/a.txt")).unwrap(), "ONE\nTWO\nthree\nfour\nfive\nsix\nseven\nEIGHT\n");
        assert!(dir.join("b-made.txt").exists() && !a.dir.exists() && !b.dir.exists());
        // The project's own index is untouched (nothing staged).
        let staged = git(&dir, &["diff", "--cached", "--name-only"], None).unwrap();
        assert!(staged.is_empty(), "{staged}");
        drop(base);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&unix);
    }

    #[test]
    fn not_in_a_repository() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-iso-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(Baseline::take(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
