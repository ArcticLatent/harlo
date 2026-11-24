//! Claude Code as a model (Claude Agent).
//!
//! A turn hands the person's message to the `claude` program, which works
//! with its own tools under the person's Claude plan, and its steps come
//! back as the session's.
//!
//! Claude Code asks before acting; each request goes through the turn's
//! permission rules and, when they say ask, to the person. Its session
//! continues from turn to turn (`--resume`).
//!
//! It can also search the web for a turn on another model, with its own
//! `WebSearch` tool and nothing else.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;
use std::process::Stdio;

use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio_util::sync::CancellationToken;

use crate::config::ModelRef;
use crate::message::{Assistant, Cache, ErrorInfo, Part, StepTime, Tokens, ToolState, ToolTime};
use crate::rules::{Effect, Rule, decide};
use crate::turn::{Answer, Ask, Event, Gate, Outcome, Sink, now};
use crate::{Error, id};

/// The provider ID Claude Code's models are under.
pub const PROVIDER: &str = "claude-agent";

/// One turn for Claude Code.
#[derive(Debug, Clone)]
pub struct ClaudeTurn {
    /// The program to run, usually `claude`.
    pub program: PathBuf,
    /// The folder it works in.
    pub cwd: PathBuf,
    /// The agent the steps are recorded under.
    pub agent: String,
    /// The model, with the effort as its variant.
    pub model: ModelRef,
    /// What to send.
    pub prompt: String,
    /// Claude Code's session to continue.
    pub resume: Option<String>,
    /// The permission rules its requests go through, in layers.
    pub rules: Vec<(String, Vec<Rule>)>,
}

/// How a Claude Code turn went.
#[derive(Debug)]
pub struct Ran {
    /// How it ended.
    pub outcome: Outcome,
    /// Claude Code's session, to continue next time.
    pub session: Option<String>,
}

/// The command that starts Claude Code for `turn`.
fn command(turn: &ClaudeTurn) -> Command {
    let mut command = Command::new(&turn.program);
    command.args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-prompt-tool",
        "stdio",
        "--model",
        &turn.model.model,
    ]);
    if let Some(effort) = &turn.model.variant {
        command.args(["--effort", effort]);
    }
    if let Some(session) = &turn.resume {
        command.args(["--resume", session]);
    }
    command
        .current_dir(&turn.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

/// A permission request as the engine names it: the action and what it
/// acts on.
fn action_of(cwd: &std::path::Path, tool: &str, input: &Value) -> (String, String) {
    let path = |key: &str| {
        let full = input[key].as_str().unwrap_or_default();
        std::path::Path::new(full)
            .strip_prefix(cwd)
            .map_or_else(|_| full.to_owned(), |p| p.display().to_string())
    };
    match tool {
        "Bash" => (
            "shell".to_owned(),
            input["command"].as_str().unwrap_or_default().to_owned(),
        ),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => ("edit".to_owned(), path("file_path")),
        "Read" => ("read".to_owned(), path("file_path")),
        "Glob" | "Grep" => (
            tool.to_lowercase(),
            input["pattern"].as_str().unwrap_or_default().to_owned(),
        ),
        "WebFetch" => (
            "webfetch".to_owned(),
            input["url"].as_str().unwrap_or_default().to_owned(),
        ),
        "WebSearch" => (
            "websearch".to_owned(),
            input["query"].as_str().unwrap_or_default().to_owned(),
        ),
        other => (other.to_owned(), "*".to_owned()),
    }
}

/// The text of a tool result's content.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The step under way, and what's known of it.
struct Step {
    assistant: Assistant,
    /// Claude Code's ID for the model message it stands for.
    claude: String,
}

/// What a run keeps while it reads Claude Code's output.
struct Reading<'a, S: Sink> {
    turn: &'a ClaudeTurn,
    sink: &'a S,
    step: Option<Step>,
    session: Option<String>,
    /// Actions allowed from now on in this run (answered "always").
    always: Vec<(String, String)>,
}

