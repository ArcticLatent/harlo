//! The loop: send the history, run the tools the model calls, send their
//! results, until it answers without calling any.
//!
//! The model's output items go back as they came (reasoning with its
//! encrypted content, messages, calls), each call answered by its
//! `function_call_output`. Tools that only look run at once; the rest one
//! after another, in the order called.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use futures_util::future::join_all;
use serde_json::{Value, json};

use crate::tools::{self, Tools};
use crate::{auth, responses};

/// Rounds of tool calls in one run, at most.
const MAX_ROUNDS: usize = 200;

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
    /// The session's id (the prompt cache key).
    id: String,
    /// Totals of this session's requests.
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
    pub fn new(cwd: PathBuf, model: String, effort: Option<String>) -> Self {
        let spill = std::env::temp_dir().join("harlo-agent");
        let mut b = [0u8; 16];
        let _ = getrandom::fill(&mut b);
        let id: String = b.iter().map(|x| format!("{x:02x}")).collect();
        Agent { instructions: instructions(&cwd), tools: Tools::new(cwd, spill), model, effort, history: Vec::new(), id, totals: Totals::default() }
    }

    pub async fn run(&mut self, prompt: &str, show: &mut dyn FnMut(Shown)) -> Result<()> {
        self.history.push(json!({ "role": "user", "content": [{ "type": "input_text", "text": prompt }] }));
        for _ in 0..MAX_ROUNDS {
            let items = self.ask(show).await?;
            let calls: Vec<Value> = items.iter().filter(|i| i["type"] == "function_call").cloned().collect();
            self.history.extend(items);
            if calls.is_empty() {
                return Ok(());
            }
            let parsed: Vec<(String, String, Value)> = calls
                .iter()
                .map(|c| {
                    let args = serde_json::from_str(c["arguments"].as_str().unwrap_or("{}")).unwrap_or(Value::Null);
                    (c["call_id"].as_str().unwrap_or_default().to_owned(), c["name"].as_str().unwrap_or_default().to_owned(), args)
                })
                .collect();
            for (_, name, args) in &parsed {
                show(Shown::ToolStart { name, args });
            }
            let outcomes: Vec<tools::Outcome> = if parsed.iter().all(|(_, n, _)| tools::read_only(n)) {
                join_all(parsed.iter().map(|(_, name, args)| self.tools.call(name, args))).await
            } else {
                let mut out = Vec::new();
                for (_, name, args) in &parsed {
                    out.push(self.tools.call(name, args).await);
                }
                out
            };
            for ((call_id, _, args), o) in parsed.iter().zip(outcomes) {
                let text = if args.is_null() { "The arguments weren't valid JSON; call the tool again.".to_owned() } else { o.text };
                show(Shown::ToolEnd { text: &text, failed: o.failed });
                self.history.push(json!({ "type": "function_call_output", "call_id": call_id, "output": text }));
            }
        }
        bail!("Stopped after {MAX_ROUNDS} rounds of tool calls")
    }

    /// One request: its output items. A token the API refuses is refreshed
    /// once and the request sent again.
    async fn ask(&mut self, show: &mut dyn FnMut(Shown)) -> Result<Vec<Value>> {
        let body = responses::request(&self.model, &self.instructions, &self.history, tools::specs(), self.effort.as_deref(), &self.id);
        let model = self.model.clone();
        let mut force = false;
        loop {
            let token = auth::access_token(force).await?;
            let mut items = Vec::new();
            let totals = &mut self.totals;
            let result = responses::stream(&token, &body, |ev| match ev {
                responses::Event::Text(t) => show(Shown::Text(&t)),
                responses::Event::Reasoning(t) => show(Shown::Reasoning(&t)),
                responses::Event::Item(i) => items.push(i),
                responses::Event::Completed { usage } => {
                    totals.requests += 1;
                    totals.input += usage["input_tokens"].as_u64().unwrap_or(0);
                    totals.cached += usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0);
                    totals.output += usage["output_tokens"].as_u64().unwrap_or(0);
                    show(Shown::Usage { model: &model, usage: &usage })
                }
            })
            .await;
            match result {
                Ok(()) => return Ok(items),
                Err(e) if !force && e.downcast_ref::<responses::ApiError>().is_some_and(|a| a.status == 401) => force = true,
                Err(e) => return Err(e),
            }
        }
    }
}

/// The system prompt: short; the project's own instructions after it.
fn instructions(cwd: &Path) -> String {
    let mut s = String::from(
        "You are harlo-agent, a coding agent. You work in the user's project with your tools, and finish the task you're given.\n\
         \n\
         - Look before you change: read a file before editing it, and search (grep, find) rather than guess paths.\n\
         - Change files with edit (small, exact replacements) or write (new files). Never edit files through bash.\n\
         - Run commands with bash to build, test and check your work. Commands can't take input.\n\
         - When a tool fails, read its message: it says what to do next.\n\
         - Keep going until the task is done, then reply with a short summary of what you changed and anything left open. Be concise.\n",
    );
    for (path, text) in project_instructions(cwd) {
        s.push_str(&format!("\n<project_instructions path=\"{}\">\n{}\n</project_instructions>\n", path.display(), text.trim()));
    }
    s.push_str(&format!("\nWorking directory: {}\n", cwd.display()));
    s
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
