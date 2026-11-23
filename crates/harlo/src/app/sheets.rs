//! Pages that open over the window from the left rail: usage stats across
//! sessions, and Settings, whose sections hold Harlo's own settings, its
//! appearance, the keyboard shortcuts, the server's set-up and About. The
//! setup guide opens on the first start and walks through what Harlo needs.
//!
//! Harlo's own settings are saved with the layout. The server's set-up
//! (providers, web search, MCP servers, plugins) belongs to the shared
//! server, which other clients use too; it changes only when the user asks.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use harlo_core::git::{self, Identity};
use harlo_core::notifications::{self as notify, Kind};
use harlo_core::{autostart, config};

use harlo_core::layout::Appearance;
use harlo_core::server::{Location, McpServer, Plugin, Provider, Stats, WebSearch};
use harlo_core::theme::Mode;

use super::projects::{ProjectEvent, Projects};
use super::{App, Message};
use crate::model::ModelChoice;
use crate::task::Task;

/// A page over the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    /// Usage across sessions.
    Stats,
    /// Settings, at [`Sheets::section`].
    Settings,
    /// The setup guide, at [`Sheets::step`].
    Setup,
}

/// A step of the setup guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    /// OpenCode, which runs the agents.
    #[default]
    OpenCode,
    /// The models agents use, and the providers that serve them.
    Models,
    /// Git, and the name and email on commits.
    Git,
    /// How Harlo looks.
    Theme,
}

impl Step {
    /// Every step, in order.
    pub const ALL: [Self; 4] = [Self::OpenCode, Self::Models, Self::Git, Self::Theme];

    /// Its title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::OpenCode => "OpenCode",
            Self::Models => "Models",
            Self::Git => "Git",
            Self::Theme => "Theme",
        }
    }

    /// What it's for, in a sentence.
    #[must_use]
    pub fn about(self) -> &'static str {
        match self {
            Self::OpenCode => {
                "OpenCode runs the agents, in a background service on this computer. Harlo \
                 shows them, the work they do and what they ask you."
            }
            Self::Models => {
                "Agents need a model. OpenCode's own free models work without signing in; \
                 a provider or a subscription adds more."
            }
            Self::Git => {
                "Harlo shows each project's changes and commits them with git, under your \
                 name."
            }
            Self::Theme => "How Harlo looks. Settings → Appearance has these and more.",
        }
    }

    /// Its place, counted from 0.
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|s| *s == self)
            .unwrap_or_default()
    }
}

/// The command that installs OpenCode, from its docs.
pub const OPENCODE_INSTALL: &str = "curl -fsSL https://opencode.ai/v2/install | bash";

/// A section of Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    /// Harlo's general settings.
    #[default]
    General,
    /// The colour theme, light or dark.
    Appearance,
    /// Desktop notifications.
    Notifications,
    /// The keyboard shortcuts.
    Shortcuts,
    /// Each project's settings and worktrees.
    Projects,
    /// Git: the name on commits, undo's snapshots, commit messages.
    Git,
    /// The server's model providers.
    Models,
    /// Web search, MCP servers and plugins.
    Tools,
    /// Prompts the server sends on a schedule.
    Schedules,
    /// The server Harlo follows.
    Server,
    /// Harlo's version and Slint's notice.
    About,
}

impl Section {
    /// Every section, in the order Settings lists them.
    pub const ALL: [Self; 11] = [
        Self::General,
        Self::Appearance,
        Self::Notifications,
        Self::Shortcuts,
        Self::Projects,
        Self::Git,
        Self::Models,
        Self::Tools,
        Self::Schedules,
        Self::Server,
        Self::About,
    ];

    /// Its title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Notifications => "Notifications",
            Self::Shortcuts => "Shortcuts",
            Self::Projects => "Projects",
            Self::Git => "Git",
            Self::Models => "Models",
            Self::Tools => "Tools for agents",
            Self::Schedules => "Schedules",
            Self::Server => "Server",
            Self::About => "About",
        }
    }

    /// The group it's listed under, if any.
    #[must_use]
    pub fn group(self) -> Option<&'static str> {
        match self {
            Self::General
            | Self::Appearance
            | Self::Notifications
            | Self::Shortcuts
            | Self::About => None,
            Self::Projects | Self::Git => Some("Your projects"),
            Self::Models | Self::Tools => Some("Agents and models"),
            Self::Schedules => Some("Automation"),
            Self::Server => Some("Connections"),
        }
    }
}

