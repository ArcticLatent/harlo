//! Synthetic harness for UI development and benchmarks. It speaks the
//! normalized event vocabulary directly, so it exercises the same reducer
//! and UI paths as pi/omp without spending tokens.
//!
//! Commands understood:
//! - `prompt`: streams a canned answer (thinking, a tool call, markdown).
//!   A message containing `fast` streams at ~240 deltas/s instead of ~60;
//!   `edit` makes the call an edit of notes.md, `tests` a test run.
//! - `abort`: stops the current stream.
//! - `harlo_demo_history { count }`: emits `count` finished exchanges at once.
//! - A prompt containing `ask` sends an omp-style `ask` request (two
//!   questions); one containing `choose` sends a `select` with option
//!   descriptions. Answers come back as an info notice.
//! - A prompt containing `subagents` starts two pretend omp subagents
//!   (`get_subagents`, `cancel_subagent`, `steer_subagent` answer for them;
//!   each has a transcript file in omp's format).
//! - A prompt containing `todos` runs a pretend `todo` tool call (the phases
//!   come back from `get_state`); `queue demo` fills the message queue
//!   (`remove_queued_message` / `promote_queued_message` work on it); `btw`
//!   streams a side answer.
//! - A prompt containing `approve` asks for approval before its tool call,
//!   as omp does in `always-ask` mode (`Allow tool: bash` with Approve/Deny).
//! - `get_available_models` / `set_model`, `get_available_thinking_levels` /
//!   `set_thinking_level` and `set_session_name` change what `get_state`
//!   reports.
//! - Each prompt costs $0.25 (`get_session_stats`), for spending limits.
//! - `goal` (create / get / pause / resume / drop) runs a pretend omp goal:
//!   it uses tokens while active and completes after a few seconds, or
//!   stops at its token budget (`goal_updated` reports each change).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::event::{BlockKind, HarnessEvent, HarnessKind};
use crate::process::{Harness, Outgoing};

