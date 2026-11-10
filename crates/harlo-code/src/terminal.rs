//! A terminal's screen drawn in Rust: harlo-core's grid of coloured cells
//! in the code font, each cell on whole device pixels. Only the rows that
//! changed since the last frame are drawn again. Box-drawing and block
//! characters are drawn as shapes, so lines and boxes join up between cells.

use std::collections::HashMap;

use cosmic_text::{Attrs, AttrsList, BufferLine, Ellipsize, Family, Hinting, LayoutGlyph, LineEnding, Shaping, SwashContent, Weight, Wrap};
use harlo_core::terminal::{Cell, CursorShape, Rgb, Screen};

use crate::fonts::{self, Fonts};

/// How the cursor looks this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorLook {
    /// The terminal has the keyboard: a solid cursor (else an outline).
    pub focused: bool,
    /// The blink's on phase.
    pub on: bool,
    pub color: Rgb,
}

#[derive(Clone, Copy, PartialEq)]
struct CursorMark {
    row: usize,
    col: usize,
    shape: CursorShape,
    focused: bool,
    on: bool,
    color: Rgb,
}

/// Glyphs by character and style (bold 1, italic 2), laid out once.
#[derive(Default)]
struct Glyphs {
    ascii: Vec<Option<Option<LayoutGlyph>>>,
    other: HashMap<(char, u8), Option<LayoutGlyph>>,
}

impl Glyphs {
    fn get(&mut self, fonts: &mut Fonts, family: &str, size: f32, c: char, style: u8) -> Option<&LayoutGlyph> {
        let lay = |fonts: &mut Fonts| {
            let mut attrs = Attrs::new().family(Family::Name(family));
            if style & 1 != 0 {
                attrs = attrs.weight(Weight::BOLD);
            }
            if style & 2 != 0 {
                attrs = attrs.style(cosmic_text::Style::Italic);
            }
            let mut b = BufferLine::new(c.to_string(), LineEnding::Lf, AttrsList::new(&attrs), Shaping::Advanced);
            b.layout(&mut fonts.system, size, None, Wrap::None, Ellipsize::None, None, 4, Hinting::Disabled)
                .first()
                .and_then(|l| l.glyphs.first().cloned())
        };
        if (c as u32) < 128 {
            if self.ascii.is_empty() {
                self.ascii.resize(128 * 4, None);
            }
            let i = c as usize * 4 + style as usize;
            if self.ascii[i].is_none() {
                self.ascii[i] = Some(lay(fonts));
            }
            self.ascii[i].as_ref().and_then(|g| g.as_ref())
        } else {
            self.other.entry((c, style)).or_insert_with(|| lay(fonts)).as_ref()
        }
    }
}

pub struct TermPainter {
    family: String,
    font_px: f32,
    /// The padding around the grid (logical).
    pad: f32,
    metrics_key: Option<(String, u32, u32)>,
    dpr: f32,
    /// A cell, and the baseline from its top (device pixels).
    cell_w: usize,
    cell_h: usize,
    baseline: i32,
    glyphs: Glyphs,

    pixels: Vec<u8>,
    width: usize,
    height: usize,
    /// What's on `pixels` now.
    drawn: Vec<Cell>,
    drawn_cols: usize,
    drawn_rows: usize,
    drawn_cursor: Option<CursorMark>,
    drawn_bg: Option<Rgb>,
}

impl Default for TermPainter {
    fn default() -> Self {
        Self::new()
    }
}

impl TermPainter {
    pub fn new() -> Self {
        Self {
            family: "JetBrains Mono".into(),
            font_px: 13.0,
            pad: 8.0,
            metrics_key: None,
            dpr: 1.0,
            cell_w: 8,
            cell_h: 17,
            baseline: 13,
            glyphs: Glyphs::default(),
            pixels: Vec::new(),
            width: 0,
            height: 0,
            drawn: Vec::new(),
            drawn_cols: 0,
            drawn_rows: 0,
            drawn_cursor: None,
            drawn_bg: None,
        }
    }

