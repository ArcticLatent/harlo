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

use futures_util::StreamExt as _;
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
    /// Where earlier sessions are searched and, when notes are on, notes
    /// kept; none, and the model isn't offered `search_sessions` or
    /// `remember`.
    pub memory: Option<std::sync::Arc<dyn crate::memory::Memory>>,
    /// Where messages that steer the turn come from; none, and nothing
    /// joins it once it started.
    pub steering: Option<std::sync::Arc<dyn Steering>>,
    /// What the session knows of the files it read and wrote; none, and
    /// edits aren't checked for changes made since.
    pub files: Option<std::sync::Arc<dyn crate::stale::Ledger>>,
    /// Where background jobs start; none runs background commands in the
    /// foreground.
    pub jobs: Option<std::sync::Arc<dyn crate::jobs::Jobs>>,
    /// The tools Code Mode scripts can call; none leaves `execute` out.
    pub code: Option<std::sync::Arc<dyn crate::codemode::Catalog>>,
    /// Whether scripts may call the turn's own tools, each under the
    /// tool's own rules.
    pub script_tools: bool,
    /// Whether `read` can list what a source file declares instead of its
    /// text.
    pub outline: bool,
    /// Where web searches go; none leaves `websearch` out.
    pub search: Option<std::sync::Arc<dyn crate::web::Searcher>>,
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
    /// Files no tool reads or changes, whatever the rules say, such as
    /// where the server keeps its password; with their links followed.
    pub sealed: Vec<PathBuf>,
    /// The sandbox commands run in; none runs them unconfined.
    pub sandbox: Option<crate::sandbox::Sandbox>,
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

/// The actions the calls of the tool named `tool` ask leave for.
fn actions(tool: &str) -> Vec<&str> {
    match tool {
        "write" => vec!["edit"],
        "remember" => vec!["remember", crate::rules::REMEMBER_USER],
        "search_sessions" => vec!["search_sessions", crate::rules::SEARCH_EVERYWHERE],
        other => vec![other],
    }
}

/// Whether the rules of `turn` let `tool` run for some call of it.
pub(crate) fn permitted(turn: &Turn, tool: &str) -> bool {
    let layers: Vec<(&str, &[Rule])> = turn
        .rules
        .iter()
        .map(|(n, r)| (n.as_str(), r.as_slice()))
        .collect();
    actions(tool)
        .iter()
        .any(|action| crate::rules::permits(&layers, action))
}

/// The tools offered to the model this turn: the built-in ones, and those
/// the turn's skills, memory, todos, Code Mode and subagents add, less what
/// a rule denies.
///
/// A tool the rules refuse whatever it's called with is left out, so an
/// agent that only reads isn't sent how to write on every request.
pub(crate) fn offered(turn: &Turn) -> Vec<Value> {
    let mut definitions = all(turn);
    let layers: Vec<(&str, &[Rule])> = turn
        .rules
        .iter()
        .map(|(n, r)| (n.as_str(), r.as_slice()))
        .collect();
    definitions.retain(|tool| {
        let name = tool["name"].as_str().unwrap_or_default();
        actions(name)
            .iter()
            .any(|action| crate::rules::permits(&layers, action))
    });
    definitions
}

