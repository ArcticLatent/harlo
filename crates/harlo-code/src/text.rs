//! The text: its lines, and positions in it as QML counts them (UTF-16 code
//! units, a line break one).

use std::cell::RefCell;
use std::sync::Arc;

use crate::syntax::State;

pub struct Line {
    pub text: String,
    /// Its length in UTF-16 code units.
    pub units: u32,
    /// Laid out (shaped) for drawing; dropped when it changes.
    pub(crate) layout: Option<Box<cosmic_text::BufferLine>>,
    /// Colours: (byte where a run starts, token class), in order.
    pub(crate) spans: Vec<(u32, u8)>,
    /// The highlighter's state after this line (shared with the line
    /// before when they're the same).
    pub(crate) state: Option<Arc<State>>,
}

impl Line {
    pub fn new(text: String) -> Self {
        let units = utf16_len(&text) as u32;
        Self { text, units, layout: None, spans: Vec::new(), state: None }
    }

    /// The byte at UTF-16 column `col` (clamped, and to a character).
    pub fn byte_at(&self, col: usize) -> usize {
        if self.units as usize == self.text.len() {
            return col.min(self.text.len());
        }
        let mut units = 0;
        for (i, c) in self.text.char_indices() {
            let next = units + c.len_utf16();
            if next > col {
                return i;
            }
            units = next;
        }
        self.text.len()
    }

    /// The UTF-16 column of byte `byte`.
    pub fn col_at(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        if self.units as usize == self.text.len() {
            return byte;
        }
        utf16_len(&self.text[..floor_char(&self.text, byte)])
    }
}

pub fn utf16_len(s: &str) -> usize {
    if s.is_ascii() { s.len() } else { s.chars().map(char::len_utf16).sum() }
}

/// `byte` moved back to the start of its character.
pub fn floor_char(s: &str, mut byte: usize) -> usize {
    byte = byte.min(s.len());
    while !s.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// What a replacement did to the lines: `first..first + removed` became
/// `first..first + added`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Splice {
    pub first: usize,
    pub removed: usize,
    pub added: usize,
}

pub struct Text {
    pub(crate) lines: Vec<Line>,
    /// Where each line starts (UTF-16), valid for the first `known` lines.
    starts: RefCell<(Vec<usize>, usize)>,
}

impl Default for Text {
    fn default() -> Self {
        Self::new("")
    }
}

impl Text {
    pub fn new(text: &str) -> Self {
        let lines = text.split('\n').map(|l| Line::new(l.to_owned())).collect();
        Self { lines, starts: RefCell::new((Vec::new(), 0)) }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", |l| l.text.as_str())
    }

    fn ensure_starts(&self, upto: usize) {
        let mut s = self.starts.borrow_mut();
        let (starts, known) = &mut *s;
        if *known > upto {
            return;
        }
        starts.resize(self.lines.len(), 0);
        let mut at = if *known == 0 { 0 } else { starts[*known - 1] + self.lines[*known - 1].units as usize + 1 };
        let end = (upto + 1).min(self.lines.len());
        for i in *known..end {
            starts[i] = at;
            at += self.lines[i].units as usize + 1;
        }
        *known = end;
    }

    fn forget_starts_from(&mut self, line: usize) {
        let s = self.starts.get_mut();
        s.1 = s.1.min(line);
    }

    /// Where line `i` starts.
    pub fn line_start(&self, i: usize) -> usize {
        if i >= self.lines.len() {
            return self.len();
        }
        self.ensure_starts(i);
        self.starts.borrow().0[i]
    }

    /// The whole text's length (UTF-16).
    pub fn len(&self) -> usize {
        let last = self.lines.len() - 1;
        self.line_start(last) + self.lines[last].units as usize
    }

    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].text.is_empty()
    }

    /// The line a position is on.
    pub fn line_of(&self, pos: usize) -> usize {
        let n = self.lines.len();
        self.ensure_starts(n - 1);
        let s = self.starts.borrow();
        match s.0[..n].binary_search(&pos) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    /// A position as (line, byte in it), clamped to the text.
    pub fn locate(&self, pos: usize) -> (usize, usize) {
        let line = self.line_of(pos);
        let col = pos - self.line_start(line);
        (line, self.lines[line].byte_at(col))
    }

    /// (line, byte) as a position.
    pub fn pos(&self, line: usize, byte: usize) -> usize {
        if line >= self.lines.len() {
            return self.len();
        }
        self.line_start(line) + self.lines[line].col_at(byte)
    }

    /// A position moved onto a character (not inside a surrogate pair),
    /// and into the text.
    pub fn clamp(&self, pos: usize) -> usize {
        let (l, b) = self.locate(pos.min(self.len()));
        self.pos(l, b)
    }

    pub fn slice(&self, a: usize, b: usize) -> String {
        let (a, b) = (a.min(b), a.max(b));
        let (l1, b1) = self.locate(a);
        let (l2, b2) = self.locate(b);
        if l1 == l2 {
            return self.lines[l1].text[b1..b2].to_owned();
        }
        let mut out = String::new();
        out.push_str(&self.lines[l1].text[b1..]);
        for l in &self.lines[l1 + 1..l2] {
            out.push('\n');
            out.push_str(&l.text);
        }
        out.push('\n');
        out.push_str(&self.lines[l2].text[..b2]);
        out
    }

    pub fn to_string(&self) -> String {
        let mut out = String::with_capacity(self.lines.iter().map(|l| l.text.len() + 1).sum());
        for (i, l) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&l.text);
        }
        out
    }

    /// Replaces `a..b` with `with`; returns what was there and how the
    /// lines changed.
    pub fn replace(&mut self, a: usize, b: usize, with: &str) -> (String, Splice) {
        let (a, b) = (a.min(b), a.max(b));
        let (l1, b1) = self.locate(a);
        let (l2, b2) = self.locate(b);
        let removed = self.slice(a, b);
        let head = &self.lines[l1].text[..b1];
        let tail = &self.lines[l2].text[b2..];
        let mut joined = String::with_capacity(head.len() + with.len() + tail.len());
        joined.push_str(head);
        joined.push_str(with);
        joined.push_str(tail);
        let new: Vec<Line> = joined.split('\n').map(|l| Line::new(l.to_owned())).collect();
        let added = new.len();
        self.lines.splice(l1..=l2, new);
        self.forget_starts_from(l1);
        (removed, Splice { first: l1, removed: l2 - l1 + 1, added })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16() {
        let t = Text::new("ab\n😀x\nend");
        assert_eq!(t.len(), 2 + 1 + 3 + 1 + 3);
        assert_eq!(t.line_start(1), 3);
        assert_eq!(t.line_start(2), 7);
        assert_eq!(t.locate(5), (1, 4)); // after the emoji
        assert_eq!(t.locate(4), (1, 0)); // inside the pair: its start
        assert_eq!(t.pos(1, 4), 5);
        assert_eq!(t.slice(3, 6), "😀x");
        assert_eq!(t.line_of(7), 2);
        assert_eq!(t.line_of(6), 1);
    }

    #[test]
    fn replace_splices_lines() {
        let mut t = Text::new("one\ntwo\nthree");
        let (removed, s) = t.replace(2, 9, "X\nY");
        assert_eq!(removed, "e\ntwo\nt");
        assert_eq!(t.to_string(), "onX\nYhree");
        assert_eq!(s, Splice { first: 0, removed: 3, added: 2 });
        assert_eq!(t.line_start(1), 4);
        t.replace(0, 0, "\n");
        assert_eq!(t.to_string(), "\nonX\nYhree");
        assert_eq!(t.line_count(), 3);
    }
}
