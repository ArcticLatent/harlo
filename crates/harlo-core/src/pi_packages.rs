//! pi's extensions: bare pi has no guardrails, no web access and little else,
//! so harlo offers a chosen few that it knows work in it (their prompts,
//! panels and tools reach harlo's own). It installs them once, with pi's own
//! commands, and leaves them off: turning one on is up to you, in Settings →
//! Extensions, where new versions come from too. Off is pi's own package
//! filter, so it loads nothing and adds nothing to requests. Whatever else
//! someone installed with pi keeps working; harlo leaves it alone.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// An extension harlo offers: from npm, at the version harlo was tried with
/// (a first install happens without anyone choosing it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extension {
    pub name: &'static str,
    pub tested: &'static str,
    pub title: &'static str,
    pub what: &'static str,
    /// What it adds to every request, about (measured against bare pi).
    pub tokens: u32,
    /// Only installed when someone turns it on.
    pub optional: bool,
    /// What to know before turning it on (rich text).
    pub note: &'static str,
}

pub const PERMISSIONS: &str = "@gotgenes/pi-permission-system";
const TODO: &str = "@juicesharp/rpiv-todo";
pub const EXTENSIONS: [Extension; 6] = [
    Extension { name: PERMISSIONS, tested: "42.0.0", title: "Permissions", what: "Asks before edits and commands, as the shield in the message box says, and blocks secrets and destructive commands. Off, pi runs everything without asking, and sessions have no shield.", tokens: 0, optional: false, note: "" },
    Extension { name: "@juicesharp/rpiv-ask-user-question", tested: "2.15.0", title: "Questions", what: "Lets the agent ask you, with choices, instead of guessing.", tokens: 1300, optional: false, note: "" },
    Extension { name: TODO, tested: "2.15.0", title: "To-do list", what: "A task list the agent keeps, shown above the message box.", tokens: 1300, optional: false, note: "" },
    Extension { name: "pi-subagents", tested: "0.84.0", title: "Subagents", what: "Hands work to other agents, in the background; they show above the message box.", tokens: 1800, optional: false, note: "" },
    Extension { name: "pi-web-access", tested: "0.46.0", title: "Web access", what: "Web search and reading pages, GitHub repositories, PDFs and videos. Needs no API key.", tokens: 1300, optional: false, note: "" },
    Extension { name: "pi-claude-bridge", tested: "0.9.3", title: "Claude", what: "Claude's models in pi, run through your Claude Code.", tokens: 0, optional: true, note: CLAUDE_NOTE },
];

const CLAUDE_NOTE: &str = "Use it at your own risk. It isn't installed until you turn it on (about 320 MB).<br><br>It runs Claude Code through Anthropic's Agent SDK with your own Claude login. <a href=\"https://code.claude.com/docs/en/legal-and-compliance\">Anthropic's terms</a> say products built on the Agent SDK should use an API key, and don't allow routing requests through a Pro, Max or Ultra plan on a user's behalf. They may enforce that without notice.<br><br>Whether this extension counts is unclear, and any consequence for your account or bill is yours. harlo's own Claude agent doesn't need it.";

/// A package's name in a source (`npm:@a/b@1.2.0` → `@a/b`).
pub fn name_of(source: &str) -> &str {
    let bare = source.strip_prefix("npm:").unwrap_or(source);
    match bare[1.min(bare.len())..].find('@') {
        Some(at) => &bare[..at + 1],
        None => bare,
    }
}

/// Which machine's pi: this one's, or another's over its SSH connection
/// (its own folder there, its own `pi`).
#[derive(Clone, Copy)]
pub enum At<'a> {
    Here,
    On(&'a crate::remote::Host),
}

/// pi's folder on another machine, in its shell.
const REMOTE_DIR: &str = r#""${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}""#;

/// A file in pi's folder (`rel`): None when it isn't there.
fn read_rel(at: At, rel: &str) -> Result<Option<String>, String> {
    match at {
        At::Here => match std::fs::read_to_string(agent_dir().join(rel)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            other => other.map(Some).map_err(|e| e.to_string()),
        },
        At::On(host) => {
            let f = format!("{REMOTE_DIR}/{}", crate::remote::quote(rel));
            let out = crate::remote::run(host, &format!("f={f}; [ -e \"$f\" ] || exit 3; cat -- \"$f\""), Duration::from_secs(30)).map_err(|e| format!("{e:#}"))?;
            match out.status.code() {
                Some(0) => Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned())),
                Some(3) => Ok(None),
                _ => Err(crate::remote::ssh_error(&host.name, &out)),
            }
        }
    }
}

