//! Answers as blocks the timeline lists one by one: paragraphs and lists,
//! headings, code, quotes, rules and tables.
//!
//! The window draws inline Markdown (emphasis, code spans, links, lists)
//! itself, but not headings, code blocks, quotes, rules or tables. Splitting
//! an answer into blocks lets the timeline draw those as rows of their own,
//! and keeps each row short however long the answer grows.
//!
//! Inline text takes the theme's reply colours: [`painted`] wraps code
//! spans, bold and italic text in colour tags.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use std::str::FromStr as _;

use harlo_core::theme::{Rgba, Tokens};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::SyntaxSet;

/// One block of an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A paragraph or a list, as inline Markdown.
    Text(String),
    /// A heading, as inline Markdown, and its level from 1 to 6.
    Heading {
        /// From 1, the largest, to 6.
        level: u8,
        /// The heading's inline Markdown.
        text: String,
    },
    /// A code block: its language, as the fence names it, and its text.
    Code {
        /// The fence's language, such as `rust`; empty when it names none.
        language: String,
        /// The code, without a final newline.
        text: String,
    },
    /// A quote, as inline Markdown.
    Quote(String),
    /// A horizontal rule.
    Rule,
    /// A table, as lines of plain text with its columns aligned.
    Table(String),
}

/// Splits `markdown` into the blocks the timeline lists.
///
/// Blocks come out in order. A code block that's still being written (no
/// closing fence yet) holds everything after its opening fence.
#[must_use]
pub fn blocks(markdown: &str) -> Vec<Block> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let mut out = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    let mut code: Option<(String, String)> = None;
    let mut table: Option<Table> = None;
    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    start = range.start;
                    match &tag {
                        Tag::CodeBlock(kind) => {
                            let language = match kind {
                                CodeBlockKind::Fenced(info) => {
                                    info.split_whitespace().next().unwrap_or("").to_owned()
                                }
                                CodeBlockKind::Indented => String::new(),
                            };
                            code = Some((language, String::new()));
                        }
                        Tag::Table(_) => table = Some(Table::default()),
                        _ => {}
                    }
                }
                if let Some(table) = &mut table {
                    match tag {
                        Tag::TableRow | Tag::TableHead => table.rows.push(Vec::new()),
                        Tag::TableCell => {
                            if let Some(row) = table.rows.last_mut() {
                                row.push(String::new());
                            }
                        }
                        _ => {}
                    }
                }
                depth += 1;
            }
            Event::End(end) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let source = &markdown[start..range.end];
                    if let Some(block) = finish(end, source, &mut code, &mut table) {
                        out.push(block);
                    }
                }
            }
            Event::Text(text) | Event::Code(text) | Event::InlineHtml(text) | Event::Html(text) => {
                if let Some((_, body)) = &mut code {
                    body.push_str(&text);
                } else if let Some(cell) = table
                    .as_mut()
                    .and_then(|t| t.rows.last_mut())
                    .and_then(|row| row.last_mut())
                {
                    cell.push_str(&text);
                } else if depth == 0 {
                    out.push(Block::Text(text.into_string()));
                }
            }
            Event::Rule if depth == 0 => out.push(Block::Rule),
            _ => {}
        }
    }
    // A code block still being written has no end event yet.
    if let Some((language, text)) = code {
        out.push(Block::Code {
            language,
            text: text.trim_end_matches('\n').to_owned(),
        });
    }
    out
}

/// A table's cells, row by row, as it's read.
#[derive(Debug, Default)]
struct Table {
    rows: Vec<Vec<String>>,
}

/// The block a top-level element that just ended makes, if any.
fn finish(
    end: TagEnd,
    source: &str,
    code: &mut Option<(String, String)>,
    table: &mut Option<Table>,
) -> Option<Block> {
    match end {
        TagEnd::Paragraph | TagEnd::List(_) | TagEnd::HtmlBlock => {
            Some(Block::Text(source.trim_end().to_owned()))
        }
        TagEnd::Heading(level) => Some(Block::Heading {
            level: heading_level(level),
            text: heading_text(source),
        }),
        TagEnd::CodeBlock => code.take().map(|(language, text)| Block::Code {
            language,
            text: text.trim_end_matches('\n').to_owned(),
        }),
        TagEnd::BlockQuote(_) => Some(Block::Quote(unquote(source))),
        TagEnd::Table => table.take().map(|t| Block::Table(aligned(&t.rows))),
        _ => None,
    }
}

