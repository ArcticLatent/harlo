//! harlo's cache of what sessions last showed.
//!
//! Per session file, its last context report and its last rows, kept under
//! `$XDG_CACHE_HOME/harlo/history` so a session opens with them while its
//! file is read again.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

/// Where the session caches are kept (`$XDG_CACHE_HOME/harlo/history`, else
/// `~/.cache/harlo/history`).
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|d| d.join("harlo/history"))
}

/// A session file's cache entries in `dir` share one stable name: FNV-1a of
/// its path, so the same file finds them again and no file system character
/// in the path matters.
fn stem(dir: &Path, path: &Path) -> PathBuf {
    let hash = path.as_os_str().as_encoded_bytes().iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3));
    dir.join(format!("{hash:016x}"))
}

/// Where the last context report of a session file is kept in `dir`: shown
/// while the file opens again.
pub fn report_file(dir: &Path, path: &Path) -> PathBuf {
    stem(dir, path).with_extension("report.json")
}

/// Where the last rows a session showed are kept in `dir`.
pub fn rows_file(dir: &Path, path: &Path) -> PathBuf {
    stem(dir, path).with_extension("rows.json")
}

/// A deleted session file's cache entries go with it.
pub fn forget(path: &Path) {
    let Some(dir) = dir() else { return };
    let _ = std::fs::remove_file(report_file(&dir, path));
    let _ = std::fs::remove_file(rows_file(&dir, path));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_entries_share_a_stable_name() {
        let dir = Path::new("/cache");
        let report = report_file(dir, Path::new("/a/s.jsonl"));
        let rows = rows_file(dir, Path::new("/a/s.jsonl"));
        let hash = |p: &Path| p.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('.').next()).map(str::to_owned);
        assert_eq!(hash(&report), hash(&rows), "{report:?} {rows:?}");
        assert_eq!(report, report_file(dir, Path::new("/a/s.jsonl")));
        assert_ne!(report, report_file(dir, Path::new("/a/t.jsonl")));
        assert!(report.to_string_lossy().ends_with(".report.json") && rows.to_string_lossy().ends_with(".rows.json"));
    }
}