/// Writes a file in pi's folder (its folders made; replaced at once).
fn write_rel(at: At, rel: &str, text: &str) -> Result<(), String> {
    match at {
        At::Here => {
            let file = agent_dir().join(rel);
            let temp = file.with_extension("harlo");
            std::fs::create_dir_all(file.parent().expect("a folder")).map_err(|e| e.to_string())?;
            std::fs::write(&temp, text).map_err(|e| e.to_string())?;
            std::fs::rename(&temp, &file).map_err(|e| e.to_string())
        }
        At::On(host) => {
            let f = format!("{REMOTE_DIR}/{}", crate::remote::quote(rel));
            let script = format!("f={f}; mkdir -p \"$(dirname \"$f\")\" && cat > \"$f.harlo\" && mv -- \"$f.harlo\" \"$f\"");
            let out = crate::remote::run_with_input(host, &script, Some(text.as_bytes()), Duration::from_secs(30)).map_err(|e| format!("{e:#}"))?;
            if out.status.success() { Ok(()) } else { Err(crate::remote::ssh_error(&host.name, &out)) }
        }
    }
}

/// pi's own folder (`$PI_CODING_AGENT_DIR`, else `~/.pi/agent`).
fn agent_dir() -> PathBuf {
    std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from).unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".pi/agent"))
}

fn read_settings(at: At) -> Value {
    read_rel(at, "settings.json").ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}))
}

/// The settings, to change them: one that can't be read isn't replaced.
fn settings_to_change(at: At) -> Result<Value, String> {
    let Some(text) = read_rel(at, "settings.json")? else { return Ok(json!({})) };
    match serde_json::from_str::<Value>(&text) {
        Ok(settings) if settings.is_object() => Ok(settings),
        _ => Err("pi's settings.json isn't valid. Fix it, then try again.".to_owned()),
    }
}

fn write_settings(at: At, settings: &Value) -> Result<(), String> {
    write_rel(at, "settings.json", &(serde_json::to_string_pretty(settings).expect("json") + "\n"))
}

fn entry_source(entry: &Value) -> Option<&str> {
    entry.as_str().or_else(|| entry["source"].as_str())
}

fn entry_of<'a>(settings: &'a Value, name: &str) -> Option<&'a Value> {
    settings["packages"].as_array()?.iter().find(|p| entry_source(p).is_some_and(|s| name_of(s) == name))
}

/// A package's entry, off: pi's filter, with nothing of it loaded.
fn off_entry(source: &str) -> Value {
    json!({ "source": source, "extensions": [], "skills": [], "prompts": [], "themes": [] })
}

/// Whether an entry loads the package (a filter someone wrote themselves
/// counts as on, unless it leaves out what makes it work).
fn entry_on(entry: &Value) -> bool {
    entry.is_string() || entry["extensions"].as_array().is_none_or(|a| !a.is_empty())
}

/// `settings` with the package's entry on or off, the rest as it was.
fn with_switch(mut settings: Value, name: &str, on: bool) -> Value {
    if let Some(list) = settings.get_mut("packages").and_then(Value::as_array_mut)
        && let Some(entry) = list.iter_mut().find(|p| entry_source(p).is_some_and(|s| name_of(s) == name))
    {
        let source = entry_source(entry).unwrap_or_default().to_owned();
        *entry = if on { json!(source) } else { off_entry(&source) };
    }
    settings
}

/// Whether pi has the package at all, on or off (reads its settings, not
/// `pi list`, so it's quick).
pub fn configured(at: At, name: &str) -> bool {
    entry_of(&read_settings(at), name).is_some()
}

/// Whether sessions load the package.
pub fn enabled(name: &str) -> bool {
    enabled_at(At::Here, name)
}

/// The same, of that machine's pi (blocking).
pub fn enabled_at(at: At, name: &str) -> bool {
    entry_of(&read_settings(at), name).is_some_and(entry_on)
}

/// The same from settings text already read (another machine's, say).
pub fn enabled_in(settings: &str, name: &str) -> bool {
    serde_json::from_str::<Value>(settings).ok().as_ref().and_then(|s| entry_of(s, name)).is_some_and(entry_on)
}

