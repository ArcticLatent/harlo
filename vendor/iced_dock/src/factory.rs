//! Tree mutations for the docking layout.

use crate::model::{
    Axis, DockOperation, Layout, NodeEntry, NodeId, NodeKind, Pane, PaneGrave, Panel,
    ProportionalGroup,
};
use crate::{Error, Result};

/// All structural changes to a [`Layout`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Factory;

impl Factory {
    pub fn insert_panel<K: Copy>(
        &self,
        layout: &mut Layout<K>,
        id: impl Into<String>,
        title: impl Into<String>,
        content: K,
    ) -> NodeId {
        layout.nodes.insert(NodeEntry {
            kind: NodeKind::Panel(Panel::new(id, title, content)),
            owner: None,
        })
    }

    pub fn create_pane<K>(&self, layout: &mut Layout<K>) -> NodeId {
        layout.nodes.insert(NodeEntry {
            kind: NodeKind::Pane(Pane::new()),
            owner: None,
        })
    }

    pub fn create_proportional<K>(
        &self,
        layout: &mut Layout<K>,
        axis: Axis,
        children: Vec<NodeId>,
    ) -> NodeId {
        let id = layout.nodes.insert(NodeEntry {
            kind: NodeKind::Proportional(ProportionalGroup::new(axis, children.clone())),
            owner: None,
        });
        for child in children {
            layout.set_owner(child, Some(id));
        }
        id
    }

    pub fn add_panel_to_pane<K>(
        &self,
        layout: &mut Layout<K>,
        pane: NodeId,
        panel: NodeId,
    ) -> Result {
        if !layout.is_leaf(panel) {
            return Err(Error::NotPanel { node: panel });
        }
        self.ensure_accepts_panel(layout, pane)?;
        if let Some(NodeKind::Pane(ref mut p)) = layout.get_mut(pane).map(|e| &mut e.kind) {
            p.tabs.push(panel);
            p.active = Some(panel);
            layout.set_owner(panel, Some(pane));
            Ok(())
        } else {
            Err(Error::NotPane { node: pane })
        }
    }

    pub fn insert_panel_at<K>(
        &self,
        layout: &mut Layout<K>,
        pane: NodeId,
        panel: NodeId,
        index: usize,
    ) -> Result {
        if !layout.is_leaf(panel) {
            return Err(Error::NotPanel { node: panel });
        }
        self.ensure_accepts_panel(layout, pane)?;
        if let Some(NodeKind::Pane(ref mut p)) = layout.get_mut(pane).map(|e| &mut e.kind) {
            let index = index.min(p.tabs.len());
            p.tabs.insert(index, panel);
            p.active = Some(panel);
            layout.set_owner(panel, Some(pane));
            Ok(())
        } else {
            Err(Error::NotPane { node: pane })
        }
    }

    pub fn move_panel_to_pane_at<K>(
        &self,
        layout: &mut Layout<K>,
        source: NodeId,
        target_pane: NodeId,
        index: usize,
    ) -> Result {
        let old_owner = layout.get(source).and_then(|e| e.owner);
        // Checked before detaching, or a rejected move would orphan the panel.
        self.ensure_accepts_panel(layout, target_pane)?;
        self.remove_from_parent(layout, source)?;
        self.insert_panel_at(layout, target_pane, source, index)?;
        if let Some(owner) = old_owner {
            if owner != target_pane {
                self.collapse_owner(layout, owner);
            }
        }
        Ok(())
    }

    pub fn move_panel_to_pane<K>(
        &self,
        layout: &mut Layout<K>,
        source: NodeId,
        target_pane: NodeId,
    ) -> Result {
        // Checked before detaching, or a rejected move would orphan the panel.
        self.ensure_accepts_panel(layout, target_pane)?;
        self.remove_from_parent(layout, source)?;
        self.add_panel_to_pane(layout, target_pane, source)
    }

    /// An element pane holds exactly one panel; once filled it takes no more.
    fn ensure_accepts_panel<K>(&self, layout: &Layout<K>, pane: NodeId) -> Result {
        match layout.kind(pane) {
            Some(NodeKind::Pane(p)) if p.is_element() && !p.tabs.is_empty() => {
                Err(Error::ElementOccupied { pane })
            }
            _ => Ok(()),
        }
    }

