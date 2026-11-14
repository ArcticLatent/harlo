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
    /// A piece of a long code block or table (see [`MarkdownRenderer::render_rows`]).
    pub part: Part,
    /// A code piece's first line's number (1 for a whole block).
    pub first_line: usize,
}

/// Where a block stands in the long code block or table it's a piece of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Part {
    #[default]
    Whole,
    First,
    Middle,
    Last,
}

impl Part {
    /// As a row's state says it ("" whole).
    pub fn as_str(self) -> &'static str {
        match self {
            Part::Whole => "",
            Part::First => "part-first",
            Part::Middle => "part-middle",
            Part::Last => "part-last",
        }
    }
}

const CACHE_LIMIT: usize = 8192;
/// What the cache holds, in bytes, before it starts again.
const CACHE_BYTES: usize = 32 << 20;
/// A long code block or table is shown in pieces of this many lines or
/// rows, each a row of its own: only those in view are laid out, and while
/// it streams only its last is redone. (2000 lines, or 1500 table rows, in
/// one took a second to lay out, and froze the window as long.)
const PIECE_LINES: usize = 150;
const PIECE_ROWS: usize = 100;
/// Text held back while streaming before a sentence boundary is revealed.
const STREAM_HOLD_CHARS: usize = 320;

#[derive(Default)]
pub struct MarkdownRenderer {
    cache: HashMap<u64, Arc<MdBlock>>,
    /// The cached blocks' html and code, in bytes.
    bytes: usize,
}

impl MarkdownRenderer {
    /// `text`'s top-level blocks, each whole.
    pub fn render(&mut self, text: &str) -> Vec<Arc<MdBlock>> {
        self.blocks(text, false, false)
    }

    /// `text`'s blocks as a transcript shows them: a long code block or
    /// table in pieces ([`PIECE_LINES`], [`PIECE_ROWS`]); while it
    /// `streaming`, its last block isn't kept (each version of it would be).
    pub fn render_rows(&mut self, text: &str, streaming: bool) -> Vec<Arc<MdBlock>> {
        self.blocks(text, true, streaming)
    }

    fn blocks(&mut self, text: &str, pieces: bool, streaming: bool) -> Vec<Arc<MdBlock>> {
        if self.cache.len() > CACHE_LIMIT || self.bytes > CACHE_BYTES {
            self.cache.clear();
            self.bytes = 0;
        }
        // (Top-level blocks: their source and their events.)
        let mut groups: Vec<(usize, usize, Vec<Event<'_>>)> = Vec::new();
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
                groups.push((start, end.max(start), std::mem::take(&mut events)));
            }
        }
        let n = groups.len();
        let mut out = Vec::new();
        for (i, (start, end, events)) in groups.into_iter().enumerate() {
            let keep = !(streaming && i + 1 == n);
            let source = &text[start..end];
            if pieces {
                if let Some(Event::Start(Tag::CodeBlock(_))) = events.first() {
                    let (code, lang) = code_of(&events);
                    if code.lines().count() > PIECE_LINES {
                        out.extend(self.code_pieces(&code, &lang, keep));
                        continue;
                    }
                }
                if matches!(events.first(), Some(Event::Start(Tag::Table(_)))) && events.iter().filter(|e| matches!(e, Event::Start(Tag::TableRow))).count() > PIECE_ROWS {
                    // (Remade whole each time, in a millisecond; its pieces
                    // stay the same once written, as their widths do.)
                    let whole = render_block(events);
                    let parts = table_pieces(&whole.html);
                    let last = parts.len().saturating_sub(1);
                    for (k, (html, part)) in parts.into_iter().enumerate() {
                        let key = hash_source(&html);
                        out.push(self.keep(key, keep || k < last, || MdBlock { kind: MdBlockKind::Prose, html, code: String::new(), lang: String::new(), part, first_line: 1 }));
                    }
                    continue;
                }
            }
            let key = hash_source(source);
            out.push(self.keep(key, keep, || render_block(events)));
        }
        out
    }

    /// A block from the cache, else made (and kept, if `keep`).
    fn keep(&mut self, key: u64, keep: bool, make: impl FnOnce() -> MdBlock) -> Arc<MdBlock> {
        if let Some(b) = self.cache.get(&key) {
            return b.clone();
        }
        let b = Arc::new(make());
        if keep {
            self.bytes += b.html.len() + b.code.len();
            self.cache.insert(key, b.clone());
        }
        b
    }

    /// A long code block's pieces, each highlighted on its own (once: only
    /// the last is new while it streams).
    fn code_pieces(&mut self, code: &str, lang: &str, keep: bool) -> Vec<Arc<MdBlock>> {
        let lines: Vec<&str> = code.split_inclusive('\n').collect();
        let count = lines.len().div_ceil(PIECE_LINES);
        let mut out = Vec::with_capacity(count);
        for (k, chunk) in lines.chunks(PIECE_LINES).enumerate() {
            let piece: String = chunk.concat();
            let piece = piece.strip_suffix('\n').unwrap_or(&piece).to_owned();
            let part = if k == 0 { Part::First } else if k + 1 == count { Part::Last } else { Part::Middle };
            let first_line = k * PIECE_LINES + 1;
            let mut h = DefaultHasher::new();
            (lang, &piece, first_line, part as u8).hash(&mut h);
            let lang = lang.to_owned();
            out.push(self.keep(h.finish(), keep || k + 1 < count, || MdBlock { kind: MdBlockKind::Code, html: highlight(&piece, &lang), code: piece, lang, part, first_line }));
        }
        out
    }
}

