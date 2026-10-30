//! An ACP agent's session behind harlo's harness interface (the protocol
//! and the shared connection are in `acp.rs`). harlo's commands (pi RPC
//! shaped) become ACP requests; the agent's `session/update`s become harlo
//! events (each reply `messageId` a message, its thought and text chunks
//! and tool calls its blocks); its permission requests become harlo's
//! approval cards.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};

use crate::acp::{self, Conn, Incoming, Replay};
use crate::event::{BlockKind, HarnessEvent, HarnessKind, NoticeLevel};
use crate::process::{Harness, HarnessConfig, Outgoing, Pending, RemoteLaunch, SessionTarget};

/// A reply being streamed: one of the agent's `messageId`s.
struct Step {
    id: String,
    /// Per block: its kind, its text, and for a tool call its id, name and
    /// arguments.
    blocks: Vec<(BlockKind, String, String, String, Value)>,
    shown: bool,
}

impl Step {
    fn new(id: String) -> Self {
        Self { id, blocks: Vec::new(), shown: false }
    }

    fn show(&mut self, out: &mut Vec<HarnessEvent>) {
        if !std::mem::replace(&mut self.shown, true) {
            out.push(HarnessEvent::MessageStart { id: self.id.clone(), message: json!({ "role": "assistant", "content": [] }) });
        }
    }
}

#[derive(Default)]
struct State {
    session_id: String,
    /// Prompted at least once: the agent keeps it, and harlo lists it.
    kept: bool,
    /// Prompts sent and not answered yet (the agent queues them).
    prompts: u32,
    step: Option<Step>,
    /// Tool calls shown, by id (the agent also closes ids of its own, its
    /// approvals', which harlo never saw).
    tools: HashMap<String, String>,
    messages: Vec<Value>,
    /// ACP's model id, and the session's choices.
    model: String,
    models: Vec<Value>,
    efforts: Vec<String>,
    effort: String,
    /// harlo's permission shield: always-ask, write (edits), yolo.
    approval: String,
    /// UI request id → the agent's request id and the options (label →
    /// option id).
    waiting: HashMap<String, (Value, Vec<(String, String)>)>,
    context_size: u64,
    context_used: u64,
    commands: Vec<Value>,
    title: String,
    /// harlo's own `/title`: its answer isn't shown.
    quiet: bool,
    next: u64,
}

impl State {
    fn next_id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    /// The current reply, `id` when it's a new one (ending the one before).
    fn step(&mut self, id: Option<&str>, out: &mut Vec<HarnessEvent>) -> &mut Step {
        let differs = match (&self.step, id) {
            (Some(s), Some(id)) => s.id != id,
            (None, _) => true,
            (Some(_), None) => false,
        };
        if differs {
            if let Some(done) = self.step.take() {
                end_step(self, done, None, out);
            }
            let id = id.map_or_else(|| self.next_id("acp-m"), str::to_owned);
            self.step = Some(Step::new(id));
        }
        self.step.as_mut().expect("a step")
    }

    fn model_value(&self) -> Value {
        if self.model.is_empty() {
            return Value::Null;
        }
        let (provider, id) = acp::split_model(&self.model);
        let name = self.models.iter().find(|m| m["modelId"] == self.model.as_str()).and_then(|m| m["name"].as_str()).map_or_else(|| id.clone(), acp::model_name);
        json!({ "provider": provider, "id": id, "name": name })
    }
}

struct Session {
    kind: HarnessKind,
    state: Mutex<State>,
    events: mpsc::UnboundedSender<HarnessEvent>,
    conn: Mutex<Option<Arc<Conn>>>,
    program: PathBuf,
    /// The session's folder, as its machine sees it.
    cwd: String,
    /// On another machine: how to reach it.
    remote: Option<RemoteLaunch>,
    inbox: mpsc::UnboundedSender<Incoming>,
    /// Set once the session is open (or failed to open).
    opened: watch::Sender<Option<Result<(), String>>>,
}

impl Session {
    /// The session in harlo's index.
    fn sref(&self, id: &str) -> acp::SessionRef {
        acp::SessionRef { kind: self.kind, id: id.to_owned(), host: self.remote.as_ref().map(|r| r.host.clone()).unwrap_or_default() }
    }

    fn emit(&self, ev: HarnessEvent) {
        let _ = self.events.send(ev);
    }

    /// harlo's tools (its MCP bridge, while orchestration is on), as ACP's
    /// `mcpServers`.
    fn tools(&self) -> Value {
        match crate::mcp_bridge_for(self.remote.as_ref()) {
            Some(cmd) if !cmd.is_empty() => json!([{ "name": "harlo", "command": cmd[0], "args": cmd[1..], "env": [] }]),
            _ => json!([]),
        }
    }