pub(crate) fn spawn() -> (Harness, mpsc::UnboundedReceiver<HarnessEvent>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Outgoing>();
    let (events, events_rx) = mpsc::unbounded_channel();
    let pending = Arc::default();
    let harness = Harness::from_parts(HarnessKind::Demo, tx, Arc::clone(&pending));

    tokio::spawn(async move {
        let _ = events.send(HarnessEvent::Ready);
        let seq = Arc::new(AtomicU64::new(0));
        let abort = Arc::new(AtomicBool::new(false));
        let subagents: Arc<std::sync::Mutex<Vec<Value>>> = Arc::default();
        let todos: Arc<std::sync::Mutex<Value>> = Arc::new(std::sync::Mutex::new(json!([])));
        let queue: Arc<std::sync::Mutex<(Vec<String>, Vec<String>)>> = Arc::default();
        let approval: Approval = Arc::default();
        let goal: Arc<std::sync::Mutex<Value>> = Arc::new(std::sync::Mutex::new(Value::Null));
        // The model, thinking level and name (`get_state`), as set.
        let mut model = json!({ "provider": "anthropic", "id": "claude-demo", "name": "Claude Demo" });
        let (mut thinking, mut name) = ("medium".to_owned(), String::new());
        let mut cost = 0.0f64;
        while let Some(out) = rx.recv().await {
            let Outgoing::Frame(cmd) = out else { break };
            let id = cmd["id"].as_str().map(str::to_owned);
            let ty = cmd["type"].as_str().unwrap_or_default().to_owned();
            if ty == "prompt" {
                cost += 0.25;
            }
            let reply = |frame: Value| {
                let waiter = id.as_deref().and_then(|id| pending.lock().unwrap().remove(id));
                if let Some(w) = waiter {
                    let _ = w.send(frame);
                }
            };
            let respond = |data: Value| reply(json!({"type":"response","id":id,"command":ty,"success":true,"data":data}));
            let fail = |error: &str| reply(json!({"type":"response","id":id,"command":ty,"success":false,"error":error}));
            match ty.as_str() {
                "prompt" if cmd["message"].as_str().is_some_and(|m| m.contains("ask") || m.contains("choose")) => {
                    respond(json!({"disposition":"started"}));
                    let text = cmd["message"].as_str().unwrap_or_default();
                    let n = seq.fetch_add(1, Ordering::Relaxed);
                    let _ = events.send(HarnessEvent::MessageEnd { id: format!("u{n}"), message: user_message(text) });
                    // "choose" first: the "Ask first" note mentions the ask tool.
                    let request = if !text.contains("choose") {
                        json!({
                            "type": "extension_ui_request", "id": format!("ui_{n}"), "method": "ask", "timeout": 120000,
                            "questions": [
                                { "id": "db", "header": "Storage", "question": "Which database should the cache use?",
                                  "options": [
                                      { "label": "SQLite", "description": "One file next to the app; no server." },
                                      { "label": "Postgres", "description": "Shared with the rest of the stack.", "preview": "DATABASE_URL=postgres://localhost/harlo" },
                                      { "label": "In memory", "description": "Fastest, lost on restart." }
                                  ], "recommended": 0 },
                                { "id": "features", "header": "Scope", "question": "Which features go in the first release?", "multi": true,
                                  "options": [{ "label": "Search" }, { "label": "Sync" }, { "label": "Export" }] }
                            ]
                        })
                    } else {
                        json!({
                            "type": "extension_ui_request", "id": format!("ui_{n}"), "method": "select", "title": "Pick a branch strategy",
                            "options": ["Rebase", "Merge"],
                            "optionDetails": [{ "description": "Linear history." }, { "description": "Keeps the merge commit." }]
                        })
                    };
                    let _ = events.send(HarnessEvent::UiRequest(request));
                }
                "prompt" if cmd["message"].as_str().is_some_and(|m| m.contains("subagents")) => {
                    respond(json!({"disposition":"started"}));
                    let n = seq.fetch_add(1, Ordering::Relaxed);
                    let _ = events.send(HarnessEvent::MessageEnd { id: format!("u{n}"), message: user_message(cmd["message"].as_str().unwrap_or_default()) });
                    tokio::spawn(run_subagents(events.clone(), Arc::clone(&subagents)));
                }
                "prompt" if cmd["message"].as_str().is_some_and(|m| m.contains("todos")) => {
                    respond(json!({"disposition":"started"}));
                    let n = seq.fetch_add(1, Ordering::Relaxed);
                    let _ = events.send(HarnessEvent::MessageEnd { id: format!("u{n}"), message: user_message(cmd["message"].as_str().unwrap_or_default()) });
                    *todos.lock().unwrap() = json!([
                        { "name": "Investigate", "tasks": [
                            { "content": "Read the build script", "status": "completed" },
                            { "content": "Find the failing test", "status": "in_progress" } ] },
                        { "name": "Fix", "tasks": [
                            { "content": "Patch the parser", "status": "pending" },
                            { "content": "Run the suite", "status": "blocked", "blocker": "needs the fix first" } ] }
                    ]);
                    let call = format!("todo-{n}");
                    let _ = events.send(HarnessEvent::ToolStart { call_id: call.clone(), name: "todo".into(), args: json!({ "op": "init" }) });
                    let _ = events.send(HarnessEvent::ToolEnd { call_id: call, result: json!({ "content": [{ "type": "text", "text": "4 tasks" }] }), is_error: false });
                }
                "prompt" if cmd["message"].as_str().is_some_and(|m| m.contains("queue demo")) => {
                    respond(json!({"disposition":"started"}));
                    *queue.lock().unwrap() = (vec!["Use the existing parser".into()], vec!["Then run the tests".into(), "Update the changelog".into()]);
                    let q = queue.lock().unwrap().clone();
                    let _ = events.send(HarnessEvent::QueueChanged { steering: q.0, follow_up: q.1 });
                }
                "remove_queued_message" | "promote_queued_message" => {
                    let text = cmd["message"].as_str().unwrap_or_default().to_owned();
                    let mut q = queue.lock().unwrap();
                    let found = if ty == "remove_queued_message" {
                        let before = q.0.len() + q.1.len();
                        q.0.retain(|m| *m != text);
                        q.1.retain(|m| *m != text);
                        before != q.0.len() + q.1.len()
                    } else if let Some(i) = q.1.iter().position(|m| *m == text) {
                        let m = q.1.remove(i);
                        q.0.push(m);
                        true
                    } else {
                        false
                    };
                    let snapshot = q.clone();
                    drop(q);
                    respond(if ty == "remove_queued_message" { json!({ "removed": found }) } else { json!({ "promoted": found }) });
                    let _ = events.send(HarnessEvent::QueueChanged { steering: snapshot.0, follow_up: snapshot.1 });
                }
                "get_session_stats" => respond(json!({ "contextUsage": { "tokens": 0, "contextWindow": 0 }, "cost": cost })),
                "get_state" => {
                    let mut state = json!({ "model": model, "thinkingLevel": thinking, "todoPhases": todos.lock().unwrap().clone(), "goal": goal_state(&goal.lock().unwrap()) });
                    if !name.is_empty() {
                        state["sessionName"] = json!(name);
                    }
                    respond(state)
                }
                "get_available_models" => respond(json!({ "models": DEMO_MODELS.iter().map(|(p, id, n, r)| json!({ "provider": p, "id": id, "name": n, "reasoning": r })).collect::<Vec<_>>() })),
                "set_model" => match DEMO_MODELS.iter().find(|(p, id, _, _)| cmd["provider"] == *p && cmd["modelId"] == *id) {
                    Some((p, id, n, _)) => {
                        model = json!({ "provider": p, "id": id, "name": n });
                        respond(model.clone());
                    }
                    None => fail("no such model"),
                },
                "get_available_thinking_levels" => respond(json!({ "levels": ["off", "low", "medium", "high"] })),
                "set_thinking_level" => {
                    thinking = cmd["level"].as_str().unwrap_or("medium").to_owned();
                    respond(Value::Null);
                }
                "set_session_name" => {
                    name = cmd["name"].as_str().unwrap_or_default().to_owned();
                    respond(Value::Null);
                }
                "goal" => {
                    let op = cmd["op"].as_str().unwrap_or_default();
                    let mut g = goal.lock().unwrap();
                    let status = g["status"].as_str().unwrap_or_default().to_owned();
                    let open = matches!(status.as_str(), "active" | "paused" | "budget-limited");
                    let error = match op {
                        "get" => None,
                        "create" if cmd["objective"].as_str().is_none_or(|o| o.trim().is_empty()) => Some("objective is required when op=create"),
                        "create" if open => Some("A goal is already active; drop it first."),
                        "create" => {
                            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                            *g = json!({ "id": format!("goal-{}", seq.fetch_add(1, Ordering::Relaxed)), "objective": cmd["objective"], "status": "active",
                                         "tokensUsed": 0, "timeUsedSeconds": 0, "createdAt": now, "updatedAt": now });
                            if let Some(budget) = cmd["token_budget"].as_u64() {
                                g["tokenBudget"] = json!(budget);
                            }
                            tokio::spawn(run_goal(events.clone(), Arc::clone(&goal)));
                            None
                        }
                        "pause" if status == "active" || status == "budget-limited" => { g["status"] = json!("paused"); None }
                        "resume" if status == "paused" => { g["status"] = json!("active"); None }
                        "drop" if open => { g["status"] = json!("dropped"); None }
                        _ => Some("No goal to change."),
                    };
                    match error {
                        Some(e) => fail(e),
                        None => {
                            if op != "get" {
                                let _ = events.send(HarnessEvent::Goal(g.clone()));
                            }
                            if g["status"] == "dropped" {
                                *g = Value::Null;
                            }
                            respond(json!({ "goal": if g.is_null() { Value::Null } else { g.clone() }, "state": goal_state(&g) }));
                        }
                    }
                }
                "btw" => {
                    let question = cmd["question"].as_str().unwrap_or_default().to_owned();
                    let id = format!("btw-{}", seq.fetch_add(1, Ordering::Relaxed));
                    let record = json!({ "id": id, "question": question, "answer": "", "status": "running", "leafId": null, "createdAt": 0, "updatedAt": 0 });
                    let _ = events.send(HarnessEvent::Btw(json!({ "type": "btw_record", "record": record })));
                    respond(json!({ "record": record }));
                    let events = events.clone();
                    tokio::spawn(async move {
                        let mut answer = String::new();
                        for piece in ["The lock ", "keeps two ", "workers from ", "writing the cache at once."] {
                            tokio::time::sleep(Duration::from_millis(120)).await;
                            answer.push_str(piece);
                            let _ = events.send(HarnessEvent::Btw(json!({ "type": "btw_delta", "recordId": id, "delta": piece })));
                        }
                        let done = json!({ "id": id, "question": question, "answer": answer, "status": "complete", "leafId": null, "createdAt": 0, "updatedAt": 1 });
                        let _ = events.send(HarnessEvent::Btw(json!({ "type": "btw_record", "record": done })));
                    });
                }
                // Like omp, the list leaves out detached (background) subagents;
                // only their events tell about them.
                "get_subagents" => respond(json!({ "subagents": subagents.lock().unwrap().iter().filter(|a| a["detached"] != true).cloned().collect::<Vec<_>>() })),
                "cancel_subagent" => {
                    let target = cmd["subagentId"].as_str().unwrap_or_default().to_owned();
                    let mut cancelled = false;
                    for a in subagents.lock().unwrap().iter_mut() {
                        if a["id"] == target.as_str() && a["status"] == "running" {
                            a["status"] = json!("aborted");
                            cancelled = true;
                        }
                    }
                    respond(json!({ "cancelled": cancelled }));
                    if cancelled {
                        let _ = events.send(HarnessEvent::SubagentActivity(json!({ "type": "subagent_lifecycle", "payload": { "id": target, "status": "aborted" } })));
                    }
                }
                "steer_subagent" => {
                    respond(Value::Null);
                    let text = format!("Demo steered {}: {}", cmd["subagentId"].as_str().unwrap_or_default(), cmd["message"].as_str().unwrap_or_default());
                    let _ = events.send(HarnessEvent::Notice { level: crate::event::NoticeLevel::Info, text, system: false });
                }
                "extension_ui_response" if approval.lock().unwrap().as_ref().is_some_and(|(id, _)| cmd["id"].as_str() == Some(id)) => {
                    let (_, reply) = approval.lock().unwrap().take().unwrap();
                    let _ = reply.send(cmd["value"].as_str().unwrap_or("Deny").to_owned());
                }
                "extension_ui_response" => {
                    let mut answer = cmd.clone();
                    if let Some(o) = answer.as_object_mut() {
                        o.remove("type");
                        o.remove("id");
                    }
                    let _ = events.send(HarnessEvent::Notice { level: crate::event::NoticeLevel::Info, text: format!("Demo received: {answer}"), system: false });
                }
                "prompt" => {
                    respond(json!({"disposition":"started"}));
                    abort.store(false, Ordering::Relaxed);
                    let text = cmd["message"].as_str().unwrap_or_default().to_owned();
                    let rate = if text.contains("fast") { 240 } else { 60 };
                    let ask = text.contains("approve").then(|| Arc::clone(&approval));
                    // "later": it starts after a while (a phone hangs up first, as
                    // the app does in the background).
                    let wait = Duration::from_secs(if text.contains("later") { 2 } else { 0 });
                    let (events, seq, abort) = (events.clone(), seq.clone(), abort.clone());
                    tokio::spawn(async move {
                        tokio::time::sleep(wait).await;
                        stream_answer(events, seq, abort, text, rate, ask).await
                    });
                }
                "abort" => {
                    abort.store(true, Ordering::Relaxed);
                    respond(Value::Null);
                }
                "harlo_demo_history" => {
                    let count = cmd["count"].as_u64().unwrap_or(500);
                    for i in 0..count {
                        emit_exchange(&events, &seq, i);
                    }
                    respond(json!({ "count": count }));
                }
                _ => respond(Value::Null),
            }
        }
        let _ = events.send(HarnessEvent::Exited { code: Some(0), stderr_tail: String::new() });
    });

    (harness, events_rx)
}