/// One of Harlo's own settings that a switch turns on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// [`Preferences::reopen_tabs`].
    ReopenTabs,
    /// [`Preferences::glass`].
    Glass,
    /// Whether Harlo starts at login: the desktop's autostart entry, not a
    /// preference.
    StartAtLogin,
    /// [`Preferences::keep_in_tray`].
    KeepInTray,
    /// Whether Harlo notifies at all.
    Notify,
    /// Whether it notifies while its window has focus.
    NotifyFocused,
    /// Whether notifications play a sound.
    NotifySound,
    /// Whether an event notifies.
    NotifyOn(Kind),
    /// Whether Claude plan limits notify.
    NotifyLimits,
    /// Whether quiet hours apply.
    Quiet,
    /// Whether questions get through quiet hours.
    QuietQuestions,
    /// Whether OpenCode keeps snapshots for undo: its config, not a
    /// preference.
    Snapshots,
    /// Whether OpenCode allows automations, which schedules are: its
    /// config, not a preference.
    Automations,
}

impl Setting {
    /// Every setting, in the order the UI numbers them.
    pub const ALL: [Self; 15] = [
        Self::ReopenTabs,
        Self::Glass,
        Self::StartAtLogin,
        Self::KeepInTray,
        Self::Notify,
        Self::NotifyFocused,
        Self::NotifySound,
        Self::NotifyOn(Kind::Finished),
        Self::NotifyOn(Kind::Failed),
        Self::NotifyOn(Kind::Asks),
        Self::NotifyLimits,
        Self::Quiet,
        Self::QuietQuestions,
        Self::Snapshots,
        Self::Automations,
    ];
}

/// A text field in Settings or the guide, saved by its button or Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The name or email on commits.
    Identity(Identity),
    /// When quiet hours start.
    QuietFrom,
    /// When they end.
    QuietTo,
    /// A notification's title template.
    Title(Kind),
    /// A notification's message template.
    Message(Kind),
    /// The shown project's name.
    ProjectName,
    /// The command a new worktree of the shown project runs.
    StartCommand,
    /// A new worktree's name.
    NewWorktree,
    /// The GitLab server git signs in to.
    GitLabHost,
    /// The Forgejo server.
    ForgejoHost,
    /// A Forgejo access token, to sign in with.
    ForgejoToken,
    /// A new schedule's name.
    ScheduleName,
    /// When it runs, as typed.
    ScheduleTime,
    /// The prompt it sends.
    SchedulePrompt,
}

impl Field {
    /// Every field, in the order the UI numbers them.
    pub const ALL: [Self; 19] = [
        Self::Identity(Identity::Name),
        Self::Identity(Identity::Email),
        Self::QuietFrom,
        Self::QuietTo,
        Self::Title(Kind::Finished),
        Self::Message(Kind::Finished),
        Self::Title(Kind::Failed),
        Self::Message(Kind::Failed),
        Self::Title(Kind::Asks),
        Self::Message(Kind::Asks),
        Self::ProjectName,
        Self::StartCommand,
        Self::NewWorktree,
        Self::GitLabHost,
        Self::ForgejoHost,
        Self::ForgejoToken,
        Self::ScheduleName,
        Self::ScheduleTime,
        Self::SchedulePrompt,
    ];
}

/// Starting Harlo at login, through the desktop's autostart folder.
#[derive(Debug, Clone)]
pub struct Autostart {
    /// The autostart folder, such as `~/.config/autostart`.
    pub folder: PathBuf,
    /// The program the entry runs: this one.
    pub program: PathBuf,
    /// Whether the entry is there.
    pub on: bool,
}

/// One of Harlo's own settings chosen from a few options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingChoice {
    /// [`Preferences::appearance`].
    Appearance,
    /// [`Preferences::contrast`].
    Contrast,
    /// [`Preferences::diff_colors`].
    DiffColors,
    /// [`Preferences::motion`].
    Motion,
    /// [`Preferences::chat_width`].
    ChatWidth,
    /// [`Preferences::text_size`].
    TextSize,
    /// [`Preferences::code_size`].
    CodeSize,
    /// The shown project's colour, or none.
    ProjectColor,
    /// Whether removing the shown project keeps or archives its sessions.
    ProjectSessions,
    /// How a new schedule's time is given.
    ScheduleWhen,
}

