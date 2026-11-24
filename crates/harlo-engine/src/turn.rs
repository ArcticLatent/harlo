//! The loop: send the history to the model, run the tools it calls, and
//! go on until it answers without calling any.
//!
//! Each model step becomes one assistant message. Before a tool runs, the
//! permission rules decide; when they say ask, the [`Gate`] waits for a
//! person. Everything that happens is reported to the [`Sink`] as it
//! happens, so a server can store it and pass it on.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::chat::{self, Block, Endpoint, Piece};
use crate::config::{ModelRef, ProviderSpec};
use crate::message::{
    Assistant, Created, ErrorInfo, Message, Part, Snapshot, StepTime, ToolState, ToolTime,
};
use crate::question::Asked;
use crate::rules::{Effect, Rule, decide};
use crate::tools::{self, Plan};
use crate::{Error, id};

/// The most model steps one turn takes before it stops itself.
const MAX_STEPS: usize = 200;

/// Everything a turn needs.
#[derive(Debug, Clone)]
pub struct Turn {
    /// The project folder tools run in.
    pub cwd: PathBuf,
    /// The agent's name, such as `build`.
    pub agent: String,
    /// The model to ask.
    pub model: ModelRef,
    /// Where that model is served.
    pub provider: ProviderSpec,
    /// The system prompt.
    pub system: String,
    /// Permission rules in named layers, applied in order.
    pub rules: Vec<(String, Vec<Rule>)>,
    /// The session so far, ending with the message being answered.
    pub history: Vec<Message>,
    /// Records the project's files around each step that runs tools, so
    /// the step can be undone; none, and steps aren't recorded.
    pub shadow: Option<crate::snapshot::Shadow>,
    /// Where notes are kept and earlier sessions searched; none, and the
    /// model isn't offered `remember` or `search_sessions`.
    pub memory: Option<std::sync::Arc<dyn crate::memory::Memory>>,
    /// Where messages that steer the turn come from; none, and nothing
    /// joins it once it started.
    pub steering: Option<std::sync::Arc<dyn Steering>>,
    /// What the session knows of the files it read and wrote; none, and
    /// edits aren't checked for changes made since.
    pub files: Option<std::sync::Arc<dyn crate::stale::Ledger>>,
    /// Where subagents start; none offers no subagent tool.
    pub subagents: Option<std::sync::Arc<dyn crate::subagent::Subagents>>,
    /// The agents a subagent may run as: `(id, what for)`.
    pub subagent_agents: Vec<(String, String)>,
    /// Whether the model keeps a todo list (`experimental.todo`).
    pub todo: bool,
    /// The skills the model may load, and people may have attached.
    pub skills: Vec<crate::skills::Skill>,
    /// The context size, in tokens, from which the next step starts from a
    /// summary; none never compacts by itself.
    pub compact_at: Option<u64>,
}

