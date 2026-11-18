//! What a wiki writer is told of the code before it reads any: a map of
//! the repository for writing a wiki, and cards for the declarations an
//! update is about.
//!
//! [`code_map`] gives the packages and what each uses, the entry points and
//! the most-called declarations, as text. [`cards`] gives each named
//! declaration's signature, doc line, callers and callees as data, so the
//! wiki can say which pages cite its callers.

// Rust guideline compliant 2026-10-09

use crate::{Graph, Kind, Manifest, Ref};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

/// Most-called declarations listed in the map.
const MOST_CALLED: usize = 20;
/// Entry points listed in the map.
const ENTRIES: usize = 10;
/// Packages listed in the map.
const PACKAGES: usize = 30;
/// External dependencies named per package.
const EXTERNAL: usize = 8;
/// Callers and callees kept on a card.
const CARD_CALLS: usize = 20;

/// A declaration's card: where it is, how it's declared, and its calls.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    /// The file it's in.
    pub path: String,
    /// Its full name.
    pub name: String,
    /// Its kind's word (`fn`, `type`, …).
    pub kind: String,
    /// Its first and last lines, from 1.
    pub start: usize,
    pub end: usize,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// The declarations calling it (outside tests), first ones first.
    pub callers: Vec<Link>,
    /// What it calls in the repository.
    pub callees: Vec<Link>,
}

/// One end of a call, on a card.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// The calling (or called) declaration's file.
    pub path: String,
    /// Its full name (`(top level)` for a call outside any).
    pub name: String,
    /// The call's line (a caller) or the declaration's (a callee).
    pub line: usize,
    /// Whether the graph is sure of it.
    pub sure: bool,
}

/// The repository at a glance, for writing its wiki: packages, entry
/// points and the most-called declarations (tests left out).
#[must_use]
pub fn code_map(graph: &Graph<'_>, manifests: &[Manifest]) -> String {
    let mut out = String::new();
    packages(graph, manifests, &mut out);
    let mut entries = Vec::new();
    let mut counts: HashMap<Ref, HashSet<(usize, Option<usize>)>> = HashMap::new();
    for edge in graph.edges() {
        let (path, call) = graph.site(edge.site);
        let caller_test = call.from.is_some_and(|i| graph.files()[edge.site.file].1.symbols[i].test) || is_test_path(path);
        if !caller_test && !graph.symbol(edge.to).test {
            counts.entry(edge.to).or_default().insert((edge.site.file, call.from));
        }
    }
    for (file, (path, f)) in graph.files().iter().enumerate() {
        for (symbol, s) in f.symbols.iter().enumerate() {
            // (Not a build script's, an example's or a benchmark's.)
            let aside = path.ends_with("build.rs") || ["examples/", "benches/"].iter().any(|d| path.starts_with(d) || path.contains(&format!("/{d}")));
            if s.kind == Kind::Function && s.short() == "main" && !s.test && !is_test_path(path) && !aside {
                entries.push(Ref { file, symbol });
            }
        }
    }
    if !entries.is_empty() {
        out.push_str("\nEntry points:\n");
        for r in entries.iter().take(ENTRIES) {
            let _ = writeln!(out, "- {} ({})", graph.symbol(*r).name, at(graph, *r));
        }
        if entries.len() > ENTRIES {
            let _ = writeln!(out, "- … {} more", entries.len() - ENTRIES);
        }
    }
    let mut most: Vec<(usize, Ref)> = counts.into_iter().filter(|(r, c)| graph.symbol(*r).kind == Kind::Function && c.len() > 1).map(|(r, c)| (c.len(), r)).collect();
    most.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    if !most.is_empty() {
        out.push_str("\nMost called (by how many declarations call them):\n");
        for (n, r) in most.iter().take(MOST_CALLED) {
            let s = graph.symbol(*r);
            let doc = s.doc.as_deref().filter(|d| !d.is_empty()).map_or(String::new(), |d| format!(": {d}"));
            let _ = writeln!(out, "- {} ({}), {n}{doc}", s.name, at(graph, *r));
        }
    }
    out.trim().to_owned()
}

