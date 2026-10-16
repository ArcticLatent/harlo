//! An embedded terminal: a shell on a pty, emulated by `alacritty_terminal`
//! (the engine of the Alacritty terminal), rendered as one rich-text line per
//! screen row. The UI calls `render` after a wakeup and sends input as
//! bytes; keys are translated with `key_bytes`.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use anyhow::{Context, Result};

const SCROLLBACK: usize = 10_000;

/// What the terminal reports back to the UI (from the pty thread).
#[derive(Debug, Clone, PartialEq)]
pub enum TermEvent {
    /// Output arrived; call `render`.
    Wakeup,
    Title(String),
    Bell,
    /// The shell exited.
    Exited(Option<i32>),
}

/// Colours: the 16 ANSI colours plus foreground, background and cursor.
#[derive(Debug, Clone)]
pub struct Palette {
    pub ansi: [Rgb; 16],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub selection_bg: Rgb,
}

pub fn parse_rgb(hex: &str) -> Option<Rgb> {
    let h = hex.trim().trim_start_matches('#');
    // `#AARRGGBB` (Qt) or `#RRGGBB`: keep the RGB part.
    let h = if h.len() == 8 { &h[2..] } else { h };
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Rgb { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 })
}

impl Palette {
    /// From `#RRGGBB` strings: 16 ANSI colours, then fg, bg, cursor,
    /// selection. Missing or bad entries keep xterm-ish defaults.
    pub fn from_hex(colors: &[String]) -> Palette {
        const DEFAULTS: [&str; 20] = [
            "#1d1f21", "#cc6666", "#b5bd68", "#f0c674", "#81a2be", "#b294bb", "#8abeb7", "#c5c8c6", "#666666", "#d54e53", "#b9ca4a",
            "#e7c547", "#7aa6da", "#c397d8", "#70c0b1", "#eaeaea", "#c5c8c6", "#1d1f21", "#c5c8c6", "#373b41",
        ];
        let at = |i: usize| colors.get(i).and_then(|c| parse_rgb(c)).unwrap_or_else(|| parse_rgb(DEFAULTS[i]).expect("default colour"));
        Palette {
            ansi: std::array::from_fn(at),
            foreground: at(16),
            background: at(17),
            cursor: at(18),
            selection_bg: at(19),
        }
    }
}

#[derive(Clone, Copy)]
struct Size {
    cols: usize,
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
        self.cols
    }
}

/// Forwards terminal events to the UI and answers the shell's queries.
#[derive(Clone)]
struct Proxy {
    sink: Arc<dyn Fn(TermEvent) + Send + Sync>,
    sender: Arc<Mutex<Option<EventLoopSender>>>,
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => (self.sink)(TermEvent::Wakeup),
            Event::Title(t) => (self.sink)(TermEvent::Title(t)),
            Event::ResetTitle => (self.sink)(TermEvent::Title(String::new())),
            Event::Bell => (self.sink)(TermEvent::Bell),
            Event::ChildExit(status) => (self.sink)(TermEvent::Exited(status.code())),
            Event::Exit => (self.sink)(TermEvent::Exited(None)),
            // Replies the shell asked for (cursor position, device attributes…).
            Event::PtyWrite(text) => {
                if let Some(s) = self.sender.lock().unwrap().as_ref() {
                    let _ = s.send(Msg::Input(Cow::Owned(text.into_bytes())));
                }
            }
            _ => {}
        }
    }
}

/// The visible screen: one rich-text line per row, and the cursor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Screen {
    pub lines: Vec<String>,
    pub cursor_row: i32,
    pub cursor_col: i32,
    /// Hidden by the program, or scrolled out of view.
    pub cursor_visible: bool,
    /// Rows scrolled back into history (0 = at the bottom).
    pub scrolled_back: usize,
}

pub struct Terminal {
    term: Arc<FairMutex<Term<Proxy>>>,
    sender: EventLoopSender,
    size: Size,
}

