//! The call graph: which declarations each call reaches, from names alone.
//!
//! A call is matched with the repository's callable declarations of its
//! name (functions; types, called as constructors; macros, for `name!`),
//! then narrowed in steps, the most telling first. The first step that
//! leaves any candidates decides: one candidate is a *sure* call, more are
//! *possible* calls. A name nothing in the repository declares is a
//! library's, and isn't in the graph.
//!
//! A call on something (`x.f()`, `X::f()`):
//! 1. on `self`, `this`, `Self` (or Go's receiver variable): the caller's own type;
//! 2. on a name that is a type's, or reads as one (`Ledger::open`,
//!    `self.ledger.post`, `order_book.add` for `OrderBook`): that type's,
//!    sure only where the type is near (declared in the file, imported, in
//!    its folder); a path names the module too (`reqwest::Client::new`
//!    reaches only a `Client` in a module named `reqwest`);
//! 3. on a module's name (`ledger::post`, `service.place`): the
//!    declarations in a file or folder of that name;
//! 4. a type or module the repository doesn't declare (written as a path,
//!    capitalised, or imported): the library's, left out;
//! 5. otherwise (a variable, a field): the methods of types whose names
//!    hold its word (`handle.send` and `SessionHandle::send`), each possible;
//!    none when no type's name does.
//!
//! A bare call (`f()`):
//! 1. in languages with an implied `this` (Java, C#, C++, Kotlin, Swift, …):
//!    the caller's own type;
//! 2. the caller's file;
//! 3. a function the file imports: by name, or with its whole module
//!    (`use ledger::*`), the import naming its file or folder;
//! 4. the caller's folder, in languages whose packages are folders or whose
//!    names are module-wide (Go, C, C++, Swift, Java, …);
//! 5. anywhere in the repository, in languages with global names (C, Bash,
//!    Lua, PHP, Ruby, …).
//!
//! Elsewhere (Rust, Python, JavaScript) a bare call to nothing in reach is
//! a library's or a built-in's, and left out.
//!
//! With the repository's packages ([`Graph::with_packages`]), a call
//! reaches only its own package and those its package depends on.
//!
//! Outside languages with an implied `this`, a bare call reaches only free
//! functions and types, not methods. A call to a name bound in the caller
//! (a closure in a variable, a parameter) isn't recorded at all, and test
//! code is reachable only from tests.

// Rust guideline compliant 2026-10-09

use crate::{Call, FileIndex, Kind, Lang, Packages, Symbol};
use std::collections::{BTreeMap, HashMap};

/// More candidates than this make a call too ambiguous to be worth listing.
const MAX_POSSIBLE: usize = 8;

/// A declaration in the graph: its file's position in [`Graph::files`] and
/// its own in that file's symbols.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ref {
    pub file: usize,
    pub symbol: usize,
}

/// A call site: its file's position in [`Graph::files`] and its own in that file's calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Site {
    pub file: usize,
    pub call: usize,
}

/// One call reaching one declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Edge {
    pub site: Site,
    pub to: Ref,
    /// The only declaration the call can reach (else one of several).
    pub sure: bool,
}

