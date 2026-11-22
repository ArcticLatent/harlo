use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::builder::compile::{
    active_panel_in_pane, build_tree, first_pane, insert_panel_into_state, pane_for_panel,
    BuiltLayout,
};
use crate::builder::spec::{LayoutTree, PanelDef};
use crate::factory::Factory;
use crate::manager::DockManager;
use crate::model::{DockOperation, HandleSide, NodeId, NodeKind};
use crate::spatial::{adjacent_pane, pane_bounds_map, Direction};
use crate::widget::{dispatch_action, DockAction, DockWidgetState, TabAction};
use crate::{Error, Result};

/// Target pane for opening a new panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneTarget {
    /// Open in the pane registered with [`TabsNode::named`](crate::builder::TabsNode::named).
    /// A pane that collapsed when its last tab closed is re-created where it was.
    Named(String),
    /// Open in the pane that last received focus.
    Active,
    /// Open in the first pane encountered in a preorder tree walk.
    First,
    /// Harlo: open in the pane holding the panel with this id, right after it.
    Beside(String),
}

/// Which pane receives focus when building a [`DockSession`].
#[derive(Debug, Clone, Default)]
pub enum InitialFocus<'a> {
    /// First pane in preorder tree walk.
    #[default]
    FirstPane,
    /// Pane registered with [`TabsNode::named`](crate::builder::TabsNode::named).
    NamedPane(Cow<'a, str>),
    /// Pane that owns the panel with this id (active tab comes from `.active()` at compile time).
    NamedPanel(Cow<'a, str>),
}

/// Direction to cycle the active tab within the focused pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelCycle {
    Next,
    Prev,
}

/// High-level handle for a dock layout and runtime panel operations.
pub struct DockSession<K> {
    inner: Rc<RefCell<DockWidgetState<K>>>,
}

