//! A terminal: the screen `alacritty_terminal` keeps, and the shell behind
//! it.
//!
//! A shell on this computer runs in a PTY that alacritty's own thread reads,
//! updating the screen and signalling each change. A shell on the server has
//! its output fed in with [`Screen::feed`], and what the user types leaves
//! through the channel [`Screen::remote`] returns.
//!
//! [`Screen::view`] turns the visible part of the screen into lines of runs
//! for the window to draw: each run is text in one style, starting at a
//! column, so the window places it by the cell grid rather than by how wide
//! the font draws it.

// Rust guideline compliant 2026-10-09

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alacritty_terminal::event::{Event, EventListener, Notify as _, OnResize as _, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{self, Term, TermMode, viewport_to_point};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, Rgb};
use harlo_core::terminal::Input;
use harlo_core::theme::{Rgba, Tokens};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// What a terminal tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// The screen changed. Sent once until the screen is next drawn.
    Changed,
    /// The shell set its title; empty to go back to the folder's name.
    Title(String),
    /// The shell ended.
    Exited,
    /// The shell asked something the terminal answers with these bytes,
    /// such as where the cursor is.
    Reply(String),
}

/// Passes alacritty's events on to the app as [`Signal`]s.
#[derive(Clone)]
struct Listener {
    signals: UnboundedSender<Signal>,
    /// Whether a [`Signal::Changed`] went out that the window hasn't drawn.
    changed: Arc<AtomicBool>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let signal = match event {
            Event::Wakeup => {
                if self.changed.swap(true, Ordering::AcqRel) {
                    return;
                }
                Signal::Changed
            }
            Event::Title(title) => Signal::Title(title),
            Event::ResetTitle => Signal::Title(String::new()),
            Event::Exit | Event::ChildExit(_) => Signal::Exited,
            Event::PtyWrite(text) => Signal::Reply(text),
            _ => return,
        };
        // The app is gone only when the terminal closed.
        let _ = self.signals.send(signal);
    }
}

/// A terminal's size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Size {
    columns: usize,
    lines: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

impl Size {
    /// The size PTYs take; the cells' pixel size doesn't matter to shells.
    fn window(self) -> WindowSize {
        WindowSize {
            num_lines: u16::try_from(self.lines).unwrap_or(u16::MAX),
            num_cols: u16::try_from(self.columns).unwrap_or(u16::MAX),
            cell_width: 1,
            cell_height: 1,
        }
    }
}

/// The size a terminal starts at, until the window says how much room it
/// has.
const START: Size = Size {
    columns: 80,
    lines: 24,
};

/// Where the shell runs.
enum Link {
    /// Here, read by alacritty's thread.
    Local(Notifier),
    /// On the server: output comes in through [`Screen::feed`], input leaves
    /// on `input`.
    Remote {
        input: UnboundedSender<Input>,
        parser: Box<Processor>,
    },
}

/// A terminal's screen and the shell behind it.
pub struct Screen {
    term: Arc<FairMutex<Term<Listener>>>,
    link: Link,
    size: Size,
    changed: Arc<AtomicBool>,
}

impl std::fmt::Debug for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Screen")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        if let Link::Local(notifier) = &self.link {
            // Ending the reading thread closes the PTY, which hangs the shell
            // up. The thread is gone already if the shell ended.
            let _ = notifier.0.send(Msg::Shutdown);
        }
    }
}

/// A new screen whose events go to the returned channel.
fn screen() -> (
    Arc<FairMutex<Term<Listener>>>,
    Listener,
    UnboundedReceiver<Signal>,
) {
    let (signals, receiver) = unbounded_channel();
    let listener = Listener {
        signals,
        changed: Arc::new(AtomicBool::new(false)),
    };
    let term = Term::new(term::Config::default(), &START, listener.clone());
    (Arc::new(FairMutex::new(term)), listener, receiver)
}

