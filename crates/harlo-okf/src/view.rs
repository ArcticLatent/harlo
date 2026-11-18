//! What harlo shows of a wiki, and the requests it makes of one, as JSON.
//!
//! [`answer`] takes one request and answers it, so the same code serves a
//! repository on this machine (called directly) and one on another
//! (`harlo-agent wiki --okf` there, the request on its standard input).
//! Reading is from the files, with no model: the pages and the links
//! between them, what the wiki owes the code, a page's data for reading
//! (its footnotes numbered and marked), and a search of its sections. The
//! one request that writes is `check` ([`stamp::run`]).

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Value, json};

use crate::bundle::{self, Bundle};
use crate::check::{self, Level};
use crate::cite::{self, Found};
use crate::ledger::{self, Ledger};
use crate::page::{self, Page};
use crate::stamp::{self, Options};
use crate::units::{self, Kind};
use crate::work::{self, Item, Work};

/// Answers a request of the wiki in the repository at its `root`.
///
/// Requests, by `op`:
/// - `state`: [`state`]'s answer.
/// - `page` (`path`): [`page`]'s.
/// - `search` (`query`, `limit`): [`search`]'s.
/// - `pageReport` (`path`): `{ text }`, what one page owes, a line each.
/// - `outOfDate`: `{ exists, pages }`, whether there's a wiki, and the
///   pages that owe the code something.
/// - `check` (`options`: `{ writer, now, confirm, leave: [{ path, why }],
///   verifier, reviewed }`): `{ current, text, refused, pages }`, after
///   [`stamp::run`] with those options.
///
/// Anything that can't be answered is `{ error }`.
pub fn answer(request: &Value) -> Value {
    let root = Path::new(request["root"].as_str().unwrap_or_default());
    let text = |k: &str| request[k].as_str().unwrap_or_default();
    match text("op") {
        "state" => state(root),
        "page" => page(root, text("path")),
        "search" => search(root, text("query"), request["limit"].as_u64().map_or(12, |n| usize::try_from(n).unwrap_or(12))),
        "pageReport" => json!({ "text": page_report(root, text("path")) }),
        "outOfDate" => json!({ "exists": bundle::exists(root), "pages": out_of_date(root) }),
        "check" => run_check(root, &request["options"]),
        other => json!({ "error": format!("No wiki request {other:?}") }),
    }
}

/// A check with the options a request gives.
fn run_check(root: &Path, options: &Value) -> Value {
    let strings = |k: &str| -> Vec<String> { options[k].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default() };
    let actor = |k: &str| options[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let leave = options["leave"]
        .as_array()
        .map(|a| a.iter().map(|v| (v["path"].as_str().unwrap_or_default().to_owned(), v["why"].as_str().unwrap_or_default().to_owned())).collect())
        .unwrap_or_default();
    let options = Options {
        writer: actor("writer"),
        now: options["now"].as_str().unwrap_or_default().to_owned(),
        confirm: strings("confirm"),
        leave,
        verifier: actor("verifier"),
        reviewed: strings("reviewed"),
    };
    match stamp::run(root, &options) {
        Ok(o) => {
            let pages = bundle::load(root).map_or(0, |b| b.pages.len());
            json!({ "current": o.is_current(), "text": o.report(), "refused": o.refused, "pages": pages })
        }
        Err(e) => json!({ "error": format!("The wiki couldn't be checked: {e}") }),
    }
}

/// A wiki read once: its pages, its ledger and what it owes.
struct Read {
    bundle: Bundle,
    ledger: Ledger,
    work: Work,
}

fn read(root: &Path) -> Option<Read> {
    if !bundle::exists(root) {
        return None;
    }
    let bundle = bundle::load(root).ok()?;
    let ledger = ledger::read(root).unwrap_or_else(|_| Ledger::new());
    let work = work::work(&bundle, &ledger);
    Some(Read { bundle, ledger, work })
}

/// A page's trust tier (OKF §5.3): "human", "machine" or "unverified".
fn tier(p: &Page) -> &'static str {
    if p.front.verified.iter().any(|s| s.by.starts_with("human:")) {
        "human"
    } else if p.front.verified.is_empty() {
        "unverified"
    } else {
        "machine"
    }
}

