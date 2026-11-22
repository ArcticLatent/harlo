//! The main area: the conversation anchored in the center, with the tool
//! panels docked around it.
//!
//! The center is one pane of documents: sessions, the file editor and the
//! start screen. Its tabs reorder along its strip, but nothing leaves it,
//! joins it or splits beside it. The tools (Context, Todos, Changes, Files
//! and Notes) open in the right dock, terminals in a dock below the center,
//! and all of them can be dragged into tab groups or splits of their own
//! around the center.
//!
//! Harlo's own state stays the source of truth for what's open: the session
//! tabs, the open file and which document is current. After each update
//! [`App::sync_dock`] makes the dock match it. The dock owns only where things
//! sit: its splits, panes and tab order. Tool panels are the exception: the
//! dock alone knows which are open.

// Rust guideline compliant 2026-10-09

use std::collections::HashSet;
use std::fmt;

use harlo_core::layout::DockNode;
use iced::Task;
use iced_dock::model::{Axis, DockOperation, NodeId, NodeKind};
use iced_dock::{
    DockEvent, DockSession, LayoutTree, PaneTarget, PanelCycle, PanelDef, horizontal, panel, tabs,
    vertical,
};

use super::{App, FilesEvent, Message, Panel};
use crate::timeline::Timeline;

/// The center pane, which holds the documents.
pub const CENTER: &str = "center";

/// The right dock's pane, where the rail opens tools.
pub const RIGHT: &str = "right";

/// The dock below the center, where terminals open.
pub const BOTTOM: &str = "bottom";

/// How much of the center's height the bottom dock takes when it opens.
const BOTTOM_SHARE: f32 = 0.32;

/// The tab group of documents: sessions, the editor and the start screen.
const DOCUMENTS: &str = "documents";

/// The tab group of the tool panels.
const TOOLS: &str = "tools";

/// How much of the width the right dock takes when it opens.
const RIGHT_SHARE: f32 = 0.26;

/// The tools the right dock starts with, the first one shown.
const STARTING_TOOLS: [Panel; 2] = [Panel::Changes, Panel::Notes];

/// What a dock tab shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockKey {
    /// The start screen, for a new session.
    Home,
    /// A session, by its slot in the dock's list of session IDs.
    Session(u32),
    /// The file editor.
    Editor,
    /// A tool panel.
    Panel(Panel),
    /// A terminal, by its number.
    Terminal(u32),
}

impl DockKey {
    /// Whether this is a document rather than a tool panel.
    #[must_use]
    pub fn is_document(self) -> bool {
        !matches!(self, Self::Panel(_) | Self::Terminal(_))
    }
}

/// A way to move around the dock from the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockMove {
    /// Focus the pane in a direction.
    Focus(iced_dock::Direction),
    /// Move the current tool tab to the pane in a direction.
    Carry(iced_dock::Direction),
    /// Split the current tool tab off into a new pane in a direction.
    Split(iced_dock::Direction),
}

/// The dock's layout, and the session IDs its tabs refer to.
pub struct Docking {
    /// The layout and its focus.
    session: DockSession<DockKey>,
    /// Session IDs by slot; a slot is never reused for another session.
    slots: Vec<String>,
    /// The document that was current after the last update; new documents
    /// open after it.
    previous: Option<DockKey>,
    /// Tools the user placed, or that the starting layout holds: opening
    /// another tool from the rail doesn't take their place.
    kept: HashSet<Panel>,
    /// The tab being dragged.
    dragging: Option<DockKey>,
}

impl fmt::Debug for Docking {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Docking")
            .field("slots", &self.slots)
            .field("kept", &self.kept)
            .finish_non_exhaustive()
    }
}

impl Default for Docking {
    fn default() -> Self {
        Self::new()
    }
}

impl Docking {
    /// The starting layout: the start screen in the center, and Changes
    /// with Notes beside it in the right dock.
    ///
    /// # Panics
    ///
    /// Never in practice: the starting layout is fixed and valid, so a
    /// failure to build it is a bug in this function.
    #[must_use]
    pub fn new() -> Self {
        let tools = tabs(STARTING_TOOLS.map(|tool| def(DockKey::Panel(tool), panel_id(tool))))
            .named(RIGHT)
            .group(TOOLS)
            .active(panel_id(STARTING_TOOLS[0]));
        let tree = horizontal([center(vec![def(DockKey::Home, "home".to_owned())]), tools])
            .weights([1.0 - RIGHT_SHARE, RIGHT_SHARE]);
        Self {
            session: DockSession::from_tree(tree).expect("the starting layout is valid"),
            slots: Vec::new(),
            previous: None,
            kept: STARTING_TOOLS.into_iter().collect(),
            dragging: None,
        }
    }