impl<S: Sink> Reading<'_, S> {
    /// The step for Claude Code's message `claude`, started if new.
    fn step_for(&mut self, claude: &str) -> &mut Step {
        if self.step.as_ref().is_some_and(|s| s.claude != claude) {
            self.end_step(None);
        }
        let (sink, turn) = (self.sink, self.turn);
        self.step.get_or_insert_with(|| {
            let message = id::ascending_id("msg");
            let created = now();
            sink.emit(Event::StepStarted {
                message: message.clone(),
                agent: turn.agent.clone(),
                model: turn.model.clone(),
                time: created,
            });
            sink.emit(Event::FirstToken {
                message: message.clone(),
            });
            Step {
                assistant: Assistant {
                    id: message,
                    time: StepTime {
                        created,
                        first_token: Some(created),
                        ..StepTime::default()
                    },
                    agent: turn.agent.clone(),
                    model: turn.model.clone(),
                    content: Vec::new(),
                    finish: None,
                    raw_finish: None,
                    cost: 0.0,
                    tokens: Tokens::default(),
                    error: None,
                    snapshot: None,
                },
                claude: claude.to_owned(),
            }
        })
    }

    /// Ends the step under way, failed with `error` when given.
    fn end_step(&mut self, error: Option<ErrorInfo>) {
        let Some(mut step) = self.step.take() else {
            return;
        };
        let at = now();
        let failed = error.is_some();
        let assistant = &mut step.assistant;
        for part in &mut assistant.content {
            if let Part::Tool { state, time, .. } = part
                && matches!(state, ToolState::Running { .. })
            {
                *state = ToolState::Error {
                    input: state.input().clone(),
                    error: ErrorInfo {
                        kind: "aborted".to_owned(),
                        message: "It ended before the call did.".to_owned(),
                    },
                    metadata: Map::new(),
                };
                time.completed = Some(at);
            }
        }
        let called = assistant
            .content
            .iter()
            .any(|p| matches!(p, Part::Tool { .. }));
        assistant.finish = Some(
            if failed {
                "error"
            } else if called {
                "tool-calls"
            } else {
                "stop"
            }
            .to_owned(),
        );
        assistant.error = error;
        assistant.time.streamed.get_or_insert(at);
        assistant.time.completed = Some(at);
        self.sink.emit(Event::StepEnded {
            step: Box::new(step.assistant),
        });
    }

    /// Takes a model message from Claude Code.
    fn assistant(&mut self, message: &Value) {
        let claude = message["id"].as_str().unwrap_or_default().to_owned();
        let sink = self.sink;
        let step = self.step_for(&claude);
        let id = step.assistant.id.clone();
        let usage = &message["usage"];
        if usage.is_object() {
            let number = |k: &str| usage[k].as_u64().unwrap_or(0);
            step.assistant.tokens = Tokens {
                input: number("input_tokens"),
                output: number("output_tokens"),
                reasoning: 0,
                cache: Cache {
                    read: number("cache_read_input_tokens"),
                    write: number("cache_creation_input_tokens"),
                },
            };
        }
        for block in message["content"].as_array().into_iter().flatten() {
            let at = step.assistant.content.len();
            match block["type"].as_str() {
                Some("text") => {
                    let text = block["text"].as_str().unwrap_or_default().to_owned();
                    sink.emit(Event::TextStarted {
                        message: id.clone(),
                        at,
                    });
                    sink.emit(Event::TextDelta {
                        message: id.clone(),
                        at,
                        delta: text.clone(),
                    });
                    sink.emit(Event::TextEnded {
                        message: id.clone(),
                        at,
                        text: text.clone(),
                    });
                    step.assistant.content.push(Part::Text { text });
                }
                Some("thinking") => {
                    let text = block["thinking"].as_str().unwrap_or_default().to_owned();
                    sink.emit(Event::ReasoningStarted {
                        message: id.clone(),
                        at,
                    });
                    sink.emit(Event::ReasoningDelta {
                        message: id.clone(),
                        at,
                        delta: text.clone(),
                    });
                    sink.emit(Event::ReasoningEnded {
                        message: id.clone(),
                        at,
                        text: text.clone(),
                    });
                    step.assistant.content.push(Part::Reasoning {
                        text,
                        signature: None,
                    });
                }
                Some("tool_use") => {
                    let call = block["id"].as_str().unwrap_or_default().to_owned();
                    let name = block["name"].as_str().unwrap_or_default().to_owned();
                    let input = block["input"].as_object().cloned().unwrap_or_default();
                    sink.emit(Event::ToolInputStarted {
                        message: id.clone(),
                        call: call.clone(),
                        name: name.clone(),
                    });
                    sink.emit(Event::ToolInputEnded {
                        message: id.clone(),
                        call: call.clone(),
                        text: Value::Object(input.clone()).to_string(),
                    });
                    sink.emit(Event::ToolCalled {
                        message: id.clone(),
                        call: call.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                    step.assistant.content.push(Part::Tool {
                        id: call,
                        name,
                        state: ToolState::Running {
                            input,
                            metadata: Map::new(),
                        },
                        time: ToolTime {
                            created: now(),
                            ..ToolTime::default()
                        },
                    });
                }
                _ => {}
            }
        }
    }

    /// Takes tool results from Claude Code.
    fn results(&mut self, message: &Value) {
        let Some(step) = self.step.as_mut() else {
            return;
        };
        if step.assistant.time.streamed.is_none() {
            step.assistant.time.streamed = Some(now());
            self.sink.emit(Event::StepStreamed {
                step: Box::new(step.assistant.clone()),
            });
        }
        let message_id = step.assistant.id.clone();
        for block in message["content"].as_array().into_iter().flatten() {
            if block["type"] != "tool_result" {
                continue;
            }
            let call = block["tool_use_id"].as_str().unwrap_or_default();
            let text = result_text(&block["content"]);
            for part in &mut step.assistant.content {
                let Part::Tool {
                    id, state, time, ..
                } = part
                else {
                    continue;
                };
                if id != call {
                    continue;
                }
                let input = state.input().clone();
                *state = if block["is_error"].as_bool() == Some(true) {
                    ToolState::Error {
                        input,
                        error: ErrorInfo {
                            kind: "tool".to_owned(),
                            message: text.clone(),
                        },
                        metadata: Map::new(),
                    }
                } else {
                    ToolState::Completed {
                        input,
                        content: vec![json!({ "type": "text", "text": text })],
                        metadata: Map::new(),
                    }
                };
                time.completed = Some(now());
                self.sink.emit(Event::ToolEnded {
                    message: message_id.clone(),
                    call: call.to_owned(),
                    state: state.clone(),
                });
            }
        }
    }

    /// Answers Claude Code's request to use a tool; `None` lets the turn
    /// go on, `Some` stops it.
    async fn permission(
        &mut self,
        request: &Value,
        gate: &impl Gate,
        stop: &CancellationToken,
    ) -> (Value, bool) {
        let tool = request["tool_name"].as_str().unwrap_or_default();
        let input = &request["input"];
        let (action, resource) = action_of(&self.turn.cwd, tool, input);
        let allow = json!({ "behavior": "allow", "updatedInput": input });
        if self.always.contains(&(action.clone(), resource.clone())) {
            return (allow, false);
        }
        let layers: Vec<(&str, &[Rule])> = self
            .turn
            .rules
            .iter()
            .map(|(n, r)| (n.as_str(), r.as_slice()))
            .collect();
        let decision = decide(&layers, &action, &resource);
        match decision.effect {
            Effect::Allow => return (allow, false),
            Effect::Deny => {
                let said = format!("Refused: {action} on {resource} isn't allowed.");
                return (json!({ "behavior": "deny", "message": said }), false);
            }
            Effect::Ask => {}
        }
        let call = request["tool_use_id"].as_str().unwrap_or_default();
        let message = self
            .step
            .as_ref()
            .map(|s| s.assistant.id.clone())
            .unwrap_or_default();
        let ask = Ask {
            id: id::ascending_id("per"),
            message,
            call: call.to_owned(),
            action: action.clone(),
            resources: vec![resource.clone()],
            save: crate::rules::save_patterns(&action, std::slice::from_ref(&resource)),
            metadata: input.as_object().cloned().unwrap_or_default(),
            why: "Claude Code asks before this.".to_owned(),
        };
        let answer = tokio::select! {
            a = gate.ask(ask) => a,
            () = stop.cancelled() => Answer::Reject(None),
        };
        match answer {
            Answer::Once => (allow, false),
            Answer::Always => {
                self.always.push((action, resource));
                (allow, false)
            }
            Answer::Reject(Some(said)) => (
                json!({ "behavior": "deny",
                        "message": format!("The person declined this call and said: {said}") }),
                false,
            ),
            Answer::Reject(None) => (
                json!({ "behavior": "deny", "message": "The person declined this call.",
                        "interrupt": true }),
                true,
            ),
        }
    }
}

impl<S: Sink> Reading<'_, S> {
    /// Takes one event from Claude Code; how the turn ended, once it has.
    async fn take(
        &mut self,
        event: &Value,
        stdin: &mut ChildStdin,
        gate: &impl Gate,
        stop: &CancellationToken,
    ) -> Option<Outcome> {
        match event["type"].as_str() {
            Some("system") => {
                if let Some(session) = event["session_id"].as_str() {
                    self.session = Some(session.to_owned());
                }
            }
            Some("assistant") => self.assistant(&event["message"]),
            Some("user") => self.results(&event["message"]),
            Some("control_request") if event["request"]["subtype"] == "can_use_tool" => {
                let (answer, halt) = self.permission(&event["request"], gate, stop).await;
                let reply = json!({ "type": "control_response", "response": {
                    "subtype": "success", "request_id": event["request_id"], "response": answer,
                } });
                if send(stdin, &reply).await.is_err() || halt {
                    self.end_step(None);
                    return Some(Outcome::Interrupted);
                }
            }
            Some("control_request") => {
                let reply = json!({ "type": "control_response", "response": {
                    "subtype": "error", "request_id": event["request_id"],
                    "error": "Harlo doesn't answer this request.",
                } });
                let _ = send(stdin, &reply).await;
            }
            Some("result") => {
                if let Some(session) = event["session_id"].as_str() {
                    self.session = Some(session.to_owned());
                }
                let error = event["is_error"].as_bool() == Some(true)
                    || event["subtype"].as_str().is_some_and(|s| s != "success");
                if error {
                    let why = event["result"]
                        .as_str()
                        .or_else(|| event["subtype"].as_str())
                        .unwrap_or("Claude Code failed.")
                        .to_owned();
                    self.end_step(Some(ErrorInfo {
                        kind: "provider".to_owned(),
                        message: why.clone(),
                    }));
                    return Some(Outcome::Failed(Error::provider(why)));
                }
                self.end_step(None);
                return Some(Outcome::Succeeded);
            }
            _ => {}
        }
        None
    }
}

