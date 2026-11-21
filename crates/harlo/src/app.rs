//! The app's state, messages and update, and the window's top-level layout.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, SystemTime};

use harlo_core::layout::{Layout, Store, Tabs};
use harlo_core::server::{self, Client, Delivery, Location, Update};
use harlo_core::sessions::Project;
use harlo_core::theme::{self, Mode, Tokens};
use harlo_core::timeline::{Entry, entries};
use harlo_core::workspace::{EventKind, Fetch, Fetched, Session, Workspace};
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{row, stack, text_editor};
use iced::{Element, Fill, Subscription, Task, Theme};

use crate::assets::Assets;
use crate::timeline::{self, Timeline};
use crate::view;

/// How often session ages are refreshed.
const TICK: Duration = Duration::from_secs(30);

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
    /// Send the composer's text.
    Send,
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
    sending: bool,
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
            panel: layout
                .panel
                .as_deref()
                .and_then(|title| Panel::ALL.into_iter().find(|p| p.title() == title)),
            tabs: Vec::new(),
            active: None,
            timelines: HashMap::new(),
            new_folder: None,
            composer: text_editor::Content::new(),
            send_error: None,
            sending: false,
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
        let task = self.apply(message);
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
            Message::ToggleSidebar => self.sidebar = !self.sidebar,
            Message::Filter(filter) => self.filter = filter,
            Message::ToggleMore(folder) => {
                if !self.expanded.remove(&folder) {
                    self.expanded.insert(folder);
                }
            }
            Message::OpenPanel(panel) => {
                self.panel = if self.panel == Some(panel) {
                    None
                } else {
                    Some(panel)
                };
            }
            Message::ClosePanel => self.panel = None,
            Message::Compose(action) => self.composer.perform(action),
            Message::ToggleLocations => self.picking_location = !self.picking_location,
            Message::PickLocation(location) => {
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
                    self.workspace = Workspace::new(self.workspace.order().to_vec());
                    self.refresh();
                }
            }
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
            Message::Send => return self.send(),
            Message::Sent(Ok(session)) => {
                self.sending = false;
                self.composer = text_editor::Content::new();
                let id = session.id.clone();
                let fetches = self.workspace.upsert(*session);
                self.refresh();
                return Task::batch([self.open(id.clone()), self.load(id), self.fetch(fetches)]);
            }
            Message::Sent(Err(reason)) => {
                self.sending = false;
                self.send_error = Some(reason);
            }
            Message::History(id, result) => {
                if let Some(timeline) = self.timelines.get_mut(&id) {
                    match result {
                        Ok(rows) => timeline.set(rows),
                        Err(reason) => timeline.fail(reason),
                    }
                }
            }
            Message::Timeline(id, event) => {
                if let Some(timeline) = self.timelines.get_mut(&id) {
                    timeline.update(&event);
                }
            }
            Message::Server(update) => return self.server(update),
            Message::Fetched(Ok(result)) => {
                let fetches = self.workspace.fetched(result);
                self.refresh();
                return self.fetch(fetches);
            }
            Message::Fetched(Err(_)) => {
                // The row stays as it was until the next event or resync corrects it.
            }
            Message::Tick(now) => {
                self.now = now;
                self.refresh();
            }
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
                let reload = match (&event.kind, event.kind.session_id()) {
                    // Until the timeline streams (M3), it reloads when a step or run ends.
                    (EventKind::StepEnded { .. } | EventKind::Done { .. }, Some(id))
                        if self.timelines.contains_key(id) =>
                    {
                        self.load(id.to_owned())
                    }
                    _ => Task::none(),
                };
                return Task::batch([self.fetch(fetches), reload]);
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

    /// Loads a session's history into its timeline.
    fn load(&self, id: String) -> Task<Message> {
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

    /// Sends the composer's text: to the open session, or as the start of a new one.
    fn send(&mut self) -> Task<Message> {
        let text = self.composer.text().trim_end().to_owned();
        if text.trim().is_empty() || self.sending {
            return Task::none();
        }
        let Some(client) = self.client.clone() else {
            self.send_error = Some("Not connected to OpenCode yet.".to_owned());
            return Task::none();
        };
        self.sending = true;
        self.send_error = None;
        let target = self.active.clone();
        let folder = self.new_folder.clone().unwrap_or_else(home_folder);
        Task::perform(send(client, target, folder, text), Message::Sent)
    }

    /// Returns a session's title; empty until the server names it.
    #[must_use]
    pub fn title(&self, id: &str) -> String {
        self.workspace
            .session(id)
            .and_then(|s| s.title.clone())
            .unwrap_or_default()
    }

    /// Returns a session's agent and model, when known.
    #[must_use]
    pub fn agent_and_model(&self, id: &str) -> (Option<String>, Option<String>) {
        let session = self.workspace.session(id);
        (
            session.and_then(|s| s.agent.clone()),
            session.and_then(|s| s.model.as_ref().map(|m| m.id.clone())),
        )
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
        let mut layout = row![view::rails::left(t, &self.assets)];
        if self.sidebar {
            layout = layout.push(view::sidebar::view(self));
        }
        let main = self
            .active
            .as_deref()
            .and_then(|id| Some((id, self.timelines.get(id)?)))
            .map_or_else(
                || view::home::view(self),
                |(id, timeline)| view::session::view(self, id, timeline),
            );
        layout = layout.push(main);
        if let Some(panel) = self.panel {
            layout = layout.push(view::panel::view(t, panel, self.active.is_some()));
        }
        let layout = layout.push(view::rails::right(t, self.panel)).height(Fill);
        if self.picking_location && self.sidebar {
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

/// The user's home folder, where new sessions start unless another is chosen.
fn home_folder() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_owned())
}

/// Sends `text` to `target`, or creates a session in `folder` and sends it there.
async fn send(
    client: Client,
    target: Option<String>,
    folder: String,
    text: String,
) -> Result<Box<Session>, String> {
    let session = match target {
        Some(id) => client.session(&id).await,
        None => client.create_session(&folder).await,
    }
    .map_err(|e| format!("Couldn't send: {e}"))?;
    client
        .prompt(&session.id, &server::message_id(), &text, Delivery::Queue)
        .await
        .map_err(|e| format!("Couldn't send: {e}"))?;
    Ok(Box::new(session))
}
