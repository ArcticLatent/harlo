//! Desktop notifications: which events notify, when they're quiet, and what
//! they say.
//!
//! A notification's title and message come from a template per event, with
//! the session's details filled in for the `{…}` variables in
//! [`VARIABLES`]. An empty template uses the event's default.

// Rust guideline compliant 2026-10-09

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Minutes in a day.
const DAY_MINUTES: u16 = 24 * 60;

/// The longest `{last_message}` a notification shows, in characters.
pub const LAST_MESSAGE_CHARS: usize = 250;

/// The variables a template can use, without their braces.
pub const VARIABLES: [&str; 7] = [
    "project_name",
    "worktree",
    "branch",
    "session_name",
    "agent_name",
    "model_name",
    "last_message",
];

/// What happened, for a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A run finished.
    Finished,
    /// A run failed.
    Failed,
    /// The agent asks something or wants an action approved.
    Asks,
}

impl Kind {
    /// Every kind, in the order Settings lists them.
    pub const ALL: [Self; 3] = [Self::Finished, Self::Failed, Self::Asks];

    /// The default title and message.
    #[must_use]
    pub fn defaults(self) -> (&'static str, &'static str) {
        match self {
            Self::Finished => ("{agent_name} finished", "{session_name}: {last_message}"),
            Self::Failed => (
                "{agent_name} stopped with an error",
                "{session_name}: {last_message}",
            ),
            Self::Asks => ("{agent_name} needs you", "{session_name}: {last_message}"),
        }
    }
}

/// An event's switch and its templates; empty templates use the defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Template {
    /// Whether the event notifies.
    pub on: bool,
    /// The title's template.
    pub title: String,
    /// The message's template.
    pub message: String,
}

impl Default for Template {
    fn default() -> Self {
        Self {
            on: true,
            title: String::new(),
            message: String::new(),
        }
    }
}

/// Hours without notifications, such as the night.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Quiet {
    /// Whether they apply.
    pub on: bool,
    /// When they start, in minutes after midnight.
    pub from: u16,
    /// When they end, in minutes after midnight.
    pub to: u16,
    /// Whether questions and approvals still notify during them.
    pub questions: bool,
}

impl Default for Quiet {
    fn default() -> Self {
        Self {
            on: false,
            from: 22 * 60,
            to: 8 * 60,
            questions: false,
        }
    }
}

impl Quiet {
    /// Whether `minute` (after midnight) falls in them; they may span
    /// midnight, and equal ends cover nothing.
    #[must_use]
    pub fn covers(&self, minute: u16) -> bool {
        if !self.on || self.from == self.to {
            return false;
        }
        if self.from < self.to {
            (self.from..self.to).contains(&minute)
        } else {
            minute >= self.from || minute < self.to
        }
    }
}

/// The user's choices for desktop notifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches in Settings, saved as they are"
)]
pub struct Notifications {
    /// Whether Harlo notifies at all.
    pub enabled: bool,
    /// Whether it notifies while its window has focus; never for the
    /// session in view.
    pub while_focused: bool,
    /// Whether notifications play the desktop's message sound.
    pub sound: bool,
    /// A run finished.
    pub finished: Template,
    /// A run failed.
    pub failed: Template,
    /// The agent asks something.
    pub asks: Template,
    /// Whether Claude plan limits notify when used up and when they reset.
    pub limits: bool,
    /// Hours without notifications.
    pub quiet: Quiet,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            enabled: true,
            while_focused: false,
            sound: true,
            finished: Template::default(),
            failed: Template::default(),
            asks: Template::default(),
            limits: true,
            quiet: Quiet::default(),
        }
    }
}

impl Notifications {
    /// The template for `kind`.
    #[must_use]
    pub fn template(&self, kind: Kind) -> &Template {
        match kind {
            Kind::Finished => &self.finished,
            Kind::Failed => &self.failed,
            Kind::Asks => &self.asks,
        }
    }

