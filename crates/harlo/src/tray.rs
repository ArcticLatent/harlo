//! Harlo's tray icon: the logo with a dot while sessions work (blue) or
//! one needs you (amber), and a menu listing them.
//!
//! The icon is a `StatusNotifierItem` on the session bus, which the desktop's
//! tray shows. It runs on a thread of its own; what the user chooses in it
//! comes back through the callback given to [`Tray::start`].

// Rust guideline compliant 2026-10-09

use std::fmt;

use harlo_core::sessions::SessionSummary;
use ksni::blocking::{Handle, TrayMethods as _};
use ksni::menu::StandardItem;
use ksni::{Category, Icon, MenuItem, Status, ToolTip};
use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
use resvg::usvg;

use crate::app::App;

/// Harlo's logo, as the sidebar shows it.
const LOGO: &[u8] = include_bytes!("../ui/images/logo-mark.svg");

/// The sizes the icon is drawn at; the tray picks the closest.
const SIZES: [u16; 3] = [22, 32, 48];

/// The dot's colours: blue while sessions work, amber when one needs you.
const WORKING: [u8; 3] = [0x4c, 0x8d, 0xf6];
const WAITING: [u8; 3] = [0xf5, 0xa5, 0x24];

/// The ring around the dot, so it stands apart from the logo.
const RING: [u8; 3] = [0x1e, 0x1e, 0x22];

/// The dot's radius, and its ring's width, as parts of the icon's size.
const DOT: f32 = 0.24;
const RING_WIDTH: f32 = 0.07;

/// How many characters of a session's title the menu shows.
const TITLE_CHARS: usize = 48;

/// What the user chose in the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Show the window.
    Show,
    /// Show the window on this session.
    Open(String),
    /// Quit Harlo.
    Quit,
}

/// A session the tray lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// OpenCode's session ID.
    pub id: String,
    /// Its title, as the sidebar shows it.
    pub title: String,
}

/// What the tray shows: the sessions that need the user, then those working.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// Sessions waiting for an approval or an answer.
    pub waiting: Vec<Entry>,
    /// Sessions working, and not waiting.
    pub working: Vec<Entry>,
}

/// The sessions of every project, and of their worktrees, that need the
/// user or are working.
#[must_use]
pub fn summary(app: &App) -> Summary {
    let mut summary = Summary::default();
    let sessions = app.projects.iter().flat_map(|project| {
        project
            .sessions
            .iter()
            .chain(project.worktrees.iter().flat_map(|w| &w.sessions))
    });
    for session in sessions {
        let entry = || entry(session);
        if session.waiting {
            summary.waiting.push(entry());
        } else if session.busy {
            summary.working.push(entry());
        }
    }
    summary
}

/// A session as the tray lists it.
fn entry(session: &SessionSummary) -> Entry {
    let title = if session.title.is_empty() {
        "Untitled session".to_owned()
    } else {
        session.title.clone()
    };
    Entry {
        id: session.id.clone(),
        title,
    }
}

/// What the tray's tooltip says about `summary`.
#[must_use]
pub fn describe(summary: &Summary) -> String {
    let count = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    let mut parts = Vec::new();
    if !summary.waiting.is_empty() {
        parts.push(count(
            summary.waiting.len(),
            "session needs you",
            "sessions need you",
        ));
    }
    if !summary.working.is_empty() {
        parts.push(count(summary.working.len(), "working", "working"));
    }
    if parts.is_empty() {
        "Nothing running".to_owned()
    } else {
        parts.join(" · ")
    }
}

/// Harlo's icon in the tray.
pub struct Tray {
    /// The running tray service.
    handle: Handle<Item>,
    /// What it shows now, so unchanged summaries aren't sent again.
    shown: Summary,
}

impl fmt::Debug for Tray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tray")
            .field("shown", &self.shown)
            .finish_non_exhaustive()
    }
}

impl Tray {
    /// Puts Harlo's icon in the desktop's tray, calling `on` with what the
    /// user chooses in it, on the tray's own thread.
    ///
    /// # Errors
    ///
    /// Fails when there's no session bus, or the desktop shows no tray.
    pub fn start(on: impl Fn(Choice) + Send + 'static) -> Result<Self, ksni::Error> {
        let item = Item {
            summary: Summary::default(),
            icons: Icons::draw(),
            on: Box::new(on),
        };
        Ok(Self {
            handle: item.spawn()?,
            shown: Summary::default(),
        })
    }

    /// Shows `summary`, if it changed.
    pub fn show(&mut self, summary: Summary) {
        if summary == self.shown {
            return;
        }
        self.shown = summary.clone();
        // `None` only once the tray has stopped: there's nothing to show it on.
        let _ = self.handle.update(move |item| item.summary = summary);
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // Takes the icon away now rather than when the process ends.
        drop(self.handle.shutdown());
    }
}