    /// The arrangement as saved, keeping the tabs it can still show; the
    /// starting layout when it has no center, or can't be built.
    ///
    /// Sessions are kept by their `session:<id>` names, before the server
    /// says whether they still exist; ones that don't close once it does.
    #[must_use]
    pub fn restore(saved: &DockNode) -> Self {
        let mut slots = Vec::new();
        let mut centers = 0;
        let Some(tree) = rebuild(saved, &mut slots, &mut centers) else {
            return Self::new();
        };
        if centers != 1 {
            return Self::new();
        }
        let Ok(session) = DockSession::from_tree(tree) else {
            return Self::new();
        };
        let mut docking = Self {
            session,
            slots,
            previous: None,
            kept: HashSet::new(),
            dragging: None,
        };
        // Tools that come back stay where they were put.
        docking.kept = docking
            .keys()
            .into_iter()
            .filter_map(|key| match key {
                DockKey::Panel(tool) => Some(tool),
                _ => None,
            })
            .collect();
        docking
    }

    /// The arrangement to save.
    #[must_use]
    pub fn saved(&self) -> Option<DockNode> {
        let state = self.session.state();
        let state = state.borrow();
        let root = state.layout.root_child()?;
        self.save_node(&state.layout, root)
    }

    fn save_node(&self, layout: &iced_dock::Layout<DockKey>, node: NodeId) -> Option<DockNode> {
        match layout.kind(node)? {
            NodeKind::Proportional(group) => {
                let parts: Vec<(DockNode, f32)> = group
                    .children
                    .iter()
                    .zip(&group.proportions)
                    .filter_map(|(&child, &share)| Some((self.save_node(layout, child)?, share)))
                    .collect();
                let total: f32 = parts.iter().map(|(_, share)| share).sum();
                let shares = parts
                    .iter()
                    .map(|(_, share)| thousandths(*share, total))
                    .collect();
                Some(DockNode::Split {
                    across: group.axis == Axis::Horizontal,
                    shares,
                    parts: parts.into_iter().map(|(part, _)| part).collect(),
                })
            }
            NodeKind::Pane(pane) => {
                let name_of = |tab: NodeId| match layout.kind(tab) {
                    Some(NodeKind::Panel(p)) => Some(self.id(p.content)),
                    _ => None,
                };
                Some(DockNode::Tabs {
                    name: pane.name.clone(),
                    group: pane.group.clone(),
                    tabs: pane.tabs.iter().filter_map(|&tab| name_of(tab)).collect(),
                    shown: pane.active.and_then(name_of),
                })
            }
            _ => None,
        }
    }

    /// The dock's layout state, for the widget.
    #[must_use]
    pub fn state(&self) -> std::rc::Rc<std::cell::RefCell<iced_dock::DockWidgetState<DockKey>>> {
        self.session.state()
    }

    /// The key for a session, giving it a slot the first time.
    pub fn session_key(&mut self, id: &str) -> DockKey {
        let slot = self.slots.iter().position(|s| s == id).unwrap_or_else(|| {
            self.slots.push(id.to_owned());
            self.slots.len() - 1
        });
        DockKey::Session(u32::try_from(slot).unwrap_or(u32::MAX))
    }

    /// The session ID behind a slot.
    #[must_use]
    pub fn session_id(&self, slot: u32) -> Option<&str> {
        self.slots
            .get(usize::try_from(slot).ok()?)
            .map(String::as_str)
    }

    /// The panel ID the dock knows a tab by.
    fn id(&self, key: DockKey) -> String {
        match key {
            DockKey::Home => "home".to_owned(),
            DockKey::Editor => "editor".to_owned(),
            DockKey::Session(slot) => format!("session:{}", self.session_id(slot).unwrap_or("?")),
            DockKey::Panel(tool) => panel_id(tool),
            DockKey::Terminal(n) => format!("terminal:{n}"),
        }
    }

