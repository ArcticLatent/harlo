//! Languages read as text rather than through a grammar: what's declared in
//! them is plain from their lines.
//!
//! - YAML: its top-level keys and the keys right under them
//!   (`jobs.build`, `services.web`), each a table.
//! - SQL: its `CREATE` statements: tables and views as tables, functions,
//!   procedures and triggers as functions, types as types, schemas as modules.

// Rust guideline compliant 2026-10-09

use crate::scan::{MAX_SIGNATURE, clip, own_hashes, squash, strip_comment};
use crate::{FileIndex, Kind, Lang, Symbol};

/// Byte ranges in a file's text.
type Ranges = Vec<(usize, usize)>;

/// The symbols of `text` in a language read as text; `None` for the others.
pub(crate) fn scan(lang: Lang, text: &str) -> Option<FileIndex> {
    let (symbols, comments) = match lang {
        Lang::Yaml => yaml(text),
        Lang::Sql => sql(text),
        _ => return None,
    };
    let mut symbols = symbols;
    let hashes = own_hashes(text, &comments, &symbols);
    for (s, h) in symbols.iter_mut().zip(hashes) {
        s.hash = h;
    }
    Some(FileIndex { lang, symbols, imports: Vec::new(), calls: Vec::new() })
}

fn symbol(name: String, kind: Kind, start: usize, end: usize, signature: &str, parent: Option<usize>) -> Symbol {
    Symbol { name, kind, start, end, signature: clip(&squash(signature), MAX_SIGNATURE), doc: None, public: true, test: false, parent, hash: String::new() }
}

/// YAML's keys two levels deep, and its comments' byte ranges.
fn yaml(text: &str) -> (Vec<Symbol>, Ranges) {
    let mut comments = Vec::new();
    // Each line's indent and key, if it starts a mapping entry.
    let mut keys: Vec<(usize, usize, String)> = Vec::new();
    let mut last = 0;
    let mut at = 0;
    for (i, line) in text.split('\n').enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(c) = comment_at(line) {
            comments.push((at + c, at + line.len()));
        }
        let code = comment_at(line).map_or(line, |c| &line[..c]).trim_end();
        if !code.trim().is_empty() {
            last = i + 1;
            if let Some(key) = yaml_key(code.trim_start()) {
                keys.push((i + 1, indent, key.to_owned()));
            }
        }
        at += line.len() + 1;
    }
    let mut symbols = Vec::new();
    let mut child_indent = None;
    let mut top: Option<usize> = None;
    for (line, indent, key) in &keys {
        if *indent == 0 {
            child_indent = None;
            symbols.push(symbol(key.clone(), Kind::Table, *line, *line, key, None));
            top = Some(symbols.len() - 1);
            continue;
        }
        let Some(parent) = top else { continue };
        let ci = *child_indent.get_or_insert(*indent);
        if *indent == ci {
            let name = format!("{}.{key}", symbols[parent].name);
            symbols.push(symbol(name, Kind::Table, *line, *line, key, Some(parent)));
        }
    }
    // Each key runs until the next key at its level or above.
    let ends: Vec<usize> = (0..symbols.len())
        .map(|i| {
            let level = usize::from(symbols[i].parent.is_some());
            let next = symbols[i + 1..].iter().find(|s| usize::from(s.parent.is_some()) <= level).map(|s| s.start - 1);
            next.unwrap_or(last)
        })
        .collect();
    for (s, end) in symbols.iter_mut().zip(ends) {
        s.end = last_code_line(text, s.start, end);
    }
    (symbols, comments)
}

/// The key a YAML line starts, if it starts one (`name:`, `"a b": 1`).
fn yaml_key(line: &str) -> Option<&str> {
    if line.starts_with(['-', '#', '&', '*', '!', '|', '>', '%', '{', '[']) || line == "---" || line == "..." {
        return None;
    }
    let (key, rest) = if let Some(q) = line.strip_prefix('"').or_else(|| line.strip_prefix('\'')) {
        let close = q.find(['"', '\''])?;
        (&q[..close], &q[close + 1..])
    } else {
        let colon = line.find(": ").or_else(|| line.ends_with(':').then(|| line.len() - 1))?;
        (&line[..colon], &line[colon..])
    };
    (rest.starts_with(':') && !key.is_empty() && !key.contains(' ') || line.starts_with(['"', '\''])).then_some(key.trim())
}

/// Where a YAML line's comment starts, if it has one (`#` at its start or after a space, outside quotes).
fn comment_at(line: &str) -> Option<usize> {
    let mut quote = None;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match (quote, b) {
            (None, b'"' | b'\'') => quote = Some(b),
            (Some(q), _) if b == q => quote = None,
            (None, b'#') if i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t' => return Some(i),
            _ => {}
        }
    }
    None
}

/// The last line from `start` to `end` (from 1) with something on it.
fn last_code_line(text: &str, start: usize, end: usize) -> usize {
    let lines: Vec<&str> = text.split('\n').collect();
    (start..=end.min(lines.len())).rev().find(|&l| !lines[l - 1].trim().is_empty() && !lines[l - 1].trim_start().starts_with('#')).unwrap_or(start)
}

