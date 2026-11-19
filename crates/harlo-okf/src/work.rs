//! The work list: what the wiki owes the code as it is on disk now.
//!
//! [`work`] compares the bundle and its ledger with the working tree,
//! uncommitted code included, and lists:
//!
//! - **unowned files**: code files no page covers or cites, and not left out;
//! - **new declarations**: notable declarations (see [`ledger::notable`])
//!   that weren't in their file when the wiki last looked, cited by no page
//!   and not left out, each given to the page that covers its file;
//! - **stale claims**: footnotes whose declaration (or whole file) changed
//!   since the page was stamped;
//! - **unresolved and ambiguous claims**: footnotes naming nothing in their
//!   file, or more than one thing;
//! - **absent paths**: covered or cited files and folders that aren't there.
//!
//! A declaration that moved (to another file, or under a renamed type) with
//! its own text unchanged is found by its short name and hash, when exactly
//! one declaration fits; its footnote is rewritten by [`Work::record`]
//! rather than listed. An empty list means the wiki is current, and
//! [`Work::record`] records the new state with no model.

// Rust guideline compliant 2026-10-09

use crate::bundle::Bundle;
use crate::cite::{self, Found};
use crate::indexed::Declarations;
use crate::ledger::{self, Claim, Ledger, Seen};
use crate::page::Page;
use crate::scope;
use crate::split;
use crate::units::{self, Kind, Unit};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io;
use std::path::Path;

/// Something a page owes the code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// A notable declaration added since the wiki last looked at its file.
    New { path: String, name: String, kind: Kind, line: usize },
    /// A claim whose code changed since the page was stamped.
    Stale { label: String, path: String, locator: Option<String>, line: usize, lines: Option<(usize, usize)> },
    /// A claim naming nothing in its file.
    Unresolved { label: String, path: String, locator: String, line: usize },
    /// A claim naming several declarations.
    Ambiguous { label: String, path: String, locator: String, line: usize, candidates: Vec<String> },
    /// A covered (no label) or cited path that isn't there.
    Absent { path: String, label: Option<String>, line: Option<usize> },
}

/// A footnote whose declaration moved, and where to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub page: String,
    pub label: String,
    /// The footnote's line in the page, from 1.
    pub line: usize,
    pub from: String,
    pub path: String,
    /// The new locator (`fn Book::post`).
    pub locator: String,
}

/// The work list.
#[derive(Clone, Debug, Default)]
pub struct Work {
    /// Code files no page answers for.
    pub unowned: Vec<String>,
    /// Those files grouped into pages-to-be, by folder.
    pub groups: Vec<split::Group>,
    /// Page → what it owes, in the page's order.
    pub pages: BTreeMap<String, Vec<Item>>,
    /// Footnotes fixed by themselves when the list is recorded.
    pub moved: Vec<Moved>,
    /// The claims to record: current ones, and the old ones of claims with work.
    claims: BTreeMap<String, BTreeMap<String, Claim>>,
    /// The file snapshots to record.
    files: BTreeMap<String, Seen>,
    /// Stale claims as they are now: (page, label) → claim, recorded once accepted.
    current: BTreeMap<(String, String), Claim>,
}

impl Work {
    /// Whether the wiki is current (moves are fixed without a model).
    pub fn is_empty(&self) -> bool {
        self.unowned.is_empty() && self.pages.values().all(Vec::is_empty)
    }

    /// The number of things to do.
    pub fn len(&self) -> usize {
        self.unowned.len() + self.pages.values().map(Vec::len).sum::<usize>()
    }

    /// Takes a page's stale claims as reviewed: `label` alone, or all of
    /// them. Returns whether there was one to take.
    pub fn accept(&mut self, page: &str, label: Option<&str>) -> bool {
        let keys: Vec<(String, String)> = self.current.keys().filter(|(p, l)| p == page && label.is_none_or(|x| x == l)).cloned().collect();
        for key in &keys {
            if let Some(claim) = self.current.remove(key) {
                self.claims.entry(key.0.clone()).or_default().insert(key.1.clone(), claim);
            }
        }
        if let Some(items) = self.pages.get_mut(page) {
            items.retain(|i| !matches!(i, Item::Stale { label: l, .. } if keys.iter().any(|(_, k)| k == l)));
            if items.is_empty() {
                self.pages.remove(page);
            }
        }
        !keys.is_empty()
    }

