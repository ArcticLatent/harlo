//! A repository's wiki files: page paths, discovery, reading pages and
//! their Claims sidecars (`openwiki/.claims/<page>.json`), with OpenWiki's
//! containment rules (no symbolic links, nothing outside the repository).

use crate::collate;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// What can go wrong, in OpenWiki's error families (the tools turn each
/// into its own answer).
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Caller-correctable retrieval input.
    Retrieval(String),
    /// Workspace configuration or discovery.
    Workspace(String),
    /// Already a host answer: (code, message).
    Host(&'static str, String),
    /// The page doesn't exist.
    PageMissing(String),
    /// Any other Claims failure (invalid sidecars, unreadable files…).
    Claims(String),
    /// A Claims path that isn't safe (a link, an alias, outside the
    /// repository): never treated as recoverable.
    ClaimsSecurity(String),
    /// Lifecycle failure: (code, message).
    Run(&'static str, String),
    /// Something unexpected.
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub const CLAIMS_DIRECTORY: &str = ".claims";
const RESERVED: [&str; 3] = ["index.md", "log.md", "instructions.md"];

pub fn has_traversal(path: &str) -> bool {
    path.split('/').any(|s| s == "." || s == "..")
}

/// Node's `path.posix.normalize`.
pub fn posix_normalize(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let absolute = p.starts_with('/');
    let trailing = p.ends_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if !absolute {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let mut s = out.join("/");
    if s.is_empty() && !absolute {
        s = ".".into();
    }
    if trailing && !s.is_empty() {
        s.push('/');
    }
    if absolute { format!("/{s}") } else { s }
}

fn basename(p: &str) -> &str {
    p.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

/// Whether a virtual page owns Claims state (`isGroundedWikiPage`).
pub fn is_grounded(page: &str) -> bool {
    let slashed = page.replace('\\', "/");
    if has_traversal(&slashed) {
        return false;
    }
    let normalized = posix_normalize(&format!("/{}", slashed.trim_start_matches('/')));
    let lower = normalized.to_lowercase();
    normalized.starts_with("/openwiki/")
        && normalized.ends_with(".md")
        && !lower.split('/').any(|s| s == CLAIMS_DIRECTORY)
        && !RESERVED.contains(&basename(&lower))
}

/// The canonical `/openwiki/...md` path of a page (`normalizeWikiPagePath`).
pub fn normalize_page(page: &str) -> Result<String> {
    let slashed = crate::jsre::trim(page).replace('\\', "/");
    if has_traversal(&slashed) {
        return Err(Error::Claims(format!("Claim page cannot contain traversal segments: {page}")));
    }
    let absolute = posix_normalize(&format!("/{}", slashed.trim_start_matches('/')));
    if !absolute.starts_with("/openwiki/") || !absolute.ends_with(".md") {
        return Err(Error::Claims(format!("Claim page must be a Markdown file below /openwiki: {page}")));
    }
    if !is_grounded(&absolute) {
        return Err(Error::Claims(format!("Claim page is reserved or structural: {page}")));
    }
    Ok(absolute)
}

/// A generated Markdown page from a tool, with or without `openwiki/`
/// (`normalizeWikiToolPagePath`): structural pages allowed, Claims files not.
pub fn normalize_tool_page(page: &str) -> Result<String> {
    let slashed = crate::jsre::trim(page).replace('\\', "/");
    if has_traversal(&slashed) {
        return Err(Error::Claims(format!("Wiki page cannot contain traversal segments: {page}")));
    }
    let unrooted = slashed.trim_start_matches('/');
    let rooted = if unrooted == "openwiki" || unrooted.starts_with("openwiki/") { unrooted.to_owned() } else { format!("openwiki/{unrooted}") };
    let normalized = posix_normalize(&format!("/{rooted}"));
    let lower = normalized.to_lowercase();
    if !normalized.starts_with("/openwiki/") || !normalized.ends_with(".md") || lower.split('/').any(|s| s == CLAIMS_DIRECTORY) {
        return Err(Error::Claims(format!("Wiki page must be a Markdown file below /openwiki: {page}")));
    }
    Ok(normalized)
}

/// `/openwiki/a/b.md` → `a/b.json` (below `.claims`).
pub fn sidecar_relative(page: &str) -> Result<String> {
    let p = normalize_page(page)?;
    let rel = &p["/openwiki/".len()..];
    Ok(format!("{}.json", rel.strip_suffix(".md").unwrap_or(rel)))
}

/// One repository's wiki files.
pub struct Store {
    pub root: PathBuf,
    real_root: std::cell::OnceCell<std::result::Result<PathBuf, String>>,
}

fn io_message(e: &std::io::Error) -> String {
    let code = match e.kind() {
        std::io::ErrorKind::NotFound => "ENOENT: no such file or directory",
        std::io::ErrorKind::PermissionDenied => "EACCES: permission denied",
        _ => "EIO: i/o error",
    };
    code.to_owned()
}

impl Error {
    pub fn message(&self) -> &str {
        match self {
            Error::Retrieval(m) | Error::Workspace(m) | Error::Host(_, m) | Error::PageMissing(m) | Error::Claims(m) | Error::ClaimsSecurity(m) | Error::Run(_, m) | Error::Other(m) => m,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// `sha256:<hex>` of some bytes.
pub fn sha256_version(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("sha256:{}", hex(&sha2::Sha256::digest(bytes)))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Store {
    pub fn new(root: &Path) -> Store {
        Store { root: root.to_path_buf(), real_root: std::cell::OnceCell::new() }
    }

    pub fn wiki_dir(&self) -> PathBuf {
        self.root.join("openwiki")
    }

    fn display(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|_| p.to_string_lossy().into_owned())
    }

    fn real_root(&self) -> Result<PathBuf> {
        self.real_root
            .get_or_init(|| std::fs::canonicalize(&self.root).map_err(|e| io_message(&e)))
            .clone()
            .map_err(|e| Error::ClaimsSecurity(format!("Unable to resolve claims root {}: {e}", self.root.display())))
    }

    /// A contained path's physical location, refusing links and aliases.
    fn physical(&self, p: &Path) -> Result<PathBuf> {
        let real = std::fs::canonicalize(p).map_err(|e| Error::Claims(format!("Unable to resolve {}: {}", self.display(p), io_message(&e))))?;
        let root = self.real_root()?;
        let rel = p.strip_prefix(&self.root).unwrap_or(p);
        if !real.starts_with(&root) || real == root || real != root.join(rel) {
            return Err(Error::ClaimsSecurity(format!("Claims path traverses a symbolic link or filesystem alias: {}", self.display(p))));
        }
        Ok(real)
    }

    /// An existing contained regular file (None: absent).
    pub fn existing_file(&self, p: &Path) -> Result<Option<PathBuf>> {
        let meta = match std::fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::Claims(format!("Unable to inspect {}: {}", self.display(p), io_message(&e)))),
        };
        if meta.file_type().is_symlink() {
            return Err(Error::ClaimsSecurity(format!("Claims path cannot be a symbolic link: {}", self.display(p))));
        }
        if !meta.is_file() {
            return Err(Error::Claims(format!("Claims path is not a regular file: {}", self.display(p))));
        }
        self.physical(p).map(Some)
    }

    /// An existing contained directory (None: absent).
    pub fn existing_dir(&self, p: &Path) -> Result<Option<PathBuf>> {
        let meta = match std::fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::Claims(format!("Unable to inspect {}: {}", self.display(p), io_message(&e)))),
        };
        if meta.file_type().is_symlink() {
            return Err(Error::ClaimsSecurity(format!("Claims path cannot be a symbolic link: {}", self.display(p))));
        }
        if !meta.is_dir() {
            return Err(Error::Claims(format!("Claims path is not a directory: {}", self.display(p))));
        }
        self.physical(p).map(Some)
    }

    /// Generated pages that own Claims, in OpenWiki's order.
    pub fn discover_pages(&self) -> Result<Vec<String>> {
        let Some(dir) = self.existing_dir(&self.wiki_dir())? else { return Ok(Vec::new()) };
        let mut files = Vec::new();
        collect_files(&dir, "", false, &mut files)?;
        let mut pages: Vec<String> = files.into_iter().map(|f| format!("/openwiki/{f}")).filter(|p| is_grounded(p)).collect();
        pages.sort_by(|a, b| collate::compare(a, b));
        Ok(pages)
    }

    /// Pages that have a sidecar (orphans included), in OpenWiki's order.
    pub fn discover_sidecar_pages(&self) -> Result<Vec<String>> {
        let Some(dir) = self.existing_dir(&self.wiki_dir().join(CLAIMS_DIRECTORY))? else { return Ok(Vec::new()) };
        let mut files = Vec::new();
        collect_files(&dir, "", true, &mut files)?;
        let mut pages: Vec<String> = files
            .into_iter()
            .filter(|f| f.ends_with(".json"))
            .map(|f| format!("/openwiki/{}.md", f.strip_suffix(".json").unwrap()))
            .filter(|p| is_grounded(p))
            .collect();
        pages.sort_by(|a, b| collate::compare(a, b));
        Ok(pages)
    }

    pub fn page_path(&self, page: &str) -> Result<PathBuf> {
        let p = normalize_page(page)?;
        Ok(self.root.join(&p[1..]))
    }

    /// A page's Markdown.
    pub fn read_markdown(&self, page: &str) -> Result<String> {
        let normalized = normalize_page(page)?;
        let path = self.root.join(&normalized[1..]);
        let Some(physical) = self.existing_file(&path)? else {
            return Err(Error::PageMissing(format!("Unable to read {normalized}: file does not exist")));
        };
        std::fs::read(&physical)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| Error::Claims(format!("Unable to read {normalized}: {}", io_message(&e))))
    }

    pub fn sidecar_path(&self, page: &str) -> Result<PathBuf> {
        Ok(self.wiki_dir().join(CLAIMS_DIRECTORY).join(sidecar_relative(page)?))
    }

    /// A page's validated sidecar (None: none).
    pub fn load_page(&self, page: &str) -> Result<Option<Value>> {
        let sidecar = self.sidecar_path(page)?;
        let Some(physical) = self.existing_file(&sidecar)? else { return Ok(None) };
        let raw = match std::fs::read(&physical) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::Claims(format!("Unable to read {}: {}", self.display(&sidecar), io_message(&e)))),
        };
        let parsed: Value = serde_json::from_str(&raw).map_err(|e| Error::Claims(format!("Invalid JSON in {}: {e}", self.display(&sidecar))))?;
        validate_page_claims(&parsed, &format!("claim sidecar {}", self.display(&sidecar)))?;
        Ok(Some(parsed))
    }

    /// A page's `sha256:` version (its bytes).
    pub fn hash_page(&self, page: &str) -> Result<String> {
        let normalized = normalize_page(page)?;
        let path = self.root.join(&normalized[1..]);
        let Some(physical) = self.existing_file(&path)? else {
            return Err(Error::PageMissing(format!("Unable to hash {normalized}: file does not exist")));
        };
        std::fs::read(&physical).map(|b| sha256_version(&b)).map_err(|e| Error::Claims(format!("Unable to hash {normalized}: {}", io_message(&e))))
    }

    /// Rewrites an existing page in place.
    pub fn write_markdown(&self, page: &str, content: &str) -> Result<()> {
        let normalized = normalize_page(page)?;
        let path = self.root.join(&normalized[1..]);
        let Some(physical) = self.existing_file(&path)? else {
            return Err(Error::PageMissing(format!("Unable to write {normalized}: file does not exist")));
        };
        std::fs::write(&physical, content).map_err(|e| Error::Claims(format!("Unable to write {normalized}: {}", io_message(&e))))
    }

    /// Validates and writes a page's sidecar atomically.
    pub fn write_page(&self, page: &str, claims: &Value) -> Result<()> {
        let normalized = normalize_page(page)?;
        validate_page_claims(claims, &format!("claims for {normalized}"))?;
        let sidecar = self.sidecar_path(page)?;
        let dir = self.ensure_contained_dir(sidecar.parent().unwrap_or(&self.root))?;
        let name = sidecar.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let text = format!("{}\n", crate::util::json_pretty(claims));
        crate::util::write_new_then_rename(&dir.join(&name), &text, 0o666).map_err(|e| Error::Claims(format!("Unable to persist {}: {}", self.display(&sidecar), io_message(&e))))
    }

    /// Removes a page's sidecar, if any.
    pub fn delete_page(&self, page: &str) -> Result<()> {
        let sidecar = self.sidecar_path(page)?;
        let Some(dir) = self.existing_dir(sidecar.parent().unwrap_or(&self.root))? else { return Ok(()) };
        match std::fs::remove_file(dir.join(sidecar.file_name().unwrap_or_default())) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Claims(format!("Unable to remove claims for {}: {}", normalize_page(page)?, io_message(&e)))),
        }
    }

    /// A directory inside the repository, made if missing, refusing links.
    fn ensure_contained_dir(&self, p: &Path) -> Result<PathBuf> {
        let Ok(rel) = p.strip_prefix(&self.root) else {
            return Err(Error::ClaimsSecurity(format!("Claims directory escapes the repository: {}", p.display())));
        };
        if rel.as_os_str().is_empty() {
            return Err(Error::ClaimsSecurity("Claims directory escapes the repository: ".into()));
        }
        let mut current = self.root.clone();
        for seg in rel.components() {
            current.push(seg);
            let meta = match std::fs::symlink_metadata(&current) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => return Err(Error::Claims(format!("Unable to inspect {}: {}", self.display(&current), io_message(&e)))),
            };
            if meta.file_type().is_symlink() {
                return Err(Error::ClaimsSecurity(format!("Claims path cannot be a symbolic link: {}", self.display(&current))));
            }
            if !meta.is_dir() {
                return Err(Error::Claims(format!("Claims path is not a directory: {}", self.display(&current))));
            }
            self.physical(&current)?;
        }
        std::fs::create_dir_all(p).map_err(|e| Error::Claims(format!("Unable to create {}: {}", self.display(p), io_message(&e))))?;
        self.existing_dir(p)?.ok_or_else(|| Error::Claims(format!("Claims directory disappeared while being created: {}", self.display(p))))
    }
}

