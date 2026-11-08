//! Orchestration: agents drive harlo.
//!
//! harlo's tools reach an agent as an MCP server: `harlo --mcp-bridge`, a
//! stdio server the agent starts like any other. It answers the protocol
//! itself and hands each tool call to the running harlo over a local
//! socket, saying who's calling: `HARLO_SESSION` (set in every agent harlo
//! starts). With no harlo running it offers no tools, so an
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

/// When agents start helpers (Settings → Orchestration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Starting {
    /// Only when the user asks (or with the Delegate button).
    WhenAsked,
    /// They offer a split ([`propose_helpers`](tools)); the user decides.
    Offer,
    /// On their own, when it helps.
    OnTheirOwn,
}

impl Starting {
    /// From the setting: "1" on their own, "asked" only when asked, else
    /// (the default) they offer.
    pub fn from_setting(value: &str) -> Starting {
        match value {
            "1" => Starting::OnTheirOwn,
            "asked" => Starting::WhenAsked,
            _ => Starting::Offer,
        }
    }
}

/// When agents may start sessions (see [`Starting`]).
pub fn when_to_start(starting: Starting) -> &'static str {
    match starting {
        Starting::OnTheirOwn => "Start one when another agent's work would help: a second opinion, work in parallel, a different model or agent.",
        Starting::WhenAsked => "Start one only when the user asks for another agent or session (a handoff, a second opinion, a helper, a committee), or their message is a harlo Delegate request. Otherwise do the work yourself (with your own subagents, if you have them).",
        Starting::Offer => "Start one when the user asks for another agent or session (a handoff, a second opinion, a helper, a committee), or their message is a harlo Delegate request. When a big task splits into parts that could be done at the same time (each one substantial, and not editing the same files), or a part is better done by another agent or model, don't start them yourself: offer the split with propose_helpers, and the user decides. Otherwise do the work yourself (with your own subagents, if you have them).",
    }
}

