//! One line of text that ends in "…" when it doesn't fit.
//!
//! iced 0.14's text widget can clip a line but not shorten it. This widget
//! measures the text with the real font while laying out, keeps the longest
//! prefix that fits with an ellipsis, and caches the result until the text or
//! the available width changes.

// Rust guideline compliant 2026-10-09

use std::borrow::Cow;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text::{self as core_text, Paragraph as _};
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::alignment::Vertical;
use iced::{Color, Element, Font, Length, Pixels, Rectangle, Size, mouse};

/// The character added to shortened text.
const ELLIPSIS: char = '…';

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

/// The shaped line and what it was shaped for.
struct State<P> {
    key: Option<(String, Font, f32, f32)>,
    paragraph: P,
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
            paragraph: Renderer::Paragraph::default(),
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
        let key = (self.content.to_string(), font, size, width);
        if state.key.as_ref() != Some(&key) {
            state.paragraph = fit::<Renderer::Paragraph>(&self.content, font, size, width);
            state.key = Some(key);
        }
        let limits = limits.width(Length::Fill).height(Length::Shrink);
        layout::Node::new(limits.resolve(
            Length::Fill,
            Length::Shrink,
            state.paragraph.min_bounds(),
        ))
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
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        renderer.with_layer(clip, |renderer| {
            renderer.fill_paragraph(
                &state.paragraph,
                bounds.position(),
                self.color.unwrap_or(style.text_color),
                clip,
            );
        });
    }
}

/// Shapes `content`, shortened with an ellipsis when wider than `width`.
fn fit<P: core_text::Paragraph<Font = Font>>(
    content: &str,
    font: Font,
    size: f32,
    width: f32,
) -> P {
    let shape = |text: &str| {
        P::with_text(core_text::Text {
            content: text,
            bounds: Size::new(f32::INFINITY, f32::INFINITY),
            size: Pixels(size),
            line_height: core_text::LineHeight::default(),
            font,
            align_x: core_text::Alignment::Default,
            align_y: Vertical::Top,
            shaping: core_text::Shaping::Advanced,
            wrapping: core_text::Wrapping::None,
        })
    };
    let full = shape(content);
    if full.min_width() <= width {
        return full;
    }
    // Binary search for the longest prefix (in characters) that fits with "…".
    let ends: Vec<usize> = content
        .char_indices()
        .map(|(i, _)| i)
        .skip(1)
        .chain([content.len()])
        .collect();
    let shortened = |count: usize| {
        let end = if count == 0 { 0 } else { ends[count - 1] };
        format!("{}{ELLIPSIS}", content[..end].trim_end())
    };
    let (mut low, mut high) = (0, ends.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if shape(&shortened(mid)).min_width() <= width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    shape(&shortened(low))
}

impl<'a, Message, Theme, Renderer> From<Ellipsis<'a>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: core_text::Renderer<Font = Font> + 'a,
{
    fn from(widget: Ellipsis<'a>) -> Self {
        Element::new(widget)
    }
}