/// `markdown` as plain text, such as for a notification: the words of its
/// paragraphs, headings, lists and code, with the markup gone.
#[must_use]
pub fn plain(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    for event in Parser::new_ext(markdown, Options::all()) {
        match event {
            Event::Text(text) | Event::Code(text) => out.push_str(&text),
            // Breaks, and the ends of blocks, so words don't run together.
            Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::TableCell
                | TagEnd::BlockQuote(_),
            ) => out.push(' '),
            _ => {}
        }
    }
    out
}

fn heading_level(level: pulldown_cmark::HeadingLevel) -> u8 {
    use pulldown_cmark::HeadingLevel as H;
    match level {
        H::H1 => 1,
        H::H2 => 2,
        H::H3 => 3,
        H::H4 => 4,
        H::H5 => 5,
        H::H6 => 6,
    }
}

/// A heading's text without its `#` marks, or without a setext underline.
fn heading_text(source: &str) -> String {
    let source = source.trim();
    if source.starts_with('#') {
        source
            .trim_start_matches('#')
            .trim_end_matches('#')
            .trim()
            .to_owned()
    } else {
        // A setext heading: the text, then a line of `=` or `-`.
        let mut lines: Vec<&str> = source.lines().collect();
        lines.pop();
        lines.join(" ").trim().to_owned()
    }
}

/// A quote's text without its `>` marks.
fn unquote(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let line = line.trim_start();
            let line = line.strip_prefix('>').unwrap_or(line);
            line.strip_prefix(' ').unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Table rows as lines of text, each column padded to its widest cell.
fn aligned(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .filter_map(|row| row.get(c))
                .map(|cell| cell.trim().chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let mut line = String::new();
        for (c, width) in widths.iter().enumerate() {
            if c > 0 {
                line.push_str("  ");
            }
            let cell = row.get(c).map_or("", |cell| cell.trim());
            let _ = write!(line, "{cell:<width$}");
        }
        out.push_str(line.trim_end());
        if index == 0 && rows.len() > 1 {
            out.push('\n');
            out.push_str(&"─".repeat(widths.iter().sum::<usize>() + 2 * columns.saturating_sub(1)));
        }
    }
    out
}

/// Colours code by its language, as Markdown the window draws.
///
/// Loading the syntaxes takes a moment, so one highlighter serves the whole
/// window.
pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
    /// The tokens the code colours come from.
    tokens: Tokens,
}

impl std::fmt::Debug for Highlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlighter").finish_non_exhaustive()
    }
}

impl Highlighter {
    /// Loads the bundled syntaxes, colouring code with `tokens`' colours.
    #[must_use]
    pub fn new(tokens: &Tokens) -> Self {
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            theme: code_theme(tokens),
            tokens: *tokens,
        }
    }

    /// The tokens the colours come from.
    #[must_use]
    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    /// Colours code with `tokens` from now on; returns whether that changes
    /// its colours.
    pub fn use_tokens(&mut self, tokens: &Tokens) -> bool {
        if self.tokens == *tokens {
            return false;
        }
        self.theme = code_theme(tokens);
        self.tokens = *tokens;
        true
    }

    /// `code` in `language` as Markdown: each line its own paragraph, the
    /// tokens wrapped in `<font color>` tags, and every other character
    /// escaped, so the code shows exactly as written.
    ///
    /// Code in a language it doesn't know comes out uncoloured.
    #[must_use]
    pub fn markup(&self, language: &str, code: &str) -> String {
        let syntax = self
            .syntaxes
            .find_syntax_by_token(language)
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let plain = self.theme.settings.foreground;
        let mut lines = HighlightLines::new(syntax, &self.theme);
        let mut out = String::with_capacity(code.len() * 2);
        for (index, line) in code.split('\n').enumerate() {
            if index > 0 {
                out.push('\n');
            }
            let source = format!("{line}\n");
            let regions = lines
                .highlight_line(&source, &self.syntaxes)
                .unwrap_or_default();
            let start = out.len();
            let mut leading = true;
            for (style, piece) in regions {
                let piece = piece.trim_end_matches('\n');
                if piece.is_empty() {
                    continue;
                }
                let colour = Some(style.foreground).filter(|c| Some(*c) != plain);
                if let Some(c) = colour {
                    let _ = write!(out, "<font color=\"{}\">", hex(c));
                }
                escape_into(&mut out, piece, &mut leading);
                if colour.is_some() {
                    out.push_str("</font>");
                }
            }
            if out.len() == start {
                // An empty line still takes its height.
                out.push('\u{a0}');
            }
        }
        out
    }
}

/// Picks one of a theme's code colours.
type CodeColor = fn(&Tokens) -> Rgba;

