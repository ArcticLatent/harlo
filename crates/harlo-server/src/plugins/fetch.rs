//! Fetching a plugin with `git`: which commit an address means now, and
//! that commit's files, with no history kept.
//!
//! The files are data until a person allows what they ask for, so nothing
//! of the repository runs while it's fetched: no hooks, no submodules, and
//! a symbolic link is kept as a file holding its target, never followed
//! out of the plugin's folder. Git never stops to ask for a password.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use harlo_engine::ascending_id;

/// How long one `git` command may take: a slow connection fetches a
/// plugin's one commit well inside it, and a stalled one ends.
const WAIT: Duration = Duration::from_mins(2);

/// Whether `text` is a full commit ID.
pub(super) fn is_commit(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `src` as `git` is given it: an `https://` address without a name or
/// password in it, or a folder on this computer.
///
/// # Errors
///
/// Fails, with a line for a person, for anything else.
pub(super) fn source(src: &str) -> Result<String, String> {
    let src = src.trim();
    if src.starts_with("https://") {
        let url = reqwest::Url::parse(src).map_err(|e| format!("That isn't an address: {e}."))?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err("Leave the name and password out of the address.".to_owned());
        }
        return Ok(src.to_owned());
    }
    let path = src.strip_prefix("file://").unwrap_or(src);
    if Path::new(path).is_absolute() {
        return if Path::new(path).is_dir() {
            Ok(path.to_owned())
        } else {
            Err(format!("There's no folder {path}."))
        };
    }
    Err("A plugin comes from an https:// address, or a folder on this computer.".to_owned())
}

/// Runs `git` with `args` in `folder`; what it printed.
///
/// # Errors
///
/// Fails, with git's own last line, when it can't run, fails or takes
/// longer than [`WAIT`].
async fn git(folder: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut command = tokio::process::Command::new("git");
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.symlinks=false",
            "-c",
            "submodule.recurse=false",
        ])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    if let Some(folder) = folder {
        command.current_dir(folder);
    }
    let output = tokio::time::timeout(WAIT, command.output())
        .await
        .map_err(|_late| "git took too long.".to_owned())?
        .map_err(|e| format!("git couldn't run: {e}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    Err(said
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("git failed.")
        .trim()
        .to_owned())
}

/// The commit `version` (a tag, branch or commit) of the repository at
/// `src` means now; without one, its default branch's.
///
/// # Errors
///
/// Fails, with a line for a person, when the repository can't be reached
/// or has no such tag or branch.
pub(super) async fn resolve(src: &str, version: Option<&str>) -> Result<String, String> {
    if let Some(commit) = version.filter(|v| is_commit(v)) {
        return Ok(commit.to_lowercase());
    }
    let wanted = version.unwrap_or("HEAD");
    // (A tag that is an object of its own lists the commit it names as
    // `<tag>^{}`.)
    let peeled = format!("{wanted}^{{}}");
    let listed = git(None, &["ls-remote", "--", src, wanted, &peeled])
        .await
        .map_err(|why| format!("Couldn't reach {src}: {why}"))?;
    let refs: Vec<(&str, &str)> = listed
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(commit, _)| is_commit(commit))
        .collect();
    let named = |name: String| refs.iter().find(|(_, r)| *r == name).map(|(c, _)| *c);
    named(format!("refs/tags/{peeled}"))
        .or_else(|| named(format!("refs/tags/{wanted}")))
        .or_else(|| named(format!("refs/heads/{wanted}")))
        .or_else(|| named(wanted.to_owned()))
        .map(str::to_lowercase)
        .ok_or_else(|| match version {
            Some(version) => format!("{src} has no tag or branch {version}."),
            None => format!("{src} has no commits."),
        })
}

/// A plugin's files, fetched into a folder of their own that goes again
/// unless it's kept.
#[derive(Debug)]
pub(super) struct Fetched {
    /// The folder; none once kept.
    dir: Option<PathBuf>,
}

impl Fetched {
    /// The folder the files are in.
    pub(super) fn path(&self) -> &Path {
        self.dir.as_deref().unwrap_or(Path::new(""))
    }

    /// Moves the files to `place`, which they stay at; one already there
    /// is left, being the same commit's.
    ///
    /// # Errors
    ///
    /// Fails, with a line for a person, when they can't be moved.
    pub(super) fn keep(mut self, place: &Path) -> Result<(), String> {
        let Some(dir) = self.dir.take() else {
            return Ok(());
        };
        if place.is_dir() {
            let _ = std::fs::remove_dir_all(&dir);
            return Ok(());
        }
        let failed = |e: std::io::Error| format!("Couldn't keep the plugin: {e}");
        if let Some(parent) = place.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        std::fs::rename(&dir, place).map_err(|e| {
            let _ = std::fs::remove_dir_all(&dir);
            failed(e)
        })
    }
}

impl Drop for Fetched {
    fn drop(&mut self) {
        if let Some(dir) = self.dir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Fetches commit `commit` of the repository at `src` into a new folder
/// under `under`: its files, without its history.
///
/// # Errors
///
/// Fails, with a line for a person, when the commit can't be fetched.
pub(super) async fn fetch(src: &str, commit: &str, under: &Path) -> Result<Fetched, String> {
    let dir = under.join(format!(".{}", ascending_id("fetch")));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't make {}: {e}", dir.display()))?;
    let fetched = Fetched {
        dir: Some(dir.clone()),
    };
    let failed = |why: String| format!("Couldn't fetch {src}: {why}");
    git(Some(&dir), &["init", "-q"]).await.map_err(failed)?;
    git(
        Some(&dir),
        &[
            "fetch",
            "-q",
            "--depth",
            "1",
            "--no-tags",
            "--",
            src,
            commit,
        ],
    )
    .await
    .map_err(failed)?;
    git(Some(&dir), &["checkout", "-q", "--detach", "FETCH_HEAD"])
        .await
        .map_err(failed)?;
    // Its history isn't kept: the commit is its folder's name.
    let _ = std::fs::remove_dir_all(dir.join(".git"));
    Ok(fetched)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_is_an_https_address_or_a_folder() {
        assert_eq!(
            source(" https://github.com/someone/thing ").as_deref(),
            Ok("https://github.com/someone/thing")
        );
        source("https://me:secret@example.com/x").unwrap_err();
        source("http://example.com/x").unwrap_err();
        source("git@github.com:someone/thing.git").unwrap_err();
        source("--upload-pack=touch /tmp/x").unwrap_err();
        source("relative/folder").unwrap_err();
        let here = std::env::temp_dir();
        let here = here.to_str().expect("utf-8");
        assert_eq!(source(here).as_deref(), Ok(here));
        assert_eq!(source(&format!("file://{here}")).as_deref(), Ok(here));
        source("/nonexistent/harlo-tests/plugin").unwrap_err();
    }

    #[test]
    fn a_commit_is_forty_hex_digits() {
        assert!(is_commit("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_commit("0123456789abcdef"));
        assert!(!is_commit("v1.2.0"));
        assert!(!is_commit("../../../../../../../../../../../etc/passwd0"));
    }
}
