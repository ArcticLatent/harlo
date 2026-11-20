//! The Claude account behind pi's Claude subscription: where Claude Code is
//! and where it keeps its sign-in.
//!
//! harlo doesn't run Claude Code as an agent; pi's Claude subscription
//! extension does, signed in with Claude Code's own sign-in. Settings signs
//! in and out through it and reads the plan's limits (`quota.rs`).

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;

/// Claude Code's config folder: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")))
}

/// The person's own Claude Code: the one set in Settings (`claude_path`, if
/// it's there), else on PATH, else in its install locations (a desktop
/// launch often has a minimal PATH).
pub fn installed() -> Option<PathBuf> {
    let set = crate::db::Db::open(&crate::db::default_path()).ok().and_then(|d| d.get("settings.claude_path").ok().flatten()).unwrap_or_default();
    if !set.trim().is_empty() {
        return Some(PathBuf::from(set.trim())).filter(|p| p.is_file());
    }
    found()
}

/// Claude Code on PATH, else in its install locations.
fn found() -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH").into_iter().flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).map(|d| d.join("claude"));
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let known = home.iter().flat_map(|h| [h.join(".local/bin/claude"), h.join(".claude/local/claude")]).chain([PathBuf::from("/usr/local/bin/claude"), PathBuf::from("/usr/bin/claude")]);
    on_path.chain(known).find(|p| is_executable(p))
}

/// The Claude Code that signs in: the person's own, else the copy harlo
/// downloaded for pi's Claude subscription.
pub fn program() -> Option<PathBuf> {
    installed().or_else(crate::pi_bridge::downloaded_claude)
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_claude_on_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::scratch::Scratch::new(std::env::temp_dir().join(format!("harlo-claude-found-{}", std::process::id())));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let claude = bin.join("claude");
        std::fs::write(&claude, "#!/bin/sh\n").unwrap();
        assert!(!is_executable(&claude), "not runnable yet");
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable(&claude));
        assert!(!is_executable(&bin), "a folder isn't a program");
    }
}
