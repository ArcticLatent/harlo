//! pi's MCP servers (Settings → Tools for agents).
//!
//! Globally and per project: `~/.pi/agent/mcp.json` (`$PI_CODING_AGENT_DIR`),
//! on a host its own, and `<project>/.pi/mcp.json` (read once the project is
//! trusted in pi).
//!
//! harlo edits the file in pi's dialect (no transport field; timeouts in
//! seconds). Fields harlo doesn't edit (OAuth settings, …) are kept as they
//! are.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
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

#[cfg(not(test))]
fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// pi's MCP file for a scope.
pub fn config_path(scope: &Scope) -> PathBuf {
    match scope {
        Scope::Global => pi_dir().join("mcp.json"),
        Scope::Project(p) => p.join(".pi/mcp.json"),
        Scope::Host(h) => host_home(h).join(".pi/agent/mcp.json"),
    }
}

/// Where in the file the servers are.
const SERVERS: &str = "mcpServers";

/// pi's folder here (a scratch one in tests).
#[cfg(test)]
pub(crate) fn pi_dir() -> PathBuf {
    std::env::temp_dir().join(format!("harlo-mcp-test-pi-{}", std::process::id()))
}

#[cfg(not(test))]
pub(crate) fn pi_dir() -> PathBuf {
    std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".pi/agent"))
}

/// A server as the settings page edits it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Form {
    pub name: String,
    pub enabled: bool,
    /// "stdio", "http" or "sse".
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
    v.as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().map_or_else(|| v.to_string(), str::to_owned))).collect()).unwrap_or_default()
}

fn object(pairs: &[(String, String)]) -> Value {
    Value::Object(pairs.iter().filter(|(k, _)| !k.trim().is_empty()).map(|(k, v)| (k.trim().to_owned(), Value::String(v.clone()))).collect())
}

impl Form {
    /// From an entry in the file.
    pub fn from_entry(name: &str, v: &Value) -> Self {
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
            enabled: v["enabled"].as_bool().unwrap_or(true),
            transport: transport.to_owned(),
            command,
            url,
            headers: pairs(&v["headers"]),
            env: pairs(&v["env"]),
            cwd: v["cwd"].as_str().unwrap_or_default().to_owned(),
            timeout_secs,
        }
    }

    /// The entry in pi's dialect: `existing` with the form's fields written
    /// in (others kept).
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "whole pixels, sizes and times from floats: `as` rounds toward zero and saturates, as wanted here")]
    pub fn to_entry(&self, existing: Option<&Value>) -> Value {
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
        // (pi tells the transports apart by `command` and `url`.)
        set(&mut m, "type", None);
        set(&mut m, "command", if stdio { self.command.first().map(|c| json!(c)) } else { None });
        set(&mut m, "args", (stdio && self.command.len() > 1).then(|| json!(self.command[1..])));
        set(&mut m, "url", if stdio { None } else { Some(json!(self.url.trim())) });
        set(&mut m, "headers", (!stdio && !self.headers.is_empty()).then(|| object(&self.headers)));
        set(&mut m, "env", if self.env.is_empty() { None } else { Some(object(&self.env)) });
        set(&mut m, "cwd", (stdio && !self.cwd.trim().is_empty()).then(|| json!(self.cwd.trim())));
        let timeout = self.timeout_secs.filter(|t| *t > 0.0).map(|t| if t.fract() == 0.0 { json!(t as u64) } else { json!(t) });
        set(&mut m, "timeout", timeout);
        // (A mark of harlo's former agent; pi doesn't read it.)
        set(&mut m, "readOnly", None);
        m.insert("enabled".into(), json!(self.enabled));
        Value::Object(m)
    }

    /// What's wrong with it, if anything.
    pub fn problem(&self) -> Option<String> {
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
        if self.transport == "sse" {
            return Some("pi doesn't take the old SSE transport: use the server's HTTP address.".into());
        }
        None
    }
}

/// A scope's servers, and problems reading its files.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Listing {
    pub servers: Vec<Form>,
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

/// The servers in a scope.
pub fn load(scope: &Scope) -> Listing {
    let mut listing = Listing::default();
    match read_config(&config_path(scope)) {
        Ok(config) => listing.servers = config[SERVERS].as_object().into_iter().flatten().map(|(name, entry)| Form::from_entry(name, entry)).collect(),
        Err(e) => listing.errors.push(format!("{e:#}")),
    }
    listing
}

