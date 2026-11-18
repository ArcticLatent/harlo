//! One check of the wiki: validate, list the work, stamp what's ready.
//!
//! [`run`] is what a wiki session's `wiki_check` and Harlo's own checks do.
//! It takes the writer's decisions first (footnotes that still hold after
//! their code changed, code to leave out), then:
//!
//! - validates every page ([`check`]) and works out what the wiki owes the
//!   code ([`work`]);
//! - with a writer, stamps each page edited since its last stamp that has
//!   no errors and no broken footnotes: its stale claims count as reviewed,
//!   and Harlo writes its `generated` and `sources` keys;
//! - records the rest in the ledger, fixes footnotes whose code moved, and
//!   writes the indexes and the log.
//!
//! Without a writer (no model), nothing is stamped: edits wait for a
//! session or for someone marking the page reviewed.

// Rust guideline compliant 2026-10-09

use crate::bundle::{self, Bundle, Event};
use crate::check::{self, Level, Problem};
use crate::ledger::{self, Ledger};
use crate::page::{self, Page};
use crate::units;
use crate::work::{self, Item, Work};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io;
use std::path::Path;

/// What a check is asked to do.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Who wrote the edited pages, as an OKF actor (`claude/claude-opus-5-5`,
    /// `human:ada`); None stamps nothing.
    pub writer: Option<String>,
    /// Now, as an ISO 8601 date and time in UTC.
    pub now: String,
    /// Footnotes (`page.md#label`) that still hold though their code changed.
    pub confirm: Vec<String>,
    /// Code to leave out (`path`, `folder/`, `path#Declaration`), with why.
    pub leave: Vec<(String, String)>,
    /// Who read `reviewed` against their code, as an OKF actor (`human:ada`).
    pub verifier: Option<String>,
    /// Pages the verifier marks reviewed, edited or not.
    pub reviewed: Vec<String>,
}

/// What a check found and did.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Problems in the files, after stamping.
    pub problems: Vec<Problem>,
    pub work: Work,
    /// Pages stamped now.
    pub stamped: Vec<String>,
    /// Pages marked reviewed now.
    pub marked: Vec<String>,
    /// Edited pages not stamped, with why.
    pub held: Vec<(String, String)>,
    /// What of `confirm` and `leave` couldn't be taken, and why.
    pub refused: Vec<String>,
}

impl Outcome {
    /// Whether nothing is left to do: no work, and no errors.
    pub fn is_current(&self) -> bool {
        self.work.is_empty() && !check::has_errors(&self.problems)
    }

    /// The outcome as text, for the writer and the panel.
    pub fn report(&self) -> String {
        let mut out = String::new();
        if !self.stamped.is_empty() {
            let _ = writeln!(out, "Stamped: {}.", self.stamped.join(", "));
        }
        if !self.marked.is_empty() {
            let _ = writeln!(out, "Marked reviewed: {}.", self.marked.join(", "));
        }
        if !self.held.is_empty() {
            let list: Vec<String> = self.held.iter().map(|(p, why)| format!("{p} ({why})")).collect();
            let _ = writeln!(out, "Not stamped yet: {}.", list.join("; "));
        }
        for r in &self.refused {
            let _ = writeln!(out, "Not taken: {r}");
        }
        if !out.is_empty() {
            out.push('\n');
        }
        let problems = check::report(&self.problems);
        if !problems.is_empty() {
            out.push_str(&problems);
            out.push('\n');
        }
        out.push_str(&self.work.report());
        out
    }
}

/// Records the declarations and files `options` leaves out, with their reasons.
///
/// A key without a reason, or naming nothing in the repository, is refused.
fn leave(root: &Path, options: &Options, ledger: &mut Ledger, refused: &mut Vec<String>) {
    for (key, why) in &options.leave {
        let key = page::clean_path(key);
        let path = key.split_once('#').map_or(key.as_str(), |(p, _)| p);
        if why.trim().is_empty() {
            refused.push(format!("leaving out {key} needs a reason"));
        } else if path.is_empty() || !root.join(path).exists() {
            refused.push(format!("{path} isn't in the repository, so there's nothing to leave out"));
        } else {
            ledger.leave.insert(key, why.trim().to_owned());
        }
    }
}

/// Writes the bundle's indexes where they changed, and logs `events`.
///
/// # Errors
///
/// When a file can't be written.
fn write_indexes(bundle: &Bundle, events: &[(Event, String, String)], now: &str) -> io::Result<()> {
    if !bundle.pages.is_empty() {
        for (path, text) in bundle::indexes(&bundle.pages) {
            let file = bundle.dir().join(&path);
            if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
                if let Some(dir) = file.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(file, text)?;
            }
        }
    }
    if !events.is_empty() {
        let file = bundle.dir().join("log.md");
        let existing = std::fs::read_to_string(&file).unwrap_or_default();
        std::fs::write(file, bundle::log(&existing, now.get(..10).unwrap_or(now), events))?;
    }
    Ok(())
}

