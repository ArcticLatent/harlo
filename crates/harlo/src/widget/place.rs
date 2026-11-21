//! A widget placed at a vertical offset, laid out at its full height.
//!
//! `pin` limits its content to the space left below its offset, so in a
//! stack sized by an estimate the content is squeezed to that estimate and
//! measures wrongly. `place` lays its content out with unbounded height
//! instead, so a `sensor` inside reports the real size.
//!
//! Adapted from iced's `pin` widget (MIT).

// Rust guideline compliant 2026-10-09

use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// Content at a vertical offset, as tall as it needs to be.
#[expect(
    missing_debug_implementations,
    reason = "holds an Element, which has no Debug"
)]
pub struct Place<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    y: f32,
}

/// Places `content` at `y`, full width, at whatever height it lays out to.
pub fn place<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    y: f32,
) -> Place<'a, Message, Theme, Renderer> {
    Place {
        content: content.into(),
        y,
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Place<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> widget::tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<widget::Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut widget::Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        let unbounded = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
        let node = self
            .content
            .as_widget_mut()
            .layout(tree, renderer, &unbounded)
            .move_to(Point::new(0.0, self.y));
        let size = Size::new(width, self.y + node.size().height);
        layout::Node::with_children(size, vec![node])
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if let Some(content) = layout.children().next() {
            self.content
                .as_widget_mut()
                .operate(tree, content, renderer, operation);
        }
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Some(content) = layout.children().next() {
            self.content.as_widget_mut().update(
                tree, event, content, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        layout
            .children()
            .next()
            .map_or_else(mouse::Interaction::default, |content| {
                self.content
                    .as_widget()
                    .mouse_interaction(tree, content, cursor, viewport, renderer)
            })
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(content) = layout.children().next() {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, content, cursor, viewport);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let content = layout.children().next()?;
        self.content
            .as_widget_mut()
            .overlay(tree, content, renderer, viewport, translation)
    }
}

impl<'a, Message, Theme, Renderer> From<Place<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(place: Place<'a, Message, Theme, Renderer>) -> Self {
        Element::new(place)
    }
}
