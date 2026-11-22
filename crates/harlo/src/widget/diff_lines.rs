//! A diff's lines, drawn as one widget that only measures what's on screen.
//!
//! Each line has the same height: its number, its `+` or `-`, and its text,
//! which doesn't wrap. So the widget knows its size without measuring any
//! text, and drawing measures and caches only the lines in view. A diff of
//! thousands of lines costs no more to lay out, resize or scroll than one of
//! forty.
//!
//! The widget is as wide as its longest line, for a scrollable around it to
//! scroll across; an added or removed line's tint reaches to the edge of
//! what's visible either way.

// Rust guideline compliant 2026-10-09

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

use harlo_core::diff::{Line, LineKind, Span};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self as core_text, Paragraph as _};
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::alignment::Vertical;
use iced::{Color, Element, Font, Length, Pixels, Point, Rectangle, Size, mouse};

/// The width of the line-number gutter, enough for five digits.
const GUTTER: f32 = 40.0;

/// The space between the gutter's numbers and the marks, and between the
/// marks and the text.
const GAP: f32 = 6.0;

/// The space after the longest line.
const END: f32 = 10.0;

/// The space above and below each line's text.
const LEADING: f32 = 1.0;

/// How many spaces a tab counts as when sizing the widget.
const TAB: usize = 4;

/// The most lines kept measured; scrolling far through a long diff starts
/// again from what's in view.
const KEPT: usize = 2048;

/// The colours of a diff.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    /// Line numbers.
    pub number: Color,
    /// Unchanged lines.
    pub context: Color,
    /// Added and removed lines.
    pub changed: Color,
    /// The gap between hunks.
    pub gap: Color,
    /// The `+` of an added line.
    pub added: Color,
    /// The `-` of a removed line.
    pub removed: Color,
    /// The mark of an unchanged line or gap.
    pub plain: Color,
    /// An added line's background.
    pub added_line: Color,
    /// A removed line's background.
    pub removed_line: Color,
    /// The background of a changed word in an added line.
    pub added_word: Color,
    /// The background of a changed word in a removed line.
    pub removed_word: Color,
}

/// The lines of a diff, with numbers, marks and the changed words lit.
#[derive(Debug)]
pub struct DiffLines<'a> {
    lines: Cow<'a, [Line]>,
    colors: Colors,
    font: Font,
    size: f32,
    number_size: f32,
}

/// Creates a [`DiffLines`] showing `lines` in `font` at `size`, with line
/// numbers at `number_size`.
pub fn diff_lines<'a>(
    lines: impl Into<Cow<'a, [Line]>>,
    colors: Colors,
    font: Font,
    size: f32,
    number_size: f32,
) -> DiffLines<'a> {
    DiffLines {
        lines: lines.into(),
        colors,
        font,
        size,
        number_size,
    }
}

impl DiffLines<'_> {
    /// The height of one line.
    fn line_height(&self) -> f32 {
        core_text::LineHeight::default()
            .to_absolute(Pixels(self.size))
            .0
            + 2.0 * LEADING
    }
}

/// A line measured for drawing, and what it was measured from.
struct Row<P> {
    kind: LineKind,
    number: Option<u32>,
    spans: Vec<Span>,
    numeral: P,
    mark: P,
    body: P,
}

/// What's been measured: the font's character width, the longest line, and
/// the lines drawn lately.
struct Measured<P> {
    /// The font size the widths are for.
    size: Option<u32>,
    /// The width of one character, the font being monospaced.
    advance: f32,
    /// The lines the longest one was found for, by address and count.
    of: Option<(usize, usize)>,
    /// The longest line, in characters.
    longest: usize,
    /// Lines drawn lately, by place.
    rows: HashMap<usize, Row<P>>,
}

