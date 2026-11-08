//! `--mode rpc`: pi's RPC protocol, so harlo drives harlo-agent the way it
//! drives pi. Commands come in on stdin, one JSON object a line; responses
//! (`{"type":"response", id, command, success, data|error}`) and events go
//! out on stdout the same way.
//!
//! One run at a time: a prompt while one works is refused (steer and
//! follow_up queue instead). Calls that change something ask first, as
//! harlo's approval modes say: `always-ask` before write, edit, bash and
//! fetch, `write` before bash and fetch, `yolo`/`auto` never; in plan mode files can't be
//! changed and commands ask. Reading never asks.
//!
//! Started by harlo with `--harlo-tools`: harlo's own tools (other
//! sessions, reports to a parent, the project's plan) go to it on the same
//! pipe: `{"type":"harlo_call", id, tool, args}` out, and its answer
//! `{"type":"harlo_result", call: <that id>, ok, result|error}` in (a
//! command like any other, answered). harlo knows the caller by the pipe.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use futures_util::future::BoxFuture;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::agent::{Compacted, Agent, Gate, Harlo, Inbox, Shown};
use crate::{providers, session};

/// The thinking levels offered when the model doesn't list its own (the
/// reasoning effort sent).
const LEVELS: [&str; 3] = ["low", "medium", "high"];
const DEFAULT_LEVEL: &str = "medium";
const PLAN_REFUSAL: &str = "Plan mode is on: don't change files yet. Look around, then present your plan; the user turns plan mode off when it's time to make the changes.";
const PLAN_REFUSAL_MCP: &str = "Plan mode is on: this MCP tool may change things (its server doesn't mark it read-only), so it can't run yet. Use what only looks, then present your plan.";
/// How long harlo has to answer one of its tools.
const HARLO_WAIT: std::time::Duration = std::time::Duration::from_secs(90);
const DENIED: &str = "The user denied this call. Don't try it another way: ask what they'd like instead, or carry on without it.";
const DENIED_OUTSIDE: &str = "The user said no to running this outside the sandbox. Do it inside the sandbox another way, or ask what they would like instead.";

/// Where the session is kept.
pub enum Keep {
    New,
    File(PathBuf),
    Nowhere,
}

struct Shared {
    out: mpsc::UnboundedSender<Value>,
    /// Approvals shown, waiting for an answer.
    waiting: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    modes: Mutex<Modes>,
    /// Tools the user always allows in this session.
    always: Mutex<HashSet<String>>,
    next_ui: AtomicU64,
    /// Calls of harlo's tools, waiting for its answer.
    harlo_waiting: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    next_harlo: AtomicU64,
    /// What get_state and get_messages say (also while a run has the agent).
    view: Mutex<View>,
    inbox: Arc<Mutex<Inbox>>,
    /// The run working, if any: its stop.
    running: Mutex<Option<CancellationToken>>,
    /// Settings changed while a run had the agent: applied when it ends.
    pending: Mutex<Pending>,
    /// The plan's models.
    models: Mutex<Vec<providers::ModelInfo>>,
    /// The session's subagents running (`task`), by id.
    subagents: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// Starts a run for background commands that ended, when none is
    /// going (set once the agent is shared).
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    /// What side questions are asked with while a run has the agent (as
    /// it started).
    aside: Mutex<crate::agent::AsideParts>,
}

struct Modes {
    approval: String,
    plan: bool,
}

#[derive(Default)]
struct View {
    model: String,
    effort: String,
    name: Option<String>,
    session_file: Option<String>,
    session_id: String,
    messages: Vec<Value>,
    last_usage: Value,
}

#[derive(Default)]
struct Pending {
    model: Option<String>,
    effort: Option<String>,
    name: Option<String>,
}

impl Shared {
    fn emit(&self, v: Value) {
        let _ = self.out.send(v);
    }

