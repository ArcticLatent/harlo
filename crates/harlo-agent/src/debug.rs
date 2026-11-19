//! The `debug` tool: a program run under a debugger, stopped at
//! breakpoints, stepped and looked into.
//!
//! It speaks the Debug Adapter Protocol to the adapter for the program's
//! language: `debugpy` for Python (from the project's `.venv` when it has
//! one), `dlv dap` for Go, and `lldb-dap` (else `gdb -i dap`) for native
//! programs (Rust, C, C++, Zig…, built with debug info). One session at a
//! time; it runs in the sandbox as `bash` does, and ends with the agent.
//! Each stop is reported in a few lines: where, the source line, the
//! stack, the frame's variables and what the program printed since.
//! It's off unless `HARLO_AGENT_DEBUG` is `on`.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::sandbox::Sandbox;

/// The tool's name.
pub const NAME: &str = "debug";

/// How long a request (not a run) may take.
const REQUEST: Duration = Duration::from_secs(30);

/// How long the program runs before a call returns, by default (and at most).
const RUN: u64 = 60;
const RUN_MAX: u64 = 600;

/// What's kept of the program's output between reports.
const OUTPUT_CHARS: usize = 2_000;

/// The most frames and variables a report lists, and a value's length.
const FRAMES: usize = 6;
const VARIABLES: usize = 25;
const VALUE_CHARS: usize = 120;

/// Whether the tool is switched on (`HARLO_AGENT_DEBUG=on`).
pub fn on() -> bool {
    std::env::var("HARLO_AGENT_DEBUG").is_ok_and(|v| v.trim() == "on")
}

/// The tool's spec.
pub fn spec() -> Value {
    json!({ "type": "function", "name": NAME, "strict": false,
        "description": "Run a program under a debugger to see what it does at a point: launch it with breakpoints, then continue, step and look at variables. Python (debugpy), Go (dlv) and native programs built with debug info (lldb-dap or gdb: Rust, C, C++). Each stop shows where, the stack, the frame's variables (expandable ones with [ref N]) and the program's output since. Use it when reading the code and a print don't settle what happens; one session at a time.",
        "parameters": { "type": "object", "properties": {
            "action": { "type": "string", "enum": ["launch", "break", "continue", "next", "step_in", "step_out", "pause", "wait", "stack", "vars", "eval", "threads", "stop"],
                "description": "launch: start program (stops at the first breakpoint, an uncaught exception, or its end). break: set breakpoints (replaces those of each file named; lines [] clears). continue/next/step_in/step_out: run on to the next stop. wait: keep waiting for a running program. vars: a frame's variables, or ref's children. eval: an expression in a frame." },
            "program": { "type": "string", "description": "launch: a .py script, a Go package folder or file, or a built binary" },
            "args": { "type": "array", "items": { "type": "string" }, "description": "launch: its arguments" },
            "module": { "type": "boolean", "description": "launch (Python): program is a module name (python -m)" },
            "breakpoints": { "type": "array", "items": { "type": "object", "properties": {
                "path": { "type": "string" }, "line": { "type": "integer" }, "condition": { "type": "string" } }, "required": ["path", "line"] },
                "description": "launch, break: where to stop" },
            "stop_on_entry": { "type": "boolean", "description": "launch: stop before the first line" },
            "expression": { "type": "string", "description": "eval: in the program's language" },
            "frame": { "type": "integer", "description": "vars, eval: the stack frame, 0 the innermost (default)" },
            "ref": { "type": "integer", "description": "vars: a [ref N] to expand" },
            "thread": { "type": "integer", "description": "The thread (default: the one that stopped)" },
            "timeout": { "type": "integer", "description": "Seconds to let it run before returning (60 by default, 600 at most)" },
        }, "required": ["action"], "additionalProperties": false } })
}

/// The agent's debugger: at most one session.
#[derive(Default)]
pub struct Debugger {
    session: Mutex<Option<Session>>,
}