/// Saves a server in a scope (renamed from `old_name`).
///
/// # Errors
///
/// When the form has a problem, the name is taken by another server, or the
/// config file can't be read, isn't a JSON object, or can't be written.
pub fn save(scope: &Scope, old_name: &str, form: &Form) -> Result<()> {
    if let Some(p) = form.problem() {
        bail!(p);
    }
    let path = config_path(scope);
    let mut config = read_config(&path)?;
    let before = config[SERVERS].as_object().cloned().unwrap_or_default();
    if form.name != old_name && !old_name.is_empty() && before.contains_key(&form.name) {
        bail!("There is already a server called {}.", form.name);
    }
    let Some(object) = config.as_object_mut() else { bail!("{} isn't a JSON object", crate::sidebar::tilde(&path)) };
    let servers = object.entry(SERVERS).or_insert_with(|| json!({}));
    let Some(servers) = servers.as_object_mut() else { bail!("{}: mcpServers isn't an object", crate::sidebar::tilde(&path)) };
    let previous = servers.get(old_name).or_else(|| servers.get(&form.name)).cloned();
    if form.name != old_name && !old_name.is_empty() {
        servers.shift_remove(old_name);
    }
    let entry = form.to_entry(previous.as_ref());
    // Keeps its place in the file when it was there.
    match servers.get_mut(&form.name) {
        Some(slot) => *slot = entry,
        None => {
            servers.insert(form.name.clone(), entry);
        }
    }
    if config[SERVERS].as_object() != Some(&before) {
        write_config(&path, &config)?;
    }
    Ok(())
}

/// Turns a server on or off (only its `enabled` changes).
///
/// # Errors
///
/// When there's no such server, or the config file can't be read, isn't a
/// JSON object, or can't be written.
pub fn set_enabled(scope: &Scope, name: &str, on: bool) -> Result<()> {
    let path = config_path(scope);
    let mut config = read_config(&path)?;
    let Some(entry) = config[SERVERS].get_mut(name).and_then(Value::as_object_mut) else { bail!("There's no server called {name} here.") };
    entry.insert("enabled".into(), json!(on));
    write_config(&path, &config)
}

/// A server's entry (None: there's none).
///
/// # Errors
///
/// When the config file can't be read or isn't a JSON object.
pub fn entry(scope: &Scope, name: &str) -> Result<Option<Value>> {
    let config = read_config(&config_path(scope))?;
    Ok(config[SERVERS].get(name).cloned())
}

