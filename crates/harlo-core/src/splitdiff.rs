//! Side-by-side diffs (the diff tab): a file's whole-file diff laid out as
//! two aligned columns, the old version on the left and the new one on the
//! right. Removed and added lines next to each other
//! pair up on one row (the part of the line that changed is marked); the
//! side with fewer lines gets filler rows, so the two stay level.

use crate::git::{FileDiff, LineKind};

/// A row's side is unchanged, changed (removed on the left, added on the
/// right), or filler (nothing there, the other side has a line).
pub const SAME: u8 = 0;
pub const CHANGED: u8 = 1;
pub const FILLER: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell {
    pub kind: u8,
    /// Line number in that version (0 for filler).
    pub no: u32,
    /// Where the line starts in the side's text, in UTF-16 units (what Qt
    /// counts positions in).
    pub offset: u32,
    /// The changed part of a paired line: UTF-16 columns `[from, to)`.
    pub inline: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Row {
    pub left: Cell,
    pub right: Cell,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Split {
    /// Each side's text, one line per row (filler rows are empty lines).
    pub left: String,
    pub right: String,
    pub rows: Vec<Row>,
    /// `(hunk, row)`: where each hunk of the usual diff (the one staging
    /// works with) starts changing, to put its buttons there.
    pub hunks: Vec<(usize, usize)>,
}

/// Columns `[from, to)` of a line, if any.
type Span = Option<(u32, u32)>;
/// Lines of a change block: text and line number.
type Lines<'a> = Vec<(&'a str, u32)>;

struct Builder {
    split: Split,
    left_len: u32,
    right_len: u32,
}

impl Builder {
    fn push(&mut self, left: Option<(&str, u32, u8)>, right: Option<(&str, u32, u8)>) {
        let first = self.split.rows.is_empty();
        let mut row = Row {
            left: side(&mut self.split.left, &mut self.left_len, first, left),
            right: side(&mut self.split.right, &mut self.right_len, first, right),
        };
        if let (Some(l), Some(r)) = (left, right)
            && l.2 == CHANGED
            && r.2 == CHANGED
        {
            (row.left.inline, row.right.inline) = inline(l.0, r.0);
        }
        self.split.rows.push(row);
    }

    /// Removed lines, then added ones: paired up row by row.
    fn flush(&mut self, removed: &mut Lines, added: &mut Lines) {
        for i in 0..removed.len().max(added.len()) {
            let l = removed.get(i).map(|&(t, n)| (t, n, CHANGED));
            let r = added.get(i).map(|&(t, n)| (t, n, CHANGED));
            self.push(l, r);
        }
        removed.clear();
        added.clear();
    }
}

/// One side of a row: its line added to the side's text (`len`: that
/// text's length in UTF-16 units).
fn side(text: &mut String, len: &mut u32, first: bool, cell: Option<(&str, u32, u8)>) -> Cell {
    if !first {
        text.push('\n');
        *len += 1;
    }
    let (t, no, kind) = cell.unwrap_or(("", 0, FILLER));
    let c = Cell { kind, no, offset: *len, inline: None };
    text.push_str(t);
    *len += t.encode_utf16().count() as u32;
    c
}

/// The changed middle of two paired lines (their common start and end
/// left out), in UTF-16 columns; None when they share nothing (the whole
/// line is marked by its colour already).
fn inline(old: &str, new: &str) -> (Span, Span) {
    let (a, b): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a.iter().rev().zip(b.iter().rev()).take(max_suffix).take_while(|(x, y)| x == y).count();
    if prefix + suffix == 0 {
        return (None, None);
    }
    let units = |s: &[char]| s.iter().map(|c| c.len_utf16() as u32).sum::<u32>();
    let range = |s: &[char]| {
        let (from, to) = (units(&s[..prefix]), units(&s[..s.len() - suffix]));
        (from < to).then_some((from, to))
    };
    (range(&a), range(&b))
}

/// Lays out `full` (the diff with the whole file as context) side by side;
/// `hunks` is the usual diff of the same file, whose hunks staging uses.
pub fn split(full: &FileDiff, hunks: &FileDiff) -> Split {
    let mut b = Builder { split: Split::default(), left_len: 0, right_len: 0 };
    let (mut removed, mut added): (Lines, Lines) = (Vec::new(), Vec::new());
    for line in full.hunks.iter().flat_map(|h| &h.lines) {
        match line.kind {
            LineKind::Context => {
                b.flush(&mut removed, &mut added);
                let (o, n) = (line.old_no.unwrap_or(0), line.new_no.unwrap_or(0));
                b.push(Some((&line.text, o, SAME)), Some((&line.text, n, SAME)));
            }
            LineKind::Removed => {
                // (A removal after additions starts a new block.)
                if !added.is_empty() {
                    b.flush(&mut removed, &mut added);
                }
                removed.push((&line.text, line.old_no.unwrap_or(0)));
            }
            LineKind::Added => added.push((&line.text, line.new_no.unwrap_or(0))),
            LineKind::NoNewline => {}
        }
    }
    b.flush(&mut removed, &mut added);
    let mut split = b.split;
    for (index, hunk) in hunks.hunks.iter().enumerate() {
        let Some(first) = hunk.lines.iter().find(|l| matches!(l.kind, LineKind::Added | LineKind::Removed)) else { continue };
        let row = split.rows.iter().position(|r| match first.kind {
            LineKind::Removed => r.left.kind == CHANGED && Some(r.left.no) == first.old_no,
            _ => r.right.kind == CHANGED && Some(r.right.no) == first.new_no,
        });
        if let Some(row) = row {
            split.hunks.push((index, row));
        }
    }
    split
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(text: &str) -> FileDiff {
        crate::git::parse_diff_for_tests("f.txt", text)
    }

    #[test]
    fn pairs_and_fills() {
        let full = diff("@@ -1,4 +1,4 @@\n a\n-b\n-c\n+B\n d\n+e\n");
        let s = split(&full, &full);
        let kinds: Vec<(u8, u8)> = s.rows.iter().map(|r| (r.left.kind, r.right.kind)).collect();
        assert_eq!(kinds, vec![(SAME, SAME), (CHANGED, CHANGED), (CHANGED, FILLER), (SAME, SAME), (FILLER, CHANGED)]);
        assert_eq!(s.left, "a\nb\nc\nd\n");
        assert_eq!(s.right, "a\nB\n\nd\ne");
        let nos: Vec<(u32, u32)> = s.rows.iter().map(|r| (r.left.no, r.right.no)).collect();
        assert_eq!(nos, vec![(1, 1), (2, 2), (3, 0), (4, 3), (0, 4)]);
        // Offsets: where each row's line starts.
        assert_eq!(s.rows.iter().map(|r| r.right.offset).collect::<Vec<_>>(), vec![0, 2, 4, 5, 7]);
        assert_eq!(s.hunks, vec![(0, 1)]);
    }

    #[test]
    fn marks_the_changed_part() {
        let full = diff("@@ -1 +1 @@\n-let x = 1;\n+let x = 42;\n");
        let r = split(&full, &full).rows[0];
        assert_eq!((r.left.inline, r.right.inline), (Some((8, 9)), Some((8, 10))));
        // Nothing in common: the line colour says it all.
        let r = split(&diff("@@ -1 +1 @@\n-abc\n+xyz\n"), &FileDiff::default()).rows[0];
        assert_eq!((r.left.inline, r.right.inline), (None, None));
        // An insertion: marked on the new side only. (UTF-16 columns.)
        let r = split(&diff("@@ -1 +1 @@\n-é()\n+é(a)\n"), &FileDiff::default()).rows[0];
        assert_eq!((r.left.inline, r.right.inline), (None, Some((2, 3))));
    }

    #[test]
    fn new_and_deleted_files() {
        let s = split(&diff("@@ -0,0 +1,2 @@\n+x\n+y\n"), &FileDiff::default());
        assert!(s.rows.iter().all(|r| r.left.kind == FILLER && r.right.kind == CHANGED));
        assert_eq!((s.left.as_str(), s.right.as_str()), ("\n", "x\ny"));
        let s = split(&diff("@@ -1 +0,0 @@\n-x\n\\ No newline at end of file\n"), &FileDiff::default());
        assert_eq!(s.rows.len(), 1);
        assert_eq!((s.rows[0].left.kind, s.rows[0].right.kind), (CHANGED, FILLER));
    }
}
