//! Orchestration: agents drive harlo.
//!
//! harlo's tools reach an agent as an MCP server: `harlo --mcp-bridge`, a
//! stdio server the agent starts like any other. It answers the protocol
//! itself and hands each tool call to the running harlo over a local
//! socket, saying who's calling: `HARLO_SESSION` (set in every agent harlo
//! starts), or `HARLO_OPENCODE` (OpenCode's shared server, whose running
//! session is the caller). With no harlo running it offers no tools, so an
//! agent started elsewhere sees nothing amiss. Agents start MCP servers with
//! few environment variables, so each entry names the socket (`--socket`)
//! and sets `HARLO_SESSION` itself (or as `${HARLO_SESSION}`).

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
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
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
            "name": "list_agents",
            "description": "The coding agents harlo can start sessions with (claude, opencode, pi, omp): their ids and whether they're installed.",
            "inputSchema": s(json!({}), &[]),
        },
        {
            "name": "list_sessions",
            "description": "harlo's open sessions: each one's id, title, agent, status (busy / idle / waiting for input), folder, and which session started it.",
            "inputSchema": s(json!({}), &[]),
        },
        {
            "name": "start_session",
            "description": "Starts a new harlo session with a coding agent and sends it a first prompt. It becomes your subagent: the user sees it, and when it finishes its last reply is sent back to you (unless notify is false). The new agent has none of your context: make the prompt a self-contained briefing.",
            "inputSchema": s(json!({
                "agent": { "type": "string", "description": "claude, opencode, pi or omp (see list_agents)." },
                "prompt": { "type": "string", "description": "The first message: the task, its context, the relevant files, what's done and what's left." },
                "title": { "type": "string", "description": "A short name for the session." },
                "model": { "type": "string", "description": "provider/model, if not the agent's default." },
                "folder": { "type": "string", "description": "Where it works (default: your folder)." },
                "worktree": { "type": "string", "description": "A new git worktree on this branch name to work in (isolated from your checkout)." },
                "notify": { "type": "boolean", "description": "Send its last reply back to you when it finishes (default true)." }
            }), &["agent", "prompt"]),
        },
        {
            "name": "send_message",
            "description": "Sends a follow-up message to a harlo session (queued if it's busy).",
            "inputSchema": s(json!({
                "session": { "type": "string", "description": "The session's id (from start_session or list_sessions)." },
                "message": { "type": "string" }
            }), &["session", "message"]),
        },
        {
            "name": "read_session",
            "description": "A harlo session's status and its last reply (or the last few messages).",
            "inputSchema": s(json!({
                "session": { "type": "string" },
                "messages": { "type": "integer", "description": "How many of its latest messages to return (default 1: the last reply)." }
            }), &["session"]),
        },
        {
            "name": "stop_session",
            "description": "Stops what a harlo session is doing (it stays open; send_message continues it).",
            "inputSchema": s(json!({ "session": { "type": "string" } }), &["session"]),
        }
    ])
}

/// What the bridge asks to learn whether orchestration is on.
pub const ENABLED: &str = "_enabled";

/// What the bridge sends harlo for a tool call (one line), and harlo's
/// answer (one line): `{ok, result}` or `{ok: false, error}`.
pub fn call_line(session: &str, opencode: bool, tool: &str, args: &Value) -> String {
    json!({ "session": session, "opencode": opencode, "tool": tool, "args": args }).to_string()
}

