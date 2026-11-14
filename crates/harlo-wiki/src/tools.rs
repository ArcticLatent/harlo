//! The wiki's tools as OpenWiki defines them (names, descriptions, input
//! schemas, answers and errors), for harlo's sessions and its MCP server.

use crate::retrieval::{self, ReadRequest, SearchRequest};
use crate::store::Error;
use crate::workspaces;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// All ten tools as OpenWiki lists them.
pub static ALL: LazyLock<Vec<Value>> = LazyLock::new(|| serde_json::from_str(include_str!("tools.json")).unwrap());

pub const READING: [&str; 4] = ["openwiki_list_workspaces", "openwiki_list_wikis", "openwiki_search", "openwiki_read"];

pub fn tool(name: &str) -> Option<&'static Value> {
    ALL.iter().find(|t| t["name"] == name)
}

/// The four reading tools' definitions.
pub fn reading() -> Vec<Value> {
    READING.iter().filter_map(|n| tool(n).cloned()).collect()
}

/// A tool's answer: Ok(the result), Err(the error text).
pub type Answer = Result<Value, String>;

/// What a host shows for an error (OpenWiki's `code: message`).
pub fn error_text(op: &str, e: Error) -> String {
    match e {
        Error::Host(code, m) => format!("{code}: {m}"),
        Error::Retrieval(m) => format!("invalid_input: {m}"),
        Error::Workspace(m) => format!("invalid_state: {m}"),
        Error::PageMissing(_) if op == "read" => "invalid_input: The requested OpenWiki page does not exist.".into(),
        Error::PageMissing(_) | Error::Claims(_) | Error::ClaimsSecurity(_) if op != "run" => {
            format!("invalid_state: Unable to {op} the repository OpenWiki safely. Check the wiki and retry.")
        }
        Error::Run(code, m) => format!("{code}: {m}"),
        _ => "OpenWiki MCP operation failed.".into(),
    }
}

// ---- Input validation (OpenWiki's schemas: strings trimmed, then checked) ----

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn join_path(path: &str, key: &str) -> String {
    if path.is_empty() { key.to_owned() } else { format!("{path}.{key}") }
}

/// Checks a value as OpenWiki's schema library does, collecting every
/// issue (`message at path`), and returns it with its strings trimmed.
fn check(schema: &Value, v: Option<&Value>, path: &str, issues: &mut Vec<String>) -> Value {
    let mut issue = |m: String| issues.push(if path.is_empty() { m } else { format!("{m} at {path}") });
    let expected = match schema["type"].as_str() {
        Some("integer") => "int",
        Some(t) => t,
        None => return v.cloned().unwrap_or(Value::Null),
    };
    let Some(v) = v else {
        issue(format!("Invalid input: expected {expected}, received undefined"));
        return Value::Null;
    };
    match expected {
        "object" => {
            let Some(o) = v.as_object() else {
                issue(format!("Invalid input: expected object, received {}", type_name(v)));
                return Value::Null;
            };
            let props = schema["properties"].as_object().cloned().unwrap_or_default();
            let required: Vec<&str> = schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let mut out = Map::new();
            let mut inner = Vec::new();
            for (k, s) in &props {
                match o.get(k) {
                    Some(x) => {
                        let c = check(s, Some(x), &join_path(path, k), &mut inner);
                        out.insert(k.clone(), c);
                    }
                    None if required.contains(&k.as_str()) => {
                        check(s, None, &join_path(path, k), &mut inner);
                    }
                    None => {}
                }
            }
            let unknown: Vec<String> = o.keys().filter(|k| !props.contains_key(*k)).map(|k| format!("\"{k}\"")).collect();
            drop(issue);
            issues.extend(inner);
            if !unknown.is_empty() {
                let m = format!("Unrecognized key{}: {}", if unknown.len() == 1 { "" } else { "s" }, unknown.join(", "));
                issues.push(if path.is_empty() { m } else { format!("{m} at {path}") });
            }
            Value::Object(out)
        }
        "array" => {
            let Some(a) = v.as_array() else {
                issue(format!("Invalid input: expected array, received {}", type_name(v)));
                return Value::Null;
            };
            let n = a.len() as u64;
            let mut size = None;
            if let Some(m) = schema["minItems"].as_u64().filter(|m| n < *m) {
                size = Some(format!("Too small: expected array to have >={m} items"));
            }
            if let Some(m) = schema["maxItems"].as_u64().filter(|m| n > *m) {
                size = Some(format!("Too big: expected array to have <={m} items"));
            }
            drop(issue);
            let items: Vec<Value> = a.iter().enumerate().map(|(i, x)| check(&schema["items"], Some(x), &format!("{path}[{i}]"), issues)).collect();
            if let Some(m) = size {
                issues.push(if path.is_empty() { m } else { format!("{m} at {path}") });
            }
            Value::Array(items)
        }
        "string" => {
            let Some(s) = v.as_str() else {
                issue(format!("Invalid input: expected string, received {}", type_name(v)));
                return Value::Null;
            };
            if let Some(e) = schema["enum"].as_array() {
                if !e.iter().any(|x| x == s) {
                    let opts: Vec<String> = e.iter().map(|x| x.to_string()).collect();
                    issue(format!("Invalid option: expected one of {}", opts.join("|")));
                }
                return Value::String(s.to_owned());
            }
            if let Some(p) = schema["pattern"].as_str() {
                if !regex::Regex::new(p).map(|r| r.is_match(s)).unwrap_or(false) {
                    issue("Invalid UUID".into());
                }
                return Value::String(s.to_owned());
            }
            let s = crate::jsre::trim(s).to_owned();
            let n = crate::jsre::len16(&s) as u64;
            if let Some(m) = schema["minLength"].as_u64().filter(|m| n < *m) {
                issue(format!("Too small: expected string to have >={m} characters"));
            }
            if let Some(m) = schema["maxLength"].as_u64().filter(|m| n > *m) {
                issue(format!("Too big: expected string to have <={m} characters"));
            }
            Value::String(s)
        }
        "int" => {
            let Some(f) = v.as_f64() else {
                issue(format!("Invalid input: expected int, received {}", type_name(v)));
                return Value::Null;
            };
            if f.fract() != 0.0 {
                issue("Invalid input: expected int, received number".into());
                return v.clone();
            }
            if let Some(m) = schema["minimum"].as_f64().filter(|m| f < *m) {
                issue(format!("Too small: expected number to be >={}", crate::front::js_number(m)));
            }
            if let Some(m) = schema["maximum"].as_f64().filter(|m| f > *m) {
                issue(format!("Too big: expected number to be <={}", crate::front::js_number(m)));
            }
            v.clone()
        }
        "boolean" => {
            if !v.is_boolean() {
                issue(format!("Invalid input: expected boolean, received {}", type_name(v)));
            }
            v.clone()
        }
        _ => v.clone(),
    }
}

