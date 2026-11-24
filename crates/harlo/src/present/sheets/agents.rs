//! Settings → Agents' rows: The server's agents in the open project, and the
//! approvals it saved when someone chose "Always allow".

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Agent, Catalog};
use lucide_icons::Icon;

use super::{heading, int, note, pair};
use crate::app::{App, Load, Status};
use crate::present::glyph;
use crate::ui::SheetRow;
use crate::window;

/// The agents, then the saved approvals.
pub(super) fn agent_rows(app: &App) -> Vec<SheetRow> {
    let mut rows = Vec::new();
    agents(app, &mut rows);
    approvals(app, &mut rows);
    rows
}

/// The open project's agents, as the server defines them.
fn agents(app: &App, rows: &mut Vec<SheetRow>) {
    rows.push(heading("Agents"));
    rows.push(note(
        "Tab in the composer switches primary agents. Add your own in an agents folder.",
    ));
    match app.catalog() {
        Some(catalog) if catalog.agents.is_empty() => rows.push(note("No agents.")),
        Some(catalog) => {
            for agent in &catalog.agents {
                // A primary agent's kind stands out; a subagent's stays quiet.
                let tone = if agent.primary { 0 } else { 3 };
                rows.push(SheetRow {
                    glyph: glyph(if agent.read_only {
                        Icon::Eye
                    } else {
                        Icon::Bot
                    }),
                    ..pair(&agent.name, &agent_words(agent, catalog), tone)
                });
                if let Some(what) = agent.description.as_deref().and_then(first_sentence) {
                    rows.push(note(what));
                }
            }
        }
        None if matches!(app.status, Status::Connected { .. }) => rows.push(note("Loading…")),
        None => rows.push(note("Connect to a server to see its agents.")),
    }
}

/// The first sentence of `text`, or none when it's blank: an agent's
/// description often goes on to tell other agents how to call it.
fn first_sentence(text: &str) -> Option<&str> {
    let text = text.trim();
    let end = text.find(". ").map_or(text.len(), |at| at + 1);
    Some(&text[..end]).filter(|first| !first.is_empty())
}

/// How `agent` runs: as a session's agent, a subagent or both, with the
/// model it always uses and whether it can edit files.
fn agent_words(agent: &Agent, catalog: &Catalog) -> String {
    let mut words = vec![match (agent.primary, agent.subagent) {
        (true, true) => "Primary and subagent".to_owned(),
        (false, true) => "Subagent".to_owned(),
        (_, false) => "Primary".to_owned(),
    }];
    if let Some((provider, id)) = &agent.model {
        words.push(
            catalog
                .model(provider, id)
                .map_or_else(|| id.clone(), |model| model.name.clone()),
        );
    }
    if agent.read_only {
        words.push("Read-only".to_owned());
    }
    words.join(" · ")
}

/// What "Always allow" saved, each with its project and Remove.
fn approvals(app: &App, rows: &mut Vec<SheetRow>) {
    let page = &app.sheets.approvals;
    rows.push(heading("Saved approvals"));
    rows.push(note(
        "From \u{201c}Always allow\u{201d}. Remove one to be asked again.",
    ));
    if let Some(why) = &page.error {
        rows.push(pair("Not removed", why, 2));
    }
    match &page.list {
        Load::Loaded(list) if list.is_empty() => rows.push(note("No saved approvals.")),
        Load::Loaded(list) => {
            for (index, approval) in list.iter().enumerate() {
                let project = app
                    .project_name_by_id(&approval.project_id)
                    .unwrap_or_else(|| approval.project_id.clone());
                let mut row = pair(
                    &format!("{}: {}", action_name(&approval.action), approval.resource),
                    &project,
                    3,
                );
                row.action = "Remove".into();
                row.setting = window::REMOVE_APPROVAL;
                row.choice = int(index);
                rows.push(row);
            }
        }
        Load::Failed(why) => rows.push(pair("Couldn't list them", why, 2)),
        Load::Loading => rows.push(note("Loading…")),
        Load::Idle => rows.push(note("Connect to a server to see them.")),
    }
}

/// `action` with a capital first letter, as a label.
fn action_name(action: &str) -> String {
    let mut chars = action.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
