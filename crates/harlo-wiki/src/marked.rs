//! The Markdown lexer OpenWiki reads pages with (marked 18.2, GFM), ported
//! so that sections, their raw text and their heading anchors come out the
//! same. Only what the wiki needs: the top-level blocks with their raw text,
//! and each heading's inline text. Its patterns are marked's own, run by
//! [`crate::jsre`]; offsets are bytes throughout (marked's are UTF-16 units,
//! but every length here comes from the same string it's used on).

use crate::jsre::{self, Re};
use std::collections::HashMap;
use std::sync::LazyLock;

macro_rules! re {
    ($name:ident, $src:expr) => {
        static $name: LazyLock<Re> = LazyLock::new(|| Re::new($src, ""));
    };
    ($name:ident, $src:expr, $flags:expr) => {
        static $name: LazyLock<Re> = LazyLock::new(|| Re::new($src, $flags));
    };
}

const TAG: &str = "address|article|aside|base|basefont|blockquote|body|caption|center|col|colgroup|dd|details|dialog|dir|div|dl|dt|fieldset|figcaption|figure|footer|form|frame|frameset|h[1-6]|head|header|hr|html|iframe|legend|li|link|main|menu|menuitem|meta|nav|noframes|ol|optgroup|option|p|param|search|section|summary|table|tbody|td|tfoot|th|thead|title|tr|track|ul";

// ---- Block rules (marked's block.gfm, as compiled) ----

