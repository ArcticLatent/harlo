//! MCP servers for omp, pi, Claude, OpenCode and Hermes (Settings → MCP),
//! globally and per project:
//!
//! | | global | project |
//! |---|---|---|
//! | omp | `~/.omp/agent/mcp.json` | `<project>/.omp/mcp.json` |
//! | pi | `~/.pi/agent/mcp.json` (`$PI_CODING_AGENT_DIR`) | `<project>/.pi/mcp.json` (trusted projects) |
//! | Claude | `~/.claude.json` (`$CLAUDE_CONFIG_DIR`), changed with `claude mcp` | `<project>/.mcp.json` |
//! | OpenCode | `~/.config/opencode/opencode.json[c]` | `<project>/opencode.json[c]` |
//! | Hermes | `~/.hermes/config.yaml` (`$HERMES_HOME`), changed with `hermes config` | (none) |
//!
//! harlo shows one list for all and edits each agent's file in its own
//! dialect: omp's and OpenCode's timeouts are in milliseconds, pi's in
//! seconds; pi has no SSE; only pi has `exposure` and `description`;
//! Claude has no off switch; OpenCode's servers are `local` (an argv) or
//! `remote`, with `environment`. Fields harlo doesn't edit (OAuth settings,
//! pi's per-tool exposure, …) are kept as they are. Claude Code rewrites
//! its `~/.claude.json` as it runs, so harlo changes its global servers with
//! Claude's own `claude mcp add-json / remove`; a file with comments
//! (OpenCode's `.jsonc`) is read but not rewritten. Hermes' YAML has
//! comments too: its servers are read and written with `hermes config
//! get / set / unset`, which keep them.

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
pub const HARNESSES: [HarnessKind; 5] = [HarnessKind::Omp, HarnessKind::Pi, HarnessKind::Claude, HarnessKind::OpenCode, HarnessKind::Hermes];

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
        (HarnessKind::OpenCode, Scope::Host(h)) => opencode_file(&host_home(h).join(".config/opencode")),
        (HarnessKind::Hermes, Scope::Host(h)) => host_home(h).join(".hermes/config.yaml"),
        (HarnessKind::Hermes, Scope::Global) => hermes_home().join("config.yaml"),
        // (Hermes has no project servers: a name nothing is read from.)
        (HarnessKind::Hermes, Scope::Project(p)) => p.join(".hermes-has-no-project-servers"),
        (HarnessKind::Pi, Scope::Host(h)) => host_home(h).join(".pi/agent/mcp.json"),
        (_, Scope::Host(h)) => host_home(h).join(".omp/agent/mcp.json"),
        (HarnessKind::OpenCode, Scope::Global) => opencode_file(&std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("opencode")),
        (HarnessKind::OpenCode, Scope::Project(p)) => opencode_file(p),
        (HarnessKind::Pi, Scope::Global) => std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".pi/agent")).join("mcp.json"),
        (HarnessKind::Pi, Scope::Project(p)) => p.join(".pi/mcp.json"),
        (_, Scope::Global) => home().join(".omp/agent/mcp.json"),
        (_, Scope::Project(p)) => p.join(".omp/mcp.json"),
    }
}

/// OpenCode's config in a folder: `opencode.json`, else `opencode.jsonc`
/// when that's the one there.
fn opencode_file(dir: &Path) -> PathBuf {
    let json = dir.join("opencode.json");
    let jsonc = dir.join("opencode.jsonc");
    if !crate::remote::is_remote(dir) && !json.exists() && jsonc.exists() { jsonc } else { json }
}

/// Where in the file the servers are.
fn servers_key(kind: HarnessKind) -> &'static str {
    match kind {
        HarnessKind::OpenCode => "mcp",
        HarnessKind::Hermes => "mcp_servers",
        _ => "mcpServers",
    }
}

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".hermes"))
}

