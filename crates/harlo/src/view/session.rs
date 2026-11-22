//! An open session: the tab bar, its header, the timeline and the composer.

// Rust guideline compliant 2026-10-09

use harlo_core::server::Delivery;
use harlo_core::timeline::{Todo, TodoStatus, todo_counts};
use iced::widget::{button, column, container, responsive, row, rule, space, text};
use iced::{Center, Element, Fill, Padding};
use lucide_icons::Icon;

use crate::app::{App, Child, Message, Panel};
use crate::style::{self, size};
use crate::timeline::{self, Activity, Timeline};
use crate::view::composer::{self, Composer};
use crate::view::subagents::ChildState;
use crate::view::{breathing, icon, pickers, waiting};
use crate::widget::ellipsis;

/// Space either side of the composer.
const MARGIN: f32 = 24.0;

/// A session's tab: its header, the timeline and the composer.
pub fn view<'a>(app: &'a App, id: &'a str, timeline: &'a Timeline) -> Element<'a, Message> {
    responsive(move |size| at_width(app, id, timeline, size.width)).into()
}

/// A session's tab in a pane `pane` wide.
fn at_width<'a>(
    app: &'a App,
    id: &'a str,
    timeline: &'a Timeline,
    pane: f32,
) -> Element<'a, Message> {
    let t = &app.tokens;
    let owned = id.to_owned();
    let crew = app.crew(id);
    let body = timeline
        .view(t, app.moment(), &crew)
        .map(move |event| Message::Timeline(owned.clone(), event));
    let busy = app.is_busy(id);
    // The composer is as wide as the timeline column, less its margins.
    let width = (pane - 2.0 * MARGIN).min(timeline::WIDTH);
    let wide = width >= composer::INLINE_CONTROLS;
    // The pickers fill the tool row, so the keys are told in the placeholder.
    let placeholder = if busy {
        "Enter queues a message for when it's done · Ctrl+Enter steers it now"
    } else {
        "Send a message to continue this session · Tab switches agents"
    };
    let composer = composer::view(
        t,
        Composer {
            content: &app.composer,
            placeholder,
            hint: None,
            error: app.send_error.as_deref(),
            attached: &app.draft.attached,
            menu: app.menu.as_ref(),
            busy,
            wide,
            feedback: app.feedback(),
        },
        Some(pickers::controls(app)),
        pickers::shield(app),
    );
    let mut stack = column![].spacing(8);
    if let Some(cards) = waiting::view(app) {
        stack = stack.push(cards);
    }
    let composer = container(stack.push(inbox(app, id)).push(composer))
        .max_width(timeline::WIDTH)
        .padding(Padding {
            top: 4.0,
            bottom: 16.0,
            left: 0.0,
            right: 0.0,
        });
    let content = column![
        header(app, id),
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
        body,
        container(composer).center_x(Fill).padding([0.0, MARGIN]),
    ];
    container(content)
        .width(Fill)
        .height(Fill)
        .style(style::fill(t.background_base))
        .into()
}

/// The kind of queued item OpenCode adds itself, as a note to the agent.
const SYNTHETIC: &str = "synthetic";

/// Prompts waiting for the session, each of which can steer or be cancelled.
///
/// While the session works, a queued prompt can be turned into a steer.
/// Once it stops, OpenCode holds what's waiting until the next prompt, so
/// that's what the rows say instead. A prompt still on its way to the queue
/// shows last, saying so.
fn inbox<'a>(app: &'a App, id: &str) -> Element<'a, Message> {
    let t = &app.tokens;
    let busy = app.is_busy(id);
    let moment = app.moment();
    let mut list = column![].spacing(4);
    // While a card asks the user, it says what the agent waits for.
    if let Some(activity) = app.activity(id).filter(|_| !app.needs_user(id)) {
        list = list.push(activity_line(app, activity));
    }
    // The agent's list, while something on it is still to do.
    if let Some(todos) = app.todos(id)
        && todos.iter().any(|todo| !todo.status.is_settled())
    {
        list = list.push(todo_line(app, todos));
    }
    // Subagents still at work, or asking the user, each a click away.
    for child in app.active_children(id) {
        list = list.push(child_line(app, child));
    }
    // OpenCode's own notes to the agent, such as the reminder that it's
    // planning, aren't the user's to see or cancel.
    let waiting = app
        .inbox(id)
        .iter()
        .filter(|item| item.kind != SYNTHETIC)
        .map(|item| (item, false));
    let sending = app.queued_sending(id).map(|item| (item, true));
    for (item, on_its_way) in waiting.chain(sending) {
        let steering = item.delivery == Delivery::Steer;
        let label = item.text.clone().unwrap_or_else(|| item.kind.clone());
        let mut line = row![
            icon(
                if steering {
                    Icon::CornerDownRight
                } else {
                    Icon::Clock
                },
                13.0,
                t.icon_base
            ),
            ellipsis(label)
                .size(size::SMALL)
                .color(style::color(t.text_base)),
        ]
        .spacing(8)
        .align_y(Center);
        if on_its_way {
            line = line.push(breathing(
                text("Sending…")
                    .size(size::SMALL)
                    .style(moment.breathing(t.text_weak)),
            ));
            list = list.push(
                container(line)
                    .width(Fill)
                    .padding([5, 12])
                    .style(style::boxed(t.surface_raised_base, None, size::RADIUS)),
            );
            continue;
        }
        if !busy {
            line = line.push(
                text("Sends before your next message")
                    .size(size::SMALL)
                    .color(style::color(t.text_weak)),
            );
        } else if !steering {
            line = line.push(
                button(
                    text("Steer")
                        .size(size::SMALL)
                        .color(style::color(t.text_interactive_base)),
                )
                .padding([2, 6])
                .on_press(Message::SteerQueued(item.id.clone()))
                .style(style::ghost(t, false)),
            );
        }
        line = line.push(
            button(icon(Icon::X, 12.0, t.icon_base))
                .padding(3)
                .on_press(Message::CancelQueued(item.id.clone()))
                .style(style::ghost(t, false)),
        );
        list = list.push(
            container(line)
                .width(Fill)
                .padding([5, 12])
                .style(style::boxed(t.surface_raised_base, None, size::RADIUS)),
        );
    }
    list.into()
}

