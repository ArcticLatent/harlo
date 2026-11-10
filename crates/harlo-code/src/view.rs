//! A document on screen: the visible lines laid out, drawn into an image,
//! and the mouse and keys mapped back to positions. Sizes here are logical
//! pixels (QML's) unless they say device pixels.

use cosmic_text::{Attrs, AttrsList, BufferLine, CacheKey, Ellipsize, Family, Hinting, LayoutGlyph, LineEnding, Shaping, SwashContent, Wrap};

use crate::doc::Doc;
use crate::fonts::{self, Fonts};
use crate::syntax::{self, Colouring};
use crate::text::Splice;

/// An sRGB colour with alpha, 0–255 each (not premultiplied).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }
    /// This colour with its alpha scaled by `a`.
    pub fn with_alpha(self, a: f32) -> Self {
        let [r, g, b, a0] = self.0;
        Self([r, g, b, (a0 as f32 * a).round().clamp(0.0, 255.0) as u8])
    }
}

/// The colours the view draws with (from harlo's theme).
#[derive(Clone, Debug)]
pub struct Palette {
    pub foreground: Rgba,
    pub background: Rgba,
    pub gutter_text: Rgba,
    pub border: Rgba,
    pub selection: Rgba,
    pub find: Rgba,
    pub find_current: Rgba,
    pub find_edge: Rgba,
    pub bracket: Rgba,
    pub bracket_edge: Rgba,
    pub arrival: Rgba,
    pub arrival_text: Rgba,
    pub cursor: Rgba,
    /// A removed (old) line, its changed part, its number.
    pub removed: Rgba,
    pub removed_strong: Rgba,
    pub removed_text: Rgba,
    pub added: Rgba,
    pub added_strong: Rgba,
    pub added_text: Rgba,
    /// Token classes (`syntax`): 0 is the plain text.
    pub classes: [Rgba; 12],
}

impl Default for Palette {
    fn default() -> Self {
        let fg = Rgba::new(220, 220, 220, 255);
        Self {
            foreground: fg,
            background: Rgba::new(30, 30, 30, 255),
            gutter_text: Rgba::new(140, 140, 140, 140),
            border: Rgba::new(70, 70, 70, 255),
            selection: Rgba::new(70, 110, 170, 140),
            find: Rgba::new(230, 170, 60, 51),
            find_current: Rgba::new(230, 170, 60, 102),
            find_edge: Rgba::new(230, 170, 60, 255),
            bracket: Rgba::new(100, 150, 240, 36),
            bracket_edge: Rgba::new(100, 150, 240, 153),
            arrival: Rgba::new(100, 150, 240, 41),
            arrival_text: Rgba::new(100, 150, 240, 255),
            cursor: fg,
            removed: Rgba::new(220, 80, 80, 36),
            removed_strong: Rgba::new(220, 80, 80, 82),
            removed_text: Rgba::new(220, 80, 80, 230),
            added: Rgba::new(80, 200, 120, 36),
            added_strong: Rgba::new(80, 200, 120, 82),
            added_text: Rgba::new(80, 200, 120, 230),
            classes: [fg; 12],
        }
    }
}

/// A diff row's look. `kind`: 0 unchanged, 1 changed (removed on the old
/// side, added on the new), 2 nothing on this side (hatched), 3 a fold, 4 a
/// comment's room (both drawn over in QML). `from..to`: the changed part
/// (UTF-16 columns), if known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub kind: u8,
    /// 0 the old side (red), 1 the new (green).
    pub side: u8,
    pub number: i32,
    pub from: i32,
    pub to: i32,
}

impl Row {
    /// Rows from a flat list of whole numbers, five a row: `kind,side,
    /// number,from,to,…` (read straight off: a big diff has thousands).
    pub fn parse_list(list: &str) -> Vec<Row> {
        let mut out = Vec::new();
        let mut fields = [0i64; 5];
        let mut n = 0;
        for part in list.split(',') {
            let Ok(v) = part.trim().parse::<i64>() else { continue };
            fields[n] = v;
            n += 1;
            if n == 5 {
                let [kind, side, number, from, to] = fields;
                out.push(Row { kind: kind.clamp(0, 255) as u8, side: side.clamp(0, 1) as u8, number: number as i32, from: from as i32, to: to as i32 });
                n = 0;
            }
        }
        out
    }
}

/// What the gutter shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gutter {
    None,
    /// Line numbers.
    Lines,
    /// A diff's: the rows' numbers and +/−.
    Rows,
}

/// Lines longer than this are laid out cut short (a minified file's line).
const LONGEST_LAID_OUT: usize = 20_000;

/// Laid-out lines kept before those far from view are dropped.
const KEEP_LAID_OUT: usize = 4000;

struct Digits {
    /// "0123456789+−" laid out: each glyph, and the advance.
    glyphs: Vec<LayoutGlyph>,
    advance: f32,
    ascent: f32,
    descent: f32,
}

