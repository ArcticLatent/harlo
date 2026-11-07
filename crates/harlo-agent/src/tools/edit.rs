//! Text edits: each `old → new` must match exactly one place in the file.
//!
//! An exact match first. If there's none, the same lines with less
//! strictness, one step at a time: trailing whitespace ignored, then
//! typographic characters as plain ones (curly quotes, dashes, odd
//! spaces), then indentation (the new text re-indented to the file's). A
//! step finding more than one place refuses. Nothing fuzzier: a near
//! match picked by similarity can land on the wrong code.
//!
//! All edits match against the file as it was, then apply together. Line
//! endings (CRLF) and a byte-order mark are kept.

/// One replacement, and its 1-based first changed line once applied.
pub struct Edit {
    pub old: String,
    pub new: String,
}

/// The new content and the first changed line, or why not (for the model).
pub fn apply(content: &str, edits: &[Edit]) -> Result<(String, usize), String> {
    let (bom, body) = match content.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", content),
    };
    let crlf = body.contains("\r\n");
    let text = if crlf { body.replace("\r\n", "\n") } else { body.to_owned() };
    let mut spans: Vec<(usize, usize, String)> = Vec::new();
    for (i, e) in edits.iter().enumerate() {
        let which = if edits.len() > 1 { format!("edits[{i}]: ") } else { String::new() };
        let old = e.old.replace("\r\n", "\n");
        let new = e.new.replace("\r\n", "\n");
        if old.is_empty() {
            return Err(format!("{which}the old text is empty: give the text to replace (to create a file, use write)."));
        }
        if old == new {
            return Err(format!("{which}the old and new text are the same: nothing to change."));
        }
        spans.push(locate(&text, &old, &new).map_err(|why| format!("{which}{why}"))?);
    }
    spans.sort_by_key(|s| s.0);
    if let Some(w) = spans.windows(2).find(|w| w[1].0 < w[0].1) {
        let _ = w;
        return Err("Two edits overlap: merge them into one, or make each cover a different part.".into());
    }
    let first = spans.first().map_or(1, |s| line_of(&text, s.0));
    let mut out = text.clone();
    for (start, end, new) in spans.into_iter().rev() {
        out.replace_range(start..end, &new);
    }
    if out == text {
        return Err("The edit changes nothing.".into());
    }
    let out = if crlf { out.replace('\n', "\r\n") } else { out };
    Ok((format!("{bom}{out}"), first))
}

fn line_of(text: &str, byte: usize) -> usize {
    text[..byte].matches('\n').count() + 1
}

/// Where `old` is (byte range) and what replaces it.
fn locate(text: &str, old: &str, new: &str) -> Result<(usize, usize, String), String> {
    let exact: Vec<usize> = text.match_indices(old).map(|(i, _)| i).collect();
    match exact.len() {
        1 => return Ok((exact[0], exact[0] + old.len(), new.to_owned())),
        n if n > 1 => return Err(not_unique(text, &exact)),
        _ => {}
    }
    // Line by line, less strict each step.
    let lines: Vec<&str> = text.split('\n').collect();
    let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let old_t = old.strip_suffix('\n').unwrap_or(old);
    let new_t = new.strip_suffix('\n').unwrap_or(new);
    let want: Vec<&str> = old_t.split('\n').collect();
    for level in [Level::TrailingSpace, Level::Typographic, Level::Indentation] {
        let norm = |s: &str| level.normalize(s);
        let want_n: Vec<String> = want.iter().map(|l| norm(l)).collect();
        if want_n.iter().all(|l| l.is_empty()) {
            break;
        }
        let found: Vec<usize> = (0..lines.len().saturating_sub(want.len() - 1)).filter(|&s| (0..want.len()).all(|k| norm(lines[s + k]) == want_n[k])).collect();
        match found.len() {
            0 => continue,
            1 => {
                let s = found[0];
                let e = s + want.len() - 1;
                let start = starts[s];
                let end = starts[e] + lines[e].len();
                let replacement = if level == Level::Indentation { reindent(new_t, want[0], lines[s]) } else { new_t.to_owned() };
                return Ok((start, end, replacement));
            }
            _ => {
                let at: Vec<usize> = found.iter().map(|&s| starts[s]).collect();
                return Err(not_unique(text, &at));
            }
        }
    }
    Err(not_found(text, &lines, &want, new))
}

#[derive(Clone, Copy, PartialEq)]
enum Level {
    TrailingSpace,
    Typographic,
    Indentation,
}

impl Level {
    fn normalize(self, line: &str) -> String {
        match self {
            Level::TrailingSpace => line.trim_end().to_owned(),
            Level::Typographic => plain(line.trim_end()),
            Level::Indentation => plain(line.trim()),
        }
    }
}

/// Typographic characters as the plain ones a model types.
fn plain(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' | '\u{2032}' => '\'',
            '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' | '\u{2033}' => '"',
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{2212}' => '-',
            '\u{00a0}' | '\u{2007}' | '\u{202f}' | '\u{2000}'..='\u{200a}' | '\u{3000}' => ' ',
            c => c,
        })
        .collect()
}

