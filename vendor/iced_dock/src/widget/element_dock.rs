use std::cell::RefCell;
use std::rc::Rc;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::tree::{State, Tag, Tree};
use iced::advanced::widget::{Operation, Widget};
use iced::advanced::{self, Clipboard, Shell};
use iced::mouse::{self, Cursor};
use iced::touch;
use iced::{Border, Element, Event, Length, Point, Rectangle, Size, Vector};

use crate::manager::{DockManager, DropZone};
use crate::model::{HandleSide, NodeId};
use crate::style::{self, Catalog, DockStyle};
use crate::widget::action::{DockAction, TabAction};
use crate::widget::compose;
use crate::widget::state::DockWidgetState;
use crate::widget::tab_dock::drop_zone_rect;

/// Grip dot diameter and the gap between dots.
const GRIP_DOT: f32 = 3.0;
const GRIP_GAP: f32 = 3.0;
const GRIP_DOTS: usize = 3;

#[derive(Default)]
struct ElementDockState {
    /// Handle strip, relative to the pane origin (set in `layout`).
    handle: Rectangle,
    /// Press position on the handle while a drag is pending.
    press: Option<Point>,
    hovered: bool,
}

/// Element pane: one panel, no tab strip, moved by dragging a handle strip on one side.
pub struct ElementDock<'a, K, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Theme: Catalog,
    Renderer: advanced::Renderer,
{
    dock_state: Rc<RefCell<DockWidgetState<K>>>,
    pane_id: NodeId,
    panel: NodeId,
    can_drag: bool,
    content: Element<'a, Message, Theme, Renderer>,
    on_event: Rc<dyn Fn(DockAction) -> Message>,
    class: Rc<<Theme as Catalog>::Class<'static>>,
    theme: Rc<RefCell<Option<Theme>>>,
    side: HandleSide,
    handle_size: f32,
    separator_height: f32,
    pane_padding: f32,
    drag_threshold: f32,
    drop_edge_fraction: f32,
}

impl<'a, K, Message, Theme, Renderer> ElementDock<'a, K, Message, Theme, Renderer>
where
    Theme: Catalog + Clone,
    Renderer: advanced::Renderer,
{
    pub(crate) fn new(
        dock_state: Rc<RefCell<DockWidgetState<K>>>,
        pane_id: NodeId,
        panel: NodeId,
        can_drag: bool,
        content: Element<'a, Message, Theme, Renderer>,
        on_event: Rc<dyn Fn(DockAction) -> Message>,
        class: Rc<<Theme as Catalog>::Class<'static>>,
        theme: Rc<RefCell<Option<Theme>>>,
        side: HandleSide,
        handle_size: f32,
        separator_height: f32,
        pane_padding: f32,
        drag_threshold: f32,
        drop_edge_fraction: f32,
    ) -> Self {
        Self {
            dock_state,
            pane_id,
            panel,
            can_drag,
            content,
            on_event,
            class,
            theme,
            side,
            handle_size,
            separator_height,
            pane_padding,
            drag_threshold,
            drop_edge_fraction,
        }
    }

    fn layout_style_or_default(&self) -> DockStyle {
        match self.theme.borrow().as_ref() {
            Some(t) => Catalog::style(t, &self.class),
            None => style::default(&iced::Theme::Dark),
        }
    }

    /// See [`TabDock`](crate::widget::tab_dock::TabDock): a drag owns the pointer, so the
    /// content is blinded for its duration.
    fn content_cursor(&self, cursor: Cursor) -> Cursor {
        if self.dock_state.borrow().drag.is_some() {
            Cursor::Unavailable
        } else {
            cursor
        }
    }

    fn is_picked(&self) -> bool {
        self.dock_state
            .borrow()
            .drag
            .is_some_and(|session| session.source_pane == self.pane_id)
    }
}

/// Split `inner` into the handle strip on `side` and the remaining content area.
fn split_handle(inner: Rectangle, side: HandleSide, size: f32) -> (Rectangle, Rectangle) {
    match side {
        HandleSide::Left | HandleSide::Right => {
            let w = size.min(inner.width);
            let rest = inner.width - w;
            let (hx, cx) = if side == HandleSide::Left {
                (inner.x, inner.x + w)
            } else {
                (inner.x + rest, inner.x)
            };
            (
                Rectangle {
                    x: hx,
                    width: w,
                    ..inner
                },
                Rectangle {
                    x: cx,
                    width: rest,
                    ..inner
                },
            )
        }
        HandleSide::Top | HandleSide::Bottom => {
            let h = size.min(inner.height);
            let rest = inner.height - h;
            let (hy, cy) = if side == HandleSide::Top {
                (inner.y, inner.y + h)
            } else {
                (inner.y + rest, inner.y)
            };
            (
                Rectangle {
                    y: hy,
                    height: h,
                    ..inner
                },
                Rectangle {
                    y: cy,
                    height: rest,
                    ..inner
                },
            )
        }
    }
}

