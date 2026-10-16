//! `TerminalSession`: one embedded terminal (harlo-core `terminal`) as a list
//! of rich-text screen lines plus the cursor, for TerminalView.qml.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel = crate::qt_base::qobject::QAbstractListModel;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(bool, running)]
        #[qproperty(QString, title)]
        #[qproperty(QString, cwd)]
        #[qproperty(i32, cursor_row, cxx_name = "cursorRow")]
        #[qproperty(i32, cursor_col, cxx_name = "cursorCol")]
        #[qproperty(bool, cursor_visible, cxx_name = "cursorVisible")]
        /// Rows scrolled back into history (0 = following the output).
        #[qproperty(i32, scrolled_back, cxx_name = "scrolledBack")]
        /// Exit message once the shell has ended.
        #[qproperty(QString, exit_text, cxx_name = "exitText")]
        type TerminalSession = super::TerminalSessionRust;
    }

    impl cxx_qt::Threading for TerminalSession {}

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
        /// Ends the shell.
        #[qinvokable]
        fn stop(self: Pin<&mut TerminalSession>);
    }

    extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut TerminalSession>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut TerminalSession>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut TerminalSession>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QVector_i32);
        #[inherit]
        fn index(self: &TerminalSession, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        /// The terminal rang its bell.
        #[qsignal]
        fn bell(self: Pin<&mut TerminalSession>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TerminalSession, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TerminalSession) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TerminalSession, parent: &QModelIndex) -> i32;
    }
}

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant, QVector};
use harlo_core::terminal::{Palette, Screen, TermEvent, Terminal, key_bytes};

use qobject::TerminalSession;

const ROLE_HTML: i32 = 0x100;

pub struct TerminalSessionRust {
    running: bool,
    title: QString,
    cwd: QString,
    cursor_row: i32,
    cursor_col: i32,
    cursor_visible: bool,
    scrolled_back: i32,
    exit_text: QString,

    terminal: Option<Terminal>,
    palette: Palette,
    lines: Vec<String>,
    /// A render is already queued (wakeups come in bursts).
    pending: Arc<AtomicBool>,
}

impl Default for TerminalSessionRust {
    fn default() -> Self {
        Self {
            running: false,
            title: QString::default(),
            cwd: QString::default(),
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: false,
            scrolled_back: 0,
            exit_text: QString::default(),
            terminal: None,
            palette: Palette::from_hex(&[]),
            lines: Vec::new(),
            pending: Arc::new(AtomicBool::new(false)),
        }
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
                args.push(format!("cd {} 2>/dev/null; exec \"$SHELL\" -l", harlo_core::remote::quote(&path)));
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
                        this.render();
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
                });
            }
        };
        match Terminal::spawn(local_dir, shell, cols.max(2) as usize, rows.max(1) as usize, sink) {
            Ok(t) => {
                self.as_mut().rust_mut().terminal = Some(t);
                self.as_mut().set_cwd(QString::from(&dir.to_string_lossy().into_owned()));
                self.as_mut().set_exit_text(QString::default());
                self.as_mut().set_running(true);
                self.render();
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
        self.render();
    }

    pub fn resize(mut self: Pin<&mut Self>, cols: i32, rows: i32) {
        if let Some(t) = self.as_mut().rust_mut().terminal.as_mut() {
            t.resize(cols.max(2) as usize, rows.max(1) as usize);
        }
        self.render();
    }

    pub fn key(self: Pin<&mut Self>, key: i32, modifiers: i32, text: &QString) {
        let Some(t) = self.terminal.as_ref() else { return };
        let bytes = key_bytes(key, modifiers as u32, &text.to_string(), t.app_cursor());
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
        self.render();
    }

    pub fn select_start(self: Pin<&mut Self>, row: i32, col: i32, kind: i32) {
        if let Some(t) = self.terminal.as_ref() {
            t.select_start(row, col, kind);
        }
        self.render();
    }

    pub fn select_update(self: Pin<&mut Self>, row: i32, col: i32, right_half: bool) {
        if let Some(t) = self.terminal.as_ref() {
            t.select_update(row, col, right_half);
        }
        self.render();
    }

    pub fn select_clear(self: Pin<&mut Self>) {
        if let Some(t) = self.terminal.as_ref() {
            t.select_clear();
        }
        self.render();
    }

    pub fn selected_text(&self) -> QString {
        QString::from(&self.terminal.as_ref().map(|t| t.selected_text()).unwrap_or_default())
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        if let Some(t) = self.as_mut().rust_mut().terminal.take() {
            t.kill();
        }
        self.set_running(false);
    }

    /// Re-reads the screen; only changed lines are reported.
    fn render(mut self: Pin<&mut Self>) {
        let Some(t) = self.terminal.as_ref() else { return };
        let Screen { lines, cursor_row, cursor_col, cursor_visible, scrolled_back } = t.render(&self.palette);
        if lines.len() != self.lines.len() {
            unsafe {
                self.as_mut().begin_reset_model();
                self.as_mut().rust_mut().lines = lines;
                self.as_mut().end_reset_model();
            }
        } else {
            let changed: Vec<usize> = lines.iter().zip(&self.lines).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i).collect();
            if !changed.is_empty() {
                self.as_mut().rust_mut().lines = lines;
                let root = QModelIndex::default();
                let (first, last) = (changed[0] as i32, *changed.last().unwrap() as i32);
                let (a, b) = (self.index(first, 0, &root), self.index(last, 0, &root));
                self.as_mut().data_changed(&a, &b, &QVector::<i32>::default());
            }
        }
        if self.cursor_row != cursor_row {
            self.as_mut().set_cursor_row(cursor_row);
        }
        if self.cursor_col != cursor_col {
            self.as_mut().set_cursor_col(cursor_col);
        }
        if self.cursor_visible != cursor_visible {
            self.as_mut().set_cursor_visible(cursor_visible);
        }
        if self.scrolled_back != scrolled_back as i32 {
            self.as_mut().set_scrolled_back(scrolled_back as i32);
        }
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        match (self.lines.get(index.row() as usize), role) {
            (Some(l), ROLE_HTML) => QVariant::from(&QString::from(l)),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_HTML, QByteArray::from("html"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.lines.len() as i32
    }
}