/// Runs a check of the wiki in the repository at `root`.
///
/// # Errors
///
/// When the bundle or the ledger can't be read, or a file can't be written.
pub fn run(root: &Path, options: &Options) -> io::Result<Outcome> {
    let mut ledger = ledger::read(root)?;
    let before = serde_json::to_string(&ledger).unwrap_or_default();
    let mut out = Outcome::default();
    leave(root, options, &mut ledger, &mut out.refused);
    let bundle = bundle::load(root)?;
    let problems = check::check(&bundle);
    let mut work = work::work(&bundle, &ledger);
    for c in &options.confirm {
        let (page, label) = c.rsplit_once('#').map_or((c.as_str(), ""), |(p, l)| (p, l.trim_start_matches("[^").trim_end_matches(']')));
        let page = page.strip_prefix(".okf/").unwrap_or(page);
        if label.is_empty() || !work.accept(page, Some(label)) {
            out.refused.push(format!("{c} isn't a stale footnote (write page.md#label)"));
        }
    }

    // The pages to stamp: edited (or marked reviewed) and ready.
    let mut ready = Vec::new();
    if options.writer.is_some() {
        for p in &bundle.pages {
            let text = std::fs::read_to_string(bundle.dir().join(&p.path))?;
            if ledger.pages.get(&p.path) == Some(&units::digest(text.as_bytes())) {
                continue;
            }
            match blocker(p, &problems, &work) {
                Some(why) => out.held.push((p.path.clone(), why)),
                None => {
                    work.accept(&p.path, None);
                    ready.push(p.path.clone());
                }
            }
        }
    }
    // The pages marked reviewed: their stale claims count as read.
    let mut marked = Vec::new();
    if options.verifier.is_some() {
        for path in &options.reviewed {
            let path = path.strip_prefix(".okf/").unwrap_or(path);
            let Some(p) = bundle.page(path) else {
                out.refused.push(format!("{path} isn't a page of the wiki"));
                continue;
            };
            match blocker(p, &problems, &work) {
                Some(why) => out.refused.push(format!("{path} can't be marked reviewed yet: {why}")),
                None => {
                    work.accept(path, None);
                    marked.push(path.to_owned());
                }
            }
        }
    }
    work.record(root, &mut ledger)?;

    let mut events = Vec::new();
    if let Some(writer) = &options.writer {
        for path in &ready {
            let file = bundle.dir().join(path);
            let p = page::parse(path, &std::fs::read_to_string(&file)?);
            let generated = format!("{{ by: {}, at: {} }}", page::scalar(writer), page::scalar(&options.now));
            let front = page::set_keys(&p.front_text, &[("generated", Some(generated)), ("sources", sources(&p))]);
            let text = page::file(&front, &p.body);
            std::fs::write(&file, &text)?;
            let title = p.front.title.clone().unwrap_or_else(|| path.clone());
            let event = if !ledger.pages.contains_key(path) {
                Event::Creation
            } else if p.front.status.as_deref() == Some("deprecated") {
                Event::Deprecation
            } else {
                Event::Update
            };
            events.push((event, title, path.clone()));
            ledger.pages.insert(path.clone(), units::digest(text.as_bytes()));
        }
    }
    if let Some(verifier) = &options.verifier {
        for path in &marked {
            let file = bundle.dir().join(path);
            let p = page::parse(path, &std::fs::read_to_string(&file)?);
            // (One entry per verifier: their latest.)
            let mut verified: Vec<page::Stamp> = p.front.verified.iter().filter(|s| s.by != *verifier).cloned().collect();
            verified.push(page::Stamp { by: verifier.clone(), at: options.now.clone() });
            let front = page::set_keys(&p.front_text, &[("verified", Some(page::stamps(&verified))), ("sources", sources(&p))]);
            let text = page::file(&front, &p.body);
            std::fs::write(&file, &text)?;
            ledger.pages.insert(path.clone(), units::digest(text.as_bytes()));
        }
    }
    let present: BTreeSet<&str> = bundle.pages.iter().map(|p| p.path.as_str()).collect();
    ledger.pages.retain(|p, _| present.contains(p.as_str()));

    // The indexes and the log, from the pages as they are now.
    let bundle = bundle::load(root)?;
    write_indexes(&bundle, &events, &options.now)?;
    if serde_json::to_string(&ledger).unwrap_or_default() != before {
        ledger::write(root, &ledger)?;
    }
    out.problems = check::check(&bundle);
    out.work = work;
    out.stamped = ready;
    out.marked = marked;
    Ok(out)
}

/// Why an edited page can't be stamped yet, if it can't.
fn blocker(p: &Page, problems: &[Problem], work: &Work) -> Option<String> {
    let errors = problems.iter().filter(|x| x.file == p.path && x.level == Level::Error).count();
    if errors > 0 {
        return Some(format!("{errors} {}", if errors == 1 { "error" } else { "errors" }));
    }
    let broken = work.pages.get(&p.path).map_or(0, |items| items.iter().filter(|i| matches!(i, Item::Unresolved { .. } | Item::Ambiguous { .. } | Item::Absent { label: Some(_), .. })).count());
    (broken > 0).then(|| format!("{broken} {} no code", if broken == 1 { "footnote finds" } else { "footnotes find" }))
}

