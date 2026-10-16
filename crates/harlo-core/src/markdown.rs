//! Markdown → top-level blocks of Qt rich text.
//!
//! Each top-level block (paragraph, list, table, code fence, …) becomes its
//! own transcript row, so a streaming message only re-renders its trailing
//! block, and long messages stay virtualized. Rendered blocks are cached by
//! source text, so unchanged blocks keep the same `Arc` (cheap equality in
//! the row diff) and code is highlighted once.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings};
use syntect::parsing::SyntaxSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdBlockKind {
    Prose,
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdBlock {
    pub kind: MdBlockKind,
    /// Qt rich-text HTML.
    pub html: String,
    /// Raw code for code blocks (copy button); empty otherwise.
    pub code: String,
    /// Code-fence language; empty otherwise.
    pub lang: String,
}

const CACHE_LIMIT: usize = 8192;
/// Text held back while streaming before a sentence boundary is revealed.
const STREAM_HOLD_CHARS: usize = 320;

#[derive(Default)]
pub struct MarkdownRenderer {
    cache: HashMap<u64, Arc<MdBlock>>,
}

impl MarkdownRenderer {
    pub fn render(&mut self, text: &str) -> Vec<Arc<MdBlock>> {
        if self.cache.len() > CACHE_LIMIT {
            self.cache.clear();
        }
        let mut out = Vec::new();
        let mut depth = 0usize;
        let mut events: Vec<Event<'_>> = Vec::new();
        let mut start = 0usize;
        for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
            if depth == 0 {
                start = range.start;
            }
            let end = range.end;
            match &event {
                Event::Start(_) => depth += 1,
                Event::End(_) => depth = depth.saturating_sub(1),
                _ => {}
            }
            events.push(event);
            if depth == 0 {
                let key = hash_source(&text[start..end.max(start)]);
                let block = self
                    .cache
                    .entry(key)
                    .or_insert_with(|| Arc::new(render_block(std::mem::take(&mut events))))
                    .clone();
                events.clear();
                out.push(block);
            }
        }
        out
    }
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
}

