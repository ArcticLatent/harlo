//! `run_code`: the model writes a short Python (or JavaScript) program that
//! calls tools (`tools.read(...)`, `tools.grep(...)`, an MCP server's
//! tools), looks at what they return, decides what to call next, and prints
//! only what's worth keeping. One round instead of several, and the tools'
//! output stays out of the conversation unless the program prints it.
//!
//! - Programs run in a kernel, one per language, that keeps its variables
//!   between calls (`fresh` starts a new one; a program that times out
//!   ends it): `python3 -I`, or `node` for JavaScript, in the sandbox as
//!   bash commands are, so it asks first where bash does. Its prints and
//!   its commands' output come back; stdin is closed to the program.
//! - Its calls come to harlo-agent on the kernel's stdout as JSON lines
//!   (its own prints are captured beside them) and are answered on its
//!   stdin. Every frame is checked: only tools that only look (ours and an
//!   MCP server's read-only ones) can be called, at most [`MAX_CALLS`] in
//!   one program.
//! - Offered when an MCP server has read-only tools (see [`offered`])
//!   and there's a python3.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::fmt::Write as _;
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::tools::{Outcome, Tools};

const DEFAULT_TIMEOUT: u64 = 120;
const MAX_TIMEOUT: u64 = 600;
/// Calls one program may make.
const MAX_CALLS: usize = 300;
/// Our tools a program may call (all only look).
const OURS: &[&str] = &["read", "grep", "find", "ls", "current_time", "lsp"];

/// Whether the tool is offered (with a python3 to run it): when an MCP
/// server has tools that only look, since that's where it pays (measured:
/// a scene's inventory over MCP took 74% fewer tokens with it, while
/// coding tasks hardly used it and took 12% more with it offered).
/// `HARLO_AGENT_CODE=on` offers it always, `off` never.
pub fn offered(mcp: Option<&crate::mcp::Mcp>) -> bool {
    let wanted = match std::env::var("HARLO_AGENT_CODE").as_deref().map(str::trim) {
        Ok("on") => true,
        Ok("off") => false,
        _ => mcp.is_some_and(crate::mcp::Mcp::has_read_only),
    };
    wanted && python()
}

/// Whether there's a node for JavaScript programs.
fn node() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| std::process::Command::new("node").arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success()))
}

fn python() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        std::process::Command::new("python3")
            .args(["-I", "-c", "import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

/// The tools a program may call, from the session's specs (ours that only
/// look, and the MCP servers' read-only ones).
pub fn callable<'a>(specs: &'a [Value], mcp: Option<&crate::mcp::Mcp>) -> Vec<&'a Value> {
    specs
        .iter()
        .filter(|s| {
            let n = s["name"].as_str().unwrap_or_default();
            OURS.contains(&n) || (n != crate::mcp::CALL && mcp.is_some_and(|m| m.has(n) && m.read_only(n)))
        })
        .collect()
}

