//! `a.localeCompare(b)` as OpenWiki sorts pages and workspace names with it
//! (Unicode's root collation): punctuation and symbols before digits before
//! letters, letters by their base letter first, then accents, then case
//! (lowercase first). Exact for ASCII; other characters follow their base
//! letter where they have one, else their code point.

use std::cmp::Ordering;

const ASCII_ORDER: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789aAbBcCdDeEfFgGhHiIjJkKlLmMnNoOpPqQrRsStTuUvVwWxXyYzZ";

fn ascii_primary(c: char) -> Option<u32> {
    let lower = c.to_ascii_lowercase();
    ASCII_ORDER.find(lower).map(|i| i as u32)
}

/// One character's (primary, secondary, tertiary) weights.
fn weights(c: char) -> (u32, u32, u32) {
    if c.is_ascii() {
        let p = ascii_primary(c).unwrap_or(c as u32);
        return (p, 0, c.is_ascii_uppercase() as u32);
    }
    // A Latin letter with a mark: its base letter, then the mark.
    let base = fold(c);
    if let Some(b) = base {
        let p = ascii_primary(b).unwrap_or(b as u32);
        let upper = c.is_uppercase() as u32;
        return (p, c.to_lowercase().next().unwrap_or(c) as u32, upper);
    }
    (1000 + c as u32, 0, c.is_uppercase() as u32)
}

fn fold(c: char) -> Option<char> {
    let table = [
        ("àáâãäåāăą", 'a'), ("ÀÁÂÃÄÅĀĂĄ", 'A'), ("çćĉċč", 'c'), ("ÇĆĈĊČ", 'C'), ("ďđ", 'd'), ("ĎĐ", 'D'),
        ("èéêëēĕėęě", 'e'), ("ÈÉÊËĒĔĖĘĚ", 'E'), ("ĝğġģ", 'g'), ("ĜĞĠĢ", 'G'), ("ĥħ", 'h'), ("ĤĦ", 'H'),
        ("ìíîïĩīĭįı", 'i'), ("ÌÍÎÏĨĪĬĮİ", 'I'), ("ĵ", 'j'), ("Ĵ", 'J'), ("ķ", 'k'), ("Ķ", 'K'), ("ĺļľŀł", 'l'),
        ("ĹĻĽĿŁ", 'L'), ("ñńņňŉ", 'n'), ("ÑŃŅŇ", 'N'), ("òóôõöøōŏő", 'o'), ("ÒÓÔÕÖØŌŎŐ", 'O'), ("ŕŗř", 'r'),
        ("ŔŖŘ", 'R'), ("śŝşšß", 's'), ("ŚŜŞŠ", 'S'), ("ţťŧ", 't'), ("ŢŤŦ", 'T'), ("ùúûüũūŭůűų", 'u'),
        ("ÙÚÛÜŨŪŬŮŰŲ", 'U'), ("ŵ", 'w'), ("Ŵ", 'W'), ("ýÿŷ", 'y'), ("ÝŶŸ", 'Y'), ("źżž", 'z'), ("ŹŻŽ", 'Z'),
    ];
    table.iter().find(|(set, _)| set.contains(c)).map(|(_, b)| *b)
}

/// `a.localeCompare(b)`.
pub fn compare(a: &str, b: &str) -> Ordering {
    let wa: Vec<_> = a.chars().map(weights).collect();
    let wb: Vec<_> = b.chars().map(weights).collect();
    let level = |f: fn(&(u32, u32, u32)) -> u32| wa.iter().map(f).cmp(wb.iter().map(f));
    level(|w| w.0).then_with(|| level(|w| w.1)).then_with(|| level(|w| w.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_like_javascript() {
        let mut v = vec!["ab", "a-b", "a_b", "a.b", "a/b", "aB", "Ab", "a b", "a1", "a10", "a2"];
        v.sort_by(|a, b| compare(a, b));
        assert_eq!(v, ["a b", "a_b", "a-b", "a.b", "a/b", "a1", "a10", "a2", "ab", "aB", "Ab"]);
        let mut p = vec!["/openwiki/a.md", "/openwiki/a-b.md", "/openwiki/a/b.md", "/openwiki/a_b.md", "/openwiki/Capital/Case.md", "/openwiki/capital.md", "/openwiki/crlf.md"];
        p.sort_by(|a, b| compare(a, b));
        assert_eq!(p, ["/openwiki/a_b.md", "/openwiki/a-b.md", "/openwiki/a.md", "/openwiki/a/b.md", "/openwiki/capital.md", "/openwiki/Capital/Case.md", "/openwiki/crlf.md"]);
        let mut l = vec!["b", "á", "a", "A", "Á", "e", "é"];
        l.sort_by(|a, b| compare(a, b));
        assert_eq!(l, ["a", "A", "á", "Á", "b", "e", "é"]);
    }
}
