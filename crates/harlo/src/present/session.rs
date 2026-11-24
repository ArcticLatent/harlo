//! The open session's header and the lines above its composer.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Delivery, Goal, GoalStatus};
use harlo_core::theme::Tokens;
use harlo_core::timeline::{TodoStatus, todo_counts};
use lucide_icons::Icon;
use slint::SharedString;

use super::glyph;
use crate::app::{App, Panel};
use crate::model::ChildState;
use crate::theme::color;
use crate::timeline::Activity;
use crate::ui::{InboxRow, State};

/// The kind of queued item OpenCode adds itself, as a note to the agent.
const SYNTHETIC: &str = "synthetic";

/// What the composer says while the session works, and while it waits.
const PLACEHOLDER_BUSY: &str =
    "Enter queues a message for when it's done · Ctrl+Enter steers it now";
const PLACEHOLDER: &str = "Send a message to continue this session…";

/// Shows session `id`'s header and composer placeholder.
pub fn header(app: &App, id: &str, state: &State<'_>) {
    let t = &app.tokens;
    let title = app.title(id);
    state.set_session_id(id.into());
    state.set_session_title(if title.is_empty() {
        "Untitled session".into()
    } else {
        title.into()
    });
    if let Some(parent) = app.parent_of(id) {
        let name = parent
            .title
            .clone()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Untitled session".to_owned());
        state.set_session_parent(name.into());
        state.set_session_parent_id(parent.id.as_str().into());
    } else {
        state.set_session_parent(SharedString::new());
        state.set_session_parent_id(SharedString::new());
    }
    // The agent by name, with its colour and whether it leaves files alone;
    // a subagent's session runs one the picker doesn't offer.
    let current = app.current_agent();
    let agent = current.as_deref().and_then(|id| app.agent(id));
    let read_only = agent.as_ref().is_some_and(|a| a.read_only);
    state.set_session_agent_glyph(glyph(if read_only { Icon::Eye } else { Icon::Bot }));
    state.set_session_agent_color(color(agent.as_ref().and_then(|a| a.color).map_or_else(
        || crate::theme::ink(t, t.accent_base),
        |c| crate::theme::ink(t, c),
    )));
    let name = agent.map(|a| a.name).or(current).unwrap_or_default();
    state.set_session_agent(name.into());
    state.set_session_read_only(read_only);
    state.set_session_model(
        app.current_model_choice()
            .map(|m| m.name)
            .unwrap_or_default()
            .into(),
    );
    state.set_session_worktree(app.worktree_of(id).unwrap_or_default().into());
    state.set_session_branch(app.worktree_branch_of(id).unwrap_or_default().into());
    let busy = app.is_busy(id);
    state.set_session_busy(busy);
    state.set_session_placeholder(if busy { PLACEHOLDER_BUSY } else { PLACEHOLDER }.into());
}

/// The lines above session `id`'s composer: its goal, what the agent
/// does, its todos, subagents at work, and prompts waiting for it.
#[must_use]
pub fn inbox(app: &App, id: &str) -> Vec<InboxRow> {
    let t = &app.tokens;
    let mut rows = Vec::new();
    if let Some(goal) = app.goal(id) {
        rows.push(goal_row(t, goal));
    }
    // While a card asks the user, it says what the agent waits for.
    if let Some(activity) = app.activity(id).filter(|_| !app.needs_user(id)) {
        rows.push(activity_row(app, activity));
    }
    // The agent's list, while something on it is still to do.
    if let Some(todos) = app.todos(id)
        && todos.iter().any(|todo| !todo.status.is_settled())
    {
        let (done, total) = todo_counts(todos);
        let current = todos
            .iter()
            .find(|todo| todo.status == TodoStatus::InProgress)
            .or_else(|| todos.iter().find(|todo| todo.status == TodoStatus::Pending))
            .map(|todo| todo.content.clone())
            .unwrap_or_default();
        rows.push(InboxRow {
            kind: 1,
            glyph: glyph(Icon::ListChecks),
            glyph_color: color(t.icon_base),
            strong: format!("{done} of {total}").into(),
            text: current.into(),
            // A way to the list, unless it's already open beside.
            action: if app.panel_shown(Panel::Todos) {
                SharedString::new()
            } else {
                "Todos".into()
            },
            done: number(done),
            total: number(total),
            ..InboxRow::default()
        });
    }
    // Subagents still at work, or asking the user, each a click away.
    for child in app.active_children(id) {
        let (note, note_color, working) = match child.state {
            ChildState::Waiting => ("Needs you", t.icon_warning_base, false),
            ChildState::Working | ChildState::Idle => ("Working", t.text_interactive_base, true),
        };
        rows.push(InboxRow {
            kind: 2,
            id: child.id.into(),
            glyph: glyph(Icon::Bot),
            glyph_color: color(child.color.unwrap_or(t.icon_base)),
            strong: child.agent.into(),
            text: child.task.into(),
            note: note.into(),
            note_color: color(note_color),
            working,
            action: "Open".into(),
            ..InboxRow::default()
        });
    }
    // OpenCode's own notes to the agent aren't the user's to see or cancel.
    let busy = app.is_busy(id);
    let waiting = app
        .inbox(id)
        .iter()
        .filter(|item| item.kind != SYNTHETIC)
        .map(|item| (item, false));
    let sending = app.queued_sending(id).map(|item| (item, true));
    for (item, on_its_way) in waiting.chain(sending) {
        let steering = item.delivery == Delivery::Steer;
        let mut row = InboxRow {
            kind: 3,
            id: item.id.as_str().into(),
            glyph: glyph(if steering {
                Icon::CornerDownRight
            } else {
                Icon::Clock
            }),
            glyph_color: color(t.icon_base),
            text: item
                .text
                .clone()
                .unwrap_or_else(|| item.kind.clone())
                .into(),
            note_color: color(t.text_weak),
            ..InboxRow::default()
        };
        if on_its_way {
            row.note = "Sending…".into();
            row.working = true;
        } else {
            row.cancel = true;
            if !busy {
                // Once it stops, OpenCode holds what's waiting until the next prompt.
                row.note = "Sends before your next message".into();
            } else if !steering {
                row.action = "Steer".into();
            }
        }
        rows.push(row);
    }
    rows
}