    /// Every tab in the dock, pane by pane.
    #[must_use]
    pub fn keys(&self) -> Vec<DockKey> {
        let state = self.session.state();
        let state = state.borrow();
        let layout = &state.layout;
        let mut keys = Vec::new();
        let mut stack: Vec<_> = layout.root_child().into_iter().collect();
        while let Some(node) = stack.pop() {
            match layout.kind(node) {
                Some(NodeKind::Proportional(group)) => stack.extend(group.children.iter().rev()),
                Some(NodeKind::Pane(pane)) => {
                    keys.extend(pane.tabs.iter().filter_map(|&tab| match layout.kind(tab) {
                        Some(NodeKind::Panel(p)) => Some(p.content),
                        _ => None,
                    }));
                }
                _ => {}
            }
        }
        keys
    }

    /// The center's tabs, in order.
    #[must_use]
    pub fn center(&self) -> Vec<DockKey> {
        let ids = self.session.panels_in(CENTER);
        let keys = self.keys();
        ids.iter()
            .filter_map(|id| keys.iter().copied().find(|&key| &self.id(key) == id))
            .collect()
    }

    /// Whether a tab is in the dock.
    #[must_use]
    pub fn has(&self, key: DockKey) -> bool {
        self.session.panel_node(&self.id(key)).is_some()
    }

    /// Whether a tab is the one its pane shows.
    #[must_use]
    pub fn is_shown(&self, key: DockKey) -> bool {
        let id = self.id(key);
        self.session
            .pane_for_panel(&id)
            .and_then(|pane| self.session.active_panel_in_pane(pane))
            .is_some_and(|active| active == id)
    }

    /// How many tabs share a tab's pane.
    #[must_use]
    pub fn tab_count(&self, key: DockKey) -> usize {
        self.session.pane_tab_count(&self.id(key))
    }

    /// The tab shown in the focused pane.
    #[must_use]
    pub fn focused(&self) -> Option<DockKey> {
        let id = self.session.active_panel()?;
        self.keys().into_iter().find(|&key| self.id(key) == id)
    }

    /// Whether a tool was placed to stay (see [`Docking::kept`]).
    #[must_use]
    pub fn is_kept(&self, tool: Panel) -> bool {
        self.kept.contains(&tool)
    }

    /// Opens a document in the center, after `after` when that's there.
    fn open_document(&self, key: DockKey, after: Option<DockKey>) {
        let target = match after.filter(|&a| a != key && self.has(a)) {
            Some(after) => PaneTarget::Beside(self.id(after)),
            None => PaneTarget::Named(CENTER.to_owned()),
        };
        let _ = self.session.open_panel(target, def(key, self.id(key)));
    }

    /// Opens a tool at the full height of the right dock, in place of the
    /// tools opened there from the rail; tools placed by hand stay.
    fn open_tool(&mut self, tool: Panel) {
        let key = DockKey::Panel(tool);
        let in_right = self.session.panels_in(RIGHT);
        let opened = self
            .session
            .open_panel(PaneTarget::Named(RIGHT.to_owned()), def(key, self.id(key)))
            .is_ok()
            || self
                .session
                .open_panel_split(
                    RIGHT,
                    CENTER,
                    DockOperation::Right,
                    RIGHT_SHARE,
                    def(key, self.id(key)),
                )
                .is_ok();
        if !opened {
            return;
        }
        for id in in_right {
            if let Some(other) = tool_named(&id)
                && other != tool
                && !self.kept.contains(&other)
            {
                let _ = self.session.close_panel(&id);
            }
        }
    }

    /// Opens a terminal beside `beside` when that's in the dock, else in the
    /// bottom dock, making that below the center when it's gone.
    pub(super) fn open_terminal(&self, key: DockKey, beside: Option<DockKey>) {
        if let Some(other) = beside.filter(|&b| self.has(b))
            && self
                .session
                .open_panel(PaneTarget::Beside(self.id(other)), def(key, self.id(key)))
                .is_ok()
        {
            return;
        }
        if self
            .session
            .open_panel(PaneTarget::Named(BOTTOM.to_owned()), def(key, self.id(key)))
            .is_ok()
        {
            return;
        }
        let _ = self.session.open_panel_split(
            BOTTOM,
            CENTER,
            DockOperation::Bottom,
            BOTTOM_SHARE,
            def(key, self.id(key)),
        );
    }