    fn label(&self) -> &'static str {
        self.kind.label()
    }

    /// The connection, once the session is open.
    async fn ready(&self) -> Result<Arc<Conn>> {
        let mut rx = self.opened.subscribe();
        let opened = rx.wait_for(Option::is_some).await.map(|v| v.clone()).ok().flatten();
        match opened {
            Some(Ok(())) => self.conn.lock().unwrap().clone().ok_or_else(|| anyhow::anyhow!("{} stopped", self.label())),
            Some(Err(e)) => bail!("{e}"),
            None => bail!("{} stopped", self.label()),
        }
    }

    /// Connects, then loads the session (`resume`) or starts one.
    async fn open(self: &Arc<Self>, resume: Option<String>) -> Result<()> {
        let (kind, program, remote) = (self.kind, self.program.clone(), self.remote.clone());
        let conn = tokio::task::spawn_blocking(move || acp::conn(kind, &program, remote.as_ref())).await??;
        *self.conn.lock().unwrap() = Some(Arc::clone(&conn));
        let answer = match resume {
            Some(id) => {
                // Its history comes as updates before the answer.
                let (tx, mut rx) = mpsc::unbounded_channel();
                conn.route(&id, tx);
                // (A load with other tools than before can be refused: then
                // with none, which keeps the ones it has.)
                let mut answer = conn.request("session/load", json!({ "sessionId": id, "cwd": self.cwd, "mcpServers": self.tools() })).await;
                if answer.is_err() && self.tools().as_array().is_some_and(|t| !t.is_empty()) {
                    answer = conn.request("session/load", json!({ "sessionId": id, "cwd": self.cwd, "mcpServers": [] })).await;
                }
                let mut replay = Replay::default();
                let mut later = Vec::new();
                while let Ok(i) = rx.try_recv() {
                    match i {
                        Incoming::Update(u) if matches!(u["sessionUpdate"].as_str(), Some("user_message_chunk" | "agent_message_chunk" | "agent_thought_chunk" | "tool_call" | "tool_call_update" | "plan")) => replay.update(&u),
                        other => later.push(other),
                    }
                }
                conn.route(&id, self.inbox.clone());
                for i in later {
                    let _ = self.inbox.send(i);
                }
                let answer = answer?;
                let mut st = self.state.lock().unwrap();
                st.session_id = id;
                st.kept = true;
                for m in &replay.messages {
                    for b in m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall") {
                        st.tools.insert(b["id"].as_str().unwrap_or_default().to_owned(), b["name"].as_str().unwrap_or_default().to_owned());
                    }
                }
                st.messages = replay.messages;
                let r = self.sref(&st.session_id);
                acp::save_history(&r, &st.messages, answer["models"]["currentModelId"].as_str().unwrap_or_default());
                st.title = acp::entry(&r).map(|e| e.title).unwrap_or_default();
                answer
            }
            None => {
                let answer = conn.request("session/new", json!({ "cwd": self.cwd, "mcpServers": self.tools() })).await?;
                let id = answer["sessionId"].as_str().unwrap_or_default().to_owned();
                if id.is_empty() {
                    bail!("{} didn't start a session", self.label());
                }
                conn.route(&id, self.inbox.clone());
                self.state.lock().unwrap().session_id = id;
                answer
            }
        };
        let (wanted, id) = {
            let mut st = self.state.lock().unwrap();
            st.models = answer["models"]["availableModels"].as_array().cloned().unwrap_or_default();
            let current = answer["models"]["currentModelId"].as_str().unwrap_or_default().to_owned();
            let wanted = (!st.model.is_empty() && st.model != current).then(|| st.model.clone());
            st.model = current;
            options(&mut st, &answer["configOptions"]);
            (wanted, st.session_id.clone())
        };
        // The model harlo picked for it.
        if let Some(model) = wanted
            && conn.request("session/set_model", json!({ "sessionId": id, "modelId": model })).await.is_ok()
        {
            self.state.lock().unwrap().model = model;
        }
        Ok(())
    }
}

/// The session's config options: its reasoning effort.
fn options(st: &mut State, options: &Value) {
    for o in options.as_array().into_iter().flatten() {
        if o["id"] == "reasoning_effort" || o["category"] == "thought_level" {
            st.efforts = o["options"].as_array().into_iter().flatten().filter_map(|v| v["value"].as_str()).filter(|v| *v != "default").map(str::to_owned).collect();
            st.effort = o["currentValue"].as_str().filter(|v| *v != "default").unwrap_or_default().to_owned();
        }
    }
}