/// A tool's parameters in the order a call's positional arguments fill
/// them: the required ones first, then the rest, each as declared.
fn order(spec: &Value) -> Vec<String> {
    let props: Vec<String> = spec["parameters"]["properties"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    let required: Vec<&str> = spec["parameters"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let (mut first, rest): (Vec<String>, Vec<String>) = props.into_iter().partition(|p| required.contains(&p.as_str()));
    first.extend(rest);
    first
}

/// The tool's spec, the tools it may call named in it.
pub fn spec(callable: &[&Value]) -> Value {
    let mut sdk = String::new();
    for s in callable {
        let required: Vec<&str> = s["parameters"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let params: Vec<String> = order(s).into_iter().map(|p| if required.contains(&p.as_str()) { p } else { format!("{p}=None") }).collect();
        let about: String = s["description"].as_str().unwrap_or_default().split(". ").next().unwrap_or_default().chars().take(140).collect();
        let _ = write!(sdk, "\n- tools.{}({}): {}", s["name"].as_str().unwrap_or_default(), params.join(", "), about.trim_end_matches('.'));
    }
    let js = node();
    let mut properties = json!({
        "description": { "type": "string", "description": "What it does, in a few words (shown to the user)" },
        "code": { "type": "string", "description": "The program" },
        "fresh": { "type": "boolean", "description": "Start a new kernel (its variables gone)" },
        "timeout": { "type": "integer", "description": "Seconds (120 by default, up to 600)" },
    });
    if js {
        properties["language"] = json!({ "type": "string", "enum": ["python", "javascript"], "description": "python by default" });
    }
    json!({
        "type": "function", "name": "run_code", "strict": false,
        "description": format!(
            "Run a short {} program that calls tools and prints only what matters: for steps that depend on each other (list, then look into what the list says), or many calls whose output you'd only filter. One call instead of several rounds, and what the tools return stays out of the conversation unless you print it. \
             It runs in a kernel (in the working directory, the sandbox as bash's) that keeps its variables for your next run_code; call tools as tools.name(args) (positional or by name{}), each returns its text as a string (an MCP tool's as its server sends it: JSON from some, plain lines from others, so parse it only where it starts with {{ or [), a failed call raises ToolError with its message. Only what you print comes back, so print a short, finished answer. \
             Not for changing files (use edit and write) or one plain call. Its tools:{sdk}",
            if js { "Python (or JavaScript)" } else { "Python" },
            if js { "; in JavaScript, print with console.log" } else { "" },
        ),
        "parameters": { "type": "object", "properties": properties, "required": ["description", "code"], "additionalProperties": false },
    })
}

/// The Python kernel: reads programs, a JSON line each, runs each with
/// `tools` in the same globals, and sends its calls and its output as JSON
/// lines.
const BOOT: &str = r#"
import io, json, os, sys, tempfile, threading, traceback, linecache
_out = os.fdopen(os.dup(1), "w", encoding="utf-8")
_in = os.fdopen(os.dup(0), "r", encoding="utf-8")
_null = os.open(os.devnull, os.O_RDONLY); os.dup2(_null, 0)
_buf = io.StringIO()
class _Cap(io.TextIOBase):
    def writable(self): return True
    def write(self, s):
        if _buf.tell() < 1000000: _buf.write(s)
        return len(s)
sys.stdin = io.StringIO("")
class ToolError(Exception):
    pass
def _send(frame):
    _out.write(json.dumps(frame) + "\n"); _out.flush()
# (A call and its answer as one: a program's threads take turns.)
_calling = threading.Lock()
def _call(name, args, kwargs):
    with _calling:
        _send({"t": "call", "name": name, "args": list(args), "kwargs": kwargs})
        line = _in.readline()
    if not line:
        raise SystemExit("harlo-agent went away")
    r = json.loads(line)
    if r.get("failed"):
        raise ToolError(r.get("text", ""))
    return r.get("text", "")
class _Tools:
    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        return lambda *a, **k: _call(name, a, k)
    def call(self, name, *a, **k):
        return _call(name, a, k)
_globals = {"__name__": "__main__", "tools": _Tools(), "ToolError": ToolError}
_n = 0
while True:
    line = _in.readline()
    if not line:
        break
    code = json.loads(line)["code"]
    _n += 1
    name = "<program %d>" % _n
    _buf.seek(0); _buf.truncate()
    _cmds = tempfile.TemporaryFile()
    os.dup2(_cmds.fileno(), 1); os.dup2(_cmds.fileno(), 2)
    sys.stdout = sys.stderr = _Cap()
    error = None
    try:
        exec(compile(code, name, "exec"), _globals)
    except SystemExit as e:
        if e.code not in (None, 0):
            error = "SystemExit: %s" % (e.code,)
    except BaseException as e:
        # (The program's own lines only.)
        linecache.cache[name] = (len(code), None, code.splitlines(True), name)
        frames = [f for f in traceback.extract_tb(e.__traceback__) if f.filename == name]
        error = "Traceback (most recent call last):\n" + "".join(traceback.format_list(frames)) + "".join(traceback.format_exception_only(type(e), e))
    sys.stdout.flush()
    _cmds.seek(0)
    cmds = _cmds.read(200000).decode("utf-8", "replace")
    _cmds.close()
    _send({"t": "done", "output": _buf.getvalue(), "commands": cmds, "error": error})
"#;

/// The JavaScript kernel (node): the same, frames marked with a leading
/// \x1e so a command's own output on stdout can't pass for one. Calls are
/// answered synchronously, so `tools.read({path})` returns its text.
const BOOT_JS: &str = r#"
const fs = require("fs"), vm = require("vm"), util = require("util");
const RS = "\x1e";
let pending = "";
function readLine() {
  const buf = Buffer.alloc(65536);
  for (;;) {
    const i = pending.indexOf("\n");
    if (i >= 0) { const line = pending.slice(0, i); pending = pending.slice(i + 1); return line; }
    let n = 0;
    try { n = fs.readSync(0, buf, 0, buf.length, null); } catch (e) { if (e.code === "EAGAIN") continue; throw e; }
    if (n === 0) return null;
    pending += buf.toString("utf8", 0, n);
  }
}
function send(frame) { fs.writeSync(1, RS + JSON.stringify(frame) + "\n"); }
class ToolError extends Error {}
ToolError.prototype.name = "ToolError";
function call(name, a) {
  const named = a.length === 1 && a[0] !== null && typeof a[0] === "object" && !Array.isArray(a[0]);
  send({ t: "call", name, args: named ? [] : a, kwargs: named ? a[0] : {} });
  const line = readLine();
  if (line === null) process.exit(0);
  const r = JSON.parse(line);
  if (r.failed) throw new ToolError(r.text || "");
  return r.text || "";
}
const tools = new Proxy({}, { get: (_, name) => (typeof name === "string" && !name.startsWith("_")) ? (...a) => call(name, a) : undefined });
let out = "";
const write = (...a) => { if (out.length < 1000000) out += a.map((x) => typeof x === "string" ? x : util.inspect(x, { depth: 4 })).join(" ") + "\n"; };
const ctx = vm.createContext({ tools, ToolError, require, process, Buffer, console: { log: write, info: write, warn: write, error: write, dir: write }, setTimeout, setInterval, clearTimeout, clearInterval, URL, TextEncoder, TextDecoder });
let n = 0;
for (;;) {
  const line = readLine();
  if (line === null) break;
  const code = JSON.parse(line).code;
  out = ""; n += 1;
  let error = null;
  try {
    vm.runInContext(code, ctx, { filename: "<program " + n + ">" });
  } catch (e) {
    const head = (e && e.name !== undefined) ? e.name + ": " + e.message : String(e);
    const frames = String((e && e.stack) || "").split("\n").filter((l) => /^\s+at .*<program /.test(l));
    error = [head, ...frames].join("\n");
  }
  send({ t: "done", output: out, commands: "", error });
}
"#;

/// A program kernel: its process and its pipes. Dropped, it ends.
pub struct Kernel {
    _group: crate::tools::bash::Group,
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    stderr: Arc<StdMutex<String>>,
    /// Lines on stdout that aren't frames (JavaScript: a command's output).
    marked: bool,
}

/// The session's kernels, by language.
pub type Kernels = tokio::sync::Mutex<HashMap<&'static str, Kernel>>;

impl Kernel {
    fn start(language: &'static str, tools: &Tools) -> Result<Self, String> {
        let sandbox = tools.sandbox.as_deref();
        if let Some(why) = sandbox.and_then(|sb| sb.broken.as_ref()) {
            return Err(format!("Programs can't run: the sandbox can't start here ({why})."));
        }
        let (line, boot) = if language == "javascript" { ("exec node -e \"$HARLO_AGENT_PROGRAM\"", BOOT_JS) } else { ("exec python3 -I -c \"$HARLO_AGENT_PROGRAM\"", BOOT) };
        let mut cmd = crate::tools::bash::command(line, &tools.cwd, sandbox);
        cmd.env("HARLO_AGENT_PROGRAM", boot).stdin(Stdio::piped()).kill_on_drop(true);
        let program = if language == "javascript" { "node" } else { "python3" };
        let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {program}: {e}"))?;
        let group = crate::tools::bash::Group::of(&child);
        let (Some(stdin), Some(stdout), Some(mut err)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            return Err(format!("Couldn't talk to {program}."));
        };
        let stderr = Arc::new(StdMutex::new(String::new()));
        let keep = Arc::clone(&stderr);
        tokio::spawn(async move {
            let mut buf = vec![0; 4096];
            while let Ok(n) = err.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let mut s = keep.lock().unwrap();
                s.push_str(&String::from_utf8_lossy(&buf[..n]));
                if s.len() > 8192 {
                    let at = s.len() - 4096;
                    let at = (at..s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(at);
                    s.drain(..at);
                }
            }
        });
        Ok(Kernel { _group: group, _child: child, stdin, lines: BufReader::new(stdout).lines(), stderr, marked: language == "javascript" })
    }
}

/// Runs a `run_code` call: its program in its language's kernel (started
/// when there's none, or `fresh`), its calls answered (`callable`: what it
/// may call).
pub async fn run(args: &Value, tools: &Tools, mcp: Option<&crate::mcp::Mcp>, callable: &[&Value]) -> Outcome {
    let fail = |t: String| Outcome::new(t, true);
    let code = args["code"].as_str().unwrap_or_default();
    if code.trim().is_empty() {
        return fail("Give the program (code).".into());
    }
    let language: &'static str = match args["language"].as_str().map(str::trim) {
        None | Some("" | "python" | "py") => "python",
        Some("javascript" | "js" | "node") if node() => "javascript",
        Some(other) => return fail(format!("run_code runs python{}, not {other}.", if node() { " or javascript" } else { "" })),
    };
    let secs = args["timeout"].as_u64().filter(|t| *t > 0).unwrap_or(DEFAULT_TIMEOUT).min(MAX_TIMEOUT);
    let mut kernels = tools.kernels.lock().await;
    if args["fresh"] == true {
        kernels.remove(language);
    }
    let kernel = match kernels.remove(language) {
        Some(k) => k,
        None => match Kernel::start(language, tools) {
            Ok(k) => k,
            Err(why) => return fail(why),
        },
    };
    let (kernel, ended, calls, stray) = exchange(kernel, code, secs, tools, mcp, callable).await;
    if let Some(k) = kernel {
        kernels.insert(language, k);
    }
    drop(kernels);
    let frame = match ended {
        Err(why) => return fail(why),
        Ok(f) => f,
    };
    let mut text = frame["output"].as_str().unwrap_or_default().to_owned();
    let commands = format!("{}{stray}", frame["commands"].as_str().unwrap_or_default());
    if !commands.trim().is_empty() {
        let _ = write!(text, "\n[Its commands' output]\n{commands}");
    }
    let error = frame["error"].as_str().filter(|e| !e.trim().is_empty());
    if let Some(e) = error {
        let _ = write!(text, "\n[The program failed]\n{}", tail(e, 30));
    }
    let total = text.len();
    let mut shown = crate::tools::bash::shorten(&text, total, &tools.spill_dir);
    if shown.trim().is_empty() {
        shown = "(it printed nothing: print what you need)".into();
    }
    shown.push_str(&format!("\n[{}]", made(&calls).trim_start_matches(", ").trim_start_matches("after ")).replace("[]", "[no tool calls]"));
    let mut out = Outcome::new(shown, error.is_some());
    out.details = json!({ "calls": calls.iter().map(|(n, f)| json!({ "tool": n, "failed": f })).collect::<Vec<_>>() });
    out
}

/// What became of a program: the kernel (None: it ended, and its
/// variables with it), its done frame or why there's none, its calls, and
/// stdout lines that weren't frames.
type Exchanged = (Option<Kernel>, Result<Value, String>, Vec<(String, bool)>, String);

/// Sends `code` to `kernel` and answers its calls until it's done, within `secs`.
async fn exchange(mut kernel: Kernel, code: &str, secs: u64, tools: &Tools, mcp: Option<&crate::mcp::Mcp>, callable: &[&Value]) -> Exchanged {
    let mut calls: Vec<(String, bool)> = Vec::new();
    let mut stray = String::new();
    let work = async {
        let first = json!({ "code": code }).to_string() + "\n";
        kernel.stdin.write_all(first.as_bytes()).await.map_err(|e| format!("Couldn't send the program: {e}"))?;
        while let Ok(Some(line)) = kernel.lines.next_line().await {
            let body = if kernel.marked {
                match line.strip_prefix('\u{1e}') {
                    Some(b) => b,
                    None => {
                        if stray.len() < 200_000 {
                            stray.push_str(&line);
                            stray.push('\n');
                        }
                        continue;
                    }
                }
            } else {
                line.as_str()
            };
            let Ok(frame) = serde_json::from_str::<Value>(body) else { continue };
            match frame["t"].as_str() {
                Some("call") => {
                    let name = frame["name"].as_str().unwrap_or_default().to_owned();
                    let reply = if calls.len() >= MAX_CALLS {
                        Outcome::new(format!("The program made more than {MAX_CALLS} calls: no more are answered."), true)
                    } else {
                        match callable.iter().find(|s| s["name"] == name.as_str()) {
                            None => Outcome::new(format!("A program can't call {name}: its tools are {}.", names(callable)), true),
                            Some(spec) => match arguments(spec, &frame) {
                                Err(why) => Outcome::new(format!("{why:#}"), true),
                                Ok(a) => match mcp.filter(|m| m.has(&name)) {
                                    Some(m) => m.call_whole(&name, &a).await,
                                    None => tools.call(&name, &a).await,
                                },
                            },
                        }
                    };
                    calls.push((name, reply.failed));
                    let answer = json!({ "text": reply.text, "failed": reply.failed }).to_string() + "\n";
                    if kernel.stdin.write_all(answer.as_bytes()).await.is_err() {
                        break;
                    }
                }
                Some("done") => return Ok(frame),
                _ => {}
            }
        }
        Err("The program ended its kernel (it exited, or crashed); the next run_code starts a new one.".to_owned())
    };
    let ended = tokio::time::timeout(Duration::from_secs(secs), work).await;
    match ended {
        Ok(Ok(frame)) => (Some(kernel), Ok(frame), calls, stray),
        Err(_) => {
            let why = format!("The program was stopped after {secs} seconds{}, and its kernel with it (its variables are gone). Give it a larger timeout, or do less in one program.", made(&calls));
            (None, Err(why), calls, stray)
        }
        Ok(Err(why)) => {
            let err_text = kernel.stderr.lock().unwrap().clone();
            let why = format!("{why}{}", if err_text.trim().is_empty() { String::new() } else { format!("\n{}", tail(&err_text, 20)) });
            (None, Err(why), calls, stray)
        }
    }
}

/// A call's arguments as the tool takes them: positional ones in the
/// tool's order, then those by name.
fn arguments(spec: &Value, frame: &Value) -> anyhow::Result<Value> {
    let order = order(spec);
    let mut out = serde_json::Map::new();
    let positional = frame["args"].as_array().cloned().unwrap_or_default();
    if positional.len() > order.len() {
        return Err(anyhow::anyhow!("{} takes {} arguments ({}), not {}.", spec["name"].as_str().unwrap_or_default(), order.len(), order.join(", "), positional.len()));
    }
    for (p, v) in order.iter().zip(positional) {
        out.insert(p.clone(), v);
    }
    for (k, v) in frame["kwargs"].as_object().into_iter().flatten() {
        if !order.contains(k) {
            return Err(anyhow::anyhow!("{} has no argument {k}: it takes {}.", spec["name"].as_str().unwrap_or_default(), order.join(", ")));
        }
        out.insert(k.clone(), v.clone());
    }
    // (None for an optional one: left out.)
    out.retain(|_, v| !v.is_null());
    Ok(Value::Object(out))
}

fn names(callable: &[&Value]) -> String {
    callable.iter().filter_map(|s| s["name"].as_str()).collect::<Vec<_>>().join(", ")
}

/// ", after 5 tool calls (read ×3, grep ×2; 1 failed)".
fn made(calls: &[(String, bool)]) -> String {
    if calls.is_empty() {
        return String::new();
    }
    let mut counts: Vec<(String, usize)> = Vec::new();
    for (n, _) in calls {
        match counts.iter_mut().find(|(c, _)| c == n) {
            Some(c) => c.1 += 1,
            None => counts.push((n.clone(), 1)),
        }
    }
    let list: Vec<String> = counts.iter().map(|(n, c)| if *c > 1 { format!("{n} ×{c}") } else { n.clone() }).collect();
    let failed = calls.iter().filter(|(_, f)| *f).count();
    format!(
        ", after {} tool call{} ({}{})",
        calls.len(),
        if calls.len() == 1 { "" } else { "s" },
        list.join(", "),
        if failed > 0 { format!("; {failed} failed") } else { String::new() }
    )
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.trim_end().lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn tools_in(dir: &Path) -> Tools {
        Tools::new(dir.to_path_buf(), dir.join(".out"))
    }

    #[tokio::test]
    async fn a_program_calls_tools_and_prints_what_matters() {
        if !python() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("harlo-agent-program-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.py"), "def old_name():\n    pass\n").unwrap();
        std::fs::write(dir.join("src/b.py"), "from a import old_name\nold_name()\n").unwrap();
        std::fs::write(dir.join("src/c.py"), "print('nothing here')\n").unwrap();
        let t = tools_in(&dir);
        let specs = crate::tools::specs();
        let list = callable(specs.as_array().unwrap(), None);
        assert_eq!(names(&list), "read, grep, find, ls, current_time");
        let code = "import json\nhits = tools.grep('old_name', 'src')\nfiles = sorted({l.split(':')[0] for l in hits.splitlines()})\nfor f in files:\n    first = tools.read(path=f, limit=1)\n    print(f, '|', first.splitlines()[0])\ntry:\n    tools.read('missing.py')\nexcept ToolError as e:\n    print('missing:', 'Can' in str(e))\nimport subprocess\nsubprocess.run(['echo', 'from a command'])\n";
        let o = run(&json!({ "description": "find old_name", "code": code }), &t, None, &list).await;
        assert!(!o.failed, "{}", o.text);
        assert_eq!(
            o.text,
            "src/a.py | def old_name():\nsrc/b.py | from a import old_name\nmissing: True\n\n[Its commands' output]\nfrom a command\n[4 tool calls (grep, read ×3; 1 failed)]"
        );
        // Not callable, wrong arguments, a failing program.
        let o = run(&json!({ "description": "x", "code": "try:\n    tools.write('x', 'y')\nexcept ToolError as e:\n    print(e)\ntools.ls('a', 'b')\n" }), &t, None, &list).await;
        assert!(o.failed && o.text.starts_with("A program can't call write: its tools are read, grep"), "{}", o.text);
        assert!(o.text.contains("ToolError: ls takes 1 arguments (path), not 2."), "{}", o.text);
        assert!(!o.text.contains("BOOT") && !o.text.contains("_call"), "the traceback is the program's own: {}", o.text);
        let o = run(&json!({ "description": "x", "code": "import time\ntime.sleep(5)" , "timeout": 1 }), &t, None, &list).await;
        assert!(o.failed && o.text.contains("stopped after 1 seconds"), "{}", o.text);
        let o = run(&json!({ "description": "x", "code": "x = 1" }), &t, None, &list).await;
        assert!(o.text.starts_with("(it printed nothing"), "{}", o.text);
        // The kernel keeps its variables; fresh starts a new one.
        let o = run(&json!({ "description": "x", "code": "print(x + 1)" }), &t, None, &list).await;
        assert!(o.text.starts_with("2\n"), "{}", o.text);
        let o = run(&json!({ "description": "x", "code": "print(x)", "fresh": true }), &t, None, &list).await;
        assert!(o.failed && o.text.contains("NameError"), "{}", o.text);
        if node() {
            let js = "const hits = tools.grep('old_name', 'src');\nvar files = [...new Set(hits.split('\\n').map((l) => l.split(':')[0]))].sort();\nconsole.log(files.join(','), tools.read({ path: files[0], limit: 1 }).split('\\n')[0]);\nrequire('child_process').spawnSync('echo', ['raw'], { stdio: 'inherit' });";
            let o = run(&json!({ "description": "x", "code": js, "language": "javascript" }), &t, None, &list).await;
            assert_eq!(o.text, "src/a.py,src/b.py def old_name():\n\n[Its commands' output]\nraw\n[2 tool calls (grep, read)]", "{}", o.text);
            let o = run(&json!({ "description": "x", "code": "console.log(files.length); tools.write({ path: 'x' })", "language": "javascript" }), &t, None, &list).await;
            assert!(o.failed && o.text.starts_with("2\n") && o.text.contains("ToolError: A program can't call write"), "{}", o.text);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn calls_from_many_threads_each_get_their_own_answer() {
        if !python() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("harlo-agent-program-threads-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..60 {
            std::fs::write(dir.join(format!("f{i}.txt")), format!("file {i}\n")).unwrap();
        }
        let t = tools_in(&dir);
        let specs = crate::tools::specs();
        let list = callable(specs.as_array().unwrap(), None);
        let code = "from concurrent.futures import ThreadPoolExecutor\ndef one(i):\n    return i, tools.read('f%d.txt' % i).strip()\nwith ThreadPoolExecutor(max_workers=12) as pool:\n    got = list(pool.map(one, range(60)))\nprint(sum(1 for i, text in got if text.endswith('file %d' % i)))\n";
        let started = std::time::Instant::now();
        let o = run(&json!({ "description": "x", "code": code, "timeout": 20 }), &t, None, &list).await;
        assert!(!o.failed, "{}", o.text);
        assert!(o.text.starts_with("60\n"), "{}", o.text);
        assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
        let _ = std::fs::remove_dir_all(dir);
    }
}
