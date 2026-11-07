//! OpenAI's Responses API, streamed (server-sent events). With a ChatGPT
//! plan's token: `store: false`, `stream: true`, the whole history in
//! `input` each time, the system prompt as `instructions`.

use anyhow::{Result, anyhow, bail};
use futures_util::StreamExt as _;
use serde_json::{Value, json};

use crate::auth::RESOURCE;

/// What a stream says, as it comes.
#[derive(Debug)]
pub enum Event {
    Text(String),
    Reasoning(String),
    /// Finished: its usage (`input_tokens`, `output_tokens`, …).
    Completed { usage: Value },
}

/// A model the signed-in account may use: (id, name to show).
pub async fn models(token: &str) -> Result<Vec<(String, String)>> {
    let resp = reqwest::Client::new().get(format!("{RESOURCE}/models")).bearer_auth(token).send().await?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        bail!("{}", refusal(status.as_u16(), &body));
    }
    // A plan's list (`models`, in the server's order, only those to list);
    // an API key's (`data`).
    if let Some(list) = body["models"].as_array() {
        return Ok(list
            .iter()
            .filter(|m| m["visibility"].as_str().is_none_or(|v| v == "list"))
            .filter_map(|m| {
                let id = m["slug"].as_str().or_else(|| m["id"].as_str())?.to_owned();
                let name = m["display_name"].as_str().unwrap_or(&id).to_owned();
                Some((id, name))
            })
            .collect());
    }
    Ok(body["data"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str().map(|id| (id.to_owned(), id.to_owned()))).collect())
}

/// Sends one request and calls `on` with each event until it completes.
pub async fn stream(token: &str, model: &str, instructions: &str, input: Value, effort: Option<&str>, mut on: impl FnMut(Event)) -> Result<()> {
    let mut body = json!({ "model": model, "instructions": instructions, "input": input, "store": false, "stream": true });
    if let Some(e) = effort {
        body["reasoning"] = json!({ "effort": e, "summary": "auto" });
    }
    let resp = reqwest::Client::new().post(format!("{RESOURCE}/responses")).bearer_auth(token).json(&body).send().await?;
    let status = resp.status();
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
                "response.output_text.delta" => on(Event::Text(ev["delta"].as_str().unwrap_or_default().to_owned())),
                "response.reasoning_summary_text.delta" => on(Event::Reasoning(ev["delta"].as_str().unwrap_or_default().to_owned())),
                "response.completed" => {
                    on(Event::Completed { usage: ev["response"]["usage"].clone() });
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

/// (end of the event's text, end including the blank line).
fn find_event_end(buf: &[u8]) -> Option<(usize, usize)> {
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
