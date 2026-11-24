//! Settings → Schedules' rows: the list, one schedule's page, and the form
//! for a new one.

// Rust guideline compliant 2026-10-09

use harlo_core::schedules::{describe_now, moment_now};
use harlo_core::server::{Automation, AutomationAction, AutomationStatus};
use lucide_icons::Icon;

use super::{PROJECT, choice, glyph, heading, home_relative, int, live_field, note, pair, switch};
use crate::app::{App, Field, Load, SchedulePage, Setting, SettingChoice};
use crate::model::session_label;
use crate::ui::SheetRow;
use crate::window;

/// The list of schedules, one schedule, or the new one's form.
pub(super) fn schedule_rows(app: &App) -> Vec<SheetRow> {
    let page = &app.sheets.schedules;
    match &page.page {
        SchedulePage::List => list_rows(app),
        SchedulePage::New => new_rows(app),
        SchedulePage::Shown(id) => match page.get(id) {
            Some(automation) => shown_rows(app, automation),
            None => list_rows(app),
        },
    }
}

/// What schedules are, the switch that allows them, and every schedule.
fn list_rows(app: &App) -> Vec<SheetRow> {
    let page = &app.sheets.schedules;
    let mut rows = vec![
        heading("Prompts on a timer"),
        note("The server sends each prompt on time, even with Harlo closed."),
    ];
    match &page.allowed {
        Load::Loaded(on) => rows.push(switch(
            Setting::Automations,
            "Allow schedules",
            "Shared by every app on this server.",
            *on,
        )),
        Load::Failed(why) => rows.push(pair("OpenCode's config", why, 2)),
        // Another computer's server reads its own config.
        Load::Idle => rows.push(note(
            "Schedules run unless automations is set to false in OpenCode's config on the \
             server's computer.",
        )),
        Load::Loading => {}
    }
    if let Some(why) = &page.allow_error {
        rows.push(pair("Not changed", why, 2));
    }

    rows.push(heading("Your schedules"));
    match &page.list {
        // While they're off, the way to one is the switch above.
        Load::Loaded(list) if list.is_empty() && page.allowed == Load::Loaded(false) => {
            rows.push(note(
                "No schedules yet. Turn on Allow schedules to add one.",
            ));
        }
        Load::Loaded(list) if list.is_empty() => rows.push(note("No schedules yet.")),
        Load::Loaded(list) => {
            for (index, automation) in list.iter().enumerate() {
                rows.push(SheetRow {
                    kind: PROJECT,
                    name: automation.name.as_str().into(),
                    value: summary(app, automation).into(),
                    glyph: glyph(status_icon(automation.status)),
                    setting: window::PICK_SCHEDULE,
                    choice: int(index),
                    ..SheetRow::default()
                });
            }
        }
        Load::Failed(why) => rows.push(pair("Couldn't list them", why, 2)),
        Load::Idle | Load::Loading => rows.push(note("Loading…")),
    }
    if page.allowed != Load::Loaded(false) {
        let mut new = pair("", "", 3);
        new.action = "New schedule".into();
        new.setting = window::NEW_SCHEDULE;
        rows.push(new);
    }
    rows
}

/// A schedule's page: when it runs, where its prompt goes, how its runs
/// went, and pausing, resuming and deleting it.
fn shown_rows(app: &App, automation: &Automation) -> Vec<SheetRow> {
    let page = &app.sheets.schedules;
    let mut back = pair(&automation.name, &describe_now(&automation.trigger), 3);
    back.action = "All schedules".into();
    back.setting = window::ALL_SCHEDULES;
    let mut rows = vec![back];
    if let Some(why) = &page.error {
        rows.push(pair("Not changed", why, 2));
    }

    rows.push(heading("When"));
    rows.push(pair("Runs", &describe_now(&automation.trigger), 0));
    rows.push(match (automation.status, automation.next_due_at) {
        (AutomationStatus::Paused, _) => pair("Next", "Paused", 4),
        (AutomationStatus::Done, _) => pair("Next", "It won't run again", 3),
        (AutomationStatus::Active, Some(next)) => pair("Next", &capitalised(&moment_now(next)), 0),
        (AutomationStatus::Active, None) => pair("Next", "Not set", 3),
    });
    rows.push(pair(
        "Time zone",
        &format!("{} · server time", automation.time_zone),
        3,
    ));

    rows.push(heading("What it sends"));
    rows.push(pair(
        "Project",
        &project_name(app, &automation.directory),
        0,
    ));
    match &automation.action {
        AutomationAction::Prompt {
            prompt,
            session,
            subagent,
        } => {
            let to = match (session, subagent) {
                (Some(id), _) => session_name(app, id),
                (None, Some(agent)) => format!("A new {agent} subagent each time"),
                (None, None) => String::new(),
            };
            rows.push(pair("Goes to", &to, 0));
            rows.push(note(prompt));
        }
        AutomationAction::Code { code } => {
            rows.push(pair("Runs", "Code, with no model", 0));
            rows.push(note(code));
        }
        AutomationAction::Shell { command } => {
            rows.push(pair("Runs", "A shell command", 0));
            rows.push(note(command));
        }
    }

    rows.push(heading("Runs so far"));
    let last = match (automation.last_run_at, &automation.last_outcome) {
        (Some(at), Some(outcome)) => format!("{}, {outcome}", moment_now(at)),
        (Some(at), None) => moment_now(at),
        _ => "Not yet".to_owned(),
    };
    let failed = automation
        .last_outcome
        .as_deref()
        .is_some_and(|o| o == "failed" || o == "expired");
    rows.push(pair("Last", &last, if failed { 2 } else { 0 }));
    rows.push(pair("Count", &automation.run_count.to_string(), 0));
    if let Some(output) = automation.last_output.as_deref().filter(|o| !o.is_empty()) {
        rows.push(note(output));
    }

    rows.push(heading("Change"));
    match automation.status {
        AutomationStatus::Active => {
            let mut pause = pair("Pause", "Runs stop until you resume the schedule.", 3);
            pause.action = if page.busy { "Pausing…" } else { "Pause" }.into();
            pause.setting = window::PAUSE_SCHEDULE;
            rows.push(pause);
        }
        AutomationStatus::Paused => {
            let mut resume = pair(
                "Resume",
                "A missed one-off runs once; a repeating one at its next time.",
                3,
            );
            resume.action = if page.busy { "Resuming…" } else { "Resume" }.into();
            resume.setting = window::RESUME_SCHEDULE;
            rows.push(resume);
        }
        AutomationStatus::Done => {}
    }
    let asked = page.deleting.as_deref() == Some(automation.id.as_str());
    let mut delete = pair(
        "Delete",
        if asked {
            "Delete it for every app using this server?"
        } else {
            "The session and its history are kept."
        },
        if asked { 4 } else { 3 },
    );
    delete.action = if asked { "Delete for good" } else { "Delete" }.into();
    delete.setting = window::DELETE_SCHEDULE;
    delete.risky = true;
    rows.push(delete);
    rows
}

