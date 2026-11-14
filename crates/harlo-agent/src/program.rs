//! `run_code`: the model writes a short Python program that calls tools
//! (`tools.read(...)`, `tools.grep(...)`, an MCP server's tools), looks at
//! what they return, decides what to call next, and prints only what's
//! worth keeping. One round instead of several, and the tools' output stays
//! out of the conversation unless the program prints it.
//!
//! - The program runs in a fresh `python3 -I`, in the sandbox as bash
//!   commands are, so it asks first where bash does. Its prints and its
//!   commands' output come back; stdin is closed.
//! - Its calls come to harlo-agent on the program's stdout as JSON lines
//!   (its own prints are captured beside them) and are answered on its
//!   stdin. Every frame is checked: only tools that only look (ours and an
//!   MCP server's read-only ones) can be called, at most [`MAX_CALLS`].
//! - Offered when an MCP server has read-only tools (see [`offered`])
//!   and there's a python3.

use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

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
        sdk.push_str(&format!("\n- tools.{}({}): {}", s["name"].as_str().unwrap_or_default(), params.join(", "), about.trim_end_matches('.')));
    }
    json!({
        "type": "function", "name": "run_code", "strict": false,
        "description": format!(
            "Run a short Python program that calls tools and prints only what matters: for steps that depend on each other (list, then look into what the list says), or many calls whose output you'd only filter. One call instead of several rounds, and what the tools return stays out of the conversation unless you print it. \
             The program runs as a script (python3, in the working directory, the sandbox as bash's); call tools as tools.name(args) (positional or by name), each returns its text as a str (an MCP tool's as its server sends it: JSON from some, plain lines from others, so json.loads it only where it starts with {{ or [), a failed call raises ToolError with its message. Only what you print comes back, so print a short, finished answer. \
             Not for changing files (use edit and write) or one plain call. Its tools:{sdk}"
        ),
        "parameters": { "type": "object", "properties": {
            "description": { "type": "string", "description": "What it does, in a few words (shown to the user)" },
            "code": { "type": "string", "description": "The program" },
            "timeout": { "type": "integer", "description": "Seconds (120 by default, up to 600)" },
        }, "required": ["description", "code"], "additionalProperties": false },
    })
}

/// The program's side: reads its code, runs it with `tools`, sends its
/// calls and its output as JSON lines.
const BOOT: &str = r#"
import io, json, os, sys, tempfile, traceback
_out = os.fdopen(os.dup(1), "w", encoding="utf-8")
_in = os.fdopen(os.dup(0), "r", encoding="utf-8")
_null = os.open(os.devnull, os.O_RDONLY); os.dup2(_null, 0)
_cmds = tempfile.TemporaryFile()
os.dup2(_cmds.fileno(), 1); os.dup2(_cmds.fileno(), 2)
_buf = io.StringIO()
class _Cap(io.TextIOBase):
    def writable(self): return True
    def write(self, s):
        if _buf.tell() < 1000000: _buf.write(s)
        return len(s)
sys.stdout = sys.stderr = _Cap()
sys.stdin = io.StringIO("")
class ToolError(Exception):
    pass
def _send(frame):
    _out.write(json.dumps(frame) + "\n"); _out.flush()
def _call(name, args, kwargs):
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
code = json.loads(_in.readline())["code"]
error = None
try:
    exec(compile(code, "<program>", "exec"), {"__name__": "__main__", "tools": _Tools(), "ToolError": ToolError})
except SystemExit as e:
    if e.code not in (None, 0):
        error = "SystemExit: %s" % (e.code,)
except BaseException as e:
    # (The program's own lines only.)
    import linecache
    linecache.cache["<program>"] = (len(code), None, code.splitlines(True), "<program>")
    frames = [f for f in traceback.extract_tb(e.__traceback__) if f.filename == "<program>"]
    error = "Traceback (most recent call last):\n" + "".join(traceback.format_list(frames)) + "".join(traceback.format_exception_only(type(e), e))
sys.stdout.flush()
_cmds.seek(0)
cmds = _cmds.read(200000).decode("utf-8", "replace")
_send({"t": "done", "output": _buf.getvalue(), "commands": cmds, "error": error})
"#;

