//! A large file's outline: its definitions (types, functions, methods,
//! headings) with their line numbers, read from the text alone (no
//! language server: a `read` doesn't start one). A `read` of a file too big
//! to show whole returns it instead of the file's first lines
//! (`HARLO_AGENT_OUTLINE=off`: the first lines). Measured: a bug three
//! calls deep in a 4,900-line module found with 41% fewer tokens; 19% fewer
//! over both tasks with big files.

use std::path::Path;

/// How much outline a read returns, at most.
const MAX_BYTES: usize = 12 * 1024;
/// A definition's line is cut here.
const LINE_CHARS: usize = 120;
/// Fewer definitions than this: not worth an outline.
const MIN_ENTRIES: usize = 3;

pub fn on() -> bool {
    std::env::var("HARLO_AGENT_OUTLINE").map_or(true, |v| v.trim() != "off")
}

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    Rust,
    Python,
    Go,
    Script,
    C,
    Markdown,
}

fn lang(path: &Path) -> Option<Lang> {
    Some(match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "rs" => Lang::Rust,
        "py" | "pyi" => Lang::Python,
        "go" => Lang::Go,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Lang::Script,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Lang::C,
        "md" | "markdown" => Lang::Markdown,
        _ => return None,
    })
}

/// `s` without any of `prefixes` at its start (each word as often as it's
/// there, in any order).
fn strip_words<'a>(mut s: &'a str, words: &[&str]) -> &'a str {
    loop {
        let before = s;
        for w in words {
            if let Some(rest) = s.strip_prefix(w) {
                s = rest.trim_start();
            }
        }
        if s == before {
            return s;
        }
    }
}

fn ident_start(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
}

/// Whether the line (`t`: trimmed, `indent`: its leading width) starts a
/// definition worth listing.
fn defines(lang: Lang, t: &str, indent: usize, next: &str) -> bool {
    match lang {
        Lang::Rust => {
            let mut s = t;
            if let Some(rest) = s.strip_prefix("pub(") {
                s = rest.split_once(')').map_or(rest, |(_, r)| r).trim_start();
            }
            let s = strip_words(s, &["pub ", "async ", "unsafe ", "default ", "extern \"C\" ", "extern "]);
            if let Some(rest) = s.strip_prefix("const ") {
                // (`const fn`, or a constant at the top.)
                return rest.starts_with("fn ") || indent == 0;
            }
            ["fn ", "struct ", "enum ", "trait ", "impl ", "impl<", "mod ", "type ", "union ", "macro_rules!"].iter().any(|k| s.starts_with(k)) || (indent == 0 && s.starts_with("static "))
        }
        Lang::Python => t.starts_with("def ") || t.starts_with("async def ") || t.starts_with("class "),
        Lang::Go => indent == 0 && (t.starts_with("func ") || t.starts_with("type ")),
        Lang::Script => {
            let s = strip_words(t, &["export ", "default ", "declare ", "abstract "]);
            if ["function ", "function*", "async function", "class ", "interface ", "enum ", "namespace ", "module "].iter().any(|k| s.starts_with(k)) {
                return true;
            }
            if s.starts_with("type ") && s.contains('=') {
                return true;
            }
            // A function given a name at the top: `const name = (…) =>`, `= function`.
            if indent == 0 && (s.starts_with("const ") || s.starts_with("let ")) && (s.contains("=>") || s.contains("= function") || s.contains("= async")) {
                return true;
            }
            // A method in a class: `name(…) {` (not a call or a keyword).
            if indent > 0 && t.ends_with('{') {
                let m = strip_words(t, &["public ", "private ", "protected ", "static ", "async ", "get ", "set ", "readonly ", "override "]);
                let name: String = m.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '#').collect();
                let after = m[name.len()..].trim_start();
                return !name.is_empty()
                    && ident_start(&name)
                    && after.starts_with('(')
                    && !["if", "for", "while", "switch", "catch", "return", "function", "with", "else"].contains(&name.as_str());
            }
            false
        }
        Lang::C => {
            if indent > 0 {
                return false;
            }
            let s = strip_words(t, &["typedef ", "template<", "export "]);
            if ["struct ", "class ", "enum ", "union ", "namespace "].iter().any(|k| s.starts_with(k)) && !s.ends_with(';') {
                return true;
            }
            // A function's definition: a name and its arguments, its body
            // opening on this line or the next.
            let opens = t.ends_with('{') || (t.ends_with(')') && next.trim_start().starts_with('{'));
            opens && ident_start(t) && t.contains('(') && !t.starts_with("#") && !["if", "for", "while", "switch", "return", "else", "do"].iter().any(|k| t.starts_with(&format!("{k} ")) || t.starts_with(&format!("{k}(")))
        }
        // (Indented four spaces, it's code.)
        Lang::Markdown => indent < 4 && t.starts_with('#') && t.trim_start_matches('#').starts_with(' '),
    }
}

