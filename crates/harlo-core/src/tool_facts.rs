//! What a finished tool call came to, in a word or two, as a phone shows it
//! beside the call: lines added and removed by an edit (and the diff), lines
//! written, tests passed, an exit code, lines read, matches found. Read from
//! the call's arguments and output, whatever the agent (patches, Claude's
//! old and new strings, Harlo's edits).

use serde_json::Value;

/// A diff shown with a call, at most (lines).
const DIFF_LINES: usize = 400;

/// The result line and, for edits and writes, a diff (lines starting with
/// `+`, `-` or ` `). Empty when there's nothing to say.
pub fn facts(name: &str, args: &Value, output: &str, is_error: bool) -> (String, String) {
    let lower = name.to_lowercase();
    let short = lower.rsplit(['.', '_', ':']).next().unwrap_or(&lower).to_owned();
    if lower.contains("edit") || lower.contains("patch") || lower.contains("replace") {
        if let Some((added, removed, diff)) = edit_diff(args) {
            return (counts(added, removed), diff);
        }
    } else if short == "write" || lower.contains("write") || lower.contains("create") {
        if let Some(content) = text_arg(args, &["content", "contents", "file_text", "text"]) {
            let lines: Vec<&str> = content.lines().collect();
            let diff = lines.iter().take(DIFF_LINES).map(|l| format!("+{l}")).collect::<Vec<_>>().join("\n");
            return (format!("+{}", lines.len()), diff);
        }
    } else if is_shell(name) {
        return (command_result(output, is_error), String::new());
    } else if matches!(short.as_str(), "ask" | "askuserquestion") && !is_error {
        return (ask_result(output), String::new());
    } else if matches!(short.as_str(), "read" | "view" | "cat" | "readfile" | "file") && !is_error {
        // (A big file's read says its whole length in a note first.)
        if let Some(n) = output.strip_prefix('[').and_then(|o| o.split_once(" lines (")).and_then(|(head, _)| head.rsplit(' ').next()?.parse::<usize>().ok())
            && output.lines().next().is_some_and(|l| l.contains("These are lines "))
        {
            return (format!("{n} lines"), String::new());
        }
        let n = output.lines().count();
        return (if n == 1 { "1 line".into() } else { format!("{n} lines") }, String::new());
    } else if matches!(short.as_str(), "grep" | "search" | "glob" | "find" | "rg" | "ls" | "list") && !is_error {
        let n = output.lines().filter(|l| !l.trim().is_empty()).count();
        return (match n {
            0 => "nothing found".into(),
            1 => "1 result".into(),
            n => format!("{n} results"),
        }, String::new());
    }
    (if is_error { "failed".into() } else { String::new() }, String::new())
}

/// What was answered to a question for the user, in a few words: the
/// options picked or the words typed, each question's after the other's;
/// "dismissed" when none was. Harlo's ("The user answered:\n- Q\n  A")
/// and Claude's ("User has answered your questions: "Q"="A", …").
fn ask_result(output: &str) -> String {
    let mut said: Vec<String> = Vec::new();
    if let Some(rest) = output.strip_prefix("The user answered:") {
        said = rest.lines().filter(|l| l.starts_with("  ")).map(|l| l.trim().replace("(in their words) ", "")).filter(|a| a != "(nothing picked)").collect();
    } else if output.contains("answered") && output.contains("\"=\"") {
        // "Q"="A" pairs: the answer is the quoted text after each "=".
        let mut rest = output;
        while let Some(at) = rest.find("\"=\"") {
            rest = &rest[at + 3..];
            let end = rest.find('"').unwrap_or(rest.len());
            said.push(rest[..end].to_owned());
            rest = &rest[end..];
        }
    } else if output.contains("dismissed") || output.contains("declined") {
        return "dismissed".into();
    }
    let text = said.join("; ");
    if text.chars().count() > 40 { format!("{}…", text.chars().take(39).collect::<String>().trim_end()) } else { text }
}

/// A tool that runs a shell command.
pub fn is_shell(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(lower.rsplit(['.', '_', ':']).next().unwrap_or(&lower), "bash" | "shell" | "exec" | "command" | "run" | "terminal")
}