/// SQL's `CREATE` statements, and its comments' byte ranges.
fn sql(text: &str) -> (Vec<Symbol>, Ranges) {
    let (statements, comments) = statements(text);
    let mut symbols = Vec::new();
    for (a, b) in statements {
        // The statement without its comments, for reading its words.
        let mut code = text[a..b].to_owned();
        for &(ca, cb) in comments.iter().filter(|&&(ca, _)| ca >= a && ca < b) {
            code.replace_range(ca - a..cb.min(b) - a, &" ".repeat(cb.min(b) - ca));
        }
        let Some((kind, name)) = created(&code) else { continue };
        let lead = code.len() - code.trim_start().len();
        let start = line_of(text, a + lead);
        let end = line_of(text, b.saturating_sub(1).max(a + lead));
        let head = code.trim_start();
        let cut = [" AS ", " as ", "$$", " BEGIN", " RETURNS"].iter().filter_map(|w| head.find(w)).min();
        let cut = if kind == Kind::Table { head.find('(').or(cut) } else { cut };
        let signature = head[..cut.unwrap_or(head.len())].trim();
        let doc = comments.iter().rev().find(|&&(_, cb)| cb <= a + lead && line_of(text, cb) + 1 >= start).map(|&(ca, cb)| strip_comment(&text[ca..cb]));
        let mut s = symbol(name, kind, start, end, signature, None);
        s.doc = doc.and_then(|d| d.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_owned));
        symbols.push(s);
    }
    (symbols, comments)
}

/// What a `CREATE` statement creates, and its name.
fn created(code: &str) -> Option<(Kind, String)> {
    let mut words = code.split_whitespace().map(|w| w.trim_end_matches('('));
    if !words.next()?.eq_ignore_ascii_case("create") {
        return None;
    }
    const SKIP: [&str; 10] = ["or", "replace", "temp", "temporary", "unlogged", "materialized", "global", "local", "definer", "recursive"];
    let mut word = words.next()?;
    while SKIP.iter().any(|s| word.eq_ignore_ascii_case(s)) || word.contains('=') {
        word = words.next()?;
    }
    let kind = match word.to_ascii_lowercase().as_str() {
        "table" | "view" => Kind::Table,
        "function" | "procedure" | "trigger" => Kind::Function,
        "type" | "domain" => Kind::Type,
        "schema" => Kind::Module,
        _ => return None,
    };
    let mut name = words.next()?;
    if name.eq_ignore_ascii_case("if") {
        // `IF NOT EXISTS`
        words.next();
        words.next();
        name = words.next()?;
    }
    let name = name.split('(').next().unwrap_or(name).trim_end_matches(';');
    let name: String = name.chars().filter(|c| !matches!(c, '"' | '`' | '[' | ']')).collect();
    (!name.is_empty()).then_some((kind, name))
}

/// SQL statements' byte ranges (ending at `;` outside strings, comments and
/// dollar quotes), and the comments' ranges.
fn statements(text: &str) -> (Ranges, Ranges) {
    let b = text.as_bytes();
    let (mut out, mut comments) = (Vec::new(), Vec::new());
    let (mut start, mut i) = (0, 0);
    while i < b.len() {
        match b[i] {
            b'-' if b.get(i + 1) == Some(&b'-') => {
                let end = text[i..].find('\n').map_or(b.len(), |e| i + e);
                comments.push((i, end));
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = text[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                comments.push((i, end));
                i = end;
            }
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                i += 1;
            }
            // `$$ … $$` or `$tag$ … $tag$`: a function body, `;`s and all
            // (its comments still comments).
            b'$' => {
                let tag_end = text[i + 1..].find(|c: char| !(c.is_alphanumeric() || c == '_')).map(|e| i + 1 + e);
                if let Some(t) = tag_end.filter(|&t| b[t] == b'$') {
                    let tag = &text[i..=t];
                    let body = text[t + 1..].find(tag).map_or(b.len(), |e| t + 1 + e);
                    body_comments(text, t + 1, body, &mut comments);
                    i = (body + tag.len()).min(b.len());
                } else {
                    i += 1;
                }
            }
            b';' => {
                out.push((start, i + 1));
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    if !text[start..].trim().is_empty() {
        out.push((start, b.len()));
    }
    (out, comments)
}

/// The comments' ranges in a dollar-quoted body, `text[a..b]`.
fn body_comments(text: &str, a: usize, b: usize, comments: &mut Ranges) {
    let bytes = text.as_bytes();
    let mut i = a;
    while i < b {
        match bytes[i] {
            b'-' if i + 1 < b && bytes[i + 1] == b'-' => {
                let end = text[i..b].find('\n').map_or(b, |e| i + e);
                comments.push((i, end));
                i = end;
            }
            b'/' if i + 1 < b && bytes[i + 1] == b'*' => {
                let end = text[i + 2..b].find("*/").map_or(b, |e| i + 2 + e + 2);
                comments.push((i, end));
                i = end;
            }
            q @ (b'\'' | b'"') => {
                i += 1;
                while i < b && bytes[i] != q {
                    i += 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
}

/// The line (from 1) holding byte `at`.
fn line_of(text: &str, at: usize) -> usize {
    text.as_bytes()[..at.min(text.len())].iter().filter(|&&c| c == b'\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_keys() {
        assert_eq!(yaml_key("name: CI"), Some("name"));
        assert_eq!(yaml_key("jobs:"), Some("jobs"));
        assert_eq!(yaml_key("\"on\": push"), Some("on"));
        assert_eq!(yaml_key("- run: x"), None);
        assert_eq!(yaml_key("http://x"), None);
        assert_eq!(comment_at("a: \"#x\" # c"), Some(8));
    }

    #[test]
    fn sql_statements() {
        let (s, c) = statements("create function f() as $$ begin; -- why\n'--'; end $$;\n-- note\nselect 1;");
        assert_eq!(s.len(), 2);
        // The body's comment counts; the `--` in its string doesn't.
        assert_eq!(c.len(), 2);
        assert_eq!(created("CREATE OR REPLACE VIEW public.\"active\" AS"), Some((Kind::Table, "public.active".into())));
        assert_eq!(created("create table if not exists users("), Some((Kind::Table, "users".into())));
        assert_eq!(created("create index i on t"), None);
    }
}
