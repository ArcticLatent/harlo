//! OpenAI-compatible chat completions, streamed: what most servers speak
//! (llama.cpp, Ollama, vLLM, LM Studio, OpenRouter, …). The reply comes as
//! pi's stream events and is kept as pi's assistant message, like the
//! Responses API's.

use anyhow::Result;
use serde_json::{Value, json};

use crate::responses::{ApiError, Dropped, Events, Failed, client, refusal_text, send};

/// A request's body.
pub fn request(model: &str, instructions: &str, messages: &[Value], tools: &Value, effort: Option<&str>) -> Value {
    let mut all = vec![json!({ "role": "system", "content": instructions })];
    all.extend(messages.iter().cloned());
    let mut body = json!({ "model": model, "messages": all, "stream": true, "stream_options": { "include_usage": true } });
    let functions: Vec<Value> = tools
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| json!({ "type": "function", "function": { "name": t["name"], "description": t["description"], "parameters": t["parameters"] } }))
        .collect();
    if !functions.is_empty() {
        body["tools"] = json!(functions);
        body["tool_choice"] = json!("auto");
    }
    if let Some(e) = effort {
        body["reasoning_effort"] = json!(e);
    }
    body
}

/// pi's messages as chat messages. Thinking isn't sent back (servers
/// differ on whether they take it).
/// pi's messages as Chat Completions'. A tool's images follow its round's
/// results, as the user's; `images` false (`model` doesn't take them): a
/// note in their place.
pub fn messages(history: &[Value], model: &str, images: bool) -> Vec<Value> {
    let text_of = |content: &Value| -> String {
        match content {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts.iter().filter(|p| p["type"].as_str().is_none_or(|t| t == "text")).filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        }
    };
    let image = |b: &Value| {
        if images { json!({ "type": "image_url", "image_url": { "url": crate::images::data_url(b) } }) } else { json!({ "type": "text", "text": crate::images::not_sent(model) }) }
    };
    // (A round's tool images, shown once its results are in.)
    let mut shown: Vec<Value> = Vec::new();
    let mut out = Vec::new();
    for m in history {
        if m["role"] != "toolResult" && !shown.is_empty() {
            out.push(json!({ "role": "user", "content": std::mem::take(&mut shown) }));
        }
        match m["role"].as_str() {
            Some("compactionSummary") => out.push(json!({ "role": "user", "content": crate::compact::replayed(m) })),
            Some("user") | Some("custom") if crate::images::has_image(m) => {
                let parts: Vec<Value> = m["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|b| match b["type"].as_str() {
                        Some("image") => Some(image(b)),
                        _ => b["text"].as_str().filter(|t| !t.is_empty()).map(|t| json!({ "type": "text", "text": t })),
                    })
                    .collect();
                out.push(json!({ "role": "user", "content": parts }));
            }
            Some("user") | Some("custom") => {
                let text = text_of(&m["content"]);
                if !text.is_empty() {
                    out.push(json!({ "role": "user", "content": text }));
                }
            }
            Some("assistant") => {
                let blocks = m["content"].as_array().cloned().unwrap_or_default();
                let text: Vec<&str> = blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
                let calls: Vec<Value> = blocks
                    .iter()
                    .filter(|b| b["type"] == "toolCall")
                    .map(|b| {
                        let id = b["id"].as_str().unwrap_or_default();
                        json!({ "id": id.split_once('|').map_or(id, |(c, _)| c), "type": "function", "function": { "name": b["name"], "arguments": b["arguments"].to_string() } })
                    })
                    .collect();
                if text.is_empty() && calls.is_empty() {
                    continue;
                }
                let mut msg = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text.join("")) } });
                if !calls.is_empty() {
                    msg["tool_calls"] = json!(calls);
                }
                out.push(msg);
            }
            Some("toolResult") => {
                let id = m["toolCallId"].as_str().unwrap_or_default();
                let call_id = id.split_once('|').map_or(id, |(c, _)| c);
                let pictures: Vec<&Value> = m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "image").collect();
                let mut content = text_of(&m["content"]);
                if !pictures.is_empty() {
                    if images {
                        let n = pictures.len();
                        content.push_str(&format!("\n[{} shown after this round's results.]", if n == 1 { "Its image is".to_owned() } else { format!("Its {n} images are") }));
                        shown.push(json!({ "type": "text", "text": format!("Images from {} (call {call_id}):", m["toolName"].as_str().unwrap_or("a tool")) }));
                        shown.extend(pictures.into_iter().map(image));
                    } else {
                        content.push_str(&format!("\n{}", crate::images::not_sent(model)));
                    }
                }
                out.push(json!({ "role": "tool", "tool_call_id": call_id, "content": content }));
            }
            _ => {}
        }
    }
    if !shown.is_empty() {
        out.push(json!({ "role": "user", "content": shown }));
    }
    out
}

