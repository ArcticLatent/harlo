//! Fuzzy file search for `@file` mentions. Each working directory gets a
//! cached, gitignore-aware file list that is rebuilt in the background once
//! it is older than [`STALE_AFTER`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

const STALE_AFTER: Duration = Duration::from_secs(30);
const MAX_FILES: usize = 100_000;

pub struct FileIndex {
    pub root: PathBuf,
    /// Paths relative to `root`, `/`-separated.
    pub files: Vec<String>,
    pub built_at: Instant,
    /// True when the walk stopped at [`MAX_FILES`].
    pub truncated: bool,
}

impl FileIndex {
    pub fn build(root: &Path) -> Self {
        if crate::remote::is_remote(root) {
            return Self::build_remote(root);
        }
        let mut files = Vec::new();
        let mut truncated = false;
        let walker = ignore::WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .parents(true)
            .require_git(false)
            .build();
        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(root) {
                files.push(rel.to_string_lossy().into_owned());
            }
            if files.len() >= MAX_FILES {
                truncated = true;
                break;
            }
        }
        files.sort();
        Self { root: root.to_owned(), files, built_at: Instant::now(), truncated }
    }

    /// A remote folder's files, in one SSH command: git's list (tracked and
    /// untracked, not ignored), else every file outside `.git`.
    fn build_remote(root: &Path) -> Self {
        let script = |dir: &str| format!(
            "cd {} || exit 2; {{ git ls-files -co --exclude-standard 2>/dev/null || find . -type f -not -path '*/.git/*' | sed 's|^\\./||'; }} | head -n {}",
            crate::remote::quote(dir),
            MAX_FILES + 1
        );
        let out = crate::remote::split(root)
            .and_then(|(h, dir)| Some((crate::remote::host(&h)?, dir)))
            .and_then(|(host, dir)| crate::remote::run_ok(&host, &script(&dir), Duration::from_secs(60)).ok())
            .unwrap_or_default();
        let mut files: Vec<String> = out.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect();
        let truncated = files.len() > MAX_FILES;
        files.truncate(MAX_FILES);
        files.sort();
        Self { root: root.to_owned(), files, built_at: Instant::now(), truncated }
    }

    /// Best matches for `query`, best first. An empty query lists the
    /// shallowest paths.
    pub fn search(&self, query: &str, limit: usize) -> Vec<String> {
        let query = query.trim();
        if query.is_empty() {
            let mut shallow: Vec<&String> = self.files.iter().collect();
            shallow.sort_by_key(|f| (f.matches('/').count(), f.len()));
            return shallow.into_iter().take(limit).cloned().collect();
        }
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut scored = pattern.match_list(self.files.iter(), &mut matcher);
        // Prefer shorter paths among equal scores.
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.len().cmp(&b.0.len())));
        scored.into_iter().take(limit).map(|(f, _)| f.clone()).collect()
    }
}

fn cache() -> &'static Mutex<HashMap<PathBuf, Arc<FileIndex>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<FileIndex>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Returns the index for `root`, building it on first use. A stale index is
/// returned immediately and replaced by a fresh one in the background.
/// Blocking: call from a worker thread.
pub fn index_for(root: &Path) -> Arc<FileIndex> {
    let existing = cache().lock().unwrap().get(root).cloned();
    match existing {
        Some(idx) => {
            if idx.built_at.elapsed() > STALE_AFTER {
                let root = root.to_owned();
                // Mark as fresh first so concurrent searches don't all rebuild.
                let placeholder = Arc::new(FileIndex {
                    root: idx.root.clone(),
                    files: idx.files.clone(),
                    built_at: Instant::now(),
                    truncated: idx.truncated,
                });
                cache().lock().unwrap().insert(root.clone(), placeholder);
                std::thread::spawn(move || {
                    let fresh = Arc::new(FileIndex::build(&root));
                    cache().lock().unwrap().insert(root, fresh);
                });
            }
            idx
        }
        None => {
            let idx = Arc::new(FileIndex::build(root));
            cache().lock().unwrap().insert(root.to_owned(), idx.clone());
            idx
        }
    }
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    /// Path relative to the listing's root, `/`-separated.
    pub rel: String,
    pub is_dir: bool,
}

/// Lists one directory under `root`: directories first, then files, both
/// by name. Honours .gitignore and hides `.git`; other dotfiles are shown.
pub fn list_dir(root: &Path, rel: &str) -> std::io::Result<Vec<DirEntry>> {
    let dir = root.join(rel);
    if crate::remote::is_remote(&dir) {
        return list_remote_dir(&dir, rel);
    }
    if !dir.is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} is not a folder", dir.display())));
    }
    let walker = ignore::WalkBuilder::new(&dir)
        .max_depth(Some(1))
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .require_git(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build();
    let mut out: Vec<DirEntry> = walker
        .flatten()
        .filter(|e| e.depth() == 1)
        .filter_map(|e| {
            let is_dir = e.file_type()?.is_dir();
            let rel = e.path().strip_prefix(root).ok()?.to_string_lossy().into_owned();
            Some(DirEntry { name: e.file_name().to_string_lossy().into_owned(), rel, is_dir })
        })
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// [`list_dir`] on a remote host: its entries, less what git ignores there.
fn list_remote_dir(dir: &Path, rel: &str) -> std::io::Result<Vec<DirEntry>> {
    let entries = crate::remote::list_dir(dir).map_err(std::io::Error::other)?;
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).filter(|n| *n != ".git").collect();
    let ignored: std::collections::HashSet<String> = crate::remote::split(dir)
        .and_then(|(h, d)| Some((crate::remote::host(&h)?, d)))
        .and_then(|(host, d)| {
            let input = names.join("\n") + "\n";
            let script = format!("cd {} && git check-ignore --stdin 2>/dev/null", crate::remote::quote(&d));
            crate::remote::run_with_input(&host, &script, Some(input.as_bytes()), Duration::from_secs(20)).ok()
        })
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_owned).collect())
        .unwrap_or_default();
    let prefix = if rel.is_empty() { String::new() } else { format!("{}/", rel.trim_end_matches('/')) };
    let mut out: Vec<DirEntry> = entries
        .into_iter()
        .filter(|e| e.name != ".git" && !ignored.contains(&e.name))
        .map(|e| DirEntry { rel: format!("{prefix}{}", e.name), name: e.name, is_dir: e.is_dir })
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// Text files larger than this are not opened in the editor.
pub const MAX_EDIT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    Text(String),
    Binary,
    TooLarge(u64),
}