impl Debugger {
    /// The answer to a call's `args`, in the project at `cwd`, and whether it failed.
    pub async fn call(&self, args: &Value, cwd: &Path, sandbox: Option<&Sandbox>) -> (String, bool) {
        let action = args["action"].as_str().unwrap_or_default();
        let mut slot = self.session.lock().await;
        if action == "launch" {
            if let Some(old) = slot.take() {
                old.end().await;
            }
            let started = Session::launch(args, cwd, sandbox).await;
            return match started {
                Ok((s, report)) => {
                    if !s.ended {
                        *slot = Some(s);
                    }
                    (report, false)
                }
                Err(why) => (why, true),
            };
        }
        if action == "stop"
            && let Some(s) = slot.take()
        {
            let output = s.take_output();
            s.end().await;
            return (format!("Stopped debugging.{}", output_note(&output)), false);
        }
        let Some(s) = slot.as_mut() else {
            return ("No program is being debugged: launch one first.".to_owned(), true);
        };
        let answer = s.act(action, args, cwd).await;
        if s.ended
            && let Some(s) = slot.take()
        {
            s.end().await;
        }
        match answer {
            Ok(text) => (text, false),
            Err(why) => (why, true),
        }
    }
}

/// Which adapter a program runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Python,
    Go,
    Native,
}

/// The program's kind, from its name and the project.
fn kind_of(program: &str, module: bool, cwd: &Path) -> Kind {
    let p = cwd.join(program);
    if module || program.ends_with(".py") || program.ends_with(".pyw") {
        Kind::Python
    } else if program.ends_with(".go") || (p.is_dir() && (p.join("go.mod").is_file() || has_go(&p))) {
        Kind::Go
    } else {
        Kind::Native
    }
}

fn has_go(dir: &Path) -> bool {
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| e.file_name().to_string_lossy().ends_with(".go"))
}

/// A debugging session: the adapter, its messages and where the program
/// last stopped.
struct Session {
    writer: Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>>,
    pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Value>>>>,
    events: mpsc::UnboundedReceiver<Value>,
    output: Arc<StdMutex<String>>,
    seq: i64,
    child: tokio::process::Child,
    pgid: i32,
    stderr: Arc<StdMutex<String>>,
    /// The thread that last stopped, and its frames' ids (innermost first).
    thread: i64,
    frames: Vec<i64>,
    /// The breakpoints by file (as the adapter takes them).
    breakpoints: HashMap<PathBuf, Vec<Value>>,
    /// The program has exited.
    ended: bool,
}