pub struct View {
    pub doc: Doc,
    pub colouring: Colouring,
    family: String,
    font_px: f32,
    gutter_px: f32,
    line_px: f32,
    dpr: f32,
    /// One character's width (logical), from the font.
    char_w: f32,
    /// What the cached layouts were made with.
    laid_key: Option<(String, u32, u32)>,
    laid_out: usize,
    digits: Option<Digits>,
    pub width: f32,
    pub height: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub top_pad: f32,
    pub bottom_pad: f32,
    pub text_pad: f32,
    pub gutter: Gutter,
    /// The gutter's width for `Gutter::Rows` (set by its owner).
    pub rows_gutter_width: f32,
    pub palette: Palette,
    pub rows: Vec<Row>,
    pub matches: Vec<usize>,
    pub match_len: usize,
    pub current_match: i32,
    pub arrival: Option<usize>,
    pub arrival_glow: f32,
    pub focused: bool,
    pub cursor_on: bool,
    /// Text being composed in an input method, shown at the cursor.
    pub preedit: String,
    /// Where Up and Down aim (content x), kept across lines.
    pub goal_x: Option<f32>,
    /// The widest line: (columns, line), and whether to measure again.
    widest: (usize, usize),
    widest_dirty: bool,
    brackets: Option<((u64, usize, usize), Option<(usize, usize)>)>,
}

impl Default for View {
    fn default() -> Self {
        Self::new()
    }
}

/// Columns a line takes (tabs to the next stop of 4, wide characters 2).
fn columns(s: &str) -> usize {
    let mut n = 0;
    for c in s.chars() {
        n += match c {
            '\t' => 4 - n % 4,
            c if (c as u32) < 0x1100 => 1,
            c if matches!(c as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F300..=0x1F64F | 0x1F900..=0x1F9FF | 0x20000..=0x3FFFD) => 2,
            _ => 1,
        };
    }
    n
}

impl View {
    pub fn new() -> Self {
        Self {
            doc: Doc::new(),
            colouring: Colouring::new(),
            family: "JetBrains Mono".into(),
            font_px: 13.0,
            gutter_px: 12.0,
            line_px: 17.0,
            dpr: 1.0,
            char_w: 0.0,
            laid_key: None,
            laid_out: 0,
            digits: None,
            width: 0.0,
            height: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            top_pad: 8.0,
            bottom_pad: 40.0,
            text_pad: 8.0,
            gutter: Gutter::Lines,
            rows_gutter_width: 58.0,
            palette: Palette::default(),
            rows: Vec::new(),
            matches: Vec::new(),
            match_len: 0,
            current_match: -1,
            arrival: None,
            arrival_glow: 0.0,
            focused: false,
            cursor_on: true,
            preedit: String::new(),
            goal_x: None,
            widest: (0, 0),
            widest_dirty: true,
            brackets: None,
        }
    }

    // ---- the text changing ----------------------------------------------------

    /// Takes in the document's changes (after every edit).
    pub fn sync(&mut self) {
        let splices = self.doc.take_splices();
        for s in &splices {
            if s.removed == usize::MAX {
                self.colouring.restart(&mut self.doc.text.lines);
                self.widest_dirty = true;
                self.laid_out = 0;
                continue;
            }
            self.colouring.spliced(s);
            self.track_width(s);
        }
    }

    /// The file's path: its language, comments and indentation.
    pub fn set_path(&mut self, path: &str) {
        self.doc.set_path(path);
        self.colouring.set_path(path, &mut self.doc.text.lines);
    }

    /// Shows another document (kept from an earlier view).
    pub fn set_doc(&mut self, doc: Doc) {
        let read_only = self.doc.read_only;
        self.doc = doc;
        self.doc.read_only = read_only;
        self.doc.take_splices();
        self.colouring.restart(&mut self.doc.text.lines);
        self.forget_layouts();
        self.widest_dirty = true;
        self.laid_out = 0;
        self.brackets = None;
        self.goal_x = None;
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
    }

    /// Colours lines until `deadline`; whether any on screen changed.
    pub fn colour(&mut self, deadline: std::time::Instant) -> bool {
        let visible = self.visible_lines();
        self.colouring
            .work(&mut self.doc.text.lines, deadline)
            .is_some_and(|r| r.start < visible.end && r.end > visible.start)
    }

    /// Colours the lines on screen now if the pass in order hasn't got
    /// there (a guess, far down); whether any changed.
    pub fn colour_shown(&mut self) -> bool {
        let shown = self.visible_lines();
        self.colouring.show(&mut self.doc.text.lines, shown)
    }

    /// Line `i` has its colours (in order or guessed).
    pub fn line_coloured(&self, i: usize) -> bool {
        self.doc.text.lines.get(i).is_some_and(|l| !l.spans.is_empty())
    }

    /// Lines are still to be coloured.
    pub fn colouring_busy(&self) -> bool {
        self.colouring.busy(&self.doc.text.lines)
    }

