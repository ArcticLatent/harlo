//! `task`: subagents inside one session. Each task is a new agent in this
//! process with the session's model, folder and instructions but none of
//! its conversation, a smaller set of tools, and its own log (beside the
//! session's, in a folder named after it). Up to [`AT_ONCE`] run at a
//! time; their final messages come back together, in the order asked.
//! They use no `task` and none of harlo's tools: no subagents of their
//! own, no helper sessions. Approvals go through the session's own; Stop
//! ends them all.
//!
//! What harlo's Subagents box shows comes as omp's frames:
//! `subagent_lifecycle` when one starts and ends, `subagent_progress` as it
//! calls tools. The result's `details.progress` lists them for a saved
//! session.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures_util::future::{BoxFuture, join_all};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::agent::{Agent, Ended, Gate, Notify, Shown};
use crate::session::{Log, Past};
use crate::tools::Outcome;

/// Tasks in one call, at most, and running at once.
pub const MAX_TASKS: usize = 8;
pub const AT_ONCE: usize = 4;
/// Of a subagent's answer, what its end frame carries.
const RESULT_SHOWN: usize = 2000;

/// What each kind may use.
pub const EXPLORE: &[&str] = &["read", "grep", "find", "ls", "bash", "fetch", "recall", "current_time"];
pub const GENERAL: &[&str] = &["read", "write", "edit", "bash", "grep", "find", "ls", "fetch", "recall", "current_time"];

/// Whether sessions get `task` (`HARLO_AGENT_NO_TASK` set: not, to compare).
pub fn enabled() -> bool {
    std::env::var_os("HARLO_AGENT_NO_TASK").is_none()
}

/// `profiles`: the agent profiles a task may name.
pub fn spec(profiles: &[crate::profiles::Profile]) -> Value {
    let agents = if profiles.is_empty() {
        String::new()
    } else {
        let listed: Vec<String> = profiles.iter().take(crate::profiles::LISTED).map(|p| format!("{}: {}", p.name, p.description.trim())).collect();
        format!(" agent names one of these profiles instead of kind (its instructions and tools, maybe its model): {}.", listed.join("; "))
    };
    let mut spec = json!({
        "type": "function", "name": "task", "strict": false,
        "description": "Runs subagents: each takes one task with a fresh context (none of this conversation, so its prompt must say everything it needs: \
                        the goal, paths, what to report), works with its own tools, and its final message comes back to you. Several run at the same time. \
                        Use it for independent parts worth doing in parallel, or a search through much of the code that would fill your context; \
                        not for a quick look you can do yourself. kind explore (default) reads, searches and runs commands but changes no files; \
                        general may also edit and write files; in a git project, two or more general tasks in one call each work in their own copy of the files, \
                        and their changes are brought back here when they finish (any that clash with another's are kept aside, and the result says where).",
        "parameters": {
            "type": "object",
            "properties": { "tasks": { "type": "array", "maxItems": MAX_TASKS, "items": {
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "A few words, shown to the user" },
                    "prompt": { "type": "string", "description": "The whole task, self-contained" },
                    "kind": { "type": "string", "enum": ["explore", "general"] },
                    "agent": { "type": "string", "description": "An agent profile's name, instead of kind" },
                    "model": { "type": "string", "description": "provider/model to run it on, if not the subagents' default (a smaller one for simple work, a stronger one for hard)" },
                    "effort": { "type": "string", "enum": ["off", "minimal", "low", "medium", "high", "xhigh"], "description": "Its reasoning effort, if not the default" },
                },
                "required": ["description", "prompt"], "additionalProperties": false,
            } } },
            "required": ["tasks"], "additionalProperties": false,
        },
    });
    if let Some(d) = spec["description"].as_str().map(|d| format!("{d}{agents}")) {
        spec["description"] = json!(d);
    }
    spec
}

/// What a session's subagents take from it.
pub struct Ctx {
    pub model: String,
    pub effort: Option<String>,
    pub cwd: PathBuf,
    pub gate: Option<Arc<dyn Gate>>,
    /// The session's file (none: kept nowhere, and so are they).
    pub parent_file: Option<PathBuf>,
    pub cache_key: String,
    pub notify: Option<Notify>,
    pub known: crate::agent::Known,
    /// Those running, by id: for cancelling one.
    pub running: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// The agent profiles tasks may name.
    pub profiles: Vec<crate::profiles::Profile>,
    /// What subagents run on when a task doesn't say (None: the session's model).
    pub task_model: Option<String>,
    /// The session's token budget, which they share.
    pub budget: Option<Arc<crate::agent::Budget>>,
    /// What they go on with when their model's plan is used up.
    pub fallback: Option<String>,
}