fn counts(added: usize, removed: usize) -> String {
    format!("+{added} −{removed}")
}

fn text_arg<'a>(args: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| args[*k].as_str())
}

/// An edit's lines added and removed, and its diff: from old and new
/// strings (one pair, or a list of edits), or from a patch in its arguments.
fn edit_diff(args: &Value) -> Option<(usize, usize, String)> {
    const PAIRS: [(&str, &str); 5] = [("old_string", "new_string"), ("old_text", "new_text"), ("oldText", "newText"), ("oldString", "newString"), ("old_str", "new_str")];
    let pair = |v: &Value| PAIRS.iter().find_map(|(o, n)| Some((v[*o].as_str()?.to_owned(), v[*n].as_str()?.to_owned())));
    let mut pairs: Vec<(String, String)> = pair(args).into_iter().collect();
    if let Some(edits) = args["edits"].as_array() {
        pairs.extend(edits.iter().filter_map(pair));
    }
    if !pairs.is_empty() {
        let (mut added, mut removed, mut lines) = (0, 0, Vec::new());
        for (old, new) in &pairs {
            let (a, r, l) = line_diff(old, new);
            added += a;
            removed += r;
            if !lines.is_empty() {
                lines.push(" …".to_owned());
            }
            lines.extend(l);
        }
        lines.truncate(DIFF_LINES);
        return Some((added, removed, lines.join("\n")));
    }
    // A patch (`*** Begin Patch`, a unified diff): its + and - lines.
    let patch = args.as_object()?.values().filter_map(Value::as_str).find(|s| {
        s.contains("*** Begin Patch") || s.contains("\n@@") || s.starts_with("@@") || s.lines().any(|l| l.starts_with('+') || l.starts_with('-'))
    })?;
    let (mut added, mut removed, mut lines) = (0, 0, Vec::new());
    for l in patch.lines() {
        if l.starts_with("+++") || l.starts_with("---") || l.starts_with("***") {
            continue;
        }
        if l.starts_with('+') {
            added += 1;
        } else if l.starts_with('-') {
            removed += 1;
        } else if !l.starts_with(' ') {
            continue;
        }
        if lines.len() < DIFF_LINES {
            lines.push(l.to_owned());
        }
    }
    (added + removed > 0).then(|| (added, removed, lines.join("\n")))
}

