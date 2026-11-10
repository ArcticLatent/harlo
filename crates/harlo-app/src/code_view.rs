//! `CodeView`: harlo's code view, drawn in Rust (harlo-code) into an image
//! that Qt only shows: the file editor, and each side of a diff. The Qt side
//! (cpp/harlo_codeview.h) passes the keys, the mouse, the wheel and the
//! input method here as plain values. Its properties and functions follow
//! QML's TextEdit where they can (positions in UTF-16 code units).
//!
//! A file's unsaved document (its text and undo history) outlives its view:
//! it's kept by path until the file is opened again (`restore`), saved or
//! discarded (`CodeDrafts`).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;
        include!("cxx-qt-lib/qrectf.h");
        type QRectF = cxx_qt_lib::QRectF;
    }

    #[namespace = "harlo"]
    unsafe extern "C++Qt" {
        include!("harlo_codeview.h");
        #[qobject]
        type CodeItem;
    }

    #[namespace = "harlo"]
    unsafe extern "C++" {
        fn clipboard_text(selection: bool) -> QString;
        fn set_clipboard_text(text: &QString, selection: bool);
    }

    extern "RustQt" {
        #[qobject]
        #[base = CodeItem]
        #[qml_element]
        #[qproperty(QString, text, READ = text_value, WRITE = set_text, NOTIFY)]
        /// The file's path: its language, comments and indentation.
        #[qproperty(QString, path, READ, WRITE = set_path, NOTIFY)]
        /// Its language's name ("" when it isn't coloured).
        #[qproperty(QString, language)]
        #[qproperty(bool, read_only, cxx_name = "readOnly", READ, WRITE = set_read_only, NOTIFY)]
        #[qproperty(i32, length)]
        #[qproperty(i32, line_count, cxx_name = "lineCount")]
        /// The line the cursor is on (from 0).
        #[qproperty(i32, cursor_line, cxx_name = "cursorLine")]
        #[qproperty(i32, cursor_position, cxx_name = "cursorPosition", READ, WRITE = set_cursor_position, NOTIFY)]
        #[qproperty(i32, selection_start, cxx_name = "selectionStart")]
        #[qproperty(i32, selection_end, cxx_name = "selectionEnd")]
        #[qproperty(QString, selected_text, cxx_name = "selectedText", READ = selected_text_value, NOTIFY)]
        /// The cursor's rectangle in the text (as `positionToRectangle`).
        #[qproperty(QRectF, cursor_rectangle, cxx_name = "cursorRectangle")]
        /// It differs from what was loaded or last saved.
        #[qproperty(bool, modified)]
        #[qproperty(bool, can_undo, cxx_name = "canUndo")]
        #[qproperty(bool, can_redo, cxx_name = "canRedo")]
        /// One level of indentation (a tab or spaces), from the text.
        #[qproperty(QString, indent_unit, cxx_name = "indentUnit")]
        #[qproperty(f64, content_x, cxx_name = "contentX", READ, WRITE = set_content_x, NOTIFY)]
        #[qproperty(f64, content_y, cxx_name = "contentY", READ, WRITE = set_content_y, NOTIFY)]
        #[qproperty(f64, content_width, cxx_name = "contentWidth")]
        #[qproperty(f64, content_height, cxx_name = "contentHeight")]
        #[qproperty(f64, line_height, cxx_name = "lineHeight")]
        /// Where column 0 is (item x, unscrolled), past the gutter.
        #[qproperty(f64, text_x, cxx_name = "textX")]
        /// Above the first line.
        #[qproperty(f64, top_padding, cxx_name = "topPadding", READ, WRITE = set_top_padding, NOTIFY)]
        /// 0 none, 1 line numbers, 2 a diff's rows (`setRows`).
        #[qproperty(i32, gutter, READ, WRITE = set_gutter, NOTIFY)]
        /// The gutter's width (set for a diff's rows; else measured).
        #[qproperty(f64, gutter_width, cxx_name = "gutterWidth", READ, WRITE = set_gutter_width, NOTIFY)]
        #[qproperty(QString, font_family, cxx_name = "fontFamily", READ, WRITE = set_font_family, NOTIFY)]
        #[qproperty(f64, font_pixel_size, cxx_name = "fontPixelSize", READ, WRITE = set_font_pixel_size, NOTIFY)]
        #[qproperty(f64, gutter_pixel_size, cxx_name = "gutterPixelSize", READ, WRITE = set_gutter_pixel_size, NOTIFY)]
        /// The colours, as JSON (see `palette_from_json`).
        #[qproperty(QString, colors, READ, WRITE = set_colors, NOTIFY)]
        /// The find matches (`findAll`): how many, and the current one.
        #[qproperty(i32, match_count, cxx_name = "matchCount")]
        #[qproperty(i32, current_match, cxx_name = "currentMatch", READ, WRITE = set_current_match, NOTIFY)]
        /// A line just arrived at (from 0; -1 none), tinted by `arrivalGlow`.
        #[qproperty(i32, arrival_line, cxx_name = "arrivalLine", READ, WRITE = set_arrival_line, NOTIFY)]
        #[qproperty(f64, arrival_glow, cxx_name = "arrivalGlow", READ, WRITE = set_arrival_glow, NOTIFY)]
        /// Lines still to be coloured.
        #[qproperty(bool, colouring)]
        /// How long the last frame took to draw (ms).
        #[qproperty(f64, paint_ms, cxx_name = "paintMs")]
        type CodeView = super::CodeViewRust;

        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type CodeDrafts = super::CodeDraftsRust;
    }

    extern "RustQt" {
        #[cxx_name = "textValue"]
        fn text_value(self: &CodeView) -> QString;
        #[cxx_name = "setText"]
        fn set_text(self: Pin<&mut CodeView>, text: QString);
        #[cxx_name = "selectedTextValue"]
        fn selected_text_value(self: &CodeView) -> QString;
        #[cxx_name = "setPath"]
        fn set_path(self: Pin<&mut CodeView>, path: QString);
        #[cxx_name = "setReadOnly"]
        fn set_read_only(self: Pin<&mut CodeView>, on: bool);
        #[cxx_name = "setCursorPosition"]
        fn set_cursor_position(self: Pin<&mut CodeView>, pos: i32);
        #[cxx_name = "setContentX"]
        fn set_content_x(self: Pin<&mut CodeView>, x: f64);
        #[cxx_name = "setContentY"]
        fn set_content_y(self: Pin<&mut CodeView>, y: f64);
        #[cxx_name = "setTopPadding"]
        fn set_top_padding(self: Pin<&mut CodeView>, pad: f64);
        #[cxx_name = "setGutter"]
        fn set_gutter(self: Pin<&mut CodeView>, mode: i32);
        #[cxx_name = "setGutterWidth"]
        fn set_gutter_width(self: Pin<&mut CodeView>, width: f64);
        #[cxx_name = "setFontFamily"]
        fn set_font_family(self: Pin<&mut CodeView>, family: QString);
        #[cxx_name = "setFontPixelSize"]
        fn set_font_pixel_size(self: Pin<&mut CodeView>, size: f64);
        #[cxx_name = "setGutterPixelSize"]
        fn set_gutter_pixel_size(self: Pin<&mut CodeView>, size: f64);
        #[cxx_name = "setColors"]
        fn set_colors(self: Pin<&mut CodeView>, json: QString);
        #[cxx_name = "setCurrentMatch"]
        fn set_current_match(self: Pin<&mut CodeView>, i: i32);
        #[cxx_name = "setArrivalLine"]
        fn set_arrival_line(self: Pin<&mut CodeView>, line: i32);
        #[cxx_name = "setArrivalGlow"]
        fn set_arrival_glow(self: Pin<&mut CodeView>, glow: f64);
    }

    extern "RustQt" {
        /// Shows the unsaved document kept for `path`, if there is one.
        #[qinvokable]
        fn restore(self: Pin<&mut CodeView>, path: &QString) -> bool;
        /// Keeps the unsaved document (text and undo) for `restore`, as the
        /// view is about to go; false when there's nothing to keep.
        #[qinvokable]
        fn keep(self: Pin<&mut CodeView>) -> bool;
        /// Drops the edits: `text` (the file as it is), not modified.
        #[qinvokable]
        #[cxx_name = "discardEdits"]
        fn discard_edits(self: Pin<&mut CodeView>, text: &QString);
        /// What's shown is now the file on disk.
        #[qinvokable]
        #[cxx_name = "markSaved"]
        fn mark_saved(self: Pin<&mut CodeView>);
        #[qinvokable]
        fn select(self: Pin<&mut CodeView>, start: i32, end: i32);
        #[qinvokable]
        #[cxx_name = "selectAll"]
        fn select_all(self: Pin<&mut CodeView>);
        #[qinvokable]
        fn insert(self: Pin<&mut CodeView>, pos: i32, text: &QString);
        #[qinvokable]
        fn remove(self: Pin<&mut CodeView>, start: i32, end: i32);
        /// Edits in one undo step: JSON `[[start, end, text], ...]` in the
        /// text as it is now.
        #[qinvokable]
        fn apply(self: Pin<&mut CodeView>, edits: &QString) -> bool;
        #[qinvokable]
        fn undo(self: Pin<&mut CodeView>);
        #[qinvokable]
        fn redo(self: Pin<&mut CodeView>);
        #[qinvokable]
        fn copy(self: Pin<&mut CodeView>);
        #[qinvokable]
        fn cut(self: Pin<&mut CodeView>);
        #[qinvokable]
        fn paste(self: Pin<&mut CodeView>);
        #[qinvokable]
        #[cxx_name = "toggleComment"]
        fn toggle_comment(self: Pin<&mut CodeView>) -> bool;
        /// Where a position is, in the text (x from column 0, y from the top
        /// of the first line).
        #[qinvokable]
        #[cxx_name = "positionToRectangle"]
        fn position_to_rectangle(self: Pin<&mut CodeView>, pos: i32) -> QRectF;
        /// The position at a point in the text (as `positionToRectangle`).
        #[qinvokable]
        #[cxx_name = "positionAt"]
        fn position_at(self: Pin<&mut CodeView>, x: f64, y: f64) -> i32;
        /// Where line `line` (from 0) starts.
        #[qinvokable]
        #[cxx_name = "lineStart"]
        fn line_start(self: &CodeView, line: i32) -> i32;
        #[qinvokable]
        #[cxx_name = "lineOf"]
        fn line_of(self: &CodeView, pos: i32) -> i32;
        /// Scrolls a position a third of the way down unless it's in view
        /// (`always`: even then).
        #[qinvokable]
        fn reveal(self: Pin<&mut CodeView>, pos: i32, always: bool);
        /// Finds `query` (JSON of where, at most `limit`), marking each.
        #[qinvokable]
        #[cxx_name = "findAll"]
        fn find_all(self: Pin<&mut CodeView>, query: &QString, match_case: bool, limit: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "clearFind"]
        fn clear_find(self: Pin<&mut CodeView>);
        /// How many matches are marked on screen.
        #[qinvokable]
        #[cxx_name = "visibleMatches"]
        fn visible_matches(self: &CodeView) -> i32;
        /// The bracket by the cursor and its partner, marked: JSON `[a, b]`
        /// or `[]`.
        #[qinvokable]
        #[cxx_name = "bracketPair"]
        fn bracket_pair(self: Pin<&mut CodeView>) -> QString;
        /// A diff's rows: five numbers a row, comma-separated: kind, side,
        /// number, from, to (see harlo_code::view::Row).
        #[qinvokable]
        #[cxx_name = "setRows"]
        fn set_rows(self: Pin<&mut CodeView>, list: &QString);
        /// Line `line` (from 0) has its colours (for the self-tests).
        #[qinvokable]
        #[cxx_name = "lineColoured"]
        fn line_coloured(self: &CodeView, line: i32) -> bool;
        /// The row (line) at item y.
        #[qinvokable]
        #[cxx_name = "rowAt"]
        fn row_at(self: &CodeView, y: f64) -> i32;
    }

    unsafe extern "RustQt" {
        /// A right click at (x, y): the view's menu.
        #[qsignal]
        #[cxx_name = "contextMenuRequested"]
        fn context_menu_requested(self: Pin<&mut CodeView>, x: f64, y: f64);
    }

    unsafe extern "RustQt" {
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut CodeView>, painter: *mut QPainter);
        #[cxx_override]
        #[cxx_name = "keyHook"]
        fn key_hook(self: Pin<&mut CodeView>, key: i32, modifiers: i32, text: &QString, press: bool) -> bool;
        #[cxx_override]
        #[cxx_name = "pointerHook"]
        fn pointer_hook(self: Pin<&mut CodeView>, kind: i32, x: f64, y: f64, button: i32, modifiers: i32);
        #[cxx_override]
        #[cxx_name = "wheelHook"]
        fn wheel_hook(self: Pin<&mut CodeView>, dx: f64, dy: f64, modifiers: i32, pixels: bool) -> bool;
        #[cxx_override]
        #[cxx_name = "inputHook"]
        fn input_hook(self: Pin<&mut CodeView>, commit: &QString, preedit: &QString, replace_start: i32, replace_length: i32);
        #[cxx_override]
        #[cxx_name = "focusHook"]
        fn focus_hook(self: Pin<&mut CodeView>, focused: bool);
        #[cxx_override]
        #[cxx_name = "sizeHook"]
        fn size_hook(self: Pin<&mut CodeView>);
        #[cxx_override]
        #[cxx_name = "timerHook"]
        fn timer_hook(self: Pin<&mut CodeView>, id: i32);
        #[cxx_override]
        #[cxx_name = "claimHook"]
        fn claim_hook(self: Pin<&mut CodeView>, key: i32, modifiers: i32, text: &QString) -> bool;
        #[cxx_override]
        fn editable(self: &CodeView) -> bool;
        #[cxx_override]
        #[cxx_name = "cursorX"]
        fn cursor_x(self: &CodeView) -> f64;
        #[cxx_override]
        #[cxx_name = "cursorY"]
        fn cursor_y(self: &CodeView) -> f64;
        #[cxx_override]
        #[cxx_name = "cursorHeight"]
        fn cursor_height(self: &CodeView) -> f64;
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn update(self: Pin<&mut CodeView>);
        #[inherit]
        fn width(self: &CodeView) -> f64;
        #[inherit]
        fn height(self: &CodeView) -> f64;
        #[inherit]
        #[cxx_name = "pixelRatio"]
        fn pixel_ratio(self: &CodeView) -> f64;
        #[inherit]
        unsafe fn blit(self: &CodeView, painter: *mut QPainter, data: *const u8, width: i32, height: i32);
        #[inherit]
        #[cxx_name = "startTicks"]
        fn start_ticks(self: Pin<&mut CodeView>, ms: i32) -> i32;
        #[inherit]
        #[cxx_name = "stopTicks"]
        fn stop_ticks(self: Pin<&mut CodeView>, id: i32);
        #[inherit]
        #[cxx_name = "inputMethodMoved"]
        fn input_method_moved(self: &CodeView);
    }

    extern "RustQt" {
        /// Drops the unsaved document kept for `path`.
        #[qinvokable]
        fn discard(self: &CodeDrafts, path: &QString);
        /// The kept document for `path` was saved.
        #[qinvokable]
        #[cxx_name = "markSaved"]
        fn mark_saved(self: &CodeDrafts, path: &QString);
    }

    impl cxx_qt::Constructor<()> for CodeView {}
}

