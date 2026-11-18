//! Reading one file: its syntax tree walked into symbols, imports and calls.
//!
//! The walk is the same for every language; what differs is in
//! [`rules`](crate::rules) (comments, imports, calls, test files) and in
//! [`defs`](crate::defs) (what declares what, under which name, and whether
//! it's public). YAML and SQL are read as text ([`text`](crate::text)).

// Rust guideline compliant 2026-10-09

use crate::rules::{self, Callee, is_comment};
use crate::{Call, FileIndex, Import, Kind, Lang, Symbol, lang, text};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

/// The longest signature kept, in characters; longer ones end in `…`.
///
/// Long enough for a typical declaration with its parameters and return
/// type; a signature longer than this is usually a parameter list a reader
/// would open the code for anyway.
pub(crate) const MAX_SIGNATURE: usize = 240;

/// The longest doc line kept, in characters.
const MAX_DOC: usize = 200;

/// The longest call receiver kept, in characters (`self.orders.ledger`).
const MAX_RECEIVER: usize = 80;

/// The symbols, imports and calls of `text`, the contents of the file at `path`.
///
/// Returns `None` for a language the index doesn't read. Code the grammar
/// can't parse still gives what it can: tree-sitter recovers around errors.
#[must_use]
pub fn scan(path: &str, text: &str) -> Option<FileIndex> {
    scan_with(&mut Parser::new(), path, text)
}

/// The syntax tree of `text` as an S-expression, for writing a language's rules.
///
/// Returns `None` for a language read as text, or one the index doesn't read.
#[must_use]
pub fn syntax_tree(path: &str, text: &str) -> Option<String> {
    let mut parser = Parser::new();
    parser.set_language(&rules::grammar(lang(path)?)?).ok()?;
    Some(parser.parse(text, None)?.root_node().to_sexp())
}

/// [`scan`] with a parser to reuse across files.
pub(crate) fn scan_with(parser: &mut Parser, path: &str, source: &str) -> Option<FileIndex> {
    let lang = lang(path)?;
    let Some(grammar) = rules::grammar(lang) else { return text::scan(lang, source) };
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(rules::prepare(lang, source).as_bytes(), None)?;
    let mut walk = Walk {
        lang,
        src: source.as_bytes(),
        test_file: rules::test_file(lang, path),
        symbols: Vec::new(),
        imports: Vec::new(),
        calls: Vec::new(),
        plain_comments: Vec::new(),
        locals: HashSet::new(),
    };
    walk.run(tree.root_node());
    let hashes = own_hashes(source, &walk.plain_comments, &walk.symbols);
    for (s, h) in walk.symbols.iter_mut().zip(hashes) {
        s.hash = h;
    }
    Some(FileIndex { lang, symbols: walk.symbols, imports: walk.imports, calls: walk.calls })
}

/// A declaration a node makes, before it's placed among the others.
pub(crate) struct Def<'t> {
    pub kind: Kind,
    pub name: String,
    /// The node its lines, signature and doc start from (an arrow function's
    /// `const`, an exported declaration's `export`).
    pub outer: Node<'t>,
    /// The node its signature ends before.
    pub body: Option<Node<'t>>,
    /// Its own visibility, before what its container says.
    pub public: bool,
    pub test: bool,
    /// Its signature, where it isn't its text up to its body (a C `typedef`).
    pub signature: Option<String>,
}

/// Where the walk is: the symbol it's in, and whether that's inside a function.
#[derive(Clone, Copy, Default)]
struct Ctx {
    sym: Option<usize>,
    in_fn: bool,
}

pub(crate) struct Walk<'s> {
    pub lang: Lang,
    pub src: &'s [u8],
    test_file: bool,
    symbols: Vec<Symbol>,
    imports: Vec<Import>,
    calls: Vec<Call>,
    /// Byte ranges of comments that aren't doc comments (left out of hashes).
    plain_comments: Vec<(usize, usize)>,
    /// Names bound locally (variables, parameters), by the symbol they're
    /// in: a call to one calls a closure or a function value, not a declaration.
    locals: HashSet<(Option<usize>, String)>,
}

