//! `task`: subagents inside one session. Each task is a new agent in this
//! process with a smaller set of tools and its own log (beside the
//! session's, in a folder named after it). It starts from the session's
//! conversation as it is (a fork: its first request is the session's last
//! one plus its part, so the cache serves it), except two or more that
//! edit side by side (each in its own copy of the files) and a task on
//! another model or profile: those start fresh, with the session's
//! instructions only. Up to [`AT_ONCE`] run at a
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
pub const EXPLORE: &[&str] = &["read", "grep", "find", "ls", "bash", "fetch", "recall", "current_time", "lsp"];
pub const GENERAL: &[&str] = &["read", "write", "edit", "bash", "grep", "find", "ls", "fetch", "recall", "current_time", "lsp"];

/// Whether sessions get `task` (`HARLO_AGENT_NO_TASK` set: not, to compare).
pub fn enabled() -> bool {
    std::env::var_os("HARLO_AGENT_NO_TASK").is_none()
}

/// Whether subagents start from the conversation so far (forks; measured:
/// as often right, fewer tokens where they look around) instead of with
/// nothing (`HARLO_AGENT_TASK=fresh`).
pub fn forks() -> bool {
    std::env::var("HARLO_AGENT_TASK").as_deref() != Ok("fresh")
}

/// What a fork starts from: the session's requests as they were sent
/// (instructions, tools and the conversation before the call), so its
/// first request shares their cache.
pub struct Fork {
    pub system: String,
    pub specs: Value,
    pub history: Vec<Value>,
}