    /// Fixes moved footnotes in their pages and records the current state.
    ///
    /// Takes the ledger's stamped page hashes along with a page's fix.
    ///
    /// Claims with work keep what they were stamped with, so they stay on
    /// the list until a page deals with them; files with new declarations
    /// keep their old snapshot for the same reason. Returns whether the
    /// ledger changed.
    ///
    /// # Errors
    ///
    /// When a page can't be read or written.
    pub fn record(&self, root: &Path, ledger: &mut Ledger) -> io::Result<bool> {
        let mut by_page: BTreeMap<&str, Vec<&Moved>> = BTreeMap::new();
        for m in &self.moved {
            by_page.entry(&m.page).or_default().push(m);
        }
        for (page, moves) in by_page {
            let file = root.join(crate::bundle::DIR).join(page);
            let text = std::fs::read_to_string(&file)?;
            let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
            for m in moves {
                if let Some(l) = lines.get_mut(m.line - 1) {
                    let indent = &l[..l.len() - l.trim_start().len()];
                    *l = format!("{indent}[^{}]: {}, {}", m.label, m.path, m.locator);
                }
            }
            let fixed = lines.join("\n");
            std::fs::write(&file, &fixed)?;
            // (Harlo's own fix: a page stamped as it was stays stamped.)
            if ledger.pages.get(page).is_some_and(|h| *h == units::digest(text.as_bytes())) {
                ledger.pages.insert(page.to_owned(), units::digest(fixed.as_bytes()));
            }
        }
        let before = (ledger.claims.clone(), ledger.files.clone());
        ledger.version = ledger.version.max(1);
        ledger.claims.clone_from(&self.claims);
        ledger.files.clone_from(&self.files);
        Ok((ledger.claims.clone(), ledger.files.clone()) != before)
    }

    /// The list as text, for a writer or the panel.
    pub fn report(&self) -> String {
        let mut out = String::new();
        if self.is_empty() {
            out.push_str("The wiki is current.\n");
        } else {
            let n = self.len();
            let _ = writeln!(out, "{n} {} to do.", if n == 1 { "thing" } else { "things" });
        }
        if self.unowned.len() > LIST_FILES && !self.groups.is_empty() {
            let _ = writeln!(out, "\n{} files no page covers, by folder (a page for each, as a start; merge or split as the code suggests, or leave some out):", self.unowned.len());
            for g in &self.groups {
                let n = g.files.len();
                let pages = if g.pages() > 1 { format!(" (about {} pages)", g.pages()) } else { String::new() };
                let _ = writeln!(out, "- {}: {n} {}, {} lines{pages}", g.name, if n == 1 { "file" } else { "files" }, g.lines);
            }
        } else if !self.unowned.is_empty() {
            out.push_str("\nFiles no page covers (add them to a page's covers, or leave them out):\n");
            for p in &self.unowned {
                let _ = writeln!(out, "- {p}");
            }
        }
        for (page, items) in self.pages.iter().filter(|(_, i)| !i.is_empty()) {
            let _ = writeln!(out, "\n{page}:");
            for item in items {
                let _ = writeln!(out, "- {}", describe(item));
            }
        }
        if !self.moved.is_empty() {
            out.push_str("\nFootnotes whose code moved (fixed by themselves):\n");
            for m in &self.moved {
                let _ = writeln!(out, "- {} [^{}]: {} → {}, {}", m.page, m.label, m.from, m.path, m.locator);
            }
        }
        out
    }
}

/// Up to this many uncovered files are listed one by one, more by folder.
const LIST_FILES: usize = 12;

impl Item {
    /// The item in words, as the report gives it.
    pub fn text(&self) -> String {
        describe(self)
    }

    /// The footnote it's about, if it's about one.
    pub fn label(&self) -> Option<&str> {
        match self {
            Item::Stale { label, .. } | Item::Unresolved { label, .. } | Item::Ambiguous { label, .. } => Some(label),
            Item::Absent { label, .. } => label.as_deref(),
            Item::New { .. } => None,
        }
    }
}

