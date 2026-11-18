//! A wiki page: an OKF concept file, read and rewritten in place.
//!
//! A page is Markdown with YAML front matter. Of its keys, the writer edits
//! `type`, `title`, `description`, `tags`, `status` and `covers` (Harlo's
//! own key: the files and folders the page answers for). Harlo writes the
//! rest when a page is stamped: `sources` (from `covers` and the footnotes),
//! `generated`, and the hidden ledger. Other keys are kept as they are, as
//! OKF asks of consumers.
//!
//! A claim cites code with a footnote whose definition names a repository
//! path and, after a comma, the declaration: `[^post]: src/ledger.rs, fn
//! Ledger::post`. A path ending in `/` names a folder; a path alone, a file.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use yaml_rust::{Yaml, YamlLoader};

/// A `{ by, at }` stamp: who wrote or checked a page, and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub by: String,
    pub at: String,
}

/// The front matter keys the engine reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Front {
    /// OKF's `type` (`Architecture`, `Workflow`, `Decision`, …).
    pub kind: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    /// `draft`, `stable` or `deprecated` (absent: stable).
    pub status: Option<String>,
    /// Harlo's key: the repository files and folders the page answers for.
    pub covers: Vec<String>,
    pub generated: Option<Stamp>,
    pub verified: Vec<Stamp>,
    /// Why the front matter couldn't be read, if it couldn't.
    pub problem: Option<String>,
}

/// A footnote definition naming code: `[^label]: path, locator`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cite {
    pub label: String,
    /// A repository path: a file, or a folder ending in `/`.
    pub path: String,
    /// The declaration, as written (`fn Ledger::post`), if any.
    pub locator: Option<String>,
    /// Its line in the file, from 1.
    pub line: usize,
}

/// A page as read from its file.
#[derive(Clone, Debug)]
pub struct Page {
    /// Its path in the bundle (`architecture/wiki.md`).
    pub path: String,
    pub front: Front,
    /// The front matter's text, between the `---` lines.
    pub front_text: String,
    /// Whether the file has front matter at all.
    pub has_front: bool,
    pub body: String,
    /// The body's first line in the file, from 1.
    pub body_line: usize,
    pub cites: Vec<Cite>,
    /// Footnote references in the body: (label, line).
    pub refs: Vec<(String, usize)>,
    /// Link targets in the body: (target, line).
    pub links: Vec<(String, usize)>,
}

/// Splits a file into its front matter and body: (front text, body, body line).
fn split(text: &str) -> Option<(&str, &str, usize)> {
    let rest = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n"))?;
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let front = &rest[..at];
            let body = &rest[at + line.len()..];
            return Some((front, body, front.lines().count() + 3));
        }
        at += line.len();
    }
    None
}

