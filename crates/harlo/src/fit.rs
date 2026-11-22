//! Which side panels fit beside the main area at a window size, and how big
//! the user may make them.
//!
//! The main area comes first. When it would get narrower than [`MAIN_MIN`],
//! the right panel makes way, then the sidebar. Hiding a panel for space
//! doesn't change the user's choice: once there's room it's back, and a
//! panel the user closed stays closed. A panel the user opens while there's
//! no room for it is kept, and the other one makes way instead.

// Rust guideline compliant 2026-10-09

/// Width of the left and right rails, as `Size.rail` in the UI.
pub const RAIL: f32 = 56.0;

/// The sidebar's width until the user drags its edge.
pub const SIDEBAR: f32 = 288.0;

/// The narrowest and widest the sidebar can be dragged.
///
/// Below 220 px session titles are cut to a word or two; above 480 px the
/// list only adds empty space.
pub const SIDEBAR_RANGE: (f32, f32) = (220.0, 480.0);

/// The right panel's width until the user drags its edge.
pub const PANEL: f32 = 360.0;

/// The narrowest and widest the right panel can be dragged.
///
/// 260 px keeps a file path and its counts on one line in Changes; past
/// 720 px a wide diff is better maximized.
pub const PANEL_RANGE: (f32, f32) = (260.0, 720.0);

/// The terminals' height until the user drags their top edge.
pub const TERMINAL: f32 = 260.0;

/// The shortest the terminals can be dragged: a few lines of output under
/// their tab strip.
pub const TERMINAL_MIN: f32 = 120.0;

/// The least height the main area keeps above the terminals: the tab strip
/// and a composer with a line or two of conversation.
pub const ABOVE_TERMINAL: f32 = 260.0;

/// The narrowest the main area gets before side panels make way.
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

/// One of the side panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The sessions sidebar on the left.
    Sidebar,
    /// The tool panel on the right.
    Panel,
}

/// The side panels the user wants open, at the widths they chose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wanted {
    /// The sidebar's width, if it's open.
    pub sidebar: Option<f32>,
    /// The right panel's width, if one is open.
    pub panel: Option<f32>,
    /// A panel the user opened while it didn't fit; it stays, and the other
    /// one makes way.
    pub keep: Option<Side>,
}

/// Which side panels are on screen, and how wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown {
    /// The sidebar's width, when it's on screen.
    pub sidebar: Option<f32>,
    /// The right panel's width, when it's on screen.
    pub panel: Option<f32>,
}

impl Shown {
    /// The width left for the main area in a window `width` wide.
    #[must_use]
    pub fn main_width(self, width: f32) -> f32 {
        width - 2.0 * RAIL - self.sidebar.unwrap_or(0.0) - self.panel.unwrap_or(0.0)
    }

    /// Whether `side` is on screen.
    #[must_use]
    pub fn has(self, side: Side) -> bool {
        match side {
            Side::Sidebar => self.sidebar.is_some(),
            Side::Panel => self.panel.is_some(),
        }
    }
}

/// Works out which of the wanted panels are on screen in a window `width` wide.
#[must_use]
pub fn shown(width: f32, wanted: Wanted) -> Shown {
    let mut shown = Shown {
        sidebar: wanted.sidebar,
        panel: wanted.panel,
    };
    let order = if wanted.keep == Some(Side::Panel) {
        [Side::Sidebar, Side::Panel]
    } else {
        [Side::Panel, Side::Sidebar]
    };
    for side in order {
        if shown.main_width(width) >= MAIN_MIN {
            break;
        }
        if wanted.keep == Some(side) {
            continue;
        }
        match side {
            Side::Sidebar => shown.sidebar = None,
            Side::Panel => shown.panel = None,
        }
    }
    shown
}

/// The widest `side` can be dragged in a window `width` wide while the main
/// area keeps [`MAIN_MIN`] beside what else is shown, within its range.
#[must_use]
pub fn widest(side: Side, width: f32, shown: Shown) -> f32 {
    let (least, most) = range(side);
    let other = match side {
        Side::Sidebar => shown.panel,
        Side::Panel => shown.sidebar,
    };
    let room = width - 2.0 * RAIL - other.unwrap_or(0.0) - MAIN_MIN;
    room.clamp(least, most)
}

/// The narrowest and widest `side` can be.
#[must_use]
pub fn range(side: Side) -> (f32, f32) {
    match side {
        Side::Sidebar => SIDEBAR_RANGE,
        Side::Panel => PANEL_RANGE,
    }
}

/// How tall the terminals are in a window `height` tall, when the user
/// chose `wanted`: no shorter than [`TERMINAL_MIN`], and leaving
/// [`ABOVE_TERMINAL`] above them while the window has room for both.
#[must_use]
pub fn terminal_height(height: f32, wanted: f32) -> f32 {
    wanted.min(height - ABOVE_TERMINAL).max(TERMINAL_MIN)
}