    pub fn set_font(&mut self, family: &str, font_px: f32) {
        if family != self.family || font_px != self.font_px {
            self.family = family.to_owned();
            self.font_px = font_px.max(1.0);
            self.metrics_key = None;
        }
    }

    pub fn set_padding(&mut self, pad: f32) {
        if pad != self.pad {
            self.pad = pad.max(0.0);
            self.drawn_bg = None;
        }
    }

    pub fn padding(&self) -> f32 {
        self.pad
    }

    /// Measures a cell at `dpr`, dropping glyphs laid out otherwise.
    fn ensure_metrics(&mut self, fonts: &mut Fonts, dpr: f32) {
        let key = (self.family.clone(), self.font_px.to_bits(), dpr.to_bits());
        if self.metrics_key.as_ref() == Some(&key) {
            return;
        }
        self.metrics_key = Some(key);
        self.dpr = dpr;
        self.glyphs = Glyphs::default();
        self.drawn_bg = None;
        let size = self.font_px * dpr;
        let attrs = Attrs::new().family(Family::Name(&self.family));
        let mut probe = BufferLine::new("M", LineEnding::Lf, AttrsList::new(&attrs), Shaping::Advanced);
        let line = probe.layout(&mut fonts.system, size, None, Wrap::None, Ellipsize::None, None, 4, Hinting::Disabled).first();
        let (w, ascent, descent) = line.map_or((size * 0.6, size, size * 0.3), |l| (l.glyphs.first().map_or(size * 0.6, |g| g.w), l.max_ascent, l.max_descent));
        self.cell_w = w.round().max(1.0) as usize;
        self.cell_h = (ascent + descent).ceil().max(1.0) as usize;
        self.baseline = ((self.cell_h as f32 - (ascent + descent)) / 2.0 + ascent).round() as i32;
    }

    /// A cell's size in logical pixels at `dpr` (whole device pixels each).
    pub fn cell_size(&mut self, dpr: f32) -> (f32, f32) {
        let mut f = fonts::lock();
        self.ensure_metrics(&mut f, dpr);
        (self.cell_w as f32 / dpr, self.cell_h as f32 / dpr)
    }

    /// The image last drawn: RGBA, premultiplied (and opaque).
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Draws `screen` into a `width`×`height` device-pixel image; only rows
    /// that changed since the last call, unless the size, the font or the
    /// background did. `cursor` is None when no cursor shows.
    pub fn render(&mut self, screen: &Screen, background: Rgb, cursor: Option<CursorLook>, width: usize, height: usize, dpr: f32) {
        let mut guard = fonts::lock();
        let fonts = &mut *guard;
        self.ensure_metrics(fonts, dpr);
        let full = self.drawn_bg != Some(background) || width != self.width || height != self.height || screen.cols != self.drawn_cols || screen.rows != self.drawn_rows;
        if full {
            self.width = width;
            self.height = height;
            self.pixels.clear();
            self.pixels.resize(width * height * 4, 0);
            let bg = [background.r, background.g, background.b, 255];
            for px in self.pixels.chunks_exact_mut(4) {
                px.copy_from_slice(&bg);
            }
            self.drawn.clear();
            self.drawn_cursor = None;
        }
        if !full {
            self.scroll_drawn(screen);
        }
        let mark = cursor.filter(|_| screen.cursor_visible).map(|c| CursorMark {
            row: screen.cursor_row.max(0) as usize,
            col: screen.cursor_col.max(0) as usize,
            shape: screen.cursor_shape,
            focused: c.focused,
            on: c.on || !c.focused,
            color: c.color,
        });
        let cursor_rows = if mark != self.drawn_cursor { [self.drawn_cursor.map(|m| m.row), mark.map(|m| m.row)] } else { [None, None] };
        let cols = screen.cols;
        for r in 0..screen.rows {
            let row = screen.row(r);
            let same = !full && self.drawn.get(r * cols..(r + 1) * cols) == Some(row);
            if same && !cursor_rows.contains(&Some(r)) {
                continue;
            }
            self.draw_row(fonts, r, row, background, mark.filter(|m| m.row == r));
        }
        self.drawn.clear();
        self.drawn.extend_from_slice(&screen.cells);
        self.drawn_cols = screen.cols;
        self.drawn_rows = screen.rows;
        self.drawn_cursor = mark;
        self.drawn_bg = Some(background);
    }

