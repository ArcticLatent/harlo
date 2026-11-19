//! The wiki's map: where things are, for an agent's instructions.
//!
//! An agent's first tool calls mostly ask where something is. The wiki
//! already knows: each section's footnotes name the files and declarations
//! it explains. [`map`] lists every page (title and one line) and, under
//! it, each section with the code its footnotes cite, so an agent can go
//! straight to the right place, or read the one page that explains it.
//!
//! It's built from the pages alone, with no model, and stays within
//! [`BUDGET`]: past it, declarations go first, then sections.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::Path;

use crate::bundle::{self, Bundle, title_of};
use crate::page::Page;

/// The map's largest size in bytes, about 3,000 tokens: the same in every
/// request (so it's cached), but sent with each one.
pub const BUDGET: usize = 12_000;

/// The first line of the map.
const HEAD: &str = "Wiki map (.okf/): each page, and the code each of its sections cites. To find where something is, start here; read a page only for how parts fit together or why.";

/// How much of each page the map shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Detail {
    /// Sections with their files and declarations.
    Declarations,
    /// Sections with their files.
    Files,
    /// Pages only, with the files they cite.
    Pages,
}

/// The map of the repository's wiki at `root`; None when it has no pages.
#[must_use]
pub fn map(root: &Path) -> Option<String> {
    let bundle = bundle::load(root).ok()?;
    of_bundle(&bundle)
}

/// The map of a loaded wiki; None when it has no pages.
#[must_use]
pub fn of_bundle(bundle: &Bundle) -> Option<String> {
    let pages: Vec<&Page> = bundle
        .pages
        .iter()
        .filter(|p| p.front.status.as_deref() != Some("deprecated"))
        .collect();
    if pages.is_empty() {
        return None;
    }
    for detail in [Detail::Declarations, Detail::Files, Detail::Pages] {
        let text = render(&pages, detail);
        if text.len() <= BUDGET {
            return Some(text);
        }
    }
    Some(trimmed(&render(&pages, Detail::Pages)))
}

/// The map cut at a line boundary within [`BUDGET`], saying how much went.
fn trimmed(text: &str) -> String {
    let mut out = String::new();
    let mut left = 0;
    for line in text.lines() {
        if left > 0 || out.len() + line.len() + 1 > BUDGET - 40 {
            left += usize::from(line.starts_with("- "));
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if left > 0 {
        let _ = writeln!(out, "- … {left} more pages in .okf/index.md");
    }
    out
}

/// A file and the declarations cited in it, in order of first mention.
type Cited = (String, Vec<String>);

/// A section's heading (None before the first one) and the code it cites.
type Section = (Option<String>, Vec<Cited>);

fn render(pages: &[&Page], detail: Detail) -> String {
    let mut out = format!("{HEAD}\n");
    for p in pages {
        let _ = write!(out, "- {}: {}", p.path, title_of(p));
        if let Some(d) = p.front.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
            let _ = write!(out, ". {}", d.replace('\n', " "));
        }
        out.push('\n');
        let sections = sections(p);
        if detail == Detail::Pages {
            let all: Vec<Cited> = sections.into_iter().flat_map(|(_, places)| places).collect();
            let files = places(&merge(all), false);
            if !files.is_empty() {
                let _ = writeln!(out, "  code: {files}");
            }
            continue;
        }
        for (heading, cited) in sections {
            let line = places(&cited, detail == Detail::Declarations);
            if line.is_empty() {
                continue;
            }
            match heading {
                Some(h) => {
                    let _ = writeln!(out, "  - {h}: {line}");
                }
                None => {
                    let _ = writeln!(out, "  - {line}");
                }
            }
        }
    }
    out
}

/// A page's sections in order: (heading, None before the first one) and
/// the code they cite, by file in order of first mention: (path, its
/// declarations).
fn sections(p: &Page) -> Vec<Section> {
    let headings = headings(p);
    let mut out: Vec<Section> = Vec::new();
    for (label, line) in &p.refs {
        let Some(cite) = p.cites.iter().find(|c| &c.label == label) else { continue };
        let heading = headings.iter().take_while(|(at, _)| at < line).last().map(|(_, h)| h.clone());
        if out.last().is_none_or(|(h, _)| *h != heading) {
            match out.iter().position(|(h, _)| *h == heading) {
                // (A heading met again later keeps its first place.)
                Some(at) => {
                    let section = out.remove(at);
                    out.push(section);
                }
                None => out.push((heading, Vec::new())),
            }
        }
        let Some((_, cited)) = out.last_mut() else { continue };
        let name = cite.locator.as_deref().map(declaration).filter(|n| !n.is_empty());
        match cited.iter_mut().find(|(path, _)| *path == cite.path) {
            Some((_, names)) => {
                if let Some(n) = name
                    && !names.contains(&n)
                {
                    names.push(n);
                }
            }
            None => cited.push((cite.path.clone(), name.into_iter().collect())),
        }
    }
    out
}

/// The page's section headings below its title: (file line, heading).
fn headings(p: &Page) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fence = false;
    for (n, line) in p.body.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        let level = t.chars().take_while(|c| *c == '#').count();
        if (2..=4).contains(&level) && t[level..].starts_with(' ') {
            out.push((p.body_line + n, t[level..].trim().trim_end_matches('#').trim().to_owned()));
        }
    }
    out
}