impl Session {
    /// Starts `args["program"]` under its adapter and runs it to its first stop.
    async fn launch(args: &Value, cwd: &Path, sandbox: Option<&Sandbox>) -> Result<(Self, String), String> {
        let program = args["program"].as_str().map(str::trim).filter(|p| !p.is_empty()).ok_or("Give the program to launch.")?;
        let module = args["module"].as_bool().unwrap_or(false);
        let kind = kind_of(program, module, cwd);
        let list: Vec<String> = args["args"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
        let stop_on_entry = args["stop_on_entry"].as_bool().unwrap_or(false);
        let abs = |p: &str| cwd.join(p).to_string_lossy().into_owned();
        let (command, launch) = match kind {
            Kind::Python => {
                let python = python(cwd);
                let ok = tokio::process::Command::new(&python).args(["-c", "import debugpy"]).stdout(Stdio::null()).stderr(Stdio::null()).status().await.is_ok_and(|s| s.success());
                if !ok {
                    return Err(format!("debugpy isn't installed for {python}: install it (pip install debugpy, or uv add --dev debugpy), then launch again."));
                }
                let mut launch = json!({ "request": "launch", "cwd": cwd, "args": list, "console": "internalConsole", "justMyCode": true, "stopOnEntry": stop_on_entry, "python": [python], "redirectOutput": true });
                if module {
                    launch["module"] = json!(program);
                } else {
                    launch["program"] = json!(abs(program));
                }
                (vec![python, "-m".into(), "debugpy.adapter".into()], launch)
            }
            Kind::Go => {
                if which("dlv").is_none() {
                    return Err("Delve (dlv) isn't installed: go install github.com/go-delve/delve/cmd/dlv@latest, then launch again.".to_owned());
                }
                let mode = if cwd.join(program).is_file() && !program.ends_with(".go") { "exec" } else { "debug" };
                let launch = json!({ "request": "launch", "mode": mode, "program": abs(program), "args": list, "cwd": cwd, "stopOnEntry": stop_on_entry });
                (vec!["dlv".into(), "dap".into(), "--listen=127.0.0.1:0".into()], launch)
            }
            Kind::Native => {
                let bin = cwd.join(program);
                if !bin.is_file() {
                    return Err(format!("{program} isn't there: build it first (with debug info), then launch the binary."));
                }
                let launch = json!({ "request": "launch", "program": bin, "args": list, "cwd": cwd, "stopOnEntry": stop_on_entry, "stopAtBeginningOfMainSubprogram": stop_on_entry });
                if which("lldb-dap").is_some() {
                    (vec!["lldb-dap".into()], launch)
                } else if which("gdb").is_some() {
                    (vec!["gdb".into(), "-q".into(), "-i".into(), "dap".into()], launch)
                } else {
                    return Err("No native debugger is installed: install lldb (lldb-dap) or gdb 14+, then launch again.".to_owned());
                }
            }
        };
        let mut s = Self::start(&command, kind, cwd, sandbox).await?;
        let caps = s.request("initialize", json!({ "clientID": "harlo", "clientName": "harlo", "adapterID": command[0], "linesStartAt1": true, "columnsStartAt1": true, "pathFormat": "path", "supportsVariableType": true, "supportsRunInTerminalRequest": false }), REQUEST).await.map_err(|e| s.failed("didn't start", &e))?;
        // (Some adapters answer launch only after configurationDone: it's
        // sent now and awaited after.)
        let launched = s.send_request("launch", launch).await?;
        s.until_event("initialized", REQUEST).await.map_err(|e| s.failed("didn't start the program", &e))?;
        for spec in args["breakpoints"].as_array().into_iter().flatten() {
            let path = cwd.join(spec["path"].as_str().unwrap_or_default());
            s.breakpoints.entry(path).or_default().push(source_breakpoint(spec));
        }
        let mut notes = String::new();
        for (path, list) in s.breakpoints.clone() {
            let set = s.request("setBreakpoints", json!({ "source": { "path": path }, "breakpoints": list }), REQUEST).await?;
            notes.push_str(&unverified(&set, &path, cwd));
        }
        let filters: Vec<&str> = caps["exceptionBreakpointFilters"].as_array().into_iter().flatten().filter(|f| f["default"] == true || f["filter"] == "uncaught").filter_map(|f| f["filter"].as_str()).collect();
        let _ = s.request("setExceptionBreakpoints", json!({ "filters": filters }), REQUEST).await;
        if caps["supportsConfigurationDoneRequest"] == true {
            s.request("configurationDone", json!({}), REQUEST).await?;
        }
        match tokio::time::timeout(Duration::from_secs(RUN_MAX), launched).await {
            Ok(Ok(r)) if r["success"] == true => {}
            Ok(Ok(r)) => return Err(s.failed("couldn't launch the program", &message(&r))),
            _ => return Err(s.failed("couldn't launch the program", "no answer")),
        }
        let report = s.run_to_stop(timeout(args), cwd).await?;
        Ok((s, format!("{notes}{report}")))
    }

    /// The adapter started (`command`), its messages read.
    async fn start(command: &[String], kind: Kind, cwd: &Path, sandbox: Option<&Sandbox>) -> Result<Self, String> {
        let line = format!("exec {}", command.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" "));
        let mut cmd = crate::tools::bash::command(&line, cwd, sandbox);
        cmd.stdin(Stdio::piped()).kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", command[0]))?;
        let pgid = child.id().map_or(0, |p| i32::try_from(p).unwrap_or(i32::MAX));
        if pgid > 0 {
            crate::jobs::end_at_exit(pgid);
        }
        let (Some(stdin), Some(stdout), Some(err)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            return Err(format!("Couldn't talk to {}.", command[0]));
        };
        let stderr = Arc::new(StdMutex::new(String::new()));
        tokio::spawn(keep_tail(err, Arc::clone(&stderr)));
        let output = Arc::new(StdMutex::new(String::new()));
        let (reader, writer): (Box<dyn AsyncRead + Unpin + Send>, Box<dyn AsyncWrite + Unpin + Send>) = if kind == Kind::Go {
            // (Delve listens on a port it prints; then its stdout is the
            // program's, read on so a print doesn't stop it.)
            let mut lines = BufReader::new(stdout).lines();
            let address = tokio::time::timeout(REQUEST, async {
                while let Ok(Some(l)) = lines.next_line().await {
                    if let Some(a) = l.split("listening at:").nth(1) {
                        return Some(a.trim().to_owned());
                    }
                }
                None
            })
            .await
            .ok()
            .flatten()
            .ok_or_else(|| format!("dlv didn't start{}", tail_note(&stderr)))?;
            let program_output = Arc::clone(&output);
            tokio::spawn(async move {
                while let Ok(Some(l)) = lines.next_line().await {
                    // (Its own note that it's building the program.)
                    if !l.starts_with("Building ") {
                        keep_output(&program_output, &format!("{l}\n"));
                    }
                }
            });
            let stream = tokio::net::TcpStream::connect(&address).await.map_err(|e| format!("Couldn't reach dlv at {address}: {e} (it needs the sandbox's network)."))?;
            let (r, w) = stream.into_split();
            (Box::new(r), Box::new(w))
        } else {
            (Box::new(stdout), Box::new(stdin))
        };
        let writer: Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>> = Arc::new(Mutex::new(writer));
        let pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Value>>>> = Arc::default();
        let (tx, events) = mpsc::unbounded_channel();
        tokio::spawn(read_loop(reader, Arc::clone(&writer), Arc::clone(&pending), tx, Arc::clone(&output)));
        Ok(Self { writer, pending, events, output, seq: 0, child, pgid, stderr, thread: 0, frames: Vec::new(), breakpoints: HashMap::new(), ended: false })
    }

