//! Dev check: each file named on stdin's frontmatter fields as JSON (null:
//! none or unparseable), one line per file.

// Rust guideline compliant 2026-10-09

use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let path = line.unwrap();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let f = harlo_wiki::front::fields(&text).map_or(serde_json::Value::Null, serde_json::Value::Object);
        println!("{}", serde_json::json!({"path": path, "fields": f, "body": harlo_wiki::front::body(&text).len()}));
    }
}