impl SettingChoice {
    /// Every choice, in the order the UI numbers them.
    pub const ALL: [Self; 10] = [
        Self::Appearance,
        Self::Contrast,
        Self::DiffColors,
        Self::Motion,
        Self::ChatWidth,
        Self::TextSize,
        Self::CodeSize,
        Self::ProjectColor,
        Self::ProjectSessions,
        Self::ScheduleWhen,
    ];

    /// The options, as Settings names them, in order.
    #[must_use]
    pub fn options(self) -> &'static [&'static str] {
        match self {
            Self::Appearance => &["System", "Light", "Dark"],
            Self::Contrast => &["Standard", "More"],
            Self::DiffColors => &["Green and red", "Blue and orange"],
            Self::Motion => &["Off", "Fast", "Normal", "Slow"],
            Self::ChatWidth => &["Narrow", "Normal", "Wide", "Full"],
            Self::TextSize => &["Small", "Normal", "Large", "Larger"],
            Self::CodeSize => &["Small", "Normal", "Large"],
            // None, then OpenCode's colours.
            Self::ProjectColor => &[
                "None", "Orange", "Yellow", "Cyan", "Green", "Red", "Pink", "Blue", "Purple",
                "Gray",
            ],
            Self::ProjectSessions => &["Keep them", "Archive them"],
            Self::ScheduleWhen => &["Once", "Every", "Cron"],
        }
    }
}

/// The time the stats cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    /// The last seven days.
    Week,
    /// The last thirty days.
    #[default]
    Month,
    /// Since the first session.
    All,
}

impl Range {
    /// How far back it reaches, or `None` for all time.
    fn span(self) -> Option<Duration> {
        /// Seconds in a day.
        const DAY: u64 = 24 * 60 * 60;
        match self {
            Self::Week => Some(Duration::from_secs(7 * DAY)),
            Self::Month => Some(Duration::from_secs(30 * DAY)),
            Self::All => None,
        }
    }
}

/// What a page loaded, or why it couldn't.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Load<T> {
    /// Not asked for yet.
    #[default]
    Idle,
    /// On its way.
    Loading,
    /// Here.
    Loaded(T),
    /// It failed, for this reason.
    Failed(String),
}

/// The server's set-up, each part loaded on its own.
#[derive(Debug, Clone, PartialEq)]
pub struct SetUp {
    /// The model providers.
    pub providers: Result<Vec<Provider>, String>,
    /// The web search services.
    pub search: Result<Vec<WebSearch>, String>,
    /// The MCP servers.
    pub mcp: Result<Vec<McpServer>, String>,
    /// The plugins.
    pub plugins: Result<Vec<Plugin>, String>,
}

/// Something that happened to the pages.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Open a page, or close it if it's open.
    Toggle(Sheet),
    /// Open Settings at a section, or close it if it's open there.
    Open(Section),
    /// Close the open page.
    Close,
    /// Show another section of Settings.
    Section(Section),
    /// Search Settings for this text; empty shows the section again.
    Query(String),
    /// Turn one of Harlo's own settings on or off.
    Switch(Setting),
    /// Use the theme with this id while the window is in its mode.
    Theme(String),
    /// Take option `.1` (counted from 0) of a choice.
    Choose(SettingChoice, usize),
    /// Make this the model new sessions use, in OpenCode's global config.
    DefaultModel(ModelChoice),
    /// The model new sessions use was set, or why it couldn't be.
    DefaultModelSet(Result<(), String>),
    /// Something in Settings → Git.
    GitSettings(super::GitEvent),
    /// Something about a git hosting sign-in.
    Hosting(super::HostingEvent),
    /// Something in Settings → Schedules.
    Schedule(super::ScheduleEvent),
    /// Starting at login was turned on (`true`) or off, or why it couldn't be.
    AutostartSet(Result<bool, String>),
    /// Open the setup guide at its first step.
    OpenSetup,
    /// Show this step of the guide.
    Step(Step),
    /// Go to the guide's next step, or finish it after the last.
    Next,
    /// Go back a step.
    Back,
    /// Install OpenCode on this computer, in a terminal.
    InstallOpenCode,
    /// Git's setup on this computer arrived.
    Git(git::Setup),
    /// Save this text from a field.
    Text(Field, String),
    /// Send a test notification.
    TestNotice,
    /// Something done in Settings → Projects.
    Project(ProjectEvent),
    /// The name or email was saved as this, or why it couldn't be.
    IdentitySet(Identity, Result<String, String>),
    /// Show stats for another range.
    Range(Range),
    /// Stats arrived for load `serial`.
    Stats(u64, Result<Stats, String>),
    /// The set-up arrived for load `serial`.
    SetUp(u64, Box<SetUp>),
}