impl Screen {
    /// Starts `program` in `folder` on this computer, `id` naming its
    /// window to it.
    ///
    /// # Errors
    ///
    /// Returns an error when the PTY can't be opened or the program can't
    /// start.
    pub fn local(
        program: &str,
        folder: &str,
        id: u32,
    ) -> std::io::Result<(Self, UnboundedReceiver<Signal>)> {
        let options = tty::Options {
            shell: Some(tty::Shell::new(program.to_owned(), Vec::new())),
            working_directory: Some(PathBuf::from(folder)),
            env: HashMap::from([
                ("TERM".to_owned(), "xterm-256color".to_owned()),
                ("COLORTERM".to_owned(), "truecolor".to_owned()),
            ]),
            ..tty::Options::default()
        };
        let pty = tty::new(&options, START.window(), u64::from(id))?;
        let (term, listener, signals) = screen();
        let changed = Arc::clone(&listener.changed);
        let reader = EventLoop::new(Arc::clone(&term), listener, pty, false, false)?;
        let notifier = Notifier(reader.channel());
        // The thread ends when the shell does or the screen is dropped.
        drop(reader.spawn());
        Ok((
            Self {
                term,
                link: Link::Local(notifier),
                size: START,
                changed,
            },
            signals,
        ))
    }

    /// A screen for a shell on the server: what it prints goes in with
    /// [`Screen::feed`], and what the user types comes out of the second
    /// channel.
    #[must_use]
    pub fn remote() -> (Self, UnboundedReceiver<Signal>, UnboundedReceiver<Input>) {
        let (term, listener, signals) = screen();
        let (input, typed) = unbounded_channel();
        let screen = Self {
            term,
            link: Link::Remote {
                input,
                parser: Box::new(Processor::new()),
            },
            size: START,
            changed: Arc::clone(&listener.changed),
        };
        (screen, signals, typed)
    }

    /// Shows what the shell on the server printed.
    pub fn feed(&mut self, bytes: &[u8]) {
        if let Link::Remote { parser, .. } = &mut self.link {
            parser.advance(&mut *self.term.lock(), bytes);
            self.touch();
        }
    }

    /// Sends `bytes` to the shell as typed.
    pub fn write(&mut self, bytes: impl Into<Cow<'static, [u8]>>) {
        match &self.link {
            Link::Local(notifier) => notifier.notify(bytes),
            Link::Remote { input, .. } => {
                let _ = input.send(Input::Write(bytes.into().into_owned()));
            }
        }
    }

