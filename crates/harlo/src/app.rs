//! The app's state, messages and update.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use harlo_core::approvals::{self, Shield};
use harlo_core::composer::{self as compose, Attached, Choice, Draft, MenuItem, Outgoing, Trigger};
use harlo_core::layout::{Layout, Preferences, Store, Tabs};
use harlo_core::server::{
    self, Attachment, Catalog, Client, Delivery, FileMatch, Location, Update,
};
use harlo_core::sessions::Project;
use harlo_core::theme::{self, Mode, Scheme, Tokens};
use harlo_core::timeline::{Entry, entries};
use harlo_core::workspace::{EventKind, Fetch, Fetched, InboxItem, ModelRef, Session, Workspace};
use std::time::Instant;

use crate::fit::{self, Side};
use crate::keys::{Key, KeyPress};
use crate::model::{self, AgentChoice, Feedback, Folder, ModelChoice, Picker, Variant};
use crate::task::{Effect, Task};
use crate::text::{Cursor, Edit, Position, TextBox};
use crate::timeline::{self, Activity, Timeline};

mod agents;
mod appearance;
mod arrange;
mod asides;
mod changes;
mod claude;
mod commit_message;
mod computers;
mod context;
mod files;
mod git_settings;
mod goals;
mod hosting;
mod jobs;
mod line_comments;
mod local_models;
mod mcp;
mod memory;
mod notes;
mod notices;
mod own_tools;
mod phone;
mod pictures;
mod projects;
mod providers;
mod schedules;
mod session_menu;
mod sheets;
mod subagents;
mod terminal;
mod todos;
mod undo;
mod updates;
mod voice;

pub use agents::{ApprovalEvent, Approvals};
pub use arrange::{Document, Edge};
pub use asides::{Answer, Aside, AsideEvent};
pub use changes::{
    ChangedFile, Changes, Event as ChangesEvent, GitAction, List, Loaded as ChangesLoaded,
};
pub use claude::{Action as ClaudeAction, Claude, Event as ClaudeEvent};
pub use commit_message::{clean_message, commit_prompt};
pub use computers::{ComputerEvent, IMPORTING, imported_text};
pub use context::{
    ContextView, Event as ContextEvent, Facts as ContextFacts, Loaded as ContextLoaded,
};
pub use files::{Body as FileBody, Files, FilesEvent, OpenFile, Outside, TextFile};
pub use git_settings::{GitEvent, GitSettings};
pub use goals::{GoalAction, GoalEvent};
pub use hosting::{HostingEvent, HostingState, SigningIn};
pub use jobs::JobEvent;
pub use line_comments::{Commenting, LineComment, LineRef, comments_message};
pub use local_models::{EngineState, Getting, LocalEvent, LocalModels, ServerState};
pub use mcp::{McpEvent, McpServers, McpView};
pub use memory::{Kept, Memory, MemoryEvent};
pub use notes::{Note, NoteKey, Notes, NotesEvent};
pub use notices::Notice;
pub use own_tools::OwnToolsEvent;
pub use phone::{Host as PhoneHost, Paired, PhoneEvent, PhoneState, Shown};
pub use pictures::{PictureEvent, PicturesState, Pixels, Sent, Thumb, Viewing};
pub use projects::{ProjectEvent, Projects, icon_url};
pub use providers::{Attempt as SignInAttempt, FORM_FIELDS, ProviderEvent, Providers};
pub use schedules::{
    Draft as ScheduleDraft, Page as SchedulePage, ScheduleEvent, Schedules,
    session_of as schedule_session,
};
pub use session_menu::SessionAction;
pub use sheets::{
    Autostart, Event as SheetEvent, Field, Load, Range, Section, SetUp, Setting, SettingChoice,
    Sheet, Sheets, Step,
};
pub use subagents::Child;
pub use terminal::{Event as TerminalEvent, Shell, Terminals};
pub use undo::StagedUndo;
pub use updates::{Phase as UpdatePhase, ServerRestart, UpdateEvent, Updates, VERSION};
pub use voice::{Listening, Voice, VoiceEvent};
mod waiting;
pub(crate) use claude::{epoch_seconds, resets_in};
pub(crate) use waiting::capitalized;

/// How often session ages are refreshed.
const TICK: Duration = Duration::from_secs(30);

/// The avatars the user can pick, by name, in the order the picker shows
/// them; the rail's `Avatars` list holds the pictures in the same order.
pub const AVATARS: [&str; 6] = harlo_core::phone::theme::AVATARS;

impl App {
    /// The user's avatar, as its place in [`AVATARS`]; the first when the
    /// saved name is unknown.
    #[must_use]
    pub fn avatar(&self) -> usize {
        AVATARS
            .iter()
            .position(|name| *name == self.preferences.avatar)
            .unwrap_or(0)
    }

    /// Uses the avatar at `index` in [`AVATARS`], and saves the choice.
    fn pick_avatar(&mut self, index: usize) -> Task<Message> {
        let Some(name) = AVATARS.get(index) else {
            return Task::none();
        };
        (*name).clone_into(&mut self.preferences.avatar);
        self.save_preferences()
    }
}

/// What the button after a stopped run sends.
const CONTINUE: &str = "Continue";

/// How many files the `@` menu asks the server for.
const FILE_MATCHES: usize = 20;

/// Height of a menu row with its gap, and how many rows show before it scrolls.
const MENU_ROW: f32 = 32.0;
const MENU_ROWS: f32 = 8.0;

/// The scroller inside the composer's menu.
pub const MENU_SCROLLER: &str = "composer-menu-list";

/// The open `/` or `@` menu above the composer.
#[derive(Debug, Clone)]
pub struct MenuState {
    /// The word that opened it.
    pub trigger: Trigger,
    /// What it offers.
    pub items: Vec<MenuItem>,
    /// The highlighted row.
    pub selected: usize,
}

/// Tool panels: the right rail opens one at a time on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    /// Context window use and session facts.
    Context,
    /// The session's todo list.
    Todos,
    /// The session's subagents, and how each is doing.
    Subagents,
    /// Files the session changed.
    Changes,
    /// The project's files.
    Files,
    /// The project's notes.
    Notes,
    /// Side questions asked with `/btw`, and their answers.
    Asides,
}

impl Panel {
    /// Every panel, in rail order.
    pub const ALL: [Self; 7] = [
        Self::Context,
        Self::Todos,
        Self::Subagents,
        Self::Changes,
        Self::Files,
        Self::Notes,
        Self::Asides,
    ];

