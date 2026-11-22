//! A diff drawn as lines: numbers, `+`/`-` marks, tinted backgrounds, and the
//! changed words highlighted. Used by tool rows, approvals and the Changes panel.

// Rust guideline compliant 2026-10-09

use harlo_core::diff::{Line, LineKind};
use harlo_core::theme::{Rgba, Tokens};
use iced::widget::text::Span;
use iced::widget::{Column, column, container, rich_text, row, space, text};
use iced::{Center, Element, Fill, Font};

use crate::style::{self, MONO, size};

/// The width of the line-number gutter, enough for five digits.
const GUTTER: f32 = 40.0;

/// How far the "N more lines" note is indented, to sit under the text.
const NOTE_INDENT: f32 = GUTTER - 6.0;

/// How much colour a changed word's background takes, over the line's tint.
const WORD_TINT: f32 = 0.38;

/// A file's name with its added and removed line counts.
pub fn header<'a, M: 'a>(t: &Tokens, file: &str, added: u64, removed: u64) -> Element<'a, M> {
    row![
        text(file.to_owned())
            .font(MONO)
            .size(size::CODE)
            .color(style::color(t.text_strong)),
        space().width(Fill),
        counts(t, added, removed),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

/// "+3 −1", in the added and removed colours; a side with nothing is left out.
pub fn counts<'a, M: 'a>(t: &Tokens, added: u64, removed: u64) -> Element<'a, M> {
    let mut list = row![].spacing(4);
    if added > 0 || removed == 0 {
        list = list.push(
            text(format!("+{added}"))
                .size(size::SMALL)
                .color(style::color(t.icon_success_base)),
        );
    }
    if removed > 0 {
        list = list.push(
            text(format!("−{removed}"))
                .size(size::SMALL)
                .color(style::color(t.icon_critical_base)),
        );
    }
    list.into()
}

/// The first `limit` lines of a diff in a sunken box, then how many are left.
///
/// `more` is sent by the "N more lines" note, if given; without it the note
/// is plain text.
pub fn lines<'a, M: Clone + 'a>(
    t: &Tokens,
    lines: &[Line],
    limit: usize,
    more: Option<M>,
) -> Element<'a, M> {
    let mut list = Column::new();
    for line in lines.iter().take(limit) {
        list = list.push(line_view(t, line));
    }
    let hidden = lines.len().saturating_sub(limit);
    if hidden > 0 {
        let note = text(format!("… {hidden} more lines"))
            .size(size::SMALL)
            .color(style::color(t.text_weak));
        let note: Element<'a, M> = match more {
            Some(message) => iced::widget::button(note)
                .padding([2, 6])
                .on_press(message)
                .style(style::ghost(t, false))
                .into(),
            None => note.into(),
        };
        list = list.push(container(note).padding([4.0, NOTE_INDENT]));
    }
    container(list)
        .width(Fill)
        .padding([5, 0])
        .style(style::boxed(t.surface_inset_base, None, size::RADIUS))
        .into()
}

/// One line: its number, its mark, and its text with changed words lit.
fn line_view<'a, M: 'a>(t: &Tokens, line: &Line) -> Element<'a, M> {
    let (mark, tint, colour) = match line.kind {
        LineKind::Added => ("+", Some(t.icon_success_base), t.text_strong),
        LineKind::Removed => ("-", Some(t.icon_critical_base), t.text_strong),
        LineKind::Context => (" ", None, t.text_base),
        LineKind::Gap => ("⋯", None, t.text_weaker),
    };
    let number = line.number.map(|n| n.to_string()).unwrap_or_default();
    let word = tint.map(|c| style::color(c.over(t.surface_inset_base, WORD_TINT)));
    // Only a line with changed words needs rich text; most are plain.
    let body: Element<'a, M> = if line.spans.iter().any(|piece| piece.changed) {
        let spans: Vec<Span<'a, (), Font>> = line
            .spans
            .iter()
            .map(|piece| {
                let span = iced::widget::span(piece.text.clone())
                    .font(MONO)
                    .color(style::color(colour));
                match word.filter(|_| piece.changed) {
                    Some(background) => span.background(background),
                    None => span,
                }
            })
            .collect();
        rich_text(spans).size(size::CODE).width(Fill).into()
    } else {
        text(line.text())
            .font(MONO)
            .size(size::CODE)
            .color(style::color(colour))
            .width(Fill)
            .into()
    };
    let content = row![
        container(
            text(number)
                .font(MONO)
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        )
        .width(GUTTER)
        .align_right(GUTTER)
        .padding(iced::padding::right(6)),
        text(mark)
            .font(MONO)
            .size(size::CODE)
            .color(style::color(sign(t, line.kind))),
        body,
    ]
    .spacing(6);
    container(content)
        .width(Fill)
        .padding(iced::padding::vertical(1).right(10))
        .style(style::patch_line(t, tint))
        .into()
}

/// The colour of a line's `+` or `-`.
fn sign(t: &Tokens, kind: LineKind) -> Rgba {
    match kind {
        LineKind::Added => t.icon_success_base,
        LineKind::Removed => t.icon_critical_base,
        LineKind::Context | LineKind::Gap => t.text_weaker,
    }
}

/// A file's diff under its header, for cards and panels.
pub fn file<'a, M: Clone + 'a>(
    t: &Tokens,
    (name, added, removed): (&str, u64, u64),
    shown: &[Line],
    limit: usize,
    more: Option<M>,
) -> Element<'a, M> {
    column![
        header(t, name, added, removed),
        lines(t, shown, limit, more)
    ]
    .spacing(6)
    .into()
}
