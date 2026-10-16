//! What harlo adds to a user message, and how to take it off again for
//! display: `<file name="…">…</file>` blocks (attached files, the way pi's
//! `@file` arguments look; see harlo-core's `attach`) and the "ask first"
//! note. Titles and transcripts show file names and a tag instead.

/// Splits `<file name="…">…</file>` blocks off a user message: the file
/// names, and the message with the blocks removed.
pub fn split_blocks(text: &str) -> (Vec<String>, String) {
    const OPEN: &str = "<file name=\"";
    let mut names = Vec::new();
    let mut rest = String::new();
    let mut s = text;
    while let Some(start) = s.find(OPEN) {
        let after = &s[start + OPEN.len()..];
        let Some(q) = after.find("\">") else { break };
        let body = &after[q + 2..];
        // Inlined text starts on its own line and ends with "\n</file>";
        // anything else is a one-line note.
        let end = if body.starts_with('\n') { body.find("\n</file>").map(|i| i + "\n</file>".len()) } else { body.find("</file>").map(|i| i + "</file>".len()) };
        let Some(end) = end else { break };
        rest.push_str(&s[..start]);
        names.push(after[..q].to_owned());
        s = &body[end..];
        s = s.strip_prefix('\n').unwrap_or(s);
    }
    rest.push_str(s);
    (names, rest.trim().to_owned())
}

/// Appended to a prompt by the composer's "Ask first" toggle (omp only):
/// the agent clarifies with its `ask` tool before starting.
pub const ASK_FIRST_NOTE: &str = "\n\nBefore you start, use the ask tool to ask me about anything ambiguous and any decisions I should make, instead of assuming.";

/// The message without the "ask first" note, and whether it had one.
pub fn strip_ask_first(text: &str) -> (&str, bool) {
    match text.trim_end().strip_suffix(ASK_FIRST_NOTE.trim_start()) {
        Some(rest) => (rest.trim_end(), true),
        None => (text, false),
    }
}

/// Appended to messages sent in plan mode (removed again for display).
pub const PLAN_NOTE: &str = "\n\n[Plan mode] Don't change any files yet. Explore what you need, then reply with a step-by-step plan for me to review. Wait until I switch to Build before you implement it.";

/// The message without the plan-mode note, and whether it had one.
pub fn strip_plan(text: &str) -> (&str, bool) {
    match text.trim_end().strip_suffix(PLAN_NOTE.trim_start()) {
        Some(rest) => (rest.trim_end(), true),
        None => (text, false),
    }
}

/// A short title for a user message: its first non-empty line without file
/// blocks, or the attached file names when there is no text.
pub fn title_text(text: &str) -> String {
    let (files, rest) = split_blocks(text);
    let line = rest.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    if !line.is_empty() || files.is_empty() {
        return line.to_owned();
    }
    files.iter().map(|f| f.rsplit('/').next().unwrap_or(f)).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_first_note_round_trips() {
        let sent = format!("Add a cache{ASK_FIRST_NOTE}");
        assert_eq!(strip_ask_first(&sent), ("Add a cache", true));
        assert_eq!(strip_ask_first("Add a cache"), ("Add a cache", false));
        assert_eq!(title_text(&sent), "Add a cache");
        // Plan first, then ask first (as sent).
        let both = format!("Add a cache{PLAN_NOTE}{ASK_FIRST_NOTE}");
        let (rest, ask) = strip_ask_first(&both);
        assert!(ask);
        assert_eq!(strip_plan(rest), ("Add a cache", true));
        assert_eq!(strip_plan("Add a cache"), ("Add a cache", false));
    }

    #[test]
    fn titles() {
        assert_eq!(title_text("<file name=\"/a/b.md\">\nx\n</file>\nFix it\nmore"), "Fix it");
        assert_eq!(title_text("<file name=\"/a/b.md\">\nx\n</file>\n<file name=\"/c.png\"></file>\n"), "b.md, c.png");
        assert_eq!(title_text("  \nplain"), "plain");
    }
}
