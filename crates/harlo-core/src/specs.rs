//! Specs: a project's intent, kept beside its code so it outlives the
//! chats. A spec is a `SPEC.md` in a folder, saying what that part is
//! for, how it behaves, where its edges are and why it's built that way;
//! the repository's root `SPEC.md` does so for the whole product.
//!
//! With specs on for a project (its settings), its sessions get harlo's
//! spec tools (`spec_list`, `spec_search`, `spec_read`, through harlo's
//! MCP bridge, for every agent) and instructions to read the specs before
//! changing code and to keep them true in the same change. Agents write
//! specs with their own file tools, so those edits show in the Changes
//! panel and undo like any other. The Specs panel lists them.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

pub const FILE: &str = "SPEC.md";

/// What a spec says it is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spec {
    /// From the repository's root ("SPEC.md", "crates/core/SPEC.md").
    pub path: String,
    pub title: String,
    pub summary: String,
}

/// Folders never searched (without git to say what's ignored).
const SKIP: [&str; 9] = ["node_modules", "target", "dist", "build", "vendor", ".venv", "venv", "__pycache__", ".git"];

/// The repository a folder is in (its top), else the folder; here or on
/// its host.
pub fn root_of(folder: &Path) -> PathBuf {
    crate::git::repo_root(folder).unwrap_or_else(|| folder.to_path_buf())
}

/// The specs under `root`: what git tracks or would (ignored files left
/// out), else a walk, never in dependencies; the root's first, then by
/// path.
pub fn find(root: &Path) -> Vec<Spec> {
    load(root).iter().map(|(p, text)| describe(p, text)).collect()
}

/// The specs' texts under `root`, ordered as [`find`] lists them: `(path
/// from the root, text)`. On another machine, all in one command.
fn load(root: &Path) -> Vec<(String, String)> {
    let mut texts: Vec<(String, String)> = match crate::remote::split(root) {
        Some((host, dir)) => load_on(&host, &dir),
        None => {
            let out = Command::new("git").arg("-C").arg(root).args(["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", FILE, &format!("**/{FILE}")]).output();
            let paths: Vec<String> = match out {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).split('\0').filter(|p| !p.is_empty() && root.join(p).is_file()).map(str::to_owned).collect(),
                _ => {
                    let mut found = Vec::new();
                    walk(root, root, 0, &mut found);
                    found
                }
            };
            paths.into_iter().map(|p| {
                let text = std::fs::read_to_string(root.join(&p)).unwrap_or_default();
                (p, text)
            }).collect()
        }
    };
    texts.retain(|(p, _)| !p.split('/').any(|c| SKIP.contains(&c)));
    texts.sort_by_key(|(p, _)| (p != FILE, p.matches('/').count(), p.clone()));
    texts.dedup_by(|a, b| a.0 == b.0);
    texts
}

/// [`load`] on `host`: each spec's path, then its text (at most 256 KB).
fn load_on(host: &str, dir: &str) -> Vec<(String, String)> {
    let Some(host) = crate::remote::host(host) else { return Vec::new() };
    let q = crate::remote::quote;
    let prune = SKIP.iter().map(|d| format!("-name {}", q(d))).collect::<Vec<_>>().join(" -o ");
    let script = format!(
        r#"cd {dir} || exit 0
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then git -c core.quotePath=false ls-files --cached --others --exclude-standard -- {file} '**/{file}'
else find . -maxdepth 13 \( {prune} -o -name '.?*' \) -prune -o -name {file} -type f -print | sed 's|^\./||'; fi | head -n 2000 | while IFS= read -r f; do [ -f "$f" ] || continue; printf '%s\037' "$f"; head -c 262144 -- "$f"; printf '\036'; done; true"#,
        dir = q(dir),
        file = FILE,
    );
    let Ok(out) = crate::remote::run(&host, &script, std::time::Duration::from_secs(60)) else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .split('\u{1e}')
        .filter_map(|r| r.split_once('\u{1f}'))
        .map(|(p, t)| (p.trim_start_matches('\n').to_owned(), t.to_owned()))
        .collect()
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > 12 || out.len() > 2000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        if path.is_dir() {
            if !name.starts_with('.') && !SKIP.contains(&name.as_str()) {
                walk(root, &path, depth + 1, out);
            }
        } else if name == FILE
            && let Ok(rel) = path.strip_prefix(root)
        {
            out.push(rel.to_string_lossy().into_owned());
        }
    }
}