/// The socket the bridge reaches harlo on: `--socket <path>` (agents start
/// MCP servers with few environment variables), else `socket_path()`.
fn bridge_socket() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == "--socket").and_then(|i| args.get(i + 1)).map(PathBuf::from).unwrap_or_else(socket_path)
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
    let opencode = std::env::var_os("HARLO_OPENCODE").is_some();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        // Notifications get no answer.
        let Some(id) = msg.get("id").cloned() else { continue };
        let reply = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
        let answer = match msg["method"].as_str().unwrap_or_default() {
            "initialize" => reply(json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "harlo", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "harlo's sessions: start other coding agents, send them messages, read their replies.",
            })),
            "ping" => reply(json!({})),
            // No harlo running, or orchestration off in it: no tools.
            // (A socket left by a harlo that crashed doesn't count.)
            "tools/list" => {
                let on = ask_harlo(&call_line(&session, opencode, ENABLED, &json!({}))).is_some_and(|a| a["ok"] == true);
                reply(json!({ "tools": if on { tools() } else { json!([]) } }))
            }
            "tools/call" => {
                let p = &msg["params"];
                let tool = p["name"].as_str().unwrap_or_default();
                let answer = ask_harlo(&call_line(&session, opencode, tool, &p["arguments"]));
                let (text, error) = match answer {
                    Some(a) if a["ok"] == true => (a["result"].to_string(), false),
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

// Settings → Orchestration: the skills that teach agents to use the tools,
// and the bridge in pi's and omp's MCP config. (Claude and
// OpenCode get the bridge with each session harlo starts.)

/// A skill harlo installs.
pub struct Skill {
    pub name: &'static str,
    /// For the settings page.
    pub summary: &'static str,
    text: &'static str,
}

pub const SKILLS: [Skill; 4] = [
    Skill { name: "harlo", summary: "The tools: start, prompt, read and stop other agents' sessions.", text: include_str!("../skills/harlo/SKILL.md") },
    Skill { name: "harlo-handoff", summary: "Hand the task to another agent, with a full briefing.", text: include_str!("../skills/harlo-handoff/SKILL.md") },
    Skill { name: "harlo-advisor", summary: "A second opinion from another agent (it doesn't edit).", text: include_str!("../skills/harlo-advisor/SKILL.md") },
    Skill { name: "harlo-committee", summary: "Two agents from different families agree on a plan.", text: include_str!("../skills/harlo-committee/SKILL.md") },
];

/// In each folder harlo installed a skill to (and so may update or remove).
const SKILL_MARK: &str = ".harlo-skill";

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// Where agents find skills: `~/.agents/skills` (pi, omp, OpenCode) and
/// Claude's (Claude, omp, OpenCode).
pub fn skill_dirs() -> [PathBuf; 2] {
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".claude"));
    [home().join(".agents/skills"), claude.join("skills")]
}

fn ours(dir: &std::path::Path) -> bool {
    dir.join(SKILL_MARK).is_file()
}

/// The skills harlo has installed.
pub fn installed_skills() -> Vec<&'static str> {
    SKILLS.iter().filter(|s| skill_dirs().iter().any(|d| ours(&d.join(s.name)))).map(|s| s.name).collect()
}

fn write_skill(dir: &std::path::Path, skill: &Skill) -> anyhow::Result<()> {
    use anyhow::Context;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let file = dir.join("SKILL.md");
    if std::fs::read_to_string(&file).ok().as_deref() != Some(skill.text) {
        std::fs::write(&file, skill.text).with_context(|| format!("writing {}", file.display()))?;
    }
    std::fs::write(dir.join(SKILL_MARK), "harlo installed this skill, and updates or removes this folder (Settings → Orchestration).\n")?;
    Ok(())
}