/// The packages, what each holds and uses.
fn packages(graph: &Graph<'_>, manifests: &[Manifest], out: &mut String) {
    let folder = |m: &Manifest| m.path.rsplit_once('/').map_or(String::new(), |(d, _)| d.to_owned());
    let mut files: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let folders: Vec<String> = manifests.iter().map(folder).collect();
    for (path, f) in graph.files() {
        let owner = folders.iter().filter(|d| d.is_empty() || path.starts_with(&format!("{d}/"))).max_by_key(|d| d.len());
        if let Some(d) = owner {
            let e = files.entry(d.clone()).or_default();
            e.0 += 1;
            e.1 += f.symbols.len();
        }
    }
    let names: HashSet<String> = manifests.iter().filter_map(|m| m.name.as_deref().map(normal)).collect();
    let listed: Vec<&Manifest> = manifests.iter().filter(|m| files.contains_key(&folder(m))).collect();
    if listed.is_empty() {
        return;
    }
    out.push_str("Packages (folder, name: files, declarations; uses):\n");
    for m in listed.iter().take(PACKAGES) {
        let d = folder(m);
        let (n, decls) = files.get(&d).copied().unwrap_or_default();
        let shown = if d.is_empty() { "." } else { d.as_str() };
        let name = m.name.as_deref().map_or(String::new(), |n| format!(" {n}"));
        let runtime: Vec<&str> = m.dependencies.iter().filter(|x| !x.dev).map(|x| x.name.as_str()).collect();
        let inside: Vec<&str> = runtime.iter().copied().filter(|x| names.contains(&normal(x))).collect();
        let outside: Vec<&str> = runtime.iter().copied().filter(|x| !names.contains(&normal(x))).collect();
        let mut uses = inside.join(", ");
        if !outside.is_empty() {
            let more = if outside.len() > EXTERNAL { format!(", … {} more", outside.len() - EXTERNAL) } else { String::new() };
            let ext = outside.iter().take(EXTERNAL).copied().collect::<Vec<_>>().join(", ");
            uses = if uses.is_empty() { format!("{ext}{more}") } else { format!("{uses}; {ext}{more}") };
        }
        let uses = if uses.is_empty() { String::new() } else { format!("; {uses}") };
        let _ = writeln!(out, "- {shown}{name}: {n} files, {decls}{uses}");
    }
    if listed.len() > PACKAGES {
        let _ = writeln!(out, "- … {} more", listed.len() - PACKAGES);
    }
}

/// The cards of the declarations named in `wanted` (`(path, full name)`):
/// those found, in the order asked.
#[must_use]
pub fn cards(graph: &Graph<'_>, wanted: &[(String, String)]) -> Vec<Card> {
    let mut out = Vec::new();
    for (path, name) in wanted {
        let Some(r) = graph.find(name, Some(path)).into_iter().find(|r| graph.symbol(*r).kind != Kind::Impl) else { continue };
        let s = graph.symbol(r);
        let mut seen = HashSet::new();
        let callers: Vec<Link> = graph
            .callers(r)
            .into_iter()
            .filter_map(|e| {
                let (p, call) = graph.site(e.site);
                let from = call.from.map(|i| &graph.files()[e.site.file].1.symbols[i]);
                if from.is_some_and(|f| f.test) || is_test_path(p) || !seen.insert((e.site.file, call.from)) {
                    return None;
                }
                Some(Link { path: p.to_owned(), name: from.map_or_else(|| "(top level)".to_owned(), |f| f.name.clone()), line: call.line, sure: e.sure })
            })
            .take(CARD_CALLS)
            .collect();
        let mut seen = HashSet::new();
        let callees: Vec<Link> = graph
            .callees(r)
            .into_iter()
            .filter(|e| seen.insert(e.to))
            .map(|e| Link { path: graph.path(e.to).to_owned(), name: graph.symbol(e.to).name.clone(), line: graph.symbol(e.to).start, sure: e.sure })
            .take(CARD_CALLS)
            .collect();
        out.push(Card {
            path: path.clone(),
            name: s.name.clone(),
            kind: s.kind.word().to_owned(),
            start: s.start,
            end: s.end,
            signature: s.signature.clone(),
            doc: s.doc.clone(),
            callers,
            callees,
        });
    }
    out
}

fn at(graph: &Graph<'_>, r: Ref) -> String {
    format!("{}:{}", graph.path(r), graph.symbol(r).start)
}

/// A package name for matching (`harlo-core` and `harlo_core` alike).
fn normal(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

/// A test file by its path (`tests/`, `test_x.py`, `x_test.go`, `x.spec.ts`).
pub(crate) fn is_test_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.starts_with("test/")
        || path.contains("/test/")
        || file.starts_with("test_")
        || file.contains("_test.")
        || file.contains(".test.")
        || file.contains(".spec.")
}
