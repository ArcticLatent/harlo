//! Declarations from harlo's code index, where it's installed.
//!
//! The `harlo-index` helper reads code with each language's grammar (see
//! the `harlo-index` crate), so it finds declarations the scanner in
//! [`units`](crate::units) can't (C++ methods, Kotlin, PHP, …) and names
//! the rest the same way. It's a separate program, beside harlo's own,
//! so the grammars don't make every binary that checks a wiki bigger.
//!
//! [`helper`] finds it: beside harlo's programs, in an install's
//! `lib/harlo`, or on another machine beside harlo's agent, where harlo
//! puts it on first use of the wiki there.
//!
//! [`Declarations::load`] runs it once for a repository (it keeps its own
//! cache, so a second run reads only the files that changed); a file it
//! didn't read (a language it doesn't know, a file too big) comes from the
//! scanner. Without the helper, or when it fails, everything does.
//!
//! [`lookup`] asks it about a name, for an agent's lookup tool.

// Rust guideline compliant 2026-10-09

use crate::units::{self, Kind, Unit};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The longest the helper may take before the scanner is used instead.
///
/// A first run on a very large repository (tens of thousands of files)
/// takes this order of time; later runs read only changed files. Waiting
/// longer would hold up a wiki check that the scanner can answer anyway.
const HELPER_LIMIT: Duration = Duration::from_secs(60);

/// How often the helper is checked on while it runs.
const POLL: Duration = Duration::from_millis(10);

/// The environment variable naming the helper (`off` for none), for tests
/// and for machines where it isn't beside harlo.
pub const HELPER_ENV: &str = "HARLO_INDEX";

/// Where a repository's declarations come from.
#[derive(Debug, Default)]
pub struct Declarations {
    /// The index's symbols by file, when the helper ran.
    files: Option<HashMap<String, Vec<harlo_index::Symbol>>>,
}

impl Declarations {
    /// The repository at `root`'s declarations: the index's, when the
    /// helper is installed (see [`helper`]) and runs.
    #[must_use]
    pub fn load(root: &Path) -> Declarations {
        helper().map_or_else(Declarations::default, |h| Declarations::load_with(&h, root))
    }

    /// [`Declarations::load`] with the helper at `helper`.
    #[must_use]
    pub fn load_with(helper: &Path, root: &Path) -> Declarations {
        Declarations { files: run_helper(helper, root, false) }
    }

    /// Whether the index is there (else every file comes from the scanner).
    #[must_use]
    pub fn indexed(&self) -> bool {
        self.files.is_some()
    }

    /// The index's declaration named `name` (its full name) in the file at
    /// `path`, when the index read it.
    #[must_use]
    pub fn symbol(&self, path: &str, name: &str) -> Option<&harlo_index::Symbol> {
        self.files.as_ref()?.get(path)?.iter().find(|s| s.name == name && s.kind != harlo_index::Kind::Impl)
    }

    /// Up to `limit` declarations whose short names hold `query` (any case;
    /// not tests or `impl` blocks): exact names first, then those starting
    /// with it, then the rest, each by path. Empty without the index.
    #[must_use]
    pub fn matching(&self, query: &str, limit: usize) -> Vec<(&str, &harlo_index::Symbol)> {
        let q = query.trim().rsplit(['.', ':']).next().unwrap_or_default().to_lowercase();
        let Some(files) = &self.files else { return Vec::new() };
        if q.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<(u8, &str, &harlo_index::Symbol)> = files
            .iter()
            .flat_map(|(p, symbols)| symbols.iter().map(move |s| (p.as_str(), s)))
            .filter(|(_, s)| !s.test && s.kind != harlo_index::Kind::Impl)
            .filter_map(|(p, s)| {
                let short = s.short().to_lowercase();
                let rank = if short == q { 0 } else if short.starts_with(&q) { 1 } else if short.contains(&q) { 2 } else { return None };
                Some((rank, p, s))
            })
            .collect();
        found.sort_by(|a, b| (a.0, a.1, a.2.start).cmp(&(b.0, b.1, b.2.start)));
        found.into_iter().take(limit).map(|(_, p, s)| (p, s)).collect()
    }