/// The pages a page links to, as bundle paths, each once.
fn links(p: &Page, present: &BTreeSet<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (target, _) in &p.links {
        if let Some(to) = page::link_page(&p.path, target)
            && present.contains(to.as_str())
            && to != p.path
            && !out.contains(&to)
        {
            out.push(to);
        }
    }
    out
}

/// Whether a page was edited since it was last stamped.
fn edited(r: &Read, p: &Page) -> bool {
    let text = std::fs::read_to_string(r.bundle.dir().join(&p.path)).unwrap_or_default();
    r.ledger.pages.get(&p.path) != Some(&units::digest(text.as_bytes()))
}

/// The wiki of the repository at `root`.
///
/// Its shape: `{ root, exists, pages: [{ path, title, description, type,
/// status, tier, links, owes, edited }], work: { count, new, stale, broken,
/// unowned, pages, current, text }, errors, warnings, last }`.
pub fn state(root: &Path) -> Value {
    let Some(r) = read(root) else {
        return json!({ "root": root.to_string_lossy(), "exists": false, "pages": [] });
    };
    let present: BTreeSet<&str> = r.bundle.pages.iter().map(|p| p.path.as_str()).collect();
    let mut ordered: Vec<&Page> = r.bundle.pages.iter().collect();
    // (The overview first: where to start.)
    ordered.sort_by_key(|p| (p.path != "overview.md", p.path.clone()));
    let pages: Vec<Value> = ordered
        .iter()
        .map(|p| {
            let title = p.front.title.clone().unwrap_or_else(|| bundle::folder_title(p.path.rsplit('/').next().unwrap_or(&p.path).trim_end_matches(".md")));
            json!({
                "path": p.path,
                "title": title,
                "description": p.front.description.clone().unwrap_or_default(),
                "type": p.front.kind.clone().unwrap_or_default(),
                "status": p.front.status.clone().unwrap_or_default(),
                "tier": tier(p),
                "links": links(p, &present),
                "owes": r.work.pages.get(&p.path).map_or(0, Vec::len),
                "edited": edited(&r, p),
            })
        })
        .collect();
    let items = r.work.pages.values().flatten();
    let count = |f: fn(&Item) -> bool| items.clone().filter(|i| f(i)).count();
    let problems = check::check(&r.bundle);
    let level = |l: Level| problems.iter().filter(|p| p.level == l).count();
    let last = r.bundle.pages.iter().filter_map(|p| p.front.generated.as_ref().map(|g| g.at.clone())).max().unwrap_or_default();
    json!({
        "root": root.to_string_lossy(),
        "exists": true,
        "pages": pages,
        "work": {
            "count": r.work.len(),
            "new": count(|i| matches!(i, Item::New { .. })),
            "stale": count(|i| matches!(i, Item::Stale { .. })),
            "broken": count(|i| matches!(i, Item::Unresolved { .. } | Item::Ambiguous { .. } | Item::Absent { .. })),
            "unowned": r.work.unowned.len(),
            "pages": r.work.pages.len(),
            "current": r.work.is_empty(),
            "text": r.work.report(),
        },
        "errors": level(Level::Error),
        "warnings": level(Level::Warning),
        "last": last,
    })
}

/// A footnote's state, as the page shows it.
fn footnote_state(items: &[Item], label: &str) -> &'static str {
    match items.iter().find(|i| i.label() == Some(label)) {
        Some(Item::Stale { .. }) => "stale",
        Some(Item::Unresolved { .. } | Item::Ambiguous { .. }) => "missing",
        Some(Item::Absent { .. }) => "gone",
        _ => "current",
    }
}

