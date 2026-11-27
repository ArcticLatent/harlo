//! `OpenAI`'s Responses API: the request a session's history makes, and
//! the events of its streamed reply.
//!
//! Nothing is stored at the provider (`store: false`): each request sends
//! the whole conversation. The model's reasoning comes back encrypted and
//! goes back the same way, to the provider and model that wrote it.

// Rust guideline compliant 2026-10-09

use serde_json::{Map, Value, json};

use crate::Error;
use crate::chat::{self, Assembler, Piece, Signer};
use crate::message::{Cache, Message, Part, Tokens};

/// The wire's name in reasoning signatures.
const WIRE: &str = "responses";

/// A chat completions content part as a Responses input part.
fn input_part(part: &Value) -> Option<Value> {
    if let Some(picture) = chat::picture(part) {
        let url = match picture {
            Ok((mime, data)) => format!("data:{mime};base64,{data}"),
            Err(url) => url,
        };
        return Some(json!({ "type": "input_image", "image_url": url }));
    }
    let text = part["text"].as_str().filter(|t| !t.is_empty())?;
    Some(json!({ "type": "input_text", "text": text }))
}

/// A message from the person holding `content`; none when it's empty.
#[expect(
    clippy::needless_pass_by_value,
    reason = "callers build the parts for it and have no use for them after"
)]
fn from_person(content: Vec<Value>) -> Option<Value> {
    (!content.is_empty()).then(|| json!({ "role": "user", "content": content }))
}

/// Text as the person's input parts.
fn said(text: &str) -> Vec<Value> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![json!({ "type": "input_text", "text": text })]
    }
}

/// A step's items: its signed reasoning, text, calls and their results.
fn step_items(at: Signer<'_>, content: &[Part], out: &mut Vec<Value>) {
    let mut pictures = Vec::new();
    for part in content {
        match part {
            Part::Reasoning { text, signature } => {
                let Some(kept) = at.mine(WIRE, signature.as_ref()) else {
                    continue;
                };
                let summary: Vec<Value> = if text.is_empty() {
                    Vec::new()
                } else {
                    vec![json!({ "type": "summary_text", "text": text })]
                };
                out.push(json!({
                    "type": "reasoning", "summary": summary,
                    "encrypted_content": kept["encrypted_content"],
                }));
            }
            Part::Text { text } if !text.is_empty() => out.push(json!({
                "type": "message", "role": "assistant",
                "content": [{ "type": "output_text", "text": text }],
            })),
            Part::Text { .. } => {}
            Part::Tool {
                id, name, state, ..
            } => {
                out.push(json!({
                    "type": "function_call", "call_id": id, "name": name,
                    "arguments": Value::Object(state.input().clone()).to_string(),
                }));
                out.push(json!({
                    "type": "function_call_output", "call_id": id,
                    "output": chat::result_text(state),
                }));
                pictures.extend(chat::result_pictures(state).iter().filter_map(input_part));
            }
        }
    }
    // Results carry text only: pictures follow as the person's message.
    if !pictures.is_empty() {
        let mut content = said("The pictures the tools returned:");
        content.extend(pictures);
        out.extend(from_person(content));
    }
}

/// The session's messages as Responses input items.
fn input(at: Signer<'_>, history: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in crate::compaction::since_summary(history) {
        match message {
            Message::Compaction(c) => {
                if let Some(summary) = c.summary.as_ref().filter(|_| c.status == "completed") {
                    let text = format!("{}{summary}", crate::compaction::SUMMARY_HEADING);
                    out.extend(from_person(said(&text)));
                }
            }
            Message::User {
                text,
                skills,
                files,
                ..
            } => {
                let mut whole: String = skills.iter().map(|s| s.text.as_str()).collect();
                whole.push_str(text);
                let mut content = said(&whole);
                content.extend(
                    files
                        .iter()
                        .filter_map(crate::attachment::File::content)
                        .filter_map(|p| input_part(&p)),
                );
                out.extend(from_person(content));
            }
            Message::Synthetic { text, .. } => out.extend(from_person(said(text))),
            Message::Assistant(step) => step_items(at, &step.content, &mut out),
            Message::Idle { .. } | Message::Shell(_) => {}
        }
    }
    out
}

/// The provider whose requests name their conversation for its cache.
/// Others that speak Responses may refuse a field they don't know.
const KEYED: &str = "openai";