/// A code block's text (less its last newline) and its fence's language.
fn code_of(events: &[Event<'_>]) -> (String, String) {
    let lang = match events.first() {
        Some(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))) => info.split_whitespace().next().unwrap_or_default().to_owned(),
        _ => String::new(),
    };
    let code: String = events
        .iter()
        .filter_map(|e| match e {
            Event::Text(t) => Some(t.as_ref()),
            _ => None,
        })
        .collect();
    (code.strip_suffix('\n').unwrap_or(&code).to_owned(), lang)
}

/// A long table's rendered html in pieces of [`PIECE_ROWS`] rows: the first
/// with the header, every one the full width with the same column widths
/// (from the longest text in each column of the header and the first
/// piece: they don't change as the rest streams in, so neither do the
/// pieces written), so they line up as one table.
fn table_pieces(html: &str) -> Vec<(String, Part)> {
    let (Some(body), Some(end)) = (html.find("<tbody>"), html.rfind("</tbody>")) else {
        return vec![(html.to_owned(), Part::Whole)];
    };
    let open_end = html.find('>').map_or(0, |i| i + 1);
    let open = html[..open_end].replacen("<table", "<table width=\"100%\"", 1);
    let head = &html[open_end..body];
    let rows: Vec<&str> = html[body + "<tbody>".len()..end].split_inclusive("</tr>").filter(|r| r.contains("<tr")).collect();
    // Each column's widest text, as a share of the width.
    // (A cell's tag: <td> or <th>, maybe with attributes; not <thead>.)
    let is_cell = |c: &str| (c.starts_with('d') || c.starts_with('h')) && matches!(c.as_bytes().get(1), Some(b'>' | b' '));
    let cells = |row: &str| -> Vec<usize> { row.split("<t").skip(1).filter(|c| is_cell(c)).map(|c| plain_text(&format!("<t{c}")).chars().count().clamp(3, 60)).collect() };
    let mut widest: Vec<usize> = Vec::new();
    for r in std::iter::once(head).chain(rows.iter().take(PIECE_ROWS).copied()) {
        for (i, w) in cells(r).into_iter().enumerate() {
            if i >= widest.len() {
                widest.push(w);
            } else {
                widest[i] = widest[i].max(w);
            }
        }
    }
    let total: usize = widest.iter().sum::<usize>().max(1);
    let pct: Vec<usize> = widest.iter().map(|w| (w * 100 / total).max(1)).collect();
    // A row's cells given their widths.
    let sized = |row: &str, tag: &str| -> String {
        let mut out = String::with_capacity(row.len() + 16 * pct.len());
        let mut col = 0;
        let mut rest = row;
        let open = format!("<{tag}");
        while let Some(at) = rest.find(&open) {
            let after = &rest[at + open.len()..];
            out.push_str(&rest[..at + open.len()]);
            if after.starts_with('>') || after.starts_with(' ') {
                out.push_str(&format!(" width=\"{}%\"", pct.get(col).copied().unwrap_or(1)));
                col += 1;
            }
            rest = after;
        }
        out.push_str(rest);
        out
    };
    let chunks: Vec<&[&str]> = rows.chunks(PIECE_ROWS).collect();
    let count = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(k, chunk)| {
            let part = if k == 0 { Part::First } else if k + 1 == count { Part::Last } else { Part::Middle };
            let mut piece = open.clone();
            if k == 0 {
                piece.push_str(&sized(head, "th"));
                piece.push_str("<tbody>");
                piece.push_str(&chunk.concat());
            } else {
                piece.push_str("<tbody>");
                piece.push_str(&sized(chunk[0], "td"));
                piece.push_str(&chunk[1..].concat());
            }
            piece.push_str("</tbody></table>");
            (piece, part)
        })
        .collect()
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
        return MdBlock { kind: MdBlockKind::Code, html: highlight(&code, &lang), code, lang, part: Part::Whole, first_line: 1 };
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
        // An image: its words, in italics (the picture shows under the
        // reply, from its file), marked with the file for the phone, which
        // shows the picture in their place (Qt's rich text ignores the mark).
        Event::Start(Tag::Image { dest_url, .. }) => Event::InlineHtml(CowStr::from(format!("<em data-img=\"{}\">", attr_escape(&dest_url)))),
        Event::End(TagEnd::Image) => Event::InlineHtml(CowStr::Borrowed("</em>")),
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
    MdBlock { kind: MdBlockKind::Prose, html: html.trim_end().to_owned(), code: String::new(), lang: String::new(), part: Part::Whole, first_line: 1 }
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
    (6, "variable.parameter, variable.other.member, support.variable, variable.language, entity.name.label"),
    (7, "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, support.type, support.class, storage.type, entity.other.inherited-class, entity.name.table, entity.name.section, entity.section"),
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
    H.get_or_init(|| Highlighter { syntaxes: two_face::syntax::extra_newlines(), theme: class_theme() })
}

