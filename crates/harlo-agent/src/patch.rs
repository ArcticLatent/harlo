//! Edits as a patch, for GPT models (measured: as often right, fewer
//! tokens where several files change): one `apply_patch` call changes,
//! adds, moves and deletes files, in the patch format below. A hunk's lines are found in order from where the
//! last one ended (after its `@@` line, when it names one), exactly
//! first, then with less strictness as `edit` does (trailing whitespace,
//! typographic characters, indentation). Nothing is written unless every
//! hunk of every file applies.
//!
//! ```text
//! *** Begin Patch
//! *** Update File: src/a.rs
//! @@ fn main() {
//!      let x = 1;
//! -    println!("{x}");
//! +    println!("x = {x}");
//! *** Add File: src/b.rs
//! +pub fn b() {}
//! *** Delete File: src/old.rs
//! *** End Patch
//! ```

use serde_json::Value;

use crate::tools::edit::{not_found, plain};

/// Whether `apply_patch` is offered instead of `edit` to `model`
/// (`provider/id`): for GPT models, the format they're trained on.
/// `HARLO_AGENT_EDIT=patch` or `edit` (or `hashline`) decides instead.
pub fn on(model: &str) -> bool {
    match std::env::var("HARLO_AGENT_EDIT").as_deref() {
        Ok("patch") => true,
        Ok("edit" | "hashline") => false,
        _ => model.rsplit('/').next().unwrap_or(model).starts_with("gpt-"),
    }
}

/// Text written for `edit`, made to name apply_patch.
pub fn reworded(text: &str) -> String {
    text.replace("Change files with edit (small, exact replacements) or write (new files)", "Change files with apply_patch (or write, for a whole new file)")
        .replace("To change part of a file, use edit.", "To change part of a file, use apply_patch.")
        .replace("use edit or write.", "use apply_patch or write.")
}

pub fn spec() -> Value {
    serde_json::json!({
        "type": "function", "name": "apply_patch", "strict": false,
        "description": "Change, add, move or delete files with a patch. The input starts with `*** Begin Patch` and ends with `*** End Patch`. Between them, for each file:\n\
            `*** Update File: <path>` (then `*** Move to: <path>` to rename it), followed by hunks. A hunk starts with `@@`, optionally followed by a line it comes after (a function or class line, to say where); then its lines, each starting with ' ' (kept), '-' (removed) or '+' (added). Give about 3 kept lines before and after each change; the kept and removed lines must be as the file has them now.\n\
            `*** Add File: <path>` followed by the new file's lines, each starting with '+'.\n\
            `*** Delete File: <path>`.\n\
            Paths are relative to the working directory. All of it applies, or none of it. Refused if a file changed since you last read it.",
        "parameters": {
            "type": "object",
            "properties": { "input": { "type": "string", "description": "The whole patch" } },
            "required": ["input"], "additionalProperties": false,
        },
    })
}

#[derive(Debug, PartialEq)]
pub enum Op {
    Add { path: String, content: String },
    Delete { path: String },
    Update { path: String, move_to: Option<String>, hunks: Vec<Hunk> },
}

impl Hunk {
    /// The lines it expects in the file (kept and removed).
    fn old(&self) -> Vec<String> {
        self.lines.iter().filter(|l| l.0 != '+').map(|l| l.1.clone()).collect()
    }
}

