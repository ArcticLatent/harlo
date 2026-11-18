//! A source file's declarations ("units"), each with its lines and a hash.
//!
//! A wiki page's footnote names a file and a declaration in it
//! (`crates/x/src/run.rs, fn start`). [`scan`] finds every declaration a
//! file has, in the languages harlo's projects use, from the text alone:
//! no language server, no build. Each [`Unit`] carries its lines, so a
//! footnote opens the code where it is now, and the hash of its own text,
//! so the engine can tell a declaration whose code changed (the claims
//! citing it are stale) from one that only moved.
//!
//! A unit's own text is its lines without those of the units nested in it:
//! a method's change makes claims about that method stale, not every claim
//! about its type. Lines are compared trimmed and without blank lines, so
//! re-indenting code changes no hash.
//!
//! Languages: Rust, Go, Python, JavaScript and TypeScript, Swift, C and
//! C++, QML, Markdown (headings) and TOML (tables). The rules follow each
//! language's common forms; code they misread is still listed as a file,
//! and its claims are checked against the whole file instead.

// Rust guideline compliant 2026-10-09

use crate::lex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;

/// A language the scanner knows, by file extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Rust,
    Go,
    Python,
    /// JavaScript and TypeScript.
    Script,
    Swift,
    /// C, C++ and Objective-C headers.
    C,
    Qml,
    Markdown,
    Toml,
}

/// The language of a file at `path`, if the scanner knows it.
pub fn lang(path: &str) -> Option<Lang> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => Lang::Rust,
        "go" => Lang::Go,
        "py" | "pyi" => Lang::Python,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Lang::Script,
        "swift" => Lang::Swift,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "m" | "mm" => Lang::C,
        "qml" => Lang::Qml,
        "md" | "markdown" => Lang::Markdown,
        "toml" => Lang::Toml,
        _ => return None,
    })
}

/// What a unit declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A function or a method.
    Function,
    /// A type: a struct, enum, class, trait, protocol, interface or alias.
    Type,
    /// A block adding to a type: a Rust `impl`, a Swift `extension`.
    Impl,
    /// A module or namespace.
    Module,
    /// A constant or a static.
    Constant,
    Macro,
    /// A property (QML, Swift computed properties).
    Property,
    Signal,
    /// A QML inline component.
    Component,
    /// A QML object with an `id`.
    Object,
    /// A Markdown heading.
    Heading,
    /// A TOML table.
    Table,
}

impl Kind {
    /// The word a footnote uses for it (`fn`, `type`, `property`, …).
    pub fn word(self) -> &'static str {
        match self {
            Kind::Function => "fn",
            Kind::Type => "type",
            Kind::Impl => "impl",
            Kind::Module => "mod",
            Kind::Constant => "const",
            Kind::Macro => "macro",
            Kind::Property => "property",
            Kind::Signal => "signal",
            Kind::Component => "component",
            Kind::Object => "id",
            Kind::Heading => "section",
            Kind::Table => "table",
        }
    }

    /// Whether units nested in it take its name as a prefix.
    fn qualifies(self) -> bool {
        matches!(self, Kind::Type | Kind::Impl | Kind::Module | Kind::Component | Kind::Object)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// A declaration in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// Its name, with its container's (`Ledger::post`, `Cart.total`).
    pub name: String,
    pub kind: Kind,
    /// Its first and last lines, from 1.
    pub start: usize,
    pub end: usize,
    /// Whether other code can use it: `pub`, `export`, a capital in Go,
    /// no leading `_` in Python, not `private` in Swift.
    pub public: bool,
    /// Whether it's test code (`#[test]`, `mod tests`, `test_…`).
    pub test: bool,
    /// The unit it's nested in, as an index into the same list.
    pub parent: Option<usize>,
    /// The hash of its own text (see the module docs).
    pub hash: String,
    /// The hash as wikis stamped before plain comments were left out of it:
    /// a claim stamped then is current if this still matches.
    pub legacy: String,
}

impl Unit {
    /// Its name without its containers' (`post` for `Ledger::post`).
    pub fn short(&self) -> &str {
        let after = self.name.rsplit_once("::").map_or(self.name.as_str(), |(_, s)| s);
        after.rsplit_once('.').map_or(after, |(_, s)| s)
    }
}