/// Whether sessions have this list: pi's to-do extension's, if it's on.
pub fn todos_enabled() -> bool {
    enabled(TODO)
}

/// pi's settings file in a machine's home folder.
pub const SETTINGS_REL: &str = ".pi/agent/settings.json";
const REL: &str = SETTINGS_REL;

/// pi's settings on another machine, as last read (read again in the
/// background at most every half minute). None until read.
pub fn settings_on(host: &str) -> Option<String> {
    crate::remote::home_file_soon(host, REL, Duration::from_secs(30))
}

/// Whether that machine's pi sessions have the extension: as its settings
/// were last read; `unknown` until they have been.
pub fn enabled_on(host: &str, name: &str, unknown: bool) -> bool {
    settings_on(host).map_or(unknown, |s| enabled_in(&s, name))
}

/// Whether a session there ("" here) keeps a to-do list.
pub fn todos_enabled_on(host: &str) -> bool {
    if host.is_empty() { todos_enabled() } else { enabled_on(host, TODO, true) }
}

/// A model harlo doesn't offer: Claude through Claude Code, without Claude
/// Code here (the package lists its models whether or not they can run).
pub fn hidden_model(provider: &str) -> bool {
    provider == "claude-bridge" && harlo_harness::resolve_program(harlo_harness::HarnessKind::Claude).is_none()
}

/// What the permission package lets through, asks about and refuses, unless
/// there are rules already (written when it's first turned on). Without any it asks about everything, its own
/// companions' tools too. Reading and the agent's own bookkeeping pass; edits
/// and commands ask (the shield answers for you where it may); secrets and
/// commands no one means are refused whatever the shield says.
fn policy() -> Value {
    let reading = ["ls *", "ls", "cat *", "head *", "tail *", "wc *", "pwd", "grep *", "rg *", "which *", "git status*", "git diff*", "git log*", "git show*", "git branch", "git rev-parse *"];
    let never = ["rm -rf /", "rm -rf /*", "rm -rf ~", "rm -rf ~/*", "rm -fr /", "rm -fr /*", "rm -fr ~", "rm -fr ~/*", "mkfs*", "dd * of=/dev/*", "* > /dev/sd*", "* > /dev/nvme*"];
    let never = never.iter().map(|c| (*c).to_owned()).chain(never.iter().map(|c| format!("sudo {c}")));
    let mut bash = serde_json::Map::new();
    bash.insert("*".to_owned(), json!("ask"));
    for c in reading {
        bash.insert(c.to_owned(), json!("allow"));
    }
    for c in never {
        bash.insert(c, json!("deny"));
    }
    let mut path = serde_json::Map::new();
    path.insert("*".to_owned(), json!("allow"));
    for p in ["*.env", "*.env.*", "~/.ssh/*", "~/.aws/*", "~/.gnupg/*", "~/.kube/config", "~/.netrc", "~/.pi/agent/auth.json", "~/.claude/.credentials.json", "~/.omp/agent/agent.db", "~/.config/gh/hosts.yml"] {
        path.insert(p.to_owned(), json!("deny"));
    }
    path.insert("*.env.example".to_owned(), json!("allow"));
    json!({ "permission": {
        "*": "ask",
        "read": "allow", "grep": "allow", "find": "allow", "ls": "allow",
        "ask_user_question": "allow", "todo": "allow",
        "subagent": "allow", "contact_supervisor": "allow", "bg_wait": "allow", "subagent_supervisor": "allow",
        "web_search": "allow", "source_check": "allow", "fetch_content": "allow", "get_search_content": "allow",
        "skill": "allow",
        // (Sandboxed scripts: what they call is asked about like any other call.)
        "codemode": "allow", "tool_search": "allow",
        "mcp": { "*": "ask", "harlo": "allow" },
        "path": path,
        "bash": bash,
        "external_directory": "ask",
    } })
}

/// The permission package's rules, in pi's folder.
const POLICY: &str = "extensions/pi-permission-system/config.json";

/// The file with the permission package's rules (yours to change); on
/// another machine as `ssh://host/…` (its home asked once).
pub fn policy_file(at: At) -> Result<PathBuf, String> {
    match at {
        At::Here => Ok(agent_dir().join(POLICY)),
        At::On(host) => {
            let out = crate::remote::run(host, &format!("printf '%s' {REMOTE_DIR}"), Duration::from_secs(20)).map_err(|e| format!("{e:#}"))?;
            Ok(crate::remote::join(&host.name, &format!("{}/{POLICY}", String::from_utf8_lossy(&out.stdout).trim())))
        }
    }
}

