//! OpenCode's automations: prompts the server sends to a session at a time,
//! every so often, or on a cron schedule, whether or not a client is open.
//!
//! They're experimental in OpenCode: creating one needs
//! `experimental.automations` in the config of its folder, and the API may
//! change between releases.

// Rust guideline compliant 2026-10-09

use reqwest::Method;
use serde_json::{Value, json};

use crate::client::Client;
use crate::error::Error;
use crate::generated as g;

/// When an automation runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// Once: an instant with an offset, a wall-clock time such as
    /// `2026-10-27T09:00`, or a phrase such as `in 20 minutes`. Once it
    /// has run, the server shows the instant it worked out.
    At(String),
    /// Every so often, such as `30 minutes` or `2 hours` (at least a
    /// minute).
    Every(String),
    /// On a cron schedule, such as `0 9 * * 1-5`.
    Cron(String),
    /// When a connected MCP server pushes an event.
    Event {
        /// The MCP server.
        server: String,
        /// Its event.
        event: String,
    },
}

/// What an automation does when it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Sends a prompt to a session.
    Prompt {
        /// The prompt.
        prompt: String,
        /// The session it goes to, or `None` when a subagent gets it.
        session: Option<String>,
        /// The subagent that gets it, started under the automation's
        /// session.
        subagent: Option<String>,
    },
    /// Runs Code Mode JavaScript, with no model.
    Code {
        /// The program.
        code: String,
    },
}

/// Whether an automation runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// It runs when due.
    Active,
    /// It waits to be resumed.
    Paused,
    /// It won't run again: a one-off that ran, or one that stopped itself.
    Done,
}

/// An automation on the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Automation {
    /// Its ID, `aut_…`.
    pub id: String,
    /// The short label lists show.
    pub name: String,
    /// The session that created it, if one did.
    pub session_id: Option<String>,
    /// The folder it runs in.
    pub directory: String,
    /// When it runs.
    pub trigger: Trigger,
    /// The time zone its times are in, such as `America/Toronto`.
    pub time_zone: String,
    /// What it does.
    pub action: Action,
    /// Whether it runs.
    pub status: Status,
    /// When it runs next, in milliseconds since the Unix epoch.
    pub next_due_at: Option<f64>,
    /// How many times it has run.
    pub run_count: u64,
    /// When it last ran, in milliseconds since the Unix epoch.
    pub last_run_at: Option<f64>,
    /// How its last run went: `fired`, `late`, `skipped`, `disabled`,
    /// `failed` or `expired`.
    pub last_outcome: Option<String>,
    /// What its last run said, such as where the prompt went.
    pub last_output: Option<String>,
}

impl From<g::AutomationInfo> for Automation {
    fn from(info: g::AutomationInfo) -> Self {
        let trigger = match info.trigger {
            g::AutomationTrigger1::At(t) => Trigger::At(t.at),
            g::AutomationTrigger1::Every(t) => Trigger::Every(t.every.to_string()),
            g::AutomationTrigger1::Cron(t) => Trigger::Cron(t.cron),
            g::AutomationTrigger1::Event(t) => Trigger::Event {
                server: t.server.to_string(),
                event: t.event.to_string(),
            },
        };
        let action = match info.action {
            g::AutomationAction::Prompt(p) => {
                let (session, subagent) = match p.to {
                    g::AutomationTarget::Variant0 { session, .. } => {
                        (Some(session.to_string()), None)
                    }
                    g::AutomationTarget::Variant1 { subagent, .. } => (None, Some(subagent.0)),
                };
                Action::Prompt {
                    prompt: p.prompt,
                    session,
                    subagent,
                }
            }
            g::AutomationAction::Code(c) => Action::Code { code: c.code },
        };
        Self {
            id: info.id.to_string(),
            name: info.name,
            session_id: info.session_id.map(|id| id.to_string()),
            directory: info.directory,
            trigger,
            time_zone: info.tz,
            action,
            status: match info.status {
                g::AutomationStatus::Active => Status::Active,
                g::AutomationStatus::Paused => Status::Paused,
                g::AutomationStatus::Done => Status::Done,
            },
            next_due_at: info.next_due_at,
            run_count: u64::try_from(info.run_count).unwrap_or_default(),
            last_run_at: info.last_run_at,
            last_outcome: info.last_outcome.map(|o| o.to_string()),
            last_output: info.last_output,
        }
    }
}