pub(crate) fn spawn(cfg: HarnessConfig, program: PathBuf) -> Result<(Harness, mpsc::UnboundedReceiver<HarnessEvent>)> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Outgoing>();
    let (events, events_rx) = mpsc::unbounded_channel();
    let pending: Pending = Arc::default();
    let harness = Harness::from_parts(cfg.kind, tx, Arc::clone(&pending));
    let (inbox, mut inbox_rx) = mpsc::unbounded_channel::<Incoming>();
    let mut state = State::default();
    let mut it = cfg.extra_args.iter();
    while let Some(a) = it.next() {
        if a == "--model"
            && let Some(m) = it.next()
        {
            state.model = m.clone();
        }
    }
    let resume = match &cfg.session {
        SessionTarget::Resume(path) => acp::session_of(path).map(|r| r.id),
        _ => None,
    };
    let (opened, _) = watch::channel(None);
    let cwd = if cfg.remote.is_some() { crate::process::on_host(&cfg.cwd) } else { cfg.cwd.to_string_lossy().into_owned() };
    let session = Arc::new(Session { kind: cfg.kind, state: Mutex::new(state), events, conn: Mutex::new(None), program, cwd, remote: cfg.remote.clone(), inbox, opened });

    let s = Arc::clone(&session);
    tokio::spawn(async move {
        match s.open(resume).await {
            Ok(()) => {
                s.opened.send_replace(Some(Ok(())));
                s.emit(HarnessEvent::Ready);
            }
            Err(e) => {
                let text = format!("{} didn't start: {e:#}", s.label());
                s.opened.send_replace(Some(Err(text.clone())));
                s.emit(HarnessEvent::Notice { level: NoticeLevel::Error, text, system: false });
                s.emit(HarnessEvent::Exited { code: None, stderr_tail: String::new() });
            }
        }
    });
    let s = Arc::clone(&session);
    tokio::spawn(async move {
        while let Some(i) = inbox_rx.recv().await {
            match i {
                Incoming::Update(u) => update(&s, &u),
                Incoming::Request { id, method, params } => request(&s, id, &method, &params),
                Incoming::Closed => {
                    s.emit(HarnessEvent::Notice { level: NoticeLevel::Error, text: format!("{} stopped.", s.label()), system: false });
                    s.emit(HarnessEvent::Exited { code: None, stderr_tail: String::new() });
                    break;
                }
            }
        }
    });
    let s = Arc::clone(&session);
    tokio::spawn(async move {
        while let Some(out) = rx.recv().await {
            match out {
                Outgoing::Frame(cmd) => command(&s, &pending, cmd).await,
                Outgoing::Kill => break,
            }
        }
        // Gone: its updates no longer come here (the agent keeps running
        // for harlo's other sessions).
        let id = s.state.lock().unwrap().session_id.clone();
        if let Some(c) = s.conn.lock().unwrap().as_ref() {
            c.unroute(&id);
        }
    });
    Ok((harness, events_rx))
}