    /// The widget ID of the panel's rail button.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Context => "panel-context",
            Self::Todos => "panel-todos",
            Self::Subagents => "panel-subagents",
            Self::Changes => "panel-changes",
            Self::Files => "panel-files",
            Self::Notes => "panel-notes",
            Self::Asides => "panel-asides",
        }
    }

    /// The panel's title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Context => "Context",
            Self::Todos => "Todos",
            Self::Subagents => "Subagents",
            Self::Changes => "Changes",
            Self::Files => "Files",
            Self::Notes => "Notes",
            Self::Asides => "Side questions",
        }
    }

    /// The panel with `title`, in any case.
    #[must_use]
    pub fn named(title: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.title().eq_ignore_ascii_case(title))
    }
}

/// A saved size, or `default` when none was saved, kept within `range`.
fn size(saved: Option<u32>, default: f32, (least, most): (f32, f32)) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a size in pixels is far below 2^24"
    )]
    saved.map_or(default, |n| n as f32).clamp(least, most)
}

/// A size in whole logical pixels, for saving.
fn pixels(size: f32) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sizes are positive and far below u32::MAX"
    )]
    let whole = size.round().max(0.0) as u32;
    whole
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
        /// What the server is: `The server`, or `Harlo` for Harlo's own.
        server: String,
        /// Its version.
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

/// What Harlo keeps for each session beside its history, by session ID.
#[derive(Debug, Default)]
struct PerSession {
    /// Side questions asked about it.
    asides: HashMap<String, Vec<Aside>>,
    /// Its goal, once read and while it has one.
    goals: HashMap<String, harlo_core::server::Goal>,
    /// Its background jobs, once read: the running ones and the latest
    /// ended.
    jobs: HashMap<String, Vec<harlo_core::server::Job>>,
}

/// Something the user did, or something the server reported.
#[derive(Debug, Clone)]
pub enum Message {
    /// Show or hide the sidebar.
    ToggleSidebar,
    /// Use the avatar at this place in [`AVATARS`] as the user's picture.
    PickAvatar(usize),
    /// The session filter changed.
    Filter(String),
    /// Show all of a folder's sessions, or fewer again, by folder.
    ToggleMore(String),
    /// Show a tool panel on the right, in place of the one there.
    OpenPanel(Panel),
    /// Show a tool panel on the right, or close it when it's shown.
    TogglePanel(Panel),
    /// Close the tool panel, maximized or on the right.
    ClosePanel,
    /// An edge was dragged this far, right or down.
    Resize {
        /// The edge.
        edge: Edge,
        /// How far, in logical pixels.
        by: f32,
    },
    /// The drag of an edge ended.
    ResizeEnd,
    /// Show a tab in the center, as when it's clicked.
    ShowDocument(Document),
    /// Close a tab in the center.
    CloseDocument(Document),
    /// Give a tool panel the whole space between the rails, until [`Message::Restore`].
    Maximize(Panel),
    /// Put the maximized panel back where it was, and the layout with it.
    Restore,
    /// Something happened to the terminals.
    Terminal(TerminalEvent),
    /// Something happened to the pages over the window.
    Sheet(SheetEvent),
    /// Something happened to the Claude subscription.
    Claude(ClaudeEvent),
    /// Something happened to Harlo's own tools for agents.
    OwnTools(OwnToolsEvent),
    /// Something happened to dictation.
    Voice(VoiceEvent),
    /// Something happened to local models.
    Local(LocalEvent),
    /// Something that happened to phone access.
    Phone(PhoneEvent),
    /// Something happened to Harlo's own update.
    Update(UpdateEvent),
    /// Something happened in Settings → Memory.
    Memory(MemoryEvent),
    /// Something happened to pictures: a thumbnail, the viewer, the grid.
    Pictures(PictureEvent),
    /// Something happened to a side question.
    Aside(AsideEvent),
    /// Something happened to a session's goal.
    Goal(GoalEvent),
    /// Something happened to a session's background jobs.
    Job(JobEvent),
    /// Something done from the open session's menu, or what came of it.
    SessionMenu(SessionAction),
    /// The composer was edited.
    Compose(Edit),
    /// Open or close the location picker.
    ToggleLocations,
    /// Follow another server.
    PickLocation(Location),
    /// Open a session in a tab, or switch to its tab.
    OpenSession(String),
    /// A notification about this session was clicked.
    NoticeOpened(String),
    /// Close a session's tab.
    CloseTab(String),
    /// Close the active tab, if a session is shown.
    CloseActiveTab,
    /// Quit Harlo, even while the tray keeps it running.
    Quit,
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
    /// Ask the open session's agent to go on after a stop, leaving the
    /// composer as it is.
    ContinueRun,
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
    /// Switch to the next agent a session can run, or the one before.
    CycleAgent {
        /// Go back rather than forward.
        back: bool,
    },
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
    /// The branch checked out in a worktree's folder, read.
    Branch(String, Option<String>),
    /// The clock moved on; ages are measured from this time.
    Tick(SystemTime),
    /// The desktop's colour scheme is dark (`true`) or light.
    SystemDark(bool),
    /// A fade ended; the window shows the rows once more, still.
    Settled,
    /// The window was resized, to this width and height.
    Resized(f32, f32),
    /// Answer an approval request in the open session.
    Approve {
        /// The request's ID.
        request: String,
        /// Allow once, always, or refuse.
        reply: server::PermissionReply,
    },
    /// Start refusing an approval request with a reason, by request ID.
    Refuse(String),
    /// The reason typed for refusing.
    RefuseReason(String),
    /// Send the refusal with its reason.
    RefuseSend,
    /// Stop refusing with a reason.
    RefuseCancel,
    /// An answer to an approval or a question finished: the session, the
    /// request or form ID, and why it failed.
    Answered(String, String, Result<(), String>),
    /// Choose an option in a question, or toggle it in a multiple choice.
    FormChoose {
        /// The question's ID.
        form: String,
        /// The field's key.
        key: String,
        /// The option's value.
        value: String,
    },
    /// Type into a question's field.
    FormType {
        /// The question's ID.
        form: String,
        /// The field's key.
        key: String,
        /// The text.
        text: String,
    },
    /// Answer yes or no in a question.
    FormFlag {
        /// The question's ID.
        form: String,
        /// The field's key.
        key: String,
        /// The answer.
        flag: bool,
    },
    /// Send a question's answers, by question ID.
    FormSend(String),
    /// Dismiss a question without answering, by question ID.
    FormDismiss(String),
    /// Open a link a question points to.
    OpenLink(String),
    /// Choose how much the session may do without asking.
    PickShield(Shield),
    /// The window gained or lost focus.
    Focused(bool),
    /// An undo was staged in a session, or couldn't be.
    UndoStaged(String, Result<undo::Staged, String>),
    /// The agent's changes a staged undo is checked against, fetched for a
    /// session whose undo was staged before Harlo opened or by another app.
    UndoChecked(String, Result<Vec<server::FileDiff>, String>),
    /// Keep a session's staged undo.
    UndoKeep(String),
    /// Cancel a session's staged undo, putting its files back.
    UndoCancel(String),
    /// The Changes panel reported something, or its load or git action finished.
    Changes(ChangesEvent),
    /// The Files panel or an open file reported something.
    Files(FilesEvent),
    /// A note was read, edited or saved.
    Notes(NotesEvent),
    /// The Context panel asked to reload, or its load finished.
    Context(ContextEvent),
    /// A staged undo was kept or cancelled, or couldn't be.
    UndoSettled {
        /// The session.
        session: String,
        /// Whether it was kept, rather than cancelled.
        kept: bool,
        /// Whether that worked.
        result: Result<(), String>,
    },
}

