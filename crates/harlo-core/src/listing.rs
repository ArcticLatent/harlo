//! Tool output that is a file's lines, shown as code: each line's number in
//! a gutter and its text highlighted for the file's language, never wrapped.
//!
//! Two kinds are recognised:
//! - a command that prints part of a file (`sed -n 'A,Bp' f`, `cat f`,
//!   `head f`, `tail f`, `awk 'NR>=A' f`): numbered from where it starts
//!   (a tail's start isn't known, so it gets no numbers);
//! - output whose lines already carry their numbers (`cat -n`, `nl`,
//!   `grep -n` in one file, a read tool's `N→text`): those numbers, the text
//!   after them highlighted (for grep, a context line is marked).

use crate::markdown::highlight_lines;
use crate::mcp::split_command;

/// One line: its number ("" when not known), whether it's grep context
/// (not a match), and its text as highlighted rich text.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeLine {
    pub num: String,
    pub context: bool,
    pub html: String,
}

/// The lines as code, and the file they're from (its extension picks the
/// language).
#[derive(Debug, Clone, PartialEq)]
pub struct CodeView {
    pub path: String,
    pub lines: Vec<CodeLine>,
}

/// The output of a call (`tool`, its shell `command` if it's one, the
/// `path` it's about if it names one) as code, or None when it isn't a
/// file's lines (or there'd be nothing to add: no numbers, no language).
/// `limit`: the most lines returned (from the top).
pub fn view(tool: &str, command: &str, path: &str, output: &str, limit: usize) -> Option<CodeView> {
    // (Output kept to its end starts with "…": where it starts isn't known.)
    let (cut, output) = match output.strip_prefix("…\n") {
        Some(rest) => (true, rest),
        None => (false, output),
    };
    let mut lines: Vec<&str> = output.lines().collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return None;
    }
    let printed = if command.trim().is_empty() { None } else { file_print(command) };
    let numbered = numbered(&lines);
    // Where the file's language comes from: the numbered lines' own path
    // (grep in several files names each), else the command's file, else the
    // call's.
    let (file, nums, bodies, context): (String, Vec<String>, Vec<&str>, Vec<bool>) = match (numbered, &printed) {
        (Some(rows), _) => {
            let file = rows.path.clone().or_else(|| printed.as_ref().map(|p| p.path.clone())).or_else(|| command_file(command)).unwrap_or_else(|| path.to_owned());
            (file, rows.nums, rows.bodies, rows.context)
        }
        (None, Some(p)) => {
            let start = if cut { None } else { p.start };
            let nums = lines.iter().enumerate().map(|(i, _)| start.map(|s| (s + i).to_string()).unwrap_or_default()).collect();
            (p.path.clone(), nums, lines.clone(), vec![false; lines.len()])
        }
        // A read tool's plain lines: its file's language, no numbers.
        (None, None) if is_read(tool) && !path.is_empty() => (path.to_owned(), vec![String::new(); lines.len()], lines.clone(), vec![false; lines.len()]),
        _ => return None,
    };
    let known = has_language(&file);
    if !known && nums.iter().all(String::is_empty) {
        return None;
    }
    let take = bodies.len().min(limit.max(1));
    let html = if known { highlight_lines(&file, &bodies[..take]) } else { bodies[..take].iter().map(|b| crate::transcript::escape_html(b)).collect() };
    let lines = (0..take).map(|i| CodeLine { num: nums[i].clone(), context: context[i], html: html[i].clone() }).collect();
    Some(CodeView { path: file, lines })
}

/// How many lines `view` would give, unlimited (for "N more lines").
pub fn line_count(output: &str) -> usize {
    let output = output.strip_prefix("…\n").unwrap_or(output);
    let mut n = output.lines().count();
    for l in output.lines().rev() {
        if !l.trim().is_empty() {
            break;
        }
        n -= 1;
    }
    n
}

fn is_read(tool: &str) -> bool {
    matches!(tool.to_ascii_lowercase().as_str(), "read" | "view" | "read_file" | "readfile" | "cat")
}

/// A language the highlighter knows for this file.
fn has_language(path: &str) -> bool {
    crate::markdown::language_of(path).is_some()
}

/// A command that prints (part of) one file: the file, and the line it
/// starts at (None: not known, as for a tail).
#[derive(Debug, Clone, PartialEq)]
pub struct Printed {
    pub path: String,
    pub start: Option<usize>,
}

