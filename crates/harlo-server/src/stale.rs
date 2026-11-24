//! What each session knows of the files it read and wrote, for the
//! engine's stale-file checks, and which session last wrote each file.
//!
//! Kept in memory: after a restart a session starts knowing no files, so
//! nothing it does is refused for changes made while the server was down.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use harlo_engine::stale::Ledger;

use crate::server::{Server, lock};

/// The files sessions know, and who wrote them last.
#[derive(Debug, Default)]
pub(crate) struct Known {
    /// Each file's fingerprint as each session last saw it, by session.
    seen: HashMap<String, HashMap<PathBuf, u64>>,
    /// The session that last wrote each file.
    writers: HashMap<PathBuf, String>,
}

/// One session's view, handed to its turns.
#[derive(Debug)]
pub(crate) struct SessionFiles {
    /// The server.
    pub server: Arc<Server>,
    /// The session.
    pub session: String,
}

impl Ledger for SessionFiles {
    fn known(&self, path: &Path) -> Option<u64> {
        let live = lock(&self.server.live);
        live.known.seen.get(&self.session)?.get(path).copied()
    }

    fn note(&self, path: &Path, fingerprint: u64, wrote: bool) {
        let mut live = lock(&self.server.live);
        live.known
            .seen
            .entry(self.session.clone())
            .or_default()
            .insert(path.to_path_buf(), fingerprint);
        if wrote {
            live.known
                .writers
                .insert(path.to_path_buf(), self.session.clone());
        }
    }

    fn writer(&self, path: &Path) -> Option<String> {
        let writer = lock(&self.server.live)
            .known
            .writers
            .get(path)
            .filter(|w| **w != self.session)
            .cloned()?;
        let session = self.server.session(&writer).ok()?;
        Some(session.title.unwrap_or(writer))
    }
}

impl Server {
    /// Forgets what session `id` knew of its files, as after an undo puts
    /// them back.
    pub(crate) fn forget_files(&self, id: &str) {
        lock(&self.live).known.seen.remove(id);
    }
}
