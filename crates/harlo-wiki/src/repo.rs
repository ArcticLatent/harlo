//! The repository as a writing run sees it, as OpenWiki sees it.
//!
//! The last update's record, a hash of the wiki's content, a fingerprint of
//! every source the wiki may read, what changed since a commit, and whether an
//! update has nothing to do.

// Rust guideline compliant 2026-10-09

use crate::ignore::{self, Ignore};
use crate::util;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};

pub const WIKI_DIR: &str = "openwiki";
pub const PAGE_MANIFEST_PATH: &str = "openwiki/.page-manifest.json";
pub const UPDATE_METADATA_PATH: &str = "openwiki/.last-update.json";
pub const RUN_STATE_NAME: &str = ".run.json";
pub const INSTRUCTIONS_FILE: &str = "INSTRUCTIONS.md";

pub type Result<T> = std::result::Result<T, String>;

// ---- Git ----

fn git_command(root: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.arg("--no-pager").args(args).current_dir(root).stdin(Stdio::null());
    c
}

/// Git's output, or why it failed.
fn git_strict(root: &Path, args: &[&str]) -> std::result::Result<Vec<u8>, String> {
    let out = git_command(root, args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(out.stdout)
}

/// Git's output lines, trimmed and without blanks (none when it fails).
pub fn git_lines(root: &Path, args: &[&str]) -> Vec<String> {
    match git_strict(root, args) {
        Ok(out) => String::from_utf8_lossy(&out).split('\n').map(|l| crate::jsre::trim(l).to_owned()).filter(|l| !l.is_empty()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Its output and errors together, trimmed (as OpenWiki's `runGit`).
fn git_text(root: &Path, args: &[&str]) -> String {
    let Ok(out) = git_command(root, args).output() else { return String::new() };
    let a = String::from_utf8_lossy(&out.stdout);
    let b = String::from_utf8_lossy(&out.stderr);
    [crate::jsre::trim(&a), crate::jsre::trim(&b)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n").trim().to_owned()
}

fn is_object_id(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn git_head(root: &Path) -> Option<String> {
    git_lines(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).into_iter().next().filter(|h| is_object_id(h))
}

pub fn is_readable_commit(root: &Path, head: &str) -> bool {
    is_object_id(head) && git_strict(root, &["cat-file", "-e", &format!("{head}^{{commit}}")]).is_ok()
}

pub fn is_wiki_path(p: &str) -> bool {
    p == WIKI_DIR || p.starts_with("openwiki/")
}

fn normalize_git_path(v: &str) -> String {
    crate::jsre::trim(v).replace('\\', "/")
}

/// Paths changed since `base` (committed), in the work tree, and new ones,
/// leaving out the wiki's and ignored ones, in code-unit order.
pub fn changed_paths(root: &Path, ignore: &Ignore, base: Option<&str>) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let mut add = |p: String| {
        if !paths.contains(&p) {
            paths.push(p);
        }
    };
    if let Some(b) = base {
        for c in git_lines(root, &["diff", "--name-only", &format!("{b}..HEAD")]) {
            add(normalize_git_path(&c));
        }
    }
    for c in git_lines(root, &["diff", "--name-only", "HEAD"]) {
        add(normalize_git_path(&c));
    }
    for c in git_lines(root, &["ls-files", "--others", "--exclude-standard"]) {
        add(normalize_git_path(&c));
    }
    let mut out: Vec<String> = paths.into_iter().filter(|p| !p.is_empty() && !is_wiki_path(p) && !ignore.ignores(p, false)).collect();
    out.sort_by(|a, b| util::cmp16(a, b));
    out
}

/// The files a wiki's run sets up beside it (not code it describes).
const SETUP_FILES: [&str; 4] = ["AGENTS.md", "CLAUDE.md", ".openwikiignore", ".github/workflows/openwiki-update.yml"];

/// harlo: files added since `base`, committed or in the work tree.
///
/// Up to `head` (the commit checked) and in the work tree since, past the
/// wiki's, its setup files and ignored ones, in code-unit order.
///
/// # Errors
///
/// git's message when it couldn't say (so nothing can be taken as unchanged).
pub fn added_paths(root: &Path, ignore: &Ignore, base: &str, head: &str) -> std::result::Result<Vec<String>, String> {
    let mut paths = split_nul(&git_strict(root, &["diff", "--name-only", "-z", "--no-ext-diff", "--diff-filter=A", base, head])?)?;
    paths.extend(split_nul(&git_strict(root, &["diff", "--name-only", "-z", "--no-ext-diff", "--diff-filter=A", head])?)?);
    paths.extend(split_nul(&git_strict(root, &["ls-files", "--others", "--exclude-standard", "-z"])?)?);
    let mut out: Vec<String> = Vec::new();
    for p in paths.iter().map(|p| p.replace('\\', "/")) {
        // (Added, then deleted again: not there.)
        if !p.is_empty() && !is_wiki_path(&p) && !SETUP_FILES.contains(&p.as_str()) && !ignore.ignores(&p, false) && !out.contains(&p) && root.join(&p).exists() {
            out.push(p);
        }
    }
    out.sort_by(|a, b| util::cmp16(a, b));
    Ok(out)
}

/// harlo: lines from which a source file no Claim cites is named to an
/// update; smaller files are helpers that a page about their feature covers
/// without citing each one.
pub const UNCOVERED_LINES: usize = 300;

/// harlo: a test, a fixture or vendored (minified) code: not what a wiki
/// describes, however large.
fn is_test_or_vendored(p: &str) -> bool {
    let name = p.rsplit('/').next().unwrap_or(p);
    let stem = name.split_once('.').map_or(name, |(s, _)| s);
    let low = stem.to_ascii_lowercase();
    let in_dir = p.split('/').rev().skip(1).map(str::to_ascii_lowercase).any(|d| matches!(d.as_str(), "test" | "tests" | "benches" | "fixtures" | "testdata" | "vendor" | "third_party") || d.ends_with("tests"));
    // (`foo_test`, `test_foo`, `FooTests`, `foo.test.ts`; not `latest`.)
    let named = matches!(low.as_str(), "test" | "tests") || low.starts_with("test_") || ["_test", "_tests", "_spec"].iter().any(|s| low.ends_with(s)) || ["Test", "Tests", "Spec"].iter().any(|s| stem.ends_with(s));
    in_dir || named || name.contains(".min.") || name.contains(".test.") || name.contains(".spec.")
}

/// harlo: committed source files added since the wiki was first committed
/// that no page cites, largest first.
///
/// Those of [`UNCOVERED_LINES`] or more, past the wiki's, its setup files,
/// ignored ones, tests and vendored code. An update looks only at what
/// changed since the last one, so a feature that update left out would
/// otherwise never come up again; naming it lets a later update catch up.
/// Code that was there when the wiki was written and left out was the
/// writer's choice, so it isn't named (the whole repository is, when the
/// wiki was never committed); nor is code a plan already decided on.
///
/// # Errors
///
/// git's message when it couldn't list the files.
pub fn uncovered_paths(root: &Path, ignore: &Ignore, cited: &HashSet<String>) -> std::result::Result<Vec<String>, String> {
    // (The commit that first added the wiki's record of its updates.)
    let start = git_lines(root, &["log", "--diff-filter=A", "--format=%H", "--", UPDATE_METADATA_PATH]).pop();
    let candidates = match &start {
        Some(c) => git_strict(root, &["diff", "--name-only", "-z", "--no-ext-diff", "--diff-filter=A", c, "HEAD"])?,
        None => git_strict(root, &["ls-files", "--cached", "-z"])?,
    };
    let reviewed: HashSet<String> = read_last_update(root)?.map(|l| l.reviewed.into_iter().collect()).unwrap_or_default();
    let mut weighed: Vec<(usize, String)> = Vec::new();
    for p in split_nul(&candidates)?.iter().map(|p| p.replace('\\', "/")) {
        let source = p.rsplit_once('.').is_some_and(|(_, ext)| SOURCE_EXTENSIONS.contains(&ext));
        if !source || cited.contains(&p) || reviewed.contains(&p) || is_wiki_path(&p) || SETUP_FILES.contains(&p.as_str()) || ignore.ignores(&p, false) || is_test_or_vendored(&p) {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(&p)) else { continue };
        let lines = bytes.iter().filter(|c| **c == b'\n').count();
        if lines >= UNCOVERED_LINES {
            weighed.push((lines, p));
        }
    }
    weighed.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| util::cmp16(&a.1, &b.1)));
    Ok(weighed.into_iter().map(|(_, p)| p).collect())
}

/// harlo: lines added and removed in a source file since the last update
/// from which it needs a model, whatever else changed (see grown_paths).
pub const GROWN_LINES: usize = 50;

const SOURCE_EXTENSIONS: [&str; 39] = [
    "rs", "py", "js", "mjs", "cjs", "ts", "mts", "cts", "tsx", "jsx", "go", "java", "kt", "kts", "swift", "c", "h", "cc", "cpp", "cxx", "hpp", "hh", "cs", "rb", "php",
    "qml", "sh", "lua", "scala", "ex", "exs", "zig", "dart", "vue", "svelte", "sql", "m", "mm", "fs",
];

/// Keywords that open a definition at any depth (a method too), after
/// any modifiers.
const DEFINITIONS: [&str; 17] = ["fn", "struct", "enum", "trait", "impl", "mod", "class", "def", "function", "interface", "type", "func", "fun", "object", "protocol", "extension", "module"];
/// Words that open a statement, not a method, before a bracket and a body.
const STATEMENTS: [&str; 25] = [
    "if", "for", "while", "switch", "catch", "with", "return", "else", "elif", "elseif", "case", "match", "when", "unless", "until", "foreach", "using", "lock", "fixed", "synchronized", "new", "await", "yield", "throw", "do",
];
const MODIFIERS: [&str; 16] = ["export", "default", "async", "unsafe", "static", "abstract", "final", "public", "private", "protected", "internal", "open", "extern", "override", "inline", "virtual"];

fn is_test_path(p: &str) -> bool {
    let name = p.rsplit('/').next().unwrap_or(p);
    let dirs = format!("/{p}");
    ["/tests/", "/test/", "/Tests/", "/__tests__/", "/spec/"].iter().any(|d| dirs.contains(d))
        || name.starts_with("test_")
        || ["_test.", ".test.", ".spec.", "Tests."].iter().any(|t| name.contains(t))
}

/// The name a definition line defines ("pub fn run(" → "run", "  def
/// go(self):" → "go"), if a keyword says it is one.
fn defined_name(line: &str) -> Option<String> {
    let line = line.trim_start();
    // (`pub(crate) …`: its scope in brackets.)
    let line = match line.strip_prefix("pub(").and_then(|r| r.split_once(')')) {
        Some((_, rest)) => rest.trim_start(),
        None => line,
    };
    // (A Go method: its name is after the receiver.)
    if let Some(rest) = line.strip_prefix("func (") {
        let name: String = rest.split_once(')')?.1.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        return (!name.is_empty()).then_some(name);
    }
    let mut words = line.split(|c: char| c.is_whitespace() || c == '(' || c == '<' || c == ':' || c == '{' || c == '=' || c == ';').filter(|w| !w.is_empty()).peekable();
    while let Some(w) = words.peek() {
        if MODIFIERS.contains(w) || *w == "pub" || *w == "const" {
            words.next();
        } else {
            break;
        }
    }
    let keyword = words.next()?;
    if !DEFINITIONS.contains(&keyword) {
        return None;
    }
    words.next().map(|n| n.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_')).to_owned()).filter(|n| !n.is_empty())
}

fn is_identifier(w: &str) -> bool {
    w.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$') && w.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// A method or function declared without a keyword, at any depth, read
/// from `text` (a changed line and the changed lines after it): a name,
/// its generic parameters, its parameters, then its body (`added() {`,
/// `int added(void) const {`, `added<T extends (...a: any[]) => any>(v:
/// T): T {`, `{` on a line of its own), a signature only (`added(v:
/// number): number;`), or a name given a function (`added = () => 2;`,
/// `added: async (x) => {`). When the text ends inside one (its generics
/// or parameters still open, a return type and no body yet), it counts:
/// what can't be read isn't taken as unchanged.
fn method_name(text: &str) -> Option<String> {
    let blanked = blank_literals(text.trim_start());
    let t = blanked.as_str();
    if !t.starts_with(|c: char| c.is_alphabetic() || c == '_' || c == '$' || c == '*' || c == '&') {
        return None;
    }
    let chars: Vec<char> = t.chars().collect();
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    // The head: up to the first `(` outside generic parameters, which are
    // set aside (their brackets, `=>` and `->` included).
    let mut head = String::new();
    let mut i = 0;
    let mut angle = 0usize;
    let mut inner = 0usize;
    let ended_in_head = loop {
        let Some(&c) = chars.get(i) else { break true };
        if angle > 0 {
            match c {
                '(' | '[' | '{' => inner += 1,
                ')' | ']' | '}' => inner = inner.saturating_sub(1),
                ';' if inner == 0 => return None,
                '<' if inner == 0 => angle += 1,
                '>' if inner == 0 && !matches!(at(i.wrapping_sub(1)), '=' | '-') => angle -= 1,
                _ => {}
            }
        } else {
            match c {
                '(' => break false,
                ';' | '{' | '}' => return None,
                // (Generics follow a name, an optional one too, or open a
                // function's value.)
                '<' if head.trim_end().is_empty() || head.trim_end().ends_with(|c: char| c.is_alphanumeric() || matches!(c, '_' | '$' | '=' | ':' | '?')) => angle = 1,
                _ => head.push(c),
            }
        }
        i += 1;
    };
    let head = head.trim();
    let first = head.split_whitespace().next().unwrap_or_default();
    if STATEMENTS.contains(&first) {
        return None;
    }
    // (A name given a function: split at its `=` or its one `:`.)
    let split = head.find('=').or_else(|| head.find(':').filter(|_| !head.contains("::")));
    if let Some(at_split) = split {
        let (name, value) = (head[..at_split].trim_end(), head[at_split + 1..].trim());
        if name.contains(['=', '.', ':']) {
            return None;
        }
        let name = name.rsplit(char::is_whitespace).next().unwrap_or_default();
        // (An optional member: `added?: () => number`.)
        let name = optional(name).unwrap_or(name);
        let value = value.strip_prefix("async").map_or(value, str::trim_start);
        if !is_identifier(name) || !(value.is_empty() || value == "function") {
            return None;
        }
        if value == "function" || ended_in_head {
            return Some(name.to_owned());
        }
        // (An arrow after its parameters, or the text ending before one.)
        let rest: String = chars[i..].iter().collect();
        let rest = rest.split(';').next().unwrap_or_default();
        return (rest.contains("=>") || !rest.contains(')')).then(|| name.to_owned());
    }
    // (An optional member, `added?(): number;`: its `?` against its name,
    // where a ternary's has space around it.)
    let head = optional(head).unwrap_or(head);
    if head.contains(['=', '.', '"', '\'', '!', ',', ';', '|', '?', '[', ']', '@', '#', '+', '%', '^', '~', '/']) || head.contains("->") {
        return None;
    }
    let words: Vec<&str> = head.split_whitespace().map(|w| w.trim_start_matches(['*', '&'])).filter(|w| !w.is_empty()).collect();
    let name = *words.last()?;
    if !is_identifier(name) || name.contains("::") {
        return None;
    }
    if ended_in_head {
        // (Its generics still open: `added<` and the text ends.)
        return (angle > 0).then(|| name.to_owned());
    }
    // Its parameters, to their closing bracket.
    let mut depth = 0usize;
    let mut close = None;
    let mut j = i;
    while j < chars.len() {
        match chars[j] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(j);
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    // (Still open where the text ends.)
    let Some(close) = close else { return Some(name.to_owned()) };
    // Then a body or a signature's end, past a return type or a qualifier.
    let rest: String = chars[close + 1..].iter().collect();
    let end = rest.find(['{', ';']);
    let between = rest[..end.unwrap_or(rest.len())].trim();
    const QUALIFIERS: [&str; 10] = ["const", "noexcept", "override", "final", "throws", "volatile", "where", "mutating", "&", "&&"];
    let typed = between.starts_with(':') || between.starts_with("->");
    let qualified = typed || between.split_whitespace().next().is_some_and(|w| QUALIFIERS.contains(&w));
    match end.map(|e| rest[e..].starts_with('{')) {
        // (A body.)
        Some(true) => ((between.is_empty() || qualified) && (typed || !between.contains("=>"))).then(|| name.to_owned()),
        // (A signature only: typed.)
        Some(false) => typed.then(|| name.to_owned()),
        // (The text ends: a return type or qualifier means a body may follow.)
        None => (!between.is_empty() && qualified).then(|| name.to_owned()),
    }
}

/// `name?` without its `?`, when it ends so (straight after a name).
fn optional(head: &str) -> Option<&str> {
    let bare = head.strip_suffix('?')?;
    bare.ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$').then_some(bare)
}

/// `text` with its string literals' contents and its comments blanked
/// (to spaces, lines kept), so a bracket in them closes nothing. A `'`
/// opens a literal only when the line closes it (Rust's `'a` doesn't).
fn blank_literals(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or('\0');
        if c == '/' && next == '/' {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '/' && next == '*' {
            out.push_str("  ");
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            if i < chars.len() {
                out.push_str("  ");
                i += 2;
            }
            continue;
        }
        if matches!(c, '"' | '`' | '\'') {
            // (Its end: the next unescaped same quote, on this line for '.)
            let mut j = i + 1;
            while j < chars.len() && chars[j] != c && !(c == '\'' && chars[j] == '\n') {
                j += if chars[j] == '\\' { 2 } else { 1 };
            }
            if j < chars.len() && chars[j] == c {
                out.push(c);
                out.extend(chars[i + 1..j].iter().map(|&x| if x == '\n' { '\n' } else { ' ' }));
                out.push(c);
                i = j + 1;
                continue;
            }
            if c != '\'' {
                // (Not closed: blank to the end.)
                out.push(c);
                out.extend(chars[i + 1..].iter().map(|&x| if x == '\n' { '\n' } else { ' ' }));
                break;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The name a run of added (or removed) lines declares at its `i`th: by
/// a keyword on that line, or a method read on through the lines after it.
fn declared_name(run: &[&str], i: usize) -> Option<String> {
    defined_name(run[i]).or_else(|| method_name(&run[i..].join("\n")))
}

/// What a top-level line declares, whatever the language: the line up to
/// its value or body ("export const added = () => 2;" → "export const
/// added", "int added(void) {" → "int added"), so a changed value keeps
/// its key and a new name doesn't. None for lines that declare nothing:
/// indented ones, blanks, comments, imports, closing brackets, attributes.
fn top_level_key(line: &str) -> Option<String> {
    if line.is_empty() || line.starts_with(char::is_whitespace) {
        return None;
    }
    const SKIP: [&str; 21] = ["//", "/*", "*", "#", "--", "@", "}", ")", "]", "end", "import ", "from ", "use ", "using ", "package ", "require", "include", "extern crate", "\"\"\"", "'''", "<"];
    if SKIP.iter().any(|p| line.starts_with(p)) {
        return None;
    }
    let cut = line.find(['(', '=', '{', ':', ';', '[']).unwrap_or(line.len());
    let key = line[..cut].split_whitespace().collect::<Vec<_>>().join(" ");
    (!key.is_empty()).then_some(key)
}

/// The path on a diff's `+++ ` line: C-style quoted (`"src/a\tb.rs"`)
/// when git quoted it, else up to the tab git ends a name with spaces with.
fn header_path(p: &str) -> String {
    if p.starts_with('"') {
        // (Up to its closing quote: a backslash escapes the next byte.)
        let mut escaped = false;
        for (i, c) in p.char_indices().skip(1) {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => return unquote(&p[..=i]),
                _ => escaped = false,
            }
        }
        return unquote(p);
    }
    p.trim_end_matches('\t').to_owned()
}

/// A path git printed C-style quoted (`"src/a\tb.rs"`), as it is.
fn unquote(p: &str) -> String {
    let Some(inner) = p.strip_prefix('"').and_then(|p| p.strip_suffix('"')) else { return p.to_owned() };
    let mut bytes: Vec<u8> = Vec::new();
    let mut it = inner.bytes().peekable();
    while let Some(b) = it.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match it.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'r') => bytes.push(b'\r'),
            Some(b'a') => bytes.push(7),
            Some(b'b') => bytes.push(8),
            Some(b'f') => bytes.push(12),
            Some(b'v') => bytes.push(11),
            Some(d @ b'0'..=b'7') => {
                let mut v = u32::from(d - b'0');
                for _ in 0..2 {
                    match it.peek() {
                        Some(n @ b'0'..=b'7') => {
                            v = v * 8 + u32::from(*n - b'0');
                            it.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(u8::try_from(v).unwrap_or(u8::MAX));
            }
            Some(c) => bytes.push(c),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// harlo: source files changed since `base` in ways the wiki may need to cover.
///
/// Committed or in the work tree: a new declaration (a keyword definition at
/// any depth, or a top-level line declaring a new name, in any language; tests
/// aside), or `GROWN_LINES` lines or more added and removed. A small edit to
/// code no fact cites needs no update; these may.
///
/// # Errors
///
/// git's message when it couldn't say.
pub fn grown_paths(root: &Path, ignore: &Ignore, base: &str) -> std::result::Result<Vec<String>, String> {
    // (Prefixes, quoting, colour, external tools and text conversion set
    // here: a user's git settings don't change what's read.)
    let out = git_strict(
        root,
        &["-c", "core.quotePath=false", "diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", "--src-prefix=a/", "--dst-prefix=b/", base],
    )?;
    let text = String::from_utf8_lossy(&out);
    struct File {
        path: String,
        lines: usize,
        new_names: HashSet<String>,
        gone_names: HashSet<String>,
        new_keys: HashSet<String>,
        gone_keys: HashSet<String>,
    }
    let mut files: Vec<File> = Vec::new();
    let mut current: Option<usize> = None;
    let mut in_header = false;
    // (Each changed line: its file and hunk, whether it was added, the line.)
    let mut runs: Vec<(usize, usize, bool, &str)> = Vec::new();
    let mut hunk = 0;
    for line in text.split('\n') {
        if line.starts_with("diff --git ") {
            current = None;
            in_header = true;
            continue;
        }
        if in_header {
            if let Some(path) = line.strip_prefix("+++ ") {
                let path = header_path(path);
                current = path.strip_prefix("b/").map(|p| {
                    files.push(File { path: p.replace('\\', "/"), lines: 0, new_names: HashSet::new(), gone_names: HashSet::new(), new_keys: HashSet::new(), gone_keys: HashSet::new() });
                    files.len() - 1
                });
                in_header = false;
            } else if line.starts_with("@@") {
                in_header = false;
            }
            continue;
        }
        let Some(i) = current else { continue };
        if line.starts_with("@@") {
            hunk += 1;
            continue;
        }
        let (added, body) = match line.as_bytes().first() {
            Some(b'+') => (true, &line[1..]),
            Some(b'-') => (false, &line[1..]),
            _ => continue,
        };
        let f = &mut files[i];
        f.lines += 1;
        if let Some(key) = top_level_key(body) {
            if added { f.new_keys.insert(key) } else { f.gone_keys.insert(key) };
        }
        runs.push((i, hunk, added, body));
    }
    // (Names, from each run of lines added or removed together: a
    // declaration may go on over several.)
    let mut start = 0;
    while start < runs.len() {
        let (file, h, added, _) = runs[start];
        let end = start + runs[start..].iter().take_while(|(f, n, a, _)| (*f, *n, *a) == (file, h, added)).count();
        // (A file with GROWN_LINES changed needs a model anyway.)
        if files[file].lines >= GROWN_LINES {
            start = end;
            continue;
        }
        let run: Vec<&str> = runs[start..end].iter().map(|(_, _, _, l)| *l).collect();
        for k in 0..run.len() {
            if let Some(name) = declared_name(&run, k) {
                let f = &mut files[file];
                if added { f.new_names.insert(name) } else { f.gone_names.insert(name) };
            }
        }
        start = end;
    }
    // (Every changed file git names, NUL-delimited: one the parsing above
    // missed can't be taken as unchanged.)
    let listed = git_strict(
        root,
        &["-c", "core.quotePath=false", "diff", "--name-only", "-z", "--no-renames", "--no-ext-diff", "--diff-filter=d", base],
    )?;
    let parsed: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
    let missed: Vec<String> = split_nul(&listed).map_err(|e| e.clone())?.into_iter().map(|p| p.replace('\\', "/")).filter(|p| !parsed.contains(p)).collect();
    let mut out: Vec<String> = files
        .into_iter()
        .filter(|f| !is_wiki_path(&f.path) && !SETUP_FILES.contains(&f.path.as_str()) && !ignore.ignores(&f.path, false))
        .filter(|f| f.path.rsplit_once('.').is_some_and(|(_, ext)| SOURCE_EXTENSIONS.contains(&ext)))
        .filter(|f| {
            f.lines >= GROWN_LINES
                || (!is_test_path(&f.path) && (f.new_names.iter().any(|n| !f.gone_names.contains(n)) || f.new_keys.iter().any(|k| !f.gone_keys.contains(k))))
        })
        .map(|f| f.path)
        .chain(missed.into_iter().filter(|p| !is_wiki_path(p) && !SETUP_FILES.contains(&p.as_str()) && !ignore.ignores(p, false) && p.rsplit_once('.').is_some_and(|(_, ext)| SOURCE_EXTENSIONS.contains(&ext))))
        .collect();
    out.sort_by(|a, b| util::cmp16(a, b));
    out.dedup();
    Ok(out)
}

const COMMITS: usize = 20;
const SUBJECT_CHARS: usize = 200;
const BODY_CHARS: usize = 400;
const TOTAL_CHARS: i64 = 6000;

fn strip_controls(v: &str) -> String {
    v.chars().filter(|c| *c != '\r' && !matches!(*c as u32, 0x00..=0x08 | 0x0b..=0x1f | 0x7f..=0x9f)).collect()
}

/// The newest commits since `base` (sha, subject, body), as planning hints:
/// none while `.openwikiignore` has rules.
pub fn commit_context(root: &Path, ignore: &Ignore, base: Option<&str>) -> Vec<Value> {
    let Some(base) = base else { return Vec::new() };
    if ignore.is_active() {
        return Vec::new();
    }
    let range = format!("{base}..HEAD");
    let max = format!("--max-count={COMMITS}");
    let exclude = format!(":(exclude){WIKI_DIR}");
    let Ok(out) = git_strict(root, &["log", "-z", "--no-merges", "--no-show-signature", &max, "--format=%H%n%s%n%b", &range, "--", ".", &exclude]) else {
        return Vec::new();
    };
    let out = String::from_utf8_lossy(&out);
    let mut commits = Vec::new();
    let mut remaining = TOTAL_CHARS;
    for record in out.split('\0') {
        if remaining <= 0 {
            break;
        }
        let rec = crate::jsre::trim(record);
        let mut lines = rec.split('\n');
        let sha = crate::jsre::trim(lines.next().unwrap_or("")).to_owned();
        let subject = lines.next().unwrap_or("");
        let body: Vec<&str> = lines.collect();
        if !is_object_id(&sha) {
            continue;
        }
        let subject = util::truncate_points(crate::jsre::trim(&strip_controls(subject)), usize::try_from(i64::try_from(SUBJECT_CHARS).unwrap_or(i64::MAX).min(remaining)).unwrap_or(0));
        remaining -= i64::try_from(crate::jsre::len16(&subject)).unwrap_or(i64::MAX);
        let collapsed = collapse_blank_lines(&strip_controls(&body.join("\n")));
        let body = util::truncate_points(crate::jsre::trim(&collapsed), usize::try_from(i64::try_from(BODY_CHARS).unwrap_or(i64::MAX).min(remaining.max(0))).unwrap_or(0));
        remaining -= i64::try_from(crate::jsre::len16(&body)).unwrap_or(i64::MAX);
        let mut c = Map::new();
        c.insert("sha".into(), sha.into());
        c.insert("subject".into(), subject.into());
        c.insert("body".into(), body.into());
        commits.push(Value::Object(c));
    }
    commits
}

/// `\n{3,}` → `\n\n`.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = 0;
    for c in s.chars() {
        if c == '\n' {
            run += 1;
            continue;
        }
        if run > 0 {
            out.push_str(if run >= 2 { "\n\n" } else { "\n" });
            run = 0;
        }
        out.push(c);
    }
    if run > 0 {
        out.push_str(if run >= 2 { "\n\n" } else { "\n" });
    }
    out
}

// ---- The last update ----

#[derive(Debug, Clone, PartialEq)]
pub struct LastUpdate {
    pub updated_at: String,
    pub command: String,
    pub git_head: Option<String>,
    pub model: String,
    pub status: String,
    pub language: Option<String>,
    /// harlo: uncovered code a plan already decided on (see
    /// [`uncovered_paths`]): it isn't asked about again.
    pub reviewed: Vec<String>,
}

impl LastUpdate {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("updatedAt".into(), self.updated_at.clone().into());
        m.insert("command".into(), self.command.clone().into());
        if let Some(h) = &self.git_head {
            m.insert("gitHead".into(), h.clone().into());
        }
        m.insert("model".into(), self.model.clone().into());
        m.insert("status".into(), self.status.clone().into());
        if let Some(l) = &self.language {
            m.insert("language".into(), l.clone().into());
        }
        if !self.reviewed.is_empty() {
            m.insert("uncoveredReviewed".into(), self.reviewed.clone().into());
        }
        Value::Object(m)
    }

    /// As OpenWiki reads one back (lenient: unknown values fall back).
    pub fn from_json(v: &Value) -> Option<LastUpdate> {
        let s = |k: &str| v.get(k).and_then(Value::as_str);
        Some(LastUpdate {
            updated_at: s("updatedAt")?.to_owned(),
            command: if s("command")? == "init" { "init" } else { "update" }.to_owned(),
            git_head: s("gitHead").map(str::to_owned),
            model: s("model")?.to_owned(),
            status: if s("status") == Some("interrupted") { "interrupted" } else { "complete" }.to_owned(),
            language: s("language").map(str::to_owned),
            reviewed: v.get("uncoveredReviewed").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect(),
        })
    }
}

/// The last update's record, if there is one that can be read.
///
/// # Errors
///
/// The I/O error's message when the record is there but can't be read.
pub fn read_last_update(root: &Path) -> Result<Option<LastUpdate>> {
    match std::fs::read(root.join(UPDATE_METADATA_PATH)) {
        Ok(b) => Ok(serde_json::from_slice::<Value>(&b).ok().and_then(|v| LastUpdate::from_json(&v))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Records a run's end (or start, as interrupted).
///
/// `git_head` None takes the current HEAD, Some(None) leaves it out. An
/// update keeps the uncovered code already decided on; a wiki written anew
/// starts over.
///
/// # Errors
///
/// The I/O error's message when the record can't be written.
pub fn write_last_update(root: &Path, command: &str, model: &str, status: &str, language: Option<&str>, git_head: Option<Option<&str>>) -> Result<()> {
    let head = match git_head {
        Some(None) => None,
        Some(Some(h)) => Some(h.to_owned()),
        None => self::git_head(root),
    };
    let reviewed = if command == "update" { read_last_update(root)?.map(|l| l.reviewed).unwrap_or_default() } else { Vec::new() };
    let m = LastUpdate {
        reviewed,
        updated_at: util::now_iso(),
        command: command.to_owned(),
        git_head: head,
        model: model.to_owned(),
        status: status.to_owned(),
        language: language.filter(|l| !l.is_empty()).map(str::to_owned),
    };
    util::write_text_atomic(&root.join(UPDATE_METADATA_PATH), &format!("{}\n", util::json_pretty(&m.to_json())), 0o644).map_err(|e| e.to_string())
}

/// harlo: adds uncovered code a plan decided on to the last update's record.
///
/// # Errors
///
/// The I/O error's message when the record can't be read or written.
pub fn add_reviewed(root: &Path, paths: &[String]) -> Result<()> {
    let Some(mut last) = read_last_update(root)? else { return Ok(()) };
    let before = last.reviewed.len();
    for p in paths {
        if !last.reviewed.contains(p) {
            last.reviewed.push(p.clone());
        }
    }
    if last.reviewed.len() == before {
        return Ok(());
    }
    last.reviewed.sort_by(|a, b| util::cmp16(a, b));
    util::write_text_atomic(&root.join(UPDATE_METADATA_PATH), &format!("{}\n", util::json_pretty(&last.to_json())), 0o644).map_err(|e| e.to_string())
}

/// The wiki's own instructions (openwiki/INSTRUCTIONS.md), when it has any.
///
/// # Errors
///
/// The I/O error's message when the file is there but can't be read.
pub fn wiki_goal(root: &Path) -> Result<Option<String>> {
    match std::fs::read(root.join(WIKI_DIR).join(INSTRUCTIONS_FILE)) {
        Ok(b) => {
            let t = String::from_utf8_lossy(&b);
            let t = crate::jsre::trim(&t);
            Ok((!t.is_empty()).then(|| t.to_owned()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// ---- The wiki's content ----

/// A hash of the wiki's files, its run records left out (an update that
/// changes none of them changes nothing).
///
/// # Errors
///
/// The I/O error's message when a file of the wiki can't be read.
pub fn content_snapshot(root: &Path) -> Result<String> {
    let mut h = Sha256::new();
    add_dir(&mut h, &root.join(WIKI_DIR), "")?;
    Ok(hex(&h.finalize()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn race(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory | std::io::ErrorKind::IsADirectory) || e.raw_os_error() == Some(21) || e.raw_os_error() == Some(20)
}

fn add_dir(h: &mut Sha256, dir: &Path, rel: &str) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if race(&e) => {
            h.update(b"missing");
            return Ok(());
        }
        Err(e) => return Err(e.to_string()),
    };
    let mut list: Vec<(String, std::fs::FileType)> = Vec::new();
    for e in entries {
        let e = e.map_err(|e| e.to_string())?;
        let t = e.file_type().map_err(|e| e.to_string())?;
        list.push((e.file_name().to_string_lossy().into_owned(), t));
    }
    list.sort_by(|a, b| crate::collate::compare(&a.0, &b.0));
    for (name, t) in list {
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if r == ".last-update.json" || r == RUN_STATE_NAME {
            continue;
        }
        let p = dir.join(&name);
        if t.is_dir() {
            h.update(format!("dir:{r}\0").as_bytes());
            add_dir(h, &p, &r)?;
            continue;
        }
        if !t.is_file() {
            continue;
        }
        match std::fs::read(&p) {
            Ok(b) => {
                h.update(format!("file:{r}\0").as_bytes());
                h.update(&b);
                h.update(b"\0");
            }
            Err(e) if race(&e) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

// ---- The source ----

const FINGERPRINT_VERSION: &str = "openwiki-source-fingerprint-v1";

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub fingerprint: String,
    pub git_head: Option<String>,
}

fn field(h: &mut Sha256, label: &str, value: &[u8]) {
    h.update(label.as_bytes());
    h.update(b"\0");
    h.update(value.len().to_string().as_bytes());
    h.update(b"\0");
    h.update(value);
    h.update(b"\0");
}

fn fail(m: &str) -> String {
    format!("Git failed while creating the repository source fingerprint: git {m}")
}

fn split_nul(out: &[u8]) -> Result<Vec<String>> {
    if out.is_empty() {
        return Ok(Vec::new());
    }
    if *out.last().unwrap() != 0 {
        return Err("Git returned non-NUL-terminated fingerprint output.".into());
    }
    Ok(out[..out.len() - 1].split(|b| *b == 0).map(|s| String::from_utf8_lossy(s).into_owned()).collect())
}

fn git_path(v: &str) -> Result<String> {
    if v.is_empty() || v.starts_with('/') {
        return Err(format!("Git returned an invalid repository path: {v}"));
    }
    let n = crate::store::posix_normalize(v);
    if n == ".." || n.starts_with("../") {
        return Err(format!("Git returned an escaping repository path: {v}"));
    }
    Ok(n)
}

fn is_source_path(p: &str, ignore: &Ignore) -> bool {
    if p == ignore::FILE {
        return true;
    }
    if p == ".git" || p.starts_with(".git/") {
        return false;
    }
    !is_wiki_path(p) && !ignore.ignores(p, false)
}

/// A fingerprint of every source the wiki may read (HEAD, the work tree's
/// status, and each visible file's kind, mode and bytes), and HEAD.
///
/// # Errors
///
/// The message when a visible file can't be read.
pub fn source_snapshot(root: &Path, ignore: &Ignore) -> Result<Source> {
    if !root.is_absolute() {
        return Err("Repository source fingerprint requires an absolute root.".into());
    }
    // (OpenWiki asks all four at once and reports the first to fail: a
    // list, since the head takes two calls when it fails.)
    let tracked_out = git_strict(root, &["ls-files", "--cached", "-z"]).map_err(|_| fail("ls-files --cached -z"))?;
    let untracked_out = git_strict(root, &["ls-files", "--others", "--exclude-standard", "-z"]).map_err(|_| fail("ls-files --others --exclude-standard -z"))?;
    let status_out = git_strict(root, &["status", "--porcelain=v1", "--untracked-files=all", "--no-renames", "-z"])
        .map_err(|_| fail("status --porcelain=v1 --untracked-files=all --no-renames -z"))?;
    let head = match git_strict(root, &["rev-parse", "--verify", "HEAD"]) {
        Ok(o) => String::from_utf8_lossy(&o).trim_end().to_owned(),
        Err(_) => match git_strict(root, &["symbolic-ref", "-q", "HEAD"]) {
            Ok(o) if !String::from_utf8_lossy(&o).trim_end().is_empty() => format!("unborn:{}", String::from_utf8_lossy(&o).trim_end()),
            _ => return Err("Unable to resolve repository HEAD for fingerprinting.".into()),
        },
    };
    if head.is_empty() {
        return Err("Unable to resolve repository HEAD for fingerprinting.".into());
    }
    let tracked: Vec<String> = split_nul(&tracked_out)?.iter().map(|p| git_path(p)).collect::<Result<_>>()?;
    let tracked_set: std::collections::HashSet<&str> = tracked.iter().map(String::as_str).collect();
    let mut candidates: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in tracked.iter().cloned().chain(split_nul(&untracked_out)?.iter().map(|p| git_path(p)).collect::<Result<Vec<_>>>()?) {
        if seen.insert(p.clone()) {
            candidates.push(p);
        }
    }
    if std::fs::symlink_metadata(root.join(ignore::FILE)).is_ok() && seen.insert(ignore::FILE.to_owned()) {
        candidates.push(ignore::FILE.to_owned());
    }
    let mut visible: Vec<String> = candidates.into_iter().filter(|p| is_source_path(p, ignore)).collect();
    visible.sort_by(|a, b| util::cmp16(a, b));
    let mut status: Vec<(String, String)> = Vec::new();
    for rec in split_nul(&status_out)? {
        if rec.len() < 4 || rec.as_bytes()[2] != b' ' {
            return Err("Git returned malformed porcelain status output.".into());
        }
        let p = git_path(&rec[3..])?;
        if is_source_path(&p, ignore) {
            status.push((rec[..2].to_owned(), p));
        }
    }
    status.sort_by(|a, b| util::cmp16(&format!("{}\0{}", a.0, a.1), &format!("{}\0{}", b.0, b.1)));
    let mut h = Sha256::new();
    field(&mut h, "format", FINGERPRINT_VERSION.as_bytes());
    field(&mut h, "head", head.as_bytes());
    for (code, p) in &status {
        field(&mut h, "status-code", code.as_bytes());
        field(&mut h, "status-path", p.as_bytes());
    }
    for p in &visible {
        source_entry(&mut h, root, p, tracked_set.contains(p.as_str()))?;
    }
    Ok(Source { fingerprint: format!("sha256:{}", hex(&h.finalize())), git_head: (!head.starts_with("unborn:")).then_some(head) })
}

fn source_entry(h: &mut Sha256, root: &Path, rel: &str, tracked: bool) -> Result<()> {
    let abs = root.join(rel);
    if !abs.starts_with(root) || abs == root {
        return Err(format!("Source fingerprint path escaped the repository: {rel}"));
    }
    field(h, "path", rel.as_bytes());
    let meta = match std::fs::symlink_metadata(&abs) {
        Ok(m) => m,
        Err(e) if tracked && e.kind() == std::io::ErrorKind::NotFound => {
            field(h, "kind", b"tracked-missing");
            return Ok(());
        }
        Err(_) => return Err(format!("Unable to inspect source path {rel}.")),
    };
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if meta.is_file() {
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true);
        #[cfg(unix)]
        opts.custom_flags(libc_nofollow());
        let mut f = opts.open(&abs).map_err(|_| format!("Unable to safely open source path {rel}."))?;
        let opened = f.metadata().map_err(|_| format!("Unable to safely open source path {rel}."))?;
        #[cfg(unix)]
        let same = opened.is_file() && opened.dev() == meta.dev() && opened.ino() == meta.ino();
        #[cfg(not(unix))]
        let same = opened.is_file();
        if !same {
            return Err(format!("Source path changed while fingerprinting {rel}."));
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut bytes).map_err(|_| format!("Unable to safely open source path {rel}."))?;
        #[cfg(unix)]
        let exec = opened.mode() & 0o111 != 0;
        #[cfg(not(unix))]
        let exec = false;
        field(h, "executable", if exec { b"yes" } else { b"no" });
        field(h, "kind", b"file");
        field(h, "bytes", &bytes);
        return Ok(());
    }
    #[cfg(unix)]
    let exec = meta.mode() & 0o111 != 0;
    #[cfg(not(unix))]
    let exec = false;
    field(h, "executable", if exec { b"yes" } else { b"no" });
    if meta.file_type().is_symlink() {
        field(h, "kind", b"symlink");
        let target = std::fs::read_link(&abs).map_err(|_| format!("Unable to inspect source path {rel}."))?;
        #[cfg(unix)]
        let t = std::os::unix::ffi::OsStrExt::as_bytes(target.as_os_str()).to_vec();
        #[cfg(not(unix))]
        let t = target.to_string_lossy().into_owned().into_bytes();
        field(h, "target", &t);
        return Ok(());
    }
    if meta.is_dir() {
        field(h, "kind", b"directory");
        return Ok(());
    }
    Err(format!("Unsupported source entry type at {rel}."))
}

#[cfg(target_os = "linux")]
fn libc_nofollow() -> i32 {
    0o400000
}
#[cfg(all(unix, not(target_os = "linux")))]
fn libc_nofollow() -> i32 {
    0x0100
}

// ---- Whether an update has anything to do ----

/// Whether an update can be skipped: with the last update's record, or why not.
#[derive(Debug)]
pub enum Noop {
    Skip { git_head: String, model: String, language: Option<String> },
    Run(String),
}

static STATUS_LINE: std::sync::LazyLock<crate::jsre::Re> = std::sync::LazyLock::new(|| crate::jsre::Re::new(r"^[ !?ACDMRTU]{1,2} (.+)$", "u"));
static SHORT_STATUS: std::sync::LazyLock<crate::jsre::Re> = std::sync::LazyLock::new(|| crate::jsre::Re::new(r"^(?:[ MARCUD?!]{2})\s+(.+)$", "u"));
static NAME_STATUS: std::sync::LazyLock<crate::jsre::Re> = std::sync::LazyLock::new(|| crate::jsre::Re::new(r"^(?:[ACDMRTUXB]\d*)\s+(.+)$", "u"));

fn is_metadata_status_line(line: &str) -> bool {
    let p = STATUS_LINE.exec(line).and_then(|m| m.get(1).map(str::to_owned)).unwrap_or_else(|| line.to_owned());
    let p = crate::jsre::trim(&p).replace('\\', "/");
    [PAGE_MANIFEST_PATH, UPDATE_METADATA_PATH].iter().any(|m| p == *m || p.ends_with(&format!(" -> {m}")))
}

fn references_ignored(line: &str, ignore: &Ignore) -> bool {
    let text = SHORT_STATUS.exec(line).and_then(|m| m.get(1).map(str::to_owned)).or_else(|| NAME_STATUS.exec(crate::jsre::trim(line)).and_then(|m| m.get(1).map(str::to_owned)));
    let Some(text) = text else { return false };
    let parts: Vec<&str> = if text.contains('\t') {
        text.split('\t').collect()
    } else if text.contains(" -> ") {
        text.split(" -> ").collect()
    } else {
        vec![text.as_str()]
    };
    parts.into_iter().map(normalize_git_path).filter(|p| !p.is_empty()).any(|p: String| ignore.ignores(&p, false))
}

/// Whether an update can be skipped: the last one complete at a commit,
/// the same language, a clean work tree, and no commits since but the
/// wiki's own or ignored paths.
///
/// # Errors
///
/// The I/O error's message when the last update's record is there but can't be
/// read.
pub fn update_noop(root: &Path, ignore: &Ignore, requested_language: Option<&str>) -> Result<Noop> {
    let Some(last) = read_last_update(root)? else { return Ok(Noop::Run("missing previous update git head".into())) };
    let Some(last_head) = last.git_head.clone() else { return Ok(Noop::Run("missing previous update git head".into())) };
    if last.status == "interrupted" {
        return Ok(Noop::Run("previous update was interrupted".into()));
    }
    if let Some(req) = requested_language
        && crate::lang::primary(Some(req)) != crate::lang::primary(last.language.as_deref()) {
            return Ok(Noop::Run("output language changed".into()));
        }
    let Some(head) = git_head(root) else { return Ok(Noop::Run("missing current git head".into())) };
    let status = git_text(root, &["status", "--short", "--untracked-files=all"]);
    let meaningful = status.split('\n').map(str::trim_end).filter(|l| !l.is_empty()).filter(|l| !is_metadata_status_line(l)).any(|l| !references_ignored(l, ignore));
    if meaningful {
        return Ok(Noop::Run("worktree has changes".into()));
    }
    if head != last_head {
        let diff = git_text(root, &["diff", "--name-only", &format!("{last_head}..HEAD")]);
        let paths: Vec<String> = diff.split('\n').map(normalize_git_path).filter(|p| !p.is_empty()).collect();
        if paths.is_empty() || paths.iter().any(|p| !is_wiki_path(p) && !ignore.ignores(p, false)) {
            return Ok(Noop::Run("git head changed".into()));
        }
    }
    Ok(Noop::Skip { git_head: head, model: last.model, language: last.language })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_are_keyed() {
        assert_eq!(top_level_key("export const added = () => 2;").as_deref(), Some("export const added"));
        assert_eq!(top_level_key("int added(void) { return 2; }").as_deref(), Some("int added"));
        assert_eq!(top_level_key("pub const LIMIT: u32 = 100;").as_deref(), Some("pub const LIMIT"));
        assert_eq!(top_level_key("LIMIT = 200").as_deref(), Some("LIMIT"));
        for none in ["    x += 1;", "}", "// note", "# note", "use std::fs;", "import os", "#[derive(Debug)]", "@decorator", "", "#include <a.h>"] {
            assert_eq!(top_level_key(none), None, "{none:?}");
        }
        assert_eq!(unquote(r#""src/a\tb.rs""#), "src/a\tb.rs");
        assert_eq!(unquote(r#""src/\346\226\260.rs""#), "src/新.rs");
        assert_eq!(unquote(r#""src/q\"x.rs""#), "src/q\"x.rs");
        assert_eq!(unquote("src/plain.rs"), "src/plain.rs");
    }

    #[test]
    fn definitions_are_found() {
        assert_eq!(defined_name("pub fn run(x: u32) {").as_deref(), Some("run"));
        assert_eq!(defined_name("pub(crate) async fn go<T>() {}").as_deref(), Some("go"));
        assert_eq!(defined_name("export default function App() {").as_deref(), Some("App"));
        assert_eq!(defined_name("class Queue:").as_deref(), Some("Queue"));
        assert_eq!(defined_name("async def fetch(url):").as_deref(), Some("fetch"));
        assert_eq!(defined_name("func (s *Server) Start() error {").as_deref(), Some("Start"), "a Go method: past its receiver");
        assert_eq!(defined_name("    def inner(self):").as_deref(), Some("inner"), "a method too");
        assert_eq!(defined_name("let x = 1;"), None);
        // Methods and functions without a keyword.
        for (line, name) in [
            ("    added() { return 2; }", "added"),
            ("  async load(id: string): Promise<Order> {", "load"),
            ("  get size(): number {", "size"),
            ("    public int added() {", "added"),
            ("    int added(void) const {", "added"),
            ("  private added = (): number => 2;", "added"),
            ("  added = () => 2;", "added"),
            ("  added: async (x) => {", "added"),
            ("  added: function () {", "added"),
            ("  *items() {", "items"),
            ("  fun added() = 2", "added"),
            ("  added<T>(value: T): T { return value; }", "added"),
            ("  added<K, V>(m: Map<K, V>): V {", "added"),
            ("    std::vector<int> added(int n) const {", "added"),
            ("  added = <T>(x: T) => x;", "added"),
            ("  added<T extends (...args: any[]) => any>(value: T): T {", "added"),
            ("  added<T extends { a: (x: number) => void }>(value: T): T {", "added"),
            ("  added(value: number): number;", "added"),
            ("  added(): () => void {", "added"),
            ("  added(value = ')'): string { return value; }", "added"),
            ("  added(value = \")\"): string { return value; }", "added"),
            ("  added(value = `)`): string { return value; }", "added"),
            ("  added(value: number /* ) */): number { return value; }", "added"),
            ("  added?(): number;", "added"),
            ("  added?<T>(value: T): T;", "added"),
            ("  added?: () => number;", "added"),
        ] {
            assert_eq!(declared_name(&[line], 0).as_deref(), Some(name), "{line}");
        }
        // Statements and calls aren't.
        for line in [
            "    if (x) {", "    } else if (x) {", "    for (const a of b) {", "    while (go()) {", "    catch (e) {", "    switch (k) {",
            "    foo(x).then(y => {", "    items.forEach((x) => {", "    Some(x) => {", "    foo(x);", "    foo<Bar>(x);", "    a ? f(x) : g(y);", "    a ?(b) : c;", "    log(')') // ) {", "    go(x) /* { */;",
            "    let x = foo(1);", "    const y = bar(2);", "    print(x)", "    self.go(x)", "    return f(x) {", "    x == y(1)", "    onClicked: go()",
            "    property var x: compute(a)", "    case 1: f(x)", "    a.b = function () {",
        ] {
            assert_eq!(declared_name(&[line], 0), None, "{line}");
        }
        // Over several lines.
        for (run, name) in [
            (&["  added(", "    value: number,", "  ): number {", "    return value;", "  }"][..], Some("added")),
            (&["  added(value: number): number", "  {", "    return value;", "  }"][..], Some("added")),
            (&["int added(void)", "{"][..], Some("added")),
            (&["  added<T>(", "    value: T,", "  ): T {"][..], Some("added")),
            (&["    foo(", "      a,", "    );", "    if (b) {"][..], None),
            (&["    print(x)", "    for k in {1, 2}:"][..], None),
            (&["    go(x)", "    if (y) {"][..], None),
            (&["    run(a, () => {", "      b();", "    });"][..], None),
            (&["    describe(\"a\", () => {", "      it();", "    });"][..], None),
            (&["    spawn(async move {", "      go().await;", "    });"][..], None),
            (&["    foo(x)", "    bar(y)"][..], None),
            // Generics over lines, a constraint with a function type, a
            // long signature: all read through.
            (&["  added<", "    T,", "  >(value: T): T {", "    return value;", "  }"][..], Some("added")),
            (&["  added<T extends (...args: any[]) => any>(value: T): T {", "    return value;", "  }"][..], Some("added")),
            (&["static int", "added(void)", "{"][..], Some("added")),
            // Ending inside one: counted, not taken as unchanged.
            (&["  added<", "    T,"][..], Some("added")),
            (&["  added(", "    value: number,"][..], Some("added")),
            (&["  added(value: number): number"][..], Some("added")),
            (&["  added(", "    value: number, // ) {", "  ): number {"][..], Some("added")),
            (&["  added(x: &'a str) -> u8 {"][..], Some("added")),
        ] {
            assert_eq!(declared_name(run, 0).as_deref(), name, "{run:?}");
        }
        assert_eq!(header_path("b/src/space name.rs\t"), "b/src/space name.rs");
        assert_eq!(header_path("\"b/src/t\\tab.rs\"\t"), "b/src/t\tab.rs");
        assert_eq!(header_path("\"b/src/q\\\"uote.rs\""), "b/src/q\"uote.rs");
        assert!(is_test_path("tests/test_orders.py") && is_test_path("src/a_test.go") && is_test_path("web/app.spec.ts") && !is_test_path("src/testing.rs"));
    }

    #[test]
    fn blank_lines_collapse() {
        assert_eq!(collapse_blank_lines("a\n\n\n\nb\nc\n\nd"), "a\n\nb\nc\n\nd");
    }
}
