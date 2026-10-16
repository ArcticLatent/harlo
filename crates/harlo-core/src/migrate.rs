//! harlo was called piqt. On the first start under the new name, piqt's
//! data, settings and autostart entry move over:
//!
//! - `~/.local/share/piqt/*` → `~/.local/share/harlo/` (`piqt.db` becomes
//!   `harlo.db`), except `worktrees/`: git keeps their paths, so they stay
//!   where they are (the database still points at them);
//! - `~/.config/piqt/` → `~/.config/harlo/` (`piqt.conf` → `harlo.conf`);
//! - `~/.config/autostart/piqt.desktop` → `harlo.desktop`.
//!
//! Only when harlo has nothing of its own yet, so it runs once.

use std::path::{Path, PathBuf};

fn base(var: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(fallback)))
}

/// piqt's data is there and harlo has none yet.
pub fn needed() -> bool {
    base("XDG_DATA_HOME", ".local/share").is_some_and(|d| d.join("piqt").is_dir() && !d.join("harlo").exists())
}

/// piqt (the old build) is running: its data can't move from under it.
pub fn piqt_running() -> bool {
    let Ok(procs) = std::fs::read_dir("/proc") else { return false };
    procs.flatten().any(|p| std::fs::read_to_string(p.path().join("comm")).is_ok_and(|c| c.trim() == "piqt"))
}

/// What moved (for the log); empty when there was nothing to move.
pub fn from_piqt() -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(data) = base("XDG_DATA_HOME", ".local/share") {
        move_data(&data.join("piqt"), &data.join("harlo"), &mut notes);
    }
    if let Some(config) = base("XDG_CONFIG_HOME", ".config") {
        move_config(&config.join("piqt"), &config.join("harlo"), &mut notes);
        let old_entry = config.join("autostart").join("piqt.desktop");
        if old_entry.is_file() && std::fs::remove_file(&old_entry).is_ok() {
            match std::env::current_exe() {
                Ok(exe) if crate::autostart::set(true, &exe).is_ok() => notes.push("start at login: moved to harlo.desktop".to_owned()),
                _ => notes.push("start at login: the old entry is gone; turn it on again in Settings → General".to_owned()),
            }
        }
    }
    notes
}

fn move_data(old: &Path, new: &Path, notes: &mut Vec<String>) {
    if !old.is_dir() || new.exists() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(new) {
        notes.push(format!("couldn't create {}: {e}", new.display()));
        return;
    }
    let Ok(entries) = std::fs::read_dir(old) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Worktrees stay: git (and the database) know them by their paths.
        if name == "worktrees" {
            continue;
        }
        let target = match name.strip_prefix("piqt.db") {
            Some(rest) => format!("harlo.db{rest}"),
            None => name.clone(),
        };
        match std::fs::rename(entry.path(), new.join(&target)) {
            Ok(()) => notes.push(format!("moved {} to {}", entry.path().display(), new.join(&target).display())),
            Err(e) => notes.push(format!("couldn't move {}: {e}", entry.path().display())),
        }
    }
}

fn move_config(old: &Path, new: &Path, notes: &mut Vec<String>) {
    if !old.is_dir() || new.exists() {
        return;
    }
    match std::fs::rename(old, new) {
        Ok(()) => {
            notes.push(format!("moved {} to {}", old.display(), new.display()));
            let _ = std::fs::rename(new.join("piqt.conf"), new.join("harlo.conf"));
        }
        Err(e) => notes.push(format!("couldn't move {}: {e}", old.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_piqts_data_but_not_its_worktrees() {
        let root = std::env::temp_dir().join(format!("harlo-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join("data/piqt");
        for f in ["piqt.db", "piqt.db-wal", "phone/identity.json", "whisper/ggml-base.bin", "worktrees/wt/.git"] {
            let p = old.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, f).unwrap();
        }
        let new = root.join("data/harlo");
        let mut notes = Vec::new();
        move_data(&old, &new, &mut notes);
        assert_eq!(std::fs::read_to_string(new.join("harlo.db")).unwrap(), "piqt.db");
        assert!(new.join("harlo.db-wal").is_file());
        assert!(new.join("phone/identity.json").is_file() && new.join("whisper/ggml-base.bin").is_file());
        assert!(old.join("worktrees/wt/.git").is_file() && !new.join("worktrees").exists());

        // Once: harlo's own data is left alone.
        std::fs::write(old.join("piqt.db"), "again").unwrap();
        move_data(&old, &new, &mut notes);
        assert_eq!(std::fs::read_to_string(new.join("harlo.db")).unwrap(), "piqt.db");

        let (cold, cnew) = (root.join("config/piqt"), root.join("config/harlo"));
        std::fs::create_dir_all(&cold).unwrap();
        std::fs::write(cold.join("piqt.conf"), "[window]").unwrap();
        move_config(&cold, &cnew, &mut notes);
        assert_eq!(std::fs::read_to_string(cnew.join("harlo.conf")).unwrap(), "[window]");
        assert!(!cold.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