/// Installs or removes a skill. A folder of that name that harlo didn't
/// make is left alone.
pub fn set_skill(name: &str, on: bool) -> anyhow::Result<()> {
    let skill = SKILLS.iter().find(|s| s.name == name).ok_or_else(|| anyhow::anyhow!("There's no skill {name}."))?;
    let dirs = skill_dirs().map(|d| d.join(name));
    if on {
        if let Some(theirs) = dirs.iter().find(|d| d.exists() && !ours(d)) {
            anyhow::bail!("{} is another skill of the same name; harlo left it alone.", crate::sidebar::tilde(theirs));
        }
        for dir in &dirs {
            write_skill(dir, skill)?;
        }
    } else {
        for dir in dirs.iter().filter(|d| ours(d)) {
            std::fs::remove_dir_all(dir).map_err(|e| anyhow::anyhow!("removing {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

/// Brings the installed skills up to this harlo's version.
pub fn refresh_skills() {
    for skill in &SKILLS {
        for dir in skill_dirs().map(|d| d.join(skill.name)).iter().filter(|d| ours(d)) {
            if let Err(e) = write_skill(dir, skill) {
                tracing::warn!("orchestration: updating {}: {e:#}", dir.display());
            }
        }
    }
}

/// The agents whose MCP config gets the bridge from Settings.
pub const BRIDGE_AGENTS: [harlo_harness::HarnessKind; 2] = [harlo_harness::HarnessKind::Pi, harlo_harness::HarnessKind::Omp];

/// The bridge's entry: this harlo, its socket. (pi and omp hand their own
/// variables, `HARLO_SESSION` among them, to the servers they start.)
fn bridge_form(harlo: &std::path::Path) -> crate::mcp::Form {
    crate::mcp::Form {
        name: "harlo".into(),
        enabled: true,
        transport: "stdio".into(),
        command: vec![harlo.to_string_lossy().into_owned(), "--mcp-bridge".into(), "--socket".into(), socket_path().to_string_lossy().into_owned()],
        exposure: "direct".into(),
        description: "harlo's sessions: start other coding agents, send them messages, read their replies.".into(),
        ..Default::default()
    }
}

/// An entry is the bridge, here or a host's (and not some other server
/// called harlo).
fn is_bridge(entry: &Value) -> bool {
    entry["args"].as_array().is_some_and(|a| a.iter().any(|x| x == "--mcp-bridge" || x.as_str().is_some_and(|s| s.ends_with("/harlo_mcp_bridge.py"))))
}

/// The agent has the bridge.
pub fn bridge_installed(kind: harlo_harness::HarnessKind) -> bool {
    crate::mcp::entry(kind, &crate::mcp::Scope::Global, "harlo").ok().flatten().is_some_and(|e| is_bridge(&e))
}

/// Adds the bridge to an agent's MCP config, or removes it.
pub fn set_bridge(kind: harlo_harness::HarnessKind, on: bool, harlo: &std::path::Path) -> anyhow::Result<()> {
    let scope = crate::mcp::Scope::Global;
    let existing = crate::mcp::entry(kind, &scope, "harlo")?;
    if existing.as_ref().is_some_and(|e| !is_bridge(e)) {
        anyhow::bail!("{} already has an MCP server called harlo; harlo left it alone.", kind.label());
    }
    match on {
        true => crate::mcp::put_entry(kind, &scope, Some(&bridge_form(harlo)), "harlo"),
        false if existing.is_some() => crate::mcp::put_entry(kind, &scope, None, "harlo"),
        false => Ok(()),
    }
}

/// Points the installed bridges at this harlo (it may have moved).
pub fn refresh_bridges(harlo: &std::path::Path) {
    for kind in BRIDGE_AGENTS.into_iter().filter(|k| bridge_installed(*k)) {
        if let Err(e) = crate::mcp::put_entry(kind, &crate::mcp::Scope::Global, Some(&bridge_form(harlo)), "harlo") {
            tracing::warn!("orchestration: updating {}'s bridge: {e:#}", kind.label());
        }
    }
}

/// An agent to delegate to (the composer's Delegate menu).
pub struct Member {
    /// "OpenCode"
    pub label: String,
    /// "opencode"
    pub agent: String,
    /// "" for its default.
    pub model: String,
}

impl Member {
    fn describe(&self) -> String {
        match self.model.trim() {
            "" => format!("{} (agent \"{}\", its default model)", self.label, self.agent),
            m => format!("{} (agent \"{}\", model \"{m}\")", self.label, self.agent),
        }
    }
}

const NO_EDITS: &str = "\"This is analysis only. Do NOT edit, create, or delete any files. Do NOT write code.\"";

/// What the Delegate menu sends the session's agent: the skill's
/// instructions in a few lines, so nothing has to be installed. `action`:
/// handoff, advisor or committee (two members).
pub fn delegate_prompt(action: &str, members: &[Member], note: &str, worktree: bool) -> Option<String> {
    let note = note.trim();
    let text = match (action, members) {
        ("handoff", [m, ..]) => {
            let place = if worktree { ", in a new git worktree (pass a short branch name taken from the task as `worktree`)" } else { "" };
            let mut t = format!(
                "Hand this task off to a new session: {}, started with harlo's start_session tool{place}.\n\
                 It has none of our context, so make its prompt a self-contained briefing: the task, why it exists, the relevant files (by path), what's done and what isn't, what was tried and why it failed, the decisions made, what \"done\" looks like, and the constraints. Keep my intent exactly: an investigation stays an investigation.\n\
                 Title it \"[Handoff] <the task>\". Don't wait for it: tell me it's started, and stop.",
                m.describe()
            );
            if !note.is_empty() {
                t.push_str(&format!("\n\nAlso: {note}"));
            }
            t
        }
        ("advisor", [m, ..]) => format!(
            "Get a second opinion from {}: start it with harlo's start_session tool, titled \"[Advisor] <the topic>\".\n\
             It has none of our context, so make its prompt a self-contained briefing: the question, what you've considered and ruled out, the relevant files (by path, for it to read), and the ask: \"Give me a recommendation, with your reasoning.\" End the prompt with: {NO_EDITS}\n\
             Then end your turn. Its reply comes back to you as a [harlo] message; don't poll for it. Then give me its verdict and your own recommendation.\n\n\
             The question: {}",
            m.describe(),
            if note.is_empty() { "the approach we're taking on the current task." } else { note }
        ),
        ("committee", [a, b, ..]) => format!(
            "Form a committee to step back, find the root cause and agree on a plan. With harlo's start_session tool, start two sessions with the same prompt, each titled \"[Committee] <the task>\":\n\
             - {}\n\
             - {}\n\
             The prompt is about the problem, not your theory: the goal, the symptoms, what's been tried, the relevant files (by path), and the question \"What's the root cause, and what's the plan?\" End it, and every later message to them, with: {NO_EDITS}\n\
             Then end your turn. Their replies come back to you as [harlo] messages; don't poll or hurry them. Where they disagree, send each the other's arguments with send_message, until they agree. Then give me the agreed plan, where they disagreed, and how that was settled.\n\n\
             The problem: {}",
            a.describe(),
            b.describe(),
            if note.is_empty() { "what we're stuck on in the current task." } else { note }
        ),
        _ => return None,
    };
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(label: &str, agent: &str, model: &str) -> Member {
        Member { label: label.into(), agent: agent.into(), model: model.into() }
    }

    #[test]
    fn delegate_prompts() {
        let h = delegate_prompt("handoff", &[m("Claude", "claude", "")], "  keep the API  ", true).unwrap();
        assert!(h.contains("Claude (agent \"claude\", its default model)") && h.contains("as `worktree`") && h.ends_with("Also: keep the API"));
        let a = delegate_prompt("advisor", &[m("OpenCode", "opencode", "openai/gpt-5")], "", false).unwrap();
        assert!(a.contains("model \"openai/gpt-5\"") && a.contains("Do NOT write code.") && a.ends_with("the current task."));
        let c = delegate_prompt("committee", &[m("Claude", "claude", ""), m("pi", "pi", "google/gemini")], "flaky tests", false).unwrap();
        assert!(c.contains("- Claude (agent") && c.contains("- pi (agent \"pi\", model \"google/gemini\")") && c.ends_with("The problem: flaky tests"));
        // A committee needs two.
        assert!(delegate_prompt("committee", &[m("Claude", "claude", "")], "", false).is_none());
        assert!(delegate_prompt("nope", &[m("Claude", "claude", "")], "", false).is_none());
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

/// What an agent on the host runs for harlo's tools (`python3 <bridge>
/// <socket>`), with the bridge put there and the socket forwarded: done
/// before each session starts there (an SSH connection that ended took
/// its forward with it).
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
        "mkdir -p {d} && chmod 700 {d} && cat > {s}.tmp && mv -f {s}.tmp {s} && rm -f {k} && command -v python3",
        d = q(&dir),
        s = q(&script),
        k = q(&sock)
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

// The same on a remote host (Settings → Orchestration with a host as the
// instance): its skill folders and its pi / omp config, over SSH.

/// A host's skill folders, as it sees them.
fn host_skill_dirs(name: &str) -> [String; 2] {
    let home = harlo_harness::on_host(&crate::mcp::host_home(name));
    let home = home.trim_end_matches('/');
    [format!("{home}/.agents/skills"), format!("{home}/.claude/skills")]
}

/// What's set up for orchestration here (`host` None) or on a host: the
/// skills, which are installed, the bridges in pi's and omp's config, and
/// the skill folders.
pub fn overview(host: Option<&str>) -> anyhow::Result<Value> {
    let (installed, dirs): (Vec<&str>, Vec<String>) = match host {
        None => (installed_skills(), skill_dirs().iter().map(|d| crate::sidebar::tilde(d)).collect()),
        Some(name) => {
            let h = crate::remote::host(name).ok_or_else(|| anyhow::anyhow!("unknown remote host {name}"))?;
            let dirs = host_skill_dirs(name);
            let q = crate::remote::quote;
            let script = SKILLS
                .iter()
                .map(|s| format!("{{ [ -f {}/{n}/{m} ] || [ -f {}/{n}/{m} ]; }} && echo {n}", q(&dirs[0]), q(&dirs[1]), n = s.name, m = SKILL_MARK))
                .collect::<Vec<_>>()
                .join("; ");
            let out = crate::remote::run_ok(&h, &format!("{script}; true"), std::time::Duration::from_secs(20))?;
            let found: Vec<&str> = SKILLS.iter().filter(|s| out.lines().any(|l| l.trim() == s.name)).map(|s| s.name).collect();
            (found, dirs.iter().map(|d| crate::remote::display(&crate::remote::join(name, d))).collect())
        }
    };
    let bridges: serde_json::Map<String, Value> = BRIDGE_AGENTS
        .iter()
        .map(|k| (k.as_str().to_owned(), json!(bridge_installed_on(*k, host))))
        .collect();
    let skills: Vec<Value> = SKILLS.iter().map(|s| json!({ "name": s.name, "summary": s.summary, "installed": installed.contains(&s.name) })).collect();
    Ok(json!({ "skills": skills, "bridges": bridges, "skillDirs": dirs }))
}

/// Installs or removes a skill here or on a host.
pub fn set_skill_on(host: Option<&str>, name: &str, on: bool) -> anyhow::Result<()> {
    let Some(host_name) = host else { return set_skill(name, on) };
    let skill = SKILLS.iter().find(|s| s.name == name).ok_or_else(|| anyhow::anyhow!("There's no skill {name}."))?;
    let h = crate::remote::host(host_name).ok_or_else(|| anyhow::anyhow!("unknown remote host {host_name}"))?;
    let q = crate::remote::quote;
    let dirs = host_skill_dirs(host_name).map(|d| format!("{d}/{name}"));
    if on {
        // A folder of that name that harlo didn't make stays as it is.
        let check = dirs.iter().map(|d| format!("{{ [ -e {d} ] && [ ! -f {d}/{SKILL_MARK} ] && echo {d}; }}", d = q(d))).collect::<Vec<_>>().join("; ");
        let theirs = crate::remote::run_ok(&h, &format!("{check}; true"), std::time::Duration::from_secs(20))?;
        if let Some(d) = theirs.lines().map(str::trim).find(|l| !l.is_empty()) {
            anyhow::bail!("{host_name}:{d} is another skill of the same name; harlo left it alone.");
        }
        for d in &dirs {
            crate::remote::run_ok(&h, &format!("mkdir -p {}", q(d)), std::time::Duration::from_secs(20))?;
            crate::remote::write(&crate::remote::join(host_name, &format!("{d}/SKILL.md")), skill.text.as_bytes())?;
            crate::remote::write(&crate::remote::join(host_name, &format!("{d}/{SKILL_MARK}")), b"harlo installed this skill, and updates or removes this folder (Settings -> Orchestration).\n")?;
        }
    } else {
        let script = dirs.iter().map(|d| format!("{{ [ -f {d}/{SKILL_MARK} ] && rm -rf {d}; }}", d = q(d))).collect::<Vec<_>>().join("; ");
        crate::remote::run_ok(&h, &format!("{script}; true"), std::time::Duration::from_secs(20))?;
    }
    Ok(())
}

/// The bridge's entry for a host's pi or omp: `python3 <bridge> <socket>`
/// (put there, and the socket forwarded, when a session starts there).
fn host_bridge_form(name: &str) -> crate::mcp::Form {
    let (script, sock) = host_bridge_paths(name);
    crate::mcp::Form {
        name: "harlo".into(),
        enabled: true,
        transport: "stdio".into(),
        command: vec!["python3".into(), script, sock],
        exposure: "direct".into(),
        description: "harlo's sessions: start other coding agents, send them messages, read their replies.".into(),
        ..Default::default()
    }
}

fn scope_of(host: Option<&str>) -> crate::mcp::Scope {
    host.map_or(crate::mcp::Scope::Global, |h| crate::mcp::Scope::Host(h.to_owned()))
}

/// The agent has the bridge, here or on a host.
pub fn bridge_installed_on(kind: harlo_harness::HarnessKind, host: Option<&str>) -> bool {
    crate::mcp::entry(kind, &scope_of(host), "harlo").ok().flatten().is_some_and(|e| is_bridge(&e))
}

/// Adds the bridge to an agent's MCP config here or on a host, or removes it.
pub fn set_bridge_on(kind: harlo_harness::HarnessKind, host: Option<&str>, on: bool, harlo: &std::path::Path) -> anyhow::Result<()> {
    let Some(name) = host else { return set_bridge(kind, on, harlo) };
    let scope = scope_of(host);
    let existing = crate::mcp::entry(kind, &scope, "harlo")?;
    if existing.as_ref().is_some_and(|e| !is_bridge(e)) {
        anyhow::bail!("{} on {name} already has an MCP server called harlo; harlo left it alone.", kind.label());
    }
    match on {
        true => crate::mcp::put_entry(kind, &scope, Some(&host_bridge_form(name)), "harlo"),
        false if existing.is_some() => crate::mcp::put_entry(kind, &scope, None, "harlo"),
        false => Ok(()),
    }
}