/// The outline of `text` (a file at `path`), as `line: definition` rows,
/// or None: not a language it knows, or too few definitions.
pub fn of(path: &Path, text: &str) -> Option<String> {
    let lang = lang(path)?;
    let lines: Vec<&str> = text.lines().collect();
    let mut entries: Vec<(usize, usize, String)> = Vec::new();
    let mut fence = false;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if lang == Lang::Markdown && t.starts_with("```") {
            fence = !fence;
            continue;
        }
        if t.is_empty() || fence {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let next = lines.get(i + 1).copied().unwrap_or_default();
        if !defines(lang, t, indent, next) {
            continue;
        }
        let (indent, shown) = if lang == Lang::Markdown {
            // A heading's level is its depth, its words without their links.
            let level = t.len() - t.trim_start_matches('#').len();
            (level, format!("{} {}", "#".repeat(level), plain_heading(&t[level..])))
        } else {
            (indent, line.trim_end().replace('\t', "    "))
        };
        let shown = if shown.chars().count() > LINE_CHARS { format!("{}…", shown.chars().take(LINE_CHARS).collect::<String>()) } else { shown };
        entries.push((i + 1, indent, shown));
    }
    if entries.len() < MIN_ENTRIES {
        return None;
    }
    let render = |rows: &[&(usize, usize, String)]| rows.iter().map(|(n, _, l)| format!("{n}: {l}")).collect::<Vec<_>>().join("\n");
    // Too long: the outer levels only, then cut.
    let mut depths: Vec<usize> = entries.iter().map(|e| e.1).collect();
    depths.sort_unstable();
    depths.dedup();
    for keep in depths.iter().rev() {
        let rows: Vec<&(usize, usize, String)> = entries.iter().filter(|e| e.1 <= *keep).collect();
        let out = render(&rows);
        if out.len() <= MAX_BYTES {
            let left = entries.len() - rows.len();
            let what = if lang == Lang::Markdown { "deeper headings" } else { "nested definitions" };
            return Some(if left > 0 { format!("{out}\n[{left} {what} not shown: read a part to see them]") } else { out });
        }
    }
    let top: Vec<&(usize, usize, String)> = entries.iter().filter(|e| e.1 == depths[0]).collect();
    let mut out = String::new();
    let mut shown = 0;
    for (n, _, l) in &top {
        let row = format!("{n}: {l}\n");
        if out.len() + row.len() > MAX_BYTES {
            break;
        }
        out.push_str(&row);
        shown += 1;
    }
    Some(format!("{}[{} more definitions not shown: grep for what you need]", out, entries.len() - shown))
}