/// The models the demo offers: provider, id, name, reasoning.
const DEMO_MODELS: [(&str, &str, &str, bool); 2] = [("anthropic", "claude-demo", "Claude Demo", true), ("openai", "gpt-demo", "GPT Demo", false)];

fn next_id(seq: &AtomicU64) -> String {
    format!("demo-m{}", seq.fetch_add(1, Ordering::Relaxed))
}

fn user_message(text: &str) -> Value {
    json!({"role":"user","content":[{"type":"text","text":text}]})
}

fn emit_exchange(events: &mpsc::UnboundedSender<HarnessEvent>, seq: &AtomicU64, i: u64) {
    let uid = next_id(seq);
    let prompt = format!("History prompt #{i}: explain item {i}");
    let _ = events.send(HarnessEvent::MessageStart { id: uid.clone(), message: user_message(&prompt) });
    let _ = events.send(HarnessEvent::MessageEnd { id: uid, message: user_message(&prompt) });
    let aid = next_id(seq);
    let call_id = format!("call-h{i}");
    let message = json!({
        "role": "assistant",
        "content": [
            {"type":"thinking","thinking":format!("Considering item {i}.")},
            {"type":"toolCall","id":call_id,"name":"bash","arguments":{"command":format!("ls -la src/item_{i}")}},
            {"type":"text","text":answer_markdown(i)},
        ],
        "stopReason":"stop",
    });
    let _ = events.send(HarnessEvent::MessageStart { id: aid.clone(), message: json!({"role":"assistant","content":[]}) });
    let _ = events.send(HarnessEvent::ToolStart { call_id: call_id.clone(), name: "bash".into(), args: json!({"command":format!("ls -la src/item_{i}")}) });
    let _ = events.send(HarnessEvent::ToolEnd {
        call_id,
        is_error: false,
        result: json!({"content":[{"type":"text","text":"total 8\n-rw-r--r-- 1 user user 120 main.rs\n-rw-r--r-- 1 user user  64 lib.rs"}]}),
    });
    let _ = events.send(HarnessEvent::MessageEnd { id: aid, message });
}

