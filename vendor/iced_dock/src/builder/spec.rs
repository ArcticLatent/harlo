use std::collections::HashSet;

use crate::model::{Axis, HandleSide};
use crate::Error;

/// Declarative layout description.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LayoutTree<K> {
    /// Tabbed pane (typical split leaf).
    Tabs(TabsNode<K>),
    /// Nested split container.
    Split(SplitNode<K>),
    /// Single panel without a tab strip, moved by a drag handle.
    Element(ElementNode<K>),
}

/// Panel metadata used when building or opening tabs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PanelDef<K> {
    pub id: String,
    pub title: String,
    pub content: K,
    pub can_close: bool,
    pub can_drag: bool,
    pub can_drop: bool,
    pub group: Option<String>,
}

impl<K: Copy> PanelDef<K> {
    pub fn new(id: impl Into<String>, title: impl Into<String>, content: K) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            content,
            can_close: true,
            can_drag: true,
            can_drop: true,
            group: None,
        }
    }

    #[must_use]
    pub fn can_close(mut self, value: bool) -> Self {
        self.can_close = value;
        self
    }

    #[must_use]
    pub fn can_drag(mut self, value: bool) -> Self {
        self.can_drag = value;
        self
    }

    #[must_use]
    pub fn can_drop(mut self, value: bool) -> Self {
        self.can_drop = value;
        self
    }

    #[must_use]
    pub fn group(mut self, g: impl Into<String>) -> Self {
        self.group = Some(g.into());
        self
    }
}

/// Tabbed pane node.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TabsNode<K> {
    pub name: Option<String>,
    pub panels: Vec<PanelDef<K>>,
    pub active: Option<String>,
    pub group: Option<String>,
    pub persistent: bool,
    /// Harlo: see [`crate::model::Pane::anchored`].
    #[cfg_attr(feature = "serde", serde(default))]
    pub anchored: bool,
}

impl<K: Copy> TabsNode<K> {
    pub fn new(panels: impl IntoIterator<Item = PanelDef<K>>) -> Self {
        Self {
            name: None,
            panels: panels.into_iter().collect(),
            active: None,
            group: None,
            persistent: false,
            anchored: false,
        }
    }

    /// Harlo: anchors the pane (see [`crate::model::Pane::anchored`]); it also stays when empty.
    #[must_use]
    pub fn anchored(mut self) -> Self {
        self.anchored = true;
        self.persistent = true;
        self
    }

    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn active(mut self, panel_id: impl Into<String>) -> Self {
        self.active = Some(panel_id.into());
        self
    }

    #[must_use]
    pub fn group(mut self, g: impl Into<String>) -> Self {
        self.group = Some(g.into());
        self
    }

    #[must_use]
    pub fn persistent(mut self, value: bool) -> Self {
        self.persistent = value;
        self
    }
}

/// Element node: one panel, no tab strip, dragged by a handle on [`ElementNode::handle`].
///
/// Unlike a [`TabsNode`] it is not a tab group: tabs cannot be dropped into it and it cannot be
/// dropped into a tab group. It can only be docked against the edges of other panes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ElementNode<K> {
    pub name: Option<String>,
    pub panel: PanelDef<K>,
    pub handle: HandleSide,
}

impl<K: Copy> ElementNode<K> {
    pub fn new(panel: PanelDef<K>) -> Self {
        Self {
            name: None,
            panel,
            handle: HandleSide::default(),
        }
    }

    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn handle(mut self, side: HandleSide) -> Self {
        self.handle = side;
        self
    }
}

/// Split container node.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SplitNode<K> {
    pub axis: Axis,
    pub children: Vec<LayoutTree<K>>,
    pub weights: Option<Vec<f32>>,
}

impl<K> SplitNode<K> {
    pub fn new(axis: Axis, children: impl IntoIterator<Item = LayoutTree<K>>) -> Self {
        Self {
            axis,
            children: children.into_iter().collect(),
            weights: None,
        }
    }

    #[must_use]
    pub fn weights(mut self, weights: impl IntoIterator<Item = f32>) -> Self {
        self.weights = Some(weights.into_iter().collect());
        self
    }
}

impl<K: Copy> LayoutTree<K> {
    /// Set the active tab on a `Tabs` node.
    #[must_use]
    pub fn active(mut self, panel_id: impl Into<String>) -> Self {
        if let Self::Tabs(ref mut node) = self {
            node.active = Some(panel_id.into());
        }
        self
    }

