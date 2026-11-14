//! Dev check: front matter validation, repair and field edits over a corpus
//! (scripts/wiki-bench/okf-corpus.mjs's lines on stdin), printing each
//! difference.
use harlo_wiki::okf;
use serde_json::json;
use std::io::BufRead;

fn main() {
    let (mut n, mut bad) = (0, 0);
    for line in std::io::stdin().lock().lines() {
        let v: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let content = v["content"].as_str().unwrap();
        let issues = okf::validate(content);
        let (changed, repaired) = okf::repair(content, "/openwiki/how-it_works.md", "Reference");
        let events = [json!({"by": "openwiki/0.9.0", "at": "2026-01-02T03:04:05.000Z"})];
        let ours = json!({
            "content": content,
            "valid": issues.is_empty(),
            "codes": issues.iter().map(|i| i.code.clone()).collect::<Vec<_>>(),
            "changed": changed,
            "repaired": repaired,
            "verified": okf::set_verified(&repaired, &events),
            "sources": okf::set_sources(&repaired, &[json!({"resource": "repo://x.rs#L1-L2"})]),
            "generated": okf::set_generated(&repaired, "openwiki/0.9.0", Some("2026-01-02T03:04:05.000Z")),
            "removed": okf::remove_field(&repaired, "title"),
        });
        n += 1;
        for k in ["valid", "codes", "changed", "repaired", "verified", "sources", "generated", "removed"] {
            if ours[k] != v[k] {
                bad += 1;
                println!("{:?} {k}\n  ow  {}\n  ours {}", content, v[k], ours[k]);
            }
        }
    }
    println!("{n} cases, {bad} differ");
}
