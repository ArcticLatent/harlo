//! Lookups in the call graph, as compact text for an agent: the
//! declarations a name finds, each with its signature, first doc line,
//! what it calls and what calls it.
//!
//! A call the graph resolves for certain names its one target; a possible
//! one names its candidates after "maybe". Calls from tests are counted
//! apart, after the others. Long lists are cut, saying how many more there
//! are.

// Rust guideline compliant 2026-10-09

use crate::{Edge, Graph, Kind, Ref};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

/// Declarations shown in full; more are listed by place.
const CARDS: usize = 5;
/// Places listed for a name with too many declarations, or none of its own.
const PLACES: usize = 15;
/// Calls listed on one line of a card.
const LISTED: usize = 12;
/// Lines of a caller tree.
const TREE: usize = 40;
/// The deepest caller tree asked for.
pub const MAX_DEPTH: usize = 3;

/// What to look up.
#[derive(Clone, Debug, Default)]
pub struct Query<'q> {
    /// A declaration's name: its full name (`Ledger::post`, `Cart.total`)
    /// or its short one (`post`).
    pub name: &'q str,
    /// A file, or a folder ending in `/`, the declarations are in.
    pub path: Option<&'q str>,
    /// How many levels of callers to show (1 to [`MAX_DEPTH`]).
    pub depth: usize,
}

/// The cards for `query`, or the nearest names when nothing has that one.
#[must_use]
pub fn lookup(graph: &Graph<'_>, query: &Query<'_>) -> String {
    let name = query.name.trim();
    // (A folder named without its `/`: what's in it.)
    let folder = query.path.map(|p| p.trim().trim_start_matches("./")).filter(|p| !p.is_empty()).map(|p| {
        if p.ends_with('/') || graph.files().iter().any(|(f, _)| *f == p) { p.to_owned() } else { format!("{p}/") }
    });
    let path = folder.as_deref();
    let mut found = graph.find(name, path);
    if found.is_empty() {
        // `Cart.total` for `Cart::total` and back.
        let other = if name.contains("::") { name.replace("::", ".") } else { name.replace('.', "::") };
        if other != name {
            found = graph.find(&other, path);
        }
    }
    // (An `impl` block beside its type says nothing more.)
    if found.iter().any(|r| graph.symbol(*r).kind != Kind::Impl) {
        found.retain(|r| graph.symbol(*r).kind != Kind::Impl);
    }
    if found.is_empty() {
        return near(graph, name, path);
    }
    // Code a call can reach first, tests last.
    found.sort_by_key(|r| {
        let s = graph.symbol(*r);
        (s.test, !callable(s.kind), *r)
    });
    let depth = query.depth.clamp(1, MAX_DEPTH);
    let mut out = String::new();
    for r in found.iter().take(CARDS) {
        card(graph, *r, depth, &mut out);
    }
    if found.len() > CARDS {
        let _ = writeln!(out, "{} more named {name}:", found.len() - CARDS);
        for r in found.iter().skip(CARDS).take(PLACES) {
            let _ = writeln!(out, "  {}", place(graph, *r));
        }
        more(found.len() - CARDS, PLACES, &mut out);
    }
    out.trim_end().to_owned()
}

/// One declaration's card.
fn card(graph: &Graph<'_>, r: Ref, depth: usize, out: &mut String) {
    let s = graph.symbol(r);
    let _ = writeln!(out, "{} {}  {}:{}-{}", s.kind, s.name, graph.path(r), s.start, s.end);
    if !s.signature.is_empty() {
        let _ = writeln!(out, "  {}", s.signature);
    }
    if let Some(doc) = s.doc.as_deref().filter(|d| !d.is_empty()) {
        let _ = writeln!(out, "  {doc}");
    }
    let calls = callees(graph, r);
    if !calls.is_empty() {
        let _ = writeln!(out, "  calls: {}", listed(&calls));
    }
    if !callable(s.kind) {
        return;
    }
    let (callers, tests) = callers(graph, r);
    if callers.is_empty() && tests.is_empty() {
        let _ = writeln!(out, "  called by: nothing the index finds");
    } else if depth == 1 || callers.is_empty() {
        if !callers.is_empty() {
            let names: Vec<String> = callers.iter().map(|e| caller_name(graph, e)).collect();
            let _ = writeln!(out, "  called by: {}", listed(&names));
        }
    } else {
        let _ = writeln!(out, "  called by:");
        let mut lines = 0;
        let mut seen = HashSet::from([r]);
        tree(graph, &callers, 2, depth, &mut seen, &mut lines, out);
    }
    if !tests.is_empty() {
        let names: Vec<String> = tests.iter().map(|e| caller_name(graph, e)).collect();
        let _ = writeln!(out, "  tests: {}", listed(&names));
    }
}

/// Whether calls can reach a declaration of this kind.
fn callable(kind: Kind) -> bool {
    matches!(kind, Kind::Function | Kind::Type | Kind::Macro | Kind::Component)
}