    /// Output that scrolled the screen up `k` rows: the drawn rows move up
    /// `k` with the pixels, leaving only the new ones at the bottom to draw.
    fn scroll_drawn(&mut self, screen: &Screen) {
        let (cols, n) = (screen.cols, screen.rows);
        if n < 2 || self.drawn.len() != n * cols || screen.row(0) == &self.drawn[..cols] {
            return;
        }
        let drawn = |i: usize| &self.drawn[i * cols..(i + 1) * cols];
        let Some(k) = (1..n).find(|&k| (0..n - k).all(|i| screen.row(i) == drawn(i + k))) else { return };
        let pad = (self.pad * self.dpr).round() as usize;
        let ch = self.cell_h;
        let top = pad.min(self.height);
        let end = (pad + n * ch).min(self.height);
        let from = (pad + k * ch).min(end);
        let row_bytes = self.width * 4;
        self.pixels.copy_within(from * row_bytes..end * row_bytes, top * row_bytes);
        self.drawn.copy_within(k * cols.., 0);
        // (Rows to draw again: the new ones, and the one the old cursor moved to.)
        let stale = Cell { c: '\u{ffff}', fg: Rgb { r: 0, g: 0, b: 0 }, bg: Rgb { r: 0, g: 0, b: 0 }, flags: 0 };
        for c in &mut self.drawn[(n - k) * cols..] {
            *c = stale;
        }
        if let Some(m) = self.drawn_cursor.take()
            && m.row >= k
        {
            for c in &mut self.drawn[(m.row - k) * cols..(m.row - k + 1) * cols] {
                *c = stale;
            }
        }
    }

    fn draw_row(&mut self, fonts: &mut Fonts, r: usize, row: &[Cell], background: Rgb, cursor: Option<CursorMark>) {
        let pad = (self.pad * self.dpr).round() as usize;
        let (cw, ch) = (self.cell_w, self.cell_h);
        let y0 = pad + r * ch;
        if y0 >= self.height {
            return;
        }
        let y1 = (y0 + ch).min(self.height);
        let mut c = Canvas { data: &mut self.pixels, width: self.width, y0, y1 };
        c.fill(0, self.width, background, 255);
        let light = ((cw as f32 / 8.0).round() as usize).max(1);
        // Backgrounds.
        for (col, cell) in row.iter().enumerate() {
            if cell.bg != background {
                c.fill(pad + col * cw, pad + (col + 1) * cw, cell.bg, 255);
            }
        }
        // The cursor under the text.
        let mut ink_at = None;
        if let Some(m) = cursor
            && m.focused
            && m.shape == CursorShape::Block
        {
            let wide = row.get(m.col).is_some_and(|c| c.flags & Cell::WIDE != 0);
            let x = pad + m.col * cw;
            c.fill(x, x + cw * if wide { 2 } else { 1 }, m.color, if m.on { 255 } else { 64 });
            if m.on {
                ink_at = Some(m.col);
            }
        }
        // The text.
        let size = self.font_px * self.dpr;
        for (col, cell) in row.iter().enumerate() {
            if cell.flags & Cell::SPACER != 0 {
                continue;
            }
            let x = pad + col * cw;
            let fg = if ink_at == Some(col) { background } else { cell.fg };
            if cell.c != ' ' && !draw_shape(&mut c, cell.c, x, cw, fg, light) {
                let style = (cell.flags & Cell::BOLD != 0) as u8 | ((cell.flags & Cell::ITALIC != 0) as u8) << 1;
                if let Some(g) = self.glyphs.get(fonts, &self.family, size, cell.c, style) {
                    let ph = g.physical((x as f32 - g.x, (y0 as i32 + self.baseline) as f32), 1.0);
                    c.glyph(fonts, ph.cache_key, ph.x, ph.y, fg);
                }
            }
            let w = cw * if cell.flags & Cell::WIDE != 0 { 2 } else { 1 };
            if cell.flags & Cell::UNDERLINE != 0 {
                let y = (y0 as i32 + self.baseline + light as i32 + 1).min(y1 as i32 - light as i32).max(y0 as i32) as usize;
                c.rect(x, x + w, y, y + light, fg, 255);
            }
            if cell.flags & Cell::STRIKE != 0 {
                let y = (y0 as i32 + self.baseline - (ch as f32 * 0.28).round() as i32).max(y0 as i32) as usize;
                c.rect(x, x + w, y, y + light, fg, 255);
            }
        }
        // The cursor over the text: a bar, an underline or an outline.
        if let Some(m) = cursor {
            let wide = row.get(m.col).is_some_and(|c| c.flags & Cell::WIDE != 0);
            let x = pad + m.col * cw;
            let w = cw * if wide { 2 } else { 1 };
            let t = (light * 2).min(cw);
            if !m.focused {
                c.rect(x, x + w, y0, y0 + light, m.color, 255);
                c.rect(x, x + w, y1.saturating_sub(light), y1, m.color, 255);
                c.rect(x, x + light, y0, y1, m.color, 255);
                c.rect(x + w - light, x + w, y0, y1, m.color, 255);
            } else if m.on {
                match m.shape {
                    CursorShape::Beam => c.rect(x, x + t, y0, y1, m.color, 255),
                    CursorShape::Underline => c.rect(x, x + w, y1.saturating_sub(t), y1, m.color, 255),
                    CursorShape::Block => {}
                }
            }
        }
    }
}

