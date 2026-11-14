//! Dev check: each file named on stdin lexed, one JSON line per file
//! (top-level blocks: type, raw, depth, text, the heading's inline text).
use harlo_wiki::marked::{self, Kind};
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let path = line.unwrap();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let blocks: Vec<serde_json::Value> = marked::lex(&text)
            .iter()
            .map(|b| {
                let kind = match b.kind {
                    Kind::Space => "space",
                    Kind::Code => "code",
                    Kind::Heading => "heading",
                    Kind::Hr => "hr",
                    Kind::Blockquote => "blockquote",
                    Kind::List => "list",
                    Kind::Html => "html",
                    Kind::Def => "def",
                    Kind::Table => "table",
                    Kind::Paragraph => "paragraph",
                    Kind::Text => "text",
                };
                if b.kind == Kind::Heading {
                    serde_json::json!([kind, b.raw, b.depth, b.text, marked::inline_text(&b.inline)])
                } else {
                    serde_json::json!([kind, b.raw])
                }
            })
            .collect();
        println!("{}", serde_json::json!({"path": path, "blocks": blocks}));
    }
}