/// omp's `GoalModeState` for a goal (null without one).
fn goal_state(goal: &Value) -> Value {
    if goal.is_null() {
        return Value::Null;
    }
    json!({ "enabled": goal["status"] == "active", "mode": "active", "goal": goal })
}

/// Uses tokens while the goal is active; completes after eight active
/// ticks, or stops at the token budget.
async fn run_goal(events: mpsc::UnboundedSender<HarnessEvent>, goal: Arc<std::sync::Mutex<Value>>) {
    let mut active_ticks = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let mut g = goal.lock().unwrap();
        match g["status"].as_str() {
            Some("active") => {}
            Some("paused") => continue,
            _ => return,
        }
        active_ticks += 1;
        let used = g["tokensUsed"].as_u64().unwrap_or(0) + 3500;
        g["tokensUsed"] = json!(used);
        g["timeUsedSeconds"] = json!(g["timeUsedSeconds"].as_u64().unwrap_or(0) + 1);
        if g["tokenBudget"].as_u64().is_some_and(|b| used >= b) {
            g["status"] = json!("budget-limited");
        } else if active_ticks >= 8 {
            g["status"] = json!("complete");
        }
        let _ = events.send(HarnessEvent::Goal(g.clone()));
        if g["status"] != "active" {
            return;
        }
    }
}