    pub fn dock_fill<K>(
        &self,
        layout: &mut Layout<K>,
        source_panel: NodeId,
        target_pane: NodeId,
    ) -> Result {
        let old_owner = layout.get(source_panel).and_then(|e| e.owner);
        self.move_panel_to_pane(layout, source_panel, target_pane)?;
        if let Some(owner) = old_owner {
            self.collapse_owner(layout, owner);
        }
        Ok(())
    }

    pub fn reorder_panel<K>(
        &self,
        layout: &mut Layout<K>,
        pane: NodeId,
        from: usize,
        to: usize,
    ) -> Result {
        if let Some(NodeKind::Pane(ref mut p)) = layout.get_mut(pane).map(|e| &mut e.kind) {
            if from < p.tabs.len() && to < p.tabs.len() {
                let child = p.tabs.remove(from);
                p.tabs.insert(to, child);
                Ok(())
            } else {
                Err(Error::InvalidTabIndex {
                    pane,
                    from,
                    to,
                    len: p.tabs.len(),
                })
            }
        } else {
            Err(Error::NotPane { node: pane })
        }
    }

    pub fn move_tab_in_pane<K>(
        &self,
        layout: &mut Layout<K>,
        pane: NodeId,
        panel: NodeId,
        to_index: usize,
    ) -> Result {
        let from = match layout.kind(pane) {
            Some(NodeKind::Pane(p)) => {
                p.tabs
                    .iter()
                    .position(|&id| id == panel)
                    .ok_or(Error::InvalidTabIndex {
                        pane,
                        from: 0,
                        to: to_index,
                        len: p.tabs.len(),
                    })?
            }
            _ => return Err(Error::NotPane { node: pane }),
        };

        if from == to_index || from + 1 == to_index {
            return Ok(());
        }

        let mut adjusted = to_index;
        if from < adjusted {
            adjusted -= 1;
        }
        self.reorder_panel(layout, pane, from, adjusted)
    }

    pub fn set_active_panel<K>(&self, layout: &mut Layout<K>, pane: NodeId, panel: NodeId) {
        if let Some(NodeKind::Pane(ref mut p)) = layout.get_mut(pane).map(|e| &mut e.kind) {
            if p.tabs.contains(&panel) {
                p.active = Some(panel);
            }
        }
    }

    pub fn close<K>(&self, layout: &mut Layout<K>, panel: NodeId) -> Result {
        let owner = layout
            .get(panel)
            .and_then(|e| e.owner)
            .ok_or(Error::NoOwner { panel })?;
        self.remove_from_parent(layout, panel)?;
        layout.nodes.remove(panel);
        self.collapse_owner(layout, owner);
        Ok(())
    }

    /// Split `target` and place `source` pane beside it.
    pub fn split<K>(
        &self,
        layout: &mut Layout<K>,
        source: NodeId,
        mut target: NodeId,
        op: DockOperation,
    ) -> Result {
        if op == DockOperation::Fill {
            return Err(Error::InvalidSplitOperation(op));
        }
        let axis = Axis::for_operation(op).ok_or(Error::InvalidSplitOperation(op))?;

        if let Some(old_owner) = layout.get(source).and_then(|e| e.owner) {
            // Taking `source` out of a two-child group collapses that group into the survivor,
            // so a split aimed at the group lands on the survivor in its place.
            if old_owner == target {
                if let Some(NodeKind::Proportional(pg)) = layout.kind(target) {
                    if pg.children.len() == 2 {
                        target = pg
                            .children
                            .iter()
                            .copied()
                            .find(|&c| c != source)
                            .unwrap_or(target);
                    }
                }
            }
            self.remove_from_parent(layout, source)?;
            self.collapse_owner(layout, old_owner);
        }

        let split_target = self.resolve_split_target(layout, target)?;
        let new_leaf_side = self.side_for_operation(op);

        if let Some(parent) = layout.get(split_target).and_then(|e| e.owner) {
            if let Some(NodeKind::Proportional(pg)) = layout.kind(parent) {
                if pg.axis == axis {
                    return self.insert_into_proportional(
                        layout,
                        parent,
                        split_target,
                        source,
                        new_leaf_side,
                    );
                }
            }
        }

        let old_owner = layout.get(split_target).and_then(|e| e.owner);
        let prop = self.create_proportional(
            layout,
            axis,
            if new_leaf_side {
                vec![split_target, source]
            } else {
                vec![source, split_target]
            },
        );

        if let Some(owner) = old_owner {
            self.replace_child(layout, owner, split_target, prop)?;
            layout.set_owner(prop, Some(owner));
        } else {
            layout.set_root_child(Some(prop));
        }

        layout.set_owner(source, Some(prop));
        layout.set_owner(split_target, Some(prop));
        Ok(())
    }