/// The line saying what the agent is doing, breathing until the run ends.
/// The session's goal: what it is, where it stands and why, and what
/// can be done to it. The action and the row's ID are the `/goal` words
/// they send.
fn goal_row(t: &Tokens, goal: &Goal) -> InboxRow {
    let (icon, note, tone, action) = match goal.status {
        GoalStatus::Active => (Icon::Goal, "Active", t.text_interactive_base, "pause"),
        GoalStatus::Paused => (Icon::CirclePause, "Paused", t.icon_warning_base, "resume"),
        GoalStatus::Complete => (Icon::CircleCheck, "Reached", t.icon_success_base, "clear"),
    };
    let detail = match (&goal.reason, goal.status) {
        (Some(reason), _) => reason.clone(),
        (None, GoalStatus::Active) if goal.rounds > 0 => {
            format!("Round {} of {}", goal.rounds, goal.max_rounds)
        }
        (None, _) => String::new(),
    };
    InboxRow {
        kind: 4,
        id: action.into(),
        glyph: glyph(icon),
        glyph_color: color(tone),
        strong: "Goal".into(),
        text: goal.objective.as_str().into(),
        detail: detail.into(),
        note: note.into(),
        note_color: color(tone),
        working: goal.status == GoalStatus::Active,
        action: match goal.status {
            GoalStatus::Active => "Pause",
            GoalStatus::Paused => "Resume",
            GoalStatus::Complete => "Dismiss",
        }
        .into(),
        // A reached goal has nothing left to stop; Dismiss clears it.
        cancel: goal.status != GoalStatus::Complete,
        ..InboxRow::default()
    }
}

fn activity_row(app: &App, activity: Activity) -> InboxRow {
    let t: &Tokens = &app.tokens;
    let agent = app.agent_name();
    let mut accent = t.text_interactive_base;
    let (icon, says, detail) = match activity {
        // It can't go on until someone approves; that's not "working".
        Activity::Waiting { action, detail } => {
            accent = t.icon_warning_base;
            (
                Icon::ShieldAlert,
                format!("{agent} is waiting for approval to {action}"),
                detail,
            )
        }
        Activity::Sending => (Icon::ArrowUp, format!("Sending to {agent}…"), String::new()),
        Activity::Thinking => (
            Icon::LoaderCircle,
            format!("{agent} is thinking…"),
            String::new(),
        ),
        Activity::Writing => (
            Icon::LoaderCircle,
            format!("{agent} is writing…"),
            String::new(),
        ),
        Activity::Running { label, detail } => (
            Icon::LoaderCircle,
            format!("{agent} is running {label}"),
            detail,
        ),
    };
    InboxRow {
        kind: 0,
        glyph: glyph(icon),
        glyph_color: color(accent),
        text: says.into(),
        detail: detail.into(),
        working: true,
        ..InboxRow::default()
    }
}

/// A count as the UI's `int`.
fn number(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}