    /// A run's events as pi's frames.
    fn show(&self, s: Shown) {
        match s {
            Shown::MessageStart(m) => self.emit(json!({ "type": "message_start", "message": m })),
            Shown::Stream(e) => self.emit(json!({ "type": "message_update", "assistantMessageEvent": e })),
            Shown::MessageEnd(m) => {
                self.view.lock().unwrap().messages.push(m.clone());
                self.emit(json!({ "type": "message_end", "message": m }));
            }
            Shown::Withdrawn => self.emit(json!({ "type": "message_end", "message": { "role": "assistant", "content": [], "timestamp": crate::session::now_ms() } })),
            Shown::ToolStart { call_id, name, args } => self.emit(json!({ "type": "tool_execution_start", "toolCallId": call_id, "toolName": name, "args": args })),
            Shown::ToolEnd { call_id, name, text, failed } => self.emit(json!({
                "type": "tool_execution_end", "toolCallId": call_id, "toolName": name,
                "result": { "content": [{ "type": "text", "text": text }] }, "isError": failed,
            })),
            Shown::Usage { usage, .. } => self.view.lock().unwrap().last_usage = usage.clone(),
            Shown::Queue { steering, follow_up } => self.emit(json!({ "type": "queue_update", "steering": steering, "followUp": follow_up })),
            Shown::CompactionStart { reason } => self.emit(json!({ "type": "compaction_start", "reason": reason })),
            Shown::Limits(l) => self.emit(json!({ "type": "harlo_limits", "provider": l["provider"], "account": l["account"], "windows": l["windows"] })),
            Shown::Notice { level, text } => self.emit(json!({ "type": "notice", "level": level, "message": text })),
            Shown::ModelChanged(model) => {
                self.view.lock().unwrap().model = model.to_owned();
                self.emit(json!({ "type": "model_changed", "model": self.model_json(model) }));
            }
            // (The context's size drops: harlo's meter follows it.)
            Shown::SetAside { after, .. } => self.view.lock().unwrap().last_usage = json!({ "input_tokens": after, "output_tokens": 0 }),
            Shown::CompactionEnd { reason, result, error, will_retry } => {
                if let Some(r) = result {
                    let mut v = self.view.lock().unwrap();
                    v.last_usage = json!({ "input_tokens": r["estimatedTokensAfter"], "output_tokens": 0 });
                    v.messages.push(json!({ "role": "compactionSummary", "summary": r["summary"], "tokensBefore": r["tokensBefore"], "timestamp": crate::session::now_ms() }));
                }
                let mut e = json!({ "type": "compaction_end", "reason": reason, "result": result, "aborted": result.is_none() && error.is_none(), "willRetry": will_retry });
                if let Some(err) = error {
                    e["errorMessage"] = json!(err);
                }
                self.emit(e);
            }
        }
    }

    fn queue_update(&self) {
        let (steering, follow_up) = self.inbox.lock().unwrap().texts();
        self.emit(json!({ "type": "queue_update", "steering": steering, "followUp": follow_up }));
    }

    /// Withdraws the approvals still shown (the run they were for ended).
    fn withdraw(&self) {
        for (id, _) in self.waiting.lock().unwrap().drain() {
            self.emit(json!({ "type": "extension_ui_request", "id": format!("{id}-cancel"), "method": "cancel", "targetId": id }));
        }
    }

    /// A model (`provider/id`) as pi describes one.
    fn model_json(&self, key: &str) -> Value {
        let reasoning = self.levels(key) != ["off"];
        let m = self.models.lock().unwrap().iter().find(|m| m.key() == key).cloned();
        let (provider, id) = key.split_once('/').unwrap_or((providers::CHATGPT, key));
        let base = providers::resolve(key).map(|(p, _)| p.base_url).unwrap_or_default();
        json!({
            "provider": provider, "id": id, "name": m.as_ref().map_or(id, |m| m.name.as_str()),
            "reasoning": reasoning, "baseUrl": base, "contextWindow": m.as_ref().map_or(0, |m| m.context_window),
            // (Not said: they're sent.)
            "input": if m.and_then(|m| m.images) == Some(false) { json!(["text"]) } else { json!(["text", "image"]) },
        })
    }

    fn context_window(&self, key: &str) -> u64 {
        self.models.lock().unwrap().iter().find(|m| m.key() == key).map_or(0, |m| m.context_window)
    }

    /// The model's thinking levels: its own, else the usual ones for a
    /// model that reasons, else just "off".
    fn levels(&self, key: &str) -> Vec<String> {
        let own = self.models.lock().unwrap().iter().find(|m| m.key() == key && !m.levels.is_empty()).map(|m| m.levels.clone());
        if let Some(levels) = own {
            return levels;
        }
        match providers::resolve(key) {
            Ok((p, id)) if !p.reasons(&id) => vec!["off".to_owned()],
            _ => LEVELS.iter().map(|l| l.to_string()).collect(),
        }
    }
}

/// Asks harlo before a call that changes something.
struct Approvals(Arc<Shared>);

