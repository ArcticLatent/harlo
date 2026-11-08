//! Loop detection: a call made again with the same arguments that gets the
//! same result is the model going round in circles. Keyed by (tool,
//! arguments, result), so polling (the same call, a new result each time)
//! isn't counted. The third time the model is told (hidden); the fifth, the
//! run ends. A change made (an edit or a write) starts the count again: the
//! same failing test after another fix is trying, not circling.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde_json::Value;

/// The time a repeat is pointed out to the model.
pub const NUDGE_AT: usize = 3;
/// The time it ends the run.
pub const STOP_AT: usize = 5;

/// Told (hidden) when a call repeats with the same result.
pub fn nudge(tool: &str, times: usize) -> String {
    format!(
        "[harlo] You've made the same {tool} call {times} times now and got the same result each time: doing it again won't change it. \
         Try something different; if you're stuck, end your turn and say what's in the way."
    )
}

/// What a repeated call means for the run.
#[derive(Debug, PartialEq)]
pub enum Repeat {
    No,
    /// Point it out (it's the n-th time).
    Nudge(usize),
    /// End the run (the n-th time).
    Stop(usize),
}

/// The calls of one run, counted by (tool, arguments, result).
#[derive(Default)]
pub struct Repeats {
    seen: HashMap<u64, usize>,
}

impl Repeats {
    /// Files were changed: what repeats from here on counts anew.
    pub fn changed(&mut self) {
        self.seen.clear();
    }

    pub fn see(&mut self, tool: &str, args: &Value, result: &str, failed: bool) -> Repeat {
        let mut h = DefaultHasher::new();
        tool.hash(&mut h);
        canonical(args).hash(&mut h);
        result.hash(&mut h);
        failed.hash(&mut h);
        let n = self.seen.entry(h.finish()).or_insert(0);
        *n += 1;
        match *n {
            n if n >= STOP_AT => Repeat::Stop(n),
            n if n == NUDGE_AT => Repeat::Nudge(n),
            _ => Repeat::No,
        }
    }
}

/// Arguments as text with their keys in order (a model may write them in
/// another order the next time).
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys.iter().map(|k| format!("{}:{}", Value::String((*k).clone()), canonical(&m[*k]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_same_call_and_result_is_counted() {
        let mut r = Repeats::default();
        let args = json!({ "command": "cargo test", "timeout": 60 });
        let reordered = json!({ "timeout": 60, "command": "cargo test" });
        assert_eq!(r.see("bash", &args, "1 failed", true), Repeat::No);
        assert_eq!(r.see("bash", &reordered, "1 failed", true), Repeat::No, "keys in another order: the same call");
        assert_eq!(r.see("bash", &args, "1 failed", true), Repeat::Nudge(3));
        assert_eq!(r.see("bash", &args, "1 failed", true), Repeat::No);
        assert_eq!(r.see("bash", &args, "1 failed", true), Repeat::Stop(5));
    }

    #[test]
    fn a_change_starts_the_count_again() {
        let mut r = Repeats::default();
        let test = json!({ "command": "cargo test" });
        for _ in 0..4 {
            assert_eq!(r.see("bash", &test, "1 failed", true), Repeat::No);
            assert_eq!(r.see("bash", &test, "1 failed", true), Repeat::No);
            r.changed();
        }
    }

    #[test]
    fn polling_and_other_calls_are_not() {
        let mut r = Repeats::default();
        let wait = json!({ "action": "output", "id": "j1" });
        for i in 0..10 {
            assert_eq!(r.see("jobs", &wait, &format!("line {i}"), false), Repeat::No, "a new result each time");
        }
        // The same arguments, another tool, or another outcome: not the same.
        let args = json!({ "path": "a.txt" });
        for (tool, result, failed) in [("read", "x", false), ("ls", "x", false), ("read", "y", false), ("read", "x", true)] {
            assert_eq!(r.see(tool, &args, result, failed), Repeat::No);
        }
        assert_eq!(r.see("read", &args, "x", false), Repeat::No);
        assert_eq!(r.see("read", &args, "x", false), Repeat::Nudge(3));
    }
}
