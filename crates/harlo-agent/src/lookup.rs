//! The `lookup` tool: the project's code index asked about a name.
//!
//! The `harlo-index` helper answers (see `harlo_okf::indexed::lookup`):
//! the declarations named so, each with its signature, first doc line,
//! what it calls and what calls it. It's off unless `HARLO_AGENT_LOOKUP`
//! is `on` (harlo sets it for a project with Code lookup on), and offered
//! only where the helper is found.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};
use std::path::Path;

/// The tool's name.
pub const NAME: &str = harlo_okf::indexed::LOOKUP_TOOL;

/// Whether the tool is switched on (`HARLO_AGENT_LOOKUP=on`).
pub fn on() -> bool {
    std::env::var("HARLO_AGENT_LOOKUP").is_ok_and(|v| v.trim() == "on")
}

/// Whether the model gets the tool: it's on and the helper is there.
pub fn offered() -> bool {
    on() && harlo_okf::indexed::helper().is_some()
}

pub fn spec() -> Value {
    json!({ "type": "function", "name": NAME, "strict": false, "description": harlo_okf::indexed::LOOKUP_ABOUT, "parameters": harlo_okf::indexed::lookup_parameters() })
}

/// The answer for the call's `args` in the project at `cwd`, and whether it
/// failed. Lines naming a file in `hidden` (the sandbox's) are left out.
pub async fn run(args: &Value, cwd: &Path, hidden: &[String]) -> (String, bool) {
    let name = args["name"].as_str().unwrap_or_default().trim().to_owned();
    if name.is_empty() {
        return ("Give the name to look up.".to_owned(), true);
    }
    let path = args["path"].as_str().map(|p| relative(cwd, p)).filter(|p| !p.is_empty());
    let depth = args["depth"].as_u64().and_then(|d| usize::try_from(d).ok()).unwrap_or(1);
    let root = cwd.to_path_buf();
    let asked = tokio::task::spawn_blocking(move || {
        harlo_okf::indexed::lookup(&root, &harlo_okf::indexed::Lookup { name: &name, path: path.as_deref(), depth })
    })
    .await;
    match asked {
        Ok(Ok(text)) => (text.lines().filter(|l| !hidden.iter().any(|h| l.contains(h.as_str()))).collect::<Vec<_>>().join("\n"), false),
        Ok(Err(why)) => (why, true),
        Err(e) => (format!("The code index stopped: {e}"), true),
    }
}

/// `p` as the index names it: relative to `cwd`, a folder ending in `/`.
fn relative(cwd: &Path, p: &str) -> String {
    let p = p.trim();
    let base = format!("{}/", cwd.display());
    let rel = p.strip_prefix(&base).unwrap_or(p).trim_start_matches("./");
    if rel.is_empty() || rel == "." {
        return String::new();
    }
    if !rel.ends_with('/') && cwd.join(rel).is_dir() { format!("{rel}/") } else { rel.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_as_the_index_names_them() {
        let dir = std::env::temp_dir().join(format!("harlo-lookup-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        assert_eq!(relative(&dir, "src"), "src/");
        assert_eq!(relative(&dir, &format!("{}/src/a.rs", dir.display())), "src/a.rs");
        assert_eq!(relative(&dir, "./src/"), "src/");
        assert_eq!(relative(&dir, "."), "");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
