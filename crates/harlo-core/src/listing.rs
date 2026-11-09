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
/// (not a match), and its text as highlighted rich text. A file's header
/// (grep in several files: its lines come under it) has `file` set and no
/// number.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeLine {
    pub num: String,
    pub context: bool,
    pub html: String,
    pub file: String,
}

/// The lines as code, the file they're from (its extension picks the
/// language; empty for several files) and how many lines there are in all
/// (`lines` stops at the limit).
#[derive(Debug, Clone, PartialEq)]
pub struct CodeView {
    pub path: String,
    pub lines: Vec<CodeLine>,
    pub total: usize,
    /// The output was cut after these lines (it has more than is kept).
    pub more: bool,
    /// The output was cut before these lines (kept to its end).
    pub earlier: bool,
}

/// A line before it's highlighted: the file whose language it's in, its
/// number, text, whether it's context, whether it's a file's header.
struct Pending<'a> {
    file: String,
    num: String,
    body: &'a str,
    context: bool,
    header: bool,
}

/// The listing's lines before they're highlighted, and its file (empty for
/// several), or None when the output isn't a file's lines.
fn rows<'a>(tool: &str, command: &str, path: &str, output: &'a str) -> Option<(String, Vec<Pending<'a>>)> {
    // (Output kept to its end starts with "…": where it starts isn't known.)
    let (cut, output) = match output.strip_prefix("…\n") {
        Some(rest) => (true, rest),
        None => (false, output),
    };
    // (Kept to its start, it ends "\n…".)
    let output = output.strip_suffix("\n…").unwrap_or(output);
    let mut lines: Vec<&str> = output.lines().collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return None;
    }
    let printed = if command.trim().is_empty() { None } else { file_print(command) };
    let plain = |file: &str, num: String, body| Pending { file: file.to_owned(), num, body, context: false, header: false };
    // (A read tool numbers every line: one is enough.)
    let (file, rows): (String, Vec<Pending>) = match (numbered(&lines, if is_read(tool) { 1 } else { 2 }), &printed) {
        (Some(rows), _) => {
            // Where the language comes from: each line's own path (grep
            // names it), else the command's file, else the call's.
            let fallback = printed.as_ref().map(|p| p.path.clone()).or_else(|| command_file(command)).unwrap_or_else(|| path.to_owned());
            let mut files: Vec<&str> = Vec::new();
            for r in rows.iter().flatten() {
                if let Some(p) = r.path.as_deref() {
                    if !files.contains(&p) {
                        files.push(p);
                    }
                }
            }
            let several = files.len() > 1;
            let file = if several { String::new() } else { files.first().map(|p| p.to_string()).unwrap_or(fallback.clone()) };
            let mut out = Vec::with_capacity(rows.len() + files.len());
            let mut current: Option<String> = None;
            for (row, line) in rows.into_iter().zip(&lines) {
                match row {
                    Some(r) => {
                        // (Several files: each one's lines under its name.)
                        if several && r.path.is_some() && r.path != current {
                            current = r.path.clone();
                            out.push(Pending { file: current.clone().unwrap_or_default(), num: String::new(), body: "", context: false, header: true });
                        }
                        let f = r.path.or_else(|| current.clone()).unwrap_or_else(|| file.clone());
                        out.push(Pending { file: f, num: r.num, body: r.body, context: r.context, header: false });
                    }
                    // (grep's "--" between groups: a gap, as context.)
                    None => out.push(Pending { file: current.clone().unwrap_or_else(|| file.clone()), num: String::new(), body: line, context: true, header: false }),
                }
            }
            (file, out)
        }
        (None, Some(p)) => {
            let start = if cut { None } else { p.start };
            (p.path.clone(), lines.iter().enumerate().map(|(i, l)| plain(&p.path, start.map(|s| (s + i).to_string()).unwrap_or_default(), *l)).collect())
        }
        // A read tool's plain lines: its file's language, no numbers.
        (None, None) if is_read(tool) && !path.is_empty() => (path.to_owned(), lines.iter().map(|l| plain(path, String::new(), *l)).collect()),
        _ => return None,
    };
    if !file.is_empty() && !has_language(&file) && rows.iter().all(|r| r.num.is_empty()) {
        return None;
    }
    Some((file, rows))
}