/// A short hash of `text` (16 hex digits of its SHA-256).
pub fn digest(text: &[u8]) -> String {
    let sum = Sha256::digest(text);
    sum[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// The declarations of `text`, the contents of the file at `path`, in order.
///
/// Returns nothing for a language the scanner doesn't know.
pub fn scan(path: &str, text: &str) -> Vec<Unit> {
    let Some(lang) = lang(path) else { return Vec::new() };
    let masked_bytes = lex::mask(lang, text);
    let masked = String::from_utf8_lossy(&masked_bytes);
    let file = File::new(path, lang, text, &masked);
    let mut units = match lang {
        Lang::Markdown => headings(&file),
        Lang::Toml => tables(&file),
        Lang::Python => indented(&file),
        _ => braced(&file),
    };
    units.sort_by_key(|u| (u.start, std::cmp::Reverse(u.end)));
    let mut units = finish(&file, units);
    hash(text, &lex::plain_comments(lang, text), &mut units);
    units
}

/// Fills in `units`' hashes from `text` (see the module docs): `plain` are
/// the byte ranges of the comments the hash leaves out.
pub fn hash(text: &str, plain: &[(usize, usize)], units: &mut [Unit]) {
    let spans: Vec<(usize, usize)> = units.iter().map(|u| (u.start, u.end)).collect();
    let hashes = own_hashes(text, plain, &spans);
    let legacy = own_hashes(text, &[], &spans);
    for ((u, h), l) in units.iter_mut().zip(hashes).zip(legacy) {
        u.hash = h;
        u.legacy = l;
    }
}

/// Fills in `units`' old-style hashes (comments counted) from `text`, for
/// units whose hashes came from elsewhere.
pub fn fill_legacy(text: &str, units: &mut [Unit]) {
    let spans: Vec<(usize, usize)> = units.iter().map(|u| (u.start, u.end)).collect();
    for (u, l) in units.iter_mut().zip(own_hashes(text, &[], &spans)) {
        u.legacy = l;
    }
}

/// The hash of each span's own text: its lines without those of the spans
/// nested in it (each line belongs to the innermost span holding it),
/// trimmed, without blank lines, `blank` byte ranges left out.
fn own_hashes(text: &str, blank: &[(usize, usize)], spans: &[(usize, usize)]) -> Vec<String> {
    let mut bytes = text.as_bytes().to_vec();
    for &(a, b) in blank {
        for byte in &mut bytes[a.min(text.len())..b.min(text.len())] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    // Blanking whole comments (whole characters) keeps the text UTF-8.
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let mut owner: Vec<Option<usize>> = vec![None; lines.len()];
    for (i, &(start, end)) in spans.iter().enumerate() {
        for slot in owner.iter_mut().take(end.min(lines.len())).skip(start.saturating_sub(1)) {
            *slot = Some(i);
        }
    }
    let mut own: Vec<String> = vec![String::new(); spans.len()];
    for (line, o) in lines.iter().zip(&owner) {
        if let Some(i) = o {
            let t = line.trim();
            if !t.is_empty() {
                own[*i].push_str(t);
                own[*i].push('\n');
            }
        }
    }
    own.iter().map(|t| digest(t.as_bytes())).collect()
}

/// A file's text, its masked text, and their lines.
struct File<'a> {
    path: &'a str,
    lang: Lang,
    lines: Vec<&'a str>,
    masked: Vec<&'a str>,
    /// Each line's first byte in the masked text.
    offsets: Vec<usize>,
    masked_text: &'a str,
    /// Brace depth at each line's start.
    depth: Vec<usize>,
    /// Each `{` matched with its `}` (byte offsets in the masked text).
    pairs: HashMap<usize, usize>,
}

impl<'a> File<'a> {
    fn new(path: &'a str, lang: Lang, text: &'a str, masked: &'a str) -> File<'a> {
        let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
        let mut offsets = Vec::with_capacity(lines.len());
        let mut at = 0;
        let masked_lines: Vec<&str> = masked
            .split('\n')
            .map(|l| {
                offsets.push(at);
                at += l.len() + 1;
                l.strip_suffix('\r').unwrap_or(l)
            })
            .collect();
        let mut depth = Vec::with_capacity(lines.len());
        let mut pairs = HashMap::new();
        let mut stack = Vec::new();
        let mut d = 0usize;
        for (i, line) in masked_lines.iter().enumerate() {
            depth.push(d);
            for (j, c) in line.bytes().enumerate() {
                match c {
                    b'{' => {
                        stack.push(offsets[i] + j);
                        d += 1;
                    }
                    b'}' => {
                        if let Some(open) = stack.pop() {
                            pairs.insert(open, offsets[i] + j);
                        }
                        d = d.saturating_sub(1);
                    }
                    _ => {}
                }
            }
        }
        File { path, lang, lines, masked: masked_lines, offsets, masked_text: masked, depth, pairs }
    }

    /// The line holding byte `offset` of the masked text.
    fn line_of(&self, offset: usize) -> usize {
        self.offsets.partition_point(|&o| o <= offset).saturating_sub(1)
    }

    fn indent(&self, i: usize) -> usize {
        let l = self.lines[i];
        l.len() - l.trim_start().len()
    }

    /// The next line after `i` with code on it.
    fn next_code(&self, i: usize) -> Option<usize> {
        (i + 1..self.masked.len()).find(|&j| !self.masked[j].trim().is_empty())
    }
}

/// A declaration found on a line, before its lines are known.
struct Decl {
    kind: Kind,
    name: String,
    /// Its own visibility; None takes its container's.
    public: Option<bool>,
    test: bool,
    /// Go's methods: the type they belong to.
    owner: Option<String>,
}

fn decl(kind: Kind, name: &str, public: Option<bool>) -> Option<Decl> {
    let name = name.trim();
    (!name.is_empty()).then(|| Decl { kind, name: name.to_owned(), public, test: false, owner: None })
}

/// The identifier `s` starts with.
fn ident(s: &str) -> &str {
    let end = s.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(s.len());
    &s[..end]
}

/// `s` without any of `words` at its start, as often as they're there.
fn strip_words<'s>(mut s: &'s str, words: &[&str]) -> (&'s str, Vec<&'static str>) {
    let mut seen = Vec::new();
    loop {
        let before = s;
        for w in words {
            if let Some(rest) = s.strip_prefix(w)
                && (rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace()))
            {
                s = rest.trim_start();
                // (`'static` would be nicer, but the caller's words live long enough.)
                seen.push(match *w {
                    "private" => "private",
                    "fileprivate" => "fileprivate",
                    "public" => "public",
                    "open" => "open",
                    "export" => "export",
                    "static" => "static",
                    _ => "",
                });
            }
        }
        if s == before {
            return (s, seen);
        }
    }
}

/// The last path segment of a type written in `s` (`&mut fmt::Display<T>` → `Display`).
fn type_name(s: &str) -> &str {
    let s = s.trim().trim_start_matches(['&', '*']).trim_start();
    let s = s.strip_prefix("mut ").unwrap_or(s);
    let s = s.strip_prefix("dyn ").unwrap_or(s).trim_start();
    let head = &s[..s.find(['<', '{', '(', ' ', ':', ';']).unwrap_or(s.len())];
    let head = head.rsplit("::").next().unwrap_or(head);
    head.rsplit('.').next().unwrap_or(head)
}

const CONTROL: [&str; 14] = ["if", "for", "while", "switch", "catch", "return", "function", "with", "else", "do", "case", "new", "await", "throw"];

fn rust(t: &str, parent: Option<Kind>) -> Option<Decl> {
    let mut s = t;
    let mut public = false;
    if let Some(rest) = s.strip_prefix("pub(") {
        public = true;
        s = rest.split_once(')').map_or(rest, |(_, r)| r).trim_start();
    } else if let Some(rest) = s.strip_prefix("pub ") {
        public = true;
        s = rest.trim_start();
    }
    let (s, _) = strip_words(s, &["async", "unsafe", "default", "extern"]);
    // A trait's items have no `pub`: they're as visible as the trait.
    let vis = if !public && parent == Some(Kind::Type) { None } else { Some(public) };
    let in_fn = parent == Some(Kind::Function);
    if let Some(rest) = s.strip_prefix("const ") {
        let rest = rest.trim_start();
        if let Some(f) = rest.strip_prefix("fn ") {
            return decl(Kind::Function, ident(f.trim_start()), vis);
        }
        return (!in_fn && !rest.starts_with('_')).then(|| decl(Kind::Constant, ident(rest), vis)).flatten();
    }
    if let Some(rest) = s.strip_prefix("static ") {
        let rest = rest.trim_start();
        let rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
        return (!in_fn).then(|| decl(Kind::Constant, ident(rest), vis)).flatten();
    }
    for (k, kind) in [("fn ", Kind::Function), ("struct ", Kind::Type), ("enum ", Kind::Type), ("union ", Kind::Type), ("trait ", Kind::Type), ("type ", Kind::Type)] {
        if let Some(rest) = s.strip_prefix(k) {
            return decl(kind, ident(rest.trim_start()), vis);
        }
    }
    if let Some(rest) = s.strip_prefix("mod ") {
        // (`mod name;` declares a file, not a unit here.)
        let name = ident(rest.trim_start());
        let mut d = decl(Kind::Module, name, vis)?;
        d.test = name == "tests" || name == "test";
        return (!s.contains(';') || s.contains('{')).then_some(d);
    }
    if let Some(rest) = s.strip_prefix("macro_rules!") {
        return decl(Kind::Macro, ident(rest.trim_start()), Some(true));
    }
    if s.starts_with("impl") && matches!(s.as_bytes().get(4), Some(b' ' | b'<')) {
        let mut rest = &s[4..];
        // Its generics, `impl<T: Fn<A>>`.
        if rest.starts_with('<') {
            let mut depth = 0;
            for (i, c) in rest.char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            rest = &rest[i + 1..];
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        let rest = rest.split(" where").next().unwrap_or(rest);
        let target = rest.rsplit_once(" for ").map_or(rest, |(_, t)| t);
        return decl(Kind::Impl, type_name(target), Some(false));
    }
    None
}

fn go(t: &str, top: bool, test_file: bool) -> Option<Decl> {
    let public = |n: &str| Some(n.starts_with(|c: char| c.is_uppercase()));
    if let Some(rest) = t.strip_prefix("func ") {
        let rest = rest.trim_start();
        if let Some(recv) = rest.strip_prefix('(') {
            let (recv, after) = recv.split_once(')')?;
            let name = ident(after.trim_start());
            let mut d = decl(Kind::Function, name, public(name))?;
            let owner = recv.split_whitespace().last().unwrap_or("").trim_start_matches('*');
            d.owner = Some(type_name(owner).to_owned());
            return Some(d);
        }
        let name = ident(rest);
        let mut d = decl(Kind::Function, name, public(name))?;
        d.test = test_file && ["Test", "Benchmark", "Example", "Fuzz"].iter().any(|p| name.starts_with(p));
        return Some(d);
    }
    if !top {
        return None;
    }
    if let Some(rest) = t.strip_prefix("type ") {
        let name = ident(rest.trim_start());
        return decl(Kind::Type, name, public(name));
    }
    for k in ["const ", "var "] {
        if let Some(rest) = t.strip_prefix(k) {
            let name = ident(rest.trim_start());
            return decl(Kind::Constant, name, public(name));
        }
    }
    None
}

fn script(t: &str, top: bool, in_class: bool) -> Option<Decl> {
    let exported = t.starts_with("export ");
    let (s, _) = strip_words(t, &["export", "default", "declare", "abstract"]);
    let vis = Some(exported);
    let (s, _) = strip_words(s, &["async"]);
    if let Some(rest) = s.strip_prefix("function") {
        let rest = rest.trim_start().trim_start_matches('*').trim_start();
        let name = ident(rest);
        return decl(Kind::Function, if name.is_empty() { "default" } else { name }, vis);
    }
    for (k, kind) in [("class ", Kind::Type), ("interface ", Kind::Type), ("enum ", Kind::Type), ("namespace ", Kind::Module)] {
        if let Some(rest) = s.strip_prefix(k) {
            return decl(kind, ident(rest.trim_start()), vis);
        }
    }
    if let Some(rest) = s.strip_prefix("type ")
        && s.contains('=')
    {
        return decl(Kind::Type, ident(rest.trim_start()), vis);
    }
    if top {
        for k in ["const ", "let ", "var "] {
            if let Some(rest) = s.strip_prefix(k)
                && (s.contains("=>") || s.contains("= function") || s.contains("= async"))
            {
                return decl(Kind::Function, ident(rest.trim_start()), vis);
            }
        }
    }
    if in_class {
        let (m, words) = strip_words(t, &["public", "private", "protected", "static", "async", "get", "set", "readonly", "override"]);
        let name: String = m.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '#').collect();
        let after = m[name.len()..].trim_start();
        let after = if after.starts_with('<') { after.split_once('>').map_or(after, |(_, r)| r).trim_start() } else { after };
        if !name.is_empty() && after.starts_with('(') && !CONTROL.contains(&name.as_str()) && !name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            let private = words.contains(&"private") || name.starts_with('#') || t.starts_with("protected ");
            // (Not private: as visible as its class.)
            return decl(Kind::Function, &name, private.then_some(false));
        }
    }
    None
}