/// Runs a `run_code` call: its program, its calls answered (`callable`:
/// what it may call).
pub async fn run(args: &Value, tools: &Tools, mcp: Option<&crate::mcp::Mcp>, callable: &[&Value]) -> Outcome {
    let fail = |t: String| Outcome::new(t, true);
    let code = args["code"].as_str().unwrap_or_default();
    if code.trim().is_empty() {
        return fail("Give the program (code).".into());
    }
    let secs = args["timeout"].as_u64().filter(|t| *t > 0).unwrap_or(DEFAULT_TIMEOUT).min(MAX_TIMEOUT);
    let sandbox = tools.sandbox.as_deref();
    if let Some(why) = sandbox.and_then(|sb| sb.broken.as_ref()) {
        return fail(format!("Programs can't run: the sandbox can't start here ({why})."));
    }
    let mut cmd = crate::tools::bash::command("exec python3 -I -c \"$HARLO_AGENT_PROGRAM\"", &tools.cwd, sandbox);
    cmd.env("HARLO_AGENT_PROGRAM", BOOT).stdin(Stdio::piped()).kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return fail(format!("Couldn't start python3: {e}")),
    };
    let _group = crate::tools::bash::Group::of(&child);
    let (Some(mut stdin), Some(stdout), Some(mut stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
        return fail("Couldn't talk to python3.".into());
    };
    let mut calls: Vec<(String, bool)> = Vec::new();
    let work = async {
        let first = json!({ "code": code }).to_string() + "\n";
        stdin.write_all(first.as_bytes()).await.map_err(|e| format!("Couldn't send the program: {e}"))?;
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(frame) = serde_json::from_str::<Value>(&line) else { continue };
            match frame["t"].as_str() {
                Some("call") => {
                    let name = frame["name"].as_str().unwrap_or_default().to_owned();
                    let reply = if calls.len() >= MAX_CALLS {
                        Outcome::new(format!("The program made more than {MAX_CALLS} calls: no more are answered."), true)
                    } else {
                        match callable.iter().find(|s| s["name"] == name.as_str()) {
                            None => Outcome::new(format!("A program can't call {name}: its tools are {}.", names(callable)), true),
                            Some(spec) => match arguments(spec, &frame) {
                                Err(why) => Outcome::new(why, true),
                                Ok(a) => match mcp.filter(|m| m.has(&name)) {
                                    Some(m) => m.call_whole(&name, &a).await,
                                    None => tools.call(&name, &a).await,
                                },
                            },
                        }
                    };
                    calls.push((name, reply.failed));
                    let answer = json!({ "text": reply.text, "failed": reply.failed }).to_string() + "\n";
                    if stdin.write_all(answer.as_bytes()).await.is_err() {
                        break;
                    }
                }
                Some("done") => return Ok(frame),
                _ => {}
            }
        }
        Err("The program ended without finishing (python3 stopped).".to_owned())
    };
    let ended = tokio::time::timeout(Duration::from_secs(secs), work).await;
    let mut err_text = String::new();
    let _ = tokio::time::timeout(Duration::from_millis(200), stderr.read_to_string(&mut err_text)).await;
    let _ = child.start_kill();
    let frame = match ended {
        Err(_) => return fail(format!("The program was stopped after {secs} seconds{}. Give it a larger timeout, or do less in one program.", made(&calls))),
        Ok(Err(why)) => return fail(format!("{why}{}", if err_text.trim().is_empty() { String::new() } else { format!("\n{}", tail(&err_text, 20)) })),
        Ok(Ok(f)) => f,
    };
    let mut text = frame["output"].as_str().unwrap_or_default().to_owned();
    let commands = frame["commands"].as_str().unwrap_or_default();
    if !commands.trim().is_empty() {
        text.push_str(&format!("\n[Its commands' output]\n{commands}"));
    }
    let error = frame["error"].as_str().filter(|e| !e.trim().is_empty());
    if let Some(e) = error {
        text.push_str(&format!("\n[The program failed]\n{}", tail(e, 30)));
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

/// A call's arguments as the tool takes them: positional ones in the
/// tool's order, then those by name.
fn arguments(spec: &Value, frame: &Value) -> Result<Value, String> {
    let order = order(spec);
    let mut out = serde_json::Map::new();
    let positional = frame["args"].as_array().cloned().unwrap_or_default();
    if positional.len() > order.len() {
        return Err(format!("{} takes {} arguments ({}), not {}.", spec["name"].as_str().unwrap_or_default(), order.len(), order.join(", "), positional.len()));
    }
    for (p, v) in order.iter().zip(positional) {
        out.insert(p.clone(), v);
    }
    for (k, v) in frame["kwargs"].as_object().into_iter().flatten() {
        if !order.contains(k) {
            return Err(format!("{} has no argument {k}: it takes {}.", spec["name"].as_str().unwrap_or_default(), order.join(", ")));
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
        let _ = std::fs::remove_dir_all(dir);
    }
}