async fn command(s: &Arc<Session>, pending: &Pending, cmd: Value) {
    let id = cmd["id"].as_str().map(str::to_owned);
    let ty = cmd["type"].as_str().unwrap_or_default().to_owned();
    let reply = |ok: bool, data: Value, error: &str| {
        if let Some(w) = id.as_deref().and_then(|id| pending.lock().unwrap().remove(id)) {
            let _ = w.send(json!({ "type": "response", "id": id, "command": ty, "success": ok, "data": data, "error": error }));
        }
    };
    let conn = match s.ready().await {
        Ok(c) => c,
        Err(e) => return reply(false, Value::Null, &format!("{e:#}")),
    };
    let sid = s.state.lock().unwrap().session_id.clone();
    match ty.as_str() {
        "prompt" => {
            let mut text = cmd["message"].as_str().unwrap_or_default().to_owned();
            let message = json!({ "role": "user", "content": [{ "type": "text", "text": text }] });
            {
                let mut st = s.state.lock().unwrap();
                st.messages.push(message.clone());
                let id = st.next_id("u");
                s.emit(HarnessEvent::MessageEnd { id, message });
                // Kept from its first prompt on: harlo lists it.
                if !std::mem::replace(&mut st.kept, true) {
                    let first = text.lines().next().unwrap_or_default().to_owned();
                    acp::remember(&s.sref(&sid), &s.cwd, |e| e.first = first);
                }
            }
            // Steering goes into the running turn.
            if cmd["streamingBehavior"] == "steer" && s.state.lock().unwrap().prompts > 0 {
                text = format!("/steer {text}");
            }
            let mut blocks = vec![json!({ "type": "text", "text": text })];
            blocks.extend(cmd["images"].as_array().into_iter().flatten().map(|img| json!({ "type": "image", "data": img["data"], "mimeType": img["mimeType"] })));
            send_prompt(s, &conn, blocks, false);
            reply(true, json!({ "disposition": "started" }), "");
        }
        "abort" => {
            conn.notify("session/cancel", json!({ "sessionId": sid }));
            // Its open questions: no (the agent stops waiting for them).
            let waiting: Vec<(String, (Value, Vec<(String, String)>))> = s.state.lock().unwrap().waiting.drain().collect();
            for (ui, (rid, options)) in waiting {
                conn.respond(rid, decline(&options));
                s.emit(HarnessEvent::UiCancel { target_id: ui });
            }
            reply(true, Value::Null, "");
        }
        "get_state" => {
            let st = s.state.lock().unwrap();
            let host = s.remote.as_ref().map(|r| r.host.as_str()).unwrap_or_default();
            let file = st.kept.then(|| acp::session_path(s.kind, host, &st.session_id).to_string_lossy().into_owned());
            reply(true, json!({ "model": st.model_value(), "thinkingLevel": st.effort, "sessionId": st.session_id, "sessionFile": file, "isStreaming": st.prompts > 0 }), "");
        }
        "get_session_stats" => {
            let st = s.state.lock().unwrap();
            reply(true, json!({ "contextUsage": { "tokens": st.context_used, "contextWindow": st.context_size } }), "");
        }
        "get_available_models" => {
            let st = s.state.lock().unwrap();
            let reasoning = !st.efforts.is_empty();
            let list: Vec<Value> = st
                .models
                .iter()
                .filter_map(|m| {
                    let (provider, id) = acp::split_model(m["modelId"].as_str()?);
                    Some(json!({ "provider": provider, "id": id, "name": acp::model_name(m["name"].as_str().unwrap_or_default()), "reasoning": reasoning }))
                })
                .collect();
            reply(true, json!({ "models": list }), "");
        }
        "set_model" => {
            let (provider, id) = (cmd["provider"].as_str().unwrap_or_default(), cmd["modelId"].as_str().unwrap_or_default());
            let model = if provider.is_empty() { id.to_owned() } else { format!("{provider}:{id}") };
            match conn.request("session/set_model", json!({ "sessionId": sid, "modelId": model })).await {
                Ok(_) => {
                    let m = {
                        let mut st = s.state.lock().unwrap();
                        st.model = model;
                        st.model_value()
                    };
                    s.emit(HarnessEvent::ModelChanged(m.clone()));
                    reply(true, m, "");
                }
                Err(e) => reply(false, Value::Null, &format!("{e:#}")),
            }
        }
        "get_available_thinking_levels" => {
            let levels = s.state.lock().unwrap().efforts.clone();
            reply(true, json!({ "levels": levels }), "");
        }
        "set_thinking_level" => {
            let level = cmd["level"].as_str().unwrap_or_default().to_owned();
            match conn.request("session/set_config_option", json!({ "sessionId": sid, "configId": "reasoning_effort", "value": level })).await {
                Ok(r) => {
                    options(&mut s.state.lock().unwrap(), &r["configOptions"]);
                    s.emit(HarnessEvent::ThinkingLevel(level));
                    reply(true, Value::Null, "");
                }
                Err(e) => reply(false, Value::Null, &format!("{e:#}")),
            }
        }
        // The shield: edits without asking (write), or nothing asked (yolo).
        "harlo_set_modes" => {
            let approval = cmd["approval"].as_str().unwrap_or_default().to_owned();
            let mode = match approval.as_str() {
                "write" => "accept_edits",
                "yolo" | "auto" => "dont_ask",
                _ => "default",
            };
            s.state.lock().unwrap().approval = approval;
            // (A busy session takes it when it's idle.)
            let _ = conn.request("session/set_mode", json!({ "sessionId": sid, "modeId": mode })).await;
            reply(true, Value::Null, "");
        }
        "get_commands" | "get_available_commands" => {
            let list = s.state.lock().unwrap().commands.clone();
            reply(true, json!({ "commands": list }), "");
        }
        "get_messages" => {
            let messages = s.state.lock().unwrap().messages.clone();
            reply(true, json!({ "messages": messages }), "");
        }
        "compact" => {
            send_prompt(s, &conn, vec![json!({ "type": "text", "text": "/compress" })], false);
            reply(true, Value::Null, "");
        }
        "extension_ui_response" => answer(s, &conn, &cmd),
        "get_subagents" => reply(true, json!({ "subagents": [] }), ""),
        // Edit & resend: the agent forgets that prompt and the ones after it
        // (`/undo N`), and harlo's copy ends before it.
        "fork" => {
            let n: usize = cmd["entryId"].as_str().and_then(|e| e.strip_prefix('u')).and_then(|n| n.parse().ok()).unwrap_or(usize::MAX);
            let (running, users) = {
                let st = s.state.lock().unwrap();
                let users: Vec<usize> = st.messages.iter().enumerate().filter(|(_, m)| m["role"] == "user").map(|(i, _)| i).collect();
                (st.prompts > 0, users)
            };
            if running {
                return reply(false, Value::Null, &format!("Stop {} first.", s.label()));
            }
            let Some(&at) = users.get(n) else { return reply(false, Value::Null, "That message can't be edited.") };
            if let Err(e) = quiet(s, &conn, &format!("/undo {}", users.len() - n)).await {
                return reply(false, Value::Null, &format!("{e:#}"));
            }
            let text = {
                let mut st = s.state.lock().unwrap();
                let text = st.messages[at]["content"][0]["text"].as_str().unwrap_or_default().to_owned();
                st.messages.truncate(at);
                acp::save_history(&s.sref(&sid), &st.messages, &st.model);
                text
            };
            reply(true, json!({ "text": text, "cancelled": false }), "");
        }
        // harlo's list shows the name; the agent hears it when idle.
        "set_session_name" => {
            let name = cmd["name"].as_str().unwrap_or_default().trim().to_owned();
            let (kept, idle) = {
                let mut st = s.state.lock().unwrap();
                st.title = name.clone();
                (st.kept, st.prompts == 0)
            };
            if kept {
                let n = name.clone();
                acp::remember(&s.sref(&sid), &s.cwd, |e| e.title = n);
                if idle && !name.is_empty() {
                    send_prompt(s, &conn, vec![json!({ "type": "text", "text": format!("/title {name}") })], true);
                }
            }
            s.emit(HarnessEvent::Renamed((!name.is_empty()).then_some(name)));
            reply(true, Value::Null, "");
        }
        _ => reply(false, Value::Null, &format!("{} sessions don't support {ty}", s.label())),
    }
}

