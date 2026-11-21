//! A widget laid out narrower than it asks to be, by a fixed amount.
//!
//! A pick list counts its left padding twice: as padding, and again as the
//! gap before its arrow. A chip whose left padding makes room for an icon
//! would get a wide gap; `snug` lays the list out once to learn its width,
//! then again that much narrower, which the list honours.

// Rust guideline compliant 2026-10-09

use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// Content laid out `trim` pixels narrower than its own width.
#[expect(
    missing_debug_implementations,
    reason = "holds an Element, which has no Debug"
)]
pub struct Snug<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    trim: f32,
}

/// Lays `content` out `trim` pixels narrower than it would be.
pub fn snug<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    trim: f32,
) -> Snug<'a, Message, Theme, Renderer> {
    Snug {
        content: content.into(),
        trim,
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Snug<'_, Message, Theme, Renderer>
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
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let natural = self
            .content
            .as_widget_mut()
            .layout(tree, renderer, limits)
            .size()
            .width;
        let narrower = limits.max_width((natural - self.trim).max(0.0));
        self.content
            .as_widget_mut()
            .layout(tree, renderer, &narrower)
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
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
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a, Message, Theme, Renderer> From<Snug<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(snug: Snug<'a, Message, Theme, Renderer>) -> Self {
        Element::new(snug)
    }
}
