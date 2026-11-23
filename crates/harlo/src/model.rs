//! What the window shows that's worked out from the app's state: the
//! pickers' choices and rows, the connection's description, subagents as the
//! timeline names them.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::fmt;

use harlo_core::approvals::Shield;
use harlo_core::theme::Rgba;

use crate::app::{App, Message, SheetEvent, Status, capitalized};

/// Rows a menu shows before it scrolls.
pub const MENU_ROWS: usize = 8;

/// The height of one menu row.
pub const MENU_ROW: f32 = 32.0;

/// The space between menu rows.
pub const ROW_GAP: f32 = 2.0;

/// The note on an agent whose rules stop it from editing files.
pub const READ_ONLY: &str = "read-only";

/// The name of the open menu's scrolling list.
pub const MENU_LIST: &str = "picker-menu-list";

/// The name of the model menu's filter field.
pub const MENU_FILTER: &str = "picker-menu-filter";

/// The setting that turns on OpenCode's todo list.
pub const TODO_SETTING: &str = "\"experimental\": {\n  \"todo\": true\n}";

/// How many lines of a report show under its row.
pub const REPORT_LINES: usize = 3;

/// How many characters of a report show under its row, at most.
const REPORT_CHARS: usize = 320;

/// Which picker's menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picker {
    /// The folder a new session starts in.
    Folder,
    /// The agent.
    Agent,
    /// The reasoning level.
    Variant,
    /// The model.
    Model,
    /// How much the session may do without asking.
    Shield,
    /// The model new sessions use, set in OpenCode's config from Settings.
    DefaultModel,
    /// The model new sessions in the project shown in Settings start with.
    ProjectModel,
}

impl Picker {
    /// The picker's number in the UI.
    #[must_use]
    pub fn index(self) -> i32 {
        match self {
            Self::Folder => 0,
            Self::Agent => 1,
            Self::Variant => 2,
            Self::Model => 3,
            Self::Shield => 4,
            Self::DefaultModel => 5,
            Self::ProjectModel => 6,
        }
    }

    /// The picker the UI numbers `index`.
    #[must_use]
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::Folder,
            1 => Self::Agent,
            2 => Self::Variant,
            3 => Self::Model,
            4 => Self::Shield,
            5 => Self::DefaultModel,
            6 => Self::ProjectModel,
            _ => return None,
        })
    }
}

/// One row of a picker's menu.
#[derive(Debug, Clone)]
pub struct Item {
    /// The choice's name.
    pub label: String,
    /// A note on the right, such as a model's provider.
    pub detail: Option<String>,
    /// A colour dot before the name, such as an agent's.
    pub dot: Option<Rgba>,
    /// Whether it's the current choice.
    pub selected: bool,
    /// What choosing it does.
    pub message: Message,
}

/// A model the picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    /// The provider's ID.
    pub provider_id: String,
    /// The model's ID.
    pub id: String,
    /// The name shown.
    pub name: String,
}

impl fmt::Display for ModelChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// An agent the picker offers.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentChoice {
    /// The agent's ID.
    pub id: String,
    /// The name shown.
    pub name: String,
    /// What it's for.
    pub description: Option<String>,
    /// Its configured colour.
    pub color: Option<Rgba>,
    /// Whether it leaves files alone, as for planning.
    pub read_only: bool,
}

impl fmt::Display for AgentChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// A reasoning level; `None` is the model's default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant(pub Option<String>);

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_deref().unwrap_or("default"))
    }
}

/// A folder a new session can start in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// The folder, or `None` for the home folder.
    pub directory: Option<String>,
    /// The name shown.
    pub label: String,
}

impl fmt::Display for Folder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// Feedback on the composer's input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feedback {
    /// A send is on its way: the send button says so and waits.
    pub sending: bool,
    /// A pasted image is being read.
    pub pasting: bool,
}

/// Returns whether `text` matches a filter: every word of `filter` appears
/// in it, ignoring case.
#[must_use]
pub fn matches(text: &str, filter: &str) -> bool {
    let text = text.to_lowercase();
    filter
        .to_lowercase()
        .split_whitespace()
        .all(|word| text.contains(word))
}

