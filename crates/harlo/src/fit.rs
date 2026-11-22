//! Whether the sidebar fits beside the main area at a window width.
//!
//! The main area comes first. When it would get narrower than [`MAIN_MIN`],
//! the sidebar makes way. Hiding it for space doesn't change the user's
//! choice: once there's room it's back, and a sidebar the user closed stays
//! closed. A sidebar the user opens while there's no room for it is kept.

// Rust guideline compliant 2026-10-09

use crate::style::size;

/// The narrowest the main area gets before the sidebar makes way.
///
/// Room for the composer with its pickers on one line (about 600 px)
/// would be roomier, but at 560 a 1,100 px window keeps the sidebar open
/// beside the conversation, which matters more than one line of pickers.
pub const MAIN_MIN: f32 = 560.0;

/// The window's size when Harlo opens.
pub const WINDOW: (f32, f32) = (1588.0, 947.0);

/// The smallest window Harlo can be made: both rails and a main area of
/// [`MAIN_MIN`], rounded up.
pub const WINDOW_MIN: (f32, f32) = (720.0, 480.0);

/// What's on screen beside the main area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    /// The sidebar is on screen.
    pub sidebar: bool,
}

impl Shown {
    /// The width left for the main area in a window `width` wide.
    #[must_use]
    pub fn main_width(self, width: f32) -> f32 {
        let mut main = width - 2.0 * size::RAIL;
        if self.sidebar {
            main -= size::SIDEBAR;
        }
        main
    }
}

/// Works out whether the sidebar is on screen in a window `width` wide.
///
/// `sidebar` is whether the user has it open; `keep` is whether they opened
/// it while it didn't fit, so it stays.
#[must_use]
pub fn shown(width: f32, sidebar: bool, keep: bool) -> Shown {
    let mut shown = Shown { sidebar };
    if !keep && shown.main_width(width) < MAIN_MIN {
        shown.sidebar = false;
    }
    shown
}
