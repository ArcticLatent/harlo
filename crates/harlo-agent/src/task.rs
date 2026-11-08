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
pub const EXPLORE: &[&str] = &["read", "grep", "find", "ls", "bash", "recall", "current_time"];
pub const GENERAL: &[&str] = &["read", "write", "edit", "bash", "grep", "find", "ls", "recall", "current_time"];

/// Whether sessions get `task` (`HARLO_AGENT_NO_TASK` set: not, to compare).
pub fn enabled() -> bool {
    std::env::var_os("HARLO_AGENT_NO_TASK").is_none()
}

pub fn spec() -> Value {
    json!({
        "type": "function", "name": "task", "strict": false,
        "description": "Runs subagents: each takes one task with a fresh context (none of this conversation, so its prompt must say everything it needs: \
                        the goal, paths, what to report), works with its own tools, and its final message comes back to you. Several run at the same time. \
                        Use it for independent parts worth doing in parallel, or a search through much of the code that would fill your context; \
                        not for a quick look you can do yourself. kind explore (default) reads, searches and runs commands but changes no files; \
                        general may also edit and write files (give parallel ones separate files).",
        "parameters": {
            "type": "object",
            "properties": { "tasks": { "type": "array", "maxItems": MAX_TASKS, "items": {
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "A few words, shown to the user" },
                    "prompt": { "type": "string", "description": "The whole task, self-contained" },
                    "kind": { "type": "string", "enum": ["explore", "general"] },
                },
                "required": ["description", "prompt"], "additionalProperties": false,
            } } },
            "required": ["tasks"], "additionalProperties": false,
        },
    })
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
    kind: &'static str,
    prompt: String,
    status: &'static str,
    text: String,
    usage: Value,
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
    let slots = tokio::sync::Semaphore::new(AT_ONCE);
    let done: Vec<Done> = join_all(tasks.iter().map(|t| one(t, call_id, ctx, &slots, stop))).await;
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

async fn one(task: &Value, call_id: &str, ctx: &Ctx, slots: &tokio::sync::Semaphore, stop: &CancellationToken) -> Done {
    let description = task["description"].as_str().map(str::trim).filter(|d| !d.is_empty()).unwrap_or("task").to_owned();
    let prompt = task["prompt"].as_str().unwrap_or_default().to_owned();
    let (kind, allowed) = if task["kind"] == "general" { ("general", GENERAL) } else { ("explore", EXPLORE) };
    let id = format!("task-{}", hex_id());
    let mut done = Done { id: id.clone(), description: description.clone(), kind, prompt: prompt.clone(), status: "aborted", text: String::new(), usage: json!({}) };
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
    let file = ctx.parent_file.as_ref().map(|p| p.with_extension("").join(format!("{id}.jsonl")));
    let log = match (&file, &ctx.parent_file) {
        (Some(f), Some(p)) => Log::at(f.clone(), &ctx.cwd, p),
        _ => Log::ephemeral(&ctx.cwd),
    };
    let mut agent = Agent::new(ctx.cwd.clone(), log, Past::default(), ctx.model.clone(), ctx.effort.clone());
    agent.gate = ctx.gate.clone();
    agent.allowed = Some(allowed);
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
            crate::tools::specs_of(Some(only), false).as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
        };
        let explore = names(EXPLORE);
        assert!(!explore.iter().any(|n| n == "write" || n == "edit" || n == "task"), "{explore:?}");
        assert!(explore.contains(&"bash".to_owned()) && explore.contains(&"read".to_owned()));
        let general = names(GENERAL);
        assert!(general.contains(&"edit".to_owned()) && !general.contains(&"task".to_owned()));
        let all = crate::tools::specs_of(None, true);
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
            notify: None, known: Default::default(), running: Arc::default(),
        };
        let stop = CancellationToken::new();
        assert!(run(&json!({}), "c", &ctx, &stop).await.failed);
        assert!(run(&json!({ "tasks": [{ "description": "d", "prompt": " " }] }), "c", &ctx, &stop).await.text.contains("no prompt"));
        let many: Vec<Value> = (0..9).map(|_| json!({ "description": "d", "prompt": "p" })).collect();
        assert!(run(&json!({ "tasks": many }), "c", &ctx, &stop).await.text.contains("At most 8"));
        // Stopped first: none starts, and each says so.
        stop.cancel();
        let o = run(&json!({ "tasks": [{ "description": "a", "prompt": "p" }, { "description": "b", "prompt": "q" }] }), "c", &ctx, &stop).await;
        assert!(o.failed && o.text.contains("## a (aborted)") && o.text.contains("## b (aborted)"), "{}", o.text);
        assert_eq!(o.details["progress"].as_array().unwrap().len(), 2);
        assert!(ctx.running.lock().unwrap().is_empty());
    }
}