impl Gate for Approvals {
    fn check(&self, name: &str, args: &Value) -> BoxFuture<'static, Result<(), String>> {
        let sh = self.0.clone();
        let name = name.to_owned();
        let args = args.clone();
        Box::pin(async move {
            let (approval, plan) = {
                let m = sh.modes.lock().unwrap();
                (m.approval.clone(), m.plan)
            };
            // (An MCP server's tool that may change things: as bash, but
            // not in plan mode.)
            let mcp = name.starts_with(crate::mcp::PREFIX);
            if plan && (mcp || matches!(name.as_str(), "write" | "edit")) {
                return Err(if mcp { PLAN_REFUSAL_MCP.to_owned() } else { PLAN_REFUSAL.to_owned() });
            }
            // Outside the sandbox: asked every time, in every mode.
            if name == "bash" && args["outside_sandbox"] == true && crate::sandbox::policy().on {
                let yes = yes_or_no(&sh, "Allow tool: bash\nOutside the sandbox", args["command"].as_str().unwrap_or_default()).await;
                return if yes { Ok(()) } else { Err(DENIED_OUTSIDE.to_owned()) };
            }
            let free = matches!(approval.as_str(), "yolo" | "auto");
            let ask = match approval.as_str() {
                _ if free => false,
                // (What's fetched can carry what's in the project away.)
                "write" => name == "bash" || name == "fetch" || mcp,
                _ => true,
            };
            if !ask || sh.always.lock().unwrap().contains(&name) {
                return Ok(());
            }
            let id = format!("harlo_agent_ui_{}", sh.next_ui.fetch_add(1, Ordering::Relaxed) + 1);
            let (tx, rx) = oneshot::channel();
            sh.waiting.lock().unwrap().insert(id.clone(), tx);
            let detail = match name.as_str() {
                "bash" => args["command"].as_str().unwrap_or_default().to_owned(),
                "fetch" => args["url"].as_str().unwrap_or_default().to_owned(),
                _ if mcp => {
                    let a = args.to_string();
                    if a.chars().count() > 300 { format!("{}…", a.chars().take(299).collect::<String>()) } else { a }
                }
                "edit" => format!("{} ({} change{})", args["path"].as_str().unwrap_or_default(), args["edits"].as_array().map_or(0, Vec::len), if args["edits"].as_array().map_or(0, Vec::len) == 1 { "" } else { "s" }),
                _ => args["path"].as_str().unwrap_or_default().to_owned(),
            };
            sh.emit(json!({
                "type": "extension_ui_request", "id": id, "method": "select",
                "title": format!("Allow tool: {name}"), "message": detail,
                "options": ["Approve", "Deny", "Always allow"], "alwaysHint": format!("{name} for this session"),
            }));
            let answer = rx.await.unwrap_or(Value::Null);
            match answer["value"].as_str() {
                Some("Approve") => Ok(()),
                Some("Always allow") => {
                    sh.always.lock().unwrap().insert(name);
                    Ok(())
                }
                _ => Err(DENIED.to_owned()),
            }
        })
    }

    fn confirm(&self, title: &str, message: &str) -> BoxFuture<'static, bool> {
        let (sh, title, message) = (self.0.clone(), title.to_owned(), message.to_owned());
        Box::pin(async move { yes_or_no(&sh, &title, &message).await })
    }

    fn ask(&self, questions: Value) -> BoxFuture<'static, Option<Value>> {
        let sh = self.0.clone();
        Box::pin(async move {
            let id = format!("harlo_agent_ui_{}", sh.next_ui.fetch_add(1, Ordering::Relaxed) + 1);
            let (tx, rx) = oneshot::channel();
            sh.waiting.lock().unwrap().insert(id.clone(), tx);
            sh.emit(json!({ "type": "extension_ui_request", "id": id, "method": "ask", "questions": questions }));
            let answer = rx.await.unwrap_or(Value::Null);
            (answer["cancelled"] != true && answer["answers"].is_array()).then(|| answer["answers"].clone())
        })
    }
}

/// Approve or Deny, asked as an approval (no "Always allow").
async fn yes_or_no(sh: &Shared, title: &str, message: &str) -> bool {
    let id = format!("harlo_agent_ui_{}", sh.next_ui.fetch_add(1, Ordering::Relaxed) + 1);
    let (tx, rx) = oneshot::channel();
    sh.waiting.lock().unwrap().insert(id.clone(), tx);
    sh.emit(json!({ "type": "extension_ui_request", "id": id, "method": "select", "title": title, "message": message, "options": ["Approve", "Deny"] }));
    rx.await.unwrap_or(Value::Null)["value"] == "Approve"
}

/// harlo's tools, called on the pipe.
struct Pipe(Arc<Shared>);

impl Harlo for Pipe {
    fn call(&self, tool: &str, args: &Value) -> BoxFuture<'static, Result<Value, String>> {
        let sh = self.0.clone();
        let id = format!("hc-{}", sh.next_harlo.fetch_add(1, Ordering::Relaxed) + 1);
        let (tx, rx) = oneshot::channel();
        sh.harlo_waiting.lock().unwrap().insert(id.clone(), tx);
        sh.emit(json!({ "type": "harlo_call", "id": id, "tool": tool, "args": args }));
        Box::pin(async move {
            let answer = tokio::time::timeout(HARLO_WAIT, rx).await;
            sh.harlo_waiting.lock().unwrap().remove(&id);
            match answer {
                Ok(Ok(a)) if a["ok"] == true => Ok(a["result"].clone()),
                Ok(Ok(a)) => Err(a["error"].as_str().unwrap_or("harlo couldn't do that").to_owned()),
                Ok(Err(_)) => Err("harlo is gone".to_owned()),
                Err(_) => Err("harlo didn't answer in time".to_owned()),
            }
        })
    }
}

