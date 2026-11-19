//! A project's wiki: Open Knowledge Format pages in the repository's `.okf/`.
//!
//! The engine is the harlo-okf crate. A wiki session (Create or Update in
//! the Wiki panel) writes pages with its agent's own file tools and has one
//! tool of harlo's, [`CHECK`]: it validates the pages, stamps the edited
//! ones that pass, and answers with everything still owed to the code.
//! Before a session starts, harlo runs the same check with no model: when
//! nothing is owed, it records that and no session is needed. Each runs on
//! the repository's machine ([`crate::wiki_view::ask`]).

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::wiki_view::ask;

pub use harlo_okf::bundle::DIR;

/// The wiki session's tool.
pub const CHECK: &str = "wiki_check";

/// The repository a folder is in (the wiki's root); else the folder.
pub fn root_of(folder: &Path) -> PathBuf {
    crate::git::repo_root(folder).unwrap_or_else(|| folder.to_path_buf())
}

/// Whether the repository `folder` is in has a wiki (on another machine,
/// asked there: blocking).
pub fn has_wiki(folder: &Path) -> bool {
    let root = root_of(folder);
    if crate::remote::is_remote(&root) { crate::remote::exists(&root.join(DIR), true) } else { harlo_okf::bundle::exists(&root) }
}

/// What a session in a project with a wiki is told (with [`reading_note`]'s list).
pub const NOTE: &str = "This project has a wiki in .okf/: pages written from its code on how its parts fit and why. When a question about an unfamiliar part would otherwise take a long search, read .okf/index.md, then only the pages you need. Each claim is footnoted with the code it comes from: check that code before relying on it. Skip the wiki for small, local changes.";

/// Out-of-date pages named in the note, at most.
const NOTE_PAGES: usize = 10;

/// The pages whose code changed since they were stamped (or that owe new
/// code a mention), read-only: nothing is recorded. None without a wiki
/// (or when its machine didn't answer).
pub fn out_of_date(folder: &Path) -> Option<Vec<String>> {
    let v = ask(folder, json!({ "op": "outOfDate" }));
    (v["exists"] == true).then(|| v["pages"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect()).unwrap_or_default())
}

/// [`NOTE`], with the pages that are out of date and, with `map` (the
/// project's setting), the wiki's map; None without a wiki.
pub fn reading_note(folder: &Path, map: bool) -> Option<String> {
    let pages = out_of_date(folder)?;
    let mut note = NOTE.to_owned();
    if !pages.is_empty() {
        let mut list = pages.iter().take(NOTE_PAGES).map(|p| format!(".okf/{p}")).collect::<Vec<_>>().join(", ");
        if pages.len() > NOTE_PAGES {
            list = format!("{list} and {} more", pages.len() - NOTE_PAGES);
        }
        note = format!("{note} Out of date (their code changed since they were written, so the code wins): {list}.");
    }
    if map_on(map)
        && let Some(map) = self::map(folder)
    {
        note = format!("{note}\n\n{map}");
    }
    Some(note)
}

/// Whether sessions get the wiki's map: as `setting` says (on unless
/// switched off: against no wiki −28% and −4% input for Harlo, −10% and
/// −4% for Claude Code, and 9–13% fewer tool calls than the note alone),
/// unless `HARLO_WIKI_MAP` is `on` or `off` (benchmarks).
fn map_on(setting: bool) -> bool {
    match std::env::var("HARLO_WIKI_MAP").as_deref() {
        Ok("on") => true,
        Ok("off") => false,
        _ => setting,
    }
}

/// The wiki's map ([`harlo_okf::map`]), asked on the repository's machine.
pub fn map(folder: &Path) -> Option<String> {
    ask(folder, json!({ "op": "map" }))["text"].as_str().map(str::to_owned)
}

/// The wiki session's tool, as offered.
pub fn check_tool() -> Value {
    json!({
        "name": CHECK,
        "description": "Checks the wiki (.okf/) against the code: validates every page, stamps the pages you edited that pass (harlo writes their generated and sources keys, the indexes and the log), and lists everything still owed. Call it after each page or few pages; the wiki is done when it says it's current.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "confirm": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Stale footnotes that still hold as written, after you've read their code: \"page.md#label\"."
                },
                "leave": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "A file, a folder ending in /, or file#Declaration." },
                            "why": { "type": "string", "description": "Why no page needs it, in a few words." }
                        },
                        "required": ["path", "why"]
                    },
                    "description": "Code no page needs (generated, trivial or internal plumbing), so it isn't asked about again."
                }
            }
        }
    })
}

