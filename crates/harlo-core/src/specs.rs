//! Specs: a project's intent, kept beside its code so it outlives the
//! chats. A spec is a `SPEC.md` in a folder, saying what that part is
//! for, where its edges are and why it's built that way; the repository's
//! root `SPEC.md` does so for the whole product.
//!
//! They form a graph: each spec's parent is the nearest spec in a folder
//! above it (or its frontmatter `parent`), so the tree follows the code
//! with no frontmatter at all; `depends-on`, `references` and
//! `implements` link parts across it, by a spec's `id` or folder.
//!
//! With specs on for a project (its settings), its sessions get harlo's
//! spec tools (`spec_graph`, `spec_search`, `spec_read`, `spec_validate`,
//! through harlo's MCP bridge, for every agent) and instructions to look
//! up the spec that governs their work and keep it true in the same
//! change. The tools only read: agents write specs with their own file
//! tools, so those edits are approved, show in the Changes panel and undo
//! like any other. The Specs panel shows the tree.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

pub const FILE: &str = "SPEC.md";

/// What a spec says it is, and where it sits in the graph.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spec {
    /// From the repository's root ("SPEC.md", "crates/core/SPEC.md").
    pub path: String,
    /// Its frontmatter `id`, else its folder ("crates/core"; the root's
    /// "root"): what links name (a folder names it too).
    pub id: String,
    pub title: String,
    pub summary: String,
    /// Its `type` ("product", "architecture", "module", "task", …; "" when
    /// it doesn't say).
    pub kind: String,
    /// Its `status` (draft, active, stale, done, deprecated; "" when it
    /// doesn't say).
    pub status: String,
    /// Its parent's id: its `parent`, else the nearest spec in a folder
    /// above it (none for the top).
    pub parent: Option<String>,
    /// Its links to other specs, by id: `(depends-on | references |
    /// implements, id)`.
    pub links: Vec<(String, String)>,
    pub tags: Vec<String>,
}

/// The kinds of link a spec can have to another, besides its parent.
pub const LINKS: [&str; 3] = ["depends-on", "references", "implements"];
/// What a spec's `status` can say.
pub const STATUSES: [&str; 5] = ["draft", "active", "stale", "done", "deprecated"];

/// A repository's specs as a graph, and what's wrong in their links.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    /// In tree order: each spec before its children, children by path.
    pub specs: Vec<Spec>,
    /// Each one's depth in the tree, as `specs`.
    pub depths: Vec<usize>,
    /// "src/x/SPEC.md: its parent `y` isn't a spec", ….
    pub problems: Vec<String>,
}

impl Graph {
    pub fn get(&self, id: &str) -> Option<&Spec> {
        self.specs.iter().find(|s| s.id == id)
    }

    /// The spec a link names: its id, else its folder or file.
    pub fn resolve(&self, link: &str) -> Option<&Spec> {
        let l = link.trim().trim_start_matches("./");
        self.get(l).or_else(|| {
            let folder = l.trim_end_matches(FILE).trim_end_matches('/');
            let path = if folder.is_empty() || folder == "." { FILE.to_owned() } else { format!("{folder}/{FILE}") };
            self.specs.iter().find(|s| s.path == path)
        })
    }

    /// A spec's children, by path.
    fn children<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Spec> + 'a {
        let mut found: Vec<&Spec> = self.specs.iter().filter(|s| s.parent.as_deref() == Some(id)).collect();
        found.sort_by(|a, b| a.path.cmp(&b.path));
        found.into_iter()
    }

    /// The specs that link to `id`: `(their id, how)`.
    fn linked_from<'a>(&'a self, id: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        self.specs.iter().flat_map(move |s| s.links.iter().filter(move |(_, to)| to == id).map(move |(how, _)| (s.id.as_str(), how.as_str())))
    }
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
    graph(root).specs
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

