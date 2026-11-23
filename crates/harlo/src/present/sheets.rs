//! The pages over the window: the stats, and Settings with its sections
//! (Harlo's own settings, the shortcuts, the server's set-up, About) and
//! its search.

// Rust guideline compliant 2026-10-09

use std::rc::Rc;

use harlo_core::server::{Day, Location, Stats, Tokens};
use slint::{ModelRc, SharedString, VecModel};

use lucide_icons::Icon;

use super::marks::{Marks, project_color};
use super::{glyph, sync};

mod agents;
mod local_models;
mod mcp;
mod phone;
mod plugins;
mod providers;
mod schedules;
mod voice;
use harlo_core::claude::{ClaudeCode, Limit, Login, Severity};
use harlo_core::theme::{Mode, Scheme};

use crate::app::{
    App, ClaudeAction, Field, Load, OPENCODE_INSTALL, Range, Section, SetUp, Setting,
    SettingChoice, Sheet, Status, Step, epoch_seconds, resets_in,
};
use crate::model::{compact as tokens_text, dollars as money};
use crate::theme::color;
use crate::ui::{DayBar, SettingsSection, SheetRow, StatTile, State, ThemeCard};
use harlo_core::git::Identity;
use harlo_core::hosting::{Account, Service};
use harlo_core::notifications::{Kind, VARIABLES, clock};

/// How many days of activity the bars show.
const DAYS: usize = 30;

/// The keys, by where they work: each a heading, then keys and what they do.
const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Window",
        &[
            ("Ctrl+N", "Start a new session"),
            ("Ctrl+W", "Close the tab"),
            ("Ctrl+,", "Open or close Settings"),
            (
                "Ctrl+Q",
                "Quit Harlo, even while it keeps running in the tray",
            ),
            ("Ctrl+Tab, Ctrl+Page Down", "Next tab"),
            ("Ctrl+Shift+Tab, Ctrl+Page Up", "Previous tab"),
            ("Esc", "Close a page or menu, or put a maximized panel back"),
        ],
    ),
    (
        "Message",
        &[
            ("Enter", "Send, or queue while the session works"),
            ("Shift+Enter", "New line"),
            ("Ctrl+Enter", "Steer the working session"),
            ("Tab, Shift+Tab", "Next or previous agent"),
            ("/", "Commands and skills"),
            ("@", "Files and MCP resources"),
            ("Ctrl+V", "Paste text, or attach a picture"),
            (
                "Ctrl+Shift+D",
                "Dictate: start, then stop and put in the words",
            ),
        ],
    ),
    (
        "Approvals",
        &[("Alt+A", "Allow the action once"), ("Alt+D", "Deny it")],
    ),
    (
        "Notes",
        &[("Ctrl+Enter", "Tick the todo on the cursor's line")],
    ),
    ("Editor", &[("Ctrl+S", "Save the file")]),
    (
        "Terminal",
        &[
            ("Ctrl+Shift+C", "Copy the selection"),
            ("Ctrl+Shift+V, Shift+Insert", "Paste"),
            ("Double click", "Select a word"),
            (
                "Ctrl+Tab, Ctrl+Page Up/Down",
                "Switch tabs; other keys go to the shell",
            ),
        ],
    ),
];

/// The pages' lists.
pub struct Sheets {
    rows: Rc<VecModel<SheetRow>>,
    tiles: Rc<VecModel<StatTile>>,
    days: Rc<VecModel<DayBar>>,
    dark_themes: Rc<VecModel<ThemeCard>>,
    light_themes: Rc<VecModel<ThemeCard>>,
    /// Project icons, shared with the sidebar.
    marks: Rc<Marks>,
}

impl std::fmt::Debug for Sheets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sheets").finish_non_exhaustive()
    }
}

impl Sheets {
    /// Gives the window the pages' lists.
    pub fn new(state: &State<'_>, marks: Rc<Marks>) -> Self {
        let sheets = Self {
            rows: Rc::new(VecModel::default()),
            tiles: Rc::new(VecModel::default()),
            days: Rc::new(VecModel::default()),
            dark_themes: Rc::new(VecModel::default()),
            light_themes: Rc::new(VecModel::default()),
            marks,
        };
        state.set_dark_theme_cards(ModelRc::from(Rc::clone(&sheets.dark_themes)));
        state.set_light_theme_cards(ModelRc::from(Rc::clone(&sheets.light_themes)));
        state.set_sheet_rows(ModelRc::from(Rc::clone(&sheets.rows)));
        state.set_stat_tiles(ModelRc::from(Rc::clone(&sheets.tiles)));
        state.set_stat_days(ModelRc::from(Rc::clone(&sheets.days)));
        state.set_harlo_version(env!("CARGO_PKG_VERSION").into());
        let sections: Vec<SettingsSection> = Section::ALL
            .iter()
            .map(|section| SettingsSection {
                title: section.title().into(),
                group: section.group().unwrap_or_default().into(),
                glyph: glyph(section_icon(*section)),
            })
            .collect();
        state.set_settings_sections(ModelRc::from(Rc::new(VecModel::from(sections))));
        let steps: Vec<SharedString> = Step::ALL.iter().map(|s| s.title().into()).collect();
        state.set_setup_steps(ModelRc::from(Rc::new(VecModel::from(steps))));
        let place = |section: Section| {
            Section::ALL
                .iter()
                .position(|s| *s == section)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or_default()
        };
        state.set_shortcuts_section(place(Section::Shortcuts));
        state.set_phone_section(place(Section::Phone));
        sheets
    }

    /// Fills the open page.
    pub fn show(&self, app: &App, state: &State<'_>) {
        let Some(sheet) = app.sheets.open else {
            state.set_sheet(0);
            sync(&self.dark_themes, Vec::new());
            sync(&self.light_themes, Vec::new());
            return;
        };
        let shown =
            sheet == Sheet::Settings || (sheet == Sheet::Setup && app.sheets.step == Step::Theme);
        let cards = |mode| {
            if shown {
                theme_cards(app, mode)
            } else {
                Vec::new()
            }
        };
        sync(&self.dark_themes, cards(Mode::Dark));
        sync(&self.light_themes, cards(Mode::Light));
        let (note, rows) = match sheet {
            Sheet::Stats => {
                state.set_sheet(1);
                state.set_sheet_title("Stats".into());
                state.set_stat_range(match app.sheets.range {
                    Range::Week => 0,
                    Range::Month => 1,
                    Range::All => 2,
                });
                match &app.sheets.stats {
                    Load::Idle | Load::Loading => ("Counting…".to_owned(), self.stats(None)),
                    Load::Failed(error) => (format!("Couldn't count: {error}"), self.stats(None)),
                    Load::Loaded(counted) => {
                        let note = if counted.sessions == 0 {
                            "No sessions in this time."
                        } else {
                            ""
                        };
                        (note.to_owned(), self.stats(Some(counted)))
                    }
                }
            }
            Sheet::Settings => {
                state.set_sheet(2);
                let section = app.sheets.section;
                state.set_settings_section(
                    Section::ALL
                        .iter()
                        .position(|s| *s == section)
                        .and_then(|i| i32::try_from(i).ok())
                        .unwrap_or(0),
                );
                // Only a change from Harlo's side is written back, so the
                // field keeps its cursor while the user types.
                if state.get_settings_query().as_str() != app.sheets.query {
                    state.set_settings_query(app.sheets.query.as_str().into());
                }
                if app.sheets.query.trim().is_empty() {
                    state.set_sheet_title(section.title().into());
                    let rows = if section == Section::Projects {
                        project_rows(app, &self.marks)
                    } else {
                        section_rows(app, section)
                    };
                    (setup_note(app, section), rows)
                } else {
                    state.set_sheet_title("Search results".into());
                    found(app, &app.sheets.query)
                }
            }
            Sheet::Setup => {
                state.set_sheet(3);
                let step = app.sheets.step;
                state.set_setup_step(int(step.index()));
                state.set_sheet_title(step.title().into());
                (step.about().to_owned(), step_rows(app, step))
            }
        };
        state.set_sheet_note(note.into());
        sync(&self.rows, rows);
    }

