//! One line of text that ends in "…" when it doesn't fit.
//!
//! iced 0.14's text widget can clip a line but not shorten it. This widget
//! shapes the whole line and an ellipsis once, for the text, font and size.
//! When the line is too wide it draws the longest run of whole characters
//! that fits, clipped from the shaped line, with the ellipsis after it. A new
//! width only moves the cut, found from the glyph positions, so resizing
//! never shapes text again.

// Rust guideline compliant 2026-10-09

use std::borrow::Cow;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text::{self as core_text, Paragraph as _};
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::alignment::Vertical;
use iced::{Color, Element, Font, Length, Pixels, Point, Rectangle, Size, mouse};
use unicode_segmentation::UnicodeSegmentation;

/// What shortened text ends with.
const ELLIPSIS: &str = "…";

/// A single line of text, shortened with "…" to fit its width.
#[derive(Debug)]
pub struct Ellipsis<'a> {
    content: Cow<'a, str>,
    font: Option<Font>,
    size: Option<f32>,
    color: Option<Color>,
}

/// Creates an [`Ellipsis`] showing `content`.
pub fn ellipsis<'a>(content: impl Into<Cow<'a, str>>) -> Ellipsis<'a> {
    Ellipsis {
        content: content.into(),
        font: None,
        size: None,
        color: None,
    }
}

impl Ellipsis<'_> {
    /// Sets the font; the app's default font otherwise.
    #[must_use]
    pub fn font(mut self, font: Font) -> Self {
        self.font = Some(font);
        self
    }

    /// Sets the text size in logical pixels; the default size otherwise.
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets the colour; the surrounding text colour otherwise.
    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// The shaped line and ellipsis, what they were shaped for, and where the
/// line is cut for the last width.
struct State<P> {
    key: Option<(String, Font, f32)>,
    line: P,
    dots: P,
    /// The width the cut was found for.
    width: f32,
    /// How much of the line shows when it's shortened.
    cut: Option<f32>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Ellipsis<'_>
where
    Renderer: core_text::Renderer<Font = Font>,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Paragraph> {
            key: None,
            line: Renderer::Paragraph::default(),
            dots: Renderer::Paragraph::default(),
            width: f32::NAN,
            cut: None,
        })
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let font = self.font.unwrap_or_else(|| renderer.default_font());
        let size = self.size.unwrap_or_else(|| renderer.default_size().0);
        let width = limits.max().width;
        let fresh = state.key.as_ref().is_none_or(|(content, f, s)| {
            content.as_str() != self.content.as_ref() || *f != font || s.to_bits() != size.to_bits()
        });
        if fresh {
            state.line = shape(&self.content, font, size);
            state.dots = shape(ELLIPSIS, font, size);
            state.key = Some((self.content.to_string(), font, size));
        }
        if fresh || state.width.to_bits() != width.to_bits() {
            state.cut = cut(&state.line, &state.dots, &self.content, width);
            state.width = width;
        }
        let limits = limits.width(Length::Fill).height(Length::Shrink);
        layout::Node::new(limits.resolve(Length::Fill, Length::Shrink, state.line.min_bounds()))
    }

    /// Reports the full, unshortened text, as iced's own text widget does.
    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.text(None, layout.bounds(), &self.content);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let bounds = layout.bounds();
        let color = self.color.unwrap_or(style.text_color);
        let shown = match state.cut {
            Some(cut) => Rectangle {
                width: cut,
                ..bounds
            },
            None => bounds,
        };
        if let Some(clip) = shown.intersection(viewport) {
            renderer.with_layer(clip, |renderer| {
                renderer.fill_paragraph(&state.line, bounds.position(), color, clip);
            });
        }
        if let Some(cut) = state.cut {
            let at = Point::new(bounds.x + cut, bounds.y);
            if let Some(clip) = bounds.intersection(viewport) {
                renderer.with_layer(clip, |renderer| {
                    renderer.fill_paragraph(&state.dots, at, color, clip);
                });
            }
        }
    }
}

/// Shapes one line of `content`.
fn shape<P: core_text::Paragraph<Font = Font>>(content: &str, font: Font, size: f32) -> P {
    P::with_text(core_text::Text {
        content,
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

/// Where to cut `line` (the shaped `content`) so that it and `dots` fit in
/// `width`: after the most whole characters that fit, less any spaces
/// before the ellipsis. `None` when the whole line fits.
fn cut<P: core_text::Paragraph>(line: &P, dots: &P, content: &str, width: f32) -> Option<f32> {
    if line.min_width() <= width {
        return None;
    }
    let room = (width - dots.min_width()).max(0.0);
    let graphemes: Vec<&str> = content.graphemes(true).collect();
    // How far the first `count` characters reach.
    let reach = |count: usize| {
        if count == 0 {
            0.0
        } else {
            line.grapheme_position(0, count)
                .map_or(f32::INFINITY, |p| p.x)
        }
    };
    let (mut low, mut high) = (0, graphemes.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if reach(mid) <= room {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    while low > 0 && graphemes[low - 1].trim().is_empty() {
        low -= 1;
    }
    Some(reach(low))
}

impl<'a, Message, Theme, Renderer> From<Ellipsis<'a>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: core_text::Renderer<Font = Font> + 'a,
{
    fn from(widget: Ellipsis<'a>) -> Self {
        Element::new(widget)
    }
}
