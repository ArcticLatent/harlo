//! Runs the window: builds the app, connects the window's callbacks to
//! messages, runs the work the app asks for, and shows the result.
//!
//! Everything about the app happens on the window's thread. A callback turns
//! into a [`Message`] and goes through [`App::update`] at once; the tasks it
//! returns run on the [`Runtime`]'s pool, and their messages and effects come
//! back over a channel. After each batch, the window shows the app as it is.

// Rust guideline compliant 2026-10-09

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::rc::Rc;

use harlo_core::hosting::Service;
use harlo_core::layout::Store;
use harlo_core::server::{Location, PermissionReply};
use slint::ComponentHandle as _;
use slint::winit_030::{EventResult, WinitWindowAccessor as _, winit::event::WindowEvent};

use crate::app::Load;
use crate::app::{
    self, App, ChangesEvent, ContextEvent, Edge, FilesEvent, Message, NotesEvent, Panel,
    ProjectEvent, ProviderEvent, ScheduleEvent, SchedulePage, SheetEvent, TerminalEvent,
};
use crate::keys::KeyPress;
use crate::model::{self, Picker};
use crate::options::Options;
use crate::present::{self, Presenter};
use crate::runtime::Runtime;
use crate::task::{Action, Effect};
use crate::term::{self, Key, Mods};
use crate::text::Edit;
use crate::timeline::Event as TimelineEvent;
use crate::ui::{AppWindow, Logic, Requests};

/// Why the window couldn't run.
#[derive(Debug)]
pub enum RunError {
    /// The window couldn't be made, or its event loop failed.
    Window(slint::PlatformError),
    /// The pool for background work couldn't start.
    Runtime(std::io::Error),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Window(e) => write!(f, "the window failed: {e}"),
            Self::Runtime(e) => write!(f, "couldn't start background work: {e}"),
        }
    }
}

impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Window(e) => Some(e),
            Self::Runtime(e) => Some(e),
        }
    }
}

impl From<slint::PlatformError> for RunError {
    fn from(e: slint::PlatformError) -> Self {
        Self::Window(e)
    }
}

/// The app with what runs it, shared by the window's callbacks.
struct Shell {
    app: App,
    /// Runs the app's tasks; tests leave it out, and the tasks wait in
    /// `kept` for the test to run or drop.
    runtime: Option<Runtime<Message>>,
    kept: Vec<crate::task::Task<Message>>,
    presenter: Presenter,
    window: slint::Weak<AppWindow>,
    /// The tray icon, while the desktop shows one; tests have none.
    tray: Option<crate::tray::Tray>,
}

/// The shell, and messages that arrived while it was busy.
///
/// A callback can fire while the window is being updated (setting a field's
/// text moves its cursor, say). Its message waits here, and the update in
/// progress takes it before it finishes.
struct Controller {
    shell: RefCell<Shell>,
    waiting: RefCell<VecDeque<Message>>,
    /// Every message dispatched, kept only for tests.
    log: Option<RefCell<Vec<Message>>>,
}

impl Controller {
    /// Applies `message` and every message waiting, then shows the result.
    fn dispatch(&self, message: Message) {
        if let Some(log) = &self.log {
            log.borrow_mut().push(message.clone());
        }
        self.waiting.borrow_mut().push_back(message);
        let Ok(mut shell) = self.shell.try_borrow_mut() else {
            // The update in progress takes it.
            return;
        };
        shell.drain(&self.waiting);
    }

    /// Makes a message from the app as it is, if there is one, and applies it.
    fn dispatch_with(&self, make: impl FnOnce(&App) -> Option<Message>) {
        let message = match self.shell.try_borrow() {
            Ok(shell) => make(&shell.app),
            Err(_) => None,
        };
        if let Some(message) = message {
            self.dispatch(message);
        }
    }

    /// Applies what came back from background work, then shows the result.
    fn receive(&self, actions: Vec<Action<Message>>) {
        let mut effects = Vec::new();
        for action in actions {
            match action {
                Action::Output(message) => self.waiting.borrow_mut().push_back(message),
                Action::Effect(effect) => effects.push(effect),
            }
        }
        if let Ok(mut shell) = self.shell.try_borrow_mut() {
            shell.drain(&self.waiting);
            if let Some(window) = shell.window.upgrade() {
                for effect in effects {
                    perform(&window, effect);
                }
            }
        }
    }
}

impl Shell {
    /// Applies the waiting messages, shows the app, and repeats while
    /// showing it brought more.
    fn drain(&mut self, waiting: &RefCell<VecDeque<Message>>) {
        loop {
            loop {
                let next = waiting.borrow_mut().pop_front();
                let Some(message) = next else {
                    break;
                };
                if *TRACE {
                    eprintln!("harlo: {}", kind(&message));
                }
                let task = self.app.update(message);
                if let Some(runtime) = &self.runtime {
                    runtime.spawn(task);
                } else if !task.is_none() {
                    self.kept.push(task);
                }
            }
            if let Some(window) = self.window.upgrade() {
                self.presenter.present(&self.app, &window);
            }
            if let Some(tray) = &mut self.tray {
                tray.show(crate::tray::summary(&self.app));
            }
            if waiting.borrow().is_empty() {
                break;
            }
        }
    }
}

/// Whether to print each message's kind to stderr (`HARLO_TRACE=1`), to see
/// what the window was doing around Slint's own warnings.
static TRACE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var_os("HARLO_TRACE").is_some_and(|v| v == "1"));

/// A message's kind: its name, without what it carries.
fn kind(message: &Message) -> String {
    let full = format!("{message:?}");
    let end = full.find(['(', ' ', '{']).unwrap_or(full.len());
    full[..end].to_owned()
}

/// Carries out an effect in the window.
fn perform(window: &AppWindow, effect: Effect) {
    let requests = window.global::<Requests>();
    match effect {
        Effect::Focus(target) => {
            requests.set_focus(target.into());
            requests.set_focus_serial(requests.get_focus_serial().wrapping_add(1));
        }
        Effect::ScrollTo { list, y } => {
            requests.set_scroll(list.into());
            requests.set_scroll_y(y);
            requests.set_scroll_serial(requests.get_scroll_serial().wrapping_add(1));
        }
        Effect::ToEnd(list) | Effect::Reveal(list) => {
            requests.set_scroll(list.into());
            requests.set_scroll_y(f32::INFINITY);
            requests.set_scroll_serial(requests.get_scroll_serial().wrapping_add(1));
        }
        Effect::Raise => raise(window),
        Effect::ShowInFolder(path) => {
            // A folder that can't be opened leaves nothing to show.
            if let Some(folder) = path.parent() {
                let _ = open::that_detached(folder);
            }
        }
        Effect::Quit => {
            // Fails only once the event loop has ended, which is the aim.
            let _ = slint::quit_event_loop();
        }
        Effect::Play(sound) => crate::sounds::play(sound),
    }
}

