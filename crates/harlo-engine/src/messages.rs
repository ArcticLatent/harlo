//! Anthropic's Messages API: the request a session's history makes, and
//! the events of its streamed reply.
//!
//! Thinking blocks go back with their signatures to the provider and model
//! that wrote them; others' reasoning is left out. The system prompt, the
//! end of the conversation and the end of the request before are marked for
//! the prompt cache.

// Rust guideline compliant 2026-10-09

use serde_json::{Map, Value, json};

use crate::Error;
use crate::chat::{self, Assembler, Piece, Signer};
use crate::message::{Cache, Message, Part, Tokens};

/// The wire's name in reasoning signatures.
const WIRE: &str = "messages";

/// The reply's length when the model's limit isn't known.
const DEFAULT_OUTPUT: u64 = 8_192;

/// A chat completions content part as a Messages content block.
fn block(part: &Value) -> Option<Value> {
    if let Some(picture) = chat::picture(part) {
        return Some(match picture {
            Ok((mime, data)) => json!({
                "type": "image", "source": { "type": "base64", "media_type": mime, "data": data },
            }),
            Err(url) => json!({ "type": "image", "source": { "type": "url", "url": url } }),
        });
    }
    let text = part["text"].as_str().filter(|t| !t.is_empty())?;
    Some(json!({ "type": "text", "text": text }))
}

/// Adds `blocks` as a message from `role`, joining the one before when it
/// was from the same role.
fn push(out: &mut Vec<Value>, role: &str, blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut()
        && last["role"] == role
        && let Some(content) = last["content"].as_array_mut()
    {
        content.extend(blocks);
        return;
    }
    out.push(json!({ "role": role, "content": blocks }));
}

/// The text of a person's message as a text block.
fn text(said: &str) -> Vec<Value> {
    if said.is_empty() {
        Vec::new()
    } else {
        vec![json!({ "type": "text", "text": said })]
    }
}

/// A step's content as the assistant's blocks, its signed reasoning first
/// where it came.
fn assistant_blocks(at: Signer<'_>, content: &[Part]) -> Vec<Value> {
    content
        .iter()
        .filter_map(|part| match part {
            Part::Text { text } if !text.is_empty() => {
                Some(json!({ "type": "text", "text": text }))
            }
            Part::Reasoning { text, signature } => {
                let kept = at.mine(WIRE, signature.as_ref())?;
                if let Some(data) = kept["data"].as_str() {
                    return Some(json!({ "type": "redacted_thinking", "data": data }));
                }
                let signature = kept["signature"].as_str()?;
                Some(json!({ "type": "thinking", "thinking": text, "signature": signature }))
            }
            Part::Tool {
                id, name, state, ..
            } => Some(json!({
                "type": "tool_use", "id": id, "name": name, "input": state.input(),
            })),
            Part::Text { .. } => None,
        })
        .collect()
}

/// The session's messages as Messages' messages.
fn messages(at: Signer<'_>, history: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in crate::compaction::since_summary(history) {
        match message {
            Message::Compaction(c) => {
                if let Some(summary) = c.summary.as_ref().filter(|_| c.status == "completed") {
                    let said = format!("{}{summary}", crate::compaction::SUMMARY_HEADING);
                    push(&mut out, "user", text(&said));
                }
            }
            Message::User {
                text: said,
                skills,
                files,
                ..
            } => {
                let mut whole: String = skills.iter().map(|s| s.text.as_str()).collect();
                whole.push_str(said);
                let mut blocks = text(&whole);
                blocks.extend(
                    files
                        .iter()
                        .filter_map(crate::attachment::File::content)
                        .filter_map(|p| block(&p)),
                );
                push(&mut out, "user", blocks);
            }
            Message::Synthetic { text: said, .. } => push(&mut out, "user", text(said)),
            Message::Assistant(step) => {
                push(&mut out, "assistant", assistant_blocks(at, &step.content));
                let results: Vec<Value> = step
                    .content
                    .iter()
                    .filter_map(|part| {
                        let Part::Tool { id, state, .. } = part else {
                            return None;
                        };
                        let mut content = text(&chat::result_text(state));
                        content.extend(chat::result_pictures(state).iter().filter_map(block));
                        if content.is_empty() {
                            content = text("(no output)");
                        }
                        let failed = matches!(state, crate::message::ToolState::Error { .. });
                        Some(json!({
                            "type": "tool_result", "tool_use_id": id, "content": content,
                            "is_error": failed,
                        }))
                    })
                    .collect();
                push(&mut out, "user", results);
            }
            Message::Idle { .. } | Message::Shell(_) => {}
        }
    }
    out
}

