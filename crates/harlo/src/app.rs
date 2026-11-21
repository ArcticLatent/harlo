//! The app's state, messages and update, and the window's top-level layout.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use harlo_core::composer::{self as compose, Attached, Choice, Draft, MenuItem, Outgoing, Trigger};
use harlo_core::layout::{Layout, Store, Tabs};
use harlo_core::server::{
    self, Attachment, Catalog, Client, Delivery, FileMatch, Location, Update,
};
use harlo_core::sessions::Project;
use harlo_core::theme::{self, Mode, Tokens};
use harlo_core::timeline::{Entry, entries};
use harlo_core::workspace::{EventKind, Fetch, Fetched, InboxItem, ModelRef, Session, Workspace};
use iced::animation::Animation;
use iced::keyboard::{self, Key, key::Named};
use iced::time::Instant;
use iced::widget::{Id, row, scrollable, space, stack, text_editor};
use iced::{Element, Fill, Subscription, Task, Theme};

use crate::assets::Assets;
use crate::fit::{self, Shown, Side};
use crate::motion::{self, Moment};
use crate::style::size;
use crate::timeline::{self, Activity, Timeline};
use crate::view;
use crate::view::composer::Feedback;
use crate::view::pickers::{self, AgentChoice, Folder, ModelChoice, Picker, Variant};
use crate::widget::{Edge, reveal};

/// How often session ages are refreshed.
const TICK: Duration = Duration::from_secs(30);

/// How many files the `@` menu asks the server for.
const FILE_MATCHES: usize = 20;

/// Height of a menu row with its gap, and how many rows show before it scrolls.
const MENU_ROW: f32 = 32.0;
const MENU_ROWS: f32 = 8.0;

/// The scroller inside the composer's menu.
const MENU_SCROLLER: &str = "composer-menu-list";

/// The open `/` or `@` menu above the composer.
#[derive(Debug, Clone)]
pub struct MenuState {
    /// The word that opened it.
    pub trigger: Trigger,
    /// What it offers.
    pub items: Vec<MenuItem>,
    /// The highlighted row.
    pub selected: usize,
    /// The list's scroller, moved to keep the highlight in view.
    pub scroller: Id,
}

/// Panels the right rail opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Context window use and session facts.
    Context,
    /// The session's todo list.
    Todos,
    /// Files the session changed.
    Changes,
    /// The project's files.
    Files,
    /// The project's notes.
    Notes,
}

impl Panel {
    /// Every panel, in rail order.
    pub const ALL: [Self; 5] = [
        Self::Context,
        Self::Todos,
        Self::Changes,
        Self::Files,
        Self::Notes,
    ];

    /// The widget ID of the panel's rail button.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Context => "panel-context",
            Self::Todos => "panel-todos",
            Self::Changes => "panel-changes",
            Self::Files => "panel-files",
            Self::Notes => "panel-notes",
        }
    }

    /// The panel's title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Context => "Context",
            Self::Todos => "Todos",
            Self::Changes => "Changes",
            Self::Files => "Files",
            Self::Notes => "Notes",
        }
    }
}

/// How the connection to the current server stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Trying to reach the server.
    Connecting {
        /// Which attempt this is, from 1.
        attempt: u32,
    },
    /// Connected; the sidebar is live.
    Connected {
        /// The server's OpenCode version.
        version: String,
    },
    /// The connection failed or broke and will be retried.
    Lost {
        /// Why, in words for the user.
        reason: String,
        /// How long until the next attempt, when it was reported.
        retry_in: Duration,
    },
}

/// Something the user did, or something the server reported.
#[derive(Debug, Clone)]
pub enum Message {
    /// Show or hide the sidebar.
    ToggleSidebar,
    /// The session filter changed.
    Filter(String),
    /// Show all of a folder's sessions, or fewer again, by folder.
    ToggleMore(String),
    /// Open a right panel, or close it when it's already open.
    OpenPanel(Panel),
    /// Close the right panel.
    ClosePanel,
    /// The composer was edited.
    Compose(text_editor::Action),
    /// Open or close the location picker.
    ToggleLocations,
    /// Follow another server.
    PickLocation(Location),
    /// Open a session in a tab, or switch to its tab.
    OpenSession(String),
    /// Close a session's tab.
    CloseTab(String),
    /// Close the active tab, if a session is shown.
    CloseActiveTab,
    /// Switch to the next tab, or the previous one.
    CycleTab {
        /// Whether to go backwards.
        back: bool,
    },
    /// Go to the start screen to begin a new session, optionally in a folder.
    NewSession(Option<String>),
    /// Choose the folder a new session starts in; `None` is the home folder.
    PickFolder(Option<String>),
    /// Send the composer's text; a working session gets it queued.
    Send,
    /// Send the composer's text to steer the working session.
    Steer,
    /// Stop the open session's run.
    Stop,
    /// Have a queued prompt steer the session, by item ID.
    SteerQueued(String),
    /// Cancel a queued prompt, by item ID.
    CancelQueued(String),
    /// An action on the server finished, or why it failed.
    Acted(Result<(), String>),
    /// Move the menu's highlight by this many rows.
    MenuMove(i32),
    /// Pick a menu row, or the highlighted one.
    MenuPick(Option<usize>),
    /// Close the menu until its word changes.
    MenuClose,
    /// Files matching an `@` word: which search it was, and its result.
    Found(u64, Result<Vec<FileMatch>, String>),
    /// What a folder offers, loaded.
    Catalog(String, Result<Box<Catalog>, String>),
    /// Choose the agent, by ID.
    PickAgent(String),
    /// Choose the model.
    PickModel(ModelChoice),
    /// Open a picker's menu, or close it if it's open.
    TogglePicker(Picker),
    /// Close the open picker's menu.
    ClosePicker,
    /// The text in the open menu's filter changed.
    PickerFilter(String),
    /// Move the open menu's highlight by this many rows.
    PickerMove(i32),
    /// Choose the open menu's highlighted row.
    PickerSubmit,
    /// The open menu's list scrolled to this offset from its top.
    PickerScrolled(f32),
    /// Ask for a folder to start a new session in.
    AddProject,
    /// The folder chosen to start a new session in, if any.
    ProjectChosen(Option<PathBuf>),
    /// Choose the reasoning level.
    PickVariant(Variant),
    /// Open the file dialog to attach files.
    Attach,
    /// Files chosen in the dialog.
    Attached(Vec<PathBuf>),
    /// Remove an attachment, by position.
    Detach(usize),
    /// Attach the clipboard's image, if it holds one.
    PasteImage,
    /// The clipboard's image as PNG, if there was one.
    Pasted(Option<Vec<u8>>),
    /// A send finished: the session it went to, or why it failed.
    Sent(Result<Box<Session>, String>),
    /// A session's history loaded.
    History(String, Result<Vec<Entry>, String>),
    /// A session's timeline reported something.
    Timeline(String, timeline::Event),
    /// The server connection reported something.
    Server(Update),
    /// A follow-up load finished.
    Fetched(Result<Fetched, String>),
    /// The clock moved on; ages are measured from this time.
    Tick(SystemTime),
    /// The window was resized, to this size.
    Resized(iced::Size),
    /// A frame of motion is due, at this time.
    Frame(Instant),
}

/// How long reading a pasted image takes before the composer says so.
///
/// Most clipboards answer at once; a note that flashes up for a frame
/// would only be noise.
const PASTE_NOTICE: Duration = Duration::from_millis(150);

