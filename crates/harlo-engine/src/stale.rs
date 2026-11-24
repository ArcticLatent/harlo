//! Stale-file checks: an agent may only change a file it knows.
//!
//! Each time a session reads or writes a file, what the file held is noted.
//! An edit or a write to a file the session noted before is refused when
//! the file changed since, by another session or outside Harlo, and the
//! agent is told who changed it and to read it again. A file the session
//! never read can still be edited: the edit's text must match what's
//! there.

// Rust guideline compliant 2026-10-09

use std::hash::{Hash as _, Hasher as _};
use std::path::Path;

/// What a session knows of the files it read and wrote.
pub trait Ledger: Send + Sync + std::fmt::Debug {
    /// The fingerprint of `path` as the session last read or wrote it.
    fn known(&self, path: &Path) -> Option<u64>;

    /// Notes that the session now knows `path` as `fingerprint`; `wrote`
    /// when the session wrote it.
    fn note(&self, path: &Path, fingerprint: u64, wrote: bool);

    /// Who last wrote `path` through Harlo, when that wasn't this session:
    /// the other session's name.
    fn writer(&self, path: &Path) -> Option<String>;
}

/// A fingerprint of a file's bytes, to tell whether it changed.
#[must_use]
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Checks that `path` (shown as `shown`), holding `now`, is as the session
/// last knew it.
///
/// # Errors
///
/// When it changed since, with a message for the model naming who
/// changed it.
pub fn check(ledger: &dyn Ledger, path: &Path, shown: &str, now: &[u8]) -> Result<(), String> {
    let Some(known) = ledger.known(path) else {
        return Ok(());
    };
    if known == fingerprint(now) {
        return Ok(());
    }
    let by = ledger.writer(path).map_or_else(
        || "outside this session".to_owned(),
        |name| format!("by the session \"{name}\""),
    );
    Err(format!(
        "{shown} was changed {by} since you last read it. Read it again, then make your \
         change to what it holds now."
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug, Default)]
    struct Notes(Mutex<HashMap<PathBuf, u64>>);

    impl Ledger for Notes {
        fn known(&self, path: &Path) -> Option<u64> {
            self.0.lock().expect("lock").get(path).copied()
        }

        fn note(&self, path: &Path, fingerprint: u64, _wrote: bool) {
            self.0
                .lock()
                .expect("lock")
                .insert(path.to_path_buf(), fingerprint);
        }

        fn writer(&self, _path: &Path) -> Option<String> {
            Some("Fix the parser".to_owned())
        }
    }

    #[test]
    fn a_change_since_the_last_read_is_refused() {
        let notes = Notes::default();
        let path = Path::new("/p/a.txt");
        assert!(check(&notes, path, "a.txt", b"alpha").is_ok(), "never read");
        notes.note(path, fingerprint(b"alpha"), false);
        assert!(check(&notes, path, "a.txt", b"alpha").is_ok(), "unchanged");
        let why = check(&notes, path, "a.txt", b"beta").expect_err("changed");
        assert!(why.contains("\"Fix the parser\""), "{why}");
    }
}
