//! Finds the declaration a footnote's locator names in a file's units.
//!
//! A locator is what a writer puts after the path: `fn Ledger::post`,
//! `WorkflowCard.stageColor`, `post`, `section Install`. Its first word may
//! say the kind (`fn`, `struct`, `class`, `property`, `section`, …); the
//! rest is the name, whole (`Ledger::post`) or short (`post`), with `::`
//! and `.` read alike. A name that fits more than one declaration is
//! ambiguous, and [`locate`] returns the candidates, so the writer can be
//! told to qualify it.

// Rust guideline compliant 2026-10-09

use crate::units::{Kind, Unit};

/// What a locator found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// One declaration: its index in the units.
    One(usize),
    /// Several fit: their full names.
    Ambiguous(Vec<String>),
    /// None fits.
    Missing,
}

/// The kind a locator's first word asks for, if it names one.
fn kind_word(word: &str) -> Option<&'static [Kind]> {
    Some(match word.to_ascii_lowercase().as_str() {
        "fn" | "func" | "function" | "method" | "def" | "init" => &[Kind::Function],
        "type" | "struct" | "enum" | "union" | "trait" | "class" | "interface" | "protocol" | "actor" | "typealias" => &[Kind::Type],
        "impl" | "extension" => &[Kind::Impl, Kind::Type],
        "mod" | "module" | "namespace" => &[Kind::Module],
        "const" | "static" | "var" | "let" => &[Kind::Constant, Kind::Property],
        "macro" | "macro_rules!" => &[Kind::Macro],
        "property" => &[Kind::Property],
        "signal" => &[Kind::Signal],
        "component" => &[Kind::Component],
        "id" | "object" => &[Kind::Object],
        "section" | "heading" => &[Kind::Heading],
        "table" => &[Kind::Table],
        _ => return None,
    })
}

/// A name with `::` read as `.`, without call brackets or code marks.
fn normal(name: &str) -> String {
    name.trim().trim_matches('`').trim_end_matches("()").replace("::", ".")
}

/// The locator's kinds (if it names some) and its name.
pub fn split(locator: &str) -> (Option<&'static [Kind]>, String) {
    let locator = locator.trim().trim_matches('`');
    if let Some((first, rest)) = locator.split_once(char::is_whitespace)
        && let Some(kinds) = kind_word(first)
    {
        return (Some(kinds), normal(rest));
    }
    (None, normal(locator))
}

/// The declaration in `units` that `locator` names.
pub fn locate(units: &[Unit], locator: &str) -> Found {
    let (kinds, name) = split(locator);
    if name.is_empty() {
        return Found::Missing;
    }
    let fits_kind = |u: &Unit| kinds.is_none_or(|k| k.contains(&u.kind));
    let full = |u: &Unit| normal(&u.name);
    let tiers: [&dyn Fn(&Unit) -> bool; 3] = [
        &|u| full(u) == name,
        &|u| full(u).ends_with(&format!(".{name}")),
        // (Headings: a locator may give only their start.)
        &|u| u.kind == Kind::Heading && full(u).to_lowercase().starts_with(&name.to_lowercase()),
    ];
    for tier in tiers {
        let mut hits: Vec<usize> = (0..units.len()).filter(|&i| tier(&units[i]) && fits_kind(&units[i])).collect();
        if hits.len() > 1 {
            // Test code only when nothing else fits.
            let real: Vec<usize> = hits.iter().copied().filter(|&i| !units[i].test).collect();
            if !real.is_empty() {
                hits = real;
            }
        }
        // The kind the locator names first wins (`impl Ledger`: the impl, not the struct).
        if hits.len() > 1
            && let Some(first) = kinds.and_then(|k| k.iter().find(|k| hits.iter().any(|&i| units[i].kind == **k)))
        {
            hits.retain(|&i| units[i].kind == *first);
        }
        // Several blocks of one name (a type's `impl`s) are one declaration here.
        if hits.len() > 1 && hits.iter().all(|&i| units[i].name == units[hits[0]].name && units[i].kind == units[hits[0]].kind) {
            hits.truncate(1);
        }
        match hits.len() {
            0 => {}
            1 => return Found::One(hits[0]),
            _ => return Found::Ambiguous(hits.iter().map(|&i| format!("{} {}", units[i].kind, units[i].name)).collect()),
        }
    }
    Found::Missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::scan;

    #[test]
    fn finds_by_full_short_and_kind() {
        let src = "pub struct Ledger {}\nimpl Ledger {\n    pub fn post(&self) {}\n    pub fn new() -> Self { Ledger {} }\n}\nimpl Cart {\n    pub fn new() -> Self { Cart {} }\n}\n#[cfg(test)]\nmod tests {\n    fn post() {}\n}\n";
        let units = scan("src/ledger.rs", src);
        let name = |f: Found| match f {
            Found::One(i) => units[i].name.clone(),
            other => format!("{other:?}"),
        };
        assert_eq!(name(locate(&units, "fn Ledger::post")), "Ledger::post");
        assert_eq!(name(locate(&units, "post")), "Ledger::post");
        assert_eq!(name(locate(&units, "`Ledger.post()`")), "Ledger::post");
        assert_eq!(name(locate(&units, "struct Ledger")), "Ledger");
        assert_eq!(name(locate(&units, "impl Ledger")), "Ledger");
        assert_eq!(name(locate(&units, "new")), "Ambiguous([\"fn Ledger::new\", \"fn Cart::new\"])");
        assert_eq!(locate(&units, "fn missing"), Found::Missing);
        let md = scan("README.md", "# Harlo\n## Install on Arch\nText\n");
        assert_eq!(locate(&md, "section Install"), Found::One(1));
    }
}