    /// Fills the stats' tiles and day bars, and returns the per-model rows.
    fn stats(&self, stats: Option<&Stats>) -> Vec<SheetRow> {
        let Some(stats) = stats else {
            sync(&self.tiles, Vec::new());
            sync(&self.days, Vec::new());
            return Vec::new();
        };
        let tile = |value: String, label: &str| StatTile {
            value: value.into(),
            label: label.into(),
        };
        sync(
            &self.tiles,
            vec![
                tile(stats.sessions.to_string(), "sessions"),
                tile(stats.prompts.to_string(), "prompts"),
                tile(stats.steps.to_string(), "model steps"),
                tile(tokens_text(stats.tokens.total()), "tokens"),
                tile(money(stats.cost), "cost"),
                tile(stats.active_days.to_string(), "active days"),
                tile(stats.streak.to_string(), "day streak"),
            ],
        );
        sync(&self.days, days(stats));
        let mut rows = Vec::new();
        if !stats.models.is_empty() {
            rows.push(heading("Models"));
            for model in &stats.models {
                rows.push(pair(
                    &format!("{}/{}", model.provider, model.model),
                    &format!(
                        "{} · {} · {}",
                        steps(model.steps),
                        tokens_summary(model.tokens),
                        money(model.cost)
                    ),
                    0,
                ));
            }
        }
        if stats.subagents > 0 {
            rows.push(note(&format!(
                "Sessions leave out the {} subagent sessions they started.",
                stats.subagents
            )));
        }
        rows
    }
}

/// A section's icon.
fn section_icon(section: Section) -> Icon {
    match section {
        Section::General => Icon::Settings,
        Section::Appearance => Icon::Palette,
        Section::Notifications => Icon::Bell,
        Section::Shortcuts => Icon::Keyboard,
        Section::Voice => Icon::Mic,
        Section::Projects => Icon::FolderGit2,
        Section::Git => Icon::GitBranch,
        Section::Models => Icon::Bot,
        Section::LocalModels => Icon::Cpu,
        Section::Agents => Icon::UsersRound,
        Section::Tools => Icon::Plug,
        Section::Schedules => Icon::CalendarClock,
        Section::Server => Icon::Server,
        Section::Phone => Icon::Smartphone,
        Section::About => Icon::Info,
    }
}

/// A section's rows.
fn section_rows(app: &App, section: Section) -> Vec<SheetRow> {
    match section {
        Section::General => general_rows(app),
        Section::Appearance => appearance_rows(app),
        Section::Notifications => notification_rows(app),
        Section::Shortcuts => shortcuts(),
        Section::Voice => voice::voice_rows(app),
        Section::LocalModels => local_models::local_rows(app),
        // Searched without their icons.
        Section::Projects => project_rows(app, &Marks::default()),
        Section::Git => git_settings_rows(app),
        Section::Models => {
            // A provider's sign-in page shows alone.
            if app.sheets.providers.shown.is_some() {
                return providers::sign_in_rows(app);
            }
            let mut rows = providers::provider_rows(app);
            rows.extend(claude_rows(app));
            rows
        }
        Section::Tools => {
            let setup = match &app.sheets.setup {
                Load::Loaded(setup) => Some(setup.as_ref()),
                Load::Idle | Load::Loading | Load::Failed(_) => None,
            };
            mcp::page_rows(app, setup)
                .or_else(|| setup.map(|setup| setup_rows(app, setup)))
                .unwrap_or_default()
        }
        Section::Agents => agents::agent_rows(app),
        Section::Schedules => schedules::schedule_rows(app),
        Section::Server => server_rows(app),
        Section::Phone => phone::phone_rows(app),
        Section::About => about_rows(app),
    }
}

/// How loading the server's set-up stands, for the sections that show it.
fn setup_note(app: &App, section: Section) -> String {
    if !matches!(section, Section::Models | Section::Tools) {
        return String::new();
    }
    match &app.sheets.setup {
        Load::Idle | Load::Loading => "Loading…".to_owned(),
        Load::Failed(error) => format!("Couldn't load it: {error}"),
        Load::Loaded(_) => String::new(),
    }
}

/// The rows of every section that match `query`, under their sections'
/// titles, and a note when none do.
fn found(app: &App, query: &str) -> (String, Vec<SheetRow>) {
    let needle = query.trim().to_lowercase();
    let matches = |row: &SheetRow| {
        matches!(row.kind, 1 | 3 | 4)
            && (row.name.to_lowercase().contains(&needle)
                || row.value.to_lowercase().contains(&needle))
    };
    let mut rows = Vec::new();
    for section in Section::ALL {
        let titled = section.title().to_lowercase().contains(&needle);
        let hits: Vec<SheetRow> = section_rows(app, section)
            .into_iter()
            .filter(|row| titled || matches(row))
            .filter(|row| row.kind != 0 && row.kind != THEMES)
            .collect();
        if !hits.is_empty() {
            rows.push(heading(section.title()));
            rows.extend(hits);
        }
    }
    // The themes found show after the rows, as cards.
    let themed = !themes_found(app, &needle).is_empty();
    if themed {
        rows.push(themes_row(app.mode()));
    }
    let note = if rows.is_empty() {
        format!("No setting matches “{}”.", query.trim())
    } else {
        String::new()
    };
    (note, rows)
}

/// The kind of row that shows the theme cards.
const THEMES: i32 = 5;

/// The kind of row with a text field and a button that saves it.
const FIELD: i32 = 6;

/// The kind of row that shows a command and a button that runs it.
const COMMAND: i32 = 7;