/// The icon in each state, drawn once.
struct Icons {
    idle: Vec<Icon>,
    working: Vec<Icon>,
    waiting: Vec<Icon>,
}

impl Icons {
    /// Draws the logo at each size, plain and with each dot.
    fn draw() -> Self {
        let logo = usvg::Tree::from_data(LOGO, &usvg::Options::default()).ok();
        let draw = |dot: Option<[u8; 3]>| {
            SIZES
                .iter()
                .filter_map(|&size| icon(logo.as_ref(), size, dot))
                .collect()
        };
        Self {
            idle: draw(None),
            working: draw(Some(WORKING)),
            waiting: draw(Some(WAITING)),
        }
    }
}

/// The logo at `size` pixels square, with a dot of colour `dot` at its
/// lower right; `None` when the pixmap can't be made.
fn icon(logo: Option<&usvg::Tree>, size: u16, dot: Option<[u8; 3]>) -> Option<Icon> {
    let mut pixmap = Pixmap::new(u32::from(size), u32::from(size))?;
    let extent = f32::from(size);
    if let Some(logo) = logo {
        let view = logo.size();
        let scale = extent / view.width().max(view.height());
        let left = (extent - view.width() * scale) / 2.0;
        let top = (extent - view.height() * scale) / 2.0;
        resvg::render(
            logo,
            Transform::from_row(scale, 0.0, 0.0, scale, left, top),
            &mut pixmap.as_mut(),
        );
    }
    if let Some(colour) = dot {
        let radius = extent * DOT;
        let centre = extent - radius;
        for (colour, r) in [(RING, radius), (colour, radius - extent * RING_WIDTH)] {
            let circle = PathBuilder::from_circle(centre, centre, r)?;
            let mut paint = Paint::default();
            paint.set_color_rgba8(colour[0], colour[1], colour[2], u8::MAX);
            paint.anti_alias = true;
            pixmap.fill_path(
                &circle,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
    // The tray wants ARGB in network byte order, not premultiplied.
    let data = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let c = pixel.demultiply();
            [c.alpha(), c.red(), c.green(), c.blue()]
        })
        .collect();
    let pixels = i32::from(size);
    Some(Icon {
        width: pixels,
        height: pixels,
        data,
    })
}

/// A menu label for `text`: underscores would otherwise mark access keys,
/// and long titles are cut.
fn label(text: &str) -> String {
    let mut cut: String = text.chars().take(TITLE_CHARS).collect();
    if text.chars().nth(TITLE_CHARS).is_some() {
        cut.push('…');
    }
    cut.replace('_', "__")
}

/// The tray service's state.
struct Item {
    summary: Summary,
    icons: Icons,
    on: Box<dyn Fn(Choice) + Send>,
}

impl ksni::Tray for Item {
    fn id(&self) -> String {
        "harlo".to_owned()
    }

    fn title(&self) -> String {
        "Harlo".to_owned()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        if self.summary.waiting.is_empty() {
            Status::Active
        } else {
            Status::NeedsAttention
        }
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        if !self.summary.waiting.is_empty() {
            self.icons.waiting.clone()
        } else if !self.summary.working.is_empty() {
            self.icons.working.clone()
        } else {
            self.icons.idle.clone()
        }
    }

    fn attention_icon_pixmap(&self) -> Vec<Icon> {
        self.icons.waiting.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Harlo".to_owned(),
            description: describe(&self.summary),
            ..ToolTip::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.on)(Choice::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut menu = Vec::new();
        for (heading, entries) in [
            ("Needs you", &self.summary.waiting),
            ("Working", &self.summary.working),
        ] {
            if entries.is_empty() {
                continue;
            }
            menu.push(
                StandardItem {
                    label: heading.to_owned(),
                    enabled: false,
                    ..StandardItem::default()
                }
                .into(),
            );
            for entry in entries {
                let id = entry.id.clone();
                menu.push(
                    StandardItem {
                        label: label(&entry.title),
                        activate: Box::new(move |item: &mut Self| {
                            (item.on)(Choice::Open(id.clone()));
                        }),
                        ..StandardItem::default()
                    }
                    .into(),
                );
            }
            menu.push(MenuItem::Separator);
        }
        menu.push(
            StandardItem {
                label: "Show Harlo".to_owned(),
                activate: Box::new(|item: &mut Self| (item.on)(Choice::Show)),
                ..StandardItem::default()
            }
            .into(),
        );
        menu.push(
            StandardItem {
                label: "Quit Harlo".to_owned(),
                shortcut: vec![vec!["Control".to_owned(), "Q".to_owned()]],
                activate: Box::new(|item: &mut Self| (item.on)(Choice::Quit)),
                ..StandardItem::default()
            }
            .into(),
        );
        menu
    }
}