    /// Closes a tab, if it's in the dock.
    pub(super) fn close(&mut self, key: DockKey) {
        let _ = self.session.close_panel(&self.id(key));
        if let DockKey::Panel(tool) = key {
            self.kept.remove(&tool);
        }
    }

    /// Makes a tab the one its pane shows, focusing that pane.
    pub(super) fn select(&self, key: DockKey) {
        let _ = self.session.select_panel(&self.id(key));
    }
}

/// The center pane holding `defs`.
fn center(defs: Vec<PanelDef<DockKey>>) -> LayoutTree<DockKey> {
    tabs(defs).named(CENTER).group(DOCUMENTS).anchored()
}

/// A share of `total` in thousandths.
fn thousandths(share: f32, total: f32) -> u32 {
    if total <= 0.0 {
        return 1000;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a share of a positive total is between 0 and 1000"
    )]
    let part = (share / total * 1000.0).round().clamp(0.0, 1000.0) as u32;
    part
}

/// Builds a saved part of the arrangement, leaving out tabs it can't show
/// and parts left empty, except the center. Sessions get slots in `slots`;
/// `centers` counts the panes of documents.
fn rebuild(
    saved: &DockNode,
    slots: &mut Vec<String>,
    centers: &mut usize,
) -> Option<LayoutTree<DockKey>> {
    match saved {
        DockNode::Split {
            across,
            shares,
            parts,
        } => {
            let mut kept = Vec::new();
            let mut weights = Vec::new();
            for (index, part) in parts.iter().enumerate() {
                if let Some(tree) = rebuild(part, slots, centers) {
                    kept.push(tree);
                    #[expect(clippy::cast_precision_loss, reason = "a share is at most 1000")]
                    weights.push(shares.get(index).copied().unwrap_or(1000).max(1) as f32);
                }
            }
            match kept.len() {
                0 => None,
                1 => kept.pop(),
                _ if *across => Some(horizontal(kept).weights(weights)),
                _ => Some(vertical(kept).weights(weights)),
            }
        }
        DockNode::Tabs {
            name,
            group,
            tabs: names,
            shown,
        } => {
            let mut defs: Vec<PanelDef<DockKey>> = names
                .iter()
                .filter_map(|name| Some(def(key_named(name, slots)?, name.clone())))
                .collect();
            let is_center = name.as_deref() == Some(CENTER);
            if group.as_deref() == Some(DOCUMENTS) || defs.iter().any(|d| d.content.is_document()) {
                // Documents belong in the center alone.
                if !is_center {
                    *centers += 2;
                    return None;
                }
                *centers += 1;
            }
            if is_center {
                if defs.is_empty() {
                    defs.push(def(DockKey::Home, "home".to_owned()));
                }
                let shown = shown.clone().filter(|s| defs.iter().any(|d| &d.id == s));
                let tree = center(defs);
                return Some(match shown {
                    Some(shown) => tree.active(shown),
                    None => tree,
                });
            }
            if defs.is_empty() {
                return None;
            }
            let shown = shown.clone().filter(|s| defs.iter().any(|d| &d.id == s));
            let mut tree = tabs(defs).group(TOOLS);
            if let Some(name) = name {
                tree = tree.named(name.clone());
            }
            if let Some(shown) = shown {
                tree = tree.active(shown);
            }
            Some(tree)
        }
    }
}

/// The tab a saved name stands for; files and unknown names are left out.
fn key_named(name: &str, slots: &mut Vec<String>) -> Option<DockKey> {
    if name == "home" {
        return Some(DockKey::Home);
    }
    if let Some(id) = name.strip_prefix("session:") {
        slots.push(id.to_owned());
        return Some(DockKey::Session(u32::try_from(slots.len() - 1).ok()?));
    }
    tool_named(name).map(DockKey::Panel)
}

/// The tool a panel ID names.
fn tool_named(id: &str) -> Option<Panel> {
    let title = id.strip_prefix("panel:")?;
    Panel::ALL
        .into_iter()
        .find(|p| p.title().eq_ignore_ascii_case(title))
}

/// The panel ID of a tool panel.
fn panel_id(tool: Panel) -> String {
    format!("panel:{}", tool.title().to_lowercase())
}