/// Something that happened during a turn.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The model's server failed before replying; the step asks again at
    /// `at`.
    RetryScheduled {
        /// The step's message ID.
        message: String,
        /// The attempt that comes next, from 2.
        attempt: u32,
        /// When, in Unix milliseconds.
        at: i64,
        /// Why the last attempt failed.
        error: String,
        /// The HTTP status it failed with, if any.
        status: Option<u16>,
    },
    /// The engine added a message of its own to the history, such as the
    /// instructions of a folder the agent read in.
    Noted {
        /// The message.
        message: Box<Message>,
    },
    /// The session began summarizing itself.
    CompactionStarted {
        /// The compaction's message ID.
        id: String,
        /// `manual` or `auto`.
        reason: String,
    },
    /// The summary is written, or failed.
    CompactionEnded {
        /// The compaction, completed or failed.
        compaction: Box<crate::compaction::Compaction>,
    },
    /// A model step began; its assistant message has ID `message`.
    StepStarted {
        /// The step's message ID.
        message: String,
        /// The agent.
        agent: String,
        /// The model.
        model: ModelRef,
        /// When, in Unix milliseconds.
        time: i64,
    },
    /// The model's first token came.
    FirstToken {
        /// The step's message ID.
        message: String,
    },
    /// A text block began at `at` in the step's content.
    TextStarted {
        /// The step's message ID.
        message: String,
        /// Its place in the step's content.
        at: usize,
    },
    /// More text.
    TextDelta {
        /// The step's message ID.
        message: String,
        /// Its place in the step's content.
        at: usize,
        /// The new text.
        delta: String,
    },
    /// A text block is complete.
    TextEnded {
        /// The step's message ID.
        message: String,
        /// Its place in the step's content.
        at: usize,
        /// All of it.
        text: String,
    },
    /// A reasoning block began.
    ReasoningStarted {
        /// The step's message ID.
        message: String,
        /// Its place in the step's content.
        at: usize,
    },
    /// More reasoning.
    ReasoningDelta {
        /// The step's message ID.
        message: String,
        /// Its place in the step's content.
        at: usize,
        /// The new text.
        delta: String,
    },
    /// A reasoning block is complete.
    ReasoningEnded {
        /// The step's message ID.
        message: String,
        /// Its place in the step's content.
        at: usize,
        /// All of it.
        text: String,
    },
    /// The model began writing a tool call.
    ToolInputStarted {
        /// The step's message ID.
        message: String,
        /// The call's ID.
        call: String,
        /// The tool.
        name: String,
    },
    /// The call's arguments are written.
    ToolInputEnded {
        /// The step's message ID.
        message: String,
        /// The call's ID.
        call: String,
        /// The arguments as the model wrote them.
        text: String,
    },
    /// The model's reply is complete; its calls are about to be checked.
    StepStreamed {
        /// The step as it stands, its calls still running.
        step: Box<Assistant>,
    },
    /// A call is checked and waits for permission or runs.
    ToolCalled {
        /// The step's message ID.
        message: String,
        /// The call's ID.
        call: String,
        /// The tool.
        name: String,
        /// Its arguments.
        input: Map<String, Value>,
    },
    /// A call ended: done, failed or refused.
    ToolEnded {
        /// The step's message ID.
        message: String,
        /// The call's ID.
        call: String,
        /// How it ended, with its result.
        state: ToolState,
    },
    /// A step is complete, with everything in it.
    StepEnded {
        /// The step's message.
        step: Box<Assistant>,
    },
}

/// Where a turn reports what happens.
pub trait Sink: Send + Sync {
    /// Takes one event; called in order.
    fn emit(&self, event: Event);
}

/// A request for a person's permission.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// Its ID, such as `per_…`.
    pub id: String,
    /// The step's message ID.
    pub message: String,
    /// The tool call's ID.
    pub call: String,
    /// The action, such as `edit`.
    pub action: String,
    /// What it acts on.
    pub resources: Vec<String>,
    /// What "always" would allow from now on.
    pub save: Vec<String>,
    /// What to show while asking, such as the diff.
    pub metadata: Map<String, Value>,
    /// Why it asks: the rule that decided, for a person.
    pub why: String,
}

/// A person's answer to an [`Ask`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Run this call.
    Once,
    /// Run it, and allow the same from now on in this session.
    Always,
    /// Don't run it. With a message the model reads it and goes on;
    /// without one the turn stops.
    Reject(Option<String>),
}

/// Where a turn picks up what people sent while it ran, to steer it.
pub trait Steering: Send + Sync + std::fmt::Debug {
    /// The messages that arrived to steer the turn, now delivered; none
    /// when nothing came.
    fn take(&self) -> Vec<Message>;
}

/// Where a turn asks a person: for permission, and the agent's questions.
pub trait Gate: Send + Sync {
    /// Waits for a person's answer to `ask`.
    fn ask(&self, ask: Ask) -> impl Future<Output = Answer> + Send;

    /// Waits for a person's answers to the agent's questions: one list per
    /// question, or `None` when they dismissed them.
    fn question(&self, asked: Asked) -> impl Future<Output = Option<Vec<Vec<String>>>> + Send;
}

/// How a turn ended.
#[derive(Debug)]
pub enum Outcome {
    /// The model answered.
    Succeeded,
    /// It was stopped, or a person refused a call without saying why.
    Interrupted,
    /// Something failed.
    Failed(Error),
}