fn describe(item: &Item) -> String {
    match item {
        Item::New { path, name, kind, line } => format!("new {kind} {name} in {path}:{line}: cite it, or leave it out"),
        Item::Stale { label, path, locator, lines, .. } => {
            let what = locator.as_ref().map_or_else(|| path.clone(), |l| format!("{path}, {l}"));
            let at = lines.map_or(String::new(), |(a, b)| format!(" (lines {a}–{b})"));
            format!("[^{label}] {what}{at} changed since the page was written")
        }
        Item::Unresolved { label, path, locator, .. } => format!("[^{label}] names nothing in {path}: {locator}"),
        Item::Ambiguous { label, path, locator, candidates, .. } => format!("[^{label}] {path}, {locator} fits several: {}", candidates.join(", ")),
        Item::Absent { path, label: Some(label), .. } => format!("[^{label}] cites {path}, which isn't there"),
        Item::Absent { path, label: None, .. } => format!("covers {path}, which isn't there"),
    }
}

/// A file read from the working tree, scanned when needed.
struct Source {
    text: String,
    hash: String,
    units: Option<Vec<Unit>>,
}

/// The working tree's files, read once each.
struct Tree<'a> {
    root: &'a Path,
    files: HashMap<String, Option<Source>>,
    /// Where declarations come from, found on first need.
    decls: Option<Declarations>,
}

impl Tree<'_> {
    fn get(&mut self, path: &str) -> Option<&mut Source> {
        let root = self.root;
        self.files
            .entry(path.to_owned())
            .or_insert_with(|| {
                let text = std::fs::read_to_string(root.join(path)).ok()?;
                let hash = units::digest(text.as_bytes());
                Some(Source { text, hash, units: None })
            })
            .as_mut()
    }

    fn units(&mut self, path: &str) -> Option<&[Unit]> {
        if self.decls.is_none() {
            self.decls = Some(Declarations::load(self.root));
        }
        self.get(path)?;
        let Tree { files, decls, .. } = self;
        let s = files.get_mut(path)?.as_mut()?;
        if s.units.is_none() {
            s.units = Some(decls.as_ref()?.units(path, &s.text));
        }
        s.units.as_deref()
    }
}

/// A path's folders end in `/`: `covers: [src/run]` means the folder.
fn folder_aware(root: &Path, path: &str) -> String {
    if !path.ends_with('/') && !path.is_empty() && root.join(path).is_dir() { format!("{path}/") } else { path.to_owned() }
}

/// How closely `cover` holds `path`: its length when it does.
fn holds(cover: &str, path: &str) -> Option<usize> {
    (cover == path || (cover.ends_with('/') && path.starts_with(cover)) || cover.is_empty()).then_some(cover.len())
}

/// The short name of a full one (`post` for `Ledger::post`).
fn short(name: &str) -> &str {
    let after = name.rsplit_once("::").map_or(name, |(_, s)| s);
    after.rsplit_once('.').map_or(after, |(_, s)| s)
}

/// Whether a claim stamped from an unchanged file still holds: it was
/// checked with this very locator. (One stamped before locators were kept
/// is checked against the code again, and keeps its locator from then on.)
fn same_claim(w: &Claim, locator: &str) -> bool {
    !w.locator.is_empty() && w.locator == locator.trim()
}