fn hash_source(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn render_block(events: Vec<Event<'_>>) -> MdBlock {
    if let Some(Event::Start(Tag::CodeBlock(kind))) = events.first() {
        let lang = match kind {
            CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or_default().to_owned(),
            CodeBlockKind::Indented => String::new(),
        };
        let code: String = events
            .iter()
            .filter_map(|e| match e {
                Event::Text(t) => Some(t.as_ref()),
                _ => None,
            })
            .collect();
        let code = code.strip_suffix('\n').unwrap_or(&code).to_owned();
        return MdBlock { kind: MdBlockKind::Code, html: highlight(&code, &lang), code, lang };
    }

    let is_table = matches!(events.first(), Some(Event::Start(Tag::Table(_))));
    // Model output is untrusted: show raw HTML as text instead of letting Qt
    // interpret it, and map constructs Qt rich text lacks onto plain text.
    let events = events.into_iter().map(|e| match e {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        Event::TaskListMarker(done) => Event::Text(CowStr::Borrowed(if done { "☑ " } else { "☐ " })),
        Event::InlineMath(s) | Event::DisplayMath(s) => Event::Code(s),
        Event::End(TagEnd::HtmlBlock) => Event::SoftBreak,
        Event::Start(Tag::HtmlBlock) => Event::SoftBreak,
        other => other,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    if is_table {
        html = html.replacen(
            "<table>",
            "<table border=\"1\" cellspacing=\"0\" cellpadding=\"6\" style=\"border-collapse: collapse;\">",
            1,
        );
    }
    MdBlock { kind: MdBlockKind::Prose, html: html.trim_end().to_owned(), code: String::new(), lang: String::new() }
}

struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

/// Highlighted code names token classes instead of colours: spans get
/// `class="sN"` and the app's theme supplies the colours (a `<style>` with
/// `.s1 … .s11`), so switching themes recolours without re-rendering.
///
/// | class | tokens |
/// |---|---|
/// | s1 comment | s2 keyword | s3 string | s4 number/constant | s5 function |
/// | s6 variable | s7 type | s8 operator | s9 diff added | s10 removed | s11 changed |
const TOKEN_CLASSES: &[(u8, &str)] = &[
    (1, "comment, punctuation.definition.comment"),
    (2, "keyword, storage.modifier, storage.type.function, keyword.control, entity.name.tag, markup.heading"),
    (3, "string, punctuation.definition.string, markup.raw"),
    (4, "constant.numeric, constant.language, constant.character, constant.other, support.constant"),
    (5, "entity.name.function, support.function, variable.function, entity.other.attribute-name"),
    (6, "variable.parameter, variable.other.member, support.variable, variable.language"),
    (7, "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, support.type, support.class, storage.type, entity.other.inherited-class"),
    (8, "keyword.operator"),
    (9, "markup.inserted"),
    (10, "markup.deleted"),
    (11, "markup.changed"),
];

/// A syntect theme whose colours encode the token class in the red channel.
fn class_theme() -> Theme {
    let class = |n: u8| Color { r: n, g: 0, b: 0, a: 0xff };
    Theme {
        settings: ThemeSettings { foreground: Some(class(0)), ..ThemeSettings::default() },
        scopes: TOKEN_CLASSES
            .iter()
            .map(|(n, scopes)| ThemeItem {
                scope: scopes.parse::<ScopeSelectors>().expect("scope selectors"),
                style: StyleModifier { foreground: Some(class(*n)), background: None, font_style: None },
            })
            .collect(),
        ..Theme::default()
    }
}

fn highlighter() -> &'static Highlighter {
    static H: OnceLock<Highlighter> = OnceLock::new();
    H.get_or_init(|| Highlighter { syntaxes: SyntaxSet::load_defaults_newlines(), theme: class_theme() })
}

/// Appends a highlighted piece: plain text for the default class.
fn push_token(html: &mut String, class: u8, piece: &str) {
    if class == 0 {
        escape_into(html, piece);
    } else {
        html.push_str(&format!("<span class=\"s{class}\">"));
        escape_into(html, piece);
        html.push_str("</span>");
    }
}

/// Loads the syntax definitions ahead of time (they take a moment to parse).
pub fn warm_up() {
    highlighter();
}

/// Highlights code into a Qt rich-text `<pre>` block (token classes; see
/// `TOKEN_CLASSES`).
pub fn highlight(code: &str, lang: &str) -> String {
    let h = highlighter();
    let syntax = h
        .syntaxes
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| h.syntaxes.find_syntax_plain_text());
    let mut lines = HighlightLines::new(syntax, &h.theme);
    let mut html = String::with_capacity(code.len() * 3);
    html.push_str("<pre style=\"margin:0;\">");
    for line in code.split_inclusive('\n') {
        match lines.highlight_line(line, &h.syntaxes) {
            Ok(ranges) => {
                for (style, piece) in ranges {
                    push_token(&mut html, style.foreground.r, piece);
                }
            }
            Err(_) => escape_into(&mut html, line),
        }
    }
    html.push_str("</pre>");
    html
}

/// Highlights consecutive lines of a file (picked by extension) into one
/// rich-text span string per line. Highlighter state carries across lines,
/// so multi-line strings and comments mostly colour correctly.
pub fn highlight_lines(path: &str, lines: &[&str]) -> Vec<String> {
    let h = highlighter();
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
    let syntax = h
        .syntaxes
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| h.syntaxes.find_syntax_plain_text());
    let mut state = HighlightLines::new(syntax, &h.theme);
    lines
        .iter()
        .map(|line| {
            let with_newline = format!("{line}\n");
            let mut html = String::with_capacity(line.len() * 3);
            match state.highlight_line(&with_newline, &h.syntaxes) {
                Ok(ranges) => {
                    for (style, piece) in ranges {
                        let piece = piece.trim_end_matches('\n');
                        if piece.is_empty() {
                            continue;
                        }
                        push_token(&mut html, style.foreground.r, piece);
                    }
                }
                Err(_) => escape_into(&mut html, line),
            }
            html
        })
        .collect()
}

