//! MCP servers for Claude and Harlo (Settings → MCP), globally and per
//! project:
//!
//! | | global | project |
//! |---|---|---|
//! | Claude | `~/.claude.json` (`$CLAUDE_CONFIG_DIR`), changed with `claude mcp` | `<project>/.mcp.json` |
//! | Harlo | `~/.config/harlo-agent/mcp.json` (`$XDG_CONFIG_HOME`) | `<project>/.harlo/mcp.json` (approved entries) |
//!
//! harlo shows one list for both and edits each agent's file in its own
//! dialect: Harlo's timeouts are in seconds and it has no SSE; Claude has no
//! folder, timeout or off switch. Fields harlo doesn't edit (OAuth settings,
//! …) are kept as they are. Claude Code
//! rewrites its `~/.claude.json` as it runs, so harlo changes its global
//! servers with Claude's own `claude mcp add-json / remove`.
//!
//! Harlo starts a project's server only when that entry, as it is, was
//! approved for the project (a repository can bring a file that runs a
//! command): saving or switching one here approves it, in
//! `mcp-approved.json` beside Harlo's own `mcp.json` (on the project's
//! machine), `{ project: { name: hash of the entry } }`. Harlo has no SSE.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use harlo_harness::HarnessKind;
use serde_json::{Map, Value, json};

/// Where servers are configured: for every project here, for every
/// project on a remote host (its own global files), or for one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Global,
    Host(String),
    Project(PathBuf),
}

impl Scope {
    /// "global", "host:<name>", or the project's path.
    pub fn parse(s: &str) -> Self {
        match s {
            "" | "global" => Scope::Global,
            s => match s.strip_prefix("host:") {
                Some(name) => Scope::Host(name.to_owned()),
                None => Scope::Project(PathBuf::from(s)),
            },
        }
    }
}

/// A host's home folder (as its check found it, else asked), as an
/// `ssh://` path.
pub fn host_home(name: &str) -> PathBuf {
    let home = crate::remote::info_of(name)
        .map(|i| i.home)
        .filter(|h| !h.is_empty())
        .or_else(|| crate::remote::host(name).and_then(|h| crate::remote::home(&h).ok()))
        .unwrap_or_else(|| "~".into());
    crate::remote::join(name, &home)
}

/// The harnesses harlo manages MCP servers for, in display order.
pub const HARNESSES: [HarnessKind; 2] = [HarnessKind::Claude, HarnessKind::Harlo];

/// The `claude` that changes Claude's global servers (Settings → Providers
/// sets it; else the installed one).
static CLAUDE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn use_claude(program: Option<PathBuf>) {
    *CLAUDE.lock().unwrap() = program;
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// The harness's MCP file for a scope.
pub fn config_path(kind: HarnessKind, scope: &Scope) -> PathBuf {
    match (kind, scope) {
        (HarnessKind::Claude, Scope::Global) => std::env::var_os("CLAUDE_CONFIG_DIR").map(|d| PathBuf::from(d).join(".claude.json")).unwrap_or_else(|| home().join(".claude.json")),
        (HarnessKind::Claude, Scope::Project(p)) => p.join(".mcp.json"),
        // A host's own (its defaults: harlo can't see its environment).
        (HarnessKind::Claude, Scope::Host(h)) => host_home(h).join(".claude.json"),
        (_, Scope::Host(h)) => host_home(h).join(".config/harlo-agent/mcp.json"),
        (_, Scope::Global) => harlo_dir().join("mcp.json"),
        (_, Scope::Project(p)) => p.join(".harlo/mcp.json"),
    }
}

/// Where in the file the servers are.
const SERVERS: &str = "mcpServers";

/// Harlo's config folder here (a scratch one in tests).
#[cfg(test)]
fn harlo_dir() -> PathBuf {
    std::env::temp_dir().join(format!("harlo-mcp-test-config-{}", std::process::id()))
}

#[cfg(not(test))]
fn harlo_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".config")).join("harlo-agent")
}

/// Where Harlo keeps its approvals of a project's servers, and the
/// project's key there: on the project's machine.
fn harlo_approvals(project: &Path) -> (PathBuf, String) {
    match crate::remote::split(project) {
        Some((name, p)) => (host_home(&name).join(".config/harlo-agent/mcp-approved.json"), p),
        None => (harlo_dir().join("mcp-approved.json"), project.to_string_lossy().into_owned()),
    }
}