    fn track_width(&mut self, s: &Splice) {
        if self.widest_dirty {
            return;
        }
        let line = self.widest.1;
        if line >= s.first && line < s.first + s.removed {
            self.widest_dirty = true;
            return;
        }
        if line >= s.first + s.removed {
            self.widest.1 = line + s.added - s.removed;
        }
        for i in s.first..s.first + s.added {
            let c = columns(self.doc.text.line(i));
            if c > self.widest.0 {
                self.widest = (c, i);
            }
        }
    }

    fn widest_columns(&mut self) -> usize {
        if self.widest_dirty {
            self.widest = (0, 0);
            for (i, l) in self.doc.text.lines.iter().enumerate() {
                let c = columns(&l.text);
                if c > self.widest.0 {
                    self.widest = (c, i);
                }
            }
            self.widest_dirty = false;
        }
        self.widest.0
    }

    // ---- the font -------------------------------------------------------------------

    pub fn set_font(&mut self, family: &str, font_px: f32, gutter_px: f32) {
        if family == self.family && font_px == self.font_px && gutter_px == self.gutter_px {
            return;
        }
        self.family = family.to_owned();
        self.font_px = font_px.max(1.0);
        self.gutter_px = gutter_px.max(1.0);
        // (JetBrains Mono's ascent and descent.)
        self.line_px = (self.font_px * 1.32).round();
        self.char_w = 0.0;
        self.forget_layouts();
    }

    pub fn font_px(&self) -> f32 {
        self.font_px
    }

    pub fn line_height(&self) -> f32 {
        self.line_px
    }

    fn forget_layouts(&mut self) {
        for l in &mut self.doc.text.lines {
            l.layout = None;
        }
        self.laid_out = 0;
        self.laid_key = None;
        self.digits = None;
    }

