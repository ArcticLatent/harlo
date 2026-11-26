//! Plan limits as phones show them: how much of the Claude plan on Harlo's
//! computer is used, read from Claude Code as Settings → Providers reads
//! it.
//!
//! Reading takes Claude Code a few seconds and an answer from Claude's
//! servers, so a phone that asks gets the last reading at once and a
//! `limits` event when a newer one arrives. While a paired phone wants
//! limit notifications, the limits are also read now and then, and a limit
//! used up or reset is told to the phones that aren't connected.

// Rust guideline compliant 2026-10-09

use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::watch;

use crate::claude::{self, Account, ClaudeCode, Limit, Login, Watch};

use super::push::{self, Alert, What};
use super::{Hub, lock, now_secs};

/// How old a reading may be before a phone that asks has it read again.
///
/// Short enough that opening the app shows the limits as they are, long
/// enough that reconnecting a few times starts Claude Code once.
pub(super) const FRESH_FOR: Duration = Duration::from_mins(2);

/// How often the limits are read for notifications: as often as the
/// window reads them for its own.
const READING_EVERY: Duration = Duration::from_mins(10);

/// How soon after phone access starts the limits are first read for
/// notifications: once the server has settled.
const FIRST_READING: Duration = Duration::from_secs(30);

/// The provider's name on a phone, which shows it as written.
const PROVIDER: &str = "Claude";

/// The plan limits phones are shown, and what was told of them.
#[derive(Debug)]
pub(super) struct Limits {
    /// The Claude Code asked; none where phones are shown no limits.
    program: Option<OsString>,
    /// The last reading, as phones read it.
    reading: watch::Sender<Value>,
    /// Where reading stands.
    state: Mutex<State>,
}

/// Where reading the limits stands.
#[derive(Debug, Default)]
struct State {
    /// When the last reading was started.
    asked: Option<Instant>,
    /// Whether one is under way.
    busy: bool,
    /// What was told of the limits so far.
    watch: Watch,
}

/// What Claude Code said of its plan.
enum Read {
    /// A plan's limits.
    Plan(Account, Vec<Limit>),
    /// No plan to show: no Claude Code, signed out, or billed per use.
    NoPlan,
    /// It couldn't say this time.
    Unknown,
}

impl Limits {
    /// The limits of the plan the Claude Code at `program` is signed in
    /// to, none read yet.
    pub(super) fn new(program: Option<OsString>) -> Self {
        Self {
            program,
            reading: watch::Sender::new(reading(None, 0)),
            state: Mutex::default(),
        }
    }

    /// The last reading, as a phone reads it.
    pub(super) fn current(&self) -> Value {
        self.reading.borrow().clone()
    }

    /// Tells of each newer reading.
    pub(super) fn subscribe(&self) -> watch::Receiver<Value> {
        self.reading.subscribe()
    }
}

/// Has the limits read again unless the last reading is younger than
/// `fresh_for`, or one is under way; phones hear of what it finds.
///
/// Must be called inside a Tokio runtime.
pub(super) fn refresh(hub: &Arc<Hub>, fresh_for: Duration) {
    let limits = &hub.limits;
    let Some(program) = limits.program.clone() else {
        return;
    };
    let mut state = lock(&limits.state);
    let fresh = state.asked.is_some_and(|at| at.elapsed() < fresh_for);
    if state.busy || fresh {
        return;
    }
    state.busy = true;
    state.asked = Some(Instant::now());
    drop(state);
    let hub = Arc::clone(hub);
    tokio::spawn(async move {
        let read = read(program).await;
        let now = now_secs();
        let alerts = {
            let mut state = lock(&hub.limits.state);
            state.busy = false;
            match &read {
                Read::Plan(_, limits) => state.watch.read(limits, now),
                Read::NoPlan | Read::Unknown => Vec::new(),
            }
        };
        match &read {
            Read::Plan(account, limits) => {
                hub.limits
                    .reading
                    .send_replace(reading(Some((account, limits)), now));
            }
            Read::NoPlan => {
                hub.limits.reading.send_replace(reading(None, now));
            }
            // The last reading stays.
            Read::Unknown => {}
        }
        for alert in alerts {
            tell(&hub, &alert, now).await;
        }
    });
}

/// Reads the limits now and then while a paired phone wants to hear of
/// them; runs until phone access ends.
pub(super) async fn watch(hub: Arc<Hub>) {
    if hub.limits.program.is_none() {
        return;
    }
    tokio::time::sleep(FIRST_READING).await;
    loop {
        let wanted = lock(&hub.devices)
            .list()
            .iter()
            .any(|d| d.push.as_ref().is_some_and(|p| p.limits));
        if wanted && lock(&hub.push).is_some() {
            // (A reading a phone just asked for counts.)
            refresh(&hub, READING_EVERY / 2);
        }
        tokio::time::sleep(READING_EVERY).await;
    }
}