/// Its title (frontmatter `title:`, else its first heading, else its
/// folder's name) and summary (frontmatter `summary:`, else its first
/// paragraph).
fn describe(rel: &str, text: &str) -> Spec {
    let (front, body) = split_front(text);
    let field = |k: &str| front.lines().find_map(|l| l.strip_prefix(&format!("{k}:"))).map(|v| v.trim().trim_matches(['"', '\'']).to_owned()).filter(|v| !v.is_empty());
    let heading = body.lines().find_map(|l| l.strip_prefix("# ")).map(|h| h.trim().to_owned());
    let folder = Path::new(rel).parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    let title = field("title").or(heading).or(folder).unwrap_or_else(|| "The product".into());
    let paragraph = body
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty() && !p.starts_with('#') && !p.starts_with("```") && !p.starts_with('|') && !p.starts_with("<!--"))
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "));
    let summary = field("summary").or(paragraph).unwrap_or_default();
    let summary = if summary.chars().count() > 240 { format!("{}…", summary.chars().take(239).collect::<String>()) } else { summary };
    Spec { path: rel.to_owned(), title, summary }
}

/// `(frontmatter, the rest)` of a Markdown file.
fn split_front(text: &str) -> (&str, &str) {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    if let Some(rest) = t.strip_prefix("---\n").or_else(|| t.strip_prefix("---\r\n"))
        && let Some(end) = rest.find("\n---")
    {
        let after = &rest[end + 4..];
        return (&rest[..end], after.split_once('\n').map_or("", |(_, b)| b));
    }
    ("", t)
}

/// Lines of the specs that have all of `query`'s words (any case), with
/// the heading they're under: at most `limit`.
pub fn search(root: &Path, query: &str, limit: usize) -> Vec<Value> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (path, text) in load(root) {
        let mut heading = String::new();
        for (i, line) in text.lines().enumerate() {
            if line.starts_with('#') {
                heading = line.trim_start_matches('#').trim().to_owned();
            }
            let lower = line.to_lowercase();
            if words.iter().all(|w| lower.contains(w.as_str())) {
                hits.push(json!({ "spec": path, "line": i + 1, "section": heading, "text": line.trim() }));
                if hits.len() >= limit {
                    return hits;
                }
            }
        }
    }
    hits
}

/// A spec's text, by its path from the root (only a SPEC.md inside it).
pub fn read(root: &Path, rel: &str) -> Result<String, String> {
    let rel = rel.trim().trim_start_matches("./");
    let rel = if rel.is_empty() || rel == "." { FILE.to_owned() } else if rel.ends_with(FILE) { rel.to_owned() } else { format!("{}/{FILE}", rel.trim_end_matches('/')) };
    if rel.split('/').any(|c| c == "..") || rel.starts_with('/') {
        return Err("Give the spec's path from the repository's root.".into());
    }
    let missing = || format!("There's no {rel}. spec_list lists the specs.");
    if crate::remote::is_remote(root) {
        return crate::remote::read(&root.join(&rel)).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|_| missing());
    }
    std::fs::read_to_string(root.join(&rel)).map_err(|_| missing())
}

/// What harlo tells agents in a project with specs on.
pub const INSTRUCTIONS: &str = "This project keeps specs: SPEC.md files beside the code, one in a folder for that part (what it's for, how it behaves, its boundaries, and why it's built that way), and one at the repository's root for the whole product. They're the project's memory between sessions. Before changing code, read the specs of what you'll touch (spec_list, spec_search, spec_read). When your change alters behavior, a boundary or a decision, update the spec in the same change with your file tools; when you add a part that deserves one, write its SPEC.md. Keep specs short, current and about intent, not a copy of the code.";

/// The spec tools, as MCP lists them.
pub fn tools() -> Value {
    let s = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    json!([
        {
            "name": "spec_list",
            "description": format!("The project's specs (SPEC.md files: what each part is for, how it behaves, and why), each with its title and summary. Look here before changing code. {INSTRUCTIONS}"),
            "inputSchema": s(json!({}), &[]),
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "spec_search",
            "description": "Finds the lines of the project's specs that have all of the words given (any case), with their spec and section.",
            "inputSchema": s(json!({ "query": { "type": "string", "description": "Words to find." } }), &["query"]),
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "spec_read",
            "description": "A spec's whole text, by its path from the repository's root (\"SPEC.md\", \"src/auth/SPEC.md\", or just the folder).",
            "inputSchema": s(json!({ "path": { "type": "string" } }), &["path"]),
            "annotations": { "readOnlyHint": true },
        }
    ])
}

