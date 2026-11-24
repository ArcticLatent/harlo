//! Talking to a model's server: OpenAI-compatible chat completions, what
//! most model servers speak (`llama.cpp`, Ollama, vLLM, LM Studio,
//! `OpenRouter`, …), and through [`crate::messages`] and
//! [`crate::responses`] the two other APIs providers use.
//!
//! [`prepare`] makes a request for a provider's API and [`stream`] sends it
//! and reports the reply's pieces as they come: text, reasoning and tool
//! calls, each started, grown and ended. The reply's blocks come back in
//! order with the usage and why it ended.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Map, Value, json};
use sse_stream::SseByteStream;
use tokio_util::sync::CancellationToken;

use crate::catalog::Wire;
use crate::message::{Cache, Message, Part, Tokens, ToolState};
use crate::{Error, ModelRef, ProviderSpec};

/// The longest the server may stay silent during a reply.
const IDLE: Duration = Duration::from_mins(5);

/// Where a request goes.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// The API's base URL, up to `/chat/completions` (or `/messages`,
    /// `/responses`).
    pub base_url: String,
    /// The key.
    pub api_key: Option<String>,
    /// Headers the model or its variant adds.
    pub headers: Vec<(String, String)>,
    /// The API the server speaks.
    pub wire: Wire,
    /// The provider and model asked, whose reasoning signatures the reply
    /// carries.
    pub signer: (String, String),
}

/// The request for `model` of `provider` after `history`, with `system` as
/// its instructions and `tools` offered: where it goes and its body.
pub(crate) fn prepare(
    provider: &ProviderSpec,
    model: &ModelRef,
    system: &str,
    history: &[Message],
    tools: &[Value],
) -> (Endpoint, Value) {
    let spec = provider.models.iter().find(|m| m.id == model.model);
    let (extra, headers) = spec
        .map(|m| m.request_extras(model.variant.as_deref()))
        .unwrap_or_default();
    let output = spec.and_then(|m| m.output);
    let at = Signer {
        provider: &provider.id,
        model: &model.model,
    };
    let body = match &provider.wire {
        Wire::Chat => request(&model.model, system, history, tools, &extra),
        Wire::Messages => crate::messages::request(at, output, system, history, tools, &extra),
        Wire::Responses => crate::responses::request(at, system, history, tools, &extra),
        Wire::Unsupported(_) => Value::Null,
    };
    let endpoint = Endpoint {
        base_url: provider.base_url.clone(),
        api_key: provider.api_key.clone(),
        headers,
        wire: provider.wire.clone(),
        signer: (provider.id.clone(), model.model.clone()),
    };
    (endpoint, body)
}

/// Whose reasoning blocks go back to the model: those its provider and
/// model signed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Signer<'a> {
    pub provider: &'a str,
    pub model: &'a str,
}

impl Signer<'_> {
    /// A reasoning block's signature, `kept` with whose it is.
    pub fn sign(&self, wire: &str, kept: &Value) -> Value {
        json!({ "wire": wire, "provider": self.provider, "model": self.model, "kept": kept })
    }

    /// What signature `signature` keeps, when this provider and model made
    /// it over `wire`.
    pub fn mine<'v>(&self, wire: &str, signature: Option<&'v Value>) -> Option<&'v Value> {
        let s = signature?;
        (s["wire"] == wire && s["provider"] == self.provider && s["model"] == self.model)
            .then(|| &s["kept"])
    }
}

/// A picture part as chat completions send it (`image_url` with a data
/// URL): its type and base64 data; or its URL when it isn't data.
pub(crate) fn picture(part: &Value) -> Option<Result<(String, String), String>> {
    let url = part["image_url"]["url"].as_str()?;
    Some(
        url.strip_prefix("data:")
            .and_then(|rest| rest.split_once(";base64,"))
            .map(|(mime, data)| (mime.to_owned(), data.to_owned()))
            .ok_or_else(|| url.to_owned()),
    )
}