fn string(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn strings(y: &Yaml) -> Vec<String> {
    match y {
        Yaml::Array(items) => items.iter().filter_map(string).collect(),
        other => string(other).into_iter().collect(),
    }
}

fn stamp(y: &Yaml) -> Option<Stamp> {
    Some(Stamp { by: string(&y["by"])?, at: string(&y["at"]).unwrap_or_default() })
}

fn front(text: &str) -> Front {
    let docs = match YamlLoader::load_from_str(text) {
        Ok(d) => d,
        Err(e) => return Front { problem: Some(format!("the front matter isn't valid YAML: {e}")), ..Front::default() },
    };
    let Some(y) = docs.into_iter().next() else { return Front::default() };
    if !matches!(y, Yaml::Hash(_)) {
        return Front { problem: Some("the front matter isn't a mapping of keys".into()), ..Front::default() };
    }
    let verified = match &y["verified"] {
        Yaml::Array(items) => items.iter().filter_map(stamp).collect(),
        Yaml::Hash(_) => stamp(&y["verified"]).into_iter().collect(),
        _ => Vec::new(),
    };
    Front {
        kind: string(&y["type"]),
        title: string(&y["title"]),
        description: string(&y["description"]),
        tags: strings(&y["tags"]),
        status: string(&y["status"]),
        covers: strings(&y["covers"]).into_iter().map(|c| clean_path(&c)).collect(),
        generated: stamp(&y["generated"]),
        verified,
        problem: None,
    }
}

/// A repository path as a page writes it: no `repo://`, no leading `./` or `/`.
pub fn clean_path(p: &str) -> String {
    let p = p.trim().trim_matches('`');
    let p = p.strip_prefix("repo://").unwrap_or(p);
    let p = p.strip_prefix("./").unwrap_or(p);
    p.trim_start_matches('/').to_owned()
}

/// Reads the page at `path` (in the bundle) from its `text`.
pub fn parse(path: &str, text: &str) -> Page {
    let (front_text, body, body_line, has_front) = match split(text) {
        Some((f, b, l)) => (f, b, l, true),
        None => ("", text, 1, false),
    };
    let mut page = Page {
        path: path.to_owned(),
        front: if has_front { front(front_text) } else { Front::default() },
        front_text: front_text.to_owned(),
        has_front,
        body: body.to_owned(),
        body_line,
        cites: Vec::new(),
        refs: Vec::new(),
        links: Vec::new(),
    };
    let mut fence: Option<String> = None;
    for (n, line) in body.lines().enumerate() {
        let number = body_line + n;
        let t = line.trim_start();
        if let Some(f) = &fence {
            if t.starts_with(f.as_str()) {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(t[..3].to_owned());
            continue;
        }
        if let Some(rest) = t.strip_prefix("[^")
            && let Some((label, def)) = rest.split_once("]:")
        {
            page.cites.push(cite(label, def, number));
            continue;
        }
        let code_free = without_code_spans(line);
        let mut rest = code_free.as_str();
        while let Some(at) = rest.find("[^") {
            let after = &rest[at + 2..];
            match after.find(']') {
                Some(end) if !after[..end].contains(char::is_whitespace) => {
                    page.refs.push((after[..end].to_owned(), number));
                    rest = &after[end..];
                }
                _ => rest = after,
            }
        }
        for target in link_targets(&code_free) {
            page.links.push((target, number));
        }
    }
    page
}

fn cite(label: &str, def: &str, line: usize) -> Cite {
    let def = def.trim().trim_end_matches('.');
    let (path, locator) = match def.split_once(", ").or_else(|| def.split_once(" — ")).or_else(|| def.split_once(" - ")) {
        Some((p, l)) => (p, Some(l.trim().trim_matches('`').to_owned()).filter(|l| !l.is_empty())),
        None => (def, None),
    };
    Cite { label: label.trim().to_owned(), path: clean_path(path), locator, line }
}

/// `line` with its inline code spans blanked.
fn without_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
        } else {
            out.push(if in_code { ' ' } else { c });
        }
    }
    out
}

/// The targets of the Markdown links on a line: `[words](target)`.
fn link_targets(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("](") {
        let after = &rest[at + 2..];
        let Some(end) = after.find(')') else { break };
        let target = after[..end].split_whitespace().next().unwrap_or("");
        if !target.is_empty() {
            out.push(target.to_owned());
        }
        rest = &after[end..];
    }
    out
}

/// `a/./b/../c` → `a/c`; None when it climbs above the start.
pub fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// The page a link on page `from` leads to, as a bundle path, when it
/// leads to a page (`/workflows/runs.md`, `../runs.md#retries`).
pub fn link_page(from: &str, target: &str) -> Option<String> {
    let target = target.trim_matches(['<', '>']);
    if target.contains("://") || target.starts_with('#') || target.starts_with("mailto:") {
        return None;
    }
    let path = target.split(['#', '?']).next().unwrap_or(target);
    if !path.ends_with(".md") {
        return None;
    }
    let folder = from.rsplit_once('/').map_or("", |(f, _)| f);
    normalize(&match path.strip_prefix('/') {
        Some(abs) => abs.to_owned(),
        None => format!("{folder}/{path}"),
    })
}

