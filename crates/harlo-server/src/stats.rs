//! Usage stats over the stored sessions: sessions, prompts, steps, tokens,
//! cost and tool calls since a start, by day and by model.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;

use harlo_engine::{Message, Part, Tokens, ToolState, now};
use serde_json::{Value, json};

use crate::server::{Refusal, Server};

/// Adds `more` to `sum`.
fn add(sum: &mut Tokens, more: &Tokens) {
    sum.input += more.input;
    sum.output += more.output;
    sum.reasoning += more.reasoning;
    sum.cache.read += more.cache.read;
    sum.cache.write += more.cache.write;
}

/// The local calendar day of Unix milliseconds `at`.
fn day(at: i64) -> String {
    jiff::Timestamp::from_millisecond(at).map_or_else(
        |_| String::new(),
        |t| {
            let zone = jiff::tz::TimeZone::try_system().unwrap_or(jiff::tz::TimeZone::UTC);
            t.to_zoned_datetime(zone).date().to_string()
        },
    )
}

/// How many days in a row, up to the last one in `days`, had steps.
fn streak(days: &BTreeMap<String, u64>) -> u64 {
    let mut dates = days
        .keys()
        .filter_map(|d| d.parse::<jiff::civil::Date>().ok())
        .rev();
    let Some(mut last) = dates.next() else {
        return 0;
    };
    let mut run = 1;
    for date in dates {
        if last.yesterday().ok() == Some(date) {
            run += 1;
            last = date;
        } else {
            break;
        }
    }
    run
}

/// Tool calls by how they ended.
#[derive(Default)]
struct Calls {
    all: u64,
    succeeded: u64,
    failed: u64,
    unfinished: u64,
}

impl Server {
    /// Usage since `from` (Unix milliseconds), or since the first session.
    ///
    /// # Errors
    ///
    /// When the store fails.
    pub(crate) fn stats(&self, from: Option<i64>) -> Result<Value, Refusal> {
        let sessions = self.store.sessions()?;
        let start = from.unwrap_or_else(|| {
            sessions
                .iter()
                .map(|s| s.time.created)
                .min()
                .unwrap_or_else(now)
        });
        let (mut roots, mut subagents, mut prompts, mut steps) = (0, 0, 0, 0);
        let (mut tokens, mut cost) = (Tokens::default(), 0.0);
        let mut calls = Calls::default();
        let mut days: BTreeMap<String, u64> = BTreeMap::new();
        let mut models: BTreeMap<(String, String), (u64, Tokens, f64)> = BTreeMap::new();
        for session in sessions.iter().filter(|s| s.time.created >= start) {
            if session.parent_id.is_some() {
                subagents += 1;
            } else {
                roots += 1;
            }
            for message in self.store.messages(&session.id)? {
                match message {
                    Message::User { .. } => prompts += 1,
                    Message::Assistant(step) => {
                        steps += 1;
                        add(&mut tokens, &step.tokens);
                        cost += step.cost;
                        *days.entry(day(step.time.created)).or_default() += 1;
                        let model = models
                            .entry((step.model.provider.clone(), step.model.model.clone()))
                            .or_default();
                        model.0 += 1;
                        add(&mut model.1, &step.tokens);
                        model.2 += step.cost;
                        for part in &step.content {
                            if let Part::Tool { state, .. } = part {
                                calls.all += 1;
                                match state {
                                    ToolState::Completed { .. } => calls.succeeded += 1,
                                    ToolState::Error { .. } => calls.failed += 1,
                                    ToolState::Running { .. } => calls.unfinished += 1,
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(json!({
            "range": { "from": start, "to": now() },
            "sessions": roots, "subagents": subagents, "prompts": prompts, "steps": steps,
            "tokens": tokens, "cost": cost,
            "tools": { "mode": "summary", "totals": {
                "calls": calls.all, "succeeded": calls.succeeded,
                "failed": calls.failed, "unfinished": calls.unfinished,
            } },
            "activeDays": days.len(), "streak": streak(&days),
            "activity": days.iter().map(|(date, steps)| json!({ "date": date, "steps": steps })).collect::<Vec<_>>(),
            "models": models.iter().map(|((provider, id), (steps, tokens, cost))| json!({
                "model": { "id": id, "providerID": provider },
                "steps": steps, "tokens": tokens, "cost": cost,
            })).collect::<Vec<_>>(),
        }))
    }
}
