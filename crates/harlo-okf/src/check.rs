//! The validator: what's wrong with the bundle's files, all at once.
//!
//! **Errors** break OKF v0.2 conformance (§11) or Harlo's footnotes, and a
//! page with one isn't stamped:
//!
//! - a page without readable front matter, or without `type`;
//! - an `index.md` with front matter (other than the root's `okf_version`),
//!   a `log.md` date heading that isn't `YYYY-MM-DD`;
//! - a footnote used and never defined, defined twice, or whose definition
//!   doesn't start with a repository path (or a web address);
//! - a `covers` path leaving the repository.
//!
//! **Warnings** are worth fixing but don't stop anything: no title or
//! description (the indexes need them), an unknown `status`, a timestamp
//! without a UTC offset, a footnote nothing refers to, and a page whose
//! paragraphs mostly cite nothing.
//!
//! **Notes** are what OKF says consumers must tolerate: links to pages or
//! files that aren't there, and indexes that the next stamp rewrites.

// Rust guideline compliant 2026-10-09

use crate::bundle::{self, Bundle};
use crate::page::{self, Page};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};
use std::path::Path;

/// How much a problem matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warning,
    Note,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Note => "note",
        })
    }
}

/// A problem in a bundle file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub level: Level,
    /// The file, as a path in the bundle (`architecture/wiki.md`).
    pub file: String,
    /// Its line, from 1, when there is one.
    pub line: Option<usize>,
    pub message: String,
}

/// The `status` values OKF defines.
const STATUSES: [&str; 3] = ["draft", "stable", "deprecated"];

/// A page whose paragraphs cite less than this share is mostly unsourced.
const CITED_SHARE: f64 = 0.34;
/// Pages with fewer paragraphs than this aren't judged on it.
const MIN_PARAGRAPHS: usize = 4;

/// Every problem in the bundle, page by page.
pub fn check(bundle: &Bundle) -> Vec<Problem> {
    let mut out = Vec::new();
    let paths: BTreeSet<&str> = bundle.pages.iter().map(|p| p.path.as_str()).collect();
    for p in &bundle.pages {
        page_problems(p, &bundle.root, &paths, &mut out);
    }
    reserved(&bundle.dir(), "", &mut out);
    let wanted = bundle::indexes(&bundle.pages);
    for (path, text) in &wanted {
        let now = std::fs::read_to_string(bundle.dir().join(path)).unwrap_or_default();
        if now != *text {
            let what = if now.is_empty() { "is missing" } else { "is out of date" };
            out.push(Problem { level: Level::Note, file: path.clone(), line: None, message: format!("{path} {what}; the next stamp writes it") });
        }
    }
    out.sort_by(|a, b| (a.level, &a.file, a.line).cmp(&(b.level, &b.file, b.line)));
    out
}

fn push(out: &mut Vec<Problem>, level: Level, page: &Page, line: Option<usize>, message: String) {
    out.push(Problem { level, file: page.path.clone(), line, message });
}

fn page_problems(p: &Page, root: &Path, pages: &BTreeSet<&str>, out: &mut Vec<Problem>) {
    if !p.has_front {
        push(out, Level::Error, p, Some(1), "no front matter: start the page with `---`, `type: …`, `---`".into());
    } else if let Some(problem) = &p.front.problem {
        push(out, Level::Error, p, Some(2), problem.clone());
    } else {
        front_problems(p, out);
    }
    footnote_problems(p, out);
    if p.has_front && p.front.problem.is_none() {
        cited_share(p, out);
    }
    for (target, line) in &p.links {
        if let Some(missing) = broken_link(p, root, pages, target) {
            push(out, Level::Note, p, Some(*line), missing);
        }
    }
}

fn front_problems(p: &Page, out: &mut Vec<Problem>) {
    let f = &p.front;
    if f.kind.as_deref().is_none_or(|k| k.trim().is_empty()) {
        push(out, Level::Error, p, Some(2), "no `type` (Architecture, Workflow, Decision, …)".into());
    }
    if f.title.as_deref().is_none_or(|t| t.trim().is_empty()) {
        push(out, Level::Warning, p, Some(2), "no `title`".into());
    }
    if f.description.as_deref().is_none_or(|d| d.trim().is_empty()) {
        push(out, Level::Warning, p, Some(2), "no `description`: the index shows it beside the title".into());
    }
    if let Some(s) = &f.status
        && !STATUSES.contains(&s.as_str())
    {
        push(out, Level::Warning, p, Some(2), format!("`status: {s}` isn't draft, stable or deprecated"));
    }
    for c in &f.covers {
        if c.split('/').any(|part| part == "..") {
            push(out, Level::Error, p, Some(2), format!("covers {c}, outside the repository"));
        }
    }
    let stamps = f.generated.iter().chain(&f.verified);
    for s in stamps {
        if !s.at.is_empty() && !timestamp(&s.at) {
            push(out, Level::Warning, p, Some(2), format!("`{}` isn't a date and time with a UTC offset (2026-11-17T10:00:00Z)", s.at));
        }
    }
}