/// A repository's call graph, resolved on demand.
#[derive(Debug)]
pub struct Graph<'a> {
    files: Vec<(&'a str, &'a FileIndex)>,
    /// Callable declarations by their short names and packages.
    callable: HashMap<(&'a str, Option<usize>), Vec<Ref>>,
    /// Methods by their short names and their types' (normalized, see [`norm`]).
    typed: HashMap<(&'a str, String), Vec<Ref>>,
    /// The packages declaring each callable name.
    declared_in: HashMap<&'a str, Vec<Option<usize>>>,
    /// Each file's package.
    package_of: Vec<Option<usize>>,
    /// Call sites by the names they call.
    calls: HashMap<&'a str, Vec<Site>>,
    /// The repository's packages, when its manifests are known.
    packages: Packages,
}

impl<'a> Graph<'a> {
    /// The graph of the files of an index (path → file).
    #[must_use]
    pub fn new(files: &'a BTreeMap<String, FileIndex>) -> Graph<'a> {
        let files: Vec<(&str, &FileIndex)> = files.iter().map(|(p, f)| (p.as_str(), f)).collect();
        let mut calls: HashMap<&str, Vec<Site>> = HashMap::new();
        for (file, (_, f)) in files.iter().enumerate() {
            for (call, c) in f.calls.iter().enumerate() {
                calls.entry(c.name.trim_end_matches('!')).or_default().push(Site { file, call });
            }
        }
        let package_of = vec![None; files.len()];
        let mut graph =
            Graph { files, callable: HashMap::new(), typed: HashMap::new(), declared_in: HashMap::new(), package_of, calls, packages: Packages::default() };
        graph.sort_callable();
        graph
    }

    /// Files callable declarations by name and package.
    fn sort_callable(&mut self) {
        self.callable.clear();
        self.declared_in.clear();
        self.typed.clear();
        for (file, (_, f)) in self.files.iter().enumerate() {
            let package = self.package_of[file];
            for (symbol, s) in f.symbols.iter().enumerate() {
                if matches!(s.kind, Kind::Function | Kind::Type | Kind::Macro | Kind::Component) {
                    let refs = self.callable.entry((s.short(), package)).or_default();
                    if refs.is_empty() {
                        self.declared_in.entry(s.short()).or_default().push(package);
                    }
                    refs.push(Ref { file, symbol });
                }
            }
        }
        let methods: Vec<(&'a str, String, Ref)> =
            self.callable.values().flatten().filter_map(|&r| self.type_of(r).map(|t| (self.symbol(r).short(), norm(t), r))).collect();
        for (name, t, r) in methods {
            self.typed.entry((name, t)).or_default().push(r);
        }
    }

    /// The callable declarations named `name` that code in `file` can reach.
    fn reachable(&self, file: usize, name: &str) -> Vec<Ref> {
        let from = self.package_of[file];
        let Some(packages) = self.declared_in.get(name) else { return Vec::new() };
        packages
            .iter()
            .filter(|&&p| self.packages.reaches_package(from, p))
            .flat_map(|&p| self.callable.get(&(name, p)).into_iter().flatten().copied())
            .collect()
    }

    /// The graph with the repository's packages: a call reaches only
    /// declarations in its own package or one its package depends on.
    #[must_use]
    pub fn with_packages(mut self, packages: Packages) -> Graph<'a> {
        self.package_of = self.files.iter().map(|(p, _)| packages.package(p)).collect();
        self.packages = packages;
        self.sort_callable();
        self
    }

    /// The files, in path order: [`Ref::file`] and [`Site::file`] index them.
    #[must_use]
    pub fn files(&self) -> &[(&'a str, &'a FileIndex)] {
        &self.files
    }

    /// The path of a declaration's file.
    #[must_use]
    pub fn path(&self, r: Ref) -> &'a str {
        self.files[r.file].0
    }

    /// A declaration.
    #[must_use]
    pub fn symbol(&self, r: Ref) -> &'a Symbol {
        &self.files[r.file].1.symbols[r.symbol]
    }

    /// A call site's file path and call.
    #[must_use]
    pub fn site(&self, s: Site) -> (&'a str, &'a Call) {
        let (path, f) = self.files[s.file];
        (path, &f.calls[s.call])
    }

    /// The declarations named `name`: its full name (`Ledger::post`), else
    /// its short one (`post`); `path` (a file, or a folder ending in `/`)
    /// narrows them.
    #[must_use]
    pub fn find(&self, name: &str, path: Option<&str>) -> Vec<Ref> {
        let within = |r: &Ref| path.is_none_or(|p| self.path(*r) == p || (p.ends_with('/') && self.path(*r).starts_with(p)));
        let mut out = Vec::new();
        for (file, (_, f)) in self.files.iter().enumerate() {
            for (symbol, s) in f.symbols.iter().enumerate() {
                if s.name == name {
                    out.push(Ref { file, symbol });
                }
            }
        }
        out.retain(within);
        if out.is_empty() {
            for (file, (_, f)) in self.files.iter().enumerate() {
                for (symbol, s) in f.symbols.iter().enumerate() {
                    if s.short() == name {
                        out.push(Ref { file, symbol });
                    }
                }
            }
            out.retain(within);
        }
        out
    }

    /// What the call at `site` reaches: its targets, and whether it's sure.
    #[must_use]
    pub fn resolve(&self, site: Site) -> (Vec<Ref>, bool) {
        let (_, f) = self.files[site.file];
        let call = &f.calls[site.call];
        let (name, macro_call) = match call.name.strip_suffix('!') {
            Some(n) => (n, true),
            None => (call.name.as_str(), false),
        };
        // (Test code is reachable only from tests.)
        let from_test = call.from.is_some_and(|i| f.symbols[i].test);
        let from = self.package_of[site.file];
        // (A call reaches code in its own language, or one that shares its
        // names: TypeScript's into JavaScript, Kotlin's into Java.)
        let family = family(f.lang);
        let keep = |r: &Ref| {
            (self.symbol(*r).kind == Kind::Macro) == macro_call
                && (from_test || !self.symbol(*r).test)
                && self.packages.reaches_package(from, self.package_of[r.file])
                && self::family(self.files[r.file].1.lang) == family
        };
        if let Some(found) = call.receiver.as_deref().and_then(|receiver| self.typed_fast(site.file, call, receiver, name, &keep)) {
            return found;
        }
        let all: Vec<Ref> = self
            .reachable(site.file, name)
            .into_iter()
            .filter(|r| (self.symbol(*r).kind == Kind::Macro) == macro_call && (from_test || !self.symbol(*r).test) && self::family(self.files[r.file].1.lang) == family)
            .collect();
        if all.is_empty() {
            return (Vec::new(), false);
        }
        let (targets, sure) = match call.receiver.as_deref() {
            Some(receiver) => self.on_receiver(site.file, call, receiver, &all),
            None => self.bare(site.file, call, &all),
        };
        if targets.len() > MAX_POSSIBLE {
            return (Vec::new(), false);
        }
        let sure = sure && targets.len() == 1;
        (targets, sure)
    }

    /// Every call the declaration `from` makes (directly, not in what's nested in it).
    #[must_use]
    pub fn callees(&self, from: Ref) -> Vec<Edge> {
        let f = self.files[from.file].1;
        let mut out = Vec::new();
        for (call, c) in f.calls.iter().enumerate() {
            if c.from == Some(from.symbol) {
                let site = Site { file: from.file, call };
                let (targets, sure) = self.resolve(site);
                out.extend(targets.into_iter().map(|to| Edge { site, to, sure }));
            }
        }
        out
    }

    /// Every call reaching the declaration `to`.
    #[must_use]
    pub fn callers(&self, to: Ref) -> Vec<Edge> {
        let s = self.symbol(to);
        let Some(sites) = self.calls.get(s.short()) else { return Vec::new() };
        let mut out = Vec::new();
        for &site in sites {
            let (targets, sure) = self.resolve(site);
            if targets.contains(&to) {
                out.push(Edge { site, to, sure });
            }
        }
        out
    }

    /// Every edge in the graph.
    #[must_use]
    pub fn edges(&self) -> Vec<Edge> {
        let mut out = Vec::new();
        for (file, (_, f)) in self.files.iter().enumerate() {
            for call in 0..f.calls.len() {
                let site = Site { file, call };
                let (targets, sure) = self.resolve(site);
                out.extend(targets.into_iter().map(|to| Edge { site, to, sure }));
            }
        }
        out
    }

    /// Steps 1 and 2 for a call on a type, or the caller's own, through the
    /// methods by type (without gathering every declaration of the name: a
    /// repository's `new` or `default` can number thousands).
    fn typed_fast(&self, file: usize, call: &Call, receiver: &str, name: &str, keep: &impl Fn(&Ref) -> bool) -> Option<(Vec<Ref>, bool)> {
        let f = self.files[file].1;
        let lookup = |t: &str| -> Vec<Ref> { self.typed.get(&(name, norm(t))).into_iter().flatten().copied().filter(keep).collect() };
        if is_self(f.lang, receiver, call.from.map(|i| &f.symbols[i])) {
            let own = call.from.map(|i| Ref { file, symbol: i }).and_then(|r| self.type_of(r))?;
            let mine: Vec<Ref> = lookup(own).into_iter().filter(|r| self.type_of(*r) == Some(own)).collect();
            return (!mine.is_empty()).then_some((mine, true));
        }
        if f.lang == Lang::Rust && matches!(receiver, "crate" | "super") {
            return None;
        }
        // On a value just made (`Graph::new(x).with(y)`, `Cart(items).total()`,
        // `new Cart().total()`): its type's method, when it has one.
        if let Some(t) = constructed(receiver) {
            let typed = lookup(t);
            if !typed.is_empty() {
                return Some(self.pick(file, typed));
            }
        }
        // A parameter whose type the signature gives (`tree: &Outline`).
        if let Some(t) = call.from.and_then(|i| param_type(&f.symbols[i].signature, receiver)) {
            let typed = lookup(t);
            if !typed.is_empty() {
                return Some(self.pick(file, typed));
            }
            // (A type none of whose methods has the name: not one by the
            // variable's name either.)
            if t.len() > 1 && t.starts_with(char::is_uppercase) {
                return Some((Vec::new(), false));
            }
        }
        let last = last_segment(receiver);
        let mut typed = lookup(last);
        if let Some(module) = outer_module(receiver) {
            typed.retain(|r| module_named(self.path(*r), module));
        }
        if typed.is_empty() {
            // A type (capitalized) none of whose methods has the name: a
            // library's (`Vec::new`) or one the code doesn't write (derived).
            // (So is a qualified path, `<X as Default>::default`.)
            return (last.starts_with(char::is_uppercase) || receiver.starts_with('<')).then_some((Vec::new(), false));
        }
        Some(self.pick(file, typed))
    }

    /// Of a type's methods, those near `file` for certain, else all as possible.
    fn pick(&self, file: usize, typed: Vec<Ref>) -> (Vec<Ref>, bool) {
        let near = self.near(file, &typed, |r| self.type_of(r).unwrap_or_default());
        let (targets, sure) = if near.is_empty() { (typed, false) } else { (near, true) };
        if targets.len() > MAX_POSSIBLE {
            return (Vec::new(), false);
        }
        let sure = sure && targets.len() == 1;
        (targets, sure)
    }

    fn on_receiver(&self, file: usize, call: &Call, receiver: &str, all: &[Ref]) -> (Vec<Ref>, bool) {
        let (_, f) = self.files[file];
        let methods: Vec<Ref> = all.iter().copied().filter(|r| self.type_of(*r).is_some()).collect();
        // 1. On the caller's own instance or type.
        let own = call.from.map(|i| Ref { file, symbol: i }).and_then(|r| self.type_of(r));
        if is_self(f.lang, receiver, call.from.map(|i| &f.symbols[i])) {
            let mine: Vec<Ref> = own.map(|own| methods.iter().copied().filter(|r| self.type_of(*r) == Some(own)).collect()).unwrap_or_default();
            if !mine.is_empty() {
                return (mine, true);
            }
            // Rust's `self::f()`: this module's own function.
            return if f.lang == Lang::Rust { self.bare(file, call, all) } else { (methods, false) };
        }
        // Rust's `crate::f()`, `super::f()`: the crate's own functions.
        if f.lang == Lang::Rust && matches!(receiver, "crate" | "super") {
            let (found, sure) = self.bare(file, call, all);
            if !found.is_empty() {
                return (found, sure);
            }
            let root: Vec<Ref> = all
                .iter()
                .copied()
                .filter(|r| self.type_of(*r).is_none() && self.package_of[r.file] == self.package_of[file])
                .filter(|r| matches!(self.path(*r).rsplit('/').next(), Some("lib.rs" | "main.rs" | "mod.rs")))
                .collect();
            return (root, true);
        }
        let last = last_segment(receiver);
        // A path names its module too (`reqwest::Client`, `ledger::Ledger`):
        // only declarations in a module of that name.
        let all: Vec<Ref> = match outer_module(receiver) {
            Some(module) => all.iter().copied().filter(|r| module_named(self.path(*r), module)).collect(),
            None => all.to_vec(),
        };
        // 2. On a type, or what reads as one: sure where the type is near
        // (this file, imported, this folder).
        let typed: Vec<Ref> = all.iter().copied().filter(|r| self.type_of(*r).is_some_and(|t| same_word(t, last))).collect();
        if !typed.is_empty() {
            let near = self.near(file, &typed, |r| self.type_of(r).unwrap_or_default());
            return if near.is_empty() { (typed, false) } else { (near, true) };
        }
        // 3. On a module: a file or folder of that name.
        let in_module: Vec<Ref> = all.iter().copied().filter(|r| self.type_of(*r).is_none() && module_named(self.path(*r), last)).collect();
        if !in_module.is_empty() {
            return (in_module, true);
        }
        // A type or module the repository doesn't declare (`Vec::new`,
        // `HashMap.of`, an imported library's `requests.get`): the library's.
        let first = receiver.split([':', '.']).next().unwrap_or(receiver);
        if receiver.contains("::") || last.starts_with(char::is_uppercase) || f.imports.iter().any(|i| has_word(&i.text, first)) {
            return (Vec::new(), false);
        }
        // 5. On something else (a variable, a field): the methods of types
        // whose names share its word (`handle` and `SessionHandle`).
        let word = value_word(receiver);
        if word.len() < 3 {
            return (Vec::new(), false);
        }
        (methods.into_iter().filter(|r| self.type_of(*r).is_some_and(|t| shares_word(t, word))).collect(), false)
    }

    /// Of `found`, those whose `key` (a type's name, say) is near the caller's
    /// file: declared in it, named in its imports, or in its folder.
    fn near(&self, file: usize, found: &[Ref], key: impl Fn(Ref) -> &'a str) -> Vec<Ref> {
        let (path, f) = self.files[file];
        let here: Vec<Ref> = found.iter().copied().filter(|r| r.file == file || self.declares(file, key(*r))).collect();
        if !here.is_empty() {
            return here;
        }
        let imported: Vec<Ref> = found.iter().copied().filter(|r| f.imports.iter().any(|i| has_word(&i.text, key(*r)))).collect();
        if !imported.is_empty() {
            return imported;
        }
        found.iter().copied().filter(|r| folder_of(self.path(*r)) == folder_of(path)).collect()
    }

    /// Whether the file declares a type (or impl) named `name`.
    fn declares(&self, file: usize, name: &str) -> bool {
        self.files[file].1.symbols.iter().any(|s| s.kind.qualifies() && s.short() == name)
    }

    fn bare(&self, file: usize, call: &Call, all: &[Ref]) -> (Vec<Ref>, bool) {
        let (path, f) = self.files[file];
        let lang = f.lang;
        let caller = call.from.map(|i| Ref { file, symbol: i });
        // 1. A method of the caller's own type, where `this` is implied.
        if implied_this(lang)
            && let Some(own) = caller.and_then(|r| self.type_of(r))
        {
            let mine: Vec<Ref> = all.iter().copied().filter(|r| self.type_of(*r) == Some(own) && self.files[r.file].1.lang == lang).collect();
            if !mine.is_empty() {
                return (mine, true);
            }
        }
        let free: Vec<Ref> = if implied_this(lang) { all.to_vec() } else { all.iter().copied().filter(|r| self.type_of(*r).is_none()).collect() };
        // 2. In the caller's file.
        let here: Vec<Ref> = free.iter().copied().filter(|r| r.file == file).collect();
        if !here.is_empty() {
            return (here, true);
        }
        // 3. Imported by name, or with everything in its module (`use ledger::*`).
        let name = call.name.trim_end_matches('!');
        let imports: Vec<&str> = f.imports.iter().map(|i| i.text.as_str()).filter(|t| has_word(t, name) || is_glob(t)).collect();
        let imported: Vec<Ref> = free.iter().copied().filter(|r| imports.iter().any(|t| names_path(t, self.path(*r)))).collect();
        if !imported.is_empty() {
            return (imported, true);
        }
        // 4. In the caller's folder, where a folder's names are shared.
        if folder_wide(lang) {
            let folder = folder_of(path);
            let near: Vec<Ref> = free.iter().copied().filter(|r| folder_of(self.path(*r)) == folder && self.files[r.file].1.lang == lang).collect();
            if !near.is_empty() {
                return (near, true);
            }
        }
        // 5. Anywhere, where names are global.
        if global_names(lang) {
            return (free.iter().copied().filter(|r| self.files[r.file].1.lang == lang).collect(), true);
        }
        (Vec::new(), false)
    }

    /// The short name of the type a declaration belongs to, if it's a method.
    fn type_of(&self, r: Ref) -> Option<&'a str> {
        let (_, f) = self.files[r.file];
        let s = &f.symbols[r.symbol];
        if s.kind != Kind::Function {
            return None;
        }
        match s.parent.map(|p| &f.symbols[p]) {
            Some(p) if p.kind == Kind::Module => None,
            Some(p) if p.kind.qualifies() => Some(p.short()),
            Some(_) => None,
            // (Go's methods and C++'s out-of-line ones are named with their type.)
            None => {
                let sep = f.lang.separator();
                s.name.rsplit_once(sep).map(|(q, _)| q.rsplit_once(sep).map_or(q, |(_, t)| t))
            }
        }
    }
}