    /// The template for `kind`, to change.
    pub fn template_mut(&mut self, kind: Kind) -> &mut Template {
        match kind {
            Kind::Finished => &mut self.finished,
            Kind::Failed => &mut self.failed,
            Kind::Asks => &mut self.asks,
        }
    }

    /// Whether `kind` notifies at local time `minute` (after midnight), or
    /// at any time when the local time isn't known.
    #[must_use]
    pub fn allows(&self, kind: Kind, minute: Option<u16>) -> bool {
        self.enabled && self.template(kind).on && !self.hushed(minute, kind == Kind::Asks)
    }

    /// Whether quiet hours hold notifications back at `minute`; a question
    /// gets through when they let questions through.
    #[must_use]
    pub fn hushed(&self, minute: Option<u16>, question: bool) -> bool {
        !(question && self.quiet.questions) && minute.is_some_and(|m| self.quiet.covers(m))
    }

    /// The title and message for `kind` with `values` filled in.
    #[must_use]
    pub fn render(&self, kind: Kind, values: &Values) -> (String, String) {
        let template = self.template(kind);
        let (title, message) = kind.defaults();
        let pick = |own: &str, default: &'static str| {
            if own.trim().is_empty() {
                default.to_owned()
            } else {
                own.to_owned()
            }
        };
        (
            render(&pick(&template.title, title), values),
            render(&pick(&template.message, message), values),
        )
    }
}

/// What the variables stand for in a notification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values {
    /// The project's name.
    pub project_name: String,
    /// The worktree's name, when the session is in one.
    pub worktree: String,
    /// The git branch.
    pub branch: String,
    /// The session's title.
    pub session_name: String,
    /// The agent's name.
    pub agent_name: String,
    /// The model's name.
    pub model_name: String,
    /// The agent's last words, the error, or what it asks.
    pub last_message: String,
}

impl Values {
    /// The value of `variable`, named without braces.
    fn get(&self, variable: &str) -> Option<&str> {
        Some(match variable {
            "project_name" => &self.project_name,
            "worktree" => &self.worktree,
            "branch" => &self.branch,
            "session_name" => &self.session_name,
            "agent_name" => &self.agent_name,
            "model_name" => &self.model_name,
            "last_message" => &self.last_message,
            _ => return None,
        })
    }
}

/// `template` with each known `{variable}` replaced by its value. Unknown
/// ones stay as they are; separators left at either end by empty values,
/// such as a `: ` before an empty message, go.
#[must_use]
pub fn render(template: &str, values: &Values) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let known = after
            .find('}')
            .and_then(|end| Some((values.get(&after[..end])?, end)));
        if let Some((value, end)) = known {
            out.push_str(value);
            rest = &after[end + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    let separators: &[char] = &[' ', ':', '·', '-', '—', ','];
    out.trim_matches(|c: char| separators.contains(&c) || c.is_whitespace())
        .to_owned()
}

/// `text` on one line, cut to `max` characters with an ellipsis.
#[must_use]
pub fn summary(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let cut: String = line.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// Minutes after local midnight at `time`, or `None` when the local time
/// zone can't be read.
#[must_use]
pub fn local_minute(time: SystemTime) -> Option<u16> {
    let zoned = jiff::Timestamp::try_from(time)
        .ok()?
        .to_zoned_datetime(jiff::tz::TimeZone::try_system().ok()?);
    let minute = i32::from(zoned.hour()) * 60 + i32::from(zoned.minute());
    u16::try_from(minute).ok().filter(|m| *m < DAY_MINUTES)
}

/// `minute` (after midnight) as `HH:MM`.
#[must_use]
pub fn clock(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60 % 24, minute % 60)
}

/// The minute after midnight `text` names as `H:MM` or `HH:MM`.
#[must_use]
pub fn parse_clock(text: &str) -> Option<u16> {
    let (hours, minutes) = text.trim().split_once(':')?;
    if minutes.len() != 2 {
        return None;
    }
    let (hours, minutes) = (hours.parse::<u16>().ok()?, minutes.parse::<u16>().ok()?);
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}