/// Light or dark and the themes for it, then the colours, text and motion.
fn appearance_rows(app: &App) -> Vec<SheetRow> {
    vec![
        choice(
            app,
            SettingChoice::Appearance,
            "Light or dark",
            "Harlo keeps a dark and a light theme; System follows the desktop.",
        ),
        themes_row(app.mode()),
        heading("Colours"),
        choice(
            app,
            SettingChoice::Contrast,
            "Contrast",
            "More sets text, hints and icons further apart from the background.",
        ),
        choice(
            app,
            SettingChoice::DiffColors,
            "Diff colours",
            "Added and removed lines.",
        ),
        switch(
            Setting::Glass,
            "Glass",
            "Menus and dialogs are translucent, blurring what's under them.",
            app.preferences.glass,
        ),
        heading("Text"),
        choice(
            app,
            SettingChoice::TextSize,
            "Text size",
            "The interface and the chat.",
        ),
        choice(
            app,
            SettingChoice::CodeSize,
            "Code size",
            "Code, diffs and the terminal.",
        ),
        choice(
            app,
            SettingChoice::ChatWidth,
            "Chat width",
            "How long the chat's lines get in a wide window.",
        ),
        heading("Motion"),
        choice(
            app,
            SettingChoice::Motion,
            "Animation",
            "Panels sliding, new things fading in, and work in progress breathing.",
        ),
    ]
}

/// The row of theme cards for `mode`.
fn themes_row(mode: Mode) -> SheetRow {
    let (name, setting) = match mode {
        Mode::Dark => ("Dark themes", 0),
        Mode::Light => ("Light themes", 1),
    };
    SheetRow {
        kind: THEMES,
        name: name.into(),
        setting,
        ..SheetRow::default()
    }
}

/// A row choosing one of `choice`'s options, with `what` under its name.
fn choice(app: &App, choice: SettingChoice, name: &str, what: &str) -> SheetRow {
    let options: Vec<SharedString> = choice.options().iter().map(|&o| o.into()).collect();
    SheetRow {
        kind: 4,
        name: name.into(),
        value: what.into(),
        setting: int(app.chosen(choice)),
        options: ModelRc::new(VecModel::from(options)),
        choice: int(SettingChoice::ALL
            .iter()
            .position(|&c| c == choice)
            .unwrap_or_default()),
        ..SheetRow::default()
    }
}

/// `n` as the UI's int, saturating.
fn int(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// The cards for the `mode` themes Settings shows: all of them in
/// Appearance, or those a search finds, and none unless the window is in
/// `mode`.
fn theme_cards(app: &App, mode: Mode) -> Vec<ThemeCard> {
    if mode != app.mode() {
        return Vec::new();
    }
    let needle = app.sheets.query.trim().to_lowercase();
    let guided = app.sheets.open == Some(Sheet::Setup);
    let themes = if needle.is_empty() || guided {
        let shown = if guided {
            app.sheets.step == Step::Theme
        } else {
            app.sheets.section == Section::Appearance
        };
        if !shown {
            return Vec::new();
        }
        app.themes.iter().collect()
    } else {
        themes_found(app, &needle)
    };
    let chosen = &app.theme_for(mode).id;
    themes
        .into_iter()
        .filter(|theme| theme.mode == mode)
        .map(|theme| {
            let t = theme.tokens();
            ThemeCard {
                id: theme.id.as_str().into(),
                name: theme.name.as_str().into(),
                chosen: theme.id == *chosen,
                background: color(t.background_base),
                sidebar: color(t.background_stronger),
                raised: color(t.surface_raised_strong),
                text: color(t.text_strong),
                weak: color(t.text_weak),
                brand: color(t.surface_brand_base),
                keyword: color(t.syntax_keyword),
                function: color(t.syntax_function),
                text_string: color(t.syntax_string),
            }
        })
        .collect()
}

/// The themes of the window's mode a search for `needle` (lower case)
/// finds: by name, or all of them for a search for themes or appearance.
fn themes_found<'a>(app: &'a App, needle: &str) -> Vec<&'a Scheme> {
    let all = ["theme", "themes", "appearance", "colours", "colors"]
        .iter()
        .any(|word| word.starts_with(needle) && needle.len() >= 3);
    app.themes
        .iter()
        .filter(|t| t.mode == app.mode())
        .filter(|t| all || t.name.to_lowercase().contains(needle))
        .collect()
}

/// Harlo's own general settings.
fn general_rows(app: &App) -> Vec<SheetRow> {
    let mut rows = model_rows(app);
    rows.extend([
        heading("Startup"),
        switch(
            Setting::ReopenTabs,
            "Reopen tabs",
            "Open the sessions that were open when Harlo last closed.",
            app.preferences.reopen_tabs,
        ),
    ]);
    if let Some(autostart) = &app.autostart {
        rows.push(switch(
            Setting::StartAtLogin,
            "Start at login",
            "Open Harlo when you log in to this computer.",
            autostart.on,
        ));
    }
    if let Some(why) = &app.sheets.autostart_error {
        rows.push(pair("Not changed", why, 2));
    }
    if app.tray {
        rows.push(switch(
            Setting::KeepInTray,
            "Keep running in the tray",
            "Closing the window leaves Harlo in the tray, still telling you when a \
             session needs you. Quit from the tray or with Ctrl+Q.",
            app.preferences.keep_in_tray,
        ));
    }
    let mut guide = pair("Setup guide", "OpenCode, models, git and the theme", 3);
    guide.action = "Open".into();
    guide.setting = crate::window::OPEN_SETUP;
    rows.extend([heading("Help"), guide]);
    rows
}

/// The model new sessions use, with a button to change it where Harlo may.
fn model_rows(app: &App) -> Vec<SheetRow> {
    let mut rows = vec![heading("New sessions")];
    let model = app.default_model_choice().map_or_else(
        || "Chosen by OpenCode".to_owned(),
        |m| format!("{} · {}", m.name, m.provider_id),
    );
    let mut row = pair("Model", &model, 0);
    if app.global_config().is_some() {
        row.action = "Change".into();
        row.setting = crate::window::CHOOSE_DEFAULT_MODEL;
    }
    rows.push(row);
    if let Some(why) = &app.sheets.model_error {
        rows.push(pair("Not changed", why, 2));
    }
    rows.push(note(if app.global_config().is_some() {
        "Set in OpenCode's own config, so it applies to every app using this server. A \
         project's opencode.json can choose another; the model picked in a session wins."
    } else {
        "This server keeps its default model in its own opencode.json, on the computer it \
         runs on."
    }));
    rows
}