/// A tab's definition: its ID, a placeholder title (the view supplies the
/// real one) and its group.
fn def(key: DockKey, id: String) -> PanelDef<DockKey> {
    let group = if key.is_document() { DOCUMENTS } else { TOOLS };
    // The start screen goes when another document is current; closing it by
    // hand would leave nothing to start a session from.
    panel(id, String::new(), key)
        .group(group)
        .can_close(key != DockKey::Home)
}

impl App {
    /// The document shown as current: the file being edited, the active
    /// session, or the start screen.
    #[must_use]
    pub fn current_document(&mut self) -> DockKey {
        if self.fs.shown.is_some() {
            DockKey::Editor
        } else if let Some(id) = self.active.clone() {
            self.dock.session_key(&id)
        } else {
            DockKey::Home
        }
    }

    /// The session a dock key refers to.
    #[must_use]
    pub fn docked_session(&self, key: DockKey) -> Option<&str> {
        match key {
            DockKey::Session(slot) => self.dock.session_id(slot),
            _ => None,
        }
    }

    /// Whether a tool panel is on screen: the one its pane shows, and not
    /// covered by another panel taking the whole window.
    #[must_use]
    pub fn panel_shown(&self, tool: Panel) -> bool {
        match self.maximized {
            Some(big) => big == tool,
            None => self.dock.is_shown(DockKey::Panel(tool)),
        }
    }

    /// Whether a tool panel is open in the dock, shown or not.
    #[must_use]
    pub fn panel_open(&self, tool: Panel) -> bool {
        self.dock.has(DockKey::Panel(tool))
    }

    /// Makes the dock match what's open, and loads the session coming into view.
    pub(super) fn sync_dock(&mut self) -> Task<Message> {
        let current = self.current_document();
        let after = self.dock.previous.or(Some(current));
        // Sessions: one tab each, opened after the document that was current.
        let wanted: Vec<DockKey> = self
            .tabs
            .clone()
            .iter()
            .map(|id| self.dock.session_key(id))
            .collect();
        for &key in &wanted {
            if !self.dock.has(key) {
                self.dock.open_document(key, after);
            }
        }
        for key in self.dock.keys() {
            // Until the saved tabs are restored, their places are kept.
            if matches!(key, DockKey::Session(_))
                && !wanted.contains(&key)
                && self.restore.is_none()
            {
                self.dock.close(key);
            }
        }
        // The editor, while a file is open, and the start screen, while it's current.
        for (key, want) in [
            (DockKey::Editor, !self.fs.open.is_empty()),
            (DockKey::Home, current == DockKey::Home),
        ] {
            match (want, self.dock.has(key)) {
                (true, false) => self.dock.open_document(key, after),
                (false, true) => self.dock.close(key),
                _ => {}
            }
        }
        if !self.dock.is_shown(current) {
            self.dock.select(current);
        }
        self.dock.previous = Some(current);
        match self.docked_session(current).map(str::to_owned) {
            Some(id) if !self.timelines.contains_key(&id) => {
                self.timelines.insert(id.clone(), Timeline::new(&id));
                self.load(id)
            }
            _ => Task::none(),
        }
    }

    /// Makes a tab the current document, as when it's clicked.
    fn adopt(&mut self, key: DockKey) -> Task<Message> {
        match key {
            DockKey::Home => self.update(Message::NewSession(None)),
            DockKey::Session(_) => match self.docked_session(key).map(str::to_owned) {
                Some(id) if self.fs.shown.is_some() || self.active.as_deref() != Some(&id) => {
                    self.open(id)
                }
                _ => Task::none(),
            },
            DockKey::Editor => match self.fs.open.first().map(|f| f.key.clone()) {
                Some(file) if self.fs.shown.as_deref() != Some(&file) => {
                    self.update(Message::Files(FilesEvent::Show(file)))
                }
                _ => Task::none(),
            },
            DockKey::Panel(_) | DockKey::Terminal(_) => Task::none(),
        }
    }