/// What changed between two texts, line by line: the lines in between a
/// common start and end (a few kept around them as context).
fn line_diff(old: &str, new: &str) -> (usize, usize, Vec<String>) {
    let (o, n): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let start = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
    let end = o[start..].iter().rev().zip(n[start..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (gone, came) = (&o[start..o.len() - end], &n[start..n.len() - end]);
    let mut lines: Vec<String> = o[start.saturating_sub(2)..start].iter().map(|l| format!(" {l}")).collect();
    lines.extend(gone.iter().map(|l| format!("-{l}")));
    lines.extend(came.iter().map(|l| format!("+{l}")));
    lines.extend(o[o.len() - end..].iter().take(2).map(|l| format!(" {l}")));
    (came.len(), gone.len(), lines)
}

/// A command's outcome: tests passed or failed (cargo, pytest, jest, go…),
/// its exit code when it failed.
fn command_result(output: &str, is_error: bool) -> String {
    let lower = output.to_lowercase();
    let (passed, failed) = (count_before(&lower, "passed"), count_before(&lower, "failed"));
    if failed > 0 {
        return format!("{failed} failed");
    }
    if passed > 0 {
        return format!("{passed} passed");
    }
    for marker in ["exit code ", "exited with code ", "exit status ", "exit code: "] {
        if let Some(i) = lower.rfind(marker) {
            let code: String = lower[i + marker.len()..].chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
            if !code.is_empty() && code != "0" {
                return format!("exit {code}");
            }
        }
    }
    if is_error { "failed".into() } else { String::new() }
}

/// The largest number written just before `word` ("48 passed"), summed over
/// a run's lines (cargo prints one result per test binary).
fn count_before(text: &str, word: &str) -> usize {
    let mut total = 0;
    let mut rest = text;
    while let Some(i) = rest.find(word) {
        let before = rest[..i].trim_end();
        let digits: String = before.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
        if let Ok(n) = digits.parse::<usize>() {
            total += n;
        }
        rest = &rest[i + word.len()..];
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_question_shows_what_was_answered() {
        let none = json!({});
        assert_eq!(facts("ask", &none, "The user answered:\n- Which colour?\n  Red\n- Which parts?\n  (in their words) only the api", false).0, "Red; only the api");
        assert_eq!(facts("AskUserQuestion", &none, "User has answered your questions: \"Which colour?\"=\"Blue\", \"Size?\"=\"Large\". You can now continue with the user's answers in mind.", false).0, "Blue; Large");
        assert_eq!(facts("ask", &none, "The user dismissed the questions without answering.", false).0, "dismissed");
        assert_eq!(facts("AskUserQuestion", &none, "Your questions have been answered: \"Which database?\"=\"SQLite\".", false).0, "SQLite");
        let long = format!("The user answered:\n- Q\n  {}", "word ".repeat(20));
        assert!(facts("ask", &none, &long, false).0.ends_with('…'));
        // A big file's read: its whole length, from the note.
        let big = "[src/x.rs is 900 lines (30 KB), more than one read shows. These are lines 1-2: grep it.]\na\nb";
        assert_eq!(facts("read", &none, big, false).0, "900 lines");
    }

    #[test]
    fn edits_count_their_lines() {
        let claude = json!({ "file_path": "a.rs", "old_string": "a\nb\nc\nd", "new_string": "a\nB\nB2\nc\nd" });
        let (result, diff) = facts("Edit", &claude, "", false);
        assert_eq!(result, "+2 −1");
        assert_eq!(diff, " a\n-b\n+B\n+B2\n c\n d");
        let pi = json!({ "path": "a", "edits": [{ "oldText": "x", "newText": "y" }, { "oldText": "p\nq", "newText": "p" }] });
        assert_eq!(facts("edit", &pi, "", false).0, "+1 −2");
        let omp = json!({ "input": "*** Begin Patch\n[notes.md#EB6D]\nPUT 3.=3:\n+- a new line\n-old\n*** End Patch\n" });
        assert_eq!(facts("edit", &omp, "", false).0, "+1 −1");
        let harlo = json!({ "path": "a", "edits": [{ "old_text": "x\ny", "new_text": "x\nz" }] });
        assert_eq!(facts("edit", &harlo, "", false), ("+1 −1".into(), " x\n-y\n+z".into()));
        let patch = json!({ "input": "*** Begin Patch\n*** Update File: a.rs\n@@ fn main() {\n     let x = 1;\n-    old();\n+    new();\n+    more();\n*** End Patch" });
        assert_eq!(facts("apply_patch", &patch, "", false), ("+2 −1".into(), "     let x = 1;\n-    old();\n+    new();\n+    more();".into()));
        let write = json!({ "filePath": "n.txt", "content": "one\ntwo\n" });
        assert_eq!(facts("write", &write, "", false), ("+2".into(), "+one\n+two".into()));
    }

    #[test]
    fn commands_say_how_they_went() {
        let cargo = "test result: ok. 112 passed; 0 failed; 7 ignored\ntest result: ok. 29 passed; 0 failed";
        assert_eq!(facts("bash", &json!({}), cargo, false).0, "141 passed");
        assert_eq!(facts("bash", &json!({}), "== 3 failed, 10 passed in 0.2s ==", true).0, "3 failed");
        assert_eq!(facts("Bash", &json!({}), "oops\nCommand exited with code 2", true).0, "exit 2");
        assert_eq!(facts("bash", &json!({}), "fine", false).0, "");
        assert_eq!(facts("read", &json!({}), "a\nb\nc", false).0, "3 lines");
        assert_eq!(facts("grep", &json!({}), "x:1\ny:2\n", false).0, "2 results");
        assert_eq!(facts("mystery", &json!({}), "", true).0, "failed");
    }
}