/// The Claude subscription: Claude Code on this computer, its account, and
/// OpenCode's connection, each with the next step.
fn claude_rows(app: &App) -> Vec<SheetRow> {
    let claude = &app.claude;
    let mut rows = vec![
        heading("Claude subscription"),
        note(
            "Use a Claude Pro or Max plan for Claude models in OpenCode. Claude Code keeps the \
             sign-in; OpenCode runs it.",
        ),
    ];
    let local = app.location.is_local();
    if local {
        match &claude.code {
            Load::Idle | Load::Loading => rows.push(pair("Claude Code", "Checking…", 3)),
            Load::Failed(why) | Load::Loaded(ClaudeCode::Failed(why)) => {
                rows.push(acting(pair("Claude Code", why, 2), ClaudeAction::Check));
            }
            Load::Loaded(ClaudeCode::Missing) => {
                rows.push(acting(
                    pair("Claude Code", "Not installed", 2),
                    ClaudeAction::Install,
                ));
            }
            Load::Loaded(ClaudeCode::Installed { version, login }) => {
                rows.push(acting(pair("Claude Code", version, 0), ClaudeAction::Check));
                rows.push(account_row(login));
                rows.extend(usage_rows(app));
            }
        }
    }
    rows.push(connection_row(app));
    if let Some(error) = &claude.error {
        rows.push(note(&format!("That didn't work: {error}")));
    }
    if !local {
        rows.push(note(
            "This server runs on another computer, which uses its own Claude Code. Sign in \
             there with `claude auth login`.",
        ));
    }
    rows.push(note(
        "Connecting applies to every app that uses this OpenCode server.",
    ));
    rows
}

/// Who Claude Code is signed in as.
fn account_row(login: &Login) -> SheetRow {
    match login {
        Login::SignedOut => acting(pair("Account", "Not signed in", 3), ClaudeAction::SignIn),
        Login::Other { method } => acting(
            pair(
                "Account",
                &format!("Signed in with {method}, not a subscription"),
                2,
            ),
            ClaudeAction::SignIn,
        ),
        Login::Subscription(account) => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(email) = &account.email {
                parts.push(email.clone());
            }
            if let Some(plan) = &account.plan {
                parts.push(plan_name(plan));
            }
            if parts.is_empty() {
                parts.push("Signed in".to_owned());
            }
            let mut row = acting(pair("Account", &parts.join(" · "), 1), ClaudeAction::SignIn);
            row.action = "Switch account".into();
            row
        }
    }
}

/// How much of the plan's limits is used, a row each.
fn usage_rows(app: &App) -> Vec<SheetRow> {
    match &app.claude.usage {
        Load::Idle | Load::Loaded(None) => Vec::new(),
        Load::Loading => vec![pair("Plan usage", "Checking…", 3)],
        Load::Failed(why) => vec![pair("Plan usage", &format!("Couldn't read it: {why}"), 2)],
        Load::Loaded(Some(limits)) => {
            let now = epoch_seconds(app.now);
            limits
                .iter()
                .map(|limit| {
                    let mut value = format!("{}% used", limit.percent.round());
                    if let Some(at) = limit.resets_at {
                        value.push_str(" · resets ");
                        value.push_str(&resets_in(at - now));
                    }
                    let tone = match limit.severity {
                        Severity::Normal => 0,
                        Severity::Warning => 4,
                        Severity::Critical => 2,
                    };
                    pair(&limit_name(limit), &value, tone)
                })
                .collect()
        }
    }
}

/// What a limit is for, in words.
fn limit_name(limit: &Limit) -> String {
    let model = limit.model.as_deref();
    match (limit.kind.as_str(), model) {
        ("session", _) => "Current session".to_owned(),
        ("weekly_all", _) => "This week, all models".to_owned(),
        (_, Some(model)) if limit.group == "weekly" => format!("This week, {model}"),
        (_, model) => {
            let mut chars = limit.group.chars();
            let group: String = chars.next().map_or_else(
                || "Limit".to_owned(),
                |c| c.to_uppercase().chain(chars).collect(),
            );
            match model {
                Some(model) => format!("{group}, {model}"),
                None => group,
            }
        }
    }
}

/// A plan's name as Claude Code reports it (`max`), for people (`Max plan`).
fn plan_name(plan: &str) -> String {
    let mut chars = plan.chars();
    let first: String = chars
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    format!("{first}{} plan", chars.as_str())
}

/// Whether OpenCode is connected to Claude Code's sign-in.
fn connection_row(app: &App) -> SheetRow {
    let claude = &app.claude;
    match claude.busy {
        Some(ClaudeAction::Connect) => return pair("OpenCode", "Connecting…", 3),
        Some(ClaudeAction::Disconnect) => return pair("OpenCode", "Disconnecting…", 3),
        _ => {}
    }
    match &claude.agent {
        Load::Idle | Load::Loading => pair("OpenCode", "Checking…", 3),
        Load::Failed(why) => acting(pair("OpenCode", why, 2), ClaudeAction::Check),
        // A server that just started may not have loaded it yet.
        Load::Loaded(None) => acting(
            pair("OpenCode", "Claude Agent isn't available on this server", 2),
            ClaudeAction::Check,
        ),
        Load::Loaded(Some(agent)) => {
            let stored = agent.connections.iter().any(|c| c.credential.is_some());
            if let Some(why) = agent
                .connections
                .iter()
                .find_map(|c| c.needs_sign_in.as_ref())
            {
                let row = pair("OpenCode", &format!("Needs sign-in: {why}"), 2);
                if app.location.is_local() {
                    acting(row, ClaudeAction::SignIn)
                } else {
                    row
                }
            } else if stored {
                acting(pair("OpenCode", "Connected", 1), ClaudeAction::Disconnect)
            } else if agent.connections.is_empty() {
                acting(pair("OpenCode", "Not connected", 3), ClaudeAction::Connect)
            } else {
                // Connected through an environment variable, which only
                // the server's own set-up can remove.
                pair("OpenCode", "Connected", 1)
            }
        }
    }
}

/// `row` with a button for `action`.
fn acting(mut row: SheetRow, action: ClaudeAction) -> SheetRow {
    let (label, number) = match action {
        ClaudeAction::Install => ("Install", 0),
        ClaudeAction::SignIn => ("Sign in", 1),
        ClaudeAction::Connect => ("Connect", 2),
        ClaudeAction::Disconnect => ("Disconnect", 3),
        ClaudeAction::Check => ("Check again", 4),
    };
    row.action = label.into();
    row.setting = number;
    row
}

/// The shortcuts, by section.
fn shortcuts() -> Vec<SheetRow> {
    let mut rows = Vec::new();
    for (section, keys) in SHORTCUTS {
        rows.push(heading(section));
        for (key, what) in *keys {
            rows.push(pair(key, what, 0));
        }
    }
    rows
}

/// The server Harlo follows, and how the connection stands.
fn server_rows(app: &App) -> Vec<SheetRow> {
    let address = match &app.location {
        Location::Ssh { target } => format!("ssh {target}"),
        other => other.label(),
    };
    let mut rows = vec![pair("Address", &address, 0), status_row(app)];
    rows.push(note(
        "Harlo attaches to this shared OpenCode server and leaves it running when it closes. \
         Its settings apply to every app that uses it.",
    ));
    computer_rows(app, &mut rows);
    rows
}

