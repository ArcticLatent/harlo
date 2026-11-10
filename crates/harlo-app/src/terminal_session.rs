//! `TerminalSession`: one embedded terminal (harlo-core `terminal`), drawn
//! by Rust (harlo-code's `TermPainter`) into an image that Qt only shows,
//! on the code view's item (cpp/harlo_codeview.h). TerminalView.qml passes
//! it the keys and the mouse.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;
    }

    // (Declared once, in code_view.rs: a second declaration fails to link.)
    #[namespace = "harlo"]
    unsafe extern "C++" {
        include!("harlo_codeview.h");
        type CodeItem = crate::code_view::qobject::CodeItem;
    }

    extern "RustQt" {
        #[qobject]
        #[base = CodeItem]
        #[qml_element]
        #[qproperty(bool, running)]
        #[qproperty(QString, title)]
        #[qproperty(QString, cwd)]
        /// Rows scrolled back into history (0 = following the output).
        #[qproperty(i32, scrolled_back, cxx_name = "scrolledBack")]
        /// Exit message once the shell has ended.
        #[qproperty(QString, exit_text, cxx_name = "exitText")]
        #[qproperty(QString, font_family, cxx_name = "fontFamily", READ, WRITE = set_font_family, NOTIFY)]
        #[qproperty(f64, font_pixel_size, cxx_name = "fontPixelSize", READ, WRITE = set_font_pixel_size, NOTIFY)]
        /// Around the grid.
        #[qproperty(f64, padding, READ, WRITE = set_padding, NOTIFY)]
        /// The terminal has the keyboard: a solid, blinking cursor.
        #[qproperty(bool, focused, READ, WRITE = set_focused, NOTIFY)]
        /// A cell (logical pixels, whole device pixels each).
        #[qproperty(f64, cell_width, cxx_name = "cellWidth")]
        #[qproperty(f64, cell_height, cxx_name = "cellHeight")]
        /// How long the last frame took to draw (self-tests).
        #[qproperty(f64, paint_ms, cxx_name = "paintMs")]
        type TerminalSession = super::TerminalSessionRust;
    }

    impl cxx_qt::Threading for TerminalSession {}
    impl cxx_qt::Constructor<()> for TerminalSession {}

    extern "RustQt" {
        #[cxx_name = "setFontFamily"]
        fn set_font_family(self: Pin<&mut TerminalSession>, family: QString);
        #[cxx_name = "setFontPixelSize"]
        fn set_font_pixel_size(self: Pin<&mut TerminalSession>, px: f64);
        #[cxx_name = "setPadding"]
        fn set_padding(self: Pin<&mut TerminalSession>, pad: f64);
        #[cxx_name = "setFocused"]
        fn set_focused(self: Pin<&mut TerminalSession>, focused: bool);
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// Starts the user's shell in `cwd` with `cols`×`rows` cells. `palette`:
        /// 16 ANSI colours, then foreground, background, cursor, selection.
        #[qinvokable]
        fn start(self: Pin<&mut TerminalSession>, cwd: &QString, cols: i32, rows: i32, palette: &QStringList) -> bool;
        #[qinvokable]
        fn set_palette(self: Pin<&mut TerminalSession>, palette: &QStringList);
        #[qinvokable]
        fn resize(self: Pin<&mut TerminalSession>, cols: i32, rows: i32);
        /// A key press (Qt key code, modifier bits, event text).
        #[qinvokable]
        fn key(self: Pin<&mut TerminalSession>, key: i32, modifiers: i32, text: &QString);
        #[qinvokable]
        fn paste(self: Pin<&mut TerminalSession>, text: &QString);
        /// Wheel steps: positive scrolls up (back in history).
        #[qinvokable]
        fn wheel(self: Pin<&mut TerminalSession>, lines: i32);
        /// `kind`: 1 click, 2 word, 3 line.
        #[qinvokable]
        fn select_start(self: Pin<&mut TerminalSession>, row: i32, col: i32, kind: i32);
        #[qinvokable]
        fn select_update(self: Pin<&mut TerminalSession>, row: i32, col: i32, right_half: bool);
        #[qinvokable]
        fn select_clear(self: Pin<&mut TerminalSession>);
        #[qinvokable]
        fn selected_text(self: &TerminalSession) -> QString;
        /// The link at a screen cell ("" for none).
        #[qinvokable]
        fn link_at(self: &TerminalSession, row: i32, col: i32) -> QString;
        /// A screen row's text, as last drawn (self-tests).
        #[qinvokable]
        fn row_text(self: &TerminalSession, row: i32) -> QString;
        /// Ends the shell.
        #[qinvokable]
        fn stop(self: Pin<&mut TerminalSession>);
    }

    unsafe extern "RustQt" {
        /// The terminal rang its bell.
        #[qsignal]
        fn bell(self: Pin<&mut TerminalSession>);
    }

    unsafe extern "RustQt" {
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut TerminalSession>, painter: *mut QPainter);
        #[cxx_override]
        #[cxx_name = "sizeHook"]
        fn size_hook(self: Pin<&mut TerminalSession>);
        #[cxx_override]
        #[cxx_name = "timerHook"]
        fn timer_hook(self: Pin<&mut TerminalSession>, id: i32);
        #[cxx_override]
        fn editable(self: &TerminalSession) -> bool;
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn update(self: Pin<&mut TerminalSession>);
        #[inherit]
        fn width(self: &TerminalSession) -> f64;
        #[inherit]
        fn height(self: &TerminalSession) -> f64;
        #[inherit]
        #[cxx_name = "pixelRatio"]
        fn pixel_ratio(self: &TerminalSession) -> f64;
        #[inherit]
        unsafe fn blit(self: &TerminalSession, painter: *mut QPainter, data: *const u8, width: i32, height: i32);
        #[inherit]
        #[cxx_name = "startTicks"]
        fn start_ticks(self: Pin<&mut TerminalSession>, ms: i32) -> i32;
        #[inherit]
        #[cxx_name = "stopTicks"]
        fn stop_ticks(self: Pin<&mut TerminalSession>, id: i32);
        #[inherit]
        #[cxx_name = "takeNoInput"]
        fn take_no_input(self: Pin<&mut TerminalSession>);
    }
}

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use harlo_code::terminal::{CursorLook, TermPainter};
use harlo_core::terminal::{Palette, Screen, TermEvent, Terminal, key_bytes};

