//! Subagents in a session's timeline: the card for work handed to one, and
//! the row for each report it sends back.
//!
//! A card shows how the child session stands now, which the timeline learns
//! from [`Crew`], and opens the child session when clicked.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;

use harlo_core::theme::{Rgba, Tokens};
use harlo_core::workspace::Report;
use harlo_core::workspace::ToolStatus;
use iced::widget::{button, column, container, row, space, text};
use iced::{Center, Element, Fill, padding};
use lucide_icons::Icon;

use crate::app::capitalized;
use crate::motion::Moment;
use crate::style::{self, SANS_MEDIUM, size};
use crate::timeline::Event;
use crate::view::{breathing, icon};
use crate::widget::ellipsis;

/// How many lines of a report show under its row.
pub const REPORT_LINES: usize = 3;

/// How many characters of a report show under its row, at most.
const REPORT_CHARS: usize = 320;

/// How a subagent's child session stands now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildState {
    /// It's working.
    Working,
    /// It waits for the user: an approval or an answer.
    Waiting,
    /// It finished its work, for now.
    Idle,
}

/// An agent as the timeline shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct CrewAgent {
    /// Its name for people, such as `Explore`.
    pub name: String,
    /// Its configured colour.
    pub color: Option<Rgba>,
}

/// What the timeline knows of a session's subagents.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Crew {
    /// How each child session stands, by its ID.
    pub children: HashMap<String, ChildState>,
    /// Agents by ID, for their names and colours.
    pub agents: HashMap<String, CrewAgent>,
}

impl Crew {
    /// The agent's name, or its ID with a capital when the catalog doesn't know it.
    fn name(&self, agent: &str) -> String {
        self.agents
            .get(agent)
            .map_or_else(|| capitalized(agent), |a| a.name.clone())
    }

    /// The agent's colour, when it has one.
    fn color(&self, agent: &str) -> Option<Rgba> {
        self.agents.get(agent).and_then(|a| a.color)
    }
}

/// The card for work handed to a subagent.
pub fn card<'a>(
    t: &Tokens,
    crew: &Crew,
    moment: Moment,
    (agent, task, child, status): (&str, &str, Option<&String>, ToolStatus),
) -> Element<'a, Event> {
    let accent = crew.color(agent).unwrap_or(t.text_interactive_base);
    let mut line = row![
        icon(Icon::Bot, 14.0, accent),
        text(crew.name(agent))
            .font(SANS_MEDIUM)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
        ellipsis(task.to_owned())
            .size(size::LABEL)
            .color(style::color(t.text_base)),
        space().width(Fill),
    ]
    .spacing(9)
    .align_y(Center);
    let state = child.and_then(|id| crew.children.get(id)).copied();
    line = line.push(standing(t, moment, status, state));
    if let Some(child) = child {
        line = line.push(open(t, child));
    }
    container(line)
        .width(Fill)
        .padding([7, 12])
        .style(style::boxed(
            t.surface_raised_base,
            Some(t.border_weak_base),
            size::RADIUS,
        ))
        .into()
}

/// What a card says about where its subagent is.
fn standing<'a>(
    t: &Tokens,
    moment: Moment,
    status: ToolStatus,
    state: Option<ChildState>,
) -> Element<'a, Event> {
    let says = |words: &str, colour: Rgba| -> Element<'a, Event> {
        text(words.to_owned())
            .size(size::SMALL)
            .color(style::color(colour))
            .into()
    };
    let working = |words: &str| -> Element<'a, Event> {
        let colour = t.text_interactive_base;
        breathing(
            text(words.to_owned())
                .size(size::SMALL)
                .style(moment.breathing(colour)),
        )
    };
    match (status, state) {
        (ToolStatus::Failed, _) => says("Couldn't start", t.icon_critical_base),
        (ToolStatus::Streaming | ToolStatus::Running, _) => working("Starting…"),
        (ToolStatus::Completed, Some(ChildState::Working)) => working("Working"),
        (ToolStatus::Completed, Some(ChildState::Waiting)) => {
            says("Needs you", t.icon_warning_base)
        }
        (ToolStatus::Completed, Some(ChildState::Idle)) => says("Done", t.text_weak),
        (ToolStatus::Completed, None) => space().into(),
    }
}

/// A button opening the child session.
fn open<'a>(t: &Tokens, child: &str) -> Element<'a, Event> {
    button(
        row![
            text("Open")
                .size(size::SMALL)
                .color(style::color(t.text_interactive_base)),
            icon(Icon::ArrowRight, 12.0, t.text_interactive_base),
        ]
        .spacing(4)
        .align_y(Center),
    )
    .padding([2, 6])
    .on_press(Event::OpenSession(child.to_owned()))
    .style(style::ghost(t, false))
    .into()
}

/// The row for a subagent's report, with its first lines below.
///
/// OpenCode names the agent by name for a subagent it was asked to start,
/// and by ID for a command run as one, so both are looked up.
pub fn report<'a>(t: &Tokens, crew: &Crew, report: &Report) -> Element<'a, Event> {
    let agent = crew.name(&report.agent);
    let finished = report.state == "completed";
    let (glyph, colour, says) = if finished {
        (
            Icon::CornerDownLeft,
            t.icon_base,
            format!("{agent} reported back"),
        )
    } else {
        (
            Icon::CircleAlert,
            t.icon_warning_base,
            format!("{agent} stopped: {}", report.state),
        )
    };
    let header = row![
        container(icon(glyph, 13.0, colour)).width(14).center_x(14),
        text(says)
            .size(size::SMALL)
            .color(style::color(t.text_base)),
        ellipsis(report.task.clone())
            .size(size::SMALL)
            .color(style::color(t.text_weak)),
        space().width(Fill),
        open(t, &report.child),
    ]
    .spacing(9)
    .align_y(Center);
    let gist = gist(&report.text);
    if gist.is_empty() {
        return header.into();
    }
    column![
        header,
        container(
            text(gist)
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        )
        .padding(padding::left(23)),
    ]
    .spacing(2)
    .into()
}

/// The first lines of a report, cut short with "…".
#[must_use]
pub fn gist(report: &str) -> String {
    let lines: Vec<&str> = report
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut shown = lines
        .iter()
        .take(REPORT_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    let mut cut = lines.len() > REPORT_LINES;
    if shown.chars().count() > REPORT_CHARS {
        shown = shown.chars().take(REPORT_CHARS).collect();
        cut = true;
    }
    if cut {
        shown.push('…');
    }
    shown
}