/// A locator without its keyword: `fn Ledger::post` → `Ledger::post`.
fn declaration(locator: &str) -> String {
    let locator = locator.trim();
    match locator.split_once(' ') {
        Some((word, rest)) if word.chars().all(|c| c.is_ascii_lowercase()) && !rest.trim().is_empty() => rest.trim().to_owned(),
        _ => locator.to_owned(),
    }
}

/// Every file cited across sections, once, with all its declarations.
fn merge(all: Vec<Cited>) -> Vec<Cited> {
    let mut out: Vec<Cited> = Vec::new();
    for (path, names) in all {
        match out.iter_mut().find(|(p, _)| *p == path) {
            Some((_, have)) => {
                for n in names {
                    if !have.contains(&n) {
                        have.push(n);
                    }
                }
            }
            None => out.push((path, names)),
        }
    }
    out
}

/// `a.rs (f, g); b.rs`, with or without the declarations.
fn places(cited: &[Cited], declarations: bool) -> String {
    cited
        .iter()
        .map(|(path, names)| {
            if declarations && !names.is_empty() {
                format!("{path} ({})", names.join(", "))
            } else {
                path.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::parse;

    fn bundle_of(pages: &[(&str, &str)]) -> Bundle {
        Bundle { root: "/r".into(), pages: pages.iter().map(|(p, t)| parse(p, t)).collect() }
    }

    const ROUTING: &str = "---\ntype: Architecture\ntitle: Routing\ndescription: How a model is picked.\n---\n# Routing\n\nIntro cites the package.[^pkg]\n\n## Selection\n\nRoute picks.[^route] Resolve too.[^resolve]\n\n```\n## not a heading [^route]\n```\n\n## Loading\n\nLoad builds.[^load][^route]\n\n[^pkg]: laya/\n[^route]: laya/router.py, fn Router::_route\n[^resolve]: laya/router.py, fn Router::resolve\n[^load]: laya/model.py, class Loader\n";

    #[test]
    fn sections_list_their_files_and_declarations() {
        let text = of_bundle(&bundle_of(&[("routing.md", ROUTING)])).unwrap_or_default();
        assert_eq!(
            text,
            format!(
                "{HEAD}\n- routing.md: Routing. How a model is picked.\n  - laya/\n  - Selection: laya/router.py (Router::_route, Router::resolve)\n  - Loading: laya/model.py (Loader); laya/router.py (Router::_route)\n"
            )
        );
    }

    #[test]
    fn deprecated_pages_and_empty_wikis_are_left_out() {
        let old = "---\ntype: Concept\ntitle: Old\nstatus: deprecated\n---\nGone.[^a]\n\n[^a]: a.rs\n";
        assert_eq!(of_bundle(&bundle_of(&[("old.md", old)])), None);
        assert_eq!(of_bundle(&bundle_of(&[])), None);
    }

    #[test]
    fn a_big_wiki_drops_declarations_then_sections() {
        // (Long declaration names, so the full map is over budget.)
        let long = "x".repeat(60);
        let mut pages = Vec::new();
        for i in 0..60 {
            let mut body = format!("---\ntype: Concept\ntitle: Page {i}\n---\n# Page {i}\n");
            for s in 0..3 {
                let _ = write!(body, "\n## Part {s}\n\nIt does.[^c{s}]\n");
            }
            for s in 0..3 {
                let _ = write!(body, "\n[^c{s}]: src/f{i}.rs, fn {long}{s}\n");
            }
            pages.push((format!("p{i}.md"), body));
        }
        let refs: Vec<(&str, &str)> = pages.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
        let text = of_bundle(&bundle_of(&refs)).unwrap_or_default();
        assert!(text.len() <= BUDGET, "{} bytes", text.len());
        assert!(!text.contains(&long), "declarations went first");
        assert!(text.contains("  - Part 0: src/f0.rs"), "sections stayed: {text}");
    }

    #[test]
    fn a_huge_wiki_keeps_pages_within_the_budget() {
        let mut pages = Vec::new();
        for i in 0..400 {
            pages.push((format!("p{i}.md"), format!("---\ntype: Concept\ntitle: Page number {i}\ndescription: {}\n---\nText.\n", "word ".repeat(10))));
        }
        let refs: Vec<(&str, &str)> = pages.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
        let text = of_bundle(&bundle_of(&refs)).unwrap_or_default();
        assert!(text.len() <= BUDGET, "{} bytes", text.len());
        assert!(text.ends_with("more pages in .okf/index.md\n"), "{}", &text[text.len() - 80..]);
    }

    #[test]
    fn locators_lose_their_keyword_only() {
        assert_eq!(declaration("fn Ledger::post"), "Ledger::post");
        assert_eq!(declaration("section Install"), "Install");
        assert_eq!(declaration("WorkflowCard.stageColor"), "WorkflowCard.stageColor");
        assert_eq!(declaration("Main Menu"), "Main Menu");
    }
}
