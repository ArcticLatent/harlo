//! A document being edited: its text, the selection, undo and redo, and
//! harlo's editing commands (indentation, brackets, comments). Positions are
//! UTF-16 code units, as QML counts them.

use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

use crate::lang;
use crate::text::{Splice, Text, utf16_len};

/// How an edit groups with the one before it in the undo history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A character typed: a word's worth undo together.
    Typing,
    /// Backspace or Delete: a run undoes together.
    Deleting,
    Other,
}

#[derive(Clone, Debug)]
struct Edit {
    at: usize,
    removed: String,
    inserted: String,
}

#[derive(Clone, Debug)]
struct Step {
    id: u64,
    edits: Vec<Edit>,
    before: (usize, usize),
    after: (usize, usize),
    kind: Kind,
    time: Instant,
}

/// Brackets and their partners.
fn partner_of(c: char) -> Option<char> {
    Some(match c {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        ')' => '(',
        ']' => '[',
        '}' => '{',
        _ => return None,
    })
}

/// How far a bracket's partner is looked for (characters).
const SCAN: usize = 200_000;

fn char_class(c: char) -> u8 {
    if c.is_alphanumeric() || c == '_' {
        2
    } else if c.is_whitespace() {
        0
    } else {
        1
    }
}

fn len16(s: &str) -> usize {
    utf16_len(s)
}

pub struct Doc {
    pub text: Text,
    pub anchor: usize,
    pub cursor: usize,
    undo: Vec<Step>,
    redo: Vec<Step>,
    next_id: u64,
    /// The undo step the file on disk matches (0: none, as loaded).
    saved: u64,
    path: String,
    /// One level of indentation.
    pub unit: String,
    comment: Vec<&'static str>,
    colon: bool,
    pub read_only: bool,
    splices: Vec<Splice>,
    /// Counts changes to the text.
    pub revision: u64,
}

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

impl Doc {
    pub fn new() -> Self {
        Self {
            text: Text::default(),
            anchor: 0,
            cursor: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            next_id: 1,
            saved: 0,
            path: String::new(),
            unit: "    ".into(),
            comment: Vec::new(),
            colon: false,
            read_only: false,
            splices: Vec::new(),
            revision: 0,
        }
    }

    /// A new text (a file opened): nothing to undo, not modified.
    pub fn load(&mut self, text: &str) {
        self.text = Text::new(text);
        self.anchor = 0;
        self.cursor = 0;
        self.undo.clear();
        self.redo.clear();
        self.saved = 0;
        self.unit = lang::detect_indent(&self.path, text);
        self.splices.clear();
        self.splices.push(Splice { first: 0, removed: usize::MAX, added: self.text.line_count() });
        self.revision += 1;
    }