/// A Markdown heading's words: links as their text, anchor links (`¶`,
/// `§`, none) dropped, escapes undone.
fn plain_heading(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '\\' if i + 1 < c.len() && c[i + 1].is_ascii_punctuation() => {
                out.push(c[i + 1]);
                i += 2;
                continue;
            }
            '[' => {
                // [words](target): the words, read on for links inside.
                if let Some(close) = closing(&c, i, '[', ']')
                    && c.get(close + 1) == Some(&'(')
                    && let Some(end) = closing(&c, close + 1, '(', ')')
                {
                    let words = plain_heading(&c[i + 1..close].iter().collect::<String>());
                    if !matches!(words.trim(), "" | "¶" | "§" | "#") {
                        out.push_str(&words);
                    }
                    i = end + 1;
                    continue;
                }
            }
            _ => {}
        }
        out.push(c[i]);
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Where the bracket opened at `at` closes (escapes skipped).
fn closing(c: &[char], at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    let mut i = at;
    while i < c.len() {
        if c[i] == '\\' {
            i += 2;
            continue;
        }
        if c[i] == open {
            depth += 1;
        } else if c[i] == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlines_rust() {
        let text = "use std::fmt;\n\npub const LIMIT: usize = 3;\n\n/// A thing.\n#[derive(Debug)]\npub struct Ledger {\n    pub id: u32,\n}\n\nimpl Ledger {\n    pub fn post(&self) -> u32 {\n        let x = 1;\n        x\n    }\n\n    pub(crate) async fn sync(&mut self) {}\n    const fn zero() -> u32 { 0 }\n}\n\nimpl fmt::Display for Ledger {\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) }\n}\n\nfn helper() {\n    const INNER: u32 = 2;\n}\n";
        let o = of(Path::new("src/lib.rs"), text).unwrap();
        assert_eq!(
            o,
            "3: pub const LIMIT: usize = 3;\n7: pub struct Ledger {\n11: impl Ledger {\n12:     pub fn post(&self) -> u32 {\n17:     pub(crate) async fn sync(&mut self) {}\n18:     const fn zero() -> u32 { 0 }\n21: impl fmt::Display for Ledger {\n22:     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) }\n25: fn helper() {"
        );
    }

    #[test]
    fn outlines_python_and_script() {
        let py = "import os\n\nclass Cart:\n    def total(self):\n        return 0\n\n    async def sync(self):\n        pass\n\ndef main():\n    x = Cart()\n";
        assert_eq!(of(Path::new("shop.py"), py).unwrap(), "3: class Cart:\n4:     def total(self):\n7:     async def sync(self):\n10: def main():");
        let ts = "import x from 'y';\n\nexport interface Props {\n  a: string;\n}\n\nexport const handler = async (req) => {\n  if (req) {\n    go();\n  }\n};\n\nexport default class View {\n  render() {\n    return null;\n  }\n  private static async load(id: string) {\n  }\n}\n";
        assert_eq!(
            of(Path::new("view.tsx"), ts).unwrap(),
            "3: export interface Props {\n7: export const handler = async (req) => {\n13: export default class View {\n14:   render() {\n17:   private static async load(id: string) {"
        );
    }

    #[test]
    fn outlines_c_and_markdown() {
        let c = "#include <stdio.h>\n\nstruct point {\n    int x;\n};\n\nstatic int add(int a, int b)\n{\n    if (a) {\n        return a + b;\n    }\n    return b;\n}\n\nint main(void) {\n    return add(1, 2);\n}\n";
        assert_eq!(of(Path::new("m.c"), c).unwrap(), "3: struct point {\n7: static int add(int a, int b)\n15: int main(void) {");
        let md = "# Title\n\nText.\n\n```sh\n# not a heading\n```\n\n## Part\n\n    # code, indented\n\n### Detail\n";
        assert_eq!(of(Path::new("README.md"), md).unwrap(), "1: # Title\n9: ## Part\n13: ### Detail");
        // Headings as words: links as their text, anchors dropped.
        let doc = "## [§](#examples)Examples\n## Numeric Types — [`int`](https://x/int), [`float`](#f)[¶](#numeric)\n#### pub fn [with\\_capacity](#m)(n: [usize](https://u)) -> [Vec](https://v)\\<T>\n";
        assert_eq!(of(Path::new("page.md"), doc).unwrap(), "1: ## Examples\n2: ## Numeric Types — `int`, `float`\n3: #### pub fn with_capacity(n: usize) -> Vec<T>");
    }

    #[test]
    fn keeps_to_the_outer_levels_when_long() {
        let mut py = String::new();
        for c in 0..120 {
            py.push_str(&format!("class Rule{c}:\n"));
            for m in 0..10 {
                py.push_str(&format!("    def method_number_{m}_of_rule_{c}(self, value, other_value):\n        return value\n"));
            }
        }
        let o = of(Path::new("rules.py"), &py).unwrap();
        assert!(o.starts_with("1: class Rule0:\n22: class Rule1:\n") && o.ends_with("[1200 nested definitions not shown: read a part to see them]"), "{}", &o[o.len() - 200..]);
        assert!(o.len() <= MAX_BYTES + 100);
        // Unknown languages, and too little to outline.
        assert_eq!(of(Path::new("data.csv"), &py), None);
        assert_eq!(of(Path::new("a.py"), "def one():\n    pass\n"), None);
    }
}