/// A send on its way to OpenCode.
#[derive(Debug)]
struct Sending {
    /// The session it goes to, or `None` for a new one.
    session: Option<String>,
    /// The prompt's message ID, when it's a prompt.
    id: Option<String>,
    /// The composer's text, to put back if sending fails.
    text: String,
    /// What came with it, to put back if sending fails.
    draft: Draft,
    /// The queue item shown for it, when the session is working.
    queued: Option<InboxItem>,
}

/// Everything the window shows.
#[derive(Debug)]
pub struct App {
    /// Colours for every widget.
    pub tokens: Tokens,
    /// Bundled images.
    pub assets: Assets,
    theme: Theme,
    /// Servers the location picker offers.
    pub locations: Vec<Location>,
    /// The server being followed.
    pub location: Location,
    /// How the connection stands.
    pub status: Status,
    /// Whether the location picker is open.
    pub picking_location: bool,
    client: Option<server::Client>,
    workspace: Workspace,
    /// The sidebar's projects, rebuilt whenever the workspace changes.
    pub projects: Vec<Project>,
    /// The time ages are measured from.
    pub now: SystemTime,
    /// Text in the session filter.
    pub filter: String,
    /// Folders whose full session list is open.
    pub expanded: HashSet<String>,
    /// Whether the sidebar is shown.
    pub sidebar: bool,
    /// The open right panel.
    pub panel: Option<Panel>,
    /// The window's width, which decides which panels fit.
    width: f32,
    /// A panel the user opened while it didn't fit, kept on screen.
    keep: Option<Side>,
    /// Open sessions, in tab order.
    pub tabs: Vec<String>,
    /// The session shown, if any; `None` shows the start screen.
    pub active: Option<String>,
    timelines: HashMap<String, Timeline>,
    /// The folder a new session starts in; `None` is the home folder.
    pub new_folder: Option<String>,
    /// The composer's text.
    pub composer: text_editor::Content,
    /// Why the last send failed, shown under the composer.
    pub send_error: Option<String>,
    /// The send on its way to OpenCode, if any.
    sending: Option<Sending>,
    /// When reading an image from the clipboard started, while it's going on.
    pasting: Option<Instant>,
    /// Attachment chips fading in, by name.
    chips: HashMap<String, Animation<bool>>,
    /// The send error fading in, and the error it's for.
    error_shown: (Animation<bool>, Option<String>),
    /// The time motion is drawn at: the last frame, or the last message.
    clock: Instant,
    /// One-off motion runs every frame until this time.
    settle: Instant,
    /// When the pulse of work in progress started; indicators keep time with it.
    since: Instant,
    /// The sidebar sliding open or closed.
    sidebar_slide: Animation<bool>,
    /// The right panel sliding open or closed, and the last one shown.
    panel_slide: (Animation<bool>, Option<Panel>),
    /// What the composer brings along with its text.
    pub draft: Draft,
    /// The open `/` or `@` menu.
    pub menu: Option<MenuState>,
    /// The menu word the menu was closed on: its line and start.
    dismissed: Option<(usize, usize)>,
    /// Files found for the last `@` word searched.
    files: Vec<FileMatch>,
    /// The `@` word last searched for.
    searched: Option<String>,
    /// Counts file searches, so a late answer to an old one is ignored.
    searches: u64,
    /// What each folder offers, once loaded.
    catalogs: HashMap<String, Catalog>,
    loading_catalogs: HashSet<String>,
    /// The agent for the next new session, when chosen.
    new_agent: Option<String>,
    /// The model for the next new session, when chosen.
    new_model: Option<ModelRef>,
    /// The model picker's options, for the folder in `models_for`.
    pub models: Vec<ModelChoice>,
    models_for: Option<String>,
    /// The picker whose menu is open.
    pub picker: Option<Picker>,
    /// The open menu's filter text.
    pub picker_filter: String,
    /// The open menu's highlighted row.
    pub picker_index: usize,
    /// How far the open menu's list is scrolled from its top.
    picker_scroll: f32,
    /// Where the layout is saved; `None` keeps it in memory only.
    store: Option<Store>,
    /// The layout as last saved.
    saved: Layout,
    /// Tabs saved for the other locations.
    other_tabs: BTreeMap<String, Tabs>,
    /// Saved tabs to reopen once the current location connects.
    restore: Option<Tabs>,
}

impl App {
    /// Creates the window's state, following `location`.
    ///
    /// `locations` are what the location picker offers; `order` is the saved
    /// sidebar order of project folders.
    ///
    /// # Panics
    ///
    /// Panics if the bundled default theme doesn't parse. `harlo-core`'s
    /// tests check that it does, so this would be a build mistake.
    #[must_use]
    ///
    /// `layout` arranges the window as it was last saved; its tabs for
    /// `location` reopen once the server connects.
    pub fn new(location: Location, locations: Vec<Location>, layout: Layout) -> Self {
        let theme = theme::parse(harlo_core::DEFAULT_THEME).expect("the bundled theme is valid");
        let tokens = theme.tokens(Mode::Dark);
        let clock = Instant::now();
        let panel = layout
            .panel
            .as_deref()
            .and_then(|title| Panel::ALL.into_iter().find(|p| p.title() == title));
        let mut app = Self {
            theme: crate::style::iced_theme(&theme.name, &tokens),
            tokens,
            assets: Assets::load(),
            locations,
            location,
            status: Status::Connecting { attempt: 1 },
            picking_location: false,
            client: None,
            workspace: Workspace::new(layout.order.clone()),
            projects: Vec::new(),
            now: SystemTime::now(),
            filter: String::new(),
            expanded: layout.expanded.iter().cloned().collect(),
            sidebar: layout.sidebar,
            panel,
            width: fit::WINDOW.0,
            keep: None,
            tabs: Vec::new(),
            active: None,
            timelines: HashMap::new(),
            new_folder: None,
            composer: text_editor::Content::new(),
            send_error: None,
            sending: None,
            pasting: None,
            chips: HashMap::new(),
            error_shown: (Animation::new(true), None),
            clock,
            settle: clock,
            since: clock,
            sidebar_slide: motion::slide(layout.sidebar),
            panel_slide: (motion::slide(panel.is_some()), panel),
            draft: Draft::default(),
            menu: None,
            dismissed: None,
            files: Vec::new(),
            searched: None,
            searches: 0,
            catalogs: HashMap::new(),
            loading_catalogs: HashSet::new(),
            new_agent: None,
            new_model: None,
            models: Vec::new(),
            models_for: None,
            picker: None,
            picker_filter: String::new(),
            picker_index: 0,
            picker_scroll: 0.0,
            store: None,
            other_tabs: layout.tabs.clone(),
            restore: None,
            saved: layout,
        };
        app.restore = app.other_tabs.remove(&app.location.label());
        app.refresh();
        app
    }

    /// Saves the layout to `store` after each change.
    #[must_use]
    pub fn saving_to(mut self, store: Store) -> Self {
        self.store = Some(store);
        self
    }

    /// Returns the window's layout as it is now.
    #[must_use]
    pub fn layout(&self) -> Layout {
        let mut tabs = self.other_tabs.clone();
        let current = self.restore.clone().unwrap_or_else(|| Tabs {
            open: self.tabs.clone(),
            active: self.active.clone(),
        });
        if !current.open.is_empty() {
            tabs.insert(self.location.label(), current);
        }
        let mut expanded: Vec<String> = self.expanded.iter().cloned().collect();
        expanded.sort();
        Layout {
            order: self.workspace.order().to_vec(),
            sidebar: self.sidebar,
            panel: self.panel.map(|p| p.title().to_owned()),
            expanded,
            tabs,
        }
    }