/// What Claude Code is told when it searches for another model's turn.
const SEARCH_PROMPT: &str = "You are a web search relay. Call the WebSearch tool exactly once, \
                             with the user's message as the query, verbatim. Do not answer from \
                             memory and do not call any other tool.";

/// How long a search may take.
const SEARCH_TIME: std::time::Duration = std::time::Duration::from_secs(60);

/// Searches the web for `query` with Claude Code (`program`): its
/// `WebSearch` tool alone, on its quickest model. It's stopped once the
/// search comes back.
///
/// # Errors
///
/// When Claude Code can't be started, fails, takes over a minute, or
/// ends without searching.
pub async fn search(
    program: &std::path::Path,
    query: &str,
) -> Result<crate::tools::Output, String> {
    let mut child = Command::new(program)
        .args([
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "haiku",
            "--system-prompt",
            SEARCH_PROMPT,
            "--tools",
            "WebSearch",
            "--allowedTools",
            "WebSearch",
            "--no-session-persistence",
        ])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Couldn't start Claude Code ({}): {e}", program.display()))?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return Err("Claude Code's pipes didn't open.".to_owned());
    };
    let read = async move {
        stdin
            .write_all(query.as_bytes())
            .await
            .map_err(|e| format!("Claude Code stopped listening: {e}"))?;
        drop(stdin);
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(event) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if event["type"] == "user"
                && let Some(results) = event["tool_use_result"]["results"].as_array()
            {
                return Ok(searched(query, results));
            }
            if event["type"] == "result" && event["is_error"] == true {
                let said = event["result"].as_str().unwrap_or("it failed");
                return Err(format!("Claude Code couldn't search: {said}"));
            }
        }
        Err("Claude finished without searching the web.".to_owned())
    };
    tokio::time::timeout(SEARCH_TIME, read)
        .await
        .unwrap_or_else(|_| Err("Claude didn't finish the web search within a minute.".to_owned()))
}

