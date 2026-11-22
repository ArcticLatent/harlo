use super::NodeId;

/// Single tab content (leaf node).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Panel<K> {
    pub id: String,
    pub title: String,
    pub content: K,
    pub can_close: bool,
    pub can_drag: bool,
    pub can_drop: bool,
    pub group: Option<String>,
}

impl<K: Copy> Panel<K> {
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
}

/// Side of an element pane that carries its drag handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HandleSide {
    Left,
    Right,
    #[default]
    Top,
    Bottom,
}

/// Tabbed pane host.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pane {
    pub name: Option<String>,
    pub tabs: Vec<NodeId>,
    pub active: Option<NodeId>,
    pub group: Option<String>,
    pub persistent: bool,
    /// `Some` makes this an element pane: no tab strip, exactly one panel, moved by a drag
    /// handle on this side. It never accepts other panels and never joins a tab group.
    #[cfg_attr(feature = "serde", serde(default))]
    pub handle: Option<HandleSide>,
    /// Harlo: an anchored pane stays where it is. Its tabs only reorder within its own strip,
    /// nothing from another pane joins it or splits beside it, and it never closes when empty.
    #[cfg_attr(feature = "serde", serde(default))]
    pub anchored: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Self::new()
    }
}

impl Pane {
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: None,
            tabs: Vec::new(),
            active: None,
            group: None,
            persistent: false,
            handle: None,
            anchored: false,
        }
    }

    /// Whether this is an element pane (see [`Self::handle`]).
    #[must_use]
    pub fn is_element(&self) -> bool {
        self.handle.is_some()
    }

    #[must_use]
    pub fn active_index(&self) -> Option<usize> {
        let active = self.active?;
        self.tabs.iter().position(|&id| id == active)
    }
}