fn swift(t: &str, parent: Option<Kind>, braced_line: bool) -> Option<Decl> {
    // Attributes first: `@MainActor`, `@available(iOS 17, *)`.
    let mut s = t;
    while let Some(rest) = s.strip_prefix('@') {
        let name_len = ident(rest).len();
        let mut rest = &rest[name_len..];
        if rest.starts_with('(') {
            let mut depth = 0;
            for (i, c) in rest.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            rest = &rest[i + 1..];
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        s = rest.trim_start();
    }
    let (mut s, words) = strip_words(
        s,
        &["public", "private", "fileprivate", "internal", "open", "final", "static", "override", "mutating", "nonmutating", "nonisolated", "convenience", "required", "dynamic", "lazy", "indirect", "weak", "unowned", "package"],
    );
    if let Some(rest) = s.strip_prefix("class ")
        && ["func ", "var ", "let "].iter().any(|k| rest.trim_start().starts_with(k))
    {
        s = rest.trim_start();
    }
    let public = (words.contains(&"private") || words.contains(&"fileprivate")).then_some(false);
    if let Some(rest) = s.strip_prefix("func ") {
        let rest = rest.trim_start();
        let end = rest.find(['(', '<']).unwrap_or(rest.len());
        return decl(Kind::Function, rest[..end].trim(), public);
    }
    if s.starts_with("init(") || s.starts_with("init?(") || s.starts_with("init!(") || s.starts_with("init<") {
        return decl(Kind::Function, "init", public);
    }
    if s.starts_with("deinit") && ident(s) == "deinit" {
        return decl(Kind::Function, "deinit", public);
    }
    for k in ["class ", "struct ", "enum ", "protocol ", "actor "] {
        if let Some(rest) = s.strip_prefix(k) {
            return decl(Kind::Type, ident(rest.trim_start()), public);
        }
    }
    if let Some(rest) = s.strip_prefix("extension ") {
        return decl(Kind::Impl, type_name(rest), Some(false));
    }
    if let Some(rest) = s.strip_prefix("typealias ") {
        return decl(Kind::Type, ident(rest.trim_start()), public);
    }
    // A computed property of a type: `var body: some View {`.
    if braced_line && matches!(parent, Some(Kind::Type | Kind::Impl)) {
        for k in ["var ", "let "] {
            if let Some(rest) = s.strip_prefix(k) {
                return decl(Kind::Property, ident(rest.trim_start()), public);
            }
        }
    }
    None
}

fn c_like(t: &str, next: &str, parent: Option<Kind>) -> Option<Decl> {
    if parent.is_some() && parent != Some(Kind::Module) {
        return None;
    }
    let (s, _) = strip_words(t, &["typedef", "export", "inline", "extern"]);
    let s = if s.starts_with("template") { s.split_once('>').map_or(s, |(_, r)| r).trim_start() } else { s };
    for (k, kind) in [("struct ", Kind::Type), ("class ", Kind::Type), ("union ", Kind::Type), ("enum class ", Kind::Type), ("enum ", Kind::Type), ("namespace ", Kind::Module)] {
        if let Some(rest) = s.strip_prefix(k)
            && !s.ends_with(';')
        {
            let name = ident(rest.trim_start());
            if name.is_empty() {
                return None;
            }
            return decl(kind, name, Some(true));
        }
    }
    // A function's definition: a name and its arguments, its body opening on
    // this line or the next.
    let opens = t.ends_with('{') || (!t.ends_with(';') && next.trim_start().starts_with('{'));
    let first = ident(t);
    if !opens || first.is_empty() || !t.contains('(') || CONTROL.contains(&first) || t.contains('=') {
        return None;
    }
    let before = t[..t.find('(')?].trim_end();
    let name = before.rsplit(|c: char| c.is_whitespace() || c == '*' || c == '&').next().unwrap_or(before);
    (!name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit())).then(|| decl(Kind::Function, name, Some(!t.starts_with("static ")))).flatten()
}