impl Walk<'_> {
    pub(crate) fn text(&self, node: Node<'_>) -> &str {
        rules::text(node, self.src)
    }

    /// Visits every node in order, without recursion (a deep tree can't
    /// overflow the stack).
    fn run(&mut self, root: Node<'_>) {
        let mut stack = vec![(root, Ctx::default())];
        let mut cursor = root.walk();
        let mut kids = Vec::new();
        while let Some((node, ctx)) = stack.pop() {
            let Some(inner) = self.visit(node, ctx) else { continue };
            kids.clear();
            kids.extend(node.children(&mut cursor));
            stack.extend(kids.iter().rev().map(|&k| (k, inner)));
        }
    }

    /// Takes in what `node` is; the context for its children, or `None` when
    /// they hold nothing more.
    fn visit(&mut self, node: Node<'_>, ctx: Ctx) -> Option<Ctx> {
        if is_comment(self.lang, node.kind()) {
            if !self.doc_comment(node) {
                self.plain_comments.push((node.start_byte(), node.end_byte()));
            }
            return None;
        }
        if rules::is_import(self.lang, node, self.src) {
            self.import(node);
            return None;
        }
        if let Some(callee) = rules::callee(self.lang, node) {
            self.call(&callee, ctx);
            return Some(ctx);
        }
        if self.lang == Lang::Rust && node.kind() == "token_tree" {
            self.token_calls(node, ctx);
            return Some(ctx);
        }
        self.bind(node, ctx);
        if ctx.in_fn && !rules::keeps_nested(self.lang) {
            return Some(ctx);
        }
        let Some(def) = self.def(node) else { return Some(ctx) };
        if ctx.in_fn && def.kind == Kind::Constant {
            return Some(ctx);
        }
        let at = self.add(def, ctx.sym);
        Some(Ctx { sym: Some(at), in_fn: self.symbols[at].kind == Kind::Function })
    }

    /// The calls in a Rust macro's arguments, which the grammar leaves as
    /// tokens: a path (`a::b`, `x.y`) right before `(…)` is a call, and
    /// before `!` and a group, a macro's. (Nested groups are visited apart.)
    fn token_calls(&mut self, tree: Node<'_>, ctx: Ctx) {
        let mut cursor = tree.walk();
        let kids: Vec<Node<'_>> = tree.children(&mut cursor).collect();
        let mut path: Vec<Node<'_>> = Vec::new();
        let mut bang = false;
        for (i, k) in kids.iter().enumerate() {
            match k.kind() {
                "identifier" | "self" | "crate" | "super" if path.last().is_none_or(|p| matches!(p.kind(), "::" | ".")) && !bang => path.push(*k),
                "::" | "." if path.last().is_some_and(|p| !matches!(p.kind(), "::" | ".")) && !bang => path.push(*k),
                "!" if path.last().is_some_and(|p| p.kind() == "identifier") => bang = true,
                "token_tree" if path.last().is_some_and(|p| p.kind() == "identifier") && self.text(*k).starts_with(['(', '[', '{']) => {
                    // A group after a plain path is a call only in parentheses.
                    if bang || self.text(*k).starts_with('(') {
                        let preceded = i > path.len() + usize::from(bang)
                            && kids.get(i - path.len() - usize::from(bang) - 1).is_some_and(|p| matches!(p.kind(), "." | "::"));
                        if !preceded {
                            self.token_call(&path, bang, ctx);
                        }
                    }
                    path.clear();
                    bang = false;
                }
                _ => {
                    path.clear();
                    bang = false;
                }
            }
        }
    }

    /// Records a call found among a macro's tokens.
    fn token_call(&mut self, path: &[Node<'_>], bang: bool, ctx: Ctx) {
        let Some(last) = path.last() else { return };
        let text: String = path.iter().map(|n| self.text(*n)).collect();
        let Some((name, receiver)) = split_callee(&text) else { return };
        if receiver.is_none() && !bang && self.locals.contains(&(ctx.sym, name.clone())) {
            return;
        }
        let name = if bang { format!("{name}!") } else { name };
        self.calls.push(Call { name, receiver, line: last.start_position().row + 1, column: last.start_position().column, from: ctx.sym });
    }

    /// Notes the names `node` binds, if it binds any: a variable, a parameter.
    fn bind(&mut self, node: Node<'_>, ctx: Ctx) {
        if !matches!(self.lang, Lang::Rust | Lang::Python | Lang::JavaScript | Lang::TypeScript | Lang::Tsx | Lang::Go) {
            return;
        }
        let mut cursor = node.walk();
        let named: Vec<Node<'_>> = match node.kind() {
            "let_declaration" | "parameter" | "required_parameter" | "optional_parameter" => node.child_by_field_name("pattern").into_iter().collect(),
            "assignment" | "short_var_declaration" => node.child_by_field_name("left").into_iter().collect(),
            "variable_declarator" | "default_parameter" | "typed_default_parameter" | "parameter_declaration" | "var_spec" => {
                node.children_by_field_name("name", &mut cursor).collect()
            }
            "parameters" | "formal_parameters" | "closure_parameters" | "typed_parameter" | "lambda_parameters" => vec![node],
            _ => return,
        };
        for n in named {
            // An identifier, or a list of them (`a, b := …`, parameters).
            let ids: Vec<Node<'_>> = if n.kind() == "identifier" {
                vec![n]
            } else if n.id() == node.id() || n.kind() == "expression_list" || n.kind() == "pattern_list" {
                let mut c = n.walk();
                n.named_children(&mut c).filter(|k| k.kind() == "identifier").collect()
            } else {
                Vec::new()
            };
            for id in ids {
                let name = self.text(id).to_owned();
                self.locals.insert((ctx.sym, name));
            }
        }
    }

    /// Places `def` among the symbols, inside the one at `parent`.
    fn add(&mut self, def: Def<'_>, parent: Option<usize>) -> usize {
        let (signature, doc) = (self.signature(&def), self.doc(&def));
        let container = parent.map(|p| &self.symbols[p]);
        let name = match container {
            Some(c) if c.kind.qualifies() => format!("{}{}{}", c.name, self.lang.separator(), def.name),
            _ => def.name,
        };
        let public = match (self.lang, container) {
            // A trait's items are as visible as the trait.
            (Lang::Rust, Some(c)) if c.kind == Kind::Type && !def.public => c.public,
            // Python's: a class's members are as visible as the class; a function's are its own.
            (Lang::Python, Some(c)) => def.public && c.public && c.kind == Kind::Type,
            (Lang::Rust | Lang::Go | Lang::C | Lang::Bash | Lang::Lua | Lang::Julia, _) | (_, None) => def.public,
            // Elsewhere a member is no more visible than what holds it.
            (_, Some(c)) => def.public && (c.public || !c.kind.qualifies()),
        };
        let test = def.test || self.test_file || container.is_some_and(|c| c.test);
        let (start, end) = lines(def.outer);
        let symbol = Symbol { name, kind: def.kind, start, end, signature, doc, public, test, parent, hash: String::new() };
        self.symbols.push(symbol);
        self.symbols.len() - 1
    }

    /// A declaration's text up to its body, on one line.
    fn signature(&self, def: &Def<'_>) -> String {
        if let Some(sig) = &def.signature {
            return clip(&tidy(sig), MAX_SIGNATURE);
        }
        let start = def.outer.start_byte();
        let src = self.text(def.outer);
        let end = match def.body {
            Some(b) if b.start_byte() > start => (b.start_byte() - start).min(src.len()),
            // No body: its first line, without an opening brace.
            _ => src.find('\n').unwrap_or(src.len()),
        };
        // Without the comments in it (`def f(): # note`).
        let mut head = String::with_capacity(end);
        let mut at = 0;
        for (a, b) in self.comments_in(def.outer, start + end) {
            head.push_str(src.get(at..a - start).unwrap_or(""));
            head.push(' ');
            at = b - start;
        }
        head.push_str(src.get(at.min(end)..end).unwrap_or(""));
        let head = head.trim_end().trim_end_matches('{').trim_end();
        let head = match self.lang {
            Lang::Python => head.trim_end_matches(':'),
            Lang::Scala | Lang::Kotlin | Lang::Julia => head.trim_end_matches('=').trim_end(),
            _ => head,
        };
        let head = match self.lang {
            Lang::Java | Lang::Kotlin | Lang::Scala | Lang::Swift | Lang::CSharp | Lang::Php => strip_attributes(head),
            _ => head,
        };
        let mut one = tidy(head);
        if self.lang == Lang::Go && matches!(def.outer.kind(), "type_spec" | "type_alias") {
            one.insert_str(0, "type ");
        }
        clip(&one, MAX_SIGNATURE)
    }

    /// The byte ranges of the comments in `node` that start before `end`, in order.
    fn comments_in(&self, node: Node<'_>, end: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut stack = vec![node];
        let mut cursor = node.walk();
        while let Some(n) = stack.pop() {
            if n.start_byte() >= end {
                continue;
            }
            if is_comment(self.lang, n.kind()) {
                out.push((n.start_byte(), n.end_byte().min(end)));
                continue;
            }
            stack.extend(n.children(&mut cursor).collect::<Vec<_>>().into_iter().rev());
        }
        out.sort_unstable();
        out
    }

    /// The first line of a declaration's doc comment or docstring.
    fn doc(&self, def: &Def<'_>) -> Option<String> {
        if self.lang == Lang::Python {
            let first = def.body?.named_child(0)?;
            let string = first.named_child(0).filter(|s| first.kind() == "expression_statement" && s.kind() == "string")?;
            return first_line(unquote(self.text(string)));
        }
        let anchor = self.doc_anchor(def.outer);
        // Julia's docstring: a string right above the definition.
        if self.lang == Lang::Julia
            && let Some(p) = anchor.prev_named_sibling().filter(|p| p.kind() == "string_literal" && p.end_position().row + 1 >= anchor.start_position().row)
        {
            return first_line(unquote(self.text(p)));
        }
        // The doc comments right above it (attributes may sit between them).
        let mut docs = Vec::new();
        let mut prev = anchor.prev_sibling();
        let mut row = anchor.start_position().row;
        while let Some(p) = prev {
            if p.kind() == "attribute_item" {
                row = p.start_position().row;
            } else if is_comment(self.lang, p.kind()) && p.end_position().row + 1 >= row && self.doc_comment(p) {
                docs.push(self.text(p));
                row = p.start_position().row;
            } else {
                break;
            }
            prev = p.prev_sibling();
        }
        docs.reverse();
        let csharp = self.lang == Lang::CSharp;
        docs.iter().find_map(|d| first_line(&if csharp { strip_xml(&strip_comment(d)) } else { strip_comment(d) }))
    }

    /// The node whose preceding comments document a declaration made by `outer`.
    fn doc_anchor<'t>(&self, outer: Node<'t>) -> Node<'t> {
        let wrapper = match self.lang {
            // A Go spec's doc sits above its `type` or `const` keyword.
            Lang::Go => ["type_declaration", "const_declaration"].as_slice(),
            Lang::Cpp => &["template_declaration"],
            Lang::C => &["declaration"],
            Lang::Julia => &["macrocall_expression"],
            _ => &[],
        };
        outer.parent().filter(|p| wrapper.contains(&p.kind())).unwrap_or(outer)
    }

    /// Whether a comment documents what follows it.
    fn doc_comment(&self, node: Node<'_>) -> bool {
        if let Some(by_text) = rules::doc_marker(self.lang, self.text(node)) {
            return by_text;
        }
        // Any comment right above a declaration documents it.
        let mut end = node.end_position().row;
        let mut next = node.next_sibling();
        while let Some(n) = next.filter(|n| is_comment(self.lang, n.kind()) && n.start_position().row <= end + 1) {
            end = n.end_position().row;
            next = n.next_sibling();
        }
        next.is_some_and(|n| n.start_position().row <= end + 1 && (self.def(n).is_some() || self.wraps_def(n)))
    }

    /// Whether `node` holds a declaration as its first named child (Go's
    /// `type` and `const` blocks).
    fn wraps_def(&self, node: Node<'_>) -> bool {
        matches!(node.kind(), "type_declaration" | "const_declaration" | "var_declaration")
            && node.named_child(0).is_some_and(|c| self.def(c).is_some() || c.kind() == "var_spec")
    }

    fn import(&mut self, node: Node<'_>) {
        if self.lang == Lang::Go {
            // One import per spec: `import ("fmt"; f "flag")` is two.
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                if n.kind() == "import_spec" {
                    self.imports.push(Import { text: format!("import {}", squash(self.text(n))), line: n.start_position().row + 1 });
                } else {
                    let mut cursor = n.walk();
                    stack.extend(n.named_children(&mut cursor).collect::<Vec<_>>().into_iter().rev());
                }
            }
            return;
        }
        let text = squash(self.text(node));
        self.imports.push(Import { text: text.trim_end_matches(';').trim_end().to_owned(), line: node.start_position().row + 1 });
    }

    fn call(&mut self, callee: &Callee<'_>, ctx: Ctx) {
        let (parts, at) = match *callee {
            Callee::Macro(path) => {
                let text = self.text(path);
                let parts = split_callee(text).map(|(name, receiver)| (format!("{name}!"), receiver));
                let at = parts
                    .as_ref()
                    .map_or((path.start_position().row, path.start_position().column), |(name, _)| name_at(path, text, name.trim_end_matches('!')));
                (parts, at)
            }
            Callee::Path(path) => {
                let text = self.text(path);
                // Lua's `obj:method`: a method call.
                let parts = if self.lang == Lang::Lua { split_callee(&text.replace(':', ".")) } else { split_callee(text) };
                let at = parts.as_ref().map_or((path.start_position().row, path.start_position().column), |(name, _)| name_at(path, text, name));
                (parts, at)
            }
            Callee::Parts(name, receiver) => {
                let text = self.text(name).trim();
                let parts = is_ident(text).then(|| (text.to_owned(), receiver.map(|r| short_receiver(self.text(r))).filter(|r| !r.is_empty())));
                (parts, (name.start_position().row, name.start_position().column))
            }
        };
        let Some((name, receiver)) = parts else { return };
        if receiver.is_none() && self.locals.contains(&(ctx.sym, name.clone())) {
            return;
        }
        self.calls.push(Call { name, receiver, line: at.0 + 1, column: at.1, from: ctx.sym });
    }
}