/// The other computers Harlo follows over SSH, and adding one.
fn computer_rows(app: &App, rows: &mut Vec<SheetRow>) {
    rows.push(heading("Other computers"));
    rows.push(note(
        "Harlo follows OpenCode's shared service on another computer over SSH, with your SSH \
         config, keys and agent; it can't answer a password prompt. If no service runs there, \
         Harlo starts it, as it does here, and never stops it. Pick a computer in the list at \
         the top of the sidebar.",
    ));
    for (index, target) in app.preferences.computers.iter().enumerate() {
        let location = Location::Ssh {
            target: target.clone(),
        };
        let mut row = if app.location == location {
            pair(&location.label(), "Following", 1)
        } else {
            pair(&location.label(), &format!("ssh {target}"), 3)
        };
        row.action = "Remove".into();
        row.setting = crate::window::REMOVE_COMPUTER;
        row.choice = int(index);
        rows.push(row);
    }
    let mut add = field(
        Field::Computer,
        "Computer",
        "",
        "me@host, or a name from ~/.ssh/config",
    );
    add.action = "Add and follow".into();
    rows.push(add);
    if let Some(why) = &app.sheets.computer_error {
        rows.push(pair("Not added", why, 2));
    }
}

/// How the connection to the server stands.
fn status_row(app: &App) -> SheetRow {
    let (status, tone) = match &app.status {
        Status::Connected { version } => (format!("Connected · OpenCode {version}"), 1),
        Status::Connecting { .. } => ("Connecting…".to_owned(), 3),
        Status::Lost { reason, .. } => (reason.clone(), 2),
    };
    pair("Status", &status, tone)
}

/// A step of the setup guide.
fn step_rows(app: &App, step: Step) -> Vec<SheetRow> {
    match step {
        Step::OpenCode => opencode_rows(app),
        Step::Models => {
            let mut rows = model_rows(app);
            rows.extend(section_rows(app, Section::Models));
            let loading = setup_note(app, Section::Models);
            if !loading.is_empty() {
                rows.push(note(&loading));
            }
            rows
        }
        Step::Git => {
            let mut rows = git_rows(app);
            rows.extend(hosting_rows(app));
            rows
        }
        Step::Theme => vec![
            choice(
                app,
                SettingChoice::Appearance,
                "Light or dark",
                "System follows the desktop.",
            ),
            themes_row(app.mode()),
        ],
    }
}

/// The server, how the connection stands, and OpenCode's installer when
/// the service on this computer can't be reached.
fn opencode_rows(app: &App) -> Vec<SheetRow> {
    let mut rows = vec![pair("Server", &app.location.label(), 0), status_row(app)];
    if app.offers_install() {
        rows.extend([
            heading("Install OpenCode"),
            note(
                "If OpenCode isn't on this computer yet, its installer sets it up. Harlo then \
                 starts the background service and connects on its own.",
            ),
            SheetRow {
                kind: COMMAND,
                value: OPENCODE_INSTALL.into(),
                action: "Install".into(),
                setting: crate::window::INSTALL_OPENCODE,
                ..SheetRow::default()
            },
        ]);
    } else if matches!(app.location, Location::Shared) {
        rows.push(note(
            "Harlo attaches to OpenCode's background service and leaves it running when it \
             closes.",
        ));
    } else {
        rows.push(note(
            "Harlo follows this server and leaves it running when it closes.",
        ));
    }
    rows
}

/// Settings → Git: git and the name on commits, undo's snapshots, and
/// the model that writes commit messages.
fn git_settings_rows(app: &App) -> Vec<SheetRow> {
    let mut rows = git_rows(app);
    rows.extend(hosting_rows(app));
    rows.push(heading("Undo a turn"));
    let shared = app.global_config().is_some();
    match &app.sheets.git_settings.snapshots {
        _ if !shared => rows.push(note(
            "This server keeps its snapshot setting in its own opencode.json, on the computer \
             it runs on.",
        )),
        Load::Loaded(on) => rows.push(switch(
            Setting::Snapshots,
            "Keep snapshots",
            "Before each turn, OpenCode keeps a snapshot of the project's files, so Undo can put \
             them back. Off, Undo takes back the conversation only.",
            *on,
        )),
        Load::Failed(why) => rows.push(pair("Snapshots", why, 2)),
        Load::Idle | Load::Loading => rows.push(pair("Snapshots", "Checking…", 3)),
    }
    if let Some(why) = &app.sheets.git_settings.error {
        rows.push(pair("Not changed", why, 2));
    }
    if shared {
        rows.push(note(
            "Set in OpenCode's own config, so it applies to every app using this server.",
        ));
    }

    rows.push(heading("Commit messages"));
    let chosen = app.commit_model_choice();
    let mut model = pair(
        "Model",
        &chosen.as_ref().map_or_else(
            || "OpenCode's default".to_owned(),
            |m| format!("{} · {}", m.name, m.provider_id),
        ),
        0,
    );
    model.action = "Change".into();
    model.setting = crate::window::CHOOSE_COMMIT_MODEL;
    rows.push(model);
    if chosen.is_some() {
        let mut reset = pair("", "Let OpenCode choose again", 3);
        reset.action = "Use the default".into();
        reset.setting = crate::window::DEFAULT_COMMIT_MODEL;
        rows.push(reset);
    }
    rows.push(note(
        "The ✦ button in the Changes panel writes the message for every uncommitted file, \
         from their diffs alone: no tools, and nothing kept in a session. A small, fast model \
         is enough.",
    ));
    rows
}

/// Signing git in to GitHub, GitLab and Forgejo, where Harlo has the tools
/// to.
fn hosting_rows(app: &App) -> Vec<SheetRow> {
    if app.hosting.is_none() {
        return Vec::new();
    }
    let mut rows = vec![
        heading("Git hosting"),
        note(
            "Sign in so git can push and pull over HTTPS; SSH remotes keep using your SSH \
             keys. The GitHub CLI, glab or git keeps the sign-in: Harlo keeps no token.",
        ),
    ];
    rows.extend(service_rows(app, Service::GitHub));
    rows.extend(service_rows(app, Service::GitLab));
    rows.push(field(
        Field::GitLabHost,
        "Server",
        &app.preferences.gitlab_host,
        "gitlab.com",
    ));

    let state = &app.sheets.hosting;
    rows.push(match app.forgejo_account() {
        Some(account) => {
            let mut row = pair("Forgejo", &format!("Signed in as {account}"), 1);
            row.action = "Sign out".into();
            row.setting = crate::window::SIGN_OUT;
            row.choice = service_index(Service::Forgejo);
            row
        }
        None if state.checking_token => pair("Forgejo", "Checking the token…", 3),
        None => pair("Forgejo", "Not signed in", 3),
    });
    rows.extend(outcome_row(app, Service::Forgejo));
    rows.push(field(
        Field::ForgejoHost,
        "Server",
        &app.preferences.forgejo_host,
        "codeberg.org",
    ));
    if app.forgejo_account().is_none() {
        let mut token = field(
            Field::ForgejoToken,
            "Token",
            "",
            "An access token, from the server's settings",
        );
        token.action = "Sign in".into();
        token.on = true;
        rows.push(token);
        rows.push(note(
            "Forgejo and Gitea sign in with an access token. git keeps it for that server, in \
             the desktop keyring when git has its helper.",
        ));
    }
    rows
}