fn qml(t: &str) -> Option<Decl> {
    if let Some(rest) = t.strip_prefix("function ") {
        return decl(Kind::Function, ident(rest.trim_start()), None);
    }
    if let Some(rest) = t.strip_prefix("signal ") {
        return decl(Kind::Signal, ident(rest.trim_start()), None);
    }
    let (s, _) = strip_words(t, &["readonly", "required", "default", "final", "virtual", "override"]);
    if let Some(rest) = s.strip_prefix("property ") {
        // `property list<Item> name: …`: the type, then the name.
        let rest = rest.trim_start();
        let after_type = if let Some(open) = rest.find('<').filter(|&o| o < rest.find(' ').unwrap_or(usize::MAX)) {
            rest[open..].split_once('>').map_or("", |(_, r)| r)
        } else {
            rest.split_once(' ').map_or("", |(_, r)| r)
        };
        return decl(Kind::Property, ident(after_type.trim_start()), None);
    }
    if let Some(rest) = t.strip_prefix("component ") {
        return decl(Kind::Component, ident(rest.trim_start()), Some(true));
    }
    // An object (`Rectangle {`, `delegate: Item {`): a unit only with an id,
    // found once its lines are known.
    if t.ends_with('{') {
        let head = t.trim_end_matches('{').trim_end();
        let ty = head.rsplit_once(':').map_or(head, |(_, r)| r).trim();
        if ty.starts_with(|c: char| c.is_ascii_uppercase()) && ty.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            return Some(Decl { kind: Kind::Object, name: String::new(), public: Some(false), test: false, owner: None });
        }
    }
    None
}

