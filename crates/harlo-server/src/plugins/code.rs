//! A plugin's own code: the tools it gives agents and the commands it
//! runs, written in JavaScript in its `plugin.js`.
//!
//! `plugin.json` says what the code offers (each tool's name, description
//! and parameters, each command's name) and what it may reach, so a
//! person sees both before any of it runs. `plugin.js` registers a
//! function for each: `harlo.tool("name", async (args, context) => …)` and
//! `harlo.command("name", async (text, context) => "the prompt")`.
//!
//! Each call runs the file from its start in a `QuickJS` of its own, with
//! the engine's limits on memory and running time, and ends with it:
//! nothing is kept between calls but what the code puts in its storage.
//! The code has no files, no programs and no network of its own. It
//! reaches out through `harlo` alone, and only as `plugin.json` asked
//! and a person allowed:
//!
//! - `fetch`, to the hosts it names;
//! - `project`, the session's own tools (`read`, `grep`, `glob`, `edit`,
//!   `write`, `shell`), for a tool an agent's script called: each call
//!   is the session's own, decided by its rules, and asks a person where
//!   they say to;
//! - `settings`, what a person filled in for the plugin, such as a token;
//! - `storage`, a small store of the plugin's own.
//!
//! `harlo.on("turn.ended", …)` runs when a session's turn ends, with no
//! one there: it has the network, settings and storage, not the project.

// Rust guideline compliant 2026-10-09

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use harlo_engine::codemode::{self, Answer, Catalog, Namespace, SessionTools, ToolInfo};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::server::{Server, lock};

/// The file a plugin's code is in, unless its `plugin.json` names another.
pub(super) const MAIN: &str = "plugin.js";

/// The namespace the code's calls to Harlo go through.
const HOST: &str = "host";

/// The store setting a plugin's storage is kept under, before its name.
const STORAGE: &str = "plugin/data/";

/// The store setting what a person filled in for a plugin is kept under,
/// before its name.
const FILLED: &str = "plugin/config/";

/// The session's tools a plugin's code can be allowed, as scripts have
/// them.
const PROJECT: [&str; 6] = ["read", "grep", "glob", "edit", "write", "shell"];

/// What a plugin's code can be run for with no one calling it.
pub(crate) const TURN_ENDED: &str = "turn.ended";

/// The most settings a plugin may ask a person to fill in: the window
/// numbers the fields of a page, and eight is more than a plugin with a
/// token and an address needs.
pub(crate) const SETTINGS: usize = 8;

/// The most characters of a filled-in setting: a token or an address.
const SETTING_SIZE: usize = 4096;

/// How long a call's own code may run. What it waits for a request isn't
/// counted. A tool shapes a request and its answer; ten seconds of
/// computing is a loop that won't end.
const LIMIT: Duration = Duration::from_secs(10);

/// How long a call may take with its waiting, so a tool that waits on a
/// host that never answers ends: four requests at [`FETCH_WAIT`] each.
const WALL: Duration = Duration::from_mins(2);

/// How long one request may take, as a web page's fetch may.
const FETCH_WAIT: Duration = Duration::from_secs(30);

/// The most of an answer's body the code is given. What a tool returns
/// goes to a model, which reads a few thousand characters of it; two
/// megabytes holds any API's page of results to pick those from.
const FETCH_BODY: usize = 2 * 1024 * 1024;

/// The most redirects a request follows, each to an allowed host.
const REDIRECTS: usize = 5;

/// The largest `plugin.js` that's run: a bundled library or two fits; a
/// larger file is a mistake.
const CODE_SIZE: u64 = 2 * 1024 * 1024;

/// The most a call may return, as JSON. A script's own answer is cut far
/// shorter before a model reads it; this keeps a runaway tool from
/// filling memory on the way there.
const RESULT_SIZE: usize = 256 * 1024;

/// The most a plugin's storage may hold, as JSON: tokens, cursors and
/// small caches, not data.
const STORAGE_SIZE: usize = 256 * 1024;

