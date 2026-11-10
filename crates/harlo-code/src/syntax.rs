//! Colouring code a line at a time (harlo-core's syntaxes and token
//! classes), a slice of lines at a time so the window keeps answering.
//! After an edit only the lines whose colours change are done again: it
//! stops once a line ends in the state it ended in before.

use std::sync::Arc;
use std::time::Instant;

use syntect::highlighting::{HighlightIterator, HighlightState, Highlighter};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference};

use crate::text::{Line, Splice};

/// Lines longer than this aren't coloured (a minified file's one line).
const LONGEST: usize = 10_000;

/// Lines on screen past what's coloured in order: guessed from a fresh
/// state this many lines above them (right unless a comment or string
/// opened further up), until the pass in order gets there.
const GUESS_CONTEXT: usize = 24;

/// Lines on screen fewer than this past what's coloured are left to the
/// pass in order: it gets there within a frame or two.
const SOON: usize = 300;

#[derive(Clone, PartialEq, Eq)]
pub struct State {
    parse: ParseState,
    highlight: HighlightState,
}

pub struct Colouring {
    syntax: Option<&'static SyntaxReference>,
    highlighter: Highlighter<'static>,
    /// Lines before this are coloured, their states right.
    valid: usize,
    /// Lines from `converge` to `known` still have their old states (their
    /// text didn't change): once a line before them ends as it did, they're
    /// right as they are.
    converge: usize,
    known: usize,
    /// Lines past `valid` coloured by a guess, and the state after each
    /// (to guess on from as the view moves down, or after an edit).
    guessed: std::ops::Range<usize>,
    guess_states: Vec<State>,
}

impl Default for Colouring {
    fn default() -> Self {
        Self::new()
    }
}

impl Colouring {
    pub fn new() -> Self {
        Self {
            syntax: None,
            highlighter: Highlighter::new(harlo_core::markdown::code_theme()),
            valid: 0,
            converge: 0,
            known: 0,
            guessed: 0..0,
            guess_states: Vec::new(),
        }
    }

    /// The language for a file (by its name); none colours nothing.
    pub fn set_path(&mut self, path: &str, lines: &mut [Line]) {
        let syntax = harlo_core::markdown::language_of(path);
        if syntax.map(|s| &s.name) == self.syntax.map(|s| &s.name) {
            return;
        }
        self.syntax = syntax;
        self.restart(lines);
    }

    /// The language's name ("" for none).
    pub fn language(&self) -> &str {
        self.syntax.map_or("", |s| s.name.as_str())
    }

    /// Everything to colour again (a new text).
    pub fn restart(&mut self, lines: &mut [Line]) {
        for l in lines.iter_mut() {
            l.spans.clear();
            l.state = None;
        }
        self.valid = 0;
        self.converge = 0;
        self.known = 0;
        self.forget_guess();
    }

    fn forget_guess(&mut self) {
        self.guessed = 0..0;
        self.guess_states.clear();
    }

    /// The text changed (see `Text::replace`).
    pub fn spliced(&mut self, s: &Splice) {
        let old_end = s.first + s.removed;
        let new_end = s.first + s.added;
        let shift = |x: usize| {
            if x >= old_end {
                x + s.added - s.removed
            } else if x > s.first {
                new_end
            } else {
                x
            }
        };
        let known = self.known.max(self.valid);
        self.known = if known >= old_end { known + s.added - s.removed } else { new_end };
        self.converge = shift(self.converge).max(new_end);
        self.valid = self.valid.min(s.first);
        // A guess holds up to the edit.
        if s.first <= self.guessed.start {
            self.forget_guess();
        } else if s.first < self.guessed.end {
            self.guess_states.truncate(s.first - self.guessed.start);
            self.guessed.end = s.first;
        }
    }

    /// Whether lines are still to be coloured.
    pub fn busy(&self, lines: &[Line]) -> bool {
        self.syntax.is_some() && self.valid < lines.len()
    }

    /// Colours lines until `deadline`; the lines it did (to draw again).
    pub fn work(&mut self, lines: &mut [Line], deadline: Instant) -> Option<std::ops::Range<usize>> {
        self.work_upto(lines, deadline, usize::MAX)
    }