/// A declaration's last line: its `{…}` block, or its statement up to `;`,
/// or its own line(s).
fn extent(file: &File<'_>, i: usize) -> usize {
    let strict = matches!(file.lang, Lang::Rust | Lang::C);
    let mut parens = 0i32;
    let mut line = i;
    let limit = (i + 40).min(file.masked.len());
    while line < limit {
        let text = file.masked[line];
        for (j, c) in text.bytes().enumerate() {
            match c {
                b'(' | b'[' => parens += 1,
                b')' | b']' => parens -= 1,
                b'{' if parens <= 0 => {
                    let open = file.offsets[line] + j;
                    return file.pairs.get(&open).map_or(line, |&close| file.line_of(close));
                }
                b';' if parens <= 0 => return line,
                _ => {}
            }
        }
        let trimmed = text.trim_end();
        let continues = parens > 0
            || strict
            || trimmed.ends_with([',', '(', '=', ':', '|', '&', '>'])
            || trimmed.ends_with("=>")
            || file.next_code(line).is_some_and(|n| file.masked[n].trim_start().starts_with(['{', '.', '|', '&']) || file.masked[n].trim_start().starts_with("where"));
        if !continues {
            return line;
        }
        line += 1;
    }
    i
}

/// The units of a brace language.
fn braced(file: &File<'_>) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    // The units still open: (index, last line).
    let mut open: Vec<usize> = Vec::new();
    let test_file = file.path.ends_with("_test.go");
    let mut qml_root_seen = false;
    for i in 0..file.masked.len() {
        let t = file.masked[i].trim();
        if t.is_empty() {
            continue;
        }
        // (Ends are lines from 1; `i` counts from 0.)
        while open.last().is_some_and(|&u| units[u].end <= i) {
            open.pop();
        }
        let parent = open.last().copied();
        let parent_kind = parent.map(|p| units[p].kind);
        let next = file.next_code(i).map_or("", |n| file.masked[n]);
        let found = match file.lang {
            Lang::Rust => rust(t, parent_kind),
            Lang::Go => go(t, file.depth[i] == 0, test_file),
            Lang::Script => {
                let in_class = parent.is_some_and(|p| units[p].kind == Kind::Type && file.depth[i] == file.depth[units[p].start - 1] + 1);
                script(t, parent.is_none() && file.depth[i] == 0, in_class)
            }
            Lang::Swift => swift(t, parent_kind, t.ends_with('{')),
            Lang::C => c_like(t, next, parent_kind),
            Lang::Qml => qml(t),
            _ => None,
        };
        let Some(mut d) = found else { continue };
        let end = extent(file, i);
        if d.kind == Kind::Object {
            // The root object isn't a unit: its members are the file's own.
            if !qml_root_seen && file.depth[i] == 0 {
                qml_root_seen = true;
                continue;
            }
            match object_id(file, i, end) {
                Some(id) => d.name = id,
                None => continue,
            }
        }
        if file.lang == Lang::Rust && rust_test_attr(file, i) {
            d.test = true;
        }
        let public = match d.public {
            Some(p) => p,
            None => match file.lang {
                // QML: the root object's own members are its interface.
                Lang::Qml => file.depth[i] == 1 || parent_kind == Some(Kind::Component),
                _ => parent.is_none_or(|p| units[p].public),
            },
        };
        let qualifier = d.owner.clone().or_else(|| parent.filter(|&p| units[p].kind.qualifies()).map(|p| units[p].name.clone()));
        let sep = if matches!(file.lang, Lang::Rust | Lang::C) { "::" } else { "." };
        let name = qualifier.map_or_else(|| d.name.clone(), |q| format!("{q}{sep}{}", d.name));
        units.push(Unit {
            name,
            kind: d.kind,
            start: i + 1,
            end: end + 1,
            public,
            test: d.test || parent.is_some_and(|p| units[p].test),
            parent,
            hash: String::new(),
            legacy: String::new(),
        });
        open.push(units.len() - 1);
    }
    units
}

