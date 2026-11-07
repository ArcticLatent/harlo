//! The loop: send the history, run the tools the model calls, send their
//! results, until it answers without calling any.
//!
//! The history is pi's messages, each written to the session log as it's
//! made; every request's input is derived from them, so a resumed session
//! sends what a running one would. Tools that only look run at once; the
//! rest one after another, in the order called.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use futures_util::future::{BoxFuture, join_all};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::session::{Log, now_ms};
use crate::tools::{self, Tools};
use crate::providers::{self, Api};
use crate::{chat, responses};

/// Rounds of tool calls in one run, at most.
const MAX_ROUNDS: usize = 200;

/// Left in the history when the user stops a turn.
const STOPPED_NOTE: &str = "<turn_stopped>The user stopped this turn. A call marked interrupted was stopped partway and may have partly run: check its effects before relying on them.</turn_stopped>";
const INTERRUPTED: &str = "Interrupted: the user stopped the turn while this call ran. It may have partly run.";
const NOT_RUN: &str = "Not run: the user stopped the turn before this call started.";

/// How a run ended.
#[derive(Debug, PartialEq)]
pub enum Ended {
    Done,
    /// The user stopped it.
    Stopped,
}

/// What a run shows, as it happens (pi's events, roughly).
pub enum Shown<'a> {
    MessageStart(&'a Value),
    /// A block of the reply streaming: pi's `assistantMessageEvent`
    /// (`text_delta`, `thinking_start`, `toolcall_end`, …).
    Stream(&'a Value),
    /// A message, complete (the reply's final form).
    MessageEnd(&'a Value),
    ToolStart { call_id: &'a str, name: &'a str, args: &'a Value },
    ToolEnd { call_id: &'a str, name: &'a str, text: &'a str, failed: bool },
    Usage { model: &'a str, usage: &'a Value },
    /// The messages waiting to be delivered changed.
    Queue { steering: &'a [String], follow_up: &'a [String] },
}

/// Decides whether a call that changes something may run: `Err` says why
/// not (the call's result).
pub trait Gate: Send + Sync {
    fn check(&self, name: &str, args: &Value) -> BoxFuture<'static, Result<(), String>>;
}

/// Messages sent while a run works: steering goes in at the next step,
/// follow-ups when it would stop (one at a time).
#[derive(Default)]
pub struct Inbox {
    pub steering: Vec<String>,
    pub follow_up: Vec<String>,
}

pub struct Agent {
    pub model: String,
    pub effort: Option<String>,
    instructions: String,
    tools: Tools,
    history: Vec<Value>,
    log: Log,
    /// The model and thinking level the log last recorded.
    logged: (Option<String>, Option<String>),
    /// Totals of this process's requests.
    pub totals: Totals,
    /// The last request's usage (the context's size).
    pub last_usage: Value,
    pub gate: Option<Arc<dyn Gate>>,
    pub inbox: Arc<Mutex<Inbox>>,
}

#[derive(Default)]
pub struct Totals {
    pub requests: u64,
    pub input: u64,
    pub cached: u64,
    pub output: u64,
}

impl Agent {
    /// `history`, `logged`: what the log already holds (nothing for a new
    /// session).
    pub fn new(cwd: PathBuf, log: Log, history: Vec<Value>, logged: (Option<String>, Option<String>), model: String, effort: Option<String>) -> Self {
        let spill = std::env::temp_dir().join("harlo-agent");
        Agent {
            instructions: instructions(&cwd),
            tools: Tools::new(cwd, spill),
            model,
            effort,
            history,
            log,
            logged,
            totals: Totals::default(),
            last_usage: Value::Null,
            gate: None,
            inbox: Arc::default(),
        }
    }

    pub fn session_file(&self) -> &Path {
        &self.log.path
    }

    pub fn session_id(&self) -> &str {
        &self.log.id
    }

    pub fn messages(&self) -> &[Value] {
        &self.history
    }

    /// Starts over in a new session (same folder, model and level).
    pub fn new_session(&mut self, log: Log) {
        self.log = log;
        self.history.clear();
        self.logged = (None, None);
    }

    /// Goes on in another session (a fork): its log and history.
    pub fn switch(&mut self, loaded: crate::session::Loaded) {
        self.log = loaded.log;
        self.history = loaded.messages;
        self.logged = (loaded.model, loaded.effort);
    }

    pub fn rename(&mut self, name: &str) -> Result<()> {
        self.log.append(json!({ "type": "session_info", "name": name }))
    }

    fn push(&mut self, message: Value, show: &mut (dyn FnMut(Shown) + Send)) -> Result<()> {
        self.log.message(&message)?;
        if message["role"] != "assistant" {
            show(Shown::MessageStart(&message));
        }
        show(Shown::MessageEnd(&message));
        self.history.push(message);
        Ok(())
    }

    fn push_user(&mut self, text: &str, show: &mut (dyn FnMut(Shown) + Send)) -> Result<()> {
        self.push(json!({ "role": "user", "content": [{ "type": "text", "text": text }], "timestamp": now_ms() }), show)
    }

    /// Delivers the steering messages waiting (true if there were any).
    fn take_steering(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<bool> {
        let texts: Vec<String> = std::mem::take(&mut self.inbox.lock().unwrap().steering);
        if texts.is_empty() {
            return Ok(false);
        }
        self.show_queue(show);
        for t in texts {
            self.push_user(&t, show)?;
        }
        Ok(true)
    }

    fn take_follow_up(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<bool> {
        let next = {
            let mut inbox = self.inbox.lock().unwrap();
            (!inbox.follow_up.is_empty()).then(|| inbox.follow_up.remove(0))
        };
        let Some(t) = next else { return Ok(false) };
        self.show_queue(show);
        self.push_user(&t, show)?;
        Ok(true)
    }

    fn show_queue(&self, show: &mut (dyn FnMut(Shown) + Send)) {
        let inbox = self.inbox.lock().unwrap();
        show(Shown::Queue { steering: &inbox.steering, follow_up: &inbox.follow_up });
    }

    /// Works on `prompt` until the model is done, or until `stop` is
    /// cancelled: then what was running ends (commands with everything
    /// they started), and the history says what was cut off.
    pub async fn run(&mut self, prompt: &str, stop: &CancellationToken, show: &mut (dyn FnMut(Shown) + Send)) -> Result<Ended> {
        if self.logged.0.as_deref() != Some(&self.model) {
            let (provider, id) = self.model.split_once('/').unwrap_or((providers::CHATGPT, &self.model));
            self.log.append(json!({ "type": "model_change", "provider": provider, "modelId": id }))?;
            self.logged.0 = Some(self.model.clone());
        }
        if self.logged.1 != self.effort {
            self.log.append(json!({ "type": "thinking_level_change", "thinkingLevel": self.effort.as_deref().unwrap_or("off") }))?;
            self.logged.1 = self.effort.clone();
        }
        self.push_user(prompt, show)?;
        for round in 0..MAX_ROUNDS {
            if round > 0 {
                self.take_steering(show)?;
            }
            // A reply cut off is dropped whole: its parts can't go back alone.
            let asked = tokio::select! {
                biased;
                _ = stop.cancelled() => None,
                r = self.ask(show) => Some(r),
            };
            let Some(reply) = asked else {
                self.stopped(show)?;
                return Ok(Ended::Stopped);
            };
            let reply = reply?;
            let calls: Vec<Value> = reply["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall").cloned().collect();
            self.push(reply, show)?;
            if calls.is_empty() {
                if self.take_steering(show)? || self.take_follow_up(show)? {
                    continue;
                }
                return Ok(Ended::Done);
            }
            // Arguments that weren't JSON go as null: the call says so.
            let args: Vec<Value> = calls.iter().map(|c| if c.get("argumentsError").is_some() { Value::Null } else { c["arguments"].clone() }).collect();
            let names: Vec<&str> = calls.iter().map(|c| c["name"].as_str().unwrap_or_default()).collect();
            let ids: Vec<&str> = calls.iter().map(|c| c["id"].as_str().unwrap_or_default()).collect();
            for ((id, name), args) in ids.iter().zip(&names).zip(&args) {
                show(Shown::ToolStart { call_id: id, name, args });
            }
            let pairs: Vec<(&str, &Value)> = names.iter().copied().zip(&args).collect();
            let outcomes = run_calls(&self.tools, &pairs, self.gate.as_deref(), stop).await;
            for ((c, name), o) in calls.iter().zip(&names).zip(outcomes) {
                show(Shown::ToolEnd { call_id: c["id"].as_str().unwrap_or_default(), name, text: &o.text, failed: o.failed });
                self.push(
                    json!({
                        "role": "toolResult", "toolCallId": c["id"], "toolName": c["name"],
                        "content": [{ "type": "text", "text": o.text }], "isError": o.failed, "timestamp": now_ms(),
                    }),
                    show,
                )?;
            }
            if stop.is_cancelled() {
                self.stopped(show)?;
                return Ok(Ended::Stopped);
            }
        }
        bail!("Stopped after {MAX_ROUNDS} rounds of tool calls")
    }

    /// The note a stopped turn leaves, for the model's next turn (not shown).
    fn stopped(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<()> {
        self.push(json!({ "role": "custom", "customType": "turn_stopped", "content": STOPPED_NOTE, "display": false }), show)
    }

    /// Records a failed request as pi does: an assistant message with the
    /// error (it adds nothing to what the model sees next).
    pub fn failed(&mut self, error: &str, show: &mut (dyn FnMut(Shown) + Send)) -> Result<()> {
        let (provider, id) = self.model.split_once('/').unwrap_or((providers::CHATGPT, &self.model));
        let mut m = responses::assistant_message(&[], provider, id, &Value::Null);
        m["stopReason"] = json!("error");
        m["errorMessage"] = json!(error);
        show(Shown::MessageStart(&m));
        self.push(m, show)
    }

    /// One request: the reply, as pi's assistant message. A token the API
    /// refuses is refreshed once and the request sent again.
    async fn ask(&mut self, show: &mut (dyn FnMut(Shown) + Send)) -> Result<Value> {
        let (provider, model) = providers::resolve(&self.model)?;
        let pid = provider.id.clone();
        let api = if provider.api == Api::Chat { "openai-completions" } else { "openai-responses" };
        show(Shown::MessageStart(&json!({ "role": "assistant", "content": [], "api": api, "provider": pid, "model": model, "timestamp": now_ms() })));
        let effort = provider.effort(&model, self.effort.as_deref());
        let totals = &mut self.totals;
        let last_usage = &mut self.last_usage;
        let mut counted = |usage: &Value, show: &mut (dyn FnMut(Shown) + Send)| {
            totals.requests += 1;
            totals.input += usage["input_tokens"].as_u64().unwrap_or(0);
            totals.cached += usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0);
            totals.output += usage["output_tokens"].as_u64().unwrap_or(0);
            show(Shown::Usage { model: &model, usage });
            *last_usage = usage.clone();
        };
        if provider.api == Api::Chat {
            let body = chat::request(&model, &self.instructions, &chat::messages(&self.history), &tools::specs(), effort.as_deref());
            let token = provider.token(false).await?;
            let (message, usage) = chat::stream(&provider.base_url, token.as_deref(), &body, &pid, &model, |e| show(Shown::Stream(e))).await?;
            counted(&usage, show);
            return Ok(message);
        }
        let input = responses::input(&self.history, &pid, &model);
        let body = responses::request(&model, &self.instructions, &input, tools::specs(), effort.as_deref(), &self.log.id);
        // (The plan's affinity headers keep a session's requests where its cache is.)
        let affinity = provider.is_plan().then_some(self.log.id.as_str());
        let mut force = false;
        loop {
            let token = provider.token(force).await?;
            let mut items = Vec::new();
            let mut done: Option<(Value, Vec<Value>)> = None;
            let mut blocks = Blocks::default();
            let result = responses::stream(&provider.base_url, token.as_deref(), affinity, &body, |ev| match ev {
                responses::Event::Item { index, item } => {
                    if let Some(end) = blocks.end(index, &item, &pid, &model) {
                        show(Shown::Stream(&end));
                    }
                    items.push(item);
                }
                responses::Event::Completed { usage, output } => {
                    counted(&usage, show);
                    done = Some((usage, output));
                }
                other => {
                    for e in blocks.stream(other) {
                        show(Shown::Stream(&e));
                    }
                }
            })
            .await;
            match result {
                Ok(()) => {
                    let (usage, output) = done.unwrap_or_default();
                    // The completed response's own list when it has one.
                    let items = if output.is_empty() { items } else { output };
                    return Ok(responses::assistant_message(&items, &pid, &model, &usage));
                }
                // The plan's token, refused: refreshed once.
                Err(e) if provider.is_plan() && !force && e.downcast_ref::<responses::ApiError>().is_some_and(|a| a.status == 401) => force = true,
                Err(e) => return Err(e),
            }
        }
    }
}

/// The reply's blocks as they stream: each output item is a block of the
/// assistant message (as `assistant_message` makes them, in order).
#[derive(Default)]
struct Blocks {
    /// Output index → (block index, summary part last seen).
    at: std::collections::HashMap<usize, (usize, usize)>,
    next: usize,
}

impl Blocks {
    fn stream(&mut self, ev: responses::Event) -> Vec<Value> {
        use responses::Event as E;
        match ev {
            E::Added { index, item } => {
                let ci = self.next;
                self.next += 1;
                self.at.insert(index, (ci, 0));
                match item["type"].as_str() {
                    Some("reasoning") => vec![json!({ "type": "thinking_start", "contentIndex": ci })],
                    Some("message") => vec![json!({ "type": "text_start", "contentIndex": ci })],
                    Some("function_call") => vec![json!({
                        "type": "toolcall_start", "contentIndex": ci,
                        "id": format!("{}|{}", item["call_id"].as_str().unwrap_or_default(), item["id"].as_str().unwrap_or_default()),
                        "toolName": item["name"],
                    })],
                    _ => {
                        self.next -= 1;
                        self.at.remove(&index);
                        Vec::new()
                    }
                }
            }
            E::Text { index, delta } => self.at.get(&index).map(|(ci, _)| vec![json!({ "type": "text_delta", "contentIndex": ci, "delta": delta })]).unwrap_or_default(),
            E::Args { index, delta } => self.at.get(&index).map(|(ci, _)| vec![json!({ "type": "toolcall_delta", "contentIndex": ci, "delta": delta })]).unwrap_or_default(),
            E::Reasoning { index, part, delta } => {
                let Some((ci, last)) = self.at.get_mut(&index) else { return Vec::new() };
                // Summary parts are joined by a blank line.
                let sep = if part != *last { "\n\n" } else { "" };
                *last = part;
                vec![json!({ "type": "thinking_delta", "contentIndex": *ci, "delta": format!("{sep}{delta}") })]
            }
            E::Item { .. } | E::Completed { .. } => Vec::new(),
        }
    }

    /// An item done: its block's end, with the block as it will be kept.
    fn end(&mut self, index: usize, item: &Value, provider: &str, model: &str) -> Option<Value> {
        let (ci, _) = *self.at.get(&index)?;
        let block = responses::assistant_message(std::slice::from_ref(item), provider, model, &Value::Null)["content"][0].clone();
        Some(match block["type"].as_str()? {
            "thinking" => json!({ "type": "thinking_end", "contentIndex": ci, "content": block["thinking"] }),
            "text" => json!({ "type": "text_end", "contentIndex": ci, "content": block["text"] }),
            "toolCall" => json!({ "type": "toolcall_end", "contentIndex": ci, "toolCall": block }),
            _ => return None,
        })
    }
}

/// Runs one round's calls: those that only look at once, otherwise one
/// after another, in the order called. When `stop` is cancelled, the
/// call running ends (dropping it ends its command) and what's left
/// isn't started.
async fn run_calls(tools: &Tools, calls: &[(&str, &Value)], gate: Option<&dyn Gate>, stop: &CancellationToken) -> Vec<tools::Outcome> {
    let bad_args = || tools::Outcome { text: "The arguments weren't valid JSON; call the tool again.".into(), failed: true };
    let cut = |text: &str| tools::Outcome { text: text.into(), failed: true };
    if calls.iter().all(|(n, _)| tools::read_only(n)) {
        let all = join_all(calls.iter().map(|(name, args)| async move { if args.is_null() { bad_args() } else { tools.call(name, args).await } }));
        return tokio::select! {
            biased;
            _ = stop.cancelled() => calls.iter().map(|_| cut(INTERRUPTED)).collect(),
            o = all => o,
        };
    }
    let mut out = Vec::new();
    for (name, args) in calls {
        if stop.is_cancelled() {
            out.push(cut(NOT_RUN));
            continue;
        }
        if args.is_null() {
            out.push(bad_args());
            continue;
        }
        // What changes something may need the user's yes first.
        if let Some(gate) = gate.filter(|_| !tools::read_only(name)) {
            let verdict = tokio::select! {
                biased;
                _ = stop.cancelled() => Err(NOT_RUN.to_owned()),
                v = gate.check(name, args) => v,
            };
            if let Err(why) = verdict {
                out.push(cut(&why));
                continue;
            }
        }
        let ran = tokio::select! {
            biased;
            _ = stop.cancelled() => None,
            o = tools.call(name, args) => Some(o),
        };
        out.push(ran.unwrap_or_else(|| cut(INTERRUPTED)));
    }
    out
}

/// The system prompt: short; the project's own instructions after it.
fn instructions(cwd: &Path) -> String {
    let mut s = String::from(
        "You are harlo-agent, a coding agent. You work in the user's project with your tools, and finish the task you're given.\n\
         \n\
         - Look before you change: read a file before editing it. When the task names a file, read it; search (grep, find) only when you don't know where something is.\n\
         - Change files with edit (small, exact replacements) or write (new files). Never edit files through bash.\n\
         - Run commands with bash to build, test and check your work. Commands can't take input.\n\
         - When a tool fails, read its message: it says what to do next.\n\
         - Keep going until the task is done, then reply with a short summary of what you changed and anything left open. Be concise.\n",
    );
    for (path, text) in project_instructions(cwd) {
        s.push_str(&format!("\n<project_instructions path=\"{}\">\n{}\n</project_instructions>\n", path.display(), text.trim()));
    }
    s.push_str(&format!("\nWorking directory: {}\n", cwd.display()));
    if let Some(listing) = listing(cwd) {
        s.push_str(&format!("\nIts files when the session started:\n{listing}\n"));
    }
    s
}

/// Entries of the working directory shown in the instructions, at most.
const LISTED: usize = 50;

/// The working directory's entries (folders end in /), sorted, without
/// .git; a note says how many more there are.
fn listing(cwd: &Path) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(cwd)
        .ok()?
        .flatten()
        .filter(|e| e.file_name() != ".git")
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.file_type().is_ok_and(|t| t.is_dir()) { format!("{name}/") } else { name }
        })
        .collect();
    if names.is_empty() {
        return Some("(empty)".into());
    }
    names.sort();
    let more = names.len().saturating_sub(LISTED);
    names.truncate(LISTED);
    let mut out = names.join("\n");
    if more > 0 {
        out.push_str(&format!("\n… and {more} more (use ls)"));
    }
    Some(out)
}

/// AGENTS.md (or CLAUDE.md) in the working directory and the folders
/// above it, outermost first.
fn project_instructions(cwd: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for dir in cwd.ancestors() {
        for name in ["AGENTS.md", "CLAUDE.md"] {
            let p = dir.join(name);
            if let Ok(text) = std::fs::read_to_string(&p) {
                found.push((p, text));
                break;
            }
        }
    }
    found.reverse();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn stopping_ends_the_running_call_and_skips_the_rest() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tools = Tools::new(dir.clone(), dir.join("spill"));
        let marker = dir.join("late");
        let first = json!({ "command": "echo first" });
        let slow = json!({ "command": format!("sleep 30; touch {}", marker.display()) });
        let after = json!({ "command": format!("touch {}", marker.display()) });
        let stop = CancellationToken::new();
        let s2 = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            s2.cancel();
        });
        let t = Instant::now();
        let out = run_calls(&tools, &[("bash", &first), ("bash", &slow), ("bash", &after)], None, &stop).await;
        assert!(t.elapsed() < Duration::from_secs(5), "the stop didn't end the command");
        assert_eq!(out[0].text, "first");
        assert_eq!(out[1].text, INTERRUPTED);
        assert_eq!(out[2].text, NOT_RUN);
        assert!(!marker.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_instructions_list_the_folder() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-list-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("b.py"), "").unwrap();
        std::fs::write(dir.join("a.py"), "").unwrap();
        assert_eq!(listing(&dir).as_deref(), Some("a.py\nb.py\nsrc/"));
        for i in 0..60 {
            std::fs::write(dir.join(format!("f{i:02}")), "").unwrap();
        }
        let l = listing(&dir).unwrap();
        assert_eq!(l.lines().count(), LISTED + 1);
        assert!(l.ends_with("… and 13 more (use ls)"), "{l}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn stopping_reads_interrupts_them_all() {
        let dir = std::env::temp_dir();
        let tools = Tools::new(dir.clone(), dir.join("harlo-agent"));
        let stop = CancellationToken::new();
        stop.cancel();
        let args = json!({ "path": "." });
        let out = run_calls(&tools, &[("ls", &args), ("ls", &args)], None, &stop).await;
        assert!(out.iter().all(|o| o.failed));
    }
}