/// Now, as OKF writes times.
fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// A wiki session's call of [`CHECK`]: `writer` is who it is, as an OKF
/// actor (`claude/claude-opus-5-5`).
///
/// # Errors
///
/// When the wiki or its ledger can't be read or written, or its machine
/// can't be reached.
pub fn call_check(folder: &Path, writer: &str, args: &Value) -> Result<Value> {
    let options = json!({ "writer": writer, "now": now(), "confirm": args["confirm"], "leave": args["leave"] });
    let found = check(folder, &options)?;
    Ok(json!({ "text": found.text, "current": found.current }))
}

/// What a check found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checked {
    /// Whether nothing is left to do.
    pub current: bool,
    /// What it did and what's owed, for the writer and the panel.
    pub text: String,
    /// What it was asked for that it couldn't do, and why.
    pub refused: Vec<String>,
    /// The wiki's pages, after it.
    pub pages: usize,
}

/// A check of the wiki with `options` ([`harlo_okf::view::answer`]'s).
fn check(folder: &Path, options: &Value) -> Result<Checked> {
    let v = ask(folder, json!({ "op": "check", "options": options }));
    if let Some(e) = v["error"].as_str() {
        bail!("{e}");
    }
    let strings = |a: &Value| a.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default();
    Ok(Checked {
        current: v["current"] == true,
        text: v["text"].as_str().unwrap_or_default().to_owned(),
        refused: strings(&v["refused"]),
        pages: v["pages"].as_u64().and_then(|n| usize::try_from(n).ok()).unwrap_or(0),
    })
}

/// The check harlo runs itself, with no model: records what needs none.
///
/// # Errors
///
/// When the wiki or its ledger can't be read or written, or its machine
/// can't be reached.
pub fn quiet_check(folder: &Path) -> Result<Checked> {
    check(folder, &json!({ "now": now() }))
}

/// The check before a wiki session starts, with a brief for the writer.
///
/// [`quiet_check`]'s, with harlo's code index's brief added to its text
/// when something's owed (a map of the code for a new wiki; see
/// `harlo_okf::brief`).
///
/// # Errors
///
/// When the wiki or its ledger can't be read or written, or its machine
/// can't be reached.
pub fn writer_check(folder: &Path) -> Result<Checked> {
    check(folder, &json!({ "now": now(), "brief": true }))
}

/// The person at this computer, as an OKF actor (`human:<login>`).
pub fn human() -> String {
    let login = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    let login: String = login.chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.')).collect();
    format!("human:{}", if login.is_empty() { "you" } else { &login })
}

/// Marks a page reviewed by the person here: they've read it against its
/// code. Returns why it couldn't be ("" when it was).
///
/// # Errors
///
/// When the wiki or its ledger can't be read or written, or its machine
/// can't be reached.
pub fn mark_reviewed(folder: &Path, page: &str) -> Result<String> {
    Ok(check(folder, &json!({ "verifier": human(), "reviewed": [page], "now": now() }))?.refused.join(" "))
}

/// The writer's name as an OKF actor: `agent/model` (the agent alone without one).
pub fn actor(agent: &str, model: &str) -> String {
    let model = model.rsplit('/').next().unwrap_or(model).trim();
    if model.is_empty() { agent.to_owned() } else { format!("{agent}/{model}") }
}

/// How pages are written, for both kinds of session.
const HOW: &str = "How pages are written:
- A page is Markdown with front matter: `type` (Overview, Architecture, Workflow, Concept, Decision or Reference), `title`, `description` (one sentence; the index shows it) and `covers` (the files and folders it answers for, folders ending in /). Every code file needs a page that covers it.
- Write what the code doesn't say by itself: how the parts fit, why it's built this way, the rules it keeps and what to watch out for. Don't restate signatures or walk through code line by line. A Decision page records one choice: what was needed, the options, what was picked and why.
- Footnote each claim with the code it comes from: `[^label]: path/to/file.rs, fn Name` (a type: `struct Name`; a method: `fn Type::name`). A file alone, or a folder ending in /, when no one declaration fits. harlo checks every footnote against the code and tells you when its code changes.
- Link pages to each other with bundle paths: [Runs](/workflows/runs.md).
- Edit pages with your own file tools. Leave `generated`, `sources`, index.md and log.md to harlo.
- Call wiki_check after each page or few: it checks what you wrote, stamps the pages that pass and lists what's left. You're done when it says the wiki is current.";