/// Asks the Claude Code at `program` who it's signed in as, then for its
/// plan's limits.
async fn read(program: OsString) -> Read {
    let account = match claude::check(program.clone()).await {
        ClaudeCode::Installed {
            login: Login::Subscription(account),
            ..
        } => account,
        ClaudeCode::Installed { .. } | ClaudeCode::Missing => return Read::NoPlan,
        ClaudeCode::Failed(_) => return Read::Unknown,
    };
    match claude::usage(program).await {
        Ok(Some(limits)) => Read::Plan(account, limits),
        Ok(None) => Read::NoPlan,
        Err(_) => Read::Unknown,
    }
}

/// A reading taken at `now` (seconds since the Unix epoch) as a phone
/// reads it: this computer's accounts, and no other machines'.
fn reading(plan: Option<(&Account, &[Limit])>, now: i64) -> Value {
    let accounts: Vec<Value> = plan
        .into_iter()
        .filter(|(_, limits)| !limits.is_empty())
        .map(|(account, limits)| {
            json!({
                "provider": PROVIDER,
                "plan": account.plan.as_deref().map(capitalized).unwrap_or_default(),
                "account": account.email.clone().unwrap_or_default(),
                "windows": limits.iter().map(window).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "here": { "accounts": accounts, "updatedAt": now.saturating_mul(1000) },
            "hosts": {} })
}

/// One limit as a phone reads it; its label is the one the app knows a
/// limit's name by.
fn window(limit: &Limit) -> Value {
    let label = match (limit.kind.as_str(), limit.model.as_deref()) {
        ("session", _) => "Session (5 h)".to_owned(),
        ("weekly_all", _) => "Weekly".to_owned(),
        (_, Some(model)) if limit.group == "weekly" => format!("Weekly · {model}"),
        (_, Some(model)) => format!("{} · {model}", capitalized(&limit.group)),
        (_, None) => capitalized(&limit.group),
    };
    let used_up = limit.percent >= 100.0;
    json!({
        "label": label,
        "usedFraction": limit.percent / 100.0,
        "resetsAt": limit.resets_at.map_or(0, |at| at.saturating_mul(1000)),
        "status": if used_up { "exhausted" } else { "ok" },
    })
}

/// `max` as `Max`.
fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Sends the phones that want it a notification of `alert`, unless
/// notifications are held back or Harlo is on no relay.
async fn tell(hub: &Arc<Hub>, alert: &claude::Alert, now: i64) {
    let Some(base) = lock(&hub.push).clone() else {
        return;
    };
    if hub.notifications_paused() {
        return;
    }
    let (title, body) = alert.words(now);
    let (what, limit) = match alert {
        claude::Alert::UsedUp(limit) => (What::Limit, limit),
        claude::Alert::Reset(limit) => (What::Reset, limit),
    };
    let tag = match &limit.model {
        Some(model) => format!("claude:{}:{model}", limit.kind),
        None => format!("claude:{}", limit.kind),
    };
    let alert = Alert {
        what,
        key: tag,
        title,
        body,
        badge: hub.waiting(),
    };
    // A notification that doesn't arrive is only missed.
    let _ = push::deliver(hub, &base, &alert, false).await;
}

#[cfg(test)]
mod tests {
    use crate::claude::Severity;

    use super::*;

    fn limit(kind: &str, group: &str, model: Option<&str>, percent: f64) -> Limit {
        Limit {
            kind: kind.to_owned(),
            group: group.to_owned(),
            model: model.map(str::to_owned),
            percent,
            resets_at: Some(1_800_000_000),
            severity: Severity::Normal,
            active: false,
        }
    }

    #[test]
    fn a_reading_is_this_computers_account_with_a_window_for_each_limit() {
        let account = Account {
            email: Some("a@example.com".to_owned()),
            organization: None,
            plan: Some("max".to_owned()),
        };
        let limits = [
            limit("session", "session", None, 42.0),
            limit("weekly_all", "weekly", None, 100.0),
            limit("weekly_scoped", "weekly", Some("Opus"), 91.5),
        ];
        let read = reading(Some((&account, &limits)), 1_700_000_000);
        assert_eq!(read["hosts"], json!({}));
        assert_eq!(read["here"]["updatedAt"], 1_700_000_000_000_i64);
        let accounts = read["here"]["accounts"].as_array().expect("accounts");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["provider"], "Claude");
        assert_eq!(accounts[0]["plan"], "Max");
        assert_eq!(accounts[0]["account"], "a@example.com");
        assert_eq!(
            accounts[0]["windows"],
            json!([
                { "label": "Session (5 h)", "usedFraction": 0.42,
                  "resetsAt": 1_800_000_000_000_i64, "status": "ok" },
                { "label": "Weekly", "usedFraction": 1.0,
                  "resetsAt": 1_800_000_000_000_i64, "status": "exhausted" },
                { "label": "Weekly · Opus", "usedFraction": 0.915,
                  "resetsAt": 1_800_000_000_000_i64, "status": "ok" },
            ])
        );
    }

    #[test]
    fn no_plan_is_no_account() {
        let none = reading(None, 5);
        assert_eq!(none["here"]["accounts"], json!([]));
        let account = Account::default();
        let empty = reading(Some((&account, &[])), 5);
        assert_eq!(empty["here"]["accounts"], json!([]));
    }
}
