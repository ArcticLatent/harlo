//! OpenAI's Responses API, streamed (server-sent events): a ChatGPT plan's,
//! an API key's, or a server's that speaks it. `store: false`,
//! `stream: true`, the whole history in `input` each time, the system
//! prompt as `instructions`.

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
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
            let minutes = num(format!("x-codex-{w}-window-minutes")).map(|m| m.round() as i64).filter(|m| *m > 0);
            let reset = num(format!("x-codex-{w}-reset-at")).map(|r| r.round() as i64).filter(|r| *r > 0);
            // (A window with nothing in it: a plan without that window.)
            (used != 0.0 || minutes.is_some() || reset.is_some()).then(|| json!({ "windowMinutes": minutes, "usedPercent": used, "resetsAt": reset }))
        })
        .collect()
}

/// One client for every request, so a session's requests reuse their
/// connection. A server that doesn't answer the connection in 15 s isn't
/// waited for, and a connection that died quietly is noticed.
pub(crate) fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).tcp_keepalive(Duration::from_secs(30)).build().unwrap_or_default()
    })
}

/// How long a model server may send nothing (the reply's headers, or the
/// next part of the reply) before the request counts as stalled: 10
/// minutes, room for a slow local model reading a long prompt.
/// `HARLO_AGENT_IDLE_SECS` sets another.
pub(crate) fn idle() -> Duration {
    std::env::var("HARLO_AGENT_IDLE_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0.0)
        .map_or(Duration::from_secs(600), Duration::from_secs_f64)
}

/// The most one event of a reply may hold before it ends.
pub(crate) const MAX_EVENT: usize = 16 << 20;

/// `10 min`, `45 s`.
pub(crate) fn span(d: Duration) -> String {
    let s = d.as_secs_f64();
    match () {
        _ if s >= 2.0 * 3600.0 => format!("{} h", (s / 3600.0).round()),
        _ if s >= 120.0 => format!("{} min", (s / 60.0).round()),
        _ if s >= 1.0 => format!("{} s", s.round()),
        _ => format!("{} ms", d.as_millis()),
    }
}

/// Sends a model request: its response, once its headers come (within
/// [`idle`]).
pub(crate) async fn send(req: reqwest::RequestBuilder, base: &str) -> Result<reqwest::Response> {
    let wait = idle();
    match tokio::time::timeout(wait, req.send()).await {
        Ok(r) => r.map_err(|e| Dropped(format!("Couldn't reach {base}: {e}")).into()),
        Err(_) => Err(Dropped(format!("{base} sent nothing back for {}", span(wait))).into()),
    }
}

/// A refused request's body, as text (waited for no longer than a reply).
pub(crate) async fn refusal_text(resp: reqwest::Response) -> String {
    tokio::time::timeout(idle(), resp.text()).await.ok().and_then(Result::ok).unwrap_or_default()
}

/// A reply's server-sent events, read as they come.
pub(crate) struct Events {
    resp: reqwest::Response,
    buf: Vec<u8>,
    /// How much of `buf` has been searched for an event's end.
    searched: usize,
    idle: Duration,
}

impl Events {
    pub fn new(resp: reqwest::Response) -> Self {
        Self { resp, buf: Vec::new(), searched: 0, idle: idle() }
    }

    /// The next event's data (its `data:` lines, joined); None once the
    /// server has closed the reply. A reply that stalls (nothing for
    /// [`idle`]) or breaks is [`Dropped`]; an event that grows past
    /// [`MAX_EVENT`] without ending stops it.
    pub async fn next(&mut self) -> Result<Option<String>> {
        loop {
            if let Some((text, end)) = find_event_end(&self.buf[self.searched..]) {
                let raw: Vec<u8> = self.buf.drain(..self.searched + end).collect();
                let text = String::from_utf8_lossy(&raw[..self.searched + text]);
                self.searched = 0;
                return Ok(Some(text.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim_start).collect::<Vec<_>>().join("\n")));
            }
            // (Searched once: next time only what's new, from just before
            // it, as a blank line may come split.)
            self.searched = self.buf.len().saturating_sub(3);
            if self.buf.len() > MAX_EVENT {
                bail!("The reply was stopped: one of its events passed {} MiB without ending", MAX_EVENT >> 20);
            }
            match tokio::time::timeout(self.idle, self.resp.chunk()).await {
                Err(_) => return Err(Dropped(format!("The server sent nothing for {} during the reply", span(self.idle))).into()),
                Ok(Err(e)) => return Err(Dropped(format!("The connection broke during the reply: {e}")).into()),
                Ok(Ok(None)) => return Ok(None),
                Ok(Ok(Some(part))) => self.buf.extend_from_slice(&part),
            }
        }
    }
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
    let resp = req.timeout(Duration::from_secs(30)).send().await.map_err(|e| anyhow!("Couldn't reach {url}: {e}"))?;
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
            // (The server's own compaction, when asked for: opaque.)
            Some("compaction") => content.push(json!({ "type": "compaction", "item": item })),
            // (A web search the server ran itself: what it looked for.)
            Some("web_search_call") => content.push(json!({ "type": "webSearch", "query": search_query(item), "item": item })),
            _ => {}
        }
    }
    let calls = content.iter().any(|b| b["type"] == "toolCall");
    json!({
        "role": "assistant",
        "content": content,
        "api": "openai-responses",
        "provider": provider,
        "model": model,
        // (Its cost: the agent's to say, which knows the model's price.)
        "usage": crate::used::Used::from_api(&usage).log(&usage, &crate::used::Pricing::Unknown),
        "stopReason": if calls { "toolUse" } else { "stop" },
        "timestamp": crate::session::now_ms(),
    })
}