/// The most characters of a tool's, command's or key's name.
const NAME_LENGTH: usize = 64;

/// How many of the lines a failed call logged are given with its error.
const LOGGED: usize = 10;

/// A tool a plugin's code gives agents, as `plugin.json` describes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(super) struct ToolSpec {
    /// Its name, as scripts call it.
    pub name: String,
    /// What it does, as the model reads it.
    #[serde(default)]
    pub description: String,
    /// Its argument's JSON Schema; none takes an empty object.
    #[serde(default)]
    pub parameters: Value,
}

/// A command a plugin's code runs, as `plugin.json` describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) struct CommandSpec {
    /// Its name, as typed after `/`.
    pub name: String,
    /// What it does, as the menu shows it.
    #[serde(default)]
    pub description: Option<String>,
}

/// A setting a person fills in for a plugin, as `plugin.json` asks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) struct SettingSpec {
    /// The name the code reads it by.
    pub key: String,
    /// What a person is asked for.
    #[serde(default)]
    pub label: Option<String>,
    /// Whether it's a secret, such as a token: never shown again.
    #[serde(default)]
    pub secret: bool,
}

/// What a plugin's code may reach, as `plugin.json` asks.
///
/// A key this Harlo doesn't know makes the plugin unusable rather than
/// run without what it needs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Permissions {
    /// The hosts its requests may go to: `api.example.com`,
    /// `*.example.com`, or with a port, `localhost:8080`.
    #[serde(default)]
    pub network: Vec<String>,
    /// The session's own tools its tools may call, under the session's
    /// rules: of `read`, `grep`, `glob`, `edit`, `write` and `shell`.
    #[serde(default)]
    pub project: Vec<String>,
}

/// What a call is to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A tool, called by an agent.
    Tool,
    /// A command, typed by a person.
    Command,
    /// Something that happened, with no one calling.
    Event,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Command => "command",
            Self::Event => "event",
        }
    }
}

/// The code of a plugin in use.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Program {
    /// The plugin's name.
    pub plugin: String,
    /// The name scripts reach its tools under: `tools.<namespace>`.
    pub namespace: String,
    /// What the plugin is for.
    pub description: Option<String>,
    /// The version the plugin gives itself.
    pub release: Option<String>,
    /// The file its code is in.
    pub main: PathBuf,
    /// Its tools.
    tools: Vec<ToolSpec>,
    /// Its commands, as (name, description).
    pub commands: Vec<(String, Option<String>)>,
    /// What it runs for with no one calling.
    pub events: Vec<String>,
    /// The session's tools its tools may call.
    project: Vec<String>,
    /// The settings it may read, by key.
    settings: Vec<String>,
    /// The hosts its requests may go to.
    network: Vec<String>,
}

/// Whether `name` can name a tool: a JavaScript identifier, so a script
/// writes `tools.plugin.name`.
fn tool_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.len() <= NAME_LENGTH
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether `name` can name a command: what's typed after `/`.
fn command_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.len() <= NAME_LENGTH
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_".contains(c))
}

/// A host as `plugin.json` allows it: its name, perhaps starting `*.`, and
/// its port if it names one. None when it isn't one.
fn host_entry(entry: &str) -> Option<(&str, Option<u16>)> {
    let (host, port) = match entry.rsplit_once(':') {
        Some((host, port)) => (host, Some(port.parse().ok()?)),
        None => (entry, None),
    };
    let name = host.strip_prefix("*.").unwrap_or(host);
    let named = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-.".contains(c))
        && !name.starts_with('.')
        && !name.ends_with('.');
    named.then_some((host, port))
}

