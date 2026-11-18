//! What harlo tells a wiki's writer of the code, from its code index, so
//! it reads less to find its way.
//!
//! Writing a wiki: a map of the repository (packages, entry points, the
//! most-called declarations). Updating it: a card for each changed or new
//! declaration the work list names (its signature and doc line, what calls
//! it), and the pages whose footnotes cite those callers: a change there
//! may make what they say wrong too.
//!
//! Without the `harlo-index` helper there's no brief; the work list alone
//! still says what's owed.
//!
//! Only the map is given by default: measured on the demo project, the
//! update cards made updates cost more (850k input over five cases
//! against 711k without), so they're for measuring (`HARLO_WIKI_BRIEF=all`;
//! `off` leaves out the map too).

// Rust guideline compliant 2026-10-09

use crate::indexed;
use crate::ledger::Ledger;
use crate::work::{Item, Work};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

/// Which briefs are given: `all` (the update cards too), `off` (none);
/// unset, the map only.
pub const BRIEF_ENV: &str = "HARLO_WIKI_BRIEF";

/// Declarations given cards, at most.
const CARDS: usize = 40;
/// Callers named on a card, at most.
const CALLERS: usize = 8;
/// Pages named as maybe affected by one declaration's change, at most.
const AFFECTED: usize = 8;

/// The brief for a writer of the wiki at `root`.
///
/// The code map when it's being written (`creating`), else the cards for
/// `work`'s items, with the pages `ledger` says cite their callers. None
/// without the helper, or with nothing to say.
#[must_use]
pub fn brief(root: &Path, work: &Work, ledger: &Ledger, creating: bool) -> Option<String> {
    let switch = std::env::var(BRIEF_ENV).unwrap_or_default();
    if switch.trim() == "off" || (!creating && switch.trim() != "all") {
        return None;
    }
    if creating {
        let map = indexed::code_map(root)?;
        return Some(format!("The code at a glance (harlo's code index):\n\n{map}"));
    }
    let wanted = wanted(work, ledger);
    if wanted.is_empty() {
        return None;
    }
    let cards = indexed::cards(root, &wanted.iter().map(|(p, n, _)| (p.clone(), n.clone())).collect::<Vec<_>>())?;
    let out = render(&cards, &wanted, &cites(ledger));
    (!out.is_empty()).then(|| format!("The declarations above as they are now (harlo's code index):\n\n{out}"))
}

/// The cards as text, each with the pages citing its callers (other than
/// the page it's owed to, in `wanted`).
fn render(cards: &[harlo_index::Card], wanted: &[(String, String, String)], cites: &BTreeMap<(String, String), Vec<(String, String)>>) -> String {
    let mut out = String::new();
    for card in cards {
        let page = wanted.iter().find(|(p, n, _)| *p == card.path && *n == card.name).map(|(_, _, page)| page.as_str()).unwrap_or_default();
        let _ = writeln!(out, "- {} {} ({}:{}-{}): {}", card.kind, card.name, card.path, card.start, card.end, card.signature);
        if let Some(doc) = card.doc.as_deref().filter(|d| !d.is_empty()) {
            let _ = writeln!(out, "  {doc}");
        }
        if !card.callers.is_empty() {
            let named: Vec<String> = card.callers.iter().take(CALLERS).map(|c| format!("{}{} ({}:{})", if c.sure { "" } else { "maybe " }, c.name, c.path, c.line)).collect();
            let more = if card.callers.len() > CALLERS { format!(", … {} more", card.callers.len() - CALLERS) } else { String::new() };
            let _ = writeln!(out, "  called by: {}{more}", named.join(", "));
        }
        let affected: BTreeSet<String> = card
            .callers
            .iter()
            .flat_map(|c| cites.get(&(c.path.clone(), c.name.clone())).into_iter().flatten())
            .filter(|(p, _)| p != page)
            .map(|(p, label)| format!("{p} [^{label}]"))
            .collect();
        if !affected.is_empty() {
            let shown: Vec<&str> = affected.iter().take(AFFECTED).map(String::as_str).collect();
            let more = if affected.len() > AFFECTED { format!(", … {} more", affected.len() - AFFECTED) } else { String::new() };
            let _ = writeln!(out, "  may be affected (they cite its callers): {}{more}", shown.join(", "));
        }
    }
    out.trim_end().to_owned()
}