/// The system prompt, and where a request carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum System<'a> {
    /// As the request's own `instructions`.
    Instructions(&'a str),
    /// As a developer message opening the conversation.
    First(&'a str),
}

impl<'a> System<'a> {
    /// `text` as a request to `at` carries it, opening the conversation
    /// when `first` asks for that and the model is one it matters for.
    pub(crate) fn of(text: &'a str, at: Signer<'_>, first: bool) -> Self {
        if first && appends_instructions(at) {
            Self::First(text)
        } else {
            Self::Instructions(text)
        }
    }
}

/// Whether the model adds a request's `instructions` after the
/// conversation: `OpenAI`'s GPT-6.2 and later.
///
/// Its cache holds a request from its start. Instructions added at the
/// end are outside what it holds, and are charged whole with every
/// request; the same text in a developer message at the start is held
/// with the rest.
fn appends_instructions(at: Signer<'_>) -> bool {
    let Some(version) = at
        .model
        .strip_prefix("gpt-")
        .filter(|_| at.provider == KEYED)
    else {
        return false;
    };
    let mut numbers = version
        .split(|c: char| !c.is_ascii_digit())
        .map(|n| n.parse::<u32>().unwrap_or(0));
    let major = numbers.next().unwrap_or(0);
    let minor = if version
        .split_once('.')
        .is_some_and(|(m, _)| m.parse::<u32>().is_ok())
    {
        numbers.next().unwrap_or(0)
    } else {
        0
    };
    (major, minor) >= (6, 2)
}

/// The request's body: the instructions, the history and the tools, then
/// the model's and variant's own fields over them.
pub(crate) fn request(
    at: Signer<'_>,
    system: System<'_>,
    history: &[Message],
    tools: &[Value],
    extra: &Map<String, Value>,
) -> Value {
    let mut items = input(at, history);
    let mut body = json!({
        "model": at.model,
        "stream": true,
        "store": false,
        "include": ["reasoning.encrypted_content"],
    });
    match system {
        System::Instructions(text) => body["instructions"] = json!(text),
        System::First(text) => items.insert(
            0,
            json!({ "role": "developer", "content": [{ "type": "input_text", "text": text }] }),
        ),
    }
    body["input"] = Value::Array(items);
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function", "name": t["name"], "description": t["description"],
                    "parameters": t["parameters"], "strict": false,
                })
            })
            .collect();
        body["tool_choice"] = json!("auto");
    }
    // OpenAI sends requests that carry the same key to the same cache.
    // Without one, the steps of a conversation are spread over its
    // machines, and most are charged whole though they start alike. The
    // first message's ID stands for the conversation: it never changes,
    // and no other session has it.
    if at.provider == KEYED
        && let Some(first) = history.first().map(Message::id).filter(|id| !id.is_empty())
    {
        body["prompt_cache_key"] = json!(first);
    }
    if let Value::Object(fields) = &mut body {
        fields.extend(extra.clone());
    }
    body
}

/// Usage as Responses reports it: input without the parts read from the
/// cache and written to it, output with reasoning.
fn tokens(u: &Value) -> Tokens {
    let input = u["input_tokens"].as_u64().unwrap_or(0);
    let details = &u["input_tokens_details"];
    let cached = details["cached_tokens"].as_u64().unwrap_or(0);
    // Models that charge for writing to the cache count those tokens
    // apart from the ones read; the rest is ordinary input.
    let written = details["cache_write_tokens"].as_u64().unwrap_or(0);
    Tokens {
        input: input.saturating_sub(cached).saturating_sub(written),
        output: u["output_tokens"].as_u64().unwrap_or(0),
        reasoning: u["output_tokens_details"]["reasoning_tokens"]
            .as_u64()
            .unwrap_or(0),
        cache: Cache {
            read: cached,
            write: written,
        },
    }
}

