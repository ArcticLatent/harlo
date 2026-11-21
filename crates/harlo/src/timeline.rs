//! A session's timeline: rows pinned in stack slots, built only near the viewport.
//!
//! Long sessions have thousands of rows. Building them all each frame stutters,
//! so only rows near the viewport are built. Row `i` always sits in stack slot
//! `i % SLOTS` at its offset, which keeps its widget state (shaped text) for as
//! long as it stays near the viewport. A `sensor` reports each built row's
//! real height; until then an estimate stands in. The scrollable is anchored
//! to the bottom, so the newest rows stay put while heights settle.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Tokens;
use harlo_core::timeline::Entry;
use harlo_core::workspace::ToolStatus;
use iced::widget::{
    Id, center, column, container, markdown, pin, row, scrollable, sensor, space, stack, text,
};
use iced::{Center, Element, Fill, Size, padding};
use lucide_icons::Icon;

use crate::style::{self, size};
use crate::view::icon;

/// Stack slots for rows near the viewport.
const SLOTS: usize = 64;

/// Pixels above and below the viewport whose rows stay built.
const OVERSCAN: f32 = 900.0;

/// Gap between rows.
const SPACING: f32 = 14.0;

/// The timeline column's widest size; it narrows with the window.
pub const WIDTH: f32 = 780.0;

/// A guess at the viewport height before the first scroll report.
const FIRST_VIEWPORT: f32 = 1_000.0;

/// Something the timeline reports.
#[derive(Debug, Clone)]
pub enum Event {
    /// The view scrolled or resized.
    Scrolled(scrollable::Viewport),
    /// A row was measured: its index and size.
    Measured(usize, Size),
    /// A link in an answer was clicked.
    Link(String),
}

/// Where loading a session's history stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    /// The history is on its way.
    Loading,
    /// The rows are the session's history.
    Ready,
    /// Loading failed, with the reason.
    Failed(String),
}

/// One row: its entry, and the parsed Markdown for answers.
#[derive(Debug)]
struct Row {
    entry: Entry,
    markdown: Option<markdown::Content>,
}

impl Row {
    fn new(entry: Entry) -> Self {
        let markdown = match &entry {
            Entry::Answer { text, .. } => Some(markdown::Content::parse(text)),
            _ => None,
        };
        Self { entry, markdown }
    }
}

/// A session's rows and how far the view is scrolled.
#[derive(Debug)]
pub struct Timeline {
    scroller: Id,
    /// Where loading stands.
    pub load: Load,
    rows: Vec<Row>,
    heights: Vec<f32>,
    from_bottom: f32,
    viewport: f32,
}

impl Timeline {
    /// Creates an empty timeline for a session whose history is loading.
    #[must_use]
    pub fn new(session_id: &str) -> Self {
        Self {
            scroller: Id::from(format!("timeline-{session_id}")),
            load: Load::Loading,
            rows: Vec::new(),
            heights: Vec::new(),
            from_bottom: 0.0,
            viewport: FIRST_VIEWPORT,
        }
    }

    /// Returns how many rows the timeline has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Returns whether the timeline has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Replaces the rows with a fresh history.
    ///
    /// Rows whose key and content are unchanged keep their parsed Markdown and
    /// measured height, so reloading a session doesn't make it jump.
    pub fn set(&mut self, entries: Vec<Entry>) {
        let mut old: std::collections::HashMap<String, (Row, f32)> = self
            .rows
            .drain(..)
            .zip(self.heights.drain(..))
            .map(|(row, height)| (row.entry.key().to_owned(), (row, height)))
            .collect();
        for entry in entries {
            let (row, height) = match old.remove(entry.key()) {
                Some((row, height)) if row.entry == entry => (row, height),
                Some((_, height)) => (Row::new(entry), height),
                None => {
                    let height = estimate(&entry);
                    (Row::new(entry), height)
                }
            };
            self.rows.push(row);
            self.heights.push(height);
        }
        self.load = Load::Ready;
    }

