//! harlo's tools for agents.
//!
//! harlo's tools reach an agent as an MCP server: `harlo --mcp-bridge`, a
//! stdio server the agent starts like any other. It answers the protocol
//! itself and hands each tool call to the running harlo over a local
//! socket, saying who's calling: `HARLO_SESSION` (set in every agent harlo
//! starts). With no harlo running it offers no tools, so an
//! agent started elsewhere sees nothing amiss. Agents start MCP servers with
//! few environment variables, so each entry names the socket (`--socket`)
//! and sets `HARLO_SESSION` itself (or as `${HARLO_SESSION}`).

// Rust guideline compliant 2026-10-09

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use serde_json::{Value, json};

/// The socket the running harlo listens on (its data folder is part of the
/// name, so a test's harlo has its own).
pub fn socket_path() -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    crate::db::default_path().hash(&mut h);
    let name = format!("harlo-mcp-{:012x}.sock", h.finish() & 0xffff_ffff_ffff);
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    // A socket's path has to be short (about 100 bytes).
    match dir.join(&name) {
        path if path.as_os_str().len() < 100 => path,
        _ => PathBuf::from("/tmp").join(name),
    }
}

/// The tools, as MCP lists them.
pub fn tools() -> Value {
    let s = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    json!([
        {
            "name": "list_sessions",
            "description": "harlo's open sessions: id, title, agent, status (busy / idle / waiting for input), folder, who started it. With query: past sessions (your folder's, or everywhere) whose messages match, with the passages.",
            "inputSchema": s(json!({
                "query": { "type": "string", "description": "Words to find in past sessions' messages." },
                "everywhere": { "type": "boolean", "description": "Every folder's sessions, not just yours." }
            }), &[]),
        },
        {
            "name": "read_session",
            "description": "A harlo session's status and last reply (or its latest messages); past ones too (by @session id).",
            "inputSchema": s(json!({
                "session": { "type": "string" },
                "messages": { "type": "integer", "description": "How many latest messages (default 1: the last reply)." }
            }), &["session"]),
        },
        {
            "name": "request_approval",
            "description": "Asks the user to decide something only they may (a risky or irreversible step, or a choice that's theirs). harlo shows it in your session and on their phone and notifies them; no agent can answer it. Then end your turn: their answer comes as a message.",
            "inputSchema": s(json!({
                "question": { "type": "string", "description": "The decision, with what they need to know." },
                "options": { "type": "array", "items": { "type": "string" }, "description": "Their choices (default: Approve, Reject)." }
            }), &["question"]),
        }
    ])
}

/// The tools of a session harlo started for a job (a workflow's step, a
/// wiki run), besides [`tools`]: its report and its questions go back.
pub fn helper_tools() -> Value {
    let s = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    json!([
        {
            "name": "report_done",
            "description": "Reports your task's result to harlo: once, when you're done (or can't go on), then end your turn.",
            "inputSchema": s(json!({
                "outcome": { "type": "string", "enum": ["succeeded", "failed"], "description": "failed if the task isn't done: never hide a failure in the summary." },
                "summary": { "type": "string", "description": "Three sentences: what you did, what you found, what's left." },
                "files": { "type": "array", "items": { "type": "string" }, "description": "The files you changed (paths), if any." },
                "attempt": { "type": "integer", "description": "The attempt your message named (\"attempt: N\"), when it named one: a report for an earlier attempt isn't used." }
            }), &["outcome", "summary"]),
        },
        {
            "name": "ask_parent",
            "description": "Asks a question the task doesn't settle. Then end your turn: harlo wakes you with its answer. (For what only the user may decide, ask the user instead.)",
            "inputSchema": s(json!({ "question": { "type": "string" } }), &["question"]),
        }
    ])
}

/// A session's report (`report_done`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub succeeded: bool,
    pub summary: String,
    pub files: Vec<String>,
    /// The attempt it reports, when its message named one (a workflow's).
    pub attempt: Option<u32>,
}

