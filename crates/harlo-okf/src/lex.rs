//! Masks a source file's comments and literals, so only its code is left.
//!
//! [`mask`] returns the text with the same bytes and lines, but every
//! comment, string and character literal blanked to spaces (newlines kept).
//! The scanner looks for declarations and matches braces on that text, so a
//! `{` in a string or `fn` in a comment can't mislead it. Each language
//! family knows its own literals: Rust's raw strings and lifetimes, Swift's
//! `\(…)` and JavaScript's `${…}` (code inside a string), Python's triple
//! quotes, C's preprocessor lines, Markdown's fenced code.
//!
//! The rules are the common forms of each language, not a full grammar: a
//! rare literal it misreads (a JavaScript regular expression after an
//! identifier, say) can shift a brace, and the scanner then reads that
//! file's declarations a little wrong, never panics.

// Rust guideline compliant 2026-10-09

use crate::units::Lang;

/// `text` with its comments and literals blanked, byte for byte.
pub fn mask(lang: Lang, text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let mut lexer = Lexer { b, lang, ranges: Vec::new() };
    match lang {
        Lang::Markdown => lexer.fences(),
        _ => {
            let mut i = 0;
            while i < b.len() {
                i = lexer.code(i, None);
                // (A stray closer at the top: step over it.)
                i += 1;
            }
        }
    }
    for (start, end) in lexer.ranges {
        for byte in &mut out[start..end.min(b.len())] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    out
}

struct Lexer<'a> {
    b: &'a [u8],
    lang: Lang,
    /// Byte ranges to blank.
    ranges: Vec<(usize, usize)>,
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

impl Lexer<'_> {
    fn at(&self, i: usize) -> u8 {
        self.b.get(i).copied().unwrap_or(0)
    }

    fn starts(&self, i: usize, s: &[u8]) -> bool {
        self.b.get(i..i + s.len()) == Some(s)
    }

    fn line_end(&self, i: usize) -> usize {
        self.b[i..].iter().position(|&c| c == b'\n').map_or(self.b.len(), |p| i + p)
    }

    /// Lexes code from `i`. With `until`, stops at the closer that ends an
    /// interpolation (`)` or `}` at depth 0) and returns its index;
    /// otherwise returns the end of the text, or a stray closer's index.
    fn code(&mut self, mut i: usize, until: Option<u8>) -> usize {
        let opener = match until {
            Some(b')') => b'(',
            Some(b'}') => b'{',
            _ => 0,
        };
        let mut depth = 0usize;
        let hash_comments = matches!(self.lang, Lang::Python | Lang::Toml);
        let slash_comments = !hash_comments;
        while i < self.b.len() {
            let c = self.b[i];
            if until.is_some() && c == opener {
                depth += 1;
            } else if until == Some(c) {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
            }
            if slash_comments && c == b'/' && self.at(i + 1) == b'/' {
                let end = self.line_end(i);
                self.ranges.push((i, end));
                i = end;
                continue;
            }
            if slash_comments && c == b'/' && self.at(i + 1) == b'*' {
                let end = self.block_comment(i);
                self.ranges.push((i, end));
                i = end;
                continue;
            }
            if hash_comments && c == b'#' {
                let end = self.line_end(i);
                self.ranges.push((i, end));
                i = end;
                continue;
            }
            if self.lang == Lang::C && c == b'#' && self.line_starts_at(i) {
                let end = self.preprocessor(i);
                self.ranges.push((i, end));
                i = end;
                continue;
            }
            if let Some(end) = self.literal(i) {
                self.ranges.push((i, end));
                i = end;
                continue;
            }
            i += 1;
        }
        i
    }

    /// Whether only spaces come before `i` on its line.
    fn line_starts_at(&self, i: usize) -> bool {
        self.b[..i].iter().rev().take_while(|&&c| c != b'\n').all(u8::is_ascii_whitespace)
    }

    /// The end of a `/* */` comment (nested in Rust and Swift).
    fn block_comment(&self, start: usize) -> usize {
        let nests = matches!(self.lang, Lang::Rust | Lang::Swift);
        let mut depth = 0usize;
        let mut i = start;
        while i < self.b.len() {
            if self.starts(i, b"/*") {
                depth += 1;
                i += 2;
                if !nests && depth > 1 {
                    depth = 1;
                }
            } else if self.starts(i, b"*/") {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
            } else {
                i += 1;
            }
        }
        self.b.len()
    }

    /// A preprocessor line and its `\` continuations.
    fn preprocessor(&self, mut i: usize) -> usize {
        loop {
            let end = self.line_end(i);
            if end > 0 && self.b[..end].ends_with(b"\\") && end < self.b.len() {
                i = end + 1;
            } else {
                return end;
            }
        }
    }

    /// If a literal starts at `i`, where it ends.
    fn literal(&mut self, i: usize) -> Option<usize> {
        let c = self.b[i];
        let prev = if i > 0 { self.b[i - 1] } else { b' ' };
        match self.lang {
            Lang::Rust => {
                if is_ident(prev) && !matches!(prev, b'b' | b'r' | b'c') {
                    return None;
                }
                // Raw strings: r"…", r#"…"#, br"…", cr#"…"#.
                let mut j = i;
                if matches!(c, b'b' | b'c') && self.at(j + 1) == b'r' {
                    j += 1;
                }
                if self.at(j) == b'r' && !is_ident(prev) {
                    let hashes = self.b[j + 1..].iter().take_while(|&&h| h == b'#').count();
                    if self.at(j + 1 + hashes) == b'"' {
                        let mut close = vec![b'"'];
                        close.extend(std::iter::repeat_n(b'#', hashes));
                        return Some(self.find(j + 2 + hashes, &close));
                    }
                }
                if c == b'"' {
                    return Some(self.quoted(i + 1, b'"'));
                }
                if matches!(c, b'b' | b'c') && self.at(i + 1) == b'"' && !is_ident(prev) {
                    return Some(self.quoted(i + 2, b'"'));
                }
                if c == b'\'' {
                    return self.rust_char(i);
                }
                None
            }
            Lang::Python => {
                // String prefixes (r, b, f, u and pairs) belong to the literal.
                let quote_at = if c.is_ascii_alphabetic() && !is_ident(prev) {
                    let n = self.b[i..].iter().take_while(|h| matches!(h.to_ascii_lowercase(), b'r' | b'b' | b'f' | b'u')).count();
                    (n <= 2 && matches!(self.at(i + n), b'"' | b'\'')).then_some(i + n)?
                } else if matches!(c, b'"' | b'\'') {
                    i
                } else {
                    return None;
                };
                let q = self.b[quote_at];
                if self.at(quote_at + 1) == q && self.at(quote_at + 2) == q {
                    return Some(self.find(quote_at + 3, &[q, q, q]));
                }
                Some(self.quoted(quote_at + 1, q))
            }
            Lang::Toml => match c {
                b'"' | b'\'' if self.at(i + 1) == c && self.at(i + 2) == c => Some(self.find(i + 3, &[c, c, c])),
                b'"' => Some(self.quoted(i + 1, b'"')),
                b'\'' => Some(self.find(i + 1, b"'")),
                _ => None,
            },
            Lang::Go => match c {
                b'"' | b'\'' => Some(self.quoted(i + 1, c)),
                b'`' => Some(self.find(i + 1, b"`")),
                _ => None,
            },
            Lang::C => match c {
                b'"' | b'\'' => Some(self.quoted(i + 1, c)),
                _ => None,
            },
            Lang::Swift => match c {
                b'"' if self.starts(i, b"\"\"\"") => Some(self.swift_string(i + 3, true)),
                b'"' => Some(self.swift_string(i + 1, false)),
                b'#' if self.at(i + 1) == b'"' => Some(self.find(i + 2, b"\"#")),
                _ => None,
            },
            Lang::Script | Lang::Qml => match c {
                b'"' | b'\'' => Some(self.quoted(i + 1, c)),
                b'`' => Some(self.template(i + 1)),
                b'/' if self.regex_can_start(i) => Some(self.regex(i + 1)),
                _ => None,
            },
            Lang::Markdown => None,
        }
    }

    /// The end of `needle`'s first match from `i` (or the end of the text).
    fn find(&self, i: usize, needle: &[u8]) -> usize {
        let i = i.min(self.b.len());
        self.b[i..].windows(needle.len()).position(|w| w == needle).map_or(self.b.len(), |p| i + p + needle.len())
    }

    /// The end of a one-line string with `\` escapes, closed by `q`.
    fn quoted(&self, mut i: usize, q: u8) -> usize {
        while i < self.b.len() {
            match self.b[i] {
                b'\\' => i += 2,
                c if c == q => return i + 1,
                // (Unclosed at the line's end: the literal ends there.)
                b'\n' if q != b'"' || self.lang != Lang::Rust => return i,
                _ => i += 1,
            }
        }
        self.b.len()
    }

    /// A Rust character literal at `i`, or None for a lifetime or label.
    fn rust_char(&self, i: usize) -> Option<usize> {
        if self.at(i + 1) == b'\\' {
            return Some(self.find(i + 2, b"'"));
        }
        // One character (one to four UTF-8 bytes) and the closing quote.
        let first = self.at(i + 1);
        let width = match first {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            _ => 4,
        };
        (first != b'\n' && self.at(i + 1 + width) == b'\'').then_some(i + 2 + width)
    }

    /// The end of a Swift string, its `\(…)` parts lexed as code.
    fn swift_string(&mut self, mut i: usize, multiline: bool) -> usize {
        while i < self.b.len() {
            match self.b[i] {
                b'\\' if self.at(i + 1) == b'(' => {
                    i = self.code(i + 2, Some(b')')) + 1;
                }
                b'\\' => i += 2,
                b'"' if multiline && self.starts(i, b"\"\"\"") => return i + 3,
                b'"' if !multiline => return i + 1,
                b'\n' if !multiline => return i,
                _ => i += 1,
            }
        }
        self.b.len()
    }

    /// The end of a JavaScript template literal, its `${…}` parts lexed as code.
    fn template(&mut self, mut i: usize) -> usize {
        while i < self.b.len() {
            match self.b[i] {
                b'\\' => i += 2,
                b'$' if self.at(i + 1) == b'{' => {
                    i = self.code(i + 2, Some(b'}')) + 1;
                }
                b'`' => return i + 1,
                _ => i += 1,
            }
        }
        self.b.len()
    }

    /// Whether a `/` at `i` starts a regular expression: after an operator,
    /// an opening bracket, `return`, or at a line's start; never `//`, `/*`.
    fn regex_can_start(&self, i: usize) -> bool {
        if matches!(self.at(i + 1), b'/' | b'*') {
            return false;
        }
        let before = self.b[..i].iter().rev().find(|c| !matches!(c, b' ' | b'\t')).copied();
        match before {
            None | Some(b'\n' | b'(' | b',' | b'=' | b':' | b'[' | b'!' | b'&' | b'|' | b'?' | b'{' | b'}' | b';' | b'+' | b'-' | b'*' | b'%' | b'<' | b'>' | b'~' | b'^') => true,
            Some(_) => self.b[..i].trim_ascii_end().ends_with(b"return") || self.b[..i].trim_ascii_end().ends_with(b"typeof"),
        }
    }

    /// The end of a regular expression literal (its `[…]` classes may hold `/`), flags included.
    fn regex(&self, mut i: usize) -> usize {
        let mut class = false;
        while i < self.b.len() {
            match self.b[i] {
                b'\\' => i += 2,
                b'[' => {
                    class = true;
                    i += 1;
                }
                b']' => {
                    class = false;
                    i += 1;
                }
                b'/' if !class => {
                    i += 1;
                    while i < self.b.len() && self.b[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    return i;
                }
                b'\n' => return i,
                _ => i += 1,
            }
        }
        self.b.len()
    }

    /// Markdown: the fenced code blocks (``` or ~~~), fences included.
    fn fences(&mut self) {
        let mut i = 0;
        let mut open: Option<(usize, u8, usize)> = None;
        while i < self.b.len() {
            let end = self.line_end(i);
            let line = &self.b[i..end];
            let trimmed = line.trim_ascii_start();
            let indent = line.len() - trimmed.len();
            let fence_char = trimmed.first().copied().filter(|c| matches!(c, b'`' | b'~'));
            let run = fence_char.map_or(0, |f| trimmed.iter().take_while(|&&c| c == f).count());
            match open {
                None if indent < 4 && run >= 3 => open = Some((i, fence_char.unwrap_or(b'`'), run)),
                Some((start, f, n)) if fence_char == Some(f) && run >= n && trimmed[run..].trim_ascii().is_empty() => {
                    self.ranges.push((start, end));
                    open = None;
                }
                _ => {}
            }
            i = end + 1;
        }
        if let Some((start, _, _)) = open {
            self.ranges.push((start, self.b.len()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masked(lang: Lang, text: &str) -> String {
        String::from_utf8(mask(lang, text)).unwrap()
    }

    #[test]
    fn rust_literals_and_lifetimes() {
        let src = "fn a<'x>(s: &'x str) -> char { let _ = \"{\"; let _ = r#\"}\"#; '{' } // }\n/* { /* } */ } */ fn b() {}";
        let m = masked(Lang::Rust, src);
        assert_eq!(m.matches('{').count(), m.matches('}').count(), "{m}");
        assert!(m.contains("fn a<'x>(s: &'x str)") && m.contains("fn b() {}"), "{m}");
        assert_eq!(m.len(), src.len());
    }

    #[test]
    fn interpolations_are_masked_whole() {
        let swift = "Text(\"\\(n > 1 ? \"{\" : \"}\") items\") { }";
        let literal = swift.find(") {").unwrap() - 5;
        assert_eq!(masked(Lang::Swift, swift), format!("Text({}) {{ }}", " ".repeat(literal)));
        let js = "const s = `a ${ {b: \"}\"}.b } c`; function f() {}";
        let m = masked(Lang::Script, js);
        assert!(m.starts_with("const s = ") && m.ends_with("; function f() {}"), "{m}");
        assert!(!m[..m.len() - 3].contains('{'), "{m}");
    }

    #[test]
    fn python_c_and_markdown() {
        let py = "def f(x=\"{\"):  # {\n    s = '''\n}\n'''\n";
        let m = masked(Lang::Python, py);
        assert!(!m.contains('{') && !m.contains('}') && m.starts_with("def f(x=   ):"), "{m}");
        let c = "#define OPEN {\\\n }\nint main(void) { return '}'; }\n";
        let m = masked(Lang::C, c);
        assert_eq!((m.matches('{').count(), m.matches('}').count()), (1, 1), "{m}");
        let md = "# Title\n```sh\n# not a heading\n```\n## Part\n";
        let m = masked(Lang::Markdown, md);
        assert_eq!(m.lines().filter(|l| l.starts_with('#')).count(), 2, "{m}");
    }

    #[test]
    fn regex_and_division() {
        let js = "const r = /[{}\\/]/g; const half = a / 2; if (x) { y(); }";
        let m = masked(Lang::Script, js);
        assert_eq!((m.matches('{').count(), m.matches('}').count()), (1, 1), "{m}");
        assert!(m.contains("a / 2"), "{m}");
    }
}