    /// Edge drop on the same pane as the drag source.
    pub fn split_same_pane_edge<K>(
        &self,
        layout: &mut Layout<K>,
        pane: NodeId,
        panel: NodeId,
        op: DockOperation,
    ) -> Result {
        if !op.is_edge() {
            return Err(Error::NotEdgeOperation);
        }
        let tab_count = match layout.kind(pane) {
            Some(NodeKind::Pane(p)) => p.tabs.len(),
            _ => return Err(Error::NotPane { node: pane }),
        };

        if tab_count > 1 {
            let new_pane = self.peel_panel_to_new_pane(layout, panel)?;
            self.split(layout, new_pane, pane, op)
        } else {
            let axis = Axis::for_operation(op).ok_or(Error::InvalidSplitOperation(op))?;
            let after = self.side_for_operation(op);
            let empty_pane = self.create_pane(layout);
            let old_owner = layout.get(pane).and_then(|e| e.owner);

            let children = if after {
                vec![pane, empty_pane]
            } else {
                vec![empty_pane, pane]
            };
            let prop = self.create_proportional(layout, axis, children);

            if let Some(owner) = old_owner {
                self.replace_child(layout, owner, pane, prop)?;
                layout.set_owner(prop, Some(owner));
            } else {
                layout.set_root_child(Some(prop));
            }
            layout.set_owner(pane, Some(prop));
            layout.set_owner(empty_pane, Some(prop));
            Ok(())
        }
    }

    /// Edge drop onto a different pane than the drag source.
    pub fn split_cross_pane_edge<K>(
        &self,
        layout: &mut Layout<K>,
        source_pane: NodeId,
        source_panel: NodeId,
        target: NodeId,
        op: DockOperation,
    ) -> Result {
        if !op.is_edge() {
            return Err(Error::NotEdgeOperation);
        }
        let tab_count = match layout.kind(source_pane) {
            Some(NodeKind::Pane(p)) => p.tabs.len(),
            _ => return Err(Error::NotPane { node: source_pane }),
        };

        if tab_count > 1 {
            let new_pane = self.peel_panel_to_new_pane(layout, source_panel)?;
            self.split(layout, new_pane, target, op)
        } else {
            self.split(layout, source_pane, target, op)
        }
    }

    fn peel_panel_to_new_pane<K>(&self, layout: &mut Layout<K>, panel: NodeId) -> Result<NodeId> {
        let panel_group = match layout.kind(panel) {
            Some(NodeKind::Panel(p)) => p.group.clone(),
            _ => None,
        };
        let new_pane = self.create_pane(layout);
        if let Some(group) = panel_group {
            if let Some(NodeKind::Pane(ref mut pane)) =
                layout.get_mut(new_pane).map(|e| &mut e.kind)
            {
                pane.group = Some(group);
            }
        }
        self.remove_from_parent(layout, panel)?;
        self.add_panel_to_pane(layout, new_pane, panel)?;
        Ok(new_pane)
    }

    fn resolve_split_target<K>(&self, layout: &Layout<K>, target: NodeId) -> Result<NodeId> {
        match layout.kind(target) {
            Some(NodeKind::Pane(_) | NodeKind::Proportional(_) | NodeKind::Panel(_)) => Ok(target),
            _ => Err(Error::InvalidSplitTarget { node: target }),
        }
    }

    fn side_for_operation(&self, op: DockOperation) -> bool {
        matches!(op, DockOperation::Right | DockOperation::Bottom)
    }

