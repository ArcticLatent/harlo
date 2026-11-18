//! The ledger: what the wiki was written against, in `.okf/.harlo/ledger.json`.
//!
//! OKF readers skip hidden files, so the ledger travels with the bundle
//! (committed, shared by everyone's Harlo) without being part of it. It
//! holds three things:
//!
//! - **claims**: for each page's footnote, the declaration it resolved to
//!   and that declaration's hash when the page was stamped. A different hash
//!   now means the claim is stale; no declaration means it's unresolved.
//! - **files**: for each repository file the wiki has seen, its hash and its
//!   notable declarations (as short hashes of their names). A notable
//!   declaration that isn't in this snapshot is new since the wiki last
//!   looked, and an update asks a page to cover it or leave it.
//! - **leave**: what the writer chose to leave out (`path` or
//!   `path#declaration`), with the reason, so it isn't asked about again.
//! - **pages**: each page's hash when it was stamped, so an edit shows.

// Rust guideline compliant 2026-10-09

use crate::units::{self, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// The ledger's place in a repository.
pub const PATH: &str = ".okf/.harlo/ledger.json";

/// A claim as it was stamped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// The repository file it cites.
    pub path: String,
    /// The declaration's full name; None for a whole file or a folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// The declaration's own-text hash (or the file's); empty for a folder.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// The whole file's hash then: the same now means nothing in it changed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
}

/// A file as the wiki last saw it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    pub hash: String,
    /// Its notable declarations: 8-digit hashes of their names, sorted, space-separated.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub units: String,
}

impl Seen {
    /// The snapshot of a file from its text and units.
    pub fn new(text: &str, units: &[Unit]) -> Seen {
        let mut names: Vec<String> = units.iter().filter(|u| notable(u)).map(|u| name_key(&u.name)).collect();
        names.sort();
        names.dedup();
        Seen { hash: units::digest(text.as_bytes()), units: names.join(" ") }
    }

    /// Whether a declaration named `name` was in the file then.
    pub fn had(&self, name: &str) -> bool {
        let key = name_key(name);
        self.units.split(' ').any(|k| k == key)
    }
}

/// A short key for a declaration's name.
fn name_key(name: &str) -> String {
    units::digest(name.as_bytes())[..8].to_owned()
}

/// Whether a new declaration of this kind asks for the wiki's attention:
/// public functions, types, modules, macros and QML components, not tests.
pub fn notable(u: &Unit) -> bool {
    use crate::units::Kind;
    u.public && !u.test && matches!(u.kind, Kind::Function | Kind::Type | Kind::Module | Kind::Macro | Kind::Component)
}

/// The ledger.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub version: u32,
    /// Page path → footnote label → claim.
    #[serde(default)]
    pub claims: BTreeMap<String, BTreeMap<String, Claim>>,
    /// Repository path → snapshot.
    #[serde(default)]
    pub files: BTreeMap<String, Seen>,
    /// `path` or `path#declaration` → why it's left out.
    #[serde(default)]
    pub leave: BTreeMap<String, String>,
    /// Page → the hash of its file when it was stamped: a different one
    /// now means someone edited it since.
    #[serde(default)]
    pub pages: BTreeMap<String, String>,
}

/// The ledger's format version.
const VERSION: u32 = 1;

impl Ledger {
    /// An empty ledger.
    pub fn new() -> Ledger {
        Ledger { version: VERSION, ..Ledger::default() }
    }

    /// Whether `path` (or one of its folders) is left out, or `unit` in it.
    pub fn left(&self, path: &str, unit: Option<&str>) -> Option<&str> {
        if let Some(u) = unit
            && let Some(why) = self.leave.get(&format!("{path}#{u}"))
        {
            return Some(why);
        }
        self.leave.iter().find(|(k, _)| !k.contains('#') && (k.as_str() == path || (k.ends_with('/') && path.starts_with(k.as_str())))).map(|(_, v)| v.as_str())
    }
}

/// Reads the ledger of the repository at `root` (empty when there's none).
///
/// # Errors
///
/// When the file exists but can't be read or isn't a ledger.
pub fn read(root: &Path) -> io::Result<Ledger> {
    match std::fs::read_to_string(root.join(PATH)) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{PATH} isn't a ledger: {e}"))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Ledger::new()),
        Err(e) => Err(e),
    }
}

/// Writes the ledger of the repository at `root`.
///
/// # Errors
///
/// When the file can't be written.
pub fn write(root: &Path, ledger: &Ledger) -> io::Result<()> {
    let path = root.join(PATH);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(ledger).map_err(io::Error::other)?;
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::scan;

    #[test]
    fn snapshots_and_round_trips() {
        let units = scan("a.rs", "pub fn one() {}\nfn two() {}\npub struct S;\n#[test]\nfn t() {}\n");
        let seen = Seen::new("text", &units);
        assert!(seen.had("one") && seen.had("S") && !seen.had("two") && !seen.had("t"));
        let mut l = Ledger::new();
        l.files.insert("a.rs".into(), seen);
        l.claims.entry("p.md".into()).or_default().insert("one".into(), Claim { path: "a.rs".into(), unit: Some("one".into()), hash: "x".into(), file: "y".into() });
        l.leave.insert("gen/".into(), "generated".into());
        l.leave.insert("a.rs#two".into(), "internal".into());
        let dir = std::env::temp_dir().join(format!("harlo-okf-ledger-{}", std::process::id()));
        write(&dir, &l).unwrap();
        assert_eq!(read(&dir).unwrap(), l);
        assert_eq!(l.left("gen/x.rs", None), Some("generated"));
        assert_eq!(l.left("a.rs", Some("two")), Some("internal"));
        assert_eq!(l.left("a.rs", Some("one")), None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(read(&dir).unwrap(), Ledger::new());
    }
}