/// How long reading a pasted image takes before the composer says so.
///
/// Most clipboards answer at once; a note that flashes up for a frame
/// would only be noise.
const PASTE_NOTICE: Duration = Duration::from_millis(150);

/// A send on its way to the server.
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about the window, such as whether the tray shows, not one state"
)]
pub struct App {
    /// Colours for every widget, from the chosen theme.
    pub tokens: Tokens,
    /// The bundled themes: Harlo's own dark and light first, then the rest
    /// by name.
    pub themes: Vec<Scheme>,
    /// Whether the desktop prefers dark, which Appearance's System follows.
    system_mode: harlo_core::theme::Mode,
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
    /// Whether the user has the sidebar open.
    pub sidebar: bool,
    /// The tool panel the user has open on the right, if any.
    pub panel: Option<Panel>,
    /// The tool panel covering the space between the rails, if one is.
    pub maximized: Option<Panel>,
    /// The terminals, shown under the center or hidden.
    pub terminals: Terminals,
    /// Side questions and goals, by session.
    per_session: PerSession,
    /// The pages over the window: shortcuts, stats and settings.
    pub sheets: Sheets,
    /// The window's width, which decides which panels fit.
    width: f32,
    /// The window's height.
    height: f32,
    /// A panel the user opened while it didn't fit, kept on screen.
    keep: Option<Side>,
    /// The widths the user dragged the sidebar and right panel to, and the
    /// terminals' height.
    sidebar_width: f32,
    panel_width: f32,
    terminal_height: f32,
    /// Whether an edge is being dragged; the layout is saved once it's let go.
    resizing: bool,
    /// Open sessions, in tab order.
    pub tabs: Vec<String>,
    /// The session shown, if any; `None` shows the start screen.
    pub active: Option<String>,
    timelines: HashMap<String, Timeline>,
    /// The folder a new session starts in; `None` is the home folder.
    pub new_folder: Option<String>,
    /// The home folder: where sessions outside a project work. Tests set a
    /// scratch folder.
    pub home: String,
    /// The server's global config folder on this computer, which Settings may
    /// change for the background service. Only the running app sets it;
    /// tests give a scratch folder or none.
    pub config_folder: Option<std::path::PathBuf>,
    /// The SSH config whose hosts Settings → Server offers; none in tests
    /// unless they give one.
    pub ssh_config: Option<std::path::PathBuf>,
    /// Starting Harlo at login. Only the running app sets it; tests give a
    /// scratch folder or none.
    pub autostart: Option<Autostart>,
    /// Git's global config: the user's own only in the running app, which
    /// sets it; otherwise none, so nothing reads or changes theirs.
    pub git_config: harlo_core::git::Global,
    /// The tools that sign git in to hosting sites: only the running app
    /// sets them; tests give scratch ones or none.
    pub hosting: Option<harlo_core::hosting::Hosting>,
    /// Whether sign-in pages open in the browser: only the running app
    /// sets it, so tests never open one.
    pub opens_pages: bool,
    /// Whether the desktop shows Harlo's tray icon.
    pub tray: bool,
    /// The composer's text.
    pub composer: TextBox,
    /// Why the last send failed, shown under the composer.
    pub send_error: Option<String>,
    /// The send on its way to the server, if any.
    sending: Option<Sending>,
    /// When reading an image from the clipboard started, while it's going on.
    pasting: Option<Instant>,
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
    /// The keys of the models in `models` that show unless hidden, by
    /// [`harlo_core::model_menu::shown_by_default`].
    model_defaults: std::collections::HashSet<String>,
    /// The branch checked out in each worktree's folder: `None` until
    /// read, or when it's on none.
    branches: std::collections::HashMap<String, Option<String>>,
    models_for: Option<String>,
    /// The picker whose menu is open.
    pub picker: Option<Picker>,
    /// The open menu's filter text.
    pub picker_filter: String,
    /// The computer's font families, once looked for (when a font menu
    /// first opens).
    pub system_fonts: Option<harlo_core::fonts::SystemFonts>,
    /// The open menu's highlighted row.
    pub picker_index: usize,
    /// How far the open menu's list is scrolled from its top.
    pub picker_scroll: f32,
    /// Answers being filled into questions, by question ID.
    pub drafts: HashMap<String, approvals::Draft>,
    /// Why a question's answers can't be sent yet, by question ID.
    pub problems: HashMap<String, Vec<approvals::Problem>>,
    /// The approval being refused with a reason: its request ID and the reason so far.
    pub refusing: Option<(String, String)>,
    /// Approvals and questions whose answer is on its way, by ID.
    pub answering: HashSet<String>,
    /// How much the next new session may do without asking.
    new_shield: Shield,
    /// Undos staged from here, and requests about them on their way.
    pub(crate) undos: undo::Undos,
    /// The Changes panel's lists and git actions.
    pub changes: Changes,
    /// The Files panel's trees and the open files.
    pub fs: Files,
    /// The notes opened so far.
    pub notes: Notes,
    /// The Context panel's view of the open session.
    pub context: ContextView,
    /// When to show desktop notifications.
    notices: notices::Notices,
    /// The last notification Harlo meant to show, whether or not it could.
    pub last_notice: Option<Notice>,
    /// The sound last asked for, an event's or a preview, by id; it plays
    /// only while notifications are shown, which tests leave off.
    pub last_sound: Option<&'static str>,
    /// Where the layout is saved; `None` keeps it in memory only.
    store: Option<Store>,
    /// Harlo's own settings.
    pub preferences: Preferences,
    /// How the Claude subscription stands.
    pub claude: Claude,
    /// The layout as last saved.
    saved: Layout,
    /// Tabs saved for the other locations.
    other_tabs: BTreeMap<String, Tabs>,
    /// Saved tabs to reopen once the current location connects.
    restore: Option<Tabs>,
    /// The connection to the current server; dropping it disconnects.
    following: Option<crate::task::Handle>,
    /// The connections offering Harlo's own tools to agents, by folder.
    own_tools: HashMap<String, crate::task::Handle>,
    /// Whether attaching waits, and why.
    own_tools_hold: own_tools::Hold,
    /// Why offering them last failed.
    pub own_tools_error: Option<String>,
    /// Dictation into the composer.
    pub voice: Voice,
    /// Local models: engines, their servers and downloads.
    pub local: LocalModels,
    /// Phone access: paired phones, the relay, the pairing code.
    pub phone: PhoneState,
    /// Harlo's own updates.
    pub updates: updates::Updates,
    /// Pictures' thumbnails, the viewer and the grid.
    pub pictures: PicturesState,
}

