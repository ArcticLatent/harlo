//! Drag validation and drop execution.

use iced::Rectangle;

use crate::factory::Factory;
use crate::model::{Axis, DockOperation, Layout, NodeId, NodeKind};
use crate::{Error, Result};

/// Drop zone within a target rect (edge bands + center).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl DropZone {
    #[must_use]
    pub fn from_operation(op: DockOperation) -> Self {
        match op {
            DockOperation::Fill => Self::Center,
            DockOperation::Left => Self::Left,
            DockOperation::Right => Self::Right,
            DockOperation::Top => Self::Top,
            DockOperation::Bottom => Self::Bottom,
        }
    }

    #[must_use]
    pub fn to_operation(self) -> DockOperation {
        match self {
            Self::Center => DockOperation::Fill,
            Self::Left => DockOperation::Left,
            Self::Right => DockOperation::Right,
            Self::Top => DockOperation::Top,
            Self::Bottom => DockOperation::Bottom,
        }
    }
}

/// Tab bar drop target registered during layout/update.
#[derive(Debug, Clone)]
pub struct TabBarTarget {
    pub pane: NodeId,
    pub bounds: Rectangle,
    /// Layout-space X for each insertion slot (same coordinates as tab layout bounds).
    pub insert_x: Vec<f32>,
    pub scroll_offset: f32,
}

/// Active tab drag session.
#[derive(Debug, Clone, Copy)]
pub struct DragSession {
    pub source_pane: NodeId,
    pub source_panel: NodeId,
    pub hover_target: Option<NodeId>,
    pub operation: Option<DockOperation>,
    pub tab_insert: Option<(NodeId, usize)>,
    /// Edge band size for content drop zones.
    pub drop_edge_fraction: f32,
}

impl DragSession {
    #[must_use]
    pub fn new(source_pane: NodeId, source_panel: NodeId, drop_edge_fraction: f32) -> Self {
        Self {
            source_pane,
            source_panel,
            hover_target: None,
            operation: None,
            tab_insert: None,
            drop_edge_fraction: drop_edge_fraction.clamp(0.0, 0.5),
        }
    }
}

/// Validates and executes dock drops.
#[derive(Debug, Clone, Copy, Default)]
pub struct DockManager;

impl DockManager {
    fn pane_tab_count<K>(&self, layout: &Layout<K>, pane: NodeId) -> usize {
        match layout.kind(pane) {
            Some(NodeKind::Pane(p)) => p.tabs.len(),
            _ => 0,
        }
    }

    pub(crate) fn is_element<K>(&self, layout: &Layout<K>, pane: NodeId) -> bool {
        matches!(layout.kind(pane), Some(NodeKind::Pane(p)) if p.is_element())
    }

    /// Harlo: whether `pane` is anchored (see [`crate::model::Pane::anchored`]).
    pub(crate) fn is_anchored<K>(&self, layout: &Layout<K>, pane: NodeId) -> bool {
        matches!(layout.kind(pane), Some(NodeKind::Pane(p)) if p.anchored)
    }

    /// Harlo: whether `panel` sits in an anchored pane.
    fn in_anchored<K>(&self, layout: &Layout<K>, panel: NodeId) -> bool {
        layout
            .get(panel)
            .and_then(|e| e.owner)
            .is_some_and(|owner| self.is_anchored(layout, owner))
    }

    /// Harlo: drops the drag's hover when nothing may land there, so no hint shows and a
    /// release there does nothing.
    ///
    /// A tab from an anchored pane only reorders within its own strip. Without `content_fill`,
    /// the middle of a pane takes nothing; tabs join through the strip.
    pub fn keep_valid_hover<K>(
        &self,
        session: &mut DragSession,
        layout: &Layout<K>,
        content_fill: bool,
    ) {
        let from_anchored = self.is_anchored(layout, session.source_pane);
        if let Some((pane, _)) = session.tab_insert {
            let valid = if pane == session.source_pane {
                !self.is_element(layout, pane)
            } else {
                !from_anchored && self.fill_allowed(layout, session.source_panel, pane)
            };
            if !valid {
                session.tab_insert = None;
            }
        }
        if let (Some(target), Some(op)) = (session.hover_target, session.operation) {
            let valid = !from_anchored
                && (content_fill || op != DockOperation::Fill)
                && self.validate(
                    layout,
                    session.source_pane,
                    session.source_panel,
                    target,
                    op,
                );
            if !valid {
                session.hover_target = None;
                session.operation = None;
            }
        }
    }