pub fn escape_into(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

/// How much of a still-streaming text to show.
///
/// Reveal only complete lines, so paragraphs don't reflow on every token.
/// If more than [`STREAM_HOLD_CHARS`] are held back (a long line), reveal up
/// to the last sentence boundary instead. (After OpenChamber's
/// `streamTextCommit.ts`, MIT: see THIRD-PARTY-NOTICES.md.)
pub fn committed_prefix(text: &str) -> &str {
    let line_end = text.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let held = &text[line_end..];
    if held.chars().count() <= STREAM_HOLD_CHARS {
        return &text[..line_end];
    }
    let boundary = held
        .char_indices()
        .rev()
        .find(|&(i, c)| matches!(c, '.' | '!' | '?') && held[i + c.len_utf8()..].starts_with(' '))
        .map(|(i, c)| i + c.len_utf8());
    match boundary {
        Some(b) => &text[..line_end + b],
        None => text,
    }
}

/// Rendered rich text as plain text, for reading aloud: tags dropped,
/// block ends as line breaks, entities decoded.
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let Some(gt) = rest[lt..].find('>') else {
            rest = "";
            break;
        };
        let closing = rest[lt + 1..].starts_with('/');
        let tag = rest[lt + 1..lt + gt].trim_start_matches('/').to_ascii_lowercase();
        let name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or_default();
        if matches!(name, "br" | "p" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "div" | "blockquote" | "hr") {
            out.push('\n');
        } else if closing && matches!(name, "td" | "th") {
            out.push_str(", ");
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(rest);
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&");
    decoded.lines().map(|l| l.trim().trim_end_matches([',', ' '])).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_top_level_blocks() {
        let mut r = MarkdownRenderer::default();
        let blocks = r.render("# Title\n\nPara **bold**.\n\n- a\n- b\n\n```rust\nfn main() {}\n```\n");
        assert_eq!(blocks.len(), 4);
        assert!(blocks[0].html.starts_with("<h1>"));
        assert_eq!(blocks[3].kind, MdBlockKind::Code);
        assert_eq!(blocks[3].lang, "rust");
        assert_eq!(blocks[3].code, "fn main() {}");
        assert!(blocks[3].html.contains("<span class=\"s2\">fn</span>"), "{}", blocks[3].html);
    }

    #[test]
    fn unchanged_blocks_are_reused() {
        let mut r = MarkdownRenderer::default();
        let a = r.render("First paragraph.\n\nSecond");
        let b = r.render("First paragraph.\n\nSecond paragraph grows");
        assert!(Arc::ptr_eq(&a[0], &b[0]));
        assert!(!Arc::ptr_eq(&a[1], &b[1]));
    }

    #[test]
    fn raw_html_is_escaped() {
        let mut r = MarkdownRenderer::default();
        let blocks = r.render("Hello <img src=x onerror=alert(1)> world");
        assert!(blocks[0].html.contains("&lt;img"));
    }

    #[test]
    fn commit_reveals_whole_lines() {
        assert_eq!(committed_prefix("line one\nline tw"), "line one\n");
        assert_eq!(committed_prefix("no newline yet"), "");
        let long = format!("{} Next sentence starts", "word. ".repeat(80));
        let shown = committed_prefix(&long);
        assert!(shown.ends_with('.'));
        assert!(shown.len() < long.len());
    }

    #[test]
    fn plain_text_for_speech() {
        assert_eq!(plain_text("<p>Hello <b>world</b> &amp; you</p><ul><li>one</li><li>two&nbsp;x</li></ul>"), "Hello world & you\none\ntwo x");
        assert_eq!(plain_text("<table><tr><th>A</th><th>B</th></tr></table>"), "A, B");
        assert_eq!(plain_text("a &lt;b&gt; c"), "a <b> c");
    }
}