/// Builds the app as `options` say: the servers to follow, and the layout
/// saved last time.
fn boot(options: &Options) -> App {
    let locations = options.locations();
    let location = locations.last().cloned().unwrap_or(Location::Shared);
    // A missing or unreadable layout starts from the default one.
    let store = Store::user();
    let layout = store
        .as_ref()
        .and_then(|store| store.load().ok())
        .unwrap_or_default();
    let preferences = store
        .as_ref()
        .and_then(|store| store.load_preferences().ok())
        .unwrap_or_default();
    let mut app = App::new(location, locations, layout)
        .with_preferences(preferences)
        .notifying();
    app.opencode_folder = harlo_core::config::folder();
    app.ssh_config = harlo_core::ssh::config_file();
    app.git_config = harlo_core::git::Global::new();
    app.hosting = Some(harlo_core::hosting::Hosting::new());
    app.opens_pages = true;
    app.autostart = harlo_core::autostart::folder()
        .zip(std::env::current_exe().ok())
        .map(|(folder, program)| app::Autostart {
            on: harlo_core::autostart::enabled(&folder),
            folder,
            program,
        });
    match store {
        Some(store) => app.saving_to(store),
        None => app,
    }
}

/// Opens the window and runs until it closes.
///
/// # Errors
///
/// Returns an error if the window can't be made, its event loop fails, or
/// the pool for background work can't start.
pub fn run(options: &Options) -> Result<(), RunError> {
    // A Harlo already running shows its window instead.
    let socket = crate::instance::socket_path();
    if let Some(path) = &socket
        && let Ok(Some(pid)) = crate::instance::forward(
            path,
            if options.setup {
                crate::instance::Command::Setup
            } else {
                crate::instance::Command::Show
            },
        )
    {
        eprintln!("harlo: already running (process {pid}); showed its window.");
        return Ok(());
    }
    let window = AppWindow::new()?;
    let (runtime, mut actions) = Runtime::new().map_err(RunError::Runtime)?;
    // What's chosen in the tray, or asked by a later launch, arrives as
    // background work's results do.
    let sender = runtime.sender();
    let later = runtime.sender();
    let tray = crate::tray::Tray::start(move |choice| {
        let actions = match choice {
            crate::tray::Choice::Show => vec![Action::Effect(Effect::Raise)],
            crate::tray::Choice::Open(id) => vec![
                Action::Effect(Effect::Raise),
                Action::Output(Message::OpenSession(id)),
            ],
            crate::tray::Choice::Quit => vec![Action::Effect(Effect::Quit)],
        };
        for action in actions {
            // Fails only once the window has gone.
            let _ = sender.send(action);
        }
    })
    .ok();
    let mut app = boot(options);
    app.tray = tray.is_some();
    crate::theme::apply(&window, &app.tokens, &app.theme().id);
    let start = app.start();
    runtime.spawn(start);
    let presenter = Presenter::new(&window);
    let controller = Rc::new(Controller {
        shell: RefCell::new(Shell {
            app,
            runtime: Some(runtime),
            kept: Vec::new(),
            presenter,
            window: window.as_weak(),
            tray,
        }),
        waiting: RefCell::new(VecDeque::new()),
        log: None,
    });
    connect(&window, &controller);
    let focus = Rc::clone(&controller);
    window.window().on_winit_window_event(move |_, event| {
        if let WindowEvent::Focused(focused) = event {
            focus.dispatch(Message::Focused(*focused));
        }
        EventResult::Propagate
    });
    let receiver = Rc::clone(&controller);
    slint::spawn_local(async move {
        while let Some(action) = actions.recv().await {
            let mut batch = vec![action];
            while let Ok(more) = actions.try_recv() {
                batch.push(more);
            }
            receiver.receive(batch);
        }
    })
    .map_err(|e| RunError::Window(slint::PlatformError::Other(e.to_string())))?;
    // Later launches come to this window. Without the socket Harlo still
    // runs; a later launch then starts another one.
    let _listener = socket.and_then(|path| {
        crate::instance::listen(&path, move |command| {
            let mut actions = vec![Action::Effect(Effect::Raise)];
            if command == crate::instance::Command::Setup {
                actions.push(Action::Output(Message::Sheet(SheetEvent::OpenSetup)));
            }
            for action in actions {
                // Fails only once the window has gone.
                let _ = later.send(action);
            }
        })
        .ok()
    });
    // The guide opens on the first start, and for `--setup`.
    if options.setup || !controller.shell.borrow().app.preferences.setup_done {
        controller.dispatch(Message::Sheet(SheetEvent::OpenSetup));
    }
    // Closing the window leaves Harlo in the tray, when there is one and
    // the user wants it; otherwise it quits.
    let closing = Rc::clone(&controller);
    window.window().on_close_requested(move || {
        let keep = closing
            .shell
            .try_borrow()
            .is_ok_and(|shell| shell.app.tray && shell.app.preferences.keep_in_tray);
        if !keep {
            // Fails only once the event loop has ended.
            let _ = slint::quit_event_loop();
        }
        slint::CloseRequestResponse::HideWindow
    });
    // Show the app before the first message arrives.
    controller.dispatch(Message::Resized(crate::fit::WINDOW.0, crate::fit::WINDOW.1));
    window.show()?;
    slint::run_event_loop_until_quit()?;
    Ok(())
}

/// Shows `window` again, out of the taskbar if minimized, and asks the
/// desktop to focus it (which a desktop may decline, flashing it instead).
fn raise(window: &AppWindow) {
    // Showing a shown window does nothing; a failure leaves it as it was.
    let _ = window.show();
    window.window().with_winit_window(|winit| {
        winit.set_minimized(false);
        winit.focus_window();
    });
}

/// Drives a window from an app without running the app's tasks, and keeps
/// every message the window sends: for UI tests.
///
/// The window's callbacks and the shown state are the real ones; the
/// requests to OpenCode and the disk that the app's tasks would make
/// don't happen unless the test takes the tasks and runs them.
#[derive(Clone)]
pub struct Driver {
    controller: Rc<Controller>,
}

impl fmt::Debug for Driver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Driver").finish_non_exhaustive()
    }
}

impl Driver {
    /// Connects `window` to `app` and shows it at `size`.
    #[must_use]
    pub fn new(window: &AppWindow, app: App, size: (f32, f32)) -> Self {
        crate::theme::apply(window, &app.tokens, &app.theme().id);
        let presenter = Presenter::new(window);
        let controller = Rc::new(Controller {
            shell: RefCell::new(Shell {
                app,
                runtime: None,
                kept: Vec::new(),
                presenter,
                window: window.as_weak(),
                tray: None,
            }),
            waiting: RefCell::new(VecDeque::new()),
            log: Some(RefCell::new(Vec::new())),
        });
        connect(window, &controller);
        window
            .window()
            .set_size(slint::LogicalSize::new(size.0, size.1));
        controller.dispatch(Message::Resized(size.0, size.1));
        let driver = Self { controller };
        let _ = driver.take_messages();
        driver
    }

    /// The app as it is now.
    ///
    /// # Panics
    ///
    /// Panics if called from inside a window callback, while the app updates.
    #[must_use]
    pub fn app(&self) -> std::cell::Ref<'_, App> {
        std::cell::Ref::map(self.controller.shell.borrow(), |shell| &shell.app)
    }

    /// Applies `message` as if a task had sent it, and shows the result.
    pub fn send(&self, message: Message) {
        self.controller.dispatch(message);
    }

    /// Carries out `effect` on the window, as the window's runtime does
    /// when a task asks for it.
    pub fn perform(&self, effect: Effect) {
        let window = self.controller.shell.borrow().window.upgrade();
        if let Some(window) = window {
            perform(&window, effect);
        }
    }

    /// The tasks the app started since the last call, oldest first, for
    /// the test to run.
    #[must_use]
    pub fn take_tasks(&self) -> Vec<crate::task::Task<Message>> {
        std::mem::take(&mut self.controller.shell.borrow_mut().kept)
    }

    /// The messages sent since the last call, oldest first.
    #[must_use]
    pub fn take_messages(&self) -> Vec<Message> {
        self.controller
            .log
            .as_ref()
            .map(|log| std::mem::take(&mut *log.borrow_mut()))
            .unwrap_or_default()
    }
}

