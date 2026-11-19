//! harlo's code index: each source file's declarations, imports and calls.
//!
//! [`scan`] reads one file with its language's tree-sitter grammar (YAML
//! and SQL as text) and returns a [`FileIndex`]: the file's [`Symbol`]s
//! (functions, types, methods, …) with their lines, signatures, first doc
//! lines and hashes, the [`Import`]s it makes and the [`Call`]s in it. [`index`] does that for
//! a whole repository and keeps the results in a cache keyed by each file's
//! contents, so a second run reads only the files that changed, and a
//! renamed file is not read again.
//!
//! Languages ([`Lang`]): Rust, Python, Go, JavaScript, TypeScript (and
//! TSX), Java, C, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Julia, Bash,
//! Lua and QML through their grammars; YAML (its keys) and SQL (its
//! `CREATE` statements) as text.
//!
//! Names follow harlo's wiki footnotes: a method is named with its type
//! (`Ledger::post` in Rust, `Cart.total` elsewhere), and [`Kind::word`] is
//! the word a footnote uses for its kind.
//!
//! A symbol's hash covers its own text: its lines without those of the
//! symbols nested in it, trimmed, without blank lines and without comments
//! other than doc comments. Re-indenting code or editing a plain comment
//! changes no hash; editing code or a doc comment does.
//!
//! [`index`] also reads the repository's package manifests ([`Manifest`]:
//! Cargo, npm, Python, Go, Maven, Gradle, Ruby, Composer, SwiftPM, Dart and
//! Mix), so each package's dependencies, and which files use them, are known.
//!
//! [`Graph`] resolves the calls to the declarations they reach (see its
//! module for how), for callers and callees, and [`lookup`] answers an
//! agent's question about a name from it, as compact text. [`code_map`]
//! and [`cards`] brief a wiki's writer.
//!
//! [`search`] finds code by a pattern written as code (`foo($A, $$$REST)`),
//! matched on syntax trees, and rewrites it with a template.
//!
//! Without the default `scan` feature the crate holds only these types, for
//! code that reads the `harlo-index` helper's JSON output.

// Rust guideline compliant 2026-10-09

mod brief;
#[cfg(feature = "scan")]
mod defs;
mod graph;
#[cfg(feature = "scan")]
mod index;
mod lang;
mod lookup;
#[cfg(feature = "scan")]
mod manifests;
mod packages;
#[cfg(feature = "scan")]
mod rules;
#[cfg(feature = "scan")]
mod scan;
mod structural;
#[cfg(feature = "scan")]
mod text;

use serde::{Deserialize, Serialize};
use std::fmt;

pub use brief::{Card, Link, cards, code_map};
pub use graph::{Edge, Graph, Ref, Site};
#[cfg(feature = "scan")]
pub use index::{Error, Index, default_cache, index};
pub use lang::{Lang, lang, named};
pub use lookup::{MAX_DEPTH, Query, lookup};
#[cfg(feature = "scan")]
pub use manifests::manifest;
pub use packages::{Packages, import_names, users};
#[cfg(feature = "scan")]
pub use scan::{scan, syntax_tree};
#[cfg(feature = "scan")]
pub use structural::search;
pub use structural::{Found, Rewritten, Search, Structural};

/// What a symbol declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    /// A macro definition.
    Macro,
    /// A property (QML, Swift computed properties).
    Property,
    /// A signal (QML).
    Signal,
    /// A QML inline component.
    Component,
    /// A QML object with an `id`.
    Object,
    /// A table: SQL's tables and views, YAML's keys.
    Table,
}

impl Kind {
    /// The word a wiki footnote uses for it (`fn`, `type`, `property`, …).
    #[must_use]
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
            Kind::Table => "table",
        }
    }

    /// Whether symbols nested in it take its name as a prefix.
    #[must_use]
    pub fn qualifies(self) -> bool {
        matches!(self, Kind::Type | Kind::Impl | Kind::Module | Kind::Component | Kind::Object)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// A declaration in a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// Its name, with its containers' (`Ledger::post`, `Cart.total`).
    pub name: String,
    pub kind: Kind,
    /// Its first and last lines, from 1.
    pub start: usize,
    pub end: usize,
    /// Its declaration without its body, on one line (`pub fn post(&self, x: u32) -> u32`).
    pub signature: String,
    /// The first line of its doc comment or docstring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Whether other code can use it: `pub`, `export`, a capital in Go, no
    /// leading `_` in Python.
    #[serde(default, skip_serializing_if = "is_false")]
    pub public: bool,
    /// Whether it's test code (`#[test]`, `mod tests`, `test_…`, a test file).
    #[serde(default, skip_serializing_if = "is_false")]
    pub test: bool,
    /// The symbol it's nested in, as an index into the same list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    /// The hash of its own text (see the crate docs): 16 hex digits.
    pub hash: String,
}

impl Symbol {
    /// Its name without its containers' (`post` for `Ledger::post`).
    #[must_use]
    pub fn short(&self) -> &str {
        let after = self.name.rsplit_once("::").map_or(self.name.as_str(), |(_, s)| s);
        after.rsplit_once('.').map_or(after, |(_, s)| s)
    }
}

/// An import, `use` or `include` in a file, as written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// What it imports, without its keyword, on one line (`std::fmt::{self, Display}`).
    pub text: String,
    /// Its line, from 1.
    pub line: usize,
}

/// A call in a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    /// The name called, its last part (`post` for `self.ledger.post(…)`).
    pub name: String,
    /// What it's called on or through, as written (`self.ledger`, `Ledger`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
    /// The name's line, from 1.
    pub line: usize,
    /// The name's byte column in its line, from 0 (for asking a language server about it).
    #[serde(default)]
    pub column: usize,
    /// The symbol it's in, as an index into the file's symbols.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<usize>,
}

/// One file's symbols, imports and calls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIndex {
    pub lang: Lang,
    /// Its symbols in order of their first lines (a container before what it holds).
    pub symbols: Vec<Symbol>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<Import>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<Call>,
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's skip_serializing_if passes a reference")]
fn is_false(b: &bool) -> bool {
    !*b
}

/// The package system a manifest belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    /// `Cargo.toml`.
    Cargo,
    /// `package.json`.
    Npm,
    /// `pyproject.toml`, `requirements*.txt`.
    Python,
    /// `go.mod`.
    Go,
    /// `pom.xml`.
    Maven,
    /// `build.gradle`, `build.gradle.kts`.
    Gradle,
    /// `Gemfile`, `*.gemspec`.
    Ruby,
    /// `composer.json`.
    Composer,
    /// `Package.swift`.
    Swift,
    /// `pubspec.yaml`.
    Dart,
    /// `mix.exs`.
    Mix,
}

/// A package manifest in the repository: the package and what it depends on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Its path from the repository's root.
    pub path: String,
    pub ecosystem: Ecosystem,
    /// The package's own name, when the manifest gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
}

/// One dependency in a manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    /// Its name as the manifest gives it (`serde_json`, `@types/node`,
    /// `org.slf4j:slf4j-api`, `golang.org/x/sync`).
    pub name: String,
    /// The version asked for, as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Only for tests, development or building.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dev: bool,
    /// A package in the repository: its folder, from the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<String>,
    /// Its line in the manifest, from 1.
    pub line: usize,
}