/// Why `url` isn't one the code may ask, given the `hosts` it's allowed.
///
/// A request goes over `https`, or over `http` to this computer alone,
/// to a host and port `plugin.json` names. An entry without a port means
/// the scheme's own.
fn refused(url: &reqwest::Url, hosts: &[String]) -> Option<String> {
    let host = url.host_str().unwrap_or_default().to_lowercase();
    let local = host == "localhost"
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        "http" => return Some("Only https:// reaches another computer.".to_owned()),
        other => return Some(format!("harlo.fetch doesn't speak {other}://.")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Some("Leave the name and password out of the address.".to_owned());
    }
    let allowed = hosts
        .iter()
        .filter_map(|e| host_entry(e))
        .any(|(name, port)| {
            let named = match name.strip_prefix("*.") {
                Some(under) => host
                    .strip_suffix(under)
                    .is_some_and(|sub| sub.ends_with('.')),
                None => host == name,
            };
            // (A port the entry doesn't name is the scheme's own.)
            named
                && port.map_or(url.port().is_none(), |p| {
                    url.port_or_known_default() == Some(p)
                })
        });
    (!allowed).then(|| {
        let asked = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host,
        };
        format!(
            "This plugin isn't allowed to reach {asked}; its plugin.json names the hosts it may."
        )
    })
}

/// What a plugin's `plugin.json` says of its code.
#[derive(Debug, Clone, Copy)]
pub(super) struct Declared<'a> {
    /// The file its code is in, if named.
    pub main: Option<&'a str>,
    /// Its tools.
    pub tools: &'a [ToolSpec],
    /// Its commands.
    pub commands: &'a [CommandSpec],
    /// What it runs for with no one calling.
    pub events: &'a [String],
    /// What a person fills in for it.
    pub settings: &'a [SettingSpec],
    /// What it may reach.
    pub permissions: &'a Permissions,
}

