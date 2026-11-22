//! The Todos panel's body: the list the session's agent keeps while it
//! works, as its last todo call wrote it.
//!
//! OpenCode gives agents the todo tool only when the configuration turns on
//! `experimental.todo`. Harlo never changes the server's configuration, so
//! when it's off the panel says how to turn it on.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::{Rgba, Tokens};
use harlo_core::timeline::{Todo, TodoStatus, todo_counts, todo_progress};
use iced::widget::{Column, column, container, row, scrollable, space, text};
use iced::{Element, Fill, Length, Top};
use lucide_icons::Icon;

use crate::app::{App, Message};
use crate::style::{self, MONO, SANS_MEDIUM, size};
use crate::view::icon;

/// The progress bar's height.
const BAR_HEIGHT: f32 = 6.0;

/// The space between the list and its scrollbar.
const SCROLLBAR_GAP: f32 = 4.0;

/// What turns the todo tool on, in `opencode.json`.
pub const TODO_SETTING: &str = "\"experimental\": {\n  \"todo\": true\n}";

/// The panel's body below its header.
pub fn body(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let Some(session) = app.active.as_deref() else {
        return note(t, "No session selected.".to_owned());
    };
    match app.todos(session) {
        Some([]) => note(t, "The agent cleared its list.".to_owned()),
        Some(todos) => list(t, todos),
        None if app.todo_tool() => note(
            t,
            "No list yet. The agent writes one when a task has several steps.".to_owned(),
        ),
        None => turned_off(t),
    }
}

/// Plain text in the panel, for an empty state.
fn note<'a>(t: &Tokens, words: String) -> Element<'a, Message> {
    container(
        text(words)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .padding([20, 18])
    .into()
}

/// What to do when the folder's agents don't have the todo tool.
fn turned_off<'a>(t: &Tokens) -> Element<'a, Message> {
    column![
        text("Agents here don't keep a todo list.")
            .size(size::LABEL)
            .color(style::color(t.text_base)),
        text(
            "OpenCode's todo tool is off for this folder. To turn it on, add this to opencode.json:"
        )
        .size(size::SMALL)
        .color(style::color(t.text_weak)),
        container(
            text(TODO_SETTING)
                .font(MONO)
                .size(size::SMALL)
                .color(style::color(t.text_strong))
        )
        .width(Fill)
        .padding([8, 10])
        .style(style::boxed(
            t.surface_raised_base,
            Some(t.border_weak_base),
            size::RADIUS
        )),
    ]
    .spacing(10)
    .padding([20, 18])
    .into()
}

/// How far the list is, then each item.
fn list<'a>(t: &Tokens, todos: &[Todo]) -> Element<'a, Message> {
    let (done, total) = todo_counts(todos);
    let mut items = Column::new().spacing(10);
    for todo in todos {
        items = items.push(item(t, todo));
    }
    let content = column![
        text(todo_progress(todos))
            .font(SANS_MEDIUM)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
        bar(t, done, total),
        items,
    ]
    .spacing(12)
    .padding([16, 18]);
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().spacing(SCROLLBAR_GAP),
        ))
        .style(style::thin_scroll(t))
        .height(Fill)
        .into()
}

/// One item: a mark for where it stands, and what it is.
fn item<'a>(t: &Tokens, todo: &Todo) -> Element<'a, Message> {
    let (glyph, mark, words): (Icon, Rgba, Rgba) = match todo.status {
        TodoStatus::Pending => (Icon::Circle, t.icon_base, t.text_base),
        TodoStatus::InProgress => (Icon::CircleDot, t.text_interactive_base, t.text_strong),
        TodoStatus::Completed => (Icon::CircleCheck, t.icon_success_base, t.text_weak),
        TodoStatus::Cancelled => (Icon::CircleX, t.text_weak, t.text_weak),
    };
    let mut label = text(todo.content.clone())
        .size(size::LABEL)
        .color(style::color(words));
    if todo.status == TodoStatus::InProgress {
        label = label.font(SANS_MEDIUM);
    }
    let mut line = row![container(icon(glyph, 14.0, mark)).padding([2, 0]), label].spacing(9);
    if todo.status == TodoStatus::Cancelled {
        line = line.push(
            text("Dropped")
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    line.align_y(Top).into()
}

/// A bar `done` of `total` full.
pub fn bar<'a>(t: &Tokens, done: usize, total: usize) -> Element<'a, Message> {
    let portion = |n: usize| Length::FillPortion(u16::try_from(n).unwrap_or(u16::MAX));
    let mut fill = row![];
    if done > 0 {
        fill = fill.push(
            container(space())
                .width(portion(done))
                .height(BAR_HEIGHT)
                .style(style::boxed(
                    t.text_interactive_base,
                    None,
                    BAR_HEIGHT / 2.0,
                )),
        );
    }
    if total > done {
        fill = fill.push(space().width(portion(total - done)));
    }
    container(fill)
        .width(Fill)
        .height(BAR_HEIGHT)
        .style(style::boxed(
            t.surface_raised_strong,
            None,
            BAR_HEIGHT / 2.0,
        ))
        .into()
}