pub async fn serve(keep: Keep, model: Option<String>, effort: Option<String>, harlo_tools: bool, launch: crate::agent::Launch) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = session::root().ok_or_else(|| anyhow::anyhow!("No home folder to keep sessions in"))?;
    let (log, past, name) = match keep {
        Keep::Nowhere => (session::Log::ephemeral(&cwd), session::Past::default(), None),
        Keep::New => (session::Log::new(&root, &cwd), session::Past::default(), None),
        Keep::File(p) if !p.exists() => (session::Log::new(&root, &cwd), session::Past::default(), None),
        Keep::File(p) => session::Log::open(&p)?.into_parts(),
    };
    let effort = effort.or_else(|| past.effort.clone()).unwrap_or_else(|| DEFAULT_LEVEL.to_owned());
    let model = model.or_else(|| past.model.clone()).map(|m| providers::full(&m)).unwrap_or_default();
    let mut agent = Agent::new(cwd.clone(), log, past, model, Some(effort));
    agent.launch(&launch);

    let (out, mut out_rx) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        // (Null: everything's been written.)
        while let Some(v) = out_rx.recv().await {
            if v.is_null() {
                break;
            }
            let mut line = v.to_string();
            line.push('\n');
            if stdout.write_all(line.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
                break;
            }
        }
    });
    let shared = Arc::new(Shared {
        out,
        waiting: Mutex::default(),
        modes: Mutex::new(Modes { approval: "always-ask".into(), plan: false }),
        always: Mutex::default(),
        next_ui: AtomicU64::new(0),
        harlo_waiting: Mutex::default(),
        next_harlo: AtomicU64::new(0),
        view: Mutex::default(),
        inbox: agent.inbox.clone(),
        running: Mutex::default(),
        pending: Mutex::default(),
        models: Mutex::default(),
        subagents: agent.subagents.clone(),
        wake: Mutex::default(),
        aside: Mutex::default(),
    });
    agent.gate = Some(Arc::new(Approvals(shared.clone())));
    if harlo_tools {
        agent.harlo = Some(Arc::new(Pipe(shared.clone())));
    }
    // Its subagents' frames, as omp's.
    {
        let sh = shared.clone();
        agent.notify = Some(Arc::new(move |frame| sh.emit(frame)));
    }
    {
        let mut v = shared.view.lock().unwrap();
        v.model = agent.model.clone();
        v.effort = agent.effort.clone().unwrap_or_default();
        v.name = name;
        v.session_file = (!agent.session_file().as_os_str().is_empty()).then(|| agent.session_file().display().to_string());
        v.session_id = agent.session_id().to_owned();
        v.messages = agent.messages().to_vec();
    }
    let agent = Arc::new(tokio::sync::Mutex::new(agent));
    // A background command that ends while nothing runs starts a run.
    {
        let (sh, weak) = (shared.clone(), Arc::downgrade(&agent));
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let (sh, weak) = (sh.clone(), weak.clone());
            tokio::spawn(async move {
                if sh.running.lock().unwrap().is_some() {
                    return;
                }
                if let Some(agent) = weak.upgrade() {
                    let _ = start(&sh, &agent, None).await;
                }
            });
        });
        *shared.wake.lock().unwrap() = Some(wake.clone());
        agent.lock().await.on_job_end(wake);
    }
    // The models (and a new session's model) in the background.
    {
        let sh = shared.clone();
        tokio::spawn(async move {
            let _ = load_models(&sh).await;
        });
    }

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let cmd: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                shared.emit(json!({ "type": "response", "command": "parse", "success": false, "error": format!("Not JSON: {e}") }));
                continue;
            }
        };
        handle(&shared, &agent, &root, &cwd, cmd).await;
    }
    // stdin closed: stop what's running, then go.
    if let Some(stop) = shared.running.lock().unwrap().take() {
        stop.cancel();
    }
    let _ = agent.lock().await;
    shared.emit(Value::Null);
    let _ = writer.await;
    Ok(())
}