/// The file a command prints, and where from: its last step (after `cd …
/// &&`), allowing a `| head` after it (which keeps the start).
pub fn file_print(command: &str) -> Option<Printed> {
    let steps = split_steps(command);
    let last = steps.last()?;
    let mut pipe = last.split(|s| s.as_str() == "|");
    let first = pipe.next()?;
    // (After it, only a `head`: the lines still start where they did.)
    if !pipe.all(|p| p.first().is_some_and(|w| w == "head")) {
        return None;
    }
    let words = first.iter().flat_map(|w| split_command(w)).collect::<Vec<_>>();
    let (prog, args) = words.split_first()?;
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    match prog.as_str() {
        "cat" | "bat" | "batcat" => {
            // (`cat -n` numbers its lines: that's the numbered kind.)
            if args.iter().any(|a| a.starts_with('-')) || files.len() != 1 {
                return None;
            }
            Some(Printed { path: files[0].clone(), start: Some(1) })
        }
        "head" => {
            let file = last_file(args)?;
            Some(Printed { path: file, start: Some(1) })
        }
        "tail" => {
            let file = last_file(args)?;
            // (`tail -n +K`: from line K.)
            let from = args.windows(2).find(|w| w[0] == "-n").and_then(|w| w[1].strip_prefix('+')).or_else(|| args.iter().find_map(|a| a.strip_prefix("-n+").or_else(|| a.strip_prefix("--lines=+"))));
            Some(Printed { path: file, start: from.and_then(|n| n.parse().ok()) })
        }
        "sed" => {
            // sed -n 'A,Bp' file / sed -n 'Ap' file
            if !args.iter().any(|a| a == "-n") {
                return None;
            }
            let script = args.iter().find(|a| !a.starts_with('-'))?;
            let file = args.iter().filter(|a| !a.starts_with('-')).nth(1)?;
            let body = script.strip_suffix('p')?;
            let start: usize = body.split(',').next()?.trim().parse().ok()?;
            if !body.split(',').all(|n| n.trim().parse::<usize>().is_ok() || n.trim() == "$") {
                return None;
            }
            Some(Printed { path: file.clone(), start: Some(start) })
        }
        "awk" => {
            // awk 'NR>=A && NR<=B' file / awk 'NR==A,NR==B' file
            let script = args.iter().find(|a| !a.starts_with('-'))?;
            let file = args.iter().filter(|a| !a.starts_with('-')).nth(1)?;
            let rest = script.trim().strip_prefix("NR")?;
            let rest = rest.trim_start_matches(['>', '=']);
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            let start: usize = digits.parse().ok()?;
            if script.contains('{') {
                return None;
            }
            Some(Printed { path: file.clone(), start: Some(start) })
        }
        _ => None,
    }
}

/// The last argument that isn't an option or an option's number.
fn last_file(args: &[String]) -> Option<String> {
    let mut file = None;
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a == "-n" || a == "-c" || a == "--lines" || a == "--bytes" {
            skip = true;
        } else if !a.starts_with('-') {
            file = Some(a.clone());
        }
    }
    file
}

/// A file a command names (its last argument with an extension).
fn command_file(command: &str) -> Option<String> {
    let steps = split_steps(command);
    let last = steps.last()?;
    let first = last.split(|s| s.as_str() == "|").next()?;
    first.iter().flat_map(|w| split_command(w)).skip(1).filter(|a| !a.starts_with('-')).filter(|a| has_language(a)).last()
}

/// A command line's steps (split at unquoted `&&`, `||`, `;` and new
/// lines), each as its words with an unquoted `|` as a word of its own.
fn split_steps(command: &str) -> Vec<Vec<String>> {
    let mut steps = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = command.chars().peekable();
    let end_word = |cur: &mut String, words: &mut Vec<String>| {
        if !cur.is_empty() {
            words.push(std::mem::take(cur));
        }
    };
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => {
                cur.push(c);
                quote = None;
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                cur.push(c);
                quote = Some(c);
            }
            (None, '\\') => {
                cur.push(c);
                cur.extend(chars.next());
            }
            (None, '&') if chars.peek() == Some(&'&') => {
                chars.next();
                end_word(&mut cur, &mut words);
                steps.push(std::mem::take(&mut words));
            }
            (None, '|') if chars.peek() == Some(&'|') => {
                chars.next();
                end_word(&mut cur, &mut words);
                steps.push(std::mem::take(&mut words));
            }
            (None, ';' | '\n') => {
                end_word(&mut cur, &mut words);
                steps.push(std::mem::take(&mut words));
            }
            (None, '|') => {
                end_word(&mut cur, &mut words);
                words.push("|".into());
            }
            (None, c) if c.is_whitespace() => end_word(&mut cur, &mut words),
            (None, c) => cur.push(c),
        }
    }
    end_word(&mut cur, &mut words);
    steps.push(words);
    steps.retain(|s| !s.is_empty());
    steps
}

