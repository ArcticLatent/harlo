//! Settings → Agents' rows: OpenCode's agents in the open project, and the
//! approvals it saved when someone chose "Allow always".

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Agent, Catalog};

use super::schedules::project_name;
use super::{heading, int, note, pair};
use crate::app::{App, Load, Status};
use crate::ui::SheetRow;
use crate::window;

/// The agents, then the saved approvals.
pub(super) fn agent_rows(app: &App) -> Vec<SheetRow> {
    let mut rows = Vec::new();
    agents(app, &mut rows);
    approvals(app, &mut rows);
    rows
}

/// The open project's agents, as OpenCode defines them.
fn agents(app: &App, rows: &mut Vec<SheetRow>) {
    let folder = match project_name(app, &app.folder()) {
        home if home == "~" => "your home folder".to_owned(),
        name => name,
    };
    rows.push(heading("Agents"));
    rows.push(note(&format!(
        "OpenCode's agents in {folder}. Tab in the composer switches between the primary ones; \
         other agents hand work to subagents. Add agents under agents in opencode.json, or as \
         Markdown files in a project's .opencode/agents folder, or in ~/.config/opencode/agents \
         for every project."
    )));
    match app.catalog() {
        Some(catalog) if catalog.agents.is_empty() => rows.push(note("No agents.")),
        Some(catalog) => {
            for agent in &catalog.agents {
                rows.push(pair(&agent.name, &agent_words(agent, catalog), 3));
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
        words.push("Doesn't edit files".to_owned());
    }
    words.join(" · ")
}

/// What "Allow always" saved, each with its project and Remove.
fn approvals(app: &App, rows: &mut Vec<SheetRow>) {
    let page = &app.sheets.approvals;
    rows.push(heading("Saved approvals"));
    rows.push(note(
        "What \u{201c}Allow always\u{201d} saved. OpenCode allows these in their project without \
         asking; a deny in its config still wins. Removing one makes it ask again, in every app \
         using this server.",
    ));
    if let Some(why) = &page.error {
        rows.push(pair("Not removed", why, 2));
    }
    match &page.list {
        Load::Loaded(list) if list.is_empty() => rows.push(note("None saved.")),
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