    /// Assign a stable name to a `Tabs` or `Element` node for
    /// [`PaneTarget::Named`](crate::builder::PaneTarget).
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        match self {
            Self::Tabs(ref mut node) => node.name = Some(name.into()),
            Self::Element(ref mut node) => node.name = Some(name.into()),
            Self::Split(_) => {}
        }
        self
    }

    /// Set the drag-handle side of an `Element` node.
    #[must_use]
    pub fn handle(mut self, side: HandleSide) -> Self {
        if let Self::Element(ref mut node) = self {
            node.handle = side;
        }
        self
    }

    /// Set split weights on a `Split` node.
    #[must_use]
    pub fn weights(mut self, weights: impl IntoIterator<Item = f32>) -> Self {
        if let Self::Split(ref mut node) = self {
            node.weights = Some(weights.into_iter().collect());
        }
        self
    }

    /// Harlo: anchors a `Tabs` node (see [`crate::model::Pane::anchored`]).
    #[must_use]
    pub fn anchored(self) -> Self {
        match self {
            Self::Tabs(node) => Self::Tabs(node.anchored()),
            other => other,
        }
    }

    /// Assign a tab group to a `Tabs` node.
    #[must_use]
    pub fn group(mut self, g: impl Into<String>) -> Self {
        if let Self::Tabs(ref mut node) = self {
            node.group = Some(g.into());
        }
        self
    }

    /// Mark a `Tabs` node as persistent (never collapsed when empty).
    #[must_use]
    pub fn persistent(mut self, value: bool) -> Self {
        if let Self::Tabs(ref mut node) = self {
            node.persistent = value;
        }
        self
    }
}

/// Create a panel definition (for use inside [`tabs`]).
pub fn panel<K: Copy>(id: impl Into<String>, title: impl Into<String>, content: K) -> PanelDef<K> {
    PanelDef::new(id, title, content)
}

/// Create a tabbed pane node.
pub fn tabs<K: Copy>(panels: impl IntoIterator<Item = PanelDef<K>>) -> LayoutTree<K> {
    LayoutTree::Tabs(TabsNode::new(panels))
}

/// Create an element: a single panel without a tab strip, moved by a drag handle.
///
/// The handle sits on the top edge by default; change it with [`LayoutTree::handle`].
#[must_use]
pub fn element<K: Copy>(def: PanelDef<K>) -> LayoutTree<K> {
    LayoutTree::Element(ElementNode::new(def))
}

/// Create a horizontal split.
pub fn horizontal<K>(children: impl IntoIterator<Item = LayoutTree<K>>) -> LayoutTree<K> {
    LayoutTree::Split(SplitNode::new(Axis::Horizontal, children))
}

/// Create a vertical split.
pub fn vertical<K>(children: impl IntoIterator<Item = LayoutTree<K>>) -> LayoutTree<K> {
    LayoutTree::Split(SplitNode::new(Axis::Vertical, children))
}

/// Single panel occupying the full dock area.
#[must_use]
pub fn single<K: Copy>(def: PanelDef<K>) -> LayoutTree<K> {
    LayoutTree::Tabs(TabsNode::new([def]))
}

/// Validate a layout tree before compilation.
pub(crate) fn validate_tree<K>(tree: &LayoutTree<K>) -> crate::Result {
    let mut panel_ids = HashSet::new();
    let mut pane_names = HashSet::new();
    validate_node(tree, &mut panel_ids, &mut pane_names)
}

fn validate_node<K>(
    tree: &LayoutTree<K>,
    panel_ids: &mut HashSet<String>,
    pane_names: &mut HashSet<String>,
) -> crate::Result {
    match tree {
        LayoutTree::Tabs(node) => {
            if node.panels.is_empty() {
                return Err(Error::EmptyLayout);
            }
            if let Some(name) = &node.name {
                if !pane_names.insert(name.clone()) {
                    return Err(Error::DuplicatePaneName(name.clone()));
                }
            }
            let pane_label = node.name.clone().unwrap_or_else(|| "<unnamed>".into());
            for def in &node.panels {
                if !panel_ids.insert(def.id.clone()) {
                    return Err(Error::DuplicatePanelId(def.id.clone()));
                }
            }
            if let Some(active) = &node.active {
                if !node.panels.iter().any(|p| p.id == *active) {
                    return Err(Error::UnknownActivePanel {
                        pane_name: pane_label,
                        panel: active.clone(),
                    });
                }
            }
            Ok(())
        }
        LayoutTree::Element(node) => {
            if let Some(name) = &node.name {
                if !pane_names.insert(name.clone()) {
                    return Err(Error::DuplicatePaneName(name.clone()));
                }
            }
            if !panel_ids.insert(node.panel.id.clone()) {
                return Err(Error::DuplicatePanelId(node.panel.id.clone()));
            }
            Ok(())
        }
        LayoutTree::Split(node) => {
            if node.children.is_empty() {
                return Err(Error::EmptyLayout);
            }
            if let Some(weights) = &node.weights {
                if weights.len() != node.children.len() {
                    return Err(Error::InvalidWeights {
                        expected: node.children.len(),
                        got: weights.len(),
                    });
                }
            }
            for child in &node.children {
                validate_node(child, panel_ids, pane_names)?;
            }
            Ok(())
        }
    }
}