use qobject::TerminalSession;

/// The cursor's blink, on and off.
const BLINK_MS: i32 = 600;

pub struct TerminalSessionRust {
    running: bool,
    title: QString,
    cwd: QString,
    scrolled_back: i32,
    exit_text: QString,
    font_family: QString,
    font_pixel_size: f64,
    padding: f64,
    focused: bool,
    cell_width: f64,
    cell_height: f64,
    paint_ms: f64,

    terminal: Option<Terminal>,
    palette: Palette,
    /// The screen as last read (kept once the shell has ended).
    screen: Screen,
    painter: TermPainter,
    blink_on: bool,
    blink_timer: i32,
    /// A wakeup is already queued (they come in bursts).
    pending: Arc<AtomicBool>,
}

impl Default for TerminalSessionRust {
    fn default() -> Self {
        Self {
            running: false,
            title: QString::default(),
            cwd: QString::default(),
            scrolled_back: 0,
            exit_text: QString::default(),
            font_family: QString::from("JetBrains Mono"),
            font_pixel_size: 13.0,
            padding: 8.0,
            focused: false,
            cell_width: 8.0,
            cell_height: 17.0,
            paint_ms: 0.0,
            terminal: None,
            palette: Palette::from_hex(&[]),
            screen: Screen::default(),
            painter: TermPainter::new(),
            blink_on: true,
            blink_timer: 0,
            pending: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl cxx_qt::Initialize for TerminalSession {
    fn initialize(mut self: Pin<&mut Self>) {
        // (The view's MouseArea takes the mouse, and the view the keys.)
        self.as_mut().take_no_input();
        self.measure();
    }
}

fn strings(list: &QStringList) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

impl TerminalSession {
    pub fn start(mut self: Pin<&mut Self>, cwd: &QString, cols: i32, rows: i32, palette: &QStringList) -> bool {
        if self.terminal.is_some() {
            return true;
        }
        self.as_mut().rust_mut().palette = Palette::from_hex(&strings(palette));
        let dir = PathBuf::from(cwd.to_string());
        let home = || PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/".into()));
        // A remote folder: a shell there over SSH (the shared connection).
        let remote = harlo_core::remote::split(&dir).and_then(|(h, path)| Some((harlo_core::remote::host(&h)?, path)));
        let (local_dir, shell) = match remote {
            Some((host, path)) => {
                let mut args = vec!["-t".to_owned()];
                args.extend(harlo_core::remote::ssh_args(&host));
                // (With the folders harlo starts its agents from on its PATH:
                // a login shell there may not have them, ~/.local/bin say.)
                args.push(format!("{} cd {} 2>/dev/null; exec \"$SHELL\" -l", harlo_core::remote::terminal_path(), harlo_core::remote::quote(&path)));
                (home(), Some(("ssh".to_owned(), args)))
            }
            None => (if dir.is_dir() { dir.clone() } else { home() }, None),
        };
        let dir = if shell.is_some() { dir } else { local_dir.clone() };
        let thread = self.qt_thread();
        let pending = Arc::clone(&self.pending);
        let sink = move |event: TermEvent| match event {
            TermEvent::Wakeup => {
                if !pending.swap(true, Ordering::AcqRel) {
                    let pending = Arc::clone(&pending);
                    let _ = thread.queue(move |this| {
                        pending.store(false, Ordering::Release);
                        this.wake();
                    });
                }
            }
            TermEvent::Title(t) => {
                let _ = thread.queue(move |this| this.set_title(QString::from(&t)));
            }
            TermEvent::Bell => {
                let _ = thread.queue(|this| this.bell());
            }
            TermEvent::Exited(code) => {
                let _ = thread.queue(move |mut this| {
                    this.as_mut().set_running(false);
                    this.as_mut().set_title(QString::default());
                    let text = match code {
                        Some(0) | None => "The shell exited.".to_owned(),
                        Some(c) => format!("The shell exited with status {c}."),
                    };
                    this.as_mut().set_exit_text(QString::from(&text));
                    this.as_mut().rust_mut().terminal = None;
                    this.blink();
                });
            }
        };
        match Terminal::spawn(local_dir, shell, cols.max(2) as usize, rows.max(1) as usize, sink) {
            Ok(t) => {
                self.as_mut().rust_mut().terminal = Some(t);
                self.as_mut().set_cwd(QString::from(&dir.to_string_lossy().into_owned()));
                self.as_mut().set_exit_text(QString::default());
                self.as_mut().set_running(true);
                self.wake();
                true
            }
            Err(e) => {
                self.set_exit_text(QString::from(&format!("Could not start the shell: {e:#}")));
                false
            }
        }
    }

    pub fn set_palette(mut self: Pin<&mut Self>, palette: &QStringList) {
        self.as_mut().rust_mut().palette = Palette::from_hex(&strings(palette));
        self.update();
    }

    pub fn resize(mut self: Pin<&mut Self>, cols: i32, rows: i32) {
        if let Some(t) = self.as_mut().rust_mut().terminal.as_mut() {
            t.resize(cols.max(2) as usize, rows.max(1) as usize);
        }
        self.wake();
    }

    pub fn key(self: Pin<&mut Self>, key: i32, modifiers: i32, text: &QString) {
        let Some(t) = self.terminal.as_ref() else { return };
        let bytes = key_bytes(key, modifiers as u32, &text.to_string(), t.app_cursor());
        // (Only a key that types something: Ctrl or Shift alone, on the way
        // to Ctrl+Shift+C, keeps the selection to copy.)
        if bytes.is_empty() {
            return;
        }
        t.select_clear();
        t.write(bytes);
    }

    pub fn paste(self: Pin<&mut Self>, text: &QString) {
        if let Some(t) = self.terminal.as_ref() {
            t.paste(&text.to_string());
        }
    }

    pub fn wheel(self: Pin<&mut Self>, lines: i32) {
        if let Some(t) = self.terminal.as_ref() {
            t.wheel(lines);
        }
        self.wake();
    }

    pub fn select_start(self: Pin<&mut Self>, row: i32, col: i32, kind: i32) {
        if let Some(t) = self.terminal.as_ref() {
            t.select_start(row, col, kind);
        }
        self.update();
    }

    pub fn select_update(self: Pin<&mut Self>, row: i32, col: i32, right_half: bool) {
        if let Some(t) = self.terminal.as_ref() {
            t.select_update(row, col, right_half);
        }
        self.update();
    }

    pub fn select_clear(self: Pin<&mut Self>) {
        if let Some(t) = self.terminal.as_ref() {
            t.select_clear();
        }
        self.update();
    }

    pub fn selected_text(&self) -> QString {
        QString::from(&self.terminal.as_ref().map(|t| t.selected_text()).unwrap_or_default())
    }

    pub fn link_at(&self, row: i32, col: i32) -> QString {
        QString::from(&self.terminal.as_ref().map(|t| t.link_at(row, col)).unwrap_or_default())
    }

    pub fn row_text(&self, row: i32) -> QString {
        QString::from(&self.screen.row_text(row.max(0) as usize))
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        if let Some(t) = self.as_mut().rust_mut().terminal.take() {
            t.kill();
        }
        self.as_mut().set_running(false);
        self.blink();
    }

    pub fn set_font_family(mut self: Pin<&mut Self>, family: QString) {
        if family != self.font_family {
            self.as_mut().rust_mut().font_family = family;
            self.as_mut().font_family_changed();
            self.measure();
        }
    }

    pub fn set_font_pixel_size(mut self: Pin<&mut Self>, px: f64) {
        if px != self.font_pixel_size {
            self.as_mut().rust_mut().font_pixel_size = px;
            self.as_mut().font_pixel_size_changed();
            self.measure();
        }
    }

    pub fn set_padding(mut self: Pin<&mut Self>, pad: f64) {
        if pad != self.padding {
            self.as_mut().rust_mut().padding = pad;
            self.as_mut().rust_mut().painter.set_padding(pad as f32);
            self.as_mut().padding_changed();
            self.update();
        }
    }

    pub fn set_focused(mut self: Pin<&mut Self>, focused: bool) {
        if focused != self.focused {
            self.as_mut().rust_mut().focused = focused;
            self.as_mut().focused_changed();
            self.blink();
        }
    }

    /// The cell's size for the font at the window's scale.
    fn measure(mut self: Pin<&mut Self>) {
        let dpr = self.pixel_ratio().max(0.5) as f32;
        let (family, px) = (self.font_family.to_string(), self.font_pixel_size as f32);
        let (w, h) = {
            let mut rust = self.as_mut().rust_mut();
            rust.painter.set_font(&family, px);
            rust.painter.cell_size(dpr)
        };
        if w as f64 != self.cell_width {
            self.as_mut().set_cell_width(w as f64);
        }
        if h as f64 != self.cell_height {
            self.as_mut().set_cell_height(h as f64);
        }
        self.update();
    }

    /// Output arrived (or the view moved): draw the screen again, the
    /// cursor holding still meanwhile.
    fn wake(mut self: Pin<&mut Self>) {
        let back = self.terminal.as_ref().map_or(0, |t| t.scrolled_back()) as i32;
        if back != self.scrolled_back {
            self.as_mut().set_scrolled_back(back);
        }
        self.blink();
    }

    /// Restarts the cursor's blink (running only while it can be seen).
    fn blink(mut self: Pin<&mut Self>) {
        let id = self.blink_timer;
        self.as_mut().stop_ticks(id);
        self.as_mut().rust_mut().blink_on = true;
        let id = if self.focused && self.running { self.as_mut().start_ticks(BLINK_MS) } else { 0 };
        self.as_mut().rust_mut().blink_timer = id;
        self.update();
    }

    unsafe fn paint(mut self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        let t = Instant::now();
        let dpr = self.pixel_ratio();
        let w = (self.width() * dpr).round().max(1.0) as usize;
        let h = (self.height() * dpr).round().max(1.0) as usize;
        {
            let mut rust = self.as_mut().rust_mut();
            let rust = &mut *rust;
            if let Some(term) = rust.terminal.as_ref() {
                term.render(&rust.palette, &mut rust.screen);
            }
            let cursor = rust.terminal.is_some().then_some(CursorLook { focused: rust.focused, on: rust.blink_on, color: rust.palette.cursor });
            rust.painter.render(&rust.screen, rust.palette.background, cursor, w, h, dpr as f32);
        }
        unsafe { self.blit(painter, self.painter.pixels().as_ptr(), w as i32, h as i32) };
        // (Read by the self-tests; no signal from the render thread.)
        self.as_mut().rust_mut().paint_ms = t.elapsed().as_secs_f64() * 1000.0;
    }

    fn size_hook(self: Pin<&mut Self>) {
        // (The window's scale is known once the item is in one.)
        self.measure();
    }

    fn timer_hook(mut self: Pin<&mut Self>, id: i32) {
        if id == self.blink_timer && id != 0 {
            let on = !self.blink_on;
            self.as_mut().rust_mut().blink_on = on;
            self.update();
        }
    }

    fn editable(&self) -> bool {
        false
    }
}
