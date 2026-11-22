//! Declarative layout trees and [`DockSession`].

pub mod compile;
mod index;
mod session;
mod spec;

pub use index::DockIndex;
pub use session::{DockSession, InitialFocus, PaneTarget, PanelCycle};
pub use spec::{
    element, horizontal, panel, single, tabs, vertical, ElementNode, LayoutTree, PanelDef,
    SplitNode, TabsNode,
};
