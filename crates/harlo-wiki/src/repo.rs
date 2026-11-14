//! The repository as a writing run sees it, as OpenWiki sees it: the last
//! update's record, a hash of the wiki's content, a fingerprint of every
//! source the wiki may read, what changed since a commit, and whether an
//! update has nothing to do.

use crate::ignore::{self, Ignore};
use crate::util;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Stdio};

pub const WIKI_DIR: &str = "openwiki";
pub const PAGE_MANIFEST_PATH: &str = "openwiki/.page-manifest.json";
pub const UPDATE_METADATA_PATH: &str = "openwiki/.last-update.json";
pub const RUN_STATE_NAME: &str = ".run.json";
pub const INSTRUCTIONS_FILE: &str = "INSTRUCTIONS.md";

pub type Result<T> = std::result::Result<T, String>;

// ---- Git ----

fn git_command(root: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.arg("--no-pager").args(args).current_dir(root).stdin(Stdio::null());
    c
}

/// Git's output, or why it failed.
fn git_strict(root: &Path, args: &[&str]) -> std::result::Result<Vec<u8>, String> {
    let out = git_command(root, args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(out.stdout)
}

/// Git's output lines, trimmed and without blanks (none when it fails).
pub fn git_lines(root: &Path, args: &[&str]) -> Vec<String> {
    match git_strict(root, args) {
        Ok(out) => String::from_utf8_lossy(&out).split('\n').map(|l| crate::jsre::trim(l).to_owned()).filter(|l| !l.is_empty()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Its output and errors together, trimmed (as OpenWiki's `runGit`).
fn git_text(root: &Path, args: &[&str]) -> String {
    let Ok(out) = git_command(root, args).output() else { return String::new() };
    let a = String::from_utf8_lossy(&out.stdout);
    let b = String::from_utf8_lossy(&out.stderr);
    [crate::jsre::trim(&a), crate::jsre::trim(&b)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n").trim().to_owned()
}

fn is_object_id(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn git_head(root: &Path) -> Option<String> {
    git_lines(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).into_iter().next().filter(|h| is_object_id(h))
}

pub fn is_readable_commit(root: &Path, head: &str) -> bool {
    is_object_id(head) && git_strict(root, &["cat-file", "-e", &format!("{head}^{{commit}}")]).is_ok()
}

pub fn is_wiki_path(p: &str) -> bool {
    p == WIKI_DIR || p.starts_with("openwiki/")
}

fn normalize_git_path(v: &str) -> String {
    crate::jsre::trim(v).replace('\\', "/")
}

/// Paths changed since `base` (committed), in the work tree, and new ones,
/// leaving out the wiki's and ignored ones, in code-unit order.
pub fn changed_paths(root: &Path, ignore: &Ignore, base: Option<&str>) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let mut add = |p: String| {
        if !paths.contains(&p) {
            paths.push(p);
        }
    };
    if let Some(b) = base {
        for c in git_lines(root, &["diff", "--name-only", &format!("{b}..HEAD")]) {
            add(normalize_git_path(&c));
        }
    }
    for c in git_lines(root, &["diff", "--name-only", "HEAD"]) {
        add(normalize_git_path(&c));
    }
    for c in git_lines(root, &["ls-files", "--others", "--exclude-standard"]) {
        add(normalize_git_path(&c));
    }
    let mut out: Vec<String> = paths.into_iter().filter(|p| !p.is_empty() && !is_wiki_path(p) && !ignore.ignores(p, false)).collect();
    out.sort_by(|a, b| util::cmp16(a, b));
    out
}

const COMMITS: usize = 20;
const SUBJECT_CHARS: usize = 200;
const BODY_CHARS: usize = 400;
const TOTAL_CHARS: i64 = 6000;

fn strip_controls(v: &str) -> String {
    v.chars().filter(|c| *c != '\r' && !matches!(*c as u32, 0x00..=0x08 | 0x0b..=0x1f | 0x7f..=0x9f)).collect()
}

/// The newest commits since `base` (sha, subject, body), as planning hints:
/// none while `.openwikiignore` has rules.
pub fn commit_context(root: &Path, ignore: &Ignore, base: Option<&str>) -> Vec<Value> {
    let Some(base) = base else { return Vec::new() };
    if ignore.is_active() {
        return Vec::new();
    }
    let range = format!("{base}..HEAD");
    let max = format!("--max-count={COMMITS}");
    let exclude = format!(":(exclude){WIKI_DIR}");
    let Ok(out) = git_strict(root, &["log", "-z", "--no-merges", "--no-show-signature", &max, "--format=%H%n%s%n%b", &range, "--", ".", &exclude]) else {
        return Vec::new();
    };
    let out = String::from_utf8_lossy(&out);
    let mut commits = Vec::new();
    let mut remaining = TOTAL_CHARS;
    for record in out.split('\0') {
        if remaining <= 0 {
            break;
        }
        let rec = crate::jsre::trim(record);
        let mut lines = rec.split('\n');
        let sha = crate::jsre::trim(lines.next().unwrap_or("")).to_owned();
        let subject = lines.next().unwrap_or("");
        let body: Vec<&str> = lines.collect();
        if !is_object_id(&sha) {
            continue;
        }
        let subject = util::truncate_points(crate::jsre::trim(&strip_controls(subject)), (SUBJECT_CHARS as i64).min(remaining) as usize);
        remaining -= crate::jsre::len16(&subject) as i64;
        let collapsed = collapse_blank_lines(&strip_controls(&body.join("\n")));
        let body = util::truncate_points(crate::jsre::trim(&collapsed), (BODY_CHARS as i64).min(remaining.max(0)) as usize);
        remaining -= crate::jsre::len16(&body) as i64;
        let mut c = Map::new();
        c.insert("sha".into(), sha.into());
        c.insert("subject".into(), subject.into());
        c.insert("body".into(), body.into());
        commits.push(Value::Object(c));
    }
    commits
}

/// `\n{3,}` → `\n\n`.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = 0;
    for c in s.chars() {
        if c == '\n' {
            run += 1;
            continue;
        }
        if run > 0 {
            out.push_str(if run >= 3 { "\n\n" } else if run == 2 { "\n\n" } else { "\n" });
            run = 0;
        }
        out.push(c);
    }
    if run > 0 {
        out.push_str(if run >= 2 { "\n\n" } else { "\n" });
    }
    out
}

// ---- The last update ----

#[derive(Debug, Clone, PartialEq)]
pub struct LastUpdate {
    pub updated_at: String,
    pub command: String,
    pub git_head: Option<String>,
    pub model: String,
    pub status: String,
    pub language: Option<String>,
}

impl LastUpdate {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("updatedAt".into(), self.updated_at.clone().into());
        m.insert("command".into(), self.command.clone().into());
        if let Some(h) = &self.git_head {
            m.insert("gitHead".into(), h.clone().into());
        }
        m.insert("model".into(), self.model.clone().into());
        m.insert("status".into(), self.status.clone().into());
        if let Some(l) = &self.language {
            m.insert("language".into(), l.clone().into());
        }
        Value::Object(m)
    }

    /// As OpenWiki reads one back (lenient: unknown values fall back).
    pub fn from_json(v: &Value) -> Option<LastUpdate> {
        let s = |k: &str| v.get(k).and_then(Value::as_str);
        Some(LastUpdate {
            updated_at: s("updatedAt")?.to_owned(),
            command: if s("command")? == "init" { "init" } else { "update" }.to_owned(),
            git_head: s("gitHead").map(str::to_owned),
            model: s("model")?.to_owned(),
            status: if s("status") == Some("interrupted") { "interrupted" } else { "complete" }.to_owned(),
            language: s("language").map(str::to_owned),
        })
    }
}

pub fn read_last_update(root: &Path) -> Result<Option<LastUpdate>> {
    match std::fs::read(root.join(UPDATE_METADATA_PATH)) {
        Ok(b) => Ok(serde_json::from_slice::<Value>(&b).ok().and_then(|v| LastUpdate::from_json(&v))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Records a run's end (or start, as interrupted): `git_head` None takes
/// the current HEAD, Some(None) leaves it out.
pub fn write_last_update(root: &Path, command: &str, model: &str, status: &str, language: Option<&str>, git_head: Option<Option<&str>>) -> Result<()> {
    let head = match git_head {
        Some(None) => None,
        Some(Some(h)) => Some(h.to_owned()),
        None => self::git_head(root),
    };
    let m = LastUpdate {
        updated_at: util::now_iso(),
        command: command.to_owned(),
        git_head: head,
        model: model.to_owned(),
        status: status.to_owned(),
        language: language.filter(|l| !l.is_empty()).map(str::to_owned),
    };
    util::write_text_atomic(&root.join(UPDATE_METADATA_PATH), &format!("{}\n", util::json_pretty(&m.to_json())), 0o644).map_err(|e| e.to_string())
}

/// The wiki's own instructions (openwiki/INSTRUCTIONS.md), when it has any.
pub fn wiki_goal(root: &Path) -> Result<Option<String>> {
    match std::fs::read(root.join(WIKI_DIR).join(INSTRUCTIONS_FILE)) {
        Ok(b) => {
            let t = String::from_utf8_lossy(&b);
            let t = crate::jsre::trim(&t);
            Ok((!t.is_empty()).then(|| t.to_owned()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// ---- The wiki's content ----

/// A hash of the wiki's files, its run records left out (an update that
/// changes none of them changes nothing).
pub fn content_snapshot(root: &Path) -> Result<String> {
    let mut h = Sha256::new();
    add_dir(&mut h, &root.join(WIKI_DIR), "")?;
    Ok(hex(&h.finalize()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn race(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory | std::io::ErrorKind::IsADirectory) || e.raw_os_error() == Some(21) || e.raw_os_error() == Some(20)
}

fn add_dir(h: &mut Sha256, dir: &Path, rel: &str) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if race(&e) => {
            h.update(b"missing");
            return Ok(());
        }
        Err(e) => return Err(e.to_string()),
    };
    let mut list: Vec<(String, std::fs::FileType)> = Vec::new();
    for e in entries {
        let e = e.map_err(|e| e.to_string())?;
        let t = e.file_type().map_err(|e| e.to_string())?;
        list.push((e.file_name().to_string_lossy().into_owned(), t));
    }
    list.sort_by(|a, b| crate::collate::compare(&a.0, &b.0));
    for (name, t) in list {
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if r == ".last-update.json" || r == RUN_STATE_NAME {
            continue;
        }
        let p = dir.join(&name);
        if t.is_dir() {
            h.update(format!("dir:{r}\0").as_bytes());
            add_dir(h, &p, &r)?;
            continue;
        }
        if !t.is_file() {
            continue;
        }
        match std::fs::read(&p) {
            Ok(b) => {
                h.update(format!("file:{r}\0").as_bytes());
                h.update(&b);
                h.update(b"\0");
            }
            Err(e) if race(&e) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

// ---- The source ----

const FINGERPRINT_VERSION: &str = "openwiki-source-fingerprint-v1";

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub fingerprint: String,
    pub git_head: Option<String>,
}

fn field(h: &mut Sha256, label: &str, value: &[u8]) {
    h.update(label.as_bytes());
    h.update(b"\0");
    h.update(value.len().to_string().as_bytes());
    h.update(b"\0");
    h.update(value);
    h.update(b"\0");
}

fn fail(m: &str) -> String {
    format!("Git failed while creating the repository source fingerprint: git {m}")
}

fn split_nul(out: &[u8]) -> Result<Vec<String>> {
    if out.is_empty() {
        return Ok(Vec::new());
    }
    if *out.last().unwrap() != 0 {
        return Err("Git returned non-NUL-terminated fingerprint output.".into());
    }
    Ok(out[..out.len() - 1].split(|b| *b == 0).map(|s| String::from_utf8_lossy(s).into_owned()).collect())
}

fn git_path(v: &str) -> Result<String> {
    if v.is_empty() || v.starts_with('/') {
        return Err(format!("Git returned an invalid repository path: {v}"));
    }
    let n = crate::store::posix_normalize(v);
    if n == ".." || n.starts_with("../") {
        return Err(format!("Git returned an escaping repository path: {v}"));
    }
    Ok(n)
}

fn is_source_path(p: &str, ignore: &Ignore) -> bool {
    if p == ignore::FILE {
        return true;
    }
    if p == ".git" || p.starts_with(".git/") {
        return false;
    }
    !is_wiki_path(p) && !ignore.ignores(p, false)
}

/// A fingerprint of every source the wiki may read (HEAD, the work tree's
/// status, and each visible file's kind, mode and bytes), and HEAD.
pub fn source_snapshot(root: &Path, ignore: &Ignore) -> Result<Source> {
    if !root.is_absolute() {
        return Err("Repository source fingerprint requires an absolute root.".into());
    }
    // (OpenWiki asks all four at once and reports the first to fail: a
    // list, since the head takes two calls when it fails.)
    let tracked_out = git_strict(root, &["ls-files", "--cached", "-z"]).map_err(|_| fail("ls-files --cached -z"))?;
    let untracked_out = git_strict(root, &["ls-files", "--others", "--exclude-standard", "-z"]).map_err(|_| fail("ls-files --others --exclude-standard -z"))?;
    let status_out = git_strict(root, &["status", "--porcelain=v1", "--untracked-files=all", "--no-renames", "-z"])
        .map_err(|_| fail("status --porcelain=v1 --untracked-files=all --no-renames -z"))?;
    let head = match git_strict(root, &["rev-parse", "--verify", "HEAD"]) {
        Ok(o) => String::from_utf8_lossy(&o).trim_end().to_owned(),
        Err(_) => match git_strict(root, &["symbolic-ref", "-q", "HEAD"]) {
            Ok(o) if !String::from_utf8_lossy(&o).trim_end().is_empty() => format!("unborn:{}", String::from_utf8_lossy(&o).trim_end()),
            _ => return Err("Unable to resolve repository HEAD for fingerprinting.".into()),
        },
    };
    if head.is_empty() {
        return Err("Unable to resolve repository HEAD for fingerprinting.".into());
    }
    let tracked: Vec<String> = split_nul(&tracked_out)?.iter().map(|p| git_path(p)).collect::<Result<_>>()?;
    let tracked_set: std::collections::HashSet<&str> = tracked.iter().map(String::as_str).collect();
    let mut candidates: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in tracked.iter().cloned().chain(split_nul(&untracked_out)?.iter().map(|p| git_path(p)).collect::<Result<Vec<_>>>()?) {
        if seen.insert(p.clone()) {
            candidates.push(p);
        }
    }
    if std::fs::symlink_metadata(root.join(ignore::FILE)).is_ok() && seen.insert(ignore::FILE.to_owned()) {
        candidates.push(ignore::FILE.to_owned());
    }
    let mut visible: Vec<String> = candidates.into_iter().filter(|p| is_source_path(p, ignore)).collect();
    visible.sort_by(|a, b| util::cmp16(a, b));
    let mut status: Vec<(String, String)> = Vec::new();
    for rec in split_nul(&status_out)? {
        if rec.len() < 4 || rec.as_bytes()[2] != b' ' {
            return Err("Git returned malformed porcelain status output.".into());
        }
        let p = git_path(&rec[3..])?;
        if is_source_path(&p, ignore) {
            status.push((rec[..2].to_owned(), p));
        }
    }
    status.sort_by(|a, b| util::cmp16(&format!("{}\0{}", a.0, a.1), &format!("{}\0{}", b.0, b.1)));
    let mut h = Sha256::new();
    field(&mut h, "format", FINGERPRINT_VERSION.as_bytes());
    field(&mut h, "head", head.as_bytes());
    for (code, p) in &status {
        field(&mut h, "status-code", code.as_bytes());
        field(&mut h, "status-path", p.as_bytes());
    }
    for p in &visible {
        source_entry(&mut h, root, p, tracked_set.contains(p.as_str()))?;
    }
    Ok(Source { fingerprint: format!("sha256:{}", hex(&h.finalize())), git_head: (!head.starts_with("unborn:")).then_some(head) })
}

fn source_entry(h: &mut Sha256, root: &Path, rel: &str, tracked: bool) -> Result<()> {
    let abs = root.join(rel);
    if !abs.starts_with(root) || abs == root {
        return Err(format!("Source fingerprint path escaped the repository: {rel}"));
    }
    field(h, "path", rel.as_bytes());
    let meta = match std::fs::symlink_metadata(&abs) {
        Ok(m) => m,
        Err(e) if tracked && e.kind() == std::io::ErrorKind::NotFound => {
            field(h, "kind", b"tracked-missing");
            return Ok(());
        }
        Err(_) => return Err(format!("Unable to inspect source path {rel}.")),
    };
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if meta.is_file() {
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true);
        #[cfg(unix)]
        opts.custom_flags(libc_nofollow());
        let mut f = opts.open(&abs).map_err(|_| format!("Unable to safely open source path {rel}."))?;
        let opened = f.metadata().map_err(|_| format!("Unable to safely open source path {rel}."))?;
        #[cfg(unix)]
        let same = opened.is_file() && opened.dev() == meta.dev() && opened.ino() == meta.ino();
        #[cfg(not(unix))]
        let same = opened.is_file();
        if !same {
            return Err(format!("Source path changed while fingerprinting {rel}."));
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut bytes).map_err(|_| format!("Unable to safely open source path {rel}."))?;
        #[cfg(unix)]
        let exec = opened.mode() & 0o111 != 0;
        #[cfg(not(unix))]
        let exec = false;
        field(h, "executable", if exec { b"yes" } else { b"no" });
        field(h, "kind", b"file");
        field(h, "bytes", &bytes);
        return Ok(());
    }
    #[cfg(unix)]
    let exec = meta.mode() & 0o111 != 0;
    #[cfg(not(unix))]
    let exec = false;
    field(h, "executable", if exec { b"yes" } else { b"no" });
    if meta.file_type().is_symlink() {
        field(h, "kind", b"symlink");
        let target = std::fs::read_link(&abs).map_err(|_| format!("Unable to inspect source path {rel}."))?;
        #[cfg(unix)]
        let t = std::os::unix::ffi::OsStrExt::as_bytes(target.as_os_str()).to_vec();
        #[cfg(not(unix))]
        let t = target.to_string_lossy().into_owned().into_bytes();
        field(h, "target", &t);
        return Ok(());
    }
    if meta.is_dir() {
        field(h, "kind", b"directory");
        return Ok(());
    }
    Err(format!("Unsupported source entry type at {rel}."))
}

#[cfg(target_os = "linux")]
fn libc_nofollow() -> i32 {
    0o400000
}
#[cfg(all(unix, not(target_os = "linux")))]
fn libc_nofollow() -> i32 {
    0x0100
}

// ---- Whether an update has anything to do ----

pub enum Noop {
    Skip { git_head: String, model: String, language: Option<String> },
    Run(String),
}

static STATUS_LINE: std::sync::LazyLock<crate::jsre::Re> = std::sync::LazyLock::new(|| crate::jsre::Re::new(r"^[ !?ACDMRTU]{1,2} (.+)$", "u"));
static SHORT_STATUS: std::sync::LazyLock<crate::jsre::Re> = std::sync::LazyLock::new(|| crate::jsre::Re::new(r"^(?:[ MARCUD?!]{2})\s+(.+)$", "u"));
static NAME_STATUS: std::sync::LazyLock<crate::jsre::Re> = std::sync::LazyLock::new(|| crate::jsre::Re::new(r"^(?:[ACDMRTUXB]\d*)\s+(.+)$", "u"));

fn is_metadata_status_line(line: &str) -> bool {
    let p = STATUS_LINE.exec(line).and_then(|m| m.get(1).map(str::to_owned)).unwrap_or_else(|| line.to_owned());
    let p = crate::jsre::trim(&p).replace('\\', "/");
    [PAGE_MANIFEST_PATH, UPDATE_METADATA_PATH].iter().any(|m| p == *m || p.ends_with(&format!(" -> {m}")))
}

fn references_ignored(line: &str, ignore: &Ignore) -> bool {
    let text = SHORT_STATUS.exec(line).and_then(|m| m.get(1).map(str::to_owned)).or_else(|| NAME_STATUS.exec(crate::jsre::trim(line)).and_then(|m| m.get(1).map(str::to_owned)));
    let Some(text) = text else { return false };
    let parts: Vec<&str> = if text.contains('\t') {
        text.split('\t').collect()
    } else if text.contains(" -> ") {
        text.split(" -> ").collect()
    } else {
        vec![text.as_str()]
    };
    parts.into_iter().map(normalize_git_path).filter(|p| !p.is_empty()).any(|p: String| ignore.ignores(&p, false))
}

/// Whether an update can be skipped: the last one complete at a commit,
/// the same language, a clean work tree, and no commits since but the
/// wiki's own or ignored paths.
pub fn update_noop(root: &Path, ignore: &Ignore, requested_language: Option<&str>) -> Result<Noop> {
    let Some(last) = read_last_update(root)? else { return Ok(Noop::Run("missing previous update git head".into())) };
    let Some(last_head) = last.git_head.clone() else { return Ok(Noop::Run("missing previous update git head".into())) };
    if last.status == "interrupted" {
        return Ok(Noop::Run("previous update was interrupted".into()));
    }
    if let Some(req) = requested_language {
        if crate::lang::primary(Some(req)) != crate::lang::primary(last.language.as_deref()) {
            return Ok(Noop::Run("output language changed".into()));
        }
    }
    let Some(head) = git_head(root) else { return Ok(Noop::Run("missing current git head".into())) };
    let status = git_text(root, &["status", "--short", "--untracked-files=all"]);
    let meaningful = status.split('\n').map(|l| l.trim_end()).filter(|l| !l.is_empty()).filter(|l| !is_metadata_status_line(l)).any(|l| !references_ignored(l, ignore));
    if meaningful {
        return Ok(Noop::Run("worktree has changes".into()));
    }
    if head != last_head {
        let diff = git_text(root, &["diff", "--name-only", &format!("{last_head}..HEAD")]);
        let paths: Vec<String> = diff.split('\n').map(normalize_git_path).filter(|p| !p.is_empty()).collect();
        if paths.is_empty() || paths.iter().any(|p| !is_wiki_path(p) && !ignore.ignores(p, false)) {
            return Ok(Noop::Run("git head changed".into()));
        }
    }
    Ok(Noop::Skip { git_head: head, model: last.model, language: last.language })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_collapse() {
        assert_eq!(collapse_blank_lines("a\n\n\n\nb\nc\n\nd"), "a\n\nb\nc\n\nd");
    }
}