    /// A request sent; its response comes on the receiver.
    async fn send_request(&mut self, command: &str, arguments: Value) -> Result<oneshot::Receiver<Value>, String> {
        self.seq += 1;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(self.seq, tx);
        let message = json!({ "seq": self.seq, "type": "request", "command": command, "arguments": arguments });
        write(&self.writer, &message).await.map_err(|e| format!("The debugger stopped ({e}).{}", tail_note(&self.stderr)))?;
        Ok(rx)
    }

    /// A request's response body, within `limit`.
    async fn request(&mut self, command: &str, arguments: Value, limit: Duration) -> Result<Value, String> {
        let rx = self.send_request(command, arguments).await?;
        match tokio::time::timeout(limit, rx).await {
            Ok(Ok(r)) if r["success"] == true => Ok(r["body"].clone()),
            Ok(Ok(r)) => Err(format!("{command}: {}", message(&r))),
            Ok(Err(_)) => Err(format!("The debugger stopped.{}", tail_note(&self.stderr))),
            Err(_) => Err(format!("The debugger didn't answer {command} within {} s.", limit.as_secs())),
        }
    }

    /// Waits for the event named `name` (others before it are dropped).
    async fn until_event(&mut self, name: &str, limit: Duration) -> Result<Value, String> {
        tokio::time::timeout(limit, async {
            while let Some(e) = self.events.recv().await {
                if e["event"] == name {
                    return Ok(e);
                }
            }
            Err("the debugger stopped".to_owned())
        })
        .await
        .unwrap_or_else(|_| Err(format!("no {name} event within {} s", limit.as_secs())))
    }

    /// A failure, with what the adapter said on stderr.
    fn failed(&self, what: &str, why: &str) -> String {
        format!("The debugger {what}: {why}{}", tail_note(&self.stderr))
    }

    /// The program's output since the last report (and none kept).
    fn take_output(&self) -> String {
        std::mem::take(&mut *self.output.lock().unwrap())
    }