/// A new automation that sends a prompt to a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAutomation {
    /// The short label lists show.
    pub name: String,
    /// The session that gets the prompt; the automation runs in its
    /// folder.
    pub session_id: String,
    /// When it runs: [`Trigger::At`], [`Trigger::Every`] or
    /// [`Trigger::Cron`].
    pub trigger: Trigger,
    /// The prompt.
    pub prompt: String,
}

impl NewAutomation {
    /// The request body.
    fn json(&self) -> Value {
        let trigger = match &self.trigger {
            Trigger::At(at) => json!({ "type": "at", "at": at }),
            Trigger::Every(every) => json!({ "type": "every", "every": every }),
            Trigger::Cron(cron) => json!({ "type": "cron", "cron": cron }),
            Trigger::Event { server, event } => {
                json!({ "type": "event", "server": server, "event": event })
            }
        };
        json!({
            "name": self.name,
            "sessionID": self.session_id,
            "trigger": trigger,
            "action": { "type": "prompt", "prompt": self.prompt },
        })
    }
}

impl Client {
    /// Lists every project's automations, oldest first.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn automations(&self) -> Result<Vec<Automation>, Error> {
        let v = Self::json(self.request(Method::GET, "/api/experimental/automation")).await?;
        automations(&v)
    }

    /// Creates an automation that sends a prompt to a session.
    ///
    /// # Errors
    ///
    /// Fails when automations are off in the session folder's config (a
    /// forbidden error saying so), when the trigger doesn't parse (the
    /// server says why), when the session doesn't exist, or when the
    /// server can't be reached.
    pub async fn create_automation(&self, new: &NewAutomation) -> Result<Automation, Error> {
        let req = self
            .request(Method::POST, "/api/experimental/automation")
            .json(&new.json());
        automation(&Self::json(req).await?)
    }

    /// Stops an automation from running until it's resumed.
    ///
    /// # Errors
    ///
    /// Fails when it doesn't exist or the server can't be reached.
    pub async fn pause_automation(&self, id: &str) -> Result<Automation, Error> {
        let path = format!("/api/experimental/automation/{id}/pause");
        automation(&Self::json(self.request(Method::POST, &path)).await?)
    }

    /// Lets a paused automation run again: a missed one-off runs once, and
    /// a repeating one at its next time.
    ///
    /// # Errors
    ///
    /// Fails when it doesn't exist or the server can't be reached.
    pub async fn resume_automation(&self, id: &str) -> Result<Automation, Error> {
        let path = format!("/api/experimental/automation/{id}/resume");
        automation(&Self::json(self.request(Method::POST, &path)).await?)
    }

    /// Deletes an automation.
    ///
    /// # Errors
    ///
    /// Fails when it doesn't exist or the server can't be reached.
    pub async fn delete_automation(&self, id: &str) -> Result<(), Error> {
        let path = format!("/api/experimental/automation/{id}");
        Self::send(self.request(Method::DELETE, &path)).await?;
        Ok(())
    }
}

/// The automation in an answer's `data`.
fn automation(v: &Value) -> Result<Automation, Error> {
    Ok(serde_json::from_value::<g::AutomationInfo>(v["data"].clone())?.into())
}

/// The automations in an answer's `data`.
fn automations(v: &Value) -> Result<Vec<Automation>, Error> {
    let list = v["data"].as_array().cloned().unwrap_or_default();
    list.into_iter()
        .map(|item| Ok(serde_json::from_value::<g::AutomationInfo>(item)?.into()))
        .collect()
}