    fn insert_into_proportional<K>(
        &self,
        layout: &mut Layout<K>,
        parent: NodeId,
        target: NodeId,
        source: NodeId,
        after: bool,
    ) -> Result {
        if let Some(NodeKind::Proportional(ref mut pg)) =
            layout.get_mut(parent).map(|e| &mut e.kind)
        {
            if let Some(idx) = pg.children.iter().position(|&c| c == target) {
                let insert_at = if after { idx + 1 } else { idx };
                // Harlo: the new pane takes half of the target's share, as the drop hint shows,
                // and the other panes keep theirs.
                let half = pg.proportions.get(idx).copied().unwrap_or(0.0) / 2.0;
                if let Some(share) = pg.proportions.get_mut(idx) {
                    *share = half;
                }
                pg.children.insert(insert_at, source);
                pg.proportions.insert(insert_at, half);
                pg.normalize_proportions();
                layout.set_owner(source, Some(parent));
                return Ok(());
            }
            return Err(Error::ChildNotFound {
                parent,
                child: target,
            });
        }
        Err(Error::NotProportional { node: parent })
    }

    fn replace_child<K>(
        &self,
        layout: &mut Layout<K>,
        parent: NodeId,
        old_child: NodeId,
        new_child: NodeId,
    ) -> Result {
        match layout.kind(parent) {
            Some(NodeKind::Root(_)) => {
                layout.set_root_child(Some(new_child));
                Ok(())
            }
            Some(NodeKind::Proportional(_)) => {
                if let Some(NodeKind::Proportional(ref mut pg)) =
                    layout.get_mut(parent).map(|e| &mut e.kind)
                {
                    if let Some(idx) = pg.children.iter().position(|&c| c == old_child) {
                        pg.children[idx] = new_child;
                        layout.set_owner(new_child, Some(parent));
                        return Ok(());
                    }
                }
                Err(Error::ChildNotFound {
                    parent,
                    child: old_child,
                })
            }
            Some(NodeKind::Pane(_)) => {
                if let Some(NodeKind::Pane(ref mut p)) = layout.get_mut(parent).map(|e| &mut e.kind)
                {
                    for t in &mut p.tabs {
                        if *t == old_child {
                            *t = new_child;
                            if p.active == Some(old_child) {
                                p.active = Some(new_child);
                            }
                            layout.set_owner(new_child, Some(parent));
                            return Ok(());
                        }
                    }
                }
                Err(Error::ChildNotFound {
                    parent,
                    child: old_child,
                })
            }
            _ => Err(Error::InvalidParent {
                owner: parent,
                child: old_child,
            }),
        }
    }

    pub fn remove_from_parent<K>(&self, layout: &mut Layout<K>, child: NodeId) -> Result {
        let owner = layout
            .get(child)
            .and_then(|e| e.owner)
            .ok_or(Error::NoOwner { panel: child })?;
        match layout.kind(owner) {
            Some(NodeKind::Pane(_)) => {
                if let Some(NodeKind::Pane(ref mut p)) = layout.get_mut(owner).map(|e| &mut e.kind)
                {
                    p.tabs.retain(|&c| c != child);
                    if p.active == Some(child) {
                        p.active = p.tabs.last().copied();
                    }
                }
                layout.set_owner(child, None);
                Ok(())
            }
            Some(NodeKind::Proportional(_)) => {
                if let Some(NodeKind::Proportional(ref mut pg)) =
                    layout.get_mut(owner).map(|e| &mut e.kind)
                {
                    if let Some(idx) = pg.children.iter().position(|&c| c == child) {
                        pg.children.remove(idx);
                        if idx < pg.proportions.len() {
                            pg.proportions.remove(idx);
                        }
                        pg.normalize_proportions();
                    }
                }
                layout.set_owner(child, None);
                Ok(())
            }
            _ => Err(Error::InvalidParent { owner, child }),
        }
    }