/// The center tab at `index`, as the UI numbers its tabs.
fn document(app: &App, index: i32) -> Option<app::Document> {
    app.documents()
        .into_iter()
        .nth(usize::try_from(index).ok()?)
}

/// The page over the window the UI numbers `number`.
fn sheet_of(number: i32) -> Option<app::Sheet> {
    match number {
        1 => Some(app::Sheet::Stats),
        2 => Some(app::Sheet::Settings),
        _ => None,
    }
}

/// The section of Settings the UI numbers `number`.
fn section_of(number: i32) -> Option<app::Section> {
    app::Section::ALL
        .get(usize::try_from(number).ok()?)
        .copied()
}

/// The Claude subscription step the UI numbers `number`.
fn claude_action_of(number: i32) -> Option<app::ClaudeAction> {
    match number {
        0 => Some(app::ClaudeAction::Install),
        1 => Some(app::ClaudeAction::SignIn),
        2 => Some(app::ClaudeAction::Connect),
        3 => Some(app::ClaudeAction::Disconnect),
        4 => Some(app::ClaudeAction::Check),
        _ => None,
    }
}

/// The Settings action that opens the menu of models for new sessions;
/// the Claude actions number from 0.
pub const CHOOSE_DEFAULT_MODEL: i32 = 10;

/// The Settings button that opens the setup guide.
pub const OPEN_SETUP: i32 = 11;

/// The setup guide's button that installs OpenCode.
pub const INSTALL_OPENCODE: i32 = 12;

/// The Settings button that sends a test notification.
pub const TEST_NOTICE: i32 = 13;

/// The Settings button that opens the menu of models for the shown
/// project's new sessions.
pub const CHOOSE_PROJECT_MODEL: i32 = 14;

/// The Settings button that lets OpenCode choose the shown project's model
/// again.
pub const DEFAULT_PROJECT_MODEL: i32 = 15;

/// The Settings button that chooses an image for the shown project's icon.
pub const CHOOSE_ICON: i32 = 16;

/// The Settings button that removes the shown project's icon image.
pub const CLEAR_ICON: i32 = 17;

/// The Settings button that removes one of the shown project's worktrees,
/// by its place.
pub const REMOVE_WORKTREE: i32 = 18;

/// The Settings button that removes a worktree with changes, by its place.
pub const FORCE_REMOVE_WORKTREE: i32 = 19;

/// The Settings button that removes the shown project from Harlo.
pub const HIDE_PROJECT: i32 = 20;

/// A project in Settings → Projects' list, by its place, which shows its
/// settings.
pub const PICK_PROJECT: i32 = 21;

/// The Settings button that goes back to the list of projects.
pub const ALL_PROJECTS: i32 = 22;

/// The Settings button that chooses the model writing commit messages.
pub const CHOOSE_COMMIT_MODEL: i32 = 23;

/// The Settings button that lets the server's default write them again.
pub const DEFAULT_COMMIT_MODEL: i32 = 24;

/// The Settings button that signs in to the hosting service numbered by
/// its item.
pub const SIGN_IN: i32 = 25;

/// The Settings button that signs out of the service numbered by its item.
pub const SIGN_OUT: i32 = 26;

/// The Settings button that stops the sign-in going on.
pub const CANCEL_SIGN_IN: i32 = 27;

/// The Settings button that copies the one-time code and opens its page.
pub const OPEN_SIGN_IN_PAGE: i32 = 28;

/// The Settings button that starts a new schedule.
pub const NEW_SCHEDULE: i32 = 29;

/// The Settings button that goes back to the list of schedules.
pub const ALL_SCHEDULES: i32 = 30;

/// The Settings row that opens the schedule numbered by its item, in the
/// list's order.
pub const PICK_SCHEDULE: i32 = 31;

/// The Settings button that pauses the schedule shown.
pub const PAUSE_SCHEDULE: i32 = 32;

/// The Settings button that resumes the schedule shown.
pub const RESUME_SCHEDULE: i32 = 33;

/// The Settings button that deletes the schedule shown, asking once.
pub const DELETE_SCHEDULE: i32 = 34;

/// The Settings button that creates the new schedule.
pub const CREATE_SCHEDULE: i32 = 35;

/// The Settings button that picks the new schedule's project.
pub const CHOOSE_SCHEDULE_FOLDER: i32 = 36;

/// The Settings button that picks the session the new schedule's prompt
/// goes to.
pub const CHOOSE_SCHEDULE_SESSION: i32 = 37;

/// The Settings row that opens the sign-in page of the provider numbered
/// by its item, in the server's list of integrations.
pub const OPEN_PROVIDER: i32 = 38;

/// The Settings button that goes back to the list of providers.
pub const ALL_PROVIDERS: i32 = 39;

/// The Settings button that uses the sign-in method numbered by its item.
pub const USE_SIGN_IN_METHOD: i32 = 40;

/// The Settings button that signs in with the method in use.
pub const SIGN_IN_PROVIDER: i32 = 41;

/// The Settings button that opens a browser sign-in's page again.
pub const OPEN_PROVIDER_PAGE: i32 = 42;

/// The Settings button that finishes a browser sign-in with the code typed.
pub const FINISH_SIGN_IN: i32 = 43;

/// The Settings button that stops a sign-in that has started.
pub const CANCEL_PROVIDER_SIGN_IN: i32 = 44;

/// The Settings button that removes the shown provider's connection
/// numbered by its item, asking once.
pub const REMOVE_CREDENTIAL: i32 = 45;

/// The Settings button that picks a provider to sign in to.
pub const ADD_PROVIDER: i32 = 46;

/// The Settings button that removes the saved approval numbered by its item.
pub const REMOVE_APPROVAL: i32 = 47;

/// The Settings row that shows the MCP server numbered by its item, in the
/// server's list.
pub const OPEN_MCP: i32 = 48;

/// The Settings button back from an MCP server's page, or from adding one.
pub const ALL_MCP: i32 = 49;

/// The Settings button that connects the shown MCP server (item 1) or
/// disconnects it (item 0).
pub const CONNECT_MCP: i32 = 50;

/// The Settings button that turns the shown MCP server off (item 1) or on
/// (item 0) in the config.
pub const TURN_OFF_MCP: i32 = 51;

/// The Settings button that removes the shown MCP server, asking once.
pub const REMOVE_MCP: i32 = 52;

/// The Settings button that shows the catalog and the form for a new MCP
/// server.
pub const ADD_MCP: i32 = 53;

/// The Settings button that adds the catalog's MCP server numbered by its
/// item.
pub const ADD_OFFERED_MCP: i32 = 54;

/// The Settings button that adds the MCP server the form describes.
pub const SAVE_MCP: i32 = 55;

/// The Settings button that opens the sign-in of the shown MCP server.
pub const SIGN_IN_MCP: i32 = 56;