use std::cell::RefCell;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QRectF, QString};
use harlo_code::doc::{Doc, Kind};
use harlo_code::view::{Gutter, Palette, Rgba, Row, View};

/// Unsaved documents by path, kept while their files aren't shown.
static KEPT: LazyLock<Mutex<HashMap<String, Doc>>> = LazyLock::new(Default::default);

fn kept() -> std::sync::MutexGuard<'static, HashMap<String, Doc>> {
    KEPT.lock().unwrap_or_else(|e| e.into_inner())
}

// Qt's key codes and modifiers.
const KEY_ESCAPE: i32 = 0x0100_0000;
const KEY_TAB: i32 = 0x0100_0001;
const KEY_BACKTAB: i32 = 0x0100_0002;
const KEY_BACKSPACE: i32 = 0x0100_0003;
const KEY_RETURN: i32 = 0x0100_0004;
const KEY_ENTER: i32 = 0x0100_0005;
const KEY_INSERT: i32 = 0x0100_0006;
const KEY_DELETE: i32 = 0x0100_0007;
const KEY_HOME: i32 = 0x0100_0010;
const KEY_END: i32 = 0x0100_0011;
const KEY_LEFT: i32 = 0x0100_0012;
const KEY_UP: i32 = 0x0100_0013;
const KEY_RIGHT: i32 = 0x0100_0014;
const KEY_DOWN: i32 = 0x0100_0015;
const KEY_PAGE_UP: i32 = 0x0100_0016;
const KEY_PAGE_DOWN: i32 = 0x0100_0017;
const KEY_A: i32 = 0x41;
const KEY_C: i32 = 0x43;
const KEY_V: i32 = 0x56;
const KEY_X: i32 = 0x58;
const KEY_Y: i32 = 0x59;
const KEY_Z: i32 = 0x5a;
const KEY_SLASH: i32 = 0x2f;
const SHIFT: i32 = 0x0200_0000;
const CTRL: i32 = 0x0400_0000;
const ALT: i32 = 0x0800_0000;
const META: i32 = 0x1000_0000;
const LEFT_BUTTON: i32 = 1;
const RIGHT_BUTTON: i32 = 2;
const MIDDLE_BUTTON: i32 = 4;