/// A block of the reply as it builds.
#[derive(Debug)]
enum Block {
    Text(String),
    Thinking(String),
    Call { id: String, name: String, args: String },
}

/// The reply as it streams, made into pi's events and message.
#[derive(Default)]
struct Reply {
    blocks: Vec<Block>,
    /// The text or thinking block still growing.
    open: Option<usize>,
    /// A call's stream index → its block.
    calls: std::collections::HashMap<u64, usize>,
    usage: Value,
    finish: Option<String>,
}

impl Reply {
    /// A chunk's events.
    fn take(&mut self, chunk: &Value, out: &mut Vec<Value>) {
        if chunk["usage"].is_object() {
            self.usage = chunk["usage"].clone();
        }
        let Some(choice) = chunk["choices"].get(0) else { return };
        let d = &choice["delta"];
        let thinking = d["reasoning_content"].as_str().or_else(|| d["reasoning"].as_str()).filter(|s| !s.is_empty());
        if let Some(t) = thinking {
            self.grow(true, t, out);
        }
        if let Some(t) = d["content"].as_str().filter(|s| !s.is_empty()) {
            self.grow(false, t, out);
        }
        for c in d["tool_calls"].as_array().into_iter().flatten() {
            let index = c["index"].as_u64().unwrap_or(0);
            let at = match self.calls.get(&index) {
                Some(at) => *at,
                None => {
                    self.close(out);
                    let at = self.blocks.len();
                    let id = c["id"].as_str().filter(|s| !s.is_empty()).map_or_else(|| format!("call_{}", crate::session::now_ms() + index), str::to_owned);
                    let name = c["function"]["name"].as_str().unwrap_or_default().to_owned();
                    out.push(json!({ "type": "toolcall_start", "contentIndex": at, "id": id, "toolName": name }));
                    self.blocks.push(Block::Call { id, name, args: String::new() });
                    self.calls.insert(index, at);
                    at
                }
            };
            if let Block::Call { name, args, .. } = &mut self.blocks[at] {
                if name.is_empty()
                    && let Some(n) = c["function"]["name"].as_str()
                {
                    *name = n.to_owned();
                }
                if let Some(a) = c["function"]["arguments"].as_str().filter(|a| !a.is_empty()) {
                    args.push_str(a);
                    out.push(json!({ "type": "toolcall_delta", "contentIndex": at, "delta": a }));
                }
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            self.finish = Some(f.to_owned());
        }
    }

    /// Text or thinking: the open block of that kind grows, or a new one starts.
    fn grow(&mut self, thinking: bool, delta: &str, out: &mut Vec<Value>) {
        let same = self.open.is_some_and(|at| matches!((&self.blocks[at], thinking), (Block::Thinking(_), true) | (Block::Text(_), false)));
        if !same {
            self.close(out);
            let at = self.blocks.len();
            self.blocks.push(if thinking { Block::Thinking(String::new()) } else { Block::Text(String::new()) });
            self.open = Some(at);
            out.push(json!({ "type": if thinking { "thinking_start" } else { "text_start" }, "contentIndex": at }));
        }
        let at = self.open.expect("open");
        match &mut self.blocks[at] {
            Block::Thinking(s) | Block::Text(s) => s.push_str(delta),
            Block::Call { .. } => {}
        }
        out.push(json!({ "type": if thinking { "thinking_delta" } else { "text_delta" }, "contentIndex": at, "delta": delta }));
    }

    /// Ends the growing text or thinking block.
    fn close(&mut self, out: &mut Vec<Value>) {
        if let Some(at) = self.open.take() {
            out.push(match &self.blocks[at] {
                Block::Thinking(s) => json!({ "type": "thinking_end", "contentIndex": at, "content": s }),
                Block::Text(s) => json!({ "type": "text_end", "contentIndex": at, "content": s }),
                Block::Call { .. } => return,
            });
        }
    }

    /// The blocks as pi's content; the calls' ends.
    fn finish(&mut self, out: &mut Vec<Value>) -> Vec<Value> {
        self.close(out);
        let mut content = Vec::new();
        for (at, b) in self.blocks.iter().enumerate() {
            match b {
                Block::Text(s) => content.push(json!({ "type": "text", "text": s })),
                Block::Thinking(s) => content.push(json!({ "type": "thinking", "thinking": s, "thinkingSignature": "reasoning_content" })),
                Block::Call { id, name, args } => {
                    let mut call = json!({ "type": "toolCall", "id": id, "name": name, "arguments": {} });
                    let raw = if args.trim().is_empty() { "{}" } else { args.as_str() };
                    match serde_json::from_str::<Value>(raw) {
                        Ok(v) if v.is_object() => call["arguments"] = v,
                        _ => call["argumentsError"] = json!(format!("The arguments weren't a JSON object: {raw}")),
                    }
                    out.push(json!({ "type": "toolcall_end", "contentIndex": at, "toolCall": call }));
                    content.push(call);
                }
            }
        }
        content
    }
}

/// The usage as the Responses API says it (what the rest reads).
fn usage(u: &Value) -> Value {
    json!({
        "input_tokens": u["prompt_tokens"].as_u64().unwrap_or(0),
        "output_tokens": u["completion_tokens"].as_u64().unwrap_or(0),
        "input_tokens_details": { "cached_tokens": u["prompt_tokens_details"]["cached_tokens"].as_u64().or_else(|| u["cached_tokens"].as_u64()).unwrap_or(0) },
        "output_tokens_details": { "reasoning_tokens": u["completion_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0) },
    })
}

/// Sends one request: `on` gets pi's stream events as they come; the
/// reply is pi's assistant message, with its usage as the Responses API
/// says it.
pub async fn stream(base: &str, token: Option<&str>, body: &Value, provider: &str, model: &str, mut on: impl FnMut(&Value)) -> Result<(Value, Value)> {
    let mut req = client().post(format!("{base}/chat/completions")).json(body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = send(req, base).await?;
    let status = resp.status();
    let retry_after = crate::retry::retry_after(resp.headers());
    if !status.is_success() {
        let text = refusal_text(resp).await;
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let said = body["error"]["message"].as_str().or_else(|| body["error"].as_str()).or_else(|| body["message"].as_str()).map(str::to_owned).unwrap_or_else(|| text.chars().take(300).collect());
        let code = body["error"]["type"].as_str().or_else(|| body["error"]["code"].as_str()).unwrap_or_default().to_owned();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let resets_at = body["error"]["resets_at"].as_i64().or_else(|| body["error"]["resets_in_seconds"].as_i64().map(|s| now + s));
        return Err(ApiError { status: status.as_u16(), message: format!("{provider} refused the request ({status}): {said}"), code, retry_after, resets_at }.into());
    }
    let mut reply = Reply::default();
    let mut events = Vec::new();
    let mut reader = Events::new(resp);
    let mut done = false;
    while let Some(data) = reader.next().await? {
        if data == "[DONE]" {
            done = true;
            break;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(&data) else { continue };
        if chunk["error"].is_object() || chunk["error"].is_string() {
            let e = &chunk["error"];
            let code = e["type"].as_str().or_else(|| e["code"].as_str()).unwrap_or_default().to_owned();
            return Err(Failed { code, message: format!("{provider}: {}", e["message"].as_str().or_else(|| e.as_str()).unwrap_or("the request failed")) }.into());
        }
        reply.take(&chunk, &mut events);
        for e in events.drain(..) {
            on(&e);
        }
    }
    if !done && reply.finish.is_none() {
        return Err(Dropped("The connection closed before the reply finished".into()).into());
    }
    let content = reply.finish(&mut events);
    for e in events.drain(..) {
        on(&e);
    }
    let usage = usage(&reply.usage);
    let calls = content.iter().any(|b| b["type"] == "toolCall");
    // (Cut off at the length limit, calls or not: its calls may be
    // incomplete, and aren't run.)
    let stop = match reply.finish.as_deref() {
        Some("length") => "length",
        _ if calls => "toolUse",
        _ => "stop",
    };
    let message = json!({
        "role": "assistant",
        "content": content,
        "api": "openai-completions",
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

    /// A stand-in model server that answers every request as `how` says:
    /// "hang" (never answers), "stall" (headers and one event, then
    /// nothing), "endless" (an event that never ends), "split" (a reply
    /// whose blank lines come split across writes), "429" (busy, asking
    /// for a 600 s wait).
    async fn stand_in(how: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let _ = s.read(&mut buf).await;
                    let sse = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                    let hello = "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"}}]}\n\n";
                    match how {
                        "hang" => tokio::time::sleep(std::time::Duration::from_secs(60)).await,
                        "stall" => {
                            let _ = s.write_all(format!("{sse}{hello}").as_bytes()).await;
                            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        }
                        "endless" => {
                            let _ = s.write_all(format!("{sse}data: \"").as_bytes()).await;
                            let part = vec![b'a'; 1 << 16];
                            while s.write_all(&part).await.is_ok() {}
                        }
                        "split" => {
                            let _ = s.write_all(sse.as_bytes()).await;
                            let done = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}";
                            for part in [&hello[..hello.len() - 1], "\n", done, "\r", "\n\r", "\n", "data: [DONE]\r\n\r\n"] {
                                let _ = s.write_all(part.as_bytes()).await;
                                let _ = s.flush().await;
                                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                            }
                        }
                        _ => {
                            let body = "{\"error\":{\"type\":\"rate_limit_exceeded\",\"message\":\"busy\"}}";
                            let head = format!("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 600\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                            let _ = s.write_all(head.as_bytes()).await;
                        }
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    async fn ask(how: &'static str) -> (Result<(Value, Value)>, std::time::Duration) {
        // SAFETY: every test here sets the same value.
        unsafe { std::env::set_var("HARLO_AGENT_IDLE_SECS", "1") };
        let base = stand_in(how).await;
        let started = std::time::Instant::now();
        let r = stream(&base, None, &json!({}), "stand-in", "m", |_| {}).await;
        (r, started.elapsed())
    }

    #[tokio::test]
    async fn a_server_that_goes_quiet_is_given_up_on() {
        let (r, took) = ask("hang").await;
        let e = r.unwrap_err();
        assert!(e.downcast_ref::<Dropped>().is_some() && e.to_string().contains("sent nothing back for 1 s"), "{e:#}");
        assert!(took < std::time::Duration::from_secs(5), "{took:?}");
        let (r, took) = ask("stall").await;
        let e = r.unwrap_err();
        assert!(e.downcast_ref::<Dropped>().is_some() && e.to_string().contains("sent nothing for 1 s during the reply"), "{e:#}");
        assert!(crate::retry::transient(&e).is_some(), "a stall is tried again");
        assert!(took < std::time::Duration::from_secs(5), "{took:?}");
    }

    #[tokio::test]
    async fn an_event_without_end_is_cut_off() {
        let (r, took) = ask("endless").await;
        let e = r.unwrap_err();
        assert!(e.to_string().contains("passed 16 MiB without ending"), "{e:#}");
        assert!(crate::retry::transient(&e).is_none(), "not sent again");
        assert!(took < std::time::Duration::from_secs(20), "{took:?}");
    }

    #[tokio::test]
    async fn events_split_across_writes_still_read() {
        let (r, _) = ask("split").await;
        let (message, _) = r.unwrap();
        assert_eq!(message["content"][0]["text"], "Hello", "{message}");
    }

    #[tokio::test]
    async fn a_long_wait_asked_for_is_said_not_waited() {
        let (r, took) = ask("429").await;
        let e = r.unwrap_err();
        let asked = crate::retry::transient(&e).and_then(|(_, s)| s);
        assert_eq!(asked, Some(600));
        assert!(asked.unwrap() > crate::retry::MAX_WAIT, "longer than harlo waits by itself");
        assert!(took < std::time::Duration::from_secs(5), "{took:?}");
    }

    fn feed(chunks: &[Value]) -> (Vec<Value>, Vec<Value>, Reply) {
        let mut r = Reply::default();
        let mut ev = Vec::new();
        for c in chunks {
            r.take(c, &mut ev);
        }
        let content = r.finish(&mut ev);
        (ev, content, r)
    }

    #[test]
    fn a_streamed_reply_becomes_blocks_and_events() {
        let d = |delta: Value| json!({ "choices": [{ "index": 0, "delta": delta }] });
        let (ev, content, r) = feed(&[
            d(json!({ "role": "assistant", "reasoning_content": "Let me " })),
            d(json!({ "reasoning_content": "look." })),
            d(json!({ "content": "I'll read it." })),
            d(json!({ "tool_calls": [{ "index": 0, "id": "call_a", "type": "function", "function": { "name": "read", "arguments": "" } }] })),
            d(json!({ "tool_calls": [{ "index": 0, "function": { "arguments": "{\"path\":" } }] })),
            d(json!({ "tool_calls": [{ "index": 1, "id": "call_b", "function": { "name": "ls", "arguments": "{}" } }] })),
            d(json!({ "tool_calls": [{ "index": 0, "function": { "arguments": "\"a.txt\"}" } }] })),
            json!({ "choices": [{ "index": 0, "delta": {}, "finish_reason": "tool_calls" }] }),
            json!({ "choices": [], "usage": { "prompt_tokens": 100, "completion_tokens": 20, "prompt_tokens_details": { "cached_tokens": 64 } } }),
        ]);
        assert_eq!(content, vec![
            json!({ "type": "thinking", "thinking": "Let me look.", "thinkingSignature": "reasoning_content" }),
            json!({ "type": "text", "text": "I'll read it." }),
            json!({ "type": "toolCall", "id": "call_a", "name": "read", "arguments": { "path": "a.txt" } }),
            json!({ "type": "toolCall", "id": "call_b", "name": "ls", "arguments": {} }),
        ]);
        let kinds: Vec<&str> = ev.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["thinking_start", "thinking_delta", "thinking_delta", "thinking_end", "text_start", "text_delta", "text_end",
            "toolcall_start", "toolcall_delta", "toolcall_start", "toolcall_delta", "toolcall_delta", "toolcall_end", "toolcall_end"]);
        assert_eq!(ev[13]["toolCall"]["id"], "call_b");
        assert_eq!(r.finish.as_deref(), Some("tool_calls"));
        let u = usage(&r.usage);
        assert_eq!((u["input_tokens"].as_u64(), u["input_tokens_details"]["cached_tokens"].as_u64()), (Some(100), Some(64)));
    }

    #[test]
    fn broken_arguments_are_said() {
        let (_, content, _) = feed(&[json!({ "choices": [{ "delta": { "tool_calls": [{ "index": 0, "id": "c", "function": { "name": "bash", "arguments": "{\"command\": \"ls" } }] } }] })]);
        assert!(content[0]["argumentsError"].as_str().unwrap().contains("weren't a JSON object"));
    }

    #[test]
    fn images_as_chat_parts() {
        let img = json!({ "type": "image", "data": "QUJD", "mimeType": "image/jpeg" });
        let history = vec![
            json!({ "role": "user", "content": [{ "type": "text", "text": "look" }, img.clone()] }),
            json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": "c1|x", "name": "mcp__s__shot", "arguments": {} }] }),
            json!({ "role": "toolResult", "toolCallId": "c1|x", "toolName": "mcp__s__shot", "content": [{ "type": "text", "text": "[image 1: 2×2 JPEG, 1 KB]" }, img.clone(), img.clone()], "isError": false }),
        ];
        let sent = messages(&history, "m", true);
        assert_eq!(sent[0]["content"], json!([{ "type": "text", "text": "look" }, { "type": "image_url", "image_url": { "url": "data:image/jpeg;base64,QUJD" } }]));
        assert_eq!(sent[2]["content"], "[image 1: 2×2 JPEG, 1 KB]\n[Its 2 images are shown after this round's results.]");
        assert_eq!(sent[3]["role"], "user");
        assert_eq!(sent[3]["content"].as_array().unwrap().len(), 3);
        assert_eq!(sent[3]["content"][0]["text"], "Images from mcp__s__shot (call c1):");
        let plain = messages(&history, "m", false);
        assert_eq!(plain[0]["content"][1], json!({ "type": "text", "text": "[image: not sent, m doesn't take images]" }));
        assert_eq!(plain.len(), 3);
    }

    #[test]
    fn history_as_chat_messages() {
        let history = vec![
            json!({ "role": "user", "content": [{ "type": "text", "text": "hi" }] }),
            json!({ "role": "assistant", "content": [{ "type": "thinking", "thinking": "hm" }, { "type": "text", "text": "Reading." }, { "type": "toolCall", "id": "call_1|fc_9", "name": "read", "arguments": { "path": "a" } }] }),
            json!({ "role": "toolResult", "toolCallId": "call_1|fc_9", "toolName": "read", "content": [{ "type": "text", "text": "A" }], "isError": false }),
            json!({ "role": "custom", "customType": "turn_stopped", "content": "stopped", "display": false }),
            json!({ "role": "assistant", "content": [{ "type": "thinking", "thinking": "only thought" }] }),
        ];
        assert_eq!(messages(&history, "m", true), vec![
            json!({ "role": "user", "content": "hi" }),
            json!({ "role": "assistant", "content": "Reading.", "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "read", "arguments": "{\"path\":\"a\"}" } }] }),
            json!({ "role": "tool", "tool_call_id": "call_1", "content": "A" }),
            json!({ "role": "user", "content": "stopped" }),
        ]);
        let body = request("m", "be brief", &messages(&history[..1], "m", true), &json!([{ "name": "ls", "description": "list", "parameters": { "type": "object" } }]), None);
        assert_eq!(body["messages"][0], json!({ "role": "system", "content": "be brief" }));
        assert_eq!(body["tools"][0]["function"]["name"], "ls");
        assert!(body.get("reasoning_effort").is_none());
    }
}