impl Terminal {
    /// Starts `shell` (a program and its arguments; else the user's login
    /// shell) in `cwd`.
    pub fn spawn(cwd: PathBuf, shell: Option<(String, Vec<String>)>, cols: usize, lines: usize, sink: impl Fn(TermEvent) + Send + Sync + 'static) -> Result<Terminal> {
        let size = Size { cols: cols.max(2), lines: lines.max(1) };
        let mut env = std::collections::HashMap::new();
        env.insert("TERM".to_owned(), "xterm-256color".to_owned());
        env.insert("COLORTERM".to_owned(), "truecolor".to_owned());
        env.insert("TERM_PROGRAM".to_owned(), "harlo".to_owned());
        let options = tty::Options {
            shell: shell.map(|(program, args)| tty::Shell::new(program, args)),
            working_directory: Some(cwd),
            drain_on_exit: false,
            env,
        };
        let window = WindowSize { num_lines: size.lines as u16, num_cols: size.cols as u16, cell_width: 8, cell_height: 16 };
        let pty = tty::new(&options, window, 0).context("starting the shell")?;
        let sender_slot = Arc::new(Mutex::new(None));
        let proxy = Proxy { sink: Arc::new(sink), sender: Arc::clone(&sender_slot) };
        let config = Config { scrolling_history: SCROLLBACK, ..Config::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, proxy.clone())));
        let event_loop = EventLoop::new(Arc::clone(&term), proxy, pty, false, false).context("starting the terminal")?;
        let sender = event_loop.channel();
        *sender_slot.lock().unwrap() = Some(sender.clone());
        event_loop.spawn();
        Ok(Terminal { term, sender, size })
    }

    pub fn write(&self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        // Typing jumps back to the prompt.
        self.term.lock().scroll_display(Scroll::Bottom);
        let _ = self.sender.send(Msg::Input(Cow::Owned(bytes)));
    }

    /// Pastes text, bracketed when the program asked for it.
    pub fn paste(&self, text: &str) {
        let bracketed = self.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        let bytes = if bracketed {
            format!("\x1b[200~{}\x1b[201~", text.replace("\x1b[201~", ""))
        } else {
            text
        };
        self.write(bytes.into_bytes());
    }

    pub fn resize(&mut self, cols: usize, lines: usize) {
        let size = Size { cols: cols.max(2), lines: lines.max(1) };
        if size.cols == self.size.cols && size.lines == self.size.lines {
            return;
        }
        self.size = size;
        self.term.lock().resize(size);
        let _ = self.sender.send(Msg::Resize(WindowSize { num_lines: size.lines as u16, num_cols: size.cols as u16, cell_width: 8, cell_height: 16 }));
    }

    /// Mouse wheel: scrollback, or arrow keys in full-screen programs
    /// (less, vim) that use the alternate screen.
    pub fn wheel(&self, lines: i32) {
        let mut term = self.term.lock();
        let mode = *term.mode();
        if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            drop(term);
            let key = if lines > 0 { if mode.contains(TermMode::APP_CURSOR) { "\x1bOA" } else { "\x1b[A" } } else if mode.contains(TermMode::APP_CURSOR) { "\x1bOB" } else { "\x1b[B" };
            let _ = self.sender.send(Msg::Input(Cow::Owned(key.repeat(lines.unsigned_abs() as usize).into_bytes())));
        } else {
            term.scroll_display(Scroll::Delta(lines));
        }
    }

    fn point(term: &Term<Proxy>, row: i32, col: i32) -> Point {
        let offset = term.grid().display_offset() as i32;
        let line = (row.clamp(0, term.screen_lines() as i32 - 1)) - offset;
        Point::new(Line(line), Column(col.clamp(0, term.columns() as i32 - 1) as usize))
    }

    /// Starts a selection at a screen cell (`kind`: 1 click, 2 word, 3 line).
    pub fn select_start(&self, row: i32, col: i32, kind: i32) {
        let mut term = self.term.lock();
        let ty = match kind {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        let p = Self::point(&term, row, col);
        term.selection = Some(Selection::new(ty, p, Side::Left));
    }

    pub fn select_update(&self, row: i32, col: i32, right_half: bool) {
        let mut term = self.term.lock();
        let p = Self::point(&term, row, col);
        if let Some(sel) = term.selection.as_mut() {
            sel.update(p, if right_half { Side::Right } else { Side::Left });
        }
    }

    pub fn select_clear(&self) {
        self.term.lock().selection = None;
    }

    pub fn selected_text(&self) -> String {
        self.term.lock().selection_to_string().unwrap_or_default()
    }

    /// The program asked for application cursor keys (vim, less…).
    pub fn app_cursor(&self) -> bool {
        self.term.lock().mode().contains(TermMode::APP_CURSOR)
    }

    pub fn kill(&self) {
        let _ = self.sender.send(Msg::Shutdown);
    }

    /// The visible screen as rich text in `palette`'s colours.
    pub fn render(&self, palette: &Palette) -> Screen {
        let term = self.term.lock();
        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let lines = term.screen_lines();
        let colors = content.colors;
        let selection = content.selection;
        let mut rows: Vec<Vec<(char, Style)>> = vec![Vec::new(); lines];
        for cell in content.display_iter {
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let row = cell.point.line.0 + offset;
            if row < 0 || row as usize >= lines {
                continue;
            }
            let selected = selection.is_some_and(|s| s.contains(cell.point));
            let mut fg = resolve(cell.fg, colors, palette, true);
            let mut bg = resolve(cell.bg, colors, palette, false);
            if cell.flags.contains(Flags::DIM) {
                fg = mix(fg, palette.background);
            }
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags.contains(Flags::HIDDEN) {
                fg = bg;
            }
            if selected {
                bg = palette.selection_bg;
            }
            let style = Style {
                fg: (fg != palette.foreground).then_some(fg),
                bg: (bg != palette.background).then_some(bg),
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
                strike: cell.flags.contains(Flags::STRIKEOUT),
            };
            rows[row as usize].push((cell.c, style));
        }
        let cursor = content.cursor;
        let cursor_row = cursor.point.line.0 + offset;
        let hidden = matches!(cursor.shape, alacritty_terminal::vte::ansi::CursorShape::Hidden) || !content.mode.contains(TermMode::SHOW_CURSOR);
        Screen {
            lines: rows.iter().map(|r| line_html(r)).collect(),
            cursor_row,
            cursor_col: cursor.point.column.0 as i32,
            cursor_visible: !hidden && cursor_row >= 0 && (cursor_row as usize) < lines,
            scrolled_back: content.display_offset,
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.kill();
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
struct Style {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

fn mix(a: Rgb, b: Rgb) -> Rgb {
    Rgb { r: ((a.r as u16 + b.r as u16) / 2) as u8, g: ((a.g as u16 + b.g as u16) / 2) as u8, b: ((a.b as u16 + b.b as u16) / 2) as u8 }
}

fn xterm_256(i: u8) -> Rgb {
    match i {
        16..=231 => {
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Rgb { r: level(i / 36), g: level((i / 6) % 6), b: level(i % 6) }
        }
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            Rgb { r: v, g: v, b: v }
        }
        _ => Rgb { r: 0, g: 0, b: 0 },
    }
}

fn resolve(color: Color, overrides: &alacritty_terminal::term::color::Colors, palette: &Palette, foreground: bool) -> Rgb {
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(i) => overrides[i as usize].unwrap_or_else(|| if i < 16 { palette.ansi[i as usize] } else { xterm_256(i) }),
        Color::Named(n) => {
            if let Some(c) = overrides[n as usize] {
                return c;
            }
            let idx = n as usize;
            match n {
                NamedColor::Foreground | NamedColor::BrightForeground => palette.foreground,
                NamedColor::Background => palette.background,
                NamedColor::Cursor => palette.cursor,
                NamedColor::DimForeground => mix(palette.foreground, palette.background),
                _ if idx < 16 => palette.ansi[idx],
                // Dim variants of the eight colours.
                _ if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize).contains(&idx) => {
                    mix(palette.ansi[idx - NamedColor::DimBlack as usize], palette.background)
                }
                _ => if foreground { palette.foreground } else { palette.background },
            }
        }
    }
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// One row as Qt rich text: runs of equal style become spans; spaces are
/// non-breaking so columns line up; trailing default spaces are dropped.
fn line_html(cells: &[(char, Style)]) -> String {
    let end = cells.iter().rposition(|(c, s)| *c != ' ' || s.bg.is_some() || s.underline || s.strike).map_or(0, |i| i + 1);
    let mut html = String::new();
    let mut i = 0;
    while i < end {
        let style = cells[i].1;
        let mut j = i;
        let mut text = String::new();
        while j < end && cells[j].1 == style {
            match cells[j].0 {
                '&' => text.push_str("&amp;"),
                '<' => text.push_str("&lt;"),
                '>' => text.push_str("&gt;"),
                ' ' | '\0' => text.push_str("&nbsp;"),
                c => text.push(c),
            }
            j += 1;
        }
        if style == Style::default() {
            html.push_str(&text);
        } else {
            let mut css = String::new();
            if let Some(fg) = style.fg {
                css.push_str(&format!("color:{};", hex(fg)));
            }
            if let Some(bg) = style.bg {
                css.push_str(&format!("background-color:{};", hex(bg)));
            }
            if style.bold {
                css.push_str("font-weight:bold;");
            }
            if style.italic {
                css.push_str("font-style:italic;");
            }
            match (style.underline, style.strike) {
                (true, true) => css.push_str("text-decoration:underline line-through;"),
                (true, false) => css.push_str("text-decoration:underline;"),
                (false, true) => css.push_str("text-decoration:line-through;"),
                _ => {}
            }
            html.push_str(&format!("<span style=\"{css}\">{text}</span>"));
        }
        i = j;
    }
    html
}

// Qt key codes and modifier bits (Qt::Key, Qt::KeyboardModifier).
const SHIFT: u32 = 0x0200_0000;
const CTRL: u32 = 0x0400_0000;
const ALT: u32 = 0x0800_0000;

/// The bytes a key press sends to the shell, from a Qt key code, modifier
/// bits and the event's text. `app_cursor`: the program asked for
/// application cursor keys (DECCKM).
pub fn key_bytes(key: i32, modifiers: u32, text: &str, app_cursor: bool) -> Vec<u8> {
    let key = key as u32;
    let (shift, ctrl, alt) = (modifiers & SHIFT != 0, modifiers & CTRL != 0, modifiers & ALT != 0);
    // xterm modifier parameter: 1 + shift + 2·alt + 4·ctrl.
    let m = 1 + shift as u32 + 2 * alt as u32 + 4 * ctrl as u32;
    let csi = |final_: char| -> Vec<u8> {
        if m > 1 { format!("\x1b[1;{m}{final_}") } else if app_cursor { format!("\x1bO{final_}") } else { format!("\x1b[{final_}") }.into_bytes()
    };
    let tilde = |n: u32| -> Vec<u8> { if m > 1 { format!("\x1b[{n};{m}~") } else { format!("\x1b[{n}~") }.into_bytes() };
    let seq: Option<Vec<u8>> = match key {
        0x0100_0000 => Some(b"\x1b".to_vec()),                                  // Escape
        0x0100_0001 => Some(b"\t".to_vec()),                                    // Tab
        0x0100_0002 => Some(b"\x1b[Z".to_vec()),                                // Backtab
        0x0100_0003 => Some(if ctrl { b"\x08".to_vec() } else if alt { b"\x1b\x7f".to_vec() } else { b"\x7f".to_vec() }), // Backspace
        0x0100_0004 | 0x0100_0005 => Some(if alt { b"\x1b\r".to_vec() } else { b"\r".to_vec() }), // Return, Enter
        0x0100_0006 => Some(tilde(2)),                                          // Insert
        0x0100_0007 => Some(tilde(3)),                                          // Delete
        0x0100_0010 => Some(csi('H')),                                          // Home
        0x0100_0011 => Some(csi('F')),                                          // End
        0x0100_0012 => Some(csi('D')),                                          // Left
        0x0100_0013 => Some(csi('A')),                                          // Up
        0x0100_0014 => Some(csi('C')),                                          // Right
        0x0100_0015 => Some(csi('B')),                                          // Down
        0x0100_0016 => Some(tilde(5)),                                          // PageUp
        0x0100_0017 => Some(tilde(6)),                                          // PageDown
        0x0100_0030..=0x0100_0033 => {
            // F1–F4
            let f = ['P', 'Q', 'R', 'S'][(key - 0x0100_0030) as usize];
            Some(if m > 1 { format!("\x1b[1;{m}{f}") } else { format!("\x1bO{f}") }.into_bytes())
        }
        0x0100_0034..=0x0100_003b => {
            // F5–F12
            const CODES: [u32; 8] = [15, 17, 18, 19, 20, 21, 23, 24];
            Some(tilde(CODES[(key - 0x0100_0034) as usize]))
        }
        _ => None,
    };
    if let Some(s) = seq {
        return s;
    }
    if ctrl && !text.is_empty() && text.chars().all(|c| (c as u32) < 0x20 || c == '\x7f') {
        // Qt already turned Ctrl+letter into its control character.
        let mut out = if alt { vec![0x1b] } else { Vec::new() };
        out.extend_from_slice(text.as_bytes());
        return out;
    }
    if ctrl {
        let control = match key {
            0x41..=0x5a => Some((key - 0x40) as u8), // Ctrl+A … Ctrl+Z
            0x20 | 0x40 => Some(0),                  // Ctrl+Space, Ctrl+@
            0x5b => Some(0x1b),                      // Ctrl+[
            0x5c => Some(0x1c),                      // Ctrl+\
            0x5d => Some(0x1d),                      // Ctrl+]
            0x5e => Some(0x1e),                      // Ctrl+^
            0x5f | 0x2f => Some(0x1f),               // Ctrl+_ , Ctrl+/
            _ => None,
        };
        if let Some(c) = control {
            return if alt { vec![0x1b, c] } else { vec![c] };
        }
    }
    if text.is_empty() {
        return Vec::new();
    }
    let mut out = if alt { vec![0x1b] } else { Vec::new() };
    out.extend_from_slice(text.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn translates_keys() {
        assert_eq!(key_bytes(0x0100_0004, 0, "\r", false), b"\r");
        assert_eq!(key_bytes(0x0100_0013, 0, "", false), b"\x1b[A");
        assert_eq!(key_bytes(0x0100_0013, 0, "", true), b"\x1bOA");
        assert_eq!(key_bytes(0x0100_0014, CTRL, "", false), b"\x1b[1;5C");
        assert_eq!(key_bytes(0x43, CTRL, "\x03", false), b"\x03");
        assert_eq!(key_bytes(0x43, CTRL, "", false), b"\x03");
        assert_eq!(key_bytes(0x42, ALT, "b", false), b"\x1bb");
        assert_eq!(key_bytes(0x0100_0007, 0, "", false), b"\x1b[3~");
        assert_eq!(key_bytes(0x0100_0034, 0, "", false), b"\x1b[15~");
        assert_eq!(key_bytes(0x41, 0, "é", false), "é".as_bytes());
        assert_eq!(key_bytes(0x0100_0020, 0, "", false), b"", "a lone Shift sends nothing");
    }

    #[test]
    fn renders_styled_lines() {
        let s = |c, fg| (c, Style { fg, ..Style::default() });
        let red = Some(Rgb { r: 255, g: 0, b: 0 });
        let html = line_html(&[s('a', None), s('<', red), s(' ', red), s('b', None), s(' ', None), s(' ', None)]);
        assert_eq!(html, "a<span style=\"color:#ff0000;\">&lt;&nbsp;</span>b");
        assert_eq!(parse_rgb("#80112233"), Some(Rgb { r: 0x11, g: 0x22, b: 0x33 }));
        assert_eq!(xterm_256(196), Rgb { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn runs_a_shell() {
        let (tx, rx) = std::sync::mpsc::channel();
        let term = Terminal::spawn(std::env::temp_dir(), Some(("/bin/sh".into(), Vec::new())), 60, 10, move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        term.write(b"printf 'harlo-%s\\n' term-ok\r".to_vec());
        let palette = Palette::from_hex(&[]);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut found = false;
        while Instant::now() < deadline && !found {
            let _ = rx.recv_timeout(Duration::from_millis(100));
            found = term.render(&palette).lines.iter().any(|l| l.contains("harlo-term-ok"));
        }
        assert!(found, "{:?}", term.render(&palette).lines);
        term.write(b"exit\r".to_vec());
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut exited = false;
        while Instant::now() < deadline && !exited {
            exited = matches!(rx.recv_timeout(Duration::from_millis(200)), Ok(TermEvent::Exited(_)));
        }
        assert!(exited, "the shell's exit is reported");
    }
}
