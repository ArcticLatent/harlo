//! Code Mode: the agent writes JavaScript that calls Harlo's tools as
//! functions (`await tools.<namespace>.<tool>({...})`), so several calls,
//! loops and filtering take one step instead of many.
//!
//! The script runs in `QuickJS` on a thread of its own, with a memory cap and
//! a time limit. Each tool call goes back to the async side, where the
//! [`Catalog`] runs it; the script waits for the answer. What it returns is
//! the result, followed by what it logged.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rquickjs::{CatchResultExt as _, Context, Function, Promise, Runtime};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The most a script's result and logs may hold, in bytes.
pub const OUTPUT_CAP: usize = 32 * 1024;

/// The most memory a script may use.
const MEMORY: usize = 64 * 1024 * 1024;

/// A namespace with more tools than this lists them by name only.
///
/// The list goes out with every request, so a server of a hundred tools
/// would cost a hundred signatures and descriptions each time, for a task
/// that calls two of them. Ten covers the usual server in full; beyond it
/// a script asks `search` for the signature of the tool it wants.
const LISTED: usize = 10;

/// One tool a namespace offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolInfo {
    /// Its name within the namespace.
    pub name: String,
    /// What it does.
    pub description: String,
    /// Its argument and result, as a TypeScript-like signature.
    pub signature: String,
}

/// A group of tools scripts can call, such as `automation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    /// Its name, as scripts write it.
    pub name: String,
    /// What its tools are for.
    pub description: String,
    /// Its tools.
    pub tools: Vec<ToolInfo>,
}

/// A tool call a script made, and where it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Call {
    /// `<namespace>.<tool>`.
    pub tool: String,
    /// `running`, `completed` or `error`.
    pub status: String,
}

/// A tool offered to the model on its own, outside scripts.
#[derive(Debug, Clone, PartialEq)]
pub struct Direct {
    /// The name the model calls it by.
    pub name: String,
    /// What it does.
    pub description: String,
    /// Its input's JSON Schema.
    pub parameters: Value,
    /// The namespace [`Catalog::call`] runs it in.
    pub namespace: String,
    /// Its name there.
    pub tool: String,
}

impl Direct {
    /// Its definition, as the model sees it.
    #[must_use]
    pub fn definition(&self) -> Value {
        json!({
            "name": self.name, "description": self.description, "parameters": self.parameters,
        })
    }
}

/// A tool call's answer, on its way.
pub type Answer<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;

/// A tool call's answer with the pictures it came with, on its way.
pub type Seen<'a> = Pin<
    Box<dyn Future<Output = Result<(Value, Vec<crate::attachment::File>), String>> + Send + 'a>,
>;

/// The tools scripts can call, and where their progress goes.
pub trait Catalog: Send + Sync + std::fmt::Debug {
    /// The namespaces on offer.
    fn namespaces(&self) -> Vec<Namespace>;

    /// The tools offered to the model on their own, beside `execute`.
    fn direct(&self) -> Vec<Direct> {
        Vec::new()
    }

    /// Runs `tool` of `namespace` with `args`.
    fn call<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Answer<'a>;

    /// Runs `tool` of `namespace` with `args` for a call the model made on
    /// its own, and returns the pictures it answered with too. A script's
    /// calls get values only.
    fn call_seen<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Seen<'a> {
        Box::pin(async move { Ok((self.call(namespace, tool, args).await?, Vec::new())) })
    }

    /// Tells whoever follows tool call `call` of step `message` the
    /// script's calls so far.
    fn progress(&self, _message: &str, _call: &str, _calls: &[Call]) {}
}

/// The `execute` tool, offering `namespaces`.
#[must_use]
pub fn definition(namespaces: &[Namespace]) -> Value {
    json!({
        "name": "execute",
        "description": format!(
            "Runs JavaScript that calls tools as functions, for work that takes several calls, \
             a loop or filtering: `const r = await tools.<namespace>.<tool>({{...}});`. The code \
             is the body of an async function: `return` what you want back, and console.log \
             shows up after it. `search(\"words\")` finds tools, with the signature of each \
             and the `path` to call it by. These tools only work inside execute:\n{}",
            describe(namespaces)
        ),
        "parameters": { "type": "object", "properties": {
            "code": { "type": "string", "description": "The function body to run." }
        }, "required": ["code"] }
    })
}