/// How a drag selects: by character, word or line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DragBy {
    None,
    Char,
    Word,
    Line,
}

pub struct CodeViewRust {
    path: QString,
    language: QString,
    read_only: bool,
    length: i32,
    line_count: i32,
    cursor_line: i32,
    cursor_position: i32,
    selection_start: i32,
    selection_end: i32,
    cursor_rectangle: QRectF,
    modified: bool,
    can_undo: bool,
    can_redo: bool,
    indent_unit: QString,
    content_x: f64,
    content_y: f64,
    content_width: f64,
    content_height: f64,
    line_height: f64,
    text_x: f64,
    top_padding: f64,
    gutter: i32,
    gutter_width: f64,
    font_family: QString,
    font_pixel_size: f64,
    gutter_pixel_size: f64,
    colors: QString,
    match_count: i32,
    current_match: i32,
    arrival_line: i32,
    arrival_glow: f64,
    colouring: bool,
    paint_ms: f64,

    view: View,
    pixels: Vec<u8>,
    text_cache: RefCell<Option<QString>>,
    /// The text revision `textChanged` last went out for.
    revision: u64,
    /// The cursor's rectangle in the item (for the input method).
    cursor_item: (f64, f64, f64),
    colour_timer: i32,
    blink_timer: i32,
    drag_timer: i32,
    drag: DragBy,
    /// What a word or line drag started on.
    drag_origin: (usize, usize),
    last_pointer: (f64, f64),
    last_press: Option<(Instant, f64, f64)>,
    clicks: u32,
}

impl Default for CodeViewRust {
    fn default() -> Self {
        let mut view = View::new();
        view.gutter = Gutter::Lines;
        Self {
            path: QString::default(),
            language: QString::default(),
            read_only: false,
            length: 0,
            line_count: 1,
            cursor_line: 0,
            cursor_position: 0,
            selection_start: 0,
            selection_end: 0,
            cursor_rectangle: QRectF::default(),
            modified: false,
            can_undo: false,
            can_redo: false,
            indent_unit: QString::from("    "),
            content_x: 0.0,
            content_y: 0.0,
            content_width: 0.0,
            content_height: 0.0,
            line_height: 17.0,
            text_x: 0.0,
            top_padding: 8.0,
            gutter: 1,
            gutter_width: 0.0,
            font_family: QString::from("JetBrains Mono"),
            font_pixel_size: 13.0,
            gutter_pixel_size: 12.0,
            colors: QString::default(),
            match_count: 0,
            current_match: -1,
            arrival_line: -1,
            arrival_glow: 0.0,
            colouring: false,
            paint_ms: 0.0,
            view,
            pixels: Vec::new(),
            text_cache: RefCell::new(None),
            revision: 0,
            cursor_item: (0.0, 0.0, 17.0),
            colour_timer: 0,
            blink_timer: 0,
            drag_timer: 0,
            drag: DragBy::None,
            drag_origin: (0, 0),
            last_pointer: (0.0, 0.0),
            last_press: None,
            clicks: 0,
        }
    }
}

impl cxx_qt::Initialize for qobject::CodeView {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh(true);
    }
}

/// "#rrggbb" or "#aarrggbb" (as QML writes a colour).
fn parse_color(s: &str) -> Option<Rgba> {
    let h = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some(Rgba::new(byte(0)?, byte(2)?, byte(4)?, 255)),
        8 => Some(Rgba::new(byte(2)?, byte(4)?, byte(6)?, byte(0)?)),
        _ => None,
    }
}