/// The tools, as MCP lists them (`starting`: see [`when_to_start`]).
pub fn tools(starting: Starting) -> Value {
    let s = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    let mut tools = json!([
        {
            "name": "list_agents",
            "description": "The coding agents harlo can start sessions with (harlo, claude): their ids and whether they're installed.",
            "inputSchema": s(json!({}), &[]),
        },
        {
            "name": "list_sessions",
            "description": "harlo's open sessions: each one's id, title, agent, status (busy / idle / waiting for input), folder, and which session started it.",
            "inputSchema": s(json!({}), &[]),
        },
        {
            "name": "start_session",
            "description": format!("Starts a new harlo session with a coding agent and sends it a first prompt. {} It becomes your helper, in your permission mode: the user sees it under your session, and harlo sends you its report when it finishes (unless notify is false), its questions for you, and when it waits for the user. Start every independent helper, then end your turn: harlo wakes you with what comes back; don't poll. The new agent has none of your context: make the prompt a self-contained briefing.", when_to_start(starting)),
            "inputSchema": s(json!({
                "agent": { "type": "string", "description": "harlo or claude (see list_agents)." },
                "prompt": { "type": "string", "description": "The first message: the task, its context, the relevant files, what's done and what's left." },
                "title": { "type": "string", "description": "A short name for the session." },
                "model": { "type": "string", "description": "provider/model, if not the agent's default." },
                "thinking": { "type": "string", "description": "Its thinking level (low, medium, high), if not the agent's default." },
                "folder": { "type": "string", "description": "Where it works (default: your folder)." },
                "worktree": { "type": "string", "description": "A new git worktree on this branch name to work in (isolated from your checkout)." },
                "notify": { "type": "boolean", "description": "Send its report back to you when it finishes, and its questions (default true). False: it's on its own (a handoff)." },
                "budget": { "type": "string", "description": "For a Harlo session: the most tokens it may use (sent and received, not counting those the cache served, its subagents' included), as 400k or 2m. Past it, it stops and you're told. Default: the user's budget per helper, if they set one." }
            }), &["agent", "prompt"]),
        },
        {
            "name": "send_message",
            "description": "Sends a follow-up message to a harlo session (queued if it's busy): more work for a helper, or the answer to its question. To tell all your helpers the same thing (a change of plan, stop and report), send it to \"helpers\".",
            "inputSchema": s(json!({
                "session": { "type": "string", "description": "The session's id (from start_session or list_sessions), or \"helpers\": every helper you started that's still open." },
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
            "name": "bring_over",
            "description": "Brings your helper's work over from its worktree to your folder: what it changed since its worktree started (its commits, and its uncommitted and new files), as uncommitted changes here. Nothing is committed. Files that don't fit your current version (you, or another helper, changed the same lines) are left alone and named: merge those by hand from the helper's worktree. Look over its work first; for a helper working in your own folder there's nothing to bring.",
            "inputSchema": s(json!({ "session": { "type": "string", "description": "Your helper's session id." } }), &["session"]),
        },
        {
            "name": "stop_session",
            "description": "Stops what a harlo session is doing (it stays open; send_message continues it).",
            "inputSchema": s(json!({ "session": { "type": "string" } }), &["session"]),
        },
        {
            "name": "request_approval",
            "description": "Asks the user to decide something only they may decide (to go ahead with a risky or irreversible step, or a choice that's theirs). harlo shows it in your session and on their phone, and notifies them; no agent can answer it. Then end your turn: their answer comes to you as a message.",
            "inputSchema": s(json!({
                "question": { "type": "string", "description": "What they decide, with what they need to know to decide it." },
                "options": { "type": "array", "items": { "type": "string" }, "description": "Their choices (default: Approve, Reject)." }
            }), &["question"]),
        }
    ]);
    if starting == Starting::Offer
        && let Some(list) = tools.as_array_mut()
    {
        list.push(json!({
            "name": "propose_helpers",
            "description": format!("Offers the user a split of your task into helpers that work at the same time. harlo shows them why and what each helper would do; if they agree, harlo starts the helpers (as start_session does: your helpers, in your permission mode, reporting back to you). Then end your turn: harlo tells you their answer, with the helpers' ids if it started them, and their reports come to you as they finish; don't poll. If they say no, do the work yourself. At most {MAX_PROPOSED} helpers."),
            "inputSchema": s(json!({
                "why": { "type": "string", "description": "Why splitting helps, in a sentence or two (for the user)." },
                "helpers": {
                    "type": "array",
                    "description": "The helpers, one per part.",
                    "items": s(json!({
                        "title": { "type": "string", "description": "A short name for the part." },
                        "prompt": { "type": "string", "description": "Its first message, a self-contained briefing (it has none of your context): the part, the relevant files, what \"done\" looks like, what not to touch." },
                        "agent": { "type": "string", "description": "harlo or claude. Leave it (and model) out for the project's usual helpers (the user's choice), unless a part needs another agent or model." },
                        "model": { "type": "string", "description": "provider/model, if not the agent's default." },
                        "worktree": { "type": "string", "description": "A new git worktree on this branch name to work in (for parts that edit files)." }
                    }), &["title", "prompt"])
                }
            }), &["why", "helpers"]),
        }));
    }
    tools
}

/// The most helpers one proposal may start.
pub const MAX_PROPOSED: usize = 6;

/// A helper an agent proposes (`propose_helpers`).
#[derive(Clone, Debug, PartialEq)]
pub struct Proposed {
    pub title: String,
    pub prompt: String,
    /// "" for the proposer's own.
    pub agent: String,
    pub model: String,
    pub worktree: String,
}

/// `propose_helpers`' arguments: why, and its helpers.
pub fn parse_proposal(args: &Value) -> Result<(String, Vec<Proposed>), String> {
    let text = |v: &Value, k: &str| v[k].as_str().map(str::trim).unwrap_or_default().to_owned();
    let why = text(args, "why");
    if why.is_empty() {
        return Err("Say why splitting helps (why).".into());
    }
    let helpers: Vec<Proposed> = args["helpers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| Proposed { title: text(h, "title"), prompt: text(h, "prompt"), agent: text(h, "agent").to_lowercase(), model: text(h, "model"), worktree: text(h, "worktree") })
        .collect();
    if helpers.is_empty() {
        return Err("Give the helpers.".into());
    }
    if helpers.len() > MAX_PROPOSED {
        return Err(format!("At most {MAX_PROPOSED} helpers: combine some parts."));
    }
    if let Some(h) = helpers.iter().find(|h| h.title.is_empty() || h.prompt.is_empty()) {
        return Err(format!("Each helper needs a title and a prompt{}.", if h.title.is_empty() { String::new() } else { format!(" (\"{}\" has no prompt)", h.title) }));
    }
    Ok((why, helpers))
}

/// The title of the user's card for a proposal.
pub const PROPOSAL_TITLE: &str = "Split into helpers?";
/// Its choices: the first starts them.
pub const PROPOSAL_OPTIONS: [&str; 2] = ["Start helpers", "Do it yourself"];

/// What the user's card says: why, and each helper (`agent`: its label).
pub fn proposal_question(why: &str, helpers: &[(String, String, String, String)]) -> String {
    let mut text = format!("{}\n", why.trim());
    for (title, agent, model, worktree) in helpers {
        let mut how = vec![agent.clone()];
        if !model.is_empty() {
            how.push(model.rsplit('/').next().unwrap_or(model).to_owned());
        }
        if !worktree.is_empty() {
            how.push(format!("worktree {worktree}"));
        }
        text.push_str(&format!("\n• {title} ({})", how.join(", ")));
    }
    text
}

/// What the proposer is told of the user's answer: `started`, each helper's
/// title and what became of it (Ok: where it is; Err: why it didn't start),
/// or None when they said no.
pub fn proposal_answer(started: Option<&[(String, Result<String, String>)]>) -> String {
    let Some(started) = started else {
        return "[harlo] The user turned down your split (propose_helpers): do the work yourself, in this session, without helpers.".into();
    };
    let ok: Vec<String> = started.iter().filter_map(|(t, r)| r.as_ref().ok().map(|w| format!("\"{t}\" ({w})"))).collect();
    let failed: Vec<String> = started.iter().filter_map(|(t, r)| r.as_ref().err().map(|e| format!("\"{t}\": {e}"))).collect();
    let mut text = String::from("[harlo] The user agreed to your split.");
    if !ok.is_empty() {
        text.push_str(&format!(" harlo started your helpers: {}. Their reports come to you as they finish: end your turn now (don't poll), then check their work and combine it (a helper's in a worktree: bring its changes over with bring_over; don't retype them).", ok.join(", ")));
    }
    if !failed.is_empty() {
        text.push_str(&format!(" These didn't start: {}. Do those parts yourself, or start them with start_session once the problem is fixed.", failed.join("; ")));
    }
    text
}

/// The tools of a session another session started (its helper), besides
/// [`tools`].
pub fn helper_tools() -> Value {
    let s = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    json!([
        {
            "name": "report_done",
            "description": "Reports your task's result to the session that started you: once, when you're done (or can't go on), then end your turn.",
            "inputSchema": s(json!({
                "outcome": { "type": "string", "enum": ["succeeded", "failed"], "description": "failed if the task isn't done: never hide a failure in the summary." },
                "summary": { "type": "string", "description": "Three sentences: what you did, what you found, what's left." },
                "files": { "type": "array", "items": { "type": "string" }, "description": "The files you changed (paths), if any." }
            }), &["outcome", "summary"]),
        },
        {
            "name": "ask_parent",
            "description": "Asks the session that started you a question the task doesn't settle. Then end your turn: harlo wakes you with its answer. (For what only the user may decide, use request_approval.)",
            "inputSchema": s(json!({ "question": { "type": "string" } }), &["question"]),
        }
    ])
}

/// A helper's report (`report_done`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub succeeded: bool,
    pub summary: String,
    pub files: Vec<String>,
}

impl Report {
    /// From `report_done`'s arguments.
    pub fn parse(args: &Value) -> Result<Report, String> {
        let succeeded = match args["outcome"].as_str().map(str::trim) {
            Some("succeeded") => true,
            Some("failed") => false,
            _ => return Err("Give an outcome: succeeded or failed.".into()),
        };
        let summary = args["summary"].as_str().map(str::trim).unwrap_or_default().to_owned();
        if summary.is_empty() {
            return Err("Give a summary: what you did, what you found, what's left.".into());
        }
        let files = args["files"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::trim)).filter(|f| !f.is_empty()).map(str::to_owned).collect();
        Ok(Report { succeeded, summary, files })
    }
}

/// What a helper's first message starts with: what it is, and how its
/// result goes back (harlo's tools).
pub fn helper_brief(parent: &str, task: &str) -> String {
    let parent = if parent.trim().is_empty() { String::new() } else { format!(" (\"{}\")", parent.trim()) };
    format!(
        "[harlo] You're a helper: another agent's session{parent} started you with the task below, and harlo sends it your result.\n\
         - Do only this task, yourself: start no helper sessions unless it asks.\n\
         - When the task leaves a decision open, call harlo's ask_parent tool with your question, then end your turn: the answer comes to you as a message.\n\
         - For what only the user may decide (going ahead with a risky or irreversible step, a choice that's theirs), call request_approval, then end your turn.\n\
         - When you're done, or can't go on, call report_done once: outcome succeeded or failed (never hide a failure in the summary), a three-sentence summary (what you did, what you found, what's left), and the files you changed. Then end your turn.\n\n\
         The task:\n\n{task}"
    )
}

/// What a helper in a new worktree on `branch` is told of where it works
/// (`root`: the project's main checkout, which its task may name).
pub fn worktree_note(branch: &str, root: &str) -> String {
    format!(
        "\n\nWhere you work: a new git worktree on branch {branch}, your own copy of the project apart from its main checkout ({root}). Make your changes in your working folder: where this task names files under {root}, use the same paths in yours. Leave the main checkout alone; the session that started you combines the work."
    )
}

/// What a parent is told when its helper `title` (`agent`, `key`) finished:
/// its report, else its last reply (`reply`), or its error; and what it
/// used (`used`: its model, requests and tokens, cost), when known.
pub fn finished_message(agent: &str, title: &str, key: &str, report: Option<&Report>, reply: Option<&str>, error: Option<&str>, used: Option<&str>) -> String {
    let text = finished_text(agent, title, key, report, reply, error);
    match used.map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => format!("{text}\n\nUsed: {u}"),
        None => text,
    }
}

fn finished_text(agent: &str, title: &str, key: &str, report: Option<&Report>, reply: Option<&str>, error: Option<&str>) -> String {
    let head = format!("[harlo] The {agent} session \"{title}\" ({key})");
    match report {
        Some(r) => {
            let mut text = format!("{head} finished and reported: {}.\n\n{}", if r.succeeded { "succeeded" } else { "FAILED" }, r.summary);
            if !r.files.is_empty() {
                text.push_str(&format!("\n\nFiles changed: {}", r.files.join(", ")));
            }
            text
        }
        None => match (reply.map(str::trim).filter(|r| !r.is_empty()), error) {
            (Some(reply), _) => format!("{head} finished without reporting (report_done). Its last reply:\n\n{reply}"),
            (None, Some(e)) => format!("{head} stopped with an error, without reporting: {e}"),
            (None, None) => format!("{head} finished without a reply or a report."),
        },
    }
}

/// To an agent whose helper (`title`) couldn't start: its worktree on
/// `branch` couldn't be made.
pub fn worktree_failed(title: &str, branch: &str, error: &str) -> String {
    format!(
        "[harlo] Your helper \"{title}\" didn't start: its worktree on {branch} couldn't be made ({error}). A branch can be checked out in one worktree only (yours, if you're in one, counts): start it again on a new branch name, or without a worktree."
    )
}

/// Added to a worktree helper's finished message when the branch it
/// started from moved on meanwhile ("" when it didn't).
pub fn drift_note(branch: &str, base: &str, behind: u32) -> String {
    match behind {
        0 => String::new(),
        n => format!(
            "\n\nIt worked on {branch}, in a worktree; {base} has moved on by {n} commit{} since it started, so merging it may conflict: merge {base} into it (or rebase) and check first.",
            if n == 1 { "" } else { "s" }
        ),
    }
}

/// Tokens as people write them: `400k`, `2m`, `1.5m`, `400000` (None: not
/// a count).
pub fn parse_tokens(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase().replace(['_', ','], "");
    let (num, times) = if let Some(n) = s.strip_suffix('k') {
        (n, 1e3)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 1e6)
    } else {
        (s.as_str(), 1.0)
    };
    let v: f64 = num.trim().parse().ok()?;
    (v > 0.0).then(|| (v * times).round() as u64)
}

