//! The read-only check of a wiki, for CI: `harlo-agent wiki --validate`.
//!
//! Two things, with no model and nothing written: whether the bundle's
//! files conform (OKF v0.2 §11 and harlo's footnotes, as [`check`] finds
//! them), and what the wiki owes the code by its committed ledger: pages
//! whose cited code changed, footnotes naming nothing, new declarations and
//! files no page covers (as [`work`] finds them). Errors fail the check;
//! `strict` makes anything owed fail it too.

// Rust guideline compliant 2026-10-09

use crate::bundle;
use crate::check::{self, Level, Problem};
use crate::indexed;
use crate::ledger;
use crate::work::{self, Work};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::Path;

/// What the check found.
#[derive(Debug)]
pub struct Report {
    /// The bundle's pages.
    pub pages: usize,
    /// Problems in its files, errors first.
    pub problems: Vec<Problem>,
    /// What it owes the code.
    pub work: Work,
    /// Whether declarations came from the code index (else the plain scanner).
    pub index: bool,
}

/// Checks the wiki of the repository at `root`, writing nothing.
///
/// # Errors
///
/// When the repository has no `.okf/` wiki, or it or its ledger can't be read.
pub fn validate(root: &Path) -> Result<Report, String> {
    if !bundle::exists(root) {
        return Err(format!("{} has no wiki (no {}/ folder)", root.display(), bundle::DIR));
    }
    let bundle = bundle::load(root).map_err(|e| format!("Can't read the wiki: {e}"))?;
    let ledger = ledger::read(root).map_err(|e| format!("Can't read {}: {e}", ledger::PATH))?;
    let problems = check::check(&bundle);
    let work = work::work(&bundle, &ledger);
    Ok(Report { pages: bundle.pages.len(), problems, work, index: indexed::helper().is_some() })
}

impl Report {
    /// Problems at `level`.
    fn count(&self, level: Level) -> usize {
        self.problems.iter().filter(|p| p.level == level).count()
    }

    /// Whether the check passes: no errors, and with `strict`, nothing owed.
    #[must_use]
    pub fn passes(&self, strict: bool) -> bool {
        self.count(Level::Error) == 0 && (!strict || self.work.is_empty())
    }

    /// The report in words, for a CI log.
    #[must_use]
    pub fn text(&self, strict: bool) -> String {
        let mut out = format!("harlo wiki check: {} pages in {}/\n", self.pages, bundle::DIR);
        if !self.index {
            out.push_str(
                "(harlo-index isn't beside harlo-agent, so changed files are read with the plain scanner: a footnote may show as changed when only the code around it did.)\n",
            );
        }
        for level in [Level::Error, Level::Warning, Level::Note] {
            let these: Vec<&Problem> = self.problems.iter().filter(|p| p.level == level).collect();
            if these.is_empty() {
                continue;
            }
            let _ = writeln!(out, "\n{} {}{}:", these.len(), level, if these.len() == 1 { "" } else { "s" });
            for p in these {
                let at = p.line.map_or(String::new(), |l| format!(":{l}"));
                let _ = writeln!(out, "  {}/{}{at}: {}", bundle::DIR, p.file, p.message);
            }
        }
        if self.work.is_empty() {
            out.push_str("\nThe wiki is current with the code.\n");
        } else {
            let _ = write!(out, "\n{}", self.work.report());
        }
        let errors = self.count(Level::Error);
        let verdict = if self.passes(strict) {
            "passed".to_owned()
        } else if errors > 0 {
            format!("failed: {errors} error{}", if errors == 1 { "" } else { "s" })
        } else {
            format!("failed (--strict): the wiki owes the code {} thing{}; Update it in harlo", self.work.len(), if self.work.len() == 1 { "" } else { "s" })
        };
        let _ = writeln!(out, "\nCheck {verdict}.");
        out
    }

    /// The report as JSON, for tools.
    #[must_use]
    pub fn json(&self, strict: bool) -> Value {
        let problems = |level: Level| -> Vec<Value> {
            self.problems.iter().filter(|p| p.level == level).map(|p| json!({ "file": p.file, "line": p.line, "message": p.message })).collect()
        };
        let owed: serde_json::Map<String, Value> =
            self.work.pages.iter().map(|(page, items)| (page.clone(), items.iter().map(work::Item::text).collect::<Vec<_>>().into())).collect();
        json!({
            "passed": self.passes(strict),
            "pages": self.pages,
            "index": self.index,
            "errors": problems(Level::Error),
            "warnings": problems(Level::Warning),
            "notes": problems(Level::Note),
            "owed": owed,
            "uncovered": self.work.unowned,
            "moved": self.work.moved.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_fail_and_strict_fails_on_what_is_owed() {
        let root = std::env::temp_dir().join(format!("harlo-okf-validate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(validate(&root).unwrap_err().contains("has no wiki"));
        let write = |p: &str, t: &str| {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        };
        write("src/run.rs", "pub fn run() {\n    go();\n}\n");
        write(".okf/runs.md", "---\ntype: Concept\ntitle: Runs\ndescription: How a run goes.\ncovers: [src/]\n---\n# Runs\n\nIt goes.[^run]\n\n[^run]: src/run.rs, fn run\n");
        let checked = crate::view::answer(&json!({ "op": "check", "root": root.to_string_lossy(), "options": { "now": "2026-11-18T00:00:00Z" } }));
        assert_eq!(checked["current"], true, "{checked}");
        let ledger = std::fs::read_to_string(root.join(ledger::PATH)).unwrap();
        let fine = validate(&root).unwrap();
        assert!(fine.passes(true), "{}", fine.text(true));
        assert!(fine.text(false).contains("The wiki is current with the code.\n\nCheck passed."), "{}", fine.text(false));

        write("src/run.rs", "pub fn run() {\n    stop();\n}\n");
        let stale = validate(&root).unwrap();
        assert!(stale.passes(false) && !stale.passes(true));
        let text = stale.text(true);
        assert!(text.contains("[^run] src/run.rs, fn run (lines 1–3) changed") && text.ends_with("Check failed (--strict): the wiki owes the code 1 thing; Update it in harlo.\n"), "{text}");
        assert_eq!(stale.json(true)["owed"], json!({ "runs.md": ["[^run] src/run.rs, fn run (lines 1–3) changed since the page was written"] }));

        write(".okf/bad.md", "# No front matter\n");
        let broken = validate(&root).unwrap();
        assert!(!broken.passes(false));
        assert!(broken.text(false).contains(".okf/bad.md:1: no front matter") && broken.text(false).ends_with("Check failed: 1 error.\n"), "{}", broken.text(false));
        assert_eq!(broken.json(false)["errors"][0]["file"], "bad.md");
        // Read-only: the ledger is as the stamp left it.
        assert_eq!(std::fs::read_to_string(root.join(ledger::PATH)).unwrap(), ledger);
        let _ = std::fs::remove_dir_all(&root);
    }
}