/// The palette from JSON: `{"foreground": "#…", …, "classes": [s1…s11]}`.
fn palette_from_json(json: &str, base: &Palette) -> Palette {
    let mut p = base.clone();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return p };
    let get = |k: &str| v.get(k).and_then(|c| c.as_str()).and_then(parse_color);
    macro_rules! take {
        ($($field:ident = $key:literal),* $(,)?) => { $( if let Some(c) = get($key) { p.$field = c; } )* };
    }
    take!(
        foreground = "foreground",
        background = "background",
        gutter_text = "gutterText",
        border = "border",
        selection = "selection",
        find = "find",
        find_current = "findCurrent",
        find_edge = "findEdge",
        bracket = "bracket",
        bracket_edge = "bracketEdge",
        arrival = "arrival",
        arrival_text = "arrivalText",
        cursor = "cursor",
        removed = "removed",
        removed_strong = "removedStrong",
        removed_text = "removedText",
        added = "added",
        added_strong = "addedStrong",
        added_text = "addedText",
    );
    p.classes[0] = p.foreground;
    if let Some(list) = v.get("classes").and_then(|c| c.as_array()) {
        for (i, c) in list.iter().enumerate().take(11) {
            p.classes[i + 1] = c.as_str().and_then(parse_color).unwrap_or(p.foreground);
        }
    }
    p
}

// Colouring in the background: every view's slices share one budget, so a
// few views colouring at once (a diff's two sides, the editor) still leave
// the window most of each frame.
const COLOUR_TICK_MS: i32 = 4;
const COLOUR_WINDOW: Duration = Duration::from_millis(16);
const COLOUR_PER_WINDOW: Duration = Duration::from_millis(5);

thread_local! {
    static COLOURED: std::cell::Cell<(Option<Instant>, Duration)> = const { std::cell::Cell::new((None, Duration::ZERO)) };
}

/// What's left of this window's colouring time.
fn colour_allowance() -> Duration {
    COLOURED.with(|c| {
        let (start, spent) = c.get();
        let now = Instant::now();
        match start {
            Some(s) if now.duration_since(s) < COLOUR_WINDOW => COLOUR_PER_WINDOW.saturating_sub(spent),
            _ => {
                c.set((Some(now), Duration::ZERO));
                COLOUR_PER_WINDOW
            }
        }
    })
}

fn colour_spent(d: Duration) {
    COLOURED.with(|c| {
        let (start, spent) = c.get();
        c.set((start, spent + d));
    });
}

fn to_pos(p: i32) -> usize {
    p.max(0) as usize
}

impl qobject::CodeView {
    // ---- after a change: the properties, colours, the frame ----------------

    /// Brings the properties up to date (`edited`: the text changed too).
    fn refresh(mut self: Pin<&mut Self>, edited: bool) {
        let revision = {
            let mut rust = self.as_mut().rust_mut();
            rust.view.sync();
            rust.view.doc.revision
        };
        let text_changed = edited && revision != self.revision;
        if text_changed {
            *self.text_cache.borrow_mut() = None;
            self.as_mut().rust_mut().revision = revision;
            self.as_mut().colour_for(Duration::from_millis(1));
        }
        let (len, lines, line, cursor, sel, modified, undo, redo, cw, ch, lh, tx, gw, rect, item) = {
            let mut rust = self.as_mut().rust_mut();
            let v = &mut rust.view;
            let (a, b) = v.doc.selection();
            let cursor = v.doc.cursor;
            let (x, y, _, h) = v.position_rect(cursor);
            let item = ((v.text_x() + x - v.scroll_x) as f64, (v.top_pad + y - v.scroll_y) as f64, h as f64);
            (
                v.doc.text.len() as i32,
                v.doc.text.line_count() as i32,
                v.doc.text.line_of(cursor) as i32,
                cursor as i32,
                (a as i32, b as i32),
                v.doc.modified(),
                v.doc.can_undo(),
                v.doc.can_redo(),
                v.content_width() as f64,
                v.content_height() as f64,
                v.line_height() as f64,
                v.text_x() as f64,
                v.gutter_width() as f64,
                QRectF::new(x as f64, y as f64, 1.0, h as f64),
                item,
            )
        };
        self.as_mut().rust_mut().cursor_item = item;
        let selection_moved = sel != (self.selection_start, self.selection_end);
        if text_changed {
            self.as_mut().text_changed();
        }
        self.as_mut().set_length(len);
        self.as_mut().set_line_count(lines);
        self.as_mut().set_content_width(cw);
        self.as_mut().set_content_height(ch);
        self.as_mut().set_line_height(lh);
        self.as_mut().set_text_x(tx);
        if self.gutter != 2 {
            self.as_mut().rust_mut().gutter_width = gw;
            self.as_mut().gutter_width_changed();
        }
        self.as_mut().set_selection_start(sel.0);
        self.as_mut().set_selection_end(sel.1);
        if cursor != self.cursor_position {
            self.as_mut().rust_mut().cursor_position = cursor;
            self.as_mut().cursor_position_changed();
        }
        self.as_mut().set_cursor_line(line);
        if rect != self.cursor_rectangle {
            self.as_mut().set_cursor_rectangle(rect);
        }
        if selection_moved || text_changed {
            self.as_mut().selected_text_changed();
        }
        self.as_mut().set_modified(modified);
        self.as_mut().set_can_undo(undo);
        self.as_mut().set_can_redo(redo);
        self.as_mut().sync_scroll();
        self.input_method_moved();
        self.update();
    }

    /// The scroll position, clamped, out to QML.
    fn sync_scroll(mut self: Pin<&mut Self>) {
        let (x, y) = {
            let mut rust = self.as_mut().rust_mut();
            let v = &mut rust.view;
            let (sx, sy) = (v.scroll_x, v.scroll_y);
            v.set_scroll(sx, sy);
            (v.scroll_x as f64, v.scroll_y as f64)
        };
        if x != self.content_x {
            self.as_mut().rust_mut().content_x = x;
            self.as_mut().content_x_changed();
        }
        if y != self.content_y {
            self.as_mut().rust_mut().content_y = y;
            self.as_mut().content_y_changed();
        }
    }

    /// Colours lines for up to `budget`; the rest in slices on a timer.
    fn colour_for(mut self: Pin<&mut Self>, budget: Duration) {
        let t = Instant::now();
        let (shown, busy) = {
            let mut rust = self.as_mut().rust_mut();
            let shown = rust.view.colour(t + budget);
            (shown, rust.view.colouring_busy())
        };
        colour_spent(t.elapsed());
        if shown {
            self.as_mut().update();
        }
        if busy && self.colour_timer == 0 {
            let id = self.as_mut().start_ticks(COLOUR_TICK_MS);
            self.as_mut().rust_mut().colour_timer = id;
        } else if !busy && self.colour_timer != 0 {
            let id = self.colour_timer;
            self.as_mut().stop_ticks(id);
            self.as_mut().rust_mut().colour_timer = 0;
        }
        self.as_mut().set_colouring(busy);
    }

    /// The cursor shows (again) and blinks.
    fn wake_cursor(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().view.cursor_on = true;
        let id = self.blink_timer;
        if id != 0 {
            self.as_mut().stop_ticks(id);
            self.as_mut().rust_mut().blink_timer = 0;
        }
        if self.view.focused && !self.read_only {
            let id = self.as_mut().start_ticks(530);
            self.as_mut().rust_mut().blink_timer = id;
        }
    }

    /// The cursor kept in view after it moved by keyboard.
    fn follow_cursor(mut self: Pin<&mut Self>) {
        {
            let mut rust = self.as_mut().rust_mut();
            let c = rust.view.doc.cursor;
            rust.view.reveal(c, 20.0);
        }
        self.wake_cursor();
    }

