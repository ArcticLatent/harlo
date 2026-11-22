//! A diff drawn as lines: numbers, `+`/`-` marks, tinted backgrounds, and the
//! changed words highlighted. Used by tool rows, approvals and the Changes panel.
//!
//! The lines are one [`widget::DiffLines`], which measures only the lines in
//! view, so long diffs stay quick to open, resize and scroll.

// Rust guideline compliant 2026-10-09

use std::borrow::Cow;

use harlo_core::diff::Line;
use harlo_core::theme::{Rgba, Tokens};
use iced::widget::{Column, column, container, row, scrollable, space, text};
use iced::{Center, Element, Fill};

use crate::style::{self, MONO, size};
use crate::widget;

/// How far the "N more lines" note is indented, to sit under the text.
const NOTE_INDENT: f32 = 34.0;

/// The width of the sideways scrollbar under a wide diff.
const SCROLLBAR: f32 = 4.0;

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
/// Lines don't wrap; a line wider than the box scrolls sideways. `more` is
/// sent by the "N more lines" note, if given; without it the note is plain
/// text.
pub fn lines<'a, M: Clone + 'a>(
    t: &Tokens,
    lines: impl Into<Cow<'a, [Line]>>,
    limit: usize,
    more: Option<M>,
) -> Element<'a, M> {
    let mut shown = lines.into();
    let hidden = shown.len().saturating_sub(limit);
    match &mut shown {
        Cow::Borrowed(all) => *all = &all[..all.len().min(limit)],
        Cow::Owned(all) => all.truncate(limit),
    }
    let drawn = scrollable(widget::diff_lines(
        shown,
        colors(t),
        MONO,
        size::CODE,
        size::SMALL,
    ))
    .direction(scrollable::Direction::Horizontal(
        scrollable::Scrollbar::new()
            .width(SCROLLBAR)
            .scroller_width(SCROLLBAR),
    ))
    .style(style::thin_scroll(t));
    let mut list = Column::new().push(drawn);
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

/// A diff's colours, from the theme: tints and changed words over the
/// sunken background.
fn colors(t: &Tokens) -> widget::DiffColors {
    let back = t.surface_inset_base;
    let tint = |c: Rgba| style::color(c.over(back, style::PATCH_TINT));
    let word = |c: Rgba| style::color(c.over(back, WORD_TINT));
    widget::DiffColors {
        number: style::color(t.text_weak),
        context: style::color(t.text_base),
        changed: style::color(t.text_strong),
        gap: style::color(t.text_weaker),
        added: style::color(t.icon_success_base),
        removed: style::color(t.icon_critical_base),
        plain: style::color(t.text_weaker),
        added_line: tint(t.icon_success_base),
        removed_line: tint(t.icon_critical_base),
        added_word: word(t.icon_success_base),
        removed_word: word(t.icon_critical_base),
    }
}

/// A file's diff under its header, for cards and panels.
pub fn file<'a, M: Clone + 'a>(
    t: &Tokens,
    (name, added, removed): (&str, u64, u64),
    shown: impl Into<Cow<'a, [Line]>>,
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