/// How far the agent's todo list is, what it's on, and a way to the whole list.
fn todo_line<'a>(app: &'a App, todos: &[Todo]) -> Element<'a, Message> {
    let t = &app.tokens;
    let (done, total) = todo_counts(todos);
    // What it's on, or else what's next.
    let current = todos
        .iter()
        .find(|todo| todo.status == TodoStatus::InProgress)
        .or_else(|| todos.iter().find(|todo| todo.status == TodoStatus::Pending))
        .map(|todo| todo.content.clone())
        .unwrap_or_default();
    let line = row![
        icon(Icon::ListChecks, 13.0, t.icon_base),
        text(format!("{done} of {total}"))
            .font(style::SANS_MEDIUM)
            .size(size::SMALL)
            .color(style::color(t.text_strong)),
        container(crate::view::todos::bar(t, done, total)).width(TODO_BAR),
        ellipsis(current)
            .size(size::SMALL)
            .color(style::color(t.text_base)),
        space().width(Fill),
        button(
            text("Todos")
                .size(size::SMALL)
                .color(style::color(t.text_interactive_base)),
        )
        .padding([2, 6])
        .on_press(Message::OpenPanel(Panel::Todos))
        .style(style::ghost(t, false)),
    ]
    .spacing(8)
    .align_y(Center);
    container(line)
        .width(Fill)
        .padding([5, 12])
        .style(style::boxed(t.surface_raised_base, None, size::RADIUS))
        .into()
}

/// The width of the progress bar on the todo line.
const TODO_BAR: f32 = 48.0;

/// A subagent at work or waiting for the user, with a way to open its session.
fn child_line(app: &App, child: Child) -> Element<'_, Message> {
    let t = &app.tokens;
    let moment = app.moment();
    let state: Element<'_, Message> = match child.state {
        ChildState::Waiting => text("Needs you")
            .size(size::SMALL)
            .color(style::color(t.icon_warning_base))
            .into(),
        ChildState::Working | ChildState::Idle => breathing(
            text("Working")
                .size(size::SMALL)
                .style(moment.breathing(t.text_interactive_base)),
        ),
    };
    let line = row![
        icon(Icon::Bot, 13.0, child.color.unwrap_or(t.icon_base)),
        text(child.agent)
            .font(style::SANS_MEDIUM)
            .size(size::SMALL)
            .color(style::color(t.text_strong)),
        ellipsis(child.task)
            .size(size::SMALL)
            .color(style::color(t.text_base)),
        space().width(Fill),
        state,
        button(
            text("Open")
                .size(size::SMALL)
                .color(style::color(t.text_interactive_base)),
        )
        .padding([2, 6])
        .on_press(Message::OpenSession(child.id))
        .style(style::ghost(t, false)),
    ]
    .spacing(8)
    .align_y(Center);
    container(line)
        .width(Fill)
        .padding([5, 12])
        .style(style::boxed(t.surface_raised_base, None, size::RADIUS))
        .into()
}