impl Report {
    /// From `report_done`'s arguments.
    ///
    /// # Errors
    ///
    /// When the outcome isn't succeeded or failed, or the summary is missing.
    pub fn parse(args: &Value) -> anyhow::Result<Report> {
        let succeeded = match args["outcome"].as_str().map(str::trim) {
            Some("succeeded") => true,
            Some("failed") => false,
            _ => return Err(anyhow::Error::msg("Give an outcome: succeeded or failed.")),
        };
        let summary = args["summary"].as_str().map(str::trim).unwrap_or_default().to_owned();
        if summary.is_empty() {
            return Err(anyhow::Error::msg("Give a summary: what you did, what you found, what's left."));
        }
        let files = args["files"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::trim)).filter(|f| !f.is_empty()).map(str::to_owned).collect();
        let attempt = args["attempt"].as_u64().or_else(|| args["attempt"].as_str().and_then(|a| a.trim().parse().ok())).and_then(|a| u32::try_from(a).ok());
        Ok(Report { succeeded, summary, files, attempt })
    }
}

/// harlo's message to an agent, in a line, as the user reads it in the chat.
///
/// Without its "[harlo] " prefix: what happened, without the session keys and
/// tool names the agent needs. The full text stays one click away. Unknown
/// shapes come back as their first line.
pub fn said_plainly(said: &str) -> String {
    let said = said.trim();
    let first = |t: &str| -> String {
        let line = t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
        match line.char_indices().nth(160) {
            Some((i, _)) => format!("{}…", line[..i].trim_end()),
            None => line.to_owned(),
        }
    };
    if let Some(r) = said.strip_prefix("The user answered your approval request (\"")
        && let Some(end) = r.find("\"): ") {
            return format!("You answered \"{}\": {}", &r[..end], first(&r[end + 4..]));
        }
    if let Some(r) = said.strip_prefix("The user dismissed your approval request (\"")
        && let Some(end) = r.find("\")") {
            return format!("You dismissed \"{}\".", &r[..end]);
        }
    // A wiki run harlo started (wiki::run_prompt).
    if let Some(line) = crate::wiki::prompt_summary(said) {
        return line.into();
    }
    first(said)
}

/// What a session is told when the user answered its approval request.
pub fn approval_answer(question: &str, answer: &Value) -> String {
    let said = answer["value"].as_str().map(str::trim).filter(|v| !v.is_empty());
    let question = question.trim();
    match said {
        Some(choice) => format!("[harlo] The user answered your approval request (\"{question}\"): {choice}"),
        None => format!("[harlo] The user dismissed your approval request (\"{question}\") without choosing. Don't go ahead with it; ask them, or carry on without it."),
    }
}

/// What the bridge asks to learn whether harlo's tools are on.
pub const ENABLED: &str = "_enabled";
/// What the bridge asks for a session's tools.
pub const TOOLS: &str = "_tools";
/// What the bridge asks for what a session's tools are for.
pub const INSTRUCTIONS: &str = "_instructions";

/// What harlo's tools are for, before a project's wiki.
pub const BASE_INSTRUCTIONS: &str = "harlo's sessions: read past sessions' messages, and ask the user to decide what only they may.";

/// What the bridge sends harlo for a tool call (one line), and harlo's
/// answer (one line): `{ok, result}` or `{ok: false, error}`.
pub fn call_line(session: &str, tool: &str, args: &Value) -> String {
    json!({ "session": session, "tool": tool, "args": args }).to_string()
}

/// harlo draws Mermaid: told to every session with harlo's tools.
pub const DIAGRAMS: &str = crate::visuals::INSTRUCTIONS;

/// The socket the bridge reaches harlo on: `--socket <path>` (agents start
/// MCP servers with few environment variables), else `socket_path()`.
fn bridge_socket() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == "--socket").and_then(|i| args.get(i + 1)).map_or_else(socket_path, PathBuf::from)
}

fn ask_harlo(line: &str) -> Option<Value> {
    let mut stream = std::os::unix::net::UnixStream::connect(bridge_socket()).ok()?;
    stream.write_all(line.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).ok()?;
    serde_json::from_str(&answer).ok()
}