/// Whether a receiver is the caller's own instance or type.
/// The type `signature` gives its parameter `name` (`name: &mut Type<T>`,
/// `name: Optional[Type]`, Go's `name *Type`): the type's own name.
fn param_type<'s>(signature: &'s str, name: &str) -> Option<&'s str> {
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let open = signature.find('(')?;
    let params = &signature[open + 1..];
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let at = params.match_indices(name).find(|(i, _)| {
        let before = params[..*i].chars().next_back();
        let after = &params[i + name.len()..];
        !before.is_some_and(ident) && !after.starts_with(ident) && {
            let rest = after.trim_start();
            rest.starts_with(':') && !rest.starts_with("::") || rest.starts_with('*') || rest.starts_with(char::is_uppercase)
        }
    })?;
    let mut rest = params[at.0 + name.len()..].trim_start();
    rest = rest.strip_prefix(':').unwrap_or(rest).trim_start();
    // Past references, pointers and lifetimes, and an optional's wrapper.
    loop {
        let before = rest;
        for prefix in ["&", "*", "mut ", "dyn ", "impl ", "const ", "Optional[", "Option<", "?"] {
            rest = rest.strip_prefix(prefix).unwrap_or(rest).trim_start();
        }
        if rest.starts_with('\'') {
            rest = rest.find(' ').map_or("", |i| rest[i..].trim_start());
        }
        if rest == before {
            break;
        }
    }
    let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':' || c == '.')).unwrap_or(rest.len());
    let path = &rest[..end];
    let last = path.rsplit([':', '.']).next().unwrap_or(path);
    (!last.is_empty()).then_some(last)
}

