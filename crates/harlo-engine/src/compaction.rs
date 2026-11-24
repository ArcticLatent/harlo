//! Compaction: the model summarizes a session so far, and later steps
//! start from that summary instead of the whole history.
//!
//! A person asks for it, or a turn does by itself before a step once the
//! last step came near what the model's context holds, or after the
//! model's server refused a step for being too long.
//!
//! The request for a summary is the conversation as the steps sent it,
//! tools and all, with one message after it, so the provider reads it from
//! its cache instead of charging for the whole conversation again. Only a
//! conversation that outgrew the context is cut to fit first.

// Rust guideline compliant 2026-10-09

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::chat::{self, Block};
use crate::config::{ModelRef, ModelSpec};
use crate::message::{Created, ErrorInfo, Message, Part, Tokens, ToolState};
use crate::turn::{Event, Sink, Turn, now};

/// The context size, in tokens, a session is summarized at when the
/// configuration names none.
///
/// A step reads the whole context again, so a session left to fill a
/// million-token window pays for most of a million tokens every step, and
/// several providers charge more per token above this size. Models with a
/// smaller context are summarized earlier, by [`threshold`].
pub const CAP: u64 = 200_000;

/// The reply a request leaves room for when the model's own limit isn't
/// known, in tokens.
const REPLY: u64 = 8_192;

/// The most a step is expected to add to the context, in tokens: a long
/// file read or a command's output. A step that adds more is caught when
/// the model's server refuses the next one.
const STEP: u64 = 20_000;

/// Characters per token when sizing text without the model's tokenizer.
/// Code and JSON run near three; prose nearer four. Three errs towards
/// cutting more, which is the safe side when fitting a context.
const CHARS_PER_TOKEN: u64 = 3;

/// The characters kept from the start and from the end of a tool result
/// that is cut to make the conversation fit.
const KEPT: (usize, usize) = (1_500, 500);

/// The context size, in tokens, from which a session is summarized
/// before its next step.
///
/// It leaves room in a `context` of that many tokens for the reply (up to
/// `output`) and for what a step adds, and is never above `cap`, or
/// [`CAP`] when none is given. An unknown `context` is taken to hold the
/// cap.
#[must_use]
pub fn threshold(context: Option<u64>, output: Option<u64>, cap: Option<u64>) -> u64 {
    let cap = cap.unwrap_or(CAP);
    let Some(context) = context else {
        return cap;
    };
    room(context, output)
        .saturating_sub(STEP.min(context / 10))
        .min(cap)
}

/// What fits in a `context` beside the reply, in tokens.
fn room(context: u64, output: Option<u64>) -> u64 {
    // (Some models list a reply as long as their context: at most half.)
    context - output.unwrap_or(REPLY).min(context / 2)
}

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
    the work. It sees only your summary and what comes after it. Call no tools: answer with \
    the summary alone.\n\n\
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

/// The tokens the history's last step used of the context, if it has one.
fn last_used(history: &[Message]) -> Option<u64> {
    since_summary(history).iter().rev().find_map(|m| match m {
        Message::Assistant(step) => {
            let t = &step.tokens;
            Some(t.input + t.cache.read + t.cache.write + t.output)
        }
        _ => None,
    })
}

/// About how many tokens `message` takes in a request.
fn size(message: &Message) -> u64 {
    let text = serde_json::to_string(message).unwrap_or_default();
    u64::try_from(text.len()).unwrap_or(u64::MAX) / CHARS_PER_TOKEN
}

/// `messages` cut to about `room` tokens: the long tool results are cut
/// short, oldest first, and if that isn't enough the oldest messages go.
fn fit(mut messages: Vec<Message>, room: u64) -> Vec<Message> {
    let mut total: u64 = messages.iter().map(size).sum();
    for message in &mut messages {
        if total <= room {
            return messages;
        }
        let before = size(message);
        if let Message::Assistant(step) = message {
            step.content.iter_mut().for_each(cut_result);
        }
        total = total.saturating_sub(before.saturating_sub(size(message)));
    }
    // The newest two stay whatever their size: the request and its answer.
    let mut from = 0;
    while total > room && from + 2 < messages.len() {
        total = total.saturating_sub(size(&messages[from]));
        from += 1;
    }
    // (A conversation doesn't open with the model's own step.)
    while from + 1 < messages.len() && matches!(messages[from], Message::Assistant(_)) {
        from += 1;
    }
    messages.split_off(from)
}

/// Cuts a tool result's long texts down to their start and end.
fn cut_result(part: &mut Part) {
    let Part::Tool {
        state: ToolState::Completed { content, .. },
        ..
    } = part
    else {
        return;
    };
    for block in content {
        let Some(Value::String(text)) = block.get_mut("text") else {
            continue;
        };
        let count = text.chars().count();
        if count <= KEPT.0 + KEPT.1 {
            continue;
        }
        let start: String = text.chars().take(KEPT.0).collect();
        let end: String = text.chars().skip(count - KEPT.1).collect();
        *text = format!(
            "{start}\n… ({} characters left out to fit the summary) …\n{end}",
            count - KEPT.0 - KEPT.1
        );
    }
}