/// Whether the attributes above line `i` mark Rust test code.
fn rust_test_attr(file: &File<'_>, i: usize) -> bool {
    file.lines[..i]
        .iter()
        .rev()
        .map(|l| l.trim())
        .take_while(|l| l.starts_with("#[") || l.starts_with("///") || l.starts_with("//"))
        .any(|l| l.starts_with("#[") && l.contains("test"))
}

/// The `id:` a QML object opened on line `i` gives itself.
fn object_id(file: &File<'_>, i: usize, end: usize) -> Option<String> {
    let start = file.offsets[i] + file.masked[i].rfind('{')?;
    let stop = (file.offsets.get(end + 1).copied().unwrap_or(file.masked_text.len())).min(file.masked_text.len());
    let body = &file.masked_text[start + 1..stop];
    let mut depth = 0usize;
    let mut at_start = true;
    for (j, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            ';' | '\n' => at_start = true,
            c if c.is_whitespace() => {}
            _ => {
                if at_start && depth == 0 && body[j..].starts_with("id:") {
                    let name = ident(body[j + 3..].trim_start());
                    return (!name.is_empty()).then(|| name.to_owned());
                }
                at_start = false;
            }
        }
    }
    None
}

/// Python's units: `def` and `class`, their bodies by indentation.
fn indented(file: &File<'_>) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for i in 0..file.masked.len() {
        let t = file.masked[i].trim();
        if t.is_empty() {
            continue;
        }
        while open.last().is_some_and(|&u| units[u].end < i + 1) {
            open.pop();
        }
        let s = t.strip_prefix("async ").unwrap_or(t).trim_start();
        let (kind, rest) = if let Some(r) = s.strip_prefix("def ") {
            (Kind::Function, r)
        } else if let Some(r) = s.strip_prefix("class ") {
            (Kind::Type, r)
        } else {
            continue;
        };
        let name = ident(rest.trim_start());
        if name.is_empty() {
            continue;
        }
        // The header may run over lines (its arguments): it ends where its brackets close.
        let mut parens = 0i32;
        let mut header_end = i;
        for (k, line) in file.masked.iter().enumerate().skip(i) {
            for c in line.bytes() {
                match c {
                    b'(' | b'[' => parens += 1,
                    b')' | b']' => parens -= 1,
                    _ => {}
                }
            }
            header_end = k;
            if parens <= 0 {
                break;
            }
        }
        let indent = file.indent(i);
        let mut end = header_end;
        for k in header_end + 1..file.masked.len() {
            if file.masked[k].trim().is_empty() {
                continue;
            }
            if file.indent(k) <= indent {
                break;
            }
            end = k;
        }
        let parent = open.last().copied();
        let parent_kind = parent.map(|p| units[p].kind);
        let public = !name.starts_with('_') && parent.is_none_or(|p| units[p].public && units[p].kind == Kind::Type);
        let test = name.starts_with("test") || (kind == Kind::Type && name.starts_with("Test")) || parent.is_some_and(|p| units[p].test);
        let qualified = match parent {
            Some(p) if parent_kind == Some(Kind::Type) => format!("{}.{name}", units[p].name),
            _ => name.to_owned(),
        };
        units.push(Unit { name: qualified, kind, start: i + 1, end: end + 1, public, test, parent, hash: String::new(), legacy: String::new() });
        open.push(units.len() - 1);
    }
    units
}