    /// Sends what the user typed, back at the newest output.
    pub fn typed(&mut self, bytes: Vec<u8>) {
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Bottom);
        term.selection = None;
        drop(term);
        self.touch();
        self.write(bytes);
    }

    /// Marks the screen changed by the app itself, to be drawn again.
    fn touch(&self) {
        self.changed.store(true, Ordering::Release);
    }

    /// Whether the screen changed since this was last asked; asking marks
    /// it drawn, so its next change signals again.
    #[must_use]
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }

    /// Pastes `text`, marked as a paste when the program asked for that, so
    /// its lines don't run one by one.
    pub fn paste(&mut self, text: &str) {
        let bracketed = self.mode().contains(TermMode::BRACKETED_PASTE);
        let body = if bracketed {
            // A paste can't end the paste early.
            text.replace("\x1b[201~", "")
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r")
        };
        let bytes = if bracketed {
            format!("\x1b[200~{body}\x1b[201~")
        } else {
            body
        };
        self.typed(bytes.into_bytes());
    }

    /// Fits the screen to `columns` by `lines` cells, and tells the shell.
    pub fn resize(&mut self, columns: usize, lines: usize) {
        let size = Size {
            columns: columns.max(2),
            lines: lines.max(1),
        };
        if size == self.size {
            return;
        }
        self.size = size;
        self.term.lock().resize(size);
        self.touch();
        match &mut self.link {
            Link::Local(notifier) => notifier.on_resize(size.window()),
            Link::Remote { input, .. } => {
                let window = size.window();
                let _ = input.send(Input::Resize {
                    columns: window.num_cols,
                    lines: window.num_lines,
                });
            }
        }
    }

    /// Scrolls `lines` up (positive) or down. A full-screen program that
    /// asked for it gets arrow keys instead, as it has no history to scroll.
    pub fn scroll(&mut self, lines: i32) {
        let mode = self.mode();
        if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
            let key: &[u8] = match (lines > 0, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            let count = usize::try_from(lines.unsigned_abs()).unwrap_or(0);
            self.write(key.repeat(count));
            return;
        }
        self.term.lock().scroll_display(Scroll::Delta(lines));
        self.touch();
    }

    /// Starts selecting at `column` (fractional: where in the cell) of
    /// visible line `line`; `words` selects by word.
    pub fn select_from(&mut self, line: usize, column: f32, words: bool) {
        let mut term = self.term.lock();
        let (point, side) = place(&term, line, column);
        let kind = if words {
            SelectionType::Semantic
        } else {
            SelectionType::Simple
        };
        term.selection = Some(Selection::new(kind, point, side));
        drop(term);
        self.touch();
    }

    /// Moves the selection's end to `column` of visible line `line`.
    pub fn select_to(&mut self, line: usize, column: f32) {
        let mut term = self.term.lock();
        let (point, side) = place(&term, line, column);
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side);
        }
        drop(term);
        self.touch();
    }

    /// Drops the selection.
    pub fn unselect(&mut self) {
        self.term.lock().selection = None;
        self.touch();
    }

    /// The selected text, if anything is selected.
    #[must_use]
    pub fn selected(&self) -> Option<String> {
        self.term
            .lock()
            .selection_to_string()
            .filter(|s| !s.is_empty())
    }

    /// The modes the program set: cursor keys, paste, scrolling.
    #[must_use]
    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// The visible lines' text, without trailing spaces.
    #[must_use]
    pub fn text(&self) -> Vec<String> {
        let term = self.term.lock();
        let lines = term.screen_lines();
        let columns = term.columns();
        let offset = term.grid().display_offset();
        (0..lines)
            .map(|line| {
                let start = viewport_to_point(offset, Point::new(line, Column(0)));
                let end = Point::new(start.line, Column(columns.saturating_sub(1)));
                term.bounds_to_string(start, end).trim_end().to_owned()
            })
            .collect()
    }

    /// What the window draws: the visible lines as runs, and the cursor.
    #[must_use]
    pub fn view(&self, palette: &Palette) -> View {
        let term = self.term.lock();
        let content = term.renderable_content();
        let offset = content.display_offset;
        let mut lines: Vec<Vec<Run>> = vec![Vec::new(); term.screen_lines()];
        let mut builder = Builder::default();
        let mut row = None;
        for indexed in content.display_iter {
            let Some(at) = viewport(offset, indexed.point) else {
                continue;
            };
            if row != Some(at.0) {
                if let Some(done) = row {
                    builder.finish(&mut lines, done);
                }
                row = Some(at.0);
            }
            let cell = indexed.cell;
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let selected = content
                .selection
                .is_some_and(|range| range.contains(indexed.point));
            let style = Style::of(cell, content.colors, palette, selected);
            let mut text = String::from(cell.c);
            if let Some(extra) = cell.zerowidth() {
                text.extend(extra);
            }
            let cells = if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            builder.cell(at.1, cells, &text, style);
        }
        if let Some(done) = row {
            builder.finish(&mut lines, done);
        }
        let point = content.cursor.point;
        let cursor = viewport(offset, point)
            .filter(|_| content.cursor.shape != CursorShape::Hidden)
            .map(|(line, column)| Cursor {
                line,
                column,
                shape: content.cursor.shape,
                text: term.grid()[point].c,
            });
        View {
            lines,
            cursor,
            scrolled: offset > 0,
        }
    }
}

/// The grid point and side of the cell at `column` of visible `line`.
fn place(term: &Term<Listener>, line: usize, column: f32) -> (Point, Side) {
    let last = term.columns().saturating_sub(1);
    let line = line.min(term.screen_lines().saturating_sub(1));
    let whole = column.max(0.0).floor();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "floored and not negative, and clamped to the last column after"
    )]
    let cell = (whole as usize).min(last);
    let side = if column - whole < 0.5 {
        Side::Left
    } else {
        Side::Right
    };
    let point = viewport_to_point(term.grid().display_offset(), Point::new(line, Column(cell)));
    (point, side)
}

/// A grid point as a visible line and column, if it's on screen.
fn viewport(offset: usize, point: Point) -> Option<(usize, usize)> {
    let line = point.line.0 + i32::try_from(offset).ok()?;
    Some((usize::try_from(line).ok()?, point.column.0))
}