/// The Settings button that checks the package plugins for updates.
pub const CHECK_PLUGINS: i32 = 57;

/// The Settings button that updates the plugin numbered by its item, or
/// every outdated one for item -1.
pub const UPDATE_PLUGIN: i32 = 58;

/// The Settings button that downloads dictation's speech model.
pub const DOWNLOAD_VOICE: i32 = 59;

/// The Settings button that stops the speech model's download.
pub const STOP_VOICE_DOWNLOAD: i32 = 60;

/// The Settings button that opens the menu of dictation's languages.
pub const CHOOSE_VOICE_LANGUAGE: i32 = 61;

/// The Settings button that installs or updates the engine numbered by
/// its item.
pub const INSTALL_ENGINE: i32 = 62;

/// The Settings button that stops installing the engine numbered by its
/// item.
pub const STOP_ENGINE_INSTALL: i32 = 63;

/// The Settings button that starts or stops the server of the engine
/// numbered by its item.
pub const ENGINE_SERVER: i32 = 64;

/// The Settings button that gets the suggested model numbered by its item.
pub const GET_LOCAL_MODEL: i32 = 65;

/// The Settings button that stops getting a model.
pub const STOP_LOCAL_MODEL: i32 = 66;

/// The Settings button that has llama.cpp serve the model numbered by its
/// item.
pub const SERVE_LOCAL_MODEL: i32 = 67;

/// The Settings button that adds llama.cpp's server to OpenCode's config
/// (item 1) or takes it out (item 0).
pub const LOCAL_PROVIDER: i32 = 68;

/// The Settings button that forgets the other computer numbered by its
/// item.
pub const REMOVE_COMPUTER: i32 = 69;

/// The step of Settings → Phone's button that shows a new pairing code.
pub const PAIR_PHONE: i32 = 70;

/// The step of a paired phone's Remove button; the item numbers the phone.
pub const REMOVE_PHONE: i32 = 71;

/// The step of the button that sends a test notification.
pub const TEST_PHONE: i32 = 72;

/// The Settings button that opens the menu of interface fonts.
pub const CHOOSE_INTERFACE_FONT: i32 = 73;

/// The Settings button that opens the menu of code fonts.
pub const CHOOSE_CODE_FONT: i32 = 74;

/// The About button that opens the page numbered by its item in
/// [`ABOUT_LINKS`].
pub const OPEN_ABOUT_LINK: i32 = 75;

/// The About button that shows or hides the colour themes' licence.
pub const THEME_LICENCE: i32 = 76;

/// The Notifications button that shows or hides the names a template fills in.
pub const TEMPLATE_WORDS: i32 = 77;

/// The Settings button that opens the menu of sounds for the event
/// numbered by its item, in [`harlo_core::notifications::Kind::ALL`].
pub const CHOOSE_SOUND: i32 = 78;

/// The About button that shows or hides the event sounds' licence.
pub const SOUND_LICENCE: i32 = 79;

/// The Server button that adds and follows the SSH config's host numbered
/// by its item.
pub const FOLLOW_SSH_HOST: i32 = 80;

/// The Settings button that has agents reach the shown MCP server's tools
/// through scripts (item 1) or as tools listed one by one (item 0).
pub const MCP_SCRIPTS: i32 = 81;

/// The first number of Settings → Models' switches: the one for the model
/// at place `n` of [`App::models`] is numbered this plus `n`.
pub const MODEL_SWITCHES: i32 = 10_000;

/// The pages Settings links to: About's source, releases and new issue,
/// then OpenCode's page on adding skills.
pub const ABOUT_LINKS: [&str; 4] = [
    env!("CARGO_PKG_REPOSITORY"),
    concat!(env!("CARGO_PKG_REPOSITORY"), "/releases"),
    concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new"),
    "https://opencode.ai/docs/skills/",
];

/// [`ABOUT_LINKS`]' page on adding skills.
pub const SKILLS_DOCS: i32 = 3;

/// The step a Settings button for this computer's models or other
/// computers, numbered `number`, takes for item `item`.
fn local_action(number: i32, item: i32) -> Option<Message> {
    let index = usize::try_from(item).ok();
    let engine = || index.and_then(|i| harlo_core::local_models::Engine::ALL.get(i).copied());
    let event = match number {
        INSTALL_ENGINE => app::LocalEvent::Install(engine()?),
        STOP_ENGINE_INSTALL => app::LocalEvent::StopInstall(engine()?),
        ENGINE_SERVER => app::LocalEvent::Server(engine()?),
        GET_LOCAL_MODEL => app::LocalEvent::Get(index?),
        STOP_LOCAL_MODEL => app::LocalEvent::StopGet,
        SERVE_LOCAL_MODEL => app::LocalEvent::Serve(index?),
        LOCAL_PROVIDER => app::LocalEvent::Provider(item == 1),
        REMOVE_COMPUTER => {
            return Some(Message::Sheet(SheetEvent::Computer(
                app::ComputerEvent::Remove(index?),
            )));
        }
        FOLLOW_SSH_HOST => {
            return Some(Message::Sheet(SheetEvent::Computer(
                app::ComputerEvent::Follow(index?),
            )));
        }
        PAIR_PHONE => return Some(Message::Phone(app::PhoneEvent::Pair)),
        REMOVE_PHONE => return Some(Message::Phone(app::PhoneEvent::Remove(index?))),
        TEST_PHONE => return Some(Message::Phone(app::PhoneEvent::Test)),
        _ => return None,
    };
    Some(Message::Local(event))
}

/// The step an MCP server's Settings button numbered `number` takes for
/// item `item`.
fn mcp_action(app: &App, number: i32, item: i32) -> Option<Message> {
    use app::McpEvent;
    let event = match number {
        OPEN_MCP => {
            let Load::Loaded(setup) = &app.sheets.setup else {
                return None;
            };
            let server = setup.mcp.as_ref().ok()?.get(usize::try_from(item).ok()?)?;
            McpEvent::Show(server.name.clone())
        }
        ALL_MCP => McpEvent::Back,
        CONNECT_MCP if item == 1 => McpEvent::Connect,
        CONNECT_MCP => McpEvent::Disconnect,
        TURN_OFF_MCP => McpEvent::TurnOff(item == 1),
        MCP_SCRIPTS => McpEvent::Scripts(item == 1),
        REMOVE_MCP => McpEvent::Remove,
        ADD_MCP => McpEvent::Add,
        ADD_OFFERED_MCP => McpEvent::AddOffered(usize::try_from(item).ok()?),
        SAVE_MCP => McpEvent::Save,
        SIGN_IN_MCP => {
            // The server signs in through OpenCode's integration for it,
            // on Models' sign-in page.
            let Load::Loaded(setup) = &app.sheets.setup else {
                return None;
            };
            let name = app.sheets.mcp.shown()?;
            let server = setup.mcp.as_ref().ok()?.iter().find(|s| s.name == name)?;
            let id = server.integration.clone()?;
            return Some(Message::Sheet(SheetEvent::Provider(
                app::ProviderEvent::OpenFrom(id),
            )));
        }
        _ => return None,
    };
    Some(Message::Sheet(SheetEvent::Mcp(event)))
}