/// The body without its footnote definitions, each reference made a link.
///
/// A reference becomes `[[1]](okf-fn:label)`, numbered in reading order
/// and joined to the word before it (U+2060) so a line doesn't start with
/// it; code (fenced or inline) is left as it is.
fn linked_body(body: &str, numbers: &BTreeMap<String, usize>) -> String {
    let mut out = String::with_capacity(body.len());
    let mut fence: Option<&str> = None;
    for line in body.split_inclusive('\n') {
        let t = line.trim_start();
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(&t[..3]);
            out.push_str(line);
            continue;
        }
        if t.starts_with("[^") && t.contains("]:") {
            continue;
        }
        let mut in_code = false;
        let mut rest = line;
        while !rest.is_empty() {
            if !in_code
                && let Some(after) = rest.strip_prefix("[^")
                && let Some(end) = after.find(']')
                && let Some(n) = numbers.get(&after[..end])
            {
                let _ = write!(out, "\u{2060}[\\[{n}\\]](okf-fn:{})", &after[..end]);
                rest = &after[end + 1..];
                continue;
            }
            let c = rest.chars().next().unwrap_or_default();
            if c == '`' {
                in_code = !in_code;
            }
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// A heading's anchor, as GitHub makes them ("Retry control" → "retry-control").
pub fn anchor(heading: &str) -> String {
    heading.trim().to_lowercase().chars().filter_map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { Some(c) } else if c == ' ' { Some('-') } else { None }).collect()
}

/// A wiki page's data for reading; harlo lays out its `body`.
///
/// Its shape: `{ path, title, description, type, status, tier, generated,
/// verified, edited, body, footnotes: [{ n, label, path, locator, start,
/// end, state }], backlinks: [{ path, title }], items }` (`error` when
/// there's no such page). `body` is the page's Markdown with its footnote
/// definitions taken out and each reference a link to `okf-fn:label`.
pub fn page(root: &Path, path: &str) -> Value {
    let path = path.trim_start_matches(".okf/").trim_start_matches('/');
    let Some(r) = read(root) else { return json!({ "path": path, "error": "This project has no wiki." }) };
    let Some(p) = r.bundle.page(path) else { return json!({ "path": path, "error": format!("The wiki has no page {path}.") }) };
    let items: &[Item] = r.work.pages.get(path).map_or(&[], Vec::as_slice);
    // Footnotes in the order they're first referred to, then the rest.
    let mut numbers: BTreeMap<String, usize> = BTreeMap::new();
    for label in p.refs.iter().map(|(l, _)| l).chain(p.cites.iter().map(|c| &c.label)) {
        let next = numbers.len() + 1;
        numbers.entry(label.clone()).or_insert(next);
    }
    let mut scanned: BTreeMap<&str, Vec<units::Unit>> = BTreeMap::new();
    let mut footnotes: Vec<Value> = p
        .cites
        .iter()
        .map(|c| {
            let state = footnote_state(items, &c.label);
            let mut at = (0, 0);
            if let Some(locator) = c.locator.as_deref().filter(|_| state != "gone") {
                let units = scanned.entry(&c.path).or_insert_with(|| std::fs::read_to_string(root.join(&c.path)).map(|t| units::scan(&c.path, &t)).unwrap_or_default());
                if let Found::One(i) = cite::locate(units, locator) {
                    at = (units[i].start, units[i].end);
                }
            }
            json!({ "n": numbers.get(&c.label).copied().unwrap_or(0), "label": c.label, "path": c.path, "locator": c.locator, "start": at.0, "end": at.1, "state": state })
        })
        .collect();
    footnotes.sort_by_key(|f| f["n"].as_u64().unwrap_or(0));
    let backlinks: Vec<Value> = r
        .bundle
        .pages
        .iter()
        .filter(|o| o.path != path && o.links.iter().any(|(t, _)| page::link_page(&o.path, t).as_deref() == Some(path)))
        .map(|o| json!({ "path": o.path, "title": o.front.title.clone().unwrap_or_else(|| o.path.clone()) }))
        .collect();
    let stamp = |s: &page::Stamp| json!({ "by": s.by, "at": s.at });
    json!({
        "path": path,
        "title": p.front.title.clone().unwrap_or_else(|| path.to_owned()),
        "description": p.front.description.clone().unwrap_or_default(),
        "type": p.front.kind.clone().unwrap_or_default(),
        "status": p.front.status.clone().unwrap_or_default(),
        "tier": tier(p),
        "generated": p.front.generated.as_ref().map(stamp),
        "verified": p.front.verified.iter().map(stamp).collect::<Vec<_>>(),
        "edited": edited(&r, p),
        "body": linked_body(&p.body, &numbers),
        "footnotes": footnotes,
        "backlinks": backlinks,
        "items": items.iter().map(|i| owed_text(i, &numbers)).collect::<Vec<_>>(),
    })
}

/// What a page owes, in words for the person reading it.
///
/// Footnotes go by their number on the page (the writer's report names
/// them by label).
fn owed_text(item: &Item, numbers: &BTreeMap<String, usize>) -> String {
    let n = |label: &str| numbers.get(label).map_or_else(|| format!("Footnote [^{label}]"), |n| format!("Footnote {n}"));
    let at = |path: &str, locator: &str| if locator.is_empty() { path.to_owned() } else { format!("{path}, {locator}") };
    match item {
        Item::New { path, name, kind, line } => {
            let what = match kind {
                Kind::Function => "function",
                Kind::Type => "type",
                Kind::Module => "module",
                Kind::Constant => "constant",
                other => other.word(),
            };
            format!("New {what} {name} (in {path}, line {line}) that no page mentions")
        }
        Item::Stale { label, path, locator, .. } => format!("{}: its code changed since the page was written ({})", n(label), at(path, locator.as_deref().unwrap_or_default())),
        Item::Unresolved { label, path, locator, .. } => format!("{}: names nothing in the code ({})", n(label), at(path, locator)),
        Item::Ambiguous { label, path, locator, candidates, .. } => format!("{}: fits several declarations ({}: {})", n(label), at(path, locator), candidates.join(", ")),
        Item::Absent { path, label: Some(label), .. } => format!("{}: {path} isn't there", n(label)),
        Item::Absent { path, label: None, .. } => format!("It covers {path}, which isn't there"),
    }
}

/// Words too common to search by ("how", "the"), unless a query is nothing else.
const FILLER: [&str; 28] = ["the", "a", "an", "and", "or", "of", "to", "in", "is", "it", "for", "on", "with", "as", "at", "by", "how", "what", "where", "when", "why", "does", "do", "are", "be", "from", "this", "that"];

/// A query's words worth searching by, lower case.
fn query_words(query: &str) -> Vec<String> {
    let words: Vec<String> = query.to_lowercase().split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| w.len() > 1).map(str::to_owned).collect();
    let kept: Vec<String> = words.iter().filter(|w| !FILLER.contains(&w.as_str())).cloned().collect();
    if kept.is_empty() { words } else { kept }
}