/// Line between the handle and the content, on the handle's inner edge.
fn separator_rect(handle: Rectangle, side: HandleSide, thickness: f32) -> Rectangle {
    match side {
        HandleSide::Left => Rectangle {
            x: handle.x + handle.width - thickness,
            width: thickness,
            ..handle
        },
        HandleSide::Right => Rectangle {
            width: thickness,
            ..handle
        },
        HandleSide::Top => Rectangle {
            y: handle.y + handle.height - thickness,
            height: thickness,
            ..handle
        },
        HandleSide::Bottom => Rectangle {
            height: thickness,
            ..handle
        },
    }
}

/// Grip dots laid out along the handle's long axis, centered.
fn grip_dots(handle: Rectangle, side: HandleSide) -> impl Iterator<Item = Rectangle> {
    let span = GRIP_DOTS as f32 * GRIP_DOT + (GRIP_DOTS - 1) as f32 * GRIP_GAP;
    let center = handle.center();
    let vertical = matches!(side, HandleSide::Left | HandleSide::Right);
    (0..GRIP_DOTS).map(move |i| {
        let along = -span / 2.0 + i as f32 * (GRIP_DOT + GRIP_GAP);
        let (x, y) = if vertical {
            (center.x - GRIP_DOT / 2.0, center.y + along)
        } else {
            (center.x + along, center.y - GRIP_DOT / 2.0)
        };
        Rectangle {
            x,
            y,
            width: GRIP_DOT,
            height: GRIP_DOT,
        }
    })
}

