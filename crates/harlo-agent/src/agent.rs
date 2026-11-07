//! The loop: send the history, run the tools the model calls, send their
//! results, until it answers without calling any.
//!
//! The history is pi's messages, each written to the session log as it's
//! made; every request's input is derived from them, so a resumed session
//! sends what a running one would. Tools that only look run at once; the
//! rest one after another, in the order called.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use futures_util::future::join_all;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::session::{Log, now_ms};
use crate::tools::{self, Tools};
use crate::{auth, responses};

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

/// What the user sees of a run.
pub enum Shown<'a> {
    Text(&'a str),
    Reasoning(&'a str),
    ToolStart { name: &'a str, args: &'a Value },
    ToolEnd { text: &'a str, failed: bool },
    Usage { model: &'a str, usage: &'a Value },
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
    /// Totals of this run's requests.
    pub totals: Totals,
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
        Agent { instructions: instructions(&cwd), tools: Tools::new(cwd, spill), model, effort, history, log, logged, totals: Totals::default() }
    }

    pub fn session_file(&self) -> &Path {
        &self.log.path
    }

    fn push(&mut self, message: Value) -> Result<()> {
        self.log.message(&message)?;
        self.history.push(message);
        Ok(())
    }

    /// Works on `prompt` until the model is done, or until `stop` is
    /// cancelled: then what was running ends (commands with everything
    /// they started), and the history says what was cut off.
    pub async fn run(&mut self, prompt: &str, stop: &CancellationToken, show: &mut dyn FnMut(Shown)) -> Result<Ended> {
        if self.logged.0.as_deref() != Some(&self.model) {
            self.log.append(json!({ "type": "model_change", "provider": "openai", "modelId": self.model }))?;
            self.logged.0 = Some(self.model.clone());
        }
        if self.logged.1 != self.effort {
            self.log.append(json!({ "type": "thinking_level_change", "thinkingLevel": self.effort.as_deref().unwrap_or("off") }))?;
            self.logged.1 = self.effort.clone();
        }
        self.push(json!({ "role": "user", "content": [{ "type": "text", "text": prompt }], "timestamp": now_ms() }))?;
        for _ in 0..MAX_ROUNDS {
            // A reply cut off is dropped whole: its parts can't go back alone.
            let asked = tokio::select! {
                biased;
                _ = stop.cancelled() => None,
                r = self.ask(show) => Some(r),
            };
            let Some(reply) = asked else {
                self.stopped()?;
                return Ok(Ended::Stopped);
            };
            let reply = reply?;
            let calls: Vec<Value> = reply["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall").cloned().collect();
            self.push(reply)?;
            if calls.is_empty() {
                return Ok(Ended::Done);
            }
            // Arguments that weren't JSON go as null: the call says so.
            let args: Vec<Value> = calls.iter().map(|c| if c.get("argumentsError").is_some() { Value::Null } else { c["arguments"].clone() }).collect();
            let names: Vec<&str> = calls.iter().map(|c| c["name"].as_str().unwrap_or_default()).collect();
            for (name, args) in names.iter().zip(&args) {
                show(Shown::ToolStart { name, args });
            }
            let pairs: Vec<(&str, &Value)> = names.iter().copied().zip(&args).collect();
            let outcomes = run_calls(&self.tools, &pairs, stop).await;
            for (c, o) in calls.iter().zip(outcomes) {
                show(Shown::ToolEnd { text: &o.text, failed: o.failed });
                self.push(json!({
                    "role": "toolResult", "toolCallId": c["id"], "toolName": c["name"],
                    "content": [{ "type": "text", "text": o.text }], "isError": o.failed, "timestamp": now_ms(),
                }))?;
            }
            if stop.is_cancelled() {
                self.stopped()?;
                return Ok(Ended::Stopped);
            }
        }
        bail!("Stopped after {MAX_ROUNDS} rounds of tool calls")
    }

    /// The note a stopped turn leaves, for the model's next turn (not shown).
    fn stopped(&mut self) -> Result<()> {
        self.push(json!({ "role": "custom", "customType": "turn_stopped", "content": STOPPED_NOTE, "display": false }))
    }

    /// One request: the reply, as pi's assistant message. A token the API
    /// refuses is refreshed once and the request sent again.
    async fn ask(&mut self, show: &mut dyn FnMut(Shown)) -> Result<Value> {
        let input = responses::input(&self.history, &self.model);
        let body = responses::request(&self.model, &self.instructions, &input, tools::specs(), self.effort.as_deref(), &self.log.id);
        let model = self.model.clone();
        let mut force = false;
        loop {
            let token = auth::access_token(force).await?;
            let mut items = Vec::new();
            let mut done: Option<(Value, Vec<Value>)> = None;
            let totals = &mut self.totals;
            let result = responses::stream(&token, &self.log.id, &body, |ev| match ev {
                responses::Event::Text(t) => show(Shown::Text(&t)),
                responses::Event::Reasoning(t) => show(Shown::Reasoning(&t)),
                responses::Event::Item(i) => items.push(i),
                responses::Event::Completed { usage, output } => {
                    totals.requests += 1;
                    totals.input += usage["input_tokens"].as_u64().unwrap_or(0);
                    totals.cached += usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0);
                    totals.output += usage["output_tokens"].as_u64().unwrap_or(0);
                    show(Shown::Usage { model: &model, usage: &usage });
                    done = Some((usage, output));
                }
            })
            .await;
            match result {
                Ok(()) => {
                    let (usage, output) = done.unwrap_or_default();
                    // The completed response's own list when it has one.
                    let items = if output.is_empty() { items } else { output };
                    return Ok(responses::assistant_message(&items, &model, &usage));
                }
                Err(e) if !force && e.downcast_ref::<responses::ApiError>().is_some_and(|a| a.status == 401) => force = true,
                Err(e) => return Err(e),
            }
        }
    }
}

/// Runs one round's calls: those that only look at once, otherwise one
/// after another, in the order called. When `stop` is cancelled, the
/// call running ends (dropping it ends its command) and what's left
/// isn't started.
async fn run_calls(tools: &Tools, calls: &[(&str, &Value)], stop: &CancellationToken) -> Vec<tools::Outcome> {
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
        let out = run_calls(&tools, &[("bash", &first), ("bash", &slow), ("bash", &after)], &stop).await;
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
        let out = run_calls(&tools, &[("ls", &args), ("ls", &args)], &stop).await;
        assert!(out.iter().all(|o| o.failed));
    }
}