/// What an entry's approval is kept by: its JSON, keys sorted, hashed
/// (as Harlo reads it).
pub fn entry_hash(entry: &Value) -> String {
    fn canonical(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), canonical(&m[k]))).collect::<Map<_, _>>())
            }
            Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
            v => v.clone(),
        }
    }
    use sha2::Digest;
    sha2::Sha256::digest(canonical(entry).to_string().as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Records (Some) or forgets (None) Harlo's approval of a project's server.
fn approve(scope: &Scope, name: &str, entry: Option<&Value>) -> Result<()> {
    let Scope::Project(project) = scope else { return Ok(()) };
    let (path, key) = harlo_approvals(project);
    let mut all: Value = read_text(&path)?.and_then(|t| serde_json::from_str(&t).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
    let before = all.clone();
    let projects = all.as_object_mut().expect("object");
    match entry {
        Some(e) => {
            let mine = projects.entry(key).or_insert_with(|| json!({}));
            if !mine.is_object() {
                *mine = json!({});
            }
            mine[name] = json!(entry_hash(e));
        }
        None => {
            if let Some(mine) = projects.get_mut(&key).and_then(Value::as_object_mut) {
                mine.shift_remove(name);
                if mine.is_empty() {
                    projects.shift_remove(&key);
                }
            }
        }
    }
    if all == before {
        return Ok(());
    }
    write_config(&path, &all)
}

/// Whether Harlo would start a project's server as its file has it now
/// (None: not a project's, or Harlo hasn't it).
pub fn harlo_approved(scope: &Scope, name: &str) -> Option<bool> {
    let Scope::Project(project) = scope else { return None };
    let entry = entry(HarnessKind::Harlo, scope, name).ok().flatten()?;
    let (path, key) = harlo_approvals(project);
    let all: Value = read_text(&path).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    Some(all[&key][name].as_str() == Some(entry_hash(&entry).as_str()))
}

/// A server as the settings page edits it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Form {
    pub name: String,
    pub enabled: bool,
    /// "stdio", "http" or "sse" (Claude only).
    pub transport: String,
    /// The program and its arguments (stdio).
    pub command: Vec<String>,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub timeout_secs: Option<f64>,
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()))).collect()).unwrap_or_default()
}

fn object(pairs: &[(String, String)]) -> Value {
    Value::Object(pairs.iter().filter(|(k, _)| !k.trim().is_empty()).map(|(k, v)| (k.trim().to_owned(), Value::String(v.clone()))).collect())
}