    /// The declarations of the file at `path`, whose text is `text`.
    #[must_use]
    pub fn units(&self, path: &str, text: &str) -> Vec<Unit> {
        match self.files.as_ref().and_then(|f| f.get(path)) {
            Some(symbols) => from_index(text, symbols),
            None => units::scan(path, text),
        }
    }
}

/// The helper: `$HARLO_INDEX` (`off`: none), else the first of [`candidates`]
/// for this program's folder that's there.
#[must_use]
pub fn helper() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os(HELPER_ENV).filter(|v| !v.is_empty()) {
        return (named != "off").then(|| PathBuf::from(named));
    }
    let exe = std::env::current_exe().ok()?;
    candidates(exe.parent()?).into_iter().find(|p| p.is_file())
}

/// Where the helper may be for a program in `dir`: beside it (a build:
/// `harlo-index`; a release's static download: `harlo-index-linux-<arch>`),
/// in `lib/harlo` next to its folder (an install: the static
/// `harlo-index-<arch>`, or `harlo-index`), or, for harlo's agent on another
/// machine (in `~/.cache/harlo` there), the copy harlo put beside it
/// (`harlo-index-<build>`; the newest, when there are more).
fn candidates(dir: &Path) -> Vec<PathBuf> {
    let arch = format!("harlo-index-{}", std::env::consts::ARCH);
    let lib = dir.join("../lib/harlo");
    let download = format!("harlo-index-linux-{}", std::env::consts::ARCH);
    let mut out = vec![dir.join("harlo-index"), dir.join(&arch), lib.join(&arch), lib.join("harlo-index"), dir.join(download)];
    if dir.ends_with(".cache/harlo") {
        let mut put: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with("harlo-index-") && !n.contains(".tmp.")))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        put.sort();
        out.extend(put.into_iter().rev().map(|(_, p)| p));
    }
    out
}

/// The helper's symbols for every file under `root`; `None` when it fails.
/// `fresh`: without the helper's cache (it reads everything, writes nothing).
fn run_helper(helper: &Path, root: &Path, fresh: bool) -> Option<HashMap<String, Vec<harlo_index::Symbol>>> {
    let mut args = vec![OsStr::new("symbols"), root.as_os_str()];
    if fresh {
        args.push(OsStr::new("--no-cache"));
    }
    serde_json::from_slice(&run(helper, &args).ok()?).ok()
}

/// The lookup tool's name, for agents.
pub const LOOKUP_TOOL: &str = "lookup";

/// What the lookup tool says it does, for agents.
pub const LOOKUP_ABOUT: &str = "Look up a function, method or type in the project's code index: where it's declared, its signature and doc line, what it calls and what calls it (callers of callers with depth). Quicker than grep and read for where-is and who-calls questions. Calls it can't settle are marked maybe; a name it doesn't know lists names like it.";

/// The lookup tool's parameters (JSON Schema), for agents.
#[must_use]
pub fn lookup_parameters() -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {
        "name": { "type": "string", "description": "post, Ledger::post or Cart.total" },
        "path": { "type": "string", "description": "A file or folder to look in" },
        "depth": { "type": "integer", "description": "Levels of callers, 1 to 3 (1 by default)" },
    }, "required": ["name"], "additionalProperties": false })
}

/// What to look up in the code index (see `harlo-index lookup`).
#[derive(Clone, Debug, Default)]
pub struct Lookup<'a> {
    /// A declaration's full or short name.
    pub name: &'a str,
    /// A file, or a folder ending in `/`, relative to the root.
    pub path: Option<&'a str>,
    /// Levels of callers (1 to 3).
    pub depth: usize,
}

