//! What the Wiki panel and the phone show of a project's wiki.
//!
//! All of it is read from the files, with no model and nothing written,
//! by [`harlo_okf::view`], on the repository's machine: here, or through
//! harlo's agent on another (`harlo-agent wiki --okf`). What's left here
//! is a page laid out for reading (its Markdown in blocks), and the offer
//! to update the wiki as the default branch moves. The work list reads
//! every file in scope, so these run off the UI thread.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::time::Duration;

use harlo_okf::bundle;
use harlo_okf::work::{self, Item};
use serde_json::{Value, json};

use crate::remote;
use crate::wiki::root_of;

/// How long a host gets to answer a request of its wiki.
const HOST_TIMEOUT: Duration = Duration::from_secs(120);

/// Asks the wiki of the repository `folder` is in, here or on its machine.
///
/// `request` is one of [`harlo_okf::view::answer`]'s, without its `root`.
/// A machine that can't be reached, or whose agent can't be started,
/// answers `{ error }`.
pub fn ask(folder: &Path, mut request: Value) -> Value {
    let root = root_of(folder);
    match remote::split(&root) {
        None => {
            request["root"] = Value::String(root.to_string_lossy().into_owned());
            harlo_okf::view::answer(&request)
        }
        Some((host, dir)) => {
            request["root"] = Value::String(dir);
            on_host(&host, &request).unwrap_or_else(|e| json!({ "error": format!("{e:#}") }))
        }
    }
}

/// A request answered by harlo's agent on `host_name`.
fn on_host(host_name: &str, request: &Value) -> anyhow::Result<Value> {
    let host = remote::host(host_name).ok_or_else(|| anyhow::anyhow!("No remote instance named {host_name}"))?;
    let agent = crate::agent_host::program_on(host_name)?;
    let out = remote::run_with_input(&host, &format!("exec {} wiki --okf", remote::quote_path(&agent)), Some(request.to_string().as_bytes()), HOST_TIMEOUT)?;
    if !out.status.success() {
        anyhow::bail!("{}", remote::ssh_error(host_name, &out));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| anyhow::anyhow!("{host_name}'s harlo-agent didn't answer about the wiki ({e})"))
}

/// The wiki of the repository `folder` is in.
///
/// Its shape is [`harlo_okf::view::state`]'s, `root` as harlo names it (a
/// host's included); with `error` when its machine didn't answer.
pub fn state(folder: &Path) -> Value {
    let mut v = ask(folder, json!({ "op": "state" }));
    let root = root_of(folder).to_string_lossy().into_owned();
    if v.get("error").is_some() {
        return json!({ "root": root, "exists": false, "pages": [], "error": v["error"] });
    }
    v["root"] = Value::String(root);
    v
}

/// A wiki page, laid out for reading.
///
/// Its shape is [`harlo_okf::view::page`]'s, with `blocks` (`{ kind, html,
/// code, lang, anchor }`) for its `body`, and `you` (the person here, as
/// an OKF actor); `error` when there's no such page or it couldn't be read.
pub fn page_view(folder: &Path, path: &str) -> Value {
    let mut v = ask(folder, json!({ "op": "page", "path": path }));
    if v.get("error").is_some() {
        v["path"] = Value::String(path.trim_start_matches(".okf/").trim_start_matches('/').to_owned());
        return v;
    }
    let mut renderer = crate::markdown::MarkdownRenderer::default();
    let blocks: Vec<Value> = renderer
        .render(v["body"].as_str().unwrap_or_default())
        .iter()
        .map(|b| {
            let kind = match b.kind {
                crate::markdown::MdBlockKind::Code if b.lang == "mermaid" => "mermaid",
                crate::markdown::MdBlockKind::Code => "code",
                crate::markdown::MdBlockKind::Prose => "prose",
            };
            let heading = b.html.starts_with("<h") && b.html.as_bytes().get(2).is_some_and(u8::is_ascii_digit);
            let anchor = if heading { harlo_okf::view::anchor(&crate::markdown::plain_text(&b.html)) } else { String::new() };
            json!({ "kind": kind, "html": b.html, "code": b.code, "lang": b.lang, "anchor": anchor })
        })
        .collect();
    if let Some(o) = v.as_object_mut() {
        o.remove("body");
        o.insert("blocks".into(), Value::Array(blocks));
        o.insert("you".into(), Value::String(crate::wiki::human()));
    }
    v
}