/// How a script ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Ran {
    /// What it returned (`None` for nothing), or why it failed.
    pub value: Result<Option<Value>, String>,
    /// What it logged.
    pub logs: Vec<String>,
    /// The tool calls it made.
    pub calls: Vec<Call>,
}

impl Ran {
    /// The result and logs as text for the model, cut to [`OUTPUT_CAP`];
    /// and whether some was cut.
    #[must_use]
    pub fn text(&self) -> (String, bool) {
        let mut text = match &self.value {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => "Done, with no result.".to_owned(),
            Err(why) => format!("The script failed: {why}"),
        };
        if !self.logs.is_empty() {
            let _ = write!(text, "\n\nLogs:\n{}", self.logs.join("\n"));
        }
        if text.len() <= OUTPUT_CAP {
            return (text, false);
        }
        let mut end = OUTPUT_CAP;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n… (cut at 32 KB)");
        (text, true)
    }
}

/// A call on its way from the script to the catalog.
struct Request {
    namespace: String,
    tool: String,
    args: Value,
    reply: std::sync::mpsc::Sender<Result<Value, String>>,
}

/// What the script's thread sends back.
enum FromScript {
    Call(Request),
    Ended(Result<Option<Value>, String>, Vec<String>),
}

/// The functions every script starts with: `tools`, `search` and
/// `console`.
const PRELUDE: &str = r#"
const __text = (x) => typeof x === "string" ? x : JSON.stringify(x);
globalThis.console = Object.fromEntries(["log", "info", "warn", "error", "debug"].map(
  (k) => [k, (...a) => __harlo_log(a.map(__text).join(" "))]));
const __call = (ns, tool, args) => {
  const r = JSON.parse(__harlo_call(ns, tool, JSON.stringify(args === undefined ? {} : args)));
  if ("error" in r) throw new Error(r.error);
  return r.ok;
};
globalThis.tools = new Proxy({}, { get: (_, ns) => new Proxy({}, { get: (_, tool) => (args) => {
  try { return Promise.resolve(__call(String(ns), String(tool), args)); }
  catch (e) { return Promise.reject(e); }
} }) });
globalThis.search = (q) => JSON.parse(__harlo_search(JSON.stringify(q === undefined ? "" : q)));
"#;

/// What `search(query)` finds in `namespaces`: `{items, remaining, next}`.
fn search(namespaces: &[Namespace], query: &Value) -> Value {
    let (words, namespace, limit, offset) = match query {
        Value::String(q) => (q.clone(), None, 10, 0),
        q => (
            q["query"].as_str().unwrap_or_default().to_owned(),
            q["namespace"].as_str().map(str::to_owned),
            q["limit"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(10),
            q["offset"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0),
        ),
    };
    let words: Vec<String> = words
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let mut found: Vec<(usize, Value)> = namespaces
        .iter()
        .filter(|ns| namespace.as_deref().is_none_or(|n| n == ns.name))
        .flat_map(|ns| {
            let words = &words;
            ns.tools.iter().map(move |t| {
                let hay = format!("{} {} {}", ns.name, t.name, t.description).to_lowercase();
                let score = words.iter().filter(|w| hay.contains(w.as_str())).count();
                (
                    score,
                    json!({
                        "path": format!("tools.{}.{}", ns.name, t.name),
                        "description": t.description, "signature": t.signature,
                    }),
                )
            })
        })
        .filter(|(score, _)| words.is_empty() || *score > 0)
        .collect();
    found.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    let total = found.len();
    let items: Vec<Value> = found
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(_, v)| v)
        .collect();
    let shown = offset + items.len();
    json!({
        "items": items,
        "remaining": total.saturating_sub(shown),
        "next": (shown < total).then(|| json!({ "offset": shown })),
    })
}