/// Runs `hermes config …` here, or on the host a remote path is on; its
/// output. None: Hermes isn't there.
fn hermes(path: &Path, args: &[&str]) -> Result<Option<String>> {
    let out = match crate::remote::split(path) {
        Some((name, _)) => {
            if !crate::remote::found_on(&name, "hermes") {
                return Ok(None);
            }
            let host = crate::remote::host(&name).with_context(|| format!("unknown remote host {name}"))?;
            let script = format!("hermes {}", args.iter().map(|a| crate::remote::quote(a)).collect::<Vec<_>>().join(" "));
            crate::remote::run(&host, &script, std::time::Duration::from_secs(60))?
        }
        None => {
            let Some(program) = harlo_harness::resolve_program(HarnessKind::Hermes) else { return Ok(None) };
            std::process::Command::new(&program).args(args).stdin(std::process::Stdio::null()).output().with_context(|| format!("running {}", program.display()))?
        }
    };
    let say = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("hermes config {}: {}", args[1], err.lines().chain(say.lines()).map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("failed"));
    }
    Ok(Some(say))
}

/// The last read of Hermes' servers, by config path: what was read, the
/// file's stamp then (here), and when. A read runs Hermes (half a second),
/// and the MCP page reads several times as it opens.
type HermesRead = (Value, Option<std::time::SystemTime>, std::time::Instant);
static HERMES_READS: Mutex<Option<std::collections::HashMap<PathBuf, HermesRead>>> = Mutex::new(None);

/// When the file last changed (here; None on a remote host).
fn stamp(path: &Path) -> Option<std::time::SystemTime> {
    (!crate::remote::is_remote(path)).then(|| std::fs::metadata(path).and_then(|m| m.modified()).ok()).flatten()
}

/// Hermes' servers (none in a project; `{}` when it has none or isn't
/// there): the last read while the file is unchanged (on a remote host,
/// for 15 s).
fn hermes_servers(path: &Path) -> Result<Value> {
    if path.ends_with(".hermes-has-no-project-servers") {
        return Ok(json!({}));
    }
    let now = stamp(path);
    if let Some((v, at, when)) = HERMES_READS.lock().unwrap().get_or_insert_with(Default::default).get(path) {
        let fresh = if crate::remote::is_remote(path) { when.elapsed() < std::time::Duration::from_secs(15) } else { *at == now };
        if fresh {
            return Ok(v.clone());
        }
    }
    let v = read_hermes_servers(path)?;
    HERMES_READS.lock().unwrap().get_or_insert_with(Default::default).insert(path.to_path_buf(), (v.clone(), now, std::time::Instant::now()));
    Ok(v)
}

