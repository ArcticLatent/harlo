//! A whole repository's index, kept in a cache keyed by each file's contents.

// Rust guideline compliant 2026-10-09

use crate::scan::{digest, scan_with};
use crate::{Ecosystem, FileIndex, Manifest, lang, manifest};
use serde::{Deserialize, Serialize};
use std::backtrace::Backtrace;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use tree_sitter::Parser;

/// Files larger than this are skipped: generated or minified code, whose
/// symbols nobody looks up, and which would take most of the time.
///
/// The same limit as the wiki's own scope, so both see the same files.
const MAX_BYTES: u64 = 1024 * 1024;

/// The cache's format. Bump it when the scanner's output changes, so every
/// file is read again; the crate's version is part of the key too.
const SCHEMA: u32 = 4;

/// The most threads reading files at once.
///
/// Parsing is CPU-bound and fast; beyond this the threads mostly wait on
/// the disk, and harlo runs beside other work.
const MAX_THREADS: usize = 8;

/// A repository's index: each file's symbols, imports and calls.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    /// Files by their paths from the repository's root (with `/`).
    pub files: BTreeMap<String, FileIndex>,
    /// The package manifests, by path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manifests: Vec<Manifest>,
    /// How many files were read this time (the rest came from the cache).
    #[serde(skip)]
    pub parsed: usize,
}

/// What's kept between runs: each file's contents' hash beside its index.
#[derive(Serialize, Deserialize)]
struct Cached {
    version: String,
    files: BTreeMap<String, Entry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    /// The file's contents' hash.
    sha: String,
    index: FileIndex,
}

/// Failing to read a repository.
pub struct Error {
    path: PathBuf,
    source: std::io::Error,
    backtrace: Backtrace,
}

impl Error {
    fn new(path: &Path, source: std::io::Error) -> Self {
        Self { path: path.to_owned(), source, backtrace: Backtrace::capture() }
    }

    /// The path that couldn't be read.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error").field("path", &self.path).field("source", &self.source).finish_non_exhaustive()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "can't read {}: {}", self.path.display(), self.source)?;
        if self.backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            write!(f, "\n{}", self.backtrace)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// harlo's index cache folder: `$XDG_CACHE_HOME/harlo/index`, else `~/.cache/harlo/index`.
#[must_use]
pub fn default_cache() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("harlo").join("index"))
}

/// The index of the repository at `root`, reading only files that changed
/// since the last run when `cache` (a folder) is given.
///
/// Files are those git would track (`.gitignore` respected, hidden files
/// skipped) in a language the index reads, without vendored, minified or
/// very large ones. A renamed file is found in the cache by its contents.
/// The cache is a file per repository in `cache`; failing to write it only
/// means the next run reads everything again.
///
/// # Errors
///
/// When `root` can't be read.
pub fn index(root: &Path, cache: Option<&Path>) -> Result<Index, Error> {
    let root = root.canonicalize().map_err(|e| Error::new(root, e))?;
    let (paths, manifest_paths) = files(&root);
    let cache_file = cache.map(|c| c.join(format!("{}.json", digest(root.to_string_lossy().as_bytes()))));
    let old = cache_file.as_deref().and_then(load).unwrap_or_default();
    let by_sha: HashMap<&str, &Entry> = old.values().map(|e| (e.sha.as_str(), e)).collect();

    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, usize::from).clamp(1, MAX_THREADS);
    let results: Vec<(String, Entry, bool)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut parser = Parser::new();
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = paths.get(i) else { break };
                        let Ok(bytes) = std::fs::read(root.join(path)) else { continue };
                        let sha = digest(&bytes);
                        let kept = old.get(path).filter(|e| e.sha == sha).or_else(|| by_sha.get(sha.as_str()).copied());
                        if let Some(e) = kept {
                            out.push((path.clone(), e.clone(), false));
                            continue;
                        }
                        let Ok(text) = std::str::from_utf8(&bytes) else { continue };
                        if let Some(index) = scan_with(&mut parser, path, text) {
                            out.push((path.clone(), Entry { sha, index }, true));
                        }
                    }
                    out
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
    });

    let parsed = results.iter().filter(|r| r.2).count();
    let entries: BTreeMap<String, Entry> = results.into_iter().map(|(p, e, _)| (p, e)).collect();
    let changed = parsed > 0 || entries.len() != old.len() || entries.keys().any(|k| !old.contains_key(k));
    if changed && let Some(file) = &cache_file {
        save(file, &entries);
    }
    let manifests = manifests(&root, &manifest_paths);
    Ok(Index { files: entries.into_iter().map(|(p, e)| (p, e.index)).collect(), manifests, parsed })
}

/// The manifests at `paths`, with Cargo's workspace dependencies
/// (`serde.workspace = true`) filled in from the workspace's own.
fn manifests(root: &Path, paths: &[String]) -> Vec<Manifest> {
    let mut out: Vec<Manifest> = paths.iter().filter_map(|p| manifest(p, &std::fs::read_to_string(root.join(p)).ok()?)).collect();
    let shared: Vec<(String, crate::Dependency)> = out
        .iter()
        .filter(|m| m.ecosystem == Ecosystem::Cargo && !m.path.contains('/'))
        .flat_map(|m| m.dependencies.iter().map(|d| (m.path.clone(), d.clone())))
        .collect();
    for m in &mut out {
        if m.ecosystem != Ecosystem::Cargo || !m.path.contains('/') {
            continue;
        }
        for d in &mut m.dependencies {
            if d.version.is_none()
                && d.local.is_none()
                && let Some((_, w)) = shared.iter().find(|(_, w)| w.name == d.name)
            {
                d.version.clone_from(&w.version);
                d.local.clone_from(&w.local);
            }
        }
    }
    out
}