/// harlo's own command (`/undo 2`), waited for; its reply isn't shown.
async fn quiet(s: &Arc<Session>, conn: &Arc<Conn>, text: &str) -> Result<()> {
    let sid = {
        let mut st = s.state.lock().unwrap();
        st.quiet = true;
        st.session_id.clone()
    };
    let result = conn.request("session/prompt", json!({ "sessionId": sid, "prompt": [{ "type": "text", "text": text }] })).await;
    let mut st = s.state.lock().unwrap();
    st.quiet = false;
    st.step = None;
    result.map(|_| ())
}

/// Sends a prompt; its answer ends the run (once no other is waiting).
/// `quiet`: harlo's own command, whose reply isn't shown.
fn send_prompt(s: &Arc<Session>, conn: &Arc<Conn>, blocks: Vec<Value>, quiet: bool) {
    let sid = {
        let mut st = s.state.lock().unwrap();
        st.prompts += 1;
        st.quiet = quiet;
        if st.prompts == 1 && !quiet {
            s.emit(HarnessEvent::RunStarted);
        }
        st.session_id.clone()
    };
    let (s, conn) = (Arc::clone(s), Arc::clone(conn));
    tokio::spawn(async move {
        let result = conn.request("session/prompt", json!({ "sessionId": sid, "prompt": blocks })).await;
        let mut out = Vec::new();
        {
            let mut st = s.state.lock().unwrap();
            st.prompts = st.prompts.saturating_sub(1);
            if let Err(e) = &result {
                out.push(HarnessEvent::Notice { level: NoticeLevel::Error, text: format!("{}: {e:#}", s.label()), system: false });
            }
            if st.prompts == 0 {
                if let Some(step) = st.step.take() {
                    end_step(&mut st, step, None, &mut out);
                }
                if !std::mem::take(&mut st.quiet) {
                    out.push(HarnessEvent::RunEnded);
                    out.push(HarnessEvent::Settled);
                }
            }
        }
        let kept = {
            let st = s.state.lock().unwrap();
            if st.kept && st.prompts == 0 {
                acp::save_history(&s.sref(&sid), &st.messages, &st.model);
            }
            st.kept
        };
        if kept {
            acp::remember(&s.sref(&sid), &s.cwd, |_| {});
        }
        for e in out {
            s.emit(e);
        }
    });
}