/// The time now, in Unix milliseconds.
#[must_use]
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Runs a turn until the model answers, it fails, or `stop` is cancelled.
pub async fn run(
    mut turn: Turn,
    sink: &impl Sink,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Outcome {
    let mut definitions = tools::definitions();
    definitions.push(crate::question::definition());
    // Skills a rule denies aren't offered.
    let offered: Vec<&crate::skills::Skill> = turn
        .skills
        .iter()
        .filter(|s| s.offered())
        .filter(|s| {
            let layers: Vec<(&str, &[Rule])> = turn
                .rules
                .iter()
                .map(|(n, r)| (n.as_str(), r.as_slice()))
                .collect();
            decide(&layers, "skill", &s.id).effect != Effect::Deny
        })
        .collect();
    if !offered.is_empty() {
        definitions.push(crate::skills::definition(&offered));
    }
    if turn.memory.is_some() {
        definitions.extend(crate::memory::definitions());
    }
    if turn.todo {
        definitions.push(crate::todo::definition());
    }
    if turn.subagents.is_some() {
        let layers: Vec<(&str, &[Rule])> = turn
            .rules
            .iter()
            .map(|(n, r)| (n.as_str(), r.as_slice()))
            .collect();
        let allowed: Vec<(String, String)> = turn
            .subagent_agents
            .iter()
            .filter(|(id, _)| decide(&layers, "subagent", id).effect != Effect::Deny)
            .cloned()
            .collect();
        if !allowed.is_empty() {
            definitions.push(crate::subagent::definition(&allowed));
        }
    }
    let mut compacted = false;
    for _ in 0..MAX_STEPS {
        if stop.is_cancelled() {
            return Outcome::Interrupted;
        }
        // Between steps is a safe point: what steers the turn joins it here.
        if let Some(steering) = &turn.steering {
            turn.history.extend(steering.take());
        }
        if !compacted
            && turn
                .compact_at
                .is_some_and(|limit| crate::compaction::is_full(&turn.history, limit))
        {
            // Tried once a turn: a failed summary isn't asked for again each step.
            compacted = true;
            let pending = crate::compaction::Compaction::pending(id::ascending_id("msg"), "auto");
            let done = crate::compaction::compact(
                pending,
                &turn.provider,
                &turn.model,
                &turn.system,
                &turn.history,
                sink,
                stop,
            )
            .await;
            let written = done.status == "completed";
            turn.history.push(Message::Compaction(Box::new(done)));
            if written {
                turn.history.push(Message::Synthetic {
                    id: id::ascending_id("msg"),
                    time: Created { created: now() },
                    text: crate::compaction::CARRY_ON.to_owned(),
                    description: None,
                    metadata: None,
                });
            }
        }
        let (assistant, go_on) = match step(&mut turn, &definitions, sink, gate, stop).await {
            Ok(done) => done,
            Err(e) if e.is_stopped() => return Outcome::Interrupted,
            Err(e) => return Outcome::Failed(e),
        };
        let found = instructions_met(&turn, &assistant);
        turn.history.push(Message::Assistant(Box::new(assistant)));
        note_instructions(&mut turn, found, sink);
        match go_on {
            Next::Model => {}
            Next::Done => return Outcome::Succeeded,
            Next::Halt => return Outcome::Interrupted,
        }
    }
    Outcome::Failed(Error::provider(format!(
        "Stopped after {MAX_STEPS} model steps without an answer."
    )))
}

/// Adds the instruction files `found` to the conversation, telling `sink`.
fn note_instructions(turn: &mut Turn, found: Vec<(PathBuf, String)>, sink: &impl Sink) {
    for (path, text) in found {
        let message = Message::Synthetic {
            id: id::ascending_id("msg"),
            time: Created { created: now() },
            text: format!("Instructions from: {}\n{}", path.display(), text.trim_end()),
            description: None,
            metadata: Some(json!({ "instruction": { "paths": [path] } })),
        };
        sink.emit(Event::Noted {
            message: Box::new(message.clone()),
        });
        turn.history.push(message);
    }
}

/// The instruction files of the folders that `step`'s reads went into,
/// not yet in the conversation.
fn instructions_met(turn: &Turn, step: &Assistant) -> Vec<(PathBuf, String)> {
    let mut known: Vec<PathBuf> = turn
        .history
        .iter()
        .filter_map(|m| match m {
            Message::Synthetic {
                metadata: Some(meta),
                ..
            } => meta["instruction"]["paths"].as_array().cloned(),
            _ => None,
        })
        .flatten()
        .filter_map(|p| p.as_str().map(PathBuf::from))
        .collect();
    let mut found = Vec::new();
    for part in &step.content {
        let Part::Tool {
            name,
            state: ToolState::Completed { input, .. },
            ..
        } = part
        else {
            continue;
        };
        let Some(path) = input
            .get("path")
            .and_then(Value::as_str)
            .filter(|_| name == "read")
        else {
            continue;
        };
        let file = if std::path::Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            turn.cwd.join(path)
        };
        for (path, text) in crate::instructions::on_the_way(&turn.cwd, &file, &known) {
            known.push(path.clone());
            found.push((path, text));
        }
    }
    found
}