/// The callers in `edges`, then theirs, to `depth` levels.
fn tree(graph: &Graph<'_>, edges: &[Edge], level: usize, depth: usize, seen: &mut HashSet<Ref>, lines: &mut usize, out: &mut String) {
    let indent = "  ".repeat(level);
    for (i, e) in edges.iter().enumerate() {
        if *lines >= TREE {
            let _ = writeln!(out, "{indent}… {} more", edges.len() - i);
            return;
        }
        *lines += 1;
        let _ = writeln!(out, "{indent}{}", caller_name(graph, e));
        let Some(from) = from(graph, e) else { continue };
        if level - 1 < depth && seen.insert(from) {
            let (up, _) = callers(graph, from);
            tree(graph, &up, level + 1, depth, seen, lines, out);
        }
    }
}

/// The declaration a call is in, if any.
fn from(graph: &Graph<'_>, e: &Edge) -> Option<Ref> {
    let (_, call) = graph.site(e.site);
    call.from.map(|symbol| Ref { file: e.site.file, symbol })
}

/// The calls reaching `r`, outside tests and in them; one per calling
/// declaration (the first call in it).
fn callers(graph: &Graph<'_>, r: Ref) -> (Vec<Edge>, Vec<Edge>) {
    let mut seen = HashSet::new();
    let (mut code, mut tests) = (Vec::new(), Vec::new());
    for e in graph.callers(r) {
        let key = (e.site.file, from(graph, &e).map(|f| f.symbol));
        if !seen.insert(key) {
            continue;
        }
        let test = from(graph, &e).is_some_and(|f| graph.symbol(f).test) || crate::brief::is_test_path(graph.site(e.site).0);
        if test { tests.push(e) } else { code.push(e) }
    }
    (code, tests)
}

/// What `r` calls: each target once (by place), possible ones grouped by call.
fn callees(graph: &Graph<'_>, r: Ref) -> Vec<String> {
    let mut by_site: BTreeMap<(usize, usize), Vec<&Edge>> = BTreeMap::new();
    let edges = graph.callees(r);
    for e in &edges {
        by_site.entry((e.site.file, e.site.call)).or_default().push(e);
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for group in by_site.values() {
        if group.iter().all(|e| e.sure) {
            for e in group {
                if seen.insert(e.to) {
                    out.push(format!("{} ({})", graph.symbol(e.to).name, at(graph, e.to)));
                }
            }
        } else if group.iter().any(|e| seen.insert(e.to)) {
            let options: Vec<String> = group.iter().map(|e| format!("{} ({})", graph.symbol(e.to).name, at(graph, e.to))).collect();
            out.push(format!("maybe {}", options.join(" | ")));
        }
    }
    out
}

/// A call as its caller: the declaration it's in and where the call is.
fn caller_name(graph: &Graph<'_>, e: &Edge) -> String {
    let (path, call) = graph.site(e.site);
    let who = from(graph, e).map_or("(top level)", |f| graph.symbol(f).name.as_str());
    let maybe = if e.sure { "" } else { "maybe " };
    format!("{maybe}{who} ({path}:{})", call.line)
}

/// `path:line` of a declaration.
fn at(graph: &Graph<'_>, r: Ref) -> String {
    format!("{}:{}", graph.path(r), graph.symbol(r).start)
}

/// A declaration's kind, name and place, on a line.
fn place(graph: &Graph<'_>, r: Ref) -> String {
    let s = graph.symbol(r);
    format!("{} {}  {}", s.kind, s.name, at(graph, r))
}

/// Up to [`LISTED`] of `items`, comma-separated, saying how many more.
fn listed(items: &[String]) -> String {
    let mut text = items.iter().take(LISTED).cloned().collect::<Vec<_>>().join(", ");
    if items.len() > LISTED {
        let _ = write!(text, ", … {} more", items.len() - LISTED);
    }
    text
}

fn more(total: usize, shown: usize, out: &mut String) {
    if total > shown {
        let _ = writeln!(out, "  … {} more", total - shown);
    }
}

/// The declarations whose names hold `name` (any case), when none has it.
fn near(graph: &Graph<'_>, name: &str, path: Option<&str>) -> String {
    let wanted = name.rsplit(['.', ':']).next().unwrap_or(name).to_lowercase();
    let within = |p: &str| path.is_none_or(|w| p == w || (w.ends_with('/') && p.starts_with(w)));
    let mut hits = Vec::new();
    if !wanted.is_empty() {
        for (file, (p, f)) in graph.files().iter().enumerate() {
            if !within(p) {
                continue;
            }
            for (symbol, s) in f.symbols.iter().enumerate() {
                if s.kind != Kind::Impl && s.short().to_lowercase().contains(&wanted) {
                    hits.push(Ref { file, symbol });
                }
            }
        }
    }
    if hits.is_empty() {
        let scope = path.map_or(String::new(), |p| format!(" in {p}"));
        return format!("Nothing named {name}{scope} in the index, nor like it.");
    }
    let mut out = format!("Nothing named {name}; names like it:\n");
    for r in hits.iter().take(PLACES) {
        let _ = writeln!(out, "  {}", place(graph, *r));
    }
    more(hits.len(), PLACES, &mut out);
    out.trim_end().to_owned()
}
