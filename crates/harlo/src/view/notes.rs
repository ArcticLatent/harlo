//! The Notes panel's body: the current folder's note, how many of its todos
//! are open, and buttons to add a todo or put the note into the message.

// Rust guideline compliant 2026-10-09

use harlo_core::notes::todo_counts;
use harlo_core::theme::Tokens;
use iced::keyboard::{Key, key::Named};
use iced::widget::{button, column, container, row, space, text, text_editor};
use iced::{Center, Element, Fill};
use lucide_icons::Icon;

use crate::app::{App, Message, NotesEvent};
use crate::ids;
use crate::style::{self, size};
use crate::view::icon;
use crate::widget::ellipsis;

/// The space around the text area's text.
const TEXT_PADDING: f32 = 12.0;

/// The panel's body below its header.
pub fn body(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let key = app.note_key();
    let name = folder_name(&key.folder);
    let heading = row![
        icon(Icon::Folder, 14.0, t.icon_base),
        ellipsis(name.clone())
            .font(style::SANS_MEDIUM)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
    ]
    .spacing(8)
    .align_y(Center)
    .padding([12, 18]);
    let Some(note) = app.note().filter(|note| note.loaded) else {
        return column![heading, quiet(t, "Opening the note…")].into();
    };
    let placeholder = format!(
        "Notes for {name}: ideas, decisions, things to remember.\n\n\
         Lines like \"- [ ] task\" are todos; Ctrl+Enter ticks one."
    );
    let editing = key.clone();
    let editor = text_editor(&note.content)
        .id(ids::NOTES)
        .placeholder(placeholder)
        .on_action(move |action| Message::Notes(NotesEvent::Edit(editing.clone(), action)))
        .key_binding(binding)
        .size(size::LABEL)
        .padding(TEXT_PADDING)
        .height(Fill)
        .style(style::notes(t));
    let mut status = row![].spacing(8).align_y(Center);
    let (open, all) = todo_counts(&note.content.text());
    if all > 0 {
        status = status.push(
            text(if open == 0 {
                format!("All {all} todos done")
            } else {
                format!("{open} of {all} todos open")
            })
            .size(size::SMALL)
            .color(style::color(t.text_weak)),
        );
    }
    if let Some(error) = &note.error {
        status = status.push(
            text(error.clone())
                .size(size::SMALL)
                .color(style::color(t.icon_critical_base)),
        );
    }
    let has_text = !note.content.text().trim().is_empty();
    let actions = row![
        button(label("Add a todo"))
            .padding([5, 12])
            .on_press(Message::Notes(NotesEvent::AddTodo))
            .style(style::chip(t)),
        space().width(Fill),
        button(label("Insert into message"))
            .padding([5, 12])
            .on_press_maybe(has_text.then_some(Message::Notes(NotesEvent::Insert)))
            .style(style::chip(t)),
    ]
    .spacing(8)
    .align_y(Center);
    column![
        heading,
        container(editor).padding([0, 14]).height(Fill),
        column![status, actions].spacing(8).padding([12, 14]),
    ]
    .into()
}

/// Ctrl+Enter ticks the todo under the cursor; every other key edits.
fn binding(press: text_editor::KeyPress) -> Option<text_editor::Binding<Message>> {
    let focused = matches!(press.status, text_editor::Status::Focused { .. });
    if focused
        && press.modifiers.command()
        && matches!(press.key.as_ref(), Key::Named(Named::Enter))
    {
        return Some(text_editor::Binding::Custom(Message::Notes(
            NotesEvent::ToggleTodo,
        )));
    }
    text_editor::Binding::from_key_press(press)
}

/// A button's label, kept on one line in the narrow panel.
fn label(words: &str) -> iced::widget::Text<'_> {
    text(words).size(size::SMALL).wrapping(text::Wrapping::None)
}

/// The last part of a folder's path, for the heading.
fn folder_name(folder: &str) -> String {
    folder
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("/")
        .to_owned()
}

/// A line of plain text, for a state with nothing else to show.
fn quiet<'a>(t: &Tokens, words: &'a str) -> Element<'a, Message> {
    container(
        text(words)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .padding([8, 18])
    .into()
}
