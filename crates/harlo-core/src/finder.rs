//! Fuzzy search for the finder: which of a list of names a few typed
//! letters mean, best first.
//!
//! A name matches when it holds every letter typed, in order, with
//! anything between them, so `stapp` finds "Settings: Appearance". Names
//! where the letters sit together, at the start of words, rank first.
//! The matching is `frizbee`'s, the matcher the server ranks file names
//! with.

// Rust guideline compliant 2026-10-09

use frizbee::{CaseMatching, Config, Matcher};

/// A name the query matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Its place in the list searched.
    pub index: usize,
    /// The characters of the name the query's letters fell on, by their
    /// place in it, ascending; for showing why it matched.
    pub marked: Vec<usize>,
}

/// Ranks `names` by how well `query` matches each, best first; names it
/// doesn't match are left out.
///
/// Among names that match equally well, the one listed first stays
/// first, so a list in a meaningful order, such as newest first, keeps
/// it. An empty query matches every name, in the order listed.
#[must_use]
pub fn rank(query: &str, names: &[&str]) -> Vec<Found> {
    let query = query.trim();
    if query.is_empty() {
        return (0..names.len())
            .map(|index| Found {
                index,
                marked: Vec::new(),
            })
            .collect();
    }
    // Capitals typed mean nothing more than small letters do.
    let config = Config::default().casing(CaseMatching::Ignore);
    let mut matcher = Matcher::new(query, &config);
    let mut hits = matcher.match_list_indices(names);
    // (A stable sort: equal scores keep the list's order.)
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
    hits.into_iter()
        .filter_map(|found| {
            let index = usize::try_from(found.index).ok()?;
            let name = names.get(index)?;
            Some(Found {
                index,
                marked: marked(name, &found.indices),
            })
        })
        .collect()
}

/// The places of the characters of `name` at the byte offsets `bytes`,
/// ascending.
fn marked(name: &str, bytes: &[u32]) -> Vec<usize> {
    let mut places: Vec<usize> = name
        .char_indices()
        .enumerate()
        .filter(|(_, (byte, _))| u32::try_from(*byte).is_ok_and(|byte| bytes.contains(&byte)))
        .map(|(place, _)| place)
        .collect();
    places.dedup();
    places
}

#[cfg(test)]
mod tests {
    use super::rank;

    /// The names `query` finds in `names`, best first.
    fn found<'a>(query: &str, names: &[&'a str]) -> Vec<&'a str> {
        rank(query, names).iter().map(|f| names[f.index]).collect()
    }

    #[test]
    fn letters_in_order_find_a_name_and_the_closest_fit_comes_first() {
        let names = [
            "Settings: Appearance",
            "Stats",
            "Start a new session",
            "Fix the parser's tests",
        ];
        assert_eq!(found("stat", &names)[0], "Stats");
        assert_eq!(found("appear", &names), ["Settings: Appearance"]);
        assert_eq!(found("stapp", &names), ["Settings: Appearance"]);
        assert_eq!(found("PARSER", &names), ["Fix the parser's tests"]);
        assert!(found("zzz", &names).is_empty());
        // Letters out of order aren't a match.
        assert!(found("tsta", &["Stats"]).is_empty());
    }

    #[test]
    fn nothing_typed_keeps_every_name_in_its_order() {
        let names = ["b", "a", "c"];
        assert_eq!(found("", &names), names);
        assert_eq!(found("   ", &names), names);
    }

    #[test]
    fn names_that_match_alike_keep_their_order() {
        let names = ["how are you", "how are you", "how are you today"];
        let ranked = rank("how are", &names);
        let places: Vec<usize> = ranked.iter().map(|f| f.index).collect();
        assert_eq!(places[..2], [0, 1], "{ranked:?}");
    }

    #[test]
    fn the_letters_matched_are_marked_by_character_not_by_byte() {
        let ranked = rank("res", &["Café résumé res"]);
        assert_eq!(ranked.len(), 1);
        // "Café résumé " is 12 characters (14 bytes): `res` follows it.
        assert_eq!(ranked[0].marked, [12, 13, 14], "{ranked:?}");

        let ranked = rank("stapp", &["Settings: Appearance"]);
        let letters: String = ranked[0]
            .marked
            .iter()
            .filter_map(|place| "Settings: Appearance".chars().nth(*place))
            .collect();
        assert_eq!(letters.to_lowercase(), "stapp", "{ranked:?}");
    }
}