/// A spec as its file says (title: frontmatter `title`, else its first
/// heading, else its folder's name; summary: `summary`, else its first
/// paragraph), its links as written.
fn describe(rel: &str, text: &str) -> Spec {
    let (front, body) = split_front(text);
    let fields = front_fields(front);
    let all = |k: &str| fields.iter().filter(|(f, _)| f == k).flat_map(|(_, v)| v.iter().cloned()).collect::<Vec<String>>();
    let one = |k: &str| all(k).into_iter().next().filter(|v| !v.is_empty());
    let heading = body.lines().find_map(|l| l.strip_prefix("# ")).map(|h| h.trim().to_owned());
    let folder = Path::new(rel).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let name = Path::new(&folder).file_name().map(|n| n.to_string_lossy().into_owned());
    let title = one("title").or(heading).or(name).unwrap_or_else(|| "The product".into());
    let paragraph = body
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty() && !p.starts_with('#') && !p.starts_with("```") && !p.starts_with('|') && !p.starts_with("<!--"))
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "));
    let summary = one("summary").or(paragraph).unwrap_or_default();
    let summary = if summary.chars().count() > 240 { format!("{}…", summary.chars().take(239).collect::<String>()) } else { summary };
    Spec {
        path: rel.to_owned(),
        id: one("id").unwrap_or_else(|| if folder.is_empty() { "root".into() } else { folder }),
        title,
        summary,
        kind: one("type").unwrap_or_default(),
        status: one("status").unwrap_or_default().to_lowercase(),
        parent: one("parent"),
        links: LINKS.iter().flat_map(|k| all(k).into_iter().filter(|v| !v.is_empty()).map(move |v| (k.to_string(), v))).collect(),
        tags: all("tags"),
    }
}

/// A frontmatter's fields: `key: value`, `key: [a, b]`, and `key:` with
/// `- a` lines under it; quotes taken off. (Enough YAML for specs.)
fn front_fields(front: &str) -> Vec<(String, Vec<String>)> {
    let unquote = |v: &str| v.trim().trim_matches(['"', '\'']).trim().to_owned();
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in front.lines() {
        let t = line.trim_end();
        if t.trim_start().starts_with('#') || t.trim().is_empty() {
            continue;
        }
        if let Some(item) = t.trim_start().strip_prefix("- ").filter(|_| t.starts_with([' ', '\t', '-'])) {
            if let Some(last) = out.last_mut() {
                last.1.push(unquote(item));
            }
            continue;
        }
        let Some((k, v)) = t.split_once(':').filter(|(k, _)| !k.starts_with([' ', '\t'])) else { continue };
        let v = v.trim();
        let values = match v.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            Some(list) => list.split(',').map(unquote).filter(|x| !x.is_empty()).collect(),
            None if v.is_empty() => Vec::new(),
            None => vec![unquote(v)],
        };
        out.push((k.trim().to_lowercase(), values));
    }
    out
}

/// The specs under `root` as a graph: parents and links resolved, the
/// specs in tree order, and what doesn't hold (a link to no spec, an id
/// used twice, parents in a circle, a status it can't have).
pub fn graph(root: &Path) -> Graph {
    build(load(root).iter().map(|(p, text)| describe(p, text)).collect())
}