/// The approval the demo waits for: request id and where the answer goes.
type Approval = Arc<std::sync::Mutex<Option<(String, oneshot::Sender<String>)>>>;

async fn stream_answer(
    events: mpsc::UnboundedSender<HarnessEvent>,
    seq: Arc<AtomicU64>,
    abort: Arc<AtomicBool>,
    prompt: String,
    rate: u64,
    approval: Option<Approval>,
) {
    let tick = Duration::from_micros(1_000_000 / rate);
    let send = |ev| {
        let _ = events.send(ev);
    };
    send(HarnessEvent::RunStarted);
    let uid = next_id(&seq);
    send(HarnessEvent::MessageStart { id: uid.clone(), message: user_message(&prompt) });
    send(HarnessEvent::MessageEnd { id: uid, message: user_message(&prompt) });

    let aid = next_id(&seq);
    send(HarnessEvent::MessageStart { id: aid.clone(), message: json!({"role":"assistant","content":[]}) });

    let thinking = "The user wants a demonstration. I'll list the files first, then write an answer \
                    with headings, a list, a table and a code block so every renderer is exercised.";
    let answer = answer_markdown(seq.load(Ordering::Relaxed));
    let call_id = format!("call-{aid}");
    // "approve goal": the call is omp's own goal tool, which asks for
    // approval with nothing but its op (as omp does outside yolo mode).
    // "edit": an edit of notes.md; "tests": a test run that passes.
    let (tool, args) = if approval.is_some() && prompt.contains("goal") {
        ("goal", json!({"op":"complete"}))
    } else if prompt.contains("edit") {
        ("edit", json!({"path":"notes.md","oldText":"one\ntwo","newText":"one\n2\nthree"}))
    } else if prompt.contains("tests") {
        ("bash", json!({"command":"cargo test"}))
    } else {
        ("bash", json!({"command":"ls -la"}))
    };
    let output: &[&str] = match tool {
        "edit" => &["Edited notes.md"],
        _ if prompt.contains("tests") => &["running 12 tests", "test result: ok. 12 passed; 0 failed"],
        _ => &["total 24", "drwxr-xr-x  4 user user 4096 .", "-rw-r--r--  1 user user  812 Cargo.toml", "drwxr-xr-x  3 user user 4096 crates", "-rw-r--r--  1 user user 9120 PLAN.md"],
    };

    send(HarnessEvent::BlockStart { id: aid.clone(), index: 0, kind: BlockKind::Thinking, tool: None });
    for piece in pieces(thinking) {
        if abort.load(Ordering::Relaxed) {
            return finish_aborted(&events, aid).await;
        }
        send(HarnessEvent::BlockDelta { id: aid.clone(), index: 0, kind: BlockKind::Thinking, delta: piece.to_owned() });
        tokio::time::sleep(tick).await;
    }
    send(HarnessEvent::BlockEnd { id: aid.clone(), index: 0, kind: BlockKind::Thinking, content: json!(thinking) });

    send(HarnessEvent::BlockStart { id: aid.clone(), index: 1, kind: BlockKind::ToolCall, tool: Some((call_id.clone(), tool.into())) });
    let tool_call = json!({"type":"toolCall","id":call_id,"name":tool,"arguments":args});
    send(HarnessEvent::BlockEnd { id: aid.clone(), index: 1, kind: BlockKind::ToolCall, content: tool_call.clone() });
    send(HarnessEvent::ToolStart { call_id: call_id.clone(), name: tool.into(), args: args.clone() });
    if let Some(approval) = approval {
        let (reply, answer) = oneshot::channel();
        let request_id = format!("ui_{}", next_id(&seq));
        *approval.lock().unwrap() = Some((request_id.clone(), reply));
        send(HarnessEvent::UiRequest(json!({
            "type": "extension_ui_request", "id": request_id, "method": "select",
            "title": if tool == "goal" { "Allow tool: goal".to_owned() } else { "Allow tool: bash\nReason: demo approval\nCommand: ls -la".to_owned() },
            "options": ["Approve", "Deny"]
        })));
        if answer.await.as_deref() != Ok("Approve") {
            let denied = format!("Tool call denied by user: {tool}");
            send(HarnessEvent::ToolEnd { call_id, is_error: true, result: json!({"content":[{"type":"text","text":denied}]}) });
            let message = json!({ "role":"assistant", "content":[{"type":"thinking","thinking":thinking}, tool_call], "stopReason":"stop" });
            send(HarnessEvent::MessageEnd { id: aid, message });
            send(HarnessEvent::RunEnded);
            send(HarnessEvent::Settled);
            return;
        }
    }
    let mut out = String::new();
    for line in output {
        out.push_str(line);
        out.push('\n');
        send(HarnessEvent::ToolUpdate { call_id: call_id.clone(), partial: json!({"content":[{"type":"text","text":out}]}) });
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
    send(HarnessEvent::ToolEnd { call_id: call_id.clone(), is_error: false, result: json!({"content":[{"type":"text","text":out}]}) });
    // As pi and omp keep it: the result as a message of its own.
    send(HarnessEvent::MessageEnd { id: next_id(&seq), message: json!({
        "role": "toolResult", "toolCallId": call_id, "toolName": tool, "content": [{"type":"text","text":out}], "isError": false, "timestamp": now_ms(),
    }) });

    send(HarnessEvent::BlockStart { id: aid.clone(), index: 2, kind: BlockKind::Text, tool: None });
    for piece in pieces(&answer) {
        if abort.load(Ordering::Relaxed) {
            return finish_aborted(&events, aid).await;
        }
        send(HarnessEvent::BlockDelta { id: aid.clone(), index: 2, kind: BlockKind::Text, delta: piece.to_owned() });
        tokio::time::sleep(tick).await;
    }
    send(HarnessEvent::BlockEnd { id: aid.clone(), index: 2, kind: BlockKind::Text, content: json!(answer) });
    let message = json!({
        "role":"assistant",
        "content":[{"type":"thinking","thinking":thinking}, tool_call, {"type":"text","text":answer}],
        "stopReason":"stop",
        "timestamp": now_ms(),
    });
    send(HarnessEvent::MessageEnd { id: aid, message });
    send(HarnessEvent::RunEnded);
    send(HarnessEvent::Settled);
}

async fn finish_aborted(events: &mpsc::UnboundedSender<HarnessEvent>, _aid: String) {
    let _ = events.send(HarnessEvent::Notice { level: crate::event::NoticeLevel::Warning, text: "Aborted".into(), system: false });
    let _ = events.send(HarnessEvent::RunEnded);
    let _ = events.send(HarnessEvent::Settled);
}

/// Splits text into small, token-like pieces on char boundaries.
fn pieces(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut count = 0;
    for (i, _) in text.char_indices() {
        if count == 4 {
            out.push(&text[start..i]);
            start = i;
            count = 0;
        }
        count += 1;
    }
    out.push(&text[start..]);
    out
}

fn answer_markdown(n: u64) -> String {
    format!(
        r#"## Summary for item {n}

The workspace has **two crates** so far, plus the `PLAN.md` file. Here is what each piece does:

1. `harlo-harness` spawns pi or omp and translates their JSONL events.
2. `harlo-core` reduces events into transcript rows and renders *markdown*.
3. `harlo-app` is the Qt/QML front end.

| Crate | Owns | Qt? |
|---|---|---|
| harlo-harness | processes, framing | no |
| harlo-core | state, markdown | no |
| harlo-app | bridge, QML | yes |

```rust
pub fn translate(&mut self, frame: Value) -> Vec<HarnessEvent> {{
    let ty = frame["type"].as_str().unwrap_or_default();
    match ty {{
        "agent_start" => vec![HarnessEvent::RunStarted],
        _ => vec![HarnessEvent::Other(frame)],
    }}
}}
```

> Streaming text is revealed line by line, so paragraphs don't jitter while the model types.

That's all for item {n}. Let me know if you want the [plan](https://example.com) expanded.
"#
    )
}

/// Two pretend subagents: a scout that finishes and a worker that keeps
/// running until cancelled.
async fn run_subagents(events: mpsc::UnboundedSender<HarnessEvent>, list: Arc<std::sync::Mutex<Vec<Value>>>) {
    let dir = std::env::temp_dir().join(format!("harlo-demo-subagents-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let transcript = |name: &str, task: &str, answer: &str| {
        let path = dir.join(format!("{name}.jsonl"));
        let lines = [
            json!({"type":"session","version":3,"id":name,"timestamp":"2026-10-11T12:00:00.000Z","cwd":"/tmp"}),
            json!({"type":"message","id":"m1","parentId":null,"timestamp":"2026-10-11T12:00:01.000Z","message":{"role":"user","content":[{"type":"text","text":task}]}}),
            json!({"type":"message","id":"m2","parentId":"m1","timestamp":"2026-10-11T12:00:05.000Z","message":{"role":"assistant","content":[{"type":"text","text":answer}]}}),
        ];
        let _ = std::fs::write(&path, lines.iter().map(|l| l.to_string() + "\n").collect::<String>());
        path.to_string_lossy().into_owned()
    };
    let scout = transcript("ScoutLark", "Find where the build flags are set.", "They are set in `build.rs` and `Cargo.toml` (`[profile.release]`).");
    let worker = transcript("BuildOtter", "Run the test suite and report failures.", "Running `cargo test`…");
    let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    *list.lock().unwrap() = vec![
        json!({"id":"ScoutLark","index":0,"agent":"scout","agentSource":"bundled","detached":true,"description":"Locate the build flags","status":"running","task":"Find where the build flags are set.","sessionFile":scout,"lastUpdate":now(),"parentToolCallId":"demo-task","progress":{"currentTool":"grep","currentToolArgs":{"pattern":"opt-level"},"resolvedModel":"demo-small","toolCount":3}}),
        json!({"id":"BuildOtter","index":1,"agent":"task","agentSource":"bundled","description":"Run the tests","status":"running","task":"Run the test suite and report failures.","sessionFile":worker,"lastUpdate":now(),"parentToolCallId":"demo-task","progress":{"currentTool":"bash","currentToolArgs":{"command":"cargo test"},"resolvedModel":"demo-large","toolCount":1}}),
    ];
    // The `task` tool call the two run under.
    let args = json!({ "tasks": [{ "agent": "scout", "description": "Locate the build flags" }, { "agent": "task", "description": "Run the tests" }] });
    let call = json!({ "type": "toolCall", "id": "demo-task", "name": "task", "arguments": args });
    let _ = events.send(HarnessEvent::MessageEnd { id: "demo-task-msg".into(), message: json!({ "role": "assistant", "content": [call], "stopReason": "toolUse" }) });
    let _ = events.send(HarnessEvent::ToolStart { call_id: "demo-task".into(), name: "task".into(), args });
    // omp's frames: the subagent's record as the payload.
    let ping = |events: &mpsc::UnboundedSender<HarnessEvent>, list: &[Value]| {
        for a in list {
            let _ = events.send(HarnessEvent::SubagentActivity(json!({ "type": "subagent_progress", "payload": a })));
        }
    };
    ping(&events, &list.lock().unwrap());
    tokio::time::sleep(Duration::from_millis(1500)).await;
    for a in list.lock().unwrap().iter_mut() {
        if a["id"] == "ScoutLark" && a["status"] == "running" {
            a["status"] = json!("completed");
            a["progress"]["currentTool"] = Value::Null;
            a["progress"]["toolCount"] = json!(5);
        }
    }
    ping(&events, &list.lock().unwrap());
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}