/// The helper's cards for `query` in the repository at `root` (text).
///
/// # Errors
///
/// When the helper isn't installed, fails or takes too long; the text says
/// which.
pub fn lookup(root: &Path, query: &Lookup<'_>) -> Result<String, String> {
    let helper = helper().ok_or("The code index (harlo-index) isn't installed beside harlo.")?;
    let depth = query.depth.clamp(1, 3).to_string();
    let mut args = vec![OsStr::new("lookup"), root.as_os_str(), OsStr::new(query.name), OsStr::new("--depth"), OsStr::new(&depth)];
    if let Some(p) = query.path.filter(|p| !p.trim().is_empty()) {
        args.extend([OsStr::new("--path"), OsStr::new(p)]);
    }
    let out = run(&helper, &args)?;
    Ok(String::from_utf8_lossy(&out).trim_end().to_owned())
}

/// The helper's structural search (or rewrite, which it only works out:
/// nothing is written) for `query` in the repository at `root`.
///
/// # Errors
///
/// When the helper isn't installed, fails or takes too long, or the
/// pattern doesn't parse; the text says which.
pub fn structural(root: &Path, query: &harlo_index::Search<'_>) -> Result<harlo_index::Structural, String> {
    let helper = helper().ok_or("Structural search needs the code index (harlo-index), which isn't installed beside harlo.")?;
    let limit = query.limit.to_string();
    let lang = query.lang.map(harlo_index::Lang::name);
    let mut args = vec![OsStr::new("ast"), root.as_os_str(), OsStr::new(query.pattern), OsStr::new("--limit"), OsStr::new(&limit)];
    if let Some(r) = query.rewrite {
        args.extend([OsStr::new("--rewrite"), OsStr::new(r)]);
    }
    if let Some(l) = lang {
        args.extend([OsStr::new("--lang"), OsStr::new(l)]);
    }
    for p in &query.paths {
        args.extend([OsStr::new("--path"), OsStr::new(*p)]);
    }
    let out = run(&helper, &args)?;
    serde_json::from_slice(&out).map_err(|e| format!("The code index's answer couldn't be read: {e}"))
}

/// The helper's map of the repository at `root` (packages, entry points,
/// most-called declarations), when it's installed and runs.
#[must_use]
pub fn code_map(root: &Path) -> Option<String> {
    let out = run(&helper()?, &[OsStr::new("map"), root.as_os_str()]).ok()?;
    let text = String::from_utf8_lossy(&out).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// The helper's cards for the declarations in `wanted` (`(path, full
/// name)`), when it's installed and runs.
#[must_use]
pub fn cards(root: &Path, wanted: &[(String, String)]) -> Option<Vec<harlo_index::Card>> {
    if wanted.is_empty() {
        return Some(Vec::new());
    }
    let names: Vec<String> = wanted.iter().map(|(p, n)| format!("{p}#{n}")).collect();
    let mut args = vec![OsStr::new("cards"), root.as_os_str()];
    args.extend(names.iter().map(OsStr::new));
    serde_json::from_slice(&run(&helper()?, &args).ok()?).ok()
}

/// The helper's output for `args`, within [`HELPER_LIMIT`].
fn run(helper: &Path, args: &[&OsStr]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(helper)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Can't run the code index ({}): {e}", helper.display()))?;
    let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err("The code index gave no output.".to_owned());
    };
    // Read while it runs, so a large index can't fill the pipe and stall it.
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).map(|_| out)
    });
    let errors = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stderr.read_to_string(&mut out);
        out
    });
    let deadline = Instant::now() + HELPER_LIMIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("The code index took longer than {} s.", HELPER_LIMIT.as_secs()));
            }
        }
    };
    // (A reader thread that panicked lost the output; there's nothing more to say.)
    let out = reader.join().map_err(|_panicked| "The code index's output was lost.".to_owned())?.map_err(|e| e.to_string())?;
    if !status.success() {
        let why = errors.join().unwrap_or_default();
        return Err(format!("The code index failed: {}", why.trim().trim_start_matches("harlo-index: ")));
    }
    Ok(out)
}

/// The index's symbols as the wiki's units, with their old-style hashes.
fn from_index(text: &str, symbols: &[harlo_index::Symbol]) -> Vec<Unit> {
    let mut out: Vec<Unit> = symbols
        .iter()
        .map(|s| Unit {
            name: s.name.clone(),
            kind: kind(s.kind),
            start: s.start,
            end: s.end,
            public: s.public,
            test: s.test,
            parent: s.parent,
            hash: s.hash.clone(),
            legacy: String::new(),
        })
        .collect();
    units::fill_legacy(text, &mut out);
    out
}