/// Writes `pending`'s summary of `turn`'s conversation with its model,
/// saying so through `sink`; returns it completed, or failed with why.
pub async fn compact(
    pending: Compaction,
    turn: &Turn,
    sink: &impl Sink,
    stop: &CancellationToken,
) -> Compaction {
    let tools = crate::turn::offered(turn);
    compact_with(pending, turn, &tools, false, sink, stop).await
}

/// [`compact`], with the `tools` the turn's steps are offered, and whether
/// the conversation `overflowed`: the model's server refused it as too long.
pub(crate) async fn compact_with(
    pending: Compaction,
    turn: &Turn,
    tools: &[Value],
    overflowed: bool,
    sink: &impl Sink,
    stop: &CancellationToken,
) -> Compaction {
    let (provider, model) = (&turn.provider, &turn.model);
    let mut done = pending;
    sink.emit(Event::CompactionStarted {
        id: done.id.clone(),
        reason: done.reason.clone(),
    });
    let spec: Option<&ModelSpec> = provider.models.iter().find(|m| m.id == model.model);
    let mut asked = since_summary(&turn.history).to_vec();
    // What the request may hold, when it is known to hold too much.
    let used = last_used(&turn.history);
    let holds = spec
        .and_then(|m| m.context.map(|context| room(context, m.output)))
        .map(|room| room.saturating_sub(STEP.min(room / 10)));
    let cut_to = match (holds, overflowed) {
        (Some(holds), _) if overflowed || used.is_some_and(|u| u >= holds) => Some(holds),
        // Refused, by a model whose context isn't known: half of it.
        (None, true) => Some(asked.iter().map(size).sum::<u64>() / 2),
        _ => None,
    };
    if let Some(room) = cut_to {
        asked = fit(asked, room);
    }
    asked.push(Message::User {
        id: String::new(),
        time: Created { created: now() },
        text: SUMMARIZE.to_owned(),
        skills: Vec::new(),
        files: Vec::new(),
    });
    let (to, body) = chat::prepare(provider, model, &turn.system, &asked, tools);
    done.model = Some(model.clone());
    match chat::stream(&to, &body, stop, &mut |_| {}).await {
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

    /// A step whose one tool call returned `text`.
    fn read_step(text: &str) -> Message {
        let Message::Assistant(mut made) = step(10) else {
            unreachable!("a step");
        };
        made.content = vec![Part::Tool {
            id: "call".into(),
            name: "read".into(),
            state: ToolState::Completed {
                input: serde_json::Map::new(),
                content: vec![serde_json::json!({ "type": "text", "text": text })],
                metadata: serde_json::Map::new(),
            },
            time: crate::message::ToolTime::default(),
        }];
        Message::Assistant(made)
    }

    fn prompt(text: &str) -> Message {
        Message::User {
            id: "u".into(),
            time: Created { created: 0 },
            text: text.into(),
            skills: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn a_session_is_summarized_with_room_left_and_never_past_the_cap() {
        // Room for the reply and for what a step adds.
        assert_eq!(threshold(Some(200_000), Some(64_000), None), 116_000);
        assert_eq!(threshold(Some(128_000), Some(8_192), None), 107_008);
        assert_eq!(threshold(Some(32_000), None, None), 20_608);
        // A large context is summarized at the cap, the configured one if any.
        assert_eq!(threshold(Some(1_000_000), Some(128_000), None), CAP);
        assert_eq!(
            threshold(Some(1_000_000), Some(128_000), Some(500_000)),
            500_000
        );
        assert_eq!(threshold(Some(200_000), Some(64_000), Some(50_000)), 50_000);
        // An unknown context, and a reply listed as long as the context.
        assert_eq!(threshold(None, None, None), CAP);
        assert_eq!(threshold(Some(100_000), Some(100_000), None), 40_000);
    }

    #[test]
    fn a_conversation_too_long_to_summarize_is_cut_to_fit() {
        let long = "x".repeat(30_000);
        let history = vec![
            prompt("first"),
            read_step(&long),
            prompt("second"),
            read_step(&long),
            prompt("third"),
        ];
        let whole: u64 = history.iter().map(size).sum();
        assert_eq!(
            fit(history.clone(), whole),
            history,
            "what fits is left alone"
        );

        // The older result is cut first; the newer one stays whole.
        let cut = fit(history.clone(), whole - 5_000);
        assert_eq!(cut.len(), history.len());
        assert!(size(&cut[1]) < 1_500, "{}", size(&cut[1]));
        assert_eq!(cut[3], history[3]);
        let Message::Assistant(made) = &cut[1] else {
            unreachable!("a step");
        };
        let kept = serde_json::to_string(&made.content).expect("json");
        assert!(
            kept.contains("characters left out to fit the summary"),
            "{kept}"
        );

        // Where cutting results isn't enough, the oldest messages go, and
        // what is left starts with a prompt.
        let least = fit(history.clone(), 10);
        assert_eq!(least, [prompt("third")]);
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