    fn doc(self: Pin<&mut Self>) -> &mut Doc {
        // SAFETY: the document isn't moved out of its pinned object.
        unsafe { &mut self.rust_mut().get_unchecked_mut().view.doc }
    }

    // ---- properties ----------------------------------------------------------

    pub fn text_value(&self) -> QString {
        let mut cache = self.text_cache.borrow_mut();
        cache.get_or_insert_with(|| QString::from(&self.view.doc.text.to_string())).clone()
    }

    pub fn selected_text_value(&self) -> QString {
        QString::from(&self.view.doc.selected_text())
    }

    /// A new text: nothing to undo, not modified.
    pub fn set_text(mut self: Pin<&mut Self>, text: QString) {
        let s = String::from(&text);
        if self.revision != 0 && s == self.view.doc.text.to_string() && !self.view.doc.modified() {
            return;
        }
        self.as_mut().load_text(&s);
    }

    fn load_text(mut self: Pin<&mut Self>, s: &str) {
        {
            let mut rust = self.as_mut().rust_mut();
            let v = &mut rust.view;
            v.doc.load(s);
            v.scroll_x = 0.0;
            v.scroll_y = 0.0;
            v.goal_x = None;
            v.matches.clear();
        }
        let unit = QString::from(&self.view.doc.unit);
        self.as_mut().set_indent_unit(unit);
        self.as_mut().set_match_count(0);
        // (Not an edit: textChanged, but nothing for the drafts.)
        self.as_mut().rust_mut().revision = 0;
        self.refresh(true);
    }

    pub fn set_path(mut self: Pin<&mut Self>, path: QString) {
        if path == self.path {
            return;
        }
        let p = String::from(&path);
        self.as_mut().rust_mut().view.set_path(&p);
        let language = QString::from(self.view.colouring.language());
        self.as_mut().rust_mut().path = path;
        self.as_mut().path_changed();
        self.as_mut().set_language(language);
        self.as_mut().colour_for(Duration::from_millis(1));
        self.update();
    }

    pub fn set_read_only(mut self: Pin<&mut Self>, on: bool) {
        if on == self.read_only {
            return;
        }
        self.as_mut().rust_mut().read_only = on;
        self.as_mut().rust_mut().view.doc.read_only = on;
        self.as_mut().read_only_changed();
        self.as_mut().wake_cursor();
        self.update();
    }

    pub fn set_cursor_position(mut self: Pin<&mut Self>, pos: i32) {
        self.as_mut().doc().set_cursor(to_pos(pos), false);
        self.as_mut().rust_mut().view.goal_x = None;
        self.as_mut().follow_cursor();
        self.refresh(false);
    }

    pub fn set_content_x(mut self: Pin<&mut Self>, x: f64) {
        {
            let mut rust = self.as_mut().rust_mut();
            let y = rust.view.scroll_y;
            rust.view.set_scroll(x as f32, y);
        }
        self.as_mut().sync_scroll();
        self.update();
    }

    pub fn set_content_y(mut self: Pin<&mut Self>, y: f64) {
        {
            let mut rust = self.as_mut().rust_mut();
            let x = rust.view.scroll_x;
            rust.view.set_scroll(x, y as f32);
        }
        self.as_mut().sync_scroll();
        self.update();
    }

    pub fn set_top_padding(mut self: Pin<&mut Self>, pad: f64) {
        if pad == self.top_padding {
            return;
        }
        self.as_mut().rust_mut().top_padding = pad;
        self.as_mut().rust_mut().view.top_pad = pad as f32;
        self.as_mut().top_padding_changed();
        self.refresh(false);
    }

    pub fn set_gutter(mut self: Pin<&mut Self>, mode: i32) {
        if mode == self.gutter {
            return;
        }
        self.as_mut().rust_mut().gutter = mode;
        self.as_mut().rust_mut().view.gutter = match mode {
            0 => Gutter::None,
            2 => Gutter::Rows,
            _ => Gutter::Lines,
        };
        self.as_mut().gutter_changed();
        self.refresh(false);
    }

    pub fn set_gutter_width(mut self: Pin<&mut Self>, width: f64) {
        if width == self.gutter_width {
            return;
        }
        self.as_mut().rust_mut().gutter_width = width;
        self.as_mut().rust_mut().view.rows_gutter_width = width as f32;
        self.as_mut().gutter_width_changed();
        self.refresh(false);
    }

    fn apply_font(mut self: Pin<&mut Self>) {
        let family = String::from(&self.font_family);
        let (size, gutter) = (self.font_pixel_size as f32, self.gutter_pixel_size as f32);
        self.as_mut().rust_mut().view.set_font(&family, size, gutter);
        self.refresh(false);
    }

    pub fn set_font_family(mut self: Pin<&mut Self>, family: QString) {
        if family == self.font_family {
            return;
        }
        self.as_mut().rust_mut().font_family = family;
        self.as_mut().font_family_changed();
        self.apply_font();
    }

    pub fn set_font_pixel_size(mut self: Pin<&mut Self>, size: f64) {
        if size == self.font_pixel_size || size <= 0.0 {
            return;
        }
        self.as_mut().rust_mut().font_pixel_size = size;
        self.as_mut().font_pixel_size_changed();
        self.apply_font();
    }

    pub fn set_gutter_pixel_size(mut self: Pin<&mut Self>, size: f64) {
        if size == self.gutter_pixel_size || size <= 0.0 {
            return;
        }
        self.as_mut().rust_mut().gutter_pixel_size = size;
        self.as_mut().gutter_pixel_size_changed();
        self.apply_font();
    }

    pub fn set_colors(mut self: Pin<&mut Self>, json: QString) {
        if json == self.colors {
            return;
        }
        let palette = palette_from_json(&String::from(&json), &self.view.palette);
        self.as_mut().rust_mut().view.palette = palette;
        self.as_mut().rust_mut().colors = json;
        self.as_mut().colors_changed();
        self.update();
    }

    pub fn set_current_match(mut self: Pin<&mut Self>, i: i32) {
        if i == self.current_match {
            return;
        }
        self.as_mut().rust_mut().current_match = i;
        self.as_mut().rust_mut().view.current_match = i;
        self.as_mut().current_match_changed();
        self.update();
    }

    pub fn set_arrival_line(mut self: Pin<&mut Self>, line: i32) {
        if line == self.arrival_line {
            return;
        }
        self.as_mut().rust_mut().arrival_line = line;
        self.as_mut().rust_mut().view.arrival = (line >= 0).then_some(line as usize);
        self.as_mut().arrival_line_changed();
        self.update();
    }

    pub fn set_arrival_glow(mut self: Pin<&mut Self>, glow: f64) {
        if glow == self.arrival_glow {
            return;
        }
        self.as_mut().rust_mut().arrival_glow = glow;
        self.as_mut().rust_mut().view.arrival_glow = glow as f32;
        self.as_mut().arrival_glow_changed();
        self.update();
    }

    // ---- functions -------------------------------------------------------------

    pub fn restore(mut self: Pin<&mut Self>, path: &QString) -> bool {
        let Some(doc) = kept().remove(&String::from(path)) else { return false };
        self.as_mut().rust_mut().view.set_doc(doc);
        let unit = QString::from(&self.view.doc.unit);
        self.as_mut().set_indent_unit(unit);
        self.as_mut().rust_mut().revision = 0;
        self.as_mut().colour_for(Duration::from_millis(1));
        self.refresh(true);
        true
    }

