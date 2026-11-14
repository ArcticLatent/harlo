//! Dev check: each JSON line [tag, …] on stdin answered as [tag, resolved
//! ("absent", the canonical tag, or "?"), primary subtag].
use harlo_wiki::lang::{self, Resolved};
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let v: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let t = v[0].as_str().unwrap();
        let r = match lang::resolve(Some(t)) {
            Resolved::Absent => "absent".to_owned(),
            Resolved::Language(l) => l,
            Resolved::Unrecognized(_) => "?".to_owned(),
        };
        println!("{}", serde_json::json!([t, r, lang::primary(Some(t))]));
    }
}