/// What the window draws for a terminal.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    /// The visible lines, top first, each as its runs.
    pub lines: Vec<Vec<Run>>,
    /// Where the cursor is, when it shows.
    pub cursor: Option<Cursor>,
    /// Whether the view is scrolled back from the newest output.
    pub scrolled: bool,
}

/// The cursor's place and shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Its visible line.
    pub line: usize,
    /// Its column.
    pub column: usize,
    /// How it's drawn.
    pub shape: CursorShape,
    /// The character under it.
    pub text: char,
}

/// Text in one style, from a column on.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// The column it starts at.
    pub column: usize,
    /// How many cells it covers.
    pub cells: usize,
    /// Its text.
    pub text: String,
    /// How it's drawn.
    pub style: Style,
}

/// How a cell is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the text attributes a terminal sets independently of each other"
)]
pub struct Style {
    /// The text colour.
    pub fg: Rgba,
    /// The background, when it isn't the terminal's own.
    pub bg: Option<Rgba>,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Struck through.
    pub strike: bool,
}

impl Style {
    /// The style of `cell`, with the program's colour changes over
    /// `palette`.
    fn of(
        cell: &term::cell::Cell,
        colors: &term::color::Colors,
        palette: &Palette,
        selected: bool,
    ) -> Self {
        let flags = cell.flags;
        let mut fg = palette.resolve(cell.fg, colors);
        let mut bg = palette.resolve(cell.bg, colors);
        let mut filled = !matches!(cell.bg, Color::Named(NamedColor::Background));
        if flags.contains(Flags::DIM) {
            fg.a *= DIM;
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
            fg.a = 1.0;
            filled = true;
        }
        if flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        if selected {
            bg = palette.selection;
            filled = true;
        }
        Self {
            fg,
            bg: filled.then_some(bg),
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            strike: flags.contains(Flags::STRIKEOUT),
        }
    }
}

/// How much of the text colour dim text keeps.
const DIM: f32 = 0.66;

/// Collects a line's cells into runs.
#[derive(Default)]
struct Builder {
    runs: Vec<Run>,
    /// Whether the last run may take more cells: it's plain ASCII text,
    /// whose cells the font draws exactly one wide.
    open: bool,
}

impl Builder {
    /// Adds a cell `cells` wide at `column`.
    fn cell(&mut self, column: usize, cells: usize, text: &str, style: Style) {
        let blank = text == " " && style.bg.is_none() && !style.underline && !style.strike;
        if blank {
            // Spaces draw nothing; the next text starts its own run.
            self.open = false;
            return;
        }
        let plain = cells == 1 && text.len() == 1;
        if let Some(last) = self.runs.last_mut()
            && self.open
            && plain
            && last.style == style
            && last.column + last.cells == column
        {
            last.text.push_str(text);
            last.cells += 1;
            return;
        }
        self.runs.push(Run {
            column,
            cells,
            text: text.to_owned(),
            style,
        });
        self.open = plain;
    }

    /// Ends the line and puts its runs at `line`.
    fn finish(&mut self, lines: &mut [Vec<Run>], line: usize) {
        let runs = std::mem::take(&mut self.runs);
        self.open = false;
        if let Some(slot) = lines.get_mut(line) {
            *slot = runs;
        }
    }
}

/// The terminal's colours.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    /// The 256 indexed colours: 16 named ones, a 6×6×6 cube and greys.
    indexed: Vec<Rgba>,
    /// The text colour.
    pub foreground: Rgba,
    /// The background.
    pub background: Rgba,
    /// Selected cells' background.
    pub selection: Rgba,
    /// The cursor.
    pub cursor: Rgba,
}

