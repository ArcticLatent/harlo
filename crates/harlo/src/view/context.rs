//! The Context panel's body: how full the context window is, the session's
//! usage and cost, and the instructions its agent was given.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_core::instructions::{self, Scope};
use harlo_core::server::Tokens;
use harlo_core::theme::Tokens as Theme;
use iced::widget::{Column, column, container, row, scrollable, space, text};
use iced::{Center, Element, Fill, Length};

use crate::app::{App, ContextFacts, Message};
use crate::style::{self, MONO, SANS_MEDIUM, SANS_SEMIBOLD, size};

/// The gap between the panel's sections.
const SECTION_GAP: f32 = 22.0;

/// The context bar's height.
const BAR_HEIGHT: f32 = 6.0;

/// The bar's resolution: how many parts it's split into.
const BAR_PARTS: u64 = 1_000;

/// How full the window gets before the bar warns, in parts of [`BAR_PARTS`].
const WARN_AT: u64 = 800;

/// The size of the big token count.
const BIG: f32 = 22.0;

/// The space between the panel's sections and their scrollbar.
const SCROLLBAR_GAP: f32 = 4.0;

/// The panel's body below its header.
pub fn body(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let Some(facts) = app.context_facts() else {
        return note(t, "No session selected.");
    };
    let sections = column![
        window(t, &facts),
        usage(t, &facts),
        instructions_view(t, &facts)
    ]
    .spacing(SECTION_GAP)
    .padding([16, 18]);
    scrollable(sections)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().spacing(SCROLLBAR_GAP),
        ))
        .style(style::thin_scroll(t))
        .height(Fill)
        .into()
}

/// Plain text in the panel, for an empty state.
fn note<'a>(t: &Theme, words: &str) -> Element<'a, Message> {
    container(
        text(words.to_owned())
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .padding([20, 18])
    .into()
}

/// A section's title.
fn title<'a>(t: &Theme, name: &str) -> Element<'a, Message> {
    text(name.to_owned())
        .font(SANS_MEDIUM)
        .size(size::LABEL)
        .color(style::color(t.text_strong))
        .into()
}

/// Weak text, for explanations.
fn weak<'a>(t: &Theme, words: String) -> Element<'a, Message> {
    text(words)
        .size(size::SMALL)
        .color(style::color(t.text_weak))
        .into()
}

/// A label on the left and its value on the right.
fn fact<'a>(t: &Theme, label: &str, value: String) -> Element<'a, Message> {
    row![
        text(label.to_owned())
            .size(size::LABEL)
            .color(style::color(t.text_base)),
        space().width(Fill),
        text(value)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

/// A token count, short: `950`, `12.4k`, `1.2M`.
#[must_use]
pub fn compact(n: u64) -> String {
    /// A thousand tokens.
    const K: u64 = 1_000;
    /// A million tokens.
    const M: u64 = 1_000_000;
    // Tenths, so one decimal shows without floating point.
    let tenths = |unit: u64| {
        let tenths = (n * 10 + unit / 2) / unit;
        if tenths.is_multiple_of(10) || tenths >= 1_000 {
            format!("{}", tenths / 10)
        } else {
            format!("{}.{}", tenths / 10, tenths % 10)
        }
    };
    // Counts that would round up to "1000k" show as millions.
    if n < K {
        n.to_string()
    } else if n < M - K / 2 {
        format!("{}k", tenths(K))
    } else {
        format!("{}M", tenths(M))
    }
}

/// A cost in dollars: `$0.42`, or `<$0.01` for a little.
#[must_use]
pub fn dollars(cost: f64) -> String {
    /// A cent, the smallest amount shown.
    const CENT: f64 = 0.01;
    if cost > 0.0 && cost < CENT {
        "<$0.01".to_owned()
    } else {
        format!("${cost:.2}")
    }
}

/// How full the context window is, with a bar when the model's size is known.
fn window<'a>(t: &Theme, facts: &ContextFacts<'_>) -> Element<'a, Message> {
    let mut section = Column::new().spacing(8).push(title(t, "Context window"));
    if let Some(error) = facts.error {
        return section
            .push(
                text(format!("Couldn't load it: {error}"))
                    .size(size::SMALL)
                    .color(style::color(t.icon_critical_base)),
            )
            .into();
    }
    if facts.loading {
        return section
            .push(weak(t, "Reading what the model sees…".to_owned()))
            .into();
    }
    let Some(step) = facts.context.and_then(|c| c.last_step) else {
        return section
            .push(weak(t, "Nothing in it until the model answers.".to_owned()))
            .into();
    };
    let used = step.total();
    let mut count = row![
        text(compact(used))
            .font(SANS_SEMIBOLD)
            .size(BIG)
            .color(style::color(t.text_strong))
    ]
    .spacing(6)
    .align_y(iced::Alignment::End);
    count = count.push(weak(
        t,
        match facts.limit {
            Some(limit) => format!("of {} tokens", compact(limit)),
            None => "tokens".to_owned(),
        },
    ));
    section = section.push(count);
    if let Some(limit) = facts.limit {
        let parts = (used.saturating_mul(BAR_PARTS) / limit).min(BAR_PARTS);
        section = section.push(bar(t, parts));
        let percent = used.saturating_mul(100) / limit;
        section = section.push(weak(
            t,
            if percent == 0 && used > 0 {
                "Under 1% full".to_owned()
            } else {
                format!("{percent}% full")
            },
        ));
    }
    if let Some(context) = facts.context {
        let messages = if context.messages == 1 {
            "1 message".to_owned()
        } else {
            format!("{} messages", context.messages)
        };
        section = section.push(weak(t, format!("{messages} since the last compaction")));
    }
    section.into()
}

/// A bar `parts` of [`BAR_PARTS`] full, in the warning colour when nearly full.
fn bar<'a>(t: &Theme, parts: u64) -> Element<'a, Message> {
    let colour = if parts >= WARN_AT {
        t.icon_warning_base
    } else {
        t.text_interactive_base
    };
    let portion = |n: u64| Length::FillPortion(u16::try_from(n).unwrap_or(u16::MAX));
    let mut fill = row![];
    if parts > 0 {
        fill = fill.push(
            container(space())
                .width(portion(parts))
                .height(BAR_HEIGHT)
                .style(style::boxed(colour, None, BAR_HEIGHT / 2.0)),
        );
    }
    if parts < BAR_PARTS {
        fill = fill.push(space().width(portion(BAR_PARTS - parts)));
    }
    container(fill)
        .width(Fill)
        .height(BAR_HEIGHT)
        .style(style::boxed(t.surface_inset_base, None, BAR_HEIGHT / 2.0))
        .into()
}

/// The model, agent, and what the session has used and cost.
fn usage<'a>(t: &Theme, facts: &ContextFacts<'_>) -> Element<'a, Message> {
    let mut section = Column::new().spacing(6).push(title(t, "This session"));
    if let Some(model) = &facts.model {
        section = section.push(fact(t, "Model", model.clone()));
    }
    if let Some(agent) = &facts.agent {
        section = section.push(fact(t, "Agent", agent.clone()));
    }
    let Tokens {
        input,
        output,
        reasoning,
        cache_read,
        cache_write,
    } = facts.tokens;
    section =
        section
            .push(fact(t, "Input", compact(input)))
            .push(fact(t, "Output", compact(output)));
    // Only some providers count these; nothing to say where they don't.
    for (label, n) in [
        ("Reasoning", reasoning),
        ("Cache read", cache_read),
        ("Cache write", cache_write),
    ] {
        if n > 0 {
            section = section.push(fact(t, label, compact(n)));
        }
    }
    section.push(fact(t, "Cost", dollars(facts.cost))).into()
}