async fn handle(sh: &Arc<Shared>, agent: &Arc<tokio::sync::Mutex<Agent>>, root: &std::path::Path, cwd: &std::path::Path, cmd: Value) {
    let ty = cmd["type"].as_str().unwrap_or_default().to_owned();
    let id = cmd["id"].clone();
    let reply = |result: Result<Value, String>| {
        let mut r = json!({ "type": "response", "command": ty, "success": result.is_ok() });
        if !id.is_null() {
            r["id"] = id.clone();
        }
        match result {
            Ok(data) if !data.is_null() => r["data"] = data,
            Ok(_) => {}
            Err(e) => r["error"] = json!(e),
        }
        sh.emit(r);
    };
    let busy = sh.running.lock().unwrap().is_some();
    let text = || cmd["message"].as_str().unwrap_or_default().to_owned();
    // (Fitted to send; one that can't be sent says why in its place.)
    let images = || crate::images::prompt_blocks(cmd["images"].as_array().map_or(&[][..], Vec::as_slice));
    // "/btw <question>": answered apart, while a run goes on too.
    if matches!(ty.as_str(), "prompt" | "steer" | "follow_up") && let Some(q) = btw(&text()) {
        reply(Ok(Value::Null));
        side_question(sh, agent, q).await;
        return;
    }
    match ty.as_str() {
        // "/compact [what to keep]", typed: as the compact command.
        "prompt" if text().trim() == "/compact" || text().trim_start().starts_with("/compact ") => {
            if busy {
                return reply(Err("A run is working: wait for it, or stop it first.".into()));
            }
            let focus = text().trim_start().strip_prefix("/compact").map(|f| f.trim().to_owned()).filter(|f| !f.is_empty());
            reply(Ok(Value::Null));
            compact_now(sh, agent, focus, None).await;
        }
        "compact" => {
            if busy {
                return reply(Err("A run is working: wait for it, or stop it first.".into()));
            }
            let focus = cmd["customInstructions"].as_str().map(str::to_owned);
            compact_now(sh, agent, focus, Some(id.clone())).await;
        }
        "prompt" => {
            let behavior = cmd["streamingBehavior"].as_str().unwrap_or_default();
            if busy && behavior.is_empty() {
                return reply(Err("A run is working: steer it or queue a follow-up.".into()));
            }
            if busy {
                queue(sh, behavior == "steer", text(), images());
                return reply(Ok(Value::Null));
            }
            match start(sh, agent, Some((text(), images()))).await {
                Ok(()) => reply(Ok(Value::Null)),
                Err(e) => reply(Err(e)),
            }
        }
        "steer" | "follow_up" => {
            if busy {
                queue(sh, ty == "steer", text(), images());
                return reply(Ok(Value::Null));
            }
            match start(sh, agent, Some((text(), images()))).await {
                Ok(()) => reply(Ok(Value::Null)),
                Err(e) => reply(Err(e)),
            }
        }
        "abort" => {
            if let Some(stop) = sh.running.lock().unwrap().as_ref() {
                stop.cancel();
            }
            sh.withdraw();
            reply(Ok(Value::Null))
        }
        "clear_queue" => {
            let (steering, follow_up) = std::mem::take(&mut *sh.inbox.lock().unwrap()).texts();
            sh.queue_update();
            reply(Ok(json!({ "steering": steering, "followUp": follow_up })))
        }
        "extension_ui_response" => {
            let ui = cmd["id"].as_str().unwrap_or_default();
            if let Some(tx) = sh.waiting.lock().unwrap().remove(ui) {
                let _ = tx.send(cmd.clone());
            }
        }
        // (`call`: the call's id; `id` is the command's own, answered.)
        "harlo_result" => {
            let call = cmd["call"].as_str().unwrap_or_default();
            if let Some(tx) = sh.harlo_waiting.lock().unwrap().remove(call) {
                let _ = tx.send(cmd.clone());
            }
            reply(Ok(Value::Null))
        }
        "harlo_set_modes" => {
            let mut m = sh.modes.lock().unwrap();
            if let Some(a) = cmd["approval"].as_str() {
                m.approval = a.to_owned();
            }
            if let Some(p) = cmd["plan"].as_bool() {
                m.plan = p;
            }
            drop(m);
            reply(Ok(Value::Null))
        }
        "get_state" => {
            let v = sh.view.lock().unwrap();
            let inbox = sh.inbox.lock().unwrap();
            let state = json!({
                "model": if v.model.is_empty() { Value::Null } else { sh.model_json(&v.model) },
                "thinkingLevel": v.effort,
                "isStreaming": busy,
                "isCompacting": false,
                "sessionFile": v.session_file,
                "sessionId": v.session_id,
                "sessionName": v.name,
                "messageCount": v.messages.len(),
                "pendingMessageCount": inbox.steering.len() + inbox.follow_up.len(),
                "steeringMode": "all",
                "followUpMode": "one-at-a-time",
                "autoCompactionEnabled": false,
            });
            drop((v, inbox));
            reply(Ok(state))
        }
        "get_messages" => {
            let messages = sh.view.lock().unwrap().messages.clone();
            reply(Ok(json!({ "messages": messages })))
        }
        "get_available_models" => match load_models(sh).await {
            Ok(()) => {
                let ids: Vec<String> = sh.models.lock().unwrap().iter().map(|m| m.key()).collect();
                reply(Ok(json!({ "models": ids.iter().map(|id| sh.model_json(id)).collect::<Vec<_>>() })))
            }
            Err(e) => reply(Err(e)),
        },
        "get_available_thinking_levels" => {
            let _ = load_models(sh).await;
            let model = sh.view.lock().unwrap().model.clone();
            reply(Ok(json!({ "levels": sh.levels(&model) })))
        }
        "set_model" => {
            let id = cmd["modelId"].as_str().or_else(|| cmd["model"].as_str()).unwrap_or_default();
            if id.is_empty() {
                return reply(Err("set_model needs modelId".into()));
            }
            let model = providers::full(&match cmd["provider"].as_str().filter(|p| !p.is_empty()) {
                Some(p) => format!("{p}/{id}"),
                None => id.to_owned(),
            });
            if let Err(e) = providers::resolve(&model) {
                return reply(Err(format!("{e:#}")));
            }
            let _ = load_models(sh).await;
            // A level this model doesn't take: the usual one, else its first
            // that thinks (one that only has off and high: high).
            let levels = sh.levels(&model);
            let effort = sh.view.lock().unwrap().effort.clone();
            if !levels.contains(&effort) {
                let thinks = (effort != "off").then(|| levels.iter().find(|l| *l != "off")).flatten();
                let level = if levels.iter().any(|l| l == DEFAULT_LEVEL) && effort != "off" { DEFAULT_LEVEL.to_owned() } else { thinks.unwrap_or(&levels[0]).clone() };
                match agent.try_lock() {
                    Ok(mut a) => a.effort = (level != "off").then(|| level.clone()),
                    Err(_) => sh.pending.lock().unwrap().effort = Some(level.clone()),
                }
                sh.view.lock().unwrap().effort = level.clone();
                sh.emit(json!({ "type": "thinking_level_changed", "level": level }));
            }
            match agent.try_lock() {
                Ok(mut a) => a.model = model.clone(),
                Err(_) => sh.pending.lock().unwrap().model = Some(model.clone()),
            }
            sh.view.lock().unwrap().model = model.clone();
            let m = sh.model_json(&model);
            sh.emit(json!({ "type": "model_changed", "model": m }));
            reply(Ok(m))
        }
        "set_thinking_level" => {
            let level = cmd["level"].as_str().unwrap_or_default().to_owned();
            let _ = load_models(sh).await;
            let levels = sh.levels(&sh.view.lock().unwrap().model.clone());
            if !levels.contains(&level) {
                return reply(Err(format!("This model's levels are {}", levels.join(", "))));
            }
            match agent.try_lock() {
                Ok(mut a) => a.effort = (level != "off").then(|| level.clone()),
                Err(_) => sh.pending.lock().unwrap().effort = Some(level.clone()),
            }
            sh.view.lock().unwrap().effort = level.clone();
            sh.emit(json!({ "type": "thinking_level_changed", "level": level }));
            reply(Ok(Value::Null))
        }
        "set_session_name" => {
            let name = cmd["name"].as_str().unwrap_or_default().trim().to_owned();
            let saved = match agent.try_lock() {
                Ok(mut a) => a.rename(&name).map_err(|e| format!("{e:#}")),
                Err(_) => {
                    sh.pending.lock().unwrap().name = Some(name.clone());
                    Ok(())
                }
            };
            if let Err(e) = saved {
                return reply(Err(e));
            }
            sh.view.lock().unwrap().name = (!name.is_empty()).then(|| name.clone());
            sh.emit(json!({ "type": "session_info_changed", "name": name }));
            reply(Ok(Value::Null))
        }
        "get_session_stats" => {
            let v = sh.view.lock().unwrap();
            let u = &v.last_usage;
            let tokens = u["input_tokens"].as_u64().unwrap_or(0) + u["output_tokens"].as_u64().unwrap_or(0);
            let count = |role: &str| v.messages.iter().filter(|m| m["role"] == role).count();
            let calls: usize = v.messages.iter().filter(|m| m["role"] == "assistant").map(|m| m["content"].as_array().map_or(0, |c| c.iter().filter(|b| b["type"] == "toolCall").count())).sum();
            let window = sh.context_window(&v.model);
            let stats = json!({
                "sessionFile": v.session_file, "sessionId": v.session_id,
                "userMessages": count("user"), "assistantMessages": count("assistant"), "toolCalls": calls, "toolResults": count("toolResult"),
                "totalMessages": v.messages.len(),
                "contextUsage": {
                    "tokens": tokens, "contextWindow": window, "compactAt": crate::compact::threshold(window),
                    "percent": if window > 0 { tokens as f64 * 100.0 / window as f64 } else { 0.0 },
                },
                "cost": 0.0,
            });
            drop(v);
            reply(Ok(stats))
        }
        "get_commands" | "get_available_commands" => {
            let mut commands = vec![
                json!({
                    "name": "compact", "description": "Summarize the conversation so far to free up context",
                    "source": "builtin", "input": { "hint": "what to keep (optional)" },
                }),
                json!({
                    "name": "btw", "description": "Ask a side question while the agent works: answered from the conversation, without tools, and not kept",
                    "source": "builtin", "input": { "hint": "your question" },
                }),
            ];
            let cwd = std::env::current_dir().unwrap_or_default();
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
            for s in crate::skills::find(&cwd, &home, &crate::auth::config_dir()) {
                commands.push(json!({ "name": format!("skill:{}", s.name), "description": s.description, "source": "skill", "path": s.path, "input": { "hint": "what to do (optional)" } }));
            }
            reply(Ok(json!({ "commands": commands })))
        }
        // (The running ones come as events.)
        "get_subagents" => reply(Ok(json!({ "subagents": [] }))),
        "cancel_subagent" => {
            let id = cmd["subagentId"].as_str().unwrap_or_default();
            let token = sh.subagents.lock().unwrap().get(id).cloned();
            if let Some(t) = &token {
                t.cancel();
            }
            reply(Ok(json!({ "cancelled": token.is_some() })))
        }
        "steer_subagent" => reply(Err("Harlo's subagents can't be steered: cancel one, or wait for its answer.".into())),
        "new_session" => {
            if busy {
                return reply(Err("A run is working: stop it first.".into()));
            }
            let mut a = agent.lock().await;
            let ephemeral = a.session_file().as_os_str().is_empty();
            a.new_session(if ephemeral { session::Log::ephemeral(cwd) } else { session::Log::new(root, cwd) });
            let mut v = sh.view.lock().unwrap();
            v.messages.clear();
            v.name = None;
            v.last_usage = Value::Null;
            v.session_id = a.session_id().to_owned();
            v.session_file = (!ephemeral).then(|| a.session_file().display().to_string());
            drop(v);
            sh.always.lock().unwrap().clear();
            reply(Ok(json!({ "cancelled": false })))
        }
        // The session from just before one of your messages, in the same
        // file (what you send next is a new branch; the old one stays): its
        // text comes back to edit and send again.
        "fork" => {
            if busy {
                return reply(Err("A run is working: stop it first.".into()));
            }
            let Some(entry) = cmd["entryId"].as_str().filter(|e| !e.is_empty()) else { return reply(Err("fork needs entryId".into())) };
            let mut a = agent.lock().await;
            let file = a.session_file().to_owned();
            if file.as_os_str().is_empty() || !file.exists() {
                return reply(Err("This session isn't saved yet, so its messages can't be edited.".into()));
            }
            let (loaded, text) = match session::Log::branch_before(&file, entry) {
                Ok(f) => f,
                Err(e) => return reply(Err(format!("{e:#}"))),
            };
            let name = loaded.name.clone();
            a.switch(loaded);
            let mut v = sh.view.lock().unwrap();
            v.messages = a.messages().to_vec();
            v.name = name;
            v.last_usage = Value::Null;
            v.session_id = a.session_id().to_owned();
            v.session_file = Some(a.session_file().display().to_string());
            drop(v);
            sh.always.lock().unwrap().clear();
            reply(Ok(json!({ "text": text, "cancelled": false })))
        }
        "" => reply(Err("The command has no type".into())),
        other => reply(Err(format!("harlo-agent doesn't know the command {other}"))),
    }
}

