//! Files clients watch, such as the ones open in an editor: their changes
//! are reported as `filesystem.changed` events.
//!
//! Only the few files clients ask for are watched, so they are looked at
//! a few times a second rather than through the system's file events.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::json;

use crate::server::{Server, lock};

/// How often watched files are looked at.
const EVERY: Duration = Duration::from_millis(250);

/// How a file looked: when it was last changed and its size, or `None`
/// when it wasn't there.
type Seen = Option<(SystemTime, u64)>;

/// A watched file.
#[derive(Debug)]
pub(crate) struct Watch {
    /// The folder of the clients that watch it.
    directory: String,
    /// How many times it's watched; it stops at none.
    count: usize,
    /// How it looked last time.
    seen: Seen,
}

/// The watched files, by path.
#[derive(Debug, Default)]
pub(crate) struct Watches {
    files: HashMap<PathBuf, Watch>,
    /// Whether the task that looks at them is running.
    looking: bool,
}

fn look(file: &PathBuf) -> Seen {
    let meta = std::fs::metadata(file).ok()?;
    Some((
        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        meta.len(),
    ))
}

impl Server {
    /// Reports changes to `file`, in `directory`, until it's unwatched as
    /// many times as it was watched.
    pub(crate) fn watch(self: &Arc<Self>, directory: &str, file: PathBuf) {
        let start = {
            let mut live = lock(&self.live);
            let watches = &mut live.watches;
            let seen = look(&file);
            watches
                .files
                .entry(file)
                .or_insert_with(|| Watch {
                    directory: directory.to_owned(),
                    count: 0,
                    seen,
                })
                .count += 1;
            !std::mem::replace(&mut watches.looking, true)
        };
        if start {
            tokio::spawn(Arc::clone(self).keep_looking());
        }
    }

    /// Takes back one watch of `file`.
    pub(crate) fn unwatch(&self, file: &PathBuf) {
        let mut live = lock(&self.live);
        if let Some(watch) = live.watches.files.get_mut(file) {
            watch.count -= 1;
            if watch.count == 0 {
                live.watches.files.remove(file);
            }
        }
    }

    /// Looks at the watched files until none are left.
    async fn keep_looking(self: Arc<Self>) {
        loop {
            tokio::time::sleep(EVERY).await;
            let files: Vec<(PathBuf, Seen)> = {
                let mut live = lock(&self.live);
                if live.watches.files.is_empty() {
                    live.watches.looking = false;
                    return;
                }
                live.watches
                    .files
                    .iter()
                    .map(|(f, w)| (f.clone(), w.seen))
                    .collect()
            };
            for (file, before) in files {
                let now = look(&file);
                if now == before {
                    continue;
                }
                let event = match (before, now) {
                    (None, Some(_)) => "add",
                    (Some(_), None) => "unlink",
                    _ => "change",
                };
                let directory = {
                    let mut live = lock(&self.live);
                    let Some(watch) = live.watches.files.get_mut(&file) else {
                        continue;
                    };
                    watch.seen = now;
                    watch.directory.clone()
                };
                self.emit(
                    "filesystem.changed",
                    json!({ "file": file, "event": event }),
                    Some(&directory),
                    None,
                );
            }
        }
    }
}
