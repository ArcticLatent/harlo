//! Anthropic's Messages API, streamed: Claude by API key, and the servers
//! that speak it (DeepSeek's and MiniMax's Anthropic endpoints). The reply
//! comes as pi's stream events and is kept as pi's assistant message, like
//! the other APIs'.
//!
//! The system prompt and the conversation's end are marked for the cache,
//! so each request reads what the one before it wrote. Thinking blocks go
//! back as they came, signature and all, to the provider and model that
//! made them; with thinking off they're left out.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use anyhow::Result;
use serde_json::{Value, json};

use crate::chat::Thinking;
use crate::responses::{ApiError, Dropped, Events, Failed, client, refusal_text, send};

/// The API version Claude's requests ask for.
const CLAUDE_VERSION: &str = "2026-10-21";
/// The one other servers are asked for (they read the shape, not the date).
const OTHER_VERSION: &str = "2023-06-01";
/// The beta that lets a thinking block whose history changed be dropped
/// instead of refusing the request (compaction and set-aside outputs
/// change it).
const BINDING_BETA: &str = "thinking-binding-controls-2026-08-01";

/// How a request reaches a server.
#[derive(Clone, Copy, Debug)]
pub struct Wire<'a> {
    /// Its address, up to `/messages`.
    pub base: &'a str,
    /// The API key (none: the server takes none).
    pub key: Option<&'a str>,
    /// The key as `Authorization: Bearer` (MiniMax's), not `x-api-key`.
    pub bearer: bool,
    /// Claude's own server: its version, and thinking's history check.
    pub claude: bool,
    /// The provider's own headers for the request.
    pub headers: &'a [(&'static str, String)],
}

/// A request's body. `effort`: the thinking setting for the model
/// (`disabled` or `between_tools` turn it off, `adaptive` turns it on as
/// it is, any other is an effort); none (or `none`): the model's default.
pub fn request(model: &str, instructions: &str, messages: Vec<Value>, tools: &Value, thinking: Thinking, effort: Option<&str>, max_tokens: u64) -> Value {
    let mut messages = messages;
    // (The conversation so far, cached: the next request reads it.)
    if let Some(last) = messages.last_mut().and_then(|m| m["content"].as_array_mut()).and_then(|c| c.last_mut()) {
        last["cache_control"] = json!({ "type": "ephemeral" });
    }
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "stream": true,
        "system": [{ "type": "text", "text": instructions, "cache_control": { "type": "ephemeral" } }],
        "messages": messages,
    });
    let tools: Vec<Value> = tools.as_array().into_iter().flatten().map(|t| json!({ "name": t["name"], "description": t["description"], "input_schema": t["parameters"] })).collect();
    if !tools.is_empty() {
        body["tools"] = json!(tools);
        body["tool_choice"] = json!({ "type": "auto" });
    }
    match effort {
        None | Some("none") => {}
        Some(off @ ("disabled" | "between_tools")) => body["thinking"] = json!({ "type": off }),
        Some("adaptive") => body["thinking"] = json!({ "type": "adaptive" }),
        Some(e) => {
            body["thinking"] = if thinking == Thinking::Claude {
                json!({ "type": "adaptive", "display": "summarized", "block_binding": { "prefix_mismatch_behavior": "drop_block" } })
            } else {
                json!({ "type": "adaptive" })
            };
            body["output_config"] = json!({ "effort": e });
        }
    }
    body
}

/// Whether `effort` turns thinking off, or leaves it to a model that
/// thinks only when asked (its blocks then stay out of the history sent).
pub fn thinking_off(effort: Option<&str>) -> bool {
    matches!(effort, Some("disabled" | "between_tools" | "none"))
}

/// A call id as the API takes it (letters, digits, `_` and `-`): the
/// call's own part of a Responses id, the rest replaced.
fn call_id(id: &str) -> String {
    let own = id.split_once('|').map_or(id, |(c, _)| c);
    let mut out: String = own.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).take(64).collect();
    if out.is_empty() {
        out.push_str("call");
    }
    out
}

/// What a thinking block's signature says: whose it is and what goes back.
fn signature(provider: &str, model: &str, block: &Value) -> Value {
    match block["type"].as_str() {
        Some("redacted_thinking") => json!({ "messages": provider, "model": model, "data": block["data"] }),
        _ => json!({ "messages": provider, "model": model, "signature": block["signature"] }),
    }
}

