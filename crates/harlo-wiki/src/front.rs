//! Page frontmatter: the leading `---` YAML block, parsed as OpenWiki parses
//! it (YAML 1.2 core schema, duplicate keys an error) into JSON values.

use crate::jsre::Re;
use serde_json::{Map, Number, Value};
use std::sync::LazyLock;
use yaml_rust::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust::scanner::{Marker, TScalarStyle, TokenType};

static BLOCK: LazyLock<Re> = LazyLock::new(|| Re::new(r"^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)", "u"));
static BODY: LazyLock<Re> = LazyLock::new(|| Re::new(r"^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)", "u"));

/// The frontmatter block's text and the rest of the page.
pub fn split(content: &str) -> (Option<&str>, &str) {
    match BLOCK.exec(content) {
        Some(m) => (m.get(1), &content[m.end()..]),
        None => (None, content),
    }
}

/// The page without its frontmatter, trimmed (OpenWiki's `markdownBody`).
pub fn body(markdown: &str) -> &str {
    let rest = match BODY.exec(markdown) {
        Some(m) => &markdown[m.end()..],
        None => markdown,
    };
    crate::jsre::trim(rest)
}

/// The frontmatter's fields, None when there's none, it doesn't parse, or
/// it isn't a mapping.
pub fn fields(content: &str) -> Option<Map<String, Value>> {
    let block = split(content).0?;
    match parse(&format!("\n{block}")) {
        Some(Value::Object(m)) => Some(m),
        _ => None,
    }
}

/// A YAML document as JSON (None: not valid YAML for OpenWiki's parser).
pub fn parse(text: &str) -> Option<Value> {
    // (YAML forbids tabs as indentation.)
    if text.split('\n').any(|l| l.starts_with('\t') && !l.trim().is_empty()) {
        return None;
    }
    // A final line break, so a block scalar keeps its last one, as
    // OpenWiki's parser does at the end of the block.
    let text = if text.ends_with('\n') { text.to_owned() } else { format!("{text}\n") };
    let mut b = Builder { stack: Vec::new(), anchors: Vec::new(), docs: 0, out: None, failed: false };
    let mut p = Parser::new(text.chars());
    p.load(&mut b, true).ok()?;
    if b.failed || b.docs > 1 {
        return None;
    }
    Some(b.out.unwrap_or(Value::Null))
}

enum Frame {
    Seq(Vec<Value>, usize),
    Map(Map<String, Value>, Option<String>, usize),
}

struct Builder {
    stack: Vec<Frame>,
    anchors: Vec<(usize, Value)>,
    docs: usize,
    out: Option<Value>,
    failed: bool,
}

impl Builder {
    fn put(&mut self, v: Value, anchor: usize) {
        if anchor > 0 {
            self.anchors.push((anchor, v.clone()));
        }
        match self.stack.last_mut() {
            None => self.out = Some(v),
            Some(Frame::Seq(items, _)) => items.push(v),
            Some(Frame::Map(map, key, _)) => match key.take() {
                None => *key = Some(key_string(&v)),
                Some(k) => {
                    if map.contains_key(&k) {
                        self.failed = true;
                    }
                    map.insert(k, v);
                }
            },
        }
    }
}

fn key_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => js_number(n.as_f64().unwrap_or(0.0)),
        other => other.to_string(),
    }
}

impl MarkedEventReceiver for Builder {
    fn on_event(&mut self, ev: Event, _mark: Marker) {
        match ev {
            Event::DocumentStart => self.docs += 1,
            Event::SequenceStart(a) => self.stack.push(Frame::Seq(Vec::new(), a)),
            Event::MappingStart(a) => self.stack.push(Frame::Map(Map::new(), None, a)),
            Event::SequenceEnd => {
                if let Some(Frame::Seq(items, a)) = self.stack.pop() {
                    self.put(Value::Array(items), a);
                }
            }
            Event::MappingEnd => {
                if let Some(Frame::Map(map, _, a)) = self.stack.pop() {
                    self.put(Value::Object(map), a);
                }
            }
            Event::Scalar(text, style, anchor, tag) => {
                let v = scalar(&text, style, tag.as_ref());
                self.put(v, anchor);
            }
            Event::Alias(id) => {
                let v = self.anchors.iter().rev().find(|(a, _)| *a == id).map(|(_, v)| v.clone());
                match v {
                    Some(v) => self.put(v, 0),
                    None => self.failed = true,
                }
            }
            _ => {}
        }
    }
}