/// The step a provider's Settings button numbered `number` takes for item
/// `item`.
fn provider_action(app: &App, number: i32, item: i32) -> Option<Message> {
    let event = match number {
        OPEN_PROVIDER => {
            let Load::Loaded(list) = &app.sheets.providers.list else {
                return None;
            };
            let id = list.get(usize::try_from(item).ok()?)?.id.clone();
            ProviderEvent::Open(id)
        }
        ALL_PROVIDERS => ProviderEvent::Back,
        USE_SIGN_IN_METHOD => ProviderEvent::Method(usize::try_from(item).ok()?),
        SIGN_IN_PROVIDER => ProviderEvent::SignIn,
        OPEN_PROVIDER_PAGE => ProviderEvent::OpenPage,
        FINISH_SIGN_IN => ProviderEvent::Finish,
        CANCEL_PROVIDER_SIGN_IN => ProviderEvent::Cancel,
        REMOVE_CREDENTIAL => {
            let integration = app.sheets.providers.integration()?;
            let connection = integration.connections.get(usize::try_from(item).ok()?)?;
            ProviderEvent::Remove(connection.credential.clone()?)
        }
        ADD_PROVIDER => ProviderEvent::Pick,
        _ => return None,
    };
    Some(Message::Sheet(SheetEvent::Provider(event)))
}

/// The step a button of Appearance, Notifications or About numbered `number`
/// takes for item `item`: a font menu, a link, the themes' licence, or the
/// names a template fills in.
fn look_action(number: i32, item: i32) -> Option<Message> {
    match number {
        CHOOSE_INTERFACE_FONT => Some(Message::TogglePicker(Picker::InterfaceFont)),
        CHOOSE_CODE_FONT => Some(Message::TogglePicker(Picker::CodeFont)),
        OPEN_ABOUT_LINK => ABOUT_LINKS
            .get(usize::try_from(item).ok()?)
            .map(|url| Message::OpenLink((*url).to_owned())),
        THEME_LICENCE => Some(Message::Sheet(SheetEvent::ThemeLicence)),
        TEMPLATE_WORDS => Some(Message::Sheet(SheetEvent::TemplateWords)),
        SOUND_LICENCE => Some(Message::Sheet(SheetEvent::SoundLicence)),
        CHOOSE_SOUND => {
            let kind = harlo_core::notifications::Kind::ALL.get(usize::try_from(item).ok()?)?;
            Some(Message::TogglePicker(Picker::Sound(*kind)))
        }
        _ => None,
    }
}

/// The step the Settings button numbered `number` takes for item `item`.
fn settings_action(app: &App, number: i32, item: i32) -> Option<Message> {
    let project = |event| Some(Message::Sheet(SheetEvent::Project(event)));
    let schedule = |event| Some(Message::Sheet(SheetEvent::Schedule(event)));
    let worktree = || {
        let folder = app.sheets.projects.shown.as_deref()?;
        let project = app.projects.iter().find(|p| p.directory == folder)?;
        Some(
            project
                .worktrees
                .get(usize::try_from(item).ok()?)?
                .directory
                .clone(),
        )
    };
    match number {
        CHOOSE_DEFAULT_MODEL => Some(Message::TogglePicker(Picker::DefaultModel)),
        CHOOSE_INTERFACE_FONT..=SOUND_LICENCE => look_action(number, item),
        OPEN_SETUP => Some(Message::Sheet(SheetEvent::OpenSetup)),
        INSTALL_OPENCODE => Some(Message::Sheet(SheetEvent::InstallOpenCode)),
        TEST_NOTICE => Some(Message::Sheet(SheetEvent::TestNotice)),
        CHOOSE_PROJECT_MODEL => Some(Message::TogglePicker(Picker::ProjectModel)),
        DEFAULT_PROJECT_MODEL => project(ProjectEvent::Model(None)),
        CHOOSE_ICON => project(ProjectEvent::ChooseIcon),
        CLEAR_ICON => project(ProjectEvent::ClearIcon),
        REMOVE_WORKTREE | FORCE_REMOVE_WORKTREE => project(ProjectEvent::RemoveWorktree {
            directory: worktree()?,
            force: number == FORCE_REMOVE_WORKTREE,
        }),
        HIDE_PROJECT => project(ProjectEvent::Hide),
        PICK_PROJECT => project(ProjectEvent::Pick(Some(
            app.projects
                .get(usize::try_from(item).ok()?)?
                .directory
                .clone(),
        ))),
        ALL_PROJECTS => project(ProjectEvent::Pick(None)),
        CHOOSE_COMMIT_MODEL => Some(Message::TogglePicker(Picker::CommitModel)),
        DEFAULT_COMMIT_MODEL => Some(Message::Sheet(SheetEvent::GitSettings(
            app::GitEvent::CommitModel(None),
        ))),
        SIGN_IN | SIGN_OUT => {
            let service = *Service::ALL.get(usize::try_from(item).ok()?)?;
            Some(Message::Sheet(SheetEvent::Hosting(if number == SIGN_IN {
                app::HostingEvent::SignIn(service)
            } else {
                app::HostingEvent::SignOut(service)
            })))
        }
        CANCEL_SIGN_IN => Some(Message::Sheet(SheetEvent::Hosting(
            app::HostingEvent::Cancel,
        ))),
        OPEN_SIGN_IN_PAGE => Some(Message::Sheet(SheetEvent::Hosting(
            app::HostingEvent::OpenPage,
        ))),
        NEW_SCHEDULE => schedule(ScheduleEvent::New),
        ALL_SCHEDULES => schedule(ScheduleEvent::Back),
        CREATE_SCHEDULE => schedule(ScheduleEvent::Create),
        PICK_SCHEDULE => {
            let Load::Loaded(list) = &app.sheets.schedules.list else {
                return None;
            };
            let id = list.get(usize::try_from(item).ok()?)?.id.clone();
            schedule(ScheduleEvent::Open(id))
        }
        PAUSE_SCHEDULE | RESUME_SCHEDULE | DELETE_SCHEDULE => {
            let SchedulePage::Shown(id) = &app.sheets.schedules.page else {
                return None;
            };
            let id = id.clone();
            schedule(match number {
                PAUSE_SCHEDULE => ScheduleEvent::Pause(id),
                RESUME_SCHEDULE => ScheduleEvent::Resume(id),
                _ => ScheduleEvent::Delete(id),
            })
        }
        CHOOSE_SCHEDULE_FOLDER => Some(Message::TogglePicker(Picker::ScheduleFolder)),
        OPEN_PROVIDER
        | ALL_PROVIDERS
        | USE_SIGN_IN_METHOD
        | SIGN_IN_PROVIDER
        | OPEN_PROVIDER_PAGE
        | FINISH_SIGN_IN
        | CANCEL_PROVIDER_SIGN_IN
        | REMOVE_CREDENTIAL
        | ADD_PROVIDER => provider_action(app, number, item),
        REMOVE_APPROVAL => Some(Message::Sheet(SheetEvent::Approval(
            app::ApprovalEvent::Remove(usize::try_from(item).ok()?),
        ))),
        OPEN_MCP..=SIGN_IN_MCP | MCP_SCRIPTS => mcp_action(app, number, item),
        DOWNLOAD_VOICE => Some(Message::Voice(app::VoiceEvent::Download)),
        STOP_VOICE_DOWNLOAD => Some(Message::Voice(app::VoiceEvent::StopDownload)),
        CHOOSE_VOICE_LANGUAGE => Some(Message::TogglePicker(Picker::VoiceLanguage)),
        INSTALL_ENGINE..=TEST_PHONE | FOLLOW_SSH_HOST => local_action(number, item),
        CHECK_PLUGINS => Some(Message::Sheet(SheetEvent::Plugin(app::PluginEvent::Check))),
        UPDATE_PLUGIN => Some(Message::Sheet(SheetEvent::Plugin(
            app::PluginEvent::Update(usize::try_from(item).ok()),
        ))),
        CHOOSE_SCHEDULE_SESSION => Some(Message::TogglePicker(Picker::ScheduleSession)),
        _ => claude_action_of(number).map(|action| Message::Claude(app::ClaudeEvent::Act(action))),
    }
}