fn write_policy(at: At) -> Result<(), String> {
    if read_rel(at, POLICY)?.is_some() {
        return Ok(());
    }
    write_rel(at, POLICY, &(serde_json::to_string_pretty(&policy()).expect("json") + "\n"))
}

/// What's been set up once (a record next to pi's settings, so an extension
/// someone removes with pi stays removed, and one harlo adds later comes).
const SET_UP: &str = "harlo-extensions.json";

fn set_up(at: At) -> Vec<String> {
    let v: Value = read_rel(at, SET_UP).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v["set_up"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_owned)).collect()
}

/// One change to pi's packages at a time.
static BUSY: Mutex<()> = Mutex::new(());

/// The extensions to install without being asked, not set up yet.
pub fn to_set_up(at: At) -> Vec<Extension> {
    let done = set_up(at);
    EXTENSIONS.into_iter().filter(|e| !e.optional && !done.iter().any(|d| d == e.name)).collect()
}

/// Installs what's left, off (blocking; one at a time however many ask). An
/// extension someone already has stays as it is. `progress` hears what's
/// being added. What failed is tried again next time; its error is returned.
pub fn ensure(at: At, pi: &Path, cwd: &Path, progress: &dyn Fn(&Extension)) -> Result<(), String> {
    let _busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
    let left = to_set_up(at);
    if left.is_empty() {
        return Ok(());
    }
    let mut done = set_up(at);
    let mut failed = Vec::new();
    for e in left {
        let result = if configured(at, e.name) {
            Ok(())
        } else {
            progress(&e);
            install_off(at, pi, cwd, &e)
        };
        match result {
            Ok(()) => {
                done.push(e.name.to_owned());
                let text = serde_json::to_string_pretty(&json!({ "set_up": done })).expect("json");
                write_rel(at, SET_UP, &(text + "\n"))?;
            }
            Err(why) => failed.push(format!("{}: {why}", e.title)),
        }
    }
    if failed.is_empty() { Ok(()) } else { Err(failed.join("\n")) }
}

/// Installs an extension already off: its entry goes first (pi keeps a filter
/// through an install), so no session can load it meanwhile. A failed one
/// leaves no entry (pi would try again every time it starts).
fn install_off(at: At, pi: &Path, cwd: &Path, e: &Extension) -> Result<(), String> {
    let source = format!("npm:{}@{}", e.name, e.tested);
    let mut settings = settings_to_change(at)?;
    if !settings["packages"].is_array() {
        settings["packages"] = json!([]);
    }
    settings["packages"].as_array_mut().expect("a list").push(off_entry(&source));
    write_settings(at, &settings)?;
    let result = install(at, pi, cwd, e.name, e.tested);
    if result.is_err() {
        let mut settings = settings_to_change(at)?;
        if let Some(list) = settings.get_mut("packages").and_then(Value::as_array_mut) {
            list.retain(|p| entry_source(p).is_none_or(|s| name_of(s) != e.name));
        }
        write_settings(at, &settings)?;
    }
    result
}

/// Turns an extension on or off for sessions started afterwards. On, it's
/// installed if it isn't, and permissions get their rules.
pub fn switch(at: At, pi: &Path, cwd: &Path, name: &str, on: bool) -> Result<(), String> {
    let e = EXTENSIONS.iter().find(|e| e.name == name).ok_or("Not one of harlo's extensions.")?;
    let _busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
    if on {
        if name == PERMISSIONS {
            write_policy(at)?;
        }
        if !list(at, pi, cwd)?.iter().any(|p| !p.project && name_of(&p.source) == name && !p.version.is_empty()) {
            install(at, pi, cwd, name, e.tested)?;
        }
    }
    write_settings(at, &with_switch(settings_to_change(at)?, name, on))
}

/// Installs a version with pi (it takes the place of any other of the
/// package, and stays until the next one is asked for).
pub fn install(at: At, pi: &Path, cwd: &Path, name: &str, version: &str) -> Result<(), String> {
    if !EXTENSIONS.iter().any(|e| e.name == name) {
        return Err("Not one of harlo's extensions.".into());
    }
    run_at(at, pi, cwd, &["install", &format!("npm:{name}@{version}")], Duration::from_secs(600)).map(|_| ())
}