/// GitHub's or GitLab's row: who is signed in, or the sign-in going on.
fn service_rows(app: &App, service: Service) -> Vec<SheetRow> {
    let state = &app.sheets.hosting;
    let label = service.label();
    let index = service_index(service);
    if let Some(going) = state.signing_in.as_ref().filter(|g| g.service == service) {
        let mut rows = Vec::new();
        let mut waiting = match &going.code {
            Some((code, url)) => {
                let page = url.trim_start_matches("https://");
                let mut row = pair(label, &format!("Enter {code} at {page}"), 0);
                row.action = "Copy and open".into();
                row.setting = crate::window::OPEN_SIGN_IN_PAGE;
                row.copy = code.as_str().into();
                rows.push(row);
                pair("", "Waiting for you to finish on the site…", 3)
            }
            None => pair(label, "Starting…", 3),
        };
        waiting.action = "Cancel".into();
        waiting.setting = crate::window::CANCEL_SIGN_IN;
        rows.push(waiting);
        return rows;
    }
    let mut row = match state.accounts.get(&service) {
        Some(Load::Loaded(Account::SignedIn(name))) => {
            let mut row = pair(label, &format!("Signed in as {name}"), 1);
            row.action = "Sign out".into();
            row.setting = crate::window::SIGN_OUT;
            row
        }
        Some(Load::Loaded(Account::SignedOut)) => {
            let mut row = pair(label, "Not signed in", 3);
            if state.signing_in.is_none() {
                row.action = "Sign in".into();
                row.setting = crate::window::SIGN_IN;
            }
            row
        }
        Some(Load::Loaded(Account::Missing)) => pair(
            label,
            match service {
                Service::GitHub => "Needs gh, the GitHub CLI",
                Service::GitLab | Service::Forgejo => "Needs glab, GitLab's CLI",
            },
            2,
        ),
        Some(Load::Loaded(Account::NoAnswer)) => {
            let tool = service.cli().unwrap_or_default();
            pair(label, &format!("{tool} didn't say who is signed in"), 4)
        }
        Some(Load::Failed(why)) => pair(label, why, 2),
        Some(Load::Idle | Load::Loading) | None => pair(label, "Checking…", 3),
    };
    row.choice = index;
    let mut rows = vec![row];
    rows.extend(outcome_row(app, service));
    rows
}

/// What the last sign-in or sign-out with `service` did, if it was that one.
fn outcome_row(app: &App, service: Service) -> Option<SheetRow> {
    match &app.sheets.hosting.outcome {
        Some((done, Ok(said))) if *done == service => Some(pair("", said, 1)),
        Some((done, Err(why))) if *done == service => Some(pair("", why, 2)),
        _ => None,
    }
}

/// A hosting service's number, as Settings buttons carry it.
fn service_index(service: Service) -> i32 {
    int(Service::ALL
        .iter()
        .position(|s| *s == service)
        .unwrap_or_default())
}

/// Git's version, and fields for the name and email on commits.
fn git_rows(app: &App) -> Vec<SheetRow> {
    let setup = match &app.sheets.git {
        Load::Loaded(setup) => setup,
        Load::Idle | Load::Loading => return vec![pair("Git", "Checking…", 3)],
        Load::Failed(why) => return vec![pair("Git", why, 2)],
    };
    let Some(version) = &setup.version else {
        return vec![
            pair("Git", "Not found", 2),
            note(
                "Install git with your system's package manager, then open this guide again \
                 from Settings → General.",
            ),
        ];
    };
    let mut rows = vec![
        pair("Git", version, 1),
        heading("Your name on commits"),
        field(
            Field::Identity(Identity::Name),
            "Name",
            &setup.name,
            "Your name",
        ),
        field(
            Field::Identity(Identity::Email),
            "Email",
            &setup.email,
            "you@example.com",
        ),
    ];
    if let Some(why) = &app.sheets.git_error {
        rows.push(pair("Not saved", why, 2));
    }
    rows.push(note(
        "Saved in git's global config, for every repository on this computer.",
    ));
    rows
}

/// A text field named `name` holding `value`, with `hint` while empty.
fn field(field: Field, name: &str, value: &str, hint: &str) -> SheetRow {
    SheetRow {
        kind: FIELD,
        name: name.into(),
        value: value.into(),
        hint: hint.into(),
        action: "Save".into(),
        setting: int(Field::ALL
            .iter()
            .position(|f| *f == field)
            .unwrap_or_default()),
        ..SheetRow::default()
    }
}

/// A text field that takes what's typed as it's typed, with no button.
fn live_field(which: Field, name: &str, value: &str, hint: &str) -> SheetRow {
    let mut row = field(which, name, value, hint);
    row.action = "".into();
    row
}

/// The kind of row with colour swatches, one per option of its choice.
const SWATCHES: i32 = 8;

/// The kind of row that shows a project's mark, name and folder, and opens
/// its settings.
const PROJECT: i32 = 9;