/// The listing as plain text, all of it (to copy): its lines without their
/// numbers, a file's name before its lines.
pub fn text(tool: &str, command: &str, path: &str, output: &str) -> Option<String> {
    let (_, rows) = rows(tool, command, path, output)?;
    Some(rows.iter().map(|r| if r.header { r.file.as_str() } else { r.body }).collect::<Vec<_>>().join("\n"))
}

/// The output of a call (`tool`, its shell `command` if it's one, the
/// `path` it's about if it names one) as code, or None when it isn't a
/// file's lines (or there'd be nothing to add: no numbers, no language).
/// `limit`: the most lines returned (from the top).
pub fn view(tool: &str, command: &str, path: &str, output: &str, limit: usize) -> Option<CodeView> {
    let (file, rows) = rows(tool, command, path, output)?;
    let total = rows.len();
    let take = total.min(limit.max(1));
    // Kept to its end (a log): its last lines, the end being what matters.
    let earlier = output.starts_with("…\n");
    let (from, to) = if earlier { (total - take, total) } else { (0, take) };
    // Highlighted a file at a time (each run of lines from one file).
    let mut out = Vec::with_capacity(take);
    let mut i = from;
    while i < to {
        if rows[i].header {
            out.push(CodeLine { num: String::new(), context: false, html: String::new(), file: rows[i].file.clone() });
            i += 1;
            continue;
        }
        let mut j = i;
        while j < to && !rows[j].header && rows[j].file == rows[i].file {
            j += 1;
        }
        let bodies: Vec<&str> = rows[i..j].iter().map(|r| r.body).collect();
        let html = if has_language(&rows[i].file) { highlight_lines(&rows[i].file, &bodies) } else { bodies.iter().map(|b| crate::transcript::escape_html(b)).collect() };
        for (r, h) in rows[i..j].iter().zip(html) {
            out.push(CodeLine { num: r.num.clone(), context: r.context, html: h, file: String::new() });
        }
        i = j;
    }
    Some(CodeView { path: file, lines: out, total, more: output.ends_with("\n…"), earlier })
}

/// An edit's diff (`+`, `-` and ` ` lines; ` …` between edits), numbered
/// as its file is now: each edit's new side (its ` ` and `+` lines) found
/// once in `file`, its lines numbered from there. A removed line, or an
/// edit whose new side isn't there exactly once, has none.
pub fn diff_numbers(diff: &[&str], file: &str) -> Vec<Option<usize>> {
    let mut out = vec![None; diff.len()];
    let mut start = 0;
    while start < diff.len() {
        let end = (start..diff.len()).find(|&i| diff[i] == " …").unwrap_or(diff.len());
        let new: Vec<usize> = (start..end).filter(|&i| !diff[i].starts_with('-')).collect();
        let text: String = new.iter().map(|&i| diff[i].get(1..).unwrap_or_default()).collect::<Vec<_>>().join("\n");
        if !text.trim().is_empty() {
            // (At a line's start, and only once: else it could be anywhere.)
            let mut at = file.match_indices(text.as_str()).map(|(k, _)| k).filter(|&k| k == 0 || file.as_bytes()[k - 1] == b'\n');
            if let Some(first) = at.next() {
                if at.next().is_none() {
                    let line = file[..first].matches('\n').count() + 1;
                    for (k, &i) in new.iter().enumerate() {
                        out[i] = Some(line + k);
                    }
                }
            }
        }
        start = end + 1;
    }
    out
}

/// Output whose start matters more than its end, kept from there when
/// it's long: a file's lines (a read, a command printing a file) and
/// numbered search results (`grep -n`, `rg -n`, `git grep -n`).
pub fn keeps_start(tool: &str, command: &str) -> bool {
    if is_read(tool) {
        return true;
    }
    if command.trim().is_empty() {
        return false;
    }
    file_print(command).is_some() || split_steps(command).iter().any(|words| numbered_search(words))
}

