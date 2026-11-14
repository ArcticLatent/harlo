//! A page's front matter as OpenWiki keeps it (its knowledge format): the
//! checks, the deterministic repair, the field edits, and the verification
//! stamps Claims put there. Lists are written as OpenWiki's YAML library
//! writes them (no line folding).

use crate::claims::Verification;
use crate::jsre::{Re, is_js_space};
use crate::store::{Result, Store};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::sync::LazyLock;

pub const GENERATED_FIELD: &str = "openwiki_generated";
const STRING_FIELDS: [&str; 5] = ["type", "title", "description", "resource", "timestamp"];
const OPTIONAL_STRING_FIELDS: [&str; 3] = ["description", "resource", "timestamp"];
const STATUS_VALUES: [&str; 3] = ["draft", "stable", "deprecated"];

// ---- YAML out (the `yaml` library's stringify, lineWidth 0) ----

const INDENT_STEP: &str = "  ";

#[derive(Clone)]
struct Ctx {
    indent: String,
    implicit_key: bool,
    force_block_indent: bool,
}

/// `stringify(value, { lineWidth: 0 })`.
pub fn yaml(v: &Value) -> String {
    let ctx = Ctx { indent: String::new(), implicit_key: false, force_block_indent: false };
    format!("{}\n", node(v, &ctx))
}

fn node(v: &Value, ctx: &Ctx) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) => crate::front::js_number(f),
            _ => n.to_string(),
        },
        Value::String(s) => string(s, &mut ctx.clone()),
        Value::Array(items) => {
            if items.is_empty() {
                return "[]".into();
            }
            let item_ctx = Ctx { indent: format!("{}{INDENT_STEP}", ctx.indent), implicit_key: false, force_block_indent: false };
            block(items.iter().map(|i| format!("- {}", node(i, &item_ctx))).collect(), &ctx.indent)
        }
        Value::Object(m) => {
            if m.is_empty() {
                return "{}".into();
            }
            block(m.iter().map(|(k, v)| pair(k, v, ctx)).collect(), &ctx.indent)
        }
    }
}

fn block(lines: Vec<String>, indent: &str) -> String {
    let mut out = lines[0].clone();
    for l in &lines[1..] {
        if l.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&format!("\n{indent}{l}"));
        }
    }
    out
}

fn pair(key: &str, value: &Value, ctx: &Ctx) -> String {
    let inner = format!("{}{INDENT_STEP}", ctx.indent);
    let k = string(key, &mut Ctx { indent: inner.clone(), implicit_key: true, force_block_indent: false });
    let mut out = format!("{k}:");
    let vctx = Ctx { indent: inner.clone(), implicit_key: false, force_block_indent: false };
    let v = node(value, &vctx);
    let ws = match value {
        Value::Array(a) if !a.is_empty() => format!("\n{inner}"),
        Value::Object(m) if !m.is_empty() => format!("\n{inner}"),
        Value::Array(_) | Value::Object(_) => if v.contains('\n') { format!("\n{inner}") } else { " ".into() },
        _ if v.is_empty() || v.starts_with('\n') => String::new(),
        _ => " ".into(),
    };
    out.push_str(&ws);
    out.push_str(&v);
    out
}