/// The form for a new schedule.
fn new_rows(app: &App) -> Vec<SheetRow> {
    let page = &app.sheets.schedules;
    let draft = &page.draft;
    let mut back = pair("New schedule", "Runs on the OpenCode server", 3);
    back.action = "Cancel".into();
    back.setting = window::ALL_SCHEDULES;
    let mut rows = vec![
        back,
        live_field(
            Field::ScheduleName,
            "Name",
            &draft.name,
            "Morning build check",
        ),
        heading("When"),
        choice(app, SettingChoice::ScheduleWhen, "Runs", ""),
        {
            let mut time = live_field(
                Field::ScheduleTime,
                draft.when.label(),
                &draft.time,
                draft.when.hint(),
            );
            time.label = "Time".into();
            time
        },
        note(draft.when.help()),
        heading("What it sends"),
    ];
    let folder = draft
        .folder
        .as_deref()
        .map_or_else(|| "Choose one".to_owned(), |f| project_name(app, f));
    if draft.folder.is_none() && app.projects.is_empty() {
        rows.push(pair("Project", "Add a project first, with + Project", 4));
    } else {
        let tone = if draft.folder.is_some() { 0 } else { 3 };
        let mut project = pair("Project", &folder, tone);
        project.action = "Change".into();
        project.setting = window::CHOOSE_SCHEDULE_FOLDER;
        rows.push(project);
    }
    let to = draft.session.as_deref().map_or_else(
        || "A new session named after the schedule".to_owned(),
        |id| session_name(app, id),
    );
    let mut session = pair("Goes to", &to, 0);
    session.action = "Change".into();
    session.setting = window::CHOOSE_SCHEDULE_SESSION;
    rows.push(session);
    rows.push(live_field(
        Field::SchedulePrompt,
        "Prompt",
        &draft.prompt,
        "What the agent should do each time",
    ));
    let mut create = pair("", "", 3);
    if let Some(why) = &page.error {
        create.value = why.as_str().into();
        create.tone = 2;
    }
    create.action = if page.busy {
        "Creating…"
    } else {
        "Create schedule"
    }
    .into();
    create.setting = window::CREATE_SCHEDULE;
    rows.push(create);
    rows
}

/// `text` starting with a capital, as a value in a row reads.
fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A schedule in the list: whether it runs, when, and in which project.
fn summary(app: &App, automation: &Automation) -> String {
    let when = describe_now(&automation.trigger);
    let place = project_name(app, &automation.directory);
    match (automation.status, automation.next_due_at) {
        (AutomationStatus::Paused, _) => format!("Paused · {when} · {place}"),
        (AutomationStatus::Done, _) => format!("Done · {when} · {place}"),
        (AutomationStatus::Active, Some(next)) => {
            format!("{when} · next run {} · {place}", moment_now(next))
        }
        (AutomationStatus::Active, None) => format!("{when} · {place}"),
    }
}

/// The sidebar's name for the project in `folder`, or the folder itself.
pub(super) fn project_name(app: &App, folder: &str) -> String {
    app.projects
        .iter()
        .find(|p| p.directory == folder)
        .map_or_else(|| home_relative(folder), |p| p.name.clone())
}

/// The icon a schedule's row shows for its status.
fn status_icon(status: AutomationStatus) -> Icon {
    match status {
        AutomationStatus::Active => Icon::CalendarClock,
        AutomationStatus::Paused => Icon::CirclePause,
        AutomationStatus::Done => Icon::CircleCheck,
    }
}

/// A session's title, or its ID when it isn't in the sidebar.
fn session_name(app: &App, id: &str) -> String {
    app.projects
        .iter()
        .flat_map(|p| &p.sessions)
        .find(|s| s.id == id)
        .map_or_else(|| id.to_owned(), |s| session_label(&s.title))
}