fn read_hermes_servers(path: &Path) -> Result<Value> {
    // (It fails with "Config key not set: mcp_servers" when there are none.)
    let out = match hermes(path, &["config", "get", "mcp_servers", "--json", "--raw"]) {
        Ok(Some(out)) => out,
        Ok(None) => return Ok(json!({})),
        Err(e) if format!("{e:#}").contains("not set") => return Ok(json!({ "mcp_servers": {} })),
        Err(e) => return Err(e),
    };
    let servers = out.lines().find(|l| l.trim_start().starts_with('{')).and_then(|l| serde_json::from_str::<Value>(l).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
    Ok(json!({ "mcp_servers": servers }))
}

/// Hermes' changed servers, with `hermes config set / unset`.
fn hermes_sync(path: &Path, before: &Map<String, Value>, after: &Map<String, Value>) -> Result<()> {
    if path.ends_with(".hermes-has-no-project-servers") {
        bail!("Hermes has no project MCP servers; add it under All projects.");
    }
    for name in before.keys().filter(|n| !after.contains_key(*n)) {
        hermes(path, &["config", "unset", &format!("mcp_servers.{name}")])?;
    }
    // (Read again next time: a remote host's file has no stamp to tell.)
    HERMES_READS.lock().unwrap().get_or_insert_with(Default::default).remove(path);
    for (name, new) in after {
        if before.get(name) != Some(new) {
            hermes(path, &["config", "set", &format!("mcp_servers.{name}"), &new.to_string()])?.context("Hermes isn't installed")?;
        }
    }
    HERMES_READS.lock().unwrap().get_or_insert_with(Default::default).remove(path);
    Ok(())
}

/// A server as the settings page edits it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Form {
    pub name: String,
    pub enabled: bool,
    /// "stdio", "http" or "sse" (omp only).
    pub transport: String,
    /// The program and its arguments (stdio).
    pub command: Vec<String>,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub timeout_secs: Option<f64>,
    /// pi: codemode (default), deferred, direct, hidden.
    pub exposure: String,
    /// pi: shown to the model.
    pub description: String,
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
        // OpenCode: `local` (a command) or `remote` (an address).
        let declared = match v["type"].as_str().unwrap_or_default() {
            "local" => "stdio",
            "remote" => "http",
            t => t,
        };
        let transport = match declared {
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
        let in_ms = matches!(kind, HarnessKind::Omp | HarnessKind::OpenCode);
        let timeout_secs = v["timeout"].as_f64().filter(|t| *t > 0.0).map(|t| if in_ms { t / 1000.0 } else { t });
        Form {
            name: name.to_owned(),
            // (Claude has no off switch.)
            enabled: kind == HarnessKind::Claude || v["enabled"].as_bool().unwrap_or(true),
            transport: transport.to_owned(),
            command,
            url,
            headers: pairs(&v["headers"]),
            env: pairs(if kind == HarnessKind::OpenCode { &v["environment"] } else { &v["env"] }),
            cwd: v["cwd"].as_str().unwrap_or_default().to_owned(),
            timeout_secs,
            exposure: v["exposure"].as_str().unwrap_or_default().to_owned(),
            description: v["description"].as_str().unwrap_or_default().to_owned(),
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
        if kind == HarnessKind::OpenCode {
            set(&mut m, "type", Some(json!(if stdio { "local" } else { "remote" })));
            set(&mut m, "command", stdio.then(|| json!(self.command)));
            set(&mut m, "url", (!stdio).then(|| json!(self.url.trim())));
            set(&mut m, "headers", (!stdio && !self.headers.is_empty()).then(|| object(&self.headers)));
            set(&mut m, "environment", (stdio && !self.env.is_empty()).then(|| object(&self.env)));
            set(&mut m, "timeout", self.timeout_secs.filter(|t| *t > 0.0).map(|t| json!((t * 1000.0).round() as u64)));
            for gone in ["args", "env", "cwd"] {
                m.remove(gone);
            }
            m.insert("enabled".into(), json!(self.enabled));
            return Value::Object(m);
        }
        // The transport: omp and Claude spell it out; pi infers it (written for http).
        let ty = match (kind, self.transport.as_str()) {
            // (Hermes tells by `command` or `url`.)
            (HarnessKind::Hermes, _) => None,
            (HarnessKind::Omp | HarnessKind::Claude, t) => Some(json!(t)),
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
        let timeout = self.timeout_secs.filter(|t| *t > 0.0).map(|t| match kind {
            HarnessKind::Omp => json!((t * 1000.0).round() as u64),
            _ => if t.fract() == 0.0 { json!(t as u64) } else { json!(t) },
        });
        set(&mut m, "timeout", timeout);
        if kind == HarnessKind::Pi {
            set(&mut m, "exposure", (!self.exposure.is_empty()).then(|| json!(self.exposure)));
            set(&mut m, "description", (!self.description.trim().is_empty()).then(|| json!(self.description.trim())));
        }
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
        if self.transport == "sse" && kinds.contains(&HarnessKind::Pi) {
            return Some("pi doesn't support SSE servers; untick pi, or use http.".into());
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
    /// From omp's copy when it has one, else pi's.
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

/// A config file's JSON (`{}` when missing), and whether it's plain JSON
/// (OpenCode's may have comments and trailing commas: read, not rewritten).
fn read_config(kind: HarnessKind, path: &Path) -> Result<(Value, bool)> {
    if kind == HarnessKind::Hermes {
        return Ok((hermes_servers(path)?, true));
    }
    match read_text(path)? {
        None => Ok((json!({}), true)),
        Some(t) if t.trim().is_empty() => Ok((json!({}), true)),
        Some(t) => {
            let text = if kind == HarnessKind::OpenCode { strip_jsonc(&t) } else { t.clone() };
            let v: Value = serde_json::from_str(&text).with_context(|| format!("{} isn't valid JSON", crate::sidebar::tilde(path)))?;
            if !v.is_object() {
                bail!("{} isn't a JSON object", crate::sidebar::tilde(path));
            }
            Ok((v, text == t))
        }
    }
}

/// JSON with comments (`//`, `/* */`) and trailing commas → JSON.
pub fn strip_jsonc(text: &str) -> String {
    // Comments out (strings kept as they are).
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            plain.push(c);
            if c == '\\' {
                plain.extend(chars.next());
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                plain.push(c);
            }
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        plain.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for n in chars.by_ref() {
                    if last == '*' && n == '/' {
                        break;
                    }
                    last = n;
                }
            }
            _ => plain.push(c),
        }
    }
    // Then commas just before } or ].
    let chars: Vec<char> = plain.chars().collect();
    let mut out = String::with_capacity(plain.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                i += 1;
                out.push(chars[i]);
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == ',' && chars[i + 1..].iter().find(|n| !n.is_whitespace()).is_some_and(|n| *n == '}' || *n == ']') {
            // dropped
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Writes an agent's changed servers: Claude's global ones through `claude
/// mcp`, the rest into their file (not a file with comments).
fn write_back(kind: HarnessKind, scope: &Scope, path: &Path, config: &Value, plain: bool, before: &Map<String, Value>) -> Result<()> {
    if kind == HarnessKind::Hermes {
        return hermes_sync(path, before, &config["mcp_servers"].as_object().cloned().unwrap_or_default());
    }
    if kind == HarnessKind::Claude && *scope == Scope::Global {
        let after = config["mcpServers"].as_object().cloned().unwrap_or_default();
        return claude_sync(before, &after, None);
    }
    if let (HarnessKind::Claude, Scope::Host(name)) = (kind, scope) {
        let host = crate::remote::host(name).with_context(|| format!("unknown remote host {name}"))?;
        let after = config["mcpServers"].as_object().cloned().unwrap_or_default();
        return claude_sync(before, &after, Some(&host));
    }
    if !plain {
        bail!("{} has comments, which rewriting it would lose; change its MCP servers there", crate::sidebar::tilde(path));
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

/// Both harnesses' servers in a scope.
pub fn load(scope: &Scope) -> Listing {
    let mut listing = Listing::default();
    for kind in HARNESSES {
        let path = config_path(kind, scope);
        let config = match read_config(kind, &path) {
            Ok((c, _)) => c,
            Err(e) => {
                listing.errors.push(format!("{e:#}"));
                continue;
            }
        };
        for (name, entry) in config[servers_key(kind)].as_object().into_iter().flatten() {
            let form = Form::from_entry(kind, name, entry);
            match listing.servers.iter_mut().find(|s| s.form.name == *name) {
                Some(s) => {
                    s.differs |= s.form.shared() != form.shared();
                    s.harnesses.push(kind);
                    // pi-only fields come from pi's copy.
                    if kind == HarnessKind::Pi {
                        s.form.exposure = form.exposure;
                        s.form.description = form.description;
                    }
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
        let (mut config, plain) = read_config(kind, &path)?;
        let before = config[servers_key(kind)].as_object().cloned().unwrap_or_default();
        let servers = config.as_object_mut().expect("object").entry(servers_key(kind)).or_insert_with(|| json!({}));
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
        if config[servers_key(kind)].as_object() != Some(&before) {
            write_back(kind, scope, &path, &config, plain, &before)?;
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
        let (mut config, plain) = read_config(kind, &path)?;
        let before = config[servers_key(kind)].as_object().cloned().unwrap_or_default();
        let Some(entry) = config[servers_key(kind)].get_mut(name).and_then(Value::as_object_mut) else { continue };
        // (Claude has no off switch.)
        if kind == HarnessKind::Claude {
            claude = true;
            continue;
        }
        entry.insert("enabled".into(), json!(on));
        found = true;
        write_back(kind, scope, &path, &config, plain, &before)?;
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
    let (config, _) = read_config(kind, &config_path(kind, scope))?;
    Ok(config[servers_key(kind)].get(name).cloned())
}

/// Writes one harness's entry for a server (None: removes it), leaving the
/// other harnesses' files alone.
pub fn put_entry(kind: HarnessKind, scope: &Scope, form: Option<&Form>, name: &str) -> Result<()> {
    let path = config_path(kind, scope);
    let (mut config, plain) = read_config(kind, &path)?;
    let before = config[servers_key(kind)].as_object().cloned().unwrap_or_default();
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
    if servers == before {
        return Ok(());
    }
    config.as_object_mut().expect("object").insert(servers_key(kind).to_owned(), Value::Object(servers));
    write_back(kind, scope, &path, &config, plain, &before)
}

/// Removes a server from every agent's config in a scope.
pub fn remove(scope: &Scope, name: &str) -> Result<()> {
    for kind in HARNESSES {
        let path = config_path(kind, scope);
        let (mut config, plain) = read_config(kind, &path)?;
        let before = config[servers_key(kind)].as_object().cloned().unwrap_or_default();
        let removed = config[servers_key(kind)].as_object_mut().is_some_and(|m| m.shift_remove(name).is_some());
        if removed {
            write_back(kind, scope, &path, &config, plain, &before)?;
        }
    }
    Ok(())
}

/// Servers from a pasted snippet: `{"mcpServers": {…}}` (Claude, Cursor,
/// pi, omp), `{"mcp": {"servers": {…}}}` / `{"mcp": {…}}` (OpenCode),
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
            // OpenCode: `type: local|remote`, `command: [argv]`, `environment`.
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
            let mut f = Form::from_entry(HarnessKind::Pi, &name, &s);
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

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!("harlo-mcp-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn claude_and_opencode_dialects() {
        // OpenCode: local (an argv) / remote, environment, ms.
        let oc = json!({"type":"local","command":["uvx","blender-mcp"],"environment":{"A":"1"},"timeout":30000,"enabled":false,"x":1});
        let f = Form::from_entry(HarnessKind::OpenCode, "blender", &oc);
        assert_eq!((f.command.clone(), f.env.clone(), f.timeout_secs, f.enabled), (vec!["uvx".to_owned(), "blender-mcp".to_owned()], vec![("A".to_owned(), "1".to_owned())], Some(30.0), false));
        assert_eq!(f.to_entry(HarnessKind::OpenCode, Some(&oc)), oc);
        // Claude: spelled-out type, no switch, folder or timeout.
        let claude = f.to_entry(HarnessKind::Claude, None);
        assert_eq!(claude, json!({"type":"stdio","command":"uvx","args":["blender-mcp"],"env":{"A":"1"}}));
        assert!(Form::from_entry(HarnessKind::Claude, "b", &claude).enabled);
        let remote = Form { name: "r".into(), enabled: true, transport: "http".into(), url: "https://h/mcp".into(), ..Default::default() };
        assert_eq!(remote.to_entry(HarnessKind::OpenCode, None), json!({"type":"remote","url":"https://h/mcp","enabled":true}));
        assert_eq!(Form::from_entry(HarnessKind::OpenCode, "r", &json!({"type":"remote","url":"https://h/mcp"})).transport, "http");
        assert!(Form { enabled: false, ..remote.clone() }.problem(&[HarnessKind::Claude]).is_some());
    }

    #[test]
    fn jsonc() {
        let text = "{\n  // a comment\n  \"a\": \"// not one\", /* block */ \"b\": [1, 2,],\n  \"c\": {\"d\": \"x\\\"y\",},\n}\n";
        let v: Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
        assert_eq!(v, json!({"a":"// not one","b":[1,2],"c":{"d":"x\"y"}}));
        assert_eq!(strip_jsonc("{\"a\": 1}"), "{\"a\": 1}");
    }

    #[test]
    fn reads_both_dialects() {
        let omp = json!({"type":"stdio","command":"uvx","args":["blender-mcp"],"env":{"A":"1"},"timeout":300000,"enabled":true});
        let pi = json!({"command":"uvx","args":["blender-mcp"],"env":{"A":"1"},"timeout":300,"exposure":"direct","description":"Blender","enabled":true});
        let (fo, fp) = (Form::from_entry(HarnessKind::Omp, "blender", &omp), Form::from_entry(HarnessKind::Pi, "blender", &pi));
        assert_eq!(fo.command, ["uvx", "blender-mcp"]);
        assert_eq!((fo.timeout_secs, fp.timeout_secs), (Some(300.0), Some(300.0)));
        assert_eq!(fo.shared(), fp.shared());
        assert_eq!((fp.exposure.as_str(), fp.description.as_str()), ("direct", "Blender"));
        let remote = Form::from_entry(HarnessKind::Pi, "r", &json!({"url":"https://x/mcp","headers":{"Authorization":"Bearer t"}}));
        assert_eq!((remote.transport.as_str(), remote.headers.len()), ("http", 1));
    }

    #[test]
    fn writes_each_dialect_keeping_other_fields() {
        let mut f = Form { name: "s".into(), enabled: true, transport: "stdio".into(), command: vec!["npx".into(), "-y".into(), "srv".into()], timeout_secs: Some(30.0), exposure: "direct".into(), ..Default::default() };
        let omp = f.to_entry(HarnessKind::Omp, Some(&json!({"oauth":{"clientId":"c"},"requestIdFormat":"string"})));
        assert_eq!(omp, json!({"oauth":{"clientId":"c"},"requestIdFormat":"string","type":"stdio","command":"npx","args":["-y","srv"],"timeout":30000,"enabled":true}));
        let pi = f.to_entry(HarnessKind::Pi, Some(&json!({"toolExposure":{"a":"hidden"}})));
        assert_eq!(pi, json!({"toolExposure":{"a":"hidden"},"command":"npx","args":["-y","srv"],"timeout":30,"exposure":"direct","enabled":true}));
        f.transport = "http".into();
        f.url = "https://h/mcp".into();
        let remote = f.to_entry(HarnessKind::Pi, Some(&pi));
        assert!(remote.get("command").is_none() && remote["url"] == "https://h/mcp" && remote["type"] == "http");
    }

    /// Hermes' dialect: no `type` (a command or an address says it), its
    /// timeout in seconds; no project servers.
    #[test]
    fn hermes_entries() {
        let v = json!({ "command": "npx", "args": ["-y", "@x/s"], "env": { "T": "1" }, "timeout": 30, "enabled": false });
        let f = Form::from_entry(HarnessKind::Hermes, "s", &v);
        assert_eq!((f.transport.as_str(), f.command.len(), f.timeout_secs, f.enabled), ("stdio", 3, Some(30.0), false));
        assert_eq!(f.to_entry(HarnessKind::Hermes, Some(&v)), v);
        let web = Form { name: "w".into(), enabled: true, transport: "http".into(), url: "https://h/mcp".into(), ..Default::default() };
        assert_eq!(web.to_entry(HarnessKind::Hermes, None), json!({ "url": "https://h/mcp", "enabled": true }));
        let scope = Scope::Project(scratch());
        assert_eq!(read_config(HarnessKind::Hermes, &config_path(HarnessKind::Hermes, &scope)).unwrap().0, json!({}));
        assert!(save(&scope, "", &web, &[HarnessKind::Hermes]).is_err());
    }

    #[test]
    fn saves_lists_and_removes_per_harness() {
        let dir = scratch();
        let scope = Scope::Project(dir.clone());
        // An existing omp file with another server and its own key order.
        std::fs::create_dir_all(dir.join(".omp")).unwrap();
        std::fs::write(dir.join(".omp/mcp.json"), "{\n  \"$schema\": \"x\",\n  \"mcpServers\": {\n    \"zed\": {\"command\": \"z\"},\n    \"alpha\": {\"command\": \"a\"}\n  }\n}\n").unwrap();
        let form = Form { name: "files".into(), enabled: true, transport: "stdio".into(), command: vec!["mcp-files".into()], ..Default::default() };
        save(&scope, "", &form, &[HarnessKind::Omp, HarnessKind::Pi]).unwrap();
        let omp: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".omp/mcp.json")).unwrap()).unwrap();
        assert_eq!(omp["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["zed", "alpha", "files"], "order kept");
        assert_eq!(omp["$schema"], "x");
        assert!(dir.join(".pi/mcp.json").exists());
        let l = load(&scope);
        let files = l.servers.iter().find(|s| s.form.name == "files").unwrap();
        assert_eq!(files.harnesses, [HarnessKind::Omp, HarnessKind::Pi]);
        assert!(!files.differs);
        // Rename, and only for pi: gone from omp.
        let renamed = Form { name: "files2".into(), ..form.clone() };
        save(&scope, "files", &renamed, &[HarnessKind::Pi]).unwrap();
        let l = load(&scope);
        assert!(l.servers.iter().all(|s| s.form.name != "files"));
        assert_eq!(l.servers.iter().find(|s| s.form.name == "files2").unwrap().harnesses, [HarnessKind::Pi]);
        // A name taken by another server is refused.
        assert!(save(&scope, "files2", &Form { name: "alpha".into(), ..renamed.clone() }, &[HarnessKind::Omp]).is_err());
        set_enabled(&scope, "files2", false).unwrap();
        let pi: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".pi/mcp.json")).unwrap()).unwrap();
        assert_eq!(pi["mcpServers"]["files2"]["enabled"], false);
        assert!(!load(&scope).servers.iter().find(|s| s.form.name == "files2").unwrap().form.enabled);
        assert!(set_enabled(&scope, "nope", true).is_err());
        remove(&scope, "files2").unwrap();
        assert!(load(&scope).servers.iter().all(|s| s.form.name != "files2"));
        // A broken file is reported, not overwritten.
        std::fs::write(dir.join(".pi/mcp.json"), "{oops").unwrap();
        assert_eq!(load(&scope).errors.len(), 1);
        assert!(save(&scope, "", &form, &[HarnessKind::Pi]).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_snippets() {
        let claude = r#"{"mcpServers":{"fs":{"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","/tmp"]},"web":{"type":"http","url":"https://x/mcp"}}}"#;
        let forms = parse_import(claude).unwrap();
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].command, ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]);
        assert_eq!(forms[1].transport, "http");
        let opencode = r#"{"mcp":{"gh":{"type":"local","command":["gh-mcp","serve"],"environment":{"T":"1"},"disabled":true}}}"#;
        let f = &parse_import(opencode).unwrap()[0];
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
        let mut f = Form { name: "ok".into(), transport: "sse".into(), url: "https://x".into(), ..Default::default() };
        assert!(f.problem(&[HarnessKind::Omp]).is_none());
        assert!(f.problem(&[HarnessKind::Pi]).is_some());
        f.name = "bad name".into();
        assert!(f.problem(&[HarnessKind::Omp]).is_some());
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
            println!("{} {:?} differs={} transport={} timeout={:?} exposure={}", s.form.name, s.harnesses, s.differs, s.form.transport, s.form.timeout_secs, s.form.exposure);
        }
        println!("errors: {:?}", l.errors);
    }

    /// A server saved, switched off, listed and removed with the real
    /// `hermes`, in a scratch Hermes home (`HERMES_HOME`, never yours):
    /// `HERMES_HOME=<scratch copy> cargo test -p harlo-core hermes_round_trip -- --ignored`.
    #[test]
    #[ignore]
    fn hermes_round_trip() {
        use super::*;
        let home = std::env::var("HERMES_HOME").expect("HERMES_HOME: a scratch copy");
        assert!(!home.ends_with("/.hermes"), "not the real one");
        let form = Form { name: "harlo-test".into(), enabled: true, transport: "stdio".into(), command: vec!["npx".into(), "-y".into(), "@x/s".into()], env: vec![("K".into(), "v".into())], ..Default::default() };
        save(&Scope::Global, "", &form, &[HarnessKind::Hermes]).unwrap();
        set_enabled(&Scope::Global, "harlo-test", false).unwrap();
        let listed = load(&Scope::Global).servers.into_iter().find(|s| s.form.name == "harlo-test").expect("listed");
        assert_eq!((listed.harnesses.as_slice(), listed.form.enabled, listed.form.command.len()), ([HarnessKind::Hermes].as_slice(), false, 3));
        remove(&Scope::Global, "harlo-test").unwrap();
        assert!(load(&Scope::Global).servers.iter().all(|s| s.form.name != "harlo-test"));
    }
}