/// Where `name` starts in `path` (whose text is `text`): its row and byte column, from 0.
fn name_at(path: Node<'_>, text: &str, name: &str) -> (usize, usize) {
    let start = (path.start_position().row, path.start_position().column);
    let Some(at) = text.rfind(name) else { return start };
    let before = &text[..at];
    match before.rfind('\n') {
        Some(nl) => (start.0 + before.matches('\n').count(), at - nl - 1),
        None => (start.0, start.1 + at),
    }
}

/// Each symbol's hash, from the lines it owns (see the crate docs).
///
/// `plain_comments` are byte ranges left out; `symbols` are in order of their
/// first lines, a container before what it holds.
pub(crate) fn own_hashes(source: &str, plain_comments: &[(usize, usize)], symbols: &[Symbol]) -> Vec<String> {
    let mut text = source.as_bytes().to_vec();
    for &(a, b) in plain_comments {
        for byte in &mut text[a.min(source.len())..b.min(source.len())] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    // Blanking whole comments (whole characters) keeps the text UTF-8.
    let text = String::from_utf8_lossy(&text);
    let lines: Vec<&str> = text.split('\n').collect();
    // Each line belongs to the innermost symbol holding it.
    let mut owner: Vec<Option<usize>> = vec![None; lines.len()];
    for (i, s) in symbols.iter().enumerate() {
        for slot in owner.iter_mut().take(s.end.min(lines.len())).skip(s.start - 1) {
            *slot = Some(i);
        }
    }
    let mut own = vec![String::new(); symbols.len()];
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

/// A short hash of `text` (16 hex digits of its SHA-256), as harlo's wiki uses.
pub(crate) fn digest(text: &[u8]) -> String {
    let sum = Sha256::digest(text);
    sum[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn is_ident(name: &str) -> bool {
    !name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit()) && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '!' | '?'))
}

/// A callee's name and what it's called on (`self.a.post` → `post`, `self.a`).
fn split_callee(text: &str) -> Option<(String, Option<String>)> {
    let text = strip_type_args(text.trim());
    let cut = [text.rfind("::").map(|i| (i, 2)), text.rfind('.').map(|i| (i, 1)), text.rfind("->").map(|i| (i, 2)), text.rfind('\\').map(|i| (i, 1))]
        .into_iter()
        .flatten()
        .max_by_key(|&(i, _)| i);
    let (name, receiver) = match cut {
        Some((i, w)) => (&text[i + w..], Some(text[..i].trim_end_matches('?').trim())),
        None => (text, None),
    };
    let name = name.trim().trim_start_matches(['$', '@', '&']);
    if !is_ident(name) {
        return None;
    }
    let receiver = receiver.map(short_receiver).filter(|r| !r.is_empty());
    Some((name.to_owned(), receiver))
}

/// `text` without type arguments at its end (`List<String>` → `List`).
fn strip_type_args(text: &str) -> &str {
    if !text.ends_with('>') {
        return text;
    }
    let mut depth = 0usize;
    for (i, c) in text.char_indices().rev() {
        match c {
            '>' => depth += 1,
            '<' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return text[..i].trim_end();
                }
            }
            _ => {}
        }
    }
    text
}

