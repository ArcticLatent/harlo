//! Structural search and rewrite: code found by its syntax, not its text.
//!
//! A pattern is code with metavariables: `$X` stands for one node, `$$$X`
//! for any number of them (arguments, statements), `$_` for one node it
//! doesn't name. `foo($A, $$$REST)` matches every call to `foo` with at
//! least one argument, however it's spaced or wrapped, and never one in a
//! comment or a string. A rewrite is code too, using the pattern's
//! metavariables: `$OBJ.unwrap()` rewritten to `$OBJ.expect("$OBJ")`.
//!
//! [`search`] reads every file under the paths asked for whose language has
//! a grammar ([`Lang`]: YAML and SQL have none), and parses the pattern in
//! each of their languages, leaving out those it doesn't parse in. The
//! matching is ast-grep's (`ast-grep-core`, MIT).

// Rust guideline compliant 2026-10-09

use serde::{Deserialize, Serialize};

/// One match: where it is, its text and, for a rewrite, what replaces it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    /// The file, from the root.
    pub path: String,
    /// Its first and last lines (1-based).
    pub line: usize,
    pub end_line: usize,
    /// The code matched.
    pub text: String,
    /// The code replacing it (a rewrite only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

/// A file's text after a rewrite.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rewritten {
    /// The file, from the root.
    pub path: String,
    /// Its whole new text.
    pub text: String,
}

/// What a search or a rewrite found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Structural {
    /// The matches, by file and line, at most the limit asked for.
    pub matches: Vec<Found>,
    /// Every match, past the limit too.
    pub total: usize,
    /// The files a rewrite changes, with their new text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Rewritten>,
    /// The languages the pattern parsed in, with an error, but was tried in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A structural search, or a rewrite.
#[derive(Clone, Debug, Default)]
pub struct Search<'a> {
    /// The code pattern.
    pub pattern: &'a str,
    /// The code replacing each match (a rewrite).
    pub rewrite: Option<&'a str>,
    /// Only files in this language.
    pub lang: Option<crate::Lang>,
    /// Files or folders from the root (all of it when empty).
    pub paths: Vec<&'a str>,
    /// The most matches listed.
    pub limit: usize,
}

#[cfg(feature = "scan")]
pub use run::search;

#[cfg(feature = "scan")]
mod run {
    use super::{Found, Rewritten, Search, Structural};
    use crate::{Lang, lang};
    use ast_grep_core::matcher::PatternBuilder;
    use ast_grep_core::tree_sitter::{LanguageExt, StrDoc, TSLanguage};
    use ast_grep_core::{Language, Pattern, PatternError};
    use std::borrow::Cow;
    use std::collections::BTreeMap;
    use std::path::Path;

    /// Files over this size are left out.
    const MAX_BYTES: u64 = 1024 * 1024;

    /// A language's grammar, as ast-grep matches with it.
    #[derive(Clone)]
    struct Grammar {
        ts: TSLanguage,
        /// What `$` becomes in a pattern before it's parsed: a character
        /// the language takes in a name where `$` isn't one.
        expando: char,
    }

    impl Grammar {
        fn of(lang: Lang) -> Option<Self> {
            let ts = crate::rules::grammar(lang)?;
            let expando = match lang {
                Lang::JavaScript | Lang::TypeScript | Lang::Tsx | Lang::Java | Lang::Bash | Lang::Lua | Lang::Scala | Lang::Qml | Lang::Php => '$',
                _ => 'µ',
            };
            Some(Self { ts, expando })
        }
    }