impl Declared<'_> {
    /// Whether the plugin has code that runs.
    pub(super) fn runs(&self) -> bool {
        !self.tools.is_empty() || !self.commands.is_empty() || !self.events.is_empty()
    }

    /// Why what the plugin asks for isn't something Harlo gives; none
    /// when it is.
    fn unknown(&self) -> Option<String> {
        if let Some(bad) = self
            .permissions
            .network
            .iter()
            .find(|e| host_entry(e).is_none())
        {
            return Some(format!(
                "\"{bad}\" isn't a host its code can be allowed: a name, with a port if needed."
            ));
        }
        let project = &self.permissions.project;
        if let Some(bad) = project.iter().find(|t| !PROJECT.contains(&t.as_str())) {
            return Some(format!(
                "\"{bad}\" isn't a tool its code can be allowed: {}.",
                PROJECT.join(", ")
            ));
        }
        if let Some(bad) = self.events.iter().find(|e| *e != TURN_ENDED) {
            return Some(format!(
                "\"{bad}\" isn't something its code can run for: {TURN_ENDED}."
            ));
        }
        if self.settings.len() > SETTINGS {
            return Some(format!("It asks for more than {SETTINGS} settings."));
        }
        self.settings.iter().find(|s| !tool_name(&s.key)).map(|s| {
            format!(
                "\"{}\" can't name a setting: letters, digits and _ only.",
                s.key
            )
        })
    }

    /// The file the code is in, under `root`; none when it names one
    /// outside its folder.
    fn file(&self, root: &Path) -> Option<PathBuf> {
        let main = Path::new(self.main.unwrap_or(MAIN));
        main.components()
            .all(|c| matches!(c, Component::Normal(_)))
            .then(|| root.join(main))
    }

    /// Why the plugin in `root` can't be used as its code is declared;
    /// none when it can.
    pub(super) fn fault(&self, root: &Path) -> Option<String> {
        if let Some(why) = self.unknown() {
            return Some(why);
        }
        if !self.runs() {
            return None;
        }
        let Some(file) = self.file(root) else {
            return Some("Its code must be a file in its own folder.".to_owned());
        };
        let shown = self.main.unwrap_or(MAIN);
        match std::fs::metadata(&file) {
            Ok(found) if found.is_file() && found.len() <= CODE_SIZE => {}
            Ok(found) if found.is_file() => {
                return Some(format!("Its {shown} is larger than Harlo runs."));
            }
            _ => return Some(format!("It names tools or commands but has no {shown}.")),
        }
        if let Some(tool) = self.tools.iter().find(|t| !tool_name(&t.name)) {
            return Some(format!(
                "\"{}\" can't name a tool: letters, digits and _ only.",
                tool.name
            ));
        }
        if let Some(command) = self.commands.iter().find(|c| !command_name(&c.name)) {
            return Some(format!(
                "\"{}\" can't name a command: lower-case letters, digits, - and _ only.",
                command.name
            ));
        }
        let twice = |names: Vec<&str>| {
            names
                .iter()
                .enumerate()
                .find(|(at, name)| names[..*at].contains(name))
                .map(|(_, name)| (*name).to_owned())
        };
        let tools = twice(self.tools.iter().map(|t| t.name.as_str()).collect());
        let commands = twice(self.commands.iter().map(|c| c.name.as_str()).collect());
        let settings = twice(self.settings.iter().map(|s| s.key.as_str()).collect());
        tools
            .or(commands)
            .or(settings)
            .map(|name| format!("It names \"{name}\" twice."))
    }

    /// What the code asks of a person, each as (the line shown, what's
    /// fingerprinted).
    pub(super) fn asks(&self) -> Vec<(String, Vec<u8>)> {
        let mut asks = Vec::new();
        if self.runs() {
            let shown = self.main.unwrap_or(MAIN);
            asks.push((
                format!("Run its code ({shown}) in a sandbox, for its tools and commands"),
                b"code".to_vec(),
            ));
        }
        for event in self.events {
            // (The one there is; another gets words of its own.)
            asks.push((
                "Run its code each time a session's turn ends".to_owned(),
                event.as_bytes().to_vec(),
            ));
        }
        if !self.permissions.project.is_empty() {
            let mut tools = self.permissions.project.clone();
            tools.sort();
            tools.dedup();
            let listed = tools.join(", ");
            asks.push((
                format!(
                    "Use a session's own tools from its code, under the session's rules: {listed}"
                ),
                listed.into_bytes(),
            ));
        }
        for host in &self.permissions.network {
            asks.push((
                format!("Reach {host} over the network"),
                host.as_bytes().to_vec(),
            ));
        }
        asks
    }

    /// The code of plugin `plugin` in `root`, to run; none when it has
    /// none.
    pub(super) fn program(
        &self,
        plugin: &str,
        root: &Path,
        description: Option<&str>,
        release: Option<&str>,
    ) -> Option<Program> {
        if !self.runs() {
            return None;
        }
        let mut namespace: String = plugin
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        if namespace.starts_with(|c: char| c.is_ascii_digit()) {
            namespace.insert(0, '_');
        }
        Some(Program {
            plugin: plugin.to_owned(),
            namespace,
            description: description.map(str::to_owned),
            release: release.map(str::to_owned),
            main: self.file(root)?,
            tools: self.tools.to_vec(),
            commands: self
                .commands
                .iter()
                .map(|c| (c.name.clone(), c.description.clone()))
                .collect(),
            network: self.permissions.network.clone(),
            events: self.events.to_vec(),
            project: self.permissions.project.clone(),
            settings: self.settings.iter().map(|s| s.key.clone()).collect(),
        })
    }
}

/// What `plugin.js` starts with: `harlo`, on one line so the file's own
/// lines keep their numbers but for one.
const BEFORE: &str = r#"const __run = { tool: {}, command: {}, event: {} }; globalThis.harlo = { tool: (name, run) => { __run.tool[name] = run; }, command: (name, run) => { __run.command[name] = run; }, on: (name, run) => { __run.event[name] = run; }, settings: { get: (key) => tools.host.setting({ key }) }, project: Object.fromEntries(["read", "grep", "glob", "edit", "write", "shell"].map((tool) => [tool, (args) => tools.host.project({ tool, args: args || {} })])), fetch: async (url, init) => { const r = await tools.host.fetch({ ...(init || {}), url: String(url) }); return { ...r, text: () => r.body, json: () => JSON.parse(r.body) }; }, storage: { get: (key) => tools.host.get({ key }), set: (key, value) => tools.host.set({ key, value }), delete: (key) => tools.host.delete({ key }) }, plugin: input.context.plugin, session: input.context.session };"#;

