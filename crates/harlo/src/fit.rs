//! Which side panels fit beside the conversation at a window width.
//!
//! The conversation column comes first. When it would get narrower than
//! [`MAIN_MIN`], the right panel makes way, then the sidebar. Hiding a panel
//! for space doesn't change the user's choice: once there's room it's back,
//! and a panel the user closed stays closed. A panel the user opens while
//! there's no room for it is kept, and the other one makes way instead.

// Rust guideline compliant 2026-10-09

use crate::style::size;

/// The narrowest the conversation column gets before side panels make way.
///
/// Room for the composer with its pickers on one line (about 600 px)
/// would be roomier, but at 560 a 1,100 px window keeps the sidebar open
/// beside the conversation, which matters more than one line of pickers.
pub const MAIN_MIN: f32 = 560.0;

/// The window's size when Harlo opens.
pub const WINDOW: (f32, f32) = (1588.0, 947.0);

/// The smallest window Harlo can be made: both rails and a conversation
/// column of [`MAIN_MIN`], rounded up.
pub const WINDOW_MIN: (f32, f32) = (720.0, 480.0);

/// One of the side panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The sessions sidebar on the left.
    Sidebar,
    /// The panel on the right (context, todos, changes, files, notes).
    Panel,
}

/// Which side panels are on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    /// The sidebar is on screen.
    pub sidebar: bool,
    /// The right panel is on screen.
    pub panel: bool,
}

impl Shown {
    /// The width left for the conversation column in a window `width` wide.
    #[must_use]
    pub fn main_width(self, width: f32) -> f32 {
        let mut main = width - 2.0 * size::RAIL;
        if self.sidebar {
            main -= size::SIDEBAR;
        }
        if self.panel {
            main -= size::PANEL;
        }
        main
    }

    /// Whether `side` is on screen.
    #[must_use]
    pub fn has(self, side: Side) -> bool {
        match side {
            Side::Sidebar => self.sidebar,
            Side::Panel => self.panel,
        }
    }
}

/// Works out which of the wanted panels are on screen in a window `width` wide.
///
/// `keep` is a panel the user opened while it didn't fit; it stays, and the
/// other one makes way.
#[must_use]
pub fn shown(width: f32, sidebar: bool, panel: bool, keep: Option<Side>) -> Shown {
    let mut shown = Shown { sidebar, panel };
    let order = if keep == Some(Side::Panel) {
        [Side::Sidebar, Side::Panel]
    } else {
        [Side::Panel, Side::Sidebar]
    };
    for side in order {
        if shown.main_width(width) >= MAIN_MIN {
            break;
        }
        if keep == Some(side) {
            continue;
        }
        match side {
            Side::Sidebar => shown.sidebar = false,
            Side::Panel => shown.panel = false,
        }
    }
    shown
}