/// Which code the theme's code colours go to, as Base16's styling guide
/// assigns them: the grammar's scopes, and the token for them. A narrower scope wins.
const CODE_SCOPES: &[(&str, CodeColor)] = &[
    ("comment, punctuation.definition.comment", |t| {
        t.syntax_comment
    }),
    ("keyword, storage, entity.name.tag, keyword.control", |t| {
        t.syntax_keyword
    }),
    ("keyword.operator, punctuation.accessor", |t| {
        t.syntax_operator
    }),
    ("string, markup.raw", |t| t.syntax_string),
    (
        "constant, constant.numeric, constant.language, constant.character, \
         string.regexp, entity.name.namespace, entity.name.module, entity.name.label",
        |t| t.syntax_constant,
    ),
    (
        "entity.name.function, support.function, meta.function-call variable.function, \
         variable.function",
        |t| t.syntax_function,
    ),
    (
        "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, \
         entity.name.trait, entity.name.union, entity.name.interface, support.type, \
         support.class, storage.type.primitive, storage.type.numeric",
        |t| t.syntax_type,
    ),
    (
        "entity.other.attribute-name, support.type.property-name",
        |t| t.syntax_property,
    ),
    ("variable, variable.parameter, variable.other", |t| {
        t.syntax_variable
    }),
    ("punctuation", |t| t.syntax_punctuation),
    ("markup.heading, entity.name.section", |t| t.syntax_function),
];

/// A highlighting theme whose colours are `t`'s code colours.
fn code_theme(t: &Tokens) -> Theme {
    let scopes = CODE_SCOPES
        .iter()
        .filter_map(|(scopes, color)| {
            Some(ThemeItem {
                scope: ScopeSelectors::from_str(scopes).ok()?,
                style: StyleModifier {
                    foreground: Some(code_color(color(t))),
                    background: None,
                    font_style: None,
                },
            })
        })
        .chain(
            ScopeSelectors::from_str("markup.bold")
                .ok()
                .map(|scope| ThemeItem {
                    scope,
                    style: StyleModifier {
                        foreground: None,
                        background: None,
                        font_style: Some(FontStyle::BOLD),
                    },
                }),
        )
        .collect();
    Theme {
        name: Some("Harlo".to_owned()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(code_color(t.text_base)),
            ..ThemeSettings::default()
        },
        scopes,
    }
}

/// A theme colour as the highlighter's, without its alpha.
fn code_color(c: Rgba) -> Color {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=1 first, so the scaled value fits a u8"
    )]
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color {
        r: channel(c.r),
        g: channel(c.g),
        b: channel(c.b),
        a: 255,
    }
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Pushes `text` escaped so Markdown shows it as written: punctuation is
/// escaped, and spaces at the line's start become no-break spaces so they
/// aren't dropped.
fn escape_into(out: &mut String, text: &str, leading: &mut bool) {
    for c in text.chars() {
        match c {
            ' ' if *leading => out.push('\u{a0}'),
            '\t' if *leading => out.push_str("\u{a0}\u{a0}\u{a0}\u{a0}"),
            '\t' => out.push_str("    "),
            c if c.is_ascii_punctuation() => {
                *leading = false;
                out.push('\\');
                out.push(c);
            }
            c => {
                *leading = false;
                out.push(c);
            }
        }
    }
}

/// Inline Markdown with its bold text in the strongest text colour, and the
/// rest as it was.
///
/// Answers keep colour for links and leave meaning to the type: bold is
/// bold and a shade stronger than the text around it, italics slant, and
/// code spans take the code font, none of them borrowing the colours that
/// mean waiting, done or pressable. The colour is wrapped around the bold
/// span's own Markdown, so the window still draws it bold, and code inside
/// it in the code font.
#[must_use]
pub fn painted(t: &Tokens, source: &str) -> String {
    // Each tag's place in the text, and its rank among tags at one place:
    // closing tags before opening ones, an inner span closing first and an
    // outer one opening first.
    let mut tags: Vec<(usize, (u8, usize), String)> = Vec::new();
    let mut span = |start: usize, end: usize, color: Rgba| {
        let length = end - start;
        tags.push((start, (1, usize::MAX - length), font(t, color)));
        tags.push((end, (0, length), "</font>".to_owned()));
    };
    let mut open: Vec<usize> = Vec::new();
    for (event, range) in Parser::new(source).into_offset_iter() {
        match event {
            Event::Start(Tag::Strong) => open.push(range.start),
            Event::End(TagEnd::Strong) => {
                if let Some(start) = open.pop() {
                    span(start, range.end, t.text_strong);
                }
            }
            _ => {}
        }
    }
    if tags.is_empty() {
        return source.to_owned();
    }
    tags.sort_by_key(|(at, rank, _)| (*at, *rank));
    let mut out = source.to_owned();
    // From the end, so the places still ahead stay right.
    for (at, _, tag) in tags.into_iter().rev() {
        out.insert_str(at, &tag);
    }
    out
}