/// Appends a highlighted piece: plain text for the default class.
fn push_token(html: &mut String, class: u8, piece: &str) {
    // (A line's newline stays outside its span: each line closes its own,
    // so the lines can be split into blocks.)
    let (body, newline) = piece.strip_suffix('\n').map_or((piece, ""), |b| (b, "\n"));
    if class == 0 || body.is_empty() {
        escape_into(html, body);
    } else {
        html.push_str(&format!("<span class=\"s{class}\">"));
        escape_into(html, body);
        html.push_str("</span>");
    }
    html.push_str(newline);
}

/// The syntaxes and the token-class theme (`TOKEN_CLASSES`), for the code
/// view (harlo-code), which colours a file a line at a time itself.
pub fn code_syntaxes() -> &'static syntect::parsing::SyntaxSet {
    &highlighter().syntaxes
}
pub fn code_theme() -> &'static Theme {
    &highlighter().theme
}

/// Loads the syntax definitions ahead of time (they take a moment to parse).
pub fn warm_up() {
    highlighter();
}

/// Names the syntax set doesn't know, as the one it has: `jsx` and
/// `mjs` are JavaScript, a shell session is shell, `jsonc` is JSON, a
/// systemd unit is INI, and `.fs` is F# rather than a shader.
fn alias(token: &str) -> &str {
    match token.to_ascii_lowercase().as_str() {
        "jsx" | "mjs" | "cjs" | "javascriptreact" => "js",
        "ksh" | "shell" | "console" | "shellsession" | "terminal" => "sh",
        "jsonc" | "json5" | "jsonl" | "ndjson" => "json",
        "service" | "socket" | "timer" | "mount" | "target" | "path" | "network" | "netdev" | "container" => "ini",
        "fs" | "fsharp" => "fsx",
        _ => token,
    }
}