    /// Handles the messages about the dock and its tool panels.
    pub(super) fn dock_action(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenPanel(tool) => {
                if !self.dock.is_shown(DockKey::Panel(tool)) {
                    self.toggle_panel(tool);
                }
            }
            Message::TogglePanel(tool) => self.toggle_panel(tool),
            Message::Dock(event) => return self.dock_event(&event),
            Message::CloseDocked(key) => return self.close_docked(key),
            Message::DockMove(how) => return self.dock_move(how),
            Message::Maximize(tool) => {
                if self.panel_open(tool) {
                    // The dock stays as it is underneath, so restoring
                    // brings back the same tabs.
                    self.maximized = Some(tool);
                    return self.on_maximize(tool);
                }
            }
            Message::Restore if self.maximized.is_some() => {
                self.maximized = None;
                return iced::widget::operation::focus(crate::ids::COMPOSER_TEXT);
            }
            _ => {}
        }
        Task::none()
    }

    /// Handles something the user did in the dock.
    fn dock_event(&mut self, event: &DockEvent<DockKey>) -> Task<Message> {
        match *event {
            DockEvent::TabSelected { panel, .. }
            | DockEvent::PaneFocused {
                panel: Some(panel), ..
            } => self.adopt(panel),
            DockEvent::DragStarted { panel } => {
                self.dock.dragging = Some(panel);
                Task::none()
            }
            DockEvent::DragEnded { .. } | DockEvent::DragCancelled => {
                match self.dock.dragging.take() {
                    // A tool put somewhere by hand stays there.
                    Some(DockKey::Panel(tool)) => {
                        self.dock.kept.insert(tool);
                    }
                    // Session tabs follow their new order in the center.
                    Some(_) => self.follow_center_order(),
                    None => {}
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Puts the session tabs in the order the center shows them.
    fn follow_center_order(&mut self) {
        let order: Vec<String> = self
            .dock
            .center()
            .into_iter()
            .filter_map(|key| self.docked_session(key).map(str::to_owned))
            .collect();
        self.tabs
            .sort_by_key(|id| order.iter().position(|o| o == id));
    }

    /// Closes a tab the user asked to close, as its kind closes.
    fn close_docked(&mut self, key: DockKey) -> Task<Message> {
        match key {
            DockKey::Session(_) => match self.docked_session(key).map(str::to_owned) {
                Some(id) => self.update(Message::CloseTab(id)),
                None => Task::none(),
            },
            DockKey::Editor => match self.fs.open.first().map(|f| f.key.clone()) {
                Some(file) => self.update(Message::Files(FilesEvent::Close(file))),
                None => Task::none(),
            },
            DockKey::Panel(tool) => {
                self.dock.close(DockKey::Panel(tool));
                if self.maximized == Some(tool) {
                    self.maximized = None;
                }
                Task::none()
            }
            DockKey::Terminal(id) => self.end_terminal(id),
            DockKey::Home => Task::none(),
        }
    }

    /// Opens a tool panel, shows it if it's behind another tab, or closes it
    /// if it's shown.
    fn toggle_panel(&mut self, tool: Panel) {
        let key = DockKey::Panel(tool);
        if self.maximized == Some(tool) {
            self.maximized = None;
        } else if self.dock.is_shown(key) {
            self.dock.close(key);
        } else if self.dock.has(key) {
            self.dock.select(key);
        } else {
            self.dock.open_tool(tool);
        }
    }

    /// Moves focus, or the current tool tab, around the dock.
    fn dock_move(&mut self, how: DockMove) -> Task<Message> {
        let moved = match how {
            DockMove::Focus(direction) => self.dock.session.focus_adjacent(direction),
            DockMove::Carry(direction) => self.dock.session.move_active_panel_adjacent(direction),
            DockMove::Split(direction) => self.dock.session.split_active_panel(direction),
        };
        match (how, self.dock.focused().filter(|_| moved)) {
            (_, None) => Task::none(),
            (DockMove::Focus(_), Some(key)) => self.adopt(key),
            (_, Some(key)) => {
                if let DockKey::Panel(tool) = key {
                    self.dock.kept.insert(tool);
                }
                Task::none()
            }
        }
    }

    /// Shows the next (or previous) tab in the focused pane.
    pub(super) fn cycle_dock(&mut self, back: bool) -> Task<Message> {
        let cycle = if back {
            PanelCycle::Prev
        } else {
            PanelCycle::Next
        };
        if self.dock.session.cycle_panel(cycle).is_err() {
            return Task::none();
        }
        match self.dock.focused() {
            Some(key) => self.adopt(key),
            None => Task::none(),
        }
    }
}
