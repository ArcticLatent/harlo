//! A panel that slides in from the side of the window, showing part of itself.
//!
//! A narrower container would squeeze a panel's content as it opens. `reveal`
//! lays the content out at its full width, takes only the width being shown,
//! and draws the content clipped to it, sliding in from one edge.

// Rust guideline compliant 2026-10-09

use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// The window edge a revealed panel slides in from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// It slides in from the left: its right side shows first.
    Left,
    /// It slides in from the right: its left side shows first.
    Right,
}

/// Content `full` pixels wide, of which `shown` pixels are on screen.
#[expect(
    missing_debug_implementations,
    reason = "holds an Element, which has no Debug"
)]
pub struct Reveal<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    full: f32,
    shown: f32,
    edge: Edge,
}

/// Shows `shown` of `content`'s `full` width, sliding in from `edge`.
pub fn reveal<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    full: f32,
    shown: f32,
    edge: Edge,
) -> Reveal<'a, Message, Theme, Renderer> {
    Reveal {
        content: content.into(),
        full,
        shown: shown.clamp(0.0, full),
        edge,
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Reveal<'_, Message, Theme, Renderer>
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
        Size::new(Length::Fixed(self.shown), Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let height = limits.max().height;
        let full = layout::Limits::new(Size::ZERO, Size::new(self.full, height));
        let x = match self.edge {
            Edge::Left => self.shown - self.full,
            Edge::Right => 0.0,
        };
        let node = self
            .content
            .as_widget_mut()
            .layout(tree, renderer, &full)
            .move_to(Point::new(x, 0.0));
        layout::Node::with_children(Size::new(self.shown, height), vec![node])
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
        // The hidden part takes no input: the cursor counts only over what shows.
        let cursor = if cursor.is_over(layout.bounds()) {
            cursor
        } else {
            cursor.levitate()
        };
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
        if !cursor.is_over(layout.bounds()) {
            return mouse::Interaction::default();
        }
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
        let bounds = layout.bounds();
        let Some(content) = layout.children().next() else {
            return;
        };
        let Some(visible) = bounds.intersection(viewport) else {
            return;
        };
        renderer.with_layer(visible, |renderer| {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, content, cursor, &visible);
        });
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

impl<'a, Message, Theme, Renderer> From<Reveal<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(reveal: Reveal<'a, Message, Theme, Renderer>) -> Self {
        Element::new(reveal)
    }
}
