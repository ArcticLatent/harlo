//! The open file's tab: a header with its path and Save, and the editor,
//! or the picture for an image.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Tokens;
use iced::widget::{button, center, column, container, image, row, rule, space, svg, text};
use iced::{Center, ContentFit, Element, Fill};
use lucide_icons::Icon;

use crate::app::{App, FileBody, FilesEvent, Message, OpenFile, Outside, TextFile};
use crate::style::{self, MONO, size};
use crate::view::icon;

/// The space around a picture.
const PICTURE_MARGIN: f32 = 24.0;

/// The header's height.
const HEADER: f32 = 44.0;

/// The open file's view.
pub fn view<'a>(app: &'a App, file: &'a OpenFile) -> Element<'a, Message> {
    let t = &app.tokens;
    let body: Element<'a, Message> = match &file.body {
        FileBody::Loading => note(t, "Opening…".to_owned()),
        FileBody::Text(text) => {
            let key = file.key.clone();
            let editor = text
                .editor
                .view()
                .map(move |m| Message::Files(FilesEvent::Editor(key.clone(), m)));
            column![bars(t, file, text), editor].into()
        }
        FileBody::Image(handle) => {
            picture(image(handle.clone()).content_fit(ContentFit::ScaleDown))
        }
        FileBody::Svg(handle) => picture(svg(handle.clone()).content_fit(ContentFit::ScaleDown)),
        FileBody::Binary => note(
            t,
            "This file isn't text, so it can't be edited here.".to_owned(),
        ),
        FileBody::TooLarge(bytes) => note(
            t,
            format!(
                "This file is {}, too large to open here.",
                megabytes(*bytes)
            ),
        ),
        FileBody::Failed(error) => note(t, format!("Couldn't open it: {error}")),
    };
    column![
        header(t, file),
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
        container(body).width(Fill).height(Fill),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

/// The file's path, whether it's saved, and Save.
fn header<'a>(t: &Tokens, file: &'a OpenFile) -> Element<'a, Message> {
    let mut line = row![
        text(file.path.clone())
            .font(MONO)
            .size(size::SMALL)
            .color(style::color(t.text_base)),
    ]
    .spacing(10)
    .align_y(Center)
    .height(HEADER)
    .padding([0, 18]);
    if let FileBody::Text(text) = &file.body {
        if let Some(error) = &text.error {
            line = line.push(
                text_of(format!("Couldn't save: {error}"))
                    .size(size::SMALL)
                    .color(style::color(t.icon_critical_base)),
            );
        } else if text.saving {
            line = line.push(weak(t, "Saving…"));
        } else if text.dirty {
            line = line.push(weak(t, "Unsaved changes"));
        }
        line = line.push(space().width(Fill));
        let mut save = button(text_of("Save".to_owned()).size(size::SMALL))
            .padding([4, 12])
            .style(style::primary(t));
        if text.dirty && !text.saving {
            save = save.on_press(Message::Files(FilesEvent::Save(file.key.clone())));
        }
        line = line.push(save);
    }
    line.into()
}

/// Questions above the editor: whether to save before closing, and what to
/// do about a change on disk.
fn bars<'a>(t: &Tokens, file: &'a OpenFile, text: &'a TextFile) -> Element<'a, Message> {
    let key = &file.key;
    let mut bars = column![];
    if file.closing {
        bars = bars.push(bar(
            t,
            match &file.then_open {
                Some((_, next)) => format!(
                    "Save your changes to {} before opening {}?",
                    file.name(),
                    harlo_core::files::name_of(next)
                ),
                None => format!("Save your changes to {} before closing?", file.name()),
            },
            vec![
                (
                    "Save",
                    Message::Files(FilesEvent::SaveAndClose(key.clone())),
                    true,
                ),
                (
                    "Don't save",
                    Message::Files(FilesEvent::Discard(key.clone())),
                    false,
                ),
                (
                    "Cancel",
                    Message::Files(FilesEvent::KeepOpen(key.clone())),
                    false,
                ),
            ],
        ));
    }
    match &text.outside {
        Some(Outside::Changed(_)) => {
            bars = bars.push(bar(
                t,
                "This file changed on disk while you had unsaved changes.".to_owned(),
                vec![
                    (
                        "Load the new version",
                        Message::Files(FilesEvent::Reload(key.clone())),
                        false,
                    ),
                    (
                        "Keep mine",
                        Message::Files(FilesEvent::KeepMine(key.clone())),
                        false,
                    ),
                ],
            ));
        }
        Some(Outside::Removed) => {
            bars = bars.push(bar(
                t,
                "This file was deleted on disk. Saving puts it back.".to_owned(),
                vec![(
                    "OK",
                    Message::Files(FilesEvent::KeepMine(key.clone())),
                    false,
                )],
            ));
        }
        None => {}
    }
    bars.into()
}

/// One question, with its choices on the right.
fn bar<'a>(t: &Tokens, says: String, choices: Vec<(&str, Message, bool)>) -> Element<'a, Message> {
    let mut line = row![
        icon(Icon::CircleAlert, 14.0, t.icon_warning_base),
        text_of(says)
            .size(size::SMALL)
            .color(style::color(t.text_strong)),
        space().width(Fill),
    ]
    .spacing(9)
    .align_y(Center);
    for (label, message, main) in choices {
        line = line.push({
            let choice = button(text_of(label.to_owned()).size(size::SMALL))
                .padding([4, 12])
                .on_press(message);
            if main {
                choice.style(style::primary(t))
            } else {
                choice.style(style::chip(t))
            }
        });
    }
    column![
        container(line)
            .width(Fill)
            .padding([8, 18])
            .style(style::fill(t.surface_raised_base)),
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
    ]
    .into()
}

/// A picture, scaled down to fit and centred.
fn picture<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    center(content).padding(PICTURE_MARGIN).into()
}

/// Plain text in the middle, for a file that doesn't show.
fn note<'a>(t: &Tokens, words: String) -> Element<'a, Message> {
    center(
        text_of(words)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .into()
}

fn weak<'a>(t: &Tokens, words: &str) -> iced::widget::Text<'a> {
    text_of(words.to_owned())
        .size(size::SMALL)
        .color(style::color(t.text_weak))
}

fn text_of<'a>(words: String) -> iced::widget::Text<'a> {
    text(words)
}

/// A size in megabytes, such as "12.5 MB".
fn megabytes(bytes: usize) -> String {
    /// Bytes in a megabyte.
    const MB: usize = 1024 * 1024;
    let tenths = bytes * 10 / MB;
    format!("{}.{} MB", tenths / 10, tenths % 10)
}