    pub fn keep(mut self: Pin<&mut Self>) -> bool {
        let path = String::from(&self.path);
        if path.is_empty() || self.read_only || !self.view.doc.modified() {
            return false;
        }
        let doc = std::mem::take(self.as_mut().doc());
        kept().insert(path, doc);
        // (What's left is empty, and the properties as they were: the view
        // is going, and its owner mustn't see the edits as dropped.)
        self.as_mut().rust_mut().view.set_doc(Doc::default());
        self.update();
        true
    }

    pub fn discard_edits(mut self: Pin<&mut Self>, text: &QString) {
        let path = String::from(&self.path);
        kept().remove(&path);
        self.as_mut().load_text(&String::from(text));
    }

    pub fn mark_saved(mut self: Pin<&mut Self>) {
        self.as_mut().doc().mark_saved();
        self.refresh(false);
    }

    pub fn select(mut self: Pin<&mut Self>, start: i32, end: i32) {
        self.as_mut().doc().select(to_pos(start), to_pos(end));
        self.as_mut().rust_mut().view.goal_x = None;
        self.as_mut().follow_cursor();
        self.refresh(false);
    }

    pub fn select_all(mut self: Pin<&mut Self>) {
        self.as_mut().doc().select_all();
        self.refresh(false);
    }

    /// An edit (`f`) and what follows it: the properties, the cursor shown.
    fn edit(mut self: Pin<&mut Self>, f: impl FnOnce(&mut Doc) -> bool) -> bool {
        let done = f(self.as_mut().doc());
        if done {
            self.as_mut().rust_mut().view.goal_x = None;
            self.as_mut().follow_cursor();
            self.refresh(true);
        }
        done
    }

    pub fn insert(self: Pin<&mut Self>, pos: i32, text: &QString) {
        let s = String::from(text).replace("\r\n", "\n");
        let p = to_pos(pos);
        self.edit(|d| d.apply(&[(p, p, s.clone())], Kind::Other, Some((p + s.encode_utf16().count(), p + s.encode_utf16().count()))));
    }

    pub fn remove(self: Pin<&mut Self>, start: i32, end: i32) {
        let (a, b) = (to_pos(start), to_pos(end));
        self.edit(|d| d.apply(&[(a, b, String::new())], Kind::Other, Some((a.min(b), a.min(b)))));
    }

    pub fn apply(self: Pin<&mut Self>, edits: &QString) -> bool {
        let Ok(v) = serde_json::from_str::<Vec<(i64, i64, String)>>(&String::from(edits)) else { return false };
        let list: Vec<(usize, usize, String)> = v.into_iter().map(|(a, b, s)| (a.max(0) as usize, b.max(0) as usize, s)).collect();
        self.edit(|d| d.apply(&list, Kind::Other, None))
    }

    pub fn undo(self: Pin<&mut Self>) {
        self.edit(Doc::undo);
    }

    pub fn redo(self: Pin<&mut Self>) {
        self.edit(Doc::redo);
    }

    pub fn copy(self: Pin<&mut Self>) {
        if self.view.doc.has_selection() {
            qobject::set_clipboard_text(&QString::from(&self.view.doc.selected_text()), false);
        }
    }

    pub fn cut(mut self: Pin<&mut Self>) {
        if self.read_only || !self.view.doc.has_selection() {
            return;
        }
        self.as_mut().copy();
        self.edit(Doc::backspace);
    }

    pub fn paste(self: Pin<&mut Self>) {
        let text = String::from(&qobject::clipboard_text(false)).replace("\r\n", "\n");
        if !text.is_empty() {
            self.edit(|d| d.type_text(&text));
        }
    }

    pub fn toggle_comment(self: Pin<&mut Self>) -> bool {
        self.edit(Doc::toggle_comment)
    }

    pub fn position_to_rectangle(mut self: Pin<&mut Self>, pos: i32) -> QRectF {
        let (x, y, w, h) = self.as_mut().rust_mut().view.position_rect(to_pos(pos));
        QRectF::new(x as f64, y as f64, w as f64, h as f64)
    }

    pub fn position_at(mut self: Pin<&mut Self>, x: f64, y: f64) -> i32 {
        self.as_mut().rust_mut().view.pos_at_content(x as f32, y as f32) as i32
    }

    pub fn line_start(&self, line: i32) -> i32 {
        self.view.doc.text.line_start(to_pos(line)) as i32
    }

    pub fn line_of(&self, pos: i32) -> i32 {
        self.view.doc.text.line_of(to_pos(pos)) as i32
    }

    pub fn reveal(mut self: Pin<&mut Self>, pos: i32, always: bool) {
        self.as_mut().rust_mut().view.reveal_third(to_pos(pos), always);
        self.as_mut().sync_scroll();
        self.update();
    }

    pub fn find_all(mut self: Pin<&mut Self>, query: &QString, match_case: bool, limit: i32) -> QString {
        let q = String::from(query);
        let found = self.view.doc.find_all(&q, match_case, limit.max(1) as usize);
        let json = serde_json::to_string(&found).unwrap_or_else(|_| "[]".into());
        let n = found.len() as i32;
        {
            let mut rust = self.as_mut().rust_mut();
            rust.view.matches = found;
            rust.view.match_len = q.encode_utf16().count();
        }
        self.as_mut().set_match_count(n);
        self.update();
        QString::from(&json)
    }

    pub fn clear_find(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().view.matches.clear();
        self.as_mut().set_match_count(0);
        self.as_mut().set_current_match(-1);
        self.update();
    }

    pub fn visible_matches(&self) -> i32 {
        self.view.visible_matches() as i32
    }

    pub fn bracket_pair(mut self: Pin<&mut Self>) -> QString {
        let pair = self.as_mut().rust_mut().view.bracket_pair();
        QString::from(&match pair {
            Some((a, b)) => format!("[{a},{b}]"),
            None => "[]".to_owned(),
        })
    }

    pub fn set_rows(mut self: Pin<&mut Self>, list: &QString) {
        let rows = Row::parse_list(&String::from(list));
        self.as_mut().rust_mut().view.rows = rows;
        self.update();
    }

    pub fn line_coloured(&self, line: i32) -> bool {
        self.view.line_coloured(to_pos(line))
    }

    pub fn row_at(&self, y: f64) -> i32 {
        let v = &self.view;
        ((y as f32 - v.top_pad + v.scroll_y) / v.line_height()).floor() as i32
    }

    // ---- drawing -----------------------------------------------------------------

    unsafe fn paint(mut self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        let t = Instant::now();
        let dpr = self.pixel_ratio();
        let w = (self.width() * dpr).round().max(1.0) as usize;
        let h = (self.height() * dpr).round().max(1.0) as usize;
        {
            let mut rust = self.as_mut().rust_mut();
            let rust = &mut *rust;
            rust.view.render(&mut rust.pixels, w, h, dpr as f32);
        }
        unsafe { self.blit(painter, self.pixels.as_ptr(), w as i32, h as i32) };
        // (Read by the self-tests; no signal from the render thread.)
        self.as_mut().rust_mut().paint_ms = t.elapsed().as_secs_f64() * 1000.0;
    }

