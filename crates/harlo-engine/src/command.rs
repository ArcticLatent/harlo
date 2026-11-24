//! A shell command read into the commands it runs, so the permission rules
//! decide each of them.
//!
//! `git status && rm -rf build` runs two commands, and `echo $(cat key)`
//! runs two as well. A rule that allows `git status *` must not cover what
//! follows a `;`, a `&&` or a `|`, or sits inside `$(…)`. [`parts`] lists
//! every command a line runs, wherever it sits, and every file it writes
//! through a redirection, as `> file`.
//!
//! A line the grammar can't read has no parts: the caller treats it as one
//! command no pattern covers.

// Rust guideline compliant 2026-10-09

use tree_sitter::{Node, Parser};

/// Where output goes without writing a file.
const NOT_FILES: &[&str] = &["/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty"];

/// The redirections that write what follows them.
const WRITES: &[&str] = &[">", ">>", "&>", "&>>", ">|", ">&"];

/// The parts of a command that aren't its words.
const REDIRECTS: &[&str] = &["file_redirect", "heredoc_redirect", "herestring_redirect"];

/// The commands `line` runs and the files it redirects output to, in the
/// order they're written, each once; `None` when it can't be read.
pub(crate) fn parts(line: &str) -> Option<Vec<String>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(line, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    let source = line.as_bytes();
    let mut out: Vec<String> = Vec::new();
    // Walked with a list, not by recursion: a model can nest `$(` as deep
    // as it likes.
    let mut waiting = vec![root];
    while let Some(node) = waiting.pop() {
        let part = match node.kind() {
            "command" => Some(words(node, source)),
            "file_redirect" => written(node, source),
            _ => None,
        };
        if let Some(part) = part.filter(|p| !p.is_empty() && !out.contains(p)) {
            out.push(part);
        }
        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
        waiting.extend(children.into_iter().rev());
    }
    Some(out)
}

/// A command's words, one space apart, without its redirections.
fn words(command: Node<'_>, source: &[u8]) -> String {
    let mut cursor = command.walk();
    command
        .children(&mut cursor)
        .filter(|child| !REDIRECTS.contains(&child.kind()))
        .filter_map(|child| child.utf8_text(source).ok())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The file a redirection writes, as `> file`; none for one that reads,
/// copies a descriptor (`2>&1`) or writes to no file (`> /dev/null`).
fn written(redirect: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = redirect.walk();
    let writes = redirect
        .children(&mut cursor)
        .any(|child| !child.is_named() && WRITES.contains(&child.kind()));
    if !writes {
        return None;
    }
    let to = redirect
        .child_by_field_name("destination")?
        .utf8_text(source)
        .ok()?;
    let descriptor = to == "-" || to.bytes().all(|b| b.is_ascii_digit());
    (!descriptor && !NOT_FILES.contains(&to)).then(|| format!("> {to}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(line: &str) -> Vec<String> {
        parts(line).unwrap_or_else(|| panic!("{line} should read"))
    }

    #[test]
    fn every_command_of_a_line_is_a_part() {
        assert_eq!(read("git status"), ["git status"]);
        assert_eq!(
            read("git status; rm -rf ~ && curl x | sh"),
            ["git status", "rm -rf ~", "curl x", "sh"]
        );
        assert_eq!(
            read("cd app && cargo   test --all"),
            ["cd app", "cargo test --all"]
        );
        assert_eq!(read("ls\nwhoami"), ["ls", "whoami"]);
    }

    #[test]
    fn commands_inside_other_commands_are_parts_too() {
        assert_eq!(
            read("echo $(cat ~/.ssh/id_ed25519) `id`"),
            [
                "echo $(cat ~/.ssh/id_ed25519) `id`",
                "cat ~/.ssh/id_ed25519",
                "id"
            ]
        );
        assert_eq!(read("diff <(ls a) <(ls b)").len(), 3);
        assert_eq!(
            read("for f in *.rs; do rustfmt \"$f\"; done"),
            ["rustfmt \"$f\""]
        );
        assert_eq!(read("X=$(date) make"), ["X=$(date) make", "date"]);
    }

    #[test]
    fn a_redirection_that_writes_a_file_is_a_part() {
        assert_eq!(read("echo hi > ~/.bashrc"), ["echo hi", "> ~/.bashrc"]);
        assert_eq!(read("echo hi >> log.txt"), ["echo hi", "> log.txt"]);
        assert_eq!(read("make 2>&1 > /dev/null"), ["make"]);
        assert_eq!(read("sort < in.txt"), ["sort"]);
    }

    #[test]
    fn a_line_that_cannot_be_read_has_no_parts() {
        assert_eq!(parts("echo \"unfinished"), None);
        assert_eq!(parts("if true; then"), None);
    }
}