/// The pages' state.
#[derive(Debug, Default)]
pub struct Sheets {
    /// The page shown, if any.
    pub open: Option<Sheet>,
    /// The section of Settings shown.
    pub section: Section,
    /// What Settings is searched for; empty when it isn't.
    pub query: String,
    /// The stats' range.
    pub range: Range,
    /// The stats.
    pub stats: Load<Stats>,
    /// The set-up.
    pub setup: Load<Box<SetUp>>,
    /// Why the model new sessions use couldn't be set, if it couldn't.
    pub model_error: Option<String>,
    /// Why starting at login couldn't be changed, if it couldn't.
    pub autostart_error: Option<String>,
    /// The setup guide's step.
    pub step: Step,
    /// Git on this computer, for the guide.
    pub git: Load<git::Setup>,
    /// Why the name or email on commits couldn't be saved, if it couldn't.
    pub git_error: Option<String>,
    /// Why a notification setting couldn't be taken, if it couldn't.
    pub notice_error: Option<String>,
    /// Settings → Projects.
    pub projects: Projects,
    /// Settings → Git.
    pub git_settings: super::GitSettings,
    /// Git hosting sign-ins.
    pub hosting: super::HostingState,
    /// Settings → Schedules.
    pub schedules: super::Schedules,
    /// Counts loads, so a late answer to an older one is dropped.
    serial: u64,
}

impl App {
    /// Applies something that happened to the pages, and checks the Claude
    /// subscription when Models comes into view.
    pub(super) fn sheet_event(&mut self, event: Event) -> Task<Message> {
        let models_shown = |app: &Self| match app.sheets.open {
            Some(Sheet::Settings) => app.sheets.section == Section::Models,
            Some(Sheet::Setup) => app.sheets.step == Step::Models,
            _ => false,
        };
        let before = models_shown(self);
        let guiding = self.sheets.open == Some(Sheet::Setup);
        let mut task = self.apply_sheet(event);
        // However the guide closes, it's done with until asked for again.
        if guiding && self.sheets.open != Some(Sheet::Setup) && !self.preferences.setup_done {
            self.preferences.setup_done = true;
            task = Task::batch([task, self.save_preferences()]);
        }
        if models_shown(self) && !before {
            return Task::batch([task, self.check_claude()]);
        }
        task
    }