    fn size_hook(mut self: Pin<&mut Self>) {
        let (w, h) = (self.width() as f32, self.height() as f32);
        {
            let mut rust = self.as_mut().rust_mut();
            rust.view.width = w;
            rust.view.height = h;
        }
        self.refresh(false);
    }

    fn timer_hook(mut self: Pin<&mut Self>, id: i32) {
        if id == self.colour_timer {
            let budget = colour_allowance();
            if budget >= Duration::from_millis(1) {
                self.colour_for(budget);
            }
        } else if id == self.blink_timer {
            let on = !self.view.cursor_on;
            self.as_mut().rust_mut().view.cursor_on = on;
            self.update();
        } else if id == self.drag_timer {
            self.drag_scroll();
        }
    }

    fn focus_hook(mut self: Pin<&mut Self>, focused: bool) {
        self.as_mut().rust_mut().view.focused = focused;
        if !focused {
            self.as_mut().rust_mut().view.preedit.clear();
        }
        self.as_mut().wake_cursor();
        self.update();
    }

    fn editable(&self) -> bool {
        !self.read_only
    }

    fn cursor_x(&self) -> f64 {
        self.cursor_item.0
    }

    fn cursor_y(&self) -> f64 {
        self.cursor_item.1
    }

    fn cursor_height(&self) -> f64 {
        self.cursor_item.2
    }

    // ---- keys ----------------------------------------------------------------------

    /// Keys the view handles, kept from the app's shortcuts while it has the
    /// focus.
    fn claim_hook(self: Pin<&mut Self>, key: i32, modifiers: i32, text: &QString) -> bool {
        let ctrl = modifiers & CTRL != 0;
        let alt = modifiers & ALT != 0;
        let editable = !self.read_only;
        if alt || modifiers & META != 0 {
            return false;
        }
        match key {
            KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN | KEY_HOME | KEY_END | KEY_PAGE_UP | KEY_PAGE_DOWN => true,
            KEY_BACKSPACE | KEY_DELETE | KEY_RETURN | KEY_ENTER | KEY_TAB | KEY_BACKTAB => editable,
            KEY_A | KEY_C if ctrl => true,
            KEY_X | KEY_V | KEY_Z | KEY_Y if ctrl => editable,
            KEY_SLASH if ctrl => editable && !harlo_code::lang::comment_for(&String::from(&self.path)).is_empty(),
            _ => editable && !ctrl && !String::from(text).chars().any(char::is_control) && !text.is_empty(),
        }
    }

    fn key_hook(mut self: Pin<&mut Self>, key: i32, modifiers: i32, text: &QString, press: bool) -> bool {
        if !press {
            return false;
        }
        let shift = modifiers & SHIFT != 0;
        let ctrl = modifiers & CTRL != 0;
        let alt = modifiers & ALT != 0;
        if modifiers & META != 0 || (alt && !ctrl) {
            // (Alt+arrows go to the diff, Alt+letters to menus.)
            return false;
        }
        let ro = self.read_only;
        // Moving the cursor (Shift: selecting).
        let motion = |this: Pin<&mut Self>, to: usize, keep_goal: bool| {
            let mut this = this;
            this.as_mut().doc().set_cursor(to, shift);
            if !keep_goal {
                this.as_mut().rust_mut().view.goal_x = None;
            }
            this.as_mut().follow_cursor();
            this.refresh(false);
            true
        };
        let (a, b) = self.view.doc.selection();
        let cursor = self.view.doc.cursor;
        let has_sel = a != b;
        match key {
            KEY_LEFT => {
                let to = if ctrl {
                    self.view.doc.word_left(cursor)
                } else if has_sel && !shift {
                    a
                } else {
                    self.view.doc.prev_grapheme(cursor)
                };
                motion(self, to, false)
            }
            KEY_RIGHT => {
                let to = if ctrl {
                    self.view.doc.word_right(cursor)
                } else if has_sel && !shift {
                    b
                } else {
                    self.view.doc.next_grapheme(cursor)
                };
                motion(self, to, false)
            }
            KEY_UP | KEY_DOWN if ctrl => {
                let lh = self.view.line_height() as f64;
                let y = self.content_y + if key == KEY_UP { -lh } else { lh };
                self.set_content_y(y);
                true
            }
            KEY_UP | KEY_DOWN => {
                let to = self.as_mut().rust_mut().view.pos_vertical(cursor, if key == KEY_UP { -1 } else { 1 });
                motion(self, to, true)
            }
            KEY_PAGE_UP | KEY_PAGE_DOWN => {
                let (lines, lh) = {
                    let v = &self.view;
                    ((((v.height - v.top_pad) / v.line_height()).floor() as isize - 1).max(1), v.line_height())
                };
                let lines = if key == KEY_PAGE_UP { -lines } else { lines };
                let to = self.as_mut().rust_mut().view.pos_vertical(cursor, lines);
                {
                    let mut rust = self.as_mut().rust_mut();
                    let (sx, sy) = (rust.view.scroll_x, rust.view.scroll_y);
                    rust.view.set_scroll(sx, sy + lines as f32 * lh);
                }
                motion(self, to, true)
            }
            KEY_HOME => {
                let to = if ctrl { 0 } else { self.view.doc.home(cursor) };
                motion(self, to, false)
            }
            KEY_END => {
                let to = if ctrl { self.view.doc.text.len() } else { self.view.doc.line_end(cursor) };
                motion(self, to, false)
            }
            KEY_A if ctrl => {
                self.select_all();
                true
            }
            KEY_C if ctrl => {
                self.copy();
                true
            }
            KEY_INSERT if ctrl => {
                self.copy();
                true
            }
            _ if ro => false,
            KEY_X if ctrl => {
                self.cut();
                true
            }
            KEY_V if ctrl => {
                self.paste();
                true
            }
            KEY_INSERT if shift => {
                self.paste();
                true
            }
            KEY_Z if ctrl && shift => {
                self.redo();
                true
            }
            KEY_Z if ctrl => {
                self.undo();
                true
            }
            KEY_Y if ctrl => {
                self.redo();
                true
            }
            KEY_SLASH if ctrl => {
                self.toggle_comment();
                true
            }
            KEY_RETURN | KEY_ENTER if !ctrl => {
                self.edit(Doc::newline);
                true
            }
            KEY_TAB if !ctrl => {
                self.edit(Doc::tab);
                true
            }
            KEY_BACKTAB if !ctrl => {
                self.edit(|d| d.shift_lines(false));
                true
            }
            KEY_BACKSPACE => {
                if ctrl {
                    self.edit(|d| d.delete_word(true));
                } else {
                    self.edit(Doc::backspace);
                }
                true
            }
            KEY_DELETE if shift && !ctrl => {
                self.cut();
                true
            }
            KEY_DELETE => {
                if ctrl {
                    self.edit(|d| d.delete_word(false));
                } else {
                    self.edit(Doc::delete);
                }
                true
            }
            KEY_ESCAPE => false,
            _ => {
                let t = String::from(text);
                // (Ctrl+Alt is AltGr on some keyboards: its characters type.)
                if t.is_empty() || t.chars().any(char::is_control) || (ctrl && !alt) {
                    return false;
                }
                let mut chars = t.chars();
                if let (Some(c @ ('}' | ']' | ')')), None) = (chars.next(), chars.next())
                    && self.as_mut().edit(|d| d.closer(c))
                {
                    return true;
                }
                self.edit(|d| d.type_text(&t));
                true
            }
        }
    }