    /// As `work`, stopping before line `upto`.
    fn work_upto(&mut self, lines: &mut [Line], deadline: Instant, upto: usize) -> Option<std::ops::Range<usize>> {
        let syntax = self.syntax?;
        let syntaxes = harlo_core::markdown::code_syntaxes();
        let first = self.valid;
        let mut n = 0;
        while self.valid < lines.len().min(upto) {
            if n % 8 == 0 && n > 0 && Instant::now() >= deadline {
                break;
            }
            n += 1;
            let i = self.valid;
            let before = if i == 0 {
                Arc::new(State { parse: ParseState::new(syntax), highlight: HighlightState::new(&self.highlighter, ScopeStack::new()) })
            } else {
                lines[i - 1].state.clone().expect("the line before is coloured")
            };
            let mut state = (*before).clone();
            let spans = self.colour_line(&mut state, &lines[i].text, syntaxes);
            let state = if state == *before { before } else { Arc::new(state) };
            let converged = i >= self.converge && i < self.known && lines[i].state.as_deref() == Some(&*state);
            lines[i].spans = spans;
            lines[i].state = Some(state);
            self.valid = i + 1;
            if converged {
                self.valid = self.known;
            }
        }
        if self.valid >= self.guessed.end {
            self.forget_guess();
        }
        (self.valid > first).then_some(first..self.valid.min(lines.len()))
    }

    /// One line's colours, the state carried on past it.
    fn colour_line(&self, state: &mut State, text: &str, syntaxes: &syntect::parsing::SyntaxSet) -> Vec<(u32, u8)> {
        let mut spans = Vec::new();
        if text.len() > LONGEST {
            return spans;
        }
        let mut with_newline = String::with_capacity(text.len() + 1);
        with_newline.push_str(text);
        with_newline.push('\n');
        if let Ok(ops) = state.parse.parse_line(&with_newline, syntaxes) {
            let mut at = 0usize;
            for (style, piece) in HighlightIterator::new(&mut state.highlight, &ops, &with_newline, &self.highlighter) {
                let class = style.foreground.r;
                if at < text.len() && spans.last().is_none_or(|&(_, c)| c != class) {
                    spans.push((at as u32, class));
                }
                at += piece.len();
            }
        }
        spans
    }

    /// Colours the lines on screen (`shown`) now: in order when what's
    /// coloured is close above them, else a guess (see GUESS_CONTEXT).
    /// Whether any changed.
    pub fn show(&mut self, lines: &mut [Line], shown: std::ops::Range<usize>) -> bool {
        let Some(syntax) = self.syntax else { return false };
        let end = shown.end.min(lines.len());
        if end <= self.valid {
            return false;
        }
        if shown.start <= self.valid + GUESS_CONTEXT {
            // (Close: carry on in order, exactly.)
            return self.work_upto(lines, Instant::now() + std::time::Duration::from_secs(1), end).is_some();
        }
        if shown.start <= self.valid + SOON {
            return false;
        }
        let syntaxes = harlo_core::markdown::code_syntaxes();
        let covered = self.guessed.start <= shown.start && self.guessed.end >= end;
        if covered {
            return false;
        }
        // Down from where the guess got to, or a new guess.
        let (from, mut state) = match self.guess_states.last() {
            Some(st) if self.guessed.start <= shown.start && self.guessed.end >= shown.start => (self.guessed.end, st.clone()),
            _ => {
                let from = shown.start.saturating_sub(GUESS_CONTEXT).max(self.valid);
                self.guessed = from..from;
                self.guess_states.clear();
                (from, State { parse: ParseState::new(syntax), highlight: HighlightState::new(&self.highlighter, ScopeStack::new()) })
            }
        };
        for line in &mut lines[from..end] {
            line.spans = self.colour_line(&mut state, &line.text, syntaxes);
            self.guess_states.push(state.clone());
        }
        self.guessed.end = end;
        true
    }
}