/// Pixels of one row band, `y0..y1`.
struct Canvas<'a> {
    data: &'a mut [u8],
    width: usize,
    y0: usize,
    y1: usize,
}

#[inline]
fn blend(d: &mut [u8], c: Rgb, a: u32) {
    if a == 0 {
        return;
    }
    if a >= 255 {
        d[0] = c.r;
        d[1] = c.g;
        d[2] = c.b;
        d[3] = 255;
        return;
    }
    let ia = 255 - a;
    d[0] = ((c.r as u32 * a + d[0] as u32 * ia + 127) / 255) as u8;
    d[1] = ((c.g as u32 * a + d[1] as u32 * ia + 127) / 255) as u8;
    d[2] = ((c.b as u32 * a + d[2] as u32 * ia + 127) / 255) as u8;
}

impl Canvas<'_> {
    /// Columns `x0..x1` of the whole band.
    fn fill(&mut self, x0: usize, x1: usize, c: Rgb, a: u32) {
        self.rect(x0, x1, self.y0, self.y1, c, a);
    }

    /// A rectangle (device pixels), cut to the band.
    fn rect(&mut self, x0: usize, x1: usize, y0: usize, y1: usize, c: Rgb, a: u32) {
        let (x1, y0, y1) = (x1.min(self.width), y0.max(self.y0), y1.min(self.y1));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for y in y0..y1 {
            for d in self.data[(y * self.width + x0) * 4..(y * self.width + x1) * 4].chunks_exact_mut(4) {
                blend(d, c, a);
            }
        }
    }

    /// One pixel at coverage `a` (0–255).
    fn dot(&mut self, x: usize, y: usize, c: Rgb, a: u32) {
        if x < self.width && y >= self.y0 && y < self.y1 {
            let o = (y * self.width + x) * 4;
            blend(&mut self.data[o..o + 4], c, a);
        }
    }

    fn glyph(&mut self, fonts: &mut Fonts, key: cosmic_text::CacheKey, x: i32, y: i32, color: Rgb) {
        let Some(image) = fonts.glyphs.get_image(&mut fonts.system, key) else { return };
        let left = x + image.placement.left;
        let top = y - image.placement.top;
        let (w, h) = (image.placement.width as i32, image.placement.height as i32);
        let col0 = (-left).max(0);
        let col1 = (self.width as i32 - left).min(w);
        let row0 = (self.y0 as i32 - top).max(0);
        let row1 = (self.y1 as i32 - top).min(h);
        for row in row0..row1 {
            let py = (top + row) as usize;
            for col in col0..col1 {
                let o = (py * self.width + (left + col) as usize) * 4;
                let d = &mut self.data[o..o + 4];
                match image.content {
                    SwashContent::Mask => blend(d, color, image.data[(row * w + col) as usize] as u32),
                    SwashContent::Color => {
                        let i = ((row * w + col) * 4) as usize;
                        let s = &image.data[i..i + 4];
                        blend(d, Rgb { r: s[0], g: s[1], b: s[2] }, s[3] as u32);
                    }
                    SwashContent::SubpixelMask => {
                        let i = ((row * w + col) * 4) as usize;
                        let cov = image.data[i..i + 3].iter().map(|&v| v as u32).sum::<u32>() / 3;
                        blend(d, color, cov);
                    }
                }
            }
        }
    }
}

