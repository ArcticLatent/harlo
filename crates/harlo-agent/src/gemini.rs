//! Google's Gemini API (`streamGenerateContent`, server-sent events), by an
//! AI Studio key. The reply comes as pi's stream events and is kept as
//! pi's assistant message, like the other APIs'.
//!
//! Gemini's thinking is bound to its thought signatures: a part may carry
//! one (a function call, a thought, the reply's last text), and each goes
//! back in its part, as it came, to the model that made it. Calls another
//! model made carry the placeholder Google gives for them instead. The
//! server caches what requests share by itself.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use anyhow::Result;
use serde_json::{Value, json};

use crate::responses::{ApiError, Dropped, Events, Failed, client, refusal_text, send};

/// What a function call from elsewhere carries in place of a signature.
const NO_SIGNATURE: &str = "skip_thought_signature_validator";

/// A request's body. `effort`: the thinking level (`low`, `high`, …), sent
/// as Gemini's name for it; none: the model's default.
pub fn request(instructions: &str, contents: &[Value], tools: &Value, effort: Option<&str>, max_tokens: u64) -> Value {
    let mut body = json!({
        "contents": contents,
        "systemInstruction": { "parts": [{ "text": instructions }] },
        "generationConfig": { "maxOutputTokens": max_tokens, "thinkingConfig": { "includeThoughts": true } },
    });
    let functions: Vec<Value> = tools.as_array().into_iter().flatten().map(|t| json!({ "name": t["name"], "description": t["description"], "parametersJsonSchema": t["parameters"] })).collect();
    if !functions.is_empty() {
        body["tools"] = json!([{ "functionDeclarations": functions }]);
        body["toolConfig"] = json!({ "functionCallingConfig": { "mode": "AUTO" } });
    }
    if let Some(e) = effort.filter(|e| !e.is_empty()) {
        body["generationConfig"]["thinkingConfig"]["thinkingLevel"] = json!(e.to_ascii_uppercase());
    }
    body
}

/// What a block's signature says (as kept in pi's message): whose it is,
/// its signature, and a call's id from the server.
fn signature(provider: &str, model: &str, sig: Option<&str>, id: Option<&str>) -> String {
    json!({ "gemini": provider, "model": model, "signature": sig, "id": id }).to_string()
}

/// A kept signature, when `provider`'s `model` made it.
fn own(kept: &Value, provider: &str, model: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(kept.as_str()?).ok()?;
    (v["gemini"] == provider && v["model"] == model).then_some(v)
}

