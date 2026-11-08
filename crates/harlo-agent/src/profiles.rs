//! Agent profiles for `task`: Markdown files with frontmatter (`name`,
//! `description`, `tools`, `model`) whose body is added to a subagent's
//! instructions. Found in `~/.claude/agents`, harlo-agent's `agents`
//! folder, then the project's `.claude/agents` and `.harlo/agents` (a
//! later one wins by name). Read here, not through the sandbox.
//!
//! A profile can't add to what a subagent may do: its tools are mapped to
//! ours (Claude Code's names too), and capped at `general`'s; unknown ones
//! are dropped.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Of a profile's body, what's added to the instructions.
const KEPT: usize = 20_000;
/// Profiles listed in `task`'s description, at most.
pub const LISTED: usize = 30;

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub description: String,
    /// The tools it may use (of `general`'s).
    pub tools: &'static [&'static str],
    /// `provider/model`, if it names one ("" none).
    pub model: String,
    pub body: String,
    pub path: PathBuf,
}

impl Profile {
    /// Whether it changes files (a writer beside others gets a worktree).
    pub fn writes(&self) -> bool {
        self.tools.iter().any(|t| matches!(*t, "write" | "edit"))
    }
}

fn folders(cwd: &Path, home: &Path, config: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![home.join(".claude/agents"), config.join("agents")];
    let top = cwd.ancestors().find(|d| d.join(".git").exists()).unwrap_or(cwd);
    if top != home {
        dirs.extend([top.join(".claude/agents"), top.join(".harlo/agents")]);
    }
    dirs
}

pub fn find(cwd: &Path, home: &Path, config: &Path) -> Vec<Profile> {
    let mut found: Vec<Profile> = Vec::new();
    for dir in folders(cwd, home, config) {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "md")).collect();
        files.sort();
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Some(p) = parse(&text, path) else { continue };
            match found.iter_mut().find(|f| f.name == p.name) {
                Some(f) => *f = p,
                None => found.push(p),
            }
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn parse(text: &str, path: PathBuf) -> Option<Profile> {
    let rest = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n"))?;
    let (head, body) = rest.split_once("\n---")?;
    let body = body.split_once('\n').map_or("", |(_, b)| b).trim();
    let field = |k: &str| head.lines().find_map(|l| l.split_once(':').filter(|(key, _)| key.trim() == k && !l.starts_with(' ')).map(|(_, v)| v.trim().trim_matches('"').trim_matches('\'').to_owned()));
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = field("name").filter(|n| !n.is_empty()).unwrap_or(stem);
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    // A list on its line ("Read, Grep" or "[Read, Grep]"), or YAML's
    // "- Read" lines after it.
    let tools: Option<Vec<String>> = field("tools").map(|v| {
        let listed: Vec<String> = v.trim_matches(|c| c == '[' || c == ']').split(',').map(|t| t.trim().trim_matches('"').to_owned()).filter(|t| !t.is_empty()).collect();
        if !listed.is_empty() {
            return listed;
        }
        let at = head.lines().position(|l| l.starts_with("tools:")).unwrap_or(0);
        head.lines().skip(at + 1).map_while(|l| l.trim().strip_prefix("- ").map(|t| t.trim().trim_matches('"').to_owned())).collect()
    });
    let tools = match tools {
        Some(t) => intern(t.iter().filter_map(|t| ours(t)).collect()),
        None => crate::task::GENERAL,
    };
    let model = field("model").filter(|m| m.contains('/')).unwrap_or_default();
    Some(Profile { name, description: field("description").unwrap_or_default(), tools, model, body: body.chars().take(KEPT).collect(), path })
}

/// One of ours for a tool's name (Claude Code's or ours), if `general` has it.
fn ours(name: &str) -> Option<&'static str> {
    let n = match name {
        "Read" | "NotebookRead" => "read",
        "Grep" => "grep",
        "Glob" => "find",
        "LS" => "ls",
        "Bash" => "bash",
        "Edit" | "MultiEdit" | "NotebookEdit" => "edit",
        "Write" => "write",
        other => other,
    };
    crate::task::GENERAL.iter().copied().find(|g| *g == n)
}

/// A tool list kept for the process (the same list once).
fn intern(mut tools: Vec<&'static str>) -> &'static [&'static str] {
    static KNOWN: Mutex<Vec<&'static [&'static str]>> = Mutex::new(Vec::new());
    // (`current_time` and `recall` come with any.)
    for t in ["current_time", "recall"] {
        if !tools.contains(&t) {
            tools.push(t);
        }
    }
    tools.sort_unstable();
    tools.dedup();
    let mut known = KNOWN.lock().unwrap();
    if let Some(k) = known.iter().find(|k| **k == tools.as_slice()) {
        return k;
    }
    let leaked: &'static [&'static str] = Box::leak(tools.into_boxed_slice());
    known.push(leaked);
    leaked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_and_their_tools() {
        let root = std::env::temp_dir().join(format!("harlo-agent-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (home, config) = (root.join("home"), root.join("home/.config/harlo-agent"));
        let proj = home.join("proj");
        for d in [proj.join(".git"), home.join(".claude/agents"), config.join("agents"), proj.join(".claude/agents")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(home.join(".claude/agents/reviewer.md"), "---\nname: reviewer\ndescription: Reviews code\ntools: Read, Grep, Glob, WebFetch\nmodel: sonnet\n---\nYou review.\n").unwrap();
        std::fs::write(config.join("agents/fixer.md"), "---\ndescription: Fixes things\nmodel: fake/small\n---\nYou fix.\n").unwrap();
        std::fs::write(proj.join(".claude/agents/reviewer.md"), "---\nname: reviewer\ndescription: Reviews this project\ntools:\n  - Read\n  - Bash\n  - Write\n---\nProject review.\n").unwrap();
        std::fs::write(proj.join(".claude/agents/bad.md"), "no frontmatter").unwrap();
        let found = find(&proj, &home, &config);
        assert_eq!(found.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["fixer", "reviewer"]);
        let (fixer, reviewer) = (&found[0], &found[1]);
        assert_eq!((fixer.tools, fixer.model.as_str(), fixer.body.as_str()), (crate::task::GENERAL, "fake/small", "You fix."));
        assert_eq!(reviewer.description, "Reviews this project");
        assert_eq!(reviewer.tools, ["bash", "current_time", "read", "recall", "write"]);
        assert!(reviewer.writes() && reviewer.model.is_empty());
        // Unknown tools dropped, the same list kept once.
        let a = parse("---\nname: a\ntools: Read, WebFetch\n---\n", PathBuf::from("a.md")).unwrap();
        let b = parse("---\nname: b\ntools: [\"Read\"]\n---\n", PathBuf::from("b.md")).unwrap();
        assert_eq!(a.tools, ["current_time", "read", "recall"]);
        assert!(std::ptr::eq(a.tools, b.tools) && !a.writes());
        let _ = std::fs::remove_dir_all(&root);
    }
}
