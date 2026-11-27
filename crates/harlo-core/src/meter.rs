//! The usage meter: how much of a provider's limits is used, at a glance.
//!
//! A [`Reading`] is one provider's use, however it was read: a Claude
//! plan's from Claude Code, the others' from the server. The meter shows
//! one of them, the provider pinned to it, else the one used last, else
//! the fullest, as a ring filled to its fullest limit.
//!
//! Most providers tell an app nothing of a limit. Such a one still shows
//! as itself, with what it does tell (an amount spent or left) and
//! whether its last request was refused over a limit.

// Rust guideline compliant 2026-10-09

use crate::claude::{self, Limit, Severity};
use crate::server::ProviderUsage;

/// The server's provider that runs a Claude plan through Claude Code.
pub const CLAUDE: &str = claude::INTEGRATION;

/// How full a limit is before the meter warns, in percent.
const WARNING_FROM: f64 = 75.0;

/// How full a limit is before the meter shows it as nearly used up, in
/// percent.
const CRITICAL_FROM: f64 = 95.0;

/// One limit of a provider, and how much of it is used.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    /// What it limits, such as `5-hour session`.
    pub label: String,
    /// How much of it is used, from 0 to 100.
    pub percent: f64,
    /// When it resets, in seconds since the Unix epoch.
    pub resets_at: Option<i64>,
    /// How urgent the provider itself says it is, when it grades it.
    pub graded: Option<Level>,
}

impl Window {
    /// How urgent it is: the provider's own grade, else by how full it is.
    #[must_use]
    pub fn level(&self) -> Level {
        self.graded.unwrap_or(if self.percent >= CRITICAL_FROM {
            Level::Critical
        } else if self.percent >= WARNING_FROM {
            Level::Warning
        } else {
            Level::Calm
        })
    }

    /// How full it is and when it resets, as of `now` (seconds since the
    /// Unix epoch): "23% · resets in 2 h 14 min".
    #[must_use]
    pub fn status(&self, now: i64) -> String {
        let percent = percent(self.percent);
        match self.resets_at {
            Some(at) if at > now => format!("{percent} · resets {}", claude::resets_in(at - now)),
            _ => percent,
        }
    }
}

/// How urgent a limit's use is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Plenty left.
    Calm,
    /// Getting close.
    Warning,
    /// At or near the limit.
    Critical,
}

/// One provider's use.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// The server's ID of the provider.
    pub provider: String,
    /// The provider's name.
    pub name: String,
    /// The plan the account is on, when it's known, such as `Max`.
    pub plan: Option<String>,
    /// Its limits, in the order the provider lists them.
    pub windows: Vec<Window>,
    /// An amount it tells of, such as `Balance` and `$4.20`.
    pub amount: Option<(String, String)>,
    /// The page where the provider shows the use itself.
    pub manage: Option<String>,
    /// Whether its last request was refused over a limit.
    pub reached: bool,
    /// Why it couldn't be read, when it couldn't.
    pub error: Option<String>,
}

impl Reading {
    /// A provider nothing is known of but its name.
    #[must_use]
    pub fn unknown(provider: &str, name: &str) -> Self {
        Self {
            provider: provider.to_owned(),
            name: name.to_owned(),
            plan: None,
            windows: Vec::new(),
            amount: None,
            manage: None,
            reached: false,
            error: None,
        }
    }

    /// A Claude plan's use, from the `limits` Claude Code reported.
    #[must_use]
    pub fn claude(limits: &[Limit], plan: Option<&str>) -> Self {
        let windows = limits
            .iter()
            .map(|limit| Window {
                label: match (limit.kind.as_str(), &limit.model) {
                    ("session", _) => "5-hour session".to_owned(),
                    ("weekly_all", _) => "This week".to_owned(),
                    (_, Some(model)) if limit.group == "weekly" => format!("This week, {model}"),
                    _ => limit.kind.replace('_', " "),
                },
                percent: limit.percent,
                resets_at: limit.resets_at,
                graded: Some(match limit.severity {
                    Severity::Normal => Level::Calm,
                    Severity::Warning => Level::Warning,
                    Severity::Critical => Level::Critical,
                }),
            })
            .collect();
        Self {
            plan: plan.map(capitalized),
            windows,
            ..Self::unknown(CLAUDE, "Claude")
        }
    }

    /// What the server read of a provider, shown under `name`.
    #[must_use]
    pub fn from_server(usage: &ProviderUsage, name: &str) -> Self {
        Self {
            provider: usage.provider.clone(),
            name: name.to_owned(),
            plan: usage.plan.clone(),
            windows: usage
                .limits
                .iter()
                .map(|limit| Window {
                    label: limit.label.clone(),
                    percent: limit.percent,
                    resets_at: limit.resets_at,
                    graded: None,
                })
                .collect(),
            amount: usage
                .amount
                .as_ref()
                .map(|a| (a.label.clone(), money(a.value, &a.currency))),
            manage: usage.manage.clone(),
            reached: usage.reached,
            error: usage.error.clone(),
        }
    }