/// A piece of the reply as it streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    /// The first token came.
    FirstToken,
    /// A text block began at `at` in the step's content.
    TextStarted { at: usize },
    /// More of block `at`'s text.
    TextDelta { at: usize, delta: String },
    /// Block `at`'s text is complete.
    TextEnded { at: usize, text: String },
    /// A reasoning block began at `at`.
    ReasoningStarted { at: usize },
    /// More of block `at`'s reasoning.
    ReasoningDelta { at: usize, delta: String },
    /// Block `at`'s reasoning is complete.
    ReasoningEnded { at: usize, text: String },
    /// A tool call began.
    CallStarted { id: String, name: String },
    /// A tool call's arguments are complete, as the model wrote them.
    CallEnded { id: String, arguments: String },
}

/// A block of the reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Block {
    Text(String),
    /// Reasoning, with what the provider needs back to continue from it.
    Reasoning(String, Option<Value>),
    Call {
        id: String,
        name: String,
        arguments: String,
    },
}

/// The whole reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Reply {
    pub blocks: Vec<Block>,
    pub tokens: Tokens,
    /// The server's finish reason, such as `stop` or `tool_calls`.
    pub finish: Option<String>,
}

/// The request's body: the system prompt, the history and the tools,
/// then the model's and variant's own fields over them.
pub(crate) fn request(
    model: &str,
    system: &str,
    history: &[Message],
    tools: &[Value],
    extra: &Map<String, Value>,
) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": system })];
    messages.extend(chat_messages(history));
    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    if !tools.is_empty() {
        let functions: Vec<Value> = tools
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t["name"], "description": t["description"], "parameters": t["parameters"] } }))
            .collect();
        body["tools"] = json!(functions);
        body["tool_choice"] = json!("auto");
    }
    if let Value::Object(fields) = &mut body {
        fields.extend(extra.clone());
    }
    body
}

/// The session's messages as chat completions' messages: each step's text
/// and calls, then a `tool` message per call with its result.
fn chat_messages(history: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in crate::compaction::since_summary(history) {
        match message {
            Message::Compaction(c) => {
                if let Some(summary) = c.summary.as_ref().filter(|_| c.status == "completed") {
                    let text = format!("{}{summary}", crate::compaction::SUMMARY_HEADING);
                    out.push(json!({ "role": "user", "content": text }));
                }
            }
            Message::User {
                text,
                skills,
                files,
                ..
            } => {
                let mut said: String = skills.iter().map(|s| s.text.as_str()).collect();
                said.push_str(text);
                let attached: Vec<Value> = files
                    .iter()
                    .filter_map(crate::attachment::File::content)
                    .collect();
                if attached.is_empty() {
                    out.push(json!({ "role": "user", "content": said }));
                } else {
                    let mut content = vec![json!({ "type": "text", "text": said })];
                    content.extend(attached);
                    out.push(json!({ "role": "user", "content": content }));
                }
            }
            Message::Synthetic { text, .. } => {
                out.push(json!({ "role": "user", "content": text }));
            }
            Message::Assistant(step) => {
                let text: String = step
                    .content
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                let calls: Vec<Value> = step
                    .content
                    .iter()
                    .filter_map(|p| match p {
                        Part::Tool { id, name, state, .. } => Some(json!({
                            "id": id,
                            "type": "function",
                            "function": { "name": name, "arguments": Value::Object(state.input().clone()).to_string() },
                        })),
                        _ => None,
                    })
                    .collect();
                if text.is_empty() && calls.is_empty() {
                    continue;
                }
                let mut m = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) } });
                if !calls.is_empty() {
                    m["tool_calls"] = json!(calls);
                }
                out.push(m);
                let mut pictures = Vec::new();
                for part in &step.content {
                    if let Part::Tool { id, state, .. } = part {
                        out.push(json!({ "role": "tool", "tool_call_id": id, "content": result_text(state) }));
                        pictures.extend(result_pictures(state));
                    }
                }
                // Tool messages carry text only: pictures follow as the
                // person's message.
                if !pictures.is_empty() {
                    let mut content =
                        vec![json!({ "type": "text", "text": "The pictures the tools returned:" })];
                    content.extend(pictures);
                    out.push(json!({ "role": "user", "content": content }));
                }
            }
            // (A person's command reaches the model as the synthetic
            // message that follows it.)
            Message::Idle { .. } | Message::Shell(_) => {}
        }
    }
    out
}