fn kind(k: harlo_index::Kind) -> Kind {
    use harlo_index::Kind as I;
    match k {
        I::Function => Kind::Function,
        I::Type => Kind::Type,
        I::Impl => Kind::Impl,
        I::Module => Kind::Module,
        I::Constant => Kind::Constant,
        I::Macro => Kind::Macro,
        I::Property => Kind::Property,
        I::Signal => Kind::Signal,
        I::Component => Kind::Component,
        I::Object => Kind::Object,
        I::Table => Kind::Table,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_symbols_become_units() {
        let symbols = vec![harlo_index::Symbol {
            name: "Cart.total".into(),
            kind: harlo_index::Kind::Function,
            start: 1,
            end: 2,
            signature: "def total(self)".into(),
            doc: None,
            public: true,
            test: false,
            parent: None,
            hash: "0123456789abcdef".into(),
        }];
        let units = from_index("def total(self):\n    return 1  # one\n", &symbols);
        assert_eq!(units[0].name, "Cart.total");
        assert_eq!(units[0].kind, Kind::Function);
        assert_eq!(units[0].hash, "0123456789abcdef");
        // The old-style hash counts the comment, as the scanner's did.
        assert_eq!(units[0].legacy, units::digest(b"def total(self):\nreturn 1  # one\n"));
    }

    #[test]
    fn helper_places() {
        let arch = std::env::consts::ARCH;
        let c = candidates(Path::new("/opt/harlo/bin"));
        assert_eq!(c[0], Path::new("/opt/harlo/bin/harlo-index"));
        assert_eq!(c[2], Path::new("/opt/harlo/bin/../lib/harlo").join(format!("harlo-index-{arch}")));
        assert_eq!(c[4], Path::new("/opt/harlo/bin").join(format!("harlo-index-linux-{arch}")), "a release's download, beside the agent");
        // On another machine: the copy harlo put beside its agent.
        let dir = std::env::temp_dir().join(format!("harlo-okf-helper-{}/.cache/harlo", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("harlo-index-00ff"), "").unwrap();
        std::fs::write(dir.join("harlo-index-00ff.tmp.12"), "").unwrap();
        let c = candidates(&dir);
        assert_eq!(c.last().unwrap(), &dir.join("harlo-index-00ff"));
        assert!(!c.iter().any(|p| p.to_string_lossy().contains(".tmp.")));
        std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn without_the_index_the_scanner_answers() {
        let decls = Declarations::default();
        assert!(!decls.indexed());
        assert_eq!(decls.units("a.rs", "pub fn a() {}\n")[0].name, "a");
        // A helper that isn't there, or fails, leaves the scanner too.
        assert!(!Declarations::load_with(Path::new("/nonexistent/harlo-index"), Path::new(".")).indexed());
        assert!(!Declarations::load_with(Path::new("false"), Path::new(".")).indexed());
    }

    /// The real helper, when it's been built (`cargo build -p harlo-index`).
    #[test]
    fn the_built_helper_indexes_a_repository() {
        let built = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/harlo-index");
        if !built.is_file() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("harlo-okf-indexed-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let text = "class Shape {\npublic:\n    double area() const { return 0; }\n};\n";
        std::fs::write(dir.join("src/shape.cpp"), text).unwrap();
        std::fs::write(dir.join("README.md"), "# Shapes\n").unwrap();
        // (Without the cache: a test leaves nothing in ~/.cache.)
        let decls = Declarations { files: run_helper(&built, &dir, true) };
        assert!(decls.indexed());
        let names: Vec<String> = decls.units("src/shape.cpp", text).into_iter().map(|u| u.name).collect();
        assert_eq!(names, ["Shape", "Shape::area"]);
        // Markdown comes from the scanner.
        assert_eq!(decls.units("README.md", "# Shapes\n")[0].kind, Kind::Heading);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