/// A page's `sources`, from its covers and footnotes (None: it has none).
fn sources(p: &Page) -> Option<String> {
    let resource = |path: &str| if path.contains("://") { path.to_owned() } else { format!("repo://{path}") };
    let mut out = String::new();
    for c in &p.cites {
        let _ = write!(out, "\n  - id: {}\n    resource: {}", page::scalar(&c.label), page::scalar(&resource(&c.path)));
        if let Some(l) = &c.locator {
            let _ = write!(out, "\n    title: {}", page::scalar(l));
        }
    }
    let cited: BTreeSet<&str> = p.cites.iter().map(|c| c.path.as_str()).collect();
    for c in p.front.covers.iter().filter(|c| !cited.contains(c.as_str())) {
        let _ = write!(out, "\n  - resource: {}", page::scalar(&resource(c)));
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let f = root.join(path);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, text).unwrap();
    }

    #[test]
    fn stamps_edited_pages_and_holds_broken_ones() {
        let root = std::env::temp_dir().join(format!("harlo-okf-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write(&root, "src/run.rs", "pub fn run() {\n    go();\n}\npub fn stop() {}\n");
        write(&root, ".okf/run.md", "---\ntype: Concept\ntitle: Runs\ndescription: How a run goes.\ncovers: [src/]\n---\n# Runs\n\nIt goes.[^run]\n\n[^run]: src/run.rs, fn run\n");
        write(&root, ".okf/bad.md", "---\ntype: Concept\ntitle: Bad\ndescription: Broken.\n---\nGone.[^x]\n\n[^x]: src/run.rs, fn gone\n");
        // Harlo records the tree before a session starts: nothing is stamped.
        let first = run(&root, &Options::default()).unwrap();
        assert!(first.stamped.is_empty());
        let writer = Options { writer: Some("claude/claude-opus-5-5".into()), now: "2026-11-17T10:00:00Z".into(), leave: vec![("src/run.rs#stop".into(), "a plain switch".into())], ..Options::default() };
        let o = run(&root, &writer).unwrap();
        assert_eq!(o.stamped, ["run.md"], "{}", o.report());
        assert_eq!(o.held, [("bad.md".to_owned(), "1 footnote finds no code".to_owned())]);
        let page = std::fs::read_to_string(root.join(".okf/run.md")).unwrap();
        assert!(page.contains("generated: { by: claude/claude-opus-5-5, at: \"2026-11-17T10:00:00Z\" }\nsources:\n  - id: run\n    resource: \"repo://src/run.rs\"\n    title: fn run\n  - resource: \"repo://src/\"\n---\n"), "{page}");
        let log = std::fs::read_to_string(root.join(".okf/log.md")).unwrap();
        assert_eq!(log, "# Log\n\n## 2026-11-17\n\n* **Creation**: [Runs](/run.md)\n");
        assert!(std::fs::read_to_string(root.join(".okf/index.md")).unwrap().contains("* [Runs](run.md) - How a run goes."));

        // Nothing edited: a second check stamps nothing and changes nothing.
        let again = run(&root, &writer).unwrap();
        assert!(again.stamped.is_empty() && std::fs::read_to_string(root.join(".okf/run.md")).unwrap() == page);

        // The code changes: stale until the writer confirms it still holds.
        write(&root, "src/run.rs", "pub fn run() {\n    go_faster();\n}\npub fn stop() {}\n");
        let stale = run(&root, &Options { now: "2026-11-18T10:00:00Z".into(), ..Options::default() }).unwrap();
        assert!(matches!(stale.work.pages["run.md"][..], [Item::Stale { .. }]), "{}", stale.report());
        let confirmed = run(&root, &Options { writer: writer.writer.clone(), confirm: vec!["run.md#run".into()], ..Options::default() }).unwrap();
        assert!(!confirmed.work.pages.contains_key("run.md") && confirmed.refused.is_empty(), "{}", confirmed.report());

        // Changed again, then read by a person: marked reviewed, its writer kept.
        write(&root, "src/run.rs", "pub fn run() {\n    go_slower();\n}\npub fn stop() {}\n");
        let reviewer = Options { verifier: Some("human:ada".into()), now: "2026-11-19T10:00:00Z".into(), reviewed: vec!["run.md".into(), "bad.md".into()], ..Options::default() };
        let marked = run(&root, &reviewer).unwrap();
        assert_eq!(marked.marked, ["run.md"]);
        assert_eq!(marked.refused, ["bad.md can't be marked reviewed yet: 1 footnote finds no code"]);
        assert!(!marked.work.pages.contains_key("run.md"), "{}", marked.report());
        let page = std::fs::read_to_string(root.join(".okf/run.md")).unwrap();
        assert!(page.contains("generated: { by: claude/claude-opus-5-5,") && page.contains("verified:\n  - by: \"human:ada\"\n    at: \"2026-11-19T10:00:00Z\"\n"), "{page}");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