impl Form {
    /// From a harness's entry.
    pub fn from_entry(kind: HarnessKind, name: &str, v: &Value) -> Self {
        let url = v["url"].as_str().unwrap_or_default().to_owned();
        let transport = match v["type"].as_str().unwrap_or_default() {
            "sse" => "sse",
            "http" | "streamable-http" => "http",
            "stdio" => "stdio",
            _ if !url.is_empty() => "http",
            _ => "stdio",
        };
        let mut command: Vec<String> = match &v["command"] {
            Value::String(c) => vec![c.clone()],
            Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
            _ => Vec::new(),
        };
        command.extend(v["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(str::to_owned)));
        let timeout_secs = v["timeout"].as_f64().filter(|t| *t > 0.0);
        Form {
            name: name.to_owned(),
            // (Claude has no off switch.)
            enabled: kind == HarnessKind::Claude || v["enabled"].as_bool().unwrap_or(true),
            transport: transport.to_owned(),
            command,
            url,
            headers: pairs(&v["headers"]),
            env: pairs(&v["env"]),
            cwd: v["cwd"].as_str().unwrap_or_default().to_owned(),
            timeout_secs,
        }
    }

    /// The harness's entry: `existing` with the form's fields written in
    /// (others kept).
    pub fn to_entry(&self, kind: HarnessKind, existing: Option<&Value>) -> Value {
        let mut m: Map<String, Value> = existing.and_then(|e| e.as_object().cloned()).unwrap_or_default();
        let set = |m: &mut Map<String, Value>, key: &str, value: Option<Value>| match value {
            Some(v) => {
                m.insert(key.to_owned(), v);
            }
            None => {
                m.remove(key);
            }
        };
        let stdio = self.transport == "stdio";
        // The transport: Claude spells it out; Harlo infers it (written for http).
        let ty = match (kind, self.transport.as_str()) {
            (HarnessKind::Claude, t) => Some(json!(t)),
            (_, "stdio") => m.get("type").filter(|t| *t == "stdio").cloned(),
            (_, _) => Some(json!("http")),
        };
        set(&mut m, "type", ty);
        set(&mut m, "command", if stdio { self.command.first().map(|c| json!(c)) } else { None });
        set(&mut m, "args", if stdio && self.command.len() > 1 { Some(json!(self.command[1..])) } else { None });
        set(&mut m, "url", if stdio { None } else { Some(json!(self.url.trim())) });
        set(&mut m, "headers", if !stdio && !self.headers.is_empty() { Some(object(&self.headers)) } else { None });
        set(&mut m, "env", if self.env.is_empty() { None } else { Some(object(&self.env)) });
        if kind == HarnessKind::Claude {
            // No folder, timeout or switch in Claude's dialect.
            for gone in ["cwd", "timeout", "enabled"] {
                m.remove(gone);
            }
            return Value::Object(m);
        }
        set(&mut m, "cwd", if stdio && !self.cwd.trim().is_empty() { Some(json!(self.cwd.trim())) } else { None });
        let timeout = self.timeout_secs.filter(|t| *t > 0.0).map(|t| if t.fract() == 0.0 { json!(t as u64) } else { json!(t) });
        set(&mut m, "timeout", timeout);
        m.insert("enabled".into(), json!(self.enabled));
        Value::Object(m)
    }

    /// What's wrong with it for `kinds`, if anything.
    pub fn problem(&self, kinds: &[HarnessKind]) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("Name is required.".into());
        }
        if !self.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Some("Use letters, digits, - and _ in the name.".into());
        }
        if self.transport == "stdio" && self.command.first().is_none_or(|c| c.trim().is_empty()) {
            return Some("Enter the command that starts the server.".into());
        }
        if self.transport != "stdio" && !(self.url.starts_with("http://") || self.url.starts_with("https://")) {
            return Some("Enter the server's http:// or https:// address.".into());
        }
        if self.transport == "sse" && kinds.contains(&HarnessKind::Harlo) {
            return Some("Harlo doesn't support SSE servers; untick Harlo, or use http.".into());
        }
        if !self.enabled && kinds == [HarnessKind::Claude] {
            return Some("Claude has no off switch for a server; remove it instead.".into());
        }
        None
    }

    /// The parts both harnesses share, to tell copies apart.
    fn shared(&self) -> Value {
        let t = if self.transport == "stdio" { "stdio" } else { "remote" };
        json!([t, self.command, self.url, self.headers, self.env, self.cwd, self.enabled])
    }
}

/// A configured server, across harnesses.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    /// From Claude's copy when it has one, else Harlo's.
    pub form: Form,
    pub harnesses: Vec<HarnessKind>,
    /// The copies differ (command, address, variables or switch).
    pub differs: bool,
}

/// A scope's servers, and problems reading its files.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Listing {
    pub servers: Vec<Listed>,
    pub errors: Vec<String>,
}