/// The opening colour tag for `color`, made readable.
fn font(t: &Tokens, color: Rgba) -> String {
    format!(
        "<font color=\"{}\">",
        hex(code_color(crate::theme::ink(t, color)))
    )
}

/// `text` escaped so Markdown shows it as written, on one paragraph per line.
#[must_use]
pub fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let start = out.len();
        let mut leading = true;
        escape_into(&mut out, line, &mut leading);
        if out.len() == start {
            out.push('\u{a0}');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_split_into_blocks() {
        let text = "Intro with `code`.\n\n## Plan\n\n- one\n- two\n\n```rust\nfn main() {}\n```\n\n> quoted\n\n---\n\n| a | bb |\n|---|---|\n| 1 | 2 |\n";
        let blocks = blocks(text);
        assert_eq!(
            blocks,
            vec![
                Block::Text("Intro with `code`.".into()),
                Block::Heading {
                    level: 2,
                    text: "Plan".into()
                },
                Block::Text("- one\n- two".into()),
                Block::Code {
                    language: "rust".into(),
                    text: "fn main() {}".into()
                },
                Block::Quote("quoted".into()),
                Block::Rule,
                Block::Table("a  bb\n─────\n1  2".into()),
            ]
        );
    }

    #[test]
    fn an_unclosed_fence_holds_the_rest() {
        let blocks = blocks("Look:\n\n```py\nprint(1)\nprint(2");
        assert_eq!(
            blocks.last(),
            Some(&Block::Code {
                language: "py".into(),
                text: "print(1)\nprint(2".into()
            })
        );
    }

    #[test]
    fn code_markup_parses_and_keeps_the_text() {
        let highlighter = Highlighter::new(&crate::theme::default_tokens());
        let code = "fn main() {\n    let x = a * b; // <tag> & `tick`\n\n}";
        let markup = highlighter.markup("rust", code);
        let styled = slint::StyledText::from_markdown(&markup).expect("valid markup");
        let shown = format!("{styled:?}");
        assert!(
            shown.contains("let x = a * b; // <tag> & `tick`"),
            "{shown}"
        );
        assert!(markup.contains("<font color="));
    }

    #[test]
    fn code_takes_the_themes_colours() {
        let dracula = harlo_core::theme::bundled()
            .into_iter()
            .find(|t| t.id == "dracula")
            .expect("dracula")
            .tokens();
        let mut highlighter = Highlighter::new(&crate::theme::default_tokens());
        assert!(highlighter.use_tokens(&dracula), "another theme");
        assert!(!highlighter.use_tokens(&dracula), "the same theme");
        let markup = highlighter.markup("rust", "fn main() { let s = \"hi\"; } // done");
        for (what, colour) in [
            ("keyword", dracula.syntax_keyword),
            ("string", dracula.syntax_string),
            ("comment", dracula.syntax_comment),
            ("function", dracula.syntax_function),
        ] {
            let colour = format!("<font color=\"{colour}\">");
            assert!(markup.contains(&colour), "{what} in {colour}: {markup}");
        }
    }

    #[test]
    fn only_bold_text_takes_a_colour_the_strongest_text() {
        let t = crate::theme::default_tokens();
        let source = "Plain, `code`, **bold with `code`**, *italic* and ***both***.";
        let markup = painted(&t, source);
        let strong = font(&t, t.text_strong);
        assert!(
            markup.contains(&format!("{strong}**bold with `code`**</font>")),
            "bold, with its code left to the code font: {markup}"
        );
        assert!(
            markup.contains("Plain, `code`, "),
            "code keeps the text's colour: {markup}"
        );
        assert!(
            markup.contains(&format!("*{strong}**both**</font>*")),
            "bold italics: {markup}"
        );
        assert_eq!(
            markup.matches("<font").count(),
            2,
            "nothing else is coloured: {markup}"
        );
        let styled = slint::StyledText::from_markdown(&markup).expect("valid markup");
        let shown = format!("{styled:?}");
        assert!(
            shown.contains("bold with code"),
            "the text is kept: {shown}"
        );
        assert_eq!(painted(&t, "no spans here"), "no spans here");
    }

    #[test]
    fn escaped_text_shows_as_written() {
        let styled = slint::StyledText::from_markdown(&escaped("# not a heading\n  *x* <b>"))
            .expect("valid");
        let shown = format!("{styled:?}");
        assert!(shown.contains("# not a heading"), "{shown}");
        assert!(shown.contains("*x* <b>"), "{shown}");
    }
}
