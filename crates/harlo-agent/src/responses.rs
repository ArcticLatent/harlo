//! OpenAI's Responses API, streamed (server-sent events): a ChatGPT plan's,
//! an API key's, or a server's that speaks it. `store: false`,
//! `stream: true`, the whole history in `input` each time, the system
//! prompt as `instructions`.

use anyhow::{Result, anyhow, bail};
use futures_util::StreamExt as _;
use serde_json::{Value, json};

/// What a stream says, as it comes. `index`: the item's place in the
/// response's output.
#[derive(Debug)]
pub enum Event {
    /// An output item started (a message, reasoning, a function call).
    Added { index: usize, item: Value },
    Text { index: usize, delta: String },
    /// Reasoning summary text; `part` counts the summary's parts.
    Reasoning { index: usize, part: usize, delta: String },
    /// A function call's arguments, as they come.
    Args { index: usize, delta: String },
    /// An output item, complete: what goes back in `input` next time.
    Item { index: usize, item: Value },
    /// Finished: its usage (`input_tokens`, `output_tokens`, …) and its
    /// output items (complete; some servers leave the reasoning's
    /// encrypted content out of the items as they come).
    Completed { usage: Value, output: Vec<Value> },
    /// The plan's limits as the response's headers give them (the Codex
    /// backend sends them), before the reply: `[{ windowMinutes,
    /// usedPercent, resetsAt (unix s) }]`.
    Limits(Vec<Value>),
}

/// The plan's limit windows in a response's headers
/// (`x-codex-{primary,secondary}-{used-percent,window-minutes,reset-at}`):
/// none when it sends none (API keys, other servers).
pub fn limits(headers: &reqwest::header::HeaderMap) -> Vec<Value> {
    let num = |k: String| headers.get(&k).and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<f64>().ok()).filter(|n| n.is_finite());
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|w| {
            let used = num(format!("x-codex-{w}-used-percent"))?;
            Some(json!({
                "windowMinutes": num(format!("x-codex-{w}-window-minutes")).map(|m| m.round() as i64),
                "usedPercent": used,
                "resetsAt": num(format!("x-codex-{w}-reset-at")).map(|r| r.round() as i64),
            }))
        })
        .collect()
}