/// Added to a worktree helper's finished message: how its work comes over.
pub fn bring_over_note(branch: &str, key: &str) -> String {
    format!("\n\nIts work is in its worktree (branch {branch}): once you've looked it over, bring it to your folder with bring_over (session {key}).")
}

/// What a parent is told of `bring_over` (`brought`: the files applied and
/// those left alone; `helper`: its worktree).
pub fn brought_text(applied: &[String], clashed: &[String], helper: &str) -> String {
    match (applied.is_empty(), clashed.is_empty()) {
        (true, true) => "The helper changed nothing since its worktree started: there's nothing to bring over.".into(),
        (false, true) => format!("Brought over, uncommitted: {}.", applied.join(", ")),
        (applied_none, false) => format!(
            "{}Not brought over, because they don't fit your version (the same lines changed here): {}. Merge those by hand from the helper's worktree ({helper}).",
            if applied_none { String::new() } else { format!("Brought over, uncommitted: {}. ", applied.join(", ")) },
            clashed.join(", ")
        ),
    }
}

/// What a parent is told when its helper asks it something.
pub fn question_message(agent: &str, title: &str, key: &str, question: &str) -> String {
    format!(
        "[harlo] The {agent} session \"{title}\" ({key}) asks you:\n\n{}\n\n\
         Answer it with send_message (session \"{key}\") when the answer follows from its task or the user's instructions; otherwise ask the user first.",
        question.trim()
    )
}