/// What follows a step.
enum Next {
    /// Its tool results go back to the model.
    Model,
    /// The model answered.
    Done,
    /// A person refused a call; the turn ends.
    Halt,
}

/// One model step and its tool calls. A failed or stopped step is still
/// reported, with its error, before the error returns.
async fn step(
    turn: &mut Turn,
    definitions: &[Value],
    sink: &impl Sink,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Result<(Assistant, Next), Error> {
    let (mut assistant, calls) = ask_model(turn, definitions, sink, stop).await?;
    // The files as the calls find them, and as they leave them.
    let start = match (&turn.shadow, calls.is_empty()) {
        (Some(shadow), false) => track(shadow).await,
        _ => None,
    };
    let next = run_calls(turn, &mut assistant, calls, sink, gate, stop).await;
    if let (Some(shadow), Some(start)) = (&turn.shadow, start) {
        let end = track(shadow).await;
        if let Some(snapshot) = assistant.snapshot.as_mut() {
            snapshot.start = Some(start);
            snapshot.end = end;
        }
    }
    sink.emit(Event::StepEnded {
        step: Box::new(assistant.clone()),
    });
    if stop.is_cancelled() {
        return Err(Error::stopped());
    }
    Ok((assistant, next))
}

/// Records the project's files; `None`, logged, when that fails, and the
/// step simply can't be undone.
async fn track(shadow: &crate::snapshot::Shadow) -> Option<String> {
    match shadow.track().await {
        Ok(id) => Some(id),
        Err(why) => {
            tracing::warn!(name: "snapshot.failed", why, "couldn't record the project's files");
            None
        }
    }
}

/// How many times a step asks the model before its failure stands.
const ATTEMPTS: u32 = 8;
/// The longest wait between attempts.
const MAX_WAIT: Duration = Duration::from_secs(30);

/// The wait before `attempt`: two seconds before the second, doubling
/// after.
fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(2_u64.saturating_mul(1 << attempt.saturating_sub(2).min(5)))
}

/// Streams the reply to `body` for step `message`, asking again after a
/// failure that may pass; notes when the first token came.
async fn stream_retrying(
    endpoint: Endpoint<'_>,
    body: &Value,
    message: &str,
    sink: &impl Sink,
    stop: &CancellationToken,
    first_token: &mut Option<i64>,
) -> Result<chat::Reply, Error> {
    let mut attempt = 1;
    loop {
        let got = {
            let mut on = |piece: Piece| {
                if piece == Piece::FirstToken {
                    *first_token = Some(now());
                }
                sink.emit(translate(message, piece));
            };
            chat::stream(endpoint, body, stop, &mut on).await
        };
        // Only a request that failed before any of the reply came is asked
        // again, so nothing shown is shown twice.
        match got {
            Err(e) if e.is_retryable() && first_token.is_none() && attempt < ATTEMPTS => {
                attempt += 1;
                let wait = e.wait().unwrap_or_else(|| backoff(attempt)).min(MAX_WAIT);
                sink.emit(Event::RetryScheduled {
                    message: message.to_owned(),
                    attempt,
                    at: now() + i64::try_from(wait.as_millis()).unwrap_or(i64::MAX),
                    error: e.to_string(),
                    status: e.status(),
                });
                tokio::select! {
                    () = stop.cancelled() => return Err(Error::stopped()),
                    () = tokio::time::sleep(wait) => {}
                }
            }
            other => return other,
        }
    }
}