/// What follows `plugin.js`: the call itself.
const AFTER: &str = r#"
;{
  const run = __run[input.kind][input.name];
  if (typeof run !== "function") {
    const how = input.kind === "event" ? "on" : input.kind;
    throw new Error(`It registers no ${input.kind} named "${input.name}" with harlo.${how}().`);
  }
  return await run(input.args, input.context);
}"#;

/// What a call of a plugin's code is made within.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Within<'a> {
    /// The session's ID and folder.
    pub session: Option<(&'a str, &'a str)>,
    /// The session's own tools, for a tool an agent's script called.
    pub tools: Option<&'a dyn SessionTools>,
}

/// What a program's code reaches of Harlo while it runs.
#[derive(Debug)]
struct Host<'a> {
    server: &'a Server,
    program: &'a Program,
    within: Within<'a>,
}

impl Host<'_> {
    /// Reads what a person filled in, as `harlo.settings.get` does.
    fn setting(&self, args: &Value) -> Result<Value, String> {
        let key = args["key"].as_str().unwrap_or_default();
        if !self.program.settings.iter().any(|k| k == key) {
            return Err(format!(
                "Its plugin.json names no setting \"{key}\"; a person fills in only those it names."
            ));
        }
        let mut filled = filled(self.server, &self.program.plugin);
        Ok(filled.remove(key).unwrap_or(Value::Null))
    }

    /// Calls one of the session's own tools, as `harlo.project` does.
    async fn project(&self, args: Value) -> Result<Value, String> {
        let tool = args["tool"].as_str().unwrap_or_default();
        if !self.program.project.iter().any(|t| t == tool) {
            return Err(format!(
                "This plugin isn't allowed harlo.project.{tool}; its plugin.json names the tools it may use."
            ));
        }
        let Some(session) = self.within.tools else {
            return Err(
                "harlo.project is there for a tool an agent's script called, not for a command or an event."
                    .to_owned(),
            );
        };
        session.call(tool, args["args"].clone()).await
    }

    /// Asks a host the plugin is allowed, as `harlo.fetch` does.
    async fn fetch(&self, args: Value) -> Result<Value, String> {
        let url = args["url"].as_str().unwrap_or_default();
        let url =
            reqwest::Url::parse(url).map_err(|e| format!("\"{url}\" isn't an address: {e}."))?;
        if let Some(why) = refused(&url, &self.program.network) {
            return Err(why);
        }
        let method = args["method"].as_str().unwrap_or("GET").to_uppercase();
        if !["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"].contains(&method.as_str()) {
            return Err(format!("harlo.fetch doesn't send {method}."));
        }
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let hosts = self.program.network.clone();
        let client = reqwest::Client::builder()
            .timeout(FETCH_WAIT)
            // (Each hop is held to the same hosts; one elsewhere is given
            // back as the answer it is.)
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= REDIRECTS {
                    attempt.error("too many redirects")
                } else if refused(attempt.url(), &hosts).is_none() {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|e| e.to_string())?;
        let mut request = client.request(method, url);
        for (name, value) in args["headers"].as_object().into_iter().flatten() {
            if let Some(value) = value.as_str() {
                request = request.header(name.as_str(), value);
            }
        }
        if let Some(body) = args["body"].as_str() {
            request = request.body(body.to_owned());
        }
        let mut response = request
            .send()
            .await
            .map_err(|e| format!("The request failed: {e}"))?;
        let status = response.status().as_u16();
        let headers: Map<String, Value> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                Some((name.as_str().to_owned(), json!(value.to_str().ok()?)))
            })
            .collect();
        let address = response.url().to_string();
        let mut body = Vec::new();
        let mut truncated = false;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("The answer broke off: {e}"))?
        {
            let room = FETCH_BODY - body.len();
            if chunk.len() > room {
                body.extend_from_slice(&chunk[..room]);
                truncated = true;
                break;
            }
            body.extend_from_slice(&chunk);
        }
        Ok(json!({
            "status": status,
            "ok": (200..300).contains(&status),
            "url": address,
            "headers": headers,
            "body": String::from_utf8_lossy(&body),
            "truncated": truncated,
        }))
    }

    /// Reads or changes the plugin's storage, as `harlo.storage` does.
    fn storage(&self, tool: &str, args: &Value) -> Result<Value, String> {
        let key = args["key"].as_str().unwrap_or_default();
        if key.is_empty() || key.len() > NAME_LENGTH {
            return Err(format!(
                "A storage key is 1 to {NAME_LENGTH} characters of text."
            ));
        }
        let setting = format!("{STORAGE}{}", self.program.plugin);
        // (One change at a time: two calls of the plugin may run at once.)
        let _one = lock(&self.server.plugins);
        let kept = self
            .server
            .store
            .setting(&setting)
            .map_err(|e| e.to_string())?;
        let mut kept: Map<String, Value> = kept
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        match tool {
            "get" => return Ok(kept.remove(key).unwrap_or(Value::Null)),
            "set" => {
                kept.insert(key.to_owned(), args["value"].clone());
            }
            _ => {
                kept.remove(key);
            }
        }
        let text = Value::Object(kept).to_string();
        if text.len() > STORAGE_SIZE {
            return Err(format!(
                "A plugin's storage holds {} KiB; this would pass it.",
                STORAGE_SIZE / 1024
            ));
        }
        self.server
            .store
            .put_setting(&setting, &text)
            .map_err(|e| e.to_string())?;
        Ok(Value::Null)
    }
}

