//! Hash-anchored edits (`HARLO_AGENT_EDIT=hashline`, to measure): `read`
//! shows each line as `N#AB:text`, AB two letters from the line's content,
//! and `edit` names lines by those anchors instead of copying old text. A
//! line that changed since it was read has another tag, and the edit is
//! refused with the lines as they are now.

use serde_json::Value;

/// Whether reads and edits use anchors.
pub fn on() -> bool {
    std::env::var("HARLO_AGENT_EDIT").is_ok_and(|v| v == "hashline")
}

/// A line's tag: FNV-1a of its text without whitespace, as two letters
/// (a line with no letters or digits by its number, so blank lines differ).
pub fn tag(line: &str, n: usize) -> String {
    let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    let seed = if squeezed.chars().any(char::is_alphanumeric) { squeezed } else { format!("{squeezed}#{n}") };
    let h = seed.bytes().fold(0x811c_9dc5_u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
    let a = (b'A' + (h % 26) as u8) as char;
    let b = (b'A' + ((h / 26) % 26) as u8) as char;
    format!("{a}{b}")
}

/// `N#AB:text` (N from 1).
pub fn annotate(n: usize, line: &str) -> String {
    format!("{n}#{}:{line}", tag(line, n))
}

/// The spec for `edit` in this mode.
pub fn spec() -> Value {
    serde_json::json!({
        "type": "function", "name": "edit", "strict": false,
        "description": "Change lines of a file by their anchors from read (N#AB: line number and tag), without copying old text. \
                        Each edit: replace the lines pos to end (end defaults to pos; empty lines deletes them), or prepend/append lines before/after pos. \
                        Anchors are of the file as you last read it; all edits apply together. If a line changed since, its tag differs and the edit is refused with the lines as they are now. \
                        Give lines without anchors.",
        "parameters": {
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "edits": { "type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "op": { "type": "string", "enum": ["replace", "prepend", "append"] },
                        "pos": { "type": "string", "description": "An anchor, as 12#QX" },
                        "end": { "type": "string", "description": "replace: the last line's anchor (default pos)" },
                        "lines": { "type": "array", "items": { "type": "string" } },
                    },
                    "required": ["op", "pos", "lines"], "additionalProperties": false,
                } },
            },
            "required": ["path", "edits"], "additionalProperties": false,
        },
    })
}

/// `12#QX` → (12, "QX").
fn anchor(a: &str) -> Option<(usize, String)> {
    let (n, t) = a.trim().split_once('#')?;
    let t = t.split(':').next().unwrap_or(t).trim();
    Some((n.trim().parse().ok().filter(|n| *n > 0)?, t.to_uppercase()))
}

/// A line as the model may have copied it: its anchor taken off.
fn bare(line: &str) -> &str {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let rest = &line[digits..];
    if digits > 0
        && let Some(r) = rest.strip_prefix('#')
        && r.len() >= 3
        && r.as_bytes()[..2].iter().all(u8::is_ascii_uppercase)
        && r.as_bytes()[2] == b':'
    {
        return &r[3..];
    }
    line
}

/// The lines around `n` as they are now (for a refusal).
fn around(lines: &[&str], n: usize) -> String {
    let (from, to) = (n.saturating_sub(3).max(1), (n + 3).min(lines.len()));
    (from..=to).map(|i| annotate(i, lines[i - 1])).collect::<Vec<_>>().join("\n")
}