    /// One action on a running session.
    async fn act(&mut self, action: &str, args: &Value, cwd: &Path) -> Result<String, String> {
        let thread = args["thread"].as_i64().unwrap_or(self.thread);
        match action {
            "continue" | "next" | "step_in" | "step_out" => {
                let command = match action {
                    "next" => "next",
                    "step_in" => "stepIn",
                    "step_out" => "stepOut",
                    _ => "continue",
                };
                self.request(command, json!({ "threadId": thread }), REQUEST).await?;
                self.run_to_stop(timeout(args), cwd).await
            }
            "wait" => self.run_to_stop(timeout(args), cwd).await,
            "pause" => {
                self.request("pause", json!({ "threadId": thread }), REQUEST).await?;
                self.run_to_stop(10, cwd).await
            }
            "break" => {
                let mut by_file: HashMap<PathBuf, Vec<Value>> = HashMap::new();
                for spec in args["breakpoints"].as_array().into_iter().flatten() {
                    by_file.entry(cwd.join(spec["path"].as_str().unwrap_or_default())).or_default().push(source_breakpoint(spec));
                }
                if by_file.is_empty() {
                    return Err("Give breakpoints: [{path, line}] (a file with no lines isn't cleared this way: call break with its path and line 0).".to_owned());
                }
                let mut out = String::new();
                for (path, mut list) in by_file {
                    list.retain(|b| b["line"].as_u64().unwrap_or(0) > 0);
                    let set = self.request("setBreakpoints", json!({ "source": { "path": path }, "breakpoints": list }), REQUEST).await?;
                    let lines: Vec<String> = list.iter().map(|b| b["line"].to_string()).collect();
                    let _ = writeln!(out, "{}: {}", rel(&path, cwd), if lines.is_empty() { "cleared".to_owned() } else { format!("lines {}", lines.join(", ")) });
                    out.push_str(&unverified(&set, &path, cwd));
                    self.breakpoints.insert(path, list);
                }
                Ok(out.trim_end().to_owned())
            }
            "stack" => {
                let body = self.request("stackTrace", json!({ "threadId": thread, "levels": 30 }), REQUEST).await?;
                let frames = body["stackFrames"].as_array().cloned().unwrap_or_default();
                let mut out = String::new();
                for (i, f) in frames.iter().enumerate() {
                    let _ = writeln!(out, "#{i} {}", frame_place(f, cwd));
                }
                Ok(if out.is_empty() { "No frames (is it stopped?).".to_owned() } else { out.trim_end().to_owned() })
            }
            "threads" => {
                let body = self.request("threads", json!({}), REQUEST).await?;
                let list: Vec<String> = body["threads"].as_array().into_iter().flatten().map(|t| format!("{} {}{}", t["id"], t["name"].as_str().unwrap_or_default(), if t["id"].as_i64() == Some(self.thread) { " (stopped here)" } else { "" })).collect();
                Ok(list.join("\n"))
            }
            "vars" => {
                if let Some(r) = args["ref"].as_i64() {
                    return self.variables(r).await;
                }
                let frame = self.frame(args)?;
                self.locals(frame).await
            }
            "eval" => {
                let expression = args["expression"].as_str().map(str::trim).filter(|e| !e.is_empty()).ok_or("Give the expression.")?;
                let frame = self.frame(args)?;
                let body = self.request("evaluate", json!({ "expression": expression, "frameId": frame, "context": "repl" }), REQUEST).await?;
                Ok(value_line(expression, &body["result"], &body["type"], &body["variablesReference"]))
            }
            other => Err(format!("No action called {other}.")),
        }
    }

    /// The id of the frame `args["frame"]` names (0: the innermost).
    fn frame(&self, args: &Value) -> Result<i64, String> {
        let i = usize::try_from(args["frame"].as_u64().unwrap_or(0)).unwrap_or(usize::MAX);
        self.frames.get(i).copied().ok_or_else(|| if self.frames.is_empty() { "It isn't stopped: there are no frames.".to_owned() } else { format!("There are {} frames (0 to {}).", self.frames.len(), self.frames.len() - 1) })
    }

    /// Lets the program run until it stops or ends (at most `secs`), and
    /// says where it is.
    async fn run_to_stop(&mut self, secs: u64, cwd: &Path) -> Result<String, String> {
        let waited = tokio::time::timeout(Duration::from_secs(secs), async {
            let mut exit = None;
            while let Some(e) = self.events.recv().await {
                match e["event"].as_str() {
                    Some("stopped") => return Ok(e["body"].clone()),
                    Some("exited") => exit = e["body"]["exitCode"].as_i64(),
                    Some("terminated") => return Err(exit),
                    _ => {}
                }
            }
            Err(exit)
        })
        .await;
        match waited {
            Err(_) => Ok(format!("Still running after {secs} s: wait for it, pause it, or stop.{}", output_note(&self.take_output()))),
            Ok(Ok(body)) => self.report(&body, cwd).await,
            Ok(Err(code)) => {
                self.ended = true;
                self.frames.clear();
                let code = code.map_or_else(String::new, |c| format!(" with code {c}"));
                Ok(format!("The program ended{code}.{}", output_note(&self.take_output())))
            }
        }
    }