/// The type of the value a receiver makes, when it's a constructor's call:
/// `Graph` for `harlo_index::Graph::new(&files)` (a type's function, as
/// Rust's constructors are), `Cart(items)` and `new Cart(items)`.
fn constructed(receiver: &str) -> Option<&str> {
    let inner = receiver.strip_suffix(')')?;
    // The opening bracket of the last group.
    let mut depth = 0usize;
    let mut open = None;
    for (i, c) in inner.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' if depth == 0 => {
                open = Some(i);
                break;
            }
            '(' => depth -= 1,
            _ => {}
        }
    }
    let head = inner[..open?].trim();
    let head = head.strip_prefix("new ").unwrap_or(head);
    let segments: Vec<&str> = head.split("::").flat_map(|s| s.split('.')).collect();
    let typed = |s: &str| s.starts_with(char::is_uppercase) && s.chars().all(|c| c.is_alphanumeric() || c == '_');
    match segments.as_slice() {
        [.., last] if typed(last) => Some(last),
        [.., ty, f] if typed(ty) && f.starts_with(char::is_lowercase) => Some(ty),
        _ => None,
    }
}

/// The languages whose code calls each other's by name: one stands for
/// its group (JavaScript for TypeScript, TSX and QML; Java for Kotlin and
/// Scala; C++ for C).
fn family(lang: Lang) -> Lang {
    match lang {
        Lang::TypeScript | Lang::Tsx | Lang::Qml => Lang::JavaScript,
        Lang::Kotlin | Lang::Scala => Lang::Java,
        Lang::C => Lang::Cpp,
        other => other,
    }
}