/// Every tool the turn has, whatever the rules say of it.
fn all(turn: &Turn) -> Vec<Value> {
    let mut definitions = tools::definitions();
    if turn.outline {
        tools::offer_outline(&mut definitions);
    }
    if turn.sandbox.is_some() {
        tools::offer_leaving(&mut definitions);
    }
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
    if let Some(memory) = &turn.memory {
        definitions.extend(crate::memory::definitions(memory.notes()));
    }
    definitions.extend(crate::web::definitions(turn.search.is_some()));
    if let Some(catalog) = &turn.code {
        let mut namespaces = catalog.namespaces();
        if turn.script_tools
            && let Some(own) = crate::script_tools::namespace(turn)
        {
            namespaces.insert(0, own);
        }
        if !namespaces.is_empty() {
            definitions.push(crate::codemode::definition(&namespaces));
        }
        definitions.extend(
            catalog
                .direct()
                .iter()
                .map(crate::codemode::Direct::definition),
        );
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
    definitions
}

/// Runs a turn until the model answers, it fails, or `stop` is cancelled.
pub async fn run(
    mut turn: Turn,
    sink: &impl Sink,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Outcome {
    let definitions = offered(&turn);
    let mut compacted = false;
    // Whether the model's server refused the last step as too long.
    let mut overflowed = false;
    for _ in 0..MAX_STEPS {
        if stop.is_cancelled() {
            return Outcome::Interrupted;
        }
        // Between steps is a safe point: what steers the turn joins it here.
        if let Some(steering) = &turn.steering {
            turn.history.extend(steering.take());
        }
        let full = turn
            .compact_at
            .is_some_and(|limit| crate::compaction::is_full(&turn.history, limit));
        if !compacted && (full || overflowed) {
            // Tried once a turn: a failed summary isn't asked for again each step.
            compacted = true;
            let pending = crate::compaction::Compaction::pending(id::ascending_id("msg"), "auto");
            let done = crate::compaction::compact_with(
                pending,
                &turn,
                &definitions,
                overflowed,
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
            // Too long for the model: the next round summarizes, then asks again.
            Err(e) if e.is_overflow() && !compacted => {
                overflowed = true;
                continue;
            }
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
    // The files as the calls find them, and as they leave them. A step
    // that only looks leaves them as they are: nothing of it to undo, so
    // nothing is recorded, which on a large project is most of what a
    // step of reading costs.
    let changes = calls.iter().any(
        |at| matches!(&assistant.content[*at], Part::Tool { name, .. } if !tools::only_looks(name)),
    );
    let start = match (&turn.shadow, changes) {
        (Some(shadow), true) => track(shadow).await,
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

/// Characters of a call's unreadable arguments the model is shown back.
const BAD_ARGUMENTS: usize = 300;

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
    endpoint: &Endpoint,
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
    let (endpoint, body) = chat::prepare(
        &turn.provider,
        &turn.model,
        &turn.system,
        &turn.history,
        definitions,
    );
    let mut first_token = None;
    let reply = stream_retrying(&endpoint, &body, &message, sink, stop, &mut first_token).await;
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
            Block::Reasoning(text, signature) => {
                assistant.content.push(Part::Reasoning { text, signature });
            }
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
                            // (Its start is enough to see what went wrong.)
                            format!(
                                "The arguments weren't a JSON object. They began: {}",
                                arguments.chars().take(BAD_ARGUMENTS).collect::<String>()
                            ),
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

/// How many of a step's calls run at once.
const TOGETHER: usize = 8;

/// A checked call that only looks, waiting to run with those beside it.
struct Waiting {
    /// Its place in the step's content.
    at: usize,
    /// Its ID.
    call: String,
    /// What the model passed it.
    input: Map<String, Value>,
    /// What it does.
    plan: Plan,
}

/// Checks, asks for and runs the step's calls; returns what follows the
/// step.
///
/// Calls are checked, and a person asked, one at a time in the order the
/// model made them. Calls beside each other that only look (a read, a
/// search, a fetch) then run at once, [`TOGETHER`] at a time. Any other
/// call runs alone, after those before it and before those after it, so
/// a read after an edit still sees the edit.
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
    let mut waiting: Vec<Waiting> = Vec::new();
    for at in calls {
        let Part::Tool {
            id, name, state, ..
        } = &assistant.content[at]
        else {
            continue;
        };
        let (call, name, input) = (id.clone(), name.clone(), state.input().clone());
        let called = if !matches!(state, ToolState::Running { .. }) {
            // (Its arguments couldn't be read: it has failed already.)
            Called::Ran(state.clone(), Vec::new())
        } else if matches!(next, Next::Halt) {
            let why = "Not run: an earlier call in this step was refused.";
            Called::Refused(failed(input, "skipped", why.to_owned()))
        } else if stop.is_cancelled() {
            let why = "Stopped before it ran.";
            Called::Refused(failed(input, "aborted", why.to_owned()))
        } else {
            match planned(turn, &name, &input) {
                Err(called) => called,
                Ok(plan) => {
                    let together = plan.together();
                    if !together {
                        // It may need what the calls before it found.
                        run_waiting(turn, assistant, &mut waiting, sink, stop).await;
                    }
                    match admit(turn, plan, &input, &message, &call, gate, stop).await {
                        Err(called) => called,
                        Ok(plan) if together => {
                            waiting.push(Waiting {
                                at,
                                call,
                                input,
                                plan,
                            });
                            continue;
                        }
                        Ok(plan) => {
                            began(assistant, at, now());
                            finish(turn, plan, input, &message, &call, gate, stop).await
                        }
                    }
                }
            }
        };
        // (The calls before this one end before it does.)
        run_waiting(turn, assistant, &mut waiting, sink, stop).await;
        let state = match called {
            Called::Ran(state, files) => {
                changed.extend(files);
                state
            }
            Called::Refused(state) => state,
            Called::Halted(state) => {
                next = Next::Halt;
                state
            }
        };
        ended(assistant, at, state, now(), sink);
    }
    run_waiting(turn, assistant, &mut waiting, sink, stop).await;
    changed.dedup();
    assistant.time.completed = Some(now());
    assistant.snapshot = Some(Snapshot {
        start: None,
        end: None,
        files: changed,
    });
    next
}

/// Notes that the call at `at` began to run at `time`.
fn began(assistant: &mut Assistant, at: usize, time: i64) {
    if let Part::Tool { time: times, .. } = &mut assistant.content[at] {
        times.ran = Some(time);
    }
}

/// Records how the call at `at` ended, at `time`, and reports it.
fn ended(assistant: &mut Assistant, at: usize, ended: ToolState, time: i64, sink: &impl Sink) {
    let message = assistant.id.clone();
    let Part::Tool {
        id,
        state,
        time: times,
        ..
    } = &mut assistant.content[at]
    else {
        return;
    };
    // (A call that failed before running keeps the state it has.)
    if matches!(state, ToolState::Running { .. }) {
        times.completed = Some(time);
        *state = ended;
    }
    sink.emit(Event::ToolEnded {
        message,
        call: id.clone(),
        state: state.clone(),
    });
}

/// Runs the calls waiting, at once, and records how each ended, in the
/// order the model made them.
async fn run_waiting(
    turn: &Turn,
    assistant: &mut Assistant,
    waiting: &mut Vec<Waiting>,
    sink: &impl Sink,
    stop: &CancellationToken,
) {
    if waiting.is_empty() {
        return;
    }
    let message = assistant.id.clone();
    let start = now();
    for call in waiting.iter() {
        began(assistant, call.at, start);
    }
    let mut results = futures_util::stream::iter(waiting.drain(..))
        .map(|call| run_one(turn, call, &message, stop))
        .buffered(TOGETHER);
    while let Some((at, state, time)) = results.next().await {
        ended(assistant, at, state, time, sink);
    }
}

/// Runs one waiting call; returns its place, how it ended and when.
async fn run_one(
    turn: &Turn,
    waiting: Waiting,
    message: &str,
    stop: &CancellationToken,
) -> (usize, ToolState, i64) {
    let result = run_noted(turn, waiting.plan, message, &waiting.call, stop).await;
    // (A call that only looks changes no file.)
    let (Called::Ran(state, _) | Called::Refused(state) | Called::Halted(state)) =
        ran(waiting.input, result);
    (waiting.at, state, now())
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

/// How long a Code Mode script may run.
const SCRIPT_LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

/// How a script's call of one of the turn's own tools failed.
pub(crate) enum ScriptCall {
    /// It failed, or was refused with a reason: the script is told why.
    Failed(String),
    /// A person refused it without a reason: the script and the turn stop.
    Halted(String),
}

/// Runs a script's call of the turn's own tool `name` with `input`, as
/// the model's own call of it would run: planned, decided by the rules,
/// asked about where they say to, and noted. `at` is the `execute` call
/// the script runs for.
pub(crate) async fn script_call(
    turn: &mut Turn,
    name: &str,
    input: &Map<String, Value>,
    at: CallOf<'_>,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Result<tools::Output, ScriptCall> {
    let said = |state: &ToolState| match state {
        ToolState::Error { error, .. } => error.message.clone(),
        _ => "The call didn't run.".to_owned(),
    };
    let plan = match planned(turn, name, input) {
        Ok(plan) => plan.for_script(input),
        Err(Called::Ran(state, _) | Called::Refused(state) | Called::Halted(state)) => {
            return Err(ScriptCall::Failed(said(&state)));
        }
    };
    let plan = match admit(turn, plan, input, at.message, at.call, gate, stop).await {
        Ok(plan) => plan,
        Err(Called::Halted(state)) => return Err(ScriptCall::Halted(said(&state))),
        Err(Called::Ran(state, _) | Called::Refused(state)) => {
            return Err(ScriptCall::Failed(said(&state)));
        }
    };
    run_noted(turn, plan, at.message, at.call, stop)
        .await
        .map_err(ScriptCall::Failed)
}

/// Decides whether a script may call tool `tool` of the catalog's
/// `namespace` with `args`: by the rules, asking a person where they say
/// to. `at` is the `execute` call the script runs for.
///
/// The call is decided under the name the tool has when it is offered to
/// the model on its own, `<namespace>_<tool>`, so a rule on a tool holds
/// whichever way it is called. Leave to run scripts isn't leave for
/// every tool a script can reach.
pub(crate) async fn script_outside(
    turn: &mut Turn,
    namespace: &str,
    tool: &str,
    args: &Value,
    at: CallOf<'_>,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Result<(), ScriptCall> {
    let said = |state: &ToolState| match state {
        ToolState::Error { error, .. } => error.message.clone(),
        _ => "The call didn't run.".to_owned(),
    };
    let input = args.as_object().cloned().unwrap_or_default();
    let plan = Plan::External {
        name: format!("{namespace}_{tool}"),
        namespace: namespace.to_owned(),
        tool: tool.to_owned(),
        input: args.clone(),
    };
    match admit(turn, plan, &input, at.message, at.call, gate, stop).await {
        Ok(_) => Ok(()),
        Err(Called::Halted(state)) => Err(ScriptCall::Halted(said(&state))),
        Err(Called::Ran(state, _) | Called::Refused(state)) => {
            Err(ScriptCall::Failed(said(&state)))
        }
    }
}

/// Runs Code Mode script `code` for the call `at`, with the turn's own
/// tools among those it can call; `input` is the call's.
async fn run_script(
    turn: &mut Turn,
    code: &str,
    input: Map<String, Value>,
    at: CallOf<'_>,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Called {
    let Some(catalog) = turn.code.clone() else {
        let why = "Code Mode isn't available here.".to_owned();
        return ran(input, Err(why));
    };
    // (Its own, so a refusal without a reason stops the script at once.)
    let own = stop.child_token();
    let project = crate::script_tools::WithProject::new(catalog.as_ref(), turn, gate, at, &own);
    let result = execute(&project, code, at.message, at.call, &own).await;
    let (changed, halted) = project.ended();
    if halted {
        let why = "The person declined a call this script made.".to_owned();
        return Called::Halted(failed(input, "rejected", why));
    }
    match ran(input, result) {
        Called::Ran(state, _) => Called::Ran(state, changed),
        other => other,
    }
}

/// Runs Code Mode script `code` for call `call` of step `message`.
async fn execute(
    catalog: &dyn crate::codemode::Catalog,
    code: &str,
    message: &str,
    call: &str,
    stop: &CancellationToken,
) -> Result<tools::Output, String> {
    let ran = crate::codemode::run(catalog, code, &json!({}), SCRIPT_LIMIT, stop, &|calls| {
        catalog.progress(message, call, calls);
    })
    .await;
    let (text, truncated) = ran.text();
    if ran.value.is_err() {
        return Err(text);
    }
    Ok(tools::Output {
        text,
        metadata: json!({ "toolCalls": ran.calls, "truncated": truncated })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        changed: Vec::new(),
        pictures: ran.pictures,
    })
}

/// The rules "always" adds for `action` on `resources`.
fn always(action: &str, resources: &[String]) -> Vec<Rule> {
    crate::rules::save_patterns(action, resources)
        .into_iter()
        .map(|resource| Rule {
            action: action.to_owned(),
            resource,
            effect: Effect::Allow,
        })
        .collect()
}

/// The plan for a call of `name`: a catalog's own tool, or one of the
/// engine's.
fn plan_for(turn: &Turn, name: &str, input: &Map<String, Value>) -> Result<Plan, String> {
    let direct = turn
        .code
        .as_ref()
        .and_then(|c| c.direct().into_iter().find(|d| d.name == name));
    match direct {
        Some(d) => Ok(Plan::External {
            name: d.name,
            namespace: d.namespace,
            tool: d.tool,
            input: Value::Object(input.clone()),
        }),
        None => Plan::new(name, input, &turn.cwd),
    }
}

/// Runs tool `tool` of the catalog's `namespace`, until `stop`.
async fn run_external(
    catalog: &dyn crate::codemode::Catalog,
    namespace: &str,
    tool: &str,
    input: Value,
    stop: &CancellationToken,
) -> Result<tools::Output, String> {
    let (value, pictures) = tokio::select! {
        v = catalog.call_seen(namespace, tool, input) => v?,
        () = stop.cancelled() => return Err("Stopped.".to_owned()),
    };
    let text = tools::outside_result(&value, &crate::web::saved_pages());
    Ok(tools::Output {
        text,
        metadata: Map::new(),
        changed: Vec::new(),
        pictures,
    })
}

/// The plan for a call of `name`; the error is how the call ends when
/// there is no such tool here or its input is wrong.
fn planned(turn: &Turn, name: &str, input: &Map<String, Value>) -> Result<Plan, Called> {
    let fail = |why: String| Called::Ran(failed(input.clone(), "tool", why), Vec::new());
    match plan_for(turn, name, input) {
        // Not offered, so not done: a model that guesses the argument gets
        // the file.
        Ok(Plan::Read {
            path,
            shown,
            offset,
            limit,
            outline: true,
            plain,
        }) if !turn.outline => Ok(Plan::Read {
            path,
            shown,
            offset,
            limit,
            outline: false,
            plain,
        }),
        Ok(Plan::TodoWrite { .. }) if !turn.todo => {
            Err(fail(format!("There is no tool called `{name}`.")))
        }
        Ok(Plan::Remember { .. }) if !turn.memory.as_ref().is_some_and(|m| m.notes()) => Err(fail(
            format!("There is no tool called `{name}`. Notes are off."),
        )),
        Ok(Plan::SearchSessions { .. }) if turn.memory.is_none() => {
            Err(fail(format!("There is no tool called `{name}`.")))
        }
        Ok(plan) => Ok(plan),
        Err(why) => Err(fail(why)),
    }
}

/// Decides whether a planned call may run, asking the person where the
/// rules say to; the error is how the call ends when it may not.
async fn admit(
    turn: &mut Turn,
    plan: Plan,
    input: &Map<String, Value>,
    message: &str,
    call: &str,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Result<Plan, Called> {
    let plan = match plan.sealing(&turn.sealed) {
        Ok(plan) => plan.guarded(&turn.rules),
        Err(why) => return Err(Called::Refused(failed(input.clone(), "denied", why))),
    };
    if let (Some(ledger), Some((path, shown, now))) = (&turn.files, changing(&plan))
        && let Err(why) = crate::stale::check(ledger.as_ref(), path, shown, now.as_bytes())
    {
        return Err(Called::Ran(failed(input.clone(), "stale", why), Vec::new()));
    }
    // Reaching outside the project is decided first, then the call itself.
    let outside = {
        let skills: Vec<&std::path::Path> =
            turn.skills.iter().filter_map(|s| s.path.parent()).collect();
        plan.outside(&turn.cwd, &skills)
    };
    // (Searching one file's contents is reading it.)
    let leaves = leaving(turn, &plan);
    let own = plan.permission();
    // Leave to run a command outside the sandbox is leave to run it, so
    // it isn't asked about twice. What refuses the command still does.
    let own = (leaves.is_none() || denied(turn, &own)).then_some(own);
    let asked = outside
        .into_iter()
        .chain(plan.reads(&turn.cwd))
        .chain(leaves)
        .chain(own);
    for permission in asked {
        permit(turn, &permission, input, message, call, gate, stop).await?;
    }
    Ok(plan)
}

/// Whether the turn's rules refuse `permission` for one of its resources.
fn denied(turn: &Turn, permission: &tools::Permission) -> bool {
    let layers: Vec<(&str, &[Rule])> = turn
        .rules
        .iter()
        .map(|(n, r)| (n.as_str(), r.as_slice()))
        .collect();
    permission
        .resources
        .iter()
        .any(|r| decide(&layers, &permission.action, r).effect == Effect::Deny)
}

/// What a command that asks to run outside the turn's sandbox needs
/// permission for, before it is decided as a command; none for any other
/// call, and where there is no sandbox to leave.
fn leaving(turn: &Turn, plan: &Plan) -> Option<tools::Permission> {
    let Plan::Shell {
        command,
        outside: true,
        ..
    } = plan
    else {
        return None;
    };
    turn.sandbox.as_ref()?;
    Some(tools::Permission {
        action: crate::rules::OUTSIDE_SANDBOX.into(),
        resources: vec![command.clone()],
        metadata: Map::new(),
    })
}

/// Runs a call that may run.
async fn finish(
    turn: &mut Turn,
    plan: Plan,
    input: Map<String, Value>,
    message: &str,
    call: &str,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Called {
    if let Plan::Question { questions } = plan {
        return ask_questions(input, questions, message, call, gate, stop).await;
    }
    // A script's calls are decided by the rules one by one, the turn's
    // own tools and those of a server or a plugin alike: they may ask,
    // and may change the rules, as the model's own calls do.
    if let Plan::Execute { code } = &plan {
        let at = CallOf { message, call };
        return run_script(turn, code, input, at, gate, stop).await;
    }
    ran(input, run_noted(turn, plan, message, call, stop).await)
}

/// Decides `permission` by the turn's rules, asking the person where they
/// say to; the error is how the call ends when it may not run.
async fn permit(
    turn: &mut Turn,
    permission: &tools::Permission,
    input: &Map<String, Value>,
    message: &str,
    call: &str,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Result<(), Called> {
    let layers: Vec<(&str, &[Rule])> = turn
        .rules
        .iter()
        .map(|(n, r)| (n.as_str(), r.as_slice()))
        .collect();
    let worst = permission
        .resources
        .iter()
        .map(|r| (r, decide(&layers, &permission.action, r)))
        .max_by_key(|(_, d)| match d.effect {
            Effect::Allow => 0,
            Effect::Ask => 1,
            Effect::Deny => 2,
        });
    let Some((resource, decision)) = worst else {
        return Ok(());
    };
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
        Effect::Allow => Ok(()),
        Effect::Deny => {
            let text = format!(
                "Refused: {} on {resource} isn't allowed ({why}).",
                permission.action
            );
            Err(Called::Refused(failed(input.clone(), "denied", text)))
        }
        Effect::Ask => {
            let ask = Ask {
                id: id::ascending_id("per"),
                message: message.to_owned(),
                call: call.to_owned(),
                action: permission.action.to_string(),
                resources: permission.resources.clone(),
                save: crate::rules::save_patterns(&permission.action, &permission.resources),
                metadata: permission.metadata.clone(),
                why: format!("Asks because {why}."),
            };
            let answer = tokio::select! {
                a = gate.ask(ask) => a,
                () = stop.cancelled() => {
                    let why = "Stopped while waiting for permission.".to_owned();
                    return Err(Called::Refused(failed(input.clone(), "aborted", why)));
                }
            };
            match answer {
                Answer::Once => Ok(()),
                Answer::Always => {
                    turn.rules.push((
                        "session".to_owned(),
                        always(&permission.action, &permission.resources),
                    ));
                    Ok(())
                }
                Answer::Reject(Some(said)) => Err(Called::Refused(failed(
                    input.clone(),
                    "rejected",
                    format!("The person declined this call and said: {said}"),
                ))),
                Answer::Reject(None) => Err(Called::Halted(failed(
                    input.clone(),
                    "rejected",
                    "The person declined this call.".to_owned(),
                ))),
            }
        }
    }
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

/// A call, by the step it is of and its own ID.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CallOf<'a> {
    /// The step.
    pub(crate) message: &'a str,
    /// The call.
    pub(crate) call: &'a str,
}

/// Runs `command` for the model and waits for it: where the turn has
/// somewhere for background jobs, a slow one carries on there.
async fn run_shell(
    turn: &Turn,
    command: &str,
    confined: Option<&crate::sandbox::Confined>,
    timeout_ms: Option<u64>,
    at: CallOf<'_>,
    stop: &CancellationToken,
) -> tools::Output {
    let Some(jobs) = &turn.jobs else {
        let limit = timeout_ms.unwrap_or(tools::SHELL_TIMEOUT_MS);
        return tools::shell::run(command, &turn.cwd, confined, limit, stop).await;
    };
    run_command(turn, jobs.as_ref(), command, confined, timeout_ms, at, stop).await
}

/// Runs `command` for the model where `jobs` can take a command over.
///
/// A command that names no time limit and outruns the usual one carries
/// on as a background job, and so does any a person asks not to wait
/// for: its work isn't lost, and the model isn't left to run it again. A
/// command that names its limit is stopped at it, as it asked.
async fn run_command(
    turn: &Turn,
    jobs: &dyn crate::jobs::Jobs,
    command: &str,
    confined: Option<&crate::sandbox::Confined>,
    timeout_ms: Option<u64>,
    at: CallOf<'_>,
    stop: &CancellationToken,
) -> tools::Output {
    use tools::shell::Within;
    let patience = timeout_ms.map_or_else(|| jobs.takes_over_after(), |_| None);
    let named = Duration::from_millis(timeout_ms.unwrap_or(tools::SHELL_TIMEOUT_MS));
    let limit = patience.unwrap_or(named);
    let began = std::time::Instant::now();
    let asked = jobs.asked_to_move(at.message, at.call);
    let (running, asked) =
        match tools::shell::run_within(command, &turn.cwd, confined, limit, stop, asked).await {
            Within::Ended(output) => return output,
            Within::OutOfTime(running) if patience.is_none() => {
                let ms = u64::try_from(limit.as_millis()).unwrap_or(u64::MAX);
                return tools::shell::out_of_time(*running, ms).await;
            }
            Within::OutOfTime(running) => (running, false),
            Within::Asked(running) => (running, true),
        };
    let so_far = running.printed();
    let id = jobs.adopt(crate::jobs::Adopted {
        message: at.message,
        call: at.call,
        command,
        cwd: &turn.cwd,
        running: *running,
        asked,
    });
    let mut output = in_background(&id);
    output.text = crate::jobs::moved(&id, began.elapsed().as_secs(), asked, &so_far);
    output
}

/// What the model is told of a command it sent to the background, now
/// job `id`.
fn in_background(id: &str) -> tools::Output {
    tools::Output {
        text: crate::jobs::started(id),
        metadata: json!({ "shellID": id, "background": true, "truncated": false })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        changed: Vec::new(),
        pictures: Vec::new(),
    }
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
        (Plan::SearchSessions { lookup }, Some(memory)) => {
            memory.look(&lookup).map(|text| tools::Output {
                text,
                metadata: Map::new(),
                changed: Vec::new(),
                pictures: Vec::new(),
            })
        }
        (
            Plan::External {
                namespace,
                tool,
                input,
                ..
            },
            _,
        ) if turn.code.is_some() => {
            let catalog = turn.code.as_ref().ok_or_else(String::new)?;
            run_external(catalog.as_ref(), &namespace, &tool, input, stop).await
        }
        (Plan::WebSearch { query }, _) if turn.search.is_some() => {
            let search = turn.search.as_ref().ok_or_else(String::new)?;
            tokio::select! {
                found = search.search(&query) => found,
                () = stop.cancelled() => Err("Stopped.".to_owned()),
            }
        }
        (Plan::Execute { code }, _) if turn.code.is_some() => {
            let catalog = turn.code.as_ref().ok_or_else(String::new)?;
            execute(catalog.as_ref(), &code, message, call, stop).await
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
        (
            Plan::Shell {
                command,
                timeout_ms,
                background,
                outside,
            },
            _,
        ) => {
            // In the sandbox, unless it was let out of it.
            let confined = turn
                .sandbox
                .as_ref()
                .filter(|_| !outside)
                .map(|sandbox| sandbox.confine(&turn.cwd, &command));
            let at = CallOf { message, call };
            if background && let Some(jobs) = &turn.jobs {
                return jobs
                    .shell(message, call, &command, &turn.cwd, confined)
                    .map(|id| in_background(&id));
            }
            Ok(run_shell(turn, &command, confined.as_ref(), timeout_ms, at, stop).await)
        }
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

/// The stored name for a chat completions finish reason.
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
