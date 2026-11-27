//! The rail's usage meter: the provider shown, its ring and the few
//! characters under it, and its menu's rows.

// Rust guideline compliant 2026-10-09

use std::rc::Rc;

use harlo_core::meter::{Level, Reading, Why};
use harlo_core::theme::Tokens;
use slint::{ModelRc, VecModel};

use super::{maker, sync};
use crate::app::{App, epoch_seconds};
use crate::theme::color;
use crate::ui::{MeterLine, MeterRow, State};

/// The meter's lists, kept for the window's lifetime.
pub struct Meter {
    rows: Rc<VecModel<MeterRow>>,
    lines: Rc<VecModel<MeterLine>>,
}

impl Meter {
    /// Makes the lists and gives them to the UI.
    pub fn new(state: &State<'_>) -> Self {
        let meter = Self {
            rows: Rc::new(VecModel::default()),
            lines: Rc::new(VecModel::default()),
        };
        state.set_meter_rows(ModelRc::from(Rc::clone(&meter.rows)));
        state.set_meter_lines(ModelRc::from(Rc::clone(&meter.lines)));
        meter
    }

    /// Shows the meter as `app` has it.
    pub fn show(&self, app: &App, state: &State<'_>, t: &Tokens) {
        let readings = app.meter_readings();
        let Some(shown) = app.meter_shown(&readings) else {
            state.set_meter_shown(false);
            sync(&self.rows, Vec::new());
            sync(&self.lines, Vec::new());
            return;
        };
        let now = epoch_seconds(app.now);
        let reading = shown.reading;
        let (logo, tone) = maker(t, Some(&reading.provider));
        state.set_meter_shown(true);
        state.set_meter_maker(logo);
        state.set_meter_tone(color(tone));
        state.set_meter_ringed(reading.fraction().is_some());
        state.set_meter_fraction(share(reading.fraction().unwrap_or(0.0)));
        state.set_meter_level(level(reading.level()));
        state.set_meter_label(reading.short().into());
        state.set_meter_name(reading.title().into());
        let why = match shown.why {
            Why::Pinned => " (pinned)",
            Why::Last => " (used last)",
            Why::Fullest => "",
        };
        state.set_meter_tip(format!("{}{why}", reading.sentence(now)).into());
        state.set_meter_note(note(reading).into());
        state.set_meter_manage(reading.manage.clone().unwrap_or_default().into());
        state.set_meter_follows(app.preferences.meter_pin.is_empty());
        let last = readings
            .iter()
            .find(|r| r.provider == app.preferences.meter_last)
            .map_or("", |r| r.name.as_str());
        state.set_meter_last(last.into());
        let lines = reading
            .windows
            .iter()
            .map(|window| MeterLine {
                label: window.label.as_str().into(),
                value: window.status(now).into(),
                fraction: share(window.percent / 100.0),
                level: level(window.level()),
            })
            .collect();
        sync(&self.lines, lines);
        let rows = readings
            .iter()
            .map(|reading| {
                let (logo, tone) = maker(t, Some(&reading.provider));
                MeterRow {
                    id: reading.provider.as_str().into(),
                    name: reading.title().into(),
                    maker: logo,
                    tone: color(tone),
                    hint: reading.short().into(),
                    pinned: app.preferences.meter_pin == reading.provider,
                }
            })
            .collect();
        sync(&self.rows, rows);
    }
}

/// What the menu says under a provider's limits: the amount it tells of,
/// that a request was refused, or why there's no limit to show.
fn note(reading: &Reading) -> String {
    let mut said = Vec::new();
    if reading.reached {
        said.push("Its last request was refused over a limit.".to_owned());
    }
    if let Some((label, amount)) = &reading.amount {
        said.push(format!("{label}: {amount}"));
    }
    if let Some(why) = &reading.error {
        said.push(format!("Its use couldn't be read. {why}"));
    }
    if said.is_empty() && reading.windows.is_empty() {
        said.push(if reading.manage.is_some() {
            "Its limits show on its own page only.".to_owned()
        } else {
            "It tells apps nothing of its limits.".to_owned()
        });
    }
    said.join("\n")
}

/// A level as the UI numbers it.
fn level(level: Level) -> i32 {
    match level {
        Level::Calm => 0,
        Level::Warning => 1,
        Level::Critical => 2,
    }
}

/// A share from 0 to 1 as the UI takes it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a share between 0 and 1, drawn as an arc"
)]
fn share(fraction: f64) -> f32 {
    fraction.clamp(0.0, 1.0) as f32
}