/// Whether `s` is an ISO 8601 date and time with an offset.
fn timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let shape = digits(0..4) && b.get(4) == Some(&b'-') && digits(5..7) && b.get(7) == Some(&b'-') && digits(8..10) && b.get(10) == Some(&b'T') && digits(11..13) && b.get(13) == Some(&b':') && digits(14..16);
    shape && (s.ends_with('Z') || s[16..].contains(['+', '-']))
}

fn footnote_problems(p: &Page, out: &mut Vec<Problem>) {
    let mut defined: BTreeMap<&str, usize> = BTreeMap::new();
    for c in &p.cites {
        if let Some(first) = defined.insert(&c.label, c.line) {
            push(out, Level::Error, p, Some(c.line), format!("[^{}] is defined twice (first on line {first})", c.label));
        }
        let web = c.path.starts_with("http://") || c.path.starts_with("https://");
        if !web && (c.path.is_empty() || c.path.contains(char::is_whitespace) || c.path.split('/').any(|part| part == "..")) {
            push(out, Level::Error, p, Some(c.line), format!("[^{}] should start with a repository path: `[^{}]: src/file.rs, fn name`", c.label, c.label));
        }
    }
    let used: BTreeSet<&str> = p.refs.iter().map(|(l, _)| l.as_str()).collect();
    for (label, line) in &p.refs {
        if !defined.contains_key(label.as_str()) {
            push(out, Level::Error, p, Some(*line), format!("[^{label}] has no definition"));
        }
    }
    for c in &p.cites {
        if !used.contains(c.label.as_str()) {
            push(out, Level::Warning, p, Some(c.line), format!("[^{}] is defined but nothing refers to it", c.label));
        }
    }
}

/// Warns when most of a page's paragraphs and list items cite nothing.
fn cited_share(p: &Page, out: &mut Vec<Problem>) {
    let mut blocks = 0usize;
    let mut cited = 0usize;
    let mut fence = false;
    let mut in_block = false;
    let mut block_cited = false;
    let mut close = |in_block: &mut bool, block_cited: bool| {
        if *in_block {
            blocks += 1;
            cited += usize::from(block_cited);
            *in_block = false;
        }
    };
    for line in p.body.lines() {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            close(&mut in_block, block_cited);
            fence = !fence;
            continue;
        }
        if fence || t.starts_with("[^") || t.starts_with('#') || t.starts_with('|') || t.starts_with('>') || t.is_empty() {
            close(&mut in_block, block_cited);
            continue;
        }
        let item = t.starts_with("- ") || t.starts_with("* ") || t.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if item {
            close(&mut in_block, block_cited);
        }
        if !in_block {
            in_block = true;
            block_cited = false;
        }
        block_cited |= t.contains("[^");
    }
    close(&mut in_block, block_cited);
    #[expect(clippy::cast_precision_loss, reason = "counts of paragraphs")]
    let share = if blocks == 0 { 1.0 } else { cited as f64 / blocks as f64 };
    if blocks >= MIN_PARAGRAPHS && share < CITED_SHARE {
        push(out, Level::Warning, p, None, format!("{} of {blocks} paragraphs cite no code: footnote the claims, or cut what the code says by itself", blocks - cited));
    }
}

/// What's missing when a link's target isn't there, if it isn't.
///
/// A link is read from the page's folder (or the bundle's root, for `/…`),
/// and may lead out of `.okf/` into the repository (`../src/run.rs`).
fn broken_link(p: &Page, root: &Path, pages: &BTreeSet<&str>, target: &str) -> Option<String> {
    let target = target.trim_matches(['<', '>']);
    if target.is_empty() || target.starts_with('#') || target.contains("://") || target.starts_with("mailto:") {
        return None;
    }
    let path = target.split(['#', '?']).next().unwrap_or(target);
    let folder = p.path.rsplit_once('/').map_or("", |(f, _)| f);
    let from_repo = match path.strip_prefix('/') {
        Some(abs) => format!("{}/{abs}", bundle::DIR),
        None => format!("{}/{folder}/{path}", bundle::DIR),
    };
    let found = page::normalize(&from_repo).is_some_and(|n| n.strip_prefix(&format!("{}/", bundle::DIR)).is_some_and(|in_bundle| pages.contains(in_bundle)) || root.join(&n).exists());
    (!found).then(|| format!("links to {target}, which isn't there"))
}

/// Checks the reserved `index.md` and `log.md` files under `dir`.
fn reserved(dir: &Path, prefix: &str, out: &mut Vec<Problem>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let rel = format!("{prefix}{name}");
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            reserved(&entry.path(), &format!("{rel}/"), out);
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
        if name == "index.md" {
            let parsed = page::parse(&rel, &text);
            let only_version = parsed.front_text.lines().all(|l| l.trim().is_empty() || l.starts_with("okf_version:"));
            if parsed.has_front && (!prefix.is_empty() || !only_version) {
                let message = if prefix.is_empty() { "the root index.md's front matter may hold only `okf_version`" } else { "index.md has front matter; only the root's may, for `okf_version`" };
                out.push(Problem { level: Level::Error, file: rel, line: Some(1), message: message.into() });
            }
        } else if name == "log.md" {
            for (n, line) in text.lines().enumerate() {
                if let Some(h) = line.strip_prefix("## ")
                    && !date(h.trim())
                {
                    out.push(Problem { level: Level::Error, file: rel.clone(), line: Some(n + 1), message: format!("log heading `{}` isn't a YYYY-MM-DD date", h.trim()) });
                }
            }
        }
    }
}