/// `harlo --mcp-bridge`: an MCP server on stdin / stdout.
pub fn run_bridge() {
    let session = std::env::var("HARLO_SESSION").unwrap_or_default();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        // Notifications get no answer.
        let Some(id) = msg.get("id").cloned() else { continue };
        let reply = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
        let answer = match msg["method"].as_str().unwrap_or_default() {
            // What this session's tools are for (the project's wiki adds
            // to it).
            "initialize" => {
                let said = ask_harlo(&call_line(&session, INSTRUCTIONS, &json!({}))).filter(|a| a["ok"] == true);
                reply(json!({
                    "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "harlo", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": said.and_then(|a| a["result"].as_str().map(str::to_owned)).unwrap_or_else(|| BASE_INSTRUCTIONS.to_owned()),
                }))
            }
            "ping" => reply(json!({})),
            // This session's tools, as harlo says; none with no harlo
            // running or orchestration off in it. (A socket left by a
            // harlo that crashed doesn't count.)
            "tools/list" => {
                let listed = ask_harlo(&call_line(&session, TOOLS, &json!({}))).filter(|a| a["ok"] == true);
                reply(json!({ "tools": listed.map(|a| a["result"].clone()).filter(Value::is_array).unwrap_or_else(|| json!([])) }))
            }
            "tools/call" => {
                let p = &msg["params"];
                let tool = p["name"].as_str().unwrap_or_default();
                let answer = ask_harlo(&call_line(&session, tool, &p["arguments"]));
                let (text, error) = match answer {
                    // (Text as text: a card reads as it is, not as a quoted
                    // JSON string with its line breaks escaped.)
                    Some(a) if a["ok"] == true => (a["result"].as_str().map_or_else(|| a["result"].to_string(), str::to_owned), false),
                    Some(a) => (a["error"].as_str().unwrap_or("harlo couldn't do that").to_owned(), true),
                    None => ("harlo isn't running.".to_owned(), true),
                };
                reply(json!({ "content": [{ "type": "text", "text": text }], "isError": error }))
            }
            _ => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "unknown method" } }),
        };
        if writeln!(out, "{answer}").and_then(|()| out.flush()).is_err() {
            return;
        }
    }
}

// The skills earlier versions installed (Settings → Teamwork), each
// folder marked as harlo's: removed, since the tools and the Delegate
// button carry what they said.
const OLD_SKILLS: [&str; 4] = ["harlo", "harlo-handoff", "harlo-advisor", "harlo-committee"];
const SKILL_MARK: &str = ".harlo-skill";

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn skill_dirs() -> [PathBuf; 2] {
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR").map_or_else(|| home().join(".claude"), PathBuf::from);
    [home().join(".agents/skills"), claude.join("skills")]
}

/// Removes the skills harlo installed here; a folder of the same name
/// that harlo didn't make (no mark) is left alone.
pub fn remove_old_skills() {
    for dir in skill_dirs().iter().flat_map(|d| OLD_SKILLS.map(|n| d.join(n))).filter(|d| d.join(SKILL_MARK).is_file()) {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => tracing::event!(name: "orchestration.skill.removed", tracing::Level::INFO, file.path = %dir.display(), "orchestration: removed harlo's old skill {{file.path}}"),
            Err(e) => tracing::event!(name: "orchestration.skill.remove.failed", tracing::Level::WARN, file.path = %dir.display(), error.message = %e, "orchestration: removing {{file.path}}: {{error.message}}"),
        }
    }
}

// Remote hosts: their agents reach this harlo through its socket, forwarded
// to the host over the SSH connection (`ssh -O forward -R`), and a small
// Python bridge there (the MCP server they start).

/// The bridge's program, uploaded to each host.
const BRIDGE_PY: &str = include_str!("../bridge/harlo_mcp_bridge.py");

/// This harlo, in the names of what it puts on hosts (two harlos, two
/// sockets).
fn local_id() -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    socket_path().hash(&mut h);
    format!("{:012x}", h.finish() & 0xffff_ffff_ffff)
}

/// Where the bridge and the socket go on a host: (script, socket), as the
/// host sees them.
pub fn host_bridge_paths(name: &str) -> (String, String) {
    // (HARLO_TEST_REMOTE_CACHE: the click tests' scratch folder, their
    // "host" being this machine.)
    let dir = match std::env::var("HARLO_TEST_REMOTE_CACHE") {
        Ok(d) if !d.is_empty() => d,
        _ => format!("{}/.cache/harlo", harlo_harness::on_host(&crate::mcp::host_home(name)).trim_end_matches('/')),
    };
    let sock = format!("{dir}/harlo-{}.sock", local_id());
    // A socket's path has to be short (about 100 bytes): else /tmp's (its
    // name is this harlo's, and only its owner may use it).
    let sock = if sock.len() < 100 { sock } else { format!("/tmp/harlo-{}.sock", local_id()) };
    (format!("{dir}/harlo_mcp_bridge.py"), sock)
}