impl Catalog for Host<'_> {
    fn namespaces(&self) -> Vec<Namespace> {
        Vec::new()
    }

    fn call<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Answer<'a> {
        Box::pin(async move {
            match (namespace, tool) {
                (HOST, "fetch") => self.fetch(args).await,
                (HOST, "get" | "set" | "delete") => self.storage(tool, &args),
                (HOST, "setting") => self.setting(&args),
                (HOST, "project") => self.project(args).await,
                _ => Err(format!("A plugin's code has no tools.{namespace}.{tool}.")),
            }
        })
    }
}

impl Program {
    /// Its tools, as scripts are told of them.
    pub(crate) fn listed(&self) -> Namespace {
        Namespace {
            name: self.namespace.clone(),
            description: self.description.clone().map_or_else(
                || format!("The tools of the plugin {}.", self.plugin),
                |what| format!("The plugin {}: {what}", self.plugin),
            ),
            tools: self
                .tools
                .iter()
                .map(|t| ToolInfo {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    signature: format!("({}): unknown", crate::mcp::object_type(&t.parameters)),
                })
                .collect(),
        }
    }

    /// Whether its tools may call the session's own.
    pub(crate) fn reaches_session(&self) -> bool {
        !self.project.is_empty()
    }

    /// Runs its `kind` named `name` with `args`, within `within`, and
    /// returns what the code returned.
    ///
    /// # Errors
    ///
    /// Fails, with a line the model or the person reads, when there's no
    /// such tool or command, the code throws, or it passes a limit.
    pub(crate) async fn call(
        &self,
        server: &Server,
        kind: Kind,
        name: &str,
        args: Value,
        within: Within<'_>,
    ) -> Result<Value, String> {
        let known = match kind {
            Kind::Tool => self.tools.iter().any(|t| t.name == name),
            Kind::Command => self.commands.iter().any(|(n, _)| n == name),
            Kind::Event => self.events.iter().any(|e| e == name),
        };
        if !known {
            return Err(format!(
                "The plugin {} has no {} {name}.",
                self.plugin,
                kind.as_str()
            ));
        }
        let failed = |why: String| format!("The plugin {}: {why}", self.plugin);
        let code = tokio::fs::read_to_string(&self.main)
            .await
            .map_err(|e| failed(format!("its code can't be read: {e}")))?;
        let input = json!({
            "kind": kind.as_str(),
            "name": name,
            "args": args,
            "context": {
                "plugin": { "name": self.plugin, "version": self.release },
                "session": within.session.map(|(id, directory)| json!({ "id": id, "directory": directory })),
            },
        });
        let host = Host {
            server,
            program: self,
            within,
        };
        let stop = CancellationToken::new();
        let code = format!("{BEFORE}\n{code}\n{AFTER}");
        let run = codemode::run(&host, &code, &input, LIMIT, &stop, &|_| {});
        tokio::pin!(run);
        let ran = tokio::select! {
            ran = &mut run => ran,
            () = tokio::time::sleep(WALL) => {
                stop.cancel();
                run.await
            }
        };
        match ran.value {
            Ok(value) => {
                let value = value.unwrap_or(Value::Null);
                if value.to_string().len() > RESULT_SIZE {
                    return Err(failed(format!(
                        "{name} returned more than {} KiB; it must return less.",
                        RESULT_SIZE / 1024
                    )));
                }
                Ok(value)
            }
            Err(why) => {
                // (Without the stack: its frames are this file's wrapping,
                // and a model reads every line.)
                let own = why.lines().take_while(|l| !l.starts_with("    at "));
                let mut why = failed(own.collect::<Vec<_>>().join("\n").trim_end().to_owned());
                let from = ran.logs.len().saturating_sub(LOGGED);
                if !ran.logs.is_empty() {
                    why.push_str("\nIt logged:\n");
                    why.push_str(&ran.logs[from..].join("\n"));
                }
                Err(why)
            }
        }
    }
}

