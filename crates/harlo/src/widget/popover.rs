//! A menu that opens from the widget it belongs to, lined up with its left edge.
//!
//! The menu opens below its anchor, or above it when there's more room there,
//! and stays inside the window, taking at most the height available on its
//! side. A click outside both the menu and the anchor, or Escape, sends the
//! dismiss message. Clicks and scrolls over the menu stay with the menu.
//!
//! The overlay's event handling is adapted from `iced_aw`'s `drop_down`
//! widget (MIT, copyright the `iced_aw` contributors).

// Rust guideline compliant 2026-10-09

use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, overlay, renderer};
use iced::keyboard::{self, key::Named};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, touch};

/// The space between the anchor and its menu.
const GAP: f32 = 6.0;

/// The space the menu keeps from the window's edges.
const MARGIN: f32 = 8.0;

/// An anchor that may show a menu.
#[expect(
    missing_debug_implementations,
    reason = "holds Elements, which have no Debug"
)]
pub struct Popover<'a, Message, Theme, Renderer> {
    anchor: Element<'a, Message, Theme, Renderer>,
    menu: Option<Element<'a, Message, Theme, Renderer>>,
    on_dismiss: Message,
}

/// Shows `menu` from `anchor` while it's `Some`; `on_dismiss` closes it.
pub fn popover<'a, Message, Theme, Renderer>(
    anchor: impl Into<Element<'a, Message, Theme, Renderer>>,
    menu: Option<Element<'a, Message, Theme, Renderer>>,
    on_dismiss: Message,
) -> Popover<'a, Message, Theme, Renderer> {
    Popover {
        anchor: anchor.into(),
        menu,
        on_dismiss,
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Popover<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: iced::advanced::Renderer + 'a,
    Theme: 'a,
{
    fn children(&self) -> Vec<widget::Tree> {
        let mut children = vec![widget::Tree::new(&self.anchor)];
        if let Some(menu) = &self.menu {
            children.push(widget::Tree::new(menu));
        }
        children
    }

    fn diff(&self, tree: &mut widget::Tree) {
        match &self.menu {
            Some(menu) => tree.diff_children(&[&self.anchor, menu]),
            None => tree.diff_children(&[&self.anchor]),
        }
    }

    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.anchor.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.anchor
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.anchor
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        self.anchor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
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
        self.anchor.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
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
        self.anchor.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let menu = self.menu.as_mut()?;
        let state = tree.children.get_mut(1)?;
        Some(overlay::Element::new(Box::new(Menu {
            state,
            content: menu,
            anchor: layout.bounds() + translation,
            on_dismiss: &self.on_dismiss,
        })))
    }
}

impl<'a, Message, Theme, Renderer> From<Popover<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(popover: Popover<'a, Message, Theme, Renderer>) -> Self {
        Element::new(popover)
    }
}

/// The open menu, drawn over everything else.
struct Menu<'a, 'b, Message, Theme, Renderer> {
    state: &'b mut widget::Tree,
    content: &'b mut Element<'a, Message, Theme, Renderer>,
    /// Where the anchor is in the window.
    anchor: Rectangle,
    on_dismiss: &'b Message,
}

impl<Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for Menu<'_, '_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let anchor = self.anchor;
        let below = bounds.height - (anchor.y + anchor.height + GAP) - MARGIN;
        let above = anchor.y - GAP - MARGIN;
        let max_width = (bounds.width - 2.0 * MARGIN).max(0.0);
        let measure = |menu: &mut Element<'_, Message, Theme, Renderer>,
                       state: &mut widget::Tree,
                       height: f32| {
            let limits = layout::Limits::new(
                Size::new(anchor.width.min(max_width), 0.0),
                Size::new(max_width, height.max(0.0)),
            );
            menu.as_widget_mut().layout(state, renderer, &limits)
        };
        // Below unless it fits only above, or there's more room above.
        let natural = measure(self.content, self.state, bounds.height);
        let opens_below = natural.size().height <= below || below >= above;
        let node = measure(
            self.content,
            self.state,
            if opens_below { below } else { above },
        );
        let size = node.size();
        let x = anchor.x.min(bounds.width - MARGIN - size.width).max(MARGIN);
        let y = if opens_below {
            anchor.y + anchor.height + GAP
        } else {
            anchor.y - GAP - size.height
        };
        node.move_to(Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        self.content
            .as_widget()
            .draw(self.state, renderer, theme, style, layout, cursor, &bounds);
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(self.state, layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let bounds = layout.bounds();
        let over = cursor.is_over(bounds);
        match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(Named::Escape),
                ..
            }) => {
                shell.publish(self.on_dismiss.clone());
                shell.capture_event();
                return;
            }
            // A click elsewhere closes the menu and still does what it does
            // there; a click on the anchor is the anchor's to handle.
            Event::Mouse(mouse::Event::ButtonPressed(_))
            | Event::Touch(touch::Event::FingerPressed { .. })
                if !over && !cursor.is_over(self.anchor) =>
            {
                shell.publish(self.on_dismiss.clone());
                return;
            }
            _ => {}
        }
        self.content.as_widget_mut().update(
            self.state, event, layout, cursor, renderer, clipboard, shell, &bounds,
        );
        // What happens over the menu stays with the menu.
        let pointer = matches!(
            event,
            Event::Mouse(
                mouse::Event::ButtonPressed(_)
                    | mouse::Event::ButtonReleased(_)
                    | mouse::Event::WheelScrolled { .. }
            ) | Event::Touch(_)
        );
        if over && pointer {
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        if !cursor.is_over(bounds) {
            return mouse::Interaction::None;
        }
        // Over the menu, the window beneath gets no hover.
        match self
            .content
            .as_widget()
            .mouse_interaction(self.state, layout, cursor, &bounds, renderer)
        {
            mouse::Interaction::None => mouse::Interaction::Idle,
            interaction => interaction,
        }
    }
}