/// One of the agent's `session/update`s.
fn update(s: &Arc<Session>, u: &Value) {
    let mut out = Vec::new();
    let mut retitled = None;
    {
        let mut st = s.state.lock().unwrap();
        let kind = u["sessionUpdate"].as_str().unwrap_or_default();
        match kind {
            "agent_message_chunk" | "agent_thought_chunk" if !st.quiet => {
                let text = u["content"]["text"].as_str().unwrap_or_default().to_owned();
                let block = if kind == "agent_thought_chunk" { BlockKind::Thinking } else { BlockKind::Text };
                let step = st.step(u["messageId"].as_str(), &mut out);
                step.show(&mut out);
                let index = match step.blocks.last() {
                    Some((k, ..)) if *k == block => step.blocks.len() - 1,
                    _ => {
                        step.blocks.push((block, String::new(), String::new(), String::new(), Value::Null));
                        out.push(HarnessEvent::BlockStart { id: step.id.clone(), index: step.blocks.len() - 1, kind: block, tool: None });
                        step.blocks.len() - 1
                    }
                };
                step.blocks[index].1.push_str(&text);
                out.push(HarnessEvent::BlockDelta { id: step.id.clone(), index, kind: block, delta: text });
            }
            "tool_call" if !st.quiet => {
                let call = u["toolCallId"].as_str().unwrap_or_default().to_owned();
                let (name, args) = acp::tool_call(u);
                st.tools.insert(call.clone(), name.clone());
                let step = st.step(None, &mut out);
                step.show(&mut out);
                let index = step.blocks.len();
                step.blocks.push((BlockKind::ToolCall, String::new(), call.clone(), name.clone(), args.clone()));
                out.push(HarnessEvent::BlockStart { id: step.id.clone(), index, kind: BlockKind::ToolCall, tool: Some((call.clone(), name.clone())) });
                out.push(HarnessEvent::BlockEnd { id: step.id.clone(), index, kind: BlockKind::ToolCall, content: json!({ "type": "toolCall", "id": call, "name": name, "arguments": args }) });
                out.push(HarnessEvent::ToolStart { call_id: call, name, args });
            }
            "tool_call_update" => {
                let call = u["toolCallId"].as_str().unwrap_or_default().to_owned();
                let done = matches!(u["status"].as_str(), Some("completed" | "failed"));
                if done && st.tools.contains_key(&call) {
                    let is_error = u["status"] == "failed";
                    let text = acp::content_text(&u["content"]);
                    st.messages.push(json!({ "role": "toolResult", "toolCallId": call, "isError": is_error, "content": [{ "type": "text", "text": text }] }));
                    out.push(HarnessEvent::ToolEnd { call_id: call, result: json!({ "content": [{ "type": "text", "text": text }] }), is_error });
                }
            }
            "plan" => {
                let lines: Vec<String> = u["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| {
                        let mark = match e["status"].as_str() {
                            Some("completed") => "☑",
                            Some("in_progress") => "◐",
                            _ => "☐",
                        };
                        format!("{mark} {}", e["content"].as_str().unwrap_or_default())
                    })
                    .collect();
                out.push(HarnessEvent::Widget { key: "plan".into(), lines: (!lines.is_empty()).then_some(lines) });
            }
            "usage_update" => {
                st.context_size = u["size"].as_u64().unwrap_or(st.context_size);
                st.context_used = u["used"].as_u64().unwrap_or(st.context_used);
            }
            "session_info_update" => {
                if let Some(title) = u["title"].as_str().filter(|t| !t.is_empty() && *t != st.title) {
                    st.title = title.to_owned();
                    retitled = Some((st.kept, st.session_id.clone(), title.to_owned()));
                    out.push(HarnessEvent::Renamed(Some(title.to_owned())));
                }
            }
            "available_commands_update" => {
                st.commands = u["availableCommands"].as_array().into_iter().flatten().map(|c| json!({ "name": c["name"], "description": c["description"] })).collect();
                out.push(HarnessEvent::CommandsChanged(Value::Array(st.commands.clone())));
            }
            "config_option_update" => options(&mut st, &u["configOptions"]),
            _ => {}
        }
    }
    if let Some((true, id, title)) = retitled {
        acp::remember(&s.sref(&id), &s.cwd, |e| e.title = title);
    }
    for e in out {
        s.emit(e);
    }
}

/// The agent asks: a permission (harlo's approval card, unless the shield
/// answers it), or anything else it asks this way.
fn request(s: &Arc<Session>, id: Value, method: &str, params: &Value) {
    let Some(conn) = s.conn.lock().unwrap().clone() else { return };
    if method != "session/request_permission" {
        return conn.respond_error(id, &format!("harlo doesn't handle {method}"));
    }
    let call = &params["toolCall"];
    let opts: Vec<(String, String, String)> = params["options"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| (o["optionId"].as_str().unwrap_or_default().to_owned(), o["name"].as_str().unwrap_or_default().to_owned(), o["kind"].as_str().unwrap_or_default().to_owned()))
        .collect();
    let option = |kind: &str| opts.iter().find(|o| o.2 == kind);
    let mut st = s.state.lock().unwrap();
    // The shield answers what it covers.
    let allow = match st.approval.as_str() {
        "yolo" | "auto" => true,
        "write" => call["kind"] == "edit",
        _ => false,
    };
    if allow && let Some(o) = option("allow_once") {
        return conn.respond(id, json!({ "outcome": { "outcome": "selected", "optionId": o.0 } }));
    }
    let ui = st.next_id("acp_ui_");
    let title = call["title"].as_str().unwrap_or_default();
    let details = acp::content_text(&call["content"]);
    let message = details.lines().take(40).collect::<Vec<_>>().join("\n");
    let (once, deny) = (option("allow_once"), option("reject_once"));
    let request = if let (Some(once), Some(deny)) = (once, deny) {
        // An approval: Approve / Deny / Always allow, as for the other agents.
        let tool = title.strip_prefix("Approve ").map_or_else(|| title.split(": ").next().unwrap_or("tool"), |t| t.split(':').next().unwrap_or("edit"));
        let always = opts.iter().find(|o| o.2 == "allow_always" && o.0 == "allow_always").or_else(|| option("allow_always"));
        let mut labels = vec![("Approve".to_owned(), once.0.clone()), ("Deny".to_owned(), deny.0.clone())];
        if let Some(a) = always {
            labels.push(("Always allow".to_owned(), a.0.clone()));
        }
        let req = json!({
            "type": "extension_ui_request", "id": ui, "method": "select",
            "title": format!("Allow tool: {tool}"), "message": if message.is_empty() { title.to_owned() } else { message },
            "options": labels.iter().map(|l| l.0.clone()).collect::<Vec<_>>(),
            "alwaysHint": always.map(|a| a.1.clone()).unwrap_or_default(),
        });
        st.waiting.insert(ui, (id, labels));
        req
    } else {
        // A question with its own choices.
        let labels: Vec<(String, String)> = opts.iter().map(|o| (o.1.clone(), o.0.clone())).collect();
        let req = json!({
            "type": "extension_ui_request", "id": ui, "method": "select",
            "title": if title.is_empty() { format!("{} asks", s.label()) } else { title.to_owned() }, "message": message,
            "options": labels.iter().map(|l| l.0.clone()).collect::<Vec<_>>(),
        });
        st.waiting.insert(ui, (id, labels));
        req
    };
    drop(st);
    s.emit(HarnessEvent::UiRequest(request));
}