/// The widget's state; drawing measures lines as they come into view.
struct State<P> {
    measured: RefCell<Measured<P>>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for DiffLines<'_>
where
    Renderer: core_text::Renderer<Font = Font>,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Paragraph> {
            measured: RefCell::new(Measured {
                size: None,
                advance: 0.0,
                of: None,
                longest: 0,
                rows: HashMap::new(),
            }),
        })
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let measured = state.measured.get_mut();
        let size = Some(self.size.to_bits());
        if measured.size != size {
            let digit: Renderer::Paragraph = shape(&[plain("0")], self.font, self.size);
            measured.advance = digit.min_width();
            measured.size = size;
            measured.rows.clear();
        }
        let of = Some((self.lines.as_ptr() as usize, self.lines.len()));
        if measured.of != of {
            measured.longest = self.lines.iter().map(width_in_chars).max().unwrap_or(0);
            measured.of = of;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a diff's lines and their lengths are far below an f32's precision"
        )]
        let (text, height) = (
            measured.longest as f32 * measured.advance,
            self.lines.len() as f32 * self.line_height(),
        );
        let wide = GUTTER + GAP + measured.advance + GAP + text + END;
        let width = match limits.max().width {
            max if max.is_finite() => max.max(wide),
            _ => wide,
        };
        layout::Node::new(Size::new(width, height))
    }

    /// Reports each line's number and text, as text widgets do.
    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        let bounds = layout.bounds();
        let height = self.line_height();
        let mut y = bounds.y;
        for line in self.lines.iter() {
            let row = Rectangle {
                y,
                height,
                ..bounds
            };
            if let Some(number) = line.number {
                let gutter = Rectangle {
                    width: GUTTER,
                    ..row
                };
                operation.text(None, gutter, &number.to_string());
            }
            operation.text(None, row, &line.text());
            y += height;
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let mut measured = state.measured.borrow_mut();
        let bounds = layout.bounds();
        // Tints reach the visible edge even when the lines are narrower.
        let right = (bounds.x + bounds.width).max(viewport.x + viewport.width);
        let area = Rectangle {
            width: right - bounds.x,
            ..bounds
        };
        let Some(visible) = area.intersection(viewport) else {
            return;
        };
        let height = self.line_height();
        let (first, last) = in_view(visible, bounds.y, height, self.lines.len());
        if measured.rows.len() > KEPT {
            measured.rows.retain(|at, _| (first..last).contains(at));
        }
        let advance = measured.advance;
        let text_x = bounds.x + GUTTER + GAP + advance + GAP;
        for at in first..last {
            let line = &self.lines[at];
            #[expect(
                clippy::cast_precision_loss,
                reason = "a diff's line count is far below an f32's precision"
            )]
            let y = bounds.y + at as f32 * height;
            let row_bounds = Rectangle::new(Point::new(bounds.x, y), Size::new(area.width, height));
            let (tint, word) = match line.kind {
                LineKind::Added => (Some(self.colors.added_line), self.colors.added_word),
                LineKind::Removed => (Some(self.colors.removed_line), self.colors.removed_word),
                LineKind::Context | LineKind::Gap => (None, Color::TRANSPARENT),
            };
            if let Some(tint) = tint {
                renderer.fill_quad(
                    Quad {
                        bounds: row_bounds,
                        ..Quad::default()
                    },
                    tint,
                );
            }
            let row = measured
                .rows
                .entry(at)
                .and_modify(|row| {
                    if row.kind != line.kind || row.number != line.number || row.spans != line.spans
                    {
                        *row = self.row(line);
                    }
                })
                .or_insert_with(|| self.row(line));
            let top = y + LEADING;
            let numeral_x = bounds.x + GUTTER - GAP - row.numeral.min_width();
            renderer.fill_paragraph(
                &row.numeral,
                Point::new(numeral_x, top),
                self.colors.number,
                visible,
            );
            renderer.fill_paragraph(
                &row.mark,
                Point::new(bounds.x + GUTTER + GAP, top),
                self.mark_color(line.kind),
                visible,
            );
            for (index, span) in line.spans.iter().enumerate() {
                if !span.changed || tint.is_none() {
                    continue;
                }
                for piece in row.body.span_bounds(index) {
                    let piece = piece + iced::Vector::new(text_x, top);
                    renderer.fill_quad(
                        Quad {
                            bounds: piece,
                            ..Quad::default()
                        },
                        word,
                    );
                }
            }
            renderer.fill_paragraph(
                &row.body,
                Point::new(text_x, top),
                self.text_color(line.kind),
                visible,
            );
        }
    }
}

impl DiffLines<'_> {
    /// Measures one line.
    fn row<P: core_text::Paragraph<Font = Font>>(&self, line: &Line) -> Row<P> {
        let numeral = line.number.map(|n| n.to_string()).unwrap_or_default();
        let mark = match line.kind {
            LineKind::Added => "+",
            LineKind::Removed => "-",
            LineKind::Context => " ",
            LineKind::Gap => "⋯",
        };
        let spans: Vec<core_text::Span<'_, (), Font>> = line
            .spans
            .iter()
            .map(|span| core_text::Span::new(span.text.as_str()))
            .collect();
        Row {
            kind: line.kind,
            number: line.number,
            spans: line.spans.clone(),
            numeral: shape(&[plain(&numeral)], self.font, self.number_size),
            mark: shape(&[plain(mark)], self.font, self.size),
            body: shape(&spans, self.font, self.size),
        }
    }

    /// The colour of a line's `+` or `-`.
    fn mark_color(&self, kind: LineKind) -> Color {
        match kind {
            LineKind::Added => self.colors.added,
            LineKind::Removed => self.colors.removed,
            LineKind::Context | LineKind::Gap => self.colors.plain,
        }
    }

    /// The colour of a line's text.
    fn text_color(&self, kind: LineKind) -> Color {
        match kind {
            LineKind::Added | LineKind::Removed => self.colors.changed,
            LineKind::Context => self.colors.context,
            LineKind::Gap => self.colors.gap,
        }
    }
}

/// One plain span of `text`.
fn plain(text: &str) -> core_text::Span<'_, (), Font> {
    core_text::Span::new(text)
}

/// Shapes `spans` as one line that doesn't wrap.
fn shape<P: core_text::Paragraph<Font = Font>>(
    spans: &[core_text::Span<'_, (), Font>],
    font: Font,
    size: f32,
) -> P {
    P::with_spans(core_text::Text {
        content: spans,
        bounds: Size::new(f32::INFINITY, f32::INFINITY),
        size: Pixels(size),
        line_height: core_text::LineHeight::default(),
        font,
        align_x: core_text::Alignment::Default,
        align_y: Vertical::Top,
        shaping: core_text::Shaping::Advanced,
        wrapping: core_text::Wrapping::None,
    })
}

/// A line's width in characters, a tab counting as a few.
fn width_in_chars(line: &Line) -> usize {
    line.spans
        .iter()
        .flat_map(|span| span.text.chars())
        .map(|c| if c == '\t' { TAB } else { 1 })
        .sum()
}

/// The lines that `visible` shows, as a range, for lines of `height` from `top`.
fn in_view(visible: Rectangle, top: f32, height: f32, count: usize) -> (usize, usize) {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to zero or more first; a diff's line count fits a usize"
    )]
    let at = |y: f32| ((y - top) / height).max(0.0) as usize;
    let first = at(visible.y).min(count);
    let last = (at(visible.y + visible.height) + 1).min(count);
    (first, last)
}

impl<'a, Message, Theme, Renderer> From<DiffLines<'a>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: core_text::Renderer<Font = Font> + 'a,
{
    fn from(widget: DiffLines<'a>) -> Self {
        Element::new(widget)
    }
}