/// The declarations `work` is about: `(path, full name, page)` for each
/// stale footnote naming one and each new declaration.
fn wanted(work: &Work, ledger: &Ledger) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    for (page, items) in &work.pages {
        for item in items {
            let found = match item {
                Item::Stale { label, path, .. } => ledger.claims.get(page).and_then(|c| c.get(label)).and_then(|c| c.unit.clone()).map(|n| (path.clone(), n)),
                Item::New { path, name, .. } => Some((path.clone(), name.clone())),
                _ => None,
            };
            if let Some((path, name)) = found
                && !out.iter().any(|(p, n, _)| *p == path && *n == name)
            {
                out.push((path, name, page.clone()));
            }
        }
    }
    out.truncate(CARDS);
    out
}

/// The footnotes citing each declaration: `(path, full name)` → `(page, label)`s.
fn cites(ledger: &Ledger) -> BTreeMap<(String, String), Vec<(String, String)>> {
    let mut out: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
    for (page, claims) in &ledger.claims {
        for (label, claim) in claims {
            if let Some(unit) = &claim.unit {
                out.entry((claim.path.clone(), unit.clone())).or_default().push((page.clone(), label.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Claim;
    use crate::units::Kind;
    use harlo_index::{Card, Link};

    #[test]
    fn cards_name_the_pages_citing_their_callers() {
        let mut work = Work::default();
        work.pages.insert("ledger.md".into(), vec![Item::Stale { label: "post".into(), path: "src/ledger.rs".into(), locator: Some("fn Ledger::post".into()), line: 9, lines: None }]);
        work.pages.insert("cli.md".into(), vec![Item::New { path: "src/cli.rs".into(), name: "undo".into(), kind: Kind::Function, line: 3 }]);
        let mut ledger = Ledger::new();
        let claim = |path: &str, unit: &str| Claim { path: path.into(), unit: Some(unit.into()), hash: String::new(), file: String::new() };
        ledger.claims.entry("ledger.md".into()).or_default().insert("post".into(), claim("src/ledger.rs", "Ledger::post"));
        ledger.claims.entry("cli.md".into()).or_default().insert("run".into(), claim("src/cli.rs", "run"));
        ledger.claims.entry("ledger.md".into()).or_default().insert("close".into(), claim("src/ledger.rs", "Ledger::close"));
        let wanted = wanted(&work, &ledger);
        assert_eq!(wanted, [("src/cli.rs".to_owned(), "undo".to_owned(), "cli.md".to_owned()), ("src/ledger.rs".into(), "Ledger::post".into(), "ledger.md".into())]);
        let link = |path: &str, name: &str, sure: bool| Link { path: path.into(), name: name.into(), line: 4, sure };
        let card = Card {
            path: "src/ledger.rs".into(),
            name: "Ledger::post".into(),
            kind: "fn".into(),
            start: 2,
            end: 6,
            signature: "pub fn post(&self, e: Entry)".into(),
            doc: Some("Posts an entry.".into()),
            callers: vec![link("src/cli.rs", "run", true), link("src/ledger.rs", "Ledger::close", false)],
            callees: Vec::new(),
        };
        let text = render(&[card], &wanted, &cites(&ledger));
        assert_eq!(
            text,
            "- fn Ledger::post (src/ledger.rs:2-6): pub fn post(&self, e: Entry)\n  Posts an entry.\n  called by: run (src/cli.rs:4), maybe Ledger::close (src/ledger.rs:4)\n  may be affected (they cite its callers): cli.md [^run]"
        );
    }
}
