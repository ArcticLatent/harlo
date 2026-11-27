//! Settings → Experimental's rows: a switch for each of the server's
//! features that are still being tried.

// Rust guideline compliant 2026-10-09

use harlo_core::config::Experiment;

use super::{note, pair, switch};
use crate::app::{App, Load, Setting};
use crate::ui::SheetRow;

/// What the page is for, then a switch for each feature where Harlo may
/// change the server's config.
pub(super) fn experimental_rows(app: &App) -> Vec<SheetRow> {
    let page = &app.sheets.experiments;
    let mut rows = vec![note(
        "Features still being tried. A change applies to every session on this server.",
    )];
    match &page.on {
        // Another computer's server reads its own config.
        _ if app.global_config().is_none() => {
            rows.push(note(
                "Set under experimental in harlo.json on the server's computer.",
            ));
        }
        Load::Loaded(on) => rows.extend(Experiment::ALL.into_iter().map(|experiment| {
            let (name, what) = words(experiment);
            switch(
                Setting::Experiment(experiment),
                name,
                what,
                on.contains(&experiment),
            )
        })),
        Load::Failed(why) => rows.push(pair("harlo.json", why, 2)),
        Load::Idle | Load::Loading => rows.push(note("Checking…")),
    }
    if let Some(why) = &page.error {
        rows.push(pair("Not changed", why, 2));
    }
    rows
}

/// A feature's name and what it does.
fn words(experiment: Experiment) -> (&'static str, &'static str) {
    match experiment {
        Experiment::Todo => (
            "Todo list",
            "Agents keep a list of a task's steps above the message box. Costs more tokens.",
        ),
        Experiment::Outline => (
            "File outlines",
            "Agents list what a source file declares and read only the part they need.",
        ),
        Experiment::ScriptTools => (
            "Session tools in scripts",
            "Scripts agents write can read, search, edit and run commands. Costs more tokens.",
        ),
        Experiment::CachedInstructions => (
            "Cached instructions",
            "For OpenAI's GPT-6.2 and later: instructions go where the cache keeps them.",
        ),
    }
}