    /// Where the program stopped: the place, its line, the stack, the
    /// innermost frame's variables and the output since.
    async fn report(&mut self, stopped: &Value, cwd: &Path) -> Result<String, String> {
        if let Some(t) = stopped["threadId"].as_i64() {
            self.thread = t;
        }
        let body = self.request("stackTrace", json!({ "threadId": self.thread, "levels": 20 }), REQUEST).await?;
        let frames = body["stackFrames"].as_array().cloned().unwrap_or_default();
        self.frames = frames.iter().filter_map(|f| f["id"].as_i64()).collect();
        let reason = stopped["description"].as_str().or_else(|| stopped["reason"].as_str()).unwrap_or("stopped");
        let mut out = String::new();
        match frames.first() {
            Some(top) => {
                let _ = writeln!(out, "Stopped ({reason}) at {}, thread {}", frame_place(top, cwd), self.thread);
                if let Some(text) = stopped["text"].as_str().filter(|t| !t.is_empty() && *t != reason) {
                    let _ = writeln!(out, "  {}", cut(text, 400));
                }
                if let (Some(path), Some(line)) = (top["source"]["path"].as_str(), top["line"].as_u64())
                    && let Some(code) = std::fs::read_to_string(path).ok().and_then(|t| t.lines().nth(usize::try_from(line).unwrap_or(1).saturating_sub(1)).map(str::to_owned))
                {
                    let _ = writeln!(out, "  {line} | {}", code.trim_end());
                }
                // (Callers in the project; the runtime's and libraries' only counted.)
                let callers: Vec<&Value> = frames.iter().skip(1).collect();
                let own: Vec<String> = callers.iter().filter(|f| in_project(f, cwd)).take(FRAMES).map(|f| frame_place(f, cwd)).collect();
                let others = callers.len() - own.len();
                if !callers.is_empty() {
                    let more = if others > 0 { format!(" (+{others} outside the project; stack lists all)") } else { String::new() };
                    let _ = writeln!(out, "Called from: {}{more}", if own.is_empty() { "-".to_owned() } else { own.join(" ← ") });
                }
                if let Some(id) = top["id"].as_i64() {
                    let locals = self.locals(id).await.unwrap_or_else(|e| e);
                    let _ = writeln!(out, "{locals}");
                }
            }
            None => {
                let _ = writeln!(out, "Stopped ({reason}), thread {}, with no frames to show.", self.thread);
            }
        }
        out.push_str(&output_note(&self.take_output()));
        Ok(out.trim_end().to_owned())
    }

    /// A frame's variables (its first scope that isn't costly).
    async fn locals(&mut self, frame: i64) -> Result<String, String> {
        let scopes = self.request("scopes", json!({ "frameId": frame }), REQUEST).await?;
        let list = scopes["scopes"].as_array().cloned().unwrap_or_default();
        let Some(scope) = list.iter().find(|s| s["expensive"] != true).or(list.first()) else { return Ok("No variables.".to_owned()) };
        let r = scope["variablesReference"].as_i64().unwrap_or(0);
        let name = scope["name"].as_str().unwrap_or("Variables");
        let vars = self.variables(r).await?;
        Ok(format!("{name}: {}", vars.replace('\n', " · ")))
    }

    /// The children of variables reference `r`, a line each.
    async fn variables(&mut self, r: i64) -> Result<String, String> {
        let body = self.request("variables", json!({ "variablesReference": r }), REQUEST).await?;
        let all: Vec<&Value> = body["variables"].as_array().into_iter().flatten().filter(|v| !v["name"].as_str().unwrap_or_default().ends_with(" variables")).collect();
        if all.is_empty() {
            return Ok("(none)".to_owned());
        }
        let mut lines: Vec<String> = all.iter().take(VARIABLES).map(|v| value_line(v["name"].as_str().unwrap_or("?"), &v["value"], &v["type"], &v["variablesReference"])).collect();
        if all.len() > VARIABLES {
            lines.push(format!("… {} more", all.len() - VARIABLES));
        }
        Ok(lines.join("\n"))
    }

    /// Ends the session: the program and the adapter.
    async fn end(mut self) {
        if !self.ended {
            let _ = tokio::time::timeout(Duration::from_secs(3), self.request("disconnect", json!({ "terminateDebuggee": true }), Duration::from_secs(3))).await;
        }
        if self.pgid > 0 {
            // SAFETY: killpg only sends a signal to the adapter's own process group.
            unsafe {
                libc::killpg(self.pgid, libc::SIGKILL);
            }
        }
        let _ = self.child.kill().await;
    }
}

/// A breakpoint as the adapter takes it.
fn source_breakpoint(spec: &Value) -> Value {
    let mut b = json!({ "line": spec["line"].as_u64().unwrap_or(0) });
    if let Some(c) = spec["condition"].as_str().filter(|c| !c.trim().is_empty()) {
        b["condition"] = json!(c);
    }
    b
}