    /// Records a failed load; rows already shown stay.
    pub fn fail(&mut self, reason: String) {
        self.load = Load::Failed(reason);
    }

    /// Applies a timeline event.
    pub fn update(&mut self, event: &Event) {
        match event {
            Event::Scrolled(viewport) => {
                // Anchored to the bottom, the offset counts up from the end.
                self.from_bottom = viewport.absolute_offset().y;
                self.viewport = viewport.bounds().height;
            }
            Event::Measured(index, size) => {
                if let Some(height) = self.heights.get_mut(*index) {
                    *height = size.height;
                }
            }
            Event::Link(_) => {}
        }
    }

    /// The timeline, scrolled with the newest rows at the bottom.
    pub fn view<'a>(&'a self, t: &Tokens) -> Element<'a, Event> {
        let body: Element<'a, Event> = match (&self.load, self.rows.is_empty()) {
            (Load::Loading, true) => return note(t, "Loading…"),
            (Load::Failed(reason), true) => {
                return note(t, &format!("Couldn't load this session: {reason}"));
            }
            (_, true) => return note(t, "No messages yet."),
            _ => self.slots(t),
        };
        let column = container(body)
            .max_width(WIDTH)
            .padding(padding::vertical(18));
        scrollable(container(column).center_x(Fill).padding([0, 24]))
            .id(self.scroller.clone())
            .anchor_bottom()
            .on_scroll(Event::Scrolled)
            .height(Fill)
            .style(style::thin_scroll(t))
            .into()
    }

    /// Pins the rows near the viewport at their offsets, row `i` in slot `i % SLOTS`.
    fn slots<'a>(&'a self, t: &Tokens) -> Element<'a, Event> {
        let total: f32 = self.heights.iter().map(|h| h + SPACING).sum::<f32>() - SPACING;
        let bottom = total - self.from_bottom;
        let (low, high) = (bottom - self.viewport - OVERSCAN, bottom + OVERSCAN);
        let mut slots: Vec<Option<Element<'a, Event>>> = (0..SLOTS).map(|_| None).collect();
        let mut top = 0.0;
        for (i, (row, height)) in self.rows.iter().zip(&self.heights).enumerate() {
            if top + height >= low && top <= high {
                slots[i % SLOTS] = Some(measured(i, row_view(t, row), top));
            }
            top += height + SPACING;
        }
        let mut layers: Vec<Element<'a, Event>> = Vec::with_capacity(SLOTS + 1);
        layers.push(space().width(Fill).height(total.max(0.0)).into());
        layers.extend(
            slots
                .into_iter()
                .map(|slot| slot.unwrap_or_else(|| space().into())),
        );
        stack(layers).width(Fill).into()
    }
}

fn measured(index: usize, content: Element<'_, Event>, top: f32) -> Element<'_, Event> {
    let content = sensor(content)
        .key(index)
        .anticipate(OVERSCAN * 2.0)
        .on_show(move |size| Event::Measured(index, size))
        .on_resize(move |size| Event::Measured(index, size));
    pin(container(content).width(Fill)).y(top).into()
}

fn note<'a>(t: &Tokens, message: &str) -> Element<'a, Event> {
    center(
        text(message.to_owned())
            .size(size::LABEL)
            .color(style::color(t.text_weak)),
    )
    .into()
}

/// A first guess at a row's height, before it's measured.
#[expect(
    clippy::cast_precision_loss,
    reason = "line counts fit easily in an f32"
)]
fn estimate(entry: &Entry) -> f32 {
    let lines = |text: &str| {
        text.lines()
            .map(|line| line.chars().count() / 95 + 1)
            .sum::<usize>() as f32
    };
    match entry {
        Entry::Prompt { text, .. } => lines(text) * 22.0 + 26.0,
        Entry::Answer { text, .. } => lines(text) * 24.0,
        Entry::Thought { .. } | Entry::Tool { .. } | Entry::Stopped { .. } => 24.0,
    }
}