/// A receiver on one line, its end kept when it's long.
fn short_receiver(r: &str) -> String {
    let r = squash(r);
    if r.chars().count() > MAX_RECEIVER {
        let tail: String = r.chars().rev().take(MAX_RECEIVER - 1).collect::<Vec<_>>().into_iter().rev().collect();
        format!("…{tail}")
    } else {
        r
    }
}

/// A node's first and last lines, from 1.
pub(crate) fn lines(node: Node<'_>) -> (usize, usize) {
    let start = node.start_position().row + 1;
    let end_pos = node.end_position();
    // A node ending at a line's start ends on the line before.
    let end = if end_pos.column == 0 && end_pos.row + 1 > start { end_pos.row } else { end_pos.row + 1 };
    (start, end.max(start))
}

/// `s` with each run of whitespace made one space.
pub(crate) fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A signature on one line, multi-line parameter lists closed up: `(a: u32, b: u32)`.
pub(crate) fn tidy(s: &str) -> String {
    squash(s).replace("( ", "(").replace(" )", ")").replace(",)", ")").replace("[ ", "[").replace(" ]", "]").replace(",]", "]")
}

/// `s` cut to `max` characters, ending in `…` when cut.
pub(crate) fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// A declaration's text without the attributes and annotations it starts
/// with (`@Test`, `@GetMapping("/a")`, `[Fact]`, `#[Route]`).
fn strip_attributes(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        let rest = if let Some(r) = s.strip_prefix('@') {
            let end = r.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).unwrap_or(r.len());
            let r = &r[end..];
            if r.starts_with('(') { after_group(r, '(', ')') } else { Some(r) }
        } else if s.starts_with('[') || s.starts_with("#[") {
            after_group(s.trim_start_matches('#'), '[', ']')
        } else {
            None
        };
        match rest {
            Some(r) if r.len() < s.len() => s = r,
            _ => return s,
        }
    }
}

