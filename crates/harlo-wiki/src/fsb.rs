//! The file access a wiki run makes, confined the way OpenWiki confines its
//! own: virtual paths ("/openwiki/x.md") under the repository root, no
//! `..`, symlinks not followed out of the root, paths a `.openwikiignore`
//! excludes and the Claims folder refused, and writes only under
//! `/openwiki/`. Results and their wording are the same, so a lifecycle
//! error reads the same.

use crate::collate;
use crate::ignore::{FILE as IGNORE_FILE, Ignore};
use crate::store::posix_normalize;
use crate::util::node_io_error;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// One listed entry: its virtual path (a folder's ends in "/").
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub is_dir: bool,
}

impl Entry {
    /// `path.posix.basename(entry.path.replace(/\/$/, ""))`.
    pub fn name(&self) -> &str {
        let p = self.path.strip_suffix('/').unwrap_or(&self.path);
        p.rsplit('/').next().unwrap_or(p)
    }
}

/// A failed read: an error it returns, or one it throws.
#[derive(Debug)]
pub enum ReadError {
    Error(String),
    Thrown(String),
}

impl ReadError {
    pub fn text(&self) -> &str {
        match self {
            ReadError::Error(s) | ReadError::Thrown(s) => s,
        }
    }
}

pub struct Backend {
    root: PathBuf,
    ignore: Ignore,
}

/// `isClaimsStatePath`.
pub fn is_claims_path(p: &str) -> bool {
    let n = posix_normalize(&p.replace('\\', "/")).to_lowercase();
    let abs = if n.starts_with('/') { n } else { format!("/{n}") };
    abs == "/openwiki/.claims" || abs.starts_with("/openwiki/.claims/")
}

/// `isOpenWikiDocsPath`.
pub fn is_docs_path(p: &str) -> bool {
    let slashed = crate::jsre::trim(p).replace('\\', "/");
    let n = posix_normalize(&format!("/{}", slashed.trim_start_matches('/')));
    let v = n.trim_start_matches('/');
    v == "openwiki" || v.starts_with("openwiki/")
}

/// `normalizeVirtualPath`.
pub fn normalize_virtual(p: &str) -> String {
    let slashed = crate::jsre::trim(p).replace('\\', "/");
    posix_normalize(&format!("/{}", slashed.trim_start_matches('/')))
}

/// `path.posix.dirname`.
pub fn dirname(p: &str) -> String {
    let t = p.trim_end_matches('/');
    match t.rfind('/') {
        None => if p.starts_with('/') { "/".into() } else { ".".into() },
        Some(0) => "/".into(),
        Some(i) => t[..i].to_owned(),
    }
}

/// `path.posix.join`.
pub fn join(a: &str, b: &str) -> String {
    let parts: Vec<&str> = [a, b].into_iter().filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return ".".into();
    }
    posix_normalize(&parts.join("/"))
}

/// `path.posix.extname`, lower-cased.
pub fn ext_lower(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);
    match base.rfind('.') {
        Some(i) if i > 0 && !base[..i].chars().all(|c| c == '.') => base[i..].to_lowercase(),
        _ => String::new(),
    }
}

fn open_flags(o: &mut fs::OpenOptions, extra: i32) {
    o.custom_flags(libc::O_NOFOLLOW | extra);
}