    fn attrs(&self) -> Attrs<'_> {
        Attrs::new().family(Family::Name(&self.family))
    }

    /// Lays out with the current font at `dpr`, dropping what was laid out
    /// otherwise.
    fn ensure_font(&mut self, fonts: &mut Fonts, dpr: f32) {
        let key = (self.family.clone(), self.font_px.to_bits(), dpr.to_bits());
        if self.laid_key.as_ref() == Some(&key) && self.char_w > 0.0 {
            return;
        }
        self.forget_layouts();
        self.dpr = dpr;
        self.laid_key = Some(key);
        let mut probe = BufferLine::new("0", LineEnding::Lf, AttrsList::new(&self.attrs()), Shaping::Advanced);
        let w = probe
            .layout(&mut fonts.system, self.font_px, None, Wrap::None, Ellipsize::None, None, 4, Hinting::Disabled)
            .first()
            .and_then(|l| l.glyphs.first())
            .map_or(self.font_px * 0.6, |g| g.w);
        self.char_w = w;
    }

    /// One character's width (logical), measuring the font if need be.
    pub fn char_width(&mut self) -> f32 {
        if self.char_w <= 0.0 {
            let mut f = fonts::lock();
            let dpr = self.dpr;
            self.ensure_font(&mut f, dpr);
        }
        self.char_w
    }

    fn layout_line(&mut self, fonts: &mut Fonts, i: usize) -> Option<&cosmic_text::LayoutLine> {
        let font_size = self.font_px * self.dpr;
        if self.doc.text.lines.get(i)?.layout.is_none() {
            let attrs = AttrsList::new(&Attrs::new().family(Family::Name(&self.family)));
            let line = &self.doc.text.lines[i];
            let mut text = line.text.strip_suffix('\r').unwrap_or(&line.text);
            if text.len() > LONGEST_LAID_OUT {
                text = &text[..crate::text::floor_char(text, LONGEST_LAID_OUT)];
            }
            let b = BufferLine::new(text, LineEnding::Lf, attrs, Shaping::Advanced);
            self.doc.text.lines[i].layout = Some(Box::new(b));
            self.laid_out += 1;
        }
        let b = self.doc.text.lines[i].layout.as_mut()?;
        b.layout(&mut fonts.system, font_size, None, Wrap::None, Ellipsize::None, None, 4, Hinting::Disabled).first()
    }

    /// Drops the layouts of lines far from `keep` once there are many.
    fn prune(&mut self, keep: std::ops::Range<usize>) {
        if self.laid_out <= KEEP_LAID_OUT {
            return;
        }
        let lo = keep.start.saturating_sub(500);
        let hi = keep.end + 500;
        let mut n = 0;
        for (i, l) in self.doc.text.lines.iter_mut().enumerate() {
            if l.layout.is_some() {
                if i < lo || i >= hi {
                    l.layout = None;
                } else {
                    n += 1;
                }
            }
        }
        self.laid_out = n;
    }

    // ---- geometry (logical) ------------------------------------------------------

    pub fn gutter_width(&self) -> f32 {
        match self.gutter {
            Gutter::None => 0.0,
            Gutter::Rows => self.rows_gutter_width,
            Gutter::Lines => {
                let digits = self.doc.text.line_count().to_string().len().max(3) as f32;
                (digits * self.gutter_px * 0.62 + 24.0).max(52.0).round()
            }
        }
    }

    /// Where column 0 is drawn (item x, unscrolled).
    pub fn text_x(&self) -> f32 {
        self.gutter_width() + if self.gutter == Gutter::Lines { 1.0 } else { 0.0 } + self.text_pad
    }

    pub fn content_height(&self) -> f32 {
        self.top_pad + self.doc.text.line_count() as f32 * self.line_px + self.bottom_pad
    }

    pub fn content_width(&mut self) -> f32 {
        let cols = self.widest_columns() as f32;
        self.text_x() + cols * self.char_width() + 40.0
    }

    pub fn max_scroll_y(&self) -> f32 {
        (self.content_height() - self.height).max(0.0)
    }

    pub fn max_scroll_x(&mut self) -> f32 {
        (self.content_width() - self.width).max(0.0)
    }

    pub fn set_scroll(&mut self, x: f32, y: f32) {
        self.scroll_x = x.clamp(0.0, self.max_scroll_x());
        self.scroll_y = y.clamp(0.0, self.max_scroll_y());
    }

    /// The lines in view (and a part one at each end).
    pub fn visible_lines(&self) -> std::ops::Range<usize> {
        let first = ((self.scroll_y - self.top_pad) / self.line_px).floor().max(0.0) as usize;
        let last = ((self.scroll_y - self.top_pad + self.height) / self.line_px).ceil().max(0.0) as usize + 1;
        first.min(self.doc.text.line_count())..last.min(self.doc.text.line_count())
    }

    /// A position's x in the text (logical, 0 at column 0).
    pub fn x_of(&mut self, pos: usize) -> f32 {
        let (l, b) = self.doc.text.locate(pos);
        let mut f = fonts::lock();
        let dpr = self.dpr;
        self.ensure_font(&mut f, dpr);
        self.x_in_line(&mut f, l, b)
    }

    fn x_in_line(&mut self, fonts: &mut Fonts, l: usize, byte: usize) -> f32 {
        let dpr = self.dpr;
        let Some(layout) = self.layout_line(fonts, l) else { return 0.0 };
        for g in &layout.glyphs {
            if g.start >= byte {
                return g.x / dpr;
            }
            if byte < g.end {
                // (Inside a cluster, a ligature: in proportion.)
                let part = (byte - g.start) as f32 / (g.end - g.start).max(1) as f32;
                return (g.x + g.w * part) / dpr;
            }
        }
        layout.w / dpr
    }

    /// The rectangle of the cursor at `pos` (content coordinates: x from
    /// column 0, y from the first line's top).
    pub fn position_rect(&mut self, pos: usize) -> (f32, f32, f32, f32) {
        let l = self.doc.text.line_of(pos);
        (self.x_of(pos), l as f32 * self.line_px, 1.0, self.line_px)
    }

    /// The position nearest content point (x, y).
    pub fn pos_at_content(&mut self, x: f32, y: f32) -> usize {
        let n = self.doc.text.line_count();
        let l = ((y / self.line_px).floor().max(0.0) as usize).min(n - 1);
        let mut f = fonts::lock();
        let dpr = self.dpr;
        self.ensure_font(&mut f, dpr);
        self.pos_in_line(&mut f, l, x)
    }

    fn pos_in_line(&mut self, fonts: &mut Fonts, l: usize, x: f32) -> usize {
        let dpr = self.dpr;
        let xd = x * dpr;
        let byte = match self.layout_line(fonts, l) {
            None => 0,
            Some(layout) => {
                let mut at = None;
                for g in &layout.glyphs {
                    if xd < g.x + g.w {
                        at = Some(if xd > g.x + g.w / 2.0 { g.end } else { g.start });
                        break;
                    }
                }
                at.unwrap_or(usize::MAX)
            }
        };
        let line = &self.doc.text.lines[l];
        let byte = if byte == usize::MAX { line.text.strip_suffix('\r').unwrap_or(&line.text).len() } else { byte };
        self.doc.text.pos(l, byte)
    }

    /// The position under item point (x, y).
    pub fn pos_at(&mut self, x: f32, y: f32) -> usize {
        let cx = x - self.text_x() + self.scroll_x;
        let cy = y - self.top_pad + self.scroll_y;
        self.pos_at_content(cx, cy)
    }

    /// The position a line up or down (`lines` negative: up), at the same x.
    pub fn pos_vertical(&mut self, from: usize, lines: isize) -> usize {
        let l = self.doc.text.line_of(from) as isize;
        let n = self.doc.text.line_count() as isize;
        let x = match self.goal_x {
            Some(x) => x,
            None => {
                let x = self.x_of(from);
                self.goal_x = Some(x);
                x
            }
        };
        let target = l + lines;
        if target < 0 {
            return 0;
        }
        if target >= n {
            return self.doc.text.len();
        }
        let mut f = fonts::lock();
        self.pos_in_line(&mut f, target as usize, x)
    }

    /// Scrolls so `pos` is in view (`margin`: room kept around it).
    pub fn reveal(&mut self, pos: usize, margin: f32) {
        let (x, y, _, h) = self.position_rect(pos);
        let text_w = (self.width - self.text_x()).max(0.0);
        let view_h = (self.height - self.top_pad).max(0.0);
        let (mut sx, mut sy) = (self.scroll_x, self.scroll_y);
        if y < sy {
            sy = y;
        } else if y + h + margin > sy + view_h {
            sy = y + h + margin - view_h;
        }
        if x < sx {
            sx = (x - margin).max(0.0);
        } else if x + margin > sx + text_w {
            sx = x + margin - text_w;
        }
        self.set_scroll(sx, sy);
    }

    /// Scrolls `pos` a third of the way down, unless it's in view
    /// (`always`: even then).
    pub fn reveal_third(&mut self, pos: usize, always: bool) {
        let (x, y, _, h) = self.position_rect(pos);
        let view_h = (self.height - self.top_pad).max(0.0);
        let text_w = (self.width - self.text_x()).max(0.0);
        let mut sy = self.scroll_y;
        if always || y < sy || y + h > sy + view_h {
            sy = y - view_h / 3.0;
        }
        let mut sx = self.scroll_x;
        if x < sx || x > sx + text_w - 40.0 {
            sx = (x - text_w / 3.0).max(0.0);
        }
        if always && x < text_w - 40.0 {
            sx = 0.0;
        }
        self.set_scroll(sx, sy);
    }

    /// The bracket pair by the cursor (cached until the text or cursor moves).
    pub fn bracket_pair(&mut self) -> Option<(usize, usize)> {
        let key = (self.doc.revision, self.doc.anchor, self.doc.cursor);
        if let Some((k, v)) = self.brackets
            && k == key
        {
            return v;
        }
        let v = self.doc.bracket_pair();
        self.brackets = Some((key, v));
        v
    }

    /// How many of the matches are in view.
    pub fn visible_matches(&self) -> usize {
        let lines = self.visible_lines();
        let from = self.doc.text.line_start(lines.start);
        let to = self.doc.text.line_start(lines.end);
        self.matches.iter().filter(|&&m| m + self.match_len >= from && m <= to).count()
    }

    // ---- drawing ------------------------------------------------------------------

    /// Draws the view (`width`×`height` device pixels) into `out`: RGBA,
    /// premultiplied, rows of `width * 4` bytes.
    pub fn render(&mut self, out: &mut Vec<u8>, width: usize, height: usize, dpr: f32) {
        // (No line on screen waits for its colours.)
        self.colour_shown();
        out.clear();
        out.resize(width * height * 4, 0);
        let bg = premultiply(self.palette.background);
        for px in out.chunks_exact_mut(4) {
            px.copy_from_slice(&bg);
        }
        let mut guard = fonts::lock();
        let fonts = &mut *guard;
        self.ensure_font(fonts, dpr);
        if self.digits.is_none() {
            self.digits = Some(self.lay_digits(fonts));
        }
        let mut c = Canvas { data: out, width, height, dpr };
        let lines = self.visible_lines();
        self.prune(lines.clone());
        let lh = self.line_px;
        let gutter_w = self.gutter_width();
        let text_x = self.text_x();
        let x0 = text_x - self.scroll_x;
        let (sel_a, sel_b) = self.doc.selection();
        let brackets = if self.doc.read_only { None } else { self.bracket_pair() };
        let p = self.palette.clone();
        let line_count = self.doc.text.line_count();
        // Where the matches in view start.
        let mut m = {
            let from = self.doc.text.line_start(lines.start);
            self.matches.partition_point(|&m| m + self.match_len < from)
        };
        let clip = ((text_x - self.text_pad / 2.0) * dpr).round() as i32;
        for i in lines.clone() {
            let top = self.top_pad + i as f32 * lh - self.scroll_y;
            let start = self.doc.text.line_start(i);
            let units = self.doc.text.lines[i].units as usize;
            let end = start + units;
            let row = self.rows.get(i).copied();
            // The row: a diff's tint or hatching.
            if let Some(r) = row {
                match r.kind {
                    1 => c.fill(0.0, top, self.width, lh, if r.side == 0 { p.removed } else { p.added }),
                    2 => c.hatch(gutter_w, top, self.width - gutter_w, lh, p.border),
                    _ => {}
                }
            }
            if self.arrival == Some(i) && self.arrival_glow > 0.0 {
                c.fill(0.0, top, self.width, lh, p.arrival.with_alpha(self.arrival_glow));
            }
            // The changed part of a diff's line.
            if let Some(r) = row
                && r.kind == 1
                && r.from >= 0
            {
                let a = self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(r.from as usize));
                let b = self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(r.to.max(r.from) as usize));
                c.fill(x0 + a, top, (b - a).max(2.0), lh, if r.side == 0 { p.removed_strong } else { p.added_strong });
            }
            // The selection (a line's break included).
            if sel_a < sel_b && sel_a <= end && sel_b > start {
                let a = if sel_a > start { self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(sel_a - start)) } else { 0.0 };
                let b = if sel_b <= end {
                    self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(sel_b - start))
                } else {
                    self.x_in_line(fonts, i, usize::MAX) + self.char_w * 0.6
                };
                if b > a {
                    c.fill(x0 + a, top, b - a, lh, p.selection);
                }
            }
            // Matches.
            while m < self.matches.len() && self.matches[m] <= end {
                let s = self.matches[m];
                if s >= start {
                    let a = self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(s - start));
                    let b = self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(s + self.match_len - start));
                    let current = m as i32 == self.current_match;
                    let (x, w) = (x0 + a - 1.0, (b - a).max(2.0) + 2.0);
                    c.fill(x, top, w, lh, if current { p.find_current } else { p.find });
                    if current {
                        c.frame(x, top, w, lh, p.find_edge);
                    }
                }
                m += 1;
            }
            // The bracket pair.
            if let Some((a, b)) = brackets {
                for q in [a, b] {
                    if q >= start && q < end {
                        let col = q - start;
                        let xa = self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(col));
                        let xb = self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(col + 1));
                        let w = (xb - xa).max(4.0);
                        c.fill(x0 + xa, top, w, lh, p.bracket);
                        c.frame(x0 + xa, top, w, lh, p.bracket_edge);
                    }
                }
            }
            // The text.
            let spans = std::mem::take(&mut self.doc.text.lines[i].spans);
            if let Some(layout) = self.layout_line(fonts, i) {
                let baseline = (top * dpr + (lh * dpr - (layout.max_ascent + layout.max_descent)) / 2.0 + layout.max_ascent).round();
                let gx = (x0 * dpr).round();
                for g in &layout.glyphs {
                    if g.x + g.w + gx < clip as f32 || g.x + gx > width as f32 {
                        continue;
                    }
                    let color = p.classes[(syntax::class_at(&spans, g.start) as usize).min(11)];
                    let ph = g.physical((gx, baseline), 1.0);
                    c.glyph(fonts, ph.cache_key, ph.x, ph.y, color, clip);
                }
            }
            self.doc.text.lines[i].spans = spans;
            // The cursor, and text being composed.
            let shows_cursor = self.focused && !self.doc.read_only;
            if shows_cursor && self.doc.cursor >= start && self.doc.cursor <= end {
                let x = x0 + self.x_in_line(fonts, i, self.doc.text.lines[i].byte_at(self.doc.cursor - start));
                if !self.preedit.is_empty() {
                    self.draw_preedit(fonts, &mut c, x, top, clip);
                } else if self.cursor_on {
                    c.fill(x, top, 1.5, lh, p.cursor);
                }
            }
            // The gutter.
            match self.gutter {
                Gutter::Lines => {
                    c.fill(0.0, top, gutter_w, lh, p.background);
                    let color = if self.arrival == Some(i) && self.arrival_glow > 0.0 {
                        p.arrival_text.with_alpha(0.55 + 0.45 * self.arrival_glow)
                    } else {
                        p.gutter_text
                    };
                    self.draw_number(fonts, &mut c, (i + 1) as i64, gutter_w - 10.0, top, color);
                }
                Gutter::Rows => {
                    let r = row.unwrap_or_default();
                    let tint = if r.side == 0 { p.removed_text } else { p.added_text };
                    c.fill(0.0, top, gutter_w, lh, p.background.with_alpha(if r.kind == 1 { 0.35 } else { 1.0 }));
                    if r.kind < 2 && r.number > 0 {
                        self.draw_number(fonts, &mut c, r.number as i64, gutter_w - 18.0, top, if r.kind == 1 { tint } else { p.gutter_text });
                    }
                    if r.kind == 1 {
                        self.draw_sign(fonts, &mut c, r.side == 1, gutter_w - 14.0, top, tint);
                    }
                }
                Gutter::None => {}
            }
        }
        // Above the first line and past the last: the gutter's background.
        if self.gutter != Gutter::None {
            let below = self.top_pad + line_count as f32 * lh - self.scroll_y;
            if below < self.height {
                c.fill(0.0, below.max(0.0), gutter_w, self.height - below.max(0.0), p.background);
            }
            if self.top_pad > self.scroll_y {
                c.fill(0.0, 0.0, gutter_w, self.top_pad - self.scroll_y, p.background);
            }
        }
        if self.gutter == Gutter::Lines {
            c.fill(gutter_w, 0.0, 1.0, self.height, p.border.with_alpha(0.6));
        }
    }

    fn lay_digits(&self, fonts: &mut Fonts) -> Digits {
        let mut b = BufferLine::new("0123456789+\u{2212}", LineEnding::Lf, AttrsList::new(&self.attrs()), Shaping::Advanced);
        let size = self.gutter_px * self.dpr;
        let layout = b.layout(&mut fonts.system, size, None, Wrap::None, Ellipsize::None, None, 4, Hinting::Disabled);
        let line = layout.first();
        let glyphs = line.map(|l| l.glyphs.clone()).unwrap_or_default();
        let advance = glyphs.first().map_or(size * 0.6, |g| g.w);
        Digits { glyphs, advance, ascent: line.map_or(size, |l| l.max_ascent), descent: line.map_or(0.0, |l| l.max_descent) }
    }

    /// A number right-aligned to `right` (logical x), in the gutter's size.
    fn draw_number(&self, fonts: &mut Fonts, c: &mut Canvas, n: i64, right: f32, top: f32, color: Rgba) {
        let Some(d) = &self.digits else { return };
        let s = n.to_string();
        let dpr = self.dpr;
        let baseline = (top * dpr + (self.line_px * dpr - (d.ascent + d.descent)) / 2.0 + d.ascent).round();
        let mut x = (right * dpr - d.advance * s.len() as f32).round();
        for ch in s.bytes() {
            if let Some(g) = d.glyphs.get((ch - b'0') as usize) {
                let ph = g.physical((x - g.x, baseline), 1.0);
                c.glyph(fonts, ph.cache_key, ph.x, ph.y, color, 0);
            }
            x += d.advance;
        }
    }

    fn draw_sign(&self, fonts: &mut Fonts, c: &mut Canvas, plus: bool, left: f32, top: f32, color: Rgba) {
        let Some(d) = &self.digits else { return };
        let dpr = self.dpr;
        let baseline = (top * dpr + (self.line_px * dpr - (d.ascent + d.descent)) / 2.0 + d.ascent).round();
        if let Some(g) = d.glyphs.get(if plus { 10 } else { 11 }) {
            let ph = g.physical(((left * dpr).round() - g.x, baseline), 1.0);
            c.glyph(fonts, ph.cache_key, ph.x, ph.y, color, 0);
        }
    }

    fn draw_preedit(&self, fonts: &mut Fonts, c: &mut Canvas, x: f32, top: f32, clip: i32) {
        let dpr = self.dpr;
        let mut b = BufferLine::new(self.preedit.as_str(), LineEnding::Lf, AttrsList::new(&self.attrs()), Shaping::Advanced);
        let layout = b.layout(&mut fonts.system, self.font_px * dpr, None, Wrap::None, Ellipsize::None, None, 4, Hinting::Disabled);
        let Some(line) = layout.first() else { return };
        let w = line.w / dpr;
        c.fill(x, top, w + 2.0, self.line_px, self.palette.background);
        let baseline = (top * dpr + (self.line_px * dpr - (line.max_ascent + line.max_descent)) / 2.0 + line.max_ascent).round();
        for g in &line.glyphs {
            let ph = g.physical(((x * dpr).round(), baseline), 1.0);
            c.glyph(fonts, ph.cache_key, ph.x, ph.y, self.palette.foreground, clip);
        }
        c.fill(x, top + self.line_px - 2.0, w, 1.0, self.palette.foreground);
        c.fill(x + w, top, 1.5, self.line_px, self.palette.cursor);
    }
}