/// `s` after the group it opens with `open` and closes with `close`, nesting counted.
fn after_group(s: &str, open: char, close: char) -> Option<&str> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(&s[i + 1..]);
            }
        }
    }
    None
}

/// A C# doc comment's text without its XML tags (`<summary>`).
fn strip_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let tag = &rest[open + 1..];
        let name_len = tag.trim_start_matches('/').find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(0);
        match tag.find('>') {
            Some(close) if name_len > 0 => rest = &tag[close + 1..],
            _ => {
                out.push('<');
                rest = tag;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A string literal's text without its quotes and prefixes (`r"""…"""`).
fn unquote(text: &str) -> &str {
    text.trim_start_matches(['r', 'u', 'R', 'U', 'b', 'f', 'F']).trim_matches(['"', '\''])
}

/// A comment's text without its markers.
pub(crate) fn strip_comment(text: &str) -> String {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        let t = t.trim_start_matches("/**").trim_start_matches("/*!").trim_start_matches("/*");
        let t = t.trim_end_matches("*/");
        let t = t.trim_start_matches("//!").trim_start_matches("///").trim_start_matches("//");
        let t = t.trim_start_matches("--").trim_start_matches('-');
        let t = t.trim_start_matches('*').trim_start_matches('#');
        out.push(t.trim().to_owned());
    }
    out.join("\n")
}

/// The first non-empty line of `text`, clipped.
pub(crate) fn first_line(text: &str) -> Option<String> {
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| clip(l, MAX_DOC))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callees() {
        let split = |t: &str| split_callee(t).map(|(n, r)| format!("{}{n}", r.map(|r| format!("{r} → ")).unwrap_or_default()));
        assert_eq!(split("self.ledger.post").as_deref(), Some("self.ledger → post"));
        assert_eq!(split("Ledger::new").as_deref(), Some("Ledger → new"));
        assert_eq!(split("List<String>").as_deref(), Some("List"));
        assert_eq!(split("a?.b").as_deref(), Some("a → b"));
        assert_eq!(split("\\App\\make").as_deref(), Some("\\App → make"));
        assert_eq!(split("f(a.b)()"), None);
        assert_eq!(split(&format!("{}.x", "a".repeat(100))).unwrap().chars().count(), MAX_RECEIVER + 4);
    }

    #[test]
    fn hashes_skip_plain_comments() {
        let s = |start, end| Symbol {
            name: String::new(),
            kind: Kind::Function,
            start,
            end,
            signature: String::new(),
            doc: None,
            public: false,
            test: false,
            parent: None,
            hash: String::new(),
        };
        let a = own_hashes("fn a() {\n  1 // x\n}\n", &[(13, 17)], &[s(1, 3)]);
        let b = own_hashes("fn a() {\n    1\n}\n", &[], &[s(1, 3)]);
        assert_eq!(a, b);
        assert_eq!(digest(b"").len(), 16);
        assert_eq!(tidy("f(\n  a,\n  b,\n)"), "f(a, b)");
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(strip_attributes("@Test @GetMapping(\"/a(b)\") public void f()"), "public void f()");
        assert_eq!(strip_attributes("[Fact] [Theory(X = 1)] public void F()"), "public void F()");
        assert_eq!(strip_attributes("#[Route('/')] public function a()"), "public function a()");
        assert_eq!(strip_xml("<summary>A cart.</summary> a < b"), "A cart. a < b");
    }
}