fn build(mut specs: Vec<Spec>) -> Graph {
    let mut problems = Vec::new();
    // Ids: the first one keeps it.
    // (A later one is taken as its folder.)
    let mut seen = std::collections::HashSet::new();
    for s in specs.iter_mut() {
        if !seen.insert(s.id.clone()) {
            let folder = s.path.trim_end_matches(FILE).trim_end_matches('/');
            let own = if folder.is_empty() { "root".to_owned() } else { folder.to_owned() };
            problems.push(format!("{}: its id `{}` is another spec's too (taken as `{own}` here)", s.path, s.id));
            s.id = own;
        }
    }
    let mut g = Graph { specs: specs.clone(), ..Default::default() };
    for s in specs.iter_mut() {
        // Its parent: as it says, else the nearest folder above with one.
        let said = s.parent.take();
        let found = said.as_deref().and_then(|p| g.resolve(p)).map(|p| p.id.clone()).filter(|p| *p != s.id);
        if let (Some(p), None) = (&said, &found) {
            problems.push(format!("{}: its parent `{p}` isn't a spec", s.path));
        }
        s.parent = found.or_else(|| {
            let mut dir = Path::new(&s.path).parent()?.to_path_buf();
            while dir.pop() {
                let path = if dir.as_os_str().is_empty() { FILE.to_owned() } else { format!("{}/{FILE}", dir.to_string_lossy()) };
                if let Some(p) = g.specs.iter().find(|x| x.path == path) {
                    return Some(p.id.clone());
                }
                if dir.as_os_str().is_empty() {
                    break;
                }
            }
            None
        });
        let mut links = Vec::new();
        for (how, to) in std::mem::take(&mut s.links) {
            match g.resolve(&to) {
                Some(t) => links.push((how, t.id.clone())),
                None => problems.push(format!("{}: {how} `{to}` isn't a spec", s.path)),
            }
        }
        s.links = links;
        if !s.status.is_empty() && !STATUSES.contains(&s.status.as_str()) {
            problems.push(format!("{}: status `{}` isn't one of {}", s.path, s.status, STATUSES.join(", ")));
        }
    }
    // Parents in a circle: the one met again starts a tree of its own.
    for i in 0..specs.len() {
        let mut at = specs[i].parent.clone();
        let mut steps = 0;
        while let Some(p) = at {
            if p == specs[i].id {
                problems.push(format!("{}: its parents go round in a circle", specs[i].path));
                specs[i].parent = None;
                break;
            }
            steps += 1;
            if steps > specs.len() {
                break;
            }
            at = specs.iter().find(|s| s.id == p).and_then(|s| s.parent.clone());
        }
    }
    // Tree order.
    g.specs = specs;
    let mut order: Vec<(Spec, usize)> = Vec::new();
    let mut placed = std::collections::HashSet::new();
    fn place(g: &Graph, s: &Spec, depth: usize, order: &mut Vec<(Spec, usize)>, placed: &mut std::collections::HashSet<String>) {
        if !placed.insert(s.path.clone()) {
            return;
        }
        order.push((s.clone(), depth));
        for c in g.children(&s.id) {
            place(g, c, depth + 1, order, placed);
        }
    }
    for s in g.specs.iter().filter(|s| s.parent.as_deref().is_none_or(|p| g.get(p).is_none())) {
        place(&g, s, 0, &mut order, &mut placed);
    }
    (g.specs, g.depths) = order.into_iter().unzip();
    g.problems = problems;
    g
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
/// the heading they're under: at most `limit`. Only specs `keep` keeps.
pub fn search(root: &Path, query: &str, limit: usize) -> Vec<Value> {
    search_in(&load(root), query, limit, |_| true)
}

fn search_in(texts: &[(String, String)], query: &str, limit: usize, keep: impl Fn(&str) -> bool) -> Vec<Value> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (path, text) in texts.iter().filter(|(p, _)| keep(p)) {
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
    let missing = || format!("There's no {rel}. spec_graph shows the specs.");
    if crate::remote::is_remote(root) {
        return crate::remote::read(&root.join(&rel)).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|_| missing());
    }
    std::fs::read_to_string(root.join(&rel)).map_err(|_| missing())
}

/// What harlo tells agents in a project with specs on.
pub const INSTRUCTIONS: &str = "This project keeps specs: SPEC.md files beside the code that hold what the code doesn't say (what each part is for, its boundaries and contracts, and the decisions behind them); the root one is the product's. They form a tree (a spec's parent is the nearest spec in a folder above it, or its frontmatter `parent:`), with links between parts (`depends-on:`, `references:`, `implements:`, naming a spec's `id` or folder). Look up the spec that governs your work when it depends on or may change a documented boundary, contract, behavior or decision (spec_graph, spec_search, spec_read); a local fix that changes none of those needn't read specs. When a change does alter one, update its spec in the same change with your file tools, and write a SPEC.md for a new part that deserves one. A spec says how things are and why, briefly: not plans, tasks or a copy of the code, and each fact in one spec (others link to it). Optional frontmatter: id, title, summary, type (product, architecture, module, task), status (draft, active, stale, done, deprecated), parent, depends-on, references, implements, tags. After changing links, run spec_validate.";

/// The spec tools, as MCP lists them.
pub fn tools() -> Value {
    let s = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    let spec = json!({ "type": "string", "description": "A spec's id, folder or path (\"root\", \"src/auth\", \"src/auth/SPEC.md\")." });
    json!([
        {
            "name": "spec_graph",
            "description": "The project's specs (SPEC.md files) as a tree: each with its title, type, status, summary and links. Without `spec`, the whole tree to `depth`; with one, that spec with the specs above it, below it and linked to or from it.",
            "inputSchema": s(json!({ "spec": spec, "depth": { "type": "integer", "description": "Levels below to show (default 3 for the whole tree, 1 around a spec)." } }), &[]),
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "spec_search",
            "description": "Finds the lines of the project's specs that have all of the words given (any case), with their spec and section. Narrow it by the specs' type, status or tag; with no words, it lists the specs that match those.",
            "inputSchema": s(json!({
                "query": { "type": "string", "description": "Words to find." },
                "type": { "type": "string" }, "status": { "type": "string" }, "tag": { "type": "string" },
            }), &[]),
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "spec_read",
            "description": "A spec's whole text, after a line saying where it sits in the graph (its parent, children and links).",
            "inputSchema": s(json!({ "spec": spec }), &["spec"]),
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "spec_validate",
            "description": "Checks the specs' graph: links and parents that name no spec, ids used twice, parents in a circle, statuses that aren't known.",
            "inputSchema": s(json!({}), &[]),
            "annotations": { "readOnlyHint": true },
        }
    ])
}