fn queue(sh: &Shared, steer: bool, text: String, images: Vec<Value>) {
    {
        let mut inbox = sh.inbox.lock().unwrap();
        let m = crate::agent::Queued { text, images };
        if steer { inbox.steering.push(m) } else { inbox.follow_up.push(m) }
    }
    sh.queue_update();
}

/// The plan's models, listed once. A session with no model yet gets the
/// plan's first (the agent takes it at its first run).
async fn load_models(sh: &Shared) -> Result<(), String> {
    if !sh.models.lock().unwrap().is_empty() {
        return Ok(());
    }
    let (list, errors) = providers::every_model().await;
    if list.is_empty() {
        return Err(if errors.is_empty() { "No models: sign in with ChatGPT or set up a provider".to_owned() } else { errors.join("; ") });
    }
    let first = list.first().map(|m| m.key());
    *sh.models.lock().unwrap() = list;
    let chosen = {
        let mut v = sh.view.lock().unwrap();
        match first {
            Some(m) if v.model.is_empty() => {
                v.model = m.clone();
                Some(m)
            }
            _ => None,
        }
    };
    // (Said at once, so harlo shows the model before the first prompt.)
    if let Some(m) = chosen {
        sh.emit(json!({ "type": "model_changed", "model": sh.model_json(&m) }));
    }
    Ok(())
}

