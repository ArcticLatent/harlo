//! Unified diffs as lines to show, with the changed words in each line marked.
//!
//! The server reports every change as a unified diff: an edit's result, an
//! approval's proposed change, a turn's changes, the working tree. [`lines`]
//! turns one into numbered rows without the file headers. Where a removed
//! line is followed by the line that replaced it, the words that differ are
//! marked as [`Span::changed`], so a one-word edit in a long line stands out.
//!
//! [`written`] makes a diff for a file written whole, which the server's write
//! tool reports only as its content.

// Rust guideline compliant 2026-10-09

use opencode_client::FileDiff;
use similar::{ChangeTag, TextDiff};

/// How alike a removed and an added line must be to mark their changed words.
///
/// From 0 (nothing shared) to 1 (the same). Below it, the line was rewritten
/// rather than edited, and marking nearly every word would only add noise.
const MIN_LIKENESS: f32 = 0.5;

/// What a line of a diff shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A line the change adds.
    Added,
    /// A line the change removes.
    Removed,
    /// An unchanged line around the change.
    Context,
    /// Unchanged lines left out between two parts of the change.
    Gap,
}

/// A piece of a line's text, and whether the change touched it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The text.
    pub text: String,
    /// Whether these words differ from the line it replaced or was replaced by.
    pub changed: bool,
}

/// One line of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// What it shows.
    pub kind: LineKind,
    /// Its line number: in the new file, or the old one for a removed line.
    pub number: Option<u32>,
    /// Its text in pieces, without the `+`, `-` or space in front.
    pub spans: Vec<Span>,
}

impl Line {
    /// Returns the line's whole text.
    #[must_use]
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn plain(kind: LineKind, number: Option<u32>, text: &str) -> Self {
        Self {
            kind,
            number,
            spans: vec![Span {
                text: text.to_owned(),
                changed: false,
            }],
        }
    }
}

/// Returns the lines of a unified diff worth showing, without its file headers.
///
/// Each hunk after the first starts with a [`LineKind::Gap`]. Removed lines
/// directly followed by added ones are paired in order, and the words that
/// differ between alike pairs are marked.
#[must_use]
pub fn lines(patch: &str) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let (mut old, mut new) = (0_u32, 0_u32);
    let mut hunks = 0_usize;
    for line in patch.lines() {
        if let Some(header) = line.strip_prefix("@@") {
            (old, new) = starts(header);
            hunks += 1;
            if hunks > 1 {
                out.push(Line::plain(LineKind::Gap, None, ""));
            }
        } else if hunks == 0 || line.starts_with('\\') {
            // File headers, and "\ No newline at end of file".
        } else if let Some(text) = line.strip_prefix('+') {
            out.push(Line::plain(LineKind::Added, Some(new), text));
            new += 1;
        } else if let Some(text) = line.strip_prefix('-') {
            out.push(Line::plain(LineKind::Removed, Some(old), text));
            old += 1;
        } else {
            let text = line.strip_prefix(' ').unwrap_or(line);
            out.push(Line::plain(LineKind::Context, Some(new), text));
            old += 1;
            new += 1;
        }
    }
    mark_words(&mut out);
    out
}

/// The first old and new line numbers of a hunk, from `-a,b +c,d @@`.
fn starts(header: &str) -> (u32, u32) {
    let number = |sign: char| {
        header
            .split_whitespace()
            .find_map(|part| part.strip_prefix(sign))
            .and_then(|range| range.split(',').next()?.parse().ok())
            .unwrap_or(1)
    };
    (number('-'), number('+'))
}

/// Marks the changed words in each removed line and the added line after it.
fn mark_words(lines: &mut [Line]) {
    let mut at = 0;
    while at < lines.len() {
        let removed = run(lines, at, LineKind::Removed);
        let added = run(lines, at + removed, LineKind::Added);
        if removed == 0 || added == 0 {
            // Past the whole run: stepping one line at a time would count
            // the rest of a long run again for each of its lines.
            at += (removed + added).max(1);
            continue;
        }
        for pair in 0..removed.min(added) {
            let (old_at, new_at) = (at + pair, at + removed + pair);
            if let Some((old, new)) = word_spans(&lines[old_at].text(), &lines[new_at].text()) {
                lines[old_at].spans = old;
                lines[new_at].spans = new;
            }
        }
        at += removed + added;
    }
}

/// How many lines of `kind` follow from `at`.
fn run(lines: &[Line], at: usize, kind: LineKind) -> usize {
    lines
        .get(at..)
        .map_or(0, |rest| rest.iter().take_while(|l| l.kind == kind).count())
}

/// Each side of an edited line in pieces, with the words that differ marked.
///
/// `None` when the lines are too unlike for that to help.
fn word_spans(old: &str, new: &str) -> Option<(Vec<Span>, Vec<Span>)> {
    let diff = TextDiff::from_words(old, new);
    if diff.ratio() < MIN_LIKENESS {
        return None;
    }
    let (mut before, mut after) = (Vec::new(), Vec::new());
    for change in diff.iter_all_changes() {
        let text = change.value();
        match change.tag() {
            ChangeTag::Equal => {
                push(&mut before, text, false);
                push(&mut after, text, false);
            }
            ChangeTag::Delete => push(&mut before, text, true),
            ChangeTag::Insert => push(&mut after, text, true),
        }
    }
    Some((before, after))
}

/// Adds text to the last span when it's marked the same, or starts a new one.
fn push(spans: &mut Vec<Span>, text: &str, changed: bool) {
    match spans.last_mut() {
        Some(last) if last.changed == changed => last.text.push_str(text),
        _ => spans.push(Span {
            text: text.to_owned(),
            changed,
        }),
    }
}

/// A diff for `content` written whole to `file`: every line added.
///
/// The write tool reports only what it wrote, so whatever the file held
/// before isn't shown.
#[must_use]
pub fn written(file: &str, content: &str) -> FileDiff {
    let count = content.lines().count();
    let mut patch = format!("--- {file}\n+++ {file}\n@@ -0,0 +1,{count} @@\n");
    for line in content.lines() {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    FileDiff {
        file: file.to_owned(),
        patch,
        additions: count as u64,
        deletions: 0,
        status: "added".to_owned(),
    }
}