fn numbered_search(words: &[String]) -> bool {
    let Some((prog, rest)) = words.split_first() else { return false };
    let prog = prog.rsplit('/').next().unwrap_or(prog);
    let args: &[String] = match prog {
        "grep" | "egrep" | "fgrep" | "rg" => rest,
        "git" if rest.first().is_some_and(|w| w == "grep") => &rest[1..],
        _ => return false,
    };
    // (Up to a pipe: what's after it is another program's.)
    args.iter()
        .take_while(|a| a.as_str() != "|")
        .any(|a| a == "--line-number" || (a.starts_with('-') && !a.starts_with("--") && a[1..].chars().all(|c| c.is_ascii_alphanumeric()) && a.contains('n')))
}

/// A read tool's plain lines (Harlo's: no numbers, to save tokens),
/// numbered as a numbered read's are ("     7→text"): from `offset` (1
/// without one), or from the range its note gives. Its notes (a big file's
/// "[… These are lines 1-N …]" before them, "[Lines a-b of N …]" after)
/// are left out; hash-anchored lines ("7#AB:text") keep their own numbers,
/// and so do a big file's outline rows ("7: text", after "[… Its outline
/// follows …]"). None when it isn't a file's lines or is numbered already.
pub fn number_read(offset: Option<u64>, output: &str) -> Option<String> {
    let mut lines: Vec<&str> = output.lines().collect();
    let first = *lines.first()?;
    if numbered(&lines, 1).is_some() || first == "(empty file)" || first.starts_with("[Image: ") || (lines.len() == 1 && first.contains(" is a binary file (")) {
        return None;
    }
    let mut start = offset.unwrap_or(1).max(1) as usize;
    if first.starts_with('[') && first.ends_with(']') && first.contains("Its outline follows") {
        // Each row its own line's number; the notes after them left out.
        let rows: Vec<String> = lines[1..]
            .iter()
            .filter(|l| !(l.starts_with('[') && l.ends_with(']')))
            .map(|l| {
                let (n, body) = l.split_once(": ").or_else(|| l.strip_suffix(':').map(|n| (n, "")))?;
                Some(format!("{:>6}→{body}", n.parse::<usize>().ok()?))
            })
            .collect::<Option<_>>()?;
        return (!rows.is_empty()).then(|| rows.join("\n"));
    }
    if first.starts_with('[') && first.ends_with(']') && first.contains("These are lines ") {
        lines.remove(0);
        start = 1;
    }
    if let Some(last) = lines.last().copied()
        && let Some(range) = last.strip_prefix("[Lines ").filter(|_| last.ends_with(".]"))
        && let Some(a) = range.split('-').next().and_then(|a| a.parse::<usize>().ok())
    {
        lines.pop();
        if lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        start = a;
    }
    if lines.is_empty() {
        return None;
    }
    // Hash-anchored: "12#AB:text".
    fn anchored(l: &str) -> Option<(usize, &str)> {
        let (n, rest) = l.split_once('#')?;
        let n: usize = n.parse().ok()?;
        let body = rest.get(2..)?.strip_prefix(':')?;
        rest[..2].chars().all(|c| c.is_ascii_alphanumeric()).then_some((n, body))
    }
    let out: Vec<String> = if lines.iter().all(|l| anchored(l).is_some()) {
        lines.iter().filter_map(|l| anchored(l)).map(|(n, b)| format!("{n:>6}→{b}")).collect()
    } else {
        lines.iter().enumerate().map(|(i, l)| format!("{:>6}→{l}", start + i)).collect()
    };
    Some(out.join("\n"))
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

/// A tool that reads a file (its output is the file's lines).
pub fn is_read(tool: &str) -> bool {
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

/// A line that carries its number: grep's path (if it names one), the
/// number, the text after it, whether it's grep context.
struct NumberedLine<'a> {
    path: Option<String>,
    num: String,
    body: &'a str,
    context: bool,
}

/// Lines that carry their numbers: grep -n (its context lines with `-`, a
/// path before each when it searched several files), cat -n and nl (a
/// tab), a read tool's `N→`. None unless every line does but grep's `--`
/// between groups (so a command that printed something else too stays
/// text), and at least `least` (two from a command: one stray "3: …"
/// isn't a listing). Each line: its number, or None for a `--`.
fn numbered<'a>(lines: &[&'a str], least: usize) -> Option<Vec<Option<NumberedLine<'a>>>> {
    let rows: Vec<Option<NumberedLine<'a>>> = lines.iter().map(|l| numbered_line(l)).collect();
    let hits = rows.iter().filter(|r| r.is_some()).count();
    let real = lines.iter().filter(|l| !l.trim().is_empty() && l.trim() != "--").count();
    if hits < least || hits < real {
        return None;
    }
    Some(rows)
}