/// Markdown's units: its headings, each to the next one at its level or above.
fn headings(file: &File<'_>) -> Vec<Unit> {
    let mut found: Vec<(usize, usize, String)> = Vec::new();
    for (i, line) in file.masked.iter().enumerate() {
        let t = line.trim_start();
        if line.len() - t.len() >= 4 {
            continue;
        }
        let level = t.bytes().take_while(|&c| c == b'#').count();
        if !(1..=6).contains(&level) || !(t.len() == level || t.as_bytes()[level] == b' ') {
            continue;
        }
        let title = plain_heading(t[level..].trim().trim_end_matches('#').trim());
        if !title.is_empty() {
            found.push((i, level, title));
        }
    }
    let mut units = Vec::new();
    let last_line = last_text_line(file);
    for (n, (i, level, title)) in found.iter().enumerate() {
        let next = found[n + 1..].iter().find(|(_, l, _)| l <= level).map_or(last_line + 1, |(j, _, _)| *j);
        let end = (*i..next).rev().find(|&k| !file.lines[k].trim().is_empty()).unwrap_or(*i);
        units.push(Unit {
            name: title.clone(),
            kind: Kind::Heading,
            start: i + 1,
            end: end + 1,
            public: *level <= 2,
            test: false,
            parent: None,
            hash: String::new(),
            legacy: String::new(),
        });
    }
    units
}

fn last_text_line(file: &File<'_>) -> usize {
    (0..file.lines.len()).rev().find(|&k| !file.lines[k].trim().is_empty()).unwrap_or(0)
}

/// A heading's words: links as their text, emphasis and code marks dropped.
fn plain_heading(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '[' => {
                let words: String = chars.by_ref().take_while(|&c| c != ']').collect();
                if chars.peek() == Some(&'(') {
                    chars.by_ref().take_while(|&c| c != ')').for_each(drop);
                }
                out.push_str(&words);
            }
            '*' | '_' | '`' => {}
            '\\' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// TOML's units: its tables, each to the next one.
fn tables(file: &File<'_>) -> Vec<Unit> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for (i, line) in file.masked.iter().enumerate() {
        let t = line.trim();
        if let Some(inner) = t.strip_prefix('[') {
            let inner = inner.trim_start_matches('[');
            if let Some((name, _)) = inner.split_once(']') {
                found.push((i, name.trim().to_owned()));
            }
        }
    }
    let last_line = last_text_line(file);
    let mut units = Vec::new();
    for (n, (i, name)) in found.iter().enumerate() {
        let next = found.get(n + 1).map_or(last_line + 1, |(j, _)| *j);
        let end = (*i..next).rev().find(|&k| !file.lines[k].trim().is_empty()).unwrap_or(*i);
        units.push(Unit {
            name: name.clone(),
            kind: Kind::Table,
            start: i + 1,
            end: end + 1,
            public: true,
            test: false,
            parent: None,
            hash: String::new(),
            legacy: String::new(),
        });
    }
    units
}