fn date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
}

/// Whether any problem stops a stamp.
pub fn has_errors(problems: &[Problem]) -> bool {
    problems.iter().any(|p| p.level == Level::Error)
}

/// The problems as text, grouped by file, errors first.
pub fn report(problems: &[Problem]) -> String {
    let mut out = String::new();
    if problems.is_empty() {
        return out;
    }
    let count = |l: Level| problems.iter().filter(|p| p.level == l).count();
    let _ = writeln!(out, "{} errors, {} warnings, {} notes.", count(Level::Error), count(Level::Warning), count(Level::Note));
    let mut by_file: BTreeMap<&str, Vec<&Problem>> = BTreeMap::new();
    for p in problems {
        by_file.entry(&p.file).or_default().push(p);
    }
    let mut files: Vec<(&str, Vec<&Problem>)> = by_file.into_iter().collect();
    files.sort_by_key(|(f, list)| (list.iter().map(|p| p.level).min(), *f));
    for (file, list) in files {
        let _ = writeln!(out, "\n{file}:");
        for p in list {
            let at = p.line.map_or(String::new(), |l| format!("line {l}: "));
            let _ = writeln!(out, "- {}: {at}{}", p.level, p.message);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_every_problem() {
        let dir = std::env::temp_dir().join(format!("harlo-okf-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |p: &str, t: &str| {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        };
        write("src/run.rs", "pub fn run() {}\n");
        write(".okf/good.md", "---\ntype: Concept\ntitle: Good\ndescription: Fine.\ngenerated: { by: harlo/1, at: 2026-11-17T10:00:00Z }\n---\n# Good\n\nRuns.[^run] See [bad](/bad.md) and [the code](../src/run.rs).\n\n[^run]: src/run.rs, fn run\n");
        write(".okf/bad.md", "---\ntitle: Bad\nstatus: final\ncovers: [../etc]\nverified: { by: human:x, at: 2026-11-17 }\n---\nOne.[^a]\n\nTwo.[^nope]\n\nThree.\n\nFour.\n\n- Five.\n- Six.\n\nSee [gone](gone.md).\n\n[^a]: src/run.rs, fn run\n[^a]: the run function\n[^unused]: src/run.rs\n");
        write(".okf/none.md", "# No front matter\n");
        write(".okf/sub/index.md", "---\ntitle: x\n---\n# Sub\n");
        write(".okf/log.md", "# Log\n\n## Yesterday\n\n## 2026-11-17\n");
        let problems = check(&bundle::load(&dir).unwrap());
        let text = report(&problems);
        std::fs::remove_dir_all(&dir).unwrap();
        let has = |level: Level, file: &str, line: Option<usize>, words: &str| {
            assert!(problems.iter().any(|p| p.level == level && p.file == file && p.line == line && p.message.contains(words)), "{level} {file} {line:?} {words}:\n{text}");
        };
        has(Level::Error, "bad.md", Some(2), "no `type`");
        has(Level::Error, "bad.md", Some(2), "outside the repository");
        has(Level::Error, "bad.md", Some(9), "[^nope] has no definition");
        has(Level::Error, "bad.md", Some(21), "defined twice");
        has(Level::Error, "bad.md", Some(21), "should start with a repository path");
        has(Level::Warning, "bad.md", Some(2), "isn't draft, stable or deprecated");
        has(Level::Warning, "bad.md", Some(2), "UTC offset");
        has(Level::Warning, "bad.md", Some(22), "nothing refers to it");
        has(Level::Warning, "bad.md", None, "5 of 7 paragraphs cite no code");
        has(Level::Note, "bad.md", Some(18), "links to gone.md");
        has(Level::Error, "none.md", Some(1), "no front matter");
        has(Level::Error, "sub/index.md", Some(1), "only the root's may");
        has(Level::Error, "log.md", Some(3), "`Yesterday`");
        has(Level::Note, "index.md", None, "is missing");
        assert!(!problems.iter().any(|p| p.file == "good.md"), "{text}");
        assert!(has_errors(&problems) && text.starts_with("8 errors, 5 warnings,"), "{text}");
    }

    #[test]
    fn timestamps_and_dates() {
        assert!(timestamp("2026-11-17T10:00:00Z") && timestamp("2026-11-17T10:00:00+02:00") && timestamp("2026-11-17T10:00:00.5-05:00"));
        assert!(!timestamp("2026-11-17") && !timestamp("2026-11-17T10:00:00"));
        assert!(date("2026-11-17") && !date("2026-1-17"));
    }
}