/// Who answers for which paths: each live page's covers and cited paths.
struct Owners<'a> {
    covers: Vec<(&'a str, Vec<String>)>,
    cited: Vec<(&'a str, Vec<String>)>,
}

impl<'a> Owners<'a> {
    fn new(root: &Path, live: &[&'a Page]) -> Owners<'a> {
        Owners {
            covers: live.iter().map(|p| (p.path.as_str(), p.front.covers.iter().map(|c| folder_aware(root, c)).collect())).collect(),
            cited: live.iter().map(|p| (p.path.as_str(), p.cites.iter().map(|c| folder_aware(root, &c.path)).collect())).collect(),
        }
    }

    /// The page answering for `path`: the closest cover, else the closest cite.
    fn of(&self, path: &str) -> Option<&'a str> {
        for list in [&self.covers, &self.cited] {
            let mut found: Option<(usize, &str)> = None;
            for (page, paths) in list {
                for c in paths {
                    if let Some(n) = holds(c, path)
                        && found.is_none_or(|(m, _)| n > m)
                    {
                        found = Some((n, page));
                    }
                }
            }
            if let Some((_, page)) = found {
                return Some(page);
            }
        }
        None
    }
}

/// One pass over the bundle and the tree.
struct Pass<'a> {
    root: &'a Path,
    ledger: &'a Ledger,
    tree: Tree<'a>,
    out: Work,
    /// (path, declaration) pairs some footnote names.
    cited: BTreeSet<(String, String)>,
    /// Footnotes naming nothing that had a recorded declaration: (page, cite index, claim).
    missing: Vec<(String, usize, Claim)>,
}

/// What one page, `path`, owes the code: its own footnotes only.
///
/// Reads just the files the page cites, so it's quick enough for every read
/// of the page. A footnote whose declaration is gone from its file counts
/// as naming nothing here (the whole bundle's [`work`] may find it moved).
/// Empty for a deprecated page, one the bundle hasn't got, or one never
/// stamped (nothing to compare with).
#[must_use]
pub fn page_items(bundle: &Bundle, ledger: &Ledger, path: &str) -> Vec<Item> {
    let Some(page) = bundle.pages.iter().find(|p| p.path == path && p.front.status.as_deref() != Some("deprecated")) else {
        return Vec::new();
    };
    if !ledger.claims.contains_key(path) {
        return Vec::new();
    }
    let root = bundle.root.as_path();
    let mut pass = Pass { root, ledger, tree: Tree { root, files: HashMap::new(), decls: None }, out: Work::default(), cited: BTreeSet::new(), missing: Vec::new() };
    pass.page(page);
    let mut items = pass.out.pages.remove(path).unwrap_or_default();
    for (_, index, claim) in pass.missing {
        if let Some(c) = page.cites.get(index) {
            let locator = c.locator.clone().unwrap_or_else(|| claim.unit.clone().unwrap_or_default());
            items.push(Item::Unresolved { label: c.label.clone(), path: c.path.clone(), locator, line: c.line });
        }
    }
    items
}

/// What the wiki at `bundle` owes the working tree, by its `ledger`.
pub fn work(bundle: &Bundle, ledger: &Ledger) -> Work {
    let root = bundle.root.as_path();
    let live: Vec<&Page> = bundle.pages.iter().filter(|p| p.front.status.as_deref() != Some("deprecated")).collect();
    let mut pass = Pass { root, ledger, tree: Tree { root, files: HashMap::new(), decls: None }, out: Work::default(), cited: BTreeSet::new(), missing: Vec::new() };
    for page in &live {
        pass.page(page);
    }
    // Deprecated pages keep what they had.
    for p in bundle.pages.iter().filter(|p| p.front.status.as_deref() == Some("deprecated")) {
        if let Some(old) = ledger.claims.get(&p.path) {
            pass.out.claims.insert(p.path.clone(), old.clone());
        }
    }
    let in_scope: Vec<String> = scope::files(root).into_iter().filter(|p| scope::needs_owner(p)).collect();
    pass.files(&in_scope, &Owners::new(root, &live));
    pass.moves(bundle, &in_scope);
    let mut out = pass.out;
    out.pages.retain(|_, items| !items.is_empty());
    if !out.unowned.is_empty() {
        out.groups = split::propose(root, &out.unowned);
    }
    out
}

impl Pass<'_> {
    /// A live page's covers and footnotes against the code now.
    fn page(&mut self, page: &Page) {
        let root = self.root;
        let old = self.ledger.claims.get(&page.path);
        let mut items = Vec::new();
        let mut claims = BTreeMap::new();
        for cover in &page.front.covers {
            if !root.join(cover).exists() {
                items.push(Item::Absent { path: cover.clone(), label: None, line: None });
            }
        }
        for (index, c) in page.cites.iter().enumerate() {
            let was = old.and_then(|o| o.get(&c.label));
            // A footnote with work keeps what it was stamped with.
            if let Some(w) = was {
                claims.insert(c.label.clone(), w.clone());
            }
            let path = folder_aware(root, &c.path);
            if path.ends_with('/') {
                // (A folder that's there; a missing one, or a file written
                // as one, is absent.)
                if root.join(&path).is_dir() {
                    claims.insert(c.label.clone(), Claim { path, unit: None, hash: String::new(), file: String::new(), locator: String::new() });
                } else {
                    items.push(Item::Absent { path: c.path.clone(), label: Some(c.label.clone()), line: Some(c.line) });
                }
                continue;
            }
            let Some(src) = self.tree.get(&c.path) else {
                if c.locator.is_some()
                    && let Some(w) = was.filter(|w| w.unit.is_some())
                {
                    self.missing.push((page.path.clone(), index, w.clone()));
                }
                items.push(Item::Absent { path: c.path.clone(), label: Some(c.label.clone()), line: Some(c.line) });
                continue;
            };
            let file = src.hash.clone();
            let Some(locator) = &c.locator else {
                let now = Claim { path: c.path.clone(), unit: None, hash: file.clone(), file, locator: String::new() };
                if was.is_none_or(|w| w.unit.is_some() || w.path != c.path || w.hash == now.hash) {
                    claims.insert(c.label.clone(), now);
                } else {
                    self.out.current.insert((page.path.clone(), c.label.clone()), now);
                    items.push(Item::Stale { label: c.label.clone(), path: c.path.clone(), locator: None, line: c.line, lines: None });
                }
                continue;
            };
            // Nothing in the file changed: the claim is as current as it was.
            if let Some(w) = was
                && w.path == c.path
                && !w.file.is_empty()
                && w.file == file
                && let Some(name) = &w.unit
                && same_claim(w, locator)
            {
                self.cited.insert((c.path.clone(), name.clone()));
                continue;
            }
            let units = self.tree.units(&c.path).unwrap_or_default();
            match cite::locate(units, locator) {
                Found::One(i) => {
                    let u = &units[i];
                    self.cited.insert((c.path.clone(), u.name.clone()));
                    let now = Claim { path: c.path.clone(), unit: Some(u.name.clone()), hash: u.hash.clone(), file, locator: locator.trim().to_owned() };
                    match was.filter(|w| w.path == c.path && w.unit.as_deref() == Some(u.name.as_str())) {
                        // (Stamped before plain comments left the hash: its old hash counts.)
                        Some(w) if w.hash != u.hash && w.hash != u.legacy => {
                            self.out.current.insert((page.path.clone(), c.label.clone()), now);
                            items.push(Item::Stale { label: c.label.clone(), path: c.path.clone(), locator: Some(locator.clone()), line: c.line, lines: Some((u.start, u.end)) });
                        }
                        _ => {
                            claims.insert(c.label.clone(), now);
                        }
                    }
                }
                Found::Ambiguous(candidates) => {
                    items.push(Item::Ambiguous { label: c.label.clone(), path: c.path.clone(), locator: locator.clone(), line: c.line, candidates });
                }
                Found::Missing => match was {
                    Some(w) if w.unit.is_some() && !w.hash.is_empty() => self.missing.push((page.path.clone(), index, w.clone())),
                    _ => items.push(Item::Unresolved { label: c.label.clone(), path: c.path.clone(), locator: locator.clone(), line: c.line }),
                },
            }
        }
        self.out.pages.insert(page.path.clone(), items);
        self.out.claims.insert(page.path.clone(), claims);
    }

    /// The files in scope: who owns them, and declarations new since the last look.
    fn files(&mut self, in_scope: &[String], owners: &Owners<'_>) {
        let ledger = self.ledger;
        for path in in_scope {
            if ledger.left(path, None).is_some() {
                continue;
            }
            let owner = owners.of(path);
            if owner.is_none() {
                self.out.unowned.push(path.clone());
            }
            let Some(src) = self.tree.get(path) else { continue };
            let seen = ledger.files.get(path.as_str());
            if let Some(s) = seen.filter(|s| s.hash == src.hash) {
                self.out.files.insert(path.clone(), s.clone());
                continue;
            }
            let text = src.text.clone();
            let units = self.tree.units(path).unwrap_or_default();
            let fresh: Vec<(String, Kind, usize)> = units
                .iter()
                .filter(|u| ledger::notable(u) && seen.is_none_or(|s| !s.had(&u.name)))
                .filter(|u| !self.cited.contains(&(path.clone(), u.name.clone())) && ledger.left(path, Some(&u.name)).is_none())
                .map(|u| (u.name.clone(), u.kind, u.start))
                .collect();
            match owner {
                // (An unowned file is one item, not one per declaration.)
                Some(page) if !fresh.is_empty() => {
                    if let Some(s) = seen {
                        self.out.files.insert(path.clone(), s.clone());
                    }
                    let items = self.out.pages.entry(page.to_owned()).or_default();
                    for (name, kind, line) in fresh {
                        items.push(Item::New { path: path.clone(), name, kind, line });
                    }
                }
                _ => {
                    self.out.files.insert(path.clone(), Seen::new(&text, units));
                }
            }
        }
    }

    /// Footnotes whose declaration moved: one fit, by short name and hash, anywhere in scope.
    fn moves(&mut self, bundle: &Bundle, in_scope: &[String]) {
        if self.missing.is_empty() {
            return;
        }
        let mut all: Vec<(String, Unit)> = Vec::new();
        for path in in_scope {
            for u in self.tree.units(path).unwrap_or_default() {
                all.push((path.clone(), u.clone()));
            }
        }
        for (page_path, index, was) in std::mem::take(&mut self.missing) {
            let Some(c) = bundle.page(&page_path).and_then(|p| p.cites.get(index)) else { continue };
            let name = was.unit.as_deref().unwrap_or_default();
            let fits: Vec<&(String, Unit)> = all.iter().filter(|(_, u)| u.short() == short(name) && (u.hash == was.hash || u.legacy == was.hash)).collect();
            let items = self.out.pages.entry(page_path.clone()).or_default();
            // (Still where it was stamped: the code didn't move, the
            // footnote's locator changed to one that names nothing.)
            let stayed = |(path, u): &&(String, Unit)| *path == c.path && u.name == name;
            if let [fit] = fits.as_slice()
                && !stayed(fit)
            {
                let (path, u) = fit;
                items.retain(|i| !matches!(i, Item::Absent { label: Some(l), .. } if *l == c.label));
                let file = self.tree.get(path).map(|s| s.hash.clone()).unwrap_or_default();
                let locator = format!("{} {}", u.kind.word(), u.name);
                let from = format!("{}, {}", c.path, c.locator.as_deref().unwrap_or_default());
                self.out.moved.push(Moved { page: page_path.clone(), label: c.label.clone(), line: c.line, from, path: path.clone(), locator: locator.clone() });
                self.out.claims.entry(page_path).or_default().insert(c.label.clone(), Claim { path: path.clone(), unit: Some(u.name.clone()), hash: u.hash.clone(), file, locator });
            } else if self.root.join(&c.path).is_file() {
                items.push(Item::Unresolved { label: c.label.clone(), path: c.path.clone(), locator: c.locator.clone().unwrap_or_default(), line: c.line });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle;
    use std::path::PathBuf;

    struct Repo(PathBuf);

    impl Repo {
        fn new(name: &str) -> Repo {
            let dir = std::env::temp_dir().join(format!("harlo-okf-work-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Repo(dir)
        }

        fn write(&self, path: &str, text: &str) {
            let f = self.0.join(path);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, text).unwrap();
        }

        fn work(&self, ledger: &Ledger) -> Work {
            work(&bundle::load(&self.0).unwrap(), ledger)
        }

        /// Works out the list and records it, as a check with nothing to do would.
        fn record(&self, ledger: &mut Ledger) -> Work {
            let w = self.work(ledger);
            w.record(&self.0, ledger).unwrap();
            w
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const LEDGER_RS: &str = "pub struct Ledger {}\nimpl Ledger {\n    pub fn post(&self) {\n        one();\n    }\n}\n";
    const PAGE: &str = "---\ntype: Concept\ntitle: Ledger\ncovers:\n  - src/\n---\n# Ledger\n\nPosting writes once.[^post]\n\n[^post]: src/ledger.rs, fn Ledger::post\n";

    #[test]
    fn current_after_recording_then_stale_and_new() {
        let repo = Repo::new("stale");
        repo.write("src/ledger.rs", LEDGER_RS);
        repo.write(".okf/ledger.md", PAGE);
        let mut ledger = Ledger::new();
        // Nothing recorded yet: the struct is new, the cited method isn't.
        let first = repo.record(&mut ledger);
        assert_eq!(first.pages["ledger.md"], [Item::New { path: "src/ledger.rs".into(), name: "Ledger".into(), kind: Kind::Type, line: 1 }]);
        // The snapshot waits until the struct is dealt with; leaving it out does.
        ledger.leave.insert("src/ledger.rs#Ledger".into(), "a plain holder".into());
        assert!(repo.record(&mut ledger).is_empty());
        assert!(ledger.files["src/ledger.rs"].had("Ledger::post"));
        assert!(repo.work(&ledger).is_empty(), "{}", repo.work(&ledger).report());

        // The method's body changes, and a new public function arrives.
        repo.write("src/ledger.rs", &LEDGER_RS.replace("one()", "two()").replace("    }\n}\n", "    }\n    pub fn close(&self) {}\n}\n"));
        let w = repo.work(&ledger);
        assert_eq!(
            w.pages["ledger.md"],
            [
                Item::Stale { label: "post".into(), path: "src/ledger.rs".into(), locator: Some("fn Ledger::post".into()), line: 11, lines: Some((3, 5)) },
                Item::New { path: "src/ledger.rs".into(), name: "Ledger::close".into(), kind: Kind::Function, line: 6 },
            ]
        );
        let text = w.report();
        assert!(text.starts_with("2 things to do.") && text.contains("[^post] src/ledger.rs, fn Ledger::post (lines 3–5) changed"), "{text}");
        // Recording keeps both on the list.
        w.record(&repo.0, &mut ledger).unwrap();
        assert_eq!(repo.work(&ledger).len(), 2);
    }

    #[test]
    fn one_page_is_checked_against_its_own_footnotes() {
        let repo = Repo::new("one-page");
        repo.write("src/ledger.rs", LEDGER_RS);
        repo.write(".okf/ledger.md", PAGE);
        let mut ledger = Ledger::new();
        // Never stamped: nothing to compare with.
        assert!(page_items(&bundle::load(&repo.0).unwrap(), &ledger, "ledger.md").is_empty());
        ledger.leave.insert("src/ledger.rs#Ledger".into(), "a plain holder".into());
        repo.record(&mut ledger);
        let items = || page_items(&bundle::load(&repo.0).unwrap(), &ledger, "ledger.md");
        assert!(items().is_empty());
        // A new function isn't this page's to report to a reader; a changed body is.
        repo.write("src/ledger.rs", &LEDGER_RS.replace("one()", "two()").replace("    }\n}\n", "    }\n    pub fn close(&self) {}\n}\n"));
        assert_eq!(items(), [Item::Stale { label: "post".into(), path: "src/ledger.rs".into(), locator: Some("fn Ledger::post".into()), line: 11, lines: Some((3, 5)) }]);
        // The declaration gone from its file: the footnote names nothing.
        repo.write("src/ledger.rs", "pub struct Ledger {}\n");
        assert_eq!(items(), [Item::Unresolved { label: "post".into(), path: "src/ledger.rs".into(), locator: "fn Ledger::post".into(), line: 11 }]);
        assert!(page_items(&bundle::load(&repo.0).unwrap(), &ledger, "elsewhere.md").is_empty());
    }

    #[test]
    fn unowned_unresolved_absent_and_left_out() {
        let repo = Repo::new("owners");
        repo.write("src/ledger.rs", LEDGER_RS);
        repo.write("tools/gen.rs", "pub fn gen() {}\n");
        repo.write("README.md", "# Readme\n");
        repo.write(".okf/ledger.md", &PAGE.replace("  - src/\n", "  - src/\n  - old/\n").replace("[^post]: src/ledger.rs, fn Ledger::post\n", "[^post]: src/ledger.rs, fn Ledger::post\n[^gone]: src/ledger.rs, fn gone\n[^file]: src/missing.rs\n"));
        let mut ledger = Ledger::new();
        ledger.leave.insert("src/ledger.rs#Ledger".into(), "a holder".into());
        let w = repo.work(&ledger);
        assert_eq!(w.unowned, ["tools/gen.rs"]);
        assert_eq!(
            w.pages["ledger.md"],
            [
                Item::Absent { path: "old/".into(), label: None, line: None },
                Item::Unresolved { label: "gone".into(), path: "src/ledger.rs".into(), locator: "fn gone".into(), line: 13 },
                Item::Absent { path: "src/missing.rs".into(), label: Some("file".into()), line: Some(14) },
            ]
        );
        ledger.leave.insert("tools/".into(), "generators".into());
        assert!(repo.work(&ledger).unowned.is_empty());
    }

    #[test]
    fn a_moved_declaration_is_followed() {
        let repo = Repo::new("moved");
        repo.write("src/ledger.rs", LEDGER_RS);
        repo.write(".okf/ledger.md", PAGE);
        let mut ledger = Ledger::new();
        ledger.leave.insert("src/".into(), "test".into());
        repo.record(&mut ledger);
        // The method moves, unchanged, to another type in another file.
        repo.write("src/ledger.rs", "pub struct Ledger {}\n");
        repo.write("src/book.rs", &LEDGER_RS.replace("Ledger", "Book"));
        let w = repo.record(&mut ledger);
        assert!(w.is_empty(), "{}", w.report());
        assert_eq!(w.moved.len(), 1);
        let page = std::fs::read_to_string(repo.0.join(".okf/ledger.md")).unwrap();
        assert!(page.ends_with("[^post]: src/book.rs, fn Book::post\n"), "{page}");
        assert!(repo.work(&ledger).moved.is_empty());
        assert_eq!(ledger.claims["ledger.md"]["post"].path, "src/book.rs");
    }

    #[test]
    fn a_changed_locator_is_checked_again() {
        let repo = Repo::new("locator");
        repo.write("src/runs.py", "def run():\n    go()\n\ndef run_all():\n    run()\n");
        let page = |locator: &str| format!("---\ntype: Concept\ntitle: Runs\ncovers:\n  - src/\n---\n# Runs\n\nIt runs.[^run]\n\n[^run]: src/runs.py, {locator}\n");
        repo.write(".okf/runs.md", &page("fn run"));
        let mut ledger = Ledger::new();
        ledger.leave.insert("src/".into(), "test".into());
        assert!(repo.record(&mut ledger).is_empty());
        assert_eq!(ledger.claims["runs.md"]["run"].locator, "fn run");
        // Only the footnote changes, to a kind that names nothing: not current.
        repo.write(".okf/runs.md", &page("class run"));
        let w = repo.work(&ledger);
        assert_eq!(w.pages["runs.md"], [Item::Unresolved { label: "run".into(), path: "src/runs.py".into(), locator: "class run".into(), line: 11 }]);
        assert!(w.moved.is_empty());
        // Put back, it passes.
        repo.write(".okf/runs.md", &page("fn run"));
        assert!(repo.work(&ledger).is_empty());
        // To another function in the unchanged file: that one is the claim now.
        repo.write(".okf/runs.md", &page("fn run_all"));
        assert!(repo.record(&mut ledger).is_empty());
        assert_eq!(ledger.claims["runs.md"]["run"].unit.as_deref(), Some("run_all"));
        repo.write(".okf/runs.md", &page("fn run"));
        assert!(repo.record(&mut ledger).is_empty());
        assert_eq!(ledger.claims["runs.md"]["run"].unit.as_deref(), Some("run"));
        // A claim stamped before locators were kept is checked by name, and kind.
        ledger.claims.get_mut("runs.md").unwrap().get_mut("run").unwrap().locator.clear();
        repo.write(".okf/runs.md", &page("class run"));
        assert!(!repo.work(&ledger).is_empty());
    }

    #[test]
    fn a_cited_folder_must_be_one() {
        let repo = Repo::new("folders");
        repo.write("src/ledger.rs", LEDGER_RS);
        repo.write("assets/logo.svg", "<svg/>");
        let page = |cite: &str| format!("---\ntype: Concept\ntitle: Assets\ncovers:\n  - src/\n---\n# Assets\n\nThey're kept.[^assets]\n\n[^assets]: {cite}\n");
        let mut ledger = Ledger::new();
        ledger.leave.insert("src/".into(), "test".into());
        ledger.leave.insert("assets/".into(), "test".into());
        // A missing folder, and a file written as one: absent.
        for cite in ["assets/missing/", "assets/logo.svg/"] {
            repo.write(".okf/assets.md", &page(cite));
            assert_eq!(repo.work(&ledger).pages["assets.md"], [Item::Absent { path: cite.into(), label: Some("assets".into()), line: Some(11) }], "{cite}");
        }
        // A real one, with or without its slash: current.
        for cite in ["assets/", "assets"] {
            repo.write(".okf/assets.md", &page(cite));
            assert!(repo.work(&ledger).is_empty(), "{cite}: {}", repo.work(&ledger).report());
        }
    }
}