/// The search for `query` as the model reads it, from Claude Code's
/// `WebSearch` results: lists of links, and its notes on them.
fn searched(query: &str, results: &[Value]) -> crate::tools::Output {
    let mut parts: Vec<String> = Vec::new();
    for r in results {
        match r {
            Value::String(text) if !text.trim().is_empty() => parts.push(text.trim().to_owned()),
            _ => parts.extend(
                r["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|link| {
                        let url = link["url"].as_str()?;
                        let title = link["title"]
                            .as_str()
                            .filter(|t| !t.is_empty())
                            .unwrap_or(url);
                        Some(format!("## [{title}]({url})"))
                    }),
            ),
        }
    }
    let text = if parts.is_empty() {
        format!("The search for \"{query}\" found nothing.")
    } else {
        parts.join("\n\n")
    };
    let mut metadata = Map::new();
    metadata.insert("provider".to_owned(), json!(PROVIDER));
    crate::tools::Output {
        text,
        metadata,
        changed: Vec::new(),
        pictures: Vec::new(),
    }
}

/// Writes one line of JSON to Claude Code.
async fn send(stdin: &mut ChildStdin, value: &Value) -> Result<(), Error> {
    let mut line = value.to_string();
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| Error::provider(format!("Claude Code stopped listening: {e}")))
}