/// One client for every request, so a session's requests reuse their
/// connection.
pub(crate) fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// A server's models list as it sends it (`url`: its models address;
/// `headers`: the provider's own).
pub async fn models_raw(url: &str, token: Option<&str>, headers: &[(&'static str, String)]) -> Result<Value> {
    let mut req = client().get(url);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let resp = req.send().await.map_err(|e| anyhow!("Couldn't reach {url}: {e}"))?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        bail!("{}", refusal(status.as_u16(), &body));
    }
    Ok(body)
}

/// A request's body: the model, its instructions, the history, tools.
/// `cache_key`: the session's id, so its requests reach the cache that
/// holds their shared start.
pub fn request(model: &str, instructions: &str, input: &[Value], tools: Value, effort: Option<&str>, cache_key: &str) -> Value {
    let mut body = json!({
        "model": model, "instructions": instructions, "input": input, "store": false, "stream": true,
        "include": ["reasoning.encrypted_content"], "prompt_cache_key": cache_key,
    });
    if tools.as_array().is_some_and(|t| !t.is_empty()) {
        body["tools"] = tools;
        body["tool_choice"] = json!("auto");
        body["parallel_tool_calls"] = json!(true);
    }
    if let Some(e) = effort {
        body["reasoning"] = json!({ "effort": e, "summary": "auto" });
    }
    body
}

/// A reply's output items as pi's assistant message: reasoning as
/// `thinking` (the whole item in `thinkingSignature`), text with its item's
/// id (and phase) in `textSignature`, calls as `toolCall` with
/// `call_id|item_id`.
pub fn assistant_message(items: &[Value], provider: &str, model: &str, usage: &Value) -> Value {
    let mut content = Vec::new();
    for item in items {
        match item["type"].as_str() {
            Some("reasoning") => {
                let summary: Vec<&str> = item["summary"].as_array().into_iter().flatten().filter_map(|p| p["text"].as_str()).collect();
                content.push(json!({ "type": "thinking", "thinking": summary.join("\n\n"), "thinkingSignature": item.to_string() }));
            }
            Some("message") => {
                let mut sig = json!({ "v": 1, "id": item["id"] });
                if let Some(phase) = item["phase"].as_str() {
                    sig["phase"] = json!(phase);
                }
                for part in item["content"].as_array().into_iter().flatten() {
                    let text = part["text"].as_str().or_else(|| part["refusal"].as_str()).unwrap_or_default();
                    content.push(json!({ "type": "text", "text": text, "textSignature": sig.to_string() }));
                }
            }
            Some("function_call") => {
                let raw = item["arguments"].as_str().unwrap_or("{}");
                let mut call = json!({
                    "type": "toolCall",
                    "id": format!("{}|{}", item["call_id"].as_str().unwrap_or_default(), item["id"].as_str().unwrap_or_default()),
                    "name": item["name"],
                    "arguments": {},
                });
                match serde_json::from_str::<Value>(raw) {
                    Ok(v) if v.is_object() => call["arguments"] = v,
                    _ => call["argumentsError"] = json!(format!("The arguments weren't a JSON object: {raw}")),
                }
                content.push(call);
            }
            _ => {}
        }
    }
    let calls = content.iter().any(|b| b["type"] == "toolCall");
    let input = usage["input_tokens"].as_u64().unwrap_or(0);
    let cached = usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0);
    let output = usage["output_tokens"].as_u64().unwrap_or(0);
    json!({
        "role": "assistant",
        "content": content,
        "api": "openai-responses",
        "provider": provider,
        "model": model,
        "usage": {
            "input": input.saturating_sub(cached), "output": output, "cacheRead": cached, "cacheWrite": 0,
            "reasoning": usage["output_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0),
            "totalTokens": input + output,
            "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0 },
        },
        "stopReason": if calls { "toolUse" } else { "stop" },
        "timestamp": crate::session::now_ms(),
    })
}

/// pi's messages as the Responses API's `input`. Reasoning and call item
/// ids go back only to the model (and provider) that made them.
pub fn input(messages: &[Value], provider: &str, model: &str) -> Vec<Value> {
    let text_of = |content: &Value| -> String {
        match content {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        }
    };
    let mut out = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        match m["role"].as_str() {
            Some("user") => {
                let parts: Vec<Value> = match &m["content"] {
                    Value::String(s) => vec![json!({ "type": "input_text", "text": s })],
                    Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str()).map(|t| json!({ "type": "input_text", "text": t })).collect(),
                    _ => Vec::new(),
                };
                if !parts.is_empty() {
                    out.push(json!({ "role": "user", "content": parts }));
                }
            }
            Some("custom") => out.push(json!({ "role": "user", "content": [{ "type": "input_text", "text": text_of(&m["content"]) }] })),
            Some("compactionSummary") => out.push(json!({ "role": "user", "content": [{ "type": "input_text", "text": crate::compact::replayed(m) }] })),
            Some("assistant") => {
                let same = m["provider"] == provider && m["model"] == model && m["api"] == "openai-responses";
                let mut texts = 0;
                for b in m["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("thinking") if same => {
                            if let Some(item) = b["thinkingSignature"].as_str().and_then(|s| serde_json::from_str::<Value>(s).ok()) {
                                out.push(item);
                            }
                        }
                        Some("text") => {
                            let sig = b["textSignature"].as_str().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(Value::Null);
                            let id = sig["id"].as_str().filter(|id| id.len() <= 64).map(str::to_owned).unwrap_or_else(|| format!("msg_h_{i}_{texts}"));
                            texts += 1;
                            let mut item = json!({ "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": b["text"], "annotations": [] }], "status": "completed", "id": id });
                            if let Some(phase) = sig["phase"].as_str() {
                                item["phase"] = json!(phase);
                            }
                            out.push(item);
                        }
                        Some("toolCall") => {
                            let id = b["id"].as_str().unwrap_or_default();
                            let (call_id, item_id) = id.split_once('|').unwrap_or((id, ""));
                            let mut item = json!({ "type": "function_call", "call_id": call_id, "name": b["name"], "arguments": b["arguments"].to_string() });
                            if same && item_id.starts_with("fc_") {
                                item["id"] = json!(item_id);
                            }
                            out.push(item);
                        }
                        _ => {}
                    }
                }
            }
            Some("toolResult") => {
                let id = m["toolCallId"].as_str().unwrap_or_default();
                let call_id = id.split_once('|').map_or(id, |(c, _)| c);
                out.push(json!({ "type": "function_call_output", "call_id": call_id, "output": text_of(&m["content"]) }));
            }
            _ => {}
        }
    }
    out
}