    // ---- the mouse ---------------------------------------------------------------

    fn pointer_hook(mut self: Pin<&mut Self>, kind: i32, x: f64, y: f64, button: i32, modifiers: i32) {
        let shift = modifiers & SHIFT != 0;
        self.as_mut().rust_mut().last_pointer = (x, y);
        match kind {
            // Press, or a double click (a press counted below).
            0 | 3 => {
                let now = Instant::now();
                if kind == 3 && self.last_press.is_some_and(|(t, _, _)| now.duration_since(t) < Duration::from_millis(40)) {
                    return;
                }
                let pos = self.as_mut().rust_mut().view.pos_at(x as f32, y as f32);
                if button == MIDDLE_BUTTON {
                    if !self.read_only {
                        let text = String::from(&qobject::clipboard_text(true));
                        if !text.is_empty() {
                            self.as_mut().doc().set_cursor(pos, false);
                            self.edit(|d| d.type_text(&text));
                        }
                    }
                    return;
                }
                if button == RIGHT_BUTTON {
                    let (a, b) = self.view.doc.selection();
                    if pos < a || pos > b || a == b {
                        self.as_mut().doc().set_cursor(pos, false);
                        self.as_mut().refresh(false);
                    }
                    self.context_menu_requested(x, y);
                    return;
                }
                if button != LEFT_BUTTON {
                    return;
                }
                let repeat = self.last_press.is_some_and(|(t, px, py)| {
                    now.duration_since(t) < Duration::from_millis(450) && (px - x).abs() < 5.0 && (py - y).abs() < 5.0
                });
                let clicks = if repeat { self.clicks % 3 + 1 } else { 1 };
                {
                    let mut rust = self.as_mut().rust_mut();
                    rust.clicks = clicks;
                    rust.last_press = Some((now, x, y));
                    rust.view.goal_x = None;
                }
                match clicks {
                    1 => {
                        self.as_mut().doc().set_cursor(pos, shift);
                        self.as_mut().rust_mut().drag = DragBy::Char;
                    }
                    2 => {
                        let (a, b) = self.view.doc.word_at(pos);
                        self.as_mut().doc().select(a, b);
                        let mut rust = self.as_mut().rust_mut();
                        rust.drag = DragBy::Word;
                        rust.drag_origin = (a, b);
                    }
                    _ => {
                        let (a, b) = self.view.doc.line_at(pos);
                        self.as_mut().doc().select(a, b);
                        let mut rust = self.as_mut().rust_mut();
                        rust.drag = DragBy::Line;
                        rust.drag_origin = (a, b);
                    }
                }
                self.as_mut().wake_cursor();
                self.refresh(false);
            }
            1 => {
                if self.drag == DragBy::None {
                    return;
                }
                self.as_mut().drag_to(x, y);
                let outside = y < self.view.top_pad as f64 || y > self.height();
                if outside && self.drag_timer == 0 {
                    let id = self.as_mut().start_ticks(16);
                    self.as_mut().rust_mut().drag_timer = id;
                }
            }
            _ => {
                if self.drag != DragBy::None && self.view.doc.has_selection() {
                    // (The primary selection, for a middle click elsewhere.)
                    qobject::set_clipboard_text(&QString::from(&self.view.doc.selected_text()), true);
                }
                self.as_mut().rust_mut().drag = DragBy::None;
                let id = self.drag_timer;
                if id != 0 {
                    self.as_mut().stop_ticks(id);
                    self.as_mut().rust_mut().drag_timer = 0;
                }
            }
        }
    }

    /// The selection stretched to the pointer.
    fn drag_to(mut self: Pin<&mut Self>, x: f64, y: f64) {
        let pos = self.as_mut().rust_mut().view.pos_at(x as f32, y as f32);
        let drag = self.drag;
        let (oa, ob) = self.drag_origin;
        match drag {
            DragBy::Char => self.as_mut().doc().set_cursor(pos, true),
            DragBy::Word | DragBy::Line => {
                let (a, b) = if drag == DragBy::Word { self.view.doc.word_at(pos) } else { self.view.doc.line_at(pos) };
                if pos < oa {
                    self.as_mut().doc().select(ob, a);
                } else {
                    self.as_mut().doc().select(oa, b.max(ob));
                }
            }
            DragBy::None => return,
        }
        self.refresh(false);
    }

    /// While dragging past the top or bottom: scroll, and select on.
    fn drag_scroll(mut self: Pin<&mut Self>) {
        let (x, y) = self.last_pointer;
        let top = self.view.top_pad as f64;
        let h = self.height();
        let dy = if y < top { y - top } else if y > h { y - h } else { 0.0 };
        if dy == 0.0 || self.drag == DragBy::None {
            let id = self.drag_timer;
            self.as_mut().stop_ticks(id);
            self.as_mut().rust_mut().drag_timer = 0;
            return;
        }
        let to = self.content_y + dy.clamp(-60.0, 60.0) * 0.5;
        self.as_mut().set_content_y(to);
        self.drag_to(x, y.clamp(top, h - 1.0));
    }

    fn wheel_hook(mut self: Pin<&mut Self>, dx: f64, dy: f64, modifiers: i32, pixels: bool) -> bool {
        let lh = self.view.line_height() as f64;
        let (mut dx, mut dy) = if pixels { (dx, dy) } else { (dx * 3.0 * lh, dy * 3.0 * lh) };
        if modifiers & SHIFT != 0 && dx == 0.0 {
            (dx, dy) = (dy, 0.0);
        }
        let (x0, y0) = (self.content_x, self.content_y);
        if dy != 0.0 {
            self.as_mut().set_content_y(y0 - dy);
        }
        if dx != 0.0 {
            self.as_mut().set_content_x(x0 - dx);
        }
        // (At an end: the wheel goes on to whatever holds the view.)
        self.content_x != x0 || self.content_y != y0
    }

    // ---- input methods ---------------------------------------------------------

    fn input_hook(mut self: Pin<&mut Self>, commit: &QString, preedit: &QString, replace_start: i32, replace_length: i32) {
        let commit = String::from(commit);
        self.as_mut().rust_mut().view.preedit = String::from(preedit);
        if !commit.is_empty() || replace_length > 0 {
            if replace_length > 0 {
                let c = self.view.doc.cursor as i64;
                let a = (c + replace_start as i64).max(0) as usize;
                let b = a + replace_length.max(0) as usize;
                self.as_mut().doc().select(a, b);
            }
            self.as_mut().edit(|d| d.type_text(&commit));
        }
        self.as_mut().wake_cursor();
        self.update();
    }
}

pub struct CodeDraftsRust;

impl Default for CodeDraftsRust {
    fn default() -> Self {
        Self
    }
}

impl qobject::CodeDrafts {
    pub fn discard(&self, path: &QString) {
        kept().remove(&String::from(path));
    }

    pub fn mark_saved(&self, path: &QString) {
        if let Some(d) = kept().get_mut(&String::from(path)) {
            d.mark_saved();
        }
    }
}