    /// The limit that is fullest, which is the one that stops work first.
    #[must_use]
    pub fn fullest(&self) -> Option<&Window> {
        self.windows
            .iter()
            .max_by(|a, b| a.percent.total_cmp(&b.percent))
    }

    /// How much of the ring is filled, from 0 to 1, or `None` when the
    /// provider tells of no limit and none was reached.
    #[must_use]
    pub fn fraction(&self) -> Option<f64> {
        if self.reached {
            return Some(1.0);
        }
        self.fullest().map(|w| (w.percent / 100.0).clamp(0.0, 1.0))
    }

    /// How urgent its use is.
    #[must_use]
    pub fn level(&self) -> Level {
        if self.reached {
            return Level::Critical;
        }
        self.windows
            .iter()
            .map(Window::level)
            .max()
            .unwrap_or(Level::Calm)
    }

    /// The few characters under the ring: the fullest limit's percent, a
    /// reached limit, an amount, or a dash when nothing is known.
    #[must_use]
    pub fn short(&self) -> String {
        if let Some(window) = self.fullest() {
            return percent(window.percent);
        }
        if self.reached {
            return "limit".to_owned();
        }
        match &self.amount {
            Some((_, amount)) => amount.clone(),
            None if self.manage.is_some() => "plan".to_owned(),
            None => "—".to_owned(),
        }
    }

    /// Its name with its plan: "Claude · Max".
    #[must_use]
    pub fn title(&self) -> String {
        match &self.plan {
            Some(plan) if !plan.eq_ignore_ascii_case(&self.name) => {
                format!("{} · {plan}", self.name)
            }
            _ => self.name.clone(),
        }
    }

    /// One sentence on its use, as of `now` (seconds since the Unix
    /// epoch), for the meter's tip.
    #[must_use]
    pub fn sentence(&self, now: i64) -> String {
        let name = &self.name;
        if let Some(window) = self.fullest() {
            let reset = match window.resets_at {
                Some(at) if at > now => format!(", resets {}", claude::resets_in(at - now)),
                _ => String::new(),
            };
            return format!(
                "{name}: {} of {} used{reset}",
                percent(window.percent),
                lower_first(&window.label)
            );
        }
        if self.reached {
            return format!("{name} refused its last request over a limit");
        }
        if let Some(why) = &self.error {
            return format!("{name}'s use couldn't be read. {why}");
        }
        match &self.amount {
            Some((label, amount)) => format!("{name}: {} {amount}", lower_first(label)),
            None if self.manage.is_some() => {
                format!("{name} shows this plan's limits on its own page only")
            }
            None => format!("{name} tells apps nothing of its limits"),
        }
    }
}

/// Which reading the meter shows, and why that one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown<'a> {
    /// The reading.
    pub reading: &'a Reading,
    /// Why it's the one shown.
    pub why: Why,
}

/// Why the meter shows a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// It was pinned to the meter.
    Pinned,
    /// It was used last.
    Last,
    /// Nothing was used yet, and it's the fullest.
    Fullest,
}

/// Picks what the meter shows of `readings`: the provider `pin` names,
/// else the one `last` names, else the fullest. `None` with nothing to show.
#[must_use]
pub fn shown<'a>(readings: &'a [Reading], pin: &str, last: &str) -> Option<Shown<'a>> {
    let named = |id: &str| readings.iter().find(|r| !id.is_empty() && r.provider == id);
    if let Some(reading) = named(pin) {
        return Some(Shown {
            reading,
            why: Why::Pinned,
        });
    }
    if let Some(reading) = named(last) {
        return Some(Shown {
            reading,
            why: Why::Last,
        });
    }
    readings
        .iter()
        .filter_map(|r| Some((r, r.fraction()?)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(reading, _)| Shown {
            reading,
            why: Why::Fullest,
        })
}

/// A share as a whole percent: "23%".
fn percent(percent: f64) -> String {
    format!("{:.0}%", percent.clamp(0.0, 100.0))
}

/// An amount of money, short enough for the rail: "$4.20", "$1.2k".
fn money(value: f64, currency: &str) -> String {
    let sign = match currency {
        "USD" => "$",
        "CNY" => "¥",
        "EUR" => "€",
        _ => "",
    };
    let amount = if value.abs() >= 1000.0 {
        format!("{:.1}k", value / 1000.0)
    } else if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    };
    if sign.is_empty() {
        format!("{amount} {currency}")
    } else {
        format!("{sign}{amount}")
    }
}

/// `text` with its first letter in upper case.
fn capitalized(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// `text` with its first letter in lower case, to stand inside a sentence.
fn lower_first(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(letters).collect()
    })
}