/// Sends one request and calls `on` with each event until it completes.
/// `affinity`: the session's id, sent as the plan's affinity headers, so
/// its requests go where their shared start is cached. `headers`: the
/// provider's own.
pub async fn stream(base: &str, token: Option<&str>, affinity: Option<&str>, headers: &[(&'static str, String)], body: &Value, mut on: impl FnMut(Event)) -> Result<()> {
    let mut req = client().post(format!("{base}/responses")).json(body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    if let Some(session) = affinity {
        req = req.header("session-id", session).header("x-client-request-id", session);
    }
    let resp = req.send().await.map_err(|e| anyhow!("Couldn't reach {base}: {e}"))?;
    let status = resp.status();
    // (Refused ones too: a used-up window says so.)
    let windows = limits(resp.headers());
    if !windows.is_empty() {
        on(Event::Limits(windows));
    }
    if !status.is_success() {
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        return Err(ApiError { status: status.as_u16(), message: refusal(status.as_u16(), &body) }.into());
    }
    let mut bytes = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = bytes.next().await {
        buf.extend_from_slice(&chunk?);
        // Events end with a blank line.
        while let Some(end) = find_event_end(&buf) {
            let raw: Vec<u8> = buf.drain(..end.1).collect();
            let text = String::from_utf8_lossy(&raw[..end.0]);
            let data: String = text.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim_start).collect::<Vec<_>>().join("\n");
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let Ok(ev) = serde_json::from_str::<Value>(&data) else { continue };
            match ev["type"].as_str().unwrap_or_default() {
                "response.output_item.added" => on(Event::Added { index: index(&ev), item: ev["item"].clone() }),
                "response.output_text.delta" => on(Event::Text { index: index(&ev), delta: delta(&ev) }),
                "response.reasoning_summary_text.delta" => {
                    on(Event::Reasoning { index: index(&ev), part: ev["summary_index"].as_u64().unwrap_or(0) as usize, delta: delta(&ev) })
                }
                "response.function_call_arguments.delta" => on(Event::Args { index: index(&ev), delta: delta(&ev) }),
                "response.output_item.done" => on(Event::Item { index: index(&ev), item: ev["item"].clone() }),
                "response.completed" => {
                    let output = ev["response"]["output"].as_array().cloned().unwrap_or_default();
                    on(Event::Completed { usage: ev["response"]["usage"].clone(), output });
                    return Ok(());
                }
                "response.failed" => {
                    let err = &ev["response"]["error"];
                    bail!("{}", explain(err["code"].as_str().unwrap_or_default(), err["message"].as_str().unwrap_or("the request failed")));
                }
                "response.incomplete" => {
                    bail!("The reply was cut off ({})", ev["response"]["incomplete_details"]["reason"].as_str().unwrap_or("no reason given"));
                }
                "error" => bail!("{}", explain(ev["code"].as_str().unwrap_or_default(), ev["message"].as_str().unwrap_or("the request failed"))),
                _ => {}
            }
        }
    }
    Err(anyhow!("The connection closed before the reply finished"))
}

fn index(ev: &Value) -> usize {
    ev["output_index"].as_u64().unwrap_or(0) as usize
}

fn delta(ev: &Value) -> String {
    ev["delta"].as_str().unwrap_or_default().to_owned()
}

/// (end of the event's text, end including the blank line).
pub(crate) fn find_event_end(buf: &[u8]) -> Option<(usize, usize)> {
    let lf = buf.windows(2).position(|w| w == b"\n\n").map(|i| (i, i + 2));
    let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| (i, i + 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

/// The API refused the request before streaming.
#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

fn refusal(status: u16, body: &Value) -> String {
    let err = &body["error"];
    // The Codex backend's: the plan used up, and when it resets.
    match err["type"].as_str() {
        Some("usage_limit_reached") => {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
            let when = err["resets_at"].as_i64().map(|at| (at - now).max(0)).or_else(|| err["resets_in_seconds"].as_i64());
            let left = when.map(|s| match s / 60 {
                m if m < 60 => format!(" It resets in {} min.", m.max(1)),
                m if m < 48 * 60 => format!(" It resets in {} h {} min.", m / 60, m % 60),
                m => format!(" It resets in {} days.", (m + 720) / 1440),
            });
            return format!("You've used your ChatGPT plan's usage for now.{}", left.unwrap_or_default());
        }
        Some("usage_not_included") => return "This ChatGPT plan doesn't include Codex usage, which the Codex sign-in uses.".into(),
        _ => {}
    }
    let code = err["code"].as_str().unwrap_or_default();
    let message = err["message"].as_str().or_else(|| body["detail"].as_str()).unwrap_or("no details");
    if !code.is_empty() {
        return explain(code, message);
    }
    match status {
        401 => format!("OpenAI refused the sign-in (401): {message}"),
        403 => format!("OpenAI refused the request (403): {message}"),
        503 => format!("ChatGPT plan requests are unavailable right now (503): {message}. Try again in a little while."),
        _ => format!("OpenAI refused the request ({status}): {message}"),
    }
}

/// What a ChatGPT plan's error means, and what to do.
fn explain(code: &str, message: &str) -> String {
    match code {
        "subscription_sharing_usage_limit_exceeded" => "You've reached your ChatGPT plan's usage limit for harlo. Manage usage in ChatGPT → Settings → Usage.".into(),
        "subscription_sharing_usage_unavailable" | "subscription_sharing_user_unavailable" => "ChatGPT plan usage is unavailable right now. Try again in a little while.".into(),
        "subscription_sharing_user_not_eligible" => "This ChatGPT account's plan can't be used in other apps.".into(),
        "subscription_sharing_unsupported_capability" => format!("The ChatGPT plan doesn't support part of this request: {message}"),
        "subscription_sharing_invalid_user" => format!("OpenAI didn't accept the sign-in: {message}"),
        "" => message.to_owned(),
        _ => format!("{message} ({code})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_used_up() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let r = refusal(429, &json!({ "error": { "type": "usage_limit_reached", "plan_type": "plus", "resets_at": now + 2 * 3600 + 5 * 60 + 30 } }));
        assert!(r.starts_with("You've used your ChatGPT plan's usage for now. It resets in 2 h 5 min"), "{r}");
        assert!(refusal(429, &json!({ "error": { "type": "usage_not_included" } })).contains("doesn't include Codex"));
    }

    #[test]
    fn reads_the_plans_limits() {
        let mut h = reqwest::header::HeaderMap::new();
        for (k, v) in [
            ("x-codex-primary-used-percent", "12.5"),
            ("x-codex-primary-window-minutes", "300"),
            ("x-codex-primary-reset-at", "1792300303"),
            ("x-codex-secondary-used-percent", "40"),
            ("x-codex-secondary-window-minutes", "10080"),
            ("x-codex-other", "1"),
        ] {
            h.insert(k, v.parse().unwrap());
        }
        assert_eq!(
            limits(&h),
            vec![
                json!({ "windowMinutes": 300, "usedPercent": 12.5, "resetsAt": 1792300303 }),
                json!({ "windowMinutes": 10080, "usedPercent": 40.0, "resetsAt": null })
            ]
        );
        assert!(limits(&reqwest::header::HeaderMap::new()).is_empty());
    }

    #[test]
    fn a_reply_goes_back_as_it_came() {
        let items = vec![
            json!({ "type": "reasoning", "id": "rs_1", "summary": [{ "type": "summary_text", "text": "Look first." }], "encrypted_content": "enc" }),
            json!({ "type": "message", "id": "msg_1", "role": "assistant", "phase": "commentary", "status": "completed", "content": [{ "type": "output_text", "text": "Reading.", "annotations": [] }] }),
            json!({ "type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "read", "arguments": "{\"path\":\"a.rs\",\"limit\":5}", "status": "completed" }),
        ];
        let usage = json!({ "input_tokens": 3000, "input_tokens_details": { "cached_tokens": 2048 }, "output_tokens": 40 });
        let reply = assistant_message(&items, "chatgpt", "m1", &usage);
        assert_eq!(reply["stopReason"], "toolUse");
        assert_eq!((reply["usage"]["input"].as_u64(), reply["usage"]["cacheRead"].as_u64()), (Some(952), Some(2048)));
        assert_eq!(reply["content"][0]["thinking"], "Look first.");
        assert_eq!(reply["content"][2]["id"], "call_1|fc_1");
        let result = json!({ "role": "toolResult", "toolCallId": "call_1|fc_1", "toolName": "read", "content": [{ "type": "text", "text": "fn a() {}" }], "isError": false });
        let msgs = vec![json!({ "role": "user", "content": [{ "type": "text", "text": "hi" }] }), reply, result];

        let same = input(&msgs, "chatgpt", "m1");
        assert_eq!(same[1], items[0], "reasoning goes back whole");
        assert_eq!(same[2], json!({ "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "Reading.", "annotations": [] }], "status": "completed", "id": "msg_1", "phase": "commentary" }));
        assert_eq!(same[3], json!({ "type": "function_call", "call_id": "call_1", "name": "read", "arguments": "{\"path\":\"a.rs\",\"limit\":5}", "id": "fc_1" }));
        assert_eq!(same[4], json!({ "type": "function_call_output", "call_id": "call_1", "output": "fn a() {}" }));

        let other = input(&msgs, "chatgpt", "m2");
        assert_eq!(other.len(), 4, "another model gets no reasoning");
        assert!(other[2].get("id").is_none(), "nor the call's item id");
        assert_eq!(input(&msgs, "openai", "m1").len(), 4, "nor the same model through another provider");
    }

    #[test]
    fn arguments_that_arent_json_are_marked() {
        let reply = assistant_message(&[json!({ "type": "function_call", "id": "fc_1", "call_id": "c", "name": "bash", "arguments": "{oops" })], "chatgpt", "m", &Value::Null);
        assert!(reply["content"][0]["argumentsError"].as_str().unwrap().contains("{oops"));
        assert_eq!(reply["content"][0]["arguments"], json!({}));
    }
}