/// Settings → Projects: the list of projects, or the one shown.
fn project_rows(app: &App, marks: &Marks) -> Vec<SheetRow> {
    let page = &app.sheets.projects;
    let shown = page
        .shown
        .as_deref()
        .and_then(|folder| app.projects.iter().find(|p| p.directory == folder));
    let Some(project) = shown else {
        return project_list(app, marks);
    };
    let server = app.server_project(&project.directory);
    let mut back = pair(&project.name, &home_relative(&project.directory), 3);
    back.action = "All projects".into();
    back.setting = crate::window::ALL_PROJECTS;
    let mut rows = vec![back];
    if let Some(why) = &page.error {
        rows.push(pair("Not changed", why, 2));
    }

    rows.push(heading("On every app"));
    if server.is_none() {
        rows.push(note(
            "OpenCode learns about the project with its first session; these wait until then.",
        ));
    } else {
        let name = server.and_then(|p| p.name.clone()).unwrap_or_default();
        rows.push(field(
            Field::ProjectName,
            "Name",
            &name,
            harlo_core::sessions::folder_name(&project.directory),
        ));
        rows.push(swatches(app));
        let mut icon = pair(
            "Icon",
            if project.icon.is_some() {
                "An image you chose"
            } else {
                "The folder, in the project's colour"
            },
            3,
        );
        icon.action = "Choose…".into();
        icon.setting = crate::window::CHOOSE_ICON;
        rows.push(icon);
        if project.icon.is_some() {
            let mut clear = pair("", "Show the folder again", 3);
            clear.action = "Remove image".into();
            clear.setting = crate::window::CLEAR_ICON;
            rows.push(clear);
        }
        rows.push(note(
            "Kept by the OpenCode server, so every app using it shows the same name, colour and icon.",
        ));
    }

    rows.push(heading("New sessions"));
    let chosen = app.shown_project_model();
    let mut model = pair(
        "Model",
        &chosen.as_ref().map_or_else(
            || "OpenCode's default".to_owned(),
            |m| format!("{} · {}", m.name, m.provider_id),
        ),
        0,
    );
    model.action = "Change".into();
    model.setting = crate::window::CHOOSE_PROJECT_MODEL;
    rows.push(model);
    if chosen.is_some() {
        let mut reset = pair("", "Let OpenCode choose again", 3);
        reset.action = "Use the default".into();
        reset.setting = crate::window::DEFAULT_PROJECT_MODEL;
        rows.push(reset);
    }
    rows.push(note(
        "Kept by Harlo on this computer. A model picked in the composer still wins.",
    ));

    if server.is_some_and(|p| p.git) {
        rows.extend(worktree_rows(app, project));
    }

    rows.extend([
        heading("Remove from Harlo"),
        choice(
            app,
            SettingChoice::ProjectSessions,
            "Its sessions",
            "Archived sessions leave the lists but stay on the server.",
        ),
    ]);
    let mut remove = pair(
        "Remove from the sidebar",
        "The folder and its files stay. + Project brings it back.",
        3,
    );
    remove.action = "Remove".into();
    remove.setting = crate::window::HIDE_PROJECT;
    rows.push(remove);
    rows
}

/// The shown project's worktrees, the button to make one, and the command
/// a new one runs.
fn worktree_rows(app: &App, project: &harlo_core::sessions::Project) -> Vec<SheetRow> {
    let page = &app.sheets.projects;
    let mut rows = vec![
        heading("Worktrees"),
        note(
            "More checkouts of the project, each on a branch of its own, so agents can work side by side. Each shows in the sidebar under the project.",
        ),
    ];
    for (index, worktree) in project.worktrees.iter().enumerate() {
        let mut row = pair(&worktree.name, &home_relative(&worktree.directory), 3);
        row.action = "Remove".into();
        row.setting = crate::window::REMOVE_WORKTREE;
        row.choice = int(index);
        rows.push(row);
        if page.needs_force.as_deref() == Some(worktree.directory.as_str()) {
            let mut force = pair("", "It has changes; removing it throws them away.", 4);
            force.action = "Remove anyway".into();
            force.setting = crate::window::FORCE_REMOVE_WORKTREE;
            force.choice = int(index);
            rows.push(force);
        }
    }
    let mut make = field(
        Field::NewWorktree,
        "New",
        "",
        "Its name, or empty for one OpenCode picks",
    );
    make.action = "Make".into();
    rows.push(make);
    if page.making {
        rows.push(pair("", "Making the worktree…", 3));
    }
    let start = app
        .server_project(&project.directory)
        .and_then(|p| p.start.clone())
        .unwrap_or_default();
    rows.extend([
        field(
            Field::StartCommand,
            "Setup",
            &start,
            "A command each new worktree runs, such as npm install",
        ),
        note("Kept by the OpenCode server, which runs it in each new worktree."),
    ]);
    rows
}

/// Every project, to choose one.
fn project_list(app: &App, marks: &Marks) -> Vec<SheetRow> {
    let hidden = app.hidden_projects().len();
    let removed = note(&format!(
        "{hidden} removed from the sidebar; + Project with its folder brings one back."
    ));
    if app.projects.is_empty() {
        let mut rows = vec![note(
            "No projects yet. Add a folder with + Project in the sidebar.",
        )];
        if hidden > 0 {
            rows.push(removed);
        }
        return rows;
    }
    let mut rows = vec![note("Choose a project to change it.")];
    for (index, project) in app.projects.iter().enumerate() {
        rows.push(SheetRow {
            kind: PROJECT,
            name: project.name.as_str().into(),
            value: home_relative(&project.directory).into(),
            color: project
                .color
                .as_deref()
                .and_then(project_color)
                .unwrap_or_default(),
            on: project.color.is_some(),
            picture: marks.image(project.icon.as_deref()),
            setting: crate::window::PICK_PROJECT,
            choice: int(index),
            ..SheetRow::default()
        });
    }
    if hidden > 0 {
        rows.push(removed);
    }
    rows
}

/// The shown project's colour as swatches: none, then OpenCode's colours.
fn swatches(app: &App) -> SheetRow {
    let mut row = choice(app, SettingChoice::ProjectColor, "Colour", "");
    row.kind = SWATCHES;
    let colors: Vec<slint::Color> = std::iter::once(slint::Color::default())
        .chain(
            harlo_core::server::PROJECT_COLORS
                .iter()
                .filter_map(|name| project_color(name)),
        )
        .collect();
    row.colors = ModelRc::new(VecModel::from(colors));
    row
}

/// `folder` with the home folder as `~`.
fn home_relative(folder: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => match folder.strip_prefix(&home) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => folder.to_owned(),
        },
        _ => folder.to_owned(),
    }
}

/// Settings → Notifications: delivery, events, quiet hours and templates.
fn notification_rows(app: &App) -> Vec<SheetRow> {
    let notices = &app.preferences.notifications;
    let mut rows = vec![
        note("Clicking a notification opens its session."),
        switch(
            Setting::Notify,
            "Notifications",
            "When a run ends or an agent needs you.",
            notices.enabled,
        ),
    ];
    if !notices.enabled {
        return rows;
    }
    let mut test = pair("Test notification", "See how one looks on this desktop", 3);
    test.action = "Try it".into();
    test.setting = crate::window::TEST_NOTICE;
    rows.extend([
        switch(
            Setting::NotifyFocused,
            "While Harlo is focused",
            "Never for the session you're looking at.",
            notices.while_focused,
        ),
        switch(
            Setting::NotifySound,
            "Sound",
            "The desktop's message sound.",
            notices.sound,
        ),
        test,
        heading("Events"),
    ]);
    for kind in Kind::ALL {
        let (name, what) = event_name(kind);
        rows.push(switch(
            Setting::NotifyOn(kind),
            name,
            what,
            notices.template(kind).on,
        ));
    }
    rows.extend([
        switch(
            Setting::NotifyLimits,
            "Plan limits",
            "A Claude plan limit is used up, or one you'd nearly used resets.",
            notices.limits,
        ),
        heading("Quiet hours"),
        switch(
            Setting::Quiet,
            "Quiet hours",
            "No notifications between these times.",
            notices.quiet.on,
        ),
    ]);
    if notices.quiet.on {
        rows.extend([
            field(
                Field::QuietFrom,
                "From",
                &clock(notices.quiet.from),
                "22:00",
            ),
            field(Field::QuietTo, "To", &clock(notices.quiet.to), "08:00"),
        ]);
        if let Some(why) = &app.sheets.notice_error {
            rows.push(pair("Not changed", why, 2));
        }
        rows.push(switch(
            Setting::QuietQuestions,
            "Let questions through",
            "Questions and approvals still notify.",
            notices.quiet.questions,
        ));
    }
    let variables: Vec<String> = VARIABLES.iter().map(|v| format!("{{{v}}}")).collect();
    rows.extend([
        heading("Templates"),
        note(&format!(
            "Leave a field empty for the default, shown faded. Variables: {}.",
            variables.join(" ")
        )),
    ]);
    for kind in Kind::ALL {
        let template = notices.template(kind);
        let (title, message) = kind.defaults();
        let event = event_name(kind).0;
        let mut title_row = field(Field::Title(kind), "Title", &template.title, title);
        title_row.label = format!("{event} title").into();
        let mut message_row = field(Field::Message(kind), "Message", &template.message, message);
        message_row.label = format!("{event} message").into();
        rows.extend([heading(event), title_row, message_row]);
    }
    rows
}

