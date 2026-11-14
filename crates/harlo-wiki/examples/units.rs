//! Dev check: the native answers for repositories named as arguments, one
//! JSON line each, shaped as scripts/wiki-bench/units.mjs prints OpenWiki's.
use harlo_wiki::ignore::Ignore;
use harlo_wiki::repo::{self, Noop};
use serde_json::{Value, json};
use std::path::Path;

fn noop(r: Result<Noop, String>) -> Value {
    match r {
        Ok(Noop::Skip { git_head, model, language }) => {
            let mut v = json!({"shouldSkip": true, "gitHead": git_head, "model": model});
            if let Some(l) = language {
                v["language"] = l.into();
            }
            v
        }
        Ok(Noop::Run(reason)) => json!({"shouldSkip": false, "reason": reason}),
        Err(e) => format!("ERR {e}").into(),
    }
}

fn main() {
    for root in std::env::args().skip(1) {
        let p = Path::new(&root);
        let ig = Ignore::load(p).unwrap();
        let last = repo::read_last_update(p).unwrap();
        let base = last.as_ref().and_then(|l| l.git_head.clone());
        let source = match repo::source_snapshot(p, &ig) {
            Ok(s) => {
                let mut v = json!({"fingerprint": s.fingerprint});
                if let Some(h) = s.git_head {
                    v["gitHead"] = h.into();
                }
                v
            }
            Err(e) => format!("ERR {e}").into(),
        };
        let out = json!({
            "root": root,
            "source": source,
            "content": repo::content_snapshot(p).map(Value::from).unwrap_or_else(|e| format!("ERR {e}").into()),
            "changed": repo::changed_paths(p, &ig, None),
            "changedSince": repo::changed_paths(p, &ig, base.as_deref()),
            "commits": repo::commit_context(p, &ig, base.as_deref()),
            "noop": noop(repo::update_noop(p, &ig, None)),
            "noopKo": noop(repo::update_noop(p, &ig, Some("ko"))),
            "last": last.map(|l| l.to_json()).unwrap_or(Value::Null),
        });
        println!("{out}");
    }
}