/// A wiki session's first message, sent as harlo's.
///
/// The transcript shows [`prompt_summary`]'s line for it. `mode` is "create", "update", or
/// "page:<path>" (one page checked against its code), and `report` what
/// harlo's check before it found.
pub fn run_prompt(mode: &str, report: &str) -> String {
    let from = crate::transcript::HARLO_PREFIX;
    if let Some(page) = mode.strip_prefix("page:") {
        return format!(
            "{from}Check the wiki page .okf/{page} against the code it covers and cites. Read that code, fix what the page gets wrong or leaves out, and footnote what you add. Leave other pages alone unless the fix needs a link from them.\n\n{HOW}\n\nharlo's check of this page found:\n\n{}",
            report.trim_end()
        );
    }
    let task = if mode == "create" {
        "Write this repository's wiki: pages in .okf/ that explain how the code works and why, for the people and agents working on it. Start with overview.md: what the project is, and a map that links each page. The groups below are a start for the other pages: merge, split or rename them as the code suggests."
    } else {
        "Bring this repository's wiki (.okf/) in line with the code again. Deal with each item below and leave pages that have none as they are. A stale footnote: read its code, then fix what the page says, or confirm it (wiki_check's `confirm`) when it still holds. A new declaration: cite it on the page it belongs to, or leave it out (`leave`) when no page needs it. A file no page covers: add it to a page's covers, or write a page for it."
    };
    format!("{from}{task}\n\n{HOW}\n\nharlo's check found:\n\n{}", report.trim_end())
}

/// What a wiki session's first message says, in a line (the transcript's;
/// `text` without harlo's prefix).
pub fn prompt_summary(text: &str) -> Option<&'static str> {
    if text.starts_with("Check the wiki page") {
        Some("Wiki: check a page. The agent reads one page against its code.")
    } else if text.starts_with("Write this repository's wiki") {
        Some("Wiki: create. The agent writes this project's wiki from its code.")
    } else if text.starts_with("Bring this repository's wiki") {
        Some("Wiki: update. The agent brings the wiki in line with the code again.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_and_actors() {
        let p = run_prompt("create", "3 things to do.\n");
        let said = p.strip_prefix(crate::transcript::HARLO_PREFIX).expect("harlo's");
        assert!(said.starts_with("Write this repository's wiki") && said.ends_with("harlo's check found:\n\n3 things to do."), "{p}");
        assert_eq!(crate::orchestration::said_plainly(said), "Wiki: create. The agent writes this project's wiki from its code.");
        let update = run_prompt("update", "");
        assert!(prompt_summary(update.strip_prefix(crate::transcript::HARLO_PREFIX).unwrap_or_default()).is_some_and(|s| s.starts_with("Wiki: update")));
        assert_eq!(actor("claude", "claude-opus-5-5"), "claude/claude-opus-5-5");
        assert_eq!(actor("harlo", "openrouter/qwen3-coder"), "harlo/qwen3-coder");
        assert_eq!(actor("claude", ""), "claude");
        assert_eq!(check_tool()["name"], CHECK);
    }

    #[test]
    fn a_session_check_stamps_its_pages() {
        let root = std::env::temp_dir().join(format!("harlo-core-wiki-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join(DIR)).unwrap();
        std::fs::write(root.join("src/run.rs"), "fn run() {}\n").unwrap();
        let first = quiet_check(&root).unwrap();
        assert!(!first.current && first.text.contains("src/run.rs") && first.pages == 0, "{first:?}");
        std::fs::write(root.join(".okf/run.md"), "---\ntype: Concept\ntitle: Runs\ndescription: Runs.\ncovers: [src/run.rs]\n---\nIt runs.[^run]\n\n[^run]: src/run.rs, fn run\n").unwrap();
        let answer = call_check(&root, "claude/claude-opus-5-5", &json!({})).unwrap();
        assert_eq!(answer["current"], true, "{}", answer["text"]);
        assert!(answer["text"].as_str().unwrap().starts_with("Stamped: run.md."));
        // The note, then the map: the page, and the code it cites.
        let map = map(&root).unwrap_or_default();
        assert!(map.ends_with("- run.md: Runs. Runs.\n  - src/run.rs (run)\n"), "{map}");
        assert_eq!(reading_note(&root, true), Some(format!("{NOTE}\n\n{map}")));
        // A project with it off: the note alone.
        assert_eq!(reading_note(&root, false).as_deref(), Some(NOTE));
        // Its code changes: other sessions are told the page is out of date.
        std::fs::write(root.join("src/run.rs"), "fn run() { go(); }\n").unwrap();
        let note = reading_note(&root, true).unwrap_or_default();
        assert!(note.contains("so the code wins): .okf/run.md.\n\nWiki map"), "{note}");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
