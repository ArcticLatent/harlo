//! The repository's packages: which folder each file's package is, and
//! which packages each one can reach through its dependencies.
//!
//! A package is a folder holding a manifest; a file belongs to the nearest
//! one above it. A dependency reaches another package of the repository by
//! its path (`path = "../core"`, `file:../ui`) or, failing that, by name.
//! Reaching is transitive: a value of a type from a dependency's dependency
//! can still be passed in.

// Rust guideline compliant 2026-10-09

use crate::{Ecosystem, FileIndex, Manifest};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The packages of a repository, from its manifests.
#[derive(Clone, Debug, Default)]
pub struct Packages {
    /// Package folders (`""` for the root).
    folders: Vec<String>,
    /// Each folder's position in `folders`.
    by_folder: HashMap<String, usize>,
    /// For each package, the packages it reaches, itself included.
    reach: Vec<HashSet<usize>>,
}

impl Packages {
    /// The packages the `manifests` describe.
    #[must_use]
    pub fn new(manifests: &[Manifest]) -> Packages {
        let folder = |m: &Manifest| m.path.rsplit_once('/').map_or(String::new(), |(d, _)| d.to_owned());
        let folders: Vec<String> = manifests.iter().map(folder).collect::<BTreeSet<_>>().into_iter().collect();
        let by_folder: HashMap<String, usize> = folders.iter().enumerate().map(|(i, f)| (f.clone(), i)).collect();
        let by_name: HashMap<String, usize> = manifests.iter().filter_map(|m| Some((normal(m.name.as_deref()?), by_folder[&folder(m)]))).collect();
        let mut direct: Vec<Vec<usize>> = vec![Vec::new(); folders.len()];
        for m in manifests {
            let from = by_folder[&folder(m)];
            for d in &m.dependencies {
                let to = d.local.as_ref().and_then(|l| by_folder.get(l)).or_else(|| by_name.get(&normal(&d.name)));
                if let Some(&to) = to {
                    direct[from].push(to);
                }
            }
        }
        // Transitive closure, by walking from each package.
        let reach = (0..folders.len())
            .map(|start| {
                let mut seen = HashSet::new();
                let mut todo = vec![start];
                while let Some(p) = todo.pop() {
                    if seen.insert(p) {
                        todo.extend(direct[p].iter().copied().filter(|q| !seen.contains(q)));
                    }
                }
                seen
            })
            .collect();
        Packages { folders, by_folder, reach }
    }

    /// The package holding the file at `path` (its number): the nearest
    /// folder above it with a manifest.
    #[must_use]
    pub fn package(&self, path: &str) -> Option<usize> {
        self.at(path)
    }

    /// Whether code in package `from` can reach declarations in package `to`
    /// (numbers from [`Packages::package`]; no package reaches and is reached by all).
    #[must_use]
    pub fn reaches_package(&self, from: Option<usize>, to: Option<usize>) -> bool {
        match (from, to) {
            (Some(a), Some(b)) => a == b || self.reach[a].contains(&b),
            _ => true,
        }
    }

    fn at(&self, path: &str) -> Option<usize> {
        path.rmatch_indices('/').find_map(|(i, _)| self.by_folder.get(&path[..i]).copied()).or_else(|| self.by_folder.get("").copied())
    }

    /// The folder of the package holding the file at `path`, if any.
    #[must_use]
    pub fn of(&self, path: &str) -> Option<&str> {
        self.at(path).map(|i| self.folders[i].as_str())
    }

    /// Whether code in the file at `from` can reach declarations in the file at `to`.
    #[must_use]
    pub fn reaches(&self, from: &str, to: &str) -> bool {
        self.reaches_package(self.at(from), self.at(to))
    }

    /// The files of each package, by its folder.
    #[must_use]
    pub fn files<'a>(&self, files: &'a BTreeMap<String, FileIndex>) -> HashMap<&str, Vec<(&'a str, &'a FileIndex)>> {
        let mut out: HashMap<&str, Vec<(&'a str, &'a FileIndex)>> = HashMap::new();
        for (path, f) in files {
            if let Some(i) = self.at(path) {
                out.entry(self.folders[i].as_str()).or_default().push((path.as_str(), f));
            }
        }
        out
    }
}