/// A search of the pages' sections ([`harlo_okf::view::search`]).
pub fn search(folder: &Path, query: &str, limit: usize) -> Value {
    let v = ask(folder, json!({ "op": "search", "query": query, "limit": limit }));
    if v.get("hits").is_some() { v } else { json!({ "hits": [], "workspaces": [], "error": v["error"] }) }
}

/// A page's work, in words, for a session that checks just that page.
pub fn page_report(folder: &Path, path: &str) -> String {
    ask(folder, json!({ "op": "pageReport", "path": path }))["text"].as_str().unwrap_or_default().to_owned()
}

/// Merged work the wiki should take in, offered as the default branch moves.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeOffer {
    pub branch: String,
    pub tip: String,
    /// Commits on the branch since it was last seen.
    pub commits: u32,
    /// What the wiki owes now, and the new declarations' names (a few).
    pub owed: usize,
    pub new: Vec<String>,
}

/// The repository's default branch here: the remote's HEAD, else a main
/// or master it has.
fn default_branch(root: &Path) -> Option<String> {
    let head = crate::git::output_in(root, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]);
    if let Some((_, b)) = head.trim().split_once('/').filter(|(_, b)| !b.is_empty()) {
        return Some(b.to_owned());
    }
    ["main", "master"].into_iter().find(|b| !crate::git::output_in(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{b}")]).trim().is_empty()).map(str::to_owned)
}

/// Names of new declarations an offer shows, at most.
const OFFER_NAMES: usize = 4;

/// Whether to offer updating the wiki as the default branch moves.
///
/// For the wiki under `folder`: returns the branch's tip (to pass as `seen` next time; "" while
/// this checkout lacks it) and the offer, if the wiki owes the code
/// something now. Nothing the first time (no `seen`): the branch is only
/// noted.
pub fn merge_offer(folder: &Path, seen: &str) -> (String, Option<MergeOffer>) {
    let root = root_of(folder);
    if remote::is_remote(&root) || !bundle::exists(&root) {
        return (String::new(), None);
    }
    let Some(branch) = default_branch(&root) else { return (String::new(), None) };
    let tip = crate::git::output_in(&root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).trim().to_owned();
    if tip.is_empty() || tip == seen || seen.is_empty() {
        return (tip, None);
    }
    // (Not in this checkout yet: not seen either, so the offer comes once it is.)
    if crate::git::output_in(&root, &["merge-base", &tip, "HEAD"]).trim() != tip {
        return (String::new(), None);
    }
    let (Ok(b), Ok(ledger)) = (bundle::load(&root), harlo_okf::ledger::read(&root)) else { return (tip, None) };
    let owed = work::work(&b, &ledger);
    if owed.is_empty() {
        return (tip, None);
    }
    let commits = crate::git::output_in(&root, &["rev-list", "--count", &format!("{seen}..{tip}")]).trim().parse().unwrap_or(0);
    let new: Vec<String> = owed.pages.values().flatten().filter_map(|i| if let Item::New { name, .. } = i { Some(name.clone()) } else { None }).take(OFFER_NAMES).collect();
    (tip.clone(), Some(MergeOffer { branch, tip, commits, owed: owed.len(), new }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_laid_out_in_blocks() {
        let root = std::env::temp_dir().join(format!("harlo-core-wiki-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |p: &str, t: &str| {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        };
        write("src/run.rs", "pub fn run() {\n    go();\n}\n");
        write(".okf/runs.md", "---\ntype: Concept\ntitle: Runs\ncovers: [src/]\n---\n# Retry control\n\nIt goes.[^run]\n\n```\n[^x]: kept in code\n```\n\n[^run]: src/run.rs, fn run\n");
        let v = page_view(&root, "runs.md");
        let st = state(&root);
        let missing = page_view(&root, "nope.md");
        std::fs::remove_dir_all(&root).unwrap();
        let blocks = v["blocks"].as_array().unwrap();
        let html: String = blocks.iter().map(|b| b["html"].as_str().unwrap_or_default()).collect();
        assert!(html.contains("okf-fn:run") && !html.contains("src/run.rs, fn run"), "{html}");
        assert_eq!(blocks[0]["anchor"], "retry-control");
        assert!(blocks.iter().any(|b| b["code"].as_str().unwrap_or_default().contains("[^x]: kept in code")));
        assert!(v.get("body").is_none() && v["you"].as_str().unwrap().starts_with("human:"));
        assert_eq!(st["root"], root.to_string_lossy().as_ref());
        assert_eq!(missing["error"], "The wiki has no page nope.md.");
    }
}