/// The line above the composer saying what the agent is doing, breathing
/// from the moment a prompt is sent until the run ends.
fn activity_line(app: &App, activity: Activity) -> Element<'_, Message> {
    let t = &app.tokens;
    let moment = app.moment();
    let agent = app.agent_name();
    let mut accent = t.text_interactive_base;
    let (glyph, says, detail) = match activity {
        // It can't go on until someone approves; that's not "working".
        Activity::Waiting { action, detail } => {
            accent = t.icon_warning_base;
            (
                Icon::ShieldAlert,
                format!("{agent} is waiting for approval to {action}"),
                (!detail.is_empty()).then_some(detail),
            )
        }
        Activity::Sending => (Icon::ArrowUp, format!("Sending to {agent}…"), None),
        Activity::Thinking => (Icon::Loader, format!("{agent} is thinking…"), None),
        Activity::Writing => (Icon::Loader, format!("{agent} is writing…"), None),
        Activity::Running { label, detail } => (
            Icon::Loader,
            format!("{agent} is running {label}"),
            (!detail.is_empty()).then_some(detail),
        ),
    };
    let mut line = row![
        breathing(icon(glyph, 13.0, accent).style(moment.breathing(accent))),
        text(says)
            .size(size::SMALL)
            .color(style::color(t.text_base)),
    ]
    .spacing(8)
    .align_y(Center);
    if let Some(detail) = detail {
        line = line.push(
            ellipsis(detail)
                .size(size::SMALL)
                .font(style::MONO)
                .color(style::color(t.text_weak)),
        );
    }
    container(line).padding([4, 4]).into()
}

/// A link back to the session that handed this one its work, for a subagent's.
fn parent_link<'a>(app: &'a App, id: &str) -> Option<Element<'a, Message>> {
    let t = &app.tokens;
    let parent = app.parent_of(id)?;
    let name = parent
        .title
        .clone()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Untitled session".to_owned());
    Some(
        button(
            row![
                icon(Icon::ArrowLeft, 12.0, t.text_interactive_base),
                ellipsis(format!("Subagent of {name}"))
                    .size(size::SMALL)
                    .color(style::color(t.text_interactive_base)),
            ]
            .spacing(5)
            .align_y(Center),
        )
        .padding(0)
        .on_press(Message::OpenSession(parent.id.clone()))
        .style(style::ghost(t, false))
        .into(),
    )
}

/// The session's title, then its agent and model, and a link to its changes.
fn header<'a>(app: &'a App, id: &str) -> Element<'a, Message> {
    let t = &app.tokens;
    let title = app.title(id);
    let title = if title.is_empty() {
        "Untitled session".to_owned()
    } else {
        title
    };
    let current = app.current_agent();
    // The agent by name, with its colour and whether it leaves files alone;
    // a subagent's session runs one the picker doesn't offer.
    let agent = current.as_deref().and_then(|id| app.agent(id));
    let model = app.current_model_choice().map(|m| m.name);
    let glyph = if agent.as_ref().is_some_and(|a| a.read_only) {
        Icon::Eye
    } else {
        Icon::Bot
    };
    let glyph_colour = agent
        .as_ref()
        .and_then(|a| a.color)
        .unwrap_or(t.icon_weak_base);
    let mut facts = row![icon(glyph, 13.0, glyph_colour)]
        .spacing(7)
        .align_y(Center);
    let name = agent.as_ref().map(|a| a.name.clone()).or(current);
    if let Some(name) = name {
        facts = facts.push(
            text(name)
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    if agent.is_some_and(|a| a.read_only) {
        facts = facts.push(
            text(pickers::READ_ONLY)
                .size(size::SMALL)
                .color(style::color(t.text_weaker)),
        );
    }
    if let Some(model) = model {
        facts = facts.push(
            text(model)
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    if app.is_busy(id) {
        // It breathes while the session works.
        let moment = app.moment();
        let working = t.text_interactive_base;
        facts = facts.push(breathing(
            row![
                icon(Icon::Loader, 12.0, working).style(moment.breathing(working)),
                text("Working")
                    .size(size::SMALL)
                    .style(moment.breathing(working)),
            ]
            .spacing(5)
            .align_y(Center),
        ));
    }
    let changes = button(
        text("Changes")
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .padding([5, 9])
    .on_press(Message::OpenPanel(Panel::Changes))
    .style(style::ghost(t, false));
    let mut titles = column![].spacing(6);
    if let Some(link) = parent_link(app, id) {
        titles = titles.push(link);
    }
    let titles = titles.push(
        column![
            ellipsis(title)
                .font(style::SANS_SEMIBOLD)
                .size(size::SESSION_TITLE)
                .color(style::color(t.text_strong)),
            facts,
        ]
        .spacing(6),
    );
    let inner = container(
        row![titles, space().width(16), changes]
            .align_y(Center)
            .width(Fill),
    )
    .max_width(timeline::WIDTH);
    container(inner).padding([14, 24]).center_x(Fill).into()
}