/// The language of a file, by its extension, else its name (a Makefile,
/// CMakeLists.txt), if the highlighter has one.
pub fn language_of(path: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
    let h = highlighter();
    let p = std::path::Path::new(path);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let known = |s: Option<&'static syntect::parsing::SyntaxReference>| s.filter(|s| s.name != "Plain Text");
    known(if ext.is_empty() { None } else { h.syntaxes.find_syntax_by_extension(alias(ext)) }).or_else(|| known(h.syntaxes.find_syntax_by_extension(name)))
}

/// A code block's language by name, for its label: "TypeScript" for
/// `ts`, "Shell" for `bash`; the word itself when the highlighter doesn't
/// know it ("mermaid"), "Text" when there's none.
pub fn language_name(token: &str) -> String {
    let token = token.trim();
    if token.is_empty() || token.eq_ignore_ascii_case("text") || token.eq_ignore_ascii_case("plain") {
        return "Text".into();
    }
    match highlighter().syntaxes.find_syntax_by_token(alias(token)) {
        Some(s) if s.name == "Plain Text" => "Text".into(),
        Some(s) => match s.name.as_str() {
            "Bourne Again Shell (bash)" => "Shell".into(),
            "Separated Values" => "CSV".into(),
            "Protocol Buffer" => "Protobuf".into(),
            "TypeScriptReact" => "TSX".into(),
            "Vue Component" => "Vue".into(),
            name => name.into(),
        },
        None => token.into(),
    }
}