/// pi's messages as the Messages API's: one role after the other (a run
/// of the user's joined), tool results first in theirs, images inline.
/// Thinking goes back only to the `provider` and `model` that made it, and
/// not at all with `thinking` false. `images` false (`model` doesn't take
/// them): a note in their place.
pub fn messages(history: &[Value], provider: &str, model: &str, images: bool, thinking: bool) -> Vec<Value> {
    let image = |b: &Value| {
        if images {
            json!({ "type": "image", "source": { "type": "base64", "media_type": b["mimeType"].as_str().unwrap_or("image/png"), "data": b["data"] } })
        } else {
            json!({ "type": "text", "text": crate::images::not_sent(model) })
        }
    };
    let parts = |content: &Value| -> Vec<Value> {
        match content {
            Value::String(s) if !s.is_empty() => vec![json!({ "type": "text", "text": s })],
            Value::Array(blocks) => blocks
                .iter()
                .filter_map(|b| match b["type"].as_str() {
                    Some("image") => Some(image(b)),
                    None | Some("text") => b["text"].as_str().filter(|t| !t.is_empty()).map(|t| json!({ "type": "text", "text": t })),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    // (A message's blocks, joined to the one before when it's the same role's.)
    let add = |out: &mut Vec<Value>, role: &str, blocks: Vec<Value>| {
        if blocks.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last["role"] == role => {
                if let Some(c) = last["content"].as_array_mut() {
                    c.extend(blocks);
                }
            }
            _ => out.push(json!({ "role": role, "content": blocks })),
        }
    };
    let mut out: Vec<Value> = Vec::new();
    for m in history {
        match m["role"].as_str() {
            Some("compactionSummary") => add(&mut out, "user", vec![json!({ "type": "text", "text": crate::compact::replayed(m) })]),
            Some("user" | "custom") => add(&mut out, "user", parts(&m["content"])),
            Some("assistant") => {
                let mut blocks = Vec::new();
                for b in m["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("thinking") if thinking => {
                            let Some(sig) = b["thinkingSignature"].as_str().and_then(|s| serde_json::from_str::<Value>(s).ok()) else { continue };
                            if sig["messages"] != provider || sig["model"] != model {
                                continue;
                            }
                            if let Some(data) = sig["data"].as_str() {
                                blocks.push(json!({ "type": "redacted_thinking", "data": data }));
                            } else if let Some(s) = sig["signature"].as_str() {
                                blocks.push(json!({ "type": "thinking", "thinking": b["thinking"].as_str().unwrap_or_default(), "signature": s }));
                            }
                        }
                        Some("text") => {
                            if let Some(t) = b["text"].as_str().filter(|t| !t.trim().is_empty()) {
                                blocks.push(json!({ "type": "text", "text": t }));
                            }
                        }
                        Some("toolCall") => {
                            let input = if b["arguments"].is_object() { b["arguments"].clone() } else { json!({}) };
                            blocks.push(json!({ "type": "tool_use", "id": call_id(b["id"].as_str().unwrap_or_default()), "name": b["name"], "input": input }));
                        }
                        _ => {}
                    }
                }
                add(&mut out, "assistant", blocks);
            }
            Some("toolResult") => {
                let mut content = parts(&m["content"]);
                if content.is_empty() {
                    content.push(json!({ "type": "text", "text": "(no output)" }));
                }
                let mut result = json!({ "type": "tool_result", "tool_use_id": call_id(m["toolCallId"].as_str().unwrap_or_default()), "content": content });
                if m["isError"] == true {
                    result["is_error"] = json!(true);
                }
                // (Results first in their message: before what the user
                // said after the last round.)
                match out.last_mut() {
                    Some(last) if last["role"] == "user" => {
                        if let Some(c) = last["content"].as_array_mut() {
                            let at = c.iter().take_while(|b| b["type"] == "tool_result").count();
                            c.insert(at, result);
                        }
                    }
                    _ => out.push(json!({ "role": "user", "content": [result] })),
                }
            }
            _ => {}
        }
    }
    out
}

/// A block of the reply as it builds.
#[derive(Debug)]
enum Block {
    Text(String),
    /// Its text, and what goes back with it.
    Thinking(String, Value),
    Call { id: String, name: String, args: String },
    /// One harlo doesn't keep (a server tool's).
    Other,
}

/// The reply as it streams, made into pi's events and message.
#[derive(Default)]
struct Reply {
    blocks: Vec<Block>,
    /// The API's block index → ours.
    at: std::collections::HashMap<u64, usize>,
    usage: Value,
    stop: Option<String>,
    done: bool,
    provider: String,
    model: String,
}

impl Reply {
    /// An event's stream events.
    fn take(&mut self, event: &Value, out: &mut Vec<Value>) {
        match event["type"].as_str() {
            Some("message_start") => self.count(&event["message"]["usage"]),
            Some("message_delta") => {
                self.count(&event["usage"]);
                if let Some(s) = event["delta"]["stop_reason"].as_str() {
                    self.stop = Some(s.to_owned());
                }
            }
            Some("message_stop") => self.done = true,
            Some("content_block_start") => {
                let b = &event["content_block"];
                let at = self.blocks.len();
                self.at.insert(event["index"].as_u64().unwrap_or(0), at);
                let block = match b["type"].as_str() {
                    Some("text") => {
                        out.push(json!({ "type": "text_start", "contentIndex": at }));
                        Block::Text(String::new())
                    }
                    Some(kind @ ("thinking" | "redacted_thinking")) => {
                        out.push(json!({ "type": "thinking_start", "contentIndex": at }));
                        let text = if kind == "thinking" { b["thinking"].as_str().unwrap_or_default().to_owned() } else { String::new() };
                        Block::Thinking(text, signature(&self.provider, &self.model, b))
                    }
                    Some("tool_use") => {
                        let id = b["id"].as_str().map_or_else(|| format!("call_{}", crate::session::now_ms()), str::to_owned);
                        let name = b["name"].as_str().unwrap_or_default().to_owned();
                        out.push(json!({ "type": "toolcall_start", "contentIndex": at, "id": id, "toolName": name }));
                        Block::Call { id, name, args: String::new() }
                    }
                    _ => Block::Other,
                };
                self.blocks.push(block);
            }
            Some("content_block_delta") => {
                let Some(&at) = event["index"].as_u64().and_then(|i| self.at.get(&i)) else { return };
                let d = &event["delta"];
                match (&mut self.blocks[at], d["type"].as_str()) {
                    (Block::Text(s), Some("text_delta")) => {
                        let t = d["text"].as_str().unwrap_or_default();
                        s.push_str(t);
                        out.push(json!({ "type": "text_delta", "contentIndex": at, "delta": t }));
                    }
                    (Block::Thinking(s, _), Some("thinking_delta")) => {
                        let t = d["thinking"].as_str().unwrap_or_default();
                        s.push_str(t);
                        out.push(json!({ "type": "thinking_delta", "contentIndex": at, "delta": t }));
                    }
                    (Block::Thinking(_, sig), Some("signature_delta")) => {
                        let s = format!("{}{}", sig["signature"].as_str().unwrap_or_default(), d["signature"].as_str().unwrap_or_default());
                        sig["signature"] = json!(s);
                    }
                    (Block::Call { args, .. }, Some("input_json_delta")) => {
                        let part = d["partial_json"].as_str().unwrap_or_default();
                        if !part.is_empty() {
                            args.push_str(part);
                            out.push(json!({ "type": "toolcall_delta", "contentIndex": at, "delta": part }));
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                let Some(&at) = event["index"].as_u64().and_then(|i| self.at.get(&i)) else { return };
                match &self.blocks[at] {
                    Block::Text(s) => out.push(json!({ "type": "text_end", "contentIndex": at, "content": s })),
                    Block::Thinking(s, _) => out.push(json!({ "type": "thinking_end", "contentIndex": at, "content": s })),
                    Block::Call { .. } => out.push(json!({ "type": "toolcall_end", "contentIndex": at, "toolCall": self.call(at) })),
                    Block::Other => {}
                }
            }
            _ => {}
        }
    }

    /// A usage report over what was said before (each a total so far).
    fn count(&mut self, u: &Value) {
        if !self.usage.is_object() {
            self.usage = json!({});
        }
        for (k, v) in u.as_object().into_iter().flatten() {
            if v.is_u64() {
                self.usage[k] = v.clone();
            }
        }
    }

    /// The call at `at` as pi's block.
    fn call(&self, at: usize) -> Value {
        let Block::Call { id, name, args } = &self.blocks[at] else { return Value::Null };
        let mut call = json!({ "type": "toolCall", "id": id, "name": name, "arguments": {} });
        let raw = if args.trim().is_empty() { "{}" } else { args.as_str() };
        match serde_json::from_str::<Value>(raw) {
            Ok(v) if v.is_object() => call["arguments"] = v,
            _ => call["argumentsError"] = json!(format!("The arguments weren't a JSON object: {raw}")),
        }
        call
    }

    /// The blocks as pi's content.
    fn content(&self) -> Vec<Value> {
        (0..self.blocks.len())
            .filter_map(|at| match &self.blocks[at] {
                Block::Text(s) => Some(json!({ "type": "text", "text": s })),
                Block::Thinking(s, sig) => Some(json!({ "type": "thinking", "thinking": s, "thinkingSignature": sig.to_string() })),
                Block::Call { .. } => Some(self.call(at)),
                Block::Other => None,
            })
            .collect()
    }
}

/// The usage as the Responses API says it (what the rest reads): the
/// input with the cache's reads and writes in it, each also apart; what
/// wasn't said is left out.
fn usage(u: &Value) -> Value {
    let mut out = json!({});
    if let Some(n) = u["input_tokens"].as_u64() {
        let read = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
        let written = u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
        out["input_tokens"] = json!(n + read + written);
        out["input_tokens_details"] = json!({ "cached_tokens": read, "cache_write_tokens": written });
    }
    if let Some(n) = u["output_tokens"].as_u64() {
        out["output_tokens"] = json!(n);
    }
    out
}

/// What was said of the usage, as the Responses API says it; Null when
/// nothing was (so it isn't taken for a request of unknown size).
fn said(u: &Value) -> Value {
    if u.as_object().is_some_and(|o| !o.is_empty()) { usage(u) } else { Value::Null }
}

/// Sends one request: `on` gets pi's stream events as they come; the
/// reply is pi's assistant message, with its usage as the Responses API
/// says it. `seen`: what it says it used as it goes.
///
/// # Errors
///
/// [`ApiError`] when the server refuses the request; [`Failed`] when the
/// reply ends in an error or a refusal; [`Dropped`] when the server can't
/// be reached or the reply stops partway. The last two carry what the
/// server said the request used.
pub async fn stream(wire: Wire<'_>, body: &Value, provider: &str, model: &str, mut on: impl FnMut(&Value), seen: &mut Value) -> Result<(Value, Value)> {
    let mut req = client().post(format!("{}/messages", wire.base)).header("anthropic-version", if wire.claude { CLAUDE_VERSION } else { OTHER_VERSION }).json(body);
    if wire.claude && body["thinking"]["block_binding"].is_object() {
        req = req.header("anthropic-beta", BINDING_BETA);
    }
    if let Some(k) = wire.key {
        req = if wire.bearer { req.bearer_auth(k) } else { req.header("x-api-key", k) };
    }
    for (k, v) in wire.headers {
        req = req.header(*k, v);
    }
    let resp = send(req, wire.base).await?;
    let status = resp.status();
    let retry_after = crate::retry::retry_after(resp.headers());
    if !status.is_success() {
        let text = refusal_text(resp).await;
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let said = body["error"]["message"].as_str().or_else(|| body["error"].as_str()).or_else(|| body["message"].as_str()).map_or_else(|| text.chars().take(300).collect(), str::to_owned);
        let code = body["error"]["type"].as_str().or_else(|| body["error"]["code"].as_str()).unwrap_or_default().to_owned();
        return Err(ApiError { status: status.as_u16(), message: format!("{provider} refused the request ({status}): {said}"), code, retry_after, resets_at: None }.into());
    }
    let mut reply = Reply { provider: provider.to_owned(), model: model.to_owned(), ..Reply::default() };
    let mut events = Vec::new();
    let mut reader = Events::new(resp);
    loop {
        let data = match reader.next().await {
            Ok(Some(d)) => d,
            Ok(None) => break,
            Err(e) => {
                return Err(match e.downcast::<Dropped>() {
                    Ok(Dropped(why, _)) => Dropped(why, said(&reply.usage)).into(),
                    Err(e) => e,
                });
            }
        };
        let Ok(event) = serde_json::from_str::<Value>(&data) else { continue };
        if event["type"] == "error" {
            let e = &event["error"];
            let code = e["type"].as_str().unwrap_or_default().to_owned();
            return Err(Failed { code, message: format!("{provider}: {}", e["message"].as_str().unwrap_or("the request failed")), usage: said(&reply.usage) }.into());
        }
        reply.take(&event, &mut events);
        if event["usage"].is_object() || event["message"]["usage"].is_object() {
            *seen = said(&reply.usage);
        }
        for e in events.drain(..) {
            on(&e);
        }
        if reply.done {
            break;
        }
    }
    if !reply.done && reply.stop.is_none() {
        return Err(Dropped("The connection closed before the reply finished".into(), said(&reply.usage)).into());
    }
    let usage = usage(&reply.usage);
    if reply.stop.as_deref() == Some("refusal") {
        let mut message = format!("{provider}: the model declined to go on with this request (a refusal)");
        let text: String = reply.content().iter().filter_map(|b| b["text"].as_str()).collect();
        if !text.trim().is_empty() {
            let _ = write!(message, ", after: {}", text.trim().chars().take(300).collect::<String>());
        }
        return Err(Failed { code: "refusal".into(), message, usage }.into());
    }
    let content = reply.content();
    let calls = content.iter().any(|b| b["type"] == "toolCall");
    // (Cut off at the length limit, calls or not: its calls may be
    // incomplete, and aren't run.)
    let stop = match reply.stop.as_deref() {
        Some("max_tokens" | "model_context_window_exceeded") => "length",
        _ if calls => "toolUse",
        _ => "stop",
    };
    let message = json!({
        "role": "assistant",
        "content": content,
        "api": "anthropic-messages",
        "provider": provider,
        "model": model,
        // (Its cost: the agent's to say, which knows the model's price.)
        "usage": crate::used::Used::from_api(&usage).log(&usage, &crate::used::Pricing::Unknown),
        "stopReason": stop,
        "timestamp": crate::session::now_ms(),
    });
    Ok((message, usage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A stand-in Messages server: it keeps each request as it came (head
    /// and body) and answers with `answer` (a whole HTTP response).
    async fn stand_in(answer: String) -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                let (answer, tx) = (answer.clone(), tx.clone());
                tokio::spawn(async move {
                    let mut req = Vec::new();
                    let mut buf = vec![0u8; 65536];
                    loop {
                        let Ok(n) = s.read(&mut buf).await else { return };
                        req.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&req).to_string();
                        if let Some((head, body)) = text.split_once("\r\n\r\n") {
                            let len = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                            if body.len() >= len || n == 0 {
                                let _ = tx.send(text);
                                break;
                            }
                        }
                    }
                    let _ = s.write_all(answer.as_bytes()).await;
                });
            }
        });
        (format!("http://{addr}"), rx)
    }

    fn sse(events: &[Value]) -> String {
        let mut out = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n".to_owned();
        for e in events {
            let _ = write!(out, "event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap());
        }
        out
    }

    /// A reply with summarized thinking, text and a call, its usage from
    /// the cache.
    fn a_reply() -> Vec<Value> {
        vec![
            json!({ "type": "message_start", "message": { "id": "msg_1", "type": "message", "role": "assistant", "content": [], "usage": { "input_tokens": 12, "cache_read_input_tokens": 4000, "cache_creation_input_tokens": 300, "output_tokens": 1 } } }),
            json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "", "signature": "" } }),
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "Read it " } }),
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "first." } }),
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "signature_delta", "signature": "EqQBsig" } }),
            json!({ "type": "content_block_stop", "index": 0 }),
            json!({ "type": "ping" }),
            json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "text", "text": "" } }),
            json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": "Reading." } }),
            json!({ "type": "content_block_stop", "index": 1 }),
            json!({ "type": "content_block_start", "index": 2, "content_block": { "type": "tool_use", "id": "toolu_01", "name": "read", "input": {} } }),
            json!({ "type": "content_block_delta", "index": 2, "delta": { "type": "input_json_delta", "partial_json": "{\"path\":" } }),
            json!({ "type": "content_block_delta", "index": 2, "delta": { "type": "input_json_delta", "partial_json": "\"a.txt\"}" } }),
            json!({ "type": "content_block_stop", "index": 2 }),
            json!({ "type": "message_delta", "delta": { "stop_reason": "tool_use" }, "usage": { "output_tokens": 57 } }),
            json!({ "type": "message_stop" }),
        ]
    }

    /// The history a request is made from: a user's picture, a round with
    /// a call (a Responses id among them), its result with an image, a
    /// note, a summary.
    fn history() -> Vec<Value> {
        let img = json!({ "type": "image", "data": "QUJD", "mimeType": "image/jpeg" });
        let sig = |p: &str, m: &str| json!({ "messages": p, "model": m, "signature": "EqQBsig" }).to_string();
        vec![
            json!({ "role": "compactionSummary", "summary": "Earlier: set up.", "tokensBefore": 9000 }),
            json!({ "role": "user", "content": [{ "type": "text", "text": "look" }, img.clone()] }),
            json!({ "role": "assistant", "content": [
                { "type": "thinking", "thinking": "Read it first.", "thinkingSignature": sig("anthropic", "claude-opus-5-5") },
                { "type": "thinking", "thinking": "another model's", "thinkingSignature": sig("anthropic", "claude-haiku-5-5") },
                { "type": "thinking", "thinking": "a Responses one", "thinkingSignature": "{\"type\":\"reasoning\"}" },
                { "type": "text", "text": "Reading." },
                { "type": "toolCall", "id": "toolu_01", "name": "read", "arguments": { "path": "a.txt" } },
                { "type": "toolCall", "id": "call_9|fc_9", "name": "shot", "arguments": {} },
            ] }),
            json!({ "role": "toolResult", "toolCallId": "toolu_01", "toolName": "read", "content": [{ "type": "text", "text": "A" }], "isError": false }),
            json!({ "role": "custom", "customType": "turn_stopped", "content": "stopped", "display": false }),
            json!({ "role": "toolResult", "toolCallId": "call_9|fc_9", "toolName": "shot", "content": [{ "type": "text", "text": "[image 1]" }, img], "isError": true }),
        ]
    }

    #[test]
    fn history_as_messages() {
        let sent = messages(&history(), "anthropic", "claude-opus-5-5", true, true);
        let image = json!({ "type": "image", "source": { "type": "base64", "media_type": "image/jpeg", "data": "QUJD" } });
        assert_eq!(sent, vec![
            json!({ "role": "user", "content": [
                { "type": "text", "text": crate::compact::replayed(&history()[0]) },
                { "type": "text", "text": "look" },
                image,
            ] }),
            json!({ "role": "assistant", "content": [
                { "type": "thinking", "thinking": "Read it first.", "signature": "EqQBsig" },
                { "type": "text", "text": "Reading." },
                { "type": "tool_use", "id": "toolu_01", "name": "read", "input": { "path": "a.txt" } },
                { "type": "tool_use", "id": "call_9", "name": "shot", "input": {} },
            ] }),
            json!({ "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "toolu_01", "content": [{ "type": "text", "text": "A" }] },
                { "type": "tool_result", "tool_use_id": "call_9", "content": [{ "type": "text", "text": "[image 1]" }, image], "is_error": true },
                { "type": "text", "text": "stopped" },
            ] }),
        ]);
        // Thinking off: none goes back; another model: not its.
        assert!(!json!(messages(&history(), "anthropic", "claude-opus-5-5", true, false)).to_string().contains("\"thinking\""));
        let other = messages(&history(), "anthropic", "claude-haiku-5-5", false, true);
        assert_eq!(other[1]["content"][0]["thinking"], "another model's");
        assert_eq!(other[0]["content"][2], json!({ "type": "text", "text": crate::images::not_sent("claude-haiku-5-5") }));
    }

    #[test]
    fn thinking_settings() {
        let at = |t: Thinking, e: Option<&str>| {
            let b = request("m", "", Vec::new(), &json!([]), t, e, 1000);
            (b.get("thinking").cloned(), b.get("output_config").cloned())
        };
        let binding = json!({ "prefix_mismatch_behavior": "drop_block" });
        assert_eq!(at(Thinking::Claude, Some("high")), (Some(json!({ "type": "adaptive", "display": "summarized", "block_binding": binding })), Some(json!({ "effort": "high" }))));
        assert_eq!(at(Thinking::Claude, Some("between_tools")), (Some(json!({ "type": "between_tools" })), None));
        assert_eq!(at(Thinking::Claude, Some("disabled")), (Some(json!({ "type": "disabled" })), None));
        assert_eq!(at(Thinking::Claude, None), (None, None));
        assert_eq!(at(Thinking::Effort, Some("low")), (Some(json!({ "type": "adaptive" })), Some(json!({ "effort": "low" }))));
        assert_eq!(at(Thinking::Effort, Some("adaptive")), (Some(json!({ "type": "adaptive" })), None));
        assert_eq!(at(Thinking::Effort, Some("none")), (None, None));
        assert!(thinking_off(Some("between_tools")) && thinking_off(Some("disabled")) && thinking_off(Some("none")) && !thinking_off(Some("high")) && !thinking_off(None));
    }

    /// The request as it reaches the server, byte for byte, and the reply
    /// as pi's events and message.
    #[tokio::test]
    async fn a_round_against_a_stand_in() {
        let (base, mut got) = stand_in(sse(&a_reply())).await;
        let tools = json!([{ "name": "read", "description": "Read a file", "parameters": { "type": "object", "properties": { "path": { "type": "string" } } } }]);
        let sent = messages(&history()[1..4], "anthropic", "claude-opus-5-5", true, true);
        let body = request("claude-opus-5-5", "Be brief.", sent, &tools, Thinking::Claude, Some("high"), 64000);
        let mut events = Vec::new();
        let mut seen = Value::Null;
        let wire = Wire { base: &base, key: Some("sk-ant-test"), bearer: false, claude: true, headers: &[] };
        let (message, usage) = stream(wire, &body, "anthropic", "claude-opus-5-5", |e| events.push(e.clone()), &mut seen).await.unwrap();

        let req = got.recv().await.unwrap();
        let (head, raw) = req.split_once("\r\n\r\n").unwrap();
        let head = head.to_ascii_lowercase();
        assert!(head.starts_with("post /messages http/1.1"), "{head}");
        for h in ["x-api-key: sk-ant-test", "anthropic-version: 2026-10-21", "anthropic-beta: thinking-binding-controls-2026-08-01", "content-type: application/json"] {
            assert!(head.contains(h), "{h} in {head}");
        }
        assert!(!head.contains("authorization"), "{head}");
        let expected = concat!(
            r#"{"model":"claude-opus-5-5","max_tokens":64000,"stream":true,"system":[{"type":"text","text":"Be brief.","cache_control":{"type":"ephemeral"}}],"#,
            r#""messages":[{"role":"user","content":[{"type":"text","text":"look"},{"type":"image","source":{"type":"base64","media_type":"image/jpeg","data":"QUJD"}}]},"#,
            r#"{"role":"assistant","content":[{"type":"thinking","thinking":"Read it first.","signature":"EqQBsig"},{"type":"text","text":"Reading."},{"type":"tool_use","id":"toolu_01","name":"read","input":{"path":"a.txt"}},{"type":"tool_use","id":"call_9","name":"shot","input":{}}]},"#,
            r#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","content":[{"type":"text","text":"A"}],"cache_control":{"type":"ephemeral"}}]}],"#,
            r#""tools":[{"name":"read","description":"Read a file","input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}],"tool_choice":{"type":"auto"},"#,
            r#""thinking":{"type":"adaptive","display":"summarized","block_binding":{"prefix_mismatch_behavior":"drop_block"}},"output_config":{"effort":"high"}}"#,
        );
        assert_eq!(raw, expected);

        let sig = json!({ "messages": "anthropic", "model": "claude-opus-5-5", "signature": "EqQBsig" }).to_string();
        assert_eq!(message["content"], json!([
            { "type": "thinking", "thinking": "Read it first.", "thinkingSignature": sig },
            { "type": "text", "text": "Reading." },
            { "type": "toolCall", "id": "toolu_01", "name": "read", "arguments": { "path": "a.txt" } },
        ]));
        assert_eq!((message["stopReason"].as_str(), message["api"].as_str()), (Some("toolUse"), Some("anthropic-messages")));
        let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["thinking_start", "thinking_delta", "thinking_delta", "thinking_end", "text_start", "text_delta", "text_end", "toolcall_start", "toolcall_delta", "toolcall_delta", "toolcall_end"]);
        assert_eq!(usage, json!({ "input_tokens": 4312, "input_tokens_details": { "cached_tokens": 4000, "cache_write_tokens": 300 }, "output_tokens": 57 }));
        assert_eq!((message["usage"]["input"].as_u64(), message["usage"]["cacheRead"].as_u64(), message["usage"]["cacheWrite"].as_u64()), (Some(12), Some(4000), Some(300)));
        assert_eq!(seen, usage);
        // What came back goes back as it came.
        let again = messages(&[message], "anthropic", "claude-opus-5-5", true, true);
        assert_eq!(again[0]["content"][0], json!({ "type": "thinking", "thinking": "Read it first.", "signature": "EqQBsig" }));
    }

    /// Others' servers: the key as a bearer token, the older version, no beta.
    #[tokio::test]
    async fn another_server_gets_its_own_headers() {
        let (base, mut got) = stand_in(sse(&a_reply())).await;
        let body = request("MiniMax-M3", "", vec![json!({ "role": "user", "content": [{ "type": "text", "text": "hi" }] })], &json!([]), Thinking::Effort, Some("adaptive"), 32768);
        let wire = Wire { base: &base, key: Some("mm-key"), bearer: true, claude: false, headers: &[] };
        stream(wire, &body, "minimax-anthropic", "MiniMax-M3", |_| {}, &mut Value::Null).await.unwrap();
        let head = got.recv().await.unwrap().split_once("\r\n\r\n").unwrap().0.to_ascii_lowercase();
        assert!(head.contains("authorization: bearer mm-key") && head.contains("anthropic-version: 2023-06-01"), "{head}");
        assert!(!head.contains("x-api-key") && !head.contains("anthropic-beta"), "{head}");
    }

    #[tokio::test]
    async fn errors_refusals_and_cut_offs() {
        let ask = |answer: String| async move {
            crate::responses::TEST_IDLE.set(Some(std::time::Duration::from_secs(1)));
            let (base, _got) = stand_in(answer).await;
            let wire = Wire { base: &base, key: Some("k"), bearer: false, claude: true, headers: &[] };
            stream(wire, &json!({}), "anthropic", "m", |_| {}, &mut Value::Null).await
        };
        // Overloaded before the reply: tried again.
        let body = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let e = ask(format!("HTTP/1.1 529 Overloaded\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())).await.unwrap_err();
        assert!(e.to_string().contains("Overloaded") && crate::retry::transient(&e).is_some(), "{e:#}");
        // Too long: compacted, not retried.
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 1000001 tokens > 1000000 maximum"}}"#;
        let e = ask(format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())).await.unwrap_err();
        assert!(crate::compact::overflowed(&e) && crate::retry::transient(&e).is_none(), "{e:#}");
        // Overloaded during it: what it said it used is kept, and it's tried again.
        let mut events = a_reply()[..3].to_vec();
        events.push(json!({ "type": "error", "error": { "type": "overloaded_error", "message": "Overloaded" } }));
        let e = ask(sse(&events)).await.unwrap_err();
        let f = e.downcast_ref::<Failed>().expect("failed");
        assert_eq!((f.usage["input_tokens"].as_u64(), f.code.as_str()), (Some(4312), "overloaded_error"));
        assert!(crate::retry::transient(&e).is_some());
        // Declined: said, with what it used, and not tried again.
        let mut events = a_reply()[..1].to_vec();
        events.extend([json!({ "type": "message_delta", "delta": { "stop_reason": "refusal" }, "usage": { "output_tokens": 3 } }), json!({ "type": "message_stop" })]);
        let e = ask(sse(&events)).await.unwrap_err();
        let f = e.downcast_ref::<Failed>().expect("failed");
        assert!(f.message.contains("declined") && f.usage["output_tokens"] == 3 && crate::retry::transient(&e).is_none(), "{e:#}");
        // At the length limit: its call isn't complete, so isn't run.
        let mut events = a_reply()[..13].to_vec();
        events.extend([json!({ "type": "message_delta", "delta": { "stop_reason": "max_tokens" }, "usage": { "output_tokens": 64000 } }), json!({ "type": "message_stop" })]);
        let (message, _) = ask(sse(&events)).await.unwrap();
        assert_eq!(message["stopReason"], "length");
        // Closed partway: dropped, with what it said, and tried again.
        let e = ask(sse(&a_reply()[..9])).await.unwrap_err();
        let d = e.downcast_ref::<Dropped>().expect("dropped");
        assert_eq!(d.1["output_tokens"].as_u64(), Some(1));
        assert!(crate::retry::transient(&e).is_some());
    }

    #[test]
    fn redacted_thinking_goes_back_as_it_came() {
        let mut r = Reply { provider: "anthropic".into(), model: "m".into(), ..Reply::default() };
        let mut out = Vec::new();
        r.take(&json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "redacted_thinking", "data": "opaque" } }), &mut out);
        r.take(&json!({ "type": "content_block_stop", "index": 0 }), &mut out);
        let message = json!({ "role": "assistant", "content": r.content() });
        assert_eq!(messages(&[message], "anthropic", "m", true, true)[0]["content"][0], json!({ "type": "redacted_thinking", "data": "opaque" }));
        assert_eq!(call_id("call.1|fc"), "call_1");
        assert_eq!(call_id(""), "call");
    }
}