/// One numbered line: its path (grep in a file), number, text, and whether
/// it's grep context.
fn numbered_line(l: &str) -> Option<NumberedLine<'_>> {
    // cat -n / nl: spaces, the number, a tab. A read tool: the number, "→".
    let trimmed = l.trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &trimmed[digits..];
        if let Some(body) = rest.strip_prefix('\t').or_else(|| rest.strip_prefix('→')) {
            return Some(NumberedLine { path: None, num: trimmed[..digits].to_owned(), body, context: false });
        }
    }
    // grep -n: [path:]N:text (a match) or [path-]N-text (context). The
    // path may hold either sign itself ("harlo-core/src/a.rs"): the first
    // place where a sign, digits and the same sign follow.
    let at = |i: usize| -> Option<NumberedLine<'_>> {
        let sep = l[i..].chars().next()?;
        let rest = &l[i + sep.len_utf8()..];
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || !rest[digits..].starts_with(sep) {
            return None;
        }
        let body = &rest[digits + 1..];
        // (Not a date or a time: "2026-11-08 …", "12:30:45 …".)
        if body.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        let path = &l[..i];
        if path.is_empty() || path.contains(char::is_whitespace) || !(path.contains('/') || path.contains('.')) {
            return None;
        }
        Some(NumberedLine { path: Some(path.to_owned()), num: rest[..digits].to_owned(), body, context: sep == '-' })
    };
    if !l.starts_with(|c: char| c.is_ascii_digit()) {
        return l.char_indices().filter(|(_, c)| *c == ':' || *c == '-').find_map(|(i, _)| at(i));
    }
    // No path: N:text or N-text.
    let digits = l.chars().take_while(char::is_ascii_digit).count();
    let sep = l[digits..].chars().next()?;
    if sep != ':' && sep != '-' {
        return None;
    }
    let body = &l[digits + 1..];
    if body.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(NumberedLine { path: None, num: l[..digits].to_owned(), body, context: sep == '-' })
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
    fn output_numbered_only_in_part_stays_text() {
        // (The end of one file, then grep in another: not one listing.)
        let out = "        assert_eq!(a, b);\n    }\n}\n\n402:    fn f() {\n403-        g();\n404-    }";
        assert!(view("bash", "tail -3 a.rs; grep -n -A2 'fn f' b.rs", "", out, 400).is_none());
        let grep = "3:    let a = 1;\n4-    b();\n--\n9:    let c = 2;";
        assert_eq!(view("bash", "grep -n -A1 let a.rs", "", grep, 400).unwrap().lines.len(), 4);
    }

    #[test]
    fn numbered_output_keeps_its_numbers() {
        let grep = "3:    let limits = Limits::from_env();\n4:    serve(limits.default);\n5-}";
        let v = view("bash", "grep -n -C1 limit src/main.rs", "", grep, 400).unwrap();
        assert_eq!(v.path, "src/main.rs");
        assert_eq!(v.lines.iter().map(|l| (l.num.as_str(), l.context)).collect::<Vec<_>>(), [("3", false), ("4", false), ("5", true)]);
        // In one named file.
        let v = view("bash", "grep -rn fn src", "", "src/a.rs:1:fn a() {}\nsrc/a.rs:9:fn b() {}", 400).unwrap();
        assert_eq!((v.path.as_str(), v.lines[1].num.as_str(), v.lines[0].file.as_str()), ("src/a.rs", "9", ""));
        // In several: each file's lines under its name, in its language.
        let out = "crates/harlo-core/src/a.rs:1:fn a() {}\ncrates/harlo-core/src/a.rs-2-}\nqml/B.qml:4:Item { property string t: \"x\" }";
        let v = view("bash", "grep -rn -A1 fn crates qml", "", out, 400).unwrap();
        let shape: Vec<(&str, &str, bool)> = v.lines.iter().map(|l| (l.file.as_str(), l.num.as_str(), l.context)).collect();
        assert_eq!(shape, [("crates/harlo-core/src/a.rs", "", false), ("", "1", false), ("", "2", true), ("qml/B.qml", "", false), ("", "4", false)]);
        assert_eq!((v.path.as_str(), v.total), ("", 5));
        assert!(v.lines[1].html.contains("<span") && v.lines[4].html.contains("<span"), "{:?}", v.lines);
        // To copy: the lines, each file's name before its own.
        assert_eq!(text("bash", "grep -rn -A1 fn crates qml", "", out).unwrap(), "crates/harlo-core/src/a.rs\nfn a() {}\n}\nqml/B.qml\nItem { property string t: \"x\" }");
        // (The limit counts the headers too; the total is all of it.)
        let v = view("bash", "grep -rn fn crates qml", "", out, 2).unwrap();
        assert_eq!((v.lines.len(), v.total), (2, 5));
        // cat -n, and a read tool's arrows.
        let v = view("bash", "cat -n x.py", "", "     1\timport os\n     2\tprint(1)", 400).unwrap();
        assert_eq!((v.path.as_str(), v.lines[1].num.as_str()), ("x.py", "2"));
        let v = view("Read", "", "/p/x.rs", "     7→fn a() {}\n     8→}", 400).unwrap();
        assert_eq!((v.path.as_str(), v.lines[0].num.as_str()), ("/p/x.rs", "7"));
        let v = view("Read", "", "/p/README.md", "1\thello", 400).unwrap();
        assert_eq!((v.lines[0].num.as_str(), v.lines[0].html.as_str()), ("1", "hello"));
        // Not code: a test run, dates, one stray number.
        assert!(view("bash", "cargo test", "", "test a ... ok\ntest b ... ok", 400).is_none());
        assert!(view("bash", "ls -l", "", "2026-11-08 a\n2026-11-09 b", 400).is_none());
    }

    #[test]
    fn search_and_file_output_keep_their_start() {
        assert!(keeps_start("Read", ""));
        assert!(keeps_start("Bash", "grep -rn limits src/"));
        assert!(keeps_start("Bash", "cd app && rg -n --hidden foo"));
        assert!(keeps_start("Bash", "git grep -n Limits | head -50"));
        assert!(keeps_start("Bash", "sed -n '1,400p' src/main.rs"));
        assert!(!keeps_start("Bash", "grep -r limits src/"));
        assert!(!keeps_start("Bash", "cargo test -- --nocapture"));
        assert!(!keeps_start("Bash", "find . -name '*.rs'"));
        assert!(view("bash", "grep -rn x src/", "", "…\nsrc/a.rs:9:x\nsrc/a.rs:10:x", 400).unwrap().earlier);
    }

    #[test]
    fn a_log_kept_to_its_end_shows_its_last_lines() {
        let full: String = (1..=1200).map(|i| format!("{i:6}\tlog line {i}\n")).collect();
        let kept = crate::transcript::tail_chars(&full, 6000);
        assert!(kept.starts_with("…\n"));
        let v = view("Bash", "nl app.log", "", &kept, 6).unwrap();
        assert!(v.earlier && v.lines.len() == 6 && v.total > 6);
        assert_eq!(v.lines[5].num, "1200");
        assert_eq!(v.lines[0].num, "1195");
    }

    #[test]
    fn an_edit_is_numbered_where_its_new_lines_are() {
        let file = "fn main() {\n    let limits = Limits::from_env();\n    serve(limits.default);\n}\n";
        let diff = [" fn main() {", "-    let limit = 100;", "-    serve(limit);", "+    let limits = Limits::from_env();", "+    serve(limits.default);", " }"];
        assert_eq!(diff_numbers(&diff, file), vec![Some(1), None, None, Some(2), Some(3), Some(4)]);
        // Two edits: each found on its own; one that's there twice, or not at all, has none.
        let file = "a\nb\nx\nc\nx\n";
        let diff = ["-q", "+b", " …", "+x", " …", "+zz"];
        assert_eq!(diff_numbers(&diff, file), vec![None, Some(2), None, None, None, None]);
        // Not inside another line.
        assert_eq!(diff_numbers(&["+b"], "ab\nb\n"), vec![Some(2)]);
    }

    #[test]
    fn harlos_plain_reads_are_numbered() {
        assert_eq!(number_read(None, "a\nb").as_deref(), Some("     1→a\n     2→b"));
        assert_eq!(number_read(Some(40), "x\ny").as_deref(), Some("    40→x\n    41→y"));
        // Its notes: out, the range they give kept.
        let outline = "[src/x.rs is 9000 lines (300 KB), more than one read shows. Its outline follows (line: definition): read the parts you need with offset and limit (up to 2000 lines), or grep it.]\n12: impl Ledger {\n14:     pub fn post(&self) {\n9000: fn last() {}\n[40 nested definitions not shown: read a part to see them]";
        assert_eq!(number_read(None, outline).as_deref(), Some("    12→impl Ledger {\n    14→    pub fn post(&self) {\n  9000→fn last() {}"), "an outline: each row its own line");
        let big = "[src/x.rs is 900 lines (30 KB), more than one read shows. These are lines 1-2: grep it for what you need, or read a range with offset and limit (up to 400 lines).]\nfn a() {}\nfn b() {}";
        assert_eq!(number_read(None, big).as_deref(), Some("     1→fn a() {}\n     2→fn b() {}"));
        assert_eq!(number_read(Some(10), "p\nq\n\n[Lines 10-11 of 90. Read on with offset=12.]").as_deref(), Some("    10→p\n    11→q"));
        assert_eq!(number_read(None, "7#AB:x\n8#Q2:y").as_deref(), Some("     7→x\n     8→y"));
        // Numbered already, or not a file's lines.
        assert_eq!(number_read(None, "     1→a"), None);
        assert_eq!(number_read(None, "(empty file)"), None);
        assert_eq!(number_read(None, "/x/a.png is a binary file (12 bytes)."), None);
        // And as a listing: numbered, from its line.
        let v = view("read", "", "src/a.rs", &number_read(Some(5), "fn a() {}").unwrap(), 400).unwrap();
        assert_eq!(v.lines[0].num, "5");
    }

    #[test]
    fn a_listing_cut_after_its_start_says_so() {
        let full: String = (1..=1200).map(|i| format!("{i:6}→fn f{i}() {{}}\n")).collect();
        let kept = crate::transcript::head_chars(&full, 6000);
        assert!(kept.ends_with("\n…") && kept.starts_with("     1→"));
        let v = view("Read", "", "src/long.rs", &kept, 400).unwrap();
        assert!(v.more);
        assert_eq!(v.lines[0].num, "1");
        assert_eq!(v.lines.len(), v.total);
        let last: usize = v.lines.last().unwrap().num.parse().unwrap();
        assert!(last > 100 && last < 1200, "{last}");
        assert!(!view("Read", "", "src/a.rs", "     1→fn a() {}", 400).unwrap().more);
    }

    #[test]
    fn many_languages_are_highlighted() {
        for f in ["Main.qml", "a.ts", "a.tsx", "Cargo.toml", "b.kt", "Dockerfile", "flake.nix", "c.zig", "d.svelte", "e.scss", "CMakeLists.txt", "x.service", "y.jsonc", "z.fs"] {
            assert!(has_language(f), "{f}");
        }
        assert!(!has_language("notes.txt"));
        let v = view("bash", "sed -n 1,2p Main.qml", "", "import QtQuick\nItem { property string t: \"hi\"; width: 10 }", 400).unwrap();
        assert!(v.lines[1].html.contains("<span class="), "{}", v.lines[1].html);
    }
}