re!(NEWLINE, r"^(?:[ \t]*(?:\n|$))+");
re!(CODE, r"^((?: {4}| {0,3}\t)[^\n]+(?:\n(?:[ \t]*(?:\n|$))*)?)+");
re!(FENCES, r"^ {0,3}(`{3,}(?=[^`\n]*(?:\n|$))|~{3,})([^\n]*)(?:\n|$)(?:|([\s\S]*?)(?:\n|$))(?: {0,3}(?:(?=`)\1`*|(?=~)\1~*) *(?=\n|$)|$)");
re!(HEADING, r"^ {0,3}(#{1,6})(?=\s|$)(.*)(?:\n+|$)");
re!(HR, r"^ {0,3}((?:-[\t ]*){3,}|(?:_[ \t]*){3,}|(?:\*[ \t]*){3,})(?:\n+|$)");
static BLOCKQUOTE: LazyLock<Re> = LazyLock::new(|| {
    Re::new(&format!(r"^( {{0,3}}> ?(?:(?![ \t]*(?:\n|$))(([^\n]+(?:\n(?! {{0,3}}((?:-[\t ]*){{3,}}|(?:_[ \t]*){{3,}}|(?:\*[ \t]*){{3,}})(?:\n+|$)| {{0,3}}#{{1,6}}(?:\s|$)| {{0,3}}>| {{0,3}}(?:`{{3,}}(?=[^`\n]*(?:\n|$))|~~~)[^\n]*(?:\n|$)| {{0,3}}(?:[*+-]|\d{{1,9}}[.)])(?:[ \t]|\n|$)| {{0,3}}(?:<\/?(?:{TAG})(?: +|\n|\/?>)|<(?:(?:script|pre|style|textarea)(?:[\s>]|$)|!--)|<\?|<![A-Za-z]|<!\[CDATA\[)|[ \t]+\n)[^\n]+)*))|[^\n]*)(?:\n|$))+"), "")
});
re!(LIST, r"^( {0,3}(?:[*+-]|\d{1,9}[.)]))([ \t][^\n]*?)?(?:\n|$)");
static HTML: LazyLock<Re> = LazyLock::new(|| {
    Re::new(&format!(r#"^ {{0,3}}(?:<(script|pre|style|textarea)[\s>][\s\S]*?(?:<\/\1>[^\n]*\n*|$)|<!--(?:-?>|[\s\S]*?(?:-->|$))[^\n]*(\n+|$)|<\?[\s\S]*?(?:\?>[^\n]*\n*|$)|<![A-Z][\s\S]*?(?:>[^\n]*\n*|$)|<!\[CDATA\[[\s\S]*?(?:\]\]>[^\n]*\n*|$)|<\/?({TAG})(?:[ \t]+|\n|\/?>|$)[\s\S]*?(?:(?:\n[ \t]*)+\n|$)|<(?!(?:script|pre|style|textarea)(?![a-z0-9-]))([a-z][a-z0-9-]*)(?: +[a-zA-Z:_][\w.:-]*(?: *= *"[^"\n]*"| *= *'[^'\n]*'| *= *[^\s"'=<>`]+)?)*? *\/?>(?=[ \t]*(?:\n|$))[\s\S]*?(?:(?:\n[ \t]*)+\n|$)|<\/[a-z][a-z0-9-]*\s*>(?=[ \t]*(?:\n|$))[\s\S]*?(?:(?:\n[ \t]*)+\n|$))"#), "i")
});
re!(DEF, r#"^ {0,3}\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\\n]|\n(?![ \t]*\n))+)\]: *(?:\n[ \t]*)?([^<\s][^\s]*|<.*?>)(?:(?: +(?:\n[ \t]*)?| *\n[ \t]*)((?:"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\\n]|\n(?!\n))*'|\((?:\\[\s\S]|[^()\\])*\))))? *(?:\n+|$)"#);
static TABLE: LazyLock<Re> = LazyLock::new(|| {
    Re::new(&format!(r"^ *([^\n ].*)\n {{0,3}}((?:\| *)?:?-+:? *(?:\| *:?-+:? *)*(?:\| *)?)(?:\n((?:(?! *\n| {{0,3}}((?:-[\t ]*){{3,}}|(?:_[ \t]*){{3,}}|(?:\*[ \t]*){{3,}})(?:\n+|$)| {{0,3}}#{{1,6}}(?:\s|$)| {{0,3}}>|(?: {{4}}| {{0,3}}\t)[^\n]| {{0,3}}(?:`{{3,}}(?=[^`\n]*(?:\n|$))|~~~)[^\n]*(?:\n|$)| {{0,3}}(?:[*+-]|1[.)])[ \t]| {{0,3}}(?:<\/?(?:{TAG})(?: +|\n|\/?>)|<(?:(?:script|pre|style|textarea)(?:[\s>]|$)|!--)|<\?|<![A-Za-z]|<!\[CDATA\[)).*(?:\n|$))*)\n*|$)"), "")
});
re!(LHEADING, r"^(?! {0,3}(?:[*+-]|\d{1,9}[.)]) |(?: {4}| {0,3}\t)| {0,3}(?:`{3,}|~{3,})| {0,3}>| {0,3}#{1,6}(?:\s|$)| {0,3}<[^\n>]+>\n| {0,3}\|?(?:[:\- ]*\|)+[\:\- ]*\n)((?:.|\n(?!\s*?\n| {0,3}(?:[*+-]|\d{1,9}[.)]) | {0,3}(?:`{3,}|~{3,})| {0,3}>| {0,3}#{1,6}(?:\s|$)| {0,3}(?:(?:-[\t ]*){3,}|(?:_[ \t]*){3,}|(?:\*[ \t]*){3,})(?:\n+|$)| {0,3}<[^\n>]+>\n| {0,3}\|?(?:[:\- ]*\|)+[\:\- ]*\n))+?)\n {0,3}(=+|-+)[ \t]*(?:\n+|$)");
static PARAGRAPH: LazyLock<Re> = LazyLock::new(|| {
    let html = format!(r"<\/?(?:{TAG})(?: +|\n|\/?>)|<(?:(?:script|pre|style|textarea)(?:[\s>]|$)|!--)|<\?|<![A-Za-z]|<!\[CDATA\[");
    Re::new(&format!(r"^([^\n]+(?:\n(?! {{0,3}}((?:-[\t ]*){{3,}}|(?:_[ \t]*){{3,}}|(?:\*[ \t]*){{3,}})(?:\n+|$)| {{0,3}}#{{1,6}}(?:\s|$)| {{0,3}}>| {{0,3}}(?:`{{3,}}(?=[^`\n]*(?:\n|$))|~~~)[^\n]*(?:\n|$)| {{0,3}}(?:[*+-]|1[.)])[ \t]+[^ \t\n]| {{0,3}}(?:{html})| *([^\n ].*)\n {{0,3}}((?:\| *)?:?-+:? *(?:\| *:?-+:? *)*(?:\| *)?)(?:\n((?:(?! *\n| {{0,3}}((?:-[\t ]*){{3,}}|(?:_[ \t]*){{3,}}|(?:\*[ \t]*){{3,}})(?:\n+|$)| {{0,3}}#{{1,6}}(?:\s|$)| {{0,3}}>|(?: {{4}}| {{0,3}}\t)[^\n]| {{0,3}}(?:`{{3,}}(?=[^`\n]*(?:\n|$))|~~~)[^\n]*(?:\n|$)| {{0,3}}(?:[*+-]|1[.)])[ \t]| {{0,3}}(?:{html})).*(?:\n|$))*)\n*|$)|[ \t]+\n)[^\n]+)*)"), "")
});
re!(TEXT, r"^[^\n]+");

// ---- marked's `other` helpers ----

re!(LIST_INTERRUPTS_PARAGRAPH, r"^ {0,3}(?:[*+-]|1[.)])[ \t]+[^ \t\n]");
re!(BLANK_LINE, r"^[ \t]*$");
re!(BLOCKQUOTE_START, r"^ {0,3}>");
re!(BLOCKQUOTE_SETEXT_REPLACE, r"\n {0,3}((?:=+|-+)[ \t]*)(?=\n|$)");
re!(BLOCKQUOTE_SETEXT_REPLACE2, r"^ {0,3}>[ \t]?", "m");
re!(BLOCKQUOTE_MARKER_WHITESPACE, r"^( {0,3}>)([ \t]+)", "m");
re!(LEADING_SPACE_TAB, r"^[ \t]+");
re!(NON_SPACE_CHAR, r"[^ ]");
re!(NESTED_CONTAINER_MARKERS, r"^(?: {0,3}(?:(?:[*+-]|\d{1,9}[.)])[ \t]+|>[ \t]?))+");
re!(TABLE_DELIMITER, r"[:|]");
re!(TABLE_ALIGN_CHARS, r"^\||\| *$");
re!(START_ANGLE, r"^<");
re!(END_ANGLE, r">$");
re!(OUTPUT_LINK_REPLACE, r"\\([\[\]])");
re!(START_A_TAG, r"^<a ", "i");
re!(END_A_TAG, r"^<\/a>", "i");
re!(START_PRE_SCRIPT, r"^<(pre|code|kbd|script)(\s|>)", "i");
re!(END_PRE_SCRIPT, r"^<\/(pre|code|kbd|script)(\s|>)", "i");
re!(UNICODE_ALNUM, r"[\p{L}\p{N}]", "u");
re!(NUMERIC_REF, r"&#(?:(\d{1,7})|[Xx]([A-Fa-f0-9]{1,6}));");
re!(NUMERIC_REF_OR_AMP, r"&#(?:(\d{1,7})|[Xx]([A-Fa-f0-9]{1,6}));|&(?!(#\d{1,7}|#[Xx][a-fA-F0-9]{1,6}|\w+);)");
re!(NEW_LINE_CHAR, r"\n");
re!(STARTING_SPACE, r"^ ");
re!(ENDING_SPACE, r" $");
re!(TAB_CHAR, r"\t");

fn indent_regex(cache: &'static [LazyLock<Re>; 4], indent: usize) -> &'static Re {
    &cache[indent.clamp(1, 4) - 1]
}

macro_rules! by_indent {
    ($name:ident, $make:expr) => {
        static $name: [LazyLock<Re>; 4] = [
            LazyLock::new(|| $make(0)),
            LazyLock::new(|| $make(1)),
            LazyLock::new(|| $make(2)),
            LazyLock::new(|| $make(3)),
        ];
    };
}

by_indent!(NEXT_BULLET, |i: usize| Re::new(&format!(r"^ {{0,{i}}}(?:[*+-]|\d{{1,9}}[.)])((?:[ \t][^\n]*)?(?:\n|$))"), ""));
by_indent!(HR_AT, |i: usize| Re::new(&format!(r"^ {{0,{i}}}((?:-[ \t]*){{3,}}|(?:_[ \t]*){{3,}}|(?:\*[ \t]*){{3,}})(?:\n+|$)"), ""));
by_indent!(FENCES_BEGIN, |i: usize| Re::new(&format!(r"^ {{0,{i}}}(?:```|~~~)"), ""));
by_indent!(HEADING_BEGIN, |i: usize| Re::new(&format!(r"^ {{0,{i}}}#{{1,6}}(?:\s|$)"), ""));
by_indent!(HTML_BEGIN, |i: usize| Re::new(&format!(r"^ {{0,{i}}}(?:<\/?(?:{TAG})(?: +|$|\/?>)|<(?:(?:script|pre|style|textarea)(?:[\s>]|$)|!--)|<\?|<![A-Za-z]|<!\[CDATA\[)"), "i"));
by_indent!(BLOCKQUOTE_BEGIN, |i: usize| Re::new(&format!(r"^ {{0,{i}}}>"), ""));

// ---- Inline rules (marked's inline.gfm) ----

re!(ESCAPE, r##"^\\([!"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])"##);
re!(INLINE_TAG, r#"^<!--(?:-?>|[\s\S]*?-->)|^<\/[a-zA-Z][a-zA-Z0-9-]*\s*>|^<[a-zA-Z][a-zA-Z0-9-]*(?:\s+[a-zA-Z:_][\w.:-]*(?:\s*=\s*"[^"]*"|\s*=\s*'[^']*'|\s*=\s*[^\s"'=<>`]+)?)*?\s*\/?>|^<\?[\s\S]*?\?>|^<![a-zA-Z]+\s[\s\S]*?>|^<!\[CDATA\[[\s\S]*?\]\]>"#);
re!(LINK, r#"^!?\[((?:\[(?:\[(?:\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|`+(?!`)[^`]*?`+(?!`)|``+(?=\])|[^\[\]\\`])*?)\]\([ \t\n]*(<(?:\\.|[^\n<>\\])+>|[^ \t\n\x00-\x1f]+|(?=\)))(?:(?:[ \t]+(?:\n[ \t]*)?|\n[ \t]*)("(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'|\((?:\\[\s\S]|[^)\\])*\)))?[ \t\n]*\)"#);
re!(REFLINK, r"^!?\[((?:\[(?:\[(?:\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|`+(?!`)[^`]*?`+(?!`)|``+(?=\])|[^\[\]\\`])*?)\]\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\\n]|\n(?![ \t]*\n))+)\]");
re!(NOLINK, r"^!?\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\\n]|\n(?![ \t]*\n))+)\](?:\[\])?");
re!(REFLINK_SEARCH, r"!?\[((?:[^\[\]\\`]*(?:\[(?:\[(?:\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|`+(?!`)[^`]*?`+(?!`)|``+(?=\]))){0,999}?[^\[\]\\`]*?)\]\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\]){1,999})\]|!?\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\]){1,999})\](?:\[\])?(?!\()");
re!(BLOCK_SKIP, r"\[(?:[^\[\]`]|(?<a>`+)[^`]+\k<a>(?!`))*?\]\((?:\\[\s\S]|[^\\\(\)]|\((?:\\[\s\S]|[^\\\(\)])*\))*\)|(?<!`)()(?<b>`+)[^`]+\k<b>(?!`)|<(?! )[^<>]*?>");
re!(EM_STRONG_LDELIM, r"^(?:\*+(?:((?!\*)(?!~)[\p{P}\p{S}])|([^\s*]))?)|^_+(?:((?!_)(?!~)[\p{P}\p{S}])|([^\s_]))?", "u");
re!(EM_STRONG_RDELIM_AST, r"^[^_*]*?__[^_*]*?\*[^_*]*?(?=__)|[^*]+(?=[^*])|(?!\*)(?!~)[\p{P}\p{S}](\*+)(?=[\s]|$)|(?:[^\s\p{P}\p{S}]|~)(\*+)(?!\*)(?=(?!~)[\s\p{P}\p{S}]|$)|(?!\*)(?!~)[\s\p{P}\p{S}](\*+)(?=(?:[^\s\p{P}\p{S}]|~))|[\s](\*+)(?!\*)(?=(?!~)[\p{P}\p{S}])|(?!\*)(?!~)[\p{P}\p{S}](\*+)(?!\*)(?=(?!~)[\p{P}\p{S}])|(?:[^\s\p{P}\p{S}]|~)(\*+)(?=(?:[^\s\p{P}\p{S}]|~))", "u");
re!(EM_STRONG_RDELIM_UND, r"^[^_*]*?\*\*[^_*]*?_[^_*]*?(?=\*\*)|[^_]+(?=[^_])|(?!_)[\p{P}\p{S}](_+)(?=[\s]|$)|[^\s\p{P}\p{S}](_+)(?!_)(?=[\s\p{P}\p{S}]|$)|(?!_)[\s\p{P}\p{S}](_+)(?=[^\s\p{P}\p{S}])|[\s](_+)(?!_)(?=[\p{P}\p{S}])|(?!_)[\p{P}\p{S}](_+)(?!_)(?=[\p{P}\p{S}])", "u");
re!(PUNCTUATION, r"^((?![*_])[\s\p{P}\p{S}])", "u");
re!(INLINE_CODE, r"^(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)");
re!(BR, r"^( {2,}|\\)\n(?!\s*$)[ \t]*");
re!(DEL_LDELIM, r"^~~?(?:((?!~)[\p{P}\p{S}])|[^\s~])", "u");
re!(DEL_RDELIM, r"^[^~]+(?=[^~])|(?!~)[\p{P}\p{S}](~~?)(?=[\s]|$)|[^\s\p{P}\p{S}](~~?)(?!~)(?=[\s\p{P}\p{S}]|$)|(?!~)[\s\p{P}\p{S}](~~?)(?=[^\s\p{P}\p{S}])|[\s](~~?)(?!~)(?=[\p{P}\p{S}])|(?!~)[\p{P}\p{S}](~~?)(?!~)(?=[\p{P}\p{S}])|[^\s\p{P}\p{S}](~~?)(?=[^\s\p{P}\p{S}])", "u");
re!(AUTOLINK, r"^<([a-zA-Z][a-zA-Z0-9+.-]{1,31}:[^\s\x00-\x1f<>]*|[a-zA-Z0-9.!#$%&'*+/=?_`{|}~-]+(@)[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+(?![-_]))>");
re!(URL, r"^(?:mailto:[A-Za-z0-9._+-]+@[a-zA-Z0-9-_]+(?:\.[a-zA-Z0-9-_]*[a-zA-Z0-9])+(?![\w-])|xmpp:[A-Za-z0-9._+-]+@[a-zA-Z0-9-_]+(?:\.[a-zA-Z0-9-_]*[a-zA-Z0-9])+(?![\w-])(?:\/[A-Za-z0-9@.]+)?)|^((?:[hH][tT][tT][pP][sS]?|[fF][tT][pP]):\/\/|www\.)(?:[a-zA-Z0-9\-]+\.?)+[^\s<]*|^[A-Za-z0-9._+-]+(@)[a-zA-Z0-9-_]+(?:\.[a-zA-Z0-9-_]*[a-zA-Z0-9])+(?![\w-])");
re!(BACKPEDAL, r#"(?:[^?!.,:;*_'"~()&]+|\([^)]*\)|&(?![a-zA-Z0-9]+;$)|[?!.,:;*_'"~)]+(?!$))+"#);
re!(INLINE_TEXT, r"^(?:[^a-zA-Z0-9](?=(?:mailto|xmpp):)|(`+|~+|[^`~])(?:(?=[`~])|(?= {2,}\n)|(?=[a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-]+@)|[\s\S]*?(?:(?=[\\<!\[`*~_]|\b_|[hH][tT][tT][pP][sS]?|[fF][tT][pP]:\/\/|(?<![^\s*_~(])www\.|$)|[^ ](?= {2,}\n)|[^a-zA-Z0-9](?=(?:mailto|xmpp):)|[^a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-](?=[a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-]+@))))");
re!(ANY_PUNCTUATION_OUTSIDE_CODE, r"(`+)(?:[^`]|[^`][\s\S]*?[^`])\1(?!`)|`+|\\([\p{P}\p{S}])", "u");

// ---- Tokens ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Space,
    Code,
    Heading,
    Hr,
    Blockquote,
    List,
    Html,
    Def,
    Table,
    Paragraph,
    Text,
}

/// A top-level block: its kind and raw source (a heading's depth and text
/// too).
#[derive(Debug, Clone)]
pub struct Block {
    pub kind: Kind,
    pub raw: String,
    pub depth: usize,
    pub text: String,
    /// A heading's inline tokens.
    pub inline: Vec<Inline>,
    /// A blockquote's (raw-only use) text.
    bq_text: String,
}

impl Block {
    fn new(kind: Kind, raw: impl Into<String>) -> Block {
        Block { kind, raw: raw.into(), depth: 0, text: String::new(), inline: Vec::new(), bq_text: String::new() }
    }
}

#[derive(Debug, Clone)]
pub enum Inline {
    Text(String),
    Escape(String),
    Html,
    Link(Vec<Inline>),
    Em(Vec<Inline>),
    Strong(Vec<Inline>),
    Del(Vec<Inline>),
    Codespan(String),
    Br,
}

/// Text of inline tokens the way OpenWiki forms an anchor from it: html
/// left out, nested tokens followed, a line break a space.
pub fn inline_text(tokens: &[Inline]) -> String {
    let mut out = String::new();
    for t in tokens {
        match t {
            Inline::Html => {}
            Inline::Link(c) | Inline::Em(c) | Inline::Strong(c) | Inline::Del(c) => out.push_str(&inline_text(c)),
            Inline::Text(s) | Inline::Escape(s) | Inline::Codespan(s) => out.push_str(s),
            Inline::Br => out.push(' '),
        }
    }
    out
}

struct Lexer {
    links: HashMap<String, ()>,
    in_link: bool,
    in_raw_block: bool,
    link_emitted: bool,
    link_paren_possible: bool,
}

/// `marked.lexer(src)`: the top-level blocks, headings with their inline
/// tokens.
pub fn lex(src: &str) -> Vec<Block> {
    let src = src.replace("\r\n", "\n").replace('\r', "\n");
    let mut lx = Lexer { links: HashMap::new(), in_link: false, in_raw_block: false, link_emitted: false, link_paren_possible: true };
    let mut tokens = Vec::new();
    lx.block_tokens(&src, &mut tokens, false);
    for t in tokens.iter_mut() {
        if t.kind == Kind::Heading {
            lx.in_raw_block = false;
            t.inline = lx.inline_tokens(&t.text.clone());
        }
    }
    tokens
}

fn rtrim<'a>(s: &'a str, c: char) -> &'a str {
    s.trim_end_matches(c)
}

fn trim_trailing_blank_lines(s: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let mut end = lines.len() as isize - 1;
    while end >= 0 && BLANK_LINE.test(lines[end as usize]) {
        end -= 1;
    }
    if lines.len() as isize - end <= 2 {
        return s.to_owned();
    }
    lines[..(end + 1) as usize].join("\n")
}

fn expand_tabs(line: &str, indent: usize) -> String {
    let mut col = indent;
    let mut out = String::new();
    for c in line.chars() {
        if c == '\t' {
            let added = 4 - (col % 4);
            out.push_str(&" ".repeat(added));
            col += added;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// Makes the lexer's patterns now, on another thread, so the first search
/// doesn't wait for them (once per process).
pub fn warm() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = std::thread::Builder::new().name("wiki-warm".into()).spawn(|| {
            lex("# A\n\nText with `code`, [a](b) and *em*.\n\n## B\n\n- one\n- two\n\n1. x\n\n> q\n\n```rs\nx\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n<div>\nh\n</div>\n\n[r]: http://x\n\n---\n");
        });
    });
}

static LABEL_SPACES: LazyLock<Re> = LazyLock::new(|| Re::new(r"\s+", ""));

/// A list item's pattern for its bullet ("*", "+", "-") or, ordered, its
/// delimiter ("." or ")"), made once each.
fn item_pattern(mark: &str, ordered: bool) -> &'static Re {
    static ITEMS: LazyLock<std::sync::Mutex<Vec<(String, bool, &'static Re)>>> = LazyLock::new(Default::default);
    let mut all = ITEMS.lock().unwrap();
    if let Some((_, _, re)) = all.iter().find(|(m, o, _)| m == mark && *o == ordered) {
        return re;
    }
    let bull = if ordered { format!(r"\d{{1,9}}\{mark}") } else { format!(r"\{mark}") };
    let re: &'static Re = Box::leak(Box::new(Re::new(&format!(r"^( {{0,3}}{bull})((?:[\t ][^\n]*)?(?:\n|$))"), "")));
    all.push((mark.to_owned(), ordered, re));
    re
}

fn normalize_label(label: &str) -> String {
    let collapsed = LABEL_SPACES.replace(jsre::trim(label), true, |_| " ".into());
    collapsed.to_lowercase().to_uppercase().to_lowercase()
}

/// marked's findClosingBracket on a byte string: the index of the
/// unbalanced closer, -1 for none, -2 for more openers than closers.
fn find_closing_bracket(s: &str, open: u8, close: u8) -> isize {
    let b = s.as_bytes();
    if !b.contains(&open) && !b.contains(&close) {
        return -1;
    }
    let mut level = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 1;
        } else if b[i] == open {
            level += 1;
        } else if b[i] == close {
            level -= 1;
            if level < 0 {
                return i as isize;
            }
        }
        i += 1;
    }
    if level > 0 { -2 } else { -1 }
}

fn split_cells(row: &str) -> usize {
    // Pipes not escaped become " |", then the row splits on " |".
    let b = row.as_bytes();
    let mut s = String::new();
    for (i, c) in row.char_indices() {
        if c == '|' {
            let mut escaped = false;
            let mut j = i;
            while j > 0 && b[j - 1] == b'\\' {
                escaped = !escaped;
                j -= 1;
            }
            s.push_str(if escaped { "|" } else { " |" });
        } else {
            s.push(c);
        }
    }
    let mut cells: Vec<&str> = s.split(" |").collect();
    if jsre::trim(cells[0]).is_empty() {
        cells.remove(0);
    }
    if !cells.is_empty() && jsre::trim(cells[cells.len() - 1]).is_empty() {
        cells.pop();
    }
    cells.len()
}

fn decode_numeric_refs(text: &str) -> String {
    if !NUMERIC_REF.test(text) {
        return text.to_owned();
    }
    NUMERIC_REF_OR_AMP.replace(text, true, |m| {
        if m.get(1).is_none() && m.get(2).is_none() {
            return "&amp;".into();
        }
        let code = match m.get(1) {
            Some(d) => d.parse::<u32>().unwrap_or(0),
            None => u32::from_str_radix(m.str(2), 16).unwrap_or(0),
        };
        if code == 0 || code > 0x10ffff || (0xd800..=0xdfff).contains(&code) {
            return "\u{fffd}".into();
        }
        if code == 38 {
            return "&amp;".into();
        }
        char::from_u32(code).map(String::from).unwrap_or_else(|| "\u{fffd}".into())
    })
}

fn first_char(s: &str) -> Option<char> {
    s.chars().next()
}

impl Lexer {
    fn block_tokens(&mut self, src: &str, tokens: &mut Vec<Block>, mut last_paragraph_clipped: bool) {
        let mut src = src;
        while !src.is_empty() {
            let last_kind = tokens.last().map(|t| t.kind);
            // space
            if let Some(m) = NEWLINE.exec(src).filter(|m| !m.whole().is_empty()) {
                let raw = m.whole();
                src = &src[raw.len()..];
                if raw.len() == 1 && !tokens.is_empty() {
                    tokens.last_mut().unwrap().raw.push('\n');
                } else {
                    tokens.push(Block::new(Kind::Space, raw));
                }
                continue;
            }
            // code
            if let Some(m) = CODE.exec(src) {
                let raw = trim_trailing_blank_lines(m.whole());
                src = &src[raw.len()..];
                if matches!(last_kind, Some(Kind::Paragraph | Kind::Text)) {
                    let last = tokens.last_mut().unwrap();
                    if !last.raw.ends_with('\n') {
                        last.raw.push('\n');
                    }
                    last.raw.push_str(&raw);
                } else {
                    tokens.push(Block::new(Kind::Code, raw));
                }
                continue;
            }
            // fences
            if let Some(m) = FENCES.exec(src) {
                let raw = m.whole();
                src = &src[raw.len()..];
                tokens.push(Block::new(Kind::Code, raw));
                continue;
            }
            // heading
            if let Some(m) = HEADING.exec(src) {
                let mut text = jsre::trim(m.str(2)).to_owned();
                if text.ends_with('#') {
                    let trimmed = rtrim(&text, '#');
                    if trimmed.is_empty() || trimmed.ends_with([' ', '\t']) {
                        text = jsre::trim(trimmed).to_owned();
                    }
                }
                let raw = rtrim(m.whole(), '\n');
                let mut b = Block::new(Kind::Heading, raw);
                b.depth = m.str(1).len();
                b.text = text;
                src = &src[raw.len()..];
                tokens.push(b);
                continue;
            }
            // hr
            if let Some(m) = HR.exec(src) {
                let raw = rtrim(m.whole(), '\n');
                src = &src[raw.len()..];
                tokens.push(Block::new(Kind::Hr, raw));
                continue;
            }
            // blockquote
            if let Some(b) = self.blockquote(src) {
                src = &src[b.raw.len()..];
                tokens.push(b);
                continue;
            }
            // list
            if let Some(raw) = self.list(src, last_kind) {
                src = &src[raw.len()..];
                tokens.push(Block::new(Kind::List, raw));
                continue;
            }
            // html
            if let Some(m) = HTML.exec(src) {
                let raw = trim_trailing_blank_lines(m.whole());
                src = &src[raw.len()..];
                tokens.push(Block::new(Kind::Html, raw));
                continue;
            }
            // def
            if let Some(m) = DEF.exec(src).filter(|m| START_ANGLE.test(m.str(2)) || find_closing_bracket(m.str(2), b'(', b')') == -1) {
                let raw = rtrim(m.whole(), '\n').to_owned();
                src = &src[raw.len()..];
                if matches!(last_kind, Some(Kind::Paragraph | Kind::Text)) {
                    let last = tokens.last_mut().unwrap();
                    if !last.raw.ends_with('\n') {
                        last.raw.push('\n');
                    }
                    last.raw.push_str(&raw);
                } else {
                    self.links.entry(normalize_label(m.str(1))).or_insert(());
                    tokens.push(Block::new(Kind::Def, raw));
                }
                continue;
            }
            // table
            if let Some(m) = TABLE.exec(src).filter(|m| {
                TABLE_DELIMITER.test(m.str(2)) && {
                    let aligns = TABLE_ALIGN_CHARS.replace(m.str(2), true, |_| String::new());
                    split_cells(m.str(1)) == aligns.split('|').count()
                }
            }) {
                let raw = rtrim(m.whole(), '\n');
                src = &src[raw.len()..];
                tokens.push(Block::new(Kind::Table, raw));
                continue;
            }
            // lheading
            if let Some(m) = LHEADING.exec(src) {
                let raw = rtrim(m.whole(), '\n');
                let mut b = Block::new(Kind::Heading, raw);
                b.text = jsre::trim(m.str(1)).to_owned();
                b.depth = if m.str(2).starts_with('=') { 1 } else { 2 };
                src = &src[raw.len()..];
                tokens.push(b);
                continue;
            }
            // paragraph (always top level here)
            if let Some(m) = PARAGRAPH.exec(src) {
                let raw = m.whole();
                if last_paragraph_clipped && last_kind == Some(Kind::Paragraph) {
                    let last = tokens.last_mut().unwrap();
                    if !last.raw.ends_with('\n') {
                        last.raw.push('\n');
                    }
                    last.raw.push_str(raw);
                } else {
                    tokens.push(Block::new(Kind::Paragraph, raw));
                }
                last_paragraph_clipped = false;
                src = &src[raw.len()..];
                continue;
            }
            // text
            if let Some(m) = TEXT.exec(src) {
                let raw = m.whole();
                src = &src[raw.len()..];
                if last_kind == Some(Kind::Text) {
                    let last = tokens.last_mut().unwrap();
                    if !last.raw.ends_with('\n') {
                        last.raw.push('\n');
                    }
                    last.raw.push_str(raw);
                } else {
                    tokens.push(Block::new(Kind::Text, raw));
                }
                continue;
            }
            // (Nothing matched: marked throws; stop.)
            break;
        }
    }

    fn blockquote(&mut self, src: &str) -> Option<Block> {
        let cap = BLOCKQUOTE.exec(src)?;
        let whole = rtrim(cap.whole(), '\n');
        let mut lines: Vec<String> = whole.split('\n').map(str::to_owned).collect();
        let mut raw = String::new();
        let mut text = String::new();
        let mut tokens: Vec<Block> = Vec::new();
        while !lines.is_empty() {
            let mut in_bq = false;
            let mut current = Vec::new();
            let mut i = 0;
            while i < lines.len() {
                if BLOCKQUOTE_START.test(&lines[i]) {
                    current.push(lines[i].clone());
                    in_bq = true;
                } else if !in_bq {
                    current.push(lines[i].clone());
                } else {
                    break;
                }
                i += 1;
            }
            lines.drain(..i);
            let current_raw = current.join("\n");
            let current_text = BLOCKQUOTE_MARKER_WHITESPACE.replace(&current_raw, true, |m| {
                format!("{}{}", m.str(1), expand_tabs(m.str(2), m.str(1).chars().count()))
            });
            let current_text = BLOCKQUOTE_SETEXT_REPLACE.replace(&current_text, true, |m| format!("\n    {}", m.str(1)));
            let current_text = BLOCKQUOTE_SETEXT_REPLACE2.replace(&current_text, true, |_| String::new());
            raw = if raw.is_empty() { current_raw } else { format!("{raw}\n{current_raw}") };
            text = if text.is_empty() { current_text.clone() } else { format!("{text}\n{current_text}") };
            self.block_tokens(&current_text, &mut tokens, true);
            if lines.is_empty() {
                break;
            }
            let last = tokens.last().map(|t| t.kind);
            match last {
                Some(Kind::Code | Kind::Heading | Kind::Hr | Kind::Html | Kind::Table) => break,
                Some(Kind::Blockquote) => {
                    let old = tokens.last().unwrap().clone();
                    let continuation = lines.join("\n");
                    let new_text = format!("{}\n{}", old.raw, BLOCKQUOTE_SETEXT_REPLACE2.replace(&continuation, true, |_| String::new()));
                    let new = self.blockquote(&new_text)?;
                    let leftover = new_text[new.raw.len()..].strip_prefix('\n').unwrap_or(&new_text[new.raw.len()..]).to_owned();
                    let leftover_count = if leftover.is_empty() { 0 } else { leftover.split('\n').count() };
                    let consumed: Vec<String> = if leftover_count > 0 { lines[..lines.len().saturating_sub(leftover_count)].to_vec() } else { lines.clone() };
                    if !consumed.is_empty() {
                        raw = format!("{raw}\n{}", consumed.join("\n"));
                    }
                    let old_len = old.bq_text.len();
                    text = format!("{}{}", &text[..text.len().saturating_sub(old_len)], new.bq_text);
                    let n = tokens.len();
                    tokens[n - 1] = new;
                    lines.drain(..consumed.len());
                    if !lines.is_empty() && BLOCKQUOTE_START.test(&lines[0]) {
                        continue;
                    }
                    break;
                }
                Some(Kind::List) => {
                    let old_raw = tokens.last().unwrap().raw.clone();
                    let new_text = format!("{old_raw}\n{}", lines.join("\n"));
                    let new_raw = self.list(&new_text, None)?;
                    let consumed_count = if new_raw.len() > old_raw.len() {
                        new_text[old_raw.len() + 1..new_raw.len()].split('\n').count()
                    } else {
                        0
                    };
                    let n = tokens.len();
                    tokens[n - 1] = Block::new(Kind::List, new_raw);
                    if consumed_count > 0 {
                        let consumed = lines[..consumed_count.min(lines.len())].join("\n");
                        raw = format!("{raw}\n{consumed}");
                        text = format!("{text}\n{consumed}");
                    }
                    lines.drain(..consumed_count.min(lines.len()));
                    continue;
                }
                _ => {}
            }
        }
        let mut b = Block::new(Kind::Blockquote, raw);
        b.bq_text = text;
        Some(b)
    }

    /// A list's raw text (its items' contents aren't needed).
    fn list(&mut self, src: &str, last_kind: Option<Kind>) -> Option<String> {
        let cap = LIST.exec(src)?;
        if last_kind == Some(Kind::Text) && !LIST_INTERRUPTS_PARAGRAPH.test(src) {
            return None;
        }
        let list_src = src;
        let mut src = src;
        let bull = jsre::trim(cap.str(1)).to_owned();
        let ordered = bull.len() > 1;
        let item_re = item_pattern(if ordered { &bull[bull.len() - 1..] } else { &bull }, ordered);
        let mut items = 0;
        while !src.is_empty() {
            let Some(cap) = item_re.exec(src) else { break };
            if HR.test(src) {
                break;
            }
            let item_start = list_src.len() - src.len();
            let cap0_len = cap.whole().len();
            let first_line = cap.str(2).split('\n').next().unwrap_or("").to_owned();
            let bullet_indent = cap.str(1).chars().count();
            src = &src[cap0_len..];
            let mut line = LEADING_SPACE_TAB.replace(&first_line, false, |m| expand_tabs(m.whole(), bullet_indent));
            let mut next_line = src.split('\n').next().unwrap_or("").to_owned();
            let mut blank_line = jsre::trim(&line).is_empty();
            let mut indent;
            let mut end_early = false;
            if blank_line {
                indent = bullet_indent + 1;
            } else {
                indent = NON_SPACE_CHAR.search(&line).max(0) as usize;
                indent = if indent > 4 { 1 } else { indent };
                indent += bullet_indent;
            }
            if blank_line && BLANK_LINE.test(&next_line) {
                src = src.get(next_line.len() + 1..).unwrap_or("");
                end_early = true;
            }
            if !end_early {
                let next_bullet = indent_regex(&NEXT_BULLET, indent);
                let hr_re = indent_regex(&HR_AT, indent);
                let fences_begin = indent_regex(&FENCES_BEGIN, indent);
                let heading_begin = indent_regex(&HEADING_BEGIN, indent);
                let html_begin = indent_regex(&HTML_BEGIN, indent);
                let bq_begin = indent_regex(&BLOCKQUOTE_BEGIN, indent);
                let mut line_is_lazy = false;
                while !src.is_empty() {
                    let raw_line = src.split('\n').next().unwrap_or("").to_owned();
                    next_line = raw_line.clone();
                    let without_tabs = LEADING_SPACE_TAB.replace(&next_line, false, |m| expand_tabs(m.whole(), 0));
                    if fences_begin.test(&next_line)
                        || heading_begin.test(&next_line)
                        || html_begin.test(&next_line)
                        || bq_begin.test(&next_line)
                        || next_bullet.test(&next_line)
                        || hr_re.test(&next_line)
                    {
                        break;
                    }
                    let ns = NON_SPACE_CHAR.search(&without_tabs);
                    if ns >= indent as isize || jsre::trim(&next_line).is_empty() {
                        line_is_lazy = false;
                    } else {
                        if blank_line {
                            break;
                        }
                        let tabbed = TAB_CHAR.replace(&line, true, |_| "    ".into());
                        if NON_SPACE_CHAR.search(&tabbed) >= 4 {
                            break;
                        }
                        if fences_begin.test(&line) || heading_begin.test(&line) || html_begin.test(&line) || hr_re.test(&line) {
                            break;
                        }
                        if !line_is_lazy {
                            let last_block_line = NESTED_CONTAINER_MARKERS.replace(&line, false, |_| String::new());
                            if FENCES.test(&last_block_line) || HEADING.test(&last_block_line) || HR.test(&last_block_line) {
                                break;
                            }
                        }
                        line_is_lazy = true;
                    }
                    blank_line = jsre::trim(&next_line).is_empty();
                    src = src.get(raw_line.len() + 1..).unwrap_or("");
                    line = slice_from(&without_tabs, indent);
                }
            }
            let _ = item_start;
            items += 1;
        }
        if items == 0 {
            return None;
        }
        let taken = list_src.len() - src.len();
        Some(jsre::trim_end(&list_src[..taken]).to_owned())
    }

    // ---- Inline ----

    fn inline_tokens(&mut self, src: &str) -> Vec<Inline> {
        let prev = self.link_paren_possible;
        self.link_paren_possible = prev && src.contains(')');
        let masked = if src.contains(['*', '_', '~']) { self.mask_src(src) } else { src.to_owned() };
        let out = self.tokenize_inlines(src, &masked);
        self.link_paren_possible = prev;
        out
    }

    fn tokenize_inlines(&mut self, src0: &str, masked: &str) -> Vec<Inline> {
        let mut tokens: Vec<Inline> = Vec::new();
        let mut src = src0;
        let mut keep_prev = false;
        let mut prev_char = String::new();
        let closers = last_closers(masked);
        while !src.is_empty() {
            if !keep_prev {
                prev_char.clear();
            }
            keep_prev = false;
            // escape
            if let Some(m) = ESCAPE.exec(src) {
                src = &src[m.whole().len()..];
                tokens.push(Inline::Escape(m.str(1).to_owned()));
                continue;
            }
            // tag
            if let Some(m) = INLINE_TAG.exec(src) {
                let raw = m.whole();
                if !self.in_link && START_A_TAG.test(raw) {
                    self.in_link = true;
                } else if self.in_link && END_A_TAG.test(raw) {
                    self.in_link = false;
                }
                if !self.in_raw_block && START_PRE_SCRIPT.test(raw) {
                    self.in_raw_block = true;
                } else if self.in_raw_block && END_PRE_SCRIPT.test(raw) {
                    self.in_raw_block = false;
                }
                src = &src[raw.len()..];
                tokens.push(Inline::Html);
                continue;
            }
            // link
            if let Some((len, t)) = self.link(src) {
                src = &src[len..];
                tokens.push(t);
                continue;
            }
            // reflink, nolink
            if let Some((len, t)) = self.reflink(src) {
                src = &src[len..];
                if let (Inline::Text(s), Some(Inline::Text(last))) = (&t, tokens.last_mut()) {
                    last.push_str(s);
                } else {
                    tokens.push(t);
                }
                continue;
            }
            // em & strong
            if let Some((len, t)) = self.em_strong(src, masked, &prev_char, &closers) {
                src = &src[len..];
                tokens.push(t);
                continue;
            }
            // code
            if let Some(m) = INLINE_CODE.exec(src) {
                let mut text = NEW_LINE_CHAR.replace(m.str(2), true, |_| " ".into());
                if NON_SPACE_CHAR.test(&text) && STARTING_SPACE.test(&text) && ENDING_SPACE.test(&text) {
                    text = text[1..text.len() - 1].to_owned();
                }
                src = &src[m.whole().len()..];
                tokens.push(Inline::Codespan(text));
                continue;
            }
            // br
            if let Some(m) = BR.exec(src) {
                src = &src[m.whole().len()..];
                tokens.push(Inline::Br);
                continue;
            }
            // del
            if let Some((len, t)) = self.del(src, masked, &prev_char, &closers) {
                src = &src[len..];
                tokens.push(t);
                continue;
            }
            // autolink
            if let Some(m) = AUTOLINK.exec(src) {
                src = &src[m.whole().len()..];
                tokens.push(Inline::Link(vec![Inline::Text(m.str(1).to_owned())]));
                continue;
            }
            // url
            if !self.in_link {
                if let Some(m) = URL.exec(src) {
                    let mut raw = m.whole().to_owned();
                    if m.get(2) != Some("@") {
                        loop {
                            let prev = raw.clone();
                            raw = BACKPEDAL.exec(&raw).map(|b| b.whole().to_owned()).unwrap_or_default();
                            if prev == raw {
                                break;
                            }
                        }
                    }
                    if !raw.is_empty() {
                        src = &src[raw.len()..];
                        tokens.push(Inline::Link(vec![Inline::Text(raw)]));
                        continue;
                    }
                }
            }
            // text
            if let Some(m) = INLINE_TEXT.exec(src) {
                let raw = m.whole();
                if raw.is_empty() {
                    break;
                }
                let text = if self.in_raw_block { raw.to_owned() } else { decode_numeric_refs(raw) };
                src = &src[raw.len()..];
                if !raw.ends_with('_') {
                    prev_char = raw.chars().last().map(String::from).unwrap_or_default();
                }
                keep_prev = true;
                if let Some(Inline::Text(last)) = tokens.last_mut() {
                    last.push_str(&text);
                } else {
                    tokens.push(Inline::Text(text));
                }
                continue;
            }
            break;
        }
        tokens
    }

    fn output_link(&mut self, cap1: &str, is_image: bool) -> Option<Inline> {
        let text = OUTPUT_LINK_REPLACE.replace(cap1, true, |m| m.str(1).to_owned());
        let (outer_in_link, outer_emitted, outer_raw) = (self.in_link, self.link_emitted, self.in_raw_block);
        self.in_link = true;
        self.link_emitted = false;
        let tokens = self.inline_tokens(&text);
        let has_link = self.link_emitted;
        self.link_emitted = outer_emitted;
        self.in_link = outer_in_link;
        if !is_image {
            if has_link {
                self.in_raw_block = outer_raw;
                return None;
            }
            self.link_emitted = true;
        }
        Some(Inline::Link(tokens))
    }

    fn link(&mut self, src: &str) -> Option<(usize, Inline)> {
        if !self.link_paren_possible {
            return None;
        }
        let cap = LINK.exec(src)?;
        let mut cap0 = cap.whole().to_owned();
        let mut cap2 = cap.str(2).to_owned();
        let label_start = if cap0.starts_with('!') { 2 } else { 1 };
        if label_end_inside_token(src, cap.str(1), label_start) {
            return None;
        }
        let trimmed_url = jsre::trim(&cap2).to_owned();
        if START_ANGLE.test(&trimmed_url) {
            if !END_ANGLE.test(&trimmed_url) {
                return None;
            }
            let rtrim_slash = rtrim(&trimmed_url[..trimmed_url.len() - 1], '\\');
            if (trimmed_url.len() - rtrim_slash.len()) % 2 == 0 {
                return None;
            }
        } else {
            let last_paren = find_closing_bracket(&cap2, b'(', b')');
            if last_paren == -2 {
                return None;
            }
            if last_paren > -1 {
                let start = if cap0.starts_with('!') { 5 } else { 4 };
                let link_len = start + cap.str(1).len() + last_paren as usize;
                cap2.truncate(last_paren as usize);
                let cut = cap0.get(..link_len.min(cap0.len())).unwrap_or(&cap0);
                cap0 = jsre::trim(cut).to_owned();
            }
        }
        let unescaped = rtrim(&cap0[..cap0.len() - 1], '\\');
        if (cap0.len() - unescaped.len()) % 2 == 0 {
            return None;
        }
        let is_image = cap0.starts_with('!');
        let t = self.output_link(cap.str(1), is_image)?;
        Some((cap0.len(), t))
    }

    fn reflink(&mut self, src: &str) -> Option<(usize, Inline)> {
        let cap = REFLINK.exec(src).or_else(|| NOLINK.exec(src))?;
        let label_start = if cap.whole().starts_with('!') { 2 } else { 1 };
        if label_end_inside_token(src, cap.str(1), label_start) {
            return None;
        }
        let label = if cap.has(2) { cap.str(2) } else { cap.str(1) };
        if !self.links.contains_key(&normalize_label(label)) {
            let c = first_char(cap.whole()).unwrap().to_string();
            return Some((c.len(), Inline::Text(c)));
        }
        let len = cap.whole().len();
        let is_image = cap.whole().starts_with('!');
        let t = self.output_link(cap.str(1), is_image)?;
        Some((len, t))
    }

    fn em_strong(&mut self, src: &str, masked_full: &str, prev_char: &str, closers: &Closers) -> Option<(usize, Inline)> {
        let m = EM_STRONG_LDELIM.exec(src)?;
        if !m.has(1) && !m.has(2) && !m.has(3) && !m.has(4) {
            return None;
        }
        let delim = m.whole().as_bytes()[0];
        let mid_run = prev_char.as_bytes().first() == Some(&delim) && prev_char.len() == 1;
        let src_start = masked_full.len() - src.len();
        let mut run_start = src_start;
        while run_start > 0 && masked_full.as_bytes()[run_start - 1] == delim {
            run_start -= 1;
        }
        let run_prev_char = masked_full[..run_start].chars().last().map(String::from).unwrap_or_default();
        let is_punct = |c: &str| c == "*" || c == "_" || (!c.is_empty() && PUNCTUATION.test(c));
        let run_prev_punct = is_punct(&run_prev_char);
        let mut prev = prev_char.to_owned();
        if run_start < src_start {
            prev = run_prev_char.clone();
        }
        let prev_punct = if run_start < src_start { run_prev_punct } else { is_punct(&prev) };
        if m.has(4) && UNICODE_ALNUM.test(&prev) {
            return None;
        }
        let next_char = if m.has(1) { m.str(1) } else { m.str(3) };
        if !(next_char.is_empty() || prev.is_empty() || prev_punct) {
            return None;
        }
        let l_length = m.whole().chars().count() - 1;
        let mut delim_total = l_length as isize;
        let mut mid_delim_total = 0isize;
        let run_length = src_start - run_start + l_length;
        let opener_can_close = !jsre::trim(&run_prev_char).is_empty() && (!next_char.is_empty() || (delim == b'*' && !run_prev_punct));
        let end_re: &Re = if delim == b'*' { &EM_STRONG_RDELIM_AST } else { &EM_STRONG_RDELIM_UND };
        let last_possible = if delim == b'*' { closers.ast } else { closers.und };
        let window_start = masked_full.len() - src.len() + l_length;
        if last_possible < window_start as isize {
            return None;
        }
        let masked = &masked_full[window_start..];
        let last_in_window = last_possible - window_start as isize;
        let mut pos = 0;
        while let Some(mm) = end_re.exec_at(masked, pos) {
            pos = mm.end().max(mm.start() + 1);
            if mm.start() as isize > last_in_window {
                break;
            }
            let r = (1..=6).find(|&i| mm.has(i));
            let Some(ri) = r else { continue };
            let r_delim = mm.str(ri);
            let mut r_length = r_delim.chars().count() as isize;
            if ri == 3 || ri == 4 {
                delim_total += r_length;
                continue;
            } else if ri == 5 || ri == 6 || opener_can_close {
                if mid_run && (ri == 5 || ri == 6) {
                    break;
                }
                if run_length % 3 != 0 && (run_length as isize + r_length) % 3 == 0 {
                    mid_delim_total += r_length;
                    continue;
                }
            }
            delim_total -= r_length;
            if delim_total > 0 {
                continue;
            }
            r_length = r_length.min(r_length + delim_total + mid_delim_total);
            let last_char_len = mm.whole().chars().next().map(char::len_utf8).unwrap_or(0);
            let end = l_length + mm.start() + last_char_len + r_length as usize;
            let raw = src.get(..end.min(src.len()))?;
            if (l_length as isize).min(r_length) % 2 == 1 {
                let text = &raw[1..raw.len() - 1];
                let t = self.inline_tokens(text);
                return Some((raw.len(), Inline::Em(t)));
            }
            let text = raw.get(2..raw.len().saturating_sub(2)).unwrap_or("");
            let t = self.inline_tokens(text);
            return Some((raw.len(), Inline::Strong(t)));
        }
        None
    }

    fn del(&mut self, src: &str, masked_full: &str, prev_char: &str, closers: &Closers) -> Option<(usize, Inline)> {
        let m = DEL_LDELIM.exec(src)?;
        let next_char = m.str(1);
        if !(next_char.is_empty() || prev_char.is_empty() || PUNCTUATION.test(prev_char)) {
            return None;
        }
        let l_length = m.whole().chars().count() - 1;
        let mut delim_total = l_length as isize;
        let window_start = masked_full.len() - src.len() + l_length;
        if closers.del < window_start as isize {
            return None;
        }
        let masked = &masked_full[window_start..];
        let last_in_window = closers.del - window_start as isize;
        let mut pos = 0;
        while let Some(mm) = DEL_RDELIM.exec_at(masked, pos) {
            pos = mm.end().max(mm.start() + 1);
            if mm.start() as isize > last_in_window {
                break;
            }
            let Some(ri) = (1..=6).find(|&i| mm.has(i)) else { continue };
            let mut r_length = mm.str(ri).chars().count() as isize;
            if r_length != l_length as isize {
                continue;
            }
            if ri == 3 || ri == 4 {
                delim_total += r_length;
                continue;
            }
            delim_total -= r_length;
            if delim_total > 0 {
                continue;
            }
            r_length = r_length.min(r_length + delim_total);
            let last_char_len = mm.whole().chars().next().map(char::len_utf8).unwrap_or(0);
            let end = l_length + mm.start() + last_char_len + r_length as usize;
            let raw = src.get(..end.min(src.len()))?;
            let text = raw.get(l_length..raw.len().saturating_sub(l_length)).unwrap_or("");
            let t = self.inline_tokens(text);
            return Some((raw.len(), Inline::Del(t)));
        }
        None
    }

    fn link_in_text(&self, text: &str) -> bool {
        if !text.contains('[') {
            return false;
        }
        for m in BLOCK_SKIP.all(text) {
            let before = text[..m.start()].chars().last();
            if LINK.test(m.whole()) && before != Some('!') {
                return true;
            }
        }
        for m in REFLINK_SEARCH.all(text) {
            let m0 = m.whole();
            let ref_start = m0.rfind('[').unwrap();
            let label = &m0[ref_start + 1..m0.len() - 1];
            if m0.starts_with('!') || !self.links.contains_key(&normalize_label(label)) {
                continue;
            }
            if ref_start > 1 && self.link_in_text(&m0[1..ref_start - 1]) {
                continue;
            }
            return true;
        }
        false
    }

    fn mask_reflink(&self, m0: &str) -> String {
        let ref_start = m0.rfind('[').unwrap();
        if !self.links.contains_key(&normalize_label(&m0[ref_start + 1..m0.len() - 1])) {
            return m0.to_owned();
        }
        if ref_start > 1 && !m0.starts_with('!') {
            let text = &m0[1..ref_start - 1];
            if self.link_in_text(text) {
                let inner = REFLINK_SEARCH.replace(text, true, |m| self.mask_reflink(m.whole()));
                return format!("[{inner}][{}]", "a".repeat(m0.len() - ref_start - 2));
            }
        }
        format!("[{}]", "a".repeat(m0.len() - 2))
    }

    fn mask_src(&self, src: &str) -> String {
        let mut masked = src.to_owned();
        if !self.links.is_empty() && src.contains('[') {
            masked = REFLINK_SEARCH.replace(&masked, true, |m| self.mask_reflink(m.whole()));
        }
        masked = ANY_PUNCTUATION_OUTSIDE_CODE.replace(&masked, true, |m| {
            if m.has(2) {
                return "+".repeat(m.whole().len());
            }
            if let Some(code) = m.get(1).filter(|c| !c.is_empty()) {
                let w = m.whole();
                return format!("{code}{}{code}", w[code.len()..w.len() - code.len()].replace('`', "+"));
            }
            m.whole().to_owned()
        });
        mask_blocks(&masked)
    }
}

fn slice_from(s: &str, n: usize) -> String {
    jsre::slice16(s, n as isize, None).to_owned()
}

fn mask_blocks(src: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    let mut pos = 0;
    while let Some(m) = BLOCK_SKIP.exec_at(src, pos) {
        let start = m.start() + m.get(2).map(str::len).unwrap_or(0);
        let mut end = m.end();
        if src[start..].starts_with('<') {
            let rest = &src[start..];
            let html = INLINE_TAG.exec(rest).or_else(|| AUTOLINK.exec(rest));
            match html {
                None => {
                    pos = start + 1;
                    continue;
                }
                Some(h) => end = start + h.whole().len(),
            }
        }
        out.push_str(&src[last..start]);
        out.push('[');
        out.push_str(&"a".repeat(end.saturating_sub(start).saturating_sub(2)));
        out.push(']');
        last = end;
        pos = if end > m.start() { end } else { m.start() + 1 };
    }
    out.push_str(&src[last..]);
    out
}

fn label_end_inside_token(src: &str, label: &str, label_start: usize) -> bool {
    if !label.contains('<') {
        return false;
    }
    let b = label.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'`' {
            if let Some(code) = INLINE_CODE.exec(&label[i..]) {
                i += code.whole().len();
                continue;
            }
        }
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let token_src = &src[(label_start + i).min(src.len())..];
        if let Some(t) = INLINE_TAG.exec(token_src).or_else(|| AUTOLINK.exec(token_src)) {
            if t.whole().len() > label.len() - i {
                return true;
            }
            i += t.whole().len();
            continue;
        }
        i += 1;
    }
    false
}

/// The last place an emphasis or strikethrough closer could be in a masked
/// run (byte indices, -1 for none).
struct Closers {
    ast: isize,
    und: isize,
    del: isize,
}

fn last_closers(src: &str) -> Closers {
    let b = src.as_bytes();
    let mut c = Closers { ast: -1, und: -1, del: -1 };
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if ch != b'*' && ch != b'_' && ch != b'~' {
            i += 1;
            continue;
        }
        let prev = if i == 0 { b'\n' } else { b[i - 1] };
        let mut j = i + 1;
        while j < b.len() && b[j] == ch {
            j += 1;
        }
        if !matches!(prev, b' ' | b'\t' | b'\n' | b'\r' | 0x0c) {
            match ch {
                b'~' => c.del = i as isize,
                b'*' => c.ast = i as isize,
                _ => c.und = i as isize,
            }
        }
        i = j;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(Kind, String)> {
        lex(src).into_iter().map(|b| (b.kind, b.raw)).collect()
    }

    #[test]
    fn blocks_keep_their_source() {
        let src = "# Title\n\nIntro.\n\n## A\n\n- one\n- two\n\n> quote\n\n```\n# code\n```\n\nSetext\n---\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let joined: String = lex(src).iter().map(|b| b.raw.as_str()).collect();
        assert_eq!(joined, src);
        let k: Vec<Kind> = kinds(src).into_iter().map(|(k, _)| k).filter(|k| *k != Kind::Space).collect();
        assert_eq!(k, [Kind::Heading, Kind::Paragraph, Kind::Heading, Kind::List, Kind::Blockquote, Kind::Code, Kind::Heading, Kind::Table]);
    }

    #[test]
    fn heading_text_and_inline() {
        let t = lex("## Queue `enqueue()` and <em>dequeue</em> ##\n");
        assert_eq!(t[0].text, "Queue `enqueue()` and <em>dequeue</em>");
        assert_eq!(inline_text(&t[0].inline), "Queue enqueue() and dequeue");
        let t = lex("## _private_ my_var *x* [link](a.md) \\_ &#65;\n");
        assert_eq!(inline_text(&t[0].inline), "private my_var x link _ A");
    }
}