/// The pictures a call returned, as image parts.
pub(crate) fn result_pictures(state: &ToolState) -> Vec<Value> {
    let ToolState::Completed { content, .. } = state else {
        return Vec::new();
    };
    content
        .iter()
        .filter(|c| {
            c["type"] == "file" && c["mime"].as_str().is_some_and(|m| m.starts_with("image/"))
        })
        .filter_map(|c| c["uri"].as_str())
        .map(|uri| json!({ "type": "image_url", "image_url": { "url": uri } }))
        .collect()
}

/// What the model is told a call returned.
pub(crate) fn result_text(state: &ToolState) -> String {
    match state {
        ToolState::Completed { content, .. } => content
            .iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        ToolState::Error { error, .. } => format!("Error: {}", error.message),
        // (A call cut off by a stop or a restart.)
        ToolState::Running { .. } => {
            "The call was interrupted before it finished; it may have partly run.".to_owned()
        }
    }
}

/// Sends `body` to `to` and streams the reply, reporting its pieces to
/// `on` as they come.
///
/// # Errors
///
/// Fails when the server can't be reached, refuses the request, reports an
/// error in its stream, goes silent for five minutes, speaks an API the
/// engine doesn't, or `stop` is cancelled.
pub(crate) async fn stream(
    to: &Endpoint,
    body: &Value,
    stop: &CancellationToken,
    on: &mut (dyn FnMut(Piece) + Send),
) -> Result<Reply, Error> {
    let client = reqwest::Client::new();
    let base = &to.base_url;
    let mut req = match &to.wire {
        Wire::Chat => client.post(format!("{base}/chat/completions")),
        Wire::Messages => client
            .post(format!("{base}/messages"))
            .header("anthropic-version", "2023-06-01"),
        Wire::Responses => client.post(format!("{base}/responses")),
        Wire::Unsupported(package) => {
            return Err(Error::provider(format!(
                "Harlo's server can't talk to this provider yet: it speaks {package}."
            )));
        }
    }
    .json(body);
    if let Some(key) = &to.api_key {
        req = match &to.wire {
            Wire::Messages => req.header("x-api-key", key),
            // Azure takes its keys in a header of its own.
            Wire::Responses if base.contains(".azure.com") => req.header("api-key", key),
            _ => req.bearer_auth(key),
        };
    }
    for (name, value) in &to.headers {
        req = req.header(name.as_str(), value.as_str());
    }
    let response = tokio::select! {
        () = stop.cancelled() => return Err(Error::stopped()),
        r = req.send() => r.map_err(|e| Error::unreachable(format!("Couldn't reach {base}: {e}")))?,
    };
    let status = response.status();
    if !status.is_success() {
        let wait = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(std::time::Duration::from_secs);
        let text = response.text().await.unwrap_or_default();
        return Err(Error::refused(
            status.as_u16(),
            refusal(status.as_u16(), &text),
            wait,
        ));
    }
    let mut events = SseByteStream::new(response.bytes_stream());
    let mut reply = Assembler {
        signer: Some((
            match &to.wire {
                Wire::Messages => "messages",
                _ => "responses",
            },
            to.signer.clone(),
        )),
        ..Assembler::default()
    };
    loop {
        let next = tokio::select! {
            () = stop.cancelled() => return Err(Error::stopped()),
            n = tokio::time::timeout(IDLE, events.next()) => n,
        };
        let Ok(next) = next else {
            return Err(Error::provider(
                "The model's server sent nothing for five minutes during the reply.",
            ));
        };
        let Some(item) = next else { break };
        let sse = item.map_err(|e| Error::provider(format!("The reply's stream broke: {e}")))?;
        let Some(data) = sse.data else { continue };
        if data.trim() == "[DONE]" {
            break;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let more = match &to.wire {
            Wire::Messages => crate::messages::take(&mut reply, &chunk, on)?,
            Wire::Responses => crate::responses::take(&mut reply, &chunk, on)?,
            _ => {
                if let Some(e) = chunk.get("error").filter(|e| !e.is_null()) {
                    let said = e["message"]
                        .as_str()
                        .or_else(|| e.as_str())
                        .unwrap_or("the request failed");
                    return Err(reported(said));
                }
                reply.take(&chunk, on);
                true
            }
        };
        if !more {
            break;
        }
    }
    Ok(reply.finish(on))
}

/// An error the model's server reported in its stream.
pub(crate) fn reported(said: &str) -> Error {
    Error::provider(format!("The model's server reported an error: {said}"))
}

/// Why a server refused, for a person.
fn refusal(status: u16, text: &str) -> String {
    let body: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let said = body["error"]["message"]
        .as_str()
        .or_else(|| body["error"].as_str())
        .or_else(|| body["message"].as_str())
        .map_or_else(|| text.chars().take(300).collect(), str::to_owned);
    format!("The model's server refused the request ({status}): {said}")
}

/// Builds the reply from its pieces, whichever API they came in.
#[derive(Default)]
pub(crate) struct Assembler {
    blocks: Vec<Block>,
    /// The text or reasoning block still growing.
    open: Option<usize>,
    /// A call's key in the stream → its block.
    calls: HashMap<String, usize>,
    pub tokens: Tokens,
    pub finish: Option<String>,
    started: bool,
    /// The wire, provider and model signatures are kept with.
    signer: Option<(&'static str, (String, String))>,
}

impl Assembler {
    fn take(&mut self, chunk: &Value, on: &mut (dyn FnMut(Piece) + Send)) {
        if let Some(u) = chunk.get("usage").filter(|u| u.is_object()) {
            self.tokens = tokens(u);
        }
        let Some(choice) = chunk["choices"].get(0) else {
            return;
        };
        let d = &choice["delta"];
        let reasoning = d["reasoning_content"]
            .as_str()
            .or_else(|| d["reasoning"].as_str())
            .unwrap_or_default();
        if !reasoning.is_empty() {
            self.grow(true, reasoning, on);
        }
        if let Some(text) = d["content"].as_str().filter(|t| !t.is_empty()) {
            self.grow(false, text, on);
        }
        for c in d["tool_calls"].as_array().into_iter().flatten() {
            let key = c["index"].as_u64().unwrap_or(0).to_string();
            if !self.calls.contains_key(&key) {
                self.call_started(
                    key.clone(),
                    c["id"].as_str().unwrap_or_default(),
                    c["function"]["name"].as_str().unwrap_or_default(),
                    on,
                );
            }
            if let Some(a) = c["function"]["arguments"].as_str() {
                self.call_grew(&key, a);
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            self.finish = Some(f.to_owned());
        }
    }

    /// A tool call began; `key` names it in later pieces.
    pub fn call_started(
        &mut self,
        key: String,
        id: &str,
        name: &str,
        on: &mut (dyn FnMut(Piece) + Send),
    ) {
        self.first(on);
        self.close(on);
        let at = self.blocks.len();
        let id = Some(id)
            .filter(|s| !s.is_empty())
            .map_or_else(|| crate::id::ascending_id("call"), str::to_owned);
        on(Piece::CallStarted {
            id: id.clone(),
            name: name.to_owned(),
        });
        self.blocks.push(Block::Call {
            id,
            name: name.to_owned(),
            arguments: String::new(),
        });
        self.calls.insert(key, at);
    }

    /// More of call `key`'s arguments.
    pub fn call_grew(&mut self, key: &str, delta: &str) {
        if let Some(at) = self.calls.get(key)
            && let Some(Block::Call { arguments, .. }) = self.blocks.get_mut(*at)
        {
            arguments.push_str(delta);
        }
    }

    /// More text.
    pub fn text(&mut self, delta: &str, on: &mut (dyn FnMut(Piece) + Send)) {
        if !delta.is_empty() {
            self.grow(false, delta, on);
        }
    }

    /// More reasoning; a fresh block when `new`.
    pub fn reasoning(&mut self, delta: &str, new: bool, on: &mut (dyn FnMut(Piece) + Send)) {
        if new {
            self.close(on);
        }
        if !delta.is_empty() {
            self.grow(true, delta, on);
        }
    }

    /// Gives the last reasoning block `signature`, or adds an empty one
    /// that holds it (reasoning the server keeps to itself).
    pub fn sign(&mut self, signature: Value, on: &mut (dyn FnMut(Piece) + Send)) {
        let signature = match &self.signer {
            Some((wire, (provider, model))) => Signer { provider, model }.sign(wire, &signature),
            None => signature,
        };
        let last = self
            .blocks
            .iter_mut()
            .rev()
            .find(|b| !matches!(b, Block::Call { .. }));
        if let Some(Block::Reasoning(_, kept @ None)) = last {
            *kept = Some(signature);
            return;
        }
        self.first(on);
        self.close(on);
        // Announced like any block, so the places of later ones hold.
        let at = self.blocks.len();
        on(Piece::ReasoningStarted { at });
        self.blocks
            .push(Block::Reasoning(String::new(), Some(signature)));
        on(Piece::ReasoningEnded {
            at,
            text: String::new(),
        });
    }

    /// Ends the growing text or reasoning block, so the next piece starts
    /// another.
    pub fn end_block(&mut self, on: &mut (dyn FnMut(Piece) + Send)) {
        self.close(on);
    }

    fn first(&mut self, on: &mut (dyn FnMut(Piece) + Send)) {
        if !self.started {
            self.started = true;
            on(Piece::FirstToken);
        }
    }

    /// The open block of that kind grows, or a new one starts.
    fn grow(&mut self, reasoning: bool, delta: &str, on: &mut (dyn FnMut(Piece) + Send)) {
        self.first(on);
        let same = self.open.is_some_and(|at| {
            matches!(
                (&self.blocks[at], reasoning),
                (Block::Reasoning(..), true) | (Block::Text(_), false)
            )
        });
        if !same {
            self.close(on);
            let at = self.blocks.len();
            self.blocks.push(if reasoning {
                Block::Reasoning(String::new(), None)
            } else {
                Block::Text(String::new())
            });
            self.open = Some(at);
            on(if reasoning {
                Piece::ReasoningStarted { at }
            } else {
                Piece::TextStarted { at }
            });
        }
        let Some(at) = self.open else { return };
        if let Block::Reasoning(s, _) | Block::Text(s) = &mut self.blocks[at] {
            s.push_str(delta);
        }
        let delta = delta.to_owned();
        on(if reasoning {
            Piece::ReasoningDelta { at, delta }
        } else {
            Piece::TextDelta { at, delta }
        });
    }

    /// Ends the growing text or reasoning block.
    fn close(&mut self, on: &mut (dyn FnMut(Piece) + Send)) {
        if let Some(at) = self.open.take() {
            match &self.blocks[at] {
                Block::Text(s) => on(Piece::TextEnded {
                    at,
                    text: s.clone(),
                }),
                Block::Reasoning(s, _) => on(Piece::ReasoningEnded {
                    at,
                    text: s.clone(),
                }),
                Block::Call { .. } => {}
            }
        }
    }

    pub fn finish(mut self, on: &mut (dyn FnMut(Piece) + Send)) -> Reply {
        self.close(on);
        for block in &self.blocks {
            if let Block::Call { id, arguments, .. } = block {
                on(Piece::CallEnded {
                    id: id.clone(),
                    arguments: arguments.clone(),
                });
            }
        }
        Reply {
            blocks: self.blocks,
            tokens: self.tokens,
            finish: self.finish,
        }
    }
}

/// Usage as chat completions report it, as OpenCode counts it: input
/// without the cached part, output with reasoning.
fn tokens(u: &Value) -> Tokens {
    let prompt = u["prompt_tokens"].as_u64().unwrap_or(0);
    let cached = u["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .or_else(|| u["cached_tokens"].as_u64())
        .unwrap_or(0);
    Tokens {
        input: prompt.saturating_sub(cached),
        output: u["completion_tokens"].as_u64().unwrap_or(0),
        reasoning: u["completion_tokens_details"]["reasoning_tokens"]
            .as_u64()
            .unwrap_or(0),
        cache: Cache {
            read: cached,
            write: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fed(chunks: &[Value]) -> (Reply, Vec<Piece>) {
        let mut pieces = Vec::new();
        let mut a = Assembler::default();
        let mut on = |p| pieces.push(p);
        for c in chunks {
            a.take(c, &mut on);
        }
        let reply = a.finish(&mut on);
        (reply, pieces)
    }

    #[test]
    fn text_reasoning_and_calls_come_in_order() {
        let (reply, pieces) = fed(&[
            json!({ "choices": [{ "delta": { "reasoning_content": "think" } }] }),
            json!({ "choices": [{ "delta": { "content": "Hel" } }] }),
            json!({ "choices": [{ "delta": { "content": "lo" } }] }),
            json!({ "choices": [{ "delta": { "tool_calls": [{ "index": 0, "id": "c1", "function": { "name": "edit", "arguments": "{\"pa" } }] } }] }),
            json!({ "choices": [{ "delta": { "tool_calls": [{ "index": 0, "function": { "arguments": "th\":\"a\"}" } }] }, "finish_reason": "tool_calls" }] }),
            json!({ "choices": [], "usage": { "prompt_tokens": 10, "completion_tokens": 5, "prompt_tokens_details": { "cached_tokens": 4 } } }),
        ]);
        assert_eq!(
            reply.blocks,
            [
                Block::Reasoning("think".into(), None),
                Block::Text("Hello".into()),
                Block::Call {
                    id: "c1".into(),
                    name: "edit".into(),
                    arguments: "{\"path\":\"a\"}".into()
                },
            ]
        );
        assert_eq!(
            (
                reply.tokens.input,
                reply.tokens.cache.read,
                reply.tokens.output
            ),
            (6, 4, 5)
        );
        assert_eq!(reply.finish.as_deref(), Some("tool_calls"));
        assert_eq!(pieces[0], Piece::FirstToken);
        assert!(pieces.contains(&Piece::TextEnded {
            at: 1,
            text: "Hello".into()
        }));
        assert_eq!(
            pieces.last(),
            Some(&Piece::CallEnded {
                id: "c1".into(),
                arguments: "{\"path\":\"a\"}".into()
            })
        );
    }

    #[test]
    fn history_becomes_calls_then_their_results() {
        use crate::message::{Assistant, Created, StepTime, ToolTime};
        let mut input = serde_json::Map::new();
        input.insert("path".into(), json!("a.txt"));
        let step = Assistant {
            id: "m".into(),
            time: StepTime::default(),
            agent: "build".into(),
            model: crate::ModelRef {
                provider: "p".into(),
                model: "m".into(),
                variant: None,
            },
            content: vec![Part::Tool {
                id: "c1".into(),
                name: "read".into(),
                state: ToolState::Running {
                    input,
                    metadata: serde_json::Map::new(),
                },
                time: ToolTime::default(),
            }],
            finish: None,
            raw_finish: None,
            cost: 0.0,
            tokens: Tokens::default(),
            error: None,
            snapshot: None,
        };
        let history = [
            Message::User {
                id: "u".into(),
                time: Created { created: 0 },
                text: "hi".into(),
                skills: Vec::new(),
                files: Vec::new(),
            },
            Message::Assistant(Box::new(step)),
        ];
        let extra = json!({ "reasoning_effort": "high" });
        let body = request(
            "m",
            "sys",
            &history,
            &[],
            extra.as_object().expect("object"),
        );
        assert_eq!(body["reasoning_effort"], "high", "a variant's fields");
        let messages = body["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 4);
        assert_eq!(
            messages[2]["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"a.txt\"}"
        );
        assert!(
            messages[3]["content"]
                .as_str()
                .is_some_and(|c| c.contains("interrupted")),
            "{}",
            messages[3]
        );
        assert!(body.get("tools").is_none());
    }
}
