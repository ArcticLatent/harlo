//! Tools for agents' plugins: the package and local plugins one by one,
//! the built-in ones in a line, and checking for and installing updates.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Plugin, PluginSource};

use super::{heading, int, note, pair};
use crate::app::{App, is_outdated};
use crate::ui::SheetRow;
use crate::window;

/// The plugins, with Check for updates, or why they couldn't be listed.
pub(super) fn plugin_rows(
    app: &App,
    listed: &Result<Vec<Plugin>, String>,
    rows: &mut Vec<SheetRow>,
) {
    let page = &app.sheets.plugins;
    rows.push(heading("Plugins"));
    rows.push(note("Add tools, providers and hooks to OpenCode."));
    if let Some(done) = &page.done {
        rows.push(pair("", done, 1));
    }
    if let Some(why) = &page.error {
        rows.push(pair("Not done", why, 2));
    }
    let list = match (app.plugin_list(), listed) {
        (Some(list), _) => list,
        (None, Err(why)) => {
            rows.push(note(&format!("Couldn't list them: {why}")));
            return;
        }
        (None, Ok(list)) => list.as_slice(),
    };

    let mut built_in = 0;
    for (index, plugin) in list.iter().enumerate() {
        if let Some(why) = &plugin.failed {
            rows.push(pair(&plugin.id, &format!("Failed: {why}"), 2));
            continue;
        }
        match &plugin.source {
            PluginSource::Builtin => built_in += 1,
            PluginSource::Package {
                target,
                version,
                outdated,
                updating,
            } => {
                let mut words = version.clone().unwrap_or_else(|| "installed".to_owned());
                let tone = if *updating {
                    words.push_str(" · updating");
                    3
                } else if *outdated {
                    words.push_str(" · update available");
                    4
                } else {
                    3
                };
                let mut row = pair(target, &words, tone);
                if *outdated && !*updating && !page.busy {
                    row.action = "Update".into();
                    row.setting = window::UPDATE_PLUGIN;
                    row.choice = int(index);
                }
                rows.push(row);
            }
            PluginSource::Local(path) => rows.push(pair(&plugin.id, path, 3)),
            PluginSource::Sdk => rows.push(pair(&plugin.id, "Added by a program", 3)),
            PluginSource::Other(what) => rows.push(pair(&plugin.id, what, 3)),
        }
    }
    if built_in > 0 {
        rows.push(pair(
            "Built in",
            &format!("{built_in} plugins that come with OpenCode"),
            3,
        ));
    }

    let outdated = list.iter().filter(|p| is_outdated(p)).count();
    let mut check = pair("", "", 3);
    check.action = if page.busy {
        "Working…"
    } else {
        "Check for updates"
    }
    .into();
    if !page.busy {
        check.setting = window::CHECK_PLUGINS;
    }
    rows.push(check);
    if outdated > 1 && !page.busy {
        let mut all = pair("", "", 3);
        all.action = format!("Update all {outdated}").into();
        all.setting = window::UPDATE_PLUGIN;
        all.choice = -1;
        rows.push(all);
    }
}