/// A line for each breakpoint the adapter couldn't place.
fn unverified(set: &Value, path: &Path, cwd: &Path) -> String {
    let mut out = String::new();
    for b in set["breakpoints"].as_array().into_iter().flatten().filter(|b| b["verified"] == false) {
        let _ = writeln!(out, "Breakpoint at {}:{} not placed{}", rel(path, cwd), b["line"].as_u64().map_or_else(|| "?".to_owned(), |l| l.to_string()), b["message"].as_str().map_or_else(String::new, |m| format!(": {m}")));
    }
    out
}

/// `name = value (type) [ref N]`.
fn value_line(name: &str, value: &Value, kind: &Value, r: &Value) -> String {
    let mut s = format!("{name} = {}", cut(value.as_str().unwrap_or_default(), VALUE_CHARS));
    if let Some(t) = kind.as_str().filter(|t| !t.is_empty()) {
        let _ = write!(s, " ({t})");
    }
    if let Some(r) = r.as_i64().filter(|r| *r > 0) {
        let _ = write!(s, " [ref {r}]");
    }
    s
}

/// Whether a frame's source is in the project.
fn in_project(f: &Value, cwd: &Path) -> bool {
    f["source"]["path"].as_str().is_some_and(|p| Path::new(p).starts_with(cwd) && Path::new(p).is_file())
}

/// `name path:line`.
fn frame_place(f: &Value, cwd: &Path) -> String {
    let name = f["name"].as_str().unwrap_or("?");
    match f["source"]["path"].as_str() {
        Some(p) => format!("{name} {}:{}", rel(Path::new(p), cwd), f["line"]),
        None => format!("{name} (no source)"),
    }
}

fn rel(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd).unwrap_or(path).display().to_string()
}

fn cut(text: &str, n: usize) -> String {
    let one = text.replace('\n', "⏎");
    if one.chars().count() > n { format!("{}…", one.chars().take(n).collect::<String>()) } else { one }
}

/// The program's output, as a report's last lines.
fn output_note(output: &str) -> String {
    if output.trim().is_empty() {
        return String::new();
    }
    let n = output.chars().count();
    let tail: String = if n > OUTPUT_CHARS { format!("…{}", output.chars().skip(n - OUTPUT_CHARS).collect::<String>()) } else { output.to_owned() };
    format!("\nOutput:\n{}", tail.trim_end())
}

/// A response's error message.
fn message(r: &Value) -> String {
    r["body"]["error"]["format"].as_str().or_else(|| r["message"].as_str()).unwrap_or("failed").to_owned()
}

fn timeout(args: &Value) -> u64 {
    args["timeout"].as_u64().unwrap_or(RUN).clamp(1, RUN_MAX)
}

/// The project's Python: its `.venv`'s, else `python3`.
fn python(cwd: &Path) -> String {
    for venv in [".venv", "venv"] {
        let p = cwd.join(venv).join("bin/python");
        if p.is_file() {
            return p.to_string_lossy().into_owned();
        }
    }
    "python3".to_owned()
}

fn which(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(program)).find(|p| p.is_file()))
}

fn shell_quote(a: &str) -> String {
    format!("'{}'", a.replace('\'', "'\\''"))
}

/// What the adapter said last on stderr, for a failure.
fn tail_note(stderr: &StdMutex<String>) -> String {
    let s = stderr.lock().unwrap();
    let tail: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = tail[tail.len().saturating_sub(3)..].join(" / ");
    if tail.is_empty() { String::new() } else { format!(" ({})", cut(&tail, 400)) }
}