impl Palette {
    /// The theme's text, background and status colours, with a fixed set
    /// for the other named colours.
    #[must_use]
    pub fn new(t: &Tokens) -> Self {
        /// The named colours the theme doesn't give, from black to bright
        /// white; 1–4 and 8 are replaced from the theme.
        const NAMED: [u32; 16] = [
            0x18_1818, 0xac_4242, 0x90_a959, 0xf4_bf75, 0x6a_9fb5, 0xaa_759f, 0x75_b5aa, 0xd8_d8d8,
            0x6b_6b6b, 0xc5_5555, 0xaa_c474, 0xfe_ca88, 0x82_b8c8, 0xc2_8cb8, 0x93_d3c3, 0xf8_f8f8,
        ];
        let back = t.background_base;
        let lay = |c: Rgba| over(c, back);
        let mut indexed: Vec<Rgba> = NAMED.iter().map(|&hex| rgb_hex(hex)).collect();
        indexed[1] = lay(t.icon_critical_base);
        indexed[2] = lay(t.icon_success_base);
        indexed[3] = lay(t.icon_warning_base);
        indexed[4] = lay(t.icon_info_base);
        indexed[8] = lay(t.text_weaker);
        // The 6×6×6 cube, then 24 greys, as xterm defines them.
        let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    indexed.push(rgb(level(r), level(g), level(b)));
                }
            }
        }
        for n in 0..24 {
            let grey = 8 + 10 * n;
            indexed.push(rgb(grey, grey, grey));
        }
        Self {
            indexed,
            foreground: lay(t.text_base),
            background: lay(back),
            selection: over(t.text_interactive_base.alpha(crate::theme::SELECTION), back),
            cursor: lay(t.text_base),
        }
    }

    /// The colour `color` stands for, with the program's changes.
    fn resolve(&self, color: Color, changed: &term::color::Colors) -> Rgba {
        match color {
            Color::Spec(Rgb { r, g, b }) => rgb(r, g, b),
            Color::Indexed(index) => changed[usize::from(index)]
                .map_or_else(|| self.indexed[usize::from(index)], from_rgb),
            Color::Named(name) => {
                if let Some(set) = changed[name] {
                    return from_rgb(set);
                }
                self.named(name)
            }
        }
    }

    /// A named colour.
    fn named(&self, name: NamedColor) -> Rgba {
        match name {
            NamedColor::Foreground | NamedColor::BrightForeground => self.foreground,
            NamedColor::Background => self.background,
            NamedColor::Cursor => self.cursor,
            NamedColor::DimForeground => Rgba {
                a: DIM,
                ..self.foreground
            },
            NamedColor::DimBlack
            | NamedColor::DimRed
            | NamedColor::DimGreen
            | NamedColor::DimYellow
            | NamedColor::DimBlue
            | NamedColor::DimMagenta
            | NamedColor::DimCyan
            | NamedColor::DimWhite => {
                let base = name as usize - NamedColor::DimBlack as usize;
                Rgba {
                    a: DIM,
                    ..self.indexed[base]
                }
            }
            other => self
                .indexed
                .get(other as usize)
                .copied()
                .unwrap_or(self.foreground),
        }
    }
}

/// An opaque colour from bytes.
fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a: 1.0,
    }
}

/// An opaque colour from `0xrrggbb`.
fn rgb_hex(hex: u32) -> Rgba {
    let [_, r, g, b] = hex.to_be_bytes();
    rgb(r, g, b)
}

fn from_rgb(c: Rgb) -> Rgba {
    rgb(c.r, c.g, c.b)
}

/// `color` laid over `back`, opaque.
fn over(color: Rgba, back: Rgba) -> Rgba {
    let mix = |front: f32, under: f32| front * color.a + under * (1.0 - color.a);
    Rgba {
        r: mix(color.r, back.r),
        g: mix(color.g, back.g),
        b: mix(color.b, back.b),
        a: 1.0,
    }
}

/// A key, as the terminal tells keys apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// A key that types this text.
    Text(String),
    /// Enter.
    Enter,
    /// Tab; Shift+Tab when shifted.
    Tab,
    /// Backspace.
    Backspace,
    /// Escape.
    Escape,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Right arrow.
    Right,
    /// Left arrow.
    Left,
    /// Home.
    Home,
    /// End.
    End,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// F1 to F12.
    F(u8),
}

