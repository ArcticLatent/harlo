//! Incremental scan of the harness session directories. Each poll stats
//! every file and re-reads only new or changed ones.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use harlo_harness::{HarnessKind, SessionInfo, store};

#[derive(Debug, Default)]
pub struct ScanDelta {
    pub upserts: Vec<SessionInfo>,
    pub removed: Vec<PathBuf>,
}

impl ScanDelta {
    pub fn is_empty(&self) -> bool {
        self.upserts.is_empty() && self.removed.is_empty()
    }
}

#[derive(Default)]
pub struct Scanner {
    known: HashMap<PathBuf, (SystemTime, u64)>,
}

impl Scanner {
    pub fn poll(&mut self) -> ScanDelta {
        let mut delta = ScanDelta::default();
        let mut seen = HashMap::with_capacity(self.known.len());
        for kind in [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::Claude, HarnessKind::OpenCode] {
            for (path, modified, size) in store::list_files(kind) {
                let changed = self.known.get(&path) != Some(&(modified, size));
                if changed {
                    if let Some(info) = store::read_session(kind, &path) {
                        delta.upserts.push(info);
                    } else {
                        // Not a session yet (header not written); retry next poll.
                        continue;
                    }
                }
                seen.insert(path, (modified, size));
            }
        }
        delta.removed = self.known.keys().filter(|p| !seen.contains_key(*p)).cloned().collect();
        self.known = seen;
        delta
    }
}
