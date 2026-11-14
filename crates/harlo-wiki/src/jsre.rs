//! JavaScript regular expressions, run the way JavaScript runs them: the
//! patterns OpenWiki's Markdown parser uses are kept in their JavaScript form
//! and translated here, so the two match the same text.
//!
//! What differs and is translated: `\s` is JavaScript's whitespace set,
//! `\w`, `\d` and `\b` are ASCII, `.` doesn't match line terminators, `\/`
//! is a slash, and inside a class `[`, `&` and `~` are plain characters.

use fancy_regex::{Captures, Regex, RegexBuilder};

const WS: &str = r"\t\n\x0B\x0C\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";
const WORD: &str = "0-9A-Za-z_";

/// A JavaScript pattern's Rust equivalent.
pub fn translate(src: &str, flags: &str) -> String {
    let mut out = String::new();
    if flags.contains('i') {
        out.push_str("(?i)");
    }
    if flags.contains('m') {
        out.push_str("(?m)");
    }
    if flags.contains('s') {
        out.push_str("(?s)");
    }
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut class = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            i += 2;
            match (n, class) {
                ('s', false) => out.push_str(&format!("[{WS}]")),
                ('s', true) => out.push_str(WS),
                ('S', false) => out.push_str(&format!("[^{WS}]")),
                ('S', true) => out.push_str(&format!("[^{WS}]")),
                ('w', false) => out.push_str(&format!("[{WORD}]")),
                ('w', true) => out.push_str(WORD),
                ('W', false) => out.push_str(&format!("[^{WORD}]")),
                ('W', true) => out.push_str(&format!("[^{WORD}]")),
                ('d', false) => out.push_str("[0-9]"),
                ('d', true) => out.push_str("0-9"),
                ('D', _) => out.push_str("[^0-9]"),
                ('b', false) => out.push_str(r"(?:(?<=[0-9A-Za-z_])(?![0-9A-Za-z_])|(?<![0-9A-Za-z_])(?=[0-9A-Za-z_]))"),
                ('B', false) => out.push_str(r"(?:(?<=[0-9A-Za-z_])(?=[0-9A-Za-z_])|(?<![0-9A-Za-z_])(?![0-9A-Za-z_]))"),
                ('/', _) => out.push('/'),
                ('x', _) => {
                    let hex: String = chars[i..(i + 2).min(chars.len())].iter().collect();
                    i += 2;
                    out.push_str(&format!("\\x{{{hex}}}"));
                }
                ('u', _) if i + 4 <= chars.len() && chars[i] != '{' => {
                    let hex: String = chars[i..i + 4].iter().collect();
                    i += 4;
                    out.push_str(&format!("\\x{{{hex}}}"));
                }
                (n, _) => {
                    out.push('\\');
                    out.push(n);
                }
            }
            continue;
        }
        i += 1;
        if class {
            match c {
                ']' => {
                    class = false;
                    out.push(']');
                }
                '[' | '&' | '~' => {
                    out.push('\\');
                    out.push(c);
                }
                _ => out.push(c),
            }
            continue;
        }
        match c {
            '[' => {
                class = true;
                out.push('[');
                // `[]` and `[^]` are JavaScript's "nothing" and "anything".
                if chars.get(i) == Some(&'^') {
                    out.push('^');
                    i += 1;
                }
                if chars.get(i) == Some(&']') {
                    i += 1;
                    class = false;
                    if out.ends_with("[^") {
                        out.truncate(out.len() - 2);
                        out.push_str(r"[\s\S]");
                    } else {
                        out.truncate(out.len() - 1);
                        out.push_str(r"[^\s\S]");
                    }
                }
            }
            '.' => out.push_str(r"[^\n\r\u{2028}\u{2029}]"),
            _ => out.push(c),
        }
    }
    out
}

/// A compiled JavaScript pattern.
#[derive(Clone)]
pub struct Re {
    re: Regex,
}

/// One match: its groups (None when a group didn't take part).
pub struct Match<'t> {
    caps: Captures<'t>,
}

impl<'t> Match<'t> {
    pub fn get(&self, i: usize) -> Option<&'t str> {
        self.caps.get(i).map(|m| m.as_str())
    }
    /// A group's text, "" when it didn't take part (JavaScript's `cap[i] || ''`).
    pub fn str(&self, i: usize) -> &'t str {
        self.get(i).unwrap_or("")
    }
    pub fn has(&self, i: usize) -> bool {
        self.get(i).is_some_and(|s| !s.is_empty())
    }
    pub fn whole(&self) -> &'t str {
        self.str(0)
    }
    pub fn start(&self) -> usize {
        self.caps.get(0).unwrap().start()
    }
    pub fn end(&self) -> usize {
        self.caps.get(0).unwrap().end()
    }
    pub fn name(&self, n: &str) -> Option<&'t str> {
        self.caps.name(n).map(|m| m.as_str())
    }
}