/// A YAML scalar for a string, double-quoted when it needs to be.
pub fn scalar(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || " _-./+@".contains(c))
        && !s.starts_with([' ', '-', '@'])
        && !s.ends_with(' ')
        && !matches!(s, "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~")
        && s.parse::<f64>().is_err();
    if plain {
        return s.to_owned();
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `front` with each top-level key in `changes` set, or removed for None.
///
/// A value is the YAML after `key:`, which may run over indented lines.
/// Other keys and comments stay as they are; new keys go at the end.
pub fn set_keys(front: &str, changes: &[(&str, Option<String>)]) -> String {
    let mut lines: Vec<String> = front.lines().map(str::to_owned).collect();
    for (key, value) in changes {
        let starts = |l: &str| l.strip_prefix(key).is_some_and(|r| r.trim_start().starts_with(':')) && !l.starts_with(' ');
        let found = lines.iter().position(|l| starts(l));
        let new_lines: Vec<String> = value.as_ref().map(|v| format!("{key}:{}{v}", if v.starts_with('\n') { "" } else { " " }).lines().map(str::to_owned).collect()).unwrap_or_default();
        match found {
            Some(i) => {
                let mut end = i + 1;
                while end < lines.len() && (lines[end].starts_with(' ') || lines[end].starts_with('-') || lines[end].trim().is_empty()) {
                    end += 1;
                }
                lines.splice(i..end, new_lines);
            }
            None => lines.extend(new_lines),
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// A whole page file from front matter text and a body.
pub fn file(front: &str, body: &str) -> String {
    let front = front.trim_end_matches('\n');
    format!("---\n{front}\n---\n{}", body.trim_start_matches('\n'))
}

/// A list of stamps as YAML (`verified`).
pub fn stamps(list: &[Stamp]) -> String {
    list.iter().map(|s| format!("\n  - by: {}\n    at: {}", scalar(&s.by), scalar(&s.at))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "---\ntype: Workflow\ntitle: \"Workflow runs\"\ndescription: How a run moves from implement to merge.\ntags: [workflows, git]\ncovers:\n  - crates/harlo-core/src/workflow.rs\n  - repo://crates/harlo-app/src/workspace/\nverified: { by: human:burce, at: 2026-11-17T10:00:00Z }\n---\n# Workflow runs\n\nA run stops when a session asks.[^asks] See [the cards](/concepts/cards.md).\n\n```rust\nlet x = \"[^not]\";\n```\n\nUse `[^code]` in prose.\n\n[^asks]: crates/harlo-app/src/workspace/workflows.rs, fn needs_you\n[^file]: `ios/Harlo/Harlo/WorkflowSheet.swift`\n";

    #[test]
    fn reads_front_matter_and_cites() {
        let p = parse("workflows/runs.md", PAGE);
        assert_eq!(p.front.kind.as_deref(), Some("Workflow"));
        assert_eq!(p.front.title.as_deref(), Some("Workflow runs"));
        assert_eq!(p.front.tags, ["workflows", "git"]);
        assert_eq!(p.front.covers, ["crates/harlo-core/src/workflow.rs", "crates/harlo-app/src/workspace/"]);
        assert_eq!(p.front.verified, [Stamp { by: "human:burce".into(), at: "2026-11-17T10:00:00Z".into() }]);
        assert_eq!(p.body_line, 11);
        assert_eq!(p.refs, [("asks".to_owned(), 13)]);
        assert_eq!(p.links, [("/concepts/cards.md".to_owned(), 13)]);
        assert_eq!(
            p.cites,
            [
                Cite { label: "asks".into(), path: "crates/harlo-app/src/workspace/workflows.rs".into(), locator: Some("fn needs_you".into()), line: 21 },
                Cite { label: "file".into(), path: "ios/Harlo/Harlo/WorkflowSheet.swift".into(), locator: None, line: 22 },
            ]
        );
    }

    #[test]
    fn rewrites_keys_in_place() {
        let p = parse("x.md", PAGE);
        let front = set_keys(&p.front_text, &[("covers", None), ("generated", Some("{ by: harlo/1, at: \"2026-11-18T00:00:00Z\" }".into())), ("title", Some(scalar("Runs: how")))]);
        assert!(!front.contains("covers") && !front.contains("workspace/"), "{front}");
        assert!(front.contains("title: \"Runs: how\"\n") && front.ends_with("generated: { by: harlo/1, at: \"2026-11-18T00:00:00Z\" }\n"), "{front}");
        let again = parse("x.md", &file(&front, &p.body));
        assert_eq!(again.front.generated, Some(Stamp { by: "harlo/1".into(), at: "2026-11-18T00:00:00Z".into() }));
        assert_eq!(again.body, p.body);
        assert_eq!(again.front.title.as_deref(), Some("Runs: how"));
    }

    #[test]
    fn broken_front_matter_is_reported() {
        let p = parse("x.md", "---\ntitle: [unclosed\n---\nBody\n");
        assert!(p.front.problem.is_some());
        let none = parse("x.md", "# No front matter\n");
        assert!(!none.has_front && none.body.starts_with("# No"));
        assert_eq!(scalar("plain words"), "plain words");
        assert_eq!(scalar("yes"), "\"yes\"");
        assert_eq!(scalar("a: b"), "\"a: b\"");
        assert_eq!(link_page("workflows/runs.md", "../concepts/cards.md#x").as_deref(), Some("concepts/cards.md"));
        assert_eq!(link_page("workflows/runs.md", "/overview.md").as_deref(), Some("overview.md"));
        assert_eq!(link_page("runs.md", "../src/run.rs"), None);
        assert_eq!(link_page("runs.md", "https://x.y/a.md"), None);
    }
}