/// Keeps the end of what `from` writes in `into`.
async fn keep_tail(mut from: impl AsyncRead + Unpin, into: Arc<StdMutex<String>>) {
    let mut buf = vec![0; 8192];
    while let Ok(n) = from.read(&mut buf).await {
        if n == 0 {
            break;
        }
        let mut s = into.lock().unwrap();
        s.push_str(&String::from_utf8_lossy(&buf[..n]));
        if s.len() > 8192 {
            let at = s.len() - 4096;
            let at = (at..s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(at);
            s.drain(..at);
        }
    }
}

/// Adds `text` to the program's output, keeping its end.
fn keep_output(output: &StdMutex<String>, text: &str) {
    let mut o = output.lock().unwrap();
    o.push_str(text);
    if o.len() > OUTPUT_CHARS * 4 {
        let at = o.len() - OUTPUT_CHARS * 2;
        let at = (at..o.len()).find(|i| o.is_char_boundary(*i)).unwrap_or(at);
        o.drain(..at);
    }
}

/// Writes one message, with its header.
async fn write(writer: &Mutex<Box<dyn AsyncWrite + Unpin + Send>>, message: &Value) -> std::io::Result<()> {
    let body = message.to_string();
    let mut w = writer.lock().await;
    w.write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await?;
    w.flush().await
}

/// Reads the adapter's messages: responses to their requests, output kept,
/// other events passed on, its own requests refused.
async fn read_loop(reader: Box<dyn AsyncRead + Unpin + Send>, writer: Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>>, pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Value>>>>, events: mpsc::UnboundedSender<Value>, output: Arc<StdMutex<String>>) {
    let mut r = BufReader::new(reader);
    let mut seq = 1_000_000;
    while let Some(m) = read_message(&mut r).await {
        match m["type"].as_str() {
            Some("response") => {
                if let Some(tx) = m["request_seq"].as_i64().and_then(|s| pending.lock().unwrap().remove(&s)) {
                    let _ = tx.send(m);
                }
            }
            Some("event") if m["event"] == "output" => {
                let category = m["body"]["category"].as_str().unwrap_or("console");
                // (The console is the adapter's own chatter, not the program's.)
                if matches!(category, "stdout" | "stderr" | "important") {
                    keep_output(&output, m["body"]["output"].as_str().unwrap_or_default());
                }
            }
            Some("event") => {
                let _ = events.send(m);
            }
            Some("request") => {
                seq += 1;
                let reply = json!({ "seq": seq, "type": "response", "request_seq": m["seq"], "command": m["command"], "success": false, "message": "not supported" });
                let _ = write(&writer, &reply).await;
            }
            _ => {}
        }
    }
    for (_, tx) in pending.lock().unwrap().drain() {
        drop(tx);
    }
    let _ = events.send(json!({ "type": "event", "event": "terminated" }));
}

/// One message (`Content-Length` framed), or None at the end.
async fn read_message(r: &mut (impl AsyncBufReadExt + Unpin)) -> Option<Value> {
    loop {
        let mut length = None;
        loop {
            let mut header = String::new();
            if r.read_line(&mut header).await.ok()? == 0 {
                return None;
            }
            let h = header.trim();
            if h.is_empty() {
                if length.is_some() {
                    break;
                }
                continue;
            }
            if let Some((k, v)) = h.split_once(':')
                && k.eq_ignore_ascii_case("content-length")
            {
                length = v.trim().parse::<usize>().ok();
            }
        }
        let mut body = vec![0; length?];
        r.read_exact(&mut body).await.ok()?;
        if let Ok(m) = serde_json::from_slice(&body) {
            return Some(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_go_to_their_debugger() {
        let dir = std::env::temp_dir().join(format!("harlo-debug-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("cmd/app")).unwrap();
        std::fs::write(dir.join("cmd/app/main.go"), "package main").unwrap();
        assert_eq!(kind_of("tool.py", false, &dir), Kind::Python);
        assert_eq!(kind_of("pkg.cli", true, &dir), Kind::Python);
        assert_eq!(kind_of("cmd/app", false, &dir), Kind::Go);
        assert_eq!(kind_of("main.go", false, &dir), Kind::Go);
        assert_eq!(kind_of("target/debug/app", false, &dir), Kind::Native);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn values_and_output_read_short() {
        assert_eq!(value_line("items", &json!("[1, 2]"), &json!("list"), &json!(7)), "items = [1, 2] (list) [ref 7]");
        assert_eq!(value_line("n", &json!("3"), &Value::Null, &json!(0)), "n = 3");
        assert_eq!(output_note(""), "");
        assert_eq!(output_note("hi\n"), "\nOutput:\nhi");
        let long = "x".repeat(OUTPUT_CHARS + 10);
        assert!(output_note(&long).starts_with("\nOutput:\n…x"));
    }

    #[tokio::test]
    async fn messages_are_read_from_their_frames() {
        let data = b"content-length: 9\r\n\r\n{\"seq\":1}Content-Length: 2\r\n\r\n{}".to_vec();
        let mut r = BufReader::new(data.as_slice());
        assert_eq!(read_message(&mut r).await, Some(json!({"seq": 1})));
        assert_eq!(read_message(&mut r).await, Some(json!({})));
        assert_eq!(read_message(&mut r).await, None);
    }
}