/// A spec tool's call in the repository at `root`.
pub fn call(root: &Path, tool: &str, args: &Value) -> Result<Value, String> {
    match tool {
        "spec_list" => {
            let list: Vec<Value> = find(root).into_iter().map(|s| json!({ "path": s.path, "title": s.title, "summary": s.summary })).collect();
            if list.is_empty() {
                return Ok(json!({ "specs": [], "note": "No specs yet. Write the root SPEC.md (the product: its goal, its users, its main parts and decisions) and one for each main part as you work on them." }));
            }
            Ok(json!({ "specs": list }))
        }
        "spec_search" => Ok(json!({ "matches": search(root, args["query"].as_str().unwrap_or_default(), 40) })),
        "spec_read" => read(root, args["path"].as_str().unwrap_or_default()).map(|text| json!({ "text": text })),
        _ => Err(format!("Unknown tool {tool}.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("harlo-specs-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["src/auth", "node_modules/x", "docs"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("SPEC.md"), "---\ntitle: Shop\nsummary: Sells socks online.\n---\n# Shop\n\nIgnored paragraph.\n").unwrap();
        std::fs::write(dir.join("src/auth/SPEC.md"), "# Sign-in\n\nWho may sign in,\nand how long a session lasts.\n\n## Decisions\n\nTokens expire after 30 days: phones stay signed in.\n").unwrap();
        std::fs::write(dir.join("docs/SPEC.md"), "No heading here, just text.\n").unwrap();
        std::fs::write(dir.join("node_modules/x/SPEC.md"), "# Not ours\n").unwrap();
        dir
    }

    #[test]
    fn finds_and_describes_specs() {
        let dir = repo();
        // No git: a walk, skipping dependencies.
        let specs = find(&dir);
        assert_eq!(specs, vec![
            Spec { path: "SPEC.md".into(), title: "Shop".into(), summary: "Sells socks online.".into() },
            Spec { path: "docs/SPEC.md".into(), title: "docs".into(), summary: "No heading here, just text.".into() },
            Spec { path: "src/auth/SPEC.md".into(), title: "Sign-in".into(), summary: "Who may sign in, and how long a session lasts.".into() },
        ]);
        // In git: what it tracks or would, ignored files left out.
        let git = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(dir.join(".gitignore"), "docs/\n").unwrap();
        assert_eq!(find(&dir).iter().map(|s| s.path.as_str()).collect::<Vec<_>>(), ["SPEC.md", "src/auth/SPEC.md"]);
        assert_eq!(root_of(&dir.join("src/auth")).canonicalize().unwrap(), dir.canonicalize().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn searches_and_reads_specs() {
        let dir = repo();
        let hits = search(&dir, "TOKENS days", 10);
        assert_eq!(hits, vec![json!({ "spec": "src/auth/SPEC.md", "line": 8, "section": "Decisions", "text": "Tokens expire after 30 days: phones stay signed in." })]);
        assert!(search(&dir, "  ", 10).is_empty());
        assert!(read(&dir, "src/auth").unwrap().starts_with("# Sign-in"));
        assert!(read(&dir, "./src/auth/SPEC.md").is_ok());
        assert!(read(&dir, "").unwrap().contains("title: Shop"));
        assert!(read(&dir, "../etc").is_err());
        assert!(read(&dir, "src").unwrap_err().contains("There's no src/SPEC.md"));
        let list = call(&dir, "spec_list", &json!({})).unwrap();
        assert_eq!(list["specs"][2]["title"], "Sign-in");
        assert_eq!(call(&dir, "spec_read", &json!({ "path": "docs" })).unwrap()["text"], "No heading here, just text.\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same on another machine (`HARLO_TEST_SSH`, e.g. "ssh myhost"):
    /// a folder of its own there, without git and then with it.
    #[test]
    #[ignore]
    fn remote_specs() {
        let args = crate::remote::parse_ssh_command(&std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH")).unwrap();
        let host = crate::remote::Host { name: "t".into(), ssh: args };
        crate::remote::set_hosts(vec![host.clone()]);
        let home = crate::remote::home(&host).unwrap();
        let dir = format!("{home}/harlo-specs-test-{}", std::process::id());
        let run = |script: &str| crate::remote::run(&host, script, std::time::Duration::from_secs(30)).unwrap();
        run(&format!(
            "mkdir -p {d}/src/auth {d}/node_modules/x {d}/.hidden && printf '# The product\\n\\nDoes things.\\n' > {d}/SPEC.md && printf -- '---\\ntitle: Sign-in\\n---\\nWho may enter.\\n' > {d}/src/auth/SPEC.md && echo no > {d}/node_modules/x/SPEC.md && echo no > {d}/.hidden/SPEC.md",
            d = crate::remote::quote(&dir)
        ));
        let root = crate::remote::join("t", &dir);
        let check = || {
            let found = find(&root);
            assert_eq!(found.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(), ["SPEC.md", "src/auth/SPEC.md"]);
            assert_eq!(found[0].title, "The product");
            assert_eq!(found[1].title, "Sign-in");
            assert_eq!(search(&root, "who ENTER", 5)[0]["spec"], "src/auth/SPEC.md");
            assert_eq!(read(&root, "src/auth").unwrap(), "---\ntitle: Sign-in\n---\nWho may enter.\n");
        };
        assert_eq!(root_of(&root), root);
        check();
        run(&format!("cd {} && git init -q && printf 'node_modules\\n.hidden\\n' > .gitignore", crate::remote::quote(&dir)));
        assert_eq!(root_of(&root.join("src")), root);
        check();
        run(&format!("rm -rf {}", crate::remote::quote(&dir)));
    }
}