    /// Applies a message, then saves the layout if it changed.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // Motion is drawn at the latest time seen, so it never runs backwards.
        let at = match message {
            Message::Frame(at) => at,
            _ => Instant::now(),
        };
        self.clock = self.clock.max(at);
        if matches!(
            message,
            Message::PickFolder(_)
                | Message::PickAgent(_)
                | Message::PickModel(_)
                | Message::PickVariant(_)
        ) {
            self.picker = None;
        }
        let task = self.apply(message);
        self.sync_motion();
        let task = Task::batch([task, self.follow_folder()]);
        let layout = self.layout();
        if layout == self.saved {
            return task;
        }
        self.saved = layout.clone();
        match self.store.clone() {
            // A failed save leaves the previous layout; the next change tries again.
            Some(store) => Task::batch([task, Task::future(store.save_later(layout)).discard()]),
            None => task,
        }
    }

    fn apply(&mut self, message: Message) -> Task<Message> {
        match message {
            message @ (Message::ToggleSidebar
            | Message::OpenPanel(_)
            | Message::ClosePanel
            | Message::Resized(_)) => self.panels(&message),
            Message::Filter(filter) => self.filter = filter,
            Message::ToggleMore(folder) => {
                if !self.expanded.remove(&folder) {
                    self.expanded.insert(folder);
                }
            }
            message @ (Message::Compose(_)
            | Message::MenuMove(_)
            | Message::MenuPick(_)
            | Message::MenuClose
            | Message::Found(..)
            | Message::Attach
            | Message::Attached(_)
            | Message::Detach(_)
            | Message::PasteImage
            | Message::Pasted(_)) => return self.compose(message),
            message @ (Message::Stop
            | Message::SteerQueued(_)
            | Message::CancelQueued(_)
            | Message::Acted(_)
            | Message::Catalog(..)
            | Message::PickAgent(_)
            | Message::PickModel(_)
            | Message::PickVariant(_)
            | Message::Sent(_)
            | Message::History(..)) => return self.session_action(message),
            message @ (Message::TogglePicker(_)
            | Message::ClosePicker
            | Message::PickerFilter(_)
            | Message::PickerMove(_)
            | Message::PickerSubmit
            | Message::PickerScrolled(_)
            | Message::AddProject
            | Message::ProjectChosen(_)) => return self.picker_action(message),
            Message::ToggleLocations => self.picking_location = !self.picking_location,
            Message::PickLocation(location) => self.pick_location(location),
            Message::OpenSession(id) => return self.open(id),
            Message::CloseTab(id) => self.close(&id),
            Message::CloseActiveTab => {
                if let Some(id) = self.active.clone() {
                    self.close(&id);
                }
            }
            Message::CycleTab { back } => {
                if let Some(id) = self.neighbour(back) {
                    return self.open(id);
                }
            }
            Message::NewSession(folder) => {
                self.active = None;
                if folder.is_some() {
                    self.new_folder = folder;
                }
                self.send_error = None;
            }
            Message::PickFolder(folder) => self.new_folder = folder,
            Message::Send => return self.send(None),
            Message::Steer => return self.send(Some(Delivery::Steer)),
            Message::Timeline(id, event) => {
                if let Some(timeline) = self.timelines.get_mut(&id) {
                    timeline.update(&event);
                    return timeline.scroll();
                }
            }
            Message::Server(update) => return self.server(update),
            Message::Fetched(Ok(result)) => {
                let fetches = self.workspace.fetched(result);
                self.refresh();
                return self.fetch(fetches);
            }
            // `update` has moved the clock on, so the next view draws the frame.
            // A failed fetch leaves the row as it was until the next event or
            // resync corrects it.
            Message::Frame(_) | Message::Fetched(Err(_)) => {}
            Message::Tick(now) => {
                self.now = now;
                self.refresh();
            }
        }
        Task::none()
    }

    /// Applies a message from the composer: typing, its menu and attachments.
    fn compose(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Compose(action) => {
                self.composer.perform(action);
                return self.refresh_menu();
            }
            Message::MenuMove(by) => return self.move_menu(by),
            Message::MenuPick(index) => return self.pick(index),
            Message::MenuClose => {
                if let Some(menu) = self.menu.take() {
                    self.dismissed = Some((menu.trigger.line, menu.trigger.start));
                }
            }
            Message::Found(search, Ok(files)) => {
                if search == self.searches {
                    self.files = files;
                    return self.refresh_menu();
                }
            }
            Message::Found(_, Err(_)) => {
                // The menu keeps what it had; the next keystroke searches again.
                self.searched = None;
            }
            Message::Attach => return Task::perform(pick_files(), Message::Attached),
            Message::Attached(paths) => {
                for path in paths {
                    let (Some(path), Some(name)) =
                        (path.to_str(), path.file_name().and_then(|n| n.to_str()))
                    else {
                        continue;
                    };
                    let mut attachment = Attachment::file(path);
                    attachment.name = Some(name.to_owned());
                    self.draft.attached.push(Attached {
                        name: name.to_owned(),
                        attachment,
                    });
                }
            }
            Message::Detach(index) => {
                if index < self.draft.attached.len() {
                    self.draft.attached.remove(index);
                }
            }
            Message::PasteImage => {
                self.pasting = Some(self.clock);
                return Task::perform(crate::clipboard::image(), Message::Pasted);
            }
            Message::Pasted(png) => {
                self.pasting = None;
                if let Some(png) = png {
                    self.attach_image(&png);
                }
            }
            // A clipboard without an image pastes only its text; other
            // messages are routed elsewhere by `apply`.
            _ => {}
        }
        Task::none()
    }

    /// Applies a message about sessions: sending, loading, acting on one, choosing its agent and model.
    fn session_action(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Sent(Ok(session)) => {
                // A new session's first prompt was left in the composer.
                if let Some(Sending { session: None, .. }) = self.sending.take() {
                    self.composer = text_editor::Content::new();
                    self.draft = Draft::default();
                    self.menu = None;
                    self.dismissed = None;
                }
                let id = session.id.clone();
                let fetches = self.workspace.upsert(*session);
                self.refresh();
                return Task::batch([self.open(id.clone()), self.load(id), self.fetch(fetches)]);
            }
            Message::Sent(Err(reason)) => {
                self.unsend();
                self.send_error = Some(reason);
            }
            Message::History(id, result) => return self.loaded(&id, result),
            Message::Stop => {
                return self.act(|client, id| async move { client.interrupt(&id).await });
            }
            Message::SteerQueued(item) => {
                return self.act(|client, id| async move {
                    client.set_delivery(&id, &item, Delivery::Steer).await
                });
            }
            Message::CancelQueued(item) => {
                return self.act(|client, id| async move { client.cancel_inbox(&id, &item).await });
            }
            Message::Acted(Err(reason)) => self.send_error = Some(reason),
            Message::Catalog(folder, result) => {
                self.loading_catalogs.remove(&folder);
                // A folder that fails to load offers nothing until the catalog
                // changes or the server reconnects, rather than retrying at once.
                self.catalogs
                    .insert(folder.clone(), result.map(|c| *c).unwrap_or_default());
                if self.models_for.as_deref() == Some(folder.as_str()) {
                    self.rebuild_models();
                }
            }
            Message::PickAgent(agent) => {
                if self.active.is_none() {
                    self.new_agent = Some(agent);
                } else {
                    return self
                        .act(|client, id| async move { client.set_agent(&id, &agent).await });
                }
            }
            Message::PickModel(choice) => {
                return self.choose_model(ModelRef {
                    provider_id: choice.provider_id,
                    id: choice.id,
                    variant: None,
                });
            }
            Message::PickVariant(Variant(variant)) => {
                if let Some(model) = self.current_model() {
                    return self.choose_model(ModelRef { variant, ..model });
                }
            }
            // Other messages are routed elsewhere by `apply`.
            _ => {}
        }
        Task::none()
    }

    fn server(&mut self, update: Update) -> Task<Message> {
        match update {
            Update::Connecting { attempt } => {
                self.status = Status::Connecting { attempt };
            }
            Update::Connected(connected) => {
                self.status = Status::Connected {
                    version: connected.version.clone(),
                };
                self.client = Some(connected.client.clone());
                self.workspace.resync(connected.snapshot.clone());
                self.forget_catalogs();
                // Tabs saved from last time reopen, then load below.
                if let Some(saved) = self.restore.take() {
                    for id in saved.open {
                        if self.workspace.session(&id).is_some() && !self.tabs.contains(&id) {
                            self.timelines.insert(id.clone(), Timeline::new(&id));
                            self.tabs.push(id);
                        }
                    }
                    self.active = saved
                        .active
                        .filter(|id| self.tabs.contains(id))
                        .or_else(|| self.tabs.last().cloned());
                }
                // Tabs of sessions deleted while away close; the rest reload.
                let gone: Vec<String> = self
                    .tabs
                    .iter()
                    .filter(|id| self.workspace.session(id).is_none())
                    .cloned()
                    .collect();
                for id in &gone {
                    self.close(id);
                }
                self.refresh();
                let reloads: Vec<Task<Message>> = self
                    .tabs
                    .clone()
                    .into_iter()
                    .map(|id| self.load(id))
                    .collect();
                return Task::batch(reloads);
            }
            Update::Event(event) => {
                let fetches = self.workspace.apply(&event.kind, event.time);
                if let EventKind::SessionDeleted { session_id }
                | EventKind::SessionArchived { session_id } = &event.kind
                {
                    self.close(session_id);
                }
                self.refresh();
                if matches!(event.kind, EventKind::CatalogChanged) {
                    // Reloaded for the current folder once this update ends.
                    self.catalogs.clear();
                }
                let mut reload = Task::none();
                let mut scroll = Task::none();
                if let Some(id) = event.kind.session_id() {
                    let busy = self.workspace.is_busy(id);
                    if let Some(timeline) = self.timelines.get_mut(id) {
                        let stale = timeline.apply(&event.kind, event.time, self.clock);
                        timeline.set_busy(busy);
                        scroll = timeline.scroll();
                        if stale {
                            reload = self.load_history(id.to_owned());
                        }
                    }
                }
                return Task::batch([self.fetch(fetches), reload, scroll]);
            }
            Update::Lost { reason, retry_in } => {
                self.status = Status::Lost { reason, retry_in };
                self.client = None;
            }
        }
        Task::none()
    }

    /// Starts the loads the workspace asked for.
    fn fetch(&self, fetches: Vec<Fetch>) -> Task<Message> {
        let Some(client) = &self.client else {
            return Task::none();
        };
        Task::batch(fetches.into_iter().map(|what| {
            let client = client.clone();
            Task::perform(
                async move {
                    server::fetch(&client, what)
                        .await
                        .map_err(|e| e.to_string())
                },
                Message::Fetched,
            )
        }))
    }

    /// Rebuilds the sidebar from the workspace.
    fn refresh(&mut self) {
        self.projects = self.workspace.sidebar(self.now);
    }

    /// Shows a session, opening a tab after the current one if it has none.
    fn open(&mut self, id: String) -> Task<Message> {
        self.send_error = None;
        if !self.tabs.contains(&id) {
            let at = self
                .active
                .as_ref()
                .and_then(|a| self.tabs.iter().position(|t| t == a))
                .map_or(self.tabs.len(), |i| i + 1);
            self.tabs.insert(at, id.clone());
        }
        self.active = Some(id.clone());
        if self.timelines.contains_key(&id) {
            return Task::none();
        }
        self.timelines.insert(id.clone(), Timeline::new(&id));
        self.load(id)
    }

    /// Closes a session's tab; the neighbouring tab, or the start screen, takes its place.
    fn close(&mut self, id: &str) {
        let Some(index) = self.tabs.iter().position(|t| t == id) else {
            return;
        };
        self.tabs.remove(index);
        self.timelines.remove(id);
        if self.active.as_deref() == Some(id) {
            let next = index.min(self.tabs.len().saturating_sub(1));
            self.active = self.tabs.get(next).cloned();
        }
    }

    /// The tab after (or before) the active one, wrapping around.
    fn neighbour(&self, back: bool) -> Option<String> {
        let count = self.tabs.len();
        let current = self
            .active
            .as_ref()
            .and_then(|a| self.tabs.iter().position(|t| t == a))?;
        let next = if back {
            (current + count - 1) % count
        } else {
            (current + 1) % count
        };
        self.tabs.get(next).cloned()
    }

    /// Loads a session's history into its timeline, its inbox, and what it's
    /// waiting to have approved.
    fn load(&self, id: String) -> Task<Message> {
        Task::batch([
            self.fetch(vec![
                Fetch::Inbox(id.clone()),
                Fetch::Permissions(id.clone()),
            ]),
            self.load_history(id),
        ])
    }

    /// Loads a session's history into its timeline.
    fn load_history(&self, id: String) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                let result = client
                    .messages(&id)
                    .await
                    .map(|messages| entries(&messages))
                    .map_err(|e| e.to_string());
                (id, result)
            },
            |(id, result)| Message::History(id, result),
        )
    }

    /// Opens, closes and fits the side panels.
    ///
    /// Opening a panel that doesn't fit keeps it on screen, and the other
    /// panel makes way; see [`fit`].
    fn panels(&mut self, message: &Message) {
        match *message {
            Message::ToggleSidebar => {
                if self.shown().sidebar {
                    self.sidebar = false;
                    self.forget_keep(Side::Sidebar);
                } else {
                    self.sidebar = true;
                    self.keep_if_hidden(Side::Sidebar);
                }
            }
            Message::OpenPanel(panel) => {
                if self.panel == Some(panel) && self.shown().panel {
                    self.panel = None;
                    self.forget_keep(Side::Panel);
                } else {
                    self.panel = Some(panel);
                    self.keep_if_hidden(Side::Panel);
                }
            }
            Message::ClosePanel => {
                self.panel = None;
                self.forget_keep(Side::Panel);
            }
            Message::Resized(size) => {
                self.width = size.width;
                // Once the kept panel fits by itself, it no longer needs keeping.
                if let Some(side) = self.keep
                    && fit::shown(self.width, self.sidebar, self.panel.is_some(), None).has(side)
                {
                    self.keep = None;
                }
            }
            _ => {}
        }
    }

    fn keep_if_hidden(&mut self, side: Side) {
        if !self.shown().has(side) {
            self.keep = Some(side);
        }
    }

    fn forget_keep(&mut self, side: Side) {
        if self.keep == Some(side) {
            self.keep = None;
        }
    }

    /// Returns which side panels are on screen at the window's width.
    #[must_use]
    pub fn shown(&self) -> Shown {
        fit::shown(self.width, self.sidebar, self.panel.is_some(), self.keep)
    }

    /// Returns the width left for the conversation column.
    #[must_use]
    pub fn main_width(&self) -> f32 {
        self.shown().main_width(self.width)
    }

    /// Starts motion for what changed: panels sliding, chips and errors fading in.
    fn sync_motion(&mut self) {
        let now = self.clock;
        let shown = self.shown();
        if self.sidebar_slide.value() != shown.sidebar {
            self.sidebar_slide.go_mut(shown.sidebar, now);
            self.settle_after(motion::SLIDE);
        }
        let panel = self.panel.filter(|_| shown.panel);
        if panel.is_some() {
            self.panel_slide.1 = panel;
        }
        if self.panel_slide.0.value() != panel.is_some() {
            self.panel_slide.0.go_mut(panel.is_some(), now);
            self.settle_after(motion::SLIDE);
        }
        let names: Vec<String> = self.draft.attached.iter().map(|a| a.name.clone()).collect();
        self.chips.retain(|name, _| names.contains(name));
        for name in names {
            if let std::collections::hash_map::Entry::Vacant(chip) = self.chips.entry(name) {
                chip.insert(motion::appear(now));
                self.settle = self.settle.max(now + motion::APPEAR);
            }
        }
        if self.error_shown.1 != self.send_error {
            self.error_shown = (motion::appear(now), self.send_error.clone());
            self.settle_after(motion::APPEAR);
        }
        if let Some(settles) = self
            .active
            .as_deref()
            .and_then(|id| self.timelines.get(id)?.settles())
        {
            self.settle = self.settle.max(settles);
        }
    }

    fn settle_after(&mut self, duration: std::time::Duration) {
        self.settle = self.settle.max(self.clock + duration);
    }

    /// Every frame while one-off motion runs, and none once it has settled.
    ///
    /// Breathing indicators redraw themselves and need no frames from here.
    fn motion(&self) -> Subscription<Message> {
        if self.clock < self.settle {
            iced::window::frames().map(Message::Frame)
        } else {
            Subscription::none()
        }
    }

    /// Returns the moment views draw motion at.
    #[must_use]
    pub fn moment(&self) -> Moment {
        Moment {
            now: self.clock,
            since: self.since,
        }
    }

    /// Switches to another server, keeping the tabs of the one left for later.
    fn pick_location(&mut self, location: Location) {
        self.picking_location = false;
        if location != self.location {
            // The subscription keyed by the old location ends, which detaches from it.
            let left = self.restore.take().unwrap_or_else(|| Tabs {
                open: std::mem::take(&mut self.tabs),
                active: self.active.take(),
            });
            if !left.open.is_empty() {
                self.other_tabs.insert(self.location.label(), left);
            }
            self.restore = self.other_tabs.remove(&location.label());
            self.location = location;
            self.status = Status::Connecting { attempt: 1 };
            self.client = None;
            self.active = None;
            self.tabs.clear();
            self.timelines.clear();
            self.forget_catalogs();
            self.workspace = Workspace::new(self.workspace.order().to_vec());
            self.refresh();
        }
    }

    /// Shows a send on its way, and frees the composer for the next one.
    ///
    /// A prompt to an idle session shows at the end of its timeline, which
    /// scrolls there; one to a working session shows in its queue. Either
    /// way the composer empties at once, and gets the text back if sending
    /// fails. A new session's first prompt stays in the composer, which
    /// says it's starting.
    fn show_sending(
        &mut self,
        target: Option<&str>,
        text: &str,
        outgoing: &Outgoing,
        delivery: Delivery,
    ) -> Task<Message> {
        let prompt = match outgoing {
            Outgoing::Prompt(prompt) => {
                Some((prompt.id.clone(), prompt.text.clone(), &prompt.skills))
            }
            _ => None,
        };
        let skills: Vec<String> = prompt
            .as_ref()
            .map(|(_, _, ids)| ids.iter().map(|id| self.skill_name(id)).collect())
            .unwrap_or_default();
        let attachments: Vec<String> = self.draft.attached.iter().map(|a| a.name.clone()).collect();
        let mut queued = None;
        let mut scroll = Task::none();
        if let (Some(session), Some((key, said, _))) = (target, &prompt) {
            if self.workspace.is_busy(session) {
                queued = Some(InboxItem {
                    id: key.clone(),
                    kind: "user".to_owned(),
                    text: Some(said.clone()),
                    attachments: attachments.clone(),
                    skills: skills.clone(),
                    delivery,
                });
            } else if let Some(timeline) = self.timelines.get_mut(session) {
                let entry = Entry::Prompt {
                    key: key.clone(),
                    text: said.clone(),
                    attachments,
                    skills,
                };
                timeline.pend(entry, self.clock);
                timeline.to_end();
                scroll = timeline.scroll();
            }
        }
        let draft = if target.is_some() {
            self.composer = text_editor::Content::new();
            std::mem::take(&mut self.draft)
        } else {
            self.draft.clone()
        };
        self.sending = Some(Sending {
            session: target.map(str::to_owned),
            id: prompt.map(|(id, _, _)| id),
            text: text.to_owned(),
            draft,
            queued,
        });
        scroll
    }

    /// Takes back a send that failed: its row goes, and its text returns to
    /// the composer unless something new was written meanwhile.
    fn unsend(&mut self) {
        let Some(sent) = self.sending.take() else {
            return;
        };
        let Some(session) = sent.session else {
            return;
        };
        if let Some(key) = &sent.id
            && let Some(timeline) = self.timelines.get_mut(&session)
        {
            timeline.unpend(key);
        }
        if self.composer.text().trim().is_empty() && self.draft.is_empty() {
            self.composer = text_editor::Content::with_text(&sent.text);
            self.draft = sent.draft;
        }
    }

    /// A skill's name, from its ID.
    fn skill_name(&self, id: &str) -> String {
        self.catalog()
            .and_then(|c| c.skills.iter().find(|s| s.id == id))
            .map_or_else(|| id.to_owned(), |s| s.name.clone())
    }

    /// Returns the prompt on its way to a working session's queue, until the
    /// queue has it.
    #[must_use]
    pub fn queued_sending(&self, id: &str) -> Option<&InboxItem> {
        let sending = self.sending.as_ref()?;
        let item = sending.queued.as_ref()?;
        let here = sending.session.as_deref() == Some(id);
        (here && !self.inbox(id).iter().any(|i| i.id == item.id)).then_some(item)
    }

    /// Returns what a session is doing, while it works or a prompt is on
    /// its way to it; `None` once it's idle.
    #[must_use]
    pub fn activity(&self, id: &str) -> Option<Activity> {
        if let Some(asked) = self.workspace.permissions(id).first() {
            return Some(Activity::Waiting {
                action: asked.action.clone(),
                detail: asked.resources.join(" "),
            });
        }
        if self.workspace.is_busy(id) {
            return Some(
                self.timelines
                    .get(id)
                    .map_or(Activity::Thinking, Timeline::activity),
            );
        }
        let sending = self.sending.as_ref()?;
        (sending.session.as_deref() == Some(id)).then_some(Activity::Sending)
    }

    /// Returns whether a new session's first prompt is on its way.
    #[must_use]
    pub fn starting(&self) -> bool {
        self.sending.as_ref().is_some_and(|s| s.session.is_none())
    }

    /// Returns whether a send is on its way.
    #[must_use]
    pub fn is_sending(&self) -> bool {
        self.sending.is_some()
    }

    /// Returns the composer's feedback: sending, pasting, chips and errors fading in.
    ///
    /// A paste being read shows only once it takes a moment.
    #[must_use]
    pub fn feedback(&self) -> Feedback<'_> {
        Feedback {
            moment: self.moment(),
            sending: self.sending.is_some(),
            pasting: self
                .pasting
                .is_some_and(|since| self.clock.saturating_duration_since(since) >= PASTE_NOTICE),
            error_shown: motion::shown(&self.error_shown.0, self.clock),
            chips: &self.chips,
        }
    }

    /// Puts a loaded history, or the reason it failed, into its session's timeline.
    fn loaded(&mut self, id: &str, result: Result<Vec<Entry>, String>) -> Task<Message> {
        let busy = self.workspace.is_busy(id);
        let Some(timeline) = self.timelines.get_mut(id) else {
            return Task::none();
        };
        timeline.set_busy(busy);
        match result {
            Ok(rows) => timeline.set(rows),
            Err(reason) => timeline.fail(reason),
        }
        timeline.scroll()
    }

    /// Sends the composer: to the open session, or as the start of a new one.
    ///
    /// A working session gets the prompt queued, or steered with `delivery`.
    fn send(&mut self, delivery: Option<Delivery>) -> Task<Message> {
        let text = self.composer.text();
        if (text.trim().is_empty() && self.draft.attached.is_empty()) || self.sending.is_some() {
            return Task::none();
        }
        let Some(client) = self.client.clone() else {
            self.send_error = Some("Not connected to OpenCode yet.".to_owned());
            return Task::none();
        };
        let folder = self.folder();
        let delivery = delivery.unwrap_or(Delivery::Queue);
        let outgoing = compose::outgoing(
            &text,
            &self.draft,
            self.catalogs.get(&folder).unwrap_or(&Catalog::default()),
            &folder,
            delivery,
        );
        let target = self.active.clone();
        if target.is_none() && outgoing == Outgoing::Compact {
            self.send_error = Some("There's nothing to summarize yet.".to_owned());
            return Task::none();
        }
        self.send_error = None;
        self.menu = None;
        let scroll = self.show_sending(target.as_deref(), &text, &outgoing, delivery);
        let new = target
            .is_none()
            .then(|| (self.new_agent.clone(), self.new_model.clone()));
        Task::batch([
            Task::perform(
                send(client, target, folder, new, outgoing, delivery),
                Message::Sent,
            ),
            scroll,
        ])
    }

    /// Runs `action` on the server for the open session; a failure shows under the composer.
    fn act<F, Fut>(&self, action: F) -> Task<Message>
    where
        F: FnOnce(Client, String) -> Fut,
        Fut: Future<Output = Result<(), server::Error>> + Send + 'static,
    {
        let (Some(client), Some(id)) = (self.client.clone(), self.active.clone()) else {
            return Task::none();
        };
        let future = action(client, id);
        Task::perform(
            async move { future.await.map_err(|e| e.to_string()) },
            Message::Acted,
        )
    }

    /// Sets the open session's model, or the next new session's.
    fn choose_model(&mut self, model: ModelRef) -> Task<Message> {
        if self.active.is_none() {
            self.new_model = Some(model);
            return Task::none();
        }
        self.act(|client, id| async move { client.set_model(&id, &model).await })
    }

    /// Attaches a pasted PNG image.
    fn attach_image(&mut self, png: &[u8]) {
        let count = self
            .draft
            .attached
            .iter()
            .filter(|a| a.attachment.uri.starts_with("data:"))
            .count();
        let name = format!("pasted-image-{}.png", count + 1);
        self.draft.attached.push(Attached {
            name: name.clone(),
            attachment: Attachment::inline("image/png", png, &name),
        });
        let reads_images = self
            .current_model()
            .and_then(|m| Some(self.catalog()?.model(&m.provider_id, &m.id)?.images));
        if reads_images == Some(false) {
            self.send_error = Some(
                "The chosen model can't see images; pick one that can before sending.".to_owned(),
            );
        }
    }

    /// Opens, updates or closes the `/` and `@` menu for the word at the cursor.
    fn refresh_menu(&mut self) -> Task<Message> {
        let cursor = self.composer.cursor().position;
        let line = self
            .composer
            .line(cursor.line)
            .map(|line| line.text.into_owned())
            .unwrap_or_default();
        let Some(trigger) = compose::trigger(&line, cursor.line, cursor.column) else {
            self.menu = None;
            self.dismissed = None;
            return Task::none();
        };
        if self.dismissed == Some((trigger.line, trigger.start)) {
            self.menu = None;
            return Task::none();
        }
        let mut task = Task::none();
        if trigger.menu == compose::Menu::At && self.searched.as_ref() != Some(&trigger.query) {
            self.searched = Some(trigger.query.clone());
            task = self.find_files(trigger.query.clone());
        }
        let empty = Catalog::default();
        let catalog = self.catalogs.get(&self.folder()).unwrap_or(&empty);
        let items = match trigger.menu {
            compose::Menu::Slash => compose::slash_options(catalog, &trigger.query),
            compose::Menu::At => compose::at_options(catalog, &self.files, &trigger.query),
        };
        let selected = match &self.menu {
            Some(menu)
                if (menu.trigger.line, menu.trigger.start) == (trigger.line, trigger.start) =>
            {
                menu.selected.min(items.len().saturating_sub(1))
            }
            _ => 0,
        };
        self.menu = Some(MenuState {
            trigger,
            items,
            selected,
            scroller: Id::new(MENU_SCROLLER),
        });
        task
    }

    /// Asks the server for files matching an `@` word.
    fn find_files(&mut self, query: String) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        self.searches += 1;
        let search = self.searches;
        let folder = self.folder();
        Task::perform(
            async move {
                client
                    .find_files(&folder, &query, FILE_MATCHES)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |result| Message::Found(search, result),
        )
    }

    /// Moves the menu's highlight, wrapping around, and keeps it in view.
    fn move_menu(&mut self, by: i32) -> Task<Message> {
        let Some(menu) = self.menu.as_mut().filter(|m| !m.items.is_empty()) else {
            return Task::none();
        };
        let len = i64::try_from(menu.items.len()).unwrap_or(i64::MAX);
        let at = i64::try_from(menu.selected).unwrap_or(0) + i64::from(by);
        menu.selected = usize::try_from(at.rem_euclid(len)).unwrap_or(0);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a menu has far fewer rows than an f32 holds"
        )]
        let bottom = (menu.selected + 1) as f32 * MENU_ROW;
        let y = (bottom - MENU_ROWS * MENU_ROW).max(0.0);
        iced::widget::operation::scroll_to(
            menu.scroller.clone(),
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(y),
            },
        )
    }

    /// Applies a message about the pickers' menus, and adding a project.
    fn picker_action(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePicker(picker) => {
                if self.picker == Some(picker) {
                    self.picker = None;
                    return Task::none();
                }
                self.picker = Some(picker);
                self.picker_filter.clear();
                self.picker_scroll = 0.0;
                self.picker_index = pickers::items(self, picker)
                    .iter()
                    .position(|item| item.selected)
                    .unwrap_or(0);
                let reveal = self.reveal_highlight();
                if picker == Picker::Model {
                    return Task::batch([
                        reveal,
                        iced::widget::operation::focus(Id::new(pickers::MENU_FILTER)),
                    ]);
                }
                return reveal;
            }
            Message::ClosePicker => self.picker = None,
            Message::PickerFilter(filter) => {
                self.picker_filter = filter;
                self.picker_index = 0;
                return self.reveal_highlight();
            }
            Message::PickerMove(by) => {
                let Some(picker) = self.picker else {
                    return Task::none();
                };
                let len = i64::try_from(pickers::items(self, picker).len()).unwrap_or(0);
                if len > 0 {
                    let at = i64::try_from(self.picker_index).unwrap_or(0) + i64::from(by);
                    self.picker_index = usize::try_from(at.rem_euclid(len)).unwrap_or(0);
                    return self.reveal_highlight();
                }
            }
            Message::PickerSubmit => {
                if let Some(item) = self.picker.and_then(|picker| {
                    pickers::items(self, picker)
                        .into_iter()
                        .nth(self.picker_index)
                }) {
                    return self.update(item.message);
                }
            }
            Message::PickerScrolled(y) => self.picker_scroll = y,
            Message::AddProject => {
                self.picker = None;
                return Task::perform(pick_folder(), Message::ProjectChosen);
            }
            Message::ProjectChosen(Some(folder)) => {
                // A new session there makes it a project.
                self.active = None;
                self.new_folder = Some(folder.to_string_lossy().into_owned());
            }
            _ => {}
        }
        Task::none()
    }

    /// Scrolls the open menu's list so its highlighted row shows.
    fn reveal_highlight(&mut self) -> Task<Message> {
        let step = pickers::MENU_ROW + pickers::ROW_GAP;
        let index = u16::try_from(self.picker_index).unwrap_or(u16::MAX);
        let rows = u16::try_from(pickers::MENU_ROWS).unwrap_or(u16::MAX);
        let top = f32::from(index) * step;
        let shown = f32::from(rows) * step;
        if top < self.picker_scroll {
            self.picker_scroll = top;
        } else if top + step > self.picker_scroll + shown {
            self.picker_scroll = top + step - shown;
        }
        iced::widget::operation::scroll_to(
            Id::new(pickers::MENU_LIST),
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(self.picker_scroll),
            },
        )
    }

    /// Puts a menu row's text in place of the menu word.
    fn pick(&mut self, index: Option<usize>) -> Task<Message> {
        let Some(menu) = self.menu.take() else {
            return Task::none();
        };
        let Some(item) = menu.items.get(index.unwrap_or(menu.selected)) else {
            return Task::none();
        };
        let empty = Catalog::default();
        let catalog = self.catalogs.get(&self.folder()).unwrap_or(&empty);
        let mut insert = compose::insertion(&item.choice, catalog);
        let trigger = &menu.trigger;
        match &item.choice {
            // A folder stays a menu word, so its files show next.
            Choice::File {
                directory: true, ..
            } => insert.truncate(insert.trim_end().len()),
            Choice::File { .. } | Choice::Agent(_) | Choice::Resource { .. } => self
                .draft
                .mention(insert.trim_end().to_owned(), item.choice.clone()),
            // `/compact` is complete; Enter now sends it.
            Choice::Compact => self.dismissed = Some((trigger.line, trigger.start)),
            Choice::Command(_) | Choice::Skill(_) => {}
        }
        self.composer.move_to(text_editor::Cursor {
            position: text_editor::Position {
                line: trigger.line,
                column: trigger.end,
            },
            selection: Some(text_editor::Position {
                line: trigger.line,
                column: trigger.start,
            }),
        });
        self.composer
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                Arc::new(insert),
            )));
        self.refresh_menu()
    }

    /// The folder the composer works in: the open session's, or the new session's.
    fn folder(&self) -> String {
        match self
            .active
            .as_deref()
            .and_then(|id| self.workspace.session(id))
        {
            Some(session) => session.directory.clone(),
            None => self.new_folder.clone().unwrap_or_else(home_folder),
        }
    }

    /// Loads what the current folder offers if it isn't loaded, and keeps the model picker on it.
    fn follow_folder(&mut self) -> Task<Message> {
        let folder = self.folder();
        let mut task = Task::none();
        if let Some(client) = self.client.clone()
            && !self.catalogs.contains_key(&folder)
            && self.loading_catalogs.insert(folder.clone())
        {
            let directory = folder.clone();
            task = Task::perform(
                async move {
                    let result = server::catalog(&client, &directory)
                        .await
                        .map(Box::new)
                        .map_err(|e| e.to_string());
                    (directory, result)
                },
                |(directory, result)| Message::Catalog(directory, result),
            );
        }
        if self.models_for.as_deref() != Some(folder.as_str()) {
            self.models_for = Some(folder);
            self.rebuild_models();
        }
        task
    }

    /// Forgets every folder's catalog, so the current one loads again.
    fn forget_catalogs(&mut self) {
        self.catalogs.clear();
        self.loading_catalogs.clear();
        self.models_for = None;
    }

    fn rebuild_models(&mut self) {
        let models = self
            .models_for
            .as_ref()
            .and_then(|folder| self.catalogs.get(folder))
            .map(|catalog| {
                catalog
                    .models
                    .iter()
                    .map(|m| ModelChoice {
                        provider_id: m.provider_id.clone(),
                        id: m.id.clone(),
                        name: m.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.models = models;
    }

    /// Returns what the current folder offers, once loaded.
    #[must_use]
    pub fn catalog(&self) -> Option<&Catalog> {
        self.catalogs.get(&self.folder())
    }

    /// The agents the agent picker offers.
    #[must_use]
    pub fn agent_choices(&self) -> Vec<AgentChoice> {
        self.catalog()
            .map(|c| {
                c.primary_agents()
                    .map(|a| AgentChoice {
                        id: a.id.clone(),
                        name: a.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The agent the open session, or the next new one, runs.
    #[must_use]
    pub fn current_agent(&self) -> Option<String> {
        let chosen = match self.active.as_deref() {
            Some(id) => self.workspace.session(id).and_then(|s| s.agent.clone()),
            None => self.new_agent.clone(),
        };
        chosen.or_else(|| Some(self.catalog()?.primary_agents().next()?.id.clone()))
    }

    /// The model the open session, or the next new one, uses.
    #[must_use]
    pub fn current_model(&self) -> Option<ModelRef> {
        let chosen = match self.active.as_deref() {
            Some(id) => self.workspace.session(id).and_then(|s| s.model.clone()),
            None => self.new_model.clone(),
        };
        chosen.or_else(|| {
            let model = self.catalog()?.default_model.as_ref()?;
            Some(ModelRef {
                provider_id: model.provider_id.clone(),
                id: model.id.clone(),
                variant: None,
            })
        })
    }

    /// The model picker's selection.
    #[must_use]
    pub fn current_model_choice(&self) -> Option<ModelChoice> {
        let model = self.current_model()?;
        let name = self
            .catalog()
            .and_then(|c| c.model(&model.provider_id, &model.id))
            .map_or_else(|| model.id.clone(), |m| m.name.clone());
        Some(ModelChoice {
            provider_id: model.provider_id,
            id: model.id,
            name,
        })
    }

    /// The reasoning levels the current model offers, its default first.
    #[must_use]
    pub fn variant_choices(&self) -> Vec<Variant> {
        let Some(model) = self.current_model() else {
            return Vec::new();
        };
        let Some(info) = self
            .catalog()
            .and_then(|c| c.model(&model.provider_id, &model.id))
        else {
            return Vec::new();
        };
        std::iter::once(Variant(None))
            .chain(info.variants.iter().map(|v| Variant(Some(v.clone()))))
            .collect()
    }

    /// The folders a new session can start in: home, then each project and its worktrees.
    #[must_use]
    pub fn folder_choices(&self) -> Vec<Folder> {
        let mut list = vec![Folder {
            directory: None,
            label: "Home folder".to_owned(),
        }];
        for project in &self.projects {
            list.push(Folder {
                directory: Some(project.directory.clone()),
                label: project.name.clone(),
            });
            for worktree in &project.worktrees {
                list.push(Folder {
                    directory: Some(worktree.directory.clone()),
                    label: format!("{} / {}", project.name, worktree.name),
                });
            }
        }
        // A folder just added isn't a project until a session starts in it.
        if let Some(folder) = &self.new_folder
            && !list.iter().any(|f| f.directory.as_ref() == Some(folder))
        {
            list.push(Folder {
                directory: Some(folder.clone()),
                label: folder.rsplit('/').next().unwrap_or(folder).to_owned(),
            });
        }
        list
    }

    /// Returns the prompts waiting for a session, oldest first.
    #[must_use]
    pub fn inbox(&self, id: &str) -> &[InboxItem] {
        self.workspace.inbox(id)
    }

    /// Returns whether a session is working right now.
    #[must_use]
    pub fn is_busy(&self, id: &str) -> bool {
        self.workspace.is_busy(id)
    }

    /// Returns a session's timeline rows, if it's open.
    #[must_use]
    pub fn timeline_entries(&self, id: &str) -> Option<Vec<Entry>> {
        Some(self.timelines.get(id)?.entries().cloned().collect())
    }

    /// Returns a session's title; empty until the server names it.
    #[must_use]
    pub fn title(&self, id: &str) -> String {
        self.workspace
            .session(id)
            .and_then(|s| s.title.clone())
            .unwrap_or_default()
    }

    /// Returns the open session's title, or `None` when no session is open.
    #[must_use]
    pub fn active_title(&self) -> Option<String> {
        let session = self.workspace.session(self.active.as_deref()?)?;
        Some(session.title.clone().unwrap_or_default())
    }

    /// Returns how many rows a session's timeline has, if it's open.
    #[must_use]
    pub fn timeline_len(&self, id: &str) -> Option<usize> {
        self.timelines.get(id).map(Timeline::len)
    }

    /// Lays out the window: rails on both edges, the sidebar, the main area and the panel.
    pub fn view(&self) -> Element<'_, Message> {
        let t = &self.tokens;
        let shown = self.shown();
        let mut layout = row![view::rails::left(t, &self.assets)];
        // Side panels slide open and closed. Each keeps its place in the row
        // (empty while closed), so the conversation's widgets keep their state.
        let sidebar = size::SIDEBAR * motion::shown(&self.sidebar_slide, self.clock);
        layout = layout.push(if sidebar > 0.0 {
            reveal(
                view::sidebar::view(self),
                size::SIDEBAR,
                sidebar,
                Edge::Left,
            )
            .into()
        } else {
            Element::from(space())
        });
        let main = self
            .active
            .as_deref()
            .and_then(|id| Some((id, self.timelines.get(id)?)))
            .map_or_else(
                || view::home::view(self),
                |(id, timeline)| view::session::view(self, id, timeline),
            );
        layout = layout.push(main);
        let panel = self.panel.filter(|_| shown.panel);
        let open = (size::PANEL + 1.0) * motion::shown(&self.panel_slide.0, self.clock);
        // While closing, the panel last shown slides away.
        layout = layout.push(match self.panel_slide.1.filter(|_| open > 0.0) {
            Some(sliding) => reveal(
                view::panel::view(t, sliding, self.active.is_some()),
                size::PANEL + 1.0,
                open,
                Edge::Right,
            )
            .into(),
            None => Element::from(space()),
        });
        let layout = layout.push(view::rails::right(t, panel)).height(Fill);
        if self.picking_location && shown.sidebar {
            stack![layout, view::locations::view(self)].into()
        } else {
            layout.into()
        }
    }

    /// Follows the current server, and ticks the clock for session ages.
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::run_with(self.location.clone(), |location| {
                server::follow(location.clone())
            })
            .map(Message::Server),
            iced::time::every(TICK).map(|_| Message::Tick(SystemTime::now())),
            keyboard::listen().filter_map(shortcut),
            if self.picker.is_some() {
                keyboard::listen().filter_map(menu_key)
            } else {
                Subscription::none()
            },
            iced::window::resize_events().map(|(_, size)| Message::Resized(size)),
            self.motion(),
        ])
    }

    /// The iced theme supplying widget defaults.
    #[must_use]
    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }
}

/// Maps a key press the focused widget didn't use to a shortcut.
///
/// Ctrl+N starts a new session, Ctrl+W closes the tab, and Ctrl+Tab,
/// Ctrl+Shift+Tab, Ctrl+Page Down and Ctrl+Page Up switch tabs.
#[must_use]
pub fn shortcut(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    if !modifiers.command() {
        return None;
    }
    match key.as_ref() {
        Key::Character("n") => Some(Message::NewSession(None)),
        Key::Character("w") => Some(Message::CloseActiveTab),
        Key::Named(Named::Tab) => Some(Message::CycleTab {
            back: modifiers.shift(),
        }),
        Key::Named(Named::PageDown) => Some(Message::CycleTab { back: false }),
        Key::Named(Named::PageUp) => Some(Message::CycleTab { back: true }),
        _ => None,
    }
}

/// Maps a key press to moving through or choosing from an open picker's menu.
#[must_use]
pub fn menu_key(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    if modifiers.command() || modifiers.alt() {
        return None;
    }
    match key.as_ref() {
        Key::Named(Named::ArrowUp) => Some(Message::PickerMove(-1)),
        Key::Named(Named::ArrowDown) => Some(Message::PickerMove(1)),
        Key::Named(Named::Enter) => Some(Message::PickerSubmit),
        _ => None,
    }
}

/// Asks for a folder to start a session in; `None` when the dialog is cancelled.
async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Add project")
        .pick_folder()
        .await
        .map(|folder| folder.path().to_path_buf())
}

/// The user's home folder, where new sessions start unless another is chosen.
fn home_folder() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_owned())
}