    /// Whether `panel` may join `target_pane` as a tab (center drop or tab-bar insert).
    ///
    /// Element panes are not tab groups: they never take a panel, and their own panel never
    /// leaves to join one.
    pub(crate) fn fill_allowed<K>(
        &self,
        layout: &Layout<K>,
        panel: NodeId,
        target_pane: NodeId,
    ) -> bool {
        let source_is_element = layout
            .get(panel)
            .and_then(|e| e.owner)
            .is_some_and(|owner| self.is_element(layout, owner));
        !source_is_element
            && !self.is_element(layout, target_pane)
            && !self.in_anchored(layout, panel)
            && !self.is_anchored(layout, target_pane)
            && self.groups_compatible(layout, panel, target_pane)
    }

    pub(crate) fn groups_compatible<K>(
        &self,
        layout: &Layout<K>,
        panel: NodeId,
        target_pane: NodeId,
    ) -> bool {
        let panel_group = match layout.kind(panel) {
            Some(NodeKind::Panel(p)) => p.group.as_deref(),
            _ => return false,
        };
        let pane_group = match layout.kind(target_pane) {
            Some(NodeKind::Pane(p)) => p.group.as_deref(),
            _ => return false,
        };
        match (panel_group, pane_group) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
    }

    #[must_use]
    pub fn validate<K>(
        &self,
        layout: &Layout<K>,
        source_pane: NodeId,
        source_panel: NodeId,
        target: NodeId,
        op: DockOperation,
    ) -> bool {
        match op {
            DockOperation::Fill => {
                source_pane != target
                    && layout.is_leaf(source_panel)
                    && matches!(layout.kind(target), Some(NodeKind::Pane(_)))
                    && self.fill_allowed(layout, source_panel, target)
            }
            DockOperation::Left
            | DockOperation::Right
            | DockOperation::Top
            | DockOperation::Bottom => match layout.kind(target) {
                // Harlo: an anchored pane neither splits off a tab nor takes one beside it.
                _ if self.is_anchored(layout, source_pane) || self.is_anchored(layout, target) => {
                    false
                }
                Some(NodeKind::Pane(_)) => {
                    (source_pane != target
                        || (self.pane_tab_count(layout, target) >= 1
                            && !self.is_element(layout, target)))
                        && (source_pane == target || self.is_target_visible(layout, source_pane))
                }
                // A split, reached through a nested drop band: the pane docks beside all of it.
                Some(NodeKind::Proportional(_)) => self.is_target_visible(layout, source_pane),
                _ => false,
            },
        }
    }

    #[must_use]
    pub fn is_target_visible<K>(&self, layout: &Layout<K>, target: NodeId) -> bool {
        matches!(
            layout.kind(target),
            Some(NodeKind::Panel(_) | NodeKind::Pane(_) | NodeKind::Proportional(_))
        )
    }

    pub fn execute<K>(&self, layout: &mut Layout<K>, session: DragSession) -> Result {
        let target = session.hover_target.ok_or(Error::MissingHoverTarget)?;
        let op = session.operation.ok_or(Error::MissingOperation)?;
        if !self.validate(
            layout,
            session.source_pane,
            session.source_panel,
            target,
            op,
        ) {
            return Err(Error::ValidationFailed);
        }
        let factory = Factory;
        if session.source_pane == target && op.is_edge() {
            factory.split_same_pane_edge(layout, target, session.source_panel, op)
        } else {
            match op {
                DockOperation::Fill => factory.dock_fill(layout, session.source_panel, target),
                _ => factory.split_cross_pane_edge(
                    layout,
                    session.source_pane,
                    session.source_panel,
                    target,
                    op,
                ),
            }
        }
    }