/// The package's latest version on npm.
pub fn latest(name: &str) -> Result<String, String> {
    let url = format!("https://registry.npmjs.org/{}/latest", name.replace('/', "%2F"));
    crate::engines::get_json_as(&url, "application/json")?["version"].as_str().map(str::to_owned).ok_or_else(|| format!("npm doesn't know {name}"))
}

/// Whether `latest` is newer than `installed`.
pub fn is_newer(installed: &str, latest: &str) -> bool {
    let key = |v: &str| v.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect::<Vec<u64>>();
    !installed.is_empty() && key(latest) > key(installed)
}

/// pi's `todo` tool's result (or a saved message of it) as the to-do dock's
/// phases: None when it says nothing about the list.
pub fn todo_phases(result: &Value) -> Option<Value> {
    let tasks: Vec<Value> = result["details"]["tasks"]
        .as_array()?
        .iter()
        .filter(|t| t["status"] != "deleted")
        .map(|t| json!({ "content": t["subject"], "status": t["status"] }))
        .collect();
    Some(if tasks.is_empty() { json!([]) } else { json!([{ "name": "Tasks", "tasks": tasks }]) })
}

/// The to-do list in a saved conversation: the last `todo` result's.
pub fn todos_in(messages: &[Value]) -> Option<Value> {
    messages.iter().rev().filter(|m| m["role"] == "toolResult" && m["toolName"] == "todo").find_map(todo_phases)
}

/// An installed package.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Package {
    /// What it was installed from (`npm:pi-web-access`, `git:…`, a folder).
    pub source: String,
    pub path: PathBuf,
    /// A project's own (its `.pi/settings.json`), not yours.
    pub project: bool,
    pub version: String,
    pub description: String,
    /// What it adds to pi: "extensions", "skills", "prompts", "themes".
    pub adds: Vec<String>,
}

/// `pi list`'s packages: a heading per scope ("User packages:", "Project
/// packages:"), each source indented, its folder under it.
pub fn parse_list(text: &str) -> Vec<Package> {
    let mut out: Vec<Package> = Vec::new();
    let mut project = false;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if indent == 0 {
            project = t.to_lowercase().starts_with("project");
            continue;
        }
        match out.last_mut() {
            // Its folder, under it.
            Some(last) if indent > 2 && last.path.as_os_str().is_empty() => last.path = PathBuf::from(t),
            _ if t.ends_with(':') => {}
            // (Off, or otherwise narrowed, pi adds "(filtered)".)
            _ => out.push(Package { source: t.trim_end_matches(" (filtered)").to_owned(), project, ..Default::default() }),
        }
    }
    out
}

/// Fills in what the package's own package.json says.
pub fn describe(p: &mut Package) {
    let manifest = std::fs::read_to_string(p.path.join("package.json")).unwrap_or_default();
    let dirs: Vec<&str> = ["extensions", "skills", "prompts", "themes"].into_iter().filter(|k| p.path.join(k).is_dir()).collect();
    describe_from(p, &manifest, &dirs);
}

/// The same from its package.json's text and which of its folders are there.
fn describe_from(p: &mut Package, manifest: &str, dirs: &[&str]) {
    let v: Value = serde_json::from_str(manifest).unwrap_or_default();
    p.version = v["version"].as_str().unwrap_or_default().to_owned();
    p.description = v["description"].as_str().unwrap_or_default().trim().to_owned();
    // Its manifest (`"pi": { "extensions": [...] }`), else the usual folders.
    p.adds = ["extensions", "skills", "prompts", "themes"]
        .into_iter()
        .filter(|k| match v["pi"].get(*k) {
            Some(list) => list.as_array().is_some_and(|a| !a.is_empty()),
            None => v["pi"].is_null() && dirs.contains(k),
        })
        .map(str::to_owned)
        .collect();
}

/// Runs pi in `cwd` (blocking): its output, or why it failed (the end of
/// what it said).
pub fn run(pi: &Path, cwd: &Path, args: &[&str], limit: Duration) -> Result<String, String> {
    let mut child = Command::new(pi)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", pi.display()))?;
    // Read both as they come (npm says a lot).
    let read = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = r.read_to_string(&mut s);
            s
        })
    };
    let out = read(Box::new(child.stdout.take().expect("piped")));
    let err = read(Box::new(child.stderr.take().expect("piped")));
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("pi didn't finish in {} s.", limit.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if status.success() {
        return Ok(out);
    }
    Err(failure(&out, &err))
}