/// A tool's input, checked against its schema (strings trimmed).
pub fn validate(name: &str, args: &Value) -> Result<Value, String> {
    let t = tool(name).ok_or_else(|| format!("Tool {name} not found"))?;
    let args = if args.is_null() { json!({}) } else { args.clone() };
    let mut issues = Vec::new();
    let checked = check(&t["inputSchema"], Some(&args), "", &mut issues);
    if issues.is_empty() {
        return Ok(checked);
    }
    Err(format!("MCP error -32602: Input validation error: Invalid arguments for tool {name}: {}", issues.join("\n")))
}

// ---- Repository roots ----

/// The canonical Git top level a tool's `root` names (refusing `/` and home).
pub fn repository_root(candidate: &str) -> Result<PathBuf, Error> {
    let invalid = |m: &str| Error::Host("invalid_input", m.into());
    if !candidate.starts_with('/') {
        return Err(invalid("The OpenWiki root must be an absolute path inside a Git repository."));
    }
    let existing = |p: &Path| -> Result<PathBuf, Error> {
        let real = std::fs::canonicalize(p).map_err(|_| invalid("The OpenWiki root must be an existing directory."))?;
        match std::fs::symlink_metadata(&real) {
            Ok(m) if m.is_dir() => Ok(real),
            Ok(_) => Err(invalid("The OpenWiki root must be a directory.")),
            Err(_) => Err(invalid("The OpenWiki root must be an existing directory.")),
        }
    };
    let dir = existing(Path::new(candidate))?;
    let git = run_git(&dir).ok_or_else(|| invalid("The OpenWiki root must be inside a Git repository."))?;
    let top = git.trim_matches(['\n', '\r', ' ', '\t']);
    if top.is_empty() || !top.starts_with('/') {
        return Err(invalid("The OpenWiki root must be inside a Git repository."));
    }
    let root = existing(Path::new(top))?;
    let home = std::fs::canonicalize(workspaces::home_dir()).unwrap_or_else(|_| workspaces::home_dir());
    if root == Path::new("/") || root == home {
        return Err(invalid("OpenWiki refuses to use the filesystem root or home directory as a repository root."));
    }
    Ok(root)
}

fn run_git(dir: &Path) -> Option<String> {
    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                let mut out = String::new();
                std::io::Read::read_to_string(child.stdout.as_mut()?, &mut out).ok()?;
                return status.success().then_some(out);
            }
            None if start.elapsed() > std::time::Duration::from_secs(10) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(2)),
        }
    }
}

// ---- Calls ----

fn s<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str)
}

/// One reading tool's answer (after validation).
pub fn call_reading(name: &str, args: &Value) -> Option<Answer> {
    let op = match name {
        "openwiki_list_workspaces" | "openwiki_list_wikis" => "list",
        "openwiki_search" => "search",
        "openwiki_read" => "read",
        _ => return None,
    };
    let args = match validate(name, args) {
        Ok(a) => a,
        Err(e) => return Some(Err(e)),
    };
    let run = || -> Result<Value, Error> {
        let root = repository_root(s(&args, "root").unwrap_or(""))?;
        match name {
            "openwiki_list_workspaces" => workspaces::list_workspaces(&root, s(&args, "wiki")),
            "openwiki_list_wikis" => workspaces::list_wikis(&root, s(&args, "workspace").unwrap_or("")),
            "openwiki_search" => retrieval::search(
                &root,
                &SearchRequest {
                    query: s(&args, "query").unwrap_or(""),
                    paths: args.get("paths").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()),
                    limit: args.get("limit").and_then(Value::as_f64),
                    workspace: s(&args, "workspace"),
                },
            ),
            _ => retrieval::read(
                &root,
                &ReadRequest {
                    page: s(&args, "page").unwrap_or(""),
                    sections: args["sections"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
                    wiki: s(&args, "wiki"),
                    include_provenance: args.get("includeProvenance").and_then(Value::as_bool).unwrap_or(false),
                },
            ),
        }
    };
    Some(run().map_err(|e| error_text(op, e)))
}