/// The class of the character at `byte` in a line's spans.
pub fn class_at(spans: &[(u32, u8)], byte: usize) -> u8 {
    match spans.binary_search_by(|&(s, _)| (s as usize).cmp(&byte)) {
        Ok(i) => spans[i].1,
        Err(0) => 0,
        Err(i) => spans[i - 1].1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Text;
    use std::time::Duration;

    fn all(c: &mut Colouring, t: &mut Text) {
        while c.busy(&t.lines) {
            c.work(&mut t.lines, Instant::now() + Duration::from_secs(5));
        }
    }

    #[test]
    fn colours_and_recolours_after_edits() {
        let mut t = Text::new("fn main() {\n    let s = \"hi\";\n}\n// done\n");
        let mut c = Colouring::new();
        c.set_path("x.rs", &mut t.lines);
        assert_eq!(c.language(), "Rust");
        all(&mut c, &mut t);
        assert!(t.lines[0].spans.iter().any(|&(_, k)| k == 2), "fn is a keyword: {:?}", t.lines[0].spans);
        assert_eq!(class_at(&t.lines[3].spans, 0), 1, "a comment");
        let let_class = class_at(&t.lines[1].spans, 4);
        assert_ne!(let_class, 1);
        // Opening a block comment colours the lines after it…
        let (_, s) = t.replace(0, 0, "/*");
        c.spliced(&s);
        all(&mut c, &mut t);
        assert_eq!(class_at(&t.lines[1].spans, 6), 1, "{:?}", t.lines[1].spans);
        // …and closing it straight away stops at the line after.
        let (_, s) = t.replace(2, 2, "*/");
        c.spliced(&s);
        let did = c.work(&mut t.lines, Instant::now() + Duration::from_secs(5)).unwrap();
        assert_eq!(class_at(&t.lines[1].spans, 4), let_class, "let is code again");
        assert!(did.end == t.lines.len(), "{did:?}");
    }

    #[test]
    fn guesses_lines_on_screen_far_down() {
        let src: String = (0..3000).map(|i| format!("let x{i} = \"s\"; // c\n")).collect();
        let mut t = Text::new(&src);
        let mut c = Colouring::new();
        c.set_path("x.rs", &mut t.lines);
        c.work(&mut t.lines, Instant::now() + Duration::from_micros(1));
        assert!(c.valid < 2000);
        assert!(c.show(&mut t.lines, 2500..2540));
        assert_eq!(class_at(&t.lines[2520].spans, 18), 1, "a comment, guessed: {:?}", t.lines[2520].spans);
        assert!(!c.show(&mut t.lines, 2500..2540), "already guessed");
        assert!(c.show(&mut t.lines, 2510..2550), "a little further down: guessed on");
        let at = t.line_start(2530);
        let (_, sp) = t.replace(at, at, "/*");
        c.spliced(&sp);
        assert_eq!(c.guessed, 2476..2530, "the guess holds above the edit");
        assert!(c.show(&mut t.lines, 2510..2550));
        assert_eq!(class_at(&t.lines[2535].spans, 0), 1, "an opened comment, guessed on from the edit");
        let (_, sp) = t.replace(at, at + 2, "");
        c.spliced(&sp);
        // The pass in order gives the same colours.
        let guessed = t.lines[2520].spans.clone();
        all(&mut c, &mut t);
        assert_eq!(t.lines[2520].spans, guessed);
    }

    #[test]
    fn stops_once_the_colours_settle() {
        let src: String = (0..2000).map(|i| format!("let x{i} = {i};\n")).collect();
        let mut t = Text::new(&src);
        let mut c = Colouring::new();
        c.set_path("x.rs", &mut t.lines);
        all(&mut c, &mut t);
        let at = t.line_start(1000);
        let (_, s) = t.replace(at, at, "y");
        c.spliced(&s);
        let t0 = Instant::now();
        let did = c.work(&mut t.lines, Instant::now() + Duration::from_secs(5)).unwrap();
        assert_eq!(did.start, 1000);
        assert!(!c.busy(&t.lines), "all valid again after {:?}", t0.elapsed());
    }
}
