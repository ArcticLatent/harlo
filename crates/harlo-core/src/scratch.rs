//! Tests' scratch folders, removed when the test ends (passed or failed),
//! so /tmp (in memory here) doesn't fill up run by run.

use std::ops::Deref;
use std::path::{Path, PathBuf};

/// A folder removed when dropped; it reads as `path` (the folder itself,
/// or a file in it).
pub struct Scratch {
    root: PathBuf,
    path: PathBuf,
}

impl Scratch {
    /// The folder `root`, made empty.
    pub fn new(root: PathBuf) -> Self {
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self { path: root.clone(), root }
    }

    /// The same folder, read as `path` in it.
    pub fn at(self, path: PathBuf) -> Self {
        let root = self.root.clone();
        std::mem::forget(self);
        Self { root, path }
    }
}

impl Deref for Scratch {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.path
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