/// Marks `message`'s last block that can carry a cache point, if it has one.
///
/// Thinking blocks can't carry one.
fn mark_end(message: &mut Value) {
    let block = message["content"].as_array_mut().and_then(|blocks| {
        blocks
            .iter_mut()
            .rev()
            .find(|b| !matches!(b["type"].as_str(), Some("thinking" | "redacted_thinking")))
    });
    if let Some(block) = block {
        block["cache_control"] = json!({ "type": "ephemeral" });
    }
}

/// Marks the conversation for the prompt cache, at two places.
///
/// Its end is marked, so the next request reads all of this one. So is the
/// end of the message before the model's last reply, which is where the
/// request before this one ended: the provider looks only a few blocks back
/// from a mark for what it has, and a reply of many blocks with their
/// results would put the last request's end out of its sight.
fn mark(messages: &mut [Value]) {
    let replied = messages.iter().rposition(|m| m["role"] == "assistant");
    if let Some(before) = replied.and_then(|at| at.checked_sub(1)) {
        mark_end(&mut messages[before]);
    }
    if let Some(last) = messages.last_mut() {
        mark_end(last);
    }
}

/// The request's body: the system prompt, the history and the tools, then
/// the model's and variant's own fields over them.
pub(crate) fn request(
    at: Signer<'_>,
    output: Option<u64>,
    system: &str,
    history: &[Message],
    tools: &[Value],
    extra: &Map<String, Value>,
) -> Value {
    let mut messages = messages(at, history);
    mark(&mut messages);
    let mut body = json!({
        "model": at.model,
        "max_tokens": output.unwrap_or(DEFAULT_OUTPUT),
        "stream": true,
        "system": [{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }],
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t["name"], "description": t["description"],
                    "input_schema": t["parameters"],
                })
            })
            .collect();
    }
    if let Value::Object(fields) = &mut body {
        fields.extend(extra.clone());
    }
    body
}

/// Usage as Messages reports it.
fn usage(u: &Value, tokens: &mut Tokens) {
    let n = |k: &str| u[k].as_u64();
    if let Some(input) = n("input_tokens") {
        tokens.input = input;
    }
    if let Some(output) = n("output_tokens") {
        tokens.output = output;
    }
    tokens.cache = Cache {
        read: n("cache_read_input_tokens").unwrap_or(tokens.cache.read),
        write: n("cache_creation_input_tokens").unwrap_or(tokens.cache.write),
    };
}