/// The files to index under `root`, and its manifests, sorted, as paths from it.
fn files(root: &Path) -> (Vec<String>, Vec<String>) {
    let mut out = Vec::new();
    let mut manifests = Vec::new();
    let walk = ignore::WalkBuilder::new(root).hidden(true).git_ignore(true).git_exclude(true).git_global(true).require_git(false).build();
    for entry in walk.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let Some(rel) = rel.to_str() else { continue };
        let rel = rel.replace('\\', "/");
        if is_manifest(&rel) && !vendored(&rel) {
            manifests.push(rel);
            continue;
        }
        if lang(&rel).is_none() || vendored(&rel) || entry.metadata().is_ok_and(|m| m.len() > MAX_BYTES) {
            continue;
        }
        out.push(rel);
    }
    out.sort();
    manifests.sort();
    (out, manifests)
}

/// Whether a file is a package manifest, by its name.
fn is_manifest(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "Cargo.toml"
            | "package.json"
            | "pyproject.toml"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "Gemfile"
            | "composer.json"
            | "Package.swift"
            | "pubspec.yaml"
            | "mix.exs"
    ) || name.ends_with(".gemspec")
        || (name.starts_with("requirements") && name.ends_with(".txt"))
}

/// Whether a path is someone else's code kept in the repository, or built code.
pub(crate) fn vendored(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.contains(".min.")
        || ["/vendor/", "/third_party/", "/third-party/", "/node_modules/", "/dist/", "/build/"].iter().any(|d| format!("/{path}").contains(d))
}

fn version() -> String {
    format!("{SCHEMA}-{}", env!("CARGO_PKG_VERSION"))
}

fn load(file: &Path) -> Option<BTreeMap<String, Entry>> {
    let text = std::fs::read(file).ok()?;
    let cached: Cached = serde_json::from_slice(&text).ok()?;
    (cached.version == version()).then_some(cached.files)
}

/// Writes the cache through a temporary file, so a reader never sees half of one.
fn save(file: &Path, files: &BTreeMap<String, Entry>) {
    let Some(dir) = file.parent() else { return };
    let cached = Cached { version: version(), files: files.clone() };
    let Ok(text) = serde_json::to_vec(&cached) else { return };
    let tmp = file.with_extension(format!("json.{}", std::process::id()));
    // A failed write only costs the next run its cache: nothing to report.
    if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, file).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("harlo-index-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, path: &str, text: &str) {
            let f = self.0.join(path);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, text).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn indexes_and_caches() {
        let repo = Scratch::new("repo");
        let cache = Scratch::new("cache");
        repo.write("src/a.rs", "pub fn a() { b(); }\n");
        repo.write("src/b.py", "def b():\n    pass\n");
        repo.write("web/node_modules/x.js", "function x() {}\n");
        repo.write("web/app.min.js", "function y() {}\n");
        repo.write("notes.txt", "hello\n");
        repo.write(".hidden/c.rs", "fn c() {}\n");

        let first = index(&repo.0, Some(&cache.0)).unwrap();
        assert_eq!(first.files.keys().collect::<Vec<_>>(), ["src/a.rs", "src/b.py"]);
        assert_eq!(first.parsed, 2);
        assert_eq!(first.files["src/a.rs"].symbols[0].name, "a");

        // Nothing changed: nothing read.
        let again = index(&repo.0, Some(&cache.0)).unwrap();
        assert_eq!(again.parsed, 0);
        assert_eq!(again.files, first.files);

        // An edit is read; a rename is found by its contents.
        repo.write("src/a.rs", "pub fn a2() {}\n");
        std::fs::rename(repo.0.join("src/b.py"), repo.0.join("src/moved.py")).unwrap();
        let after = index(&repo.0, Some(&cache.0)).unwrap();
        assert_eq!(after.parsed, 1);
        assert_eq!(after.files.keys().collect::<Vec<_>>(), ["src/a.rs", "src/moved.py"]);
        assert_eq!(after.files["src/a.rs"].symbols[0].name, "a2");

        // Without a cache, everything is read.
        assert_eq!(index(&repo.0, None).unwrap().parsed, 2);
    }

    #[test]
    fn a_missing_root_is_an_error() {
        let err = index(Path::new("/nonexistent/harlo-index"), None).unwrap_err();
        assert!(err.to_string().starts_with("can't read /nonexistent/harlo-index"));
        assert_eq!(err.path(), Path::new("/nonexistent/harlo-index"));
    }

    #[test]
    fn a_stale_or_broken_cache_is_ignored() {
        let repo = Scratch::new("stale");
        let cache = Scratch::new("stale-cache");
        repo.write("a.go", "package a\nfunc A() {}\n");
        index(&repo.0, Some(&cache.0)).unwrap();
        let file = std::fs::read_dir(&cache.0).unwrap().next().unwrap().unwrap().path();
        std::fs::write(&file, "{not json").unwrap();
        assert_eq!(index(&repo.0, Some(&cache.0)).unwrap().parsed, 1);
        assert_eq!(index(&repo.0, Some(&cache.0)).unwrap().parsed, 0);
    }
}