fn read_text(path: &Path) -> Result<Option<String>> {
    if crate::remote::is_remote(path) {
        // Missing → nothing (cat fails); other trouble surfaces on write.
        return Ok(crate::remote::read(path).ok().map(|b| String::from_utf8_lossy(&b).into_owned()));
    }
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// A config file's JSON (`{}` when missing).
fn read_config(path: &Path) -> Result<Value> {
    match read_text(path)? {
        None => Ok(json!({})),
        Some(t) if t.trim().is_empty() => Ok(json!({})),
        Some(t) => {
            let v: Value = serde_json::from_str(&t).with_context(|| format!("{} isn't valid JSON", crate::sidebar::tilde(path)))?;
            if !v.is_object() {
                bail!("{} isn't a JSON object", crate::sidebar::tilde(path));
            }
            Ok(v)
        }
    }
}

/// Writes an agent's changed servers: Claude's global ones through `claude
/// mcp`, the rest into their file.
fn write_back(kind: HarnessKind, scope: &Scope, path: &Path, config: &Value, before: &Map<String, Value>) -> Result<()> {
    if kind == HarnessKind::Claude && *scope == Scope::Global {
        let after = config["mcpServers"].as_object().cloned().unwrap_or_default();
        return claude_sync(before, &after, None);
    }
    if let (HarnessKind::Claude, Scope::Host(name)) = (kind, scope) {
        let host = crate::remote::host(name).with_context(|| format!("unknown remote host {name}"))?;
        let after = config["mcpServers"].as_object().cloned().unwrap_or_default();
        return claude_sync(before, &after, Some(&host));
    }
    write_config(path, config)
}

/// Claude's global servers (here, or on a host), changed with `claude mcp
/// remove / add-json`.
fn claude_sync(before: &Map<String, Value>, after: &Map<String, Value>, host: Option<&crate::remote::Host>) -> Result<()> {
    let program = match host {
        Some(_) => PathBuf::from("claude"),
        None => CLAUDE.lock().unwrap().clone().or_else(|| harlo_harness::resolve_program(HarnessKind::Claude)).context("Claude Code isn't installed")?,
    };
    let run = |args: &[&str]| -> Result<()> {
        let out = match host {
            Some(h) => {
                let script = format!("claude {}", args.iter().map(|a| crate::remote::quote(a)).collect::<Vec<_>>().join(" "));
                crate::remote::run(h, &script, std::time::Duration::from_secs(60))?
            }
            None => std::process::Command::new(&program).args(args).stdin(std::process::Stdio::null()).output().with_context(|| format!("running {}", program.display()))?,
        };
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let say = String::from_utf8_lossy(&out.stdout);
            bail!("claude mcp {}: {}", args[1], err.lines().chain(say.lines()).map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("failed"));
        }
        Ok(())
    };
    for (name, old) in before {
        if after.get(name) != Some(old) {
            run(&["mcp", "remove", "-s", "user", name])?;
        }
    }
    for (name, new) in after {
        if before.get(name) != Some(new) {
            run(&["mcp", "add-json", "-s", "user", name, &new.to_string()])?;
        }
    }
    Ok(())
}