/// A spec as one line: title, id when its own, type and status, path,
/// summary.
fn line(s: &Spec, depth: usize, summary: usize) -> String {
    let folder = s.path.trim_end_matches(FILE).trim_end_matches('/');
    let id = if s.id == folder || (folder.is_empty() && s.id == "root") { String::new() } else { format!(" [{}]", s.id) };
    let what: Vec<&str> = [s.kind.as_str(), s.status.as_str()].into_iter().filter(|x| !x.is_empty()).collect();
    let what = if what.is_empty() { String::new() } else { format!(" ({})", what.join(", ")) };
    let text = if s.summary.chars().count() > summary { format!("{}…", s.summary.chars().take(summary.saturating_sub(1)).collect::<String>()) } else { s.summary.clone() };
    let mut out = format!("{}- {}{id}{what} {}", "  ".repeat(depth), s.title, s.path);
    if !text.is_empty() {
        out += &format!(": {text}");
    }
    for how in LINKS {
        let to: Vec<&str> = s.links.iter().filter(|(h, _)| h == how).map(|(_, t)| t.as_str()).collect();
        if !to.is_empty() {
            out += &format!("\n{}  {how}: {}", "  ".repeat(depth), to.join(", "));
        }
    }
    out
}

/// At most this many lines of a tree.
const TREE_LINES: usize = 150;

fn tree(g: &Graph, top: Option<&Spec>, depth: usize) -> String {
    let mut out = Vec::new();
    let mut cut = 0;
    let (from, base) = match top {
        Some(t) => (g.specs.iter().position(|s| s.path == t.path).unwrap_or(0), g.depths[g.specs.iter().position(|s| s.path == t.path).unwrap_or(0)]),
        None => (0, 0),
    };
    for (i, s) in g.specs.iter().enumerate().skip(from) {
        let d = g.depths[i];
        if top.is_some() && i > from && d <= base {
            break;
        }
        let rel = d - if top.is_some() { base } else { 0 };
        if rel > depth {
            cut += 1;
            continue;
        }
        if out.len() >= TREE_LINES {
            cut += 1;
            continue;
        }
        out.push(line(s, rel, 140));
    }
    if cut > 0 {
        out.push(format!("(+{cut} more below: ask for a spec, or a greater depth)"));
    }
    out.join("\n")
}

/// Where a spec sits: its parents up to the top, its children, its links
/// both ways.
fn around(g: &Graph, s: &Spec) -> String {
    let mut up = Vec::new();
    let mut at = s.parent.as_deref().and_then(|p| g.get(p));
    while let Some(p) = at {
        if up.len() > 20 {
            break;
        }
        up.push(format!("{} ({})", p.title, p.path));
        at = p.parent.as_deref().and_then(|x| g.get(x));
    }
    let children: Vec<String> = g.children(&s.id).map(|c| format!("{} ({})", c.title, c.path)).collect();
    let from: Vec<String> = g.linked_from(&s.id).map(|(id, how)| format!("{id} ({how})")).collect();
    let mut parts = vec![format!("{} {}", s.title, s.path)];
    parts.push(if up.is_empty() { "at the top".into() } else { format!("under: {}", up.join(" < ")) });
    if !children.is_empty() {
        parts.push(format!("children: {}", children.join(", ")));
    }
    for how in LINKS {
        let to: Vec<&str> = s.links.iter().filter(|(h, _)| h == how).map(|(_, t)| t.as_str()).collect();
        if !to.is_empty() {
            parts.push(format!("{how}: {}", to.join(", ")));
        }
    }
    if !from.is_empty() {
        parts.push(format!("linked from: {}", from.join(", ")));
    }
    parts.join(" · ")
}