/// The rows of `picker`'s menu, as it shows them.
///
/// The model menu lists only the models matching the filter.
#[must_use]
pub fn items(app: &App, picker: Picker) -> Vec<Item> {
    match picker {
        Picker::Folder => app
            .folder_choices()
            .into_iter()
            .map(|folder| Item {
                selected: folder.directory == app.new_folder,
                label: folder.label,
                detail: None,
                dot: None,
                message: Message::PickFolder(folder.directory),
            })
            .collect(),
        Picker::Agent => {
            let current = app.current_agent();
            app.agent_choices()
                .into_iter()
                .map(|agent| Item {
                    selected: Some(&agent.id) == current.as_ref(),
                    label: agent.name,
                    detail: agent.read_only.then(|| READ_ONLY.to_owned()),
                    dot: agent.color,
                    message: Message::PickAgent(agent.id),
                })
                .collect()
        }
        Picker::Variant => {
            let current = Variant(app.current_model().and_then(|m| m.variant));
            app.variant_choices()
                .into_iter()
                .map(|variant| Item {
                    selected: variant == current,
                    label: variant.to_string(),
                    detail: None,
                    dot: None,
                    message: Message::PickVariant(variant),
                })
                .collect()
        }
        Picker::Shield => {
            let current = app.current_shield();
            Shield::CHOICES
                .into_iter()
                .map(|mode| Item {
                    selected: mode == current,
                    label: mode.label().to_owned(),
                    detail: None,
                    dot: None,
                    message: Message::PickShield(mode),
                })
                .collect()
        }
        Picker::Model | Picker::DefaultModel | Picker::ProjectModel => {
            let current = match picker {
                Picker::DefaultModel => app.default_model_choice(),
                Picker::ProjectModel => app.shown_project_model(),
                _ => app.current_model_choice(),
            };
            app.models
                .iter()
                .filter(|model| {
                    matches(
                        &format!("{} {}", model.name, model.provider_id),
                        &app.picker_filter,
                    )
                })
                .map(|model| Item {
                    selected: current
                        .as_ref()
                        .is_some_and(|c| c.provider_id == model.provider_id && c.id == model.id),
                    label: model.name.clone(),
                    detail: Some(model.provider_id.clone()),
                    dot: None,
                    message: match picker {
                        Picker::DefaultModel => {
                            Message::Sheet(SheetEvent::DefaultModel(model.clone()))
                        }
                        Picker::ProjectModel => Message::Sheet(SheetEvent::Project(
                            crate::app::ProjectEvent::Model(Some(model.clone())),
                        )),
                        _ => Message::PickModel(model.clone()),
                    },
                })
                .collect()
        }
    }
}

/// Describes the connection for the location picker.
#[must_use]
pub fn status_text(status: &Status) -> String {
    match status {
        Status::Connecting { attempt: 1 } => "Connecting…".to_owned(),
        Status::Connecting { attempt } => format!("Reconnecting (attempt {attempt})…"),
        Status::Connected { version } if version == harlo_core::OPENCODE_VERSION => {
            format!("Connected · OpenCode {version}")
        }
        Status::Connected { version } => format!(
            "Connected · OpenCode {version}. Harlo is tested with {}.",
            harlo_core::OPENCODE_VERSION
        ),
        Status::Lost { reason, retry_in } => format!(
            "Can't connect: {reason}. Trying again in {} s.",
            retry_in.as_secs().max(1)
        ),
    }
}

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
    #[must_use]
    pub fn name(&self, agent: &str) -> String {
        self.agents
            .get(agent)
            .map_or_else(|| capitalized(agent), |a| a.name.clone())
    }

    /// The agent's colour, when it has one.
    #[must_use]
    pub fn color(&self, agent: &str) -> Option<Rgba> {
        self.agents.get(agent).and_then(|a| a.color)
    }
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

/// Formats how long something took: `850ms`, `3.7s`, `15s`, `2m 5s`.
#[must_use]
pub fn duration(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    match ms {
        0..1_000 => format!("{ms}ms"),
        1_000..10_000 => format!("{:.1}s", d.as_secs_f64()),
        10_000..60_000 => format!("{}s", d.as_secs()),
        _ => format!("{}m {}s", d.as_secs() / 60, d.as_secs() % 60),
    }
}

/// A token count, short: `950`, `12.4k`, `1.2M`.
///
/// From 100k up no decimal shows, and counts that would round to `1000k`
/// show as millions.
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
    if n < K {
        n.to_string()
    } else if n < M - K / 2 {
        format!("{}k", tenths(K))
    } else {
        format!("{}M", tenths(M))
    }
}

/// A cost in US dollars: `$0.42`, or `<$0.01` for a little.
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