/// Box-drawing characters U+2500–U+257F: the weight of each arm (left,
/// right, up, down): 1 light, 2 heavy, 3 double; "-" for those left to the
/// font (dashes, diagonals). ╭╮╯╰ are rounded.
const BOX: [&str; 128] = [
    "1100", "2200", "0011", "0022", "-", "-", "-", "-", "-", "-", "-", "-", "0101", "0201", "0102", "0202", //
    "1001", "2001", "1002", "2002", "0110", "0210", "0120", "0220", "1010", "2010", "1020", "2020", "0111", "0211", "0121", "0112", //
    "0122", "0221", "0212", "0222", "1011", "2011", "1021", "1012", "1022", "2021", "2012", "2022", "1101", "2101", "1201", "2201", //
    "1102", "2102", "1202", "2202", "1110", "2110", "1210", "2210", "1120", "2120", "1220", "2220", "1111", "2111", "1211", "2211", //
    "1121", "1112", "1122", "2121", "1221", "2112", "1212", "2221", "2212", "2122", "1222", "2222", "-", "-", "-", "-", //
    "3300", "0033", "0301", "0103", "0303", "3001", "1003", "3003", "0310", "0130", "0330", "3010", "1030", "3030", "0311", "0133", //
    "0333", "3011", "1033", "3033", "3301", "1103", "3303", "3310", "1130", "3330", "3311", "1133", "3333", "0101", "1001", "1010", //
    "0110", "-", "-", "-", "1000", "0010", "0100", "0001", "2000", "0020", "0200", "0002", "1200", "0012", "2100", "0021", //
];