/// A spec tool's call in the repository at `root`.
pub fn call(root: &Path, tool: &str, args: &Value) -> Result<Value, String> {
    let texts = load(root);
    let g = build(texts.iter().map(|(p, t)| describe(p, t)).collect());
    let named = |k: &str| -> Result<Option<&Spec>, String> {
        match args[k].as_str().map(str::trim).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(v) => g.resolve(v).map(Some).ok_or_else(|| format!("There's no spec `{v}`. spec_graph shows them.")),
        }
    };
    let none = "No specs yet. Write the root SPEC.md (the product: its goal, its users, its main parts and decisions) and one for each main part as you work on them.";
    match tool {
        // (spec_list: what sessions from before the graph call.)
        "spec_graph" | "spec_list" => {
            if g.specs.is_empty() {
                return Ok(json!(none));
            }
            let at = named("spec")?;
            let depth = args["depth"].as_u64().map(|d| d.min(12) as usize);
            Ok(json!(match at {
                None => tree(&g, None, depth.unwrap_or(3)),
                Some(s) => {
                    let mut text = format!("{}\n\n{}", around(&g, s), tree(&g, Some(s), depth.unwrap_or(1)));
                    let linked: Vec<&Spec> = s.links.iter().filter_map(|(_, t)| g.get(t)).chain(g.linked_from(&s.id).filter_map(|(id, _)| g.get(id))).collect();
                    if !linked.is_empty() {
                        text += "\n\nLinked:\n";
                        text += &linked.iter().map(|l| line(l, 0, 140)).collect::<Vec<_>>().join("\n");
                    }
                    text
                }
            }))
        }
        "spec_search" => {
            let want = |k: &str| args[k].as_str().map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty());
            let (kind, status, tag) = (want("type"), want("status"), want("tag"));
            let fits = |s: &Spec| {
                kind.as_ref().is_none_or(|k| s.kind.to_lowercase() == *k)
                    && status.as_ref().is_none_or(|st| s.status == *st)
                    && tag.as_ref().is_none_or(|t| s.tags.iter().any(|x| x.to_lowercase() == *t))
            };
            let query = args["query"].as_str().unwrap_or_default();
            if query.trim().is_empty() {
                let list: Vec<String> = g.specs.iter().filter(|s| fits(s)).map(|s| line(s, 0, 140)).collect();
                return Ok(json!(if list.is_empty() { "No spec matches.".to_owned() } else { list.join("\n") }));
            }
            let keep = |p: &str| g.specs.iter().find(|s| s.path == p).is_some_and(fits);
            let hits = search_in(&texts, query, 40, keep);
            if hits.is_empty() {
                return Ok(json!("Nothing found."));
            }
            Ok(json!(hits.iter().map(|h| format!("{}:{} ({}): {}", h["spec"].as_str().unwrap_or_default(), h["line"], h["section"].as_str().unwrap_or_default(), h["text"].as_str().unwrap_or_default())).collect::<Vec<_>>().join("\n")))
        }
        "spec_read" => {
            let wanted = args["spec"].as_str().or(args["path"].as_str()).unwrap_or_default();
            match g.resolve(wanted) {
                Some(s) => read(root, &s.path).map(|text| json!(format!("[{}]\n\n{text}", around(&g, s)))),
                // (A folder without one yet: what read says.)
                None => read(root, wanted).map(|text| json!(text)),
            }
        }
        "spec_validate" => Ok(json!(if g.problems.is_empty() {
            format!("No problems in {} spec{}.", g.specs.len(), if g.specs.len() == 1 { "" } else { "s" })
        } else {
            g.problems.join("\n")
        })),
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
        let seen: Vec<(&str, &str, &str, &str)> = specs.iter().map(|s| (s.path.as_str(), s.id.as_str(), s.title.as_str(), s.summary.as_str())).collect();
        assert_eq!(seen, vec![
            ("SPEC.md", "root", "Shop", "Sells socks online."),
            ("docs/SPEC.md", "docs", "docs", "No heading here, just text."),
            ("src/auth/SPEC.md", "src/auth", "Sign-in", "Who may sign in, and how long a session lasts."),
        ]);
        // (No frontmatter: under the nearest spec above.)
        assert_eq!(specs[2].parent.as_deref(), Some("root"));
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
        let text = |tool: &str, args: Value| call(&dir, tool, &args).unwrap().as_str().unwrap().to_owned();
        assert_eq!(text("spec_graph", json!({})), "- Shop SPEC.md: Sells socks online.\n  - docs docs/SPEC.md: No heading here, just text.\n  - Sign-in src/auth/SPEC.md: Who may sign in, and how long a session lasts.");
        assert_eq!(text("spec_list", json!({})), text("spec_graph", json!({})));
        let docs = text("spec_read", json!({ "spec": "docs" }));
        assert_eq!(docs, "[docs docs/SPEC.md · under: Shop (SPEC.md)]\n\nNo heading here, just text.\n");
        assert_eq!(text("spec_search", json!({ "query": "tokens" })), "src/auth/SPEC.md:8 (Decisions): Tokens expire after 30 days: phones stay signed in.");
        assert!(call(&dir, "spec_read", &json!({ "spec": "nowhere" })).unwrap_err().contains("There's no nowhere/SPEC.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Frontmatter's links, parents and metadata; what doesn't hold.
    #[test]
    fn spec_graph() {
        let dir = std::env::temp_dir().join(format!("harlo-specgraph-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let put = |rel: &str, text: &str| {
            std::fs::create_dir_all(dir.join(rel).parent().unwrap()).unwrap();
            std::fs::write(dir.join(rel), text).unwrap();
        };
        put("SPEC.md", "---\nid: shop\ntype: product\ntitle: Shop\n---\nSells socks.\n");
        put("docs/arch/SPEC.md", "---\nid: architecture\ntype: architecture\nstatus: active\n---\n# Architecture\n\nOne server, one database.\n");
        put("server/SPEC.md", "---\nid: server\ntype: module\nparent: architecture\ndepends-on: [server/store, \"payments\"]\ntags:\n  - backend\n---\n# Server\n\nAnswers the shop's requests.\n");
        put("server/store/SPEC.md", "---\ntype: module\nstatus: stale\nimplements:\n  - architecture\n---\n# Store\n\nKeeps orders.\n");
        put("web/SPEC.md", "---\nid: server\nstatus: shiny\nreferences: [server/store]\n---\n# Web\n");
        put("loop/a/SPEC.md", "---\nid: a\nparent: b\n---\n# A\n");
        put("loop/b/SPEC.md", "---\nid: b\nparent: a\n---\n# B\n");
        let g = graph(&dir);
        let by = |path: &str| g.specs.iter().find(|s| s.path == path).unwrap();
        // The tree: an explicit parent, else the folder's.
        assert_eq!(by("server/SPEC.md").parent.as_deref(), Some("architecture"));
        assert_eq!(by("docs/arch/SPEC.md").parent.as_deref(), Some("shop"));
        assert_eq!(by("server/store/SPEC.md").parent.as_deref(), Some("server"));
        assert_eq!(by("server/store/SPEC.md").id, "server/store");
        // Links by id or folder; one to nothing is a problem.
        assert_eq!(by("server/SPEC.md").links, vec![("depends-on".to_owned(), "server/store".to_owned())]);
        assert_eq!(by("server/store/SPEC.md").links, vec![("implements".to_owned(), "architecture".to_owned())]);
        assert_eq!(by("server/SPEC.md").tags, ["backend"]);
        assert_eq!(by("server/store/SPEC.md").status, "stale");
        let problems = g.problems.join("\n");
        for p in ["server/SPEC.md: depends-on `payments` isn't a spec", "web/SPEC.md: its id `server` is another spec's too", "web/SPEC.md: status `shiny` isn't one of", "parents go round in a circle"] {
            assert!(problems.contains(p), "{p} in {problems}");
        }
        // Tree order, with depths.
        let order: Vec<(usize, &str)> = g.specs.iter().zip(&g.depths).map(|(s, d)| (*d, s.path.as_str())).collect();
        assert_eq!(&order[..4], &[(0, "SPEC.md"), (1, "docs/arch/SPEC.md"), (2, "server/SPEC.md"), (3, "server/store/SPEC.md")]);
        assert_eq!(g.specs.len(), 7);
        // The tools.
        let text = |tool: &str, args: Value| call(&dir, tool, &args).unwrap().as_str().unwrap().to_owned();
        let around = text("spec_graph", json!({ "spec": "server" }));
        assert!(around.starts_with("Server server/SPEC.md · under: Architecture (docs/arch/SPEC.md) < Shop (SPEC.md) · children: Store (server/store/SPEC.md) · depends-on: server/store"), "{around}");
        assert!(around.contains("\n- Server (module) server/SPEC.md: Answers the shop's requests.\n  depends-on: server/store\n  - Store (module, stale) server/store/SPEC.md: Keeps orders."), "{around}");
        assert!(text("spec_graph", json!({ "depth": 1 })).contains("(+"));
        assert_eq!(text("spec_search", json!({ "status": "stale" })), "- Store (module, stale) server/store/SPEC.md: Keeps orders.\n  implements: architecture");
        assert_eq!(text("spec_search", json!({ "query": "orders", "tag": "backend" })), "Nothing found.");
        assert!(text("spec_read", json!({ "spec": "architecture" })).contains("linked from: server/store (implements)"));
        assert!(text("spec_validate", json!({})).contains("isn't a spec"));
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