/// `profiles`: the agent profiles a task may name.
pub fn spec(profiles: &[crate::profiles::Profile]) -> Value {
    let agents = if profiles.is_empty() {
        String::new()
    } else {
        let listed: Vec<String> = profiles.iter().take(crate::profiles::LISTED).map(|p| format!("{}: {}", p.name, p.description.trim())).collect();
        format!(" agent names one of these profiles instead of kind (its instructions and tools, maybe its model): {}.", listed.join("; "))
    };
    let fresh = if forks() { ", starting fresh (their prompts must say everything they need)" } else { "" };
    let copies = format!(
        "general may also edit and write; in a git project, two or more general tasks in one call each get their own copy of the files{fresh}, \
         and their changes come back here when they finish (any that clash are kept aside; the result says where)."
    );
    let description = format!(
        "{} For independent parts worth doing in parallel, or a search through much of the code that would fill your context; \
         not for a quick look. kind explore (default) reads, searches and runs commands, changing no files; {copies}{agents}",
        if forks() { FORK_START } else { FRESH_START }
    );
    json!({
        "type": "function", "name": "task", "strict": false,
        "description": description,
        "parameters": {
            "type": "object",
            "properties": { "tasks": { "type": "array", "maxItems": MAX_TASKS, "items": {
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "A few words, shown to the user" },
                    "prompt": { "type": "string", "description": if forks() { "Which part it does and what to report (everything it needs, for general tasks side by side)" } else { "The whole task, self-contained" } },
                    "kind": { "type": "string", "enum": ["explore", "general"] },
                    "agent": { "type": "string", "description": "A profile's name, instead of kind" },
                    "model": { "type": "string", "description": "provider/model, if not the subagents' default (smaller for simple work, stronger for hard)" },
                    "effort": { "type": "string", "enum": ["off", "minimal", "low", "medium", "high", "xhigh"], "description": "If not the default" },
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
    /// The agent profiles tasks may name.
    pub profiles: Vec<crate::profiles::Profile>,
    /// What subagents run on when a task doesn't say (None: the session's model).
    pub task_model: Option<String>,
    /// The session's token budget, which they share.
    pub budget: Option<Arc<crate::agent::Budget>>,
    /// What they go on with when their model's plan is used up.
    pub fallback: Option<String>,
    /// The session's language servers (shared, not started again).
    pub lsp: Arc<crate::lsp::Servers>,
    /// Subagents as forks: what they start from.
    pub fork: Option<Arc<Fork>>,
}

const FRESH_START: &str = "Runs subagents, at the same time: each starts fresh (its prompt must say everything it needs: \
                           the goal, paths, what to report), works with its own tools, and its final message comes back to you.";
const FORK_START: &str = "Runs subagents, at the same time: each starts from this conversation (its prompt needn't repeat what you know: \
                          say which part it does and what to report), works with its own tools, and its final message comes back to you.";

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
    /// What the main agent's told about where it worked.
    note: String,
    /// It only looked, but nothing kept its commands from changing files
    /// (the sandbox is off).
    unguarded: bool,
}

/// Two or more writers: each in its own copy of the project, or, when one
/// can't be made, in the project itself, one at a time.
struct Writers {
    base: Option<Arc<crate::isolate::Baseline>>,
    /// Why there's no copy for any of them.
    why: Option<String>,
    turn: tokio::sync::Mutex<()>,
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
    let writers = if tasks.iter().filter(|t| writes(t, ctx)).count() >= 2 {
        let cwd = ctx.cwd.clone();
        let taken = tokio::task::spawn_blocking(move || crate::isolate::Baseline::take(&cwd)).await.unwrap_or_else(|e| Err(e.to_string()));
        let (base, why) = match taken {
            Ok(b) => (Some(Arc::new(b)), None),
            Err(why) => (None, Some(why)),
        };
        Some(Writers { base, why, turn: tokio::sync::Mutex::new(()) })
    } else {
        None
    };
    let slots = tokio::sync::Semaphore::new(AT_ONCE);
    let mut done: Vec<Done> = join_all(tasks.iter().map(|t| one(t, call_id, ctx, &slots, stop, writers.as_ref()))).await;
    // Their changes, brought back in the order asked.
    if let Some(base) = writers.as_ref().and_then(|w| w.base.as_ref()) {
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
    report(&done)
}

/// What the helpers came back with, for the main agent: their answers,
/// and their usage added up.
fn report(done: &[Done]) -> Outcome {
    let mut text = String::new();
    // (All of each helper's usage, its cost and the kinds of cost too:
    // the parent's totals count it as its own.)
    let mut total = crate::used::Used::default();
    for d in done {
        let answer = if d.text.trim().is_empty() { "(no answer)" } else { d.text.trim() };
        text.push_str(&format!("## {} ({})\n{answer}{}\n\n", d.description, d.status, d.note));
        total.add(&crate::used::Used::from_details(&d.usage));
    }
    if done.iter().any(|d| d.unguarded) {
        text.push_str("(Harlo's sandbox is off, so the helpers that only looked could have changed files with their commands.)\n\n");
    }
    let progress: Vec<Value> = done
        .iter()
        .map(|d| json!({ "id": d.id, "agent": d.kind, "description": d.description, "assignment": d.prompt, "status": d.status, "usage": d.usage }))
        .collect();
    Outcome {
        text: text.trim_end().to_owned(),
        failed: done.iter().all(|d| d.status != "completed"),
        details: json!({ "progress": progress, "usage": total.details() }),
        images: Vec::new(),
    }
}

async fn one(task: &Value, call_id: &str, ctx: &Ctx, slots: &tokio::sync::Semaphore, stop: &CancellationToken, writers: Option<&Writers>) -> Done {
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
    let mut done = Done { id: id.clone(), description: description.clone(), kind: kind.clone(), prompt: prompt.clone(), status: "aborted", text: String::new(), usage: json!({}), worktree: None, note: String::new(), unguarded: false };
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
    // A writer beside others: in its own worktree; none made, in the
    // project itself, when no other writer is there.
    let mut cwd = ctx.cwd.clone();
    let mut _turn = None;
    if let Some(w) = writers.filter(|_| writer) {
        let made = match &w.base {
            Some(b) => {
                let (b, name) = (b.clone(), id.clone());
                tokio::task::spawn_blocking(move || b.worktree(&name)).await.unwrap_or_else(|e| Err(e.to_string()))
            }
            None => Err(w.why.clone().unwrap_or_default()),
        };
        match made {
            Ok(tree) => {
                cwd = tree.cwd.clone();
                done.worktree = Some(tree);
            }
            Err(why) => {
                _turn = tokio::select! {
                    biased;
                    _ = token.cancelled() => None,
                    t = w.turn.lock() => Some(t),
                };
                if _turn.is_none() {
                    ctx.running.lock().unwrap().remove(&id);
                    done.text = "Stopped before it started.".into();
                    return done;
                }
                done.note = format!("\n\n(No copy of the project could be made for it ({why}), so it worked in the project itself, while no other writer did.)");
            }
        }
    }
    let file = ctx.parent_file.as_ref().map(|p| p.with_extension("").join(format!("{id}.jsonl")));
    let log = match (&file, &ctx.parent_file) {
        (Some(f), Some(p)) => Log::at(f.clone(), &cwd, p),
        _ => Log::ephemeral(&cwd),
    };
    // A fork: the session's model, its requests as they were, the same cache
    // (not in a copy of the files, nor on another model or profile).
    let other_model = task["model"].as_str().is_some_and(|m| !m.is_empty() && m != ctx.model) || ctx.task_model.as_ref().is_some_and(|m| *m != ctx.model);
    let fork = ctx.fork.as_ref().filter(|_| profile.is_none() && !other_model && done.worktree.is_none());
    let (past, model, effort) = match fork {
        // (Its log starts with its own messages: the parent's have ids of their own here.)
        Some(f) => (Past { messages: f.history.clone(), ids: (0..f.history.len()).map(|i| format!("parent-{i}")).collect(), ..Past::default() }, ctx.model.clone(), effort),
        None => (Past::default(), model, effort),
    };
    let mut agent = Agent::new(cwd, log, past, model, effort);
    // (One that only looks: its commands can't change the project.)
    if !writer && !agent.look_only() && allowed.contains(&"bash") {
        done.unguarded = true;
    }
    agent.gate = ctx.gate.clone();
    agent.budget = ctx.budget.clone();
    agent.fallback = ctx.fallback.clone();
    agent.share_lsp(ctx.lsp.clone());
    agent.allowed = Some(allowed);
    if let Some(p) = profile.filter(|p| !p.body.is_empty()) {
        agent.add_instructions(&format!("\n<agent_profile name=\"{}\">\n{}\n</agent_profile>\n", p.name, p.body));
    }
    agent.cache_key = Some(if fork.is_some() { ctx.cache_key.clone() } else { format!("{}/task", ctx.cache_key) });
    let mut asked = prompt.clone();
    if let Some(f) = fork {
        agent.fixed = Some((f.system.clone(), f.specs.clone()));
        asked = format!(
            "<subagent>\nYou're now a subagent started from the conversation above, to do one part of it: {description}. The main agent goes on with the rest. \
             Your tools are {}{} (others are refused). When you're done, your final message goes back to the main agent as your result: say briefly what you found or did.\n\n{prompt}\n</subagent>",
            allowed.join(", "),
            if writer { "" } else { ", and the project is read-only for your commands" },
        );
    }
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
    let ended = agent.run(&asked, &[], &token, &mut show).await;
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
    done.usage = agent.totals.details();
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
    fn helpers_costs_come_back_whole() {
        let done = |usage: crate::used::Used| Done {
            id: "1".into(), description: "look".into(), kind: "explore".into(), prompt: String::new(), status: "completed",
            text: "Done.".into(), usage: usage.details(), worktree: None, note: String::new(), unguarded: false,
        };
        let price = crate::used::Pricing::Priced(crate::used::Price { input: 1.0, output: 1.0, cached: 1.0 });
        let one = |input| crate::used::Used::from_api(&json!({ "input_tokens": input, "output_tokens": 10 }));
        let (priced, unknown) = (one(1_000).priced(&price), one(2_000).priced(&crate::used::Pricing::Unknown));
        let out = report(&[done(priced), done(unknown)]);
        let total = crate::used::Used::from_details(&out.details["usage"]);
        let mut want = priced;
        want.add(&unknown);
        assert_eq!(total, want);
        assert!(total.cost > 0.0 && total.cost_kind() == "partly", "{}", out.details["usage"]);
    }

    #[test]
    fn what_each_kind_may_use() {
        let names = |only: &[&str]| -> Vec<String> {
            crate::tools::specs_of(Some(only), None, false).as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
        };
        let explore = names(EXPLORE);
        assert!(!explore.iter().any(|n| n == "write" || n == "edit" || n == "task"), "{explore:?}");
        assert!(explore.contains(&"bash".to_owned()) && explore.contains(&"read".to_owned()));
        let general = names(GENERAL);
        assert!(general.contains(&"edit".to_owned()) && !general.contains(&"task".to_owned()));
        let all = crate::tools::specs_of(None, Some(&[]), false);
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
            notify: None, known: Default::default(), running: Arc::default(), profiles: Vec::new(), task_model: None, budget: None, fallback: None, lsp: Default::default(), fork: None,
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