/// "No" to a permission: its reject option (the agent takes a cancelled
/// answer as "ask someone else").
fn decline(options: &[(String, String)]) -> Value {
    match options.iter().find(|(l, id)| l == "Deny" || id.starts_with("deny") || id == "skip") {
        Some((_, id)) => json!({ "outcome": { "outcome": "selected", "optionId": id } }),
        None => json!({ "outcome": { "outcome": "cancelled" } }),
    }
}

/// harlo's answer to a permission or a question.
fn answer(s: &Arc<Session>, conn: &Arc<Conn>, cmd: &Value) {
    let ui = cmd["id"].as_str().unwrap_or_default();
    let Some((rid, options)) = s.state.lock().unwrap().waiting.remove(ui) else { return };
    let picked = cmd["value"].as_str().map(str::to_owned).or_else(|| (cmd["confirmed"] == true).then(|| "Approve".to_owned()));
    let result = match picked.and_then(|p| options.iter().find(|(l, _)| *l == p).cloned()) {
        Some((_, id)) if cmd["cancelled"] != true => json!({ "outcome": { "outcome": "selected", "optionId": id } }),
        _ => decline(&options),
    };
    conn.respond(rid, result);
}

/// A finished reply as its message.
fn end_step(st: &mut State, mut step: Step, error: Option<String>, out: &mut Vec<HarnessEvent>) {
    if step.blocks.is_empty() && error.is_none() {
        return;
    }
    step.show(out);
    let content: Vec<Value> = step
        .blocks
        .iter()
        .map(|(kind, text, call, name, args)| match kind {
            BlockKind::Text => json!({ "type": "text", "text": text }),
            BlockKind::Thinking => json!({ "type": "thinking", "thinking": text }),
            BlockKind::ToolCall => json!({ "type": "toolCall", "id": call, "name": name, "arguments": args }),
        })
        .collect();
    // Text blocks end as they're complete.
    for (index, (kind, text, ..)) in step.blocks.iter().enumerate() {
        if *kind != BlockKind::ToolCall {
            out.push(HarnessEvent::BlockEnd { id: step.id.clone(), index, kind: *kind, content: json!(text) });
        }
    }
    let mut message = json!({ "role": "assistant", "content": content, "model": st.model });
    if let Some(e) = error {
        message["stopReason"] = json!("error");
        message["errorMessage"] = json!(e);
    }
    st.messages.push(message.clone());
    out.push(HarnessEvent::MessageEnd { id: step.id, message });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (Arc<Session>, mpsc::UnboundedReceiver<HarnessEvent>) {
        let (events, rx) = mpsc::unbounded_channel();
        let (inbox, _) = mpsc::unbounded_channel();
        let (opened, _) = watch::channel(None);
        (Arc::new(Session { kind: HarnessKind::Hermes, state: Mutex::new(State::default()), events, conn: Mutex::new(None), program: "hermes".into(), cwd: "/work".into(), remote: None, inbox, opened }), rx)
    }

    /// A real Hermes 0.22.5 turn (a file written after an edit approval,
    /// `ls`, then the answer) goes through as harlo events.
    #[test]
    fn a_real_turn() {
        let (s, mut rx) = session();
        let log = include_str!("../tests/fixtures/hermes-acp.jsonl");
        let live = &log[..log.find("=== second process").unwrap()];
        for line in live.lines() {
            let Some(body) = line.strip_prefix("<- ") else { continue };
            let msg: Value = serde_json::from_str(body).unwrap();
            if msg["method"] == "session/update" {
                update(&s, &msg["params"]["update"]);
            }
        }
        {
            let mut st = s.state.lock().unwrap();
            let mut out = Vec::new();
            if let Some(step) = st.step.take() {
                end_step(&mut st, step, None, &mut out);
            }
            for e in out {
                s.emit(e);
            }
        }
        let events: Vec<HarnessEvent> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let tools: Vec<&str> = events.iter().filter_map(|e| if let HarnessEvent::ToolStart { name, .. } = e { Some(name.as_str()) } else { None }).collect();
        assert_eq!(tools, ["execute_code", "execute_code"]);
        // Two results: the approval's own id ("edit-approval-1") isn't one.
        let ends: Vec<&Value> = events.iter().filter_map(|e| if let HarnessEvent::ToolEnd { result, .. } = e { Some(result) } else { None }).collect();
        assert_eq!(ends.len(), 2);
        assert_eq!(ends[1]["content"][0]["text"], "Exit code: 0\n\nOutput:\nhello.txt\n");
        // Each reply id a message: thinking, a call; thinking, a call; thinking and the answer.
        let messages: Vec<&Value> = events.iter().filter_map(|e| if let HarnessEvent::MessageEnd { message, .. } = e { Some(message) } else { None }).collect();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["content"][1]["type"], "toolCall");
        assert!(messages[2]["content"][1]["text"].as_str().unwrap().contains("Created `hello.txt`"));
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::Renamed(Some(t)) if t == "Create hello.txt and run ls")));
        assert_eq!(s.state.lock().unwrap().context_used, 15135);
    }

    #[test]
    fn declining() {
        let opts = vec![("Approve".to_owned(), "allow_once".to_owned()), ("Deny".to_owned(), "deny".to_owned())];
        assert_eq!(decline(&opts)["outcome"]["optionId"], "deny");
        assert_eq!(decline(&[])["outcome"]["outcome"], "cancelled");
    }
}

