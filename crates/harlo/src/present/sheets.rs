//! The pages over the window: the stats, and Settings with its sections
//! (Harlo's own settings, the shortcuts, the server's set-up, About) and
//! its search.

// Rust guideline compliant 2026-10-09

use std::rc::Rc;

use harlo_core::server::{Day, McpStatus, PluginSource, Stats, Tokens};
use slint::{ModelRc, VecModel};

use lucide_icons::Icon;

use super::{glyph, sync};
use harlo_core::claude::{ClaudeCode, Login};

use crate::app::{App, ClaudeAction, Load, Range, Section, SetUp, Setting, Sheet, Status};
use crate::model::{compact as tokens_text, dollars as money};
use crate::ui::{DayBar, SettingsSection, SheetRow, StatTile, State};

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
        ],
    ),
    (
        "Notes",
        &[("Ctrl+Enter", "Tick the todo on the cursor's line")],
    ),
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
}

impl std::fmt::Debug for Sheets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sheets").finish_non_exhaustive()
    }
}

impl Sheets {
    /// Gives the window the pages' lists.
    pub fn new(state: &State<'_>) -> Self {
        let sheets = Self {
            rows: Rc::new(VecModel::default()),
            tiles: Rc::new(VecModel::default()),
            days: Rc::new(VecModel::default()),
        };
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
        sheets
    }

    /// Fills the open page.
    pub fn show(&self, app: &App, state: &State<'_>) {
        let Some(sheet) = app.sheets.open else {
            state.set_sheet(0);
            return;
        };
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
                    (setup_note(app, section), section_rows(app, section))
                } else {
                    state.set_sheet_title("Search results".into());
                    found(app, &app.sheets.query)
                }
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
        Section::Shortcuts => Icon::Keyboard,
        Section::Models => Icon::Bot,
        Section::Tools => Icon::Plug,
        Section::Server => Icon::Server,
        Section::About => Icon::Info,
    }
}

/// A section's rows.
fn section_rows(app: &App, section: Section) -> Vec<SheetRow> {
    match section {
        Section::General => general_rows(app),
        Section::Shortcuts => shortcuts(),
        Section::Models => {
            let mut rows = claude_rows(app);
            if let Load::Loaded(setup) = &app.sheets.setup {
                rows.extend(setup_rows(setup, section));
            }
            rows
        }
        Section::Tools => match &app.sheets.setup {
            Load::Loaded(setup) => setup_rows(setup, section),
            Load::Idle | Load::Loading | Load::Failed(_) => Vec::new(),
        },
        Section::Server => server_rows(app),
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
        matches!(row.kind, 1 | 3)
            && (row.name.to_lowercase().contains(&needle)
                || row.value.to_lowercase().contains(&needle))
    };
    let mut rows = Vec::new();
    for section in Section::ALL {
        let titled = section.title().to_lowercase().contains(&needle);
        let hits: Vec<SheetRow> = section_rows(app, section)
            .into_iter()
            .filter(|row| titled || matches(row))
            .filter(|row| row.kind != 0)
            .collect();
        if !hits.is_empty() {
            rows.push(heading(section.title()));
            rows.extend(hits);
        }
    }
    let note = if rows.is_empty() {
        format!("No setting matches “{}”.", query.trim())
    } else {
        String::new()
    };
    (note, rows)
}

/// Harlo's own general settings.
fn general_rows(app: &App) -> Vec<SheetRow> {
    vec![
        heading("Startup"),
        switch(
            Setting::ReopenTabs,
            "Reopen tabs",
            "Open the sessions that were open when Harlo last closed.",
            app.preferences.reopen_tabs,
        ),
    ]
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
    let mut rows = vec![pair("Address", &app.location.label(), 0)];
    let (status, tone) = match &app.status {
        Status::Connected { version } => (format!("Connected · OpenCode {version}"), 1),
        Status::Connecting { .. } => ("Connecting…".to_owned(), 3),
        Status::Lost { reason, .. } => (reason.clone(), 2),
    };
    rows.push(pair("Status", &status, tone));
    rows.push(note(
        "Harlo attaches to this shared OpenCode server and leaves it running when it closes. \
         Its settings apply to every app that uses it.",
    ));
    rows
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
    rows
}

/// The server's providers for Models, or its web search, MCP servers and
/// plugins for Tools.
fn setup_rows(setup: &SetUp, part: Section) -> Vec<SheetRow> {
    let mut rows = Vec::new();
    if part == Section::Models {
        section(&mut rows, "Model providers", &setup.providers, |p| {
            let (state, tone) = if p.enabled { ("on", 1) } else { ("off", 3) };
            pair(&p.name, state, tone)
        });
        rows.push(shared_note());
        return rows;
    }
    section(&mut rows, "Web search", &setup.search, |s| {
        pair(&s.name, "", 0)
    });
    section(&mut rows, "MCP servers", &setup.mcp, |m| {
        let (state, tone) = match &m.status {
            McpStatus::Connected => ("connected".to_owned(), 1),
            McpStatus::Pending => ("connecting".to_owned(), 3),
            McpStatus::Disabled => ("off".to_owned(), 3),
            McpStatus::Failed(why) => (format!("failed: {why}"), 2),
            McpStatus::NeedsAuth(why) => (format!("needs sign-in: {why}"), 2),
            McpStatus::Other(what) => (what.clone(), 3),
        };
        pair(m.title.as_deref().unwrap_or(&m.name), &state, tone)
    });
    section(&mut rows, "Plugins", &setup.plugins, |p| {
        let source = match &p.source {
            PluginSource::Builtin => "built in".to_owned(),
            PluginSource::Package {
                target,
                version,
                outdated,
            } => {
                let mut text = version
                    .as_ref()
                    .map_or_else(|| target.clone(), |v| format!("{target} {v}"));
                if *outdated {
                    text.push_str(" · update available");
                }
                text
            }
            PluginSource::Local(path) => path.clone(),
            PluginSource::Sdk => "added by a program".to_owned(),
            PluginSource::Other(what) => what.clone(),
        };
        match &p.failed {
            Some(why) => pair(&p.id, &format!("failed: {why}"), 2),
            None => pair(&p.id, &source, 3),
        }
    });
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
        setting: match setting {
            Setting::ReopenTabs => 0,
        },
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