    /// Re-create a named pane that collapsed when its last tab closed, in the place it held.
    ///
    /// Returns the new, empty pane; the caller fills it.
    pub fn revive_pane<K>(&self, layout: &mut Layout<K>, name: &str) -> Result<NodeId> {
        let index = layout
            .graves
            .iter()
            .position(|grave| grave.name == name)
            .ok_or_else(|| Error::UnknownPane(name.to_owned()))?;
        let grave = layout.graves.remove(index);
        let anchor = grave.anchor;
        if layout.get(anchor).is_none() {
            return Err(Error::UnknownPane(name.to_owned()));
        }

        let pane = self.create_pane(layout);
        if let Some(NodeKind::Pane(ref mut p)) = layout.get_mut(pane).map(|e| &mut e.kind) {
            p.name = Some(grave.name);
            p.group = grave.group;
            p.handle = grave.handle;
        }

        let op = operation_for(grave.axis, grave.after);
        if let Err(err) = self.split(layout, pane, anchor, op) {
            layout.nodes.remove(pane);
            return Err(err);
        }
        self.set_child_proportion(layout, pane, grave.proportion);
        Ok(pane)
    }

    /// Records where a named pane sat so [`Factory::revive_pane`] can put it back.
    fn record_pane_grave<K>(&self, layout: &mut Layout<K>, pane: NodeId) {
        let Some(NodeKind::Pane(p)) = layout.kind(pane) else {
            return;
        };
        let (Some(name), group, handle) = (p.name.clone(), p.group.clone(), p.handle) else {
            return;
        };
        let Some(parent) = layout.get(pane).and_then(|e| e.owner) else {
            return;
        };
        let Some(NodeKind::Proportional(pg)) = layout.kind(parent) else {
            return;
        };
        let Some(index) = pg.children.iter().position(|&c| c == pane) else {
            return;
        };
        let (anchor, after) = if index + 1 < pg.children.len() {
            (pg.children[index + 1], false)
        } else if index > 0 {
            (pg.children[index - 1], true)
        } else {
            return;
        };
        let axis = pg.axis;
        // Harlo: the share of the whole group, whatever the proportions add up to.
        let total: f32 = pg.proportions.iter().sum();
        let proportion = match pg.proportions.get(index) {
            Some(&share) if total > 0.0 => share / total,
            _ => 0.5,
        };

        // Graves anchored to this pane would dangle once it is gone, so they inherit its anchor,
        // scaled, since they now measure against a different group.
        for grave in &mut layout.graves {
            if grave.anchor == pane {
                grave.anchor = anchor;
                grave.axis = axis;
                grave.after = after;
                grave.proportion *= proportion;
            }
        }
        layout.graves.retain(|grave| grave.name != name);
        layout.graves.push(PaneGrave {
            name,
            group,
            anchor,
            axis,
            after,
            proportion,
            handle,
        });
    }