/// Streams the model's reply into a new step; returns it with the places
/// of its tool calls in its content.
async fn ask_model(
    turn: &Turn,
    definitions: &[Value],
    sink: &impl Sink,
    stop: &CancellationToken,
) -> Result<(Assistant, Vec<usize>), Error> {
    let message = id::ascending_id("msg");
    let created = now();
    sink.emit(Event::StepStarted {
        message: message.clone(),
        agent: turn.agent.clone(),
        model: turn.model.clone(),
        time: created,
    });
    let mut assistant = Assistant {
        id: message.clone(),
        time: StepTime {
            created,
            ..StepTime::default()
        },
        agent: turn.agent.clone(),
        model: turn.model.clone(),
        content: Vec::new(),
        finish: None,
        raw_finish: None,
        cost: 0.0,
        tokens: crate::message::Tokens::default(),
        error: None,
        snapshot: None,
    };
    let (extra, headers) = turn
        .provider
        .models
        .iter()
        .find(|m| m.id == turn.model.model)
        .map(|m| m.request_extras(turn.model.variant.as_deref()))
        .unwrap_or_default();
    let body = chat::request(
        &turn.model.model,
        &turn.system,
        &turn.history,
        definitions,
        &extra,
    );
    let endpoint = Endpoint {
        base_url: &turn.provider.base_url,
        api_key: turn.provider.api_key.as_deref(),
        headers: &headers,
    };
    let mut first_token = None;
    let reply = stream_retrying(endpoint, &body, &message, sink, stop, &mut first_token).await;
    assistant.time.first_token = first_token;
    let reply = match reply {
        Ok(reply) => reply,
        Err(e) => {
            assistant.error = Some(ErrorInfo {
                kind: if e.is_stopped() {
                    "aborted"
                } else {
                    "provider"
                }
                .to_owned(),
                message: e.to_string(),
            });
            assistant.finish = Some(if e.is_stopped() { "unknown" } else { "error" }.to_owned());
            assistant.time.completed = Some(now());
            sink.emit(Event::StepEnded {
                step: Box::new(assistant),
            });
            return Err(e);
        }
    };
    assistant.time.streamed = Some(now());
    assistant.tokens = reply.tokens;
    assistant.cost = turn
        .provider
        .models
        .iter()
        .find(|m| m.id == turn.model.model)
        .map_or(0.0, |m| m.cost(&assistant.tokens));
    assistant.finish = Some(finish_name(reply.finish.as_deref()).to_owned());
    assistant.raw_finish = reply.finish;

    let calls = fill(&mut assistant, reply.blocks, sink);
    sink.emit(Event::StepStreamed {
        step: Box::new(assistant.clone()),
    });
    Ok((assistant, calls))
}

/// Puts a reply's blocks into the step and reports its calls; returns the
/// calls' places in its content.
fn fill(assistant: &mut Assistant, blocks: Vec<Block>, sink: &impl Sink) -> Vec<usize> {
    let message = assistant.id.clone();
    let mut calls = Vec::new();
    for block in blocks {
        match block {
            Block::Text(text) => assistant.content.push(Part::Text { text }),
            Block::Reasoning(text) => assistant.content.push(Part::Reasoning { text }),
            Block::Call {
                id,
                name,
                arguments,
            } => {
                calls.push(assistant.content.len());
                let parsed = serde_json::from_str::<Value>(if arguments.trim().is_empty() {
                    "{}"
                } else {
                    &arguments
                });
                let (input, state) = match parsed {
                    Ok(Value::Object(input)) => (
                        input.clone(),
                        ToolState::Running {
                            input,
                            metadata: Map::new(),
                        },
                    ),
                    _ => (
                        Map::new(),
                        failed(
                            Map::new(),
                            "tool",
                            format!("The arguments weren't a JSON object: {arguments}"),
                        ),
                    ),
                };
                sink.emit(Event::ToolCalled {
                    message: message.clone(),
                    call: id.clone(),
                    name: name.clone(),
                    input,
                });
                assistant.content.push(Part::Tool {
                    id,
                    name,
                    state,
                    time: ToolTime {
                        created: now(),
                        ..ToolTime::default()
                    },
                });
            }
        }
    }
    calls
}