/// The namespaces, as a list for the model.
///
/// A namespace of more than [`LISTED`] tools names them without their
/// signatures and descriptions, and says how to look one up.
#[must_use]
pub fn describe(namespaces: &[Namespace]) -> String {
    let mut out = String::new();
    for ns in namespaces {
        if ns.tools.len() > LISTED {
            let names: Vec<&str> = ns.tools.iter().map(|t| t.name.as_str()).collect();
            let _ = writeln!(
                out,
                "- tools.{}: {} Its {} tools, by name;                  search({{namespace: \"{}\", query: \"words\"}}) shows how to call one: {}",
                ns.name,
                ns.description,
                names.len(),
                ns.name,
                names.join(", ")
            );
            continue;
        }
        let _ = writeln!(out, "- tools.{}: {}", ns.name, ns.description);
        for t in &ns.tools {
            let _ = writeln!(
                out,
                "  - tools.{}.{}{} — {}",
                ns.name, t.name, t.signature, t.description
            );
        }
    }
    out
}

/// Runs the script on its own thread; sends its calls and its end to
/// `out`.
fn script_thread(
    code: &str,
    input: &Value,
    namespaces: Vec<Namespace>,
    limit: Duration,
    halted: &Arc<AtomicBool>,
    out: &mpsc::UnboundedSender<FromScript>,
) {
    let logs = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let result = (|| -> Result<Option<Value>, String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        runtime.set_memory_limit(MEMORY);
        runtime.set_max_stack_size(1024 * 1024);
        let deadline = Instant::now() + limit;
        let stop = Arc::clone(halted);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            Instant::now() > deadline || stop.load(Ordering::Relaxed)
        })));
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;
        context.with(|ctx| {
            let globals = ctx.globals();
            let sink = out.clone();
            let call = Function::new(
                ctx.clone(),
                move |namespace: String, tool: String, args: String| -> String {
                    let (reply, answer) = std::sync::mpsc::channel();
                    let args = serde_json::from_str(&args).unwrap_or(Value::Null);
                    let sent = sink.send(FromScript::Call(Request {
                        namespace,
                        tool,
                        args,
                        reply,
                    }));
                    let got = match sent {
                        Ok(()) => answer
                            .recv()
                            .unwrap_or_else(|_| Err("The run was stopped.".to_owned())),
                        Err(_) => Err("The run was stopped.".to_owned()),
                    };
                    match got {
                        Ok(v) => json!({ "ok": v }).to_string(),
                        Err(e) => json!({ "error": e }).to_string(),
                    }
                },
            )
            .map_err(|e| e.to_string())?;
            globals
                .set("__harlo_call", call)
                .map_err(|e| e.to_string())?;
            let kept = Arc::clone(&logs);
            let log = Function::new(ctx.clone(), move |line: String| {
                if let Ok(mut logs) = kept.lock() {
                    logs.push(line);
                }
            })
            .map_err(|e| e.to_string())?;
            globals.set("__harlo_log", log).map_err(|e| e.to_string())?;
            let find = Function::new(ctx.clone(), move |query: String| -> String {
                let query = serde_json::from_str(&query).unwrap_or(Value::Null);
                search(&namespaces, &query).to_string()
            })
            .map_err(|e| e.to_string())?;
            globals
                .set("__harlo_search", find)
                .map_err(|e| e.to_string())?;
            let given = ctx
                .json_parse(input.to_string())
                .map_err(|e| e.to_string())?;
            globals
                .set("input", given.clone())
                .map_err(|e| e.to_string())?;
            for key in ["state", "now", "runs", "args", "event"] {
                if let Some(obj) = given.as_object()
                    && let Ok(v) = obj.get::<_, rquickjs::Value>(key)
                {
                    globals.set(key, v).map_err(|e| e.to_string())?;
                }
            }
            ctx.eval::<(), _>(PRELUDE).map_err(|e| e.to_string())?;
            let wrapped = format!("(async () => {{\n{code}\n}})()");
            let promise: Promise = ctx.eval(wrapped).catch(&ctx).map_err(|e| e.to_string())?;
            let value: rquickjs::Value = promise.finish().catch(&ctx).map_err(|e| {
                if halted.load(Ordering::Relaxed) || Instant::now() > deadline {
                    "it was stopped, or ran out of time.".to_owned()
                } else {
                    e.to_string()
                }
            })?;
            if value.is_undefined() {
                return Ok(None);
            }
            let text = ctx
                .json_stringify(value)
                .map_err(|e| e.to_string())?
                .map(|s| s.to_string())
                .transpose()
                .map_err(|e| e.to_string())?;
            Ok(text.and_then(|t| serde_json::from_str(&t).ok()))
        })
    })();
    let logs = logs.lock().map(|l| l.clone()).unwrap_or_default();
    let _ = out.send(FromScript::Ended(result, logs));
}