/// Highlights code into a Qt rich-text `<pre>` block (token classes; see
/// `TOKEN_CLASSES`).
pub fn highlight(code: &str, lang: &str) -> String {
    let h = highlighter();
    let syntax = h
        .syntaxes
        .find_syntax_by_token(alias(lang))
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
    let syntax = language_of(path).unwrap_or_else(|| h.syntaxes.find_syntax_plain_text());
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

/// One line of Markdown (a card's name, a pro or con) as inline rich text:
/// code, emphasis, links, without the paragraph around it. Raw HTML in it
/// shows as text, as in a reply.
pub fn inline_html(text: &str) -> String {
    let events = Parser::new_ext(text, options()).filter_map(|e| match e {
        Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => None,
        Event::Html(s) | Event::InlineHtml(s) => Some(Event::Text(s)),
        Event::InlineMath(s) | Event::DisplayMath(s) => Some(Event::Code(s)),
        other => Some(other),
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html.trim_end().to_owned()
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

/// `s` inside a double-quoted HTML attribute.
fn attr_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn huge_code(lines: usize) -> String {
        format!("```rust\n{}```\n", (0..lines).map(|i| format!("let x{i} = {i};\n")).collect::<String>())
    }

    #[test]
    fn a_long_code_block_comes_in_pieces_that_go_on() {
        let mut r = MarkdownRenderer::default();
        let blocks = r.render_rows(&format!("Here:\n\n{}", huge_code(400)), false);
        let parts: Vec<(Part, usize, usize)> = blocks.iter().map(|b| (b.part, b.first_line, b.code.lines().count())).collect();
        assert_eq!(parts, [(Part::Whole, 1, 0), (Part::First, 1, 150), (Part::Middle, 151, 150), (Part::Last, 301, 100)]);
        assert!(blocks[2].code.starts_with("let x150 = 150;") && blocks[2].html.contains("x150"), "{}", &blocks[2].code[..40]);
        assert!(blocks[1..].iter().all(|b| b.lang == "rust" && b.kind == MdBlockKind::Code));
        // Whole elsewhere (the wiki), and a short one whole here.
        assert_eq!(MarkdownRenderer::default().render(&huge_code(400)).len(), 1);
        assert_eq!(r.render_rows(&huge_code(150), false)[0].part, Part::Whole);
    }

    #[test]
    fn a_long_table_comes_in_pieces_that_line_up() {
        let table = format!("| # | name |\n|---|---|\n{}", (0..250).map(|i| format!("| {i} | part_{i}_with_a_longer_name |\n")).collect::<String>());
        let blocks = MarkdownRenderer::default().render_rows(&table, false);
        assert_eq!(blocks.iter().map(|b| b.part).collect::<Vec<_>>(), [Part::First, Part::Middle, Part::Last]);
        assert!(blocks[0].html.contains("<thead>") && !blocks[1].html.contains("<thead>"), "the header once: {}", &blocks[0].html[..300]);
        assert_eq!(blocks.iter().map(|b| b.html.matches("<tr").count()).collect::<Vec<_>>(), [101, 100, 50]);
        // The same widths in each, on its first row.
        let widths = |h: &str| h.split("width=\"").skip(2).map(|w| w.split('"').next().unwrap().to_owned()).take(2).collect::<Vec<_>>();
        let first = widths(&blocks[0].html);
        assert_eq!(first, widths(&blocks[1].html));
        assert_eq!(first, widths(&blocks[2].html));
        assert!(first[1].trim_end_matches('%').parse::<u32>().unwrap() > 80, "{first:?}");
        assert!(blocks.iter().all(|b| b.html.starts_with("<table width=\"100%\"") && b.html.ends_with("</table>")));
    }

    #[test]
    fn a_streaming_last_block_isnt_kept() {
        let mut r = MarkdownRenderer::default();
        let mut text = String::from("Intro.\n\n```rust\n");
        for i in 0..400 {
            text.push_str(&format!("let x{i} = {i};\n"));
            r.render_rows(&text, true);
        }
        // The intro and the finished pieces (the first two), not each version
        // of the one being written.
        assert_eq!(r.cache.len(), 3, "{}", r.cache.len());
        r.render_rows(&text, false);
        assert_eq!(r.cache.len(), 4);
    }

    /// How long one huge code block and one huge table take to render
    /// (cargo test -p harlo-core --release -- --ignored --nocapture huge_blocks).
    #[test]
    #[ignore]
    fn huge_blocks_render_time() {
        let code = format!("```rust\n{}```\n", (0..2000).map(|i| format!("fn step_{i}(rows: &mut Vec<u32>) {{ rows.push({i}); }}\n")).collect::<String>());
        let table = format!("| # | name | rows | state |\n|---|---|---|---|\n{}", (0..1500).map(|i| format!("| {i} | part_{i} | {} | ok |\n", i * 7)).collect::<String>());
        for (what, text) in [("code", &code), ("table", &table)] {
            let t = std::time::Instant::now();
            let blocks = MarkdownRenderer::default().render(text);
            let html: usize = blocks.iter().map(|b| b.html.len() + b.code.len()).sum();
            eprintln!("{what}: {} KB source, {} KB rendered, {:?}", text.len() / 1024, html / 1024, t.elapsed());
        }
    }

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
    fn inline_lines_keep_their_marks_without_a_paragraph() {
        assert_eq!(inline_html("every report gets `--format csv`"), "every report gets <code>--format csv</code>");
        assert_eq!(inline_html("**fast** and [docs](https://x.dev)"), "<strong>fast</strong> and <a href=\"https://x.dev\">docs</a>");
        assert_eq!(inline_html("a <b>tag</b> & more"), "a &lt;b&gt;tag&lt;/b&gt; &amp; more");
        assert_eq!(plain_text(&inline_html("A `csv` format")), "A csv format");
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
    fn images_read_as_their_words() {
        let mut r = MarkdownRenderer::default();
        let blocks = r.render("See ![a red sun](art/sun.png) here, and ![\"q\"](<a \"b\".png>).");
        let html = &blocks[0].html;
        assert!(html.contains("<em data-img=\"art/sun.png\">a red sun</em>") && !html.contains("<img"), "{html}");
        assert!(html.contains("<em data-img=\"a &quot;b&quot;.png\">"), "the file's name stays inside its quotes: {html}");
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

    #[test]
    fn lines_close_their_spans() {
        let html = highlight("fn main() {\n    let s = \"a\";\n}\n", "rs");
        let inner = html.trim_start_matches("<pre style=\"margin:0;\">").trim_end_matches("</pre>");
        for line in inner.split('\n') {
            assert_eq!(line.matches("<span").count(), line.matches("</span>").count(), "{line}");
        }
    }

    #[test]
    fn languages_by_name() {
        let n = |t: &str| language_name(t);
        assert_eq!([n("ts"), n("bash"), n("toml"), n("rust"), n("qml"), n(""), n("mermaid")], ["TypeScript", "Shell", "TOML", "Rust", "QML", "Text", "mermaid"]);
    }
}