fn premultiply(c: Rgba) -> [u8; 4] {
    let [r, g, b, a] = c.0;
    let m = |v: u8| ((v as u32 * a as u32 + 127) / 255) as u8;
    [m(r), m(g), m(b), a]
}

struct Canvas<'a> {
    data: &'a mut [u8],
    width: usize,
    height: usize,
    dpr: f32,
}

impl Canvas<'_> {
    /// `c` (straight colour) over `d` (premultiplied) at `a` (0–255).
    #[inline]
    fn blend(d: &mut [u8], c: [u8; 4], a: u32) {
        if a == 0 {
            return;
        }
        let ia = 255 - a;
        d[0] = ((c[0] as u32 * a + d[0] as u32 * ia + 127) / 255) as u8;
        d[1] = ((c[1] as u32 * a + d[1] as u32 * ia + 127) / 255) as u8;
        d[2] = ((c[2] as u32 * a + d[2] as u32 * ia + 127) / 255) as u8;
        d[3] = (a + (d[3] as u32 * ia + 127) / 255) as u8;
    }

    /// The device pixels a logical span covers.
    fn span(&self, x: f32, w: f32, max: usize) -> (usize, usize) {
        let a = (x * self.dpr).round().clamp(0.0, max as f32) as usize;
        let b = ((x + w) * self.dpr).round().clamp(0.0, max as f32) as usize;
        (a, b.max(a))
    }

    /// A rectangle (logical), blended.
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        let (x0, x1) = self.span(x, w, self.width);
        let (y0, y1) = self.span(y, h, self.height);
        let a = color.0[3] as u32;
        if a == 0 || x0 == x1 {
            return;
        }
        let solid = premultiply(color);
        for py in y0..y1 {
            let row = &mut self.data[(py * self.width + x0) * 4..(py * self.width + x1) * 4];
            if a == 255 {
                for d in row.chunks_exact_mut(4) {
                    d.copy_from_slice(&solid);
                }
            } else {
                for d in row.chunks_exact_mut(4) {
                    Self::blend(d, color.0, a);
                }
            }
        }
    }

    /// A rectangle's outline, a logical pixel wide.
    fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        let t = 1.0;
        self.fill(x, y, w, t, color);
        self.fill(x, y + h - t, w, t, color);
        self.fill(x, y + t, t, h - 2.0 * t, color);
        self.fill(x + w - t, y + t, t, h - 2.0 * t, color);
    }

    /// Diagonal hatching (nothing on this side of a diff).
    fn hatch(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        let (x0, x1) = self.span(x, w, self.width);
        let (y0, y1) = self.span(y, h, self.height);
        let period = (8.0 * self.dpr).round().max(2.0) as usize;
        let stroke = (1.2 * self.dpr).max(1.0);
        let a = color.0[3] as f32;
        for py in y0..y1 {
            for px in x0..x1 {
                // Distance along the row to the nearest stroke.
                let t = ((px + py) % period) as f32;
                let d = t.min(period as f32 - t);
                let cover = (stroke / 2.0 + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0 {
                    let o = (py * self.width + px) * 4;
                    Self::blend(&mut self.data[o..o + 4], color.0, (a * cover) as u32);
                }
            }
        }
    }

    /// A glyph with its origin at device (x, y), left of `min_x` cut off.
    fn glyph(&mut self, fonts: &mut Fonts, key: CacheKey, x: i32, y: i32, color: Rgba, min_x: i32) {
        let Some(image) = fonts.glyphs.get_image(&mut fonts.system, key) else { return };
        let left = x + image.placement.left;
        let top = y - image.placement.top;
        let (w, h) = (image.placement.width as i32, image.placement.height as i32);
        let col0 = (min_x.max(0) - left).max(0);
        let col1 = (self.width as i32 - left).min(w);
        let row0 = (-top).max(0);
        let row1 = (self.height as i32 - top).min(h);
        let ca = color.0[3] as u32;
        for row in row0..row1 {
            let py = (top + row) as usize;
            for col in col0..col1 {
                let o = (py * self.width + (left + col) as usize) * 4;
                let d = &mut self.data[o..o + 4];
                match image.content {
                    SwashContent::Mask => {
                        let cov = image.data[(row * w + col) as usize] as u32;
                        Self::blend(d, color.0, cov * ca / 255);
                    }
                    SwashContent::Color => {
                        let i = ((row * w + col) * 4) as usize;
                        let s = &image.data[i..i + 4];
                        Self::blend(d, [s[0], s[1], s[2], 255], s[3] as u32);
                    }
                    SwashContent::SubpixelMask => {
                        let i = ((row * w + col) * 4) as usize;
                        let cov = image.data[i..i + 3].iter().map(|&v| v as u32).sum::<u32>() / 3;
                        Self::blend(d, color.0, cov * ca / 255);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_and_maps_positions() {
        fonts::for_tests();
        let mut v = View::new();
        v.doc.set_path("a.rs");
        v.doc.load("fn main() {\n    println!(\"hi\");\n}\n");
        v.sync();
        v.width = 600.0;
        v.height = 200.0;
        let mut out = Vec::new();
        v.render(&mut out, 600, 200, 1.0);
        let lit = out.chunks_exact(4).filter(|p| p[0] > 120).count();
        assert!(lit > 50, "{lit} lit pixels");
        // Column 4 of line 1 is four characters in.
        let x = v.x_of(v.doc.text.pos(1, 4));
        assert!((x - 4.0 * v.char_width()).abs() < 0.5, "{x}");
        let p = v.pos_at(v.text_x() + x + 1.0, v.top_pad + v.line_height() * 1.5);
        assert_eq!(p, v.doc.text.pos(1, 4));
        // Past the end of a line: its end.
        let p = v.pos_at(500.0, v.top_pad + 2.0);
        assert_eq!(p, 11);
        // Up keeps the column.
        v.doc.set_cursor(v.doc.text.pos(1, 8), false);
        let up = v.pos_vertical(v.doc.cursor, -1);
        assert_eq!(up, 8);
    }

    #[test]
    fn rows_parse() {
        let rows = Row::parse_list("1,0,12,3,7,2,1,-1,-1,-1,0,1,5,-1,-1");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], Row { kind: 1, side: 0, number: 12, from: 3, to: 7 });
        assert_eq!(rows[1], Row { kind: 2, side: 1, number: -1, from: -1, to: -1 });
        assert_eq!(rows[2], Row { kind: 0, side: 1, number: 5, from: -1, to: -1 });
    }

    #[test]
    fn scroll_reveals() {
        fonts::for_tests();
        let text: String = (0..1000).map(|i| format!("line {i}\n")).collect();
        let mut v = View::new();
        v.doc.load(&text);
        v.sync();
        v.width = 400.0;
        v.height = 300.0;
        let p = v.doc.text.line_start(500);
        v.reveal_third(p, true);
        let lines = v.visible_lines();
        assert!(lines.contains(&500), "{lines:?}");
        let mut out = Vec::new();
        let t = std::time::Instant::now();
        v.render(&mut out, 400, 300, 1.0);
        v.scroll_y += 37.0;
        let t2 = std::time::Instant::now();
        v.render(&mut out, 400, 300, 1.0);
        eprintln!("first {:?}, scrolled {:?}", t2 - t, t2.elapsed());
    }
}