impl App {
    /// Creates the window's state, following `location`.
    ///
    /// `locations` are what the location picker offers; `order` is the saved
    /// sidebar order of project folders.
    ///
    /// `layout` arranges the window as it was last saved; its tabs for
    /// `location` reopen once the server connects.
    #[must_use]
    pub fn new(location: Location, locations: Vec<Location>, layout: Layout) -> Self {
        let terminal_range = (fit::TERMINAL_MIN, f32::MAX);
        let mut app = Self {
            tokens: theme::harlo(Mode::Dark).tokens(),
            themes: theme::bundled(),
            // Until the window says otherwise, as Harlo looked before it
            // followed the desktop.
            system_mode: harlo_core::theme::Mode::Dark,
            locations,
            location,
            status: Status::Connecting { attempt: 1 },
            picking_location: false,
            client: None,
            workspace: saved_workspace(&layout),
            projects: Vec::new(),
            now: SystemTime::now(),
            filter: String::new(),
            expanded: layout.expanded.iter().cloned().collect(),
            sidebar: layout.sidebar,
            panel: layout.panel.as_deref().and_then(Panel::named),
            maximized: None,
            terminals: Terminals::default(),
            per_session: PerSession::default(),
            sheets: Sheets::default(),
            width: fit::WINDOW.0,
            height: fit::WINDOW.1,
            keep: None,
            sidebar_width: size(layout.sidebar_width, fit::SIDEBAR, fit::SIDEBAR_RANGE),
            panel_width: size(layout.panel_width, fit::PANEL, fit::PANEL_RANGE),
            terminal_height: size(layout.terminal_height, fit::TERMINAL, terminal_range),
            resizing: false,
            tabs: Vec::new(),
            active: None,
            timelines: HashMap::new(),
            new_folder: None,
            home: home_folder(),
            config_folder: None,
            ssh_config: None,
            autostart: None,
            git_config: harlo_core::git::Global::new()
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1"),
            hosting: None,
            opens_pages: false,
            tray: false,
            composer: TextBox::new(),
            send_error: None,
            sending: None,
            pasting: None,
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
            model_defaults: std::collections::HashSet::new(),
            branches: std::collections::HashMap::new(),
            models_for: None,
            picker: None,
            picker_filter: String::new(),
            system_fonts: None,
            picker_index: 0,
            picker_scroll: 0.0,
            drafts: HashMap::new(),
            problems: HashMap::new(),
            refusing: None,
            answering: HashSet::new(),
            new_shield: Shield::Agent,
            undos: undo::Undos::default(),
            changes: Changes::default(),
            fs: Files::default(),
            notes: Notes::default(),
            context: ContextView::default(),
            notices: notices::Notices::new(false),
            last_notice: None,
            last_sound: None,
            store: None,
            other_tabs: layout.tabs.clone(),
            restore: None,
            preferences: Preferences::default(),
            claude: Claude::default(),
            following: None,
            own_tools: HashMap::new(),
            own_tools_hold: own_tools::Hold::None,
            own_tools_error: None,
            voice: Voice::default(),
            local: LocalModels::default(),
            phone: PhoneState::default(),
            updates: updates::Updates::default(),
            pictures: PicturesState::default(),
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

    /// Uses Harlo's own settings as saved last time.
    #[must_use]
    pub fn with_preferences(mut self, preferences: Preferences) -> Self {
        self.preferences = preferences;
        self.retheme();
        // The computers added in Settings → Server join the picker.
        for target in &self.preferences.computers {
            let location = Location::Ssh {
                target: target.clone(),
            };
            if !self.locations.contains(&location) {
                self.locations.push(location);
            }
        }
        self
    }

    /// Shows a desktop notification when a session needs the user while
    /// they're looking elsewhere.
    #[must_use]
    pub fn notifying(mut self) -> Self {
        self.notices.enabled = true;
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
            hidden: self.workspace.hidden().to_vec(),
            sidebar: self.sidebar,
            expanded,
            panel: self.panel.map(|p| p.title().to_owned()),
            sidebar_width: Some(pixels(self.sidebar_width)),
            panel_width: Some(pixels(self.panel_width)),
            terminal_height: Some(pixels(self.terminal_height)),
            tabs,
        }
    }

    /// Applies a message, then saves the layout if it changed.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // A menu with a filter has the keyboard; it goes back when it closes.
        let filtering = self.picker.filter(|p| p.filters());
        if matches!(
            message,
            Message::PickFolder(_)
                | Message::PickAgent(_)
                | Message::PickModel(_)
                | Message::PickVariant(_)
                | Message::PickShield(_)
                | Message::Sheet(
                    SheetEvent::DefaultModel(_)
                        | SheetEvent::Font { .. }
                        | SheetEvent::Project(ProjectEvent::Model(_))
                        | SheetEvent::GitSettings(GitEvent::CommitModel(_))
                )
        ) {
            self.picker = None;
        }
        let task = self.apply(message);
        let shown = self.follow_document();
        let refocus = match filtering {
            Some(picker) if self.picker.is_none() => Task::effect(Effect::Focus(
                if picker == model::Picker::Model && self.sheets.open.is_none() {
                    crate::ids::COMPOSER_TEXT
                } else {
                    crate::ids::WINDOW
                },
            )),
            _ => Task::none(),
        };
        let task = Task::batch([
            task,
            refocus,
            shown,
            self.follow_folder(),
            self.follow_changes(),
            self.follow_context(),
            self.follow_files(),
            self.follow_notes(),
            self.follow_own_tools(),
            self.follow_phone(),
            self.follow_pictures(),
            self.follow_branches(),
        ]);
        // While an edge is dragged, the layout is saved once, when it's let go.
        let layout = self.layout();
        if layout == self.saved || self.resizing {
            return task;
        }
        self.saved = layout.clone();
        match self.store.clone() {
            // A failed save leaves the previous layout; the next change tries again.
            Some(store) => Task::batch([task, Task::future(store.save_later(layout)).discard()]),
            None => task,
        }
    }

    /// Shows all of a folder's sessions in the sidebar, or only the newest.
    fn toggle_more(&mut self, folder: String) {
        if !self.expanded.remove(&folder) {
            self.expanded.insert(folder);
        }
    }

    /// Takes what a fetch loaded, and runs what that asks for.
    fn fetched(&mut self, result: Fetched) -> Task<Message> {
        let fetches = self.workspace.fetched(result);
        self.refresh();
        self.fetch(fetches)
    }