fn scalar(text: &str, style: TScalarStyle, tag: Option<&TokenType>) -> Value {
    if let Some(TokenType::Tag(handle, suffix)) = tag {
        if handle == "!!" && suffix == "str" {
            return Value::String(text.to_owned());
        }
    }
    if style != TScalarStyle::Plain {
        return Value::String(text.to_owned());
    }
    core(text)
}

/// The YAML 1.2 core schema's reading of a plain scalar.
fn core(t: &str) -> Value {
    static INT: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"^[-+]?[0-9]+$").unwrap());
    static OCT: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"^0o[0-7]+$").unwrap());
    static HEX: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"^0x[0-9a-fA-F]+$").unwrap());
    static FLOAT: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?$").unwrap());
    match t {
        "" | "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" | "-.inf" | "-.Inf" | "-.INF" | ".nan" | ".NaN" | ".NAN" => {
            // (Not a string; JSON has no infinity.)
            return Value::Number(Number::from(0));
        }
        _ => {}
    }
    if INT.is_match(t) {
        if let Ok(i) = t.parse::<i64>() {
            return Value::Number(Number::from(i));
        }
        return t.parse::<f64>().ok().and_then(Number::from_f64).map(Value::Number).unwrap_or(Value::Null);
    }
    if OCT.is_match(t) {
        return Value::Number(Number::from(u64::from_str_radix(&t[2..], 8).unwrap_or(0)));
    }
    if HEX.is_match(t) {
        return Value::Number(Number::from(u64::from_str_radix(&t[2..], 16).unwrap_or(0)));
    }
    if FLOAT.is_match(t) {
        return t.parse::<f64>().ok().and_then(Number::from_f64).map(Value::Number).unwrap_or(Value::Null);
    }
    Value::String(t.to_owned())
}

/// A number the way JavaScript writes it.
pub fn js_number(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if f == 0.0 {
        return "0".into();
    }
    // Shortest digits and the exponent, then JavaScript's layout: plain
    // from 1e-6 up to 1e21, exponential outside.
    let e = format!("{:e}", f.abs());
    let (mantissa, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exp: i32 = exp.parse().unwrap_or(0);
    let k = digits.len() as i32;
    let n = exp + 1;
    let sign = if f < 0.0 { "-" } else { "" };
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let es = if e >= 0 { format!("+{e}") } else { e.to_string() };
        if k == 1 { format!("{digits}e{es}") } else { format!("{}.{}e{es}", &digits[..1], &digits[1..]) }
    };
    format!("{sign}{body}")
}

/// A field's trimmed non-empty string (OpenWiki's `stringField`).
pub fn string_field(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) if !crate::jsre::trim(s).is_empty() => Some(crate::jsre::trim(s).to_owned()),
        _ => None,
    }
}

/// A field's string members (OpenWiki's `stringArray`).
pub fn string_array(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_as_javascript() {
        for (f, js) in [(1.0, "1"), (-0.5, "-0.5"), (1e21, "1e+21"), (1.5e-7, "1.5e-7"), (123456.789, "123456.789"), (1e-6, "0.000001"), (2.5e20, "250000000000000000000"), (0.1 + 0.2, "0.30000000000000004")] {
            assert_eq!(js_number(f), js);
        }
    }

    #[test]
    fn core_schema_and_shapes() {
        let f = fields("---\ntitle: \"Quoted: t\"\nn: 42\nb: True\nz: ~\ntags: [a, 2, true]\nsources:\n  - resource: repo://x\n    id: s\n---\n# x").unwrap();
        assert_eq!(Value::Object(f), json!({"title": "Quoted: t", "n": 42, "b": true, "z": null, "tags": ["a", 2, true], "sources": [{"resource": "repo://x", "id": "s"}]}));
        assert!(fields("---\na: 1\na: 2\n---\n").is_none());
        assert!(fields("---\ntitle: [unclosed\n---\n").is_none());
        assert!(fields("---\n- a\n---\n").is_none());
        assert_eq!(body("---\na: 1\n---\n\n# T\n\n"), "# T");
        assert_eq!(body("# T"), "# T");
    }
}
