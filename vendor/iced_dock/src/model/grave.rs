use super::{Axis, HandleSide, NodeId};

/// Where a named pane sat before it collapsed, so a panel targeted at that name reopens the
/// pane in its old place instead of landing in an unrelated one.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PaneGrave {
    pub name: String,
    pub group: Option<String>,
    /// Sibling subtree the pane sat next to; reviving splits it again.
    pub anchor: NodeId,
    pub axis: Axis,
    /// Pane sat after `anchor` along `axis`.
    pub after: bool,
    /// Share of the parent group the pane held.
    pub proportion: f32,
    /// Handle side when the pane was an element pane.
    #[cfg_attr(feature = "serde", serde(default))]
    pub handle: Option<HandleSide>,
}