/// What an agent on the host runs for harlo's tools: `python3 <bridge>
/// <socket>`.
///
/// The bridge is put there and the socket forwarded: done before each session
/// starts there (an SSH connection that ended took its forward with it).
///
/// # Errors
///
/// When the host can't be reached, the bridge can't be written there, or the
/// socket can't be forwarded.
pub fn remote_bridge(name: &str) -> anyhow::Result<Vec<String>> {
    use anyhow::{Context, bail};
    let host = crate::remote::host(name).with_context(|| format!("unknown remote host {name}"))?;
    let (script, sock) = host_bridge_paths(name);
    let q = crate::remote::quote;
    let args = crate::remote::ssh_args(&host);
    let (options, destination) = args.split_at(args.len().saturating_sub(1));
    let spec = format!("{sock}:{}", socket_path().display());
    let control = |op: &str| {
        let mut c = std::process::Command::new("ssh");
        c.args(options).args(["-O", op, "-R", &spec]).args(destination).stdin(std::process::Stdio::null());
        c.output()
    };
    // An earlier forward of this socket (a connection asks only once for
    // the same one): cancelled, so the new one really binds.
    if crate::remote::connected(&host) {
        let _ = control("cancel");
    }
    let dir = std::path::Path::new(&script).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    // The bridge (always the current one), the folder private, an old
    // socket gone (or the forward can't bind), and Python there.
    let setup = format!(
        "mkdir -p {d} && chmod 700 {d} && cat > {s}.tmp && mv -f {s}.tmp {s} && rm -f {k} && {{ {old}; }} && command -v python3",
        d = q(&dir),
        s = q(&script),
        k = q(&sock),
        old = remove_old_skills_on(name)
    );
    let out = crate::remote::run_with_input(&host, &setup, Some(BRIDGE_PY.as_bytes()), std::time::Duration::from_secs(30))?;
    if !out.status.success() {
        bail!("{name} has no python3, which harlo's tools need there");
    }
    // The forward, on the shared connection (open now: the setup used it).
    let out = control("forward").context("running ssh")?;
    if !out.status.success() {
        bail!("couldn't forward harlo's socket to {name}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(vec!["python3".into(), script, sock])
}

/// Removes the skills harlo installed on a host: a shell command, run
/// with the bridge's setup.
fn remove_old_skills_on(name: &str) -> String {
    let home = harlo_harness::on_host(&crate::mcp::host_home(name));
    let home = home.trim_end_matches('/');
    let dirs: Vec<String> = ["/.agents/skills/", "/.claude/skills/"].iter().flat_map(|d| OLD_SKILLS.map(|n| crate::remote::quote(&format!("{home}{d}{n}")))).collect();
    format!("for d in {}; do [ -f \"$d/{SKILL_MARK}\" ] && rm -rf \"$d\"; done; true", dirs.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harlos_messages_read_plainly() {
        assert_eq!(said_plainly("The user answered your approval request (\"Merge?\"): Approve"), "You answered \"Merge?\": Approve");
        assert_eq!(said_plainly("The user dismissed your approval request (\"Merge?\") without choosing."), "You dismissed \"Merge?\".");
        assert_eq!(said_plainly("Something else\nmore"), "Something else");
    }

    #[test]
    fn sessions_report_back() {
        // A report: its outcome, summary and files.
        let r = Report::parse(&json!({ "outcome": "failed", "summary": " Tried. ", "files": ["a.rs", " ", "b.rs"] })).unwrap();
        assert_eq!(r, Report { succeeded: false, summary: "Tried.".into(), files: vec!["a.rs".into(), "b.rs".into()], attempt: None });
        // A workflow's: the attempt it reports (a number, or a numeral).
        assert_eq!(Report::parse(&json!({ "outcome": "succeeded", "summary": "x", "attempt": 2 })).unwrap().attempt, Some(2));
        assert_eq!(Report::parse(&json!({ "outcome": "succeeded", "summary": "x", "attempt": " 3" })).unwrap().attempt, Some(3));
        Report::parse(&json!({ "outcome": "done", "summary": "x" })).unwrap_err();
        Report::parse(&json!({ "outcome": "succeeded", "summary": "  " })).unwrap_err();
        assert!(approval_answer("Merge?", &json!({ "value": "Approve" })).ends_with("(\"Merge?\"): Approve"));
        assert!(approval_answer("Merge?", &json!({ "cancelled": true })).contains("dismissed"));
        // Reading sessions and asking the user: nothing starts or steers another session.
        let names: Vec<String> = tools().as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect();
        assert_eq!(names, ["list_sessions", "read_session", "request_approval"]);
        let helper: Vec<String> = helper_tools().as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect();
        assert_eq!(helper, ["report_done", "ask_parent"]);
    }
}