/// Checks, asks for and runs the step's calls, in order; returns what
/// follows the step.
async fn run_calls(
    turn: &mut Turn,
    assistant: &mut Assistant,
    calls: Vec<usize>,
    sink: &impl Sink,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Next {
    let message = assistant.id.clone();
    let mut next = if calls.is_empty() {
        Next::Done
    } else {
        Next::Model
    };
    let mut changed: Vec<String> = Vec::new();
    for at in calls {
        let Part::Tool {
            id: call,
            name,
            state,
            time,
        } = &mut assistant.content[at]
        else {
            continue;
        };
        if !matches!(state, ToolState::Running { .. }) {
            sink.emit(Event::ToolEnded {
                message: message.clone(),
                call: call.clone(),
                state: state.clone(),
            });
            continue;
        }
        let input = state.input().clone();
        let ended = if matches!(next, Next::Halt) {
            failed(
                input,
                "skipped",
                "Not run: an earlier call in this step was refused.".to_owned(),
            )
        } else if stop.is_cancelled() {
            failed(input, "aborted", "Stopped before it ran.".to_owned())
        } else {
            match call_tool(turn, &message, call, name, input, time, gate, stop).await {
                Called::Ran(state, files) => {
                    changed.extend(files);
                    state
                }
                Called::Refused(state) => state,
                Called::Halted(state) => {
                    next = Next::Halt;
                    state
                }
            }
        };
        time.completed = Some(now());
        *state = ended;
        sink.emit(Event::ToolEnded {
            message: message.clone(),
            call: call.clone(),
            state: state.clone(),
        });
    }
    changed.dedup();
    assistant.time.completed = Some(now());
    assistant.snapshot = Some(Snapshot {
        start: None,
        end: None,
        files: changed,
    });
    next
}

/// How one call went.
enum Called {
    /// It ran (well or not), changing these files.
    Ran(ToolState, Vec<String>),
    /// The rules or a person refused it, with a reason the model reads.
    Refused(ToolState),
    /// A person refused it without a reason: the turn stops.
    Halted(ToolState),
}

/// Checks, asks for and runs one call.
#[expect(
    clippy::too_many_arguments,
    reason = "one call's whole context, passed down from the step"
)]
async fn call_tool(
    turn: &mut Turn,
    message: &str,
    call: &str,
    name: &str,
    input: Map<String, Value>,
    time: &mut ToolTime,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Called {
    let plan = match Plan::new(name, &input, &turn.cwd) {
        Ok(Plan::TodoWrite { .. }) if !turn.todo => {
            let why = format!("There is no tool called `{name}`.");
            return Called::Ran(failed(input, "tool", why), Vec::new());
        }
        Ok(Plan::Remember { .. } | Plan::SearchSessions { .. }) if turn.memory.is_none() => {
            let why = format!("There is no tool called `{name}`. Notes are off.");
            return Called::Ran(failed(input, "tool", why), Vec::new());
        }
        Ok(plan) => plan,
        Err(why) => return Called::Ran(failed(input, "tool", why), Vec::new()),
    };
    if let (Some(ledger), Some((path, shown, now))) = (&turn.files, changing(&plan))
        && let Err(why) = crate::stale::check(ledger.as_ref(), path, shown, now.as_bytes())
    {
        return Called::Ran(failed(input, "stale", why), Vec::new());
    }
    let permission = plan.permission();
    let layers: Vec<(&str, &[Rule])> = turn
        .rules
        .iter()
        .map(|(n, r)| (n.as_str(), r.as_slice()))
        .collect();
    let decisions: Vec<_> = permission
        .resources
        .iter()
        .map(|r| (r, decide(&layers, permission.action, r)))
        .collect();
    let worst = decisions.iter().max_by_key(|(_, d)| match d.effect {
        Effect::Allow => 0,
        Effect::Ask => 1,
        Effect::Deny => 2,
    });
    if let Some((resource, decision)) = worst {
        let why = decision.because.as_ref().map_or_else(
            || "no rule covers it".to_owned(),
            |(layer, rule)| {
                format!(
                    "the {layer} rule \"{} {}\" says {:?}",
                    rule.action, rule.resource, rule.effect
                )
                .to_lowercase()
            },
        );
        match decision.effect {
            Effect::Allow => {}
            Effect::Deny => {
                let text = format!(
                    "Refused: {} on {resource} isn't allowed ({why}).",
                    permission.action
                );
                return Called::Refused(failed(input, "denied", text));
            }
            Effect::Ask => {
                let ask = Ask {
                    id: id::ascending_id("per"),
                    message: message.to_owned(),
                    call: call.to_owned(),
                    action: permission.action.to_owned(),
                    resources: permission.resources.clone(),
                    save: vec!["*".to_owned()],
                    metadata: permission.metadata.clone(),
                    why: format!("Asks because {why}."),
                };
                let answer = tokio::select! {
                    a = gate.ask(ask) => a,
                    () = stop.cancelled() => return Called::Refused(failed(input, "aborted", "Stopped while waiting for permission.".to_owned())),
                };
                match answer {
                    Answer::Once => {}
                    Answer::Always => turn.rules.push((
                        "session".to_owned(),
                        vec![Rule::any(permission.action, Effect::Allow)],
                    )),
                    Answer::Reject(Some(said)) => {
                        return Called::Refused(failed(
                            input,
                            "rejected",
                            format!("The person declined this call and said: {said}"),
                        ));
                    }
                    Answer::Reject(None) => {
                        return Called::Halted(failed(
                            input,
                            "rejected",
                            "The person declined this call.".to_owned(),
                        ));
                    }
                }
            }
        }
    }
    time.ran = Some(now());
    if let Plan::Question { questions } = plan {
        return ask_questions(input, questions, message, call, gate, stop).await;
    }
    ran(input, run_noted(turn, plan, message, call, stop).await)
}

/// Runs a checked call, noting what a read saw or a write left.
async fn run_noted(
    turn: &Turn,
    plan: Plan,
    message: &str,
    call: &str,
    stop: &CancellationToken,
) -> Result<tools::Output, String> {
    let touched = touched(&plan);
    let result = run_plan(turn, plan, message, call, stop).await;
    if let (Ok(_), Some(ledger), Some((path, wrote))) = (&result, &turn.files, touched)
        && let Ok(bytes) = std::fs::read(&path)
    {
        ledger.note(&path, crate::stale::fingerprint(&bytes), wrote);
    }
    result
}

/// The existing file a call changes, as shown, with what it holds now.
fn changing(plan: &Plan) -> Option<(&std::path::Path, &str, &str)> {
    match plan {
        Plan::Edit {
            path,
            shown,
            before,
            ..
        }
        | Plan::Write {
            path,
            shown,
            before: Some(before),
            ..
        } => Some((path, shown, before)),
        _ => None,
    }
}

/// The file a call reads or writes, and whether it writes it.
fn touched(plan: &Plan) -> Option<(PathBuf, bool)> {
    match plan {
        Plan::Read { path, .. } => Some((path.clone(), false)),
        Plan::Edit { path, .. } | Plan::Write { path, .. } => Some((path.clone(), true)),
        _ => None,
    }
}

/// A call that ran, from what it returned.
fn ran(input: Map<String, Value>, result: Result<tools::Output, String>) -> Called {
    match result {
        Ok(out) => {
            let mut content = vec![json!({ "type": "text", "text": out.text })];
            content.extend(out.pictures.iter().map(|p| {
                json!({
                    "type": "file", "uri": format!("data:{};base64,{}", p.mime, p.data),
                    "mime": p.mime, "name": p.name,
                })
            }));
            let state = ToolState::Completed {
                input,
                content,
                metadata: out.metadata,
            };
            Called::Ran(state, out.changed)
        }
        Err(why) => Called::Ran(failed(input, "tool", why), Vec::new()),
    }
}

/// Asks the person the agent's questions and waits. Dismissed, the turn
/// stops, as a declined call without a reason does.
async fn ask_questions(
    input: Map<String, Value>,
    questions: Vec<crate::question::Question>,
    message: &str,
    call: &str,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Called {
    let asked = Asked {
        id: id::ascending_id("frm"),
        message: message.to_owned(),
        call: call.to_owned(),
        questions: questions.clone(),
    };
    let answers = tokio::select! {
        a = gate.question(asked) => a,
        () = stop.cancelled() => return Called::Refused(failed(input, "aborted", "Stopped while waiting for answers.".to_owned())),
    };
    let Some(answers) = answers else {
        return Called::Halted(failed(
            input,
            "aborted",
            "The user dismissed this question".to_owned(),
        ));
    };
    let text = crate::question::answered(&questions, &answers);
    Called::Ran(
        ToolState::Completed {
            input,
            content: vec![json!({ "type": "text", "text": text })],
            metadata: json!({ "answers": answers, "truncated": false })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        },
        Vec::new(),
    )
}

/// Runs a checked call: the memory tools through the turn's memory, the
/// rest in the project folder.
async fn run_plan(
    turn: &Turn,
    plan: Plan,
    message: &str,
    call: &str,
    stop: &CancellationToken,
) -> Result<tools::Output, String> {
    match (plan, turn.memory.as_ref()) {
        (Plan::Remember { draft }, Some(memory)) => {
            memory.remember(draft, message).map(|note| tools::Output {
                text: format!("Kept as note {}.", note.id),
                metadata: json!({ "note": note })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                changed: Vec::new(),
                pictures: Vec::new(),
            })
        }
        (Plan::SearchSessions { query }, Some(memory)) => {
            memory.search(&query).map(|text| tools::Output {
                text,
                metadata: Map::new(),
                changed: Vec::new(),
                pictures: Vec::new(),
            })
        }
        (Plan::Subagent { request }, _) => match &turn.subagents {
            Some(subagents) => subagents
                .start(message, call, request)
                .await
                .map(|session| tools::Output {
                    text: crate::subagent::started(&session),
                    metadata:
                        json!({ "sessionID": session, "status": "running", "truncated": false })
                            .as_object()
                            .cloned()
                            .unwrap_or_default(),
                    changed: Vec::new(),
                    pictures: Vec::new(),
                }),
            None => Err("There is no tool called `subagent`.".to_owned()),
        },
        (Plan::Skill { id }, _) => turn
            .skills
            .iter()
            .find(|s| s.id == id)
            .map(|skill| tools::Output {
                text: skill.loaded(),
                metadata: json!({ "skill": skill.id, "name": skill.name })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                changed: Vec::new(),
                pictures: Vec::new(),
            })
            .ok_or_else(|| format!("There's no skill {id}.")),
        (plan, _) => plan.run(&turn.cwd, stop).await,
    }
}

fn failed(input: Map<String, Value>, kind: &str, message: String) -> ToolState {
    ToolState::Error {
        input,
        error: ErrorInfo {
            kind: kind.to_owned(),
            message,
        },
        metadata: Map::new(),
    }
}

/// OpenCode's name for a chat completions finish reason.
fn finish_name(raw: Option<&str>) -> &'static str {
    match raw {
        Some("stop") => "stop",
        Some("tool_calls" | "function_call") => "tool-calls",
        Some("length") => "length",
        Some("content_filter") => "content-filter",
        _ => "unknown",
    }
}

fn translate(message: &str, piece: Piece) -> Event {
    let message = message.to_owned();
    match piece {
        Piece::FirstToken => Event::FirstToken { message },
        Piece::TextStarted { at } => Event::TextStarted { message, at },
        Piece::TextDelta { at, delta } => Event::TextDelta { message, at, delta },
        Piece::TextEnded { at, text } => Event::TextEnded { message, at, text },
        Piece::ReasoningStarted { at } => Event::ReasoningStarted { message, at },
        Piece::ReasoningDelta { at, delta } => Event::ReasoningDelta { message, at, delta },
        Piece::ReasoningEnded { at, text } => Event::ReasoningEnded { message, at, text },
        Piece::CallStarted { id, name } => Event::ToolInputStarted {
            message,
            call: id,
            name,
        },
        Piece::CallEnded { id, arguments } => Event::ToolInputEnded {
            message,
            call: id,
            text: arguments,
        },
    }
}