/// Parents fixed after sorting, and each unit's own-text hash.
fn finish(file: &File<'_>, mut units: Vec<Unit>) -> Vec<Unit> {
    // Parents by span (the lists were built in line order, so indices may
    // have moved): the innermost unit holding each one.
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..units.len() {
        while stack.last().is_some_and(|&p| units[p].end < units[i].start || (units[p].end < units[i].end)) {
            stack.pop();
        }
        if file.lang != Lang::Markdown && file.lang != Lang::Toml {
            units[i].parent = stack.last().copied();
        }
        stack.push(i);
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(path: &str, text: &str) -> Vec<String> {
        scan(path, text).iter().map(|u| format!("{} {} {}-{}{}{}", u.kind, u.name, u.start, u.end, if u.public { " pub" } else { "" }, if u.test { " test" } else { "" })).collect()
    }

    #[test]
    fn rust_units() {
        let text = "use std::fmt;\n\npub const LIMIT: usize = 3;\n\n/// A thing.\n#[derive(Debug)]\npub struct Ledger {\n    pub id: u32,\n}\n\nimpl Ledger {\n    pub fn post(&self) -> u32 {\n        let x = \"}\";\n        x\n    }\n\n    pub(crate) async fn sync(&mut self) {}\n    const fn zero() -> u32 { 0 }\n}\n\nimpl<T: Clone> fmt::Display for Ledger<T>\nwhere\n    T: Copy,\n{\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) }\n}\n\nfn helper() {\n    const INNER: u32 = 2;\n}\n\npub trait Store {\n    fn load(&self) -> u32;\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn works() {}\n}\n";
        assert_eq!(
            names("src/lib.rs", text),
            [
                "const LIMIT 3-3 pub",
                "type Ledger 7-9 pub",
                "impl Ledger 11-19",
                "fn Ledger::post 12-15 pub",
                "fn Ledger::sync 17-17 pub",
                "fn Ledger::zero 18-18",
                "impl Ledger 21-26",
                "fn Ledger::fmt 25-25",
                "fn helper 28-30",
                "type Store 32-34 pub",
                "fn Store::load 33-33 pub",
                "mod tests 37-40 test",
                "fn tests::works 39-39 test",
            ]
        );
    }

    #[test]
    fn own_text_hashes() {
        let before = "impl A {\n    fn one() { 1 }\n    fn two() { 2 }\n}\n";
        let after = "impl A {\n        fn one() { 1 } // one\n\n    fn two() { 3 }\n}\n";
        let (b, a) = (scan("a.rs", before), scan("a.rs", after));
        // Re-indenting, a blank line and a plain comment change nothing; `two` changed; its container didn't.
        assert_eq!(b[0].hash, a[0].hash);
        assert_eq!(b[1].hash, a[1].hash);
        assert_ne!(b[2].hash, a[2].hash);
        // The old hash counted the comment; a doc comment counts in both.
        assert_ne!(b[1].legacy, a[1].legacy);
        assert_eq!(b[1].legacy, b[1].hash);
        let doc = scan("a.rs", "impl A {\n    /// One.\n    fn one() { 1 }\n    fn two() { 2 }\n}\n");
        assert_ne!(doc[0].hash, b[0].hash);
    }

    #[test]
    fn go_python_script() {
        let go = "package shop\n\ntype Cart struct {\n\tItems []Item\n}\n\nfunc (c *Cart) Total() int {\n\treturn 0\n}\n\nfunc helper() {}\n\nconst Max = 3\n";
        assert_eq!(names("cart.go", go), ["type Cart 3-5 pub", "fn Cart.Total 7-9 pub", "fn helper 11-11", "const Max 13-13 pub"]);
        let py = "import os\n\nclass Cart:\n    def total(self,\n              tax):\n        return 0\n\n    def _sync(self):\n        pass\n\ndef main():\n    x = Cart()\n";
        assert_eq!(names("shop.py", py), ["type Cart 3-9 pub", "fn Cart.total 4-6 pub", "fn Cart._sync 8-9", "fn main 11-12 pub"]);
        let ts = "import x from 'y';\n\nexport interface Props {\n  a: string;\n}\n\nexport const handler = async (req) => {\n  if (req) {\n    go();\n  }\n};\n\nexport default class View {\n  render() {\n    return null;\n  }\n  private static async load(id: string) {\n  }\n}\nfunction local() {}\n";
        assert_eq!(
            names("view.tsx", ts),
            ["type Props 3-5 pub", "fn handler 7-11 pub", "type View 13-19 pub", "fn View.render 14-16 pub", "fn View.load 17-18", "fn local 20-20"]
        );
    }

    #[test]
    fn swift_and_c() {
        let swift = "import SwiftUI\n\n@MainActor\nstruct WorkflowSheet: View {\n    let run: Run\n    var body: some View {\n        Text(\"\\(run.name) {\")\n    }\n    private func open(_ key: String) {\n    }\n}\n\nextension Run {\n    static func make() -> Run { Run() }\n}\n";
        assert_eq!(
            names("Sheet.swift", swift),
            ["type WorkflowSheet 4-11 pub", "property WorkflowSheet.body 6-8 pub", "fn WorkflowSheet.open 9-10", "impl Run 13-15", "fn Run.make 14-14"]
        );
        let c = "#include <stdio.h>\n\nstruct point {\n    int x;\n};\n\nstatic int add(int a, int b)\n{\n    if (a) {\n        return a + b;\n    }\n    return b;\n}\n\nint main(void) {\n    return add(1, 2);\n}\n";
        assert_eq!(names("m.c", c), ["type point 3-5 pub", "fn add 7-13", "fn main 15-17 pub"]);
    }

    #[test]
    fn qml_units() {
        let qml = "import QtQuick\n\nItem {\n    id: root\n    property var run: null\n    readonly property list<string> keys: []\n    signal opened(string key)\n\n    function stageColor(stage) {\n        return \"{\";\n    }\n\n    Rectangle {\n        width: 3\n    }\n\n    FlatButton {\n        id: openAsking\n        onClicked: root.opened(\"x\")\n        function press() {}\n    }\n\n    component Badge: Text {\n        property int count: 0\n    }\n}\n";
        assert_eq!(
            names("WorkflowCard.qml", qml),
            [
                "property run 5-5 pub",
                "property keys 6-6 pub",
                "signal opened 7-7 pub",
                "fn stageColor 9-11 pub",
                "id openAsking 17-21",
                "fn openAsking.press 20-20",
                "component Badge 23-25 pub",
                "property Badge.count 24-24 pub",
            ]
        );
    }

    #[test]
    fn markdown_and_toml() {
        let md = "# Title\n\nText.\n\n```sh\n# not a heading\n```\n\n## Part [one](x.md)\n\nWords.\n\n### Detail\n\nMore.\n\n## Next\n";
        assert_eq!(names("README.md", md), ["section Title 1-17 pub", "section Part one 9-15 pub", "section Detail 13-15", "section Next 17-17 pub"]);
        let toml = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n\n[[bin]]\nname = \"y\"\n";
        assert_eq!(names("Cargo.toml", toml), ["table package 1-2 pub", "table dependencies 4-5 pub", "table bin 7-8 pub"]);
    }

    #[test]
    fn unknown_files_have_no_units() {
        assert!(scan("data.csv", "a,b\n").is_empty());
        assert_eq!(lang("x/y.TSX"), Some(Lang::Script));
        assert_eq!(digest(b"").len(), 16);
    }
}