/// pi's messages as Gemini's `contents`: user and model turns, one after
/// the other (a run of either joined), tool results as function responses
/// with their images. Thinking and signatures go back only to the
/// `provider` and `model` that made them. `images` false (`model` doesn't
/// take them): a note in their place.
pub fn contents(history: &[Value], provider: &str, model: &str, images: bool) -> Vec<Value> {
    let image = |b: &Value| image_part(b, model, images);
    let parts = |content: &Value| -> Vec<Value> {
        match content {
            Value::String(s) if !s.is_empty() => vec![json!({ "text": s })],
            Value::Array(blocks) => blocks
                .iter()
                .filter_map(|b| match b["type"].as_str() {
                    Some("image") => Some(image(b)),
                    None | Some("text") => b["text"].as_str().filter(|t| !t.is_empty()).map(|t| json!({ "text": t })),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    let add = |out: &mut Vec<Value>, role: &str, more: Vec<Value>| {
        if more.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last["role"] == role => {
                if let Some(p) = last["parts"].as_array_mut() {
                    p.extend(more);
                }
            }
            _ => out.push(json!({ "role": role, "parts": more })),
        }
    };
    let mut out: Vec<Value> = Vec::new();
    for m in history {
        match m["role"].as_str() {
            Some("compactionSummary") => add(&mut out, "user", vec![json!({ "text": crate::compact::replayed(m) })]),
            Some("user" | "custom") => add(&mut out, "user", parts(&m["content"])),
            Some("assistant") => add(&mut out, "model", model_parts(m, provider, model)),
            Some("toolResult") => {
                let part = json!({ "functionResponse": function_response(m, model, images) });
                // (Results first in their turn, before what the user said
                // after the last round.)
                match out.last_mut() {
                    Some(last) if last["role"] == "user" => {
                        if let Some(p) = last["parts"].as_array_mut() {
                            let at = p.iter().take_while(|x| x["functionResponse"].is_object()).count();
                            p.insert(at, part);
                        }
                    }
                    _ => out.push(json!({ "role": "user", "parts": [part] })),
                }
            }
            _ => {}
        }
    }
    out
}

/// An assistant message's blocks as a model turn's parts (see [`contents`]).
fn model_parts(m: &Value, provider: &str, model: &str) -> Vec<Value> {
    let mut more = Vec::new();
        for b in m["content"].as_array().into_iter().flatten() {
            match b["type"].as_str() {
                Some("thinking") => {
                    let Some(sig) = own(&b["thinkingSignature"], provider, model) else { continue };
                    let mut part = json!({ "text": b["thinking"].as_str().unwrap_or_default(), "thought": true });
                    if let Some(s) = sig["signature"].as_str() {
                        part["thoughtSignature"] = json!(s);
                    }
                    more.push(part);
                }
                Some("text") => {
                    let text = b["text"].as_str().unwrap_or_default();
                    let sig = own(&b["textSignature"], provider, model).and_then(|v| v["signature"].as_str().map(str::to_owned));
                    if text.is_empty() && sig.is_none() {
                        continue;
                    }
                    let mut part = json!({ "text": text });
                    if let Some(s) = sig {
                        part["thoughtSignature"] = json!(s);
                    }
                    more.push(part);
                }
                Some("toolCall") => {
                    let args = if b["arguments"].is_object() { b["arguments"].clone() } else { json!({}) };
                    let mut call = json!({ "name": b["name"], "args": args });
                    let sig = own(&b["callSignature"], provider, model);
                    if let Some(id) = sig.as_ref().and_then(|v| v["id"].as_str()) {
                        call["id"] = json!(id);
                    }
                    let mut part = json!({ "functionCall": call });
                    match sig {
                        Some(v) => {
                            if let Some(s) = v["signature"].as_str() {
                                part["thoughtSignature"] = json!(s);
                            }
                        }
                        // (Another model's call: Gemini asks a
                        // signature of the first in a turn.)
                        None => part["thoughtSignature"] = json!(NO_SIGNATURE),
                    }
                    more.push(part);
                }
                _ => {}
            }
        }
    more
}

/// An image as an inline part (`images` false: a note in its place).
fn image_part(b: &Value, model: &str, images: bool) -> Value {
    if images {
        json!({ "inlineData": { "mimeType": b["mimeType"].as_str().unwrap_or("image/png"), "data": b["data"] } })
    } else {
        json!({ "text": crate::images::not_sent(model) })
    }
}

/// A tool's result as a function response, its images in it (`images`
/// false: a note in their place).
fn function_response(m: &Value, model: &str, images: bool) -> Value {
    let text: String = m["content"].as_array().map_or_else(
        || m["content"].as_str().unwrap_or_default().to_owned(),
        |bs| bs.iter().filter(|b| b["type"].as_str().is_none_or(|t| t == "text")).filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
    );
    let key = if m["isError"] == true { "error" } else { "output" };
    let mut response = json!({ "name": m["toolName"], "response": { key: text } });
    // (The call's id, when its server gave one.)
    let id = m["toolCallId"].as_str().unwrap_or_default();
    if let Some(server) = id.strip_prefix("gemini:") {
        response["id"] = json!(server);
    }
    let pictures: Vec<Value> = m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "image").map(|b| image_part(b, model, images)).collect();
    if images && !pictures.is_empty() {
        response["parts"] = json!(pictures);
    } else if !pictures.is_empty() {
        let note = crate::images::not_sent(model);
        let r = &mut response["response"][key];
        *r = json!(format!("{}\n{note}", r.as_str().unwrap_or_default()));
    }
    response
}

/// A block of the reply as it builds.
#[derive(Debug)]
enum Block {
    /// Its text, and its signature (when one came with it).
    Text(String, Option<String>),
    Thought(String, Option<String>),
    Call { id: String, server_id: Option<String>, name: String, args: Value, signature: Option<String> },
}

/// The reply as it streams, made into pi's events and message.
#[derive(Default)]
struct Reply {
    blocks: Vec<Block>,
    /// The text or thought block still growing.
    open: Option<usize>,
    usage: Value,
    finish: Option<String>,
    /// Why the prompt was refused, if it was.
    blocked: Option<String>,
    calls: u64,
}

impl Reply {
    /// A chunk's events.
    fn take(&mut self, chunk: &Value, out: &mut Vec<Value>) {
        if let Some(u) = chunk["usageMetadata"].as_object() {
            if !self.usage.is_object() {
                self.usage = json!({});
            }
            for (k, v) in u {
                if v.is_u64() {
                    self.usage[k] = v.clone();
                }
            }
        }
        if let Some(r) = chunk["promptFeedback"]["blockReason"].as_str() {
            self.blocked = Some(r.to_owned());
        }
        let Some(c) = chunk["candidates"].get(0) else { return };
        for part in c["content"]["parts"].as_array().into_iter().flatten() {
            let sig = part["thoughtSignature"].as_str().map(str::to_owned);
            if let Some(call) = part["functionCall"].as_object() {
                self.close(out);
                let at = self.blocks.len();
                let server_id = call.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
                self.calls += 1;
                // (The server's id, marked, so its result can name it; else
                // one of ours.)
                let id = server_id.as_ref().map_or_else(|| format!("call_{}_{}", crate::session::now_ms(), self.calls), |s| format!("gemini:{s}"));
                let name = call.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
                let args = call.get("args").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
                out.push(json!({ "type": "toolcall_start", "contentIndex": at, "id": id, "toolName": name }));
                out.push(json!({ "type": "toolcall_delta", "contentIndex": at, "delta": args.to_string() }));
                self.blocks.push(Block::Call { id, server_id, name, args, signature: sig });
            } else if let Some(text) = part["text"].as_str() {
                self.grow(part["thought"] == true, text, sig, out);
            }
        }
        if let Some(f) = c["finishReason"].as_str() {
            self.finish = Some(f.to_owned());
        }
    }

    /// Text or a thought: the open block of that kind grows (taking the
    /// part's signature), or a new one starts.
    fn grow(&mut self, thought: bool, delta: &str, sig: Option<String>, out: &mut Vec<Value>) {
        let same = self.open.is_some_and(|at| matches!((&self.blocks[at], thought), (Block::Thought(..), true) | (Block::Text(..), false)));
        if !same {
            if delta.is_empty() && sig.is_some() {
                // (A signature on its own: the block it ends.)
                if let Some(Block::Text(_, s) | Block::Thought(_, s)) = self.open.map(|at| &mut self.blocks[at]) {
                    *s = sig;
                    return;
                }
            }
            self.close(out);
            let at = self.blocks.len();
            self.blocks.push(if thought { Block::Thought(String::new(), None) } else { Block::Text(String::new(), None) });
            self.open = Some(at);
            out.push(json!({ "type": if thought { "thinking_start" } else { "text_start" }, "contentIndex": at }));
        }
        let at = self.open.expect("open");
        if let Block::Thought(s, keep) | Block::Text(s, keep) = &mut self.blocks[at] {
            s.push_str(delta);
            if sig.is_some() {
                *keep = sig;
            }
        }
        if !delta.is_empty() {
            out.push(json!({ "type": if thought { "thinking_delta" } else { "text_delta" }, "contentIndex": at, "delta": delta }));
        }
    }

    /// Ends the growing text or thought block.
    fn close(&mut self, out: &mut Vec<Value>) {
        if let Some(at) = self.open.take() {
            match &self.blocks[at] {
                Block::Thought(s, _) => out.push(json!({ "type": "thinking_end", "contentIndex": at, "content": s })),
                Block::Text(s, _) => out.push(json!({ "type": "text_end", "contentIndex": at, "content": s })),
                Block::Call { .. } => {}
            }
        }
    }

    /// The blocks as pi's content; the calls' ends.
    fn finish(&mut self, provider: &str, model: &str, out: &mut Vec<Value>) -> Vec<Value> {
        self.close(out);
        let mut content = Vec::new();
        for (at, b) in self.blocks.iter().enumerate() {
            match b {
                Block::Text(s, sig) => content.push(json!({ "type": "text", "text": s, "textSignature": signature(provider, model, sig.as_deref(), None) })),
                Block::Thought(s, sig) => content.push(json!({ "type": "thinking", "thinking": s, "thinkingSignature": signature(provider, model, sig.as_deref(), None) })),
                Block::Call { id, server_id, name, args, signature: sig } => {
                    let call = json!({ "type": "toolCall", "id": id, "name": name, "arguments": args, "callSignature": signature(provider, model, sig.as_deref(), server_id.as_deref()) });
                    out.push(json!({ "type": "toolcall_end", "contentIndex": at, "toolCall": call }));
                    content.push(call);
                }
            }
        }
        content
    }
}

/// The usage as the Responses API says it (what the rest reads): the
/// prompt with its cached part, the output with the thinking in it; what
/// wasn't said is left out.
fn usage(u: &Value) -> Value {
    let mut out = json!({});
    if let Some(n) = u["promptTokenCount"].as_u64() {
        out["input_tokens"] = json!(n + u["toolUsePromptTokenCount"].as_u64().unwrap_or(0));
        out["input_tokens_details"] = json!({ "cached_tokens": u["cachedContentTokenCount"].as_u64().unwrap_or(0) });
    }
    let thoughts = u["thoughtsTokenCount"].as_u64();
    if let Some(n) = u["candidatesTokenCount"].as_u64().or(thoughts.map(|_| 0)) {
        out["output_tokens"] = json!(n + thoughts.unwrap_or(0));
        out["output_tokens_details"] = json!({ "reasoning_tokens": thoughts.unwrap_or(0) });
    }
    out
}

/// What was said of the usage; Null when nothing was.
fn said(u: &Value) -> Value {
    if u.as_object().is_some_and(|o| !o.is_empty()) { usage(u) } else { Value::Null }
}

/// Sends one request to `model`: `on` gets pi's stream events as they
/// come; the reply is pi's assistant message, with its usage as the
/// Responses API says it. `seen`: what it says it used as it goes.
///
/// # Errors
///
/// [`ApiError`] when the server refuses the request; [`Failed`] when the
/// reply ends in an error or is blocked; [`Dropped`] when the server can't
/// be reached or the reply stops partway. The last two carry what the
/// server said the request used.
pub async fn stream(to: crate::chat::Endpoint<'_>, body: &Value, provider: &str, model: &str, mut on: impl FnMut(&Value), seen: &mut Value) -> Result<(Value, Value)> {
    let base = to.base;
    let mut req = client().post(format!("{base}/models/{model}:streamGenerateContent?alt=sse")).json(body);
    if let Some(k) = to.key {
        req = req.header("x-goog-api-key", k);
    }
    for (k, v) in to.headers {
        req = req.header(*k, v);
    }
    let resp = send(req, base).await?;
    let status = resp.status();
    let retry_after = crate::retry::retry_after(resp.headers());
    if !status.is_success() {
        let text = refusal_text(resp).await;
        // (One error, or a list of them.)
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let e = if body.is_array() { &body[0]["error"] } else { &body["error"] };
        let said = e["message"].as_str().map_or_else(|| text.chars().take(300).collect(), str::to_owned);
        let code = e["status"].as_str().or_else(|| e["code"].as_str()).unwrap_or_default().to_owned();
        return Err(ApiError { status: status.as_u16(), message: format!("{provider} refused the request ({status}): {said}"), code, retry_after, resets_at: None }.into());
    }
    let mut reply = Reply::default();
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
        let Ok(chunk) = serde_json::from_str::<Value>(&data) else { continue };
        if chunk["error"].is_object() {
            let e = &chunk["error"];
            let code = e["status"].as_str().unwrap_or_default().to_owned();
            return Err(Failed { code, message: format!("{provider}: {}", e["message"].as_str().unwrap_or("the request failed")), usage: said(&reply.usage) }.into());
        }
        reply.take(&chunk, &mut events);
        if chunk["usageMetadata"].is_object() {
            *seen = said(&reply.usage);
        }
        for e in events.drain(..) {
            on(&e);
        }
    }
    let usage = usage(&reply.usage);
    if let Some(why) = &reply.blocked {
        return Err(Failed { code: "blocked".into(), message: format!("{provider} blocked the request ({why})"), usage }.into());
    }
    let Some(finish) = reply.finish.clone() else {
        return Err(Dropped("The connection closed before the reply finished".into(), said(&reply.usage)).into());
    };
    let content = reply.finish(provider, model, &mut events);
    for e in events.drain(..) {
        on(&e);
    }
    let calls = content.iter().any(|b| b["type"] == "toolCall");
    let stop = match finish.as_str() {
        "STOP" | "FINISH_REASON_UNSPECIFIED" if calls => "toolUse",
        "STOP" | "FINISH_REASON_UNSPECIFIED" => "stop",
        "MAX_TOKENS" => "length",
        other => {
            // (Declined, or the call it tried to make broke: said, with
            // what it used.)
            let mut message = format!("{provider} stopped the reply ({other})");
            let text: String = content.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
            if !text.trim().is_empty() {
                let _ = write!(message, ", after: {}", text.trim().chars().take(300).collect::<String>());
            }
            return Err(Failed { code: other.to_ascii_lowercase(), message, usage }.into());
        }
    };
    let message = json!({
        "role": "assistant",
        "content": content,
        "api": "google-generative-ai",
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

    /// A stand-in Gemini server: it keeps each request as it came (head and
    /// body) and answers with `answer` (a whole HTTP response).
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
        (format!("http://{addr}/v1beta"), rx)
    }

    fn sse(chunks: &[Value]) -> String {
        let mut out = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n".to_owned();
        for c in chunks {
            let _ = write!(out, "data: {c}\r\n\r\n");
        }
        out
    }

    fn parts(parts: &Value) -> Value {
        json!({ "candidates": [{ "content": { "role": "model", "parts": parts }, "index": 0 }] })
    }

    /// A reply: a thought summary in pieces, text, two calls (the first
    /// signed, as Gemini signs a turn's first), the usage.
    fn a_reply() -> Vec<Value> {
        vec![
            parts(&json!([{ "text": "Read it ", "thought": true }])),
            parts(&json!([{ "text": "first.", "thought": true }])),
            parts(&json!([{ "text": "Reading." }])),
            parts(&json!([{ "functionCall": { "name": "read", "args": { "path": "a.txt" } }, "thoughtSignature": "SIG_A" }, { "functionCall": { "id": "fc-2", "name": "ls", "args": {} } }])),
            json!({ "candidates": [{ "content": { "role": "model", "parts": [] }, "finishReason": "STOP", "index": 0 }],
                   "usageMetadata": { "promptTokenCount": 5000, "cachedContentTokenCount": 4096, "candidatesTokenCount": 40, "thoughtsTokenCount": 60, "totalTokenCount": 5100 } }),
        ]
    }

    fn history() -> Vec<Value> {
        let img = json!({ "type": "image", "data": "QUJD", "mimeType": "image/png" });
        let sig = |p: &str, m: &str, s: Option<&str>, id: Option<&str>| signature(p, m, s, id);
        vec![
            json!({ "role": "user", "content": [{ "type": "text", "text": "look" }, img.clone()] }),
            json!({ "role": "assistant", "content": [
                { "type": "thinking", "thinking": "Read it first.", "thinkingSignature": sig("gemini", "g", None, None) },
                { "type": "thinking", "thinking": "theirs", "thinkingSignature": "{\"type\":\"reasoning\"}" },
                { "type": "text", "text": "Reading.", "textSignature": sig("gemini", "g", Some("SIG_T"), None) },
                { "type": "toolCall", "id": "call_1", "name": "read", "arguments": { "path": "a.txt" }, "callSignature": sig("gemini", "g", Some("SIG_A"), None) },
                { "type": "toolCall", "id": "gemini:fc-2", "name": "shot", "arguments": {}, "callSignature": sig("gemini", "g", None, Some("fc-2")) },
            ] }),
            json!({ "role": "toolResult", "toolCallId": "call_1", "toolName": "read", "content": [{ "type": "text", "text": "A" }], "isError": false }),
            json!({ "role": "custom", "customType": "turn_stopped", "content": "stopped", "display": false }),
            json!({ "role": "toolResult", "toolCallId": "gemini:fc-2", "toolName": "shot", "content": [{ "type": "text", "text": "[image 1]" }, img], "isError": true }),
            json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": "call_x|fc_x", "name": "ls", "arguments": {} }] }),
        ]
    }

    #[test]
    fn history_as_contents() {
        let sent = contents(&history(), "gemini", "g", true);
        let image = json!({ "inlineData": { "mimeType": "image/png", "data": "QUJD" } });
        assert_eq!(sent, vec![
            json!({ "role": "user", "parts": [{ "text": "look" }, image] }),
            json!({ "role": "model", "parts": [
                { "text": "Read it first.", "thought": true },
                { "text": "Reading.", "thoughtSignature": "SIG_T" },
                { "functionCall": { "name": "read", "args": { "path": "a.txt" } }, "thoughtSignature": "SIG_A" },
                { "functionCall": { "name": "shot", "args": {}, "id": "fc-2" } },
            ] }),
            json!({ "role": "user", "parts": [
                { "functionResponse": { "name": "read", "response": { "output": "A" } } },
                { "functionResponse": { "name": "shot", "response": { "error": "[image 1]" }, "id": "fc-2", "parts": [image] } },
                { "text": "stopped" },
            ] }),
            json!({ "role": "model", "parts": [{ "functionCall": { "name": "ls", "args": {} }, "thoughtSignature": NO_SIGNATURE }] }),
        ]);
        // Another model: its thinking and signatures stay out; its calls get the placeholder.
        let other = contents(&history(), "gemini", "h", false);
        assert_eq!(other[1]["parts"][0], json!({ "text": "Reading." }));
        assert_eq!(other[1]["parts"][1]["thoughtSignature"], NO_SIGNATURE);
        assert_eq!(other[2]["parts"][1]["functionResponse"]["response"]["error"], format!("[image 1]\n{}", crate::images::not_sent("h")));
    }

    #[test]
    fn a_request_body() {
        let tools = json!([{ "name": "read", "description": "Read a file", "parameters": { "type": "object", "additionalProperties": false } }]);
        let body = request("Be brief.", &[json!({ "role": "user", "parts": [{ "text": "hi" }] })], &tools, Some("high"), 65536);
        assert_eq!(body.to_string(), concat!(
            r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}],"systemInstruction":{"parts":[{"text":"Be brief."}]},"#,
            r#""generationConfig":{"maxOutputTokens":65536,"thinkingConfig":{"includeThoughts":true,"thinkingLevel":"HIGH"}},"#,
            r#""tools":[{"functionDeclarations":[{"name":"read","description":"Read a file","parametersJsonSchema":{"type":"object","additionalProperties":false}}]}],"#,
            r#""toolConfig":{"functionCallingConfig":{"mode":"AUTO"}}}"#,
        ));
        let plain = request("", &[], &json!([]), None, 100);
        assert!(plain.get("tools").is_none() && plain["generationConfig"]["thinkingConfig"].get("thinkingLevel").is_none());
    }

    /// The request as it reaches the server, and the reply as pi's events
    /// and message; what came back goes back as it came.
    #[tokio::test]
    async fn a_round_against_a_stand_in() {
        let (base, mut got) = stand_in(sse(&a_reply())).await;
        let body = request("Be brief.", &[json!({ "role": "user", "parts": [{ "text": "hi" }] })], &json!([]), Some("low"), 1000);
        let mut events = Vec::new();
        let mut seen = Value::Null;
        let (message, usage) = stream(crate::chat::Endpoint { base: &base, key: Some("AIza-test"), headers: &[] }, &body, "gemini", "g", |e| events.push(e.clone()), &mut seen).await.unwrap();
        let req = got.recv().await.unwrap();
        let (head, raw) = req.split_once("\r\n\r\n").unwrap();
        let head = head.to_ascii_lowercase();
        assert!(head.starts_with("post /v1beta/models/g:streamgeneratecontent?alt=sse http/1.1"), "{head}");
        assert!(head.contains("x-goog-api-key: aiza-test") && !head.contains("authorization"), "{head}");
        assert_eq!(raw, body.to_string());

        let content = message["content"].as_array().unwrap();
        assert_eq!((content[0]["thinking"].as_str(), content[1]["text"].as_str()), (Some("Read it first."), Some("Reading.")));
        assert_eq!((content[2]["name"].as_str(), &content[2]["arguments"]), (Some("read"), &json!({ "path": "a.txt" })));
        assert_eq!(content[3]["id"], "gemini:fc-2");
        assert_eq!(message["stopReason"], "toolUse");
        let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["thinking_start", "thinking_delta", "thinking_delta", "thinking_end", "text_start", "text_delta", "text_end", "toolcall_start", "toolcall_delta", "toolcall_start", "toolcall_delta", "toolcall_end", "toolcall_end"]);
        assert_eq!(usage, json!({ "input_tokens": 5000, "input_tokens_details": { "cached_tokens": 4096 }, "output_tokens": 100, "output_tokens_details": { "reasoning_tokens": 60 } }));
        assert_eq!(seen, usage);
        let again = contents(&[message], "gemini", "g", true);
        assert_eq!(again[0]["parts"][2], json!({ "functionCall": { "name": "read", "args": { "path": "a.txt" } }, "thoughtSignature": "SIG_A" }));
        assert_eq!(again[0]["parts"][3], json!({ "functionCall": { "name": "ls", "args": {}, "id": "fc-2" } }));
    }

    #[test]
    fn a_signature_on_its_own_ends_its_block() {
        let mut r = Reply::default();
        let mut out = Vec::new();
        r.take(&parts(&json!([{ "text": "Done" }])), &mut out);
        r.take(&parts(&json!([{ "text": ".", "thoughtSignature": "S1" }])), &mut out);
        r.take(&parts(&json!([{ "text": "", "thoughtSignature": "S2" }])), &mut out);
        let content = r.finish("gemini", "g", &mut out);
        assert_eq!(content.len(), 1);
        assert_eq!(contents(&[json!({ "role": "assistant", "content": content })], "gemini", "g", true)[0]["parts"], json!([{ "text": "Done.", "thoughtSignature": "S2" }]));
    }

    #[tokio::test]
    async fn errors_blocks_and_cut_offs() {
        let ask = |answer: String| async move {
            crate::responses::TEST_IDLE.set(Some(std::time::Duration::from_secs(1)));
            let (base, _got) = stand_in(answer).await;
            stream(crate::chat::Endpoint { base: &base, key: Some("k"), headers: &[] }, &json!({}), "gemini", "g", |_| {}, &mut Value::Null).await
        };
        let http = |status: &str, body: &str| format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        // Busy or rate-limited: tried again.
        for (status, body) in [("503 Service Unavailable", r#"{"error":{"code":503,"message":"The model is overloaded.","status":"UNAVAILABLE"}}"#), ("429 Too Many Requests", r#"[{"error":{"code":429,"message":"Quota exceeded","status":"RESOURCE_EXHAUSTED"}}]"#)] {
            let e = ask(http(status, body)).await.unwrap_err();
            assert!(crate::retry::transient(&e).is_some(), "{e:#}");
        }
        // Too long: compacted, not retried.
        let body = r#"{"error":{"code":400,"message":"The input token count (1100000) exceeds the maximum number of tokens allowed (1048576).","status":"INVALID_ARGUMENT"}}"#;
        let e = ask(http("400 Bad Request", body)).await.unwrap_err();
        assert!(crate::compact::overflowed(&e) && crate::retry::transient(&e).is_none(), "{e:#}");
        // Blocked or declined: said, with what it used.
        let blocked = json!({ "promptFeedback": { "blockReason": "PROHIBITED_CONTENT" }, "usageMetadata": { "promptTokenCount": 10 } });
        let e = ask(sse(&[blocked])).await.unwrap_err();
        assert!(e.to_string().contains("PROHIBITED_CONTENT") && e.downcast_ref::<Failed>().unwrap().usage["input_tokens"] == 10, "{e:#}");
        let safety = json!({ "candidates": [{ "content": { "parts": [{ "text": "Sure" }] }, "finishReason": "SAFETY" }], "usageMetadata": { "promptTokenCount": 10, "candidatesTokenCount": 1 } });
        let e = ask(sse(&[safety])).await.unwrap_err();
        assert!(e.to_string().contains("SAFETY") && crate::retry::transient(&e).is_none(), "{e:#}");
        // At the length limit.
        let long = json!({ "candidates": [{ "content": { "parts": [{ "text": "and so" }] }, "finishReason": "MAX_TOKENS" }] });
        assert_eq!(ask(sse(&[long])).await.unwrap().0["stopReason"], "length");
        // Closed partway: dropped, with what it said, and tried again.
        let mut part = a_reply()[..2].to_vec();
        part[1]["usageMetadata"] = json!({ "promptTokenCount": 5000 });
        let e = ask(sse(&part)).await.unwrap_err();
        assert_eq!(e.downcast_ref::<Dropped>().expect("dropped").1["input_tokens"].as_u64(), Some(5000));
        assert!(crate::retry::transient(&e).is_some());
    }
}