    impl Language for Grammar {
        fn pre_process_pattern<'q>(&self, query: &'q str) -> Cow<'q, str> {
            if self.expando == '$' { Cow::Borrowed(query) } else { Cow::Owned(expand(query, self.expando)) }
        }

        fn expando_char(&self) -> char {
            self.expando
        }

        fn kind_to_id(&self, kind: &str) -> u16 {
            self.ts.id_for_node_kind(kind, true)
        }

        fn field_to_id(&self, field: &str) -> Option<u16> {
            self.ts.field_id_for_name(field).map(std::num::NonZero::get)
        }

        fn build_pattern(&self, builder: &PatternBuilder<'_>) -> Result<Pattern, PatternError> {
            builder.build(|src| StrDoc::try_new(src, self.clone()))
        }
    }

    impl LanguageExt for Grammar {
        fn get_ts_language(&self) -> TSLanguage {
            self.ts.clone()
        }
    }

    /// `query` with each metavariable's `$` as `expando` (a `$` before a
    /// capital or `_`, and `$$$`); other `$`s stay.
    fn expand(query: &str, expando: char) -> String {
        let mut out = String::with_capacity(query.len());
        let mut dollars = 0;
        for c in query.chars() {
            if c == '$' {
                dollars += 1;
                continue;
            }
            let sigil = if dollars == 3 || c.is_ascii_uppercase() || c == '_' { expando } else { '$' };
            out.extend(std::iter::repeat_n(sigil, dollars));
            dollars = 0;
            out.push(c);
        }
        out.extend(std::iter::repeat_n(if dollars == 3 { expando } else { '$' }, dollars));
        out
    }

    /// The matches of `query` under `root`, and for a rewrite the files'
    /// new text.
    ///
    /// # Errors
    ///
    /// The pattern parses in none of the files' languages (the parser's
    /// reason), or a path asked for isn't there.
    pub fn search(root: &Path, query: &Search<'_>) -> Result<Structural, String> {
        if query.pattern.trim().is_empty() {
            return Err("Give a pattern.".to_owned());
        }
        let mut patterns: BTreeMap<Lang, Option<(Grammar, Pattern)>> = BTreeMap::new();
        let mut why: Vec<String> = Vec::new();
        let mut out = Structural::default();
        for rel in files(root, &query.paths)? {
            let Some(l) = lang(&rel).filter(|l| query.lang.is_none_or(|q| q == *l || (q == Lang::TypeScript && *l == Lang::Tsx))) else { continue };
            let entry = patterns.entry(l).or_insert_with(|| {
                let grammar = Grammar::of(l)?;
                match Pattern::try_new(query.pattern, grammar.clone()) {
                    Ok(p) if !p.has_error() => Some((grammar, p)),
                    Ok(_) => {
                        why.push(format!("{l}: the pattern isn't valid code"));
                        None
                    }
                    Err(e) => {
                        why.push(format!("{l}: {e}"));
                        None
                    }
                }
            });
            let Some((grammar, pattern)) = entry else { continue };
            let Ok(text) = std::fs::read_to_string(root.join(&rel)) else { continue };
            let doc = grammar.ast_grep(&text);
            let lines = |at: usize| text[..at].matches('\n').count() + 1;
            if let Some(rewrite) = query.rewrite {
                let mut edits = doc.root().replace_all(&*pattern, rewrite);
                if edits.is_empty() {
                    continue;
                }
                edits.sort_by_key(|e| e.position);
                for e in &edits {
                    let replacement = String::from_utf8_lossy(&e.inserted_text).into_owned();
                    let end = e.position + e.deleted_length;
                    out.total += 1;
                    if out.matches.len() < query.limit {
                        out.matches.push(Found { path: rel.clone(), line: lines(e.position), end_line: lines(end), text: text[e.position..end].to_owned(), replacement: Some(replacement) });
                    }
                }
                let mut new = text.clone();
                for e in edits.iter().rev() {
                    new.replace_range(e.position..e.position + e.deleted_length, &String::from_utf8_lossy(&e.inserted_text));
                }
                if new != text {
                    out.files.push(Rewritten { path: rel.clone(), text: new });
                }
            } else {
                for m in doc.root().find_all(&*pattern) {
                    out.total += 1;
                    if out.matches.len() < query.limit {
                        let r = m.get_node().range();
                        out.matches.push(Found { path: rel.clone(), line: lines(r.start), end_line: lines(r.end), text: text[r].to_owned(), replacement: None });
                    }
                }
            }
        }
        let tried = patterns.values().filter(|p| p.is_some()).count();
        if tried == 0 {
            return Err(if why.is_empty() {
                query.lang.map_or_else(|| "No files with a grammar under those paths.".to_owned(), |l| format!("No {l} files under those paths."))
            } else {
                format!("The pattern doesn't parse: {}. Write it as a complete piece of code in the language (an expression, a statement or a declaration).", why.join("; "))
            });
        }
        out.notes = why;
        Ok(out)
    }

    /// The files under `paths` (or all of `root`), as paths from `root`,
    /// sorted. A file named outright is read even where a walk skips it.
    fn files(root: &Path, paths: &[&str]) -> Result<Vec<String>, String> {
        let starts: Vec<&str> = if paths.is_empty() { vec![""] } else { paths.to_vec() };
        let mut out = Vec::new();
        for start in starts {
            let start = start.trim().trim_start_matches("./").trim_end_matches('/');
            let at = if start.is_empty() || start == "." { root.to_path_buf() } else { root.join(start) };
            if at.is_file() {
                out.push(start.to_owned());
                continue;
            }
            if !at.is_dir() {
                return Err(format!("{start} isn't there."));
            }
            let walk = ignore::WalkBuilder::new(&at).hidden(true).git_ignore(true).git_exclude(true).git_global(true).require_git(false).build();
            for entry in walk.flatten() {
                if !entry.file_type().is_some_and(|t| t.is_file()) || entry.metadata().is_ok_and(|m| m.len() > MAX_BYTES) {
                    continue;
                }
                let Some(rel) = entry.path().strip_prefix(root).ok().and_then(Path::to_str) else { continue };
                let rel = rel.replace('\\', "/");
                if !crate::index::vendored(&rel) {
                    out.push(rel);
                }
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn tree(files: &[(&str, &str)]) -> std::path::PathBuf {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("harlo-structural-{}-{n}", std::process::id()));
            for (p, t) in files {
                std::fs::create_dir_all(dir.join(p).parent().unwrap()).unwrap();
                std::fs::write(dir.join(p), t).unwrap();
            }
            dir
        }

        #[test]
        fn metavariables_become_the_languages_names() {
            assert_eq!(expand("foo($A, $$$REST) + $_", 'µ'), "foo(µA, µµµREST) + µ_");
            assert_eq!(expand("x = \"$5\" + $$$", 'µ'), "x = \"$5\" + µµµ");
        }

        #[test]
        fn a_pattern_finds_code_however_its_written() {
            let dir = tree(&[
                ("a.py", "x = total(items,\n          tax)\n# total(a, b)\ny = 'total(c, d)'\nz = total(1)\n"),
                ("src/b.rs", "fn main() {\n    let v = maybe().unwrap();\n    other.unwrap();\n}\n"),
            ]);
            let q = Search { pattern: "total($A, $B)", limit: 50, ..Search::default() };
            let got = search(&dir, &q).unwrap();
            assert_eq!(got.total, 1, "{got:?}");
            assert_eq!((got.matches[0].path.as_str(), got.matches[0].line, got.matches[0].end_line), ("a.py", 1, 2));

            let q = Search { pattern: "$X.unwrap()", lang: Some(Lang::Rust), limit: 1, ..Search::default() };
            let got = search(&dir, &q).unwrap();
            assert_eq!((got.total, got.matches.len(), got.matches[0].text.as_str()), (2, 1, "maybe().unwrap()"));
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn a_rewrite_uses_the_metavariables() {
            let dir = tree(&[("src/b.rs", "fn main() {\n    let v = maybe().unwrap();\n    other.unwrap();\n}\n")]);
            let q = Search { pattern: "$X.unwrap()", rewrite: Some("$X.expect(\"present\")"), paths: vec!["src"], limit: 50, ..Search::default() };
            let got = search(&dir, &q).unwrap();
            assert_eq!(got.files.len(), 1);
            assert_eq!(got.files[0].text, "fn main() {\n    let v = maybe().expect(\"present\");\n    other.expect(\"present\");\n}\n");
            assert_eq!(got.matches[1].replacement.as_deref(), Some("other.expect(\"present\")"));
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn a_pattern_that_parses_nowhere_says_why() {
            let dir = tree(&[("a.py", "x = 1\n")]);
            let q = Search { pattern: "def (", limit: 50, ..Search::default() };
            assert!(search(&dir, &q).unwrap_err().contains("doesn't parse"));
            let q = Search { pattern: "x", paths: vec!["missing"], limit: 50, ..Search::default() };
            assert!(search(&dir, &q).unwrap_err().contains("isn't there"));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