struct Numbered<'a> {
    path: Option<String>,
    nums: Vec<String>,
    bodies: Vec<&'a str>,
    context: Vec<bool>,
}

/// Lines that carry their numbers: grep -n (one file; its context lines
/// with `-`), cat -n and nl (a tab), a read tool's `N→`. None unless most
/// lines do (and at least two, so one stray "3: …" stays text).
fn numbered<'a>(lines: &[&'a str]) -> Option<Numbered<'a>> {
    let mut path: Option<String> = None;
    let mut several = false;
    let mut rows: Vec<Option<(String, &'a str, bool)>> = Vec::with_capacity(lines.len());
    for l in lines {
        rows.push(numbered_line(l).map(|(p, n, body, ctx)| {
            if let Some(p) = p {
                match &path {
                    Some(q) if *q != p => several = true,
                    None => path = Some(p),
                    _ => {}
                }
            }
            (n, body, ctx)
        }));
    }
    let hits = rows.iter().filter(|r| r.is_some()).count();
    let real = lines.iter().filter(|l| !l.trim().is_empty()).count();
    if several || hits < 2 || (hits as f64) < real as f64 * 0.6 {
        return None;
    }
    let mut out = Numbered { path, nums: Vec::new(), bodies: Vec::new(), context: Vec::new() };
    for (row, l) in rows.into_iter().zip(lines) {
        match row {
            Some((n, body, ctx)) => {
                out.nums.push(n);
                out.bodies.push(body);
                out.context.push(ctx);
            }
            // (grep's "--" between groups, and the like.)
            None => {
                out.nums.push(String::new());
                out.bodies.push(l);
                out.context.push(true);
            }
        }
    }
    Some(out)
}