    /// Map pointer position inside `bounds` to a drop zone.
    ///
    /// `edge_fraction` is clamped to `0.0..=0.5`.
    #[must_use]
    pub fn hit_test_drop_zone(
        bounds: Rectangle,
        point: iced::Point,
        edge_fraction: f32,
    ) -> Option<DropZone> {
        if !bounds.contains(point) {
            return None;
        }
        let w = bounds.width;
        let h = bounds.height;
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let edge = edge_fraction.clamp(0.0, 0.5);
        let rx = (point.x - bounds.x) / w;
        let ry = (point.y - bounds.y) / h;
        // Harlo: the nearest edge wins, so a corner goes to the side it's closer to.
        let nearest = [
            (rx, DropZone::Left),
            (1.0 - rx, DropZone::Right),
            (ry, DropZone::Top),
            (1.0 - ry, DropZone::Bottom),
        ]
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .filter(|&(distance, _)| distance < edge);
        Some(nearest.map_or(DropZone::Center, |(_, zone)| zone))
    }

    /// Find the pane under `point` and the drop zone within it.
    #[must_use]
    pub fn hit_test_pane(
        point: iced::Point,
        targets: &[(NodeId, Rectangle)],
        edge_fraction: f32,
    ) -> Option<(NodeId, DropZone)> {
        let mut best: Option<(NodeId, Rectangle, f32)> = None;
        for &(id, bounds) in targets {
            if bounds.contains(point) {
                let area = bounds.width * bounds.height;
                if best.is_none_or(|(_, _, a)| area < a) {
                    best = Some((id, bounds, area));
                }
            }
        }
        let (id, bounds, _) = best?;
        Self::hit_test_drop_zone(bounds, point, edge_fraction).map(|zone| (id, zone))
    }

    fn insertion_index_at(insert_x: &[f32], x: f32) -> usize {
        if insert_x.is_empty() {
            return 0;
        }
        if insert_x.len() == 1 {
            return 0;
        }
        for i in 0..insert_x.len() - 1 {
            let threshold = f32::midpoint(insert_x[i], insert_x[i + 1]);
            if x < threshold {
                return i;
            }
        }
        insert_x.len() - 1
    }

    /// Find the tab bar under `point` and the insertion index within it.
    #[must_use]
    pub fn hit_test_tab_insert(
        point: iced::Point,
        targets: &[TabBarTarget],
    ) -> Option<(NodeId, usize)> {
        let mut best: Option<(NodeId, f32, usize)> = None;
        for target in targets {
            if !target.bounds.contains(point) {
                continue;
            }
            let area = target.bounds.width * target.bounds.height;
            if best.is_none_or(|(_, a, _)| area < a) {
                let layout_x = point.x + target.scroll_offset;
                let index = Self::insertion_index_at(&target.insert_x, layout_x);
                best = Some((target.pane, area, index));
            }
        }
        best.map(|(pane, _, index)| (pane, index))
    }

    /// Splits enclosing `pane` whose `op` edge is also the pane's, innermost first.
    ///
    /// Only splits across the edge count: one along it already ends where its edge child does,
    /// so docking beside it would land in the same place as docking beside the pane.
    #[must_use]
    pub fn edge_ancestors<K>(layout: &Layout<K>, pane: NodeId, op: DockOperation) -> Vec<NodeId> {
        let Some(axis) = Axis::for_operation(op) else {
            return Vec::new();
        };
        let after = matches!(op, DockOperation::Right | DockOperation::Bottom);
        let mut ancestors = Vec::new();
        let mut child = pane;
        while let Some(parent) = layout.get(child).and_then(|e| e.owner) {
            let Some(NodeKind::Proportional(pg)) = layout.kind(parent) else {
                break;
            };
            if pg.axis == axis {
                let edge_child = if after {
                    pg.children.last()
                } else {
                    pg.children.first()
                };
                if edge_child != Some(&child) {
                    break;
                }
            } else {
                ancestors.push(parent);
            }
            child = parent;
        }
        ancestors
    }

