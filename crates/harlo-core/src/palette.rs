//! Fuzzy ranking for the command palette.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Indices of `haystacks` that match `query`, best first. Equal scores keep
/// their input order (callers pass items newest first). An empty query
/// matches everything in input order.
pub fn rank(query: &str, haystacks: &[String]) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        return (0..haystacks.len()).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(usize, u32)> = haystacks
        .iter()
        .enumerate()
        .filter_map(|(i, h)| pattern.score(Utf32Str::new(h, &mut buf), &mut matcher).map(|s| (i, s)))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_fuzzy_matches() {
        let items: Vec<String> = ["Fix the build", "procedural rock generator", "omp update", "rock paper"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let r = rank("rock gen", &items);
        assert_eq!(r.first(), Some(&1));
        assert!(!r.contains(&2));
        assert_eq!(rank("", &items), [0, 1, 2, 3]);
        assert!(rank("zzzz", &items).is_empty());
    }
}