    /// The file's name, for its comments and indentation rules.
    pub fn set_path(&mut self, path: &str) {
        self.path = path.to_owned();
        self.comment = lang::comment_for(path);
        self.colon = lang::colon_indents(path);
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// The text's changes since this was last asked (a `removed` of
    /// `usize::MAX`: a new text altogether).
    pub fn take_splices(&mut self) -> Vec<Splice> {
        std::mem::take(&mut self.splices)
    }

    fn top(&self) -> u64 {
        self.undo.last().map_or(0, |s| s.id)
    }

    /// It differs from the file on disk.
    pub fn modified(&self) -> bool {
        self.top() != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.top();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ---- the selection ------------------------------------------------------

    pub fn selection(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.cursor
    }

    pub fn selected_text(&self) -> String {
        let (a, b) = self.selection();
        self.text.slice(a, b)
    }

    /// Moves the cursor (`extend`: the selection with it).
    pub fn set_cursor(&mut self, pos: usize, extend: bool) {
        self.cursor = self.text.clamp(pos);
        if !extend {
            self.anchor = self.cursor;
        }
    }

    pub fn select(&mut self, anchor: usize, cursor: usize) {
        self.anchor = self.text.clamp(anchor);
        self.cursor = self.text.clamp(cursor);
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    /// The character at a position ('\n' at a line's end, None at the end).
    pub fn char_at(&self, pos: usize) -> Option<char> {
        let (l, b) = self.text.locate(pos);
        let line = self.text.line(l);
        if b < line.len() {
            line[b..].chars().next()
        } else if l + 1 < self.text.line_count() {
            Some('\n')
        } else {
            None
        }
    }

    pub fn char_before(&self, pos: usize) -> Option<char> {
        if pos == 0 {
            return None;
        }
        let (l, b) = self.text.locate(pos);
        if b == 0 {
            return Some('\n');
        }
        self.text.line(l)[..b].chars().next_back()
    }

    // ---- moving -----------------------------------------------------------------

    pub fn prev_grapheme(&self, pos: usize) -> usize {
        let (l, b) = self.text.locate(pos);
        if b == 0 {
            return pos.saturating_sub(1);
        }
        let line = self.text.line(l);
        let start = line[..b].grapheme_indices(true).next_back().map_or(0, |(i, _)| i);
        self.text.pos(l, start)
    }

    pub fn next_grapheme(&self, pos: usize) -> usize {
        let (l, b) = self.text.locate(pos);
        let line = self.text.line(l);
        if b >= line.len() {
            return if l + 1 < self.text.line_count() { self.text.line_start(l + 1) } else { self.text.len() };
        }
        let g = line[b..].graphemes(true).next().map_or(1, str::len);
        self.text.pos(l, b + g)
    }

    /// The end of the next word (Ctrl+Right).
    pub fn word_right(&self, pos: usize) -> usize {
        let (l, b) = self.text.locate(pos);
        let line = self.text.line(l);
        if b >= line.len() {
            return self.next_grapheme(pos);
        }
        let mut it = line[b..].char_indices().peekable();
        while it.next_if(|&(_, c)| c.is_whitespace()).is_some() {}
        let Some(&(_, c)) = it.peek() else { return self.text.pos(l, line.len()) };
        let k = char_class(c);
        while it.next_if(|&(_, c)| char_class(c) == k).is_some() {}
        let end = it.peek().map_or(line.len(), |&(i, _)| b + i);
        self.text.pos(l, end)
    }

    /// The start of the word before (Ctrl+Left).
    pub fn word_left(&self, pos: usize) -> usize {
        let (l, b) = self.text.locate(pos);
        if b == 0 {
            return self.prev_grapheme(pos);
        }
        let line = self.text.line(l);
        let mut it = line[..b].char_indices().rev().peekable();
        while it.next_if(|&(_, c)| c.is_whitespace()).is_some() {}
        let Some(&(_, c)) = it.peek() else { return self.text.pos(l, 0) };
        let k = char_class(c);
        let mut start = b;
        while let Some((i, _)) = it.next_if(|&(_, c)| char_class(c) == k) {
            start = i;
        }
        self.text.pos(l, start)
    }

    /// The word (or run of spaces or punctuation) around a position.
    pub fn word_at(&self, pos: usize) -> (usize, usize) {
        let (l, b) = self.text.locate(pos);
        let line = self.text.line(l);
        let c = line[b..].chars().next().or_else(|| line[..b].chars().next_back());
        let Some(c) = c else { return (pos, pos) };
        let k = char_class(c);
        let mut start = b;
        for (i, c) in line[..b].char_indices().rev() {
            if char_class(c) != k {
                break;
            }
            start = i;
        }
        let mut end = b;
        for (i, c) in line[b..].char_indices() {
            if char_class(c) != k {
                break;
            }
            end = b + i + c.len_utf8();
        }
        (self.text.pos(l, start), self.text.pos(l, end))
    }

    /// The line a position is on, with its line break.
    pub fn line_at(&self, pos: usize) -> (usize, usize) {
        let l = self.text.line_of(pos);
        let start = self.text.line_start(l);
        let end = if l + 1 < self.text.line_count() { self.text.line_start(l + 1) } else { self.text.len() };
        (start, end)
    }

    /// Home: the line's first non-space, or its start when already there.
    pub fn home(&self, pos: usize) -> usize {
        let l = self.text.line_of(pos);
        let line = self.text.line(l);
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        let first = self.text.pos(l, indent);
        if pos == first { self.text.line_start(l) } else { first }
    }

    pub fn line_end(&self, pos: usize) -> usize {
        let l = self.text.line_of(pos);
        self.text.line_start(l) + self.text.lines[l].units as usize
    }

    // ---- editing ------------------------------------------------------------------

    /// Edits in one undo step: `edits` are (start, end, text) in the text as
    /// it is now, not overlapping. The selection follows the edits, or is
    /// `after`.
    pub fn apply(&mut self, edits: &[(usize, usize, String)], kind: Kind, after: Option<(usize, usize)>) -> bool {
        if self.read_only {
            return false;
        }
        let len = self.text.len();
        let mut list: Vec<(usize, usize, &str)> =
            edits.iter().map(|(a, b, s)| ((*a).min(*b).min(len), (*a).max(*b).min(len), s.as_str())).collect();
        list.sort_by(|x, y| y.0.cmp(&x.0));
        let before = (self.anchor, self.cursor);
        let (mut anchor, mut cursor) = before;
        let mut done = Vec::new();
        for (a, b, s) in list {
            let (a, b) = (self.text.clamp(a), self.text.clamp(b));
            if a == b && s.is_empty() {
                continue;
            }
            let (removed, splice) = self.text.replace(a, b, s);
            if removed == s {
                // (Nothing changed after all.)
                self.splices.push(splice);
                continue;
            }
            self.splices.push(splice);
            let n = len16(s);
            let map = |p: usize| if p >= b { p - (b - a) + n } else if p > a { a + n } else { p };
            anchor = map(anchor);
            cursor = map(cursor);
            done.push(Edit { at: a, removed, inserted: s.to_owned() });
        }
        if done.is_empty() {
            return false;
        }
        self.revision += 1;
        (self.anchor, self.cursor) = after.map_or((anchor, cursor), |(a, c)| (self.text.clamp(a), self.text.clamp(c)));
        self.record(done, before, kind);
        true
    }

    fn record(&mut self, edits: Vec<Edit>, before: (usize, usize), kind: Kind) {
        self.redo.clear();
        let now = Instant::now();
        let after = (self.anchor, self.cursor);
        if let (Some(last), [e]) = (self.undo.last_mut(), edits.as_slice()) {
            let recent = now.duration_since(last.time) < Duration::from_secs(1) && last.id != self.saved && last.kind == kind;
            let joins = match kind {
                Kind::Typing => {
                    let prev = last.edits.last().and_then(|p| p.inserted.chars().next_back());
                    let word_ends = e.inserted.chars().all(char::is_whitespace) && prev.is_some_and(|c| !c.is_whitespace());
                    e.removed.is_empty() && e.at == last.after.1 && !e.inserted.contains('\n') && !word_ends
                }
                Kind::Deleting => e.inserted.is_empty() && (e.at + len16(&e.removed) == last.after.1 || e.at == last.after.1),
                Kind::Other => false,
            };
            if recent && joins {
                last.edits.extend(edits);
                last.after = after;
                last.time = now;
                return;
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        self.undo.push(Step { id, edits, before, after, kind, time: now });
    }

    /// The undo history's newest step stands alone (the next edit doesn't
    /// join it).
    pub fn break_group(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.kind = Kind::Other;
        }
    }

    pub fn undo(&mut self) -> bool {
        if self.read_only {
            return false;
        }
        let Some(step) = self.undo.pop() else { return false };
        for e in step.edits.iter().rev() {
            let (_, s) = self.text.replace(e.at, e.at + len16(&e.inserted), &e.removed);
            self.splices.push(s);
        }
        self.revision += 1;
        (self.anchor, self.cursor) = step.before;
        self.redo.push(step);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.read_only {
            return false;
        }
        let Some(mut step) = self.redo.pop() else { return false };
        for e in &step.edits {
            let (_, s) = self.text.replace(e.at, e.at + len16(&e.removed), &e.inserted);
            self.splices.push(s);
        }
        self.revision += 1;
        (self.anchor, self.cursor) = step.after;
        step.kind = Kind::Other;
        self.undo.push(step);
        true
    }

    /// Text typed (or pasted): in place of the selection.
    pub fn type_text(&mut self, s: &str) -> bool {
        let (a, b) = self.selection();
        let kind = if a == b && s.chars().count() == 1 { Kind::Typing } else { Kind::Other };
        let at = a + len16(s);
        self.apply(&[(a, b, s.to_owned())], kind, Some((at, at)))
    }

    /// Backspace: the selection, a level of indentation, or a character.
    pub fn backspace(&mut self) -> bool {
        let (a, b) = self.selection();
        if a != b {
            return self.apply(&[(a, b, String::new())], Kind::Other, Some((a, a)));
        }
        if self.outdent_back() {
            return true;
        }
        let p = self.prev_grapheme(a);
        p < a && self.apply(&[(p, a, String::new())], Kind::Deleting, Some((p, p)))
    }

    /// Delete: the selection or the character after the cursor.
    pub fn delete(&mut self) -> bool {
        let (a, b) = self.selection();
        if a != b {
            return self.apply(&[(a, b, String::new())], Kind::Other, Some((a, a)));
        }
        let n = self.next_grapheme(a);
        n > a && self.apply(&[(a, n, String::new())], Kind::Deleting, Some((a, a)))
    }

    /// Ctrl+Backspace / Ctrl+Delete: to the word's start or end.
    pub fn delete_word(&mut self, back: bool) -> bool {
        let (a, b) = self.selection();
        if a != b {
            return self.apply(&[(a, b, String::new())], Kind::Other, Some((a, a)));
        }
        let (from, to) = if back { (self.word_left(a), a) } else { (a, self.word_right(a)) };
        from < to && self.apply(&[(from, to, String::new())], Kind::Other, Some((from, from)))
    }

    // ---- harlo's commands --------------------------------------------------------

    /// Enter: the new line keeps this one's indentation, one level more after
    /// an opening bracket (or a colon in Python and YAML); between a pair of
    /// brackets the closing one gets a line of its own.
    pub fn newline(&mut self) -> bool {
        let (a, b) = self.selection();
        let (l, ba) = self.text.locate(a);
        let line = self.text.line(l);
        let before = &line[..ba];
        let indent: String = before.chars().take_while(|&c| c == ' ' || c == '\t').collect();
        let trimmed = before.trim_end();
        let last = trimmed.chars().next_back();
        let opens = matches!(last, Some('(' | '[' | '{'));
        let deeper = opens || (last == Some(':') && self.colon);
        // (No spaces left at the end of the line you leave.)
        let from = if before.trim().is_empty() { self.text.line_start(l) } else { a - (len16(before) - len16(trimmed)) };
        let mut ins = format!("\n{indent}{}", if deeper { self.unit.as_str() } else { "" });
        let caret = from + len16(&ins);
        if opens && self.char_at(b) == last.and_then(partner_of) {
            ins.push('\n');
            ins.push_str(&indent);
        }
        self.apply(&[(from, b, ins)], Kind::Other, Some((caret, caret)))
    }

    /// Tab: a level of indentation at the cursor, or on every line a
    /// selection spans.
    pub fn tab(&mut self) -> bool {
        let (a, b) = self.selection();
        if a != b && self.text.line_of(a) != self.text.line_of(b) {
            return self.shift_lines(true);
        }
        let ins = if self.unit == "\t" {
            "\t".to_owned()
        } else {
            let col = a - self.text.line_start(self.text.line_of(a));
            " ".repeat(self.unit.len() - col % self.unit.len())
        };
        let at = a + ins.len();
        self.apply(&[(a, b, ins)], Kind::Other, Some((at, at)))
    }

    /// The first and last line a selection (or the cursor) is on: a
    /// selection ending at a line's start leaves that line out.
    fn line_span(&self) -> (usize, usize, bool) {
        let (a, b) = self.selection();
        let had = a != b;
        let b = if had && self.char_before(b) == Some('\n') { b - 1 } else { b };
        (self.text.line_of(a), self.text.line_of(b), had)
    }

    fn lines_text(&self, first: usize, last: usize) -> (usize, usize, Vec<&str>) {
        let start = self.text.line_start(first);
        let end = self.text.line_start(last) + self.text.lines[last].units as usize;
        (start, end, (first..=last).map(|i| self.text.line(i)).collect())
    }

    /// Replaces whole lines, keeping the selection on them (or the cursor
    /// where it was in its line).
    fn replace_lines(&mut self, start: usize, end: usize, old: &[&str], new: Vec<String>, had: bool) -> bool {
        let joined = new.join("\n");
        let old_len: usize = old.iter().map(|l| len16(l)).sum::<usize>() + old.len() - 1;
        if old.iter().zip(&new).all(|(o, n)| o == n) {
            return false;
        }
        let new_len = len16(&joined);
        let after = if had {
            (start, start + new_len)
        } else {
            let c = (self.cursor as isize + new_len as isize - old_len as isize).max(start as isize) as usize;
            let c = c.min(start + new_len);
            (c, c)
        };
        self.apply(&[(start, end, joined)], Kind::Other, Some(after))
    }

    /// Indents (or outdents) the lines the cursor or selection is on.
    pub fn shift_lines(&mut self, indent: bool) -> bool {
        let (first, last, had) = self.line_span();
        let (start, end, old) = self.lines_text(first, last);
        let width = if self.unit == "\t" { 4 } else { self.unit.len() };
        let new: Vec<String> = old
            .iter()
            .map(|l| {
                if indent {
                    if l.trim().is_empty() { (*l).to_owned() } else { format!("{}{l}", self.unit) }
                } else if let Some(rest) = l.strip_prefix('\t') {
                    rest.to_owned()
                } else {
                    let n = l.bytes().take(width).take_while(|&b| b == b' ').count();
                    l[n..].to_owned()
                }
            })
            .collect();
        let old: Vec<String> = old.iter().map(|s| (*s).to_owned()).collect();
        let old: Vec<&str> = old.iter().map(String::as_str).collect();
        self.replace_lines(start, end, &old, new, had)
    }

    /// Backspace in the spaces before a line's text: back a level.
    fn outdent_back(&mut self) -> bool {
        if self.unit == "\t" || self.has_selection() {
            return false;
        }
        let a = self.cursor;
        let (l, b) = self.text.locate(a);
        let before = &self.text.line(l)[..b];
        if before.is_empty() || !before.trim().is_empty() || before.contains('\t') {
            return false;
        }
        let n = (before.len() - 1) % self.unit.len() + 1;
        if n < 2 {
            return false;
        }
        self.apply(&[(a - n, a, String::new())], Kind::Deleting, Some((a - n, a - n)))
    }

    /// A closing bracket typed on a line of only spaces: lined up with the
    /// line its partner opened. False when it isn't (type it as usual).
    pub fn closer(&mut self, ch: char) -> bool {
        if self.has_selection() {
            return false;
        }
        let Some(open) = partner_of(ch) else { return false };
        let a = self.cursor;
        let (l, b) = self.text.locate(a);
        let before = self.text.line(l)[..b].to_owned();
        if before.is_empty() || !before.trim().is_empty() {
            return false;
        }
        let mut depth = 0;
        let mut found = None;
        for (p, c) in self.scan_back(a).take(SCAN) {
            if c == ch {
                depth += 1;
            } else if c == open {
                if depth == 0 {
                    found = Some(p);
                    break;
                }
                depth -= 1;
            }
        }
        let indent = match found {
            Some(p) => {
                let pl = self.text.line_of(p);
                self.text.line(pl).chars().take_while(|&c| c == ' ' || c == '\t').collect::<String>()
            }
            None => before.strip_suffix(self.unit.as_str()).unwrap_or(&before).to_owned(),
        };
        if indent == before {
            return false;
        }
        let ls = self.text.line_start(l);
        let at = ls + len16(&indent) + 1;
        self.apply(&[(ls, a, format!("{indent}{ch}"))], Kind::Other, Some((at, at)))
    }

    /// Ctrl+/: comments the lines out, or back in when they all are.
    pub fn toggle_comment(&mut self) -> bool {
        let Some(&open) = self.comment.first() else { return false };
        let close = self.comment.get(1).copied().unwrap_or("");
        let (first, last, had) = self.line_span();
        let (start, end, old) = self.lines_text(first, last);
        let used: Vec<&str> = old.iter().copied().filter(|l| !l.trim().is_empty()).collect();
        if used.is_empty() {
            return false;
        }
        let commented = |l: &str| {
            let x = l.trim();
            x.starts_with(open) && (close.is_empty() || x.ends_with(close))
        };
        let remove = used.iter().all(|l| commented(l));
        let indent = used.iter().map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
        let new: Vec<String> = old
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    return (*l).to_owned();
                }
                if !remove {
                    let tail = if close.is_empty() { String::new() } else { format!(" {close}") };
                    return format!("{}{open} {}{tail}", &l[..indent], &l[indent..]);
                }
                let i = l.find(open).unwrap_or(0);
                let mut rest = &l[i + open.len()..];
                rest = rest.strip_prefix(' ').unwrap_or(rest);
                let rest = if close.is_empty() {
                    rest.to_owned()
                } else {
                    let j = rest.rfind(close).unwrap_or(rest.len());
                    let head = &rest[..j];
                    format!("{}{}", head.strip_suffix(' ').unwrap_or(head), &rest[(j + close.len()).min(rest.len())..])
                };
                format!("{}{rest}", &l[..i])
            })
            .collect();
        let old: Vec<String> = old.iter().map(|s| (*s).to_owned()).collect();
        let old: Vec<&str> = old.iter().map(String::as_str).collect();
        self.replace_lines(start, end, &old, new, had)
    }

    // ---- brackets, search --------------------------------------------------------

    /// Characters from `pos` on: (position, character), line breaks included.
    fn scan_forward(&self, pos: usize) -> impl Iterator<Item = (usize, char)> + '_ {
        let (l0, b0) = self.text.locate(pos);
        let n = self.text.line_count();
        (l0..n).flat_map(move |l| {
            let line = self.text.line(l);
            let from = if l == l0 { b0 } else { 0 };
            let mut at = self.text.line_start(l) + self.text.lines[l].col_at(from);
            let chars = line[from..].chars().map(move |c| {
                let p = at;
                at += c.len_utf16();
                (p, c)
            });
            let end = self.text.line_start(l) + self.text.lines[l].units as usize;
            chars.chain((l + 1 < n).then_some((end, '\n')))
        })
    }

