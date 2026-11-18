//! `harlo-index`: harlo's code index as a helper program.
//!
//! harlo runs it beside its agent, here or on another machine, so the
//! grammars it carries don't make every other binary bigger. It prints JSON.
//!
//! - `harlo-index version`: its version.
//! - `harlo-index file <path>`: one file's symbols, imports and calls.
//! - `harlo-index tree <path>`: its syntax tree, for writing a language's rules.
//! - `harlo-index repo <root> [--no-cache]`: every file's, read again only
//!   where a file changed since the last run (the cache is in
//!   `~/.cache/harlo/index`).
//! - `harlo-index symbols <root> [--no-cache]`: every file's symbols only,
//!   by path (what harlo's wiki checks read).
//! - `harlo-index deps <root> [--no-cache]`: the package manifests, each
//!   dependency with the files that use it.
//! - `harlo-index sites <root> [--no-cache]`: every call site, a JSON line
//!   each, with the declarations it reaches and whether that's sure (for
//!   checking the call graph against a language server).
//! - `harlo-index lookup <root> <name> [--path <file or folder/>] [--depth 1-3] [--no-cache]`:
//!   the declarations named `name` as text, each with its signature, first
//!   doc line, what it calls and what calls it (an agent's lookup tool).
//! - `harlo-index map <root> [--no-cache]`: the packages, entry points and
//!   most-called declarations, as text (for writing a wiki).
//! - `harlo-index cards <root> <path>#<name>… [--no-cache]`: those
//!   declarations' cards (signature, doc line, callers, callees), as JSON.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(&args) {
        Ok(out) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("harlo-index: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[&str]) -> Result<String, String> {
    match args {
        ["version"] => Ok(env!("CARGO_PKG_VERSION").to_owned()),
        ["file", path] => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("can't read {path}: {e}"))?;
            let index = harlo_index::scan(path, &text).ok_or_else(|| format!("no language for {path}"))?;
            serde_json::to_string(&index).map_err(|e| e.to_string())
        }
        ["tree", path] => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("can't read {path}: {e}"))?;
            harlo_index::syntax_tree(path, &text).ok_or_else(|| format!("no grammar for {path}"))
        }
        ["lookup", root, name, rest @ ..] => lookup(root, name, rest),
        ["map" | "cards", root, rest @ ..] => {
            let cache = if rest.contains(&"--no-cache") { None } else { harlo_index::default_cache() };
            let index = harlo_index::index(Path::new(root), cache.as_deref()).map_err(|e| e.to_string())?;
            let graph = harlo_index::Graph::new(&index.files).with_packages(harlo_index::Packages::new(&index.manifests));
            if args[0] == "map" {
                return Ok(harlo_index::code_map(&graph, &index.manifests));
            }
            let wanted: Vec<(String, String)> =
                rest.iter().filter(|a| **a != "--no-cache").filter_map(|a| a.split_once('#')).map(|(p, n)| (p.to_owned(), n.to_owned())).collect();
            serde_json::to_string(&harlo_index::cards(&graph, &wanted)).map_err(|e| e.to_string())
        }
        [command @ ("repo" | "symbols" | "sites" | "deps"), root, rest @ ..] => {
            let cache = if rest == ["--no-cache"] {
                None
            } else if rest.is_empty() {
                harlo_index::default_cache()
            } else {
                return Err(usage());
            };
            let index = harlo_index::index(Path::new(root), cache.as_deref()).map_err(|e| e.to_string())?;
            if *command == "deps" {
                return Ok(deps(&index));
            }
            if *command == "sites" {
                return Ok(sites(&index));
            }
            if *command == "symbols" {
                let symbols: BTreeMap<&String, &Vec<harlo_index::Symbol>> = index.files.iter().map(|(p, f)| (p, &f.symbols)).collect();
                return serde_json::to_string(&symbols).map_err(|e| e.to_string());
            }
            serde_json::to_string(&index).map_err(|e| e.to_string())
        }
        _ => Err(usage()),
    }
}

/// `lookup`'s answer, its options in `rest`.
fn lookup(root: &str, name: &str, rest: &[&str]) -> Result<String, String> {
    let mut query = harlo_index::Query { name, path: None, depth: 1 };
    let mut cache = harlo_index::default_cache();
    let mut rest = rest.iter();
    while let Some(option) = rest.next() {
        match *option {
            "--path" => query.path = Some(rest.next().ok_or_else(usage)?),
            "--depth" => query.depth = rest.next().and_then(|d| d.parse().ok()).ok_or_else(usage)?,
            "--no-cache" => cache = None,
            _ => return Err(usage()),
        }
    }
    let index = harlo_index::index(Path::new(root), cache.as_deref()).map_err(|e| e.to_string())?;
    let graph = harlo_index::Graph::new(&index.files).with_packages(harlo_index::Packages::new(&index.manifests));
    Ok(harlo_index::lookup(&graph, &query))
}

/// The manifests, each dependency with the files that use it.
fn deps(index: &harlo_index::Index) -> String {
    let packages = harlo_index::Packages::new(&index.manifests);
    let by_package = packages.files(&index.files);
    let out: Vec<serde_json::Value> = index
        .manifests
        .iter()
        .map(|m| {
            let deps: Vec<serde_json::Value> = m
                .dependencies
                .iter()
                .map(|d| {
                    let mut v = serde_json::to_value(d).unwrap_or_default();
                    let folder = m.path.rsplit_once('/').map_or("", |(f, _)| f);
                    let files = by_package.get(folder).map_or(&[][..], Vec::as_slice);
                    v["used_by"] = serde_json::json!(harlo_index::users(files, m, &d.name));
                    v
                })
                .collect();
            serde_json::json!({"path": m.path, "ecosystem": m.ecosystem, "name": m.name, "dependencies": deps})
        })
        .collect();
    serde_json::Value::Array(out).to_string()
}

/// Every call site as a JSON line: where, what, from where, and what it reaches.
fn sites(index: &harlo_index::Index) -> String {
    let graph = harlo_index::Graph::new(&index.files).with_packages(harlo_index::Packages::new(&index.manifests));
    let mut out = Vec::new();
    for (file, (path, f)) in graph.files().iter().enumerate() {
        for (call, c) in f.calls.iter().enumerate() {
            let (targets, sure) = graph.resolve(harlo_index::Site { file, call });
            let to: Vec<serde_json::Value> = targets
                .iter()
                .map(|&r| {
                    let s = graph.symbol(r);
                    serde_json::json!({"path": graph.path(r), "name": s.name, "start": s.start, "end": s.end})
                })
                .collect();
            let from = c.from.map(|i| f.symbols[i].name.as_str());
            out.push(
                serde_json::json!({"path": path, "line": c.line, "column": c.column, "name": c.name, "receiver": c.receiver, "from": from, "to": to, "sure": sure})
                    .to_string(),
            );
        }
    }
    out.join("\n")
}

fn usage() -> String {
    "usage: harlo-index version | file <path> | tree <path> | repo <root> [--no-cache] | symbols <root> [--no-cache] | sites <root> [--no-cache] | deps <root> [--no-cache] | lookup <root> <name> [--path <p>] [--depth 1-3] [--no-cache] | map <root> [--no-cache] | cards <root> <path>#<name>… [--no-cache]".to_owned()
}