/// An event's name in Settings, and when it happens.
fn event_name(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Finished => ("Run finished", "An agent finished what you asked."),
        Kind::Failed => ("Run failed", "A run stopped with an error."),
        Kind::Asks => ("Agent needs you", "A question, or an action to approve."),
    }
}

/// Harlo's version and the server's.
fn about_rows(app: &App) -> Vec<SheetRow> {
    let mut rows = vec![
        heading("Harlo"),
        pair("Version", env!("CARGO_PKG_VERSION"), 0),
    ];
    if let Status::Connected { version } = &app.status {
        rows.push(pair("OpenCode", version, 0));
    }
    rows.push(heading("Colour themes"));
    rows.push(note(
        "The themes besides Harlo's are Base16 colour schemes from Tinted Theming \
         (github.com/tinted-theming/schemes), each by the author it credits, used under \
         this licence:",
    ));
    rows.extend(
        harlo_core::theme::TINTED_LICENSE
            .split("\n\n")
            .map(|paragraph| paragraph.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|paragraph| !paragraph.is_empty())
            .map(|paragraph| note(&paragraph)),
    );
    rows
}

/// The server's web search, MCP servers and plugins, for Tools.
fn setup_rows(app: &App, setup: &SetUp) -> Vec<SheetRow> {
    let mut rows = Vec::new();
    section(&mut rows, "Web search", &setup.search, |s| {
        pair(&s.name, "", 0)
    });
    mcp::list_rows(app, setup, &mut rows);
    mcp::own_tools_rows(app, &mut rows);
    plugins::plugin_rows(app, &setup.plugins, &mut rows);
    rows.push(shared_note());
    rows
}

/// That the server's set-up is shared.
fn shared_note() -> SheetRow {
    note("These belong to the shared OpenCode server; every app that uses it sees the same ones.")
}

/// A heading and a row per item, or why the items couldn't be listed.
fn section<T>(
    rows: &mut Vec<SheetRow>,
    title: &str,
    items: &Result<Vec<T>, String>,
    row: impl Fn(&T) -> SheetRow,
) {
    rows.push(heading(title));
    match items {
        Ok(items) if items.is_empty() => rows.push(note("None.")),
        Ok(items) => rows.extend(items.iter().map(row)),
        Err(error) => rows.push(note(&format!("Couldn't list them: {error}"))),
    }
}

/// The steps of the [`DAYS`] days up to the range's end, each placed by
/// its date, as shares of the busiest.
fn days(stats: &Stats) -> Vec<DayBar> {
    /// Milliseconds in a day.
    const DAY_MS: u64 = 24 * 60 * 60 * 1000;
    let dated: Vec<(i64, &Day)> = stats
        .activity
        .iter()
        .filter_map(|day| Some((day_number(&day.date)?, day)))
        .collect();
    // The server dates days in its own time zone; the end is the later of
    // the last active day and the range's end in UTC.
    let end_utc = i64::try_from(stats.to / DAY_MS).unwrap_or(0);
    let end = dated
        .iter()
        .map(|(n, _)| *n)
        .max()
        .unwrap_or(end_utc)
        .max(end_utc);
    let shown: Vec<(i64, &Day)> = dated
        .into_iter()
        .filter(|(n, _)| end - n < i64::try_from(DAYS).unwrap_or(i64::MAX))
        .collect();
    let most = shown.iter().map(|(_, d)| d.steps).max().unwrap_or(0).max(1);
    shown
        .iter()
        .map(|(n, day)| DayBar {
            label: format!("{}: {}", day.date, steps(day.steps)).into(),
            slot: i32::try_from(i64::try_from(DAYS).unwrap_or(0) - 1 - (end - n)).unwrap_or(0),
            #[expect(
                clippy::cast_precision_loss,
                reason = "step counts are far below where f32 loses whole numbers that matter here"
            )]
            share: day.steps as f32 / most as f32,
        })
        .collect()
}

/// Days since 1970-01-01 of a `YYYY-MM-DD` date, in the proleptic Gregorian
/// calendar.
fn day_number(date: &str) -> Option<i64> {
    /// Days from 0000-03-01 to 1970-01-01.
    const EPOCH: i64 = 719_468;
    let mut parts = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Counts years from March, so the leap day ends a year.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    Some(era * 146_097 + of_era_days - EPOCH)
}

/// A count of model steps, as words.
fn steps(n: u64) -> String {
    if n == 1 {
        "1 step".to_owned()
    } else {
        format!("{n} steps")
    }
}

/// Tokens read and written, briefly.
fn tokens_summary(tokens: Tokens) -> String {
    format!(
        "{} in, {} out",
        tokens_text(tokens.input + tokens.cache_read + tokens.cache_write),
        tokens_text(tokens.output + tokens.reasoning)
    )
}

fn heading(text: &str) -> SheetRow {
    SheetRow {
        kind: 0,
        name: text.into(),
        ..SheetRow::default()
    }
}

fn pair(name: &str, value: &str, tone: i32) -> SheetRow {
    SheetRow {
        kind: 1,
        name: name.into(),
        value: value.into(),
        tone,
        ..SheetRow::default()
    }
}

/// A switch for one of Harlo's own settings, with what it does.
fn switch(setting: Setting, name: &str, what: &str, on: bool) -> SheetRow {
    SheetRow {
        kind: 3,
        name: name.into(),
        value: what.into(),
        on,
        setting: int(Setting::ALL
            .iter()
            .position(|s| *s == setting)
            .unwrap_or_default()),
        ..SheetRow::default()
    }
}

fn note(text: &str) -> SheetRow {
    SheetRow {
        kind: 2,
        name: text.into(),
        ..SheetRow::default()
    }
}
