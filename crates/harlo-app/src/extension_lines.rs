//! `ExtensionLines`: a pi extension's view (the styled lines its component
//! draws, see harlo's pi extension), read as terminal cells (harlo-core
//! `screen_of_lines`) and drawn by harlo-code's `TermPainter` into an image
//! Qt only shows, as the terminal is. ExtensionView.qml passes its keys and
//! clicks back to the extension.

// Rust guideline compliant 2026-10-09

#![expect(clippy::needless_pass_by_value, reason = "the code cxx-qt generates for this file's bridge passes property values by value")]

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
        /// The lines, with their ANSI styling.
        #[qproperty(QStringList, lines, READ, WRITE = set_lines, NOTIFY)]
        /// Columns the lines are drawn for (the extension's width).
        #[qproperty(i32, cols, READ, WRITE = set_cols, NOTIFY)]
        #[qproperty(QString, font_family, cxx_name = "fontFamily", READ, WRITE = set_font_family, NOTIFY)]
        #[qproperty(f64, font_pixel_size, cxx_name = "fontPixelSize", READ, WRITE = set_font_pixel_size, NOTIFY)]
        /// Around the grid.
        #[qproperty(f64, padding, READ, WRITE = set_padding, NOTIFY)]
        /// A cell (logical pixels, whole device pixels each).
        #[qproperty(f64, cell_width, cxx_name = "cellWidth")]
        #[qproperty(f64, cell_height, cxx_name = "cellHeight")]
        type ExtensionLines = super::ExtensionLinesRust;
    }

    impl cxx_qt::Constructor<()> for ExtensionLines {}

    extern "RustQt" {
        #[cxx_name = "setLines"]
        fn set_lines(self: Pin<&mut ExtensionLines>, lines: QStringList);
        #[cxx_name = "setCols"]
        fn set_cols(self: Pin<&mut ExtensionLines>, cols: i32);
        #[cxx_name = "setFontFamily"]
        fn set_font_family(self: Pin<&mut ExtensionLines>, family: QString);
        #[cxx_name = "setFontPixelSize"]
        fn set_font_pixel_size(self: Pin<&mut ExtensionLines>, px: f64);
        #[cxx_name = "setPadding"]
        fn set_padding(self: Pin<&mut ExtensionLines>, pad: f64);
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// 16 ANSI colours, then foreground, background (cursor and
        /// selection are unused).
        #[qinvokable]
        fn set_palette(self: Pin<&mut ExtensionLines>, palette: &QStringList);
        /// What a key press sends a terminal program (Qt key code, modifier
        /// bits, the event's text), as the extension reads keys; "" for
        /// none.
        #[qinvokable]
        fn key_data(self: &ExtensionLines, key: i32, modifiers: i32, text: &QString) -> QString;
        /// A row's text, as last drawn (self-tests).
        #[qinvokable]
        fn row_text(self: Pin<&mut ExtensionLines>, row: i32) -> QString;
    }

    unsafe extern "RustQt" {
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut ExtensionLines>, painter: *mut QPainter);
        #[cxx_override]
        #[cxx_name = "sizeHook"]
        fn size_hook(self: Pin<&mut ExtensionLines>);
        #[cxx_override]
        fn editable(self: &ExtensionLines) -> bool;
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn update(self: Pin<&mut ExtensionLines>);
        #[inherit]
        fn width(self: &ExtensionLines) -> f64;
        #[inherit]
        fn height(self: &ExtensionLines) -> f64;
        #[inherit]
        #[cxx_name = "pixelRatio"]
        fn pixel_ratio(self: &ExtensionLines) -> f64;
        #[inherit]
        unsafe fn blit(self: &ExtensionLines, painter: *mut QPainter, data: *const u8, width: i32, height: i32);
        #[inherit]
        #[cxx_name = "takeNoInput"]
        fn take_no_input(self: Pin<&mut ExtensionLines>);
    }
}

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use harlo_code::terminal::TermPainter;
use harlo_core::terminal::{Palette, Screen, key_bytes, screen_of_lines};

use qobject::ExtensionLines;

/// The element's state: what it draws and how.
pub struct ExtensionLinesRust {
    lines: QStringList,
    cols: i32,
    font_family: QString,
    font_pixel_size: f64,
    padding: f64,
    cell_width: f64,
    cell_height: f64,

    palette: Palette,
    /// The lines as cells; read again when they, the width or the colours
    /// change.
    screen: Screen,
    stale: bool,
    painter: TermPainter,
}

impl Default for ExtensionLinesRust {
    fn default() -> Self {
        // (The painter's own padding is the one `padding` says.)
        let mut painter = TermPainter::new();
        painter.set_padding(0.0);
        Self {
            lines: QStringList::default(),
            cols: 80,
            font_family: QString::from("JetBrains Mono"),
            font_pixel_size: 13.0,
            padding: 0.0,
            cell_width: 8.0,
            cell_height: 17.0,
            palette: Palette::from_hex(&[]),
            screen: Screen::default(),
            stale: true,
            painter,
        }
    }
}

