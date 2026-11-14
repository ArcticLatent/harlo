//! Dev check: each file named on stdin's frontmatter fields as JSON (null:
//! none or unparseable), one line per file.
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let path = line.unwrap();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let f = harlo_wiki::front::fields(&text).map(serde_json::Value::Object).unwrap_or(serde_json::Value::Null);
        println!("{}", serde_json::json!({"path": path, "fields": f, "body": harlo_wiki::front::body(&text).len()}));
    }
}