/// A line of a page as plain text, for search.
///
/// Footnote references go; a link keeps its text and loses its target;
/// backticks, bold and a table's pipes go (a table's rule line is empty),
/// and so does a list item's marker. Inline code keeps its text as it is.
fn plain_line(line: &str) -> String {
    let t = line.trim();
    if t.starts_with('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
        return String::new();
    }
    let t = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).unwrap_or(t);
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    let mut in_code = false;
    while let Some(c) = rest.chars().next() {
        if c == '`' {
            in_code = !in_code;
            rest = &rest[1..];
            continue;
        }
        if in_code {
            out.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("[^") {
            rest = after.find(']').map_or("", |end| &after[end + 1..]);
            continue;
        }
        // (A link: its text, then past its target.)
        if (c == '[' || (c == '!' && rest[1..].starts_with('[')))
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close..].find(')')
        {
            out.push_str(rest[..close].trim_start_matches('!').trim_start_matches('['));
            rest = &rest[close + end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("**") {
            rest = after;
            continue;
        }
        out.push(if c == '|' { ' ' } else { c });
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Where the sentence holding byte `at` of `text` starts and ends.
///
/// A sentence ends at a full stop followed by a space or the line's end
/// (so `order.placed` and `events.md` stay whole), or at a line's end.
fn sentence(text: &str, at: usize) -> (usize, usize) {
    let at = at.min(text.len());
    let ends = |i: usize, b: u8| b == b'\n' || (b == b'.' && text.as_bytes().get(i + 1).is_none_or(u8::is_ascii_whitespace));
    let start = text.as_bytes()[..at].iter().enumerate().rev().find(|&(i, &b)| ends(i, b)).map_or(0, |(i, _)| i + 1);
    let end = text.as_bytes()[at..].iter().enumerate().find(|&(i, &b)| ends(at + i, b)).map_or(text.len(), |(i, _)| at + i + 1);
    (start, end)
}

/// A page's section, for search.
struct Section {
    heading: String,
    /// Its text as plain text, a line each.
    prose: String,
    /// Its code blocks' lines.
    code: String,
}

/// A search of the pages' sections, best first, at most `limit`.
///
/// Its shape: `{ hits: [{ page, section, heading, snippet }], workspaces:
/// [] }`. A section scores by how many of the query's words it has (a
/// title or heading counting twice); its snippet is the sentence around
/// the first.
pub fn search(root: &Path, query: &str, limit: usize) -> Value {
    let words = query_words(query);
    let Ok(bundle) = bundle::load(root) else { return json!({ "hits": [], "workspaces": [] }) };
    let mut hits: Vec<(usize, Value)> = Vec::new();
    for p in &bundle.pages {
        let title = p.front.title.clone().unwrap_or_else(|| p.path.clone());
        // Sections: the prose under each heading (the first under the
        // title), as plain text, and its code, which counts but isn't shown.
        let mut sections: Vec<Section> = vec![Section { heading: String::new(), prose: p.front.description.clone().unwrap_or_default() + "\n", code: String::new() }];
        let mut fence = false;
        for line in p.body.lines() {
            let t = line.trim_start();
            let Some(last) = sections.last_mut() else { continue };
            if t.starts_with("```") || t.starts_with("~~~") {
                fence = !fence;
            } else if fence {
                last.code.push_str(line);
                last.code.push('\n');
            } else if t.starts_with('#') {
                sections.push(Section { heading: t.trim_start_matches('#').trim().to_owned(), prose: String::new(), code: String::new() });
            } else if !t.starts_with("[^") {
                last.prose.push_str(&plain_line(line));
                last.prose.push('\n');
            }
        }
        for Section { heading, prose, code } in &sections {
            let lower = prose.to_lowercase();
            let code = code.to_lowercase();
            let head = format!("{title} {heading}").to_lowercase();
            let score: usize = words
                .iter()
                .map(|w| usize::from(lower.contains(w.as_str()) || code.contains(w.as_str())) + 2 * usize::from(head.contains(w.as_str())))
                .sum();
            if score == 0 {
                continue;
            }
            // (Around the first word the prose has; else its first sentence.)
            let at = words.iter().filter_map(|w| lower.find(w.as_str())).min().unwrap_or_else(|| prose.len() - prose.trim_start().len());
            let (start, end) = sentence(prose, at);
            let snippet: String = prose.get(start..end).unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
            let section = if heading.is_empty() { String::new() } else { anchor(heading) };
            hits.push((score, json!({ "page": p.path, "section": section, "heading": heading, "snippet": snippet.chars().take(240).collect::<String>() })));
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    json!({ "hits": hits.into_iter().take(limit).map(|(_, h)| h).collect::<Vec<_>>(), "workspaces": [] })
}

/// A page's work, in words, for a session that checks just that page.
pub fn page_report(root: &Path, path: &str) -> String {
    let Some(r) = read(root) else { return String::new() };
    match r.work.pages.get(path) {
        Some(items) if !items.is_empty() => items.iter().map(|i| format!("- {}\n", i.text())).collect(),
        _ => "Nothing flagged: its footnotes' code hasn't changed since it was written.\n".to_owned(),
    }
}

/// The pages whose code changed since they were stamped, or that owe new
/// code a mention; read-only, nothing is recorded.
pub fn out_of_date(root: &Path) -> Vec<String> {
    let (Ok(bundle), Ok(ledger)) = (bundle::load(root), ledger::read(root)) else { return Vec::new() };
    work::work(&bundle, &ledger).pages.into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_reads_with_its_footnotes() {
        let root = std::env::temp_dir().join(format!("harlo-okf-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |p: &str, t: &str| {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        };
        write("src/run.rs", "pub fn run() {\n    go();\n}\n");
        write(".okf/runs.md", "---\ntype: Concept\ntitle: Runs\ncovers: [src/]\n---\n# Runs\n\nIt goes.[^run] Not `[^code]`.[^gone]\n\n```\n[^x]: kept in code\n```\n\n[^run]: src/run.rs, fn run\n[^gone]: src/run.rs, fn gone\n");
        write(".okf/overview.md", "---\ntype: Overview\ntitle: Overview\n---\nSee [runs](/runs.md).\n");
        let at = root.to_string_lossy();
        let v = answer(&json!({ "op": "page", "root": at, "path": ".okf/runs.md" }));
        let st = answer(&json!({ "op": "state", "root": at }));
        let found = answer(&json!({ "op": "search", "root": at, "query": "how does it go", "limit": 5 }));
        let late = answer(&json!({ "op": "outOfDate", "root": at }));
        let checked = answer(&json!({ "op": "check", "root": at, "options": { "now": "2026-11-17T00:00:00Z", "leave": [{ "path": "nowhere.rs", "why": "x" }] } }));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found["hits"][0]["page"], "runs.md");
        assert_eq!(found["hits"][0]["snippet"], "It goes.");
        let notes: Vec<(u64, &str, u64, &str)> = v["footnotes"].as_array().unwrap().iter().map(|f| (f["n"].as_u64().unwrap(), f["label"].as_str().unwrap(), f["start"].as_u64().unwrap(), f["state"].as_str().unwrap())).collect();
        assert_eq!(notes, [(1, "run", 1, "current"), (2, "gone", 0, "missing")]);
        let body = v["body"].as_str().unwrap();
        assert!(body.contains("[\\[1\\]](okf-fn:run)") && body.contains("Not `[^code]`") && body.contains("[^x]: kept in code") && !body.contains("src/run.rs, fn run"), "{body}");
        assert_eq!(v["items"], json!(["Footnote 2: names nothing in the code (src/run.rs, fn gone)"]));
        assert_eq!(v["backlinks"][0]["path"], "overview.md");
        assert_eq!(st["pages"][0]["path"], "overview.md");
        assert_eq!(st["pages"][0]["links"], json!(["runs.md"]));
        assert_eq!(st["pages"][1]["owes"], 1);
        assert_eq!(st["work"]["broken"], 1);
        assert_eq!(late, json!({ "exists": true, "pages": ["runs.md"] }));
        assert_eq!((checked["current"].as_bool(), checked["pages"].as_u64()), (Some(false), Some(2)), "{checked}");
        assert_eq!(checked["refused"], json!(["nowhere.rs isn't in the repository, so there's nothing to leave out"]));
        assert_eq!(answer(&json!({ "op": "guess" }))["error"], "No wiki request \"guess\"");
        assert_eq!(query_words("how does the ledger post"), ["ledger", "post"]);
        assert_eq!(plain_line("- Goes.[^run] See [the ledger](/ledger.md#posting), `[^x]` **once**.[^y]"), "Goes. See the ledger, [^x] once.");
        assert_eq!(plain_line("| order.placed | `order_id` |"), "  order.placed   order_id  ");
        assert_eq!(plain_line("| --- | :-: |"), "");
        let text = "It emits order.placed for events.md readers. Then it stops.\nNext.";
        let (a, b) = sentence(text, text.find("events").unwrap());
        assert_eq!(&text[a..b], "It emits order.placed for events.md readers.");
        assert!(body.contains("goes.\u{2060}[\\[1\\]]"), "{body}");
    }
}