/// Runs `turn` through Claude Code, reporting to `sink` and asking
/// `gate`, until it ends or `stop`.
pub async fn run(
    turn: ClaudeTurn,
    sink: &impl Sink,
    gate: &impl Gate,
    stop: &CancellationToken,
) -> Ran {
    let failed = |why: String, session| Ran {
        outcome: Outcome::Failed(Error::provider(why)),
        session,
    };
    let mut child = match command(&turn).spawn() {
        Ok(child) => child,
        Err(e) => {
            return failed(
                format!(
                    "Couldn't start Claude Code ({}): {e}",
                    turn.program.display()
                ),
                turn.resume.clone(),
            );
        }
    };
    let (Some(mut stdin), Some(stdout), stderr) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return failed("Claude Code's pipes didn't open.".to_owned(), None);
    };
    // What it says on stderr is kept for when it fails.
    let said = tokio::spawn(async move {
        let mut text = String::new();
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                text.push_str(&line);
                text.push('\n');
            }
        }
        text
    });
    let first = json!({ "type": "user", "session_id": "",
        "message": { "role": "user", "content": [{ "type": "text", "text": turn.prompt }] },
        "parent_tool_use_id": null });
    if let Err(e) = send(&mut stdin, &first).await {
        return failed(e.to_string(), turn.resume.clone());
    }
    let mut reading = Reading {
        turn: &turn,
        sink,
        step: None,
        session: turn.resume.clone(),
        always: Vec::new(),
    };
    let mut lines = BufReader::new(stdout).lines();
    let outcome = loop {
        let line = tokio::select! {
            line = lines.next_line() => line,
            () = stop.cancelled() => {
                let _ = child.start_kill();
                reading.end_step(Some(ErrorInfo { kind: "aborted".to_owned(), message: "Stopped.".to_owned() }));
                break Outcome::Interrupted;
            }
        };
        let Ok(Some(line)) = line else {
            let _ = child.wait().await;
            let why = said.await.unwrap_or_default();
            let last = why
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("it gave no reason");
            let error = format!("Claude Code ended without finishing: {last}");
            reading.end_step(Some(ErrorInfo {
                kind: "provider".to_owned(),
                message: error.clone(),
            }));
            break Outcome::Failed(Error::provider(error));
        };
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(outcome) = reading.take(&event, &mut stdin, gate, stop).await {
            break outcome;
        }
    };
    drop(stdin);
    let _ = child.start_kill();
    Ran {
        outcome,
        session: reading.session,
    }
}

/// The models Claude Code offers, asked of `program` in `cwd`: ID, name
/// and effort levels.
///
/// # Errors
///
/// When Claude Code can't start or doesn't answer.
pub async fn models(
    program: &std::path::Path,
    cwd: &std::path::Path,
) -> Result<Vec<(String, String, Vec<String>)>, String> {
    let mut child = Command::new(program)
        .args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
        ])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Couldn't start Claude Code: {e}"))?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return Err("Claude Code's pipes didn't open.".to_owned());
    };
    let asked = json!({ "type": "control_request", "request_id": "harlo-models",
                        "request": { "subtype": "initialize" } });
    send(&mut stdin, &asked).await.map_err(|e| e.to_string())?;
    let mut lines = BufReader::new(stdout).lines();
    let answered = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(event) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if event["type"] == "control_response"
                && event["response"]["request_id"] == "harlo-models"
            {
                return Some(event["response"]["response"]["models"].clone());
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| "Claude Code didn't list its models.".to_owned())?;
    let _ = child.start_kill();
    Ok(answered
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["value"].as_str()?.to_owned();
            let name = m["displayName"].as_str().unwrap_or(&id).to_owned();
            let efforts = m["supportedEffortLevels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(str::to_owned))
                .collect();
            Some((id, name, efforts))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_read_as_actions() {
        let cwd = std::path::Path::new("/p");
        assert_eq!(
            action_of(cwd, "Bash", &json!({ "command": "ls" })),
            ("shell".to_owned(), "ls".to_owned())
        );
        assert_eq!(
            action_of(cwd, "Edit", &json!({ "file_path": "/p/src/a.rs" })),
            ("edit".to_owned(), "src/a.rs".to_owned())
        );
        assert_eq!(
            action_of(cwd, "mcp__x__y", &json!({})),
            ("mcp__x__y".to_owned(), "*".to_owned())
        );
        assert_eq!(
            result_text(&json!([{ "type": "text", "text": "a" }, { "type": "text", "text": "b" }])),
            "a\nb"
        );
    }
}