/// Draws `c` as a shape if it's a box-drawing or block character.
fn draw_shape(cv: &mut Canvas, c: char, x: usize, w: usize, fg: Rgb, light: usize) -> bool {
    let (y0, h) = (cv.y0, cv.y1 - cv.y0);
    let u = c as u32;
    match u {
        0x2500..=0x257f => {
            let arms = BOX[(u - 0x2500) as usize].as_bytes();
            if arms.len() != 4 {
                return false;
            }
            let [l, r, up, dn] = [arms[0] - b'0', arms[1] - b'0', arms[2] - b'0', arms[3] - b'0'];
            let thick = |weight: u8| match weight {
                1 => light,
                2 => light * 2,
                3 => light * 3,
                _ => 0,
            };
            let (cx, cy) = (x + w / 2, y0 + h / 2);
            // How far past the middle an arm runs, to meet the strokes across it.
            let reach_h = (thick(up).max(thick(dn)) + 1) / 2;
            let reach_v = (thick(l).max(thick(r)) + 1) / 2;
            if (0x256d..=0x2570).contains(&u) {
                // A rounded corner: a quarter circle between its two arms.
                let (dx, dy): (i32, i32) = match u {
                    0x256d => (1, 1),
                    0x256e => (-1, 1),
                    0x256f => (-1, -1),
                    _ => (1, -1),
                };
                let rad = (w.min(h) / 2) as f32;
                let (acx, acy) = (cx as f32 + 0.5 + dx as f32 * rad, cy as f32 + 0.5 + dy as f32 * rad);
                let t = light as f32;
                for py in y0..y0 + h {
                    for px in x..x + w {
                        let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                        if (fx - acx) * dx as f32 > 0.0 || (fy - acy) * dy as f32 > 0.0 {
                            continue;
                        }
                        let d = ((fx - acx).hypot(fy - acy) - rad).abs();
                        let cover = (t / 2.0 + 0.5 - d).clamp(0.0, 1.0);
                        if cover > 0.0 {
                            cv.dot(px, py, fg, (cover * 255.0) as u32);
                        }
                    }
                }
                let ty = cy - light / 2;
                let tx = cx - light / 2;
                if dx > 0 {
                    cv.rect((acx as usize).min(x + w), x + w, ty, ty + light, fg, 255);
                } else {
                    cv.rect(x, (acx as usize).max(x), ty, ty + light, fg, 255);
                }
                if dy > 0 {
                    cv.rect(tx, tx + light, (acy as usize).min(y0 + h), y0 + h, fg, 255);
                } else {
                    cv.rect(tx, tx + light, y0, (acy as usize).max(y0), fg, 255);
                }
                return true;
            }
            // Horizontal arms: one stroke, or two for a double line.
            let strokes = |weight: u8, mid: usize| -> Vec<(usize, usize)> {
                match weight {
                    0 => vec![],
                    3 => vec![(mid - light - light / 2, light), (mid + light - light / 2, light)],
                    _ => {
                        let t = thick(weight);
                        vec![(mid - t / 2, t)]
                    }
                }
            };
            for (s, t) in strokes(l, cy) {
                cv.rect(x, cx + reach_h, s, s + t, fg, 255);
            }
            for (s, t) in strokes(r, cy) {
                cv.rect(cx.saturating_sub(reach_h).max(x), x + w, s, s + t, fg, 255);
            }
            for (s, t) in strokes(up, cx) {
                cv.rect(s, s + t, y0, cy + reach_v, fg, 255);
            }
            for (s, t) in strokes(dn, cx) {
                cv.rect(s, s + t, cy.saturating_sub(reach_v).max(y0), y0 + h, fg, 255);
            }
            true
        }
        0x2580..=0x259f => {
            let frac = |n: usize, of: usize, total: usize| (total * n + of / 2) / of;
            let (x1, y1) = (x + w, y0 + h);
            match u {
                0x2580 => cv.rect(x, x1, y0, y0 + frac(1, 2, h), fg, 255),
                0x2581..=0x2588 => cv.rect(x, x1, y1 - frac((u - 0x2580) as usize, 8, h), y1, fg, 255),
                0x2589..=0x258f => cv.rect(x, x + frac((0x2590 - u) as usize, 8, w), y0, y1, fg, 255),
                0x2590 => cv.rect(x + frac(1, 2, w), x1, y0, y1, fg, 255),
                0x2591..=0x2593 => cv.rect(x, x1, y0, y1, fg, 64 * (u - 0x2590)),
                0x2594 => cv.rect(x, x1, y0, y0 + frac(1, 8, h), fg, 255),
                0x2595 => cv.rect(x1 - frac(1, 8, w), x1, y0, y1, fg, 255),
                _ => {
                    // Quadrants: upper left 1, upper right 2, lower left 4, lower right 8.
                    const Q: [u8; 10] = [4, 8, 1, 1 | 4 | 8, 1 | 8, 1 | 2 | 4, 1 | 2 | 8, 2, 2 | 4, 2 | 4 | 8];
                    let q = Q[(u - 0x2596) as usize];
                    let (mx, my) = (x + frac(1, 2, w), y0 + frac(1, 2, h));
                    for (bit, (a, b, c, d)) in [(1, (x, mx, y0, my)), (2, (mx, x1, y0, my)), (4, (x, mx, my, y1)), (8, (mx, x1, my, y1))] {
                        if q & bit != 0 {
                            cv.rect(a, b, c, d, fg, 255);
                        }
                    }
                }
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(rows: &[&str], cols: usize) -> Screen {
        let fg = Rgb { r: 220, g: 220, b: 220 };
        let bg = Rgb { r: 20, g: 20, b: 20 };
        let mut cells = Vec::new();
        for r in rows {
            let mut chars: Vec<char> = r.chars().collect();
            chars.resize(cols, ' ');
            cells.extend(chars.into_iter().map(|c| Cell { c, fg, bg, flags: 0 }));
        }
        Screen { cols, rows: rows.len(), cells, cursor_row: 0, cursor_col: 0, cursor_visible: true, ..Screen::default() }
    }

    #[test]
    fn draws_cells_and_only_changed_rows() {
        fonts::for_tests();
        let bg = Rgb { r: 20, g: 20, b: 20 };
        let mut p = TermPainter::new();
        let (cw, ch) = p.cell_size(1.0);
        assert!(cw >= 6.0 && cw <= 10.0 && ch >= 14.0 && ch <= 20.0, "{cw}×{ch}");
        let (w, h) = (400, 120);
        let mut s = screen(&["hello", "─┼─ ╭╮", "█▀▄"], 40);
        let t = std::time::Instant::now();
        p.render(&s, bg, None, w, h, 1.0);
        let lit = |p: &TermPainter, y0: usize, y1: usize| p.pixels()[y0 * w * 4..y1 * w * 4].chunks_exact(4).filter(|px| px[0] > 100).count();
        let pad = 8;
        let ch = ch as usize;
        assert!(lit(&p, pad, pad + ch) > 40, "text drawn");
        // A light line across: lit in the cell's middle row, end to end.
        let mid = pad + ch + ch / 2;
        let row = &p.pixels()[mid * w * 4..(mid + 1) * w * 4];
        let across = (pad..pad + 3 * cw as usize).filter(|x| row[x * 4] > 100).count();
        assert_eq!(across, 3 * cw as usize, "─┼─ joins up");
        let full = &p.pixels()[(pad + 2 * ch + 2) * w * 4..];
        assert!(full[pad * 4] > 200, "█ fills its cell");
        eprintln!("first frame {:?}", t.elapsed());
        // A change on one row redraws that row only.
        s.cells[0].c = 'j';
        let before = p.pixels()[(pad + 2 * ch) * w * 4..].to_vec();
        p.render(&s, bg, None, w, h, 1.0);
        assert_eq!(&p.pixels()[(pad + 2 * ch) * w * 4..], &before[..], "the rows below are as they were");
        // The cursor: a block over its cell.
        let look = CursorLook { focused: true, on: true, color: Rgb { r: 0, g: 200, b: 0 } };
        s.cursor_row = 2;
        s.cursor_col = 5;
        p.render(&s, bg, Some(look), w, h, 1.0);
        let o = ((pad + 2 * ch + ch / 2) * w + pad + 5 * cw as usize + 1) * 4;
        assert_eq!(&p.pixels()[o..o + 3], &[0, 200, 0], "the block cursor");
    }

    #[test]
    fn scrolled_output_draws_as_a_fresh_frame_would() {
        fonts::for_tests();
        let bg = Rgb { r: 20, g: 20, b: 20 };
        let look = CursorLook { focused: true, on: true, color: Rgb { r: 0, g: 200, b: 0 } };
        let lines: Vec<String> = (0..12).map(|i| format!("line {i} ─┼─ gjy")).collect();
        let at = |from: usize| {
            let rows: Vec<&str> = lines[from..from + 6].iter().map(|s| s.as_str()).collect();
            let mut s = screen(&rows, 30);
            s.cursor_row = 5;
            s.cursor_col = 3;
            s
        };
        let (w, h) = (300, 140);
        let mut p = TermPainter::new();
        p.render(&at(0), bg, Some(look), w, h, 1.0);
        for from in [1, 3, 4] {
            p.render(&at(from), bg, Some(look), w, h, 1.0);
            let mut fresh = TermPainter::new();
            fresh.render(&at(from), bg, Some(look), w, h, 1.0);
            assert!(p.pixels() == fresh.pixels(), "scrolled to line {from}: as drawn from scratch");
        }
    }
}