/// Summarizes the conversation so far (the agent is free), as a run:
/// answered when done when `reply_to` is a command's id.
async fn compact_now(sh: &Arc<Shared>, agent: &Arc<tokio::sync::Mutex<Agent>>, focus: Option<String>, reply_to: Option<Value>) {
    let mut a = agent.clone().lock_owned().await;
    let _ = load_models(sh).await;
    a.known.add(sh.models.lock().unwrap().iter());
    let stop = CancellationToken::new();
    *sh.running.lock().unwrap() = Some(stop.clone());
    let sh = sh.clone();
    // (Typed, it's a run as a prompt's is; asked for, only the command's answer.)
    let typed = reply_to.is_none();
    tokio::spawn(async move {
        if typed {
            sh.emit(json!({ "type": "agent_start" }));
        }
        let shown = sh.clone();
        let mut show = |s: Shown| shown.show(s);
        let outcome = a.compact("manual", focus.as_deref(), &stop, &mut show).await;
        let too_little = matches!(outcome, Compacted::Nothing);
        let result = match outcome {
            Compacted::Done(r) => Ok(r),
            Compacted::Nothing => Err("There's nothing to compact yet: the conversation is still short.".to_owned()),
            Compacted::Stopped => Err("Stopped.".to_owned()),
            Compacted::Failed(e) => Err(e),
        };
        drop(a);
        *sh.running.lock().unwrap() = None;
        if let Some(id) = reply_to {
            let mut r = json!({ "type": "response", "command": "compact", "id": id, "success": result.is_ok() });
            match result {
                Ok(data) => r["data"] = data,
                Err(e) => r["error"] = json!(e),
            }
            sh.emit(r);
        } else if too_little {
            // (Typed, with nothing shown yet: a line says why, not as a
            // failure.)
            sh.emit(json!({ "type": "notice", "level": "warning", "message": result.err() }));
        }
        if typed {
            sh.emit(json!({ "type": "agent_end", "messages": [] }));
            sh.emit(json!({ "type": "agent_settled" }));
        }
    });
}

