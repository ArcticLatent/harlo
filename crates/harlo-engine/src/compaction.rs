//! Compaction: the model summarizes a session so far, and later steps
//! start from that summary instead of the whole history.
//!
//! A person asks for it, or a turn does by itself before a step once the
//! last step came near the model's context limit.

// Rust guideline compliant 2026-10-09

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::chat::{self, Block, Endpoint};
use crate::config::{ModelRef, ProviderSpec};
use crate::message::{Created, ErrorInfo, Message, Tokens};
use crate::turn::{Event, Sink, now};

/// A session's summary, kept as one of its messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Compaction {
    /// Its ID; it sorts in the session's order.
    pub id: String,
    /// When it was asked for.
    pub time: Created,
    /// `pending`, `completed` or `failed`.
    pub status: String,
    /// `manual` when a person asked, `auto` when the context was full.
    pub reason: String,
    /// The model that wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    /// The summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Why it failed, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    /// What writing it cost in US dollars.
    #[serde(default)]
    pub cost: f64,
    /// The tokens writing it used.
    #[serde(default)]
    pub tokens: Tokens,
}

impl Compaction {
    /// A compaction asked for now, not yet written.
    #[must_use]
    pub fn pending(id: String, reason: &str) -> Self {
        Self {
            id,
            time: Created { created: now() },
            status: "pending".to_owned(),
            reason: reason.to_owned(),
            model: None,
            summary: None,
            error: None,
            cost: 0.0,
            tokens: Tokens::default(),
        }
    }
}

/// The request for a summary, put after the history.
pub const SUMMARIZE: &str = "Summarize the conversation so far for the agent that carries on \
    the work. It sees only your summary and what comes after it.\n\n\
    Cover what the person asked for and decided, what was done, what is in progress or blocked, \
    the next step, and the files that matter. Leave out the instructions and setup the agent was \
    given, such as instruction files and environment details: it gets those again.\n\n\
    Answer in this form, leaving out sections with nothing in them:\n\n\
    ## Goal\n- …\n\n## Decisions\n- …\n\n## Done\n- …\n\n## In progress\n- …\n\n\
    ## Next step\n1. …\n\n## Files\n- …";

/// What the model is told after a summary written in the middle of a turn.
pub const CARRY_ON: &str = "<system-update>\nThe conversation was summarized above because it \
    had grown too long. Carry on with the work where it stopped, without asking the person to \
    repeat anything.\n</system-update>";

/// What starts the summary when later steps read it.
pub const SUMMARY_HEADING: &str = "Summary of the conversation so far:\n\n";

/// The messages a step reads: from the last completed summary on, or all
/// of them when there's none.
#[must_use]
pub fn since_summary(history: &[Message]) -> &[Message] {
    let last = history.iter().rposition(
        |m| matches!(m, Message::Compaction(c) if c.status == "completed" && c.summary.is_some()),
    );
    last.map_or(history, |at| &history[at..])
}

/// Whether the history's last step used at least `limit` tokens of
/// context, so the next one should start from a summary.
#[must_use]
pub fn is_full(history: &[Message], limit: u64) -> bool {
    since_summary(history)
        .iter()
        .rev()
        .find_map(|m| match m {
            Message::Assistant(step) => Some(&step.tokens),
            _ => None,
        })
        .is_some_and(|t| t.input + t.cache.read + t.cache.write + t.output >= limit)
}

/// Writes `pending`'s summary of `history` with `model`, saying so through
/// `sink`; returns it completed, or failed with why.
pub async fn compact(
    pending: Compaction,
    provider: &ProviderSpec,
    model: &ModelRef,
    system: &str,
    history: &[Message],
    sink: &impl Sink,
    stop: &CancellationToken,
) -> Compaction {
    let mut done = pending;
    sink.emit(Event::CompactionStarted {
        id: done.id.clone(),
        reason: done.reason.clone(),
    });
    let mut asked = since_summary(history).to_vec();
    asked.push(Message::User {
        id: String::new(),
        time: Created { created: now() },
        text: SUMMARIZE.to_owned(),
        skills: Vec::new(),
        files: Vec::new(),
    });
    let spec = provider.models.iter().find(|m| m.id == model.model);
    let (extra, headers) = spec
        .map(|m| m.request_extras(model.variant.as_deref()))
        .unwrap_or_default();
    let body = chat::request(&model.model, system, &asked, &[], &extra);
    let to = Endpoint {
        base_url: &provider.base_url,
        api_key: provider.api_key.as_deref(),
        headers: &headers,
    };
    done.model = Some(model.clone());
    match chat::stream(to, &body, stop, &mut |_| {}).await {
        Ok(reply) => {
            let summary: String = reply
                .blocks
                .into_iter()
                .filter_map(|b| match b {
                    Block::Text(text) => Some(text),
                    _ => None,
                })
                .collect();
            done.cost = spec.map_or(0.0, |m| m.cost(&reply.tokens));
            done.tokens = reply.tokens;
            if summary.trim().is_empty() {
                "failed".clone_into(&mut done.status);
                done.error = Some(ErrorInfo {
                    kind: "compaction".to_owned(),
                    message: "The model wrote no summary.".to_owned(),
                });
            } else {
                "completed".clone_into(&mut done.status);
                done.summary = Some(summary);
            }
        }
        Err(e) => {
            "failed".clone_into(&mut done.status);
            done.error = Some(ErrorInfo {
                kind: "compaction".to_owned(),
                message: e.to_string(),
            });
        }
    }
    sink.emit(Event::CompactionEnded {
        compaction: Box::new(done.clone()),
    });
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Assistant, Cache, StepTime};

    fn step(tokens: u64) -> Message {
        Message::Assistant(Box::new(Assistant {
            id: "a".into(),
            time: StepTime::default(),
            agent: "build".into(),
            model: ModelRef {
                provider: "p".into(),
                model: "m".into(),
                variant: None,
            },
            content: Vec::new(),
            finish: None,
            raw_finish: None,
            cost: 0.0,
            tokens: Tokens {
                input: tokens,
                output: 0,
                reasoning: 0,
                cache: Cache::default(),
            },
            error: None,
            snapshot: None,
        }))
    }

    #[test]
    fn later_steps_read_from_the_last_summary() {
        let mut done = Compaction::pending("c".into(), "manual");
        done.status = "completed".into();
        done.summary = Some("## Goal\n- x".into());
        let history = vec![step(900), Message::Compaction(Box::new(done)), step(10)];
        assert_eq!(since_summary(&history).len(), 2);
        assert!(
            !is_full(&history, 500),
            "only steps after the summary count"
        );
        assert!(is_full(&history[..1], 500));
    }
}