/// Forgets what plugin `plugin`'s code stored and what a person filled
/// in for it.
pub(super) fn forget(server: &Server, plugin: &str) {
    for kept in [STORAGE, FILLED] {
        if let Err(e) = server.store.put_setting(&format!("{kept}{plugin}"), "") {
            tracing::warn!(name: "plugins.storage.kept", plugin, error = %e, "a removed plugin's storage stays");
        }
    }
}

/// What a person filled in for plugin `plugin`, by key.
fn filled(server: &Server, plugin: &str) -> Map<String, Value> {
    server
        .store
        .setting(&format!("{FILLED}{plugin}"))
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The settings `specs` of plugin `plugin` as clients see them: a secret
/// is said to be filled in, never shown.
pub(super) fn settings_shown(server: &Server, plugin: &str, specs: &[SettingSpec]) -> Vec<Value> {
    let filled = filled(server, plugin);
    specs
        .iter()
        .map(|spec| {
            let value = filled.get(&spec.key).and_then(Value::as_str);
            let mut shown = json!({
                "key": spec.key,
                "label": spec.label.as_deref().unwrap_or(&spec.key),
                "secret": spec.secret,
                "set": value.is_some(),
            });
            if let Some(value) = value.filter(|_| !spec.secret) {
                shown["value"] = json!(value);
            }
            shown
        })
        .collect()
}

/// Fills in `changes` for plugin `plugin`, of the settings `specs`: text
/// by key, or null or nothing to clear one.
///
/// # Errors
///
/// When a key isn't one the plugin names, a value isn't text or is too
/// long, or the store fails.
pub(super) fn fill(
    server: &Server,
    plugin: &str,
    specs: &[SettingSpec],
    changes: &Map<String, Value>,
) -> Result<(), String> {
    // (One change at a time, as for its storage.)
    let _one = lock(&server.plugins);
    let mut kept = filled(server, plugin);
    for (key, value) in changes {
        if !specs.iter().any(|s| s.key == *key) {
            return Err(format!("The plugin {plugin} has no setting {key}."));
        }
        match value {
            Value::Null => {
                kept.remove(key);
            }
            Value::String(text) if text.is_empty() => {
                kept.remove(key);
            }
            Value::String(text) if text.chars().count() > SETTING_SIZE => {
                return Err(format!("{key} is longer than {SETTING_SIZE} characters."));
            }
            Value::String(text) => {
                kept.insert(key.clone(), json!(text));
            }
            _ => return Err(format!("{key} must be text.")),
        }
    }
    server
        .store
        .put_setting(
            &format!("{FILLED}{plugin}"),
            &Value::Object(kept).to_string(),
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> reqwest::Url {
        reqwest::Url::parse(text).expect("an address")
    }

    #[test]
    fn a_request_goes_only_to_a_host_the_plugin_names() {
        let hosts = [
            "api.example.com".to_owned(),
            "*.files.example.com".to_owned(),
            "localhost:8080".to_owned(),
        ];
        for allowed in [
            "https://api.example.com/v1/x?y=1",
            "https://API.example.com/",
            "https://a.files.example.com/x",
            "http://localhost:8080/x",
        ] {
            assert_eq!(refused(&url(allowed), &hosts), None, "{allowed}");
        }
        for kept_out in [
            "https://example.com/",
            "https://api.example.com.evil.net/",
            "https://evilapi.example.com/",
            "https://files.example.com/",
            "https://xfiles.example.com/",
            "https://api.example.com:8443/",
            "http://api.example.com/",
            "http://localhost/",
            "http://localhost:9090/",
            "https://me:secret@api.example.com/",
            "file:///etc/passwd",
        ] {
            assert!(refused(&url(kept_out), &hosts).is_some(), "{kept_out}");
        }
    }

    #[test]
    fn names_are_checked_before_anything_runs() {
        assert!(tool_name("look_up2") && tool_name("_x"));
        assert!(!tool_name("look-up") && !tool_name("2x") && !tool_name(""));
        assert!(command_name("ship-it") && !command_name("Ship") && !command_name("a b"));
        assert_eq!(
            host_entry("api.example.com"),
            Some(("api.example.com", None))
        );
        assert_eq!(
            host_entry("*.example.com:8443"),
            Some(("*.example.com", Some(8443)))
        );
        for bad in [
            "",
            "https://example.com",
            "example.com/x",
            "a b",
            ".x",
            "x:port",
        ] {
            assert_eq!(host_entry(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn code_is_a_file_in_the_plugins_own_folder() {
        let dir = tempfile::tempdir().expect("a folder");
        let tools = [ToolSpec {
            name: "look".to_owned(),
            description: String::new(),
            parameters: Value::Null,
        }];
        let permissions = Permissions::default();
        let declared = |main| Declared {
            main,
            tools: &tools,
            commands: &[],
            events: &[],
            settings: &[],
            permissions: &permissions,
        };
        assert!(declared(None).fault(dir.path()).is_some(), "no plugin.js");
        std::fs::write(dir.path().join(MAIN), "").expect("written");
        assert_eq!(declared(None).fault(dir.path()), None);
        assert!(declared(Some("../plugin.js")).fault(dir.path()).is_some());
        assert!(declared(Some("/etc/passwd")).fault(dir.path()).is_some());
        let program = declared(None)
            .program("9-lives.x", dir.path(), None, None)
            .expect("code");
        assert_eq!(program.namespace, "_9_lives_x");
        assert!(!program.reaches_session());

        // What Harlo doesn't give is refused by name.
        let asking = |project: &[&str], events: &[&str]| {
            let permissions = Permissions {
                network: Vec::new(),
                project: project.iter().map(|t| (*t).to_owned()).collect(),
            };
            let events: Vec<String> = events.iter().map(|e| (*e).to_owned()).collect();
            Declared {
                main: None,
                tools: &tools,
                commands: &[],
                events: &events,
                settings: &[],
                permissions: &permissions,
            }
            .fault(dir.path())
        };
        assert_eq!(asking(&["read", "shell"], &[TURN_ENDED]), None);
        let why = asking(&["task"], &[]).expect("not a tool to give");
        assert!(why.starts_with("\"task\" isn't a tool"), "{why}");
        let why = asking(&[], &["session.created"]).expect("not an event");
        assert!(
            why.starts_with("\"session.created\" isn't something"),
            "{why}"
        );
    }
}