/// What a parent is told when its helper waits for the user (an approval,
/// its question, or one it asked with request_approval).
pub fn waiting_message(agent: &str, title: &str, key: &str, what: &str) -> String {
    let what = what.trim();
    let what = if what.is_empty() { String::new() } else { format!(": {what}") };
    format!(
        "[harlo] The {agent} session \"{title}\" ({key}) is waiting for the user{what}\n\n\
         It can't go on until they answer it in harlo (no agent can). Tell the user if they should know, or go on with other work; harlo tells you when it finishes."
    )
}

/// What a waiting request asks, in short (for the parent): an approval as
/// "approval to use <tool>: <its first line>", else its question's first line.
pub fn waiting_what(title: &str, question: &str, approval: bool) -> String {
    let short = |t: &str| {
        let line = t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
        match line.char_indices().nth(100) {
            Some((i, _)) => format!("{}…", line[..i].trim_end()),
            None => line.to_owned(),
        }
    };
    if !approval {
        return short(question);
    }
    // (Details under the title: the question is the title.)
    let question = match question.strip_prefix(title.lines().next().unwrap_or_default()) {
        Some(rest) if question.starts_with("Allow tool:") => rest,
        _ => question,
    };
    let tool = title.lines().next().unwrap_or_default().trim().trim_start_matches("Allow tool:").trim();
    match (tool, short(question)) {
        ("", what) => format!("an approval: {what}"),
        (tool, what) if what.is_empty() => format!("approval to use {tool}"),
        (tool, what) => format!("approval to use {tool} ({what})"),
    }
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

/// What the bridge asks to learn whether orchestration is on.
pub const ENABLED: &str = "_enabled";
/// What the bridge asks for a session's tools.
pub const TOOLS: &str = "_tools";
/// What the bridge asks for what a session's tools are for.
pub const INSTRUCTIONS: &str = "_instructions";

/// What harlo's tools are for, before a project's specs and plan.
pub const BASE_INSTRUCTIONS: &str = "harlo's sessions: start other coding agents, send them messages, read their replies.";

/// What the bridge sends harlo for a tool call (one line), and harlo's
/// answer (one line): `{ok, result}` or `{ok: false, error}`.
pub fn call_line(session: &str, tool: &str, args: &Value) -> String {
    json!({ "session": session, "tool": tool, "args": args }).to_string()
}

/// harlo draws Mermaid: told to every session with harlo's tools.
pub const DIAGRAMS: &str = "harlo shows Mermaid diagrams in your replies (```mermaid blocks) as pictures, on the desktop and on the phone: draw one when a flow, a structure or a sequence of steps is clearer as a picture than in words.";

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
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        // Notifications get no answer.
        let Some(id) = msg.get("id").cloned() else { continue };
        let reply = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
        let answer = match msg["method"].as_str().unwrap_or_default() {
            // What this session's tools are for (the project's specs and
            // plan add to it).
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

// Settings → Orchestration: the skills that teach agents to use the tools.
// (Claude and Harlo get the tools with each session harlo starts.)

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

/// Where agents find skills: `~/.agents/skills` (many agents) and Claude's.
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

/// An agent to delegate to (the composer's Delegate menu).
pub struct Member {
    /// "Claude"
    pub label: String,
    /// "claude"
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
             Then end your turn. Its report comes back to you as a [harlo] message; don't poll for it. Then give me its verdict and your own recommendation.\n\n\
             The question: {}",
            m.describe(),
            if note.is_empty() { "the approach we're taking on the current task." } else { note }
        ),
        ("committee", [a, b, ..]) => format!(
            "Form a committee to step back, find the root cause and agree on a plan. With harlo's start_session tool, start two sessions with the same prompt, each titled \"[Committee] <the task>\":\n\
             - {}\n\
             - {}\n\
             The prompt is about the problem, not your theory: the goal, the symptoms, what's been tried, the relevant files (by path), and the question \"What's the root cause, and what's the plan?\" End it, and every later message to them, with: {NO_EDITS}\n\
             Then end your turn. Their reports come back to you as [harlo] messages; don't poll or hurry them. Where they disagree, send each the other's arguments with send_message, until they agree. Then give me the agreed plan, where they disagreed, and how that was settled.\n\n\
             The problem: {}",
            a.describe(),
            b.describe(),
            if note.is_empty() { "what we're stuck on in the current task." } else { note }
        ),
        ("split", [m, ..]) => {
            let place = if worktree { ", each in its own new git worktree (pass a short branch name taken from its part as `worktree`)" } else { "" };
            // (Their files are in their worktrees, which you can read: taken
            // as they are, not written out again.)
            let combine = if worktree { " (bring their changes over from their worktrees with harlo's bring_over tool: don't retype them)" } else { "" };
            format!(
                "Split this task into helpers that work at the same time. Find the parts that can be done independently (each one substantial, and not editing the same files; two to four is usually right), and start a helper for each with harlo's start_session tool: {}{place}, titled \"[Split] <the part>\". Keep for yourself what depends on the other parts, and do it once they've reported.\n\
                 Each helper has none of our context, so make its prompt a self-contained briefing: its part, why it exists, the relevant files (by path), what \"done\" looks like, and what not to touch (the other parts).\n\
                 If the task doesn't split well, tell me why and do it yourself.\n\
                 Then end your turn. Their reports come back to you as [harlo] messages; don't poll. When they've all reported, check their work, combine it{combine}, and tell me what each did and what's left.\n\n\
                 The task: {}",
                m.describe(),
                if note.is_empty() { "the current task." } else { note }
            )
        }
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
        let a = delegate_prompt("advisor", &[m("Harlo", "harlo", "openai/gpt-5")], "", false).unwrap();
        assert!(a.contains("model \"openai/gpt-5\"") && a.contains("Do NOT write code.") && a.ends_with("the current task."));
        let c = delegate_prompt("committee", &[m("Claude", "claude", ""), m("Harlo", "harlo", "google/gemini")], "flaky tests", false).unwrap();
        assert!(c.contains("- Claude (agent") && c.contains("- Harlo (agent \"harlo\", model \"google/gemini\")") && c.ends_with("The problem: flaky tests"));
        let s = delegate_prompt("split", &[m("Claude", "claude", "")], "", true).unwrap();
        assert!(s.contains("don't retype them"), "{s}");
        assert!(s.contains("start a helper for each") && s.contains("each in its own new git worktree") && s.contains("[Split] <the part>") && s.ends_with("The task: the current task."));
        // A committee needs two.
        assert!(delegate_prompt("committee", &[m("Claude", "claude", "")], "", false).is_none());
        assert!(delegate_prompt("nope", &[m("Claude", "claude", "")], "", false).is_none());
    }

    #[test]
    fn token_counts() {
        assert_eq!([parse_tokens("400k"), parse_tokens(" 2M "), parse_tokens("1.5m"), parse_tokens("400000"), parse_tokens("lots"), parse_tokens("-3")], [Some(400_000), Some(2_000_000), Some(1_500_000), Some(400_000), None, None]);
    }

    #[test]
    fn what_bring_over_says() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(brought_text(&[], &[], "/w").contains("nothing to bring over"));
        assert_eq!(brought_text(&s(&["a.rs", "b.rs"]), &[], "/w"), "Brought over, uncommitted: a.rs, b.rs.");
        let both = brought_text(&s(&["a.rs"]), &s(&["c.rs"]), "/w");
        assert!(both.starts_with("Brought over, uncommitted: a.rs. Not brought over") && both.contains("c.rs") && both.contains("(/w)"), "{both}");
        assert!(tools(Starting::Offer).as_array().unwrap().iter().any(|t| t["name"] == "bring_over"));
        assert!(bring_over_note("feat", "k1").contains("bring_over (session k1)"));
    }

    #[test]
    fn a_moved_base_is_noted() {
        assert!(worktree_failed("Drift", "helper/x", "already exists").contains("\"Drift\" didn't start: its worktree on helper/x couldn't be made (already exists)"));
        assert_eq!(drift_note("helper/x", "main", 0), "");
        assert!(drift_note("helper/x", "main", 1).contains("main has moved on by 1 commit since"));
        assert!(drift_note("helper/x", "main", 3).contains("by 3 commits"));
    }

    #[test]
    fn helpers_report_back() {
        // A report: its outcome, summary and files.
        let r = Report::parse(&json!({ "outcome": "failed", "summary": " Tried. ", "files": ["a.rs", " ", "b.rs"] })).unwrap();
        assert_eq!(r, Report { succeeded: false, summary: "Tried.".into(), files: vec!["a.rs".into(), "b.rs".into()] });
        assert!(Report::parse(&json!({ "outcome": "done", "summary": "x" })).is_err());
        assert!(Report::parse(&json!({ "outcome": "succeeded", "summary": "  " })).is_err());
        let m = finished_message("Claude", "Fix", "k1", Some(&r), Some("ignored"), None, None);
        assert!(m.starts_with("[harlo] The Claude session \"Fix\" (k1) finished and reported: FAILED.") && m.ends_with("Files changed: a.rs, b.rs"));
        // None: its last reply, else its error.
        assert!(finished_message("harlo", "T", "k", None, Some("hi"), None, None).contains("without reporting (report_done). Its last reply:\n\nhi"));
        assert!(finished_message("harlo", "T", "k", None, Some(" "), Some("boom"), Some(" ")).ends_with("without reporting: boom"));
        // What it used, last.
        let m = finished_message("Harlo", "T", "k", Some(&r), None, None, Some("gpt-6.1-sol: 3 requests, 9.1k tokens in (70% cached), 400 out"));
        assert!(m.ends_with("Files changed: a.rs, b.rs\n\nUsed: gpt-6.1-sol: 3 requests, 9.1k tokens in (70% cached), 400 out"), "{m}");
        // The brief: what it is, harlo's tools, then the task.
        let b = helper_brief("Lead", "Do X");
        assert!(b.contains("session (\"Lead\") started you") && b.contains("report_done") && b.ends_with("The task:\n\nDo X"));
        assert!(!helper_brief("", "Do X").contains("session ("));
        assert!(worktree_note("helper/x", "/p").contains("a new git worktree on branch helper/x, your own copy of the project apart from its main checkout (/p)"));
        assert!(approval_answer("Merge?", &json!({ "value": "Approve" })).ends_with("(\"Merge?\"): Approve"));
        assert!(approval_answer("Merge?", &json!({ "cancelled": true })).contains("dismissed"));
        // What it waits for, in short.
        let long = format!("echo {} && find . -type f | wc -l\nsecond line", "x".repeat(150));
        let w = waiting_what("Allow tool: Bash", &long, true);
        assert!(w.starts_with("approval to use Bash (echo xxx") && w.ends_with("…)") && !w.contains("second line"), "{w}");
        assert_eq!(waiting_what("Question", "Which one?\nmore", false), "Which one?");
        let t = "Allow tool: write\nPath: a.py\nContent: x";
        assert_eq!(waiting_what(t, t, true), "approval to use write (Path: a.py)");
        // When to start sessions: as the setting says.
        let start = |st: Starting| tools(st).as_array().unwrap().iter().find(|t| t["name"] == "start_session").unwrap()["description"].as_str().unwrap().to_owned();
        assert!(start(Starting::WhenAsked).contains("only when the user asks") && !start(Starting::OnTheirOwn).contains("only when"));
        assert!(start(Starting::Offer).contains("offer the split with propose_helpers"));
        // The split offer: only when they offer.
        let proposes = |st: Starting| tools(st).as_array().unwrap().iter().any(|t| t["name"] == "propose_helpers");
        assert!(proposes(Starting::Offer) && !proposes(Starting::WhenAsked) && !proposes(Starting::OnTheirOwn));
        assert_eq!(Starting::from_setting(""), Starting::Offer);
        assert_eq!(Starting::from_setting("1"), Starting::OnTheirOwn);
        assert_eq!(Starting::from_setting("asked"), Starting::WhenAsked);
    }

    #[test]
    fn agents_offer_splits() {
        let args = json!({ "why": " Two parts. ", "helpers": [
            { "title": "API", "prompt": "Do the API", "agent": "Claude", "worktree": "split/api" },
            { "title": "UI", "prompt": "Do the UI", "model": "openai/gpt-5" }
        ] });
        let (why, helpers) = parse_proposal(&args).unwrap();
        assert_eq!(why, "Two parts.");
        assert_eq!(helpers[0], Proposed { title: "API".into(), prompt: "Do the API".into(), agent: "claude".into(), model: String::new(), worktree: "split/api".into() });
        assert!(parse_proposal(&json!({ "why": "x", "helpers": [] })).is_err());
        assert!(parse_proposal(&json!({ "why": "", "helpers": [{ "title": "a", "prompt": "b" }] })).is_err());
        assert!(parse_proposal(&json!({ "why": "x", "helpers": [{ "title": "a", "prompt": " " }] })).unwrap_err().contains("\"a\" has no prompt"));
        let many: Vec<Value> = (0..=MAX_PROPOSED).map(|i| json!({ "title": i.to_string(), "prompt": "p" })).collect();
        assert!(parse_proposal(&json!({ "why": "x", "helpers": many })).is_err());
        let q = proposal_question("Two parts.", &[("API".into(), "Claude".into(), String::new(), "split/api".into()), ("UI".into(), "Harlo".into(), "openai/gpt-5".into(), String::new())]);
        assert_eq!(q, "Two parts.\n\n• API (Claude, worktree split/api)\n• UI (Harlo, gpt-5)");
        assert!(proposal_answer(None).contains("do the work yourself"));
        let a = proposal_answer(Some(&[("API".into(), Ok("k1".into())), ("UI".into(), Err("Harlo is turned off".into()))]));
        assert!(a.contains("started your helpers: \"API\" (k1).") && a.contains("These didn't start: \"UI\": Harlo is turned off."), "{a}");
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
// instance): its skill folders, over SSH.

/// A host's skill folders, as it sees them.
fn host_skill_dirs(name: &str) -> [String; 2] {
    let home = harlo_harness::on_host(&crate::mcp::host_home(name));
    let home = home.trim_end_matches('/');
    [format!("{home}/.agents/skills"), format!("{home}/.claude/skills")]
}

/// What's set up for orchestration here (`host` None) or on a host: the
/// skills, which are installed, and the skill folders.
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
    let skills: Vec<Value> = SKILLS.iter().map(|s| json!({ "name": s.name, "summary": s.summary, "installed": installed.contains(&s.name) })).collect();
    Ok(json!({ "skills": skills, "skillDirs": dirs }))
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