/// The file's text with `edits` applied, and what to show of the result.
pub fn apply(content: &str, edits: &Value) -> Result<(String, String), String> {
    let lines: Vec<&str> = content.lines().collect();
    let list = edits.as_array().filter(|l| !l.is_empty()).ok_or("Give at least one edit: op, pos and lines.")?;
    let check = |a: &str| -> Result<usize, String> {
        let (n, t) = anchor(a).ok_or(format!("{a:?} isn't an anchor: give one as read shows it, like 12#QX."))?;
        if n > lines.len() {
            return Err(format!("Line {n} is past the file's end ({} lines).", lines.len()));
        }
        if tag(lines[n - 1], n) != t {
            return Err(format!("Line {n} changed since you read it (its anchor is now {}). The lines there now:\n{}", annotate(n, lines[n - 1]).split(':').next().unwrap_or_default(), around(&lines, n)));
        }
        Ok(n)
    };
    // (Start, end inclusive, the lines in its place; prepends end before start.)
    let mut changes: Vec<(usize, usize, Vec<String>)> = Vec::new();
    for e in list {
        let new: Vec<String> = e["lines"].as_array().into_iter().flatten().filter_map(Value::as_str).map(|l| bare(l).to_owned()).collect();
        let pos = check(e["pos"].as_str().unwrap_or_default())?;
        match e["op"].as_str().unwrap_or("replace") {
            "replace" => {
                let end = match e["end"].as_str().filter(|s| !s.trim().is_empty()) {
                    Some(a) => check(a)?,
                    None => pos,
                };
                if end < pos {
                    return Err(format!("end ({end}) is before pos ({pos})."));
                }
                changes.push((pos, end, new));
            }
            "prepend" => changes.push((pos, pos - 1, new)),
            "append" => changes.push((pos + 1, pos, new)),
            other => return Err(format!("op {other:?}: use replace, prepend or append.")),
        }
    }
    changes.sort_by_key(|c| (c.0, c.1));
    for w in changes.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        // (Inserts at the same place are fine; replaced ranges may not overlap.)
        if b.0 <= a.1 && a.1 >= a.0 {
            return Err(format!("Two edits change line {}: make them one.", b.0));
        }
    }
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    for (start, end, new) in changes.iter().rev() {
        let (s, removed) = (start - 1, (end + 1).saturating_sub(*start));
        out.splice(s..s + removed, new.iter().cloned());
    }
    // Where each change ended up, with fresh anchors.
    let mut offset: isize = 0;
    let mut view = Vec::new();
    for (start, end, new) in &changes {
        let at = (*start as isize + offset) as usize;
        let (from, to) = (at.saturating_sub(1).max(1), (at + new.len()).min(out.len()));
        view.push((from..=to.max(from)).filter(|i| *i <= out.len()).map(|i| annotate(i, &out[i - 1])).collect::<Vec<_>>().join("\n"));
        offset += new.len() as isize - (end + 1).saturating_sub(*start) as isize;
    }
    let mut text = out.join("\n");
    if content.ends_with('\n') || content.is_empty() {
        text.push('\n');
    }
    Ok((text, view.join("\n…\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tags_and_anchors() {
        assert_eq!(tag("  let x = 1;", 3), tag("let x=1;", 9));
        assert_ne!(tag("", 1), tag("", 2));
        assert_eq!(anchor("12#qx"), Some((12, "QX".into())));
        assert_eq!(anchor("12#QX:let y"), Some((12, "QX".into())));
        assert_eq!(bare("4#AB:  hello"), "  hello");
        assert_eq!(bare("4 apples"), "4 apples");
    }

    #[test]
    fn edits_by_anchor() {
        let text = "one\ntwo\nthree\nfour\n";
        let a = |n: usize, l: &str| format!("{n}#{}", tag(l, n));
        let (out, view) = apply(text, &json!([
            { "op": "replace", "pos": a(2, "two"), "lines": ["TWO", "2#ZZ:two and a half"] },
            { "op": "append", "pos": a(4, "four"), "lines": ["five"] },
            { "op": "prepend", "pos": a(1, "one"), "lines": ["zero"] },
        ]))
        .unwrap();
        assert_eq!(out, "zero\none\nTWO\ntwo and a half\nthree\nfour\nfive\n");
        assert!(view.contains(&annotate(3, "TWO")) && view.contains(&annotate(7, "five")), "{view}");
        // Deleting a range.
        let (out, _) = apply(text, &json!([{ "op": "replace", "pos": a(2, "two"), "end": a(3, "three"), "lines": [] }])).unwrap();
        assert_eq!(out, "one\nfour\n");
        // A stale anchor: refused, with the lines now.
        let e = apply("one\nTWO\nthree\n", &json!([{ "op": "replace", "pos": a(2, "two"), "lines": ["x"] }])).unwrap_err();
        assert!(e.starts_with("Line 2 changed since you read it") && e.contains(&annotate(2, "TWO")), "{e}");
        // Overlaps refused.
        let e = apply(text, &json!([{ "op": "replace", "pos": a(1, "one"), "end": a(3, "three"), "lines": [] }, { "op": "replace", "pos": a(2, "two"), "lines": [] }])).unwrap_err();
        assert!(e.contains("Two edits change line 2"), "{e}");
    }
}