/// The setting the UI numbers `number`.
fn setting_of(number: i32) -> Option<app::Setting> {
    app::Setting::ALL
        .get(usize::try_from(number).ok()?)
        .copied()
}

/// The stats' range the UI numbers `number`.
fn range_of(number: i32) -> Option<app::Range> {
    match number {
        0 => Some(app::Range::Week),
        1 => Some(app::Range::Month),
        2 => Some(app::Range::All),
        _ => None,
    }
}

/// The edge the UI numbers `number`.
fn edge_of(number: i32) -> Option<Edge> {
    match number {
        0 => Some(Edge::Sidebar),
        1 => Some(Edge::Panel),
        2 => Some(Edge::Terminal),
        _ => None,
    }
}

/// Connects the shown terminal's keys, clipboard, mouse and size.
fn connect_terminal(logic: &Logic<'_>, controller: &Rc<Controller>) {
    let c = Rc::clone(controller);
    logic.on_terminal_key(move |text, ctrl, alt, shift| {
        let Some(key) = Key::from_text(&text) else {
            return false;
        };
        // Ctrl+Tab and Ctrl+Page Up/Down switch tabs instead.
        if ctrl && matches!(key, Key::Tab | Key::PageUp | Key::PageDown) {
            return false;
        }
        let mods = Mods { ctrl, alt, shift };
        let bytes = c.shell.try_borrow().ok().and_then(|shell| {
            let active = shell.app.terminals.active()?;
            term::encode(&key, mods, active.screen.mode())
        });
        let Some(bytes) = bytes else {
            return false;
        };
        c.dispatch(Message::Terminal(TerminalEvent::Typed(bytes)));
        true
    });
    let c = Rc::clone(controller);
    logic.on_terminal_selection(move || {
        c.shell
            .try_borrow()
            .ok()
            .and_then(|shell| shell.app.terminals.active()?.screen.selected())
            .unwrap_or_default()
            .into()
    });
    let c = Rc::clone(controller);
    logic.on_terminal_paste(move |text| {
        c.dispatch(Message::Terminal(TerminalEvent::Paste(text.to_string())));
    });
    let c = Rc::clone(controller);
    logic.on_terminal_resized(move |columns, lines| {
        let count = |n: i32| usize::try_from(n).unwrap_or(1);
        c.dispatch(Message::Terminal(TerminalEvent::Resized(
            count(columns),
            count(lines),
        )));
    });
    let c = Rc::clone(controller);
    logic.on_terminal_scroll(move |lines| {
        c.dispatch(Message::Terminal(TerminalEvent::Scroll(lines)));
    });
    let c = Rc::clone(controller);
    logic.on_terminal_select_from(move |line, column, words| {
        c.dispatch(Message::Terminal(TerminalEvent::SelectFrom {
            line: usize::try_from(line).unwrap_or(0),
            column,
            words,
        }));
    });
    let c = Rc::clone(controller);
    logic.on_terminal_select_to(move |line, column| {
        c.dispatch(Message::Terminal(TerminalEvent::SelectTo(
            usize::try_from(line).unwrap_or(0),
            column,
        )));
    });
}

