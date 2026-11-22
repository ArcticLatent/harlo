//! The pages over the window: the shortcuts list, the stats, and the
//! server's set-up with About.

// Rust guideline compliant 2026-10-09

use std::rc::Rc;

use harlo_core::server::{Day, McpStatus, PluginSource, Stats, Tokens};
use slint::{ModelRc, VecModel};

use super::sync;
use crate::app::{App, Load, Range, SetUp, Sheet, Status};
use crate::model::{compact as tokens_text, dollars as money};
use crate::ui::{DayBar, SheetRow, StatTile, State};

/// How many days of activity the bars show.
const DAYS: usize = 30;

/// The keys, by where they work: each a heading, then keys and what they do.
const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Window",
        &[
            ("Ctrl+N", "Start a new session"),
            ("Ctrl+W", "Close the tab"),
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
        sheets
    }

    /// Fills the open page.
    pub fn show(&self, app: &App, state: &State<'_>) {
        let Some(sheet) = app.sheets.open else {
            state.set_sheet(0);
            return;
        };
        let (number, title) = match sheet {
            Sheet::Shortcuts => (1, "Keyboard shortcuts"),
            Sheet::Stats => (2, "Stats"),
            Sheet::Settings => (3, "Settings"),
        };
        state.set_sheet(number);
        state.set_sheet_title(title.into());
        let (note, rows) = match sheet {
            Sheet::Shortcuts => (String::new(), shortcuts()),
            Sheet::Stats => {
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
            Sheet::Settings => match &app.sheets.setup {
                Load::Idle | Load::Loading => ("Loading…".to_owned(), server_rows(app)),
                Load::Failed(error) => (format!("Couldn't load it: {error}"), server_rows(app)),
                Load::Loaded(setup) => {
                    let mut rows = server_rows(app);
                    rows.extend(setup_rows(setup));
                    (String::new(), rows)
                }
            },
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
    let mut rows = vec![heading("Server")];
    rows.push(pair("Address", &app.location.label(), 0));
    let (status, tone) = match &app.status {
        Status::Connected { version } => (format!("Connected · OpenCode {version}"), 1),
        Status::Connecting { .. } => ("Connecting…".to_owned(), 3),
        Status::Lost { reason, .. } => (reason.clone(), 2),
    };
    rows.push(pair("Status", &status, tone));
    rows.push(note(
        "These are the shared server's settings, shown as they are. Harlo doesn't change \
         them, since other clients use the same server; change them with OpenCode itself.",
    ));
    rows
}

/// The server's providers, web search, MCP servers and plugins.
fn setup_rows(setup: &SetUp) -> Vec<SheetRow> {
    let mut rows = Vec::new();
    section(&mut rows, "Model providers", &setup.providers, |p| {
        let (state, tone) = if p.enabled { ("on", 1) } else { ("off", 3) };
        pair(&p.name, state, tone)
    });
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
    rows
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
    }
}

fn note(text: &str) -> SheetRow {
    SheetRow {
        kind: 2,
        name: text.into(),
        ..SheetRow::default()
    }
}