/// Writes a server's entry (None: removes it).
///
/// # Errors
///
/// When a config file can't be read, isn't a JSON object, or can't be written.
pub fn put_entry(scope: &Scope, form: Option<&Form>, name: &str) -> Result<()> {
    let path = config_path(scope);
    let mut config = read_config(&path)?;
    let before = config[SERVERS].as_object().cloned().unwrap_or_default();
    let mut servers = before.clone();
    match form {
        Some(form) => {
            let entry = form.to_entry(before.get(name));
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
    let Some(object) = config.as_object_mut() else { bail!("{} isn't a JSON object", crate::sidebar::tilde(&path)) };
    object.insert(SERVERS.to_owned(), Value::Object(servers));
    write_config(&path, &config)
}

/// Sets (Some) or removes (None) one field of a server's entry, the rest as
/// it is (nothing when it has no entry).
///
/// # Errors
///
/// When a config file can't be read, isn't a JSON object, or can't be written.
pub fn set_field(scope: &Scope, name: &str, key: &str, value: Option<Value>) -> Result<()> {
    let path = config_path(scope);
    let mut config = read_config(&path)?;
    let before = config[SERVERS].as_object().cloned().unwrap_or_default();
    let Some(entry) = config[SERVERS].get_mut(name).and_then(Value::as_object_mut) else { return Ok(()) };
    match value {
        Some(v) => {
            entry.insert(key.to_owned(), v);
        }
        None => {
            entry.remove(key);
        }
    }
    if config[SERVERS].as_object() == Some(&before) {
        return Ok(());
    }
    write_config(&path, &config)
}

/// Removes a server from a scope.
///
/// # Errors
///
/// When the config file can't be read, isn't a JSON object, or can't be written.
pub fn remove(scope: &Scope, name: &str) -> Result<()> {
    let path = config_path(scope);
    let mut config = read_config(&path)?;
    if config[SERVERS].as_object_mut().is_some_and(|m| m.shift_remove(name).is_some()) {
        write_config(&path, &config)?;
    }
    Ok(())
}

/// Servers from a pasted snippet: `{"mcpServers": {…}}` (pi and most
/// other tools), `{"mcp": {"servers": {…}}}` / `{"mcp": {…}}`,
/// `{name: {…}}`, or one server object (named "imported").
///
/// # Errors
///
/// When the text isn't JSON, or holds no server in a form harlo knows.
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
            let mut f = Form::from_entry(&name, &s);
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
pub(crate) mod tests {
    use super::*;

    /// pi's config folder is one for the process: one test at a time.
    pub(crate) static PI_DIR: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn scratch() -> crate::scratch::Scratch {
        crate::scratch::Scratch::new(std::env::temp_dir().join(format!("harlo-mcp-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())))
    }

    #[test]
    fn reads_entries() {
        let pi = json!({"command":"uvx","args":["blender-mcp"],"env":{"A":"1"}});
        let f = Form::from_entry("blender", &pi);
        assert_eq!(f.command, ["uvx", "blender-mcp"]);
        assert_eq!(f.timeout_secs, None);
        assert_eq!(Form::from_entry("b", &json!({"type":"stdio","command":"uvx","args":["blender-mcp"],"env":{"A":"1"},"timeout":300})).timeout_secs, Some(300.0));
        let remote = Form::from_entry("r", &json!({"url":"https://x/mcp","headers":{"Authorization":"Bearer t"}}));
        assert_eq!((remote.transport.as_str(), remote.headers.len()), ("http", 1));
    }

    #[test]
    fn writes_entries_keeping_other_fields() {
        let mut f = Form { name: "s".into(), enabled: true, transport: "stdio".into(), command: vec!["npx".into(), "-y".into(), "srv".into()], timeout_secs: Some(30.0), ..Default::default() };
        let pi = f.to_entry(Some(&json!({"oauth":{"clientId":"c"},"type":"stdio","timeout":5,"readOnly":true})));
        assert_eq!(pi, json!({"oauth":{"clientId":"c"},"command":"npx","args":["-y","srv"],"timeout":30,"enabled":true}));
        f.transport = "http".into();
        f.url = "https://h/mcp".into();
        let remote = f.to_entry(Some(&pi));
        assert!(remote.get("command").is_none() && remote["url"] == "https://h/mcp" && remote.get("type").is_none());
    }

    #[test]
    fn saves_lists_and_removes() {
        let _one = PI_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch();
        let scope = Scope::Project(dir.to_path_buf());
        // An existing file with other servers and its own key order.
        std::fs::create_dir_all(dir.join(".pi")).unwrap();
        std::fs::write(dir.join(".pi/mcp.json"), "{\n  \"$schema\": \"x\",\n  \"mcpServers\": {\n    \"zed\": {\"command\": \"z\"},\n    \"alpha\": {\"command\": \"a\"}\n  }\n}\n").unwrap();
        let form = Form { name: "files".into(), enabled: true, transport: "stdio".into(), command: vec!["mcp-files".into()], ..Default::default() };
        save(&scope, "", &form).unwrap();
        let pi: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".pi/mcp.json")).unwrap()).unwrap();
        assert_eq!(pi["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["zed", "alpha", "files"], "order kept");
        assert_eq!(pi["$schema"], "x");
        let l = load(&scope);
        assert!(l.servers.iter().any(|s| s.name == "files"));
        // Renamed.
        let renamed = Form { name: "files2".into(), ..form.clone() };
        save(&scope, "files", &renamed).unwrap();
        let l = load(&scope);
        assert!(l.servers.iter().all(|s| s.name != "files"));
        // A name taken by another server is refused.
        assert!(save(&scope, "files2", &Form { name: "alpha".into(), ..renamed.clone() }).is_err());
        set_enabled(&scope, "files2", false).unwrap();
        assert!(!load(&scope).servers.iter().find(|s| s.name == "files2").unwrap().enabled);
        assert!(set_enabled(&scope, "nope", true).is_err());
        remove(&scope, "files2").unwrap();
        assert!(load(&scope).servers.iter().all(|s| s.name != "files2"));
        // A broken file is reported, not overwritten.
        std::fs::write(dir.join(".pi/mcp.json"), "{oops").unwrap();
        assert_eq!(load(&scope).errors.len(), 1);
        assert!(save(&scope, "", &form).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_snippets() {
        let snippet = r#"{"mcpServers":{"fs":{"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","/tmp"]},"web":{"type":"http","url":"https://x/mcp"}}}"#;
        let forms = parse_import(snippet).unwrap();
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
        let mut f = Form { name: "ok".into(), transport: "http".into(), url: "https://x".into(), enabled: true, ..Default::default() };
        assert!(f.problem().is_none());
        f.name = "bad name".into();
        assert!(f.problem().is_some());
    }
    #[test]
    fn pis_dialect() {
        let _one = PI_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch();
        let project = dir.join("pi-proj");
        std::fs::create_dir_all(&project).unwrap();
        let scope = Scope::Project(project.clone());
        assert_eq!(config_path(&scope), project.join(".pi/mcp.json"));
        assert_eq!(config_path(&Scope::Global), pi_dir().join("mcp.json"));
        assert!(config_path(&Scope::Host("not-a-host".into())).to_string_lossy().ends_with("/.pi/agent/mcp.json"));
        // No `type`; a folder, timeout and switch.
        let form = Form { name: "docs".into(), enabled: false, transport: "http".into(), url: "https://x/mcp".into(), timeout_secs: Some(9.0), ..Default::default() };
        save(&scope, "", &form).unwrap();
        assert_eq!(entry(&scope, "docs").unwrap().unwrap(), json!({"url": "https://x/mcp", "timeout": 9, "enabled": false}));
        assert!(load(&scope).servers.iter().any(|s| s.name == "docs" && !s.enabled));
        // pi takes no SSE.
        assert!(Form { transport: "sse".into(), ..form.clone() }.problem().unwrap().contains("SSE"));
        remove(&scope, "docs").unwrap();
        assert!(load(&scope).servers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod real_config {
    /// Lists this machine's global servers, read-only (not run by default).
    #[test]
    #[ignore = "reads this machine's own settings"]
    fn list_global() {
        let l = super::load(&super::Scope::Global);
        for s in &l.servers {
            println!("{} transport={} timeout={:?}", s.name, s.transport, s.timeout_secs);
        }
        println!("errors: {:?}", l.errors);
    }

}