/// Asks for files to attach; an empty list when the dialog is cancelled.
async fn pick_files() -> Vec<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Attach files")
        .pick_files()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|file| file.path().to_path_buf())
        .collect()
}

/// Sends `outgoing` to `target`, or creates a session in `folder` and sends it there.
///
/// A new session first gets the chosen agent and model in `new`.
async fn send(
    client: Client,
    target: Option<String>,
    folder: String,
    new: Option<(Option<String>, Option<ModelRef>)>,
    outgoing: Outgoing,
    delivery: Delivery,
) -> Result<Box<Session>, String> {
    let failed = |e: server::Error| format!("Couldn't send: {e}");
    let session = match target {
        Some(id) => client.session(&id).await,
        None => client.create_session(&folder).await,
    }
    .map_err(failed)?;
    if let Some((agent, model)) = new {
        if let Some(agent) = agent {
            client
                .set_agent(&session.id, &agent)
                .await
                .map_err(failed)?;
        }
        if let Some(model) = model {
            client
                .set_model(&session.id, &model)
                .await
                .map_err(failed)?;
        }
    }
    match outgoing {
        Outgoing::Prompt(prompt) => client.prompt(&session.id, &prompt).await,
        Outgoing::Command { name, arguments } => {
            client
                .run_command(&session.id, &name, &arguments, delivery)
                .await
        }
        Outgoing::Compact => client.compact(&session.id).await,
    }
    .map_err(failed)?;
    Ok(Box::new(session))
}