/// The names code uses to import a dependency (`serde_json` for Cargo's
/// `serde-json`, `yaml` for Python's `PyYAML`, `org.slf4j` for Maven's
/// `org.slf4j:slf4j-api`).
#[must_use]
pub fn import_names(ecosystem: Ecosystem, name: &str) -> Vec<String> {
    match ecosystem {
        Ecosystem::Cargo => vec![name.replace('-', "_")],
        Ecosystem::Python => {
            let lower = name.to_ascii_lowercase().replace('-', "_");
            // Distributions whose import differs from their name.
            let known = [
                ("pyyaml", "yaml"),
                ("beautifulsoup4", "bs4"),
                ("pillow", "PIL"),
                ("scikit_learn", "sklearn"),
                ("python_dateutil", "dateutil"),
                ("opencv_python", "cv2"),
                ("protobuf", "google.protobuf"),
                ("attrs", "attr"),
                ("python_dotenv", "dotenv"),
                ("msgpack_python", "msgpack"),
            ];
            match known.iter().find(|(d, _)| *d == lower) {
                Some((_, i)) => vec![(*i).to_owned(), lower],
                None => vec![lower],
            }
        }
        Ecosystem::Maven | Ecosystem::Gradle => vec![name.split(':').next().unwrap_or(name).to_owned()],
        Ecosystem::Ruby => vec![name.to_owned(), name.replace('-', "/")],
        Ecosystem::Dart => vec![format!("package:{name}")],
        Ecosystem::Mix => vec![name.split('_').map(capitalize).collect()],
        Ecosystem::Composer => {
            // `vendor/some-pkg` → `Vendor\SomePkg`, the usual namespace.
            let ns: Vec<String> = name.split('/').map(|p| p.split('-').map(capitalize).collect()).collect();
            vec![ns.join("\\")]
        }
        Ecosystem::Npm | Ecosystem::Go | Ecosystem::Swift => vec![name.to_owned()],
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// A package name for matching (`harlo-core` and `harlo_core` alike).
fn normal(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

/// Of a package's files (see [`Packages::files`]), those using its
/// manifest `m`'s dependency named `dep`: they import it, or call through it
/// (`serde_json::to_string`).
#[must_use]
pub fn users<'a>(package_files: &[(&'a str, &'a FileIndex)], m: &Manifest, dep: &str) -> Vec<&'a str> {
    let names = import_names(m.ecosystem, dep);
    let mentions = |text: &str| names.iter().any(|n| mentions(text, n, m.ecosystem));
    package_files
        .iter()
        .filter(|(_, f)| f.imports.iter().any(|i| mentions(&i.text)) || f.calls.iter().any(|c| c.receiver.as_deref().is_some_and(mentions)))
        .map(|(p, _)| *p)
        .collect()
}

/// Whether `text` names the module `name` at a boundary (`serde` in
/// `serde::Serialize`, not in `serde_json`; `react` in `'react/jsx'`).
fn mentions(text: &str, name: &str, ecosystem: Ecosystem) -> bool {
    text.match_indices(name).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + name.len()..].chars().next();
        let inside = |c: char| c.is_alphanumeric() || c == '_' || (ecosystem == Ecosystem::Npm && c == '-');
        !before.is_some_and(|c| inside(c) || c == '.') && !after.is_some_and(inside)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dependency;

    fn m(path: &str, name: &str, deps: &[(&str, Option<&str>)]) -> Manifest {
        Manifest {
            path: path.into(),
            ecosystem: Ecosystem::Cargo,
            name: Some(name.into()),
            dependencies: deps.iter().map(|(n, l)| Dependency { name: (*n).into(), version: None, dev: false, local: l.map(Into::into), line: 1 }).collect(),
        }
    }

    #[test]
    fn reach() {
        let p = Packages::new(&[
            m("Cargo.toml", "ws", &[]),
            m("crates/app/Cargo.toml", "app", &[("harlo-core", None)]),
            m("crates/core/Cargo.toml", "harlo_core", &[("index", Some("crates/index"))]),
            m("crates/index/Cargo.toml", "index", &[]),
            m("crates/code/Cargo.toml", "code", &[]),
        ]);
        assert_eq!(p.of("crates/app/src/a.rs"), Some("crates/app"));
        assert_eq!(p.of("scripts/x.rs"), Some(""));
        assert!(p.reaches("crates/app/src/a.rs", "crates/index/src/i.rs"));
        assert!(!p.reaches("crates/core/src/a.rs", "crates/code/src/text.rs"));
        assert!(!p.reaches("crates/index/src/a.rs", "crates/core/src/a.rs"));
        assert!(mentions("serde::Serialize", "serde", Ecosystem::Cargo) && !mentions("serde_json::Value", "serde", Ecosystem::Cargo));
        assert!(mentions("React from 'react'", "react", Ecosystem::Npm) && !mentions("'react-dom'", "react", Ecosystem::Npm));
    }
}