/// Starts a run on `prompt` and its images (the agent is free).
/// Starts a run on `what` (a prompt and its images), or (None) on the
/// background commands that ended, if any did.
/// `/btw <question>`'s question.
fn btw(text: &str) -> Option<String> {
    let t = text.trim_start();
    let q = t.strip_prefix("/btw")?;
    (q.is_empty() || q.starts_with(char::is_whitespace)).then(|| q.trim().to_owned())
}

/// A side question: answered from the conversation, beside the task (no
/// tools, nothing kept), as `harlo_aside` frames.
async fn side_question(sh: &Arc<Shared>, agent: &Arc<tokio::sync::Mutex<Agent>>, question: String) {
    if question.is_empty() {
        sh.emit(json!({ "type": "harlo_aside", "question": "", "text": "", "done": true, "error": "Ask something after /btw." }));
        return;
    }
    // Idle: the agent's own; working: as the run started, and the
    // messages so far.
    let (parts, history) = match agent.clone().try_lock_owned() {
        Ok(mut a) => (a.aside_parts().await, a.history().to_vec()),
        Err(_) => (sh.aside.lock().unwrap().clone(), sh.view.lock().unwrap().messages.clone()),
    };
    let sh = sh.clone();
    tokio::spawn(async move {
        sh.emit(json!({ "type": "harlo_aside", "question": question, "text": "", "done": false }));
        let shown = sh.clone();
        let q = question.clone();
        let mut on_text = move |t: &str| shown.emit(json!({ "type": "harlo_aside", "question": q, "text": t, "done": false }));
        match crate::agent::aside(&parts, &history, &question, &mut on_text).await {
            Ok(text) => sh.emit(json!({ "type": "harlo_aside", "question": question, "text": text, "done": true })),
            Err(e) => sh.emit(json!({ "type": "harlo_aside", "question": question, "text": "", "done": true, "error": format!("{e:#}") })),
        }
    });
}

async fn start(sh: &Arc<Shared>, agent: &Arc<tokio::sync::Mutex<Agent>>, what: Option<(String, Vec<Value>)>) -> Result<(), String> {
    let mut a = agent.clone().lock_owned().await;
    if what.is_none() && (!a.has_notices() || sh.running.lock().unwrap().is_some()) {
        return Ok(());
    }
    a.known.add(sh.models.lock().unwrap().iter());
    if a.model.is_empty() {
        load_models(sh).await?;
        let m = sh.view.lock().unwrap().model.clone();
        if m.is_empty() {
            return Err("Your plan lists no models".into());
        }
        a.model = m;
    }
    let stop = CancellationToken::new();
    *sh.running.lock().unwrap() = Some(stop.clone());
    *sh.aside.lock().unwrap() = a.aside_parts().await;
    let sh = sh.clone();
    tokio::spawn(async move {
        sh.emit(json!({ "type": "agent_start" }));
        let shown = sh.clone();
        let mut show = |s: Shown| shown.show(s);
        let result = match &what {
            Some((prompt, images)) => a.run(prompt, images, &stop, &mut show).await,
            None => a.wake(&stop, &mut show).await,
        };
        if let Err(e) = result {
            let _ = a.failed(&format!("{e:#}"), &mut show);
        }
        // What was changed while it ran.
        let p = std::mem::take(&mut *sh.pending.lock().unwrap());
        if let Some(m) = p.model {
            a.model = m;
        }
        // ("off": none.)
        if let Some(e) = p.effort {
            a.effort = (e != "off").then_some(e);
        }
        if let Some(n) = p.name {
            let _ = a.rename(&n);
        }
        let ended_since = a.has_notices();
        drop(a);
        sh.withdraw();
        *sh.running.lock().unwrap() = None;
        sh.emit(json!({ "type": "agent_end", "messages": [] }));
        sh.emit(json!({ "type": "agent_settled" }));
        // (A command that ended as the run finished: told now.)
        if ended_since && let Some(w) = sh.wake.lock().unwrap().clone() {
            w();
        }
    });
    Ok(())
}