impl Ctx {
    fn tell(&self, frame: Value) {
        if let Some(n) = &self.notify {
            n(frame);
        }
    }
}

/// One subagent, done.
struct Done {
    id: String,
    description: String,
    kind: String,
    prompt: String,
    status: &'static str,
    text: String,
    usage: Value,
    /// Where it worked, when not in the session's folder.
    worktree: Option<crate::isolate::Worktree>,
}

/// Runs the call's tasks: their answers, in order. (Boxed: a run inside
/// a run, its type named here.)
pub fn run<'a>(args: &'a Value, call_id: &'a str, ctx: &'a Ctx, stop: &'a CancellationToken) -> BoxFuture<'a, Outcome> {
    Box::pin(run_all(args, call_id, ctx, stop))
}

async fn run_all(args: &Value, call_id: &str, ctx: &Ctx, stop: &CancellationToken) -> Outcome {
    let tasks: Vec<&Value> = args["tasks"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    if tasks.is_empty() {
        return Outcome::new("Give tasks: a list of {description, prompt, kind}.", true);
    }
    if tasks.len() > MAX_TASKS {
        return Outcome::new(format!("At most {MAX_TASKS} tasks in one call."), true);
    }
    if let Some(i) = tasks.iter().position(|t| t["prompt"].as_str().is_none_or(|p| p.trim().is_empty())) {
        return Outcome::new(format!("Task {} has no prompt.", i + 1), true);
    }
    for (i, t) in tasks.iter().enumerate() {
        if let Some(a) = t["agent"].as_str().filter(|a| !a.is_empty())
            && !ctx.profiles.iter().any(|p| p.name == a)
        {
            let names: Vec<&str> = ctx.profiles.iter().map(|p| p.name.as_str()).collect();
            return Outcome::new(format!("Task {}: there's no agent profile {a}{}.", i + 1, if names.is_empty() { String::new() } else { format!(" (there are: {})", names.join(", ")) }), true);
        }
        if let Some(m) = t["model"].as_str().filter(|m| !m.is_empty())
            && let Err(e) = crate::providers::resolve(m)
        {
            return Outcome::new(format!("Task {}: {e:#}.", i + 1), true);
        }
    }
    // Two or more that edit: each in a worktree, when the project allows.
    let writers = tasks.iter().filter(|t| writes(t, ctx)).count();
    let base = if writers >= 2 {
        let cwd = ctx.cwd.clone();
        tokio::task::spawn_blocking(move || crate::isolate::Baseline::take(&cwd)).await.ok().and_then(Result::ok).map(Arc::new)
    } else {
        None
    };
    let slots = tokio::sync::Semaphore::new(AT_ONCE);
    let mut done: Vec<Done> = join_all(tasks.iter().map(|t| one(t, call_id, ctx, &slots, stop, base.clone()))).await;
    // Their changes, brought back in the order asked.
    if let Some(base) = &base {
        for d in &mut done {
            let Some(w) = d.worktree.take() else { continue };
            let b = base.clone();
            let applied = tokio::task::spawn_blocking(move || b.apply(&w)).await;
            match applied {
                Ok(crate::isolate::Applied::Nothing) | Err(_) => {}
                Ok(crate::isolate::Applied::Done(files)) => d.text.push_str(&format!("\n\n(It worked in its own copy; its changes were brought back: {}.)", files.join(", "))),
                Ok(crate::isolate::Applied::Kept { why, dir, patch, files }) => d.text.push_str(&format!(
                    "\n\n(It worked in its own copy. Its changes to {} are NOT in the project: they clash with an earlier task's ({why}). They're in {}{}. Bring them over yourself (read them and make the same changes) before you say the work is done.)",
                    if files.is_empty() { "the files".to_owned() } else { files.join(", ") },
                    dir.display(),
                    if patch.as_os_str().is_empty() { String::new() } else { format!(", as a patch in {}", patch.display()) },
                )),
            }
        }
    }
    let mut text = String::new();
    let mut total = (0, 0, 0, 0);
    for d in &done {
        let answer = if d.text.trim().is_empty() { "(no answer)" } else { d.text.trim() };
        text.push_str(&format!("## {} ({})\n{answer}\n\n", d.description, d.status));
        total.0 += d.usage["requests"].as_u64().unwrap_or(0);
        total.1 += d.usage["input"].as_u64().unwrap_or(0);
        total.2 += d.usage["cached"].as_u64().unwrap_or(0);
        total.3 += d.usage["output"].as_u64().unwrap_or(0);
    }
    let progress: Vec<Value> = done
        .iter()
        .map(|d| json!({ "id": d.id, "agent": d.kind, "description": d.description, "assignment": d.prompt, "status": d.status, "usage": d.usage }))
        .collect();
    Outcome {
        text: text.trim_end().to_owned(),
        failed: done.iter().all(|d| d.status != "completed"),
        details: json!({ "progress": progress, "usage": { "requests": total.0, "input": total.1, "cached": total.2, "output": total.3 } }),
        images: Vec::new(),
    }
}

async fn one(task: &Value, call_id: &str, ctx: &Ctx, slots: &tokio::sync::Semaphore, stop: &CancellationToken, base: Option<Arc<crate::isolate::Baseline>>) -> Done {
    let description = task["description"].as_str().map(str::trim).filter(|d| !d.is_empty()).unwrap_or("task").to_owned();
    let prompt = task["prompt"].as_str().unwrap_or_default().to_owned();
    let profile = task["agent"].as_str().and_then(|a| ctx.profiles.iter().find(|p| p.name == a));
    let (kind, allowed) = match profile {
        Some(p) => (p.name.clone(), p.tools),
        None if task["kind"] == "general" => ("general".to_owned(), GENERAL),
        None => ("explore".to_owned(), EXPLORE),
    };
    let writer = writes(task, ctx);
    // Its model: the task's, its profile's, the subagents', else the session's.
    let model = [task["model"].as_str(), profile.map(|p| p.model.as_str()), ctx.task_model.as_deref()]
        .into_iter()
        .flatten()
        .find(|m| !m.is_empty() && crate::providers::resolve(m).is_ok())
        .unwrap_or(&ctx.model)
        .to_owned();
    // (Another provider's model: its own default effort, unless asked.)
    let same_provider = model.split_once('/').map(|m| m.0) == ctx.model.split_once('/').map(|m| m.0);
    let effort = match task["effort"].as_str().filter(|e| !e.is_empty()) {
        Some("off") => None,
        Some(e) => Some(e.to_owned()),
        None if same_provider => ctx.effort.clone(),
        None => None,
    };
    let id = format!("task-{}", hex_id());
    let mut done = Done { id: id.clone(), description: description.clone(), kind: kind.clone(), prompt: prompt.clone(), status: "aborted", text: String::new(), usage: json!({}), worktree: None };
    let token = stop.child_token();
    ctx.running.lock().unwrap().insert(id.clone(), token.clone());
    // (Waiting for a free place; stopped while it waits: never started.)
    let permit = tokio::select! {
        biased;
        _ = token.cancelled() => None,
        p = slots.acquire() => p.ok(),
    };
    if permit.is_none() {
        ctx.running.lock().unwrap().remove(&id);
        done.text = "Stopped before it started.".into();
        return done;
    }
    // (A writer beside others: in its own worktree; none made, in place.)
    let mut cwd = ctx.cwd.clone();
    if let Some(b) = base.filter(|_| writer) {
        let name = id.clone();
        if let Ok(Ok(w)) = tokio::task::spawn_blocking(move || b.worktree(&name)).await {
            cwd = w.cwd.clone();
            done.worktree = Some(w);
        }
    }
    let file = ctx.parent_file.as_ref().map(|p| p.with_extension("").join(format!("{id}.jsonl")));
    let log = match (&file, &ctx.parent_file) {
        (Some(f), Some(p)) => Log::at(f.clone(), &cwd, p),
        _ => Log::ephemeral(&cwd),
    };
    let mut agent = Agent::new(cwd, log, Past::default(), model, effort);
    agent.gate = ctx.gate.clone();
    agent.budget = ctx.budget.clone();
    agent.fallback = ctx.fallback.clone();
    agent.allowed = Some(allowed);
    if let Some(p) = profile.filter(|p| !p.body.is_empty()) {
        agent.add_instructions(&format!("\n<agent_profile name=\"{}\">\n{}\n</agent_profile>\n", p.name, p.body));
    }
    agent.cache_key = Some(format!("{}/task", ctx.cache_key));
    agent.known = ctx.known.clone();
    let _ = agent.rename(&description);
    ctx.tell(json!({ "type": "subagent_lifecycle", "payload": {
        "id": id, "agent": kind, "description": description, "assignment": prompt, "status": "running",
        "sessionFile": file, "parentToolCallId": call_id,
    } }));
    let mut calls = 0u64;
    let notify = ctx.notify.clone();
    let mut show = |s: Shown| match (s, &notify) {
        (Shown::ToolStart { name, .. }, Some(n)) => {
            calls += 1;
            n(json!({ "type": "subagent_progress", "payload": { "id": id, "progress": { "id": id, "status": "running", "currentTool": name, "toolCount": calls } } }));
        }
        // (A subagent's requests draw on the same plan: its limits are the session's.)
        (Shown::Limits(l), Some(n)) => n(json!({ "type": "harlo_limits", "provider": l["provider"], "account": l["account"], "windows": l["windows"] })),
        _ => {}
    };
    let ended = agent.run(&prompt, &[], &token, &mut show).await;
    done.status = match &ended {
        Ok(Ended::Done) => "completed",
        Ok(Ended::Stopped) => "aborted",
        Err(_) => "failed",
    };
    if let Err(e) = ended {
        let why = format!("{e:#}");
        let mut quiet = |_: Shown| {};
        let _ = agent.failed(&why, &mut quiet);
        done.text = format!("Failed: {why}");
    } else {
        done.text = last_text(agent.messages());
    }
    let t = &agent.totals;
    done.usage = json!({ "requests": t.requests, "input": t.input, "cached": t.cached, "output": t.output });
    ctx.running.lock().unwrap().remove(&id);
    let shown: String = done.text.chars().take(RESULT_SHOWN).collect();
    ctx.tell(json!({ "type": "subagent_lifecycle", "payload": { "id": id, "status": done.status, "result": shown, "usage": done.usage } }));
    drop(permit);
    done
}

/// Whether a task changes files: `general`, or a profile that may.
fn writes(task: &Value, ctx: &Ctx) -> bool {
    match task["agent"].as_str().and_then(|a| ctx.profiles.iter().find(|p| p.name == a)) {
        Some(p) => p.writes(),
        None => task["kind"] == "general",
    }
}

/// The last thing the model said (its final message's text).
fn last_text(history: &[Value]) -> String {
    history
        .iter()
        .rev()
        .filter(|m| m["role"] == "assistant")
        .map(|m| m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(""))
        .find(|t| !t.trim().is_empty())
        .unwrap_or_default()
}

fn hex_id() -> String {
    let mut b = [0u8; 4];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_each_kind_may_use() {
        let names = |only: &[&str]| -> Vec<String> {
            crate::tools::specs_of(Some(only), None).as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
        };
        let explore = names(EXPLORE);
        assert!(!explore.iter().any(|n| n == "write" || n == "edit" || n == "task"), "{explore:?}");
        assert!(explore.contains(&"bash".to_owned()) && explore.contains(&"read".to_owned()));
        let general = names(GENERAL);
        assert!(general.contains(&"edit".to_owned()) && !general.contains(&"task".to_owned()));
        let all = crate::tools::specs_of(None, Some(&[]));
        assert_eq!(all.as_array().unwrap().last().unwrap()["name"], "task");
    }

    #[test]
    fn the_last_answer() {
        let h = vec![
            json!({ "role": "assistant", "content": [{ "type": "text", "text": "first" }] }),
            json!({ "role": "toolResult", "content": [] }),
            json!({ "role": "assistant", "content": [{ "type": "text", "text": "fin" }, { "type": "text", "text": "al" }] }),
            json!({ "role": "assistant", "content": [{ "type": "toolCall", "name": "ls" }] }),
        ];
        assert_eq!(last_text(&h), "final");
    }

    #[tokio::test]
    async fn bad_calls_are_refused() {
        let ctx = Ctx {
            model: "x/y".into(), effort: None, cwd: std::env::temp_dir(), gate: None, parent_file: None, cache_key: "k".into(),
            notify: None, known: Default::default(), running: Arc::default(), profiles: Vec::new(), task_model: None, budget: None, fallback: None,
        };
        let stop = CancellationToken::new();
        assert!(run(&json!({}), "c", &ctx, &stop).await.failed);
        assert!(run(&json!({ "tasks": [{ "description": "d", "prompt": " " }] }), "c", &ctx, &stop).await.text.contains("no prompt"));
        let many: Vec<Value> = (0..9).map(|_| json!({ "description": "d", "prompt": "p" })).collect();
        assert!(run(&json!({ "tasks": many }), "c", &ctx, &stop).await.text.contains("At most 8"));
        assert!(run(&json!({ "tasks": [{ "description": "d", "prompt": "p", "agent": "nobody" }] }), "c", &ctx, &stop).await.text.contains("no agent profile nobody"));
        // Stopped first: none starts, and each says so.
        stop.cancel();
        let o = run(&json!({ "tasks": [{ "description": "a", "prompt": "p" }, { "description": "b", "prompt": "q" }] }), "c", &ctx, &stop).await;
        assert!(o.failed && o.text.contains("## a (aborted)") && o.text.contains("## b (aborted)"), "{}", o.text);
        assert_eq!(o.details["progress"].as_array().unwrap().len(), 2);
        assert!(ctx.running.lock().unwrap().is_empty());
    }
}