/// `new` with `old_first`'s indentation swapped for the file's (`file_first`).
fn reindent(new: &str, old_first: &str, file_first: &str) -> String {
    let indent = |l: &str| l[..l.len() - l.trim_start().len()].to_owned();
    let (from, to) = (indent(old_first), indent(file_first));
    new.split('\n')
        .map(|l| match l.strip_prefix(from.as_str()) {
            Some(rest) if !l.trim().is_empty() => format!("{to}{rest}"),
            _ => l.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn not_unique(text: &str, at: &[usize]) -> String {
    let lines: Vec<String> = at.iter().take(5).map(|&i| line_of(text, i).to_string()).collect();
    format!(
        "The old text is in {} places (lines {}{}): include more of the surrounding lines so it matches only one.",
        at.len(),
        lines.join(", "),
        if at.len() > 5 { ", …" } else { "" }
    )
}

/// Not found: where it's closest (so the model can copy the real text), or
/// that the new text is already there.
fn not_found(text: &str, lines: &[&str], want: &[&str], new: &str) -> String {
    if !new.trim().is_empty() && text.contains(new.trim()) {
        return format!("The old text isn't in the file, but the new text already is (line {}): the edit may have been made already. Read the file to check.", line_of(text, text.find(new.trim()).unwrap_or(0)));
    }
    let norm = |s: &str| plain(s.trim());
    let want_n: Vec<String> = want.iter().map(|l| norm(l)).filter(|l| !l.is_empty()).collect();
    let mut best = (0usize, 0usize);
    for s in 0..lines.len() {
        let end = (s + want.len()).min(lines.len());
        let hits = (s..end).filter(|&k| {
            let l = norm(lines[k]);
            !l.is_empty() && want_n.contains(&l)
        });
        let hits = hits.count();
        if hits > best.1 {
            best = (s, hits);
        }
    }
    if best.1 == 0 {
        return "The old text isn't in the file, and no lines like it are. Read the file and copy the text to replace exactly.".into();
    }
    let end = (best.0 + want.len().clamp(1, 12)).min(lines.len());
    let shown: Vec<String> = (best.0..end).map(|k| format!("{:>5}  {}", k + 1, lines[k])).collect();
    format!("The old text isn't in the file. The closest lines are these; copy the text to replace from them exactly:\n{}", shown.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(content: &str, old: &str, new: &str) -> Result<String, String> {
        apply(content, &[Edit { old: old.into(), new: new.into() }]).map(|r| r.0)
    }

    #[test]
    fn exact_and_unique() {
        assert_eq!(one("a\nb\nc\n", "b", "B").unwrap(), "a\nB\nc\n");
        assert!(one("x = 1\nx = 1\n", "x = 1", "x = 2").unwrap_err().contains("in 2 places (lines 1, 2)"));
        assert!(one("a", "", "b").unwrap_err().contains("empty"));
    }

    #[test]
    fn near_duplicates_need_context() {
        let f = "def area(r):\n    return 3.14 * r * r\n\ndef area2(r):\n    return 3.14 * r * r\n";
        assert!(one(f, "    return 3.14 * r * r", "    return math.pi * r * r").unwrap_err().contains("2 places"));
        let ok = one(f, "def area2(r):\n    return 3.14 * r * r", "def area2(r):\n    return math.pi * r * r").unwrap();
        assert!(ok.ends_with("def area2(r):\n    return math.pi * r * r\n") && ok.starts_with("def area(r):\n    return 3.14"));
    }

    #[test]
    fn falls_back_step_by_step() {
        // Trailing spaces in the file.
        assert_eq!(one("let a = 1;   \nlet b = 2;\n", "let a = 1;\nlet b = 2;", "let a = 3;\nlet b = 2;").unwrap(), "let a = 3;\nlet b = 2;\n");
        // Curly quotes in the file, straight in the model's text.
        assert_eq!(one("say(\u{201c}hi\u{201d})\n", "say(\"hi\")", "say(\"bye\")").unwrap(), "say(\"bye\")\n");
        // Indentation: the model's text at another depth, re-indented.
        let f = "class A:\n    def f(self):\n        return 1\n";
        assert_eq!(one(f, "def f(self):\n    return 1", "def f(self):\n    return 2").unwrap(), "class A:\n    def f(self):\n        return 2\n");
    }

    #[test]
    fn keeps_crlf_bom_and_non_ascii() {
        assert_eq!(one("\u{feff}a\r\nb\r\n", "a\nb", "a\nc").unwrap(), "\u{feff}a\r\nc\r\n");
        assert_eq!(one("name = \"Zoë\" # ünïcode\n", "\"Zoë\"", "\"Zoé\"").unwrap(), "name = \"Zoé\" # ünïcode\n");
    }

    #[test]
    fn says_what_went_wrong() {
        let f = "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n}\n";
        let e = one(f, "    println!(\"three\");\n    println!(\"two\");", "x").unwrap_err();
        assert!(e.contains("closest lines") && e.contains("    3      println!(\"two\");"), "{e}");
        assert!(one(f, "println!(\"zero\");", "println!(\"one\");").unwrap_err().contains("already is (line 2)"));
        let e = apply(f, &[Edit { old: "println!(\"one\");".into(), new: "a".into() }, Edit { old: "\"one\");\n    println".into(), new: "b".into() }]).unwrap_err();
        assert!(e.contains("overlap"), "{e}");
    }

    #[test]
    fn several_edits_against_the_original() {
        let (out, first) = apply("a\nb\nc\nd\n", &[Edit { old: "d".into(), new: "D".into() }, Edit { old: "b".into(), new: "B".into() }]).unwrap();
        assert_eq!((out.as_str(), first), ("a\nB\nc\nD\n", 2));
    }
}