/// The instruction files: given from the start, then loaded on the way.
fn instructions_view<'a>(t: &Theme, facts: &ContextFacts<'_>) -> Element<'a, Message> {
    let places = (facts.root, facts.home);
    let mut section = Column::new().spacing(6).push(title(t, "Instructions"));
    let loaded = facts
        .context
        .map(|c| c.loaded.as_slice())
        .unwrap_or_default();
    match facts.files {
        Some([]) if loaded.is_empty() => {
            section = section.push(weak(
                t,
                "No AGENTS.md or CLAUDE.md applies to this session.".to_owned(),
            ));
        }
        Some(files) => {
            for file in files {
                let scope = match file.scope {
                    Scope::Global => "everywhere",
                    Scope::Project => "project",
                };
                section = section.push(path_row(t, &file.path, places, scope));
            }
        }
        None if facts.loading => {}
        None => {
            section = section.push(weak(
                t,
                "The files given from the start can only be listed for folders on this computer."
                    .to_owned(),
            ));
        }
    }
    for path in loaded {
        section = section.push(path_row(t, Path::new(path), places, "picked up"));
    }
    section.into()
}

/// An instruction file's path, with where it applies on the right.
///
/// `places` are the project's root and the home folder, to shorten it.
fn path_row<'a>(
    t: &Theme,
    path: &Path,
    (root, home): (&Path, &Path),
    aside: &str,
) -> Element<'a, Message> {
    row![
        text(instructions::display_in(path, root, home))
            .font(MONO)
            .size(size::SMALL)
            .color(style::color(t.text_strong))
            .width(Fill),
        text(aside.to_owned())
            .size(size::SMALL)
            .color(style::color(t.text_weak)),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}