static DOC_MARKER: LazyLock<Re> = LazyLock::new(|| Re::new(r"^(%|---|\.\.\.)", "m"));
static PLAIN_FORBIDDEN: LazyLock<Re> = LazyLock::new(|| Re::new(r#"^[\n\t ,[\]{}#&*!|>'"%@`]|^[?-]$|^[?-][ \t]|[\n:][ \t]|[ \t]\n|[\n\t ]#|[\n\t :]$"#, ""));
static NEEDS_DOUBLE: LazyLock<Re> = LazyLock::new(|| Re::new(r"[\x00-\x08\x0b-\x1f\x7f-\x9f]", "u"));
static CORE_TAGS: LazyLock<Vec<Re>> = LazyLock::new(|| {
    [
        r"^(?:~|[Nn]ull|NULL)?$",
        r"^(?:[Tt]rue|TRUE|[Ff]alse|FALSE)$",
        r"^0o[0-7]+$",
        r"^[-+]?[0-9]+$",
        r"^0x[0-9a-fA-F]+$",
        r"^(?:[-+]?\.(?:inf|Inf|INF)|\.nan|\.NaN|\.NAN)$",
        r"^[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)[eE][-+]?[0-9]+$",
        r"^[-+]?(?:\.[0-9]+|[0-9]+\.[0-9]*)$",
    ]
    .iter()
    .map(|p| Re::new(p, ""))
    .collect()
});

fn string(value: &str, ctx: &mut Ctx) -> String {
    if NEEDS_DOUBLE.test(value) {
        return double_quoted(value, ctx);
    }
    plain(value, ctx)
}

fn plain(value: &str, ctx: &mut Ctx) -> String {
    if ctx.implicit_key && value.contains('\n') {
        return quoted(value, ctx);
    }
    if PLAIN_FORBIDDEN.test(value) {
        return if ctx.implicit_key || !value.contains('\n') { quoted(value, ctx) } else { block_string(value, ctx) };
    }
    if !ctx.implicit_key && value.contains('\n') {
        return block_string(value, ctx);
    }
    if DOC_MARKER.test(value) {
        if ctx.indent.is_empty() {
            ctx.force_block_indent = true;
            return block_string(value, ctx);
        } else if ctx.implicit_key && ctx.indent == INDENT_STEP {
            return quoted(value, ctx);
        }
    }
    let indent = ctx.indent.clone();
    let s = Re::new(r"\n+", "").replace(value, true, |m| format!("{}\n{indent}", m.whole()));
    if CORE_TAGS.iter().any(|t| t.test(&s)) {
        return quoted(value, ctx);
    }
    s
}

fn quoted(value: &str, ctx: &mut Ctx) -> String {
    let has_double = value.contains('"');
    let has_single = value.contains('\'');
    if has_double && !has_single { single_quoted(value, ctx) } else { double_quoted(value, ctx) }
}

fn single_quoted(value: &str, ctx: &mut Ctx) -> String {
    static WS_NL: LazyLock<Re> = LazyLock::new(|| Re::new(r"[ \t]\n|\n[ \t]", ""));
    if (ctx.implicit_key && value.contains('\n')) || WS_NL.test(value) {
        return double_quoted(value, ctx);
    }
    let indent = if !ctx.indent.is_empty() { ctx.indent.clone() } else if DOC_MARKER.test(value) { "  ".into() } else { String::new() };
    let escaped = value.replace('\'', "''");
    let escaped = Re::new(r"\n+", "").replace(&escaped, true, |m| format!("{}\n{indent}", m.whole()));
    format!("'{escaped}'")
}

/// `JSON.stringify` of a string.
pub fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

fn double_quoted(value: &str, ctx: &Ctx) -> String {
    let json: Vec<char> = json_string(value).chars().collect();
    let indent = if !ctx.indent.is_empty() { ctx.indent.clone() } else if DOC_MARKER.test(value) { "  ".into() } else { String::new() };
    let at = |i: usize| json.get(i).copied();
    let slice = |a: usize, b: usize| json[a..b.min(json.len())].iter().collect::<String>();
    let mut out = String::new();
    let mut start = 0;
    let mut i = 0;
    while i < json.len() {
        let mut ch = json[i];
        if ch == ' ' && at(i + 1) == Some('\\') && at(i + 2) == Some('n') {
            out.push_str(&slice(start, i));
            out.push_str("\\ ");
            i += 1;
            start = i;
            ch = '\\';
        }
        if ch == '\\' {
            match at(i + 1) {
                Some('u') => {
                    out.push_str(&slice(start, i));
                    let code = slice(i + 2, i + 6);
                    match code.as_str() {
                        "0000" => out.push_str("\\0"),
                        "0007" => out.push_str("\\a"),
                        "000b" => out.push_str("\\v"),
                        "001b" => out.push_str("\\e"),
                        "0085" => out.push_str("\\N"),
                        "00a0" => out.push_str("\\_"),
                        "2028" => out.push_str("\\L"),
                        "2029" => out.push_str("\\P"),
                        c if c.starts_with("00") => out.push_str(&format!("\\x{}", &c[2..])),
                        _ => out.push_str(&slice(i, i + 6)),
                    }
                    i += 5;
                    start = i + 1;
                }
                Some('n') => {
                    if ctx.implicit_key || at(i + 2) == Some('"') || json.len() < 40 {
                        i += 1;
                    } else {
                        out.push_str(&slice(start, i));
                        out.push_str("\n\n");
                        while at(i + 2) == Some('\\') && at(i + 3) == Some('n') && at(i + 4) != Some('"') {
                            out.push('\n');
                            i += 2;
                        }
                        out.push_str(&indent);
                        if at(i + 2) == Some(' ') {
                            out.push('\\');
                        }
                        i += 1;
                        start = i + 1;
                    }
                }
                _ => i += 1,
            }
        }
        i += 1;
    }
    if start > 0 { format!("{out}{}", slice(start, json.len())) } else { json.iter().collect() }
}

fn block_string(value: &str, ctx: &mut Ctx) -> String {
    static TRAILING_WS_LINE: LazyLock<Re> = LazyLock::new(|| Re::new(r"\n[\t ]+$", ""));
    static END_NEWLINES: LazyLock<Re> = LazyLock::new(|| Re::new(r"(^|(?<!\n))\n+(?!\n|$)", ""));
    if TRAILING_WS_LINE.test(value) {
        return quoted(value, ctx);
    }
    let indent = if !ctx.indent.is_empty() { ctx.indent.clone() } else if ctx.force_block_indent || DOC_MARKER.test(value) { "  ".into() } else { String::new() };
    if value.is_empty() {
        return "|\n".into();
    }
    let chars: Vec<char> = value.chars().collect();
    let mut end_start = chars.len();
    while end_start > 0 && matches!(chars[end_start - 1], '\n' | '\t' | ' ') {
        end_start -= 1;
    }
    let mut end: String = chars[end_start..].iter().collect();
    let chomp = match end.find('\n') {
        None => "-",
        Some(p) if end_start == 0 || p != end.len() - 1 => "+",
        Some(_) => "",
    };
    let mut body: String = chars[..end_start].iter().collect();
    if !end.is_empty() {
        if end.ends_with('\n') {
            end.pop();
        }
        end = END_NEWLINES.replace(&end, true, |m| format!("{}{indent}", m.whole()));
    }
    let bchars: Vec<char> = body.chars().collect();
    let mut start_with_space = false;
    let mut start_end = 0;
    let mut start_nl: isize = -1;
    while start_end < bchars.len() {
        match bchars[start_end] {
            ' ' => start_with_space = true,
            '\n' => start_nl = start_end as isize,
            _ => break,
        }
        start_end += 1;
    }
    let take = if start_nl < start_end as isize { (start_nl + 1) as usize } else { start_end };
    let mut start: String = bchars[..take].iter().collect();
    if !start.is_empty() {
        body = bchars[take..].iter().collect();
        start = Re::new(r"\n+", "").replace(&start, true, |m| format!("{}{indent}", m.whole()));
    }
    let indent_size = if indent.is_empty() { "1" } else { "2" };
    let header = format!("{}{chomp}", if start_with_space { indent_size } else { "" });
    let body = Re::new(r"\n+", "").replace(&body, true, |m| format!("{}{indent}", m.whole()));
    format!("|{header}\n{indent}{start}{body}{end}")
}

// ---- Front matter ----

static BLOCK: LazyLock<Re> = LazyLock::new(|| Re::new(r"^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)", "u"));

/// (block, body) as `splitFrontmatter`.
pub fn split(content: &str) -> (Option<String>, String) {
    match BLOCK.exec(content) {
        Some(m) => (m.get(1).map(str::to_owned), content[m.end()..].to_owned()),
        None => (None, content.to_owned()),
    }
}

pub fn fields(content: &str) -> Option<Map<String, Value>> {
    let (block, _) = split(content);
    match crate::front::parse(&format!("\n{}", block?)) {
        Some(Value::Object(m)) => Some(m),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub code: String,
    pub line: Option<usize>,
    pub message: String,
}

pub type Validation = Vec<Issue>;

fn issue(code: &str, message: &str, line: Option<usize>) -> Issue {
    Issue { code: code.into(), line, message: message.into() }
}

fn non_empty(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| !crate::jsre::trim(s).is_empty())
}

fn is_record(v: &Value) -> bool {
    v.is_object()
}

fn iso_with_offset(value: &str) -> bool {
    static ISO: LazyLock<Re> = LazyLock::new(|| Re::new(r"^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:Z|[+-](\d{2}):(\d{2}))$", "u"));
    let Some(m) = ISO.exec(value) else { return false };
    let n = |i: usize| m.get(i).map_or(0, |s| s.parse::<u32>().unwrap_or(0));
    let (y, mo, d, h, mi, s, oh, om) = (n(1), n(2), n(3), n(4), n(5), n(6), n(7), n(8));
    let days = match mo {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=12).contains(&mo) && d >= 1 && d <= days && h <= 23 && mi <= 59 && s <= 59 && oh <= 23 && om <= 59
}

fn actor_event(v: &Value) -> bool {
    is_record(v) && non_empty(v.get("by")) && (v.get("at").is_none() || (non_empty(v.get("at")) && iso_with_offset(v["at"].as_str().unwrap_or_default())))
}

/// `validateOkfFrontmatter`: empty when it's valid.
pub fn validate(content: &str) -> Validation {
    static LINES: LazyLock<Re> = LazyLock::new(|| Re::new(r"\r?\n", "u"));
    let mut lines: Vec<String> = Vec::new();
    let mut last = 0;
    for m in LINES.all(content) {
        lines.push(content[last..m.start()].to_owned());
        last = m.end();
    }
    lines.push(content[last..].to_owned());
    if lines[0] != "---" {
        return vec![issue("missing_opening_delimiter", "File must begin with `---`.", Some(1))];
    }
    let Some(closing) = lines.iter().skip(1).position(|l| l == "---").map(|i| i + 1) else {
        return vec![issue("missing_closing_delimiter", "Opening front matter has no closing `---` delimiter.", None)];
    };
    let Some(parsed) = crate::front::parse(&format!("\n{}", lines[1..closing].join("\n"))) else {
        return vec![issue("invalid_yaml", "Front matter is not valid YAML.", None)];
    };
    let Value::Object(f) = parsed else {
        return vec![issue("invalid_yaml_root", "Front matter must be a YAML mapping.", None)];
    };
    let mut issues = Vec::new();
    if !f.contains_key("type") {
        issues.push(issue("missing_type", "Required field `type` is missing.", None));
    }
    for field in STRING_FIELDS {
        if f.contains_key(field) && !non_empty(f.get(field)) {
            issues.push(issue(&format!("invalid_{field}"), &format!("Field `{field}` must be a non-empty string."), None));
        }
    }
    if let Some(tags) = f.get("tags") {
        if !tags.as_array().is_some_and(|t| t.iter().all(|x| non_empty(Some(x)))) {
            issues.push(issue("invalid_tags", "Field `tags` must be a YAML list of non-empty strings.", None));
        }
    }
    if f.get("generated").is_some_and(|g| !actor_event(g)) {
        issues.push(issue("invalid_generated", "Field `generated` must be a mapping with a non-empty string `by` (actor) and an optional `at` ISO 8601 datetime with an explicit UTC offset.", None));
    }
    if let Some(v) = f.get("verified") {
        let events: Vec<&Value> = match v {
            Value::Array(a) => a.iter().collect(),
            other => vec![other],
        };
        if !events.iter().all(|e| actor_event(e)) {
            issues.push(issue("invalid_verified", "Field `verified` must be a `{by, at}` mapping or a YAML list of them, each with a non-empty string `by` (actor) and any `at` value as an ISO 8601 datetime with an explicit UTC offset.", None));
        }
    }
    if let Some(s) = f.get("sources") {
        if !s.as_array().is_some_and(|a| a.iter().all(|e| is_record(e) && non_empty(e.get("resource")))) {
            issues.push(issue("invalid_sources", "Field `sources` must be a YAML list of mappings, each with a non-empty string `resource`.", None));
        }
    }
    if f.get("status").is_some_and(|s| !s.as_str().is_some_and(|s| STATUS_VALUES.contains(&s))) {
        issues.push(issue("invalid_status", "Field `status` must be one of `draft`, `stable`, or `deprecated`.", None));
    }
    if f.get("stale_after").is_some_and(|s| !s.as_str().is_some_and(iso_with_offset)) {
        issues.push(issue("invalid_stale_after", "Field `stale_after` must be an ISO 8601 datetime with an explicit UTC offset.", None));
    }
    issues
}

fn is_field_line(line: &str, key: &str) -> bool {
    let Some(rest) = line.strip_prefix(key) else { return false };
    rest.trim_start_matches(is_js_space).starts_with(':')
}

fn field_end(lines: &[&str], start: usize) -> usize {
    let mut end = start + 1;
    while end < lines.len() {
        let l = lines[end];
        let compact_item = l.starts_with('-') && l[1..].chars().next().is_none_or(is_js_space);
        if l.is_empty() || l.chars().next().is_some_and(is_js_space) || compact_item {
            end += 1;
        } else {
            break;
        }
    }
    end
}

fn replace_field(content: &str, key: &str, replacement: &[String]) -> String {
    let (block, body) = split(content);
    let Some(block) = block else {
        if replacement.is_empty() {
            return content.to_owned();
        }
        return format!("---\n{}\n---\n\n{content}", replacement.join("\n"));
    };
    let lines: Vec<&str> = block.split('\n').collect();
    let Some(start) = lines.iter().position(|l| is_field_line(l, key)) else {
        if replacement.is_empty() {
            return content.to_owned();
        }
        let all: Vec<String> = lines.iter().map(|l| (*l).to_owned()).chain(replacement.iter().cloned()).collect();
        return format!("---\n{}\n---\n{body}", all.join("\n"));
    };
    let end = field_end(&lines, start);
    let next: Vec<String> = lines[..start].iter().map(|l| (*l).to_owned()).chain(replacement.iter().cloned()).chain(lines[end..].iter().map(|l| (*l).to_owned())).collect();
    if next.is_empty() {
        return body.strip_prefix("\r\n").or_else(|| body.strip_prefix('\n')).unwrap_or(&body).to_owned();
    }
    format!("---\n{}\n---\n{body}", next.join("\n"))
}

pub fn set_field(content: &str, key: &str, value: &str) -> String {
    replace_field(content, key, &[format!("{key}: {}", json_string(value))])
}

pub fn set_generated(content: &str, by: &str, at: Option<&str>) -> String {
    let members = match at {
        None => format!("by: {}", json_string(by)),
        Some(at) => format!("by: {}, at: {}", json_string(by), json_string(at)),
    };
    replace_field(content, "generated", &[format!("generated: {{ {members} }}")])
}

fn set_list(content: &str, key: &str, items: &[Value]) -> String {
    if items.is_empty() {
        return replace_field(content, key, &[]);
    }
    let text = yaml(&Value::Array(items.to_vec()));
    let mut lines = vec![format!("{key}:")];
    lines.extend(crate::jsre::trim_end(&text).split('\n').map(|l| format!("  {l}")));
    replace_field(content, key, &lines)
}

pub fn set_sources(content: &str, sources: &[Value]) -> String {
    set_list(content, "sources", sources)
}

pub fn set_verified(content: &str, events: &[Value]) -> String {
    set_list(content, "verified", events)
}

pub fn remove_field(content: &str, key: &str) -> String {
    replace_field(content, key, &[])
}

fn set_value(content: &str, key: &str, value: Value) -> String {
    let mut m = Map::new();
    m.insert(key.to_owned(), value);
    let text = yaml(&Value::Object(m));
    let lines: Vec<String> = crate::jsre::trim_end(&text).split('\n').map(str::to_owned).collect();
    replace_field(content, key, &lines)
}

fn first_heading(body: &str) -> Option<String> {
    static H: LazyLock<Re> = LazyLock::new(|| Re::new(r"^#\s+(.+?)\s*$", "mu"));
    H.exec(body).map(|m| crate::jsre::trim(m.str(1)).to_owned())
}

fn title_from_filename(path: &str) -> String {
    static EXT: LazyLock<Re> = LazyLock::new(|| Re::new(r"\.md$", "iu"));
    static SEP: LazyLock<Re> = LazyLock::new(|| Re::new(r"[-_]+", "u"));
    let base = path.rsplit('/').next().unwrap_or(path);
    let base = EXT.replace(base, false, |_| String::new());
    let spaced = SEP.replace(&base, true, |_| " ".into());
    let spaced = crate::jsre::trim(&spaced);
    if spaced.is_empty() {
        return base;
    }
    // (charAt(0).toUpperCase(): one UTF-16 unit.)
    let first = spaced.chars().next().unwrap();
    let upper: String = if (first as u32) > 0xffff { first.to_string() } else { first.to_uppercase().collect() };
    format!("{upper}{}", &spaced[first.len_utf8()..])
}

fn render(kind: &str, title: &str, generated: bool) -> String {
    let mut lines = vec![format!("type: {}", json_string(kind)), format!("title: {}", json_string(title))];
    if generated {
        lines.push(format!("{GENERATED_FIELD}: true"));
    }
    format!("---\n{}\n---\n\n", lines.join("\n"))
}

/// `repairOkfFrontmatter`: (changed, content).
pub fn repair(content: &str, path: &str, concept_type: &str) -> (bool, String) {
    if validate(content).is_empty() {
        return (false, content.to_owned());
    }
    let f = fields(content);
    let (_, body) = split(content);
    let fallback = if crate::jsre::trim(concept_type).is_empty() { "Reference" } else { concept_type };
    let title = first_heading(&body).unwrap_or_else(|| title_from_filename(path));
    let rebuild = || {
        let rebuilt = format!("{}{body}", render(fallback, &title, true));
        (rebuilt != content, rebuilt)
    };
    let Some(f) = f else { return rebuild() };
    let mut r = content.to_owned();
    let type_derived = !non_empty(f.get("type"));
    if type_derived {
        r = set_field(&r, "type", fallback);
        r = set_value(&r, GENERATED_FIELD, Value::Bool(true));
    }
    if (type_derived && !f.contains_key("title")) || (f.contains_key("title") && !non_empty(f.get("title"))) {
        r = set_field(&r, "title", &title);
    }
    for field in OPTIONAL_STRING_FIELDS {
        if f.contains_key(field) && !non_empty(f.get(field)) {
            r = remove_field(&r, field);
        }
    }
    if let Some(tags) = f.get("tags") {
        let kept: Vec<Value> = tags.as_array().map(|a| a.iter().filter(|t| non_empty(Some(t))).cloned().collect()).unwrap_or_default();
        r = if kept.is_empty() { remove_field(&r, "tags") } else { set_value(&r, "tags", Value::Array(kept)) };
    }
    if f.get("generated").is_some_and(|g| !actor_event(g)) {
        r = remove_field(&r, "generated");
    }
    if let Some(v) = f.get("verified") {
        let candidates: Vec<Value> = match v {
            Value::Array(a) => a.clone(),
            other => vec![other.clone()],
        };
        let events: Vec<Value> = candidates.into_iter().filter(|e| is_record(e) && actor_event(e)).collect();
        r = set_verified(&r, &events);
    }
    if let Some(s) = f.get("sources") {
        let sources: Vec<Value> = s.as_array().map(|a| a.iter().filter(|e| is_record(e) && non_empty(e.get("resource"))).cloned().collect()).unwrap_or_default();
        r = set_sources(&r, &sources);
    }
    if f.get("status").is_some_and(|s| !s.as_str().is_some_and(|s| STATUS_VALUES.contains(&s))) {
        r = remove_field(&r, "status");
    }
    if f.get("stale_after").is_some_and(|s| !s.as_str().is_some_and(iso_with_offset)) {
        r = remove_field(&r, "stale_after");
    }
    if validate(&r).is_empty() {
        return (r != content, r);
    }
    rebuild()
}

/// The issues as a lifecycle error words them.
pub fn describe(issues: &Validation) -> String {
    issues.iter().map(|i| format!("[{}]{} {}", i.code, i.line.map(|l| format!(" line {l}:")).unwrap_or_default(), i.message)).collect::<Vec<_>>().join("; ")
}

// ---- Verification stamps ----

fn verification_events(content: &str) -> Vec<Value> {
    let v = fields(content).and_then(|f| f.get("verified").cloned());
    let events = match v {
        Some(Value::Array(a)) => a,
        None => Vec::new(),
        Some(other) => vec![other],
    };
    events
        .into_iter()
        .filter(|e| is_record(e) && non_empty(e.get("by")) && (e.get("at").is_none() || non_empty(e.get("at"))))
        .collect()
}

fn canonical_list(content: &str) -> bool {
    fields(content).is_none_or(|f| f.get("verified").is_none_or(Value::is_array))
}

/// `synchronizeClaimsVerification`: each page's OpenWiki verification event
/// made what its Claims say (others' events kept); the pages it changed,
/// with what they were.
pub fn synchronize_verification(store: &Store, by_page: &[(String, Option<Verification>)], excluded: &HashSet<String>) -> Result<Vec<(String, String)>> {
    let mut changes = Vec::new();
    for page in store.discover_pages()? {
        if excluded.contains(&page) {
            continue;
        }
        let content = store.read_markdown(&page)?;
        let repaired = repair(&content, &page, "Reference").1;
        let current = verification_events(&repaired);
        let mut next: Vec<Value> = current.iter().filter(|e| !e["by"].as_str().unwrap_or_default().starts_with("openwiki/")).cloned().collect();
        if let Some((_, Some(v))) = by_page.iter().find(|(p, _)| *p == page) {
            next.push(json!({"by": v.by, "at": v.at}));
        }
        let projected = if current == next && canonical_list(&repaired) { repaired } else { repair(&set_verified(&repaired, &next), &page, "Reference").1 };
        if projected == content {
            continue;
        }
        store.write_markdown(&page, &projected)?;
        changes.push((page, content));
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_lists() {
        let v = json!([{"by": "openwiki/0.9.0", "at": "2026-01-02T03:04:05.000Z"}, {"by": "a: b", "at": "true"}]);
        assert_eq!(yaml(&v), "- by: openwiki/0.9.0\n  at: 2026-01-02T03:04:05.000Z\n- by: \"a: b\"\n  at: \"true\"\n");
        assert_eq!(yaml(&json!({"tags": ["x", "#y", "it's"]})), "tags:\n  - x\n  - \"#y\"\n  - it's\n");
        assert_eq!(yaml(&json!([{"resource": "repo://a.rs#L1-L2", "n": [], "m": {}}])), "- resource: repo://a.rs#L1-L2\n  n: []\n  m: {}\n");
        assert_eq!(yaml(&json!(["\"hi\" there"])), "- '\"hi\" there'\n");
        assert_eq!(yaml(&json!(["a\nb"])), "- |-\n  a\n  b\n");
    }

    #[test]
    fn repairs() {
        assert!(validate("---\ntype: Reference\ntitle: A\n---\n# A\n").is_empty());
        let (changed, fixed) = repair("# Queue design\n\nText.\n", "/openwiki/architecture/queue.md", "Reference");
        assert!(changed);
        assert_eq!(fixed, "---\ntype: \"Reference\"\ntitle: \"Queue design\"\nopenwiki_generated: true\n---\n\n# Queue design\n\nText.\n");
        let (_, fixed) = repair("---\ntitle: X\nstatus: nope\n---\nbody\n", "/openwiki/x.md", "Reference");
        assert_eq!(fixed, "---\ntitle: X\ntype: \"Reference\"\nopenwiki_generated: true\n---\nbody\n");
        assert_eq!(title_from_filename("/openwiki/how-it_works.md"), "How it works");
    }
}