/// Markdown settings for answers, from the theme tokens.
fn markdown_settings(t: &Tokens) -> markdown::Settings {
    let mut settings = markdown::Settings::with_text_size(
        size::ANSWER,
        markdown::Style {
            font: style::SANS,
            inline_code_highlight: iced::advanced::text::Highlight {
                background: style::color(t.surface_raised_strong).into(),
                border: iced::border::rounded(3),
            },
            inline_code_padding: padding::left(2).right(2),
            inline_code_color: style::color(t.icon_success_base.over(t.text_base, 0.7)),
            inline_code_font: style::MONO,
            code_block_font: style::MONO,
            link_color: style::color(t.text_interactive_base),
        },
    );
    settings.code_size = size::CODE.into();
    settings.h1_size = (size::ANSWER * 1.5).into();
    settings.h2_size = (size::ANSWER * 1.3).into();
    settings.h3_size = (size::ANSWER * 1.15).into();
    settings
}

fn row_view<'a>(t: &Tokens, row: &'a Row) -> Element<'a, Event> {
    match &row.entry {
        Entry::Prompt { text: prompt, .. } => container(
            text(prompt.as_str())
                .size(size::ANSWER)
                .color(style::color(t.text_strong)),
        )
        .width(Fill)
        .padding([11, 16])
        .style(style::boxed(
            t.surface_raised_base,
            Some(t.border_weak_base),
            size::COMPOSER_RADIUS,
        ))
        .into(),
        Entry::Answer { .. } => {
            let items = row
                .markdown
                .as_ref()
                .map_or(&[][..], markdown::Content::items);
            markdown::view(items, markdown_settings(t)).map(Event::Link)
        }
        Entry::Thought { summary, .. } => line(
            t,
            icon(Icon::Circle, 8.0, t.icon_weak_base).into(),
            "Thought",
            summary,
            None,
            false,
        ),
        Entry::Tool {
            label,
            detail,
            status,
            took,
            ..
        } => {
            let (glyph, color) = match status {
                ToolStatus::Completed => (Icon::Dot, t.icon_base),
                ToolStatus::Failed => (Icon::X, t.icon_critical_base),
                ToolStatus::Running | ToolStatus::Streaming => {
                    (Icon::Loader, t.text_interactive_base)
                }
            };
            let took = took.map(crate::view::duration);
            line(
                t,
                icon(glyph, 14.0, color).into(),
                label,
                detail,
                took,
                true,
            )
        }
        Entry::Stopped { outcome, .. } => {
            let what = if outcome == "interrupted" {
                "Stopped"
            } else {
                "The run failed"
            };
            line(
                t,
                icon(Icon::CircleAlert, 13.0, t.icon_warning_base).into(),
                what,
                "",
                None,
                false,
            )
        }
    }
}

/// A one-line row: a marker, a label, its detail and a time on the right.
fn line<'a>(
    t: &Tokens,
    marker: Element<'a, Event>,
    label: &'a str,
    detail: &'a str,
    took: Option<String>,
    mono: bool,
) -> Element<'a, Event> {
    let detail = crate::widget::ellipsis(detail)
        .size(if mono { size::CODE } else { size::SMALL })
        .font(if mono { style::MONO } else { style::SANS })
        .color(style::color(t.text_weak));
    let mut content = row![
        container(marker).width(14).center_x(14),
        text(label)
            .size(size::SMALL)
            .font(style::SANS_MEDIUM)
            .color(style::color(t.text_base)),
        detail,
    ]
    .spacing(9)
    .align_y(Center);
    if let Some(took) = took {
        content = content.push(
            text(took)
                .size(size::SMALL)
                .color(style::color(t.text_weaker)),
        );
    }
    column![content].height(24).into()
}