fn is_self(lang: Lang, receiver: &str, caller: Option<&Symbol>) -> bool {
    if matches!(receiver, "self" | "Self" | "this" | "cls" | "$this" | "static" | "@") {
        return true;
    }
    // Go names its receiver: `func (c *Cart) Total()`.
    lang == Lang::Go && caller.is_some_and(|s| s.signature.strip_prefix("func (").and_then(|r| r.split_whitespace().next()).is_some_and(|v| v == receiver))
}

/// The last part of a receiver (`crate::ledger` → `ledger`, `self.orders` → `orders`).
fn last_segment(receiver: &str) -> &str {
    let r = receiver.trim_end_matches(['(', ')', '?', '!']);
    let cut =
        [r.rfind("::").map(|i| i + 2), r.rfind('.').map(|i| i + 1), r.rfind("->").map(|i| i + 2), r.rfind('\\').map(|i| i + 1)].into_iter().flatten().max();
    cut.map_or(r, |i| &r[i..]).trim_start_matches(['$', '@', '&', '*'])
}

/// The module a path receiver names before its last part (`reqwest` in
/// `reqwest::Client`), unless that's `crate`, `self` or `super`.
fn outer_module(receiver: &str) -> Option<&str> {
    let mut parts = receiver.rsplit("::");
    parts.next()?;
    let module = parts.next()?;
    (!matches!(module, "crate" | "self" | "super" | "Self" | "") && module.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(module)
}

/// The word of a value receiver: its last part, without an index, and an
/// indexed one in the singular (`lines[i]` → `line`).
fn value_word(receiver: &str) -> &str {
    let r = receiver.trim_end_matches(['(', ')', '?', '!']);
    let (r, indexed) = match r.strip_suffix(']').and_then(|r| r.rfind('[').map(|i| &r[..i])) {
        Some(base) => (base, true),
        None => (r, false),
    };
    let word = last_segment(r);
    if indexed { word.strip_suffix('s').unwrap_or(word) } else { word }
}

/// Whether a type's name holds a receiver's word (`SessionHandle` and `handle`).
fn shares_word(type_name: &str, word: &str) -> bool {
    let (t, w) = (norm(type_name), norm(word));
    !w.is_empty() && (t.contains(&w) || w.contains(&t))
}

/// Whether two names are the same word, case and underscores aside (`order_book` and `OrderBook`).
fn same_word(a: &str, b: &str) -> bool {
    !b.is_empty() && norm(a) == norm(b)
}

/// A name lowercased without underscores, for comparing words.
fn norm(s: &str) -> String {
    s.chars().filter(|c| *c != '_').flat_map(char::to_lowercase).collect()
}

/// Whether the file at `path` is the module `name`: its file's stem or a folder's name.
fn module_named(path: &str, name: &str) -> bool {
    !name.is_empty()
        && path.split('/').enumerate().any(|(i, part)| {
            let last = i == path.matches('/').count();
            let stem = if last { part.split('.').next().unwrap_or(part) } else { part };
            // (A crate's folder `harlo-core` is the module `harlo_core`.)
            stem == name || (stem.contains('-') && stem.replace('-', "_") == name)
        })
}

/// Whether an import brings in everything in a module (`ledger::*`, `from x import *`).
fn is_glob(import: &str) -> bool {
    import.trim_end_matches(['}', ';', ' ']).ends_with('*')
}

/// Whether `text` holds `word` as a whole word.
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// Whether an import's text names the file at `path` (its stem or a folder).
fn names_path(import: &str, path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let stem = parts.last().map(|p| p.split('.').next().unwrap_or(p)).unwrap_or_default();
    // (`mod.rs`, `__init__.py`, `index.ts` are named by their folder.)
    let named = if matches!(stem, "mod" | "__init__" | "index" | "lib" | "main") && parts.len() > 1 { parts[parts.len() - 2] } else { stem };
    has_word(import, named)
}

fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// Languages where a bare call can be a method of the caller's own type.
fn implied_this(lang: Lang) -> bool {
    matches!(lang, Lang::Java | Lang::CSharp | Lang::Cpp | Lang::Kotlin | Lang::Scala | Lang::Swift | Lang::Ruby | Lang::Qml)
}

/// Languages where a folder's names are visible to each other without imports.
fn folder_wide(lang: Lang) -> bool {
    matches!(lang, Lang::Go | Lang::C | Lang::Cpp | Lang::Swift | Lang::Java | Lang::Kotlin | Lang::Scala | Lang::CSharp | Lang::Bash | Lang::Lua)
}

/// Languages whose functions share one global namespace.
fn global_names(lang: Lang) -> bool {
    matches!(lang, Lang::C | Lang::Cpp | Lang::Bash | Lang::Lua | Lang::Php | Lang::Julia | Lang::Swift | Lang::Ruby)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(last_segment("crate::ledger"), "ledger");
        assert_eq!(last_segment("self.order_book"), "order_book");
        assert!(same_word("OrderBook", "order_book"));
        assert!(module_named("crates/harlo-core/src/a.rs", "harlo_core") && is_glob("super::*") && is_glob("from x import *"));
        assert!(module_named("src/ledger.rs", "ledger") && module_named("pkg/ledger/post.go", "ledger") && !module_named("src/ledgers.rs", "ledger"));
        assert!(has_word("crate::ledger::{post, Ledger}", "post") && !has_word("poster", "post"));
        assert!(names_path("crate::ledger::post", "src/ledger/mod.rs"));
        assert_eq!(outer_module("reqwest::Client"), Some("reqwest"));
        assert_eq!(outer_module("crate::Ledger"), None);
        assert_eq!(value_word("self.doc.lines[i]"), "line");
        assert!(shares_word("SessionHandle", "handle") && !shares_word("Pixels", "img"));
        assert_eq!(param_type("fn users(tree: &Outline) -> Vec<String>", "tree"), Some("Outline"));
        assert_eq!(param_type("fn f(&self, w: &mut crate::io::Writer<'_>, x: u8)", "w"), Some("Writer"));
        assert_eq!(param_type("fn f(x: &'a Tree)", "x"), Some("Tree"));
        assert_eq!(param_type("def f(self, cart: Optional[Cart]) -> int", "cart"), Some("Cart"));
        assert_eq!(param_type("func Serve(s *Server, n int)", "s"), Some("Server"));
        assert_eq!(param_type("fn f(trees: Tree)", "tree"), None);
        assert_eq!(param_type("fn f(x: crate::Tree)", "crate"), None);
        assert_eq!(constructed("harlo_index::Graph::new(&index.files)"), Some("Graph"));
        assert_eq!(constructed("Cart(items(x))"), Some("Cart"));
        assert_eq!(constructed("new Cart()"), Some("Cart"));
        assert_eq!(constructed("load(x)"), None);
        assert_eq!(constructed("self.cart"), None);
    }
}