/// Reads a file for viewing/editing: text (UTF-8, lossy), or why not.
pub fn read_text(path: &Path) -> std::io::Result<FileContent> {
    let bytes = if let Some((name, f)) = crate::remote::split(path) {
        // On its host: the size first, then the bytes, in one command.
        let host = crate::remote::host(&name).ok_or_else(|| std::io::Error::other(format!("unknown remote host {name}")))?;
        let q = crate::remote::quote(&f);
        let script = format!("s=$(wc -c < {q}) || exit 2; if [ \"$s\" -gt {MAX_EDIT_BYTES} ]; then echo \"$s\"; exit 3; fi; cat -- {q}");
        let out = crate::remote::run(&host, &script, Duration::from_secs(60)).map_err(std::io::Error::other)?;
        match out.status.code() {
            Some(0) => out.stdout,
            Some(3) => return Ok(FileContent::TooLarge(String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(MAX_EDIT_BYTES + 1))),
            _ => return Err(std::io::Error::other(crate::remote::ssh_error(&name, &out))),
        }
    } else {
        let size = std::fs::metadata(path)?.len();
        if size > MAX_EDIT_BYTES {
            return Ok(FileContent::TooLarge(size));
        }
        std::fs::read(path)?
    };
    if let Some(at) = bytes[..bytes.len().min(8192)].iter().position(|b| *b == 0) {
        tracing::info!("{} looks binary: zero byte at offset {at} of {} bytes", path.display(), bytes.len());
        return Ok(FileContent::Binary);
    }
    Ok(FileContent::Text(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Saves text atomically (temp file + rename), keeping the file's mode.
pub fn write_text(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    // On its host: a temp file next to it, its mode copied, then renamed.
    if let Some((name, f)) = crate::remote::split(path) {
        let host = crate::remote::host(&name).ok_or_else(|| std::io::Error::other(format!("unknown remote host {name}")))?;
        let file = Path::new(&f);
        let tmp = file.with_file_name(format!(".{}.harlo-tmp", file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        let (q, qt) = (crate::remote::quote(&f), crate::remote::quote(&tmp.to_string_lossy()));
        let script = format!("cat > {qt} && {{ chmod --reference={q} {qt} 2>/dev/null || true; }} && mv -f -- {qt} {q}");
        let out = crate::remote::run_with_input(&host, &script, Some(text.as_bytes()), Duration::from_secs(60)).map_err(std::io::Error::other)?;
        if !out.status.success() {
            return Err(std::io::Error::other(crate::remote::ssh_error(&name, &out)));
        }
        return Ok(());
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.harlo-tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_reads_and_writes() {
        let root = std::env::temp_dir().join(format!("harlo-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("build")).unwrap();
        std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
        std::fs::write(root.join("README.md"), "hi").unwrap();
        std::fs::write(root.join("src/a.rs"), "fn a() {}").unwrap();
        std::fs::write(root.join("blob.bin"), [0u8, 1, 2]).unwrap();
        let top: Vec<(String, bool)> = list_dir(&root, "").unwrap().into_iter().map(|e| (e.name, e.is_dir)).collect();
        assert_eq!(top, [("src".into(), true), (".gitignore".into(), false), ("blob.bin".into(), false), ("README.md".into(), false)]);
        assert_eq!(list_dir(&root, "src").unwrap()[0].rel, "src/a.rs");
        assert_eq!(read_text(&root.join("blob.bin")).unwrap(), FileContent::Binary);
        write_text(&root.join("src/a.rs"), "fn b() {}\n").unwrap();
        assert_eq!(read_text(&root.join("src/a.rs")).unwrap(), FileContent::Text("fn b() {}\n".into()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn indexes_and_ranks_files() {
        let root = std::env::temp_dir().join(format!("harlo-files-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src/deep")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        for f in ["src/main.rs", "src/deep/maintenance.rs", "README.md", "target/junk.rs"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
        let idx = FileIndex::build(&root);
        assert!(!idx.files.iter().any(|f| f.starts_with("target")), "{:?}", idx.files);
        assert_eq!(idx.search("main", 5).first().map(String::as_str), Some("src/main.rs"));
        assert_eq!(idx.search("", 1), ["README.md"]);
        let _ = std::fs::remove_dir_all(root);
    }
}
