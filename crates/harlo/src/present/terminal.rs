//! The shown terminal's lines, cursor and colours.
//!
//! A terminal is drawn again only when its screen changed, another terminal
//! is shown, or the theme changed; then only the lines that differ are
//! replaced.

// Rust guideline compliant 2026-10-09

use std::rc::Rc;

use alacritty_terminal::vte::ansi::CursorShape;
use harlo_core::theme::Tokens;
use slint::{Model as _, ModelRc, VecModel};

use crate::app::App;
use crate::term::{Palette, Run, View};
use crate::theme::color;
use crate::ui::{State, TermCursor, TermLine, TermRun};

/// What the window shows of the terminals.
pub struct Terminal {
    lines: Rc<VecModel<TermLine>>,
    /// The runs each shown line was built from.
    shown: Vec<Vec<Run>>,
    /// The theme the palette was made from, and the palette.
    palette: Option<(Tokens, Palette)>,
    /// The terminal last drawn.
    drawn: Option<u32>,
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal")
            .field("drawn", &self.drawn)
            .finish_non_exhaustive()
    }
}

impl Terminal {
    /// Gives the window the terminal's lines.
    pub fn new(state: &State<'_>) -> Self {
        let lines = Rc::new(VecModel::default());
        state.set_term_lines(ModelRc::from(Rc::clone(&lines)));
        Self {
            lines,
            shown: Vec::new(),
            palette: None,
            drawn: None,
        }
    }

    /// Draws the shown terminal, when it changed.
    pub fn show(&mut self, app: &App, state: &State<'_>) {
        let Some(shell) = app.terminals.active().filter(|_| app.terminals_shown()) else {
            self.drawn = None;
            return;
        };
        let mut again = self.drawn != Some(shell.id);
        if self.palette.as_ref().is_none_or(|(t, _)| *t != app.tokens) {
            let palette = Palette::new(&app.tokens);
            state.set_term_background(color(palette.background));
            state.set_term_foreground(color(palette.foreground));
            state.set_term_cursor_color(color(palette.cursor));
            self.palette = Some((app.tokens, palette));
            again = true;
        }
        // Taking the mark first means a change made while drawing signals
        // again.
        let changed = shell.screen.take_changed();
        if !changed && !again {
            return;
        }
        self.drawn = Some(shell.id);
        let Some((_, palette)) = &self.palette else {
            return;
        };
        let view = shell.screen.view(palette);
        self.lines(&view, again);
        state.set_term_cursor(cursor(&view));
        state.set_term_scrolled(view.scrolled);
    }

    /// Replaces the lines that differ from `view`'s, or all of them.
    fn lines(&mut self, view: &View, all: bool) {
        if all {
            self.shown.clear();
        }
        for (index, runs) in view.lines.iter().enumerate() {
            if self.shown.get(index) == Some(runs) {
                continue;
            }
            let line = TermLine {
                runs: ModelRc::new(VecModel::from(runs.iter().map(run).collect::<Vec<_>>())),
            };
            if index < self.lines.row_count() {
                self.lines.set_row_data(index, line);
            } else {
                self.lines.push(line);
            }
            if index < self.shown.len() {
                self.shown[index].clone_from(runs);
            } else {
                self.shown.push(runs.clone());
            }
        }
        let count = view.lines.len();
        while self.lines.row_count() > count {
            self.lines.remove(self.lines.row_count() - 1);
        }
        self.shown.truncate(count);
    }
}

/// A run as the window draws it.
fn run(run: &Run) -> TermRun {
    TermRun {
        column: int(run.column),
        cells: int(run.cells),
        text: run.text.as_str().into(),
        color: color(run.style.fg),
        background: run.style.bg.map(color).unwrap_or_default(),
        filled: run.style.bg.is_some(),
        bold: run.style.bold,
        italic: run.style.italic,
        underline: run.style.underline,
        strike: run.style.strike,
    }
}

/// The cursor as the window draws it.
fn cursor(view: &View) -> TermCursor {
    let Some(at) = view.cursor else {
        return TermCursor::default();
    };
    TermCursor {
        shown: true,
        line: int(at.line),
        column: int(at.column),
        shape: match at.shape {
            CursorShape::Underline => 1,
            CursorShape::Beam => 2,
            CursorShape::HollowBlock => 3,
            CursorShape::Block | CursorShape::Hidden => 0,
        },
        text: at.text.to_string().into(),
    }
}

/// A count as the UI's `int`.
fn int(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}