fn write_config(path: &Path, v: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(v)? + "\n";
    if let Some((name, p)) = crate::remote::split(path) {
        let host = crate::remote::host(&name).with_context(|| format!("unknown remote host {name}"))?;
        let dir = Path::new(&p).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
        crate::remote::run_ok(&host, &format!("mkdir -p {}", crate::remote::quote(&dir)), std::time::Duration::from_secs(20))?;
        return crate::remote::write(path, text.as_bytes());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    crate::files::write_text(path, &text).with_context(|| format!("writing {}", path.display()))
}

/// Both agents' servers in a scope.
pub fn load(scope: &Scope) -> Listing {
    let mut listing = Listing::default();
    for kind in HARNESSES {
        let path = config_path(kind, scope);
        let config = match read_config(&path) {
            Ok(c) => c,
            Err(e) => {
                listing.errors.push(format!("{e:#}"));
                continue;
            }
        };
        for (name, entry) in config[SERVERS].as_object().into_iter().flatten() {
            let form = Form::from_entry(kind, name, entry);
            match listing.servers.iter_mut().find(|s| s.form.name == *name) {
                Some(s) => {
                    s.differs |= s.form.shared() != form.shared();
                    s.harnesses.push(kind);
                }
                None => listing.servers.push(Listed { form, harnesses: vec![kind], differs: false }),
            }
        }
    }
    listing
}

/// Saves a server for `kinds` in a scope (renamed from `old_name`), and
/// removes it from the other harness's file.
pub fn save(scope: &Scope, old_name: &str, form: &Form, kinds: &[HarnessKind]) -> Result<()> {
    if let Some(p) = form.problem(kinds) {
        bail!(p);
    }
    if kinds.is_empty() {
        bail!("Tick at least one agent.");
    }
    for kind in HARNESSES {
        let path = config_path(kind, scope);
        let mut config = read_config(&path)?;
        let before = config[SERVERS].as_object().cloned().unwrap_or_default();
        let servers = config.as_object_mut().expect("object").entry(SERVERS).or_insert_with(|| json!({}));
        let Some(servers) = servers.as_object_mut() else { bail!("{}: mcpServers isn't an object", crate::sidebar::tilde(&path)) };
        let previous = servers.get(old_name).or_else(|| servers.get(&form.name)).cloned();
        let wanted = kinds.contains(&kind);
        if !wanted && previous.is_none() {
            continue;
        }
        if form.name != old_name && !old_name.is_empty() {
            if wanted && servers.contains_key(&form.name) {
                bail!("There is already a server called {}.", form.name);
            }
            servers.shift_remove(old_name);
        }
        if wanted {
            let entry = form.to_entry(kind, previous.as_ref());
            // Keeps its place in the file when it was there.
            match servers.get_mut(&form.name) {
                Some(slot) => *slot = entry,
                None => {
                    servers.insert(form.name.clone(), entry);
                }
            }
        } else {
            servers.shift_remove(&form.name);
        }
        if config[SERVERS].as_object() != Some(&before) {
            write_back(kind, scope, &path, &config, &before)?;
        }
        // (Saved here: Harlo may start it, as it is now.)
        if kind == HarnessKind::Harlo {
            if form.name != old_name && !old_name.is_empty() {
                approve(scope, old_name, None)?;
            }
            approve(scope, &form.name, config[SERVERS].get(&form.name))?;
        }
    }
    Ok(())
}

/// Turns a server on or off in every harness's file that has it (only its
/// `enabled` changes).
pub fn set_enabled(scope: &Scope, name: &str, on: bool) -> Result<()> {
    let (mut found, mut claude) = (false, false);
    for kind in HARNESSES {
        let path = config_path(kind, scope);
        let mut config = read_config(&path)?;
        let before = config[SERVERS].as_object().cloned().unwrap_or_default();
        let Some(entry) = config[SERVERS].get_mut(name).and_then(Value::as_object_mut) else { continue };
        // (Claude has no off switch.)
        if kind == HarnessKind::Claude {
            claude = true;
            continue;
        }
        entry.insert("enabled".into(), json!(on));
        found = true;
        write_back(kind, scope, &path, &config, &before)?;
        if kind == HarnessKind::Harlo {
            approve(scope, name, config[SERVERS].get(name))?;
        }
    }
    if !found {
        if claude {
            bail!("Claude has no off switch for a server; remove it instead.");
        }
        bail!("There's no server called {name} here.");
    }
    Ok(())
}

/// One harness's entry for a server (None: it hasn't one).
pub fn entry(kind: HarnessKind, scope: &Scope, name: &str) -> Result<Option<Value>> {
    let config = read_config(&config_path(kind, scope))?;
    Ok(config[SERVERS].get(name).cloned())
}

/// Writes one harness's entry for a server (None: removes it), leaving the
/// other harnesses' files alone.
pub fn put_entry(kind: HarnessKind, scope: &Scope, form: Option<&Form>, name: &str) -> Result<()> {
    let path = config_path(kind, scope);
    let mut config = read_config(&path)?;
    let before = config[SERVERS].as_object().cloned().unwrap_or_default();
    let mut servers = before.clone();
    match form {
        Some(form) => {
            let entry = form.to_entry(kind, before.get(name));
            match servers.get_mut(name) {
                Some(slot) => *slot = entry,
                None => {
                    servers.insert(name.to_owned(), entry);
                }
            }
        }
        None => {
            servers.shift_remove(name);
        }
    }
    if kind == HarnessKind::Harlo {
        approve(scope, name, servers.get(name))?;
    }
    if servers == before {
        return Ok(());
    }
    config.as_object_mut().expect("object").insert(SERVERS.to_owned(), Value::Object(servers));
    write_back(kind, scope, &path, &config, &before)
}

/// Removes a server from every agent's config in a scope.
pub fn remove(scope: &Scope, name: &str) -> Result<()> {
    for kind in HARNESSES {
        let path = config_path(kind, scope);
        let mut config = read_config(&path)?;
        let before = config[SERVERS].as_object().cloned().unwrap_or_default();
        let removed = config[SERVERS].as_object_mut().is_some_and(|m| m.shift_remove(name).is_some());
        if removed {
            write_back(kind, scope, &path, &config, &before)?;
        }
        if removed && kind == HarnessKind::Harlo {
            approve(scope, name, None)?;
        }
    }
    Ok(())
}

/// Servers from a pasted snippet: `{"mcpServers": {…}}` (Claude, Harlo and
/// most other tools), `{"mcp": {"servers": {…}}}` / `{"mcp": {…}}`,
/// `{name: {…}}`, or one server object (named "imported").
pub fn parse_import(text: &str) -> Result<Vec<Form>> {
    let text = text.trim();
    if text.is_empty() {
        bail!("Paste a JSON snippet first.");
    }
    let v: Value = serde_json::from_str(text).map_err(|e| anyhow::anyhow!("That isn't valid JSON: {e}"))?;
    let looks_like_server = |s: &Value| s.get("command").is_some() || s.get("url").is_some() || s.get("type").is_some();
    let servers: Vec<(String, Value)> = if let Some(m) = v["mcpServers"].as_object() {
        m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    } else if let Some(m) = v["mcp"]["servers"].as_object().or_else(|| v["mcp"].as_object()) {
        m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    } else if looks_like_server(&v) {
        vec![("imported".into(), v.clone())]
    } else if let Some(m) = v.as_object().filter(|m| !m.is_empty() && m.values().all(looks_like_server)) {
        m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    } else {
        bail!("No MCP server found in that JSON.");
    };
    if servers.is_empty() {
        bail!("The snippet has no servers.");
    }
    Ok(servers
        .into_iter()
        .map(|(name, s)| {
            // Another shape: `type: local|remote`, `command: [argv]`, `environment`.
            let mut s = s;
            if s["type"] == "local" {
                s["type"] = json!("stdio");
            } else if s["type"] == "remote" {
                s["type"] = json!("http");
            }
            if s.get("env").is_none() && s.get("environment").is_some() {
                s["env"] = s["environment"].clone();
            }
            if let Some(disabled) = s["disabled"].as_bool() {
                s["enabled"] = json!(!disabled);
            }
            let mut f = Form::from_entry(HarnessKind::Harlo, &name, &s);
            // Timeouts in other tools' files: milliseconds when large.
            f.timeout_secs = f.timeout_secs.map(|t| if t >= 1000.0 { t / 1000.0 } else { t });
            f
        })
        .collect())
}

/// Splits a pasted command line into words (quotes group).
pub fn split_command(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let (mut quote, mut in_word) = (None::<char>, false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => cur.extend(chars.next()),
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                in_word = true;
            }
            (None, '\\') => {
                cur.extend(chars.next());
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            (None, c) => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Harlo's config folder is one for the process: one test at a time.
    static HARLO_DIR: Mutex<()> = Mutex::new(());

    fn scratch() -> crate::scratch::Scratch {
        crate::scratch::Scratch::new(std::env::temp_dir().join(format!("harlo-mcp-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())))
    }

    #[test]
    fn harlos_project_servers_are_approved_as_saved() {
        let _one = HARLO_DIR.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch();
        let project = dir.join("proj");
        std::fs::create_dir_all(project.join(".harlo")).unwrap();
        let scope = Scope::Project(project.clone());
        let approvals = harlo_dir().join("mcp-approved.json");
        let read = || serde_json::from_str::<Value>(&std::fs::read_to_string(&approvals).unwrap_or_default()).unwrap_or_default();
        let key = project.to_string_lossy().into_owned();
        // One the repository brought: listed, not approved.
        std::fs::write(project.join(".harlo/mcp.json"), r#"{"mcpServers": {"came": {"command": "touch", "args": ["x"]}}}"#).unwrap();
        assert_eq!(harlo_approved(&scope, "came"), Some(false));
        assert!(load(&scope).servers.iter().any(|s| s.form.name == "came" && s.harnesses == [HarnessKind::Harlo]));
        // Saved here: written in Harlo's dialect, and approved as written.
        let form = Form { name: "fs".into(), enabled: true, transport: "stdio".into(), command: vec!["srv".into(), "--x".into()], timeout_secs: Some(5.0), ..Default::default() };
        save(&scope, "", &form, &[HarnessKind::Harlo]).unwrap();
        let entry = entry(HarnessKind::Harlo, &scope, "fs").unwrap().unwrap();
        assert_eq!(entry, json!({"command": "srv", "args": ["--x"], "timeout": 5, "enabled": true}));
        assert_eq!(read()[&key]["fs"], json!(entry_hash(&entry)));
        assert_eq!((harlo_approved(&scope, "fs"), harlo_approved(&scope, "came")), (Some(true), Some(false)));
        // Switched: approved as it is now; renamed: the old name forgotten.
        set_enabled(&scope, "came", true).unwrap();
        assert_eq!(harlo_approved(&scope, "came"), Some(true));
        save(&scope, "fs", &Form { name: "files".into(), ..form.clone() }, &[HarnessKind::Harlo]).unwrap();
        assert!(read()[&key].get("fs").is_none() && harlo_approved(&scope, "files") == Some(true));
        // Changed outside harlo: no longer approved.
        let text = std::fs::read_to_string(project.join(".harlo/mcp.json")).unwrap().replace("--x", "--evil");
        std::fs::write(project.join(".harlo/mcp.json"), text).unwrap();
        assert_eq!(harlo_approved(&scope, "files"), Some(false));
        // Removed: forgotten (the project too, once it has none).
        remove(&scope, "files").unwrap();
        remove(&scope, "came").unwrap();
        assert!(read().get(&key).is_none(), "{}", read());
        // No SSE for Harlo; this computer's and a host's files.
        assert!(Form { transport: "sse".into(), url: "https://h/sse".into(), ..form.clone() }.problem(&[HarnessKind::Harlo]).unwrap().contains("Harlo"));
        assert_eq!(config_path(HarnessKind::Harlo, &Scope::Global), harlo_dir().join("mcp.json"));
        assert!(config_path(HarnessKind::Harlo, &Scope::Host("not-a-host".into())).to_string_lossy().ends_with("/.config/harlo-agent/mcp.json"));
        let _ = std::fs::remove_dir_all(harlo_dir());
    }

    #[test]
    fn hashes_as_harlo_does() {
        // (The same as harlo-agent's: keys sorted, compact.)
        let a = entry_hash(&json!({"command": "srv", "env": {"B": "2", "A": "1"}}));
        assert_eq!(a, entry_hash(&json!({"env": {"A": "1", "B": "2"}, "command": "srv"})));
        use sha2::Digest;
        let want: String = sha2::Sha256::digest(br#"{"command":"srv","env":{"A":"1","B":"2"}}"#).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(a, want);
    }

    #[test]
    fn reads_both_dialects() {
        let claude = json!({"type":"stdio","command":"uvx","args":["blender-mcp"],"env":{"A":"1"}});
        let harlo = json!({"command":"uvx","args":["blender-mcp"],"env":{"A":"1"},"timeout":300,"enabled":true});
        let (fc, fh) = (Form::from_entry(HarnessKind::Claude, "blender", &claude), Form::from_entry(HarnessKind::Harlo, "blender", &harlo));
        assert_eq!(fc.command, ["uvx", "blender-mcp"]);
        assert_eq!((fc.timeout_secs, fh.timeout_secs), (None, Some(300.0)));
        assert_eq!(fc.shared(), fh.shared());
        let remote = Form::from_entry(HarnessKind::Harlo, "r", &json!({"url":"https://x/mcp","headers":{"Authorization":"Bearer t"}}));
        assert_eq!((remote.transport.as_str(), remote.headers.len()), ("http", 1));
    }

    #[test]
    fn writes_each_dialect_keeping_other_fields() {
        let mut f = Form { name: "s".into(), enabled: true, transport: "stdio".into(), command: vec!["npx".into(), "-y".into(), "srv".into()], timeout_secs: Some(30.0), ..Default::default() };
        let claude = f.to_entry(HarnessKind::Claude, Some(&json!({"oauth":{"clientId":"c"},"timeout":5})));
        assert_eq!(claude, json!({"oauth":{"clientId":"c"},"type":"stdio","command":"npx","args":["-y","srv"]}));
        let harlo = f.to_entry(HarnessKind::Harlo, Some(&json!({"note":"kept"})));
        assert_eq!(harlo, json!({"note":"kept","command":"npx","args":["-y","srv"],"timeout":30,"enabled":true}));
        f.transport = "http".into();
        f.url = "https://h/mcp".into();
        let remote = f.to_entry(HarnessKind::Harlo, Some(&harlo));
        assert!(remote.get("command").is_none() && remote["url"] == "https://h/mcp" && remote["type"] == "http");
    }

    #[test]
    fn saves_lists_and_removes_per_harness() {
        let _one = HARLO_DIR.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch();
        let scope = Scope::Project(dir.to_path_buf());
        // An existing file with other servers and its own key order.
        std::fs::write(dir.join(".mcp.json"), "{\n  \"$schema\": \"x\",\n  \"mcpServers\": {\n    \"zed\": {\"command\": \"z\"},\n    \"alpha\": {\"command\": \"a\"}\n  }\n}\n").unwrap();
        let form = Form { name: "files".into(), enabled: true, transport: "stdio".into(), command: vec!["mcp-files".into()], ..Default::default() };
        save(&scope, "", &form, &[HarnessKind::Claude, HarnessKind::Harlo]).unwrap();
        let claude: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(claude["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["zed", "alpha", "files"], "order kept");
        assert_eq!(claude["$schema"], "x");
        assert!(dir.join(".harlo/mcp.json").exists());
        let l = load(&scope);
        let files = l.servers.iter().find(|s| s.form.name == "files").unwrap();
        assert_eq!(files.harnesses, [HarnessKind::Claude, HarnessKind::Harlo]);
        assert!(!files.differs);
        // Rename, and only for Harlo: gone from Claude's.
        let renamed = Form { name: "files2".into(), ..form.clone() };
        save(&scope, "files", &renamed, &[HarnessKind::Harlo]).unwrap();
        let l = load(&scope);
        assert!(l.servers.iter().all(|s| s.form.name != "files"));
        assert_eq!(l.servers.iter().find(|s| s.form.name == "files2").unwrap().harnesses, [HarnessKind::Harlo]);
        // A name taken by another server is refused.
        assert!(save(&scope, "files2", &Form { name: "alpha".into(), ..renamed.clone() }, &[HarnessKind::Claude]).is_err());
        set_enabled(&scope, "files2", false).unwrap();
        let harlo: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".harlo/mcp.json")).unwrap()).unwrap();
        assert_eq!(harlo["mcpServers"]["files2"]["enabled"], false);
        assert!(!load(&scope).servers.iter().find(|s| s.form.name == "files2").unwrap().form.enabled);
        assert!(set_enabled(&scope, "nope", true).is_err());
        remove(&scope, "files2").unwrap();
        assert!(load(&scope).servers.iter().all(|s| s.form.name != "files2"));
        // A broken file is reported, not overwritten.
        std::fs::write(dir.join(".harlo/mcp.json"), "{oops").unwrap();
        assert_eq!(load(&scope).errors.len(), 1);
        assert!(save(&scope, "", &form, &[HarnessKind::Harlo]).is_err());
        std::fs::remove_dir_all(dir).unwrap();
        let _ = std::fs::remove_dir_all(harlo_dir());
    }

    #[test]
    fn imports_snippets() {
        let claude = r#"{"mcpServers":{"fs":{"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","/tmp"]},"web":{"type":"http","url":"https://x/mcp"}}}"#;
        let forms = parse_import(claude).unwrap();
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].command, ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]);
        assert_eq!(forms[1].transport, "http");
        let other = r#"{"mcp":{"gh":{"type":"local","command":["gh-mcp","serve"],"environment":{"T":"1"},"disabled":true}}}"#;
        let f = &parse_import(other).unwrap()[0];
        assert_eq!((f.command.clone(), f.env.len(), f.enabled), (vec!["gh-mcp".to_owned(), "serve".to_owned()], 1, false));
        assert_eq!(parse_import(r#"{"command":"x"}"#).unwrap()[0].name, "imported");
        assert!(parse_import("nope").is_err() && parse_import("{}").is_err());
    }

    #[test]
    fn splits_command_lines() {
        assert_eq!(split_command(r#"npx -y "@scope/pkg" --dir '/a b' c\ d"#), ["npx", "-y", "@scope/pkg", "--dir", "/a b", "c d"]);
    }

    #[test]
    fn checks_forms() {
        let mut f = Form { name: "ok".into(), transport: "sse".into(), url: "https://x".into(), enabled: true, ..Default::default() };
        assert!(f.problem(&[HarnessKind::Claude]).is_none());
        assert!(f.problem(&[HarnessKind::Harlo]).is_some());
        f.name = "bad name".into();
        assert!(f.problem(&[HarnessKind::Claude]).is_some());
    }
}

#[cfg(test)]
mod real_config {
    /// Lists this machine's global servers, read-only (not run by default).
    #[test]
    #[ignore]
    fn list_global() {
        let l = super::load(&super::Scope::Global);
        for s in &l.servers {
            println!("{} {:?} differs={} transport={} timeout={:?}", s.form.name, s.harnesses, s.differs, s.form.transport, s.form.timeout_secs);
        }
        println!("errors: {:?}", l.errors);
    }
}