impl<K, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ElementDock<'_, K, Message, Theme, Renderer>
where
    K: 'static,
    Message: Clone + 'static,
    Theme: Catalog + Clone + 'static,
    Renderer: advanced::Renderer + 'static,
{
    fn tag(&self) -> Tag {
        Tag::of::<ElementDockState>()
    }

    fn state(&self) -> State {
        State::new(ElementDockState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let style = self.layout_style_or_default();
        let max = limits.max();
        let inset = self.pane_padding + style.window.border.width;
        let inner = Rectangle {
            x: inset,
            y: inset,
            width: (max.width - 2.0 * inset).max(0.0),
            height: (max.height - 2.0 * inset).max(0.0),
        };
        let (handle, content) = split_handle(inner, self.side, self.handle_size);
        tree.state.downcast_mut::<ElementDockState>().handle = handle;

        let content_limits = layout::Limits::new(Size::ZERO, content.size());
        let mut content_node = compose::child_layout(
            &mut self.content,
            &mut tree.children[0],
            renderer,
            &content_limits,
        );
        content_node.move_to_mut((content.x, content.y));
        layout::Node::with_children(max, vec![content_node])
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let pane_bounds = layout.bounds();
        self.dock_state
            .borrow_mut()
            .pane_bounds
            .push((self.pane_id, pane_bounds));

        let dock_style = Catalog::style(theme, &self.class);
        let state = tree.state.downcast_ref::<ElementDockState>();
        let window = &dock_style.window;
        let is_focused = self.dock_state.borrow().focus_frame_pane == Some(self.pane_id);
        let border = if is_focused {
            window.focused_border.unwrap_or(window.border)
        } else {
            window.border
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: pane_bounds,
                border,
                ..renderer::Quad::default()
            },
            window.background,
        );

        let handle = state.handle + Vector::new(pane_bounds.x, pane_bounds.y);
        let picked = self.is_picked();
        let (handle_bg, grip) = if picked || state.press.is_some() {
            (
                dock_style.tab.pressed_background,
                dock_style.tab.pressed_text,
            )
        } else if state.hovered && self.can_drag {
            (
                dock_style.tab.hovered_background,
                dock_style.tab.hovered_text,
            )
        } else {
            (dock_style.tab_bar.background, dock_style.tab.inactive_text)
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: handle,
                ..renderer::Quad::default()
            },
            handle_bg,
        );
        if let Some(separator) = dock_style.tab_bar.separator {
            if self.separator_height > 0.0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: separator_rect(handle, self.side, self.separator_height),
                        ..renderer::Quad::default()
                    },
                    separator,
                );
            }
        }
        if self.can_drag {
            for dot in grip_dots(handle, self.side) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: dot,
                        border: Border {
                            radius: (GRIP_DOT / 2.0).into(),
                            ..Border::default()
                        },
                        ..renderer::Quad::default()
                    },
                    grip,
                );
            }
        }

        if let Some(content_layout) = layout.children().next() {
            compose::child_draw(
                &self.content,
                &tree.children[0],
                renderer,
                theme,
                style,
                content_layout,
                self.content_cursor(cursor),
                viewport,
            );
        }

        let Some(session) = self.dock_state.borrow().drag else {
            return;
        };
        if session.tab_insert.is_some() || session.hover_target != Some(self.pane_id) {
            return;
        }
        // Harlo: the hint is the drop the session will make.
        let Some(zone) = session.operation.map(DropZone::from_operation) else {
            return;
        };
        let blocked = !DockManager.validate(
            &self.dock_state.borrow().layout,
            session.source_pane,
            session.source_panel,
            self.pane_id,
            zone.to_operation(),
        );
        let overlay = &dock_style.drop_overlay;
        let (highlight, outline) = if blocked {
            (overlay.blocked_color, overlay.blocked_border_color)
        } else {
            (overlay.color, overlay.border_color)
        };
        let zone_bounds = drop_zone_rect(pane_bounds, zone, self.drop_edge_fraction);
        // Own layer so the highlight lands above canvas-based content (see `TabDock::draw`).
        renderer.with_layer(pane_bounds, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: zone_bounds,
                    border: Border {
                        width: overlay.border_width,
                        color: outline,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                },
                highlight,
            );
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let pane_bounds = layout.bounds();
        let dragging = self.dock_state.borrow().drag.is_some();
        self.dock_state
            .borrow_mut()
            .drop_targets
            .push((self.pane_id, pane_bounds));

        if let Some(content_layout) = layout.children().next() {
            if !self.is_picked() {
                let content_cursor = self.content_cursor(cursor);
                compose::child_update(
                    &mut self.content,
                    &mut tree.children[0],
                    event,
                    content_layout,
                    content_cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
        }

        let state = tree.state.downcast_mut::<ElementDockState>();
        let handle = state.handle + Vector::new(pane_bounds.x, pane_bounds.y);
        let over_handle = cursor.position_over(handle).is_some();
        if over_handle != state.hovered {
            state.hovered = over_handle;
            shell.request_redraw();
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. })
                if !dragging =>
            {
                if let Some(pos) = cursor.position_over(pane_bounds) {
                    if over_handle && self.can_drag {
                        state.press = Some(pos);
                    }
                    shell.capture_event();
                    // Harlo: only a click that moves the focus says so.
                    if self.dock_state.borrow().focused_pane != Some(self.pane_id) {
                        shell.publish((self.on_event)(DockAction::PaneFocused {
                            pane: self.pane_id,
                            panel: Some(self.panel),
                        }));
                    }
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some(start), Some(pos)) = (state.press, cursor.position()) {
                    if start.distance(pos) >= self.drag_threshold {
                        state.press = None;
                        shell.publish((self.on_event)(DockAction::Tab(TabAction::DragStarted {
                            source_pane: self.pane_id,
                            source_panel: self.panel,
                            drop_edge_fraction: self.drop_edge_fraction,
                        })));
                        shell.capture_event();
                        shell.request_redraw();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. })
                if state.press.take().is_some() =>
            {
                shell.request_redraw();
            }
            _ => {}
        }

        // Drag bookkeeping mirrors `TabDock`, so an element-only layout still completes drags.
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        ) && self.dock_state.borrow().drag.is_some()
        {
            if let Some(pos) = cursor.position() {
                shell.publish((self.on_event)(DockAction::Tab(TabAction::DragEnded {
                    cursor: pos,
                })));
            } else {
                shell.publish((self.on_event)(DockAction::Tab(TabAction::DragCancelled)));
            }
            shell.invalidate_layout();
            shell.invalidate_widgets();
            shell.request_redraw();
        }

        if dragging {
            if let Event::Mouse(mouse::Event::CursorMoved { .. }) = event {
                if let Some(pos) = cursor.position() {
                    shell.publish((self.on_event)(DockAction::Tab(TabAction::DragMoved {
                        cursor: pos,
                    })));
                    shell.request_redraw();
                }
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<ElementDockState>();
        if self.dock_state.borrow().drag.is_some() || state.press.is_some() {
            return mouse::Interaction::Grab;
        }
        let bounds = layout.bounds();
        let handle = state.handle + Vector::new(bounds.x, bounds.y);
        if self.can_drag && cursor.is_over(handle) {
            return mouse::Interaction::Grab;
        }
        layout
            .children()
            .next()
            .map_or(mouse::Interaction::None, |content_layout| {
                compose::child_mouse_interaction(
                    &self.content,
                    &tree.children[0],
                    content_layout,
                    cursor,
                    viewport,
                    renderer,
                )
            })
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(content_layout) = layout.children().next() {
            compose::child_operate(
                &mut self.content,
                &mut tree.children[0],
                content_layout,
                renderer,
                operation,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let content_layout = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            content_layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, K, Message, Theme, Renderer> From<ElementDock<'a, K, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    K: 'static,
    Message: Clone + 'static,
    Theme: Catalog + Clone + 'static,
    Renderer: advanced::Renderer + 'static,
{
    fn from(widget: ElementDock<'a, K, Message, Theme, Renderer>) -> Self {
        Element::new(widget)
    }
}