/// Turns the window's callbacks into messages.
#[expect(clippy::too_many_lines, reason = "one short registration per callback")]
fn connect(window: &AppWindow, controller: &Rc<Controller>) {
    let logic = window.global::<Logic>();
    // Each callback holds its own handle on the controller.
    macro_rules! send {
        ($register:ident, || $message:expr) => {{
            let c = Rc::clone(controller);
            logic.$register(move || c.dispatch($message));
        }};
        ($register:ident, |$($arg:ident),*| $message:expr) => {{
            let c = Rc::clone(controller);
            logic.$register(move |$($arg),*| c.dispatch($message));
        }};
    }
    macro_rules! send_with {
        ($register:ident, |$app:ident $(, $arg:ident)*| $message:expr) => {{
            let c = Rc::clone(controller);
            logic.$register(move |$($arg),*| c.dispatch_with(|$app| $message));
        }};
    }

    send!(on_resized, |width, height| Message::Resized(width, height));
    send!(on_toggle_sidebar, || Message::ToggleSidebar);
    send_with!(on_view_picture, |_app, key| key
        .parse()
        .ok()
        .map(|key| Message::Pictures(crate::app::PictureEvent::View(key))));
    send!(on_close_pictures, || Message::Pictures(
        crate::app::PictureEvent::Close
    ));
    send!(on_toggle_pictures, || Message::Pictures(
        crate::app::PictureEvent::ToggleGrid
    ));
    send!(on_turn_picture, |clockwise| Message::Pictures(
        crate::app::PictureEvent::Turn(clockwise)
    ));
    send!(on_show_picture_in_folder, || Message::Pictures(
        crate::app::PictureEvent::ShowInFolder
    ));
    send!(on_toggle_phone_pause, || Message::Phone(
        crate::app::PhoneEvent::TogglePause
    ));
    send!(on_pick_avatar, |index| Message::PickAvatar(
        usize::try_from(index).unwrap_or(0)
    ));
    send_with!(on_toggle_panel, |_app, number| present::panel_of(number)
        .map(Message::TogglePanel));
    send!(on_toggle_terminal, || Message::Terminal(
        TerminalEvent::Toggle
    ));
    send!(on_new_terminal, || Message::Terminal(TerminalEvent::New));
    send_with!(on_toggle_sheet, |_app, number| sheet_of(number)
        .map(|sheet| Message::Sheet(SheetEvent::Toggle(sheet))));
    send!(on_close_sheet, || Message::Sheet(SheetEvent::Close));
    send_with!(on_open_section, |_app, number| section_of(number)
        .map(|section| Message::Sheet(SheetEvent::Open(section))));
    send_with!(on_pick_section, |_app, number| section_of(number)
        .map(|section| Message::Sheet(SheetEvent::Section(section))));
    send!(on_settings_search, |text| Message::Sheet(
        SheetEvent::Query(text.to_string())
    ));
    send_with!(on_switch_setting, |app, number| match number
        .checked_sub(MODEL_SWITCHES)
        .and_then(|n| usize::try_from(n).ok())
    {
        Some(n) => app.models.get(n).map(|model| {
            Message::Sheet(SheetEvent::ToggleModel(
                model.provider_id.clone(),
                model.id.clone(),
            ))
        }),
        None => setting_of(number).map(|setting| Message::Sheet(SheetEvent::Switch(setting))),
    });
    send_with!(on_settings_action, |app, number, item| settings_action(
        app, number, item
    ));
    send_with!(on_setup_step, |_app, number| usize::try_from(number)
        .ok()
        .and_then(|i| app::Step::ALL.get(i).copied())
        .map(|step| Message::Sheet(SheetEvent::Step(step))));
    send!(on_setup_next, || Message::Sheet(SheetEvent::Next));
    send!(on_setup_back, || Message::Sheet(SheetEvent::Back));
    send_with!(on_settings_text, |_app, number, text| usize::try_from(
        number
    )
    .ok()
    .and_then(|i| app::Field::ALL.get(i).copied())
    .map(|field| Message::Sheet(SheetEvent::Text(field, text.to_string()))));
    send!(on_pick_theme, |id| Message::Sheet(SheetEvent::Theme(
        id.to_string()
    )));
    send_with!(on_choose, |_app, choice, option| {
        let choice = usize::try_from(choice)
            .ok()
            .and_then(|i| app::SettingChoice::ALL.get(i).copied());
        let option = usize::try_from(option).ok();
        choice
            .zip(option)
            .map(|(choice, option)| Message::Sheet(SheetEvent::Choose(choice, option)))
    });
    send!(on_system_dark, |dark| Message::SystemDark(dark));
    // The desktop's scheme as the window opens; changes come by the callback.
    controller.dispatch(Message::SystemDark(window.get_system_dark()));
    send_with!(on_stats_range, |_app, number| range_of(number)
        .map(|range| Message::Sheet(SheetEvent::Range(range))));
    send_with!(on_maximize, |_app, number| present::panel_of(number)
        .map(Message::Maximize));
    send!(on_restore, || Message::Restore);
    send!(on_close_panel, || Message::ClosePanel);
    send_with!(on_refresh, |_app, number| present::panel_of(number)
        .and_then(refresh));

    send!(on_toggle_locations, || Message::ToggleLocations);
    send_with!(on_pick_location, |app, index| usize::try_from(index)
        .ok()
        .and_then(|i| app.locations.get(i))
        .cloned()
        .map(Message::PickLocation));
    send!(on_add_project, || Message::AddProject);
    send!(on_project_settings, |folder| Message::Sheet(
        SheetEvent::Project(ProjectEvent::Open(folder.to_string()))
    ));
    send!(on_new_session, |folder| Message::NewSession(
        (!folder.is_empty()).then(|| folder.to_string())
    ));
    send!(on_filter_edited, |text| Message::Filter(text.to_string()));
    send!(on_toggle_more, |folder| Message::ToggleMore(
        folder.to_string()
    ));
    send!(on_open_session, |id| Message::OpenSession(id.to_string()));

    send_with!(on_tab_selected, |app, index| document(app, index)
        .map(Message::ShowDocument));
    send_with!(on_tab_closed, |app, index| document(app, index)
        .map(Message::CloseDocument));
    send_with!(on_terminal_selected, |_app, id| u32::try_from(id)
        .ok()
        .map(|id| Message::Terminal(TerminalEvent::Select(id))));
    send_with!(on_terminal_closed, |_app, id| u32::try_from(id)
        .ok()
        .map(|id| Message::Terminal(TerminalEvent::Close(id))));
    connect_terminal(&logic, controller);
    send_with!(on_resize, |_app, edge, by| edge_of(edge)
        .map(|edge| Message::Resize { edge, by }));
    send!(on_resize_done, || Message::ResizeEnd);

    send_with!(on_notes_edited, |app, text, cursor, anchor| Some(
        Message::Notes(NotesEvent::Edit(
            app.note_key(),
            Edit {
                text: text.to_string(),
                cursor: usize::try_from(cursor).unwrap_or(0),
                anchor: usize::try_from(anchor).unwrap_or(0),
            }
        ))
    ));
    send!(on_toggle_todo, || Message::Notes(NotesEvent::ToggleTodo));
    send!(on_add_todo, || Message::Notes(NotesEvent::AddTodo));
    send!(on_insert_note, || Message::Notes(NotesEvent::Insert));
    send!(on_file_toggle, |path| Message::Files(FilesEvent::Toggle(
        path.to_string()
    )));
    send!(on_file_open, |path| Message::Files(FilesEvent::Open(
        path.to_string()
    )));
    send_with!(on_editor_edited, |app, text, cursor, anchor| editing(app)
        .map(|key| Message::Files(FilesEvent::Editor(
            key,
            Edit {
                text: text.to_string(),
                cursor: usize::try_from(cursor).unwrap_or(0),
                anchor: usize::try_from(anchor).unwrap_or(0),
            }
        ))));
    send_with!(on_file_save, |app| editing(app)
        .map(|key| Message::Files(FilesEvent::Save(key))));
    send_with!(on_file_save_and_close, |app| editing(app)
        .map(|key| Message::Files(FilesEvent::SaveAndClose(key))));
    send_with!(on_file_discard, |app| editing(app)
        .map(|key| Message::Files(FilesEvent::Discard(key))));
    send_with!(on_file_keep_open, |app| editing(app)
        .map(|key| Message::Files(FilesEvent::KeepOpen(key))));
    send_with!(on_file_reload, |app| editing(app)
        .map(|key| Message::Files(FilesEvent::Reload(key))));
    send_with!(on_file_keep_mine, |app| editing(app)
        .map(|key| Message::Files(FilesEvent::KeepMine(key))));
    send!(on_change_toggle, |key| Message::Changes(
        ChangesEvent::Toggle(key.to_string())
    ));
    send!(on_change_select, |key| Message::Changes(
        ChangesEvent::Select(key.to_string())
    ));
    send!(on_change_whole, |key| Message::Changes(
        ChangesEvent::Whole(key.to_string())
    ));
    send_with!(on_change_hunk, |_app, hunk| usize::try_from(hunk)
        .ok()
        .map(|hunk| Message::Changes(ChangesEvent::Hunk(hunk))));
    send!(on_end_review, || Message::Changes(ChangesEvent::EndReview));
    send!(on_diff_columns, |whole, columns| Message::Changes(
        ChangesEvent::Columns {
            whole,
            columns: usize::try_from(columns).unwrap_or_default(),
        }
    ));
    send!(on_commit_message_edited, |text| Message::Changes(
        ChangesEvent::Message(text.to_string())
    ));
    send!(on_commit, || Message::Changes(ChangesEvent::Commit));
    send!(on_write_commit_message, || Message::Changes(
        ChangesEvent::WriteMessage
    ));
    send!(on_push, || Message::Changes(ChangesEvent::Push));
    send_with!(on_change_comment, |_app, key, at| usize::try_from(at)
        .ok()
        .map(|at| Message::Changes(ChangesEvent::CommentOn(
            key.to_string(),
            at
        ))));
    send!(on_change_comment_add, |text| Message::Changes(
        ChangesEvent::AddComment(text.to_string())
    ));
    send!(on_change_comment_cancel, || Message::Changes(
        ChangesEvent::CancelComment
    ));
    send_with!(on_change_comment_remove, |_app, index| usize::try_from(
        index
    )
    .ok()
    .map(|index| Message::Changes(ChangesEvent::RemoveComment(index))));
    send!(on_change_comments_send, || Message::Changes(
        ChangesEvent::SendComments
    ));
    send!(on_change_comments_discard, || Message::Changes(
        ChangesEvent::DiscardComments
    ));

    send!(on_composer_menu_pick, |index| Message::MenuPick(
        usize::try_from(index).ok()
    ));
    send!(on_composer_edited, |text, cursor, anchor| Message::Compose(
        Edit {
            text: text.to_string(),
            cursor: usize::try_from(cursor).unwrap_or(0),
            anchor: usize::try_from(anchor).unwrap_or(0),
        }
    ));
    send!(on_send, || Message::Send);
    send!(on_stop, || Message::Stop);
    send!(on_attach, || Message::Attach);
    send!(on_dictate, || Message::Voice(app::VoiceEvent::Toggle));
    send_with!(on_detach, |_app, index| usize::try_from(index)
        .ok()
        .map(Message::Detach));

    send_with!(on_toggle_picker, |_app, number| Picker::from_index(number)
        .map(Message::TogglePicker));
    send!(on_close_picker, || Message::ClosePicker);
    send_with!(on_pick, |app, index| app.picker.and_then(|picker| {
        model::items(app, picker)
            .into_iter()
            .nth(usize::try_from(index).ok()?)
            .map(|item| item.message)
    }));
    send!(on_menu_filter_edited, |text| Message::PickerFilter(
        text.to_string()
    ));
    send!(on_menu_action, || Message::AddProject);

    // The open session's timeline and the lines above its composer.
    send_with!(on_open_panel, |_app, number| present::panel_of(number)
        .map(Message::OpenPanel));
    send_with!(on_toggle_group, |app, key| in_session(
        app,
        TimelineEvent::ToggleGroup(key.to_string())
    ));
    send_with!(on_toggle_diff, |app, key| in_session(
        app,
        TimelineEvent::ToggleDiff(key.to_string())
    ));
    send_with!(on_undo, |app, key| in_session(
        app,
        TimelineEvent::Undo(key.to_string())
    ));
    send_with!(on_review, |app, key, file| in_session(
        app,
        TimelineEvent::Review {
            tool: key.to_string(),
            file: usize::try_from(file).unwrap_or_default(),
        }
    ));
    send!(on_continue_run, || Message::ContinueRun);
    send!(on_link, |url| Message::OpenLink(url.to_string()));
    send!(on_steer_queued, |id| Message::SteerQueued(id.to_string()));
    send!(on_cancel_queued, |id| Message::CancelQueued(id.to_string()));

    // The cards for what the open session waits on.
    send!(on_undo_keep, |session| Message::UndoKeep(
        session.to_string()
    ));
    send!(on_undo_cancel, |session| Message::UndoCancel(
        session.to_string()
    ));
    send_with!(on_approve, |_app, request, reply| reply_of(reply).map(
        |reply| Message::Approve {
            request: request.to_string(),
            reply,
        }
    ));
    send!(on_refuse, |request| Message::Refuse(request.to_string()));
    send!(on_refuse_reason, |reason| Message::RefuseReason(
        reason.to_string()
    ));
    send!(on_refuse_send, || Message::RefuseSend);
    send!(on_refuse_cancel, || Message::RefuseCancel);
    send!(on_form_choose, |form, key, value| Message::FormChoose {
        form: form.to_string(),
        key: key.to_string(),
        value: value.to_string(),
    });
    send!(on_form_type, |form, key, text| Message::FormType {
        form: form.to_string(),
        key: key.to_string(),
        text: text.to_string(),
    });
    send!(on_form_flag, |form, key, flag| Message::FormFlag {
        form: form.to_string(),
        key: key.to_string(),
        flag,
    });
    send!(on_form_send, |form| Message::FormSend(form.to_string()));
    send!(on_form_dismiss, |form| Message::FormDismiss(
        form.to_string()
    ));

    let keys = Rc::clone(controller);
    logic.on_key(move |text, command, shift, alt| {
        let press = KeyPress::new(&text, command, shift, alt);
        let message = keys
            .shell
            .try_borrow()
            .ok()
            .and_then(|shell| window_key(&shell.app, &press));
        message.is_some_and(|message| {
            keys.dispatch(message);
            true
        })
    });
    let composer = Rc::clone(controller);
    logic.on_composer_key(move |text, command, shift, alt| {
        let press = KeyPress::new(&text, command, shift, alt);
        let message = composer.shell.try_borrow().ok().and_then(|shell| {
            let menu_open = shell.app.menu.as_ref().is_some_and(|m| !m.items.is_empty());
            // The approval keys work while typing too.
            shell
                .app
                .approval_key(&press)
                .or_else(|| app::compose_key(&press, menu_open))
        });
        match message {
            // The field still pastes any text that's on the clipboard.
            Some(message @ Message::PasteImage) => {
                composer.dispatch(message);
                false
            }
            Some(message) => {
                composer.dispatch(message);
                true
            }
            None => false,
        }
    });
}