impl Key {
    /// The key whose text Slint reports as `text`: the character typed, or
    /// a private-use character for a named key (see `slint::platform::Key`).
    /// `None` for keys the terminal doesn't send, such as Shift alone.
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let mut chars = text.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return (!text.is_empty()).then(|| Self::Text(text.to_owned()));
        };
        let key = match c {
            '\u{8}' => Self::Backspace,
            '\t' | '\u{19}' => Self::Tab,
            '\n' | '\r' => Self::Enter,
            '\u{1b}' => Self::Escape,
            '\u{7f}' => Self::Delete,
            '\u{F700}' => Self::Up,
            '\u{F701}' => Self::Down,
            '\u{F702}' => Self::Left,
            '\u{F703}' => Self::Right,
            '\u{F704}'..='\u{F70F}' => {
                let n = u32::from(c) - 0xF704 + 1;
                Self::F(u8::try_from(n).ok()?)
            }
            '\u{F727}' => Self::Insert,
            '\u{F729}' => Self::Home,
            '\u{F72B}' => Self::End,
            '\u{F72C}' => Self::PageUp,
            '\u{F72D}' => Self::PageDown,
            // Modifiers alone, and other named keys.
            '\u{10}'..='\u{18}' | '\u{F700}'..='\u{F8FF}' => return None,
            c => Self::Text(c.to_string()),
        };
        Some(key)
    }
}

/// The modifiers held with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    /// Control.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
}

impl Mods {
    /// The xterm modifier parameter: 1 plus a bit per modifier held.
    fn param(self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.ctrl)
    }
}

/// The bytes a key sends to the program, if it sends any.
#[must_use]
pub fn encode(key: &Key, mods: Mods, term_mode: TermMode) -> Option<Vec<u8>> {
    let app_cursor = term_mode.contains(TermMode::APP_CURSOR);
    let held = mods.param();
    // Keys sent as CSI sequences, or SS3 in application mode.
    let cursor = |letter: char| -> Vec<u8> {
        if held > 1 {
            format!("\x1b[1;{held}{letter}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{letter}").into_bytes()
        } else {
            format!("\x1b[{letter}").into_bytes()
        }
    };
    let tilde = |code: u8| -> Vec<u8> {
        if held > 1 {
            format!("\x1b[{code};{held}~").into_bytes()
        } else {
            format!("\x1b[{code}~").into_bytes()
        }
    };
    let alt = |bytes: Vec<u8>| -> Vec<u8> {
        if mods.alt {
            let mut out = vec![0x1b];
            out.extend(bytes);
            out
        } else {
            bytes
        }
    };
    let bytes = match key {
        Key::Text(text) => {
            let mut chars = text.chars();
            let only = match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            };
            match only {
                Some(c) if mods.ctrl => alt(vec![control(c)?]),
                _ if text.is_empty() => return None,
                _ => alt(text.as_bytes().to_vec()),
            }
        }
        Key::Enter => alt(vec![b'\r']),
        Key::Tab if mods.shift => b"\x1b[Z".to_vec(),
        Key::Tab => alt(vec![b'\t']),
        Key::Backspace if mods.ctrl => alt(vec![0x08]),
        Key::Backspace => alt(vec![0x7f]),
        Key::Escape => alt(vec![0x1b]),
        Key::Up => cursor('A'),
        Key::Down => cursor('B'),
        Key::Right => cursor('C'),
        Key::Left => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F(n @ 1..=4) => {
            let letter = char::from(b'P' + n - 1);
            if held > 1 {
                format!("\x1b[1;{held}{letter}").into_bytes()
            } else {
                format!("\x1bO{letter}").into_bytes()
            }
        }
        Key::F(n) => {
            let code = match n {
                5 => 15,
                6 => 17,
                7 => 18,
                8 => 19,
                9 => 20,
                10 => 21,
                11 => 23,
                12 => 24,
                _ => return None,
            };
            tilde(code)
        }
    };
    Some(bytes)
}

/// The control character Ctrl and `c` type, if there's one.
fn control(c: char) -> Option<u8> {
    let byte = u8::try_from(c).ok()?;
    match byte.to_ascii_lowercase() {
        b @ b'a'..=b'z' => Some(b - b'a' + 1),
        b'@' | b' ' | b'2' => Some(0),
        b'[' | b'3' => Some(0x1b),
        b'\\' | b'4' => Some(0x1c),
        b']' | b'5' => Some(0x1d),
        b'^' | b'6' => Some(0x1e),
        b'_' | b'-' | b'7' => Some(0x1f),
        b'?' | b'8' => Some(0x7f),
        _ => None,
    }
}
