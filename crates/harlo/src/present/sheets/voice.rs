//! Settings → Voice: dictation's speech model, its download, the language
//! it listens for and the microphone it records.

// Rust guideline compliant 2026-10-09

use harlo_core::voice::{LANGUAGE_NAMES, Model};

use super::{choice, heading, note, pair};
use crate::app::{App, SettingChoice};
use crate::ui::SheetRow;
use crate::window;

/// The section's rows.
pub(super) fn voice_rows(app: &App) -> Vec<SheetRow> {
    let voice = &app.voice;
    let mut rows = vec![
        heading("Dictation"),
        note("Ctrl+Shift+D or the microphone in the message box. It runs on this computer."),
        choice(
            app,
            SettingChoice::VoiceModel,
            "Speech model",
            "Small is more accurate and takes longer.",
        ),
    ];
    let model = app.voice_model();
    let Some(models) = &voice.models else {
        rows.push(note(
            "This system has no folder for app data, so no model can be kept.",
        ));
        return rows;
    };
    rows.push(model_row(app, model, models.ready(model)));
    if let Some(why) = &voice.download_error {
        rows.push(pair("Not downloaded", why, 2));
    }
    let language = LANGUAGE_NAMES
        .get(app.chosen(SettingChoice::VoiceLanguage))
        .copied()
        .unwrap_or_default();
    let mut row = pair("Language", language, 3);
    row.action = "Change".into();
    row.setting = window::CHOOSE_VOICE_LANGUAGE;
    rows.push(row);
    let mut row = pair("Microphone", &app.microphone_name(), 3);
    row.action = "Change".into();
    row.setting = window::CHOOSE_MICROPHONE;
    rows.push(row);
    rows.push(pair(
        "Models are kept in",
        &models.folder().display().to_string(),
        3,
    ));
    rows
}

/// How the chosen model stands, with Download or Stop.
fn model_row(app: &App, model: Model, ready: bool) -> SheetRow {
    let size = format!("{} MB", model.size().div_ceil(1_000_000));
    if let Some(fraction) = app.voice.progress {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a fraction from 0 to 1 as a whole percentage"
        )]
        let percent = (fraction * 100.0).round() as u32;
        let mut row = pair(model.label(), &format!("Downloading… {percent}%"), 3);
        row.action = "Stop".into();
        row.setting = window::STOP_VOICE_DOWNLOAD;
        return row;
    }
    if ready {
        return pair(model.label(), "Downloaded", 1);
    }
    let mut row = pair(model.label(), "Not downloaded", 3);
    row.action = format!("Download {size}").into();
    row.setting = window::DOWNLOAD_VOICE;
    row
}