impl Re {
    pub fn new(src: &str, flags: &str) -> Re {
        let t = translate(src, flags);
        let re = RegexBuilder::new(&t)
            .backtrack_limit(50_000_000)
            .build()
            .unwrap_or_else(|e| panic!("pattern {src:?} ({t:?}): {e}"));
        Re { re }
    }

    /// The first match anywhere (JavaScript's `exec` without `g`).
    pub fn exec<'t>(&self, s: &'t str) -> Option<Match<'t>> {
        self.exec_at(s, 0)
    }

    /// The first match at or after byte `pos` (a `g` pattern's `exec` with
    /// `lastIndex` = pos).
    pub fn exec_at<'t>(&self, s: &'t str, pos: usize) -> Option<Match<'t>> {
        if pos > s.len() {
            return None;
        }
        match self.re.captures_from_pos(s, pos) {
            Ok(Some(caps)) => Some(Match { caps }),
            Ok(None) => None,
            // (Too much backtracking: JavaScript would have taken its time;
            // here it counts as no match.)
            Err(_) => None,
        }
    }

    pub fn test(&self, s: &str) -> bool {
        self.exec(s).is_some()
    }

    /// Every match, left to right (a `g` pattern), an empty match moving
    /// on by one character.
    pub fn all<'t>(&self, s: &'t str) -> Vec<Match<'t>> {
        let mut out = Vec::new();
        let mut pos = 0;
        while let Some(m) = self.exec_at(s, pos) {
            pos = if m.end() == m.start() { next_char(s, m.end()) } else { m.end() };
            out.push(m);
            if pos > s.len() {
                break;
            }
        }
        out
    }

    /// `s.replace(re, f)`: every match with `g`, else the first.
    pub fn replace(&self, s: &str, global: bool, mut f: impl FnMut(&Match) -> String) -> String {
        let mut out = String::new();
        let mut last = 0;
        let matches = if global { self.all(s) } else { self.exec(s).into_iter().collect() };
        for m in &matches {
            out.push_str(&s[last..m.start()]);
            out.push_str(&f(m));
            last = m.end();
        }
        out.push_str(&s[last..]);
        out
    }

    /// The first match's byte position (`s.search(re)`), -1 for none.
    pub fn search(&self, s: &str) -> isize {
        self.exec(s).map(|m| m.start() as isize).unwrap_or(-1)
    }
}

fn next_char(s: &str, i: usize) -> usize {
    s[i..].chars().next().map(|c| i + c.len_utf8()).unwrap_or(s.len() + 1)
}

/// JavaScript's `trim()`: its whitespace and line terminators.
pub fn is_js_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
        | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_space)
}

pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_js_space)
}

/// A string's JavaScript length (UTF-16 code units).
pub fn len16(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The byte offset `n` UTF-16 units into `s` (clamped; a split surrogate
/// pair keeps its whole character out).
pub fn at16(s: &str, n: usize) -> usize {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units + c.len_utf16() > n {
            return i;
        }
        units += c.len_utf16();
    }
    s.len()
}

/// `s.slice(a, b)` in UTF-16 units, negative counting from the end.
pub fn slice16(s: &str, a: isize, b: Option<isize>) -> &str {
    let len = len16(s) as isize;
    let fix = |x: isize| if x < 0 { (len + x).max(0) } else { x.min(len) } as usize;
    let a = fix(a);
    let b = b.map(fix).unwrap_or(len as usize);
    if b <= a {
        return "";
    }
    &s[at16(s, a)..at16(s, b)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_compile_and_match_like_javascript() {
        let r = Re::new(r"^ {0,3}(#{1,6})(?=\s|$)(.*)(?:\n+|$)", "");
        let m = r.exec("## Title\n\nx").unwrap();
        assert_eq!((m.str(1), m.str(2), m.whole()), ("##", " Title", "## Title\n\n"));
        assert!(Re::new(r"[\s\S]", "").test("\n"));
        assert!(!Re::new(r"^.$", "").test("\u{2028}"));
        assert!(Re::new(r"^\w+$", "").test("ab_9"));
        assert!(!Re::new(r"^\w+$", "").test("é"));
        assert!(Re::new(r"[a-z&~]", "").test("~"));
        assert!(Re::new(r"^<a ", "i").test("<A "));
        assert!(Re::new(r"^\s$", "").test("\u{feff}"));
        assert!(!Re::new(r"^\s$", "").test("\u{85}"));
        let fences = Re::new(r"^ {0,3}(`{3,}(?=[^`\n]*(?:\n|$))|~{3,})([^\n]*)(?:\n|$)(?:|([\s\S]*?)(?:\n|$))(?: {0,3}(?:(?=`)\1`*|(?=~)\1~*) *(?=\n|$)|$)", "");
        assert_eq!(fences.exec("```\nx\n```\nafter").unwrap().whole(), "```\nx\n```");
        assert_eq!(slice16("a😀b", 1, Some(3)), "😀");
        assert_eq!(len16("😀"), 2);
    }
}