    /// Handles a message for one of the features Settings looks after.
    fn feature_event(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Claude(event) => self.claude_event(event),
            Message::OwnTools(event) => self.own_tools_event(event),
            Message::Voice(event) => self.voice_event(event),
            Message::Local(event) => self.local_event(event),
            Message::Phone(event) => self.phone_event(event),
            Message::Memory(event) => self.memory_event(event),
            Message::Pictures(event) => self.pictures_event(event),
            Message::Update(event) => self.update_event(event),
            _ => Task::none(),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one short arm per message kind, each handing it on"
    )]
    fn apply(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Quit => return Task::effect(Effect::Quit),
            Message::PickAvatar(index) => return self.pick_avatar(index),
            message @ (Message::ToggleSidebar
            | Message::Resized(..)
            | Message::OpenPanel(_)
            | Message::TogglePanel(_)
            | Message::ClosePanel
            | Message::Maximize(_)
            | Message::Restore
            | Message::Resize { .. }
            | Message::ResizeEnd
            | Message::ShowDocument(_)
            | Message::CloseDocument(_)
            | Message::CycleTab { .. }) => return self.arrange(message),
            Message::Terminal(event) => return self.terminal_event(event),
            Message::Sheet(event) => return self.sheet_event(event),
            Message::Aside(event) => return self.aside_event(event),
            Message::Job(event) => return self.job_event(event),
            Message::SessionMenu(action) => return self.session_menu(action),
            Message::Goal(event) => return self.goal_event(event),
            message @ (Message::Claude(_)
            | Message::OwnTools(_)
            | Message::Voice(_)
            | Message::Local(_)
            | Message::Phone(_)
            | Message::Memory(_)
            | Message::Pictures(_)
            | Message::Update(_)) => return self.feature_event(message),
            Message::SystemDark(dark) => self.set_system_dark(dark),
            Message::Filter(filter) => self.filter = filter,
            Message::ToggleMore(folder) => self.toggle_more(folder),
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
            | Message::CycleAgent { .. }
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
            message @ (Message::Approve { .. }
            | Message::Refuse(_)
            | Message::RefuseReason(_)
            | Message::RefuseSend
            | Message::RefuseCancel
            | Message::Answered(..)
            | Message::FormChoose { .. }
            | Message::FormType { .. }
            | Message::FormFlag { .. }
            | Message::FormSend(_)
            | Message::FormDismiss(_)
            | Message::OpenLink(_)
            | Message::PickShield(_)
            | Message::Focused(_)) => return self.waiting_action(message),
            Message::Changes(event) => return self.changes_action(event),
            Message::Files(event) => return self.files_action(event),
            Message::Notes(event) => return self.notes_action(event),
            Message::Context(event) => self.context_action(event),
            message @ (Message::UndoStaged(..)
            | Message::UndoChecked(..)
            | Message::UndoKeep(_)
            | Message::UndoCancel(_)
            | Message::UndoSettled { .. }) => return self.undo_action(message),
            Message::ToggleLocations => self.picking_location = !self.picking_location,
            Message::PickLocation(location) => return self.pick_location(location),
            message @ (Message::OpenSession(_)
            | Message::NoticeOpened(_)
            | Message::CloseTab(_)
            | Message::CloseActiveTab
            | Message::NewSession(_)) => return self.tab_action(message),
            Message::PickFolder(folder) => self.new_folder = folder,
            Message::Send => return self.send(None),
            Message::ContinueRun => return self.continue_run(),
            Message::Steer => return self.send(Some(Delivery::Steer)),
            Message::Timeline(id, event) => return self.timeline_event(id, event),
            Message::Server(update) => return self.server(update),
            Message::Fetched(Ok(result)) => return self.fetched(result),
            Message::Branch(folder, branch) => _ = self.branches.insert(folder, branch),
            // A failed fetch leaves the row as it was until the next event or
            // resync corrects it; after a fade, only the window's next showing
            // changes.
            Message::Fetched(Err(_)) | Message::Settled => {}
            Message::Tick(now) => return self.tick(now),
        }
        Task::none()
    }

    /// Moves the clock on to `now`: ages are measured from it.
    fn tick(&mut self, now: SystemTime) -> Task<Message> {
        self.now = now;
        self.refresh();
        self.claude_tick()
    }

    /// Applies something a session's timeline reported.
    fn timeline_event(&mut self, id: String, event: crate::timeline::Event) -> Task<Message> {
        match event {
            crate::timeline::Event::Undo(message_id) => return self.start_undo(id, message_id),
            crate::timeline::Event::OpenSession(child) => {
                return self.update(Message::OpenSession(child));
            }
            crate::timeline::Event::Review { tool, file } => {
                return self.review_edit(&id, &tool, file);
            }
            _ => {}
        }
        match self.timelines.get_mut(&id) {
            Some(timeline) => {
                timeline.update(&event);
                Task::none()
            }
            None => Task::none(),
        }
    }

    /// Applies a message from the composer: typing, its menu and attachments.
    fn compose(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Compose(edit) => {
                self.composer.edited(edit);
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
                    self.draft.attached.push(Attached::new(name, attachment));
                }
            }
            Message::Detach(index) => {
                if index < self.draft.attached.len() {
                    self.draft.attached.remove(index);
                }
            }
            Message::PasteImage => {
                self.pasting = Some(Instant::now());
                // A tick once the notice is due, so the window shows it.
                let notice = Task::future(async {
                    tokio::time::sleep(PASTE_NOTICE).await;
                    Message::Tick(SystemTime::now())
                });
                return Task::batch([
                    Task::perform(crate::clipboard::image(), Message::Pasted),
                    notice,
                ]);
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
                    self.composer.set("");
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
            Message::CycleAgent { back } => {
                let agents = self.agent_choices();
                if agents.len() < 2 {
                    return Task::none();
                }
                let current = self.current_agent();
                let at = agents
                    .iter()
                    .position(|a| Some(&a.id) == current.as_ref())
                    .unwrap_or(0);
                let next = if back {
                    at.checked_sub(1).unwrap_or(agents.len() - 1)
                } else {
                    (at + 1) % agents.len()
                };
                return self.update(Message::PickAgent(agents[next].id.clone()));
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
                    server: connected.server.clone(),
                    version: connected.version.clone(),
                };
                self.client = Some(connected.client.clone());
                let ask_host = self.ask_phone_host(connected.client.clone());
                self.own_tools_hold = own_tools::Hold::None;
                self.own_tools_error = None;
                self.branches.clear();
                self.workspace.resync(connected.snapshot.clone());
                self.forget_catalogs();
                // Tabs saved from last time reopen, unless the user turned
                // that off, then load below.
                if let Some(saved) = self.restore.take().filter(|_| self.preferences.reopen_tabs) {
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
                let mut reloads: Vec<Task<Message>> = self
                    .tabs
                    .clone()
                    .into_iter()
                    .map(|id| self.load(id))
                    .collect();
                // A page showing the server's set-up shows the new one's.
                if matches!(self.sheets.open, Some(Sheet::Settings | Sheet::Setup)) {
                    reloads.push(self.load_setup());
                }
                reloads.push(ask_host);
                return Task::batch(reloads);
            }
            Update::Event(event) => {
                let fetches = self.workspace.apply(&event.kind, event.time);
                let notice = self.notice(&event.kind);
                if let EventKind::SessionDeleted { session_id }
                | EventKind::SessionArchived { session_id } = &event.kind
                {
                    self.close(session_id);
                }
                self.refresh();
                self.changes_event(&event.kind);
                self.context_event(&event.kind);
                self.goal_changed(&event.kind);
                self.job_changed(&event.kind);
                let files = self.files_event(&event.kind);
                // A worktree's branch, as the server sees it change.
                if let EventKind::BranchChanged { directory, branch } = &event.kind
                    && self.branches.contains_key(directory)
                {
                    self.branches.insert(directory.clone(), branch.clone());
                }
                if matches!(event.kind, EventKind::CatalogChanged) {
                    // Reloaded for the current folder once this update ends.
                    self.catalogs.clear();
                }
                let mut reload = Task::none();
                if let Some(id) = event.kind.session_id() {
                    let busy = self.workspace.is_busy(id);
                    if let Some(timeline) = self.timelines.get_mut(id) {
                        let stale = timeline.apply(&event.kind, event.time);
                        timeline.set_busy(busy);
                        let fading = timeline.take_fading();
                        if stale {
                            reload = self.load_history(id.to_owned());
                        } else if fading {
                            reload = settle();
                        }
                    }
                }
                return Task::batch([self.fetch(fetches), reload, notice, files]);
            }
            Update::Lost { reason, retry_in } => {
                self.status = Status::Lost { reason, retry_in };
                self.client = None;
                self.withdraw_own_tools();
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

    /// Reads the branch of each worktree not read yet, for the sidebar's
    /// marks.
    fn follow_branches(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        for worktree in self.projects.iter().flat_map(|p| &p.worktrees) {
            if self.branches.contains_key(&worktree.directory) {
                continue;
            }
            // Asked once; the answer replaces this.
            self.branches.insert(worktree.directory.clone(), None);
            let (client, folder) = (client.clone(), worktree.directory.clone());
            tasks.push(Task::perform(
                async move {
                    let branch = client.branch(&folder).await.ok().flatten();
                    (folder, branch)
                },
                |(folder, branch)| Message::Branch(folder, branch),
            ));
        }
        Task::batch(tasks)
    }

    /// Returns the branch checked out in the worktree at `folder`, once
    /// read.
    #[must_use]
    pub fn branch_of(&self, folder: &str) -> Option<&str> {
        self.branches.get(folder)?.as_deref()
    }

    /// Rebuilds the sidebar from the workspace.
    fn refresh(&mut self) {
        // A session started without a project runs in the home folder.
        self.workspace.set_other_folder(Some(self.home.clone()));
        self.projects = self.workspace.sidebar(self.now);
    }

    /// Shows a session, opening a tab after the current one if it has none.
    fn open(&mut self, id: String) -> Task<Message> {
        self.send_error = None;
        self.fs.shown = None;
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

    /// Opens, closes and switches tabs, and goes to the start screen.
    fn tab_action(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenSession(id) => return self.open(id),
            Message::NoticeOpened(id) => return self.notice_opened(id),
            Message::NewSession(folder) => {
                self.fs.shown = None;
                self.active = None;
                if folder.is_some() {
                    self.new_folder = folder;
                }
                self.send_error = None;
            }
            Message::CloseTab(id) => self.close(&id),
            Message::CloseActiveTab => {
                if let Some(key) = self.fs.shown.clone() {
                    return self.files_action(FilesEvent::Close(key));
                }
                if let Some(id) = self.active.clone() {
                    self.close(&id);
                }
            }
            _ => {}
        }
        Task::none()
    }

    /// Loads a session's history into its timeline, its inbox, what it's
    /// waiting to have approved, and the sessions it handed work to.
    fn load(&self, id: String) -> Task<Message> {
        Task::batch([
            self.fetch(vec![
                Fetch::Inbox(id.clone()),
                Fetch::Permissions(id.clone()),
                Fetch::Forms(id.clone()),
                Fetch::Children(id.clone()),
            ]),
            self.load_goal(id.clone()),
            self.load_jobs(id.clone()),
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

    /// Switches to another server, keeping the tabs of the one left for later.
    fn pick_location(&mut self, location: Location) -> Task<Message> {
        self.picking_location = false;
        if location != self.location {
            // Following the new location drops the old connection, which
            // detaches from it.
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
            self.withdraw_own_tools();
            self.client = None;
            self.active = None;
            self.tabs.clear();
            self.timelines.clear();
            // Files belong to the server they were read from.
            self.fs = Files::default();
            self.forget_catalogs();
            let hidden = self.workspace.hidden().to_vec();
            self.workspace = Workspace::new(self.workspace.order().to_vec());
            self.workspace.set_hidden(hidden);
            self.refresh();
            return self.follow_location();
        }
        Task::none()
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
        let pictures: Vec<_> = (self.draft.attached.iter())
            .filter_map(|a| a.picture.clone())
            .collect();
        let mut queued = None;
        let mut scroll = Task::none();
        if let (Some(session), Some((key, said, _))) = (target, &prompt) {
            if self.workspace.is_busy(session) {
                queued = Some(InboxItem {
                    id: key.clone(),
                    kind: "user".to_owned(),
                    text: Some(said.clone()),
                    attachments: attachments.clone(),
                    pictures: pictures.clone(),
                    skills: skills.clone(),
                    delivery,
                });
            } else if let Some(timeline) = self.timelines.get_mut(session) {
                let entry = Entry::Prompt {
                    key: key.clone(),
                    text: said.clone(),
                    attachments,
                    pictures,
                    skills,
                    agent: None,
                };
                timeline.pend(entry);
                let fading = timeline.take_fading();
                scroll = Task::effect(Effect::ToEnd(crate::ids::TIMELINE));
                if fading {
                    scroll = Task::batch([scroll, settle()]);
                }
            }
        }
        let draft = if target.is_some() {
            self.composer.set("");
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
            self.composer.set(&sent.text);
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

    /// Returns the composer's feedback: a send on its way, a paste being read.
    ///
    /// A paste being read shows only once it takes a moment.
    #[must_use]
    pub fn feedback(&self) -> Feedback {
        Feedback {
            sending: self.sending.is_some(),
            pasting: self
                .pasting
                .is_some_and(|since| since.elapsed() >= PASTE_NOTICE),
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
        self.check_undo(id)
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
            self.send_error = Some("Not connected to Harlo's server yet.".to_owned());
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
        if let Outgoing::Aside(question) = &outgoing {
            return self.ask_aside(question);
        }
        let target = self.active.clone();
        if target.is_none() && outgoing == Outgoing::Compact {
            self.send_error = Some("There's nothing to summarize yet.".to_owned());
            return Task::none();
        }
        self.send_error = None;
        self.menu = None;
        let scroll = self.show_sending(target.as_deref(), &text, &outgoing, delivery);
        let new = target.is_none().then(|| NewChoices {
            agent: self.new_agent.clone(),
            model: self
                .new_model
                .clone()
                .or_else(|| self.project_model(&folder)),
            rules: self.new_shield.rules().filter(|rules| !rules.is_empty()),
        });
        let command = match &outgoing {
            Outgoing::Shell(command) => Some(command.clone()),
            _ => None,
        };
        let sent = send(client.clone(), target, folder, new, outgoing, delivery);
        let task = match command {
            None => Task::perform(sent, Message::Sent),
            Some(command) => shell(client, sent, command),
        };
        Task::batch([task, scroll])
    }

    /// Sends "Continue" to the open session, keeping what the composer
    /// holds: sending empties the composer, so it's put back after.
    fn continue_run(&mut self) -> Task<Message> {
        if self.active.is_none() || self.sending.is_some() {
            return Task::none();
        }
        let kept = self.composer.clone();
        let draft = std::mem::take(&mut self.draft);
        self.composer.set(CONTINUE);
        let task = self.send(None);
        self.composer = kept;
        self.draft = draft;
        task
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
        (self.draft.attached).push(Attached::new(
            &name,
            Attachment::inline("image/png", png, &name),
        ));
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
            .unwrap_or_default()
            .to_owned();
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
        Task::effect(Effect::ScrollTo {
            list: MENU_SCROLLER,
            y,
        })
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
                self.picker_index = model::items(self, picker)
                    .iter()
                    .position(|item| item.selected)
                    .unwrap_or(0);
                let reveal = self.reveal_highlight();
                let fonts = if matches!(picker, Picker::InterfaceFont | Picker::CodeFont)
                    && self.system_fonts.is_none()
                {
                    Task::perform(
                        async {
                            tokio::task::spawn_blocking(harlo_core::fonts::SystemFonts::load)
                                .await
                                .unwrap_or_default()
                        },
                        |fonts| Message::Sheet(SheetEvent::SystemFonts(fonts)),
                    )
                } else {
                    Task::none()
                };
                if picker.filters() {
                    return Task::batch([
                        reveal,
                        fonts,
                        Task::effect(Effect::Focus(model::MENU_FILTER)),
                    ]);
                }
                return Task::batch([reveal, fonts]);
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
                let len = i64::try_from(model::items(self, picker).len()).unwrap_or(0);
                if len > 0 {
                    let at = i64::try_from(self.picker_index).unwrap_or(0) + i64::from(by);
                    self.picker_index = usize::try_from(at.rem_euclid(len)).unwrap_or(0);
                    return self.reveal_highlight();
                }
            }
            Message::PickerSubmit => {
                if let Some(item) = self.picker.and_then(|picker| {
                    model::items(self, picker)
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
                // A new session there makes it a project; one removed from
                // Harlo comes back now.
                let folder = folder.to_string_lossy().into_owned();
                if self.workspace.hidden().contains(&folder) {
                    self.workspace.add_project(folder.clone());
                    self.refresh();
                }
                self.active = None;
                self.new_folder = Some(folder);
            }
            _ => {}
        }
        Task::none()
    }

    /// Scrolls the open menu's list so its highlighted row shows.
    fn reveal_highlight(&mut self) -> Task<Message> {
        let step = model::MENU_ROW + model::ROW_GAP;
        let index = u16::try_from(self.picker_index).unwrap_or(u16::MAX);
        let rows = u16::try_from(model::MENU_ROWS).unwrap_or(u16::MAX);
        let top = f32::from(index) * step;
        let shown = f32::from(rows) * step;
        if top < self.picker_scroll {
            self.picker_scroll = top;
        } else if top + step > self.picker_scroll + shown {
            self.picker_scroll = top + step - shown;
        }
        Task::effect(Effect::ScrollTo {
            list: model::MENU_LIST,
            y: self.picker_scroll,
        })
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
            Choice::Command(_) | Choice::Skill(_) | Choice::Aside => {}
        }
        self.composer.move_to(Cursor {
            position: Position {
                line: trigger.line,
                column: trigger.end,
            },
            selection: Some(Position {
                line: trigger.line,
                column: trigger.start,
            }),
        });
        self.composer.paste(&insert);
        self.refresh_menu()
    }

    /// The folder the composer works in: the open session's, or the new session's.
    #[must_use]
    pub fn folder(&self) -> String {
        match self
            .active
            .as_deref()
            .and_then(|id| self.workspace.session(id))
        {
            Some(session) => session.directory.clone(),
            None => self.new_folder.clone().unwrap_or_else(|| self.home.clone()),
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
        let catalog = self
            .models_for
            .as_ref()
            .and_then(|folder| self.catalogs.get(folder));
        self.model_defaults = catalog
            .map(|c| harlo_core::model_menu::shown_by_default(&c.models, self.now))
            .unwrap_or_default();
        let models = catalog
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

    /// Returns whether the model menus offer the model `id` of `provider`:
    /// as chosen in Settings → Models, else when it's recent.
    #[must_use]
    pub fn model_shown(&self, provider: &str, id: &str) -> bool {
        harlo_core::model_menu::shown(
            &self.preferences.models,
            &self.model_defaults,
            &harlo_core::model_menu::key(provider, id),
        )
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
            .map(|c| c.primary_agents().map(agent_choice).collect())
            .unwrap_or_default()
    }

    /// Any agent of the current folder, subagents included, by ID.
    #[must_use]
    pub fn agent(&self, id: &str) -> Option<AgentChoice> {
        self.catalog()?
            .agents
            .iter()
            .find(|a| a.id == id)
            .map(agent_choice)
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
            None => self
                .new_model
                .clone()
                .or_else(|| self.project_model(&self.folder())),
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

    /// The model new sessions in the current folder use unless one is
    /// picked for them: The server's configured default.
    #[must_use]
    pub fn default_model_choice(&self) -> Option<ModelChoice> {
        let model = self.catalog()?.default_model.as_ref()?;
        Some(ModelChoice {
            provider_id: model.provider_id.clone(),
            id: model.id.clone(),
            name: model.name.clone(),
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
        for project in self.projects.iter().filter(|p| !p.other) {
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

    /// A session's timeline, once its tab is open.
    #[must_use]
    pub fn timeline(&self, id: &str) -> Option<&Timeline> {
        self.timelines.get(id)
    }

    /// The folder a session works in.
    #[must_use]
    pub fn session_folder(&self, id: &str) -> Option<String> {
        self.workspace.session(id).map(|s| s.directory.clone())
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

    /// Starts following the current server, and the clock for session
    /// ages; the window runs this once it opens.
    pub fn start(&mut self) -> Task<Message> {
        let ticks = futures_util::stream::unfold((), |()| async {
            tokio::time::sleep(TICK).await;
            Some((Message::Tick(SystemTime::now()), ()))
        });
        Task::batch([
            self.follow_location(),
            Task::stream(ticks),
            claude::watch_usage(),
            self.start_local_servers(),
            self.update_timer(),
        ])
    }

    /// Follows the current server, dropping the connection to any other.
    fn follow_location(&mut self) -> Task<Message> {
        let (task, handle) =
            Task::run(server::follow(self.location.clone()), Message::Server).abortable();
        self.following = Some(handle.abort_on_drop());
        task
    }
}

/// Maps a key press to a window-wide shortcut: a new session, closing or
/// cycling tabs, and Settings.
#[must_use]
pub fn shortcut(press: &KeyPress) -> Option<Message> {
    if !press.command {
        return None;
    }
    match &press.key {
        Key::Character('n') => Some(Message::NewSession(None)),
        Key::Character('w') => Some(Message::CloseActiveTab),
        Key::Character('q') => Some(Message::Quit),
        Key::Character(',') => Some(Message::Sheet(SheetEvent::Toggle(Sheet::Settings))),
        Key::Tab => Some(Message::CycleTab { back: press.shift }),
        Key::PageDown => Some(Message::CycleTab { back: false }),
        Key::PageUp => Some(Message::CycleTab { back: true }),
        Key::Character('d') if press.shift => Some(Message::Voice(VoiceEvent::Toggle)),
        _ => None,
    }
}

/// Maps Escape, while a panel is maximized, to putting it back.
#[must_use]
pub fn restore_key(press: &KeyPress) -> Option<Message> {
    (press.key == Key::Escape).then_some(Message::Restore)
}

/// Maps a key press to moving through or choosing from an open picker's menu.
#[must_use]
pub fn menu_key(press: &KeyPress) -> Option<Message> {
    if press.command || press.alt {
        return None;
    }
    match press.key {
        Key::Up => Some(Message::PickerMove(-1)),
        Key::Down => Some(Message::PickerMove(1)),
        Key::Enter => Some(Message::PickerSubmit),
        _ => None,
    }
}

/// Maps a key press in the focused composer to what it does; `None` leaves
/// it to the text field.
///
/// With the `/` or `@` menu open, the arrow keys move through it, Enter or
/// Tab picks, and Escape closes it. Otherwise Tab switches agents, Enter
/// sends, Ctrl+Enter steers a working session, and Ctrl+V attaches an image
/// on the clipboard (the field still pastes any text).
#[must_use]
pub fn compose_key(press: &KeyPress, menu_open: bool) -> Option<Message> {
    match press.key {
        Key::Up if menu_open => Some(Message::MenuMove(-1)),
        Key::Down if menu_open => Some(Message::MenuMove(1)),
        Key::Enter | Key::Tab if menu_open && !press.shift => Some(Message::MenuPick(None)),
        Key::Escape if menu_open => Some(Message::MenuClose),
        Key::Tab if !press.command && !press.alt => Some(Message::CycleAgent { back: press.shift }),
        Key::Enter if press.command => Some(Message::Steer),
        Key::Enter if !press.shift && !press.alt => Some(Message::Send),
        Key::Character('v') if press.command && !press.shift => Some(Message::PasteImage),
        _ => None,
    }
}

/// An agent as the pickers and headers show it.
fn agent_choice(agent: &server::Agent) -> AgentChoice {
    AgentChoice {
        id: agent.id.clone(),
        name: agent.name.clone(),
        description: agent.description.clone(),
        color: agent.color.as_deref().and_then(theme::Rgba::parse),
        read_only: agent.read_only,
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
/// Waits out a fade, then has the window show the rows once more, still.
fn settle() -> Task<Message> {
    Task::future(async {
        tokio::time::sleep(crate::timeline::APPEAR).await;
        Message::Settled
    })
}

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

/// What a new session gets before its first prompt, as chosen on the start screen.
struct NewChoices {
    /// Its agent.
    agent: Option<String>,
    /// Its model.
    model: Option<ModelRef>,
    /// Its own permission rules, from the shield.
    rules: Option<Vec<server::Rule>>,
}

/// Sends `outgoing` to `target`, or creates a session in `folder` and sends it there.
///
/// A new session first gets the choices in `new`.
/// Runs `command` once `sent` has the session ready.
///
/// A command can run for long, and the server answers only once it ends, so
/// the send is done as soon as the session is there, and the composer is
/// free meanwhile. The command's row comes from the server's events.
fn shell(
    client: Client,
    sent: impl Future<Output = Result<Box<Session>, String>> + Send + 'static,
    command: String,
) -> Task<Message> {
    use futures_util::StreamExt as _;
    let steps = futures_util::stream::once(sent).flat_map(move |result| {
        let run = result.as_ref().ok().map(|session| {
            let (client, id, command) = (client.clone(), session.id.clone(), command.clone());
            async move {
                Message::Acted(
                    client
                        .run_shell(&id, &command)
                        .await
                        .map_err(|e| format!("Couldn't run the command: {e}")),
                )
            }
        });
        futures_util::stream::once(std::future::ready(Message::Sent(result)))
            .chain(futures_util::stream::iter(run).then(|run| run))
    });
    Task::stream(steps)
}

async fn send(
    client: Client,
    target: Option<String>,
    folder: String,
    new: Option<NewChoices>,
    outgoing: Outgoing,
    delivery: Delivery,
) -> Result<Box<Session>, String> {
    let failed = |e: server::Error| format!("Couldn't send: {e}");
    let session = match target {
        Some(id) => client.session(&id).await,
        None => client.create_session(&folder).await,
    }
    .map_err(failed)?;
    if let Some(NewChoices {
        agent,
        model,
        rules,
    }) = new
    {
        if let Some(rules) = rules {
            client
                .set_permissions(&session.id, &rules)
                .await
                .map_err(failed)?;
        }
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
        // A side question is asked apart, and a command runs once the
        // session is there (see `shell`).
        Outgoing::Aside(_) | Outgoing::Shell(_) => Ok(()),
    }
    .map_err(failed)?;
    Ok(Box::new(session))
}

/// The sessions' workspace, in the sidebar order and with the projects
/// hidden as `layout` saved them.
fn saved_workspace(layout: &Layout) -> Workspace {
    let mut workspace = Workspace::new(layout.order.clone());
    workspace.set_hidden(layout.hidden.clone());
    workspace
}