    /// Characters before `pos`, nearest first.
    fn scan_back(&self, pos: usize) -> impl Iterator<Item = (usize, char)> + '_ {
        let (l0, b0) = self.text.locate(pos);
        (0..=l0).rev().flat_map(move |l| {
            let line = self.text.line(l);
            let to = if l == l0 { b0 } else { line.len() };
            let mut at = self.text.line_start(l) + self.text.lines[l].col_at(to);
            let chars = line[..to].chars().rev().map(move |c| {
                at -= c.len_utf16();
                (at, c)
            });
            let lead = (l != l0).then(|| (self.text.line_start(l) + self.text.lines[l].units as usize, '\n'));
            lead.into_iter().chain(chars)
        })
    }

    /// The bracket by the cursor and its partner.
    pub fn bracket_pair(&self) -> Option<(usize, usize)> {
        if self.has_selection() {
            return None;
        }
        let c = self.cursor;
        let at = self.char_at(c);
        let before = self.char_before(c);
        let is_open = |ch: Option<char>| matches!(ch, Some('(' | '[' | '{'));
        let is_close = |ch: Option<char>| matches!(ch, Some(')' | ']' | '}'));
        let p = if is_open(at) {
            c
        } else if is_close(before) {
            c - 1
        } else if is_close(at) {
            c
        } else if is_open(before) {
            c - 1
        } else {
            return None;
        };
        let ch = self.char_at(p)?;
        let other = partner_of(ch)?;
        let mut depth = 0;
        let mut check = |(i, x): (usize, char)| {
            if x == ch {
                depth += 1;
            } else if x == other {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            None
        };
        let q = if is_open(Some(ch)) {
            self.scan_forward(p).take(SCAN).find_map(&mut check)
        } else {
            let first = check((p, ch));
            first.or_else(|| self.scan_back(p).take(SCAN).find_map(&mut check))
        }?;
        Some((p, q))
    }

    /// Where `query` is (start positions, at most `limit`), not overlapping.
    pub fn find_all(&self, query: &str, match_case: bool, limit: usize) -> Vec<usize> {
        let mut out = Vec::new();
        if query.is_empty() || query.contains('\n') {
            return out;
        }
        let needle: Vec<char> = if match_case { query.chars().collect() } else { query.chars().flat_map(char::to_lowercase).collect() };
        let ascii = query.is_ascii();
        let lower_needle = query.to_ascii_lowercase();
        for (l, line) in self.text.lines.iter().enumerate() {
            if out.len() >= limit {
                break;
            }
            let start = self.text.line_start(l);
            if ascii && (match_case || line.text.is_ascii()) {
                let hay;
                let (h, n) = if match_case {
                    (line.text.as_str(), query)
                } else {
                    hay = line.text.to_ascii_lowercase();
                    (hay.as_str(), lower_needle.as_str())
                };
                let mut from = 0;
                while let Some(i) = h[from..].find(n) {
                    out.push(start + line.col_at(from + i));
                    from += i + n.len();
                    if out.len() >= limit {
                        break;
                    }
                }
                continue;
            }
            // (Not ASCII: character by character, lower-cased.)
            let chars: Vec<(usize, char)> = line.text.char_indices().collect();
            let fold = |c: char| if match_case { c } else { c.to_lowercase().next().unwrap_or(c) };
            let mut i = 0;
            while i + needle.len() <= chars.len() {
                if (0..needle.len()).all(|k| fold(chars[i + k].1) == needle[k]) {
                    out.push(start + line.col_at(chars[i].0));
                    i += needle.len();
                    if out.len() >= limit {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(path: &str, text: &str) -> Doc {
        let mut d = Doc::new();
        d.set_path(path);
        d.load(text);
        d
    }

    fn at(d: &mut Doc, needle: &str, offset: usize) {
        let p = d.text.to_string().find(needle).unwrap() + offset;
        d.set_cursor(p, false);
    }

    #[test]
    fn enter_indents() {
        let mut d = doc("a.rs", "fn main() {\n}\n");
        at(&mut d, "{", 1);
        d.newline();
        assert_eq!(d.text.to_string(), "fn main() {\n    \n}\n");
        assert_eq!(d.cursor, 16);
        // Between a pair: the closer on its own line.
        let mut d = doc("a.rs", "fn f() {}");
        at(&mut d, "}", 0);
        d.newline();
        assert_eq!(d.text.to_string(), "fn f() {\n    \n}");
        // Python: after a colon.
        let mut d = doc("a.py", "def f():");
        d.set_cursor(8, false);
        d.newline();
        assert_eq!(d.text.to_string(), "def f():\n    ");
        // Trailing spaces go.
        let mut d = doc("a.rs", "    x;   ");
        d.set_cursor(9, false);
        d.newline();
        assert_eq!(d.text.to_string(), "    x;\n    ");
    }

    #[test]
    fn closer_lines_up() {
        let mut d = doc("a.rs", "fn f() {\n    if x {\n        ");
        d.set_cursor(d.text.len(), false);
        assert!(d.closer('}'));
        assert_eq!(d.text.to_string(), "fn f() {\n    if x {\n    }");
    }

    #[test]
    fn backspace_by_a_level() {
        let mut d = doc("a.rs", "fn f() {\n    if y {\n        x");
        d.set_cursor(d.text.line_start(2) + 8, false);
        d.backspace();
        assert_eq!(d.text.line(2), "    x");
        d.backspace();
        assert_eq!(d.text.line(2), "x");
    }

    #[test]
    fn tab_and_shift_tab_lines_undo_in_one() {
        let src = "fn add() {\n    a\n}\nnext";
        let mut d = doc("a.rs", src);
        d.select(0, 19);
        d.tab();
        assert_eq!(d.text.to_string(), "    fn add() {\n        a\n    }\nnext");
        assert_eq!(d.selection(), (0, 30));
        d.undo();
        assert_eq!(d.text.to_string(), src);
        d.select(0, 19);
        d.shift_lines(false);
        assert_eq!(d.text.to_string(), "fn add() {\na\n}\nnext");
    }

    #[test]
    fn comments_toggle() {
        let src = "fn add() {\n    a\n}\n";
        let mut d = doc("a.rs", src);
        d.select(0, 19);
        d.toggle_comment();
        assert_eq!(d.text.to_string(), "// fn add() {\n//     a\n// }\n");
        d.toggle_comment();
        assert_eq!(d.text.to_string(), src);
        let mut d = doc("a.html", "  <b>x</b>");
        d.toggle_comment();
        assert_eq!(d.text.to_string(), "  <!-- <b>x</b> -->");
        d.toggle_comment();
        assert_eq!(d.text.to_string(), "  <b>x</b>");
    }

    #[test]
    fn typing_undoes_by_word_and_tracks_saved() {
        let mut d = doc("a.txt", "");
        for c in "hello world".chars() {
            d.type_text(&c.to_string());
        }
        assert!(d.modified());
        d.undo();
        assert_eq!(d.text.to_string(), "hello");
        d.undo();
        assert_eq!(d.text.to_string(), "");
        assert!(!d.modified());
        d.redo();
        d.mark_saved();
        assert!(!d.modified());
        d.type_text("!");
        assert!(d.modified());
        d.undo();
        assert!(!d.modified());
        assert_eq!(d.text.to_string(), "hello");
    }

    #[test]
    fn brackets_and_words() {
        let d = {
            let mut d = doc("a.rs", "f(a, [b])");
            d.set_cursor(1, false);
            d
        };
        assert_eq!(d.bracket_pair(), Some((1, 8)));
        let mut d = d;
        d.set_cursor(9, false);
        assert_eq!(d.bracket_pair(), Some((8, 1)));
        let d = doc("a.rs", "let foo_bar = 1;");
        assert_eq!(d.word_at(6), (4, 11));
        assert_eq!(d.word_right(0), 3);
        assert_eq!(d.word_left(11), 4);
    }

    #[test]
    fn finds_in_utf16() {
        let d = doc("a.txt", "Total 😀 total\nTOTAL");
        assert_eq!(d.find_all("total", false, 100), vec![0, 9, 15]);
        assert_eq!(d.find_all("total", true, 100), vec![9]);
        let d = doc("a.txt", "Ärger ärger");
        assert_eq!(d.find_all("ärger", false, 100), vec![0, 6]);
    }

    #[test]
    fn apply_is_one_step() {
        let mut d = doc("a.txt", "a total b total");
        d.apply(&[(2, 7, "sum".into()), (10, 15, "sum".into())], Kind::Other, None);
        assert_eq!(d.text.to_string(), "a sum b sum");
        d.undo();
        assert_eq!(d.text.to_string(), "a total b total");
        d.redo();
        assert_eq!(d.text.to_string(), "a sum b sum");
    }
}