impl<K> DockSession<K>
where
    K: Copy,
{
    /// Build a session from a declarative layout tree.
    pub fn from_tree(tree: LayoutTree<K>) -> Result<Self> {
        Self::from_tree_with_focus(tree, InitialFocus::default())
    }

    /// Build a session and set initial pane focus.
    pub fn from_tree_with_focus(tree: LayoutTree<K>, focus: InitialFocus<'_>) -> Result<Self> {
        let built = build_tree(&tree)?;
        let focused_pane = resolve_initial_focus(&built, focus)?;
        Ok(Self::from_built(built, focused_pane))
    }

    /// Build a session from a compiled layout and index.
    #[must_use]
    pub fn from_built(built: BuiltLayout<K>, focused_pane: Option<NodeId>) -> Self {
        let state = DockWidgetState::from_built(built, focused_pane);
        Self {
            inner: Rc::new(RefCell::new(state)),
        }
    }

    /// Shared widget state for the iced dock builder.
    #[must_use]
    pub fn state(&self) -> Rc<RefCell<DockWidgetState<K>>> {
        Rc::clone(&self.inner)
    }

    /// Apply a [`DockAction`] programmatically (not for widget-originated input).
    #[expect(clippy::must_use_candidate)]
    pub fn dispatch(&self, action: DockAction) -> bool {
        dispatch_action(&mut self.inner.borrow_mut(), action)
    }

    /// Open a panel in the given pane target and activate it.
    ///
    /// A [`PaneTarget::Named`] pane that collapsed when its last tab closed is re-created where
    /// it was, so reopening a tool panel does not drop it into an unrelated pane.
    pub fn open_panel(&self, target: PaneTarget, panel: impl Into<PanelDef<K>>) -> Result {
        let def = panel.into();
        let pane_id = match self.resolve_pane(&target) {
            Ok(pane) => pane,
            Err(err) => {
                let PaneTarget::Named(name) = &target else {
                    return Err(err);
                };
                let mut state = self.inner.borrow_mut();
                let Ok(revived) = Factory.revive_pane(&mut state.layout, name) else {
                    return Err(err);
                };
                // Name is live again from here on, even if the panel insert below fails.
                state.index.panes.insert(name.clone(), revived);
                revived
            }
        };
        let factory = Factory;
        let mut state = self.inner.borrow_mut();
        if let Some(NodeKind::Pane(pane)) = state.layout.kind(pane_id) {
            if pane.is_element() && !pane.tabs.is_empty() {
                return Err(Error::ElementOccupied { pane: pane_id });
            }
        }
        let panel_id = insert_panel_into_state(&factory, &mut *state, &def)?;
        // Without a group a panel drops into any pane, which would let a tool tab join the
        // documents.
        factory.inherit_pane_group(&mut state.layout, pane_id, panel_id);
        // Harlo: a panel opened beside another goes right after it.
        let after =
            match &target {
                PaneTarget::Beside(beside) => state.index.panel_node(beside).and_then(|node| {
                    match state.layout.kind(pane_id) {
                        Some(NodeKind::Pane(pane)) => pane.tabs.iter().position(|&t| t == node),
                        _ => None,
                    }
                }),
                _ => None,
            };
        match after {
            Some(index) => {
                factory.insert_panel_at(&mut state.layout, pane_id, panel_id, index + 1)?;
            }
            None => factory.add_panel_to_pane(&mut state.layout, pane_id, panel_id)?,
        }
        factory.set_active_panel(&mut state.layout, pane_id, panel_id);
        state.layout_dirty = true;
        state.focus(pane_id);
        state.sync_index();
        Ok(())
    }

    /// Harlo: opens `panel` in a new pane named `name`, split off on `op`'s side of the pane
    /// named `beside`, giving the new pane `share` of the space the two then hold.
    ///
    /// # Errors
    ///
    /// Fails when there's no pane named `beside`, `op` isn't an edge, or the panel's id is
    /// already in the dock.
    pub fn open_panel_split(
        &self,
        name: &str,
        beside: &str,
        op: DockOperation,
        share: f32,
        panel: impl Into<PanelDef<K>>,
    ) -> Result {
        let def = panel.into();
        let target = self.resolve_pane(&PaneTarget::Named(beside.to_owned()))?;
        let factory = Factory;
        let mut state = self.inner.borrow_mut();
        let pane_id = factory.create_pane(&mut state.layout);
        if let Some(NodeKind::Pane(pane)) = state.layout.get_mut(pane_id).map(|e| &mut e.kind) {
            pane.name = Some(name.to_owned());
            pane.group.clone_from(&def.group);
        }
        let panel_id = insert_panel_into_state(&factory, &mut *state, &def)?;
        factory.add_panel_to_pane(&mut state.layout, pane_id, panel_id)?;
        factory.split(&mut state.layout, pane_id, target, op)?;
        if let Some(parent) = state.layout.get(pane_id).and_then(|e| e.owner) {
            if let Some(NodeKind::Proportional(group)) =
                state.layout.get_mut(parent).map(|e| &mut e.kind)
            {
                let at = |id: NodeId| group.children.iter().position(|&c| c == id);
                if let (Some(new), Some(old)) = (at(pane_id), at(target)) {
                    let pair = group.proportions[new] + group.proportions[old];
                    let share = share.clamp(0.05, 0.95);
                    group.proportions[new] = pair * share;
                    group.proportions[old] = pair * (1.0 - share);
                    group.normalize_proportions();
                }
            }
        }
        factory.set_active_panel(&mut state.layout, pane_id, panel_id);
        state.layout_dirty = true;
        state.focus(pane_id);
        state.sync_index();
        Ok(())
    }

    /// Harlo: how many tabs the pane holding `panel_id` has.
    #[must_use]
    pub fn pane_tab_count(&self, panel_id: &str) -> usize {
        let Some(pane) = self.pane_for_panel(panel_id) else {
            return 0;
        };
        match self.inner.borrow().layout.kind(pane) {
            Some(NodeKind::Pane(p)) => p.tabs.len(),
            _ => 0,
        }
    }

    /// Harlo: the name of the pane holding `panel_id`, if it has one.
    #[must_use]
    pub fn pane_name(&self, panel_id: &str) -> Option<String> {
        let pane = self.pane_for_panel(panel_id)?;
        match self.inner.borrow().layout.kind(pane) {
            Some(NodeKind::Pane(p)) => p.name.clone(),
            _ => None,
        }
    }

    /// Harlo: the panel ids in the pane named `name`, in tab order.
    #[must_use]
    pub fn panels_in(&self, name: &str) -> Vec<String> {
        let Ok(pane) = self.resolve_pane(&PaneTarget::Named(name.to_owned())) else {
            return Vec::new();
        };
        let state = self.inner.borrow();
        match state.layout.kind(pane) {
            Some(NodeKind::Pane(p)) => p
                .tabs
                .iter()
                .filter_map(|&tab| match state.layout.kind(tab) {
                    Some(NodeKind::Panel(panel)) => Some(panel.id.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Activate a panel by string id and focus its pane.
    pub fn select_panel(&self, panel_id: &str) -> Result {
        let panel_node = self
            .panel_node(panel_id)
            .ok_or_else(|| Error::UnknownPanel(panel_id.into()))?;
        let pane_id = self.pane_for_panel(panel_id).ok_or(Error::InvalidTarget)?;
        self.dispatch(DockAction::Tab(TabAction::Select {
            pane: pane_id,
            panel: panel_node,
        }));
        Ok(())
    }

    /// Close a panel by its string id.
    pub fn close_panel(&self, panel_id: &str) -> Result {
        let panel_node = self
            .panel_node(panel_id)
            .ok_or_else(|| Error::UnknownPanel(panel_id.into()))?;
        if self.dispatch(DockAction::Tab(TabAction::Close { panel: panel_node })) {
            Ok(())
        } else {
            Err(Error::NoOwner { panel: panel_node })
        }
    }

    /// All known panel ids.
    #[must_use]
    pub fn panel_ids(&self) -> Vec<String> {
        self.inner.borrow().index.panel_ids().cloned().collect()
    }

    /// Panel node id for a string panel id.
    #[must_use]
    pub fn panel_node(&self, panel_id: &str) -> Option<NodeId> {
        self.inner.borrow().index.panel_node(panel_id)
    }

    /// Pane that owns a panel identified by string id.
    #[must_use]
    pub fn pane_for_panel(&self, panel_id: &str) -> Option<NodeId> {
        let state = self.inner.borrow();
        pane_for_panel(&state.layout, &state.index, panel_id)
    }

    /// Pane that last received focus, if any.
    #[must_use]
    pub fn focused_pane(&self) -> Option<NodeId> {
        self.inner.borrow().focused_pane
    }

    /// Whether the given pane currently has global focus.
    #[must_use]
    pub fn is_pane_focused(&self, pane: NodeId) -> bool {
        self.focused_pane() == Some(pane)
    }

    /// Pane that currently draws the focus frame, if any.
    ///
    /// Equal to [`Self::focused_pane`] unless
    /// [`DockBuilder::focus_frame_groups`](crate::widget::DockBuilder::focus_frame_groups)
    /// restricts the frame to a subset of tab groups.
    #[must_use]
    pub fn focus_frame_pane(&self) -> Option<NodeId> {
        self.inner.borrow().focus_frame_pane
    }

    /// Focus a pane by id (does not change the active tab).
    pub fn focus_pane(&self, pane: NodeId) -> Result {
        if !matches!(
            self.inner.borrow().layout.kind(pane),
            Some(NodeKind::Pane(_))
        ) {
            return Err(Error::InvalidTarget);
        }
        self.dispatch(DockAction::PaneFocused { pane, panel: None });
        Ok(())
    }

    /// Move focus to the nearest pane in `direction`.
    ///
    /// Requires at least one draw pass so [`DockWidgetState::pane_bounds`] is populated
    /// (run the dock widget once or wait for the first frame).
    /// Returns `true` if focus moved to a neighbor.
    ///
    /// When
    /// [`DockBuilder::focus_frame_groups`](crate::widget::DockBuilder::focus_frame_groups)
    /// restricts the focus frame, only panes in those groups are candidates — ineligible
    /// panes are skipped rather than stepped through.
    #[expect(
        clippy::must_use_candidate,
        reason = "This is a command-style API; callers may intentionally ignore failed movement."
    )]
    pub fn focus_adjacent(&self, direction: Direction) -> bool {
        let state = self.inner.borrow();
        let Some(pane) = state.focused_pane else {
            return false;
        };
        // The current pane stays in the map as the origin rectangle even when it is not
        // itself an eligible target (e.g. focus sits on a tool pane).
        let bounds = pane_bounds_map(&state.pane_bounds)
            .into_iter()
            .filter(|&(id, _)| id == pane || state.frame_eligible(id))
            .collect();
        drop(state);
        let Some(adjacent) = adjacent_pane(pane, direction, &bounds) else {
            return false;
        };
        let _ = self.focus_pane(adjacent);
        true
    }

    /// Move the active tab to the nearest pane in `direction`.
    ///
    /// Uses the same spatial lookup as [`Self::focus_adjacent`], so it also requires
    /// at least one draw pass for [`DockWidgetState::pane_bounds`] to be populated.
    /// Returns `true` if the active tab moved to a neighbor.
    #[expect(
        clippy::must_use_candidate,
        reason = "This is a command-style API; callers may intentionally ignore failed movement."
    )]
    pub fn move_active_panel_adjacent(&self, direction: Direction) -> bool {
        let state = self.inner.borrow();
        let Some(source_pane) = state.focused_pane else {
            return false;
        };
        let Some(NodeKind::Pane(pane_state)) = state.layout.kind(source_pane) else {
            return false;
        };
        let Some(source_panel) = pane_state.active.or(pane_state.tabs.first().copied()) else {
            return false;
        };
        let bounds = pane_bounds_map(&state.pane_bounds);
        let Some(target_pane) = adjacent_pane(source_pane, direction, &bounds) else {
            return false;
        };
        drop(state);

        let factory = Factory;
        let manager = DockManager;
        let mut state = self.inner.borrow_mut();
        if !manager.fill_allowed(&state.layout, source_panel, target_pane) {
            return false;
        }
        if factory
            .dock_fill(&mut state.layout, source_panel, target_pane)
            .is_err()
        {
            return false;
        }
        state.layout_dirty = true;
        state.focus(target_pane);
        state.sync_index();
        true
    }

    /// Split the focused pane in `direction` and move the active tab into the new pane.
    ///
    /// Returns `false` without changing the layout when the focused pane has fewer than
    /// two tabs.
    #[expect(
        clippy::must_use_candidate,
        reason = "This is a command-style API; callers may intentionally ignore failed movement."
    )]
    pub fn split_active_panel(&self, direction: Direction) -> bool {
        let state = self.inner.borrow();
        let Some(source_pane) = state.focused_pane else {
            return false;
        };
        let Some(NodeKind::Pane(pane_state)) = state.layout.kind(source_pane) else {
            return false;
        };
        // Harlo: an anchored pane keeps its tabs.
        if pane_state.tabs.len() <= 1 || pane_state.anchored {
            return false;
        }
        let Some(source_panel) = pane_state.active.or(pane_state.tabs.first().copied()) else {
            return false;
        };
        drop(state);

        let factory = Factory;
        let mut state = self.inner.borrow_mut();
        if factory
            .split_same_pane_edge(
                &mut state.layout,
                source_pane,
                source_panel,
                operation_for_direction(direction),
            )
            .is_err()
        {
            return false;
        }

        let Some(target_pane) = state.layout.get(source_panel).and_then(|e| e.owner) else {
            state.layout_dirty = true;
            state.sync_index();
            return false;
        };
        if !matches!(state.layout.kind(target_pane), Some(NodeKind::Pane(_))) {
            state.layout_dirty = true;
            state.sync_index();
            return false;
        }

        state.layout_dirty = true;
        state.focus(target_pane);
        state.sync_index();
        true
    }

    /// Clear global pane focus (and the focus frame) without changing active tabs.
    pub fn clear_focus(&self) {
        let mut state = self.inner.borrow_mut();
        if state.focused_pane.is_some() || state.focus_frame_pane.is_some() {
            state.focused_pane = None;
            state.focus_frame_pane = None;
            state.focus_dirty = true;
        }
    }

    /// Cycle the active tab in the focused pane (wraps at ends).
    pub fn cycle_panel(&self, cycle: PanelCycle) -> Result {
        let pane = self.focused_pane().ok_or(Error::InvalidTarget)?;
        let state = self.inner.borrow();
        let NodeKind::Pane(pane_state) = state.layout.kind(pane).ok_or(Error::InvalidTarget)?
        else {
            return Err(Error::InvalidTarget);
        };
        if pane_state.tabs.is_empty() {
            return Err(Error::InvalidTarget);
        }
        let current = pane_state
            .active
            .or(pane_state.tabs.first().copied())
            .ok_or(Error::InvalidTarget)?;
        let current_index = pane_state
            .tabs
            .iter()
            .position(|&id| id == current)
            .unwrap_or(0);
        let len = pane_state.tabs.len();
        let next_index = match cycle {
            PanelCycle::Next => (current_index + 1) % len,
            PanelCycle::Prev => (current_index + len - 1) % len,
        };
        let panel = pane_state.tabs[next_index];
        drop(state);
        self.dispatch(DockAction::Tab(TabAction::Select { pane, panel }));
        Ok(())
    }

    /// Currently focused panel id (active tab in the focused pane), if any.
    #[must_use]
    pub fn active_panel(&self) -> Option<String> {
        let state = self.inner.borrow();
        let pane = state.focused_pane?;
        active_panel_in_pane(&state.layout, &state.index, pane)
    }

    /// Active panel id string in a specific pane (regardless of global focus).
    #[must_use]
    pub fn active_panel_in_pane(&self, pane: NodeId) -> Option<String> {
        let state = self.inner.borrow();
        active_panel_in_pane(&state.layout, &state.index, pane)
    }

    /// Harlo: show or hide the tab bar of the pane holding `panel_id`.
    ///
    /// A pane without its tab bar is an element pane, moved by a drag handle on `handle`'s side.
    /// Only a pane holding exactly one panel can lose its bar.
    pub fn set_tab_bar(&self, panel_id: &str, handle: Option<HandleSide>) -> Result {
        let pane_id = self
            .pane_for_panel(panel_id)
            .ok_or_else(|| Error::UnknownPanel(panel_id.into()))?;
        let mut state = self.inner.borrow_mut();
        let Some(NodeKind::Pane(pane)) = state.layout.get_mut(pane_id).map(|e| &mut e.kind) else {
            return Err(Error::InvalidTarget);
        };
        if handle.is_some() && pane.tabs.len() != 1 {
            return Err(Error::InvalidTarget);
        }
        pane.handle = handle;
        state.layout_dirty = true;
        Ok(())
    }

    /// Harlo: whether the pane holding `panel_id` shows its tab bar, and how many tabs it holds.
    #[must_use]
    pub fn tab_bar(&self, panel_id: &str) -> Option<(bool, usize)> {
        let pane_id = self.pane_for_panel(panel_id)?;
        let state = self.inner.borrow();
        match state.layout.kind(pane_id) {
            Some(NodeKind::Pane(pane)) => Some((!pane.is_element(), pane.tabs.len())),
            _ => None,
        }
    }

    fn resolve_pane(&self, target: &PaneTarget) -> Result<NodeId> {
        match target {
            PaneTarget::Named(name) => self
                .inner
                .borrow()
                .index
                .pane_node(name)
                .ok_or_else(|| Error::UnknownPane(name.clone())),
            PaneTarget::Active => self.inner.borrow().focused_pane.ok_or(Error::InvalidTarget),
            PaneTarget::First => {
                first_pane(&self.inner.borrow().layout).ok_or(Error::InvalidTarget)
            }
            PaneTarget::Beside(panel_id) => self
                .pane_for_panel(panel_id)
                .ok_or_else(|| Error::UnknownPanel(panel_id.clone())),
        }
    }
}

fn operation_for_direction(direction: Direction) -> DockOperation {
    match direction {
        Direction::Left => DockOperation::Left,
        Direction::Right => DockOperation::Right,
        Direction::Up => DockOperation::Top,
        Direction::Down => DockOperation::Bottom,
    }
}

fn resolve_initial_focus<K>(
    built: &BuiltLayout<K>,
    focus: InitialFocus<'_>,
) -> Result<Option<NodeId>> {
    match focus {
        InitialFocus::FirstPane => Ok(first_pane(&built.layout)),
        InitialFocus::NamedPane(name) => built
            .index
            .pane_node(name.as_ref())
            .ok_or_else(|| Error::UnknownPane(name.to_string()))
            .map(Some),
        InitialFocus::NamedPanel(panel_id) => {
            pane_for_panel(&built.layout, &built.index, panel_id.as_ref())
                .ok_or_else(|| Error::UnknownPanel(panel_id.to_string()))
                .map(Some)
        }
    }
}

impl<K> fmt::Debug for DockSession<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DockSession").finish_non_exhaustive()
    }
}