impl Backend {
    pub fn new(root: &Path, ignore: Ignore) -> Backend {
        Backend { root: root.to_path_buf(), ignore }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ignore(&self) -> &Ignore {
        &self.ignore
    }

    fn ignored_error(&self, p: &str, dir: bool) -> Option<String> {
        self.ignore.ignores(p, dir).then(|| format!("Path is excluded by {IGNORE_FILE}: {p}"))
    }

    fn claims_error(p: &str) -> Option<String> {
        is_claims_path(p).then(|| format!("OpenWiki Claims state is implementation-owned: {p}"))
    }

    fn docs_only_error(p: &str) -> Option<String> {
        (!is_docs_path(p)).then(|| format!("OpenWiki repository runs may only write under /openwiki/. Refused path: {p}"))
    }

    fn write_guard(&self, p: &str) -> Option<String> {
        self.ignored_error(p, false).or_else(|| Self::claims_error(p)).or_else(|| Self::docs_only_error(p))
    }

    /// `resolvePath` in virtual mode.
    fn resolve(&self, key: &str) -> Result<PathBuf, String> {
        let v = if key.starts_with('/') { key.to_owned() } else { format!("/{key}") };
        if v.contains("..") || v.starts_with('~') {
            return Err("Path traversal not allowed".into());
        }
        let cwd = self.root.to_string_lossy().into_owned();
        let rel = &v[1..];
        let full = if rel.starts_with('/') { posix_normalize(rel) } else { posix_normalize(&format!("{cwd}/{rel}")) };
        let full = if full.len() > 1 { full.trim_end_matches('/').to_owned() } else { full };
        if full != cwd && !full.starts_with(&format!("{cwd}/")) {
            return Err(format!("Path: {full} outside root directory: {cwd}"));
        }
        Ok(PathBuf::from(full))
    }

    /// `assertRealPathWithinRoot`: the deepest existing part of the path,
    /// symlinks followed, still in the root.
    fn within_root(&self, resolved: &Path, p: &str, include_leaf: bool) -> Result<PathBuf, String> {
        let Ok(real_root) = fs::canonicalize(&self.root) else { return Ok(resolved.to_path_buf()) };
        let rel = resolved.strip_prefix(&self.root).unwrap_or(Path::new(""));
        let segs: Vec<_> = rel.components().collect();
        let checked = if include_leaf { &segs[..] } else { &segs[..segs.len().saturating_sub(1)] };
        let mut current = self.root.clone();
        for s in checked {
            let next = current.join(s);
            if let Err(e) = fs::symlink_metadata(&next) {
                if !include_leaf {
                    return Err(node_io_error(&e, "lstat", &next));
                }
                break;
            }
            current = next;
        }
        let real = fs::canonicalize(&current).map_err(|e| node_io_error(&e, "realpath", &current))?;
        if !real.starts_with(&real_root) {
            return Err(format!("Path '{p}' resolves outside root directory"));
        }
        if !include_leaf {
            return Ok(real.join(resolved.file_name().unwrap_or_default()));
        }
        Ok(if current == resolved { real } else { resolved.to_path_buf() })
    }

    /// `readRaw`: a file's text.
    pub fn read_raw(&self, p: &str) -> Result<String, ReadError> {
        if let Some(e) = self.ignored_error(p, false).or_else(|| Self::claims_error(p)) {
            return Err(ReadError::Error(e));
        }
        let resolved = self.resolve(p).map_err(ReadError::Thrown)?;
        self.within_root(&resolved, p, true).map_err(ReadError::Thrown)?;
        let meta = fs::metadata(&resolved).map_err(|e| ReadError::Thrown(node_io_error(&e, "stat", &resolved)))?;
        if !meta.is_file() {
            return Err(ReadError::Error(format!("File '{p}' not found")));
        }
        let mut o = fs::OpenOptions::new();
        o.read(true);
        open_flags(&mut o, 0);
        let mut f = o.open(&resolved).map_err(|e| ReadError::Thrown(node_io_error(&e, "open", &resolved)))?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).map_err(|e| ReadError::Thrown(node_io_error(&e, "read", &resolved)))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// `write`: creates or truncates in place, folders made as needed.
    pub fn write(&self, p: &str, content: &str) -> Result<(), String> {
        if let Some(e) = self.write_guard(p) {
            return Err(e);
        }
        let n = normalize_virtual(p);
        self.raw_write(&n, content).map_err(|e| match e.strip_prefix('\0') {
            Some(n) => format!("Cannot write to {n} because it is a symlink. Symlinks are not allowed."),
            None => format!("Error writing file '{n}': {e}"),
        })
    }

    fn raw_write(&self, n: &str, content: &str) -> Result<(), String> {
        let resolved = self.resolve(n)?;
        self.within_root(&resolved, n, true)?;
        if fs::symlink_metadata(&resolved).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!("\u{0}{n}"));
        }
        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent).map_err(|e| node_io_error(&e, "mkdir", parent))?;
        }
        let mut o = fs::OpenOptions::new();
        o.write(true).create(true).truncate(true).mode(0o644);
        open_flags(&mut o, 0);
        let mut f = o.open(&resolved).map_err(|e| node_io_error(&e, "open", &resolved))?;
        f.write_all(content.as_bytes()).map_err(|e| node_io_error(&e, "write", &resolved))
    }

    /// `edit`: one replacement of `old` with `new` (`performStringReplacement`).
    pub fn edit(&self, p: &str, old: &str, new: &str) -> Result<(), String> {
        if let Some(e) = self.write_guard(p) {
            return Err(e);
        }
        let n = normalize_virtual(p);
        self.raw_edit(&n, old, new).map_err(|e| if e.starts_with("Error: ") { e } else { format!("Error editing file '{n}': {e}") })
    }

    fn raw_edit(&self, n: &str, old: &str, new: &str) -> Result<(), String> {
        let resolved = self.resolve(n)?;
        self.within_root(&resolved, n, true)?;
        let meta = fs::metadata(&resolved).map_err(|e| node_io_error(&e, "stat", &resolved))?;
        if !meta.is_file() {
            return Err(format!("Error: File '{n}' not found"));
        }
        let mut o = fs::OpenOptions::new();
        o.read(true);
        open_flags(&mut o, 0);
        let mut bytes = Vec::new();
        o.open(&resolved).and_then(|mut f| f.read_to_end(&mut bytes)).map_err(|e| node_io_error(&e, "open", &resolved))?;
        let content = String::from_utf8_lossy(&bytes);
        let next = if content.is_empty() && old.is_empty() {
            new.to_owned()
        } else if old.is_empty() {
            return Err("Error: oldString cannot be empty when file has content".into());
        } else {
            let count = content.matches(old).count();
            if count == 0 {
                return Err(format!("Error: String not found in file: '{old}'"));
            }
            if count > 1 {
                return Err(format!("Error: String '{old}' has multiple occurrences (appears {count} times) in file. Use replace_all=True to replace all instances, or provide a more specific string with surrounding context."));
            }
            content.replacen(old, new, 1)
        };
        let mut o = fs::OpenOptions::new();
        o.write(true).truncate(true);
        open_flags(&mut o, 0);
        let mut f = o.open(&resolved).map_err(|e| node_io_error(&e, "open", &resolved))?;
        f.write_all(next.as_bytes()).map_err(|e| node_io_error(&e, "write", &resolved))
    }

    /// `delete`: a file, or a folder and everything in it.
    pub fn delete(&self, p: &str) -> Result<(), String> {
        if let Some(e) = self.write_guard(p) {
            return Err(e);
        }
        let n = normalize_virtual(p);
        let resolved = self.resolve(&n).map_err(|e| format!("Error deleting '{n}': {e}"))?;
        let target = self.within_root(&resolved, &n, false).map_err(|e| format!("Error deleting '{n}': {e}"))?;
        let Ok(meta) = fs::symlink_metadata(&target) else { return Err(format!("Error: '{n}' not found")) };
        let r = if meta.is_dir() { fs::remove_dir_all(&target).map_err(|e| node_io_error(&e, "rm", &target)) } else { fs::remove_file(&target).map_err(|e| node_io_error(&e, "unlink", &target)) };
        r.map_err(|e| format!("Error deleting '{n}': {e}"))
    }

    /// `ls`: a folder's files and folders (symlinks followed), sorted as
    /// `localeCompare` sorts, without what's ignored or the Claims folder;
    /// nothing when it can't be listed. Err: the path itself is refused.
    pub fn ls(&self, p: &str) -> Result<Vec<Entry>, String> {
        if let Some(e) = self.ignored_error(p, true).or_else(|| Self::claims_error(p)) {
            return Err(e);
        }
        let mut out = self.raw_ls(p).unwrap_or_default();
        out.retain(|e| !self.ignore.ignores(&e.path, e.is_dir) && !is_claims_path(&e.path));
        Ok(out)
    }

    fn raw_ls(&self, p: &str) -> Option<Vec<Entry>> {
        let resolved = self.resolve(p).ok()?;
        self.within_root(&resolved, p, true).ok()?;
        if !fs::metadata(&resolved).ok()?.is_dir() {
            return Some(Vec::new());
        }
        let mut out = Vec::new();
        for e in fs::read_dir(&resolved).ok()? {
            let Ok(e) = e else { continue };
            let full = resolved.join(e.file_name());
            let Ok(m) = fs::metadata(&full) else { continue };
            let rel = full.strip_prefix(&self.root).unwrap_or(&full).to_string_lossy().into_owned();
            let v = format!("/{rel}");
            if m.is_file() {
                out.push(Entry { path: v, is_dir: false });
            } else if m.is_dir() {
                out.push(Entry { path: format!("{v}/"), is_dir: true });
            }
        }
        out.sort_by(|a, b| collate::compare(&a.path, &b.path));
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert!(is_claims_path("/openwiki/.claims/a.json"));
        assert!(is_claims_path("openwiki/.CLAIMS"));
        assert!(!is_claims_path("/openwiki/claims"));
        assert!(is_docs_path("openwiki/x.md"));
        assert!(!is_docs_path("/src/x.md"));
        assert!(!is_docs_path("/openwiki/../x.md"));
        assert_eq!(dirname("/openwiki/a.md"), "/openwiki");
        assert_eq!(dirname("/a.md"), "/");
        assert_eq!(join("/openwiki/a", "../b.md"), "/openwiki/b.md");
        assert_eq!(ext_lower("A.MD"), ".md");
        assert_eq!(ext_lower("x"), "");
    }
}
