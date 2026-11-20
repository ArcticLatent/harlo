//! Incremental scan of the harness session directories. Each poll stats
//! every file and re-reads only new or changed ones.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use harlo_harness::{HarnessKind, SessionInfo, store};

/// pi's sessions are listed (it's turned on in Settings → Agents).
static PI_LISTED: AtomicBool = AtomicBool::new(false);

/// Lists pi's sessions with the others, or stops (the next poll drops them).
pub fn set_pi_listed(on: bool) {
    PI_LISTED.store(on, Ordering::Relaxed);
}

/// The agents whose session files on this computer are listed.
pub fn listed_kinds() -> Vec<HarnessKind> {
    if PI_LISTED.load(Ordering::Relaxed) { vec![HarnessKind::Pi] } else { Vec::new() }
}

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

/// Reads this machine's sessions, a changed file at a time.
#[derive(Debug, Default)]
pub struct Scanner {
    known: HashMap<PathBuf, (SystemTime, u64)>,
}

impl Scanner {
    pub fn poll(&mut self) -> ScanDelta {
        let mut delta = ScanDelta::default();
        let mut seen = HashMap::with_capacity(self.known.len());
        for kind in listed_kinds() {
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