/// Runs `code` against `catalog`, with `input` as its `input` global (and
/// its fields as globals of their own), for at most `limit`.
pub async fn run(
    catalog: &dyn Catalog,
    code: &str,
    input: &Value,
    limit: Duration,
    stop: &CancellationToken,
    progress: &(dyn Fn(&[Call]) + Send + Sync),
) -> Ran {
    let (out, mut from_script) = mpsc::unbounded_channel();
    let halted = Arc::new(AtomicBool::new(false));
    let (code, input, namespaces, flag) = (
        code.to_owned(),
        input.clone(),
        catalog.namespaces(),
        Arc::clone(&halted),
    );
    std::thread::spawn(move || script_thread(&code, &input, namespaces, limit, &flag, &out));
    let mut calls: Vec<Call> = Vec::new();
    loop {
        let got = tokio::select! {
            got = from_script.recv() => got,
            () = stop.cancelled() => {
                halted.store(true, Ordering::Relaxed);
                from_script.recv().await
            }
        };
        match got {
            Some(FromScript::Call(request)) => {
                calls.push(Call {
                    tool: format!("{}.{}", request.namespace, request.tool),
                    status: "running".to_owned(),
                });
                progress(&calls);
                let answer = if stop.is_cancelled() {
                    Err("The run was stopped.".to_owned())
                } else {
                    catalog
                        .call(&request.namespace, &request.tool, request.args)
                        .await
                };
                if let Some(last) = calls.last_mut() {
                    let status = if answer.is_ok() { "completed" } else { "error" };
                    status.clone_into(&mut last.status);
                }
                progress(&calls);
                let _ = request.reply.send(answer);
            }
            Some(FromScript::Ended(value, logs)) => return Ran { value, logs, calls },
            None => {
                return Ran {
                    value: Err("The script's thread ended unexpectedly.".to_owned()),
                    logs: Vec::new(),
                    calls,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Sums;

    impl Catalog for Sums {
        fn namespaces(&self) -> Vec<Namespace> {
            vec![Namespace {
                name: "math".to_owned(),
                description: "Arithmetic.".to_owned(),
                tools: vec![ToolInfo {
                    name: "add".to_owned(),
                    description: "Adds two numbers.".to_owned(),
                    signature: "({a: number, b: number}): number".to_owned(),
                }],
            }]
        }

        fn call<'a>(&'a self, _namespace: &'a str, tool: &'a str, args: Value) -> Answer<'a> {
            Box::pin(async move {
                match tool {
                    "add" => Ok(json!(
                        args["a"].as_i64().unwrap_or(0) + args["b"].as_i64().unwrap_or(0)
                    )),
                    other => Err(format!("There's no tool {other}.")),
                }
            })
        }
    }

    /// A namespace of `count` tools, each with a signature and a
    /// description.
    fn server(count: usize) -> Namespace {
        Namespace {
            name: "records".to_owned(),
            description: "The tools of the MCP server records.".to_owned(),
            tools: (0..count)
                .map(|n| ToolInfo {
                    name: format!("fetch_{n}"),
                    description: format!("Fetches the records of kind {n}, newest first."),
                    signature: "({from: string, to: string, limit?: number}): unknown".to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_large_namespace_lists_its_tools_by_name_only() {
        // Up to the limit, every tool comes with how to call it.
        let small = describe(&[server(LISTED)]);
        assert!(
            small.contains("tools.records.fetch_9({from: string"),
            "{small}"
        );
        assert!(small.contains("Fetches the records of kind 9"), "{small}");

        // Past it, the names alone, and how to find the rest.
        let large = describe(&[server(100)]);
        assert!(large.contains("Its 100 tools, by name"), "{large}");
        assert!(
            large.contains("fetch_0, fetch_1,") && large.contains("fetch_99"),
            "{large}"
        );
        assert!(!large.contains("from: string"), "{large}");
        assert!(!large.contains("Fetches the records"), "{large}");
        assert!(
            large.contains(r#"search({namespace: "records", query: "words"})"#),
            "{large}"
        );
        assert!(
            large.len() * 5 < describe(&[server(LISTED)]).len() * 10,
            "{}",
            large.len()
        );

        // What the list left out, the search it names gives back.
        let found = search(
            &[server(100)],
            &json!({ "namespace": "records", "query": "kind 42" }),
        );
        assert_eq!(found["items"][0]["path"], "tools.records.fetch_42");
        assert!(
            found["items"][0]["signature"]
                .as_str()
                .is_some_and(|s| s.contains("from: string"))
        );
    }

    #[tokio::test]
    async fn a_script_calls_tools_logs_and_returns() {
        let code = "const a = await tools.math.add({a: 2, b: 3});\n\
                    console.log('a is', a);\n\
                    const found = search('add');\n\
                    return { a, b: await tools.math.add({a, b: 10}), path: found.items[0].path };";
        let ran = run(
            &Sums,
            code,
            &json!({}),
            Duration::from_secs(5),
            &CancellationToken::new(),
            &|_| {},
        )
        .await;
        assert_eq!(
            ran.value,
            Ok(Some(json!({ "a": 5, "b": 15, "path": "tools.math.add" })))
        );
        assert_eq!(ran.logs, ["a is 5"]);
        assert_eq!(ran.calls.len(), 2);
        assert!(
            ran.calls
                .iter()
                .all(|c| c.tool == "math.add" && c.status == "completed")
        );
    }

    #[tokio::test]
    async fn a_failed_call_throws_and_a_loop_runs_out_of_time() {
        let ran = run(
            &Sums,
            "try { await tools.math.nope({}); } catch (e) { return e.message; }",
            &json!({}),
            Duration::from_secs(5),
            &CancellationToken::new(),
            &|_| {},
        )
        .await;
        assert_eq!(ran.value, Ok(Some(json!("There's no tool nope."))));
        assert_eq!(ran.calls[0].status, "error");

        let ran = run(
            &Sums,
            "while (true) {}",
            &json!({}),
            Duration::from_millis(200),
            &CancellationToken::new(),
            &|_| {},
        )
        .await;
        assert!(ran.value.is_err(), "{ran:?}");
    }

    #[tokio::test]
    async fn input_and_its_fields_are_globals() {
        let ran = run(
            &Sums,
            "return { runs, state, same: input.now === now };",
            &json!({ "state": null, "now": 5, "runs": 2, "args": {} }),
            Duration::from_secs(5),
            &CancellationToken::new(),
            &|_| {},
        )
        .await;
        assert_eq!(
            ran.value,
            Ok(Some(json!({ "runs": 2, "state": null, "same": true })))
        );
        let (text, cut) = Ran {
            value: Ok(None),
            logs: vec!["x".into()],
            calls: Vec::new(),
        }
        .text();
        assert_eq!(
            (text.as_str(), cut),
            ("Done, with no result.\n\nLogs:\nx", false)
        );
    }
}