impl Op {
    pub fn path(&self) -> &str {
        match self {
            Op::Add { path, .. } | Op::Delete { path } | Op::Update { path, .. } => path,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Hunk {
    /// The `@@` line's text: a line the change comes after.
    pub after: Option<String>,
    /// Each line with its mark: ' ' kept, '-' removed, '+' added.
    pub lines: Vec<(char, String)>,
    /// `*** End of File`: the hunk ends the file.
    pub at_end: bool,
}

/// The patch's operations, or what's wrong with it (for the model).
pub fn parse(input: &str) -> Result<Vec<Op>, String> {
    let lines: Vec<&str> = input.lines().map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    // (A shell heredoc around it, or text before it: from Begin Patch on.)
    let Some(begin) = lines.iter().position(|l| l.trim() == "*** Begin Patch") else {
        return Err("The patch must start with `*** Begin Patch` (and end with `*** End Patch`).".into());
    };
    let end = lines.iter().rposition(|l| l.trim() == "*** End Patch").filter(|&e| e > begin).unwrap_or(lines.len());
    let body = &lines[begin + 1..end];
    let mut ops: Vec<Op> = Vec::new();
    let mut i = 0;
    let header = |l: &str, tag: &str| l.strip_prefix(tag).map(|p| p.trim().to_owned());
    while i < body.len() {
        let l = body[i];
        let n = begin + i + 2;
        if l.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some(path) = header(l, "*** Add File: ") {
            i += 1;
            let mut content = String::new();
            while i < body.len() && !body[i].starts_with("*** ") {
                match body[i].strip_prefix('+') {
                    Some(text) => {
                        content.push_str(text);
                        content.push('\n');
                    }
                    None if body[i].is_empty() => content.push('\n'),
                    None => return Err(format!("Line {}: a new file's lines each start with '+' (in {path}).", begin + i + 2)),
                }
                i += 1;
            }
            ops.push(Op::Add { path, content });
        } else if let Some(path) = header(l, "*** Delete File: ") {
            ops.push(Op::Delete { path });
            i += 1;
        } else if let Some(path) = header(l, "*** Update File: ") {
            i += 1;
            let mut move_to = None;
            if let Some(to) = body.get(i).and_then(|l| header(l, "*** Move to: ")) {
                move_to = Some(to);
                i += 1;
            }
            let mut hunks: Vec<Hunk> = Vec::new();
            let mut cur: Option<Hunk> = None;
            while i < body.len() {
                let l = body[i];
                if l.trim() == "*** End of File" {
                    cur.get_or_insert_with(Hunk::default).at_end = true;
                    i += 1;
                    continue;
                }
                if l.starts_with("*** ") {
                    break;
                }
                if let Some(rest) = l.strip_prefix("@@") {
                    hunks.extend(cur.take().filter(|h| !h.lines.is_empty()));
                    // (`@@ -12,3 +12,4 @@`, as diff writes it: no line to look for.)
                    let after = rest.trim();
                    let numbers = after.starts_with('-') && after.contains('+');
                    let after = if numbers { after.rsplit_once("@@").map_or("", |(_, r)| r).trim() } else { after.trim_end_matches("@@").trim() };
                    cur = Some(Hunk { after: (!after.is_empty()).then(|| after.to_owned()), ..Hunk::default() });
                    i += 1;
                    continue;
                }
                let h = cur.get_or_insert_with(Hunk::default);
                match l.chars().next() {
                    Some(mark @ (' ' | '-' | '+')) => h.lines.push((mark, l[1..].to_owned())),
                    // (An empty line: a kept empty line, its space dropped.)
                    None => h.lines.push((' ', String::new())),
                    Some(_) => return Err(format!("Line {}: in a hunk each line starts with ' ', '-' or '+' (in {path}), not: {l}", begin + i + 2)),
                }
                i += 1;
            }
            hunks.extend(cur.filter(|h| !h.lines.is_empty()));
            if hunks.is_empty() && move_to.is_none() {
                return Err(format!("Line {n}: no changes for {path}: give hunks after `*** Update File:`."));
            }
            ops.push(Op::Update { path, move_to, hunks });
        } else {
            return Err(format!("Line {n}: expected `*** Update File: `, `*** Add File: ` or `*** Delete File: `, not: {l}"));
        }
    }
    if ops.is_empty() {
        return Err("The patch changes nothing.".into());
    }
    Ok(ops)
}

/// Where `want` starts, from `from` on (from the end first, when `at_end`).
fn seek(lines: &[&str], want: &[String], from: usize, at_end: bool) -> Option<usize> {
    if want.is_empty() || want.len() > lines.len() {
        return None;
    }
    let last = lines.len() - want.len();
    let levels: [fn(&str) -> String; 4] = [|s| s.to_owned(), |s| s.trim_end().to_owned(), |s| s.trim().to_owned(), |s| plain(s.trim())];
    for norm in levels {
        let want_n: Vec<String> = want.iter().map(|l| norm(l)).collect();
        let fits = |s: usize| (0..want.len()).all(|k| norm(lines[s + k]) == want_n[k]);
        if at_end && last >= from && fits(last) {
            return Some(last);
        }
        if let Some(s) = (from..=last).find(|&s| fits(s)) {
            return Some(s);
        }
    }
    None
}

/// `content` with the hunks applied, and its first changed line (1-based).
pub fn apply(content: &str, hunks: &[Hunk], path: &str) -> Result<(String, usize), String> {
    let (bom, body) = match content.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", content),
    };
    let crlf = body.contains("\r\n");
    let text = if crlf { body.replace("\r\n", "\n") } else { body.to_owned() };
    let ends_nl = text.ends_with('\n') || text.is_empty();
    let mut lines: Vec<&str> = text.split('\n').collect();
    if ends_nl {
        lines.pop();
    }
    let mut at = 0;
    let mut swaps: Vec<(usize, usize, Vec<String>)> = Vec::new();
    for (k, h) in hunks.iter().enumerate() {
        let which = if hunks.len() > 1 { format!("Hunk {} of {path}", k + 1) } else { format!("The hunk for {path}") };
        if let Some(after) = &h.after {
            match seek(&lines, std::slice::from_ref(after), at, false) {
                Some(s) => at = s + 1,
                // (Not there: the hunk's own lines may still place it.)
                None if h.lines.iter().any(|l| l.0 != '+') => {}
                None => return Err(format!("{which}: its @@ line `{after}` isn't in the file (after line {at}).")),
            }
        }
        let mut marked = h.lines.clone();
        let mut old = h.old();
        if old.is_empty() {
            let pos = if h.after.is_some() && !h.at_end { at } else { lines.len() };
            swaps.push((pos, 0, marked.into_iter().map(|l| l.1).collect()));
            continue;
        }
        let mut found = seek(&lines, &old, at, h.at_end);
        // (A trailing empty kept line the file doesn't have at that spot.)
        if found.is_none() && marked.last().is_some_and(|l| l == &(' ', String::new())) {
            marked.pop();
            old.pop();
            found = seek(&lines, &old, at, h.at_end);
        }
        let Some(s) = found else {
            // Before where the last hunk ended: out of order.
            if at > 0 && seek(&lines, &old, 0, false).is_some_and(|s| s < at) {
                return Err(format!("{which} matches only before the hunk ahead of it: give a file's hunks in the order they come in the file."));
            }
            let want: Vec<&str> = old.iter().map(String::as_str).collect();
            let added: Vec<&str> = marked.iter().filter(|l| l.0 != '-').map(|l| l.1.as_str()).collect();
            let why = not_found(&text, &lines, &want, &added.join("\n"));
            let why = why
                .replace("but the new text already is", "but its new lines already are")
                .replace("The old text isn't", "Its kept and removed lines aren't")
                .replace("the text to replace", "the lines");
            return Err(format!("{which}: {why}"));
        };
        // Kept lines as the file has them; added ones as given.
        let mut k = s;
        let mut new = Vec::new();
        for (mark, text) in marked {
            match mark {
                ' ' => new.push(lines[k].to_owned()),
                '+' => new.push(text),
                _ => {}
            }
            if mark != '+' {
                k += 1;
            }
        }
        at = s + old.len();
        swaps.push((s, old.len(), new));
    }
    let first = swaps.iter().map(|s| s.0).min().unwrap_or(0) + 1;
    let mut out: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
    swaps.sort_by_key(|s| s.0);
    for (s, n, new) in swaps.into_iter().rev() {
        out.splice(s..s + n, new);
    }
    let mut joined = out.join("\n");
    if ends_nl && !out.is_empty() {
        joined.push('\n');
    }
    if joined == text {
        return Err(format!("The patch for {path} changes nothing."));
    }
    let joined = if crlf { joined.replace('\n', "\r\n") } else { joined };
    Ok((format!("{bom}{joined}"), first))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(input: &str) -> Vec<Hunk> {
        match parse(input).unwrap().into_iter().next().unwrap() {
            Op::Update { hunks, .. } => hunks,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn offered_to_gpt_models_and_named_where_edit_was() {
        if std::env::var_os("HARLO_AGENT_EDIT").is_none() {
            assert!(on("codex/gpt-6.1-sol") && on("router/openai/gpt-5") && !on("local/qwen3-4b") && !on("fake/fake-coder"));
        }
        let specs = crate::tools::specs_of(None, None, true).to_string();
        assert!(specs.contains("\"name\":\"apply_patch\"") && !specs.contains("\"name\":\"edit\"") && !specs.contains("use edit"), "{specs}");
        let rules = reworded(&crate::agent::instructions(std::path::Path::new("/nowhere"), None));
        assert!(rules.contains("Change files with apply_patch") && !rules.contains("with edit"), "{rules}");
    }

    #[test]
    fn parses_every_kind_of_change() {
        let p = "apply_patch <<'EOF'\n*** Begin Patch\n*** Add File: b.txt\n+one\n+two\n*** Update File: a.txt\n*** Move to: c.txt\n@@ fn main\n keep\n-old\n+new\n*** Delete File: d.txt\n*** End Patch\nEOF\n";
        let ops = parse(p).unwrap();
        assert_eq!(ops[0], Op::Add { path: "b.txt".into(), content: "one\ntwo\n".into() });
        assert_eq!(
            ops[1],
            Op::Update { path: "a.txt".into(), move_to: Some("c.txt".into()), hunks: vec![Hunk { after: Some("fn main".into()), lines: vec![(' ', "keep".into()), ('-', "old".into()), ('+', "new".into())], at_end: false }] }
        );
        assert_eq!(ops[2], Op::Delete { path: "d.txt".into() });
        assert!(parse("*** Update File: a\n-x").unwrap_err().contains("Begin Patch"));
        assert!(parse("*** Begin Patch\n*** Update File: a\nx\n*** End Patch").unwrap_err().contains("Line 3"));
        assert_eq!(update("*** Begin Patch\n*** Update File: a\n@@ -3,2 +3,2 @@ impl A\n-x\n+y\n*** End Patch")[0].after.as_deref(), Some("impl A"));
    }

    #[test]
    fn hunks_apply_in_order_after_their_anchor() {
        let f = "fn a() {\n    x();\n}\nfn b() {\n    x();\n}\n";
        let h = update("*** Begin Patch\n*** Update File: f.rs\n@@ fn b() {\n-    x();\n+    y();\n*** End Patch");
        assert_eq!(apply(f, &h, "f.rs").unwrap(), ("fn a() {\n    x();\n}\nfn b() {\n    y();\n}\n".into(), 5));
        // Two hunks, the second searched from where the first ended.
        let h = update("*** Begin Patch\n*** Update File: f.rs\n@@\n-    x();\n+    one();\n@@\n-    x();\n+    two();\n*** End Patch");
        assert_eq!(apply(f, &h, "f.rs").unwrap().0, "fn a() {\n    one();\n}\nfn b() {\n    two();\n}\n");
        // Added at the end, and after a line.
        let h = update("*** Begin Patch\n*** Update File: f.rs\n@@\n+fn c() {}\n*** End of File\n*** End Patch");
        assert!(apply(f, &h, "f.rs").unwrap().0.ends_with("}\nfn c() {}\n"));
        let h = update("*** Begin Patch\n*** Update File: f.rs\n@@ fn a() {\n+    first();\n*** End Patch");
        assert!(apply(f, &h, "f.rs").unwrap().0.starts_with("fn a() {\n    first();\n    x();"));
    }

    #[test]
    fn matches_with_less_strictness_and_keeps_line_endings() {
        let f = "\u{feff}let a = 1;   \r\nsay(\u{201c}hi\u{201d})\r\n";
        let h = update("*** Begin Patch\n*** Update File: f\n let a = 1;\n-say(\"hi\")\n+say(\"bye\")\n*** End Patch");
        assert_eq!(apply(f, &h, "f").unwrap().0, "\u{feff}let a = 1;   \r\nsay(\"bye\")\r\n");
        // No newline at the end, kept so.
        let h = update("*** Begin Patch\n*** Update File: f\n-b\n+c\n*** End Patch");
        assert_eq!(apply("a\nb", &h, "f").unwrap().0, "a\nc");
    }

    #[test]
    fn says_what_went_wrong() {
        let f = "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n}\n";
        let h = update("*** Begin Patch\n*** Update File: m.rs\n-    println!(\"three\");\n     println!(\"two\");\n+    x\n*** End Patch");
        let e = apply(f, &h, "m.rs").unwrap_err();
        assert!(e.starts_with("The hunk for m.rs: Its kept and removed lines aren't in the file. The closest"), "{e}");
        assert!(e.contains("    3      println!(\"two\");"), "{e}");
        let h = update("*** Begin Patch\n*** Update File: m.rs\n@@\n-    println!(\"two\");\n+2\n@@\n-    println!(\"one\");\n+1\n*** End Patch");
        assert!(apply(f, &h, "m.rs").unwrap_err().contains("in the order"), "out of order");
        let h = update("*** Begin Patch\n*** Update File: m.rs\n@@ fn nope()\n+x\n*** End Patch");
        assert!(apply(f, &h, "m.rs").unwrap_err().contains("`fn nope()` isn't in the file"));
    }
}