/// What a web search looked for ("" when the server didn't say).
fn search_query(item: &Value) -> String {
    let a = &item["action"];
    match a["query"].as_str().filter(|q| !q.is_empty()) {
        Some(q) => q.to_owned(),
        None => a["queries"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join("; "),
    }
    .chars()
    .take(300)
    .collect()
}

/// pi's messages as the Responses API's `input`. Reasoning and call item
/// ids go back only to the model (and provider) that made them. A tool's
/// images follow its round's results, as the user's; `images` false (the
/// model doesn't take them): a note in their place.
pub fn input(messages: &[Value], provider: &str, model: &str, images: bool) -> Vec<Value> {
    let image = |b: &Value| {
        if images { json!({ "type": "input_image", "image_url": crate::images::data_url(b) }) } else { json!({ "type": "input_text", "text": crate::images::not_sent(model) }) }
    };
    // (A round's tool images, shown once its results are in.)
    let mut shown: Vec<Value> = Vec::new();
    let text_of = |content: &Value| -> String {
        match content {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        }
    };
    let mut out = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        if m["role"] != "toolResult" && !shown.is_empty() {
            out.push(json!({ "role": "user", "content": std::mem::take(&mut shown) }));
        }
        match m["role"].as_str() {
            Some("user") => {
                let parts: Vec<Value> = match &m["content"] {
                    Value::String(s) => vec![json!({ "type": "input_text", "text": s })],
                    Value::Array(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b["type"].as_str() {
                            Some("image") => Some(image(b)),
                            _ => b["text"].as_str().map(|t| json!({ "type": "input_text", "text": t })),
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                if !parts.is_empty() {
                    out.push(json!({ "role": "user", "content": parts }));
                }
            }
            Some("custom") => out.push(json!({ "role": "user", "content": [{ "type": "input_text", "text": text_of(&m["content"]) }] })),
            // (The server's own compaction goes back as it came, to the
            // model that made it; others get its note.)
            Some("compactionSummary") if m["native"]["provider"] == provider && m["native"]["model"] == model && m["native"]["item"].is_object() => out.push(m["native"]["item"].clone()),
            Some("compactionSummary") => out.push(json!({ "role": "user", "content": [{ "type": "input_text", "text": crate::compact::replayed(m) }] })),
            Some("assistant") => {
                let same = m["provider"] == provider && m["model"] == model && m["api"] == "openai-responses";
                let mut texts = 0;
                for b in m["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        // (The server's web searches go back to the model
                        // that ran them.)
                        Some("webSearch") if same && b["item"].is_object() => out.push(b["item"].clone()),
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
                let pictures: Vec<&Value> = m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "image").collect();
                let mut output = text_of(&m["content"]);
                if !pictures.is_empty() {
                    if images {
                        let n = pictures.len();
                        output.push_str(&format!("\n[{} shown after this round's results.]", if n == 1 { "Its image is".to_owned() } else { format!("Its {n} images are") }));
                        shown.push(json!({ "type": "input_text", "text": format!("Images from {} (call {call_id}):", m["toolName"].as_str().unwrap_or("a tool")) }));
                        shown.extend(pictures.into_iter().map(image));
                    } else {
                        output.push_str(&format!("\n{}", crate::images::not_sent(model)));
                    }
                }
                out.push(json!({ "type": "function_call_output", "call_id": call_id, "output": output }));
            }
            _ => {}
        }
    }
    if !shown.is_empty() {
        out.push(json!({ "role": "user", "content": shown }));
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
    let resp = send(req, base).await?;
    let status = resp.status();
    let retry_after = crate::retry::retry_after(resp.headers());
    // (Refused ones too: a used-up window says so.)
    let windows = limits(resp.headers());
    if !windows.is_empty() {
        on(Event::Limits(windows));
    }
    if !status.is_success() {
        let body: Value = serde_json::from_str(&refusal_text(resp).await).unwrap_or(Value::Null);
        let code = body["error"]["type"].as_str().or_else(|| body["error"]["code"].as_str()).unwrap_or_default().to_owned();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let resets_at = body["error"]["resets_at"].as_i64().or_else(|| body["error"]["resets_in_seconds"].as_i64().map(|s| now + s));
        return Err(ApiError { status: status.as_u16(), message: refusal(status.as_u16(), &body), code, retry_after, resets_at }.into());
    }
    let mut events = Events::new(resp);
    while let Some(data) = events.next().await? {
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
                let code = err["code"].as_str().unwrap_or_default();
                return Err(Failed { code: code.to_owned(), message: explain(code, err["message"].as_str().unwrap_or("the request failed")) }.into());
            }
            "response.incomplete" => {
                bail!("The reply was cut off ({})", ev["response"]["incomplete_details"]["reason"].as_str().unwrap_or("no reason given"));
            }
            "error" => {
                let code = ev["code"].as_str().or_else(|| ev["error"]["code"].as_str()).unwrap_or_default();
                let message = ev["message"].as_str().or_else(|| ev["error"]["message"].as_str()).unwrap_or("the request failed");
                return Err(Failed { code: code.to_owned(), message: explain(code, message) }.into());
            }
            _ => {}
        }
    }
    Err(Dropped("The connection closed before the reply finished".into()).into())
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
    /// The error's type or code, as the body gave it (may be empty).
    pub code: String,
    /// Seconds the server asked to wait (`Retry-After`).
    pub retry_after: Option<u64>,
    /// When a used-up plan resets (unix seconds), if it said.
    pub resets_at: Option<i64>,
}

/// The request didn't reach the server, or the reply's stream broke.
#[derive(Debug)]
pub struct Dropped(pub String);

impl std::fmt::Display for Dropped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Dropped {}

/// The reply's stream ended with an error (`response.failed`, `error`).
#[derive(Debug)]
pub struct Failed {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failed {}

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
        // A plan with one window: the other comes empty, and is left out.
        let mut one = reqwest::header::HeaderMap::new();
        for (k, v) in [("x-codex-primary-used-percent", "1"), ("x-codex-primary-window-minutes", "10080"), ("x-codex-primary-reset-at", "1794537064"),
                       ("x-codex-secondary-used-percent", "0"), ("x-codex-secondary-window-minutes", "0")] {
            one.insert(k, v.parse().unwrap());
        }
        assert_eq!(limits(&one), vec![json!({ "windowMinutes": 10080, "usedPercent": 1.0, "resetsAt": 1794537064 })]);
    }

    #[test]
    fn images_go_with_the_users_message_and_after_a_rounds_results() {
        let img = json!({ "type": "image", "data": "QUJD", "mimeType": "image/png" });
        let call = |id: &str| json!({ "type": "toolCall", "id": format!("{id}|fc_{id}"), "name": "read", "arguments": { "path": "a.png" } });
        let result = |id: &str, with: bool| {
            let mut c = vec![json!({ "type": "text", "text": "[Image: a.png]" })];
            if with {
                c.push(img.clone());
            }
            json!({ "role": "toolResult", "toolCallId": format!("{id}|fc_{id}"), "toolName": "read", "content": c, "isError": false })
        };
        let msgs = vec![
            json!({ "role": "user", "content": [{ "type": "text", "text": "what's this" }, img.clone()] }),
            json!({ "role": "assistant", "content": [call("c1"), call("c2")], "provider": "chatgpt", "model": "m1", "api": "openai-responses" }),
            result("c1", true),
            result("c2", false),
            json!({ "role": "user", "content": [{ "type": "text", "text": "and?" }] }),
        ];
        let sent = input(&msgs, "chatgpt", "m1", true);
        assert_eq!(sent[0]["content"], json!([{ "type": "input_text", "text": "what's this" }, { "type": "input_image", "image_url": "data:image/png;base64,QUJD" }]));
        assert_eq!(sent[3]["output"], "[Image: a.png]\n[Its image is shown after this round's results.]");
        assert_eq!(sent[4]["output"], "[Image: a.png]");
        assert_eq!(sent[5], json!({ "role": "user", "content": [{ "type": "input_text", "text": "Images from read (call c1):" }, { "type": "input_image", "image_url": "data:image/png;base64,QUJD" }] }));
        assert_eq!(sent[6]["content"][0]["text"], "and?");
        // (At the end too.)
        assert_eq!(input(&msgs[..3], "chatgpt", "m1", true).last().unwrap()["content"][1]["type"], "input_image");
        // A model that doesn't take them: a note, and nothing after the results.
        let plain = input(&msgs, "chatgpt", "m1", false);
        assert_eq!(plain[0]["content"][1], json!({ "type": "input_text", "text": "[image: not sent, m1 doesn't take images]" }));
        assert_eq!(plain[3]["output"], "[Image: a.png]\n[image: not sent, m1 doesn't take images]");
        assert_eq!(plain.len(), 6);
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

        let same = input(&msgs, "chatgpt", "m1", true);
        assert_eq!(same[1], items[0], "reasoning goes back whole");
        assert_eq!(same[2], json!({ "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "Reading.", "annotations": [] }], "status": "completed", "id": "msg_1", "phase": "commentary" }));
        assert_eq!(same[3], json!({ "type": "function_call", "call_id": "call_1", "name": "read", "arguments": "{\"path\":\"a.rs\",\"limit\":5}", "id": "fc_1" }));
        assert_eq!(same[4], json!({ "type": "function_call_output", "call_id": "call_1", "output": "fn a() {}" }));

        let other = input(&msgs, "chatgpt", "m2", true);
        assert_eq!(other.len(), 4, "another model gets no reasoning");
        assert!(other[2].get("id").is_none(), "nor the call's item id");
        assert_eq!(input(&msgs, "openai", "m1", true).len(), 4, "nor the same model through another provider");
    }

    #[test]
    fn a_web_search_goes_back_only_to_its_model() {
        let item = json!({ "id": "ws_1", "type": "web_search_call", "status": "completed", "action": { "type": "search", "queries": ["rust latest"], "query": "rust latest" } });
        let text = json!({ "type": "message", "id": "msg_1", "role": "assistant", "content": [{ "type": "output_text", "text": "1.99.1", "annotations": [] }] });
        let reply = assistant_message(&[item.clone(), text], "codex", "m1", &Value::Null);
        assert_eq!(reply["content"][0], json!({ "type": "webSearch", "query": "rust latest", "item": item }));
        assert_eq!(reply["stopReason"], "stop", "a search isn't a call of ours");
        let msgs = vec![json!({ "role": "user", "content": "latest rust?" }), reply];
        assert_eq!(input(&msgs, "codex", "m1", true)[1], item);
        let other = input(&msgs, "codex", "m2", true);
        assert!(other.iter().all(|i| i["type"] != "web_search_call") && other[1]["type"] == "message");
        let only_queries = json!({ "type": "web_search_call", "action": { "queries": ["a", "b"] } });
        assert_eq!(search_query(&only_queries), "a; b");
    }

    #[test]
    fn the_servers_compaction_goes_back_only_to_its_model() {
        let item = json!({ "type": "compaction", "id": "cmp_1", "encrypted_content": "opaque" });
        let summary = json!({ "role": "compactionSummary", "summary": "note", "native": { "provider": "codex", "model": "m", "item": item } });
        assert_eq!(input(std::slice::from_ref(&summary), "codex", "m", true), [item.clone()]);
        let other = input(std::slice::from_ref(&summary), "codex", "n", true);
        assert_eq!(other[0]["role"], "user");
        assert!(other[0]["content"][0]["text"].as_str().unwrap().contains("note"));
        // Its reply: a block holding the item.
        let reply = assistant_message(std::slice::from_ref(&item), "codex", "m", &json!({}));
        assert_eq!(reply["content"], json!([{ "type": "compaction", "item": item }]));
    }

    #[test]
    fn arguments_that_arent_json_are_marked() {
        let reply = assistant_message(&[json!({ "type": "function_call", "id": "fc_1", "call_id": "c", "name": "bash", "arguments": "{oops" })], "chatgpt", "m", &Value::Null);
        assert!(reply["content"][0]["argumentsError"].as_str().unwrap().contains("{oops"));
        assert_eq!(reply["content"][0]["arguments"], json!({}));
    }
}