    fn apply_sheet(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle(sheet) if self.sheets.open == Some(sheet) => self.sheets.open = None,
            Event::Toggle(sheet) => {
                self.sheets.open = Some(sheet);
                return match sheet {
                    Sheet::Stats => self.load_stats(),
                    Sheet::Settings => {
                        self.sheets.query.clear();
                        self.open_settings()
                    }
                    Sheet::Setup => self.open_setup(),
                };
            }
            Event::Open(section)
                if self.sheets.open == Some(Sheet::Settings)
                    && self.sheets.section == section
                    && self.sheets.query.is_empty() =>
            {
                self.sheets.open = None;
            }
            Event::Open(section) => {
                self.sheets.section = section;
                self.sheets.query.clear();
                if self.sheets.open != Some(Sheet::Settings) {
                    self.sheets.open = Some(Sheet::Settings);
                    return self.open_settings();
                }
            }
            Event::Close => {
                self.sheets.open = None;
                // A menu opened from the sheet goes with it.
                self.picker = None;
            }
            Event::Section(section) => {
                self.sheets.section = section;
                self.sheets.query.clear();
                // The server runs them: what they did may have changed.
                if section == Section::Schedules {
                    return self.load_schedules();
                }
            }
            Event::Query(query) => self.sheets.query = query,
            Event::Switch(setting) => return self.flip(setting),
            Event::GitSettings(event) => return self.git_event(event),
            Event::Hosting(event) => return self.hosting_event(event),
            Event::Schedule(event) => return self.schedule_event(event),
            Event::AutostartSet(result) => match (result, self.autostart.as_mut()) {
                (Ok(on), Some(autostart)) => {
                    autostart.on = on;
                    self.sheets.autostart_error = None;
                }
                (Err(why), _) => self.sheets.autostart_error = Some(why),
                (Ok(_), None) => {}
            },
            Event::Theme(id) => return self.pick_theme(id),
            Event::DefaultModel(model) => return self.set_default_model(&model),
            Event::DefaultModelSet(result) => self.sheets.model_error = result.err(),
            Event::Choose(SettingChoice::ProjectColor, option) => {
                let color = option.checked_sub(1);
                return self.project_event(ProjectEvent::Color(color));
            }
            Event::Choose(SettingChoice::ProjectSessions, option) => {
                return self.project_event(ProjectEvent::Archive(option == 1));
            }
            Event::Choose(SettingChoice::ScheduleWhen, option) => {
                if let Some(&when) = harlo_core::schedules::When::ALL.get(option) {
                    self.sheets.schedules.draft.when = when;
                }
            }
            Event::Choose(choice, option) => {
                if self.choose(choice, option) {
                    self.retheme();
                    return self.save_preferences();
                }
            }
            event @ (Event::OpenSetup
            | Event::Step(_)
            | Event::Next
            | Event::Back
            | Event::InstallOpenCode
            | Event::Git(_)
            | Event::IdentitySet(..)) => return self.apply_guide(event),
            Event::Text(field, value) => return self.take_text(field, &value),
            Event::TestNotice => return self.test_notice(),
            Event::Project(event) => return self.project_event(event),
            Event::Range(range) => {
                self.sheets.range = range;
                return self.load_stats();
            }
            Event::Stats(serial, result) if serial == self.sheets.serial => {
                self.sheets.stats = match result {
                    Ok(stats) => Load::Loaded(stats),
                    Err(error) => Load::Failed(error),
                };
            }
            Event::SetUp(serial, setup) if serial == self.sheets.serial => {
                self.sheets.setup = Load::Loaded(setup);
            }
            Event::Stats(..) | Event::SetUp(..) => {}
        }
        Task::none()
    }

    /// Flips a switch: Harlo's own preference, starting at login, or
    /// OpenCode's snapshots.
    fn flip(&mut self, setting: Setting) -> Task<Message> {
        match setting {
            Setting::StartAtLogin => return self.toggle_autostart(),
            Setting::Snapshots => return self.git_event(super::GitEvent::ToggleSnapshots),
            Setting::Automations => {
                return self.schedule_event(super::ScheduleEvent::ToggleAllowed);
            }
            Setting::KeepInTray => {
                self.preferences.keep_in_tray = !self.preferences.keep_in_tray;
            }
            Setting::ReopenTabs => {
                self.preferences.reopen_tabs = !self.preferences.reopen_tabs;
            }
            Setting::Glass => self.preferences.glass = !self.preferences.glass,
            setting => self.switch_notice(setting),
        }
        self.save_preferences()
    }

    /// Takes the text saved from a field.
    fn take_text(&mut self, field: Field, value: &str) -> Task<Message> {
        match field {
            Field::Identity(identity) => self.set_identity(identity, value),
            Field::ProjectName => self.set_project_text(true, value),
            Field::StartCommand => self.set_project_text(false, value),
            Field::NewWorktree => self.make_worktree(value),
            Field::GitLabHost | Field::ForgejoHost | Field::ForgejoToken => {
                self.hosting_text(field, value)
            }
            Field::ScheduleName | Field::ScheduleTime | Field::SchedulePrompt => {
                self.schedule_text(field, value)
            }
            Field::QuietFrom | Field::QuietTo | Field::Title(_) | Field::Message(_) => {
                self.set_notice_text(field, value)
            }
        }
    }

    /// OpenCode's global config folder, when Harlo may change it: only for
    /// the background service on this computer, which reads it.
    #[must_use]
    pub fn global_config(&self) -> Option<&Path> {
        matches!(self.location, Location::Shared)
            .then_some(self.opencode_folder.as_deref())
            .flatten()
    }

    /// Sets `model` as the one new sessions use, in OpenCode's global
    /// config, off the UI thread. The server reloads the file and says so,
    /// which reloads the models here.
    fn set_default_model(&mut self, model: &ModelChoice) -> Task<Message> {
        let Some(folder) = self.global_config().map(Path::to_path_buf) else {
            return Task::none();
        };
        self.sheets.model_error = None;
        let id = format!("{}/{}", model.provider_id, model.id);
        Task::perform(config::set_model_later(folder, Some(id)), |result| {
            Message::Sheet(Event::DefaultModelSet(
                result.map_err(|e| format!("Couldn't change OpenCode's config: {e}")),
            ))
        })
    }

    /// Turns a notification setting on or off.
    fn switch_notice(&mut self, setting: Setting) {
        let notices = &mut self.preferences.notifications;
        let flag = match setting {
            Setting::Notify => &mut notices.enabled,
            Setting::NotifyFocused => &mut notices.while_focused,
            Setting::NotifySound => &mut notices.sound,
            Setting::NotifyOn(kind) => &mut notices.template_mut(kind).on,
            Setting::NotifyLimits => &mut notices.limits,
            Setting::Quiet => &mut notices.quiet.on,
            Setting::QuietQuestions => &mut notices.quiet.questions,
            Setting::ReopenTabs
            | Setting::Glass
            | Setting::StartAtLogin
            | Setting::KeepInTray
            | Setting::Snapshots
            | Setting::Automations => {
                return;
            }
        };
        *flag = !*flag;
    }

    /// Takes a quiet hour's time or a template from its field.
    fn set_notice_text(&mut self, field: Field, value: &str) -> Task<Message> {
        let notices = &mut self.preferences.notifications;
        self.sheets.notice_error = None;
        match field {
            Field::QuietFrom | Field::QuietTo => {
                let Some(minute) = notify::parse_clock(value) else {
                    self.sheets.notice_error = Some(format!(
                        "“{}” isn't a time; use one like 22:00.",
                        value.trim()
                    ));
                    return Task::none();
                };
                if field == Field::QuietFrom {
                    notices.quiet.from = minute;
                } else {
                    notices.quiet.to = minute;
                }
            }
            Field::Title(kind) => value
                .trim()
                .clone_into(&mut notices.template_mut(kind).title),
            Field::Message(kind) => value
                .trim()
                .clone_into(&mut notices.template_mut(kind).message),
            Field::Identity(_)
            | Field::ProjectName
            | Field::StartCommand
            | Field::NewWorktree
            | Field::GitLabHost
            | Field::ForgejoHost
            | Field::ForgejoToken
            | Field::ScheduleName
            | Field::ScheduleTime
            | Field::SchedulePrompt => {
                return Task::none();
            }
        }
        self.save_preferences()
    }

    /// Uses the theme `id` while the window is in its mode.
    fn pick_theme(&mut self, id: String) -> Task<Message> {
        let Some(mode) = self.themes.iter().find(|t| t.id == id).map(|t| t.mode) else {
            return Task::none();
        };
        match mode {
            Mode::Dark => self.preferences.dark_theme = id,
            Mode::Light => self.preferences.light_theme = id,
        }
        // A fixed appearance turns to the theme picked, so it shows; System
        // keeps following the desktop.
        if self.preferences.appearance != Appearance::System {
            self.preferences.appearance = match mode {
                Mode::Dark => Appearance::Dark,
                Mode::Light => Appearance::Light,
            };
        }
        self.retheme();
        self.save_preferences()
    }

    /// Applies something that happened in the setup guide.
    fn apply_guide(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::OpenSetup => return self.open_setup(),
            Event::Step(step) => self.sheets.step = step,
            Event::Next => match Step::ALL.get(self.sheets.step.index() + 1) {
                Some(next) => self.sheets.step = *next,
                None => self.sheets.open = None,
            },
            Event::Back => {
                if let Some(back) = self.sheets.step.index().checked_sub(1) {
                    self.sheets.step = Step::ALL[back];
                }
            }
            Event::InstallOpenCode => return self.install_opencode(),
            Event::Git(setup) => self.sheets.git = Load::Loaded(setup),
            Event::IdentitySet(identity, result) => match result {
                Ok(value) => {
                    self.sheets.git_error = None;
                    if let Load::Loaded(setup) = &mut self.sheets.git {
                        match identity {
                            Identity::Name => setup.name = value,
                            Identity::Email => setup.email = value,
                        }
                    }
                }
                Err(why) => self.sheets.git_error = Some(why),
            },
            _ => {}
        }
        Task::none()
    }

    /// Opens the setup guide at its first step, and reads what it shows.
    fn open_setup(&mut self) -> Task<Message> {
        self.sheets.open = Some(Sheet::Setup);
        self.sheets.step = Step::OpenCode;
        Task::batch([self.load_git(), self.load_setup()])
    }

    /// Reads what Settings shows from the server and this computer.
    fn open_settings(&mut self) -> Task<Message> {
        Task::batch([self.load_git(), self.load_setup(), self.load_schedules()])
    }

    /// Whether the guide offers to install OpenCode: when the background
    /// service on this computer can't be reached.
    #[must_use]
    pub fn offers_install(&self) -> bool {
        matches!(self.location, Location::Shared)
            && matches!(self.status, super::Status::Lost { .. })
    }

    /// Runs OpenCode's installer in a terminal, and opens the guide again
    /// once it ends. Harlo then starts the service and connects as usual.
    fn install_opencode(&mut self) -> Task<Message> {
        if !self.offers_install() {
            return Task::none();
        }
        // The terminal opens under the window, so the guide steps aside.
        self.sheets.open = None;
        self.run_in_terminal(
            "Install OpenCode",
            "/bin/sh",
            vec![
                "-c".to_owned(),
                format!("{OPENCODE_INSTALL}; {}", super::claude::KEEP_OPEN),
            ],
            Message::Sheet(Event::OpenSetup),
        )
    }

    /// Saves `value` as the name or email on commits, in git's global
    /// config, off the UI thread.
    fn set_identity(&mut self, identity: Identity, value: &str) -> Task<Message> {
        let value = value.trim().to_owned();
        if let Load::Loaded(setup) = &self.sheets.git {
            let current = match identity {
                Identity::Name => &setup.name,
                Identity::Email => &setup.email,
            };
            if *current == value {
                return Task::none();
            }
        }
        self.sheets.git_error = None;
        let global = self.git_config.clone();
        Task::perform(
            async move {
                let result = global.set(identity, &value).await;
                result
                    .map(|()| value)
                    .map_err(|e| format!("Couldn't save it: {e}"))
            },
            move |result| Message::Sheet(Event::IdentitySet(identity, result)),
        )
    }

    /// Turns starting at login on or off, off the UI thread.
    fn toggle_autostart(&mut self) -> Task<Message> {
        let Some(Autostart {
            folder,
            program,
            on,
        }) = self.autostart.clone()
        else {
            return Task::none();
        };
        Task::perform(autostart::set_later(folder, program, !on), move |result| {
            Message::Sheet(Event::AutostartSet(
                result
                    .map(|()| !on)
                    .map_err(|e| format!("Couldn't change the autostart entry: {e}")),
            ))
        })
    }

    /// Saves Harlo's own settings, off the UI thread; a failed save keeps
    /// them for this run, and the next change tries again.
    pub(super) fn save_preferences(&self) -> Task<Message> {
        match self.store.clone() {
            Some(store) => {
                Task::future(store.save_preferences_later(self.preferences.clone())).discard()
            }
            None => Task::none(),
        }
    }

    /// Asks for the stats over the chosen range.
    fn load_stats(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.sheets.stats = Load::Failed("Not connected to OpenCode yet.".to_owned());
            return Task::none();
        };
        self.sheets.serial += 1;
        let serial = self.sheets.serial;
        if !matches!(self.sheets.stats, Load::Loaded(_)) {
            self.sheets.stats = Load::Loading;
        }
        let from = self.sheets.range.span().and_then(|span| {
            let since = self
                .now
                .checked_sub(span)?
                .duration_since(UNIX_EPOCH)
                .ok()?;
            u64::try_from(since.as_millis()).ok()
        });
        Task::future(async move {
            let result = client.stats(from).await.map_err(|e| e.to_string());
            Message::Sheet(Event::Stats(serial, result))
        })
    }

    /// Asks for the server's set-up in the current folder.
    pub(super) fn load_setup(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.sheets.setup = Load::Failed("Not connected to OpenCode yet.".to_owned());
            return Task::none();
        };
        self.sheets.serial += 1;
        let serial = self.sheets.serial;
        if !matches!(self.sheets.setup, Load::Loaded(_)) {
            self.sheets.setup = Load::Loading;
        }
        let folder = self.folder();
        Task::future(async move {
            let text = |e: harlo_core::server::Error| e.to_string();
            let (providers, search, mcp, plugins) = futures_util::join!(
                client.providers(&folder),
                client.web_search(&folder),
                client.mcp_servers(&folder),
                client.plugins(&folder),
            );
            let setup = SetUp {
                providers: providers.map_err(text),
                search: search.map_err(text),
                mcp: mcp.map_err(text),
                plugins: plugins.map_err(text),
            };
            Message::Sheet(Event::SetUp(serial, Box::new(setup)))
        })
    }
}