/// Takes one event of the reply; whether more are to come.
///
/// # Errors
///
/// When the event reports an error.
pub(crate) fn take(
    reply: &mut Assembler,
    event: &Value,
    on: &mut (dyn FnMut(Piece) + Send),
) -> Result<bool, Error> {
    let index = event["index"].as_u64().unwrap_or(0).to_string();
    match event["type"].as_str() {
        Some("message_start") => usage(&event["message"]["usage"], &mut reply.tokens),
        Some("content_block_start") => {
            let b = &event["content_block"];
            match b["type"].as_str() {
                Some("tool_use") => reply.call_started(
                    index,
                    b["id"].as_str().unwrap_or_default(),
                    b["name"].as_str().unwrap_or_default(),
                    on,
                ),
                Some("thinking") => {
                    reply.reasoning(b["thinking"].as_str().unwrap_or_default(), true, on);
                }
                Some("redacted_thinking") => {
                    reply.sign(json!({ "data": b["data"] }), on);
                }
                _ => {
                    reply.end_block(on);
                    reply.text(b["text"].as_str().unwrap_or_default(), on);
                }
            }
        }
        Some("content_block_delta") => {
            let d = &event["delta"];
            match d["type"].as_str() {
                Some("text_delta") => reply.text(d["text"].as_str().unwrap_or_default(), on),
                Some("thinking_delta") => {
                    reply.reasoning(d["thinking"].as_str().unwrap_or_default(), false, on);
                }
                Some("signature_delta") => {
                    reply.sign(json!({ "signature": d["signature"] }), on);
                }
                Some("input_json_delta") => {
                    reply.call_grew(&index, d["partial_json"].as_str().unwrap_or_default());
                }
                _ => {}
            }
        }
        Some("content_block_stop") => reply.end_block(on),
        Some("message_delta") => {
            usage(&event["usage"], &mut reply.tokens);
            if let Some(why) = event["delta"]["stop_reason"].as_str() {
                reply.finish = Some(
                    match why {
                        "end_turn" | "stop_sequence" => "stop",
                        "tool_use" => "tool_calls",
                        "max_tokens" => "length",
                        other => other,
                    }
                    .to_owned(),
                );
            }
        }
        Some("message_stop") => return Ok(false),
        Some("error") => {
            let said = event["error"]["message"]
                .as_str()
                .unwrap_or("the request failed");
            return Err(chat::reported(said));
        }
        _ => {}
    }
    Ok(true)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chat::Block;

    const AT: Signer<'static> = Signer {
        provider: "anthropic",
        model: "claude",
    };

    #[test]
    fn a_streamed_reply_keeps_its_signed_thinking_text_and_call() {
        let mut reply = Assembler::default();
        let mut pieces = Vec::new();
        let mut on = |p| pieces.push(p);
        for event in [
            json!({ "type": "message_start", "message": { "usage": { "input_tokens": 10, "cache_read_input_tokens": 4, "output_tokens": 1 } } }),
            json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "" } }),
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "hmm" } }),
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "signature_delta", "signature": "sig" } }),
            json!({ "type": "content_block_stop", "index": 0 }),
            json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "text", "text": "" } }),
            json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": "Hi" } }),
            json!({ "type": "content_block_stop", "index": 1 }),
            json!({ "type": "content_block_start", "index": 2, "content_block": { "type": "tool_use", "id": "t1", "name": "read", "input": {} } }),
            json!({ "type": "content_block_delta", "index": 2, "delta": { "type": "input_json_delta", "partial_json": "{\"path\":" } }),
            json!({ "type": "content_block_delta", "index": 2, "delta": { "type": "input_json_delta", "partial_json": "\"a\"}" } }),
            json!({ "type": "content_block_stop", "index": 2 }),
            json!({ "type": "message_delta", "delta": { "stop_reason": "tool_use" }, "usage": { "output_tokens": 7 } }),
        ] {
            assert!(take(&mut reply, &event, &mut on).expect("taken"));
        }
        assert!(!take(&mut reply, &json!({ "type": "message_stop" }), &mut on).expect("taken"));
        let reply = reply.finish(&mut on);
        assert_eq!(
            reply.blocks,
            [
                Block::Reasoning("hmm".into(), Some(json!({ "signature": "sig" }))),
                Block::Text("Hi".into()),
                Block::Call {
                    id: "t1".into(),
                    name: "read".into(),
                    arguments: "{\"path\":\"a\"}".into()
                },
            ]
        );
        assert_eq!(reply.finish.as_deref(), Some("tool_calls"));
        assert_eq!(
            (
                reply.tokens.input,
                reply.tokens.output,
                reply.tokens.cache.read
            ),
            (10, 7, 4)
        );
    }

    /// A step whose content is `content`.
    pub(crate) fn step(content: Vec<Part>) -> Message {
        Message::Assistant(Box::new(crate::message::Assistant {
            id: "a".into(),
            time: crate::message::StepTime::default(),
            agent: "build".into(),
            model: crate::ModelRef {
                provider: "anthropic".into(),
                model: "claude".into(),
                variant: None,
            },
            content,
            finish: None,
            raw_finish: None,
            cost: 0.0,
            tokens: Tokens::default(),
            error: None,
            snapshot: None,
        }))
    }

    #[test]
    fn history_sends_own_thinking_back_and_results_as_the_person() {
        let other = Signer {
            provider: "x",
            model: "y",
        };
        let sent = request(
            AT,
            None,
            "sys",
            &[step(vec![
                Part::Reasoning {
                    text: "hmm".into(),
                    signature: Some(AT.sign(WIRE, &json!({ "signature": "sig" }))),
                },
                Part::Reasoning {
                    text: "other".into(),
                    signature: Some(other.sign(WIRE, &json!({ "signature": "no" }))),
                },
                Part::Text {
                    text: "Reading.".into(),
                },
            ])],
            &[],
            &Map::new(),
        );
        let blocks = sent["messages"][0]["content"].as_array().expect("blocks");
        assert_eq!(blocks[0]["type"], "thinking");
        assert_eq!(blocks[0]["signature"], "sig");
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1]["cache_control"]["type"], "ephemeral");
        assert_eq!(sent["max_tokens"], DEFAULT_OUTPUT);
    }

    /// A person's message saying `said`.
    fn says(said: &str) -> Message {
        Message::User {
            id: said.into(),
            time: crate::message::Created { created: 0 },
            text: said.into(),
            skills: Vec::new(),
            files: Vec::new(),
        }
    }

    /// A step of `calls` tool calls, each after a line of text.
    fn calling(calls: usize) -> Message {
        step(
            (0..calls)
                .flat_map(|n| {
                    [
                        Part::Text {
                            text: format!("Reading {n}."),
                        },
                        Part::Tool {
                            id: format!("t{n}"),
                            name: "read".into(),
                            state: crate::message::ToolState::Completed {
                                input: Map::new(),
                                content: vec![json!({ "type": "text", "text": "read" })],
                                metadata: Map::new(),
                            },
                            time: crate::message::ToolTime::default(),
                        },
                    ]
                })
                .collect(),
        )
    }

    /// Where a request's messages are marked: (message, block) pairs.
    fn marks(sent: &Value) -> Vec<(usize, usize)> {
        let messages = sent["messages"].as_array().expect("messages");
        messages
            .iter()
            .enumerate()
            .flat_map(|(m, message)| {
                let blocks = message["content"].as_array().expect("blocks");
                blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| !b["cache_control"].is_null())
                    .map(move |(b, _)| (m, b))
            })
            .collect()
    }

    #[test]
    fn a_request_marks_its_end_and_where_the_one_before_ended() {
        // A reply of thirty blocks, more than a provider looks back over.
        let history = [says("Go."), calling(2), calling(15)];
        let send = |upto: usize| request(AT, None, "sys", &history[..upto], &[], &Map::new());
        let (first, second, third) = (send(1), send(2), send(3));
        // The first request has one end only.
        assert_eq!(marks(&first), [(0, 0)]);
        // Each later one marks its own end, and the end of the last.
        let end = |sent: &Value| *marks(sent).last().expect("an end");
        assert_eq!(marks(&second), [end(&first), (2, 1)]);
        assert_eq!(marks(&third), [end(&second), (4, 14)]);
        // With the system prompt's, that is within the four a request may have.
        assert!(marks(&third).len() < 4);
    }

    #[test]
    fn a_mark_is_never_on_thinking() {
        let sent = request(
            AT,
            None,
            "sys",
            &[step(vec![
                Part::Text {
                    text: "Done.".into(),
                },
                Part::Reasoning {
                    text: "hmm".into(),
                    signature: Some(AT.sign(WIRE, &json!({ "signature": "sig" }))),
                },
            ])],
            &[],
            &Map::new(),
        );
        assert_eq!(marks(&sent), [(0, 0)]);
    }
}