/// Runs pi: this machine's (`pi` in `cwd`), or another's (its `pi`, from
/// its home folder).
pub fn run_at(at: At, pi: &Path, cwd: &Path, args: &[&str], limit: Duration) -> Result<String, String> {
    let At::On(host) = at else { return run(pi, cwd, args, limit) };
    let args: Vec<String> = args.iter().map(|a| crate::remote::quote(a)).collect();
    let out = crate::remote::run(host, &format!("cd 2>/dev/null; pi {} </dev/null", args.join(" ")), limit).map_err(|e| format!("{e:#}"))?;
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned());
    if out.status.success() { Ok(stdout) } else { Err(failure(&stdout, &stderr)) }
}

/// Why pi failed: the end of what it said.
fn failure(out: &str, err: &str) -> String {
    let said = crate::signin::plain(&format!("{err}\n{out}"));
    let tail: Vec<&str> = said.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let said: String = tail[tail.len().saturating_sub(4)..].join("\n").chars().take(600).collect();
    if said.is_empty() { "pi failed.".into() } else { said }
}

/// The packages installed for you, and (in a project) the project's own.
pub fn list(at: At, pi: &Path, cwd: &Path) -> Result<Vec<Package>, String> {
    let text = run_at(at, pi, cwd, &["list"], Duration::from_secs(30))?;
    let mut list = parse_list(&text);
    match at {
        At::Here => list.iter_mut().for_each(describe),
        // Their package.json and folders, read there in one go.
        At::On(host) => {
            let paths: Vec<String> = list.iter().map(|p| crate::remote::quote(&p.path.to_string_lossy())).collect();
            if !paths.is_empty() {
                let script = format!(
                    "for d in {}; do printf '%s\\037' \"$d\"; tr '\\n' ' ' < \"$d/package.json\" 2>/dev/null; printf '\\037'; for k in extensions skills prompts themes; do [ -d \"$d/$k\" ] && printf '%s ' \"$k\"; done; printf '\\036'; done",
                    paths.join(" ")
                );
                let out = crate::remote::run(host, &script, Duration::from_secs(30)).map_err(|e| format!("{e:#}"))?;
                let text = String::from_utf8_lossy(&out.stdout);
                for record in text.split('\u{1e}') {
                    let mut parts = record.split('\u{1f}');
                    let (Some(path), Some(manifest), Some(dirs)) = (parts.next(), parts.next(), parts.next()) else { continue };
                    let dirs: Vec<&str> = dirs.split_whitespace().collect();
                    for p in list.iter_mut().filter(|p| p.path.to_string_lossy() == path) {
                        describe_from(p, manifest, &dirs);
                    }
                }
            }
        }
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pis_list() {
        let text = "User packages:\n  npm:pi-web-access\n    /home/u/.pi/agent/npm/node_modules/pi-web-access\n  git:github.com/a/b\n    /home/u/.pi/agent/git/github.com/a/b\nProject packages:\n  ./tools/pkg\n    /w/tools/pkg\n";
        let list = parse_list(text);
        let short: Vec<(&str, &str, bool)> = list.iter().map(|p| (p.source.as_str(), p.path.to_str().unwrap(), p.project)).collect();
        assert_eq!(short, [
            ("npm:pi-web-access", "/home/u/.pi/agent/npm/node_modules/pi-web-access", false),
            ("git:github.com/a/b", "/home/u/.pi/agent/git/github.com/a/b", false),
            ("./tools/pkg", "/w/tools/pkg", true),
        ]);
        assert!(parse_list("No packages installed.\n").is_empty());
        assert_eq!(parse_list("User packages:\n  npm:a@1.0.0 (filtered)\n    /a\n")[0].source, "npm:a@1.0.0");
    }

    #[test]
    fn says_what_a_package_adds() {
        let dir = std::env::temp_dir().join(format!("harlo-pi-packages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/skills")).unwrap();
        std::fs::create_dir_all(dir.join("b/skills")).unwrap();
        std::fs::write(dir.join("a/package.json"), r#"{"version":"1.2.0","description":" Web search ","pi":{"extensions":["./dist"],"skills":[]}}"#).unwrap();
        std::fs::write(dir.join("b/package.json"), r#"{"version":"0.1.0"}"#).unwrap();
        let mut a = Package { path: dir.join("a"), ..Default::default() };
        describe(&mut a);
        assert_eq!((a.version.as_str(), a.description.as_str(), a.adds.clone()), ("1.2.0", "Web search", vec!["extensions".to_owned()]));
        // No manifest: its folders.
        let mut b = Package { path: dir.join("b"), ..Default::default() };
        describe(&mut b);
        assert_eq!(b.adds, ["skills"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runs_and_reports_failures() {
        let sh = Path::new("/bin/sh");
        assert_eq!(run(sh, Path::new("/"), &["-c", "echo hi"], Duration::from_secs(5)).unwrap(), "hi\n");
        assert_eq!(run(sh, Path::new("/"), &["-c", "echo working; echo 'npm error 404 Not Found' >&2; exit 1"], Duration::from_secs(5)).unwrap_err(), "npm error 404 Not Found\nworking");
        assert!(run(sh, Path::new("/"), &["-c", "sleep 5"], Duration::from_millis(300)).unwrap_err().contains("didn't finish"));
        assert!(install(At::Here, sh, Path::new("/"), "not-one-of-ours", "1.0.0").unwrap_err().contains("Not one"));
    }

    #[test]
    fn names_a_source() {
        assert_eq!(name_of("npm:@gotgenes/pi-permission-system@42.0.0"), PERMISSIONS);
        assert_eq!(name_of("npm:@juicesharp/rpiv-todo"), "@juicesharp/rpiv-todo");
        assert_eq!(name_of("npm:pi-web-access@0.46.0"), "pi-web-access");
        assert_eq!(name_of("pi-web-access"), "pi-web-access");
        assert_eq!(name_of(""), "");
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.9.3", "0.10.0"));
        assert!(!is_newer("42.0.0", "42.0.0"));
        assert!(!is_newer("2.15.0", "2.14.9"));
        assert!(!is_newer("", "1.0.0"));
    }

    #[test]
    fn reads_the_todo_tools_list() {
        let result = json!({ "details": { "tasks": [
            { "id": 1, "subject": "First", "status": "in_progress" },
            { "id": 2, "subject": "Gone", "status": "deleted" },
            { "id": 3, "subject": "Second", "status": "pending" },
        ] } });
        assert_eq!(todo_phases(&result).unwrap(), json!([{ "name": "Tasks", "tasks": [
            { "content": "First", "status": "in_progress" }, { "content": "Second", "status": "pending" },
        ] }]));
        assert_eq!(todo_phases(&json!({ "details": { "tasks": [] } })).unwrap(), json!([]));
        // A refused call says nothing about the list.
        assert!(todo_phases(&json!({ "details": {} })).is_none());

        let saved = [
            json!({ "role": "toolResult", "toolName": "todo", "details": { "tasks": [{ "subject": "Old", "status": "pending" }] } }),
            json!({ "role": "toolResult", "toolName": "todo", "details": { "tasks": [{ "subject": "New", "status": "completed" }] } }),
            json!({ "role": "toolResult", "toolName": "todo", "details": {} }),
            json!({ "role": "toolResult", "toolName": "bash", "details": { "tasks": [] } }),
        ];
        assert_eq!(todos_in(&saved).unwrap()[0]["tasks"][0]["content"], "New");
        assert!(todos_in(&[]).is_none());
    }

    #[test]
    fn the_policy_is_what_the_shield_promises() {
        let p = policy();
        let p = &p["permission"];
        // Anything new asks; what harlo's own bookkeeping needs passes.
        assert_eq!(p["*"], "ask");
        for tool in ["read", "todo", "ask_user_question", "subagent", "web_search"] {
            assert_eq!(p[tool], "allow", "{tool}");
        }
        assert_eq!(p["mcp"]["harlo"], "allow");
        assert!(p["write"].is_null() && p["edit"].is_null(), "edits ask");
        assert_eq!(p["path"]["~/.ssh/*"], "deny");
        assert_eq!(p["bash"]["*"], "ask");
        assert_eq!(p["bash"]["rm -rf /*"], "deny");
        assert_eq!(p["bash"]["sudo rm -rf /*"], "deny");
        // Each extension harlo sets up is one it knows about.
        assert_eq!(EXTENSIONS[0].name, PERMISSIONS);
    }

    #[test]
    fn switches_an_entry_and_nothing_else() {
        let settings = json!({ "theme": "dark", "packages": ["npm:other@1.0.0", "npm:pi-web-access@0.46.0", { "source": "npm:mine", "skills": [] }] });
        let on = |s: &Value, name: &str| entry_of(s, name).is_some_and(entry_on);
        assert!(on(&settings, "pi-web-access") && on(&settings, "mine"), "a filter of someone's own isn't off");

        let off = with_switch(settings.clone(), "pi-web-access", false);
        assert!(!on(&off, "pi-web-access") && on(&off, "other"));
        assert_eq!(off["packages"][1], off_entry("npm:pi-web-access@0.46.0"));
        assert_eq!(off["theme"], "dark");
        // All of it off: a package with skills or prompts loads none either.
        for kind in ["extensions", "skills", "prompts", "themes"] {
            assert_eq!(off["packages"][1][kind], json!([]));
        }

        let back = with_switch(off, "pi-web-access", true);
        assert_eq!(back, settings);
        // Not there: nothing to do.
        assert_eq!(with_switch(settings.clone(), "gone", true), settings);
        assert_eq!(with_switch(json!({}), "gone", true), json!({}));
    }

    #[test]
    fn only_claude_waits_to_be_asked() {
        let optional: Vec<&str> = EXTENSIONS.iter().filter(|e| e.optional).map(|e| e.name).collect();
        assert_eq!(optional, ["pi-claude-bridge"]);
        assert!(EXTENSIONS.iter().all(|e| e.optional == !e.note.is_empty()), "what's optional says why");
    }

    /// Sets everything up with a real pi, from npm (`HARLO_TEST_PI`), in a
    /// scratch `PI_CODING_AGENT_DIR` (never pi's own: it refuses without one).
    #[test]
    #[ignore]
    fn sets_up_with_a_real_pi_online() {
        let dir = std::env::var("PI_CODING_AGENT_DIR").expect("a scratch PI_CODING_AGENT_DIR");
        assert!(dir.starts_with("/tmp/"), "not a scratch folder: {dir}");
        let pi = PathBuf::from(std::env::var("HARLO_TEST_PI").expect("HARLO_TEST_PI"));
        let cwd = Path::new(&dir);
        let heard = Mutex::new(Vec::new());
        ensure(At::Here, &pi, cwd, &|e| heard.lock().unwrap().push(e.title)).unwrap();
        // Each but the optional one, there and off, at its version.
        assert_eq!(heard.lock().unwrap().len(), EXTENSIONS.len() - 1);
        assert!(to_set_up(At::Here).is_empty());
        let listed = list(At::Here, &pi, cwd).unwrap();
        for e in EXTENSIONS.iter().filter(|e| !e.optional) {
            let p = listed.iter().find(|p| name_of(&p.source) == e.name).unwrap();
            assert_eq!(p.version, e.tested, "{}", e.name);
            assert!(configured(At::Here, e.name) && !enabled(e.name), "{} is off", e.name);
            assert!(is_newer("0.0.1", &latest(e.name).unwrap()));
        }
        assert!(!configured(At::Here, "pi-claude-bridge"), "the optional one waits");
        ensure(At::Here, &pi, cwd, &|_| panic!("again")).unwrap();
        // Permissions on: its rules, written once.
        assert!(!policy_file(At::Here).unwrap().exists());
        switch(At::Here, &pi, cwd, PERMISSIONS, true).unwrap();
        assert!(enabled(PERMISSIONS) && policy_file(At::Here).unwrap().exists());
        switch(At::Here, &pi, cwd, PERMISSIONS, false).unwrap();
        assert!(!enabled(PERMISSIONS));
        // A new version of one that's off: it stays off.
        install(At::Here, &pi, cwd, TODO, "2.14.0").unwrap();
        let listed = list(At::Here, &pi, cwd).unwrap();
        assert_eq!(listed.iter().filter(|p| name_of(&p.source) == TODO).map(|p| p.version.as_str()).collect::<Vec<_>>(), ["2.14.0"]);
        assert!(!enabled(TODO));
        // The optional one, on: installed then.
        switch(At::Here, &pi, cwd, "pi-claude-bridge", true).unwrap();
        assert!(enabled("pi-claude-bridge"));
        assert_eq!(list(At::Here, &pi, cwd).unwrap().iter().find(|p| name_of(&p.source) == "pi-claude-bridge").unwrap().version, "0.9.3");
    }
}