/// One numbered line: its path (grep in a file), number, text, and whether
/// it's grep context.
fn numbered_line(l: &str) -> Option<(Option<String>, String, &str, bool)> {
    // cat -n / nl: spaces, the number, a tab. A read tool: the number, "→".
    let trimmed = l.trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &trimmed[digits..];
        if let Some(body) = rest.strip_prefix('\t').or_else(|| rest.strip_prefix('→')) {
            return Some((None, trimmed[..digits].to_owned(), body, false));
        }
    }
    // grep -n: [path:]N:text (a match) or [path-]N-text (context).
    let (path, rest) = match l.find([':', '-']) {
        Some(i) if !l[..i].chars().all(|c| c.is_ascii_digit()) => {
            let p = &l[..i];
            if p.contains(char::is_whitespace) || !(p.contains('/') || p.contains('.')) {
                return None;
            }
            (Some(p.to_owned()), &l[i + 1..])
        }
        _ => (None, l),
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let sep = rest[digits..].chars().next()?;
    if sep != ':' && sep != '-' {
        return None;
    }
    let body = &rest[digits + 1..];
    // (Not a date or a time: "2026-11-08 …", "12:30:45 …".)
    if body.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some((path, rest[..digits].to_owned(), body, sep == '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_that_print_a_file() {
        let p = |c: &str| file_print(c).map(|p| (p.path, p.start));
        assert_eq!(p("sed -n 3676,3684p Main.qml"), Some(("Main.qml".into(), Some(3676))));
        assert_eq!(p("cd crates/harlo-app/qml && sed -n '40,80p' Theme.qml"), Some(("Theme.qml".into(), Some(40))));
        assert_eq!(p("sed -n \"12p\" src/lib.rs"), Some(("src/lib.rs".into(), Some(12))));
        assert_eq!(p("cat src/main.rs"), Some(("src/main.rs".into(), Some(1))));
        assert_eq!(p("head -n 40 src/main.rs"), Some(("src/main.rs".into(), Some(1))));
        assert_eq!(p("tail -n 20 src/main.rs"), Some(("src/main.rs".into(), None)));
        assert_eq!(p("tail -n +30 src/main.rs"), Some(("src/main.rs".into(), Some(30))));
        assert_eq!(p("awk 'NR>=5 && NR<=9' a.py"), Some(("a.py".into(), Some(5))));
        assert_eq!(p("sed -n 10,90p big.rs | head -20"), Some(("big.rs".into(), Some(10))));
        // Not a plain print: a substitution, a filter, several files, numbered.
        assert_eq!(p("sed -i 's/a/b/' x.rs"), None);
        assert_eq!(p("sed -n '/fn main/,/^}/p' x.rs"), None);
        assert_eq!(p("cat a.rs | grep fn"), None);
        assert_eq!(p("cat a.rs b.rs"), None);
        assert_eq!(p("cat -n a.rs"), None);
        assert_eq!(p("cargo test"), None);
    }

    #[test]
    fn a_printed_file_is_numbered_from_its_start_and_highlighted() {
        let v = view("bash", "sed -n 3676,3678p src/main.rs", "", "    let a = 1;\n    let b = \"x\";\n}\n", 400).unwrap();
        assert_eq!(v.lines.iter().map(|l| l.num.as_str()).collect::<Vec<_>>(), ["3676", "3677", "3678"]);
        assert!(v.lines[0].html.starts_with("    "), "indentation kept: {}", v.lines[0].html);
        assert!(v.lines[1].html.contains("<span class="), "highlighted: {}", v.lines[1].html);
        // Kept to its end: where it starts isn't known.
        let v = view("bash", "cat src/main.rs", "", "…\nfn a() {}\nfn b() {}", 400).unwrap();
        assert!(v.lines.iter().all(|l| l.num.is_empty()));
        // A limit: the first lines.
        assert_eq!(view("bash", "cat a.rs", "", "a\nb\nc\nd", 2).unwrap().lines.len(), 2);
        assert_eq!(line_count("a\nb\nc\n\n"), 3);
    }

    #[test]
    fn numbered_output_keeps_its_numbers() {
        let grep = "3:    let limits = Limits::from_env();\n4:    serve(limits.default);\n5-}";
        let v = view("bash", "grep -n -C1 limit src/main.rs", "", grep, 400).unwrap();
        assert_eq!(v.path, "src/main.rs");
        assert_eq!(v.lines.iter().map(|l| (l.num.as_str(), l.context)).collect::<Vec<_>>(), [("3", false), ("4", false), ("5", true)]);
        // In one named file; several files stay as they were.
        let v = view("bash", "grep -rn fn src", "", "src/a.rs:1:fn a() {}\nsrc/a.rs:9:fn b() {}", 400).unwrap();
        assert_eq!((v.path.as_str(), v.lines[1].num.as_str()), ("src/a.rs", "9"));
        assert!(view("bash", "grep -rn fn src", "", "src/a.rs:1:fn a() {}\nsrc/b.rs:2:fn b() {}", 400).is_none());
        // cat -n, and a read tool's arrows.
        let v = view("bash", "cat -n x.py", "", "     1\timport os\n     2\tprint(1)", 400).unwrap();
        assert_eq!((v.path.as_str(), v.lines[1].num.as_str()), ("x.py", "2"));
        let v = view("Read", "", "/p/x.rs", "     7→fn a() {}\n     8→}", 400).unwrap();
        assert_eq!((v.path.as_str(), v.lines[0].num.as_str()), ("/p/x.rs", "7"));
        // Not code: a test run, dates, one stray number.
        assert!(view("bash", "cargo test", "", "test a ... ok\ntest b ... ok", 400).is_none());
        assert!(view("bash", "ls -l", "", "2026-11-08 a\n2026-11-09 b", 400).is_none());
    }

    #[test]
    fn languages_the_highlighter_lacks_borrow_a_near_one() {
        assert!(has_language("Main.qml") && has_language("a.ts") && has_language("a.tsx"));
        let v = view("bash", "sed -n 1,2p Main.qml", "", "import QtQuick\nItem { property string t: \"hi\"; width: 10 }", 400).unwrap();
        assert!(v.lines[1].html.contains("<span class="), "{}", v.lines[1].html);
    }
}
