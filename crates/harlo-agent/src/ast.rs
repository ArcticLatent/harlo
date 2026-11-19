//! The `ast_grep` and `ast_edit` tools: code found, and rewritten, by its
//! syntax.
//!
//! The `harlo-index` helper does the matching (see
//! `harlo_index::search`); `ast_edit` writes what it works out, each file
//! checked as a write is, all of them or none. They're off unless
//! `HARLO_AGENT_AST` is `on`, and offered only where the helper is found.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::Path;

/// The search tool's name.
pub const GREP: &str = "ast_grep";

/// The rewrite tool's name.
pub const EDIT: &str = "ast_edit";

/// The most matches a search lists.
const LISTED: usize = 60;

/// A listed match's text is cut to this many characters.
const MATCH_CHARS: usize = 160;

/// Whether the tools are switched on (`HARLO_AGENT_AST=on`).
pub fn on() -> bool {
    std::env::var("HARLO_AGENT_AST").is_ok_and(|v| v.trim() == "on")
}

/// Whether the model gets them: they're on and the helper is there.
pub fn offered() -> bool {
    on() && harlo_okf::indexed::helper().is_some()
}

const PATTERN: &str = "Code with metavariables: $X is one node (an expression, a name, a statement), $$$X any number of them (arguments, statements), $_ one unnamed. Examples: `foo($A, $$$REST)`, `$OBJ.unwrap()`, `if err != nil { return $$$ }`, `def $F(self, $$$): $$$`.";

/// The search tool's spec.
pub fn grep_spec() -> Value {
    json!({ "type": "function", "name": GREP, "strict": false,
        "description": "Find code by its syntax rather than its text: a pattern written as code matches however the code is spaced or wrapped, and never in comments or strings. Use it for code shapes grep can't pin down (a call with certain arguments, a method on any receiver, a statement inside a block); use grep for names and text.",
        "parameters": { "type": "object", "properties": {
            "pattern": { "type": "string", "description": PATTERN },
            "lang": { "type": "string", "description": "The language (rust, python, typescript, go, …): only its files, and the pattern read as it" },
            "path": { "type": "string", "description": "A file or folder to search (the project by default)" },
        }, "required": ["pattern"], "additionalProperties": false } })
}

/// The rewrite tool's spec.
pub fn edit_spec() -> Value {
    json!({ "type": "function", "name": EDIT, "strict": false,
        "description": "Rewrite every match of a structural pattern (as ast_grep finds them) with code using its metavariables, across files in one call: all the files change, or none do. The result lists each change, so don't re-read the files to check them. For a few distinct changes, use edit.",
        "parameters": { "type": "object", "properties": {
            "pattern": { "type": "string", "description": PATTERN },
            "rewrite": { "type": "string", "description": "The code replacing each match, with the pattern's metavariables: `$OBJ.expect(\"why\")`" },
            "lang": { "type": "string", "description": "The language (rust, python, typescript, go, …)" },
            "path": { "type": "string", "description": "A file or folder to change (the project by default)" },
        }, "required": ["pattern", "rewrite"], "additionalProperties": false } })
}

/// What the helper found for the call's `args` under `cwd`, with
/// `rewrite` or without: the matches, or why there are none.
pub async fn find(args: &Value, cwd: &Path, rewrite: bool) -> Result<harlo_index::Structural, String> {
    let s = |k: &str| args[k].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    let pattern = s("pattern").ok_or("Give a pattern.")?;
    let new = if rewrite { Some(args["rewrite"].as_str().ok_or("Give the rewrite.")?.to_owned()) } else { None };
    let lang = match s("lang") {
        Some(l) => Some(harlo_index::named(&l).ok_or_else(|| format!("No language called {l}."))?),
        None => None,
    };
    let path = s("path").map(|p| relative(cwd, &p));
    let root = cwd.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let query = harlo_index::Search {
            pattern: &pattern,
            rewrite: new.as_deref(),
            lang,
            paths: path.iter().map(String::as_str).filter(|p| !p.is_empty()).collect(),
            limit: if rewrite { usize::MAX } else { LISTED },
        };
        harlo_okf::indexed::structural(&root, &query)
    })
    .await
    .map_err(|e| format!("The code index stopped: {e}"))?
}

/// A search's matches as the model reads them: `path:line: code`, a line
/// each, leaving out files in `hidden`.
pub fn listed(found: &harlo_index::Structural, hidden: &[String]) -> String {
    let shown: Vec<&harlo_index::Found> = found.matches.iter().filter(|m| !hidden.iter().any(|h| m.path.contains(h.as_str()))).collect();
    if shown.is_empty() {
        return "No matches.".to_owned();
    }
    let mut out = String::new();
    for m in &shown {
        let _ = writeln!(out, "{}:{}: {}", m.path, m.line, one_line(&m.text));
    }
    if found.total > found.matches.len() {
        let _ = writeln!(out, "[{} of {} matches. Narrow the pattern or the path for the rest.]", found.matches.len(), found.total);
    }
    out.trim_end().to_owned()
}

/// A rewrite's changes as the model reads them: `path:line: old → new`.
pub fn changes(found: &harlo_index::Structural) -> String {
    let mut out = String::new();
    for m in found.matches.iter().take(LISTED) {
        let _ = writeln!(out, "{}:{}: {} → {}", m.path, m.line, one_line(&m.text), one_line(m.replacement.as_deref().unwrap_or_default()));
    }
    if found.matches.len() > LISTED {
        let _ = writeln!(out, "[and {} more]", found.matches.len() - LISTED);
    }
    out.trim_end().to_owned()
}

/// `text` on one line, cut short.
fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() > MATCH_CHARS { format!("{}…", joined.chars().take(MATCH_CHARS).collect::<String>()) } else { joined }
}

/// `p` relative to `cwd`.
fn relative(cwd: &Path, p: &str) -> String {
    let base = format!("{}/", cwd.display());
    let rel = p.strip_prefix(&base).unwrap_or(p).trim_start_matches("./");
    if rel == "." { String::new() } else { rel.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_are_listed_one_line_each() {
        let found = harlo_index::Structural {
            matches: vec![
                harlo_index::Found { path: "a.py".into(), line: 3, end_line: 4, text: "total(items,\n      tax)".into(), replacement: None },
                harlo_index::Found { path: ".env/x.py".into(), line: 1, end_line: 1, text: "total(a, b)".into(), replacement: None },
            ],
            total: 5,
            ..harlo_index::Structural::default()
        };
        assert_eq!(listed(&found, &[".env".into()]), "a.py:3: total(items, tax)\n[2 of 5 matches. Narrow the pattern or the path for the rest.]");
        assert_eq!(listed(&harlo_index::Structural::default(), &[]), "No matches.");
        let mut rewrite = found.clone();
        rewrite.matches[0].replacement = Some("sum(items, tax)".into());
        assert!(changes(&rewrite).starts_with("a.py:3: total(items, tax) → sum(items, tax)\n"));
        assert_eq!(relative(Path::new("/w"), "/w/src"), "src");
    }
}
