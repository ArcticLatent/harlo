//! The rail's usage meter: how much of a provider's limits is used.
//!
//! It shows the provider pinned to it from its menu, else the one a
//! session used last. A Claude plan's limits are the ones Settings →
//! Providers reads from Claude Code; the other providers' use is read by
//! the server, which keeps their keys. Both are read again now and then
//! and when a turn ends, so the ring follows the work.

// Rust guideline compliant 2026-10-09

use harlo_core::claude::{ClaudeCode, Login};
use harlo_core::meter::{self, CLAUDE, Reading, Shown};
use harlo_core::server::ProviderUsage;
use harlo_core::workspace::{EventKind, millis};

use super::{App, Load, Message, Task};

/// How long a reading of a Claude plan's limits stands before a turn that
/// ends has them read again, in milliseconds.
///
/// A reading starts Claude Code and waits for Claude's servers, so turns
/// that end one after another share one.
const CLAUDE_STANDS_MS: i64 = 60_000;

/// What the meter knows beyond the Claude plan's limits.
#[derive(Debug, Clone, Default)]
pub struct Meter {
    /// What the server's providers tell of their use.
    pub usage: Vec<ProviderUsage>,
    /// When a turn's end last had the Claude plan's limits read, in
    /// milliseconds since the Unix epoch.
    claude_read: i64,
}

/// Something that happened to the meter.
#[derive(Debug, Clone, PartialEq)]
pub enum MeterEvent {
    /// The server's providers' use was read, or couldn't be.
    Usage(Result<Vec<ProviderUsage>, String>),
    /// Show this provider whatever is used; empty follows the one used
    /// last again.
    Pin(String),
}

impl App {
    /// Every provider's use the meter can show: the Claude plan's, what
    /// the server read, and the pinned and the last used provider even
    /// when nothing is known of them.
    #[must_use]
    pub fn meter_readings(&self) -> Vec<Reading> {
        let mut readings = Vec::new();
        if let Load::Loaded(Some(limits)) = &self.claude.usage {
            let plan = match &self.claude.code {
                Load::Loaded(ClaudeCode::Installed {
                    login: Login::Subscription(account),
                    ..
                }) => account.plan.as_deref(),
                _ => None,
            };
            readings.push(Reading::claude(limits, plan));
        }
        readings.extend(self.server_readings());
        for id in [&self.preferences.meter_pin, &self.preferences.meter_last] {
            if !id.is_empty() && !readings.iter().any(|r| r.provider == *id) {
                readings.push(Reading::unknown(id, &provider_name(id)));
            }
        }
        readings
    }

    /// What the server read of its providers' use, each under the name
    /// people know the provider by.
    #[must_use]
    pub fn server_readings(&self) -> Vec<Reading> {
        self.meter
            .usage
            .iter()
            .map(|usage| Reading::from_server(usage, &provider_name(&usage.provider)))
            .collect()
    }

    /// What the meter shows of `readings`, which [`Self::meter_readings`]
    /// made.
    #[must_use]
    pub fn meter_shown<'a>(&self, readings: &'a [Reading]) -> Option<Shown<'a>> {
        meter::shown(
            readings,
            &self.preferences.meter_pin,
            &self.preferences.meter_last,
        )
    }

    /// Applies something that happened to the meter.
    pub(super) fn meter_event(&mut self, event: MeterEvent) -> Task<Message> {
        match event {
            MeterEvent::Usage(Ok(usage)) => self.meter.usage = usage,
            // An older server, or one that can't be reached: what was read
            // before still stands.
            MeterEvent::Usage(Err(_)) => {}
            MeterEvent::Pin(provider) => {
                if self.preferences.meter_pin != provider {
                    self.preferences.meter_pin = provider;
                    return self.save_preferences();
                }
            }
        }
        Task::none()
    }

    /// Asks the server what its providers tell of their use.
    pub(super) fn read_meter(&self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        Task::perform(
            async move { client.provider_usage().await.map_err(|e| e.to_string()) },
            |read| Message::Meter(MeterEvent::Usage(read)),
        )
    }

    /// Follows the server's event `kind`: a session at work makes its
    /// provider the one used last, and a turn that ended or a change of
    /// the server's providers has their use read again.
    ///
    /// `time` is the event's, in milliseconds since the Unix epoch.
    pub(super) fn meter_follow(&mut self, kind: &EventKind, time: f64) -> Task<Message> {
        let (session, ended) = match kind {
            EventKind::Busy { session_id } => (session_id, false),
            EventKind::Done { session_id, .. } => (session_id, true),
            // A provider signed in to or out of has other use to tell of.
            EventKind::CatalogChanged => return self.read_meter(),
            _ => return Task::none(),
        };
        // A session nobody picked a model for runs the server's default.
        let picked = self
            .workspace
            .session(session)
            .and_then(|s| s.model.as_ref())
            .map(|model| model.provider_id.clone());
        let default = || Some(self.catalog()?.default_model.as_ref()?.provider_id.clone());
        let Some(provider) = picked.or_else(default) else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        if self.preferences.meter_last != provider {
            self.preferences.meter_last.clone_from(&provider);
            tasks.push(self.save_preferences());
        }
        if ended {
            tasks.push(self.read_meter());
            let now = millis(time);
            let read = matches!(self.claude.usage, Load::Loaded(Some(_)));
            if provider == CLAUDE && read && now - self.meter.claude_read >= CLAUDE_STANDS_MS {
                self.meter.claude_read = now;
                tasks.push(self.read_usage());
            }
        }
        Task::batch(tasks)
    }
}

/// What people call the provider with the server's ID `id`.
fn provider_name(id: &str) -> String {
    harlo_core::makers::name(id).map_or_else(|| id.to_owned(), str::to_owned)
}