    /// Retarget an edge hover to an enclosing split when the cursor sits in one of the nested
    /// bands along the pane's edge.
    ///
    /// The edge zone gives up one `band`-thick strip per edge-sharing split (see
    /// [`Self::edge_ancestors`]), outermost split at the very edge; the rest stays with the pane.
    /// Bands shrink so the pane always keeps at least one band's share. `band <= 0` disables.
    pub fn lift_drag_hover<K>(
        session: &mut DragSession,
        layout: &Layout<K>,
        cursor: iced::Point,
        drop_targets: &[(NodeId, Rectangle)],
        band: f32,
    ) {
        if band <= 0.0 {
            return;
        }
        let (Some(pane), Some(op)) = (session.hover_target, session.operation) else {
            return;
        };
        if !op.is_edge() {
            return;
        }
        let Some(&(_, bounds)) = drop_targets.iter().find(|(id, _)| *id == pane) else {
            return;
        };
        let ancestors = Self::edge_ancestors(layout, pane, op);
        if ancestors.is_empty() {
            return;
        }
        let (dist, depth) = match op {
            DockOperation::Left => (cursor.x - bounds.x, bounds.width),
            DockOperation::Right => (bounds.x + bounds.width - cursor.x, bounds.width),
            DockOperation::Top => (cursor.y - bounds.y, bounds.height),
            DockOperation::Bottom => (bounds.y + bounds.height - cursor.y, bounds.height),
            DockOperation::Fill => return,
        };
        let zone_depth = depth * session.drop_edge_fraction;
        let band = band.min(zone_depth / (ancestors.len() + 1) as f32);
        // Band 0 is outermost and belongs to the outermost split.
        let outer_band = (0..ancestors.len()).find(|&i| dist < band * (i + 1) as f32);
        if let Some(i) = outer_band {
            session.hover_target = Some(ancestors[ancestors.len() - 1 - i]);
        }
    }

    /// Bounds of `node` on screen: the union of the drop targets of the panes beneath it.
    #[must_use]
    pub fn node_bounds<K>(
        layout: &Layout<K>,
        node: NodeId,
        drop_targets: &[(NodeId, Rectangle)],
    ) -> Option<Rectangle> {
        let is_under = |mut id: NodeId| loop {
            if id == node {
                return true;
            }
            match layout.get(id).and_then(|e| e.owner) {
                Some(owner) => id = owner,
                None => return false,
            }
        };
        drop_targets
            .iter()
            .filter(|(id, _)| is_under(*id))
            .map(|(_, bounds)| *bounds)
            .reduce(|a, b| a.union(&b))
    }

    pub fn update_drag_hover(
        session: &mut DragSession,
        cursor: iced::Point,
        drop_targets: &[(NodeId, Rectangle)],
    ) {
        if let Some((target, zone)) =
            Self::hit_test_pane(cursor, drop_targets, session.drop_edge_fraction)
        {
            session.hover_target = Some(target);
            session.operation = Some(zone.to_operation());
        } else {
            session.hover_target = None;
            session.operation = None;
        }
    }

    /// Update hover state for an active drag; tab bar insertion takes priority over content zones.
    pub fn update_drag_hover_full(
        session: &mut DragSession,
        cursor: iced::Point,
        drop_targets: &[(NodeId, Rectangle)],
        tab_bar_targets: &[TabBarTarget],
    ) {
        if let Some((pane, index)) = Self::hit_test_tab_insert(cursor, tab_bar_targets) {
            session.tab_insert = Some((pane, index));
            session.hover_target = None;
            session.operation = None;
        } else {
            session.tab_insert = None;
            Self::update_drag_hover(session, cursor, drop_targets);
        }
    }

    pub fn execute_tab_insert<K>(
        &self,
        layout: &mut Layout<K>,
        session: DragSession,
        pane: NodeId,
        index: usize,
    ) -> Result {
        if !layout.is_leaf(session.source_panel) {
            return Err(Error::NotPanel {
                node: session.source_panel,
            });
        }
        if session.source_pane != pane && !self.fill_allowed(layout, session.source_panel, pane) {
            return Err(Error::ValidationFailed);
        }
        let factory = Factory;
        if session.source_pane == pane {
            factory.move_tab_in_pane(layout, pane, session.source_panel, index)
        } else {
            factory.move_panel_to_pane_at(layout, session.source_panel, pane, index)
        }
    }
}