fn collect_files(dir: &Path, rel: &str, include_claims: bool, out: &mut Vec<String>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::Claims(format!("Unable to enumerate {}: {}", dir.display(), io_message(&e)))),
    };
    let mut names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort_by(|a, b| collate::compare(a, b));
    for name in names {
        let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let abs = dir.join(&name);
        let meta = match std::fs::symlink_metadata(&abs) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(Error::Claims(format!("Unable to inspect {}: {}", abs.display(), io_message(&e)))),
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            if !include_claims && name.to_lowercase() == CLAIMS_DIRECTORY {
                continue;
            }
            collect_files(&abs, &child_rel, include_claims, out)?;
        } else if meta.is_file() {
            out.push(child_rel);
        }
    }
    Ok(())
}

fn canonical_string(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.is_empty() && crate::jsre::trim(s) == s)
}

/// A sidecar's shape: schema version 1, a page hash, Claims with evidence,
/// optional verification; no other keys, ids and resources unique.
pub fn validate_page_claims(v: &Value, what: &str) -> Result<()> {
    let bad = |m: &str| Err(Error::Claims(format!("Invalid {what}: {m}")));
    let Some(o) = v.as_object() else { return bad("root: Expected object") };
    for k in o.keys() {
        if !["schemaVersion", "pageVersion", "claims", "verification"].contains(&k.as_str()) {
            return bad("root: Unrecognized key");
        }
    }
    if o.get("schemaVersion").and_then(Value::as_f64) != Some(1.0) {
        return bad("schemaVersion: Invalid input");
    }
    static HASH: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new("^sha256:[a-f0-9]{64}$").unwrap());
    if !o.get("pageVersion").and_then(Value::as_str).is_some_and(|s| HASH.is_match(s)) {
        return bad("pageVersion: Invalid");
    }
    let Some(claims) = o.get("claims").and_then(Value::as_array) else { return bad("claims: Expected array") };
    let mut ids = std::collections::HashSet::new();
    for c in claims {
        let Some(c) = c.as_object() else { return bad("claims: Expected object") };
        if c.keys().any(|k| !["id", "statement", "evidence"].contains(&k.as_str()))
            || !c.get("id").is_some_and(canonical_string)
            || !c.get("statement").is_some_and(canonical_string)
        {
            return bad("claims: Invalid claim");
        }
        let Some(ev) = c.get("evidence").and_then(Value::as_array).filter(|e| !e.is_empty()) else { return bad("claims: evidence") };
        let mut resources = std::collections::HashSet::new();
        for e in ev {
            let Some(e) = e.as_object() else { return bad("evidence: Expected object") };
            if e.keys().any(|k| !["resource", "version"].contains(&k.as_str()))
                || !e.get("resource").is_some_and(canonical_string)
                || !e.get("version").is_some_and(canonical_string)
            {
                return bad("evidence: Invalid");
            }
            let r = e["resource"].as_str().unwrap();
            if !resources.insert(r.to_owned()) {
                return Err(Error::Claims(format!("Duplicate evidence resource {r} for claim {} in {what}", c["id"].as_str().unwrap())));
            }
        }
        let id = c["id"].as_str().unwrap();
        if !ids.insert(id.to_owned()) {
            return Err(Error::Claims(format!("Duplicate claim id {id} in {what}")));
        }
    }
    if let Some(ver) = o.get("verification") {
        let ok = ver.as_object().is_some_and(|m| {
            m.len() == 2 && m.get("by").is_some_and(canonical_string) && m.get("at").is_some_and(canonical_string)
        });
        if !ok {
            return bad("verification: Invalid");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_paths() {
        assert_eq!(normalize_page("openwiki//a.md").unwrap(), "/openwiki/a.md");
        assert_eq!(normalize_page(" \\openwiki\\a.md ").unwrap(), "/openwiki/a.md");
        assert!(normalize_page("openwiki/./a.md").is_err());
        assert!(normalize_page("openwiki/index.md").is_err());
        assert!(normalize_page("openwiki/.claims/a.md").is_err());
        assert!(normalize_page("openwiki/A.MD").is_err());
        assert_eq!(posix_normalize("/a//b/"), "/a/b/");
        assert_eq!(sidecar_relative("/openwiki/x/y.md").unwrap(), "x/y.json");
        assert_eq!(normalize_tool_page("index.md").unwrap(), "/openwiki/index.md");
    }
}