/// What a key press means anywhere in the window: closing or moving
/// through an open menu, restoring a maximized panel, saving the file in
/// the editor, or a shortcut.
#[must_use]
pub fn window_key(app: &App, press: &KeyPress) -> Option<Message> {
    use crate::keys::Key;
    // A menu over a sheet closes before the sheet.
    if app.picker.is_some() && press.key == Key::Escape {
        return Some(Message::ClosePicker);
    }
    if app.sheets.open.is_some() && press.key == Key::Escape {
        return Some(Message::Sheet(app::SheetEvent::Close));
    }
    if app.pictures_shown() && press.key == Key::Escape {
        return Some(Message::Pictures(app::PictureEvent::Close));
    }
    if let Some(message) = app.approval_key(press) {
        return Some(message);
    }
    if app.picking_location && press.key == Key::Escape {
        return Some(Message::ToggleLocations);
    }
    if app.picker.is_some() {
        if press.key == Key::Escape {
            return Some(Message::ClosePicker);
        }
        if let Some(message) = app::menu_key(press) {
            return Some(message);
        }
    } else if app.maximized.is_some()
        && let Some(message) = app::restore_key(press)
    {
        return Some(message);
    }
    if app.voice.listening == app::Listening::Recording && press.key == Key::Escape {
        return Some(Message::Voice(app::VoiceEvent::Cancel));
    }
    // Ctrl+S saves the file whose editor is showing.
    if press.command
        && !press.shift
        && press.key == Key::Character('s')
        && let Some(key) = app.fs.shown.clone()
    {
        return Some(Message::Files(FilesEvent::Save(key)));
    }
    app::shortcut(press)
}

/// The key of the file in the editor tab, if one's open.
fn editing(app: &App) -> Option<String> {
    app.fs.open.first().map(|f| f.key.clone())
}

/// Sends `event` to the open session's timeline.
fn in_session(app: &App, event: TimelineEvent) -> Option<Message> {
    app.active.clone().map(|id| Message::Timeline(id, event))
}

/// An approval's answer by the UI's number: allow once, always, or deny.
fn reply_of(number: i32) -> Option<PermissionReply> {
    match number {
        0 => Some(PermissionReply::Once),
        1 => Some(PermissionReply::Always),
        2 => Some(PermissionReply::Reject),
        _ => None,
    }
}

/// What a panel's Refresh button does.
fn refresh(panel: Panel) -> Option<Message> {
    match panel {
        Panel::Context => Some(Message::Context(ContextEvent::Refresh)),
        Panel::Changes => Some(Message::Changes(ChangesEvent::Refresh)),
        Panel::Files => Some(Message::Files(FilesEvent::Refresh)),
        Panel::Todos | Panel::Subagents | Panel::Notes => None,
    }
}