#[cfg(test)]
mod this_computer {
    use super::*;
    use crate::process::SendMode;

    /// A turn with the real `hermes` (its model is asked, nothing is
    /// approved but what it asks for in a scratch folder):
    /// `XDG_DATA_HOME=<scratch> HARLO_ACP_DIR=<scratch> cargo test -p harlo-harness acp_run -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hermes_turn() {
        let dir = PathBuf::from(std::env::var("HARLO_ACP_DIR").expect("HARLO_ACP_DIR: a scratch folder"));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let cfg = HarnessConfig { kind: HarnessKind::Hermes, program: None, cwd: dir.clone(), session: SessionTarget::New, extra_args: vec![], env: vec![], remote: None };
            let (h, mut rx) = Harness::spawn(cfg).unwrap();
            let mut text = String::new();
            let mut tools = Vec::new();
            let mut sent = false;
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(240);
            loop {
                let ev = tokio::time::timeout_at(deadline, rx.recv()).await.expect("in time").expect("events");
                match ev {
                    HarnessEvent::Ready if !sent => {
                        let models = h.request(json!({ "type": "get_available_models" })).await.unwrap();
                        println!("models: {}", models["models"].as_array().map_or(0, Vec::len));
                        h.prompt("Write the word ok into a file ok.txt, then reply with just: done", vec![], SendMode::Normal);
                        sent = true;
                    }
                    HarnessEvent::UiRequest(r) => {
                        println!("asked: {} / {} {:?}", r["title"], r["message"], r["options"]);
                        h.answer_ui(r["id"].as_str().unwrap(), json!({ "value": "Approve" }));
                    }
                    HarnessEvent::ToolStart { name, .. } => tools.push(name),
                    HarnessEvent::BlockDelta { kind: BlockKind::Text, delta, .. } => text.push_str(&delta),
                    HarnessEvent::Settled => break,
                    HarnessEvent::Notice { text, .. } => println!("notice: {text}"),
                    HarnessEvent::Exited { .. } => panic!("exited"),
                    _ => {}
                }
            }
            // A second prompt, then edit & resend it: undone, its text back.
            h.prompt("Now reply with just: second", vec![], SendMode::Normal);
            loop {
                match tokio::time::timeout_at(deadline, rx.recv()).await.expect("in time").expect("events") {
                    HarnessEvent::Settled => break,
                    HarnessEvent::UiRequest(r) => h.answer_ui(r["id"].as_str().unwrap(), json!({ "value": "Approve" })),
                    _ => {}
                }
            }
            let before = h.request(json!({ "type": "get_messages" })).await.unwrap()["messages"].as_array().unwrap().len();
            let fork = h.request(json!({ "type": "fork", "entryId": "u1" })).await.unwrap();
            let after = h.request(json!({ "type": "get_messages" })).await.unwrap()["messages"].as_array().unwrap().len();
            println!("edit & resend: {fork} ({before} → {after} messages)");
            assert_eq!(fork["text"], "Now reply with just: second");
            assert!(after < before);
            let state = h.request(json!({ "type": "get_state" })).await.unwrap();
            println!("tools: {tools:?}\nreply: {text}\nstate: {state}");
            assert!(text.to_lowercase().contains("done"));
            // (Where the file lands is the model's choice.)
            assert!(tools.iter().any(|t| t == "write"));
            let file = PathBuf::from(state["sessionFile"].as_str().expect("kept"));
            assert!(crate::acp::list_sessions(HarnessKind::Hermes).iter().any(|(p, ..)| *p == file));
            // Loaded again: its history.
            let saved = tokio::task::spawn_blocking(move || crate::acp::saved(&file, &crate::resolve_program(HarnessKind::Hermes).unwrap(), None)).await.unwrap();
            println!("history: {} messages", saved.messages.len());
            assert!(saved.messages.len() >= 2);
            h.kill();
        });
        crate::acp::shutdown();
    }
}