impl cxx_qt::Initialize for ExtensionLines {
    fn initialize(mut self: Pin<&mut Self>) {
        // (The view's MouseArea takes the mouse, and the view the keys.)
        self.as_mut().take_no_input();
        self.measure();
    }
}

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
impl ExtensionLines {
    pub fn set_lines(mut self: Pin<&mut Self>, lines: QStringList) {
        if lines != self.lines {
            self.as_mut().rust_mut().lines = lines;
            self.as_mut().lines_changed();
            self.redraw();
        }
    }

    pub fn set_cols(mut self: Pin<&mut Self>, cols: i32) {
        let cols = cols.max(1);
        if cols != self.cols {
            self.as_mut().rust_mut().cols = cols;
            self.as_mut().cols_changed();
            self.redraw();
        }
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

    #[expect(clippy::cast_possible_truncation, reason = "a padding in pixels: `as` rounds toward zero and saturates, as wanted here")]
    pub fn set_padding(mut self: Pin<&mut Self>, pad: f64) {
        if pad != self.padding {
            self.as_mut().rust_mut().padding = pad;
            self.as_mut().rust_mut().painter.set_padding(pad as f32);
            self.as_mut().padding_changed();
            self.update();
        }
    }

    pub fn set_palette(mut self: Pin<&mut Self>, palette: &QStringList) {
        let colors: Vec<String> = palette.iter().map(std::string::ToString::to_string).collect();
        self.as_mut().rust_mut().palette = Palette::from_hex(&colors);
        self.redraw();
    }

    pub fn key_data(&self, key: i32, modifiers: i32, text: &QString) -> QString {
        let bytes = key_bytes(key, u32::try_from(modifiers).unwrap_or(0), &text.to_string(), false);
        QString::from(&String::from_utf8_lossy(&bytes).into_owned())
    }

    pub fn row_text(mut self: Pin<&mut Self>, row: i32) -> QString {
        self.as_mut().refresh();
        QString::from(&self.screen.row_text(usize::try_from(row).unwrap_or(usize::MAX)))
    }

    fn redraw(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().stale = true;
        self.update();
    }

    /// Reads the lines into cells, when they've changed.
    fn refresh(self: Pin<&mut Self>) {
        let mut rust = self.rust_mut();
        let rust = &mut *rust;
        if !rust.stale {
            return;
        }
        rust.stale = false;
        let lines: Vec<String> = rust.lines.iter().map(std::string::ToString::to_string).collect();
        screen_of_lines(&lines, usize::try_from(rust.cols).unwrap_or(1), &rust.palette, &mut rust.screen);
    }

    /// The cell's size for the font at the window's scale.
    #[expect(clippy::cast_possible_truncation, reason = "a font size in pixels: `as` rounds toward zero and saturates, as wanted here")]
    fn measure(mut self: Pin<&mut Self>) {
        let dpr = self.pixel_ratio().max(0.5) as f32;
        let (family, px) = (self.font_family.to_string(), self.font_pixel_size as f32);
        let (w, h) = {
            let mut rust = self.as_mut().rust_mut();
            rust.painter.set_font(&family, px);
            rust.painter.cell_size(dpr)
        };
        if f64::from(w) != self.cell_width {
            self.as_mut().set_cell_width(f64::from(w));
        }
        if f64::from(h) != self.cell_height {
            self.as_mut().set_cell_height(f64::from(h));
        }
        self.update();
    }

    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "whole pixels from floats: `as` rounds toward zero and saturates, as wanted here")]
    unsafe fn paint(mut self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        let dpr = self.pixel_ratio();
        let w = (self.width() * dpr).round().max(1.0) as usize;
        let h = (self.height() * dpr).round().max(1.0) as usize;
        self.as_mut().refresh();
        let mut rust = self.as_mut().rust_mut();
        let rust = &mut *rust;
        rust.painter.render(&rust.screen, rust.palette.background, None, w, h, dpr as f32);
        let pixels = self.painter.pixels();
        assert!(pixels.len() >= w * h * 4, "the extension's view drew {} bytes for a {w}x{h} frame", pixels.len());
        // SAFETY: Qt passes a painter valid for this paint call; blit reads
        // `w * h * 4` bytes, which the check above guarantees `pixels` holds,
        // and draws them before returning, while `pixels` is still borrowed.
        unsafe { self.blit(painter, pixels.as_ptr(), i32::try_from(w).unwrap_or(i32::MAX), i32::try_from(h).unwrap_or(i32::MAX)) };
    }

    fn size_hook(self: Pin<&mut Self>) {
        // (The window's scale is known once the item is in one.)
        self.measure();
    }

    fn editable(&self) -> bool {
        false
    }
}