/// Takes one event of the reply; whether more are to come.
///
/// # Errors
///
/// When the event reports an error or a failed response.
pub(crate) fn take(
    reply: &mut Assembler,
    event: &Value,
    on: &mut (dyn FnMut(Piece) + Send),
) -> Result<bool, Error> {
    let key = event["output_index"].as_u64().unwrap_or(0).to_string();
    match event["type"].as_str() {
        Some("response.output_item.added") => {
            let item = &event["item"];
            match item["type"].as_str() {
                Some("function_call") => reply.call_started(
                    key,
                    item["call_id"].as_str().unwrap_or_default(),
                    item["name"].as_str().unwrap_or_default(),
                    on,
                ),
                Some("reasoning") => reply.reasoning("", true, on),
                _ => reply.end_block(on),
            }
        }
        Some("response.output_text.delta") => {
            reply.text(event["delta"].as_str().unwrap_or_default(), on);
        }
        Some("response.reasoning_summary_part.added") => {
            if event["summary_index"].as_u64().unwrap_or(0) > 0 {
                reply.reasoning("\n\n", false, on);
            }
        }
        Some("response.reasoning_summary_text.delta" | "response.reasoning_text.delta") => {
            reply.reasoning(event["delta"].as_str().unwrap_or_default(), false, on);
        }
        Some("response.function_call_arguments.delta") => {
            reply.call_grew(&key, event["delta"].as_str().unwrap_or_default());
        }
        Some("response.output_item.done") => {
            let item = &event["item"];
            if item["type"] == "reasoning"
                && let Some(encrypted) = item["encrypted_content"].as_str()
            {
                reply.sign(json!({ "encrypted_content": encrypted }), on);
            }
            reply.end_block(on);
        }
        Some("response.completed" | "response.incomplete") => {
            let response = &event["response"];
            reply.tokens = tokens(&response["usage"]);
            let called = response["output"]
                .as_array()
                .is_some_and(|o| o.iter().any(|i| i["type"] == "function_call"));
            let cut = response["incomplete_details"]["reason"] == "max_output_tokens";
            reply.finish = Some(
                if cut {
                    "length"
                } else if called {
                    "tool_calls"
                } else {
                    "stop"
                }
                .to_owned(),
            );
            return Ok(false);
        }
        Some("response.failed") => {
            let error = &event["response"]["error"];
            let said = error["message"].as_str().unwrap_or("the request failed");
            let kind = error["code"].as_str().or_else(|| error["type"].as_str());
            return Err(chat::reported(kind, said));
        }
        Some("error") => {
            let said = event["message"]
                .as_str()
                .or_else(|| event["error"]["message"].as_str())
                .unwrap_or("the request failed");
            let kind = event["code"]
                .as_str()
                .or_else(|| event["error"]["code"].as_str())
                .or_else(|| event["error"]["type"].as_str());
            return Err(chat::reported(kind, said));
        }
        _ => {}
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::Block;
    use crate::messages::tests::step;

    const AT: Signer<'static> = Signer {
        provider: "openai",
        model: "gpt",
    };

    #[test]
    fn a_failure_the_server_reports_is_classed_by_its_code() {
        let mut reply = Assembler::default();
        let mut on = |_| {};
        let ended = |event: Value, reply: &mut Assembler, on: &mut (dyn FnMut(Piece) + Send)| {
            take(reply, &event, on).expect_err("an error")
        };
        let busy = json!({ "type": "response.failed", "response": { "error": {
            "code": "server_error", "message": "Our servers are currently overloaded." } } });
        assert!(ended(busy, &mut reply, &mut on).is_retryable());
        let limited =
            json!({ "type": "error", "code": "rate_limit_exceeded", "message": "Slow down." });
        assert!(ended(limited, &mut reply, &mut on).is_retryable());
        let wrong = json!({ "type": "response.failed", "response": { "error": {
            "code": "invalid_prompt", "message": "No." } } });
        assert!(!ended(wrong, &mut reply, &mut on).is_retryable());
    }

    #[test]
    fn a_streamed_reply_keeps_reasoning_text_and_calls() {
        let mut reply = Assembler::default();
        let mut on = |_| {};
        for event in [
            json!({ "type": "response.output_item.added", "output_index": 0, "item": { "type": "reasoning" } }),
            json!({ "type": "response.reasoning_summary_text.delta", "output_index": 0, "delta": "plan" }),
            json!({ "type": "response.output_item.done", "output_index": 0, "item": { "type": "reasoning", "encrypted_content": "enc" } }),
            json!({ "type": "response.output_item.added", "output_index": 1, "item": { "type": "message" } }),
            json!({ "type": "response.output_text.delta", "output_index": 1, "delta": "Hi" }),
            json!({ "type": "response.output_item.done", "output_index": 1, "item": { "type": "message" } }),
            json!({ "type": "response.output_item.added", "output_index": 2, "item": { "type": "function_call", "call_id": "c1", "name": "read" } }),
            json!({ "type": "response.function_call_arguments.delta", "output_index": 2, "delta": "{}" }),
        ] {
            assert!(take(&mut reply, &event, &mut on).expect("taken"));
        }
        let done = json!({ "type": "response.completed", "response": {
            "output": [{ "type": "function_call" }],
            "usage": { "input_tokens": 10, "input_tokens_details": { "cached_tokens": 6 }, "output_tokens": 3 },
        } });
        assert!(!take(&mut reply, &done, &mut on).expect("taken"));
        let reply = reply.finish(&mut on);
        assert_eq!(
            reply.blocks,
            [
                Block::Reasoning("plan".into(), Some(json!({ "encrypted_content": "enc" }))),
                Block::Text("Hi".into()),
                Block::Call {
                    id: "c1".into(),
                    name: "read".into(),
                    arguments: "{}".into()
                },
            ]
        );
        assert_eq!(reply.finish.as_deref(), Some("tool_calls"));
        assert_eq!((reply.tokens.input, reply.tokens.cache.read), (4, 6));
    }

    #[test]
    fn own_reasoning_goes_back_encrypted_and_others_stay_out() {
        let body = request(
            AT,
            System::Instructions("sys"),
            &[step(vec![
                Part::Reasoning {
                    text: "plan".into(),
                    signature: Some(AT.sign(WIRE, &json!({ "encrypted_content": "enc" }))),
                },
                Part::Reasoning {
                    text: "theirs".into(),
                    signature: None,
                },
                Part::Text { text: "Hi".into() },
            ])],
            &[],
            &Map::new(),
        );
        let input = body["input"].as_array().expect("input");
        assert_eq!(input[0]["type"], "reasoning");
        assert_eq!(input[0]["encrypted_content"], "enc");
        assert_eq!(input[1]["type"], "message");
        assert_eq!(input.len(), 2);
        assert_eq!(body["store"], false);
    }

    #[test]
    fn a_conversation_is_named_for_openais_cache_and_no_other() {
        let history = [
            Message::User {
                id: "msg_first".into(),
                time: crate::message::Created { created: 1 },
                text: "hi".into(),
                skills: Vec::new(),
                files: Vec::new(),
            },
            step(vec![Part::Text { text: "Hi".into() }]),
        ];
        let body = request(AT, System::Instructions(""), &history, &[], &Map::new());
        assert_eq!(body["prompt_cache_key"], "msg_first");
        // The next step's request, a message longer, carries the same.
        let body = request(
            AT,
            System::Instructions(""),
            &history[..1],
            &[],
            &Map::new(),
        );
        assert_eq!(body["prompt_cache_key"], "msg_first");

        let other = Signer {
            provider: "elsewhere",
            model: "gpt",
        };
        let body = request(other, System::Instructions(""), &history, &[], &Map::new());
        assert!(body.get("prompt_cache_key").is_none());
    }

    #[test]
    fn the_system_prompt_opens_the_conversation_where_instructions_would_come_last() {
        let at = |model| Signer {
            provider: "openai",
            model,
        };
        for (model, appended) in [
            ("gpt-6.2-sol", true),
            ("gpt-6.2", true),
            ("gpt-6.10-astra", true),
            ("gpt-7", true),
            ("gpt-6.1", false),
            ("gpt-5.6-mini", false),
            ("gpt-6-mini", false),
            ("o4", false),
        ] {
            assert_eq!(appends_instructions(at(model)), appended, "{model}");
        }
        let elsewhere = Signer {
            provider: "elsewhere",
            model: "gpt-6.2-sol",
        };
        assert!(!appends_instructions(elsewhere));
        // Only when asked for, and only there.
        let sol = at("gpt-6.2-sol");
        assert_eq!(
            System::of("Be brief.", sol, false),
            System::Instructions("Be brief.")
        );
        assert_eq!(
            System::of("Be brief.", at("gpt-5.6"), true),
            System::Instructions("Be brief.")
        );

        let history = [Message::User {
            id: "msg_first".into(),
            time: crate::message::Created { created: 1 },
            text: "hi".into(),
            skills: Vec::new(),
            files: Vec::new(),
        }];
        let body = request(
            sol,
            System::of("Be brief.", sol, true),
            &history,
            &[],
            &Map::new(),
        );
        assert!(body.get("instructions").is_none(), "{body}");
        assert_eq!(body["input"][0]["role"], "developer");
        assert_eq!(body["input"][0]["content"][0]["text"], "Be brief.");
        assert_eq!(body["input"][1]["role"], "user");
        let body = request(
            sol,
            System::Instructions("Be brief."),
            &history,
            &[],
            &Map::new(),
        );
        assert_eq!(body["instructions"], "Be brief.");
        assert_eq!(body["input"][0]["role"], "user");
    }

    #[test]
    fn tokens_written_to_the_cache_are_counted_apart() {
        let counted = tokens(&json!({
            "input_tokens": 1_000, "output_tokens": 7,
            "input_tokens_details": { "cached_tokens": 600, "cache_write_tokens": 300 },
        }));
        assert_eq!(counted.input, 100);
        assert_eq!(
            counted.cache,
            Cache {
                read: 600,
                write: 300
            }
        );
        // A model that counts no writes reports none.
        let counted = tokens(&json!({
            "input_tokens": 1_000, "input_tokens_details": { "cached_tokens": 600 },
        }));
        assert_eq!((counted.input, counted.cache.write), (400, 0));
    }
}