    /// Gives `child` `share` of its parent group, the rest split among its siblings as before.
    fn set_child_proportion<K>(&self, layout: &mut Layout<K>, child: NodeId, share: f32) {
        let share = share.clamp(0.05, 0.9);
        let Some(parent) = layout.get(child).and_then(|e| e.owner) else {
            return;
        };
        let Some(NodeKind::Proportional(pg)) = layout.kind(parent) else {
            return;
        };
        let Some(index) = pg.children.iter().position(|&c| c == child) else {
            return;
        };
        let rest: f32 = pg
            .proportions
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, p)| *p)
            .sum();
        if rest <= 0.0 {
            return;
        }
        let scale = (1.0 - share) / rest;
        let proportions = pg
            .proportions
            .iter()
            .enumerate()
            .map(|(i, p)| if i == index { share } else { *p * scale })
            .collect();
        let _ = self.set_proportions(layout, parent, proportions);
    }

    /// A panel with no group of its own takes the group of the pane it lands in.
    pub fn inherit_pane_group<K>(&self, layout: &mut Layout<K>, pane: NodeId, panel: NodeId) {
        let Some(NodeKind::Pane(p)) = layout.kind(pane) else {
            return;
        };
        let Some(group) = p.group.clone() else {
            return;
        };
        if let Some(NodeKind::Panel(ref mut panel)) = layout.get_mut(panel).map(|e| &mut e.kind) {
            if panel.group.is_none() {
                panel.group = Some(group);
            }
        }
    }

    fn collapse_owner<K>(&self, layout: &mut Layout<K>, owner: NodeId) {
        if owner == layout.root {
            let empty_child = match layout.kind(layout.root) {
                Some(NodeKind::Root(r)) => r.child.and_then(|child| {
                    layout
                        .get(child)
                        .map(|e| matches!(&e.kind, NodeKind::Pane(p) if p.tabs.is_empty() && !p.persistent))
                }),
                _ => None,
            };
            if empty_child == Some(true) {
                if let Some(NodeKind::Root(ref mut r)) = layout.get_mut(owner).map(|e| &mut e.kind)
                {
                    r.child = None;
                }
            }
            return;
        }

        let should_collapse = match layout.kind(owner) {
            Some(NodeKind::Pane(p)) => p.tabs.is_empty() && !p.persistent,
            Some(NodeKind::Proportional(pg)) => pg.children.len() <= 1,
            _ => false,
        };

        if !should_collapse {
            return;
        }

        if matches!(layout.kind(owner), Some(NodeKind::Pane(_))) {
            self.record_pane_grave(layout, owner);
        }

        let grand_owner = layout.get(owner).and_then(|e| e.owner);
        let replacement = match layout.kind(owner) {
            Some(NodeKind::Proportional(pg)) => pg.children.first().copied(),
            _ => None,
        };

        if let Some(go) = grand_owner {
            if let Some(rep) = replacement {
                for grave in &mut layout.graves {
                    if grave.anchor == owner {
                        grave.anchor = rep;
                    }
                }
                let _ = self.replace_child(layout, go, owner, rep);
                layout.nodes.remove(owner);
                self.collapse_owner(layout, go);
            } else if go == layout.root {
                layout.set_root_child(None);
                layout.nodes.remove(owner);
            } else {
                let _ = self.remove_from_parent(layout, owner);
                layout.nodes.remove(owner);
                self.collapse_owner(layout, go);
            }
        }
    }

    pub fn set_proportions<K>(
        &self,
        layout: &mut Layout<K>,
        group: NodeId,
        proportions: Vec<f32>,
    ) -> Result {
        if let Some(NodeKind::Proportional(ref mut pg)) = layout.get_mut(group).map(|e| &mut e.kind)
        {
            if proportions.len() == pg.children.len() {
                pg.proportions = proportions;
                pg.normalize_proportions();
                Ok(())
            } else {
                Err(Error::InvalidWeights {
                    expected: pg.children.len(),
                    got: proportions.len(),
                })
            }
        } else {
            Err(Error::NotProportional { node: group })
        }
    }

    pub fn set_binary_split_ratio<K>(
        &self,
        layout: &mut Layout<K>,
        group: NodeId,
        split_at: f32,
    ) -> Result {
        let ratio = split_at.clamp(0.1, 0.9);
        self.set_proportions(layout, group, vec![ratio, 1.0 - ratio])
    }

    pub fn adjust_splitter<K>(
        &self,
        layout: &mut Layout<K>,
        group: NodeId,
        splitter_index: usize,
        pair_ratio: f32,
    ) -> Result {
        if let Some(NodeKind::Proportional(ref mut pg)) = layout.get_mut(group).map(|e| &mut e.kind)
        {
            let n = pg.children.len();
            if splitter_index >= n.saturating_sub(1) {
                return Err(Error::InvalidSplitterIndex {
                    index: splitter_index,
                    children: n,
                });
            }
            let mut props = if pg.proportions.len() == n {
                pg.proportions.clone()
            } else {
                vec![1.0; n]
            };
            let pair_total = props[splitter_index] + props[splitter_index + 1];
            if pair_total <= 0.0 {
                return Err(Error::ZeroTotalWeight);
            }
            let min_weight = 0.05 * pair_total;
            let left = (pair_ratio * pair_total).clamp(min_weight, pair_total - min_weight);
            props[splitter_index] = left;
            props[splitter_index + 1] = pair_total - left;
            pg.proportions = props;
            pg.normalize_proportions();
            Ok(())
        } else {
            Err(Error::NotProportional { node: group })
        }
    }
}

fn operation_for(axis: Axis, after: bool) -> DockOperation {
    match (axis, after) {
        (Axis::Horizontal, false) => DockOperation::Left,
        (Axis::Horizontal, true) => DockOperation::Right,
        (Axis::Vertical, false) => DockOperation::Top,
        (Axis::Vertical, true) => DockOperation::Bottom,
    }
}
