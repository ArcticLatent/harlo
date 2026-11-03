//! `Accounts` (QML singleton): signing the agents in to their model
//! providers from harlo (Settings → Providers → Accounts, the sign-in
//! dialog), with each agent's own sign-in:
//!
//! - Claude: Claude Code's `claude auth login` (Anthropic's own flow in the
//!   browser; Claude Code keeps the token), `auth status`, `auth logout`.
//! - omp: `omp login` (its list, then its links and prompts).
//! - pi: pi's own login (its `ModelRuntime`) through `pi_auth.mjs`, run with
//!   the Node pi runs on.
//! - OpenCode: its server's integrations (`/api/integration`), on a private
//!   `opencode serve`.
//!
//! The agents keep their credentials; harlo shows the links and hands what
//! the user types to the agent, as a terminal would, and never reads them.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Bumped when an agent's accounts or providers are read again.
        #[qproperty(i32, revision)]
        /// The sign-in going on (or the last one): `{ id, agent, provider,
        /// title, steps: [{ kind: "text" | "link" | "device", text, url,
        /// open, code }], ask: null | { kind: "secret" | "code" | "text" |
        /// "select", message, placeholder, options: [{ id, label }],
        /// optional }, state: "running" | "done" | "failed" | "cancelled",
        /// message }`; "" before one.
        #[qproperty(QString, flow)]
        /// Asking a model server what it serves (`find_models`): `{ id,
        /// state: "finding" | "ready" | "failed", baseUrl, name (a
        /// suggestion), models: [{ id, context }], error }`; "" before one.
        #[qproperty(QString, server_search, cxx_name = "serverSearch")]
        /// Model servers' state, by address: `{ <url>: { state: "running" |
        /// "stopped" | "starting" | "failed", harlo (started by harlo),
        /// message } }`.
        #[qproperty(QString, server_states, cxx_name = "serverStates")]
        /// A server being removed (in the background): `{ url, state:
        /// "running" | "done", step, done, total, error, busy, summary }`
        /// (summary: what it did, when done); "" none.
        #[qproperty(QString, removal)]
        /// This computer's servers on other machines' agents, by host: `{
        /// <host>: { state: "loading" | "ready" | "error", error, agents: {
        /// <agent>: [{ name, baseUrl }] }, busy (an add or removal going on),
        /// note (why the last one didn't go) } }`.
        #[qproperty(QString, host_servers, cxx_name = "hostServers")]
        /// The model servers harlo set up on other machines, by machine: `{
        /// <host>: { state: "loading" | "ready" | "error", error, list: [{
        /// baseUrl, name, engine, models, running, log }], busy: { <url>:
        /// "starting" | "stopping" | "removing" }, note } }`.
        #[qproperty(QString, host_engines, cxx_name = "hostEngines")]
        type Accounts = super::AccountsRust;
    }

    unsafe extern "RustQt" {
        /// The agents here have a server more or less (in the background).
        #[qsignal]
        #[cxx_name = "localServersChanged"]
        fn local_servers_changed(self: Pin<&mut Accounts>);
    }

    impl cxx_qt::Threading for Accounts {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Reads an agent's accounts again (`status` follows).
        #[qinvokable]
        fn refresh(self: Pin<&mut Accounts>, agent: &QString);
        /// `{ state: "loading" | "ready", accounts: [{ id, label, detail,
        /// signOut }], error }`.
        #[qinvokable]
        fn status(self: &Accounts, agent: &QString) -> QString;
        /// Reads what an agent can sign in to (`providers` follows).
        #[qinvokable]
        fn load_providers(self: Pin<&mut Accounts>, agent: &QString);
        /// `{ state, list: [{ id, name, connected, note, methods: [{ id,
        /// type: "login" | "oauth" | "key" | "external" | "command", label,
        /// form, blocked }] }], error }`.
        #[qinvokable]
        fn providers(self: &Accounts, agent: &QString) -> QString;
        /// Starts signing in (`flow` follows). `answers`: JSON of what the
        /// dialog asked first (OpenCode's key and form fields).
        #[qinvokable]
        fn start(self: Pin<&mut Accounts>, agent: &QString, provider: &QString, method: &QString, answers: &QString);
        /// Answers what the sign-in asks (`flow.ask`).
        #[qinvokable]
        fn answer(self: Pin<&mut Accounts>, text: &QString);
        /// Stops the sign-in going on.
        #[qinvokable]
        fn cancel(self: Pin<&mut Accounts>);
        /// Signs an account out. An error, or "" (`status` follows).
        #[qinvokable]
        fn sign_out(self: Pin<&mut Accounts>, agent: &QString, id: &QString) -> QString;
        /// The dialog and Settings closed: OpenCode's private server goes.
        #[qinvokable]
        fn release(self: Pin<&mut Accounts>);
        /// Whether pi or omp uses Claude through a Claude subscription,
        /// which Anthropic bills as extra usage (checked in the background
        /// every 10 minutes; `revision` follows a change).
        #[qinvokable]
        fn extra_usage(self: Pin<&mut Accounts>, agent: &QString) -> bool;
        /// An agent's model servers, as last read (`load_servers`): `[{
        /// name, baseUrl, models: [id], place: "here" | "network" |
        /// "online", managed (harlo set it up: it starts and stops it) }]`.
        #[qinvokable]
        fn servers(self: &Accounts, agent: &QString) -> QString;
        /// Reads the agents' model servers again, in the background
        /// (`revision` follows a change).
        #[qinvokable]
        fn load_servers(self: Pin<&mut Accounts>);
        /// Asks the server at `url` what it serves (`serverSearch` follows;
        /// an empty `url` clears it).
        #[qinvokable]
        fn find_models(self: Pin<&mut Accounts>, url: &QString, key: &QString);
        /// Adds a server and the models picked (`models`: JSON `[{ id,
        /// context }]`) to the agent's config; `taken`: JSON of the provider
        /// names it has already. An error, "" (done), or "OPENCODE_BUSY"
        /// (done; OpenCode's open sessions see it once they're closed).
        #[qinvokable]
        fn add_server(self: &Accounts, agent: &QString, name: &QString, base_url: &QString, key: &QString, models: &QString, taken: &QString) -> QString;
        /// The file an agent's servers are in, for opening ("" when it has
        /// none yet).
        #[qinvokable]
        fn servers_file(self: &Accounts, agent: &QString) -> QString;
        /// Checks whether these servers (JSON list of addresses) answer
        /// (`serverStates` follows).
        #[qinvokable]
        fn check_servers(self: Pin<&mut Accounts>, urls: &QString);
        /// Starts a server with its start command (`serverStates` follows).
        #[qinvokable]
        fn start_server(self: Pin<&mut Accounts>, url: &QString);
        /// Stops a server harlo started, else runs its stop command.
        #[qinvokable]
        fn stop_server(self: Pin<&mut Accounts>, url: &QString);
        /// Adds a server one agent has to another (its address, key and
        /// models as that agent has them): "" or why not ("OPENCODE_BUSY":
        /// added, for OpenCode's next sessions).
        #[qinvokable]
        fn copy_server(self: &Accounts, from: &QString, to: &QString, name: &QString, taken: &QString) -> QString;
        /// Reads which servers the agents on `host` have (`hostServers`
        /// follows).
        #[qinvokable]
        fn load_host_servers(self: Pin<&mut Accounts>, host: &QString);
        /// Adds a server (as agent `from` has it, or the address of one harlo
        /// set up) to an agent on `host`, its port forwarded there when it's
        /// on this computer; in the background (`hostServers` follows).
        #[qinvokable]
        fn add_server_on(self: Pin<&mut Accounts>, host: &QString, agent: &QString, from: &QString, name: &QString);
        /// Takes a server out of an agent on `host` (the same way).
        #[qinvokable]
        fn remove_server_on(self: Pin<&mut Accounts>, host: &QString, agent: &QString, name: &QString);
        /// Reads the servers harlo set up on `host` (`hostEngines` follows).
        #[qinvokable]
        fn load_host_engines(self: Pin<&mut Accounts>, host: &QString);
        /// Starts one of them, there (in the background).
        #[qinvokable]
        fn start_host_engine(self: Pin<&mut Accounts>, host: &QString, url: &QString);
        /// Stops one of them, there.
        #[qinvokable]
        fn stop_host_engine(self: Pin<&mut Accounts>, host: &QString, url: &QString);
        /// Takes one of them out of the agents there, stops and forgets it,
        /// and deletes its model files with `delete_files`.
        #[qinvokable]
        fn remove_host_engine(self: Pin<&mut Accounts>, host: &QString, url: &QString, name: &QString, delete_files: bool);
        /// Adds a server set up on `host` to its agents there: JSON `{ name,
        /// baseUrl, models, agents: [id] }` (`hostServers` says how it went).
        #[qinvokable]
        fn add_host_engine(self: Pin<&mut Accounts>, host: &QString, server: &QString);
        /// Uses a server set up on `host` in an agent here (`on`), through a
        /// port forwarded over the SSH connection, or stops (JSON `{ name,
        /// baseUrl, models }`); in the background (`localServersChanged`).
        #[qinvokable]
        fn use_host_engine(self: Pin<&mut Accounts>, host: &QString, server: &QString, agent: &QString, on: bool);
        /// Asks `host` which of its engines have a newer version (an hour
        /// apart, or now with `force`): `hostEngines[host].updates`.
        #[qinvokable]
        fn check_host_updates(self: Pin<&mut Accounts>, host: &QString, force: bool);
        /// The model files harlo downloaded for a server it set up: `{
        /// files: [path], size }` (none for a server whose models its
        /// engine keeps).
        #[qinvokable]
        fn server_files(self: &Accounts, url: &QString) -> QString;
        /// Takes a server out of every agent that has it (by its name and
        /// address); one harlo set up is stopped and forgotten, and its
        /// model files deleted with `delete_files`. In the background:
        /// `removal` says how far, then whether it went.
        #[qinvokable]
        fn remove_server(self: Pin<&mut Accounts>, name: &QString, url: &QString, delete_files: bool);
        /// Takes a server out of one agent (others keep it): "" or why not.
        #[qinvokable]
        fn remove_server_from(self: Pin<&mut Accounts>, agent: &QString, name: &QString) -> QString;
        /// The log of a server harlo starts (its output; "" before one).
        #[qinvokable]
        fn server_log(self: &Accounts, url: &QString) -> QString;
        /// The extensions harlo sets up for pi: `{ state: "loading" | "ready",
        /// list: [{ name, title, what, tokens, optional, note, enabled, version,
        /// latest, newer }], error, job:
        /// { source, state: "running" | "done" | "failed", message } }`.
        #[qinvokable]
        fn packages(self: &Accounts) -> QString;
        /// Reads them again (`packages` follows, with its `host`), then asks
        /// npm what's latest. `host`: another machine's pi ("" this one's).
        #[qinvokable]
        fn load_packages(self: Pin<&mut Accounts>, host: &QString);
        /// Installs the latest version of one (`name`), or of each that has a
        /// newer one ("all"). On or off stays as it is.
        #[qinvokable]
        fn update_package(self: Pin<&mut Accounts>, host: &QString, name: &QString);
        /// Turns one on (installed then, if it isn't) or off, for new sessions
        /// and, as soon as they're idle, open ones.
        #[qinvokable]
        fn set_package_enabled(self: Pin<&mut Accounts>, host: &QString, name: &QString, on: bool);
        /// Installs the extensions not set up yet, off, in the background, if
        /// pi is here (at start: it's once, and after that only a new one).
        #[qinvokable]
        fn set_up_packages(self: Pin<&mut Accounts>);
        /// The file with the permission rules, there or not (another
        /// machine's as `ssh://host/…`; "" when it can't be asked).
        #[qinvokable]
        fn package_rules_file(self: &Accounts, host: &QString) -> QString;
    }
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::pi_packages;
use harlo_core::remote::Host;
use harlo_core::servers::*;
use harlo_core::{model_servers, signin};
use harlo_harness::HarnessKind;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use qobject::Accounts;

#[derive(Default)]
pub struct AccountsRust {
    revision: i32,
    flow: QString,
    server_search: QString,
    server_states: QString,
    removal: QString,
    host_servers: QString,
    /// `host_servers`, as a value.
    on_hosts: Value,
    host_engines: QString,
    /// `host_engines`, as a value.
    engines_on: Value,
    /// The sign-in going on: its id, and where its answers go (dropped:
    /// cancelled).
    current: Option<(u64, mpsc::UnboundedSender<String>)>,
    next_id: u64,
    status: HashMap<String, Value>,
    providers: HashMap<String, Value>,
    /// `extra_usage` per agent: the answer, and when it came (`None`:
    /// being checked).
    extra_usage: HashMap<String, (bool, Option<Instant>)>,
    /// `packages`.
    packages: Value,
    /// `servers` per agent, as last read (`load_servers`).
    servers: HashMap<String, Value>,
}

/// An agent's program: the path set in Settings → Providers (if it's
/// there), else the installed one.
fn program(kind: HarnessKind) -> Option<PathBuf> {
    let set = harlo_core::db::Db::open(&harlo_core::db::default_path()).ok().and_then(|d| d.get(&format!("settings.{}_path", kind.as_str())).ok().flatten()).unwrap_or_default();
    if set.trim().is_empty() { harlo_harness::resolve_program(kind) } else { Some(PathBuf::from(set.trim())).filter(|p| p.is_file()) }
}

/// A server's address, key and models: as agent `from` has it, else (no
/// agent has it: `from` is its address) what harlo saved when it set it
/// up, else what it serves now.
fn server_details(from: &str, name: &str) -> Result<(String, String, Vec<model_servers::Found>), String> {
    if let Some(from) = kind(from) {
        return model_servers::details(from, name).ok_or_else(|| format!("{name} isn't in {}'s file any more.", from.label()));
    }
    let url = from.to_owned();
    let mut models = saved_models(&commands_of(&url));
    if models.is_empty() {
        models = model_servers::find(&url, "").map_err(|e| format!("Start it first: harlo asks it for its models. ({e})"))?.models;
        // (The context it serves: not in its list.)
        for m in models.iter_mut().filter(|m| m.context.is_none()) {
            m.context = harlo_core::engines::model_context(&url, &m.id);
        }
    }
    Ok((url, String::new(), models))
}

/// The servers each agent on `host` has: `{ <agent>: [{ name, baseUrl }] }`
/// (the agents there harlo writes servers for).
/// Whether `kind`'s config on `host` has the server `name` at `url`.
fn servers_on_host_have(host: &str, kind: HarnessKind, name: &str, url: &str) -> bool {
    model_servers::servers_on(host, kind).is_ok_and(|l| l.iter().any(|s| s.name == name && s.base_url == url))
}

fn read_host_servers(host: &str) -> Result<Value, String> {
    let mut agents = serde_json::Map::new();
    for k in model_servers::ON_HOST.iter().filter(|k| harlo_core::remote::found_on(host, k.as_str())) {
        let list = model_servers::servers_on(host, *k)?;
        agents.insert(k.as_str().to_owned(), list.iter().map(|s| json!({ "name": s.name, "baseUrl": s.base_url })).collect());
    }
    Ok(Value::Object(agents))
}

fn kind(agent: &str) -> Option<HarnessKind> {
    HarnessKind::parse(agent).filter(|k| HarnessKind::PROVIDERS.contains(k))
}

/// What to sign in: `pi`, or `pi@arcticnas` for pi on that host (which keeps
/// its own logins).
fn target(agent: &str) -> Option<(HarnessKind, Option<Host>)> {
    match agent.split_once('@') {
        Some((a, h)) => Some((kind(a)?, Some(harlo_core::remote::host(h)?))),
        None => Some((kind(agent)?, None)),
    }
}

/// Shown for a Claude subscription offered by another agent.
const CLAUDE_SUBSCRIPTION: &str = "Anthropic doesn't allow a Claude Pro or Max subscription in other apps. Use an Anthropic API key here, or Claude Code (the Claude agent) with your subscription.";

// ---- the sign-in going on ------------------------------------------------------

#[derive(Clone, Default)]
struct Flow {
    id: u64,
    agent: String,
    provider: String,
    title: String,
    steps: Vec<Value>,
    ask: Value,
    state: String,
    message: String,
}

impl Flow {
    fn json(&self) -> String {
        json!({ "id": self.id, "agent": self.agent, "provider": self.provider, "title": self.title, "steps": self.steps,
                "ask": self.ask, "state": self.state, "message": self.message })
        .to_string()
    }
}

/// How a sign-in tells the dialog what's happening.
#[derive(Clone)]
struct Report {
    flow: Arc<Mutex<Flow>>,
    thread: CxxQtThread<Accounts>,
    /// Signing in on this host: what the page sends back to `localhost` has
    /// to reach the agent there.
    host: Option<Host>,
    /// The ports forwarded to it meanwhile.
    forwards: Arc<Mutex<Vec<u16>>>,
}

impl Report {
    fn update(&self, f: impl FnOnce(&mut Flow)) {
        let (id, json) = {
            let mut fl = self.flow.lock().unwrap();
            f(&mut fl);
            (fl.id, fl.json())
        };
        let _ = self.thread.queue(move |mut this| {
            if this.current.as_ref().is_some_and(|(i, _)| *i == id) || this.flow_id() == id {
                this.as_mut().set_flow(QString::from(&json));
            }
        });
    }

    fn text(&self, text: &str) {
        let t = text.trim().to_owned();
        if !t.is_empty() {
            self.update(|f| f.steps.push(json!({ "kind": "text", "text": t })));
        }
    }

    /// A link (`open`: harlo opens it in the browser; the agent didn't).
    fn link(&self, url: &str, label: &str, open: bool) {
        self.forward_for(url);
        let (url, label) = (url.to_owned(), label.trim().trim_end_matches(':').trim().to_owned());
        self.update(|f| {
            if !f.steps.iter().any(|s| s["url"] == url.as_str()) {
                f.steps.push(json!({ "kind": "link", "url": url, "text": label, "open": open }));
            }
        });
    }

    fn device(&self, code: &str, url: &str) {
        let (code, url) = (code.to_owned(), url.to_owned());
        self.update(|f| {
            if !f.steps.iter().any(|s| s["code"] == code.as_str()) {
                f.steps.push(json!({ "kind": "device", "code": code, "url": url }));
            }
        });
    }

    fn ask(&self, ask: Value) {
        self.update(|f| f.ask = ask);
    }

    /// A page that ends on this computer's `localhost`: forwarded to the
    /// host before it opens. (If it can't be, the agent asks for what the
    /// page ended on instead.)
    fn forward_for(&self, url: &str) {
        let (Some(host), Some(port)) = (&self.host, signin::callback_port(url)) else { return };
        let mut forwards = self.forwards.lock().unwrap();
        if forwards.contains(&port) {
            return;
        }
        match harlo_core::remote::forward_port(host, port, true) {
            Ok(()) => forwards.push(port),
            Err(e) => {
                tracing::warn!("forwarding port {port} to {}: {e:#}", host.name);
                drop(forwards);
                self.text(&format!("This computer's port {port} is taken, so the page can't reach {} by itself. When it finishes, copy the address it ended on into the box here.", host.name));
            }
        }
    }

    fn finish(&self, ok: bool, message: &str) {
        if let Some(host) = &self.host {
            for port in self.forwards.lock().unwrap().drain(..) {
                let _ = harlo_core::remote::forward_port(host, port, false);
            }
        }
        let message = message.trim().to_owned();
        self.update(|f| {
            if f.state == "running" {
                f.state = if ok { "done" } else { "failed" }.into();
                f.message = message;
            }
            f.ask = Value::Null;
        });
    }
}

impl Accounts {
    fn flow_id(&self) -> u64 {
        serde_json::from_str::<Value>(&self.flow.to_string()).ok().and_then(|v| v["id"].as_u64()).unwrap_or(0)
    }

    fn bump(mut self: Pin<&mut Self>) {
        let n = self.revision + 1;
        self.as_mut().set_revision(n);
    }

    pub fn status(&self, agent: &QString) -> QString {
        let v = self.rust().status.get(&agent.to_string()).cloned().unwrap_or_else(|| json!({ "state": "loading", "accounts": [] }));
        QString::from(&v.to_string())
    }

    pub fn providers(&self, agent: &QString) -> QString {
        let v = self.rust().providers.get(&agent.to_string()).cloned().unwrap_or_else(|| json!({ "state": "loading", "list": [] }));
        QString::from(&v.to_string())
    }

    pub fn refresh(mut self: Pin<&mut Self>, agent: &QString) {
        let agent = agent.to_string();
        let Some((k, host)) = target(&agent) else { return };
        // What it's signed in to may have changed.
        self.as_mut().rust_mut().extra_usage.remove(&agent);
        let was = self.rust().status.get(&agent).cloned().unwrap_or_else(|| json!({ "accounts": [] }));
        self.as_mut().rust_mut().status.insert(agent.clone(), json!({ "state": "loading", "accounts": was["accounts"] }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = match accounts_of(k, host.as_ref()) {
                Ok(list) => json!({ "state": "ready", "accounts": list, "error": "" }),
                Err(e) => json!({ "state": "ready", "accounts": [], "error": e }),
            };
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().status.insert(agent, v);
                this.bump();
            });
        });
    }

    pub fn load_providers(mut self: Pin<&mut Self>, agent: &QString) {
        let agent = agent.to_string();
        let Some((k, host)) = target(&agent) else { return };
        self.as_mut().rust_mut().providers.insert(agent.clone(), json!({ "state": "loading", "list": [] }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = match providers_of(k, host.as_ref()) {
                Ok(list) => json!({ "state": "ready", "list": list, "error": "" }),
                Err(e) => json!({ "state": "ready", "list": [], "error": e }),
            };
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().providers.insert(agent, v);
                this.bump();
            });
        });
    }

    pub fn start(mut self: Pin<&mut Self>, agent: &QString, provider: &QString, method: &QString, answers: &QString) {
        self.as_mut().cancel();
        let (agent, provider, method) = (agent.to_string(), provider.to_string(), method.to_string());
        let answers: Value = serde_json::from_str(&answers.to_string()).unwrap_or_default();
        let id = self.next_id + 1;
        self.as_mut().rust_mut().next_id = id;
        let (tx, rx) = mpsc::unbounded_channel();
        self.as_mut().rust_mut().current = Some((id, tx));
        // Its name, as the providers list has it.
        let title = self.rust().providers.get(&agent).and_then(|p| p["list"].as_array().cloned()).into_iter().flatten()
            .find(|p| p["id"] == provider.as_str()).and_then(|p| p["name"].as_str().map(str::to_owned)).unwrap_or_else(|| provider.clone());
        let flow = Flow { id, agent: agent.clone(), provider: provider.clone(), title, state: "running".into(), ask: Value::Null, ..Default::default() };
        self.as_mut().set_flow(QString::from(&flow.json()));
        let Some((k, host)) = target(&agent) else {
            let report = Report { flow: Arc::new(Mutex::new(flow)), thread: self.qt_thread(), host: None, forwards: Default::default() };
            report.finish(false, "harlo can't sign this agent in.");
            return;
        };
        let report = Report { flow: Arc::new(Mutex::new(flow)), thread: self.qt_thread(), host, forwards: Default::default() };
        crate::runtime().spawn(async move {
            let outcome = sign_in(k, &provider, &method, &answers, &report, rx).await;
            match outcome {
                Ok(said) => report.finish(true, &said),
                Err(e) => report.finish(false, &e),
            }
            // What it's signed in to now.
            let _ = report.thread.queue(move |mut this| {
                if this.current.as_ref().is_some_and(|(i, _)| *i == id) {
                    this.as_mut().rust_mut().current = None;
                }
                this.refresh(&QString::from(&agent));
            });
        });
    }

    pub fn answer(self: Pin<&mut Self>, text: &QString) {
        if let Some((_, tx)) = &self.current {
            let _ = tx.send(text.to_string());
        }
        // The box goes once answered (a new ask may follow).
        if let Ok(mut v) = serde_json::from_str::<Value>(&self.flow.to_string())
            && !v["ask"].is_null()
        {
            v["ask"] = Value::Null;
            let mut this = self;
            this.as_mut().set_flow(QString::from(&v.to_string()));
        }
    }

    pub fn cancel(mut self: Pin<&mut Self>) {
        if self.as_mut().rust_mut().current.take().is_some()
            && let Ok(mut v) = serde_json::from_str::<Value>(&self.flow.to_string())
            && v["state"] == "running"
        {
            v["state"] = json!("cancelled");
            v["ask"] = Value::Null;
            self.as_mut().set_flow(QString::from(&v.to_string()));
        }
    }

    pub fn sign_out(mut self: Pin<&mut Self>, agent: &QString, id: &QString) -> QString {
        let (agent_s, id) = (agent.to_string(), id.to_string());
        let Some((k, host)) = target(&agent_s) else { return QString::from("harlo can't sign this agent out.") };
        // In the background (the agent's own command takes a moment); then
        // what it's signed in to, or why it couldn't.
        let was = self.rust().status.get(&agent_s).cloned().unwrap_or_else(|| json!({ "accounts": [] }));
        self.as_mut().rust_mut().status.insert(agent_s.clone(), json!({ "state": "loading", "accounts": was["accounts"] }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = sign_out(k, &id, host.as_ref());
            let _ = thread.queue(move |mut this| match result {
                Ok(()) => this.refresh(&QString::from(&agent_s)),
                Err(e) => {
                    let mut v = this.rust().status.get(&agent_s).cloned().unwrap_or_default();
                    v["state"] = json!("ready");
                    v["error"] = json!(e);
                    this.as_mut().rust_mut().status.insert(agent_s, v);
                    this.bump();
                }
            });
        });
        QString::default()
    }

    pub fn release(self: Pin<&mut Self>) {
        if self.current.is_none() {
            *OPENCODE.lock().unwrap() = None;
            *OPENCODE_HOSTS.lock().unwrap() = None;
        }
    }

    pub fn extra_usage(mut self: Pin<&mut Self>, agent: &QString) -> bool {
        let agent = agent.to_string();
        // (`pi@host`: pi on that machine, its own sign-in.)
        let Some((k, host)) = target(&agent).filter(|(k, _)| matches!(k, HarnessKind::Pi | HarnessKind::Omp)) else { return false };
        let (on, fresh) = match self.rust().extra_usage.get(&agent) {
            Some((on, at)) => (*on, at.is_none_or(|t| t.elapsed() < Duration::from_secs(600))),
            None => (false, false),
        };
        if !fresh {
            self.as_mut().rust_mut().extra_usage.insert(agent.clone(), (on, None));
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let now = claude_subscription_in_use(k, host.as_ref());
                let _ = thread.queue(move |mut this| {
                    this.as_mut().rust_mut().extra_usage.insert(agent, (now, Some(Instant::now())));
                    if now != on {
                        this.bump();
                    }
                });
            });
        }
        on
    }
}

impl Accounts {
    pub fn servers(&self, agent: &QString) -> QString {
        let list = self.rust().servers.get(&agent.to_string()).cloned().unwrap_or_else(|| json!([]));
        QString::from(&list.to_string())
    }

    pub fn load_servers(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut all: Vec<(String, Value)> = [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::OpenCode, HarnessKind::Hermes].iter().map(|k| (k.as_str().to_owned(), servers_json(*k))).collect();
            // ("": those harlo set up that no agent has.)
            all.push((String::new(), unused_json()));
            let _ = thread.queue(move |mut this| {
                let changed = all.iter().any(|(k, v)| this.rust().servers.get(k) != Some(v));
                for (k, v) in all {
                    this.as_mut().rust_mut().servers.insert(k, v);
                }
                if changed {
                    this.bump();
                }
            });
        });
    }
}

/// An agent's model servers as the page shows them (blocking).
fn servers_json(k: HarnessKind) -> Value {
    Value::Array(
        model_servers::servers(k)
            .into_iter()
            .map(|s| {
                // Where it runs: this computer (harlo can start and stop it), the
                // local network (only whether it answers), or the internet.
                let host = s.base_url.split("://").nth(1).unwrap_or_default().split(['/', ':']).next().unwrap_or_default().trim_matches(['[', ']']).to_owned();
                let place = if ["127.0.0.1", "localhost", "::1", "0.0.0.0"].contains(&host.as_str()) {
                    "here"
                } else if harlo_harness::is_local_url(&s.base_url) {
                    "network"
                } else {
                    "online"
                };
                // Set up by harlo: it starts and stops it.
                let managed = place == "here" && commands_of(&s.base_url)["start"].as_str().is_some_and(|c| !c.is_empty());
                // Hermes takes it (not under its least context). A server on
                // this computer that didn't say: asked (Ollama's models).
                let hermes = model_servers::details(k, &s.name).is_none_or(|(url, _, mut models)| {
                    if place == "here" && models.iter().any(|m| m.context.is_none()) && model_servers::answers(&url) {
                        for m in models.iter_mut().filter(|m| m.context.is_none()) {
                            m.context = harlo_core::engines::model_context(&url, &m.id);
                        }
                    }
                    model_servers::hermes_fits(&models).is_ok()
                });
                let contexts: Vec<Value> = model_servers::details(k, &s.name).map(|(_, _, m)| m.iter().map(|m| json!(m.context)).collect()).unwrap_or_default();
                // Another machine's, reached here through SSH: that machine.
                let via = model_servers::through(&s.base_url).map(|(h, _)| h).unwrap_or_default();
                json!({
                    "name": s.name, "baseUrl": s.base_url, "models": s.models, "contexts": contexts, "place": place, "managed": managed, "hermesOk": hermes,
                    "engine": if via.is_empty() { engine_of(&s.base_url).to_owned() } else { String::new() }, "via": via,
                })
            })
            .collect(),
    )
}

impl Accounts {

    pub fn servers_file(&self, agent: &QString) -> QString {
        QString::from(&kind(&agent.to_string()).and_then(model_servers::file).filter(|f| f.is_file()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())
    }

    pub fn find_models(mut self: Pin<&mut Self>, url: &QString, key: &QString) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let (url, key) = (url.to_string(), key.to_string());
        // No address: none (the dialog opened again).
        if url.trim().is_empty() {
            self.as_mut().set_server_search(QString::default());
            return;
        }
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.as_mut().set_server_search(QString::from(&json!({ "id": id, "state": "finding", "models": [] }).to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = match model_servers::find(&url, &key) {
                Ok(l) => json!({ "id": id, "state": "ready", "baseUrl": l.base_url, "name": model_servers::suggest_name(&l.base_url),
                                 "models": l.models.iter().map(|m| json!({ "id": m.id, "context": m.context })).collect::<Vec<_>>(), "error": "" }),
                Err(e) => json!({ "id": id, "state": "failed", "models": [], "error": e }),
            };
            let _ = thread.queue(move |mut this| {
                // Only the latest search.
                if serde_json::from_str::<Value>(&this.rust().server_search.to_string()).ok().and_then(|v| v["id"].as_u64()) == Some(id) {
                    this.as_mut().set_server_search(QString::from(&v.to_string()));
                }
            });
        });
    }

    pub fn add_server(&self, agent: &QString, name: &QString, base_url: &QString, key: &QString, models: &QString, taken: &QString) -> QString {
        let Some(k) = kind(&agent.to_string()) else { return QString::from("harlo can't add model servers to this agent.") };
        let models: Vec<model_servers::Found> = serde_json::from_str::<Value>(&models.to_string())
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|m| Some(model_servers::Found { id: m["id"].as_str()?.to_owned(), context: m["context"].as_u64(), reasoning: m["reasoning"] == true, ..Default::default() }))
            .collect();
        let models = with_levels(&base_url.to_string(), models);
        let taken: Vec<String> = serde_json::from_str(&taken.to_string()).unwrap_or_default();
        match model_servers::add(k, &name.to_string(), &base_url.to_string(), &key.to_string(), &models, &taken) {
            // OpenCode's sessions read it when its server starts again.
            Ok(()) if !reread(k) => QString::from("OPENCODE_BUSY"),
            Ok(()) => QString::default(),
            Err(e) => QString::from(&e),
        }
    }
}

impl Accounts {
    pub fn copy_server(&self, from: &QString, to: &QString, name: &QString, taken: &QString) -> QString {
        let Some(to) = kind(&to.to_string()) else { return QString::from("harlo can't add model servers to this agent.") };
        let name = name.to_string();
        let (url, key, models) = match server_details(&from.to_string(), &name) {
            Ok(d) => d,
            Err(e) => return QString::from(&e),
        };
        let taken: Vec<String> = serde_json::from_str(&taken.to_string()).unwrap_or_default();
        match model_servers::add(to, &name, &url, &key, &models, &taken) {
            Ok(()) if !reread(to) => QString::from("OPENCODE_BUSY"),
            Ok(()) => QString::default(),
            Err(e) => QString::from(&e),
        }
    }

    pub fn load_host_servers(mut self: Pin<&mut Self>, host: &QString) {
        let host = host.to_string();
        if host.is_empty() {
            return;
        }
        if self.on_hosts[&host].is_null() {
            self.as_mut().change_host_servers(&host, |e| *e = json!({ "state": "loading", "agents": {} }));
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let read = read_host_servers(&host);
            let _ = thread.queue(move |mut this| {
                this.as_mut().change_host_servers(&host, |e| match read {
                    Ok(agents) => {
                        e["state"] = json!("ready");
                        e["agents"] = agents;
                        e["error"] = json!("");
                    }
                    Err(why) => {
                        e["state"] = json!("error");
                        e["error"] = json!(why);
                    }
                });
            });
        });
    }

    pub fn add_server_on(self: Pin<&mut Self>, host: &QString, agent: &QString, from: &QString, name: &QString) {
        let (from, name) = (from.to_string(), name.to_string());
        self.change_server_on(host, agent, move |host, kind| {
            let (url, key, models) = server_details(&from, &name)?;
            let models = with_levels(&url, models);
            model_servers::add_on(host, kind, &name, &url, &key, &models, true)
        });
    }

    pub fn remove_server_on(self: Pin<&mut Self>, host: &QString, agent: &QString, name: &QString) {
        let name = name.to_string();
        self.change_server_on(host, agent, move |host, kind| model_servers::remove_on(host, kind, &name));
    }

    /// An add or removal on a host, in the background, then its servers read
    /// again.
    fn change_server_on(mut self: Pin<&mut Self>, host: &QString, agent: &QString, change: impl FnOnce(&str, HarnessKind) -> Result<(), String> + Send + 'static) {
        let host = host.to_string();
        let Some(kind) = kind(&agent.to_string()).filter(|k| model_servers::ON_HOST.contains(k)) else { return };
        if self.on_hosts[&host]["busy"] == true {
            return;
        }
        self.as_mut().change_host_servers(&host, |e| {
            e["busy"] = json!(true);
            e["note"] = json!("");
        });
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let outcome = change(&host, kind);
            forget_host_read(&host);
            let read = read_host_servers(&host);
            let _ = thread.queue(move |mut this| {
                this.as_mut().change_host_servers(&host, |e| {
                    e["busy"] = json!(false);
                    e["note"] = json!(outcome.err().unwrap_or_default());
                    if let Ok(agents) = read {
                        e["state"] = json!("ready");
                        e["agents"] = agents;
                    }
                });
                // (Its sessions' model lists.)
                this.bump();
            });
        });
    }

    fn change_host_servers(mut self: Pin<&mut Self>, host: &str, f: impl FnOnce(&mut Value)) {
        let mut all = self.on_hosts.clone();
        if !all.is_object() {
            all = json!({});
        }
        if all[host].is_null() {
            all[host] = json!({ "state": "loading", "agents": {} });
        }
        f(&mut all[host]);
        let text = all.to_string();
        self.as_mut().rust_mut().on_hosts = all;
        self.as_mut().set_host_servers(QString::from(&text));
    }

    pub fn load_host_engines(mut self: Pin<&mut Self>, host: &QString) {
        let host = host.to_string();
        if host.is_empty() {
            return;
        }
        if self.engines_on[&host].is_null() {
            self.as_mut().change_host_engines(&host, |_| {});
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // (Nothing set up there yet: nothing put there to say so.)
            let read = harlo_core::engine_host::json_if_there(&host, &["overview"], Duration::from_secs(60)).map(|v| v.unwrap_or_else(|| json!({ "servers": [], "engines": [] })));
            let _ = thread.queue(move |mut this| this.as_mut().take_host_engines(&host, read));
        });
    }

    pub fn check_host_updates(mut self: Pin<&mut Self>, host: &QString, force: bool) {
        static CHECKED: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
        let host = host.to_string();
        {
            let mut checked = CHECKED.lock().unwrap();
            let m = checked.get_or_insert_with(HashMap::new);
            if host.is_empty() || (!force && m.get(&host).is_some_and(|t| t.elapsed() < Duration::from_secs(3600))) {
                return;
            }
            m.insert(host.clone(), Instant::now());
        }
        if self.engines_on[&host].is_null() {
            self.as_mut().change_host_engines(&host, |_| {});
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let Ok(Some(updates)) = harlo_core::engine_host::json_if_there(&host, &["updates"], Duration::from_secs(120)) else { return };
            let _ = thread.queue(move |mut this| this.as_mut().change_host_engines(&host, |e| e["updates"] = updates));
        });
    }

    pub fn use_host_engine(mut self: Pin<&mut Self>, host: &QString, server: &QString, agent: &QString, on: bool) {
        let host = host.to_string();
        let Some(k) = kind(&agent.to_string()) else { return };
        let v: Value = serde_json::from_str(&server.to_string()).unwrap_or_default();
        let (name, url) = (v["name"].as_str().unwrap_or_default().to_owned(), v["baseUrl"].as_str().unwrap_or_default().to_owned());
        if self.engines_on[&host]["busy"][&url] == "here" {
            return;
        }
        self.as_mut().change_host_engines(&host, |e| {
            e["busy"][&url] = json!("here");
            e["note"] = json!("");
        });
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let outcome = if on {
                (|| {
                    let here = model_servers::use_from(&host, &url)?;
                    // (Named for the machine too: this computer may have one of the same name.)
                    let tag: String = host.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
                    let models = with_levels(&url, saved_models(&v));
                    model_servers::add(k, &format!("{name}-{tag}"), &here, "", &models, &[])?;
                    reread(k);
                    Ok(())
                })()
            } else {
                let here = model_servers::used_here(&host, &url).unwrap_or_default();
                let mut out = Ok(());
                for s in model_servers::servers(k).into_iter().filter(|s| s.base_url.trim_end_matches('/') == here) {
                    out = out.and(model_servers::remove(k, &s.name));
                }
                reread(k);
                model_servers::forget_unused_uses();
                out
            };
            let read = harlo_core::engine_host::json(&host, &["overview"], Duration::from_secs(60));
            let _ = thread.queue(move |mut this| {
                this.as_mut().change_host_engines(&host, |e| {
                    if let Some(b) = e["busy"].as_object_mut() {
                        b.remove(&url);
                    }
                    e["note"] = json!(outcome.err().unwrap_or_default());
                });
                this.as_mut().take_host_engines(&host, read);
                this.as_mut().load_servers();
                this.as_mut().local_servers_changed();
            });
        });
    }

    fn take_host_engines(self: Pin<&mut Self>, host: &str, read: anyhow::Result<Value>) {
        self.change_host_engines(host, |e| match read {
            Ok(mut v) => {
                // (Those used here too: the address here.)
                for s in v["servers"].as_array_mut().into_iter().flatten() {
                    let here = s["baseUrl"].as_str().and_then(|u| model_servers::used_here(host, u)).unwrap_or_default();
                    s["here"] = json!(here);
                }
                e["state"] = json!("ready");
                e["list"] = v["servers"].take();
                e["engines"] = v["engines"].take();
                e["error"] = json!("");
            }
            Err(why) => {
                e["state"] = json!("error");
                e["error"] = json!(format!("{why:#}"));
            }
        });
    }

    pub fn start_host_engine(self: Pin<&mut Self>, host: &QString, url: &QString) {
        self.host_engine_op(host, url, "starting", |host, url| {
            let v = harlo_core::engine_host::json(host, &["start", url], Duration::from_secs(20 * 60)).map_err(|e| format!("{e:#}"))?;
            match v["error"].as_str() {
                Some(e) => Err(e.to_owned()),
                None => Ok(()),
            }
        });
    }

    pub fn stop_host_engine(self: Pin<&mut Self>, host: &QString, url: &QString) {
        self.host_engine_op(host, url, "stopping", |host, url| harlo_core::engine_host::run(host, &["stop", url], Duration::from_secs(120)).map(|_| ()).map_err(|e| format!("{e:#}")));
    }

    pub fn remove_host_engine(self: Pin<&mut Self>, host: &QString, url: &QString, name: &QString, delete_files: bool) {
        let name = name.to_string();
        self.host_engine_op(host, url, "removing", move |host, url| {
            // (Out of the agents there that have it first.)
            let mut errors = Vec::new();
            for kind in model_servers::ON_HOST {
                if servers_on_host_have(host, kind, &name, url) {
                    if let Err(e) = model_servers::remove_on(host, kind, &name) {
                        errors.push(format!("{}: {e}", kind.label()));
                    }
                }
            }
            // (And out of the agents here that reach it through SSH.)
            if let Some(here) = model_servers::used_here(host, url) {
                for kind in [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::OpenCode, HarnessKind::Hermes] {
                    for s in model_servers::servers(kind).into_iter().filter(|s| s.base_url.trim_end_matches('/') == here) {
                        if let Err(e) = model_servers::remove(kind, &s.name) {
                            errors.push(format!("{} here: {e}", kind.label()));
                        }
                    }
                    reread(kind);
                }
                model_servers::forget_unused_uses();
            }
            if !errors.is_empty() {
                return Err(errors.join("\n"));
            }
            let v = harlo_core::engine_host::json(host, &["remove", url, &name, if delete_files { "1" } else { "0" }], Duration::from_secs(600)).map_err(|e| format!("{e:#}"))?;
            let errors: Vec<String> = v["errors"].as_array().into_iter().flatten().filter_map(|e| e.as_str().map(str::to_owned)).collect();
            if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
        });
    }

    /// Something done to one of a machine's servers, there, in the
    /// background; its list read again after.
    fn host_engine_op(mut self: Pin<&mut Self>, host: &QString, url: &QString, doing: &str, op: impl FnOnce(&str, &str) -> Result<(), String> + Send + 'static) {
        let (host, url) = (host.to_string(), url.to_string());
        if !self.engines_on[&host]["busy"][&url].is_null() {
            return;
        }
        let doing = doing.to_owned();
        self.as_mut().change_host_engines(&host, |e| {
            e["busy"][&url] = json!(doing);
            e["note"] = json!("");
        });
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let outcome = op(&host, &url);
            forget_host_read(&host);
            let read = harlo_core::engine_host::json(&host, &["overview"], Duration::from_secs(60));
            let _ = thread.queue(move |mut this| {
                this.as_mut().change_host_engines(&host, |e| {
                    if let Some(b) = e["busy"].as_object_mut() {
                        b.remove(&url);
                    }
                    e["note"] = json!(outcome.err().unwrap_or_default());
                });
                this.as_mut().take_host_engines(&host, read);
                // (Agents there may have lost it.)
                this.as_mut().load_host_servers(&QString::from(&host));
            });
        });
    }

    fn change_host_engines(mut self: Pin<&mut Self>, host: &str, f: impl FnOnce(&mut Value)) {
        let mut all = self.engines_on.clone();
        if !all.is_object() {
            all = json!({});
        }
        if all[host].is_null() {
            all[host] = json!({ "state": "loading", "list": [], "busy": {} });
        }
        f(&mut all[host]);
        let text = all.to_string();
        self.as_mut().rust_mut().engines_on = all;
        self.as_mut().set_host_engines(QString::from(&text));
    }

    pub fn add_host_engine(mut self: Pin<&mut Self>, host: &QString, server: &QString) {
        let host = host.to_string();
        let v: Value = serde_json::from_str(&server.to_string()).unwrap_or_default();
        self.as_mut().change_host_servers(&host, |e| {
            e["busy"] = json!(true);
            e["note"] = json!("");
        });
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let (name, url) = (v["name"].as_str().unwrap_or_default().to_owned(), v["baseUrl"].as_str().unwrap_or_default().to_owned());
            let models: Vec<model_servers::Found> = saved_models(&v);
            let models = with_levels(&url, models);
            let mut errors = Vec::new();
            for agent in v["agents"].as_array().into_iter().flatten().filter_map(|a| kind(a.as_str()?)) {
                if let Err(e) = model_servers::add_on(&host, agent, &name, &url, "", &models, false) {
                    errors.push(format!("{}: {e}", agent.label()));
                }
            }
            forget_host_read(&host);
            let read = read_host_servers(&host);
            let _ = thread.queue(move |mut this| {
                this.as_mut().change_host_servers(&host, |e| {
                    e["busy"] = json!(false);
                    e["note"] = json!(errors.join("\n"));
                    if let Ok(agents) = read {
                        e["state"] = json!("ready");
                        e["agents"] = agents;
                    }
                });
                this.bump();
            });
        });
    }

    pub fn server_files(&self, url: &QString) -> QString {
        let url = url.to_string();
        // Ollama's: its models in Ollama's store (sizes while it runs).
        if engine_of(&url) == "Ollama" {
            let names = ollama_models_of(&url);
            let sizes = if model_servers::answers(&url) { harlo_core::engines::ollama_sizes(&url) } else { Vec::new() };
            let size: u64 = sizes.iter().filter(|(n, _)| names.contains(n)).map(|(_, s)| s).sum();
            return QString::from(&json!({ "files": names, "size": size, "ollama": true }).to_string());
        }
        let files = model_files(&url);
        let size: u64 = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
        QString::from(&json!({ "files": files, "size": size }).to_string())
    }

    pub fn remove_server_from(mut self: Pin<&mut Self>, agent: &QString, name: &QString) -> QString {
        let Some(k) = kind(&agent.to_string()) else { return QString::from("harlo can't change this agent's model servers.") };
        let out = match model_servers::remove(k, &name.to_string()) {
            Ok(()) if !reread(k) => QString::from("OPENCODE_BUSY"),
            Ok(()) => QString::default(),
            Err(e) => QString::from(&e),
        };
        // (Another machine's, used here: its forward goes when nothing here has it.)
        model_servers::forget_unused_uses();
        self.as_mut().load_servers();
        out
    }

    pub fn remove_server(mut self: Pin<&mut Self>, name: &QString, url: &QString, delete_files: bool) {
        let (name, url) = (name.to_string(), url.to_string());
        let first = json!({ "url": url, "state": "running", "step": "Removing it", "done": 0, "total": 0, "error": "", "busy": false });
        self.as_mut().set_removal(QString::from(&first.to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // How far it is, for the dialog.
            let state = std::sync::Mutex::new(first);
            let say = |f: &dyn Fn(&mut Value)| {
                let text = {
                    let mut v = state.lock().unwrap();
                    f(&mut v);
                    v.to_string()
                };
                let _ = thread.queue(move |mut this| this.as_mut().set_removal(QString::from(&text)));
            };
            let step = |text: &str, done: usize, total: usize| {
                say(&|v| {
                    v["step"] = json!(text);
                    v["done"] = json!(done);
                    v["total"] = json!(total);
                })
            };
            let (errors, busy, summary) = remove(&name, &url, delete_files, &step, &reread);
            say(&|v| {
                v["state"] = json!("done");
                v["error"] = json!(errors.join("\n"));
                v["busy"] = json!(busy);
                v["summary"] = json!(summary);
            });
            let _ = thread.queue(|mut this| this.as_mut().load_servers());
        });
    }
}


/// After harlo changed an agent's model servers: its sessions read them
/// again. OpenCode's server reloads when it's idle (false: busy, its open
/// sessions get them when they close); pi's and omp's idle sessions start
/// again when next used (they read their models only when they start).
fn reread(k: HarnessKind) -> bool {
    match k {
        HarnessKind::OpenCode => harlo_harness::reload_opencode_if_idle(),
        HarnessKind::Pi | HarnessKind::Omp => {
            crate::registry::restart_idle(k.as_str());
            true
        }
        _ => true,
    }
}











// ---- starting and stopping model servers --------------------------------------










/// Servers harlo set up that no agent has (closed before adding them, or
/// unticked everywhere): listed like the others, with no agent ticked.
fn unused_json() -> Value {
    let used: Vec<String> = [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::OpenCode, HarnessKind::Hermes].iter().flat_map(|k| model_servers::servers(*k)).map(|s| s.base_url).collect();
    Value::Array(
        set_up_servers()
            .into_iter()
            .filter(|(url, _)| !used.contains(url))
            .map(|(url, v)| {
                let models: Vec<model_servers::Found> = saved_models(&v);
                // (One set up before harlo saved names: its engine's.)
                let name = v["name"].as_str().filter(|n| !n.is_empty()).map(str::to_owned).unwrap_or_else(|| {
                    match engine_of(&url) {
                        "" | "Your command" => model_servers::suggest_name(&url),
                        engine => engine.to_lowercase(),
                    }
                });
                json!({
                    "name": name, "baseUrl": url, "models": models.iter().map(|m| m.id.clone()).collect::<Vec<_>>(), "contexts": models.iter().map(|m| json!(m.context)).collect::<Vec<_>>(),
                    "place": "here", "managed": true, "hermesOk": model_servers::hermes_fits(&models).is_ok(), "engine": engine_of(&url),
                })
            })
            .collect(),
    )
}









/// A server's models as an agent's file gets them: Basalt, Strata and NInfer's
/// Qwen3.8 tell thinking levels apart (its Qwen3.6 has only on and off).
fn with_levels(url: &str, models: Vec<model_servers::Found>) -> Vec<model_servers::Found> {
    let engine = engine_of(url);
    models.into_iter().map(|m| model_servers::Found { levels: engine == "Basalt" || engine == "Strata" || (engine == "NInfer" && m.id.contains("3.8")), ..m }).collect()
}

/// Basalt and Strata servers agents got before harlo offered their levels: offered now,
/// before any agent reads its file.
pub(crate) fn offer_levels() {
    for (url, _) in set_up_servers().into_iter().filter(|(url, _)| matches!(engine_of(url), "Basalt" | "Strata")) {
        for k in [HarnessKind::Pi, HarnessKind::OpenCode] {
            for s in model_servers::servers(k).into_iter().filter(|s| s.base_url == url) {
                if let Err(e) = model_servers::offer_levels(k, &s.name) {
                    tracing::warn!("thinking levels for {}: {e}", s.name);
                }
            }
        }
    }
}




/// The server behind a session's model when harlo can start it first.
#[derive(Clone, Debug)]
pub(crate) enum ModelServer {
    /// Set up on this computer.
    Here { name: String, url: String },
    /// Set up on `host`, for a session there: started there.
    There { host: String, name: String, url: String },
    /// Set up on another machine, for a session here: `url` is the address
    /// here its port is forwarded to.
    Through { name: String, url: String },
}

impl ModelServer {
    pub fn name(&self) -> &str {
        match self {
            ModelServer::Here { name, .. } | ModelServer::There { name, .. } | ModelServer::Through { name, .. } => name,
        }
    }

    /// Whether it has to start (or be reached again) before a message
    /// (blocking; asks the other machine for one there).
    pub fn needs_start(&self) -> bool {
        match self {
            ModelServer::Here { url, .. } => harlo_core::servers::needs_start(url),
            ModelServer::There { host, url, .. } => harlo_core::engine_host::json(host, &["needs-start", url], Duration::from_secs(30)).map_or(true, |v| v != false),
            ModelServer::Through { url, .. } => !model_servers::answers(url),
        }
    }

    /// Starts it, until it answers (blocking).
    pub fn start(&self) -> Result<(), String> {
        match self {
            ModelServer::Here { url, .. } => harlo_core::servers::ensure_running(url),
            ModelServer::There { host, url, .. } => {
                let v = harlo_core::engine_host::json(host, &["start", url], Duration::from_secs(20 * 60)).map_err(|e| format!("{e:#}"))?;
                v["error"].as_str().map_or(Ok(()), |e| Err(e.to_owned()))
            }
            ModelServer::Through { url, .. } => model_servers::reach(url),
        }
    }
}

/// The server a model (`provider/id`, Hermes's `custom:…`) is on, among an
/// agent's `list`, and the provider's name for it.
fn server_among(k: HarnessKind, model: &str, list: &[model_servers::Server]) -> Option<(String, Option<model_servers::Server>)> {
    // Hermes: "custom:<server>:<model>", or its current model as just
    // "custom:<model>" (the server whose models have it, then).
    let provider = match k {
        HarnessKind::Hermes => match model_servers::hermes_server_of(model).filter(|n| list.iter().any(|s| s.name == *n)) {
            Some(name) => name.to_owned(),
            None => {
                let id = model.strip_prefix("custom:").or_else(|| model.strip_prefix("custom/")).unwrap_or(model);
                list.iter().find(|s| s.models.iter().any(|m| m == id))?.name.clone()
            }
        },
        _ => model.split_once('/')?.0.to_owned(),
    };
    let found = list.iter().find(|s| s.name == provider).cloned();
    Some((provider, found))
}

/// omp's own "llama.cpp" and "ollama" providers: the engines' usual
/// addresses, where it finds a running server by itself.
fn omp_usual(provider: &str) -> Option<[String; 2]> {
    let port = match provider {
        "llama.cpp" => 8080,
        "ollama" => 11434,
        _ => return None,
    };
    Some([format!("http://127.0.0.1:{port}/v1"), format!("http://localhost:{port}/v1")])
}

/// The server behind a session's model on this computer when harlo can
/// start it: one it set up here, or another machine's reached through SSH.
pub(crate) fn managed_server(agent: &str, model: &str) -> Option<ModelServer> {
    let k = kind(agent)?;
    let managed = |url: &str| commands_of(url)["start"].as_str().is_some_and(|c| !c.is_empty());
    let (provider, found) = server_among(k, model, &model_servers::servers(k))?;
    if let Some(s) = found {
        if managed(&s.base_url) {
            return Some(ModelServer::Here { name: s.name, url: s.base_url });
        }
        if model_servers::through(&s.base_url).is_some() {
            return Some(ModelServer::Through { name: s.name, url: s.base_url });
        }
    }
    if k == HarnessKind::Omp
        && let Some(urls) = omp_usual(&provider)
    {
        let url = urls.iter().find(|u| managed(u))?.clone();
        let name = [HarnessKind::Pi, HarnessKind::OpenCode, HarnessKind::Hermes, HarnessKind::Omp]
            .iter()
            .find_map(|k| model_servers::servers(*k).into_iter().find(|s| s.base_url.trim_end_matches('/') == url))
            .map_or_else(|| provider.clone(), |s| s.name);
        return Some(ModelServer::Here { name, url });
    }
    None
}

/// What harlo read on another machine a moment ago: its agents' servers and
/// the servers harlo set up there (a minute; a change there clears it).
static HOST_READ: Mutex<Option<HashMap<String, (Instant, Value)>>> = Mutex::new(None);

fn host_read(key: String, read: impl FnOnce() -> Option<Value>) -> Option<Value> {
    if let Some((at, v)) = HOST_READ.lock().unwrap().get_or_insert_with(HashMap::new).get(&key)
        && at.elapsed() < Duration::from_secs(60)
    {
        return Some(v.clone());
    }
    let got = read()?;
    HOST_READ.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, (Instant::now(), got.clone()));
    Some(got)
}

/// After a change on `host`: read again next time.
pub(crate) fn forget_host_read(host: &str) {
    if let Some(m) = HOST_READ.lock().unwrap().as_mut() {
        m.retain(|k, _| k.split('\n').next() != Some(host));
    }
}

/// The server behind a session's model on `host` when harlo can start it:
/// one it set up there, or this computer's that its agent reaches through
/// SSH (blocking: reads the agent's file there, a minute at most old).
pub(crate) fn host_server(host: &str, agent: &str, model: &str) -> Option<ModelServer> {
    let k = kind(agent).filter(|k| model_servers::ON_HOST.contains(k))?;
    let list = host_read(format!("{host}\nagent {}", k.as_str()), || {
        model_servers::servers_on(host, k).ok().map(|l| Value::Array(l.into_iter().map(|s| json!({ "name": s.name, "url": s.base_url, "models": s.models })).collect()))
    })?;
    let list: Vec<model_servers::Server> = list
        .as_array()?
        .iter()
        .map(|s| model_servers::Server {
            name: s["name"].as_str().unwrap_or_default().to_owned(),
            base_url: s["url"].as_str().unwrap_or_default().to_owned(),
            models: s["models"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(str::to_owned)).collect(),
        })
        .collect();
    let (provider, found) = server_among(k, model, &list)?;
    let (name, url) = match found {
        Some(s) => (s.name, s.base_url),
        None => (provider.clone(), omp_usual(&provider).filter(|_| k == HarnessKind::Omp)?[0].clone()),
    };
    // This computer's, forwarded there.
    if model_servers::local_port(&url).is_some_and(|p| model_servers::forwards(host).contains(&p)) && commands_of(&url)["start"].as_str().is_some_and(|c| !c.is_empty()) {
        return Some(ModelServer::Here { name, url });
    }
    // (Only one on that machine's own address can be harlo's there.)
    model_servers::local_port(&url)?;
    let set_up = host_read(format!("{host}\nset up"), || harlo_core::engine_host::json_if_there(host, &["servers"], Duration::from_secs(60)).ok().flatten())?;
    let set_up: Vec<&str> = set_up.as_array()?.iter().filter_map(|s| s["baseUrl"].as_str()).collect();
    let same = |a: &str| a.trim_end_matches('/') == url.trim_end_matches('/') || a.replace("localhost", "127.0.0.1").trim_end_matches('/') == url.replace("localhost", "127.0.0.1").trim_end_matches('/');
    set_up.iter().any(|u| same(u)).then(|| ModelServer::There { host: host.to_owned(), name, url })
}






/// After a llama.cpp server harlo set up starts: the context it serves
/// this time (it fits it to free memory) into each agent that has it.
/// What couldn't be updated (Hermes under its least context, say).
pub(crate) fn sync_context(url: &str) -> Vec<String> {
    let start = commands_of(url)["start"].as_str().unwrap_or_default().to_owned();
    let llama = start.contains("llama-server");
    if !llama && !start.contains("ollama") {
        return Vec::new();
    }
    let served = if llama { harlo_core::engines::served_context(url) } else { None };
    let mut problems = Vec::new();
    for k in [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::OpenCode, HarnessKind::Hermes] {
        for s in model_servers::servers(k).into_iter().filter(|s| s.base_url == url) {
            // llama.cpp: what it serves this time. Ollama (nothing loaded
            // yet): only a context the agent lacks, from the model's own.
            let context = match served {
                Some(c) => c,
                None => {
                    let Some((_, _, models)) = model_servers::details(k, &s.name) else { continue };
                    if models.iter().all(|m| m.context.is_some()) {
                        continue;
                    }
                    let found: Vec<Option<u64>> = models.iter().map(|m| m.context.or_else(|| harlo_core::engines::model_context(url, &m.id))).collect();
                    match found.first().copied().flatten() {
                        Some(c) if found.iter().all(|f| *f == Some(c)) => c,
                        _ => continue,
                    }
                }
            };
            match model_servers::set_context(k, &s.name, context) {
                Ok(true) => {
                    reread(k);
                }
                Ok(_) => {}
                Err(e) => problems.push(format!("{}: {e}", k.label())),
            }
        }
    }
    problems
}







impl Accounts {
    fn set_state(mut self: Pin<&mut Self>, url: &str, state: Value) {
        let mut all: Value = serde_json::from_str(&self.server_states.to_string()).unwrap_or_else(|_| json!({}));
        all[url] = state;
        self.as_mut().set_server_states(QString::from(&all.to_string()));
    }

    pub fn check_servers(self: Pin<&mut Self>, urls: &QString) {
        let urls: Vec<String> = serde_json::from_str(&urls.to_string()).unwrap_or_default();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let found: Vec<(String, bool)> = urls.into_iter().map(|u| {
                let up = model_servers::answers(&u);
                (u, up)
            }).collect();
            let _ = thread.queue(move |mut this| {
                let all: Value = serde_json::from_str(&this.server_states.to_string()).unwrap_or_else(|_| json!({}));
                for (url, up) in found {
                    let was = &all[&url];
                    let harlo = STARTED.lock().unwrap().as_ref().is_some_and(|m| m.contains_key(&url));
                    // (Starting: its own watch says when it's up or failed.)
                    if was["state"] == "starting" {
                        continue;
                    }
                    let state = match (up, was["state"].as_str()) {
                        // (Still running: what its start said stays.)
                        (true, Some("running")) => json!({ "state": "running", "harlo": harlo, "message": was["message"] }),
                        (true, _) => json!({ "state": "running", "harlo": harlo, "message": "" }),
                        (false, Some("failed")) => was.clone(),
                        (false, _) => json!({ "state": "stopped", "harlo": false, "message": "" }),
                    };
                    if *was != state {
                        this.as_mut().set_state(&url, state);
                    }
                }
            });
        });
    }


    pub fn server_log(&self, url: &QString) -> QString {
        QString::from(&Some(log_path(&url.to_string())).filter(|f| f.is_file()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())
    }

    pub fn start_server(mut self: Pin<&mut Self>, url: &QString) {
        let url = url.to_string();
        let command = commands_of(&url)["start"].as_str().unwrap_or_default().to_owned();
        if command.is_empty() {
            return self.set_state(&url, json!({ "state": "failed", "harlo": false, "message": "Set its start command first." }));
        }
        if let Err(e) = launch(&url, &command) {
            return self.set_state(&url, json!({ "state": "failed", "harlo": false, "message": e }));
        }
        self.as_mut().set_state(&url, json!({ "state": "starting", "harlo": true, "message": "" }));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let state = match wait_up(&url, Duration::from_secs(15 * 60), &mut |_| {}, &|| false) {
                // (The context it serves this time, into the agents.)
                Ok(held) => json!({ "state": "running", "harlo": held, "message": sync_context(&url).join("\n") }),
                Err(e) => json!({ "state": "failed", "harlo": false, "message": e }),
            };
            // (The agents' files may say another context now.)
            let _ = thread.queue(move |mut this| {
                this.as_mut().set_state(&url, state);
                this.as_mut().load_servers();
            });
        });
    }

    pub fn stop_server(mut self: Pin<&mut Self>, url: &QString) {
        let url = url.to_string();
        let started = STARTED.lock().unwrap().as_mut().and_then(|m| m.remove(&url));
        let stop = commands_of(&url)["stop"].as_str().unwrap_or_default().to_owned();
        // One harlo set up, left running by an earlier harlo: its process.
        let left = if started.is_none() && stop.is_empty() { leftovers(&url) } else { Vec::new() };
        if started.is_none() && stop.is_empty() && left.is_empty() {
            return self.set_state(&url, json!({ "state": "running", "harlo": false, "message": "harlo didn't start it: set its stop command to stop it from here." }));
        }
        self.as_mut().set_state(&url, json!({ "state": "starting", "harlo": false, "message": "Stopping…" }));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let outcome = match started {
                Some(child) => {
                    stop_group(child);
                    Ok(())
                }
                None if !left.is_empty() => {
                    stop_pids(&left);
                    Ok(())
                }
                None => std::process::Command::new("sh").arg("-c").arg(&stop).stdin(std::process::Stdio::null()).output().map_err(|e| e.to_string()).and_then(|o| {
                    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().lines().last().unwrap_or("It failed.").to_owned()) }
                }),
            };
            // (Its port may take a moment to close.)
            for _ in 0..20 {
                if !model_servers::answers(&url) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            let up = model_servers::answers(&url);
            let state = match outcome {
                Err(e) => json!({ "state": if up { "running" } else { "stopped" }, "harlo": false, "message": format!("The stop command failed: {e}") }),
                Ok(()) if up => json!({ "state": "running", "harlo": false, "message": "It still answers." }),
                Ok(()) => json!({ "state": "stopped", "harlo": false, "message": "" }),
            };
            let _ = thread.queue(move |this| this.set_state(&url, state));
        });
    }
}

impl Accounts {
    pub fn packages(&self) -> QString {
        let v = &self.rust().packages;
        if v.is_null() {
            return QString::from(&json!({ "state": "loading", "list": [], "error": "", "job": null }).to_string());
        }
        QString::from(&v.to_string())
    }

    pub fn package_rules_file(&self, host: &QString) -> QString {
        let file = match package_host(&host.to_string()) {
            Ok(None) => pi_packages::policy_file(pi_packages::At::Here),
            Ok(Some(h)) => pi_packages::policy_file(pi_packages::At::On(&h)),
            Err(e) => Err(e),
        };
        QString::from(&file.map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())
    }

    pub fn load_packages(mut self: Pin<&mut Self>, host: &QString) {
        let host = host.to_string();
        if self.rust().packages.is_null() || self.rust().packages["host"] != host.as_str() {
            self.as_mut().rust_mut().packages = json!({ "list": [], "job": null, "error": "", "host": host });
        }
        self.as_mut().rust_mut().packages["state"] = json!("loading");
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let at_host = package_host(&host);
            let (installed, enabled): (Result<Vec<pi_packages::Package>, String>, Vec<bool>) = match &at_host {
                Err(e) => (Err(e.clone()), Vec::new()),
                Ok(h) => {
                    let at = h.as_ref().map_or(pi_packages::At::Here, pi_packages::At::On);
                    let installed = pi_program(&host).and_then(|pi| pi_packages::list(at, &pi, &home()));
                    (installed, pi_packages::EXTENSIONS.iter().map(|e| pi_packages::enabled_at(at, e.name)).collect())
                }
            };
            let row = |i: usize, e: &pi_packages::Extension, version: &str, latest: &str| {
                json!({ "name": e.name, "title": e.title, "what": e.what, "tokens": e.tokens, "optional": e.optional, "note": e.note, "enabled": enabled.get(i).copied().unwrap_or(false), "version": version, "latest": latest, "newer": pi_packages::is_newer(version, latest) })
            };
            let error = installed.as_ref().err().cloned().unwrap_or_default();
            let installed = installed.unwrap_or_default();
            let version = |name: &str| installed.iter().find(|p| !p.project && pi_packages::name_of(&p.source) == name).map(|p| p.version.clone()).unwrap_or_default();
            let list: Vec<Value> = pi_packages::EXTENSIONS.iter().enumerate().map(|(i, e)| row(i, e, &version(e.name), "")).collect();
            let shown_host = host.clone();
            let shown = thread.queue(move |mut this| {
                // (Another machine's page came up meanwhile: this one's left.)
                if this.rust().packages["host"] != shown_host.as_str() {
                    return;
                }
                let v = &mut this.as_mut().rust_mut().packages;
                v["state"] = json!("ready");
                v["list"] = json!(list);
                v["error"] = json!(error);
                this.bump();
            });
            if shown.is_err() {
                return;
            }
            // What's latest comes after (npm may be slow, or out of reach).
            let list: Vec<Value> = pi_packages::EXTENSIONS.iter().enumerate().map(|(i, e)| row(i, e, &version(e.name), &pi_packages::latest(e.name).unwrap_or_default())).collect();
            let _ = thread.queue(move |mut this| {
                if this.rust().packages["host"] != host.as_str() {
                    return;
                }
                this.as_mut().rust_mut().packages["list"] = json!(list);
                this.bump();
            });
        });
    }

    pub fn update_package(mut self: Pin<&mut Self>, host: &QString, name: &QString) {
        let (host, name) = (host.to_string(), name.to_string());
        if self.rust().packages["job"]["state"] == "running" {
            return;
        }
        let list = self.rust().packages["list"].as_array().cloned().unwrap_or_default();
        self.as_mut().rust_mut().packages["job"] = json!({ "source": name, "state": "running", "message": "" });
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = package_host(&host).and_then(|h| {
                let at = h.as_ref().map_or(pi_packages::At::Here, pi_packages::At::On);
                let pi = pi_program(&host)?;
                let mut failed = Vec::new();
                for row in list.iter().filter(|r| name == "all" && r["newer"] == true || r["name"] == name.as_str()) {
                    let (package, latest) = (row["name"].as_str().unwrap_or_default(), row["latest"].as_str().unwrap_or_default());
                    if !latest.is_empty()
                        && let Err(why) = pi_packages::install(at, &pi, &home(), package, latest)
                    {
                        failed.push(format!("{}: {why}", row["title"].as_str().unwrap_or(package)));
                    }
                }
                if failed.is_empty() { Ok(()) } else { Err(failed.join("\n")) }
            });
            let _ = thread.queue(move |mut this| {
                let (state, message) = match result {
                    Ok(()) => {
                        crate::registry::restart_when_idle_on("pi", &host);
                        ("done", "Done. New pi sessions use it; open ones as soon as they're idle.".to_owned())
                    }
                    Err(why) => ("failed", why),
                };
                this.as_mut().rust_mut().packages["job"] = json!({ "source": name, "state": state, "message": message });
                this.load_packages(&QString::from(&host));
            });
        });
    }

    pub fn set_package_enabled(mut self: Pin<&mut Self>, host: &QString, name: &QString, on: bool) {
        let (host, name) = (host.to_string(), name.to_string());
        if self.rust().packages["job"]["state"] == "running" {
            return;
        }
        self.as_mut().rust_mut().packages["job"] = json!({ "source": name, "state": "running", "message": "" });
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = package_host(&host).and_then(|h| {
                let at = h.as_ref().map_or(pi_packages::At::Here, pi_packages::At::On);
                pi_packages::switch(at, &pi_program(&host)?, &home(), &name, on)
            });
            // (Its sessions' shield and lists follow the settings as read now.)
            if !host.is_empty() {
                harlo_core::remote::home_file_cached(&host, pi_packages::SETTINGS_REL, Duration::ZERO);
            }
            let _ = thread.queue(move |mut this| {
                let (state, message) = match result {
                    Ok(()) => {
                        crate::registry::restart_when_idle_on("pi", &host);
                        ("done", if on { "On. New pi sessions have it; open ones as soon as they're idle." } else { "Off. New pi sessions are without it; open ones as soon as they're idle." }.to_owned())
                    }
                    Err(why) => ("failed", why),
                };
                this.as_mut().rust_mut().packages["job"] = json!({ "source": name, "state": state, "message": message });
                this.load_packages(&QString::from(&host));
            });
        });
    }

    pub fn set_up_packages(mut self: Pin<&mut Self>) {
        let Some(pi) = program(HarnessKind::Pi) else { return };
        if pi_packages::to_set_up(pi_packages::At::Here).is_empty() {
            return;
        }
        if self.rust().packages.is_null() {
            self.as_mut().rust_mut().packages = json!({ "state": "loading", "list": [], "error": "", "host": "" });
        }
        if self.rust().packages["host"].as_str().unwrap_or_default().is_empty() {
            self.as_mut().rust_mut().packages["job"] = json!({ "source": "all", "state": "running", "message": "Installing pi's extensions (they stay off until you turn them on)…" });
        }
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let adding = thread.clone();
            let result = pi_packages::ensure(pi_packages::At::Here, &pi, &home(), &|e: &pi_packages::Extension| {
                let message = format!("Installing pi's extensions (they stay off until you turn them on): {}…", e.title);
                let _ = adding.queue(move |mut this| {
                    this.as_mut().rust_mut().packages["job"]["message"] = json!(message);
                    this.bump();
                });
            });
            let _ = thread.queue(move |mut this| {
                let (state, message) = match result {
                    Ok(()) => ("done", "pi's extensions are installed. Turn on the ones you want.".to_owned()),
                    Err(why) => ("failed", format!("Couldn't install all of pi's extensions; harlo tries again next time it starts.\n{why}")),
                };
                // (This machine's; another's page shown stays as it is.)
                if this.rust().packages["host"].as_str().unwrap_or_default().is_empty() {
                    this.as_mut().rust_mut().packages["job"] = json!({ "source": "all", "state": state, "message": message });
                    this.load_packages(&QString::default());
                }
            });
        });
    }
}

/// Where pi's package commands run.
fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// Whether the agent uses Claude through a Claude subscription (blocking):
/// pi as its own warning checks (not when it's switched off in pi's
/// settings), omp from what it's signed in to.
fn claude_subscription_in_use(k: HarnessKind, host: Option<&Host>) -> bool {
    match k {
        HarnessKind::Pi => match host {
            Some(h) => pi_helper_on(h, &["subscription"], Duration::from_secs(60)),
            None => pi_helper(&["subscription"], Duration::from_secs(30)),
        }
        .ok()
        .and_then(|v| v["subscription"].as_bool())
        .unwrap_or(false),
        // (omp keeps it in its database: another machine's isn't read.)
        HarnessKind::Omp => host.is_none() && signin::omp_claude_subscription(None),
        _ => false,
    }
}

/// The machine whose pi the Extensions page shows: None for this one; an
/// error when it's unknown or not connected.
fn package_host(host: &str) -> Result<Option<harlo_core::remote::Host>, String> {
    if host.is_empty() {
        return Ok(None);
    }
    if harlo_core::remote::is_disconnected(host) {
        return Err(format!("{host} isn't connected."));
    }
    harlo_core::remote::host(host).map(Some).ok_or_else(|| format!("There's no machine called {host}."))
}

/// pi there: this machine's program, or (another's) just its name.
fn pi_program(host: &str) -> Result<PathBuf, String> {
    if host.is_empty() {
        return program(HarnessKind::Pi).ok_or_else(|| "pi isn't installed.".to_owned());
    }
    if harlo_core::remote::info_of(host).is_some_and(|i| !i.has("pi")) {
        return Err(format!("pi isn't installed on {host}."));
    }
    Ok(PathBuf::from("pi"))
}

// ---- what each agent is signed in to ------------------------------------------

/// `[{ id, label, detail, signOut }]` (blocking).
fn accounts_of(k: HarnessKind, host: Option<&Host>) -> Result<Vec<Value>, String> {
    match k {
        HarnessKind::Claude => {
            let out = match host {
                Some(h) => run_there(h, "claude", &["auth", "status", "--json"], Duration::from_secs(20))?,
                None => run(&program(k).ok_or("Claude Code isn't installed.")?, &["auth", "status", "--json"], Duration::from_secs(15))?,
            };
            let v: Value = serde_json::from_str(&out).map_err(|_| "Claude Code didn't say whether it's signed in.".to_owned())?;
            if v["loggedIn"].as_bool() != Some(true) {
                return Ok(Vec::new());
            }
            let plan = v["subscriptionType"].as_str().filter(|s| !s.is_empty()).map(|s| {
                let mut c = s.chars();
                format!("Claude {}", c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default())
            });
            let detail = plan.unwrap_or_else(|| match v["authMethod"].as_str().unwrap_or_default() {
                "claude.ai" => "Claude account".to_owned(),
                "" | "none" => "Signed in".to_owned(),
                m => format!("Signed in ({m})"),
            });
            let label = v["email"].as_str().filter(|e| !e.is_empty()).or(v["orgName"].as_str()).unwrap_or("Claude").to_owned();
            Ok(vec![json!({ "id": "claude", "label": label, "detail": detail, "signOut": true })])
        }
        HarnessKind::Pi => {
            let v = match host {
                Some(h) => pi_helper_on(h, &["list"], Duration::from_secs(60))?,
                None => pi_helper(&["list"], Duration::from_secs(30))?,
            };
            let stored: Vec<String> = v["credentials"].as_array().into_iter().flatten().filter_map(|c| c["id"].as_str().map(str::to_owned)).collect();
            Ok(v["providers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["configured"].as_bool() == Some(true))
                .map(|p| {
                    let id = p["id"].as_str().unwrap_or_default();
                    let has = stored.iter().any(|s| s == id);
                    let source = p["source"].as_str().unwrap_or_default();
                    json!({ "id": id, "label": p["name"], "signOut": has, "detail": if has { "Signed in".to_owned() } else { pi_source(source) } })
                })
                .collect())
        }
        HarnessKind::OpenCode => {
            let server = opencode_server_on(host)?;
            let v = server.get("/api/integration", None).map_err(|e| format!("{e:#}"))?;
            let mut out = Vec::new();
            for i in v["data"].as_array().into_iter().flatten() {
                for c in i["connections"].as_array().into_iter().flatten() {
                    let detail = match (c["type"].as_str(), c["method"].as_str()) {
                        (Some("env"), _) => format!("From ${}", c["name"].as_str().unwrap_or_default()),
                        (_, Some("key")) => "API key".to_owned(),
                        (_, Some("oauth")) => "Signed in".to_owned(),
                        _ => "Connected".to_owned(),
                    };
                    let needs = c["status"]["status"] == "needs_auth";
                    out.push(json!({ "id": c["id"].as_str().unwrap_or_default(), "label": i["name"], "signOut": c["type"] == "credential",
                                     "detail": if needs { "Needs signing in again".to_owned() } else { detail } }));
                }
            }
            Ok(out)
        }
        HarnessKind::Hermes => Ok(hermes_signed_in(host)
            .into_iter()
            .map(|id| {
                let name = HERMES_PROVIDERS.iter().find(|p| p.0 == id).map_or(id.as_str(), |p| p.1);
                json!({ "id": id, "label": name, "detail": "Signed in", "signOut": true })
            })
            .collect()),
        // omp: from its models (Settings → Providers has them).
        _ => Ok(Vec::new()),
    }
}

/// Where pi found a provider's key, in words ("models_json_command" →
/// "a command in models.json", "ANTHROPIC_API_KEY" → "$ANTHROPIC_API_KEY").
fn pi_source(source: &str) -> String {
    match source {
        "" => "Set up outside pi".into(),
        "models_json_key" => "Key in pi's models.json".into(),
        "models_json_command" => "Command in pi's models.json".into(),
        s if s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') => format!("From ${s}"),
        s => format!("From {}", s.replace('_', " ")),
    }
}

/// What an agent can sign in to (blocking).
fn providers_of(k: HarnessKind, host: Option<&Host>) -> Result<Vec<Value>, String> {
    match k {
        HarnessKind::Claude => Ok(vec![
            json!({ "id": "claudeai", "name": "Claude account", "note": "Pro, Max, Team or Enterprise: sign in on claude.ai in your browser.",
                    "methods": [{ "id": "claudeai", "type": "login", "label": "Sign in" }] }),
            json!({ "id": "console", "name": "Anthropic Console", "note": "Pay as you go with API billing.",
                    "methods": [{ "id": "console", "type": "login", "label": "Sign in" }] }),
        ]),
        HarnessKind::Omp => {
            let list = omp_list(agent_command(k, "omp", host, &["login"])?)?;
            Ok(list
                .into_iter()
                .map(|(n, name)| {
                    let blocked = signin::claude_subscription(&name);
                    json!({ "id": n.to_string(), "name": name, "note": if blocked { CLAUDE_SUBSCRIPTION } else { "" },
                            "methods": [{ "id": "login", "type": "login", "label": "Sign in", "blocked": blocked }] })
                })
                .collect())
        }
        HarnessKind::Pi => {
            let v = match host {
                Some(h) => pi_helper_on(h, &["list"], Duration::from_secs(45))?,
                None => pi_helper(&["list"], Duration::from_secs(30))?,
            };
            Ok(v["providers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| !p["oauth"].is_null() || !p["apiKey"].is_null())
                .map(|p| {
                    let id = p["id"].as_str().unwrap_or_default();
                    let mut methods = Vec::new();
                    let mut note = "";
                    if let Some(o) = p["oauth"].as_object() {
                        let blocked = id == "anthropic" && o.get("subscription").and_then(Value::as_bool) == Some(true);
                        if blocked {
                            note = CLAUDE_SUBSCRIPTION;
                        }
                        methods.push(json!({ "id": "oauth", "type": "login", "label": o.get("label").cloned().unwrap_or(json!("Sign in")), "blocked": blocked }));
                    }
                    if let Some(a) = p["apiKey"].as_object() {
                        methods.push(json!({ "id": "api_key", "type": "login", "label": a.get("label").cloned().unwrap_or(json!("API key")) }));
                    }
                    json!({ "id": id, "name": p["name"], "connected": p["configured"], "note": note, "methods": methods })
                })
                .collect())
        }
        HarnessKind::OpenCode => {
            let server = opencode_server_on(host)?;
            let v = server.get("/api/integration", None).map_err(|e| format!("{e:#}"))?;
            Ok(v["data"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|i| {
                    let mut env = Vec::new();
                    let methods: Vec<Value> = i["methods"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|m| match m["type"].as_str()? {
                            "env" => {
                                env.extend(m["names"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(|n| format!("${n}"))));
                                None
                            }
                            t @ ("key" | "oauth" | "external" | "command") => Some(json!({
                                "id": m["id"].as_str().unwrap_or(t), "type": t,
                                "label": m["label"].as_str().unwrap_or(if t == "key" { "API key" } else { "Sign in" }),
                                "form": m["form"].as_array().cloned().unwrap_or_default(),
                            })),
                            _ => None,
                        })
                        .collect();
                    let note = if env.is_empty() { String::new() } else { format!("Or set {} before starting harlo.", env.join(" or ")) };
                    json!({ "id": i["id"], "name": i["name"], "connected": i["connections"].as_array().is_some_and(|c| !c.is_empty()), "note": note, "methods": methods })
                })
                .filter(|p| p["methods"].as_array().is_some_and(|m| !m.is_empty()))
                .collect())
        }
        // Hermes sets up its providers itself, in a terminal.
        // Its own setup (in harlo's terminal, where Hermes runs: it asks
        // there), then the providers it signs in to with a link and a code
        // or takes a key for.
        HarnessKind::Hermes => {
            let place = host.map_or("here".to_owned(), |h| format!("on {}", h.name));
            let signed_in = hermes_signed_in(host);
            let mut list = vec![json!({
                "id": "hermes", "name": "Hermes' own setup",
                "note": format!("Every provider Hermes knows, and its default model: `hermes model`, in harlo's terminal {place}. When it's done, pick Check again in the model list."),
                "methods": [{ "id": "model", "type": "terminal", "label": "Set up", "command": "hermes model" }],
            })];
            list.extend(HERMES_PROVIDERS.iter().map(|&(id, name, how)| {
                let methods = match how {
                    HermesWay::Code => json!([{ "id": "oauth", "type": "login", "label": "Sign in" }]),
                    HermesWay::Key => json!([{ "id": "key", "type": "key", "label": "API key" }]),
                    // (A Claude subscription isn't for other apps.)
                    HermesWay::KeyNotSubscription => json!([
                        { "id": "oauth", "type": "login", "label": "Claude account", "blocked": true },
                        { "id": "key", "type": "key", "label": "API key" },
                    ]),
                };
                let note = match how {
                    HermesWay::Code => "Sign in with a link and a code.",
                    HermesWay::Key => "",
                    HermesWay::KeyNotSubscription => CLAUDE_SUBSCRIPTION,
                };
                json!({ "id": id, "name": name, "connected": signed_in.iter().any(|p| p == id), "note": note, "methods": methods })
            }));
            Ok(list)
        }
        HarnessKind::Demo => Ok(Vec::new()),
    }
}

/// Signs an account out (blocking).
fn sign_out(k: HarnessKind, id: &str, host: Option<&Host>) -> Result<(), String> {
    match k {
        HarnessKind::Claude => match host {
            Some(h) => run_there(h, "claude", &["auth", "logout"], Duration::from_secs(30)).map(|_| ()),
            None => run(&program(k).ok_or("Claude Code isn't installed.")?, &["auth", "logout"], Duration::from_secs(20)).map(|_| ()),
        },
        HarnessKind::Pi => match host {
            Some(h) => pi_helper_on(h, &["logout", id], Duration::from_secs(60)).map(|_| ()),
            None => {
                let v = pi_helper(&["logout", id], Duration::from_secs(30))?;
                v["error"].as_str().map_or(Ok(()), |e| Err(e.to_owned()))
            }
        },
        HarnessKind::OpenCode => opencode_server_on(host)?.request("DELETE", &format!("/api/credential/{}", harlo_harness::opencode::url_encode(id)), None, None).map(|_| ()).map_err(|e| format!("{e:#}")),
        HarnessKind::Hermes => match host {
            Some(h) => run_there(h, "hermes", &["auth", "logout", id], Duration::from_secs(30)).map(|_| ()),
            None => run(&program(k).ok_or("Hermes isn't installed.")?, &["auth", "logout", id], Duration::from_secs(30)).map(|_| ()),
        },
        _ => Err("Sign out in omp itself (its /logout).".into()),
    }
}

/// Runs an agent's command on another machine (stdout), at most `limit`.
fn run_there(host: &Host, program: &str, args: &[&str], limit: Duration) -> Result<String, String> {
    let words: Vec<String> = std::iter::once(program).chain(args.iter().copied()).map(harlo_core::remote::quote).collect();
    let out = harlo_core::remote::run(host, &format!("{} </dev/null", words.join(" ")), limit).map_err(|e| format!("{e:#}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err.lines().map(str::trim).rfind(|l| !l.is_empty()).map_or_else(|| format!("{program} failed on {}.", host.name), str::to_owned))
}

/// Runs a command to its end (stdout), at most `limit`.
fn run(program: &Path, args: &[&str], limit: Duration) -> Result<String, String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    let deadline = std::time::Instant::now() + limit;
    while child.try_wait().ok().flatten().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return Err(format!("{} didn't answer in time.", program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() && text.trim().is_empty() {
        return Err(signin::plain(&String::from_utf8_lossy(&out.stderr)).trim().lines().last().unwrap_or("It failed.").to_owned());
    }
    Ok(text)
}

/// `omp login`'s list of providers: what it prints before asking for a
/// number (it's stopped there).
fn omp_list(mut login: std::process::Command) -> Result<Vec<(u32, String)>, String> {
    let mut child = login
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("omp: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("no output")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            text.push_str(&String::from_utf8_lossy(&buf[..n]));
            if signin::plain(&text).contains("Enter number") {
                break;
            }
        }
        let _ = tx.send(text);
    });
    let text = rx.recv_timeout(Duration::from_secs(20)).unwrap_or_default();
    let _ = child.kill();
    let _ = child.wait();
    let list = signin::omp_providers(&text);
    if list.is_empty() { Err("omp didn't list its providers.".into()) } else { Ok(list) }
}

// ---- pi ---------------------------------------------------------------------------

/// pi's package folder and the Node to run it with.
fn pi_runtime() -> Result<(PathBuf, PathBuf), String> {
    let pi = program(HarnessKind::Pi).ok_or("pi isn't installed.")?;
    let real = std::fs::canonicalize(&pi).unwrap_or(pi.clone());
    let names = ["@earendil-works/pi-coding-agent", "@mariozechner/pi-coding-agent"];
    // The installer's launcher (<agent>/bin/pi): its current release.
    let mut package = None;
    if let Some(agent) = real.parent().and_then(Path::parent)
        && let Ok(version) = std::fs::read_to_string(agent.join("install/current-version"))
    {
        let release = agent.join("install/releases").join(version.trim()).join("node_modules");
        package = names.iter().map(|n| release.join(n)).find(|p| p.join("package.json").is_file());
    }
    // An npm install: the package around the program.
    if package.is_none() {
        package = real.ancestors().find(|a| a.join("package.json").is_file() && a.join("dist/index.js").is_file()).map(Path::to_path_buf);
    }
    let package = package.ok_or("harlo couldn't find pi's files (its sign-in needs them).")?;
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let node = data
        .map(|d| d.join("pi-node/current/bin/node"))
        .filter(|n| n.is_file())
        .or_else(|| std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("node")).find(|n| n.is_file())))
        .ok_or("pi's sign-in needs Node.js, which harlo couldn't find.")?;
    Ok((package, node))
}

/// `pi_auth.mjs`, written where harlo keeps such things.
fn pi_helper_file() -> Result<PathBuf, String> {
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"))).ok_or("no cache folder")?;
    let file = cache.join("harlo/pi-auth.mjs");
    let text = include_str!("pi_auth.mjs");
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text) {
        std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&file, text).map_err(|e| e.to_string())?;
    }
    Ok(file)
}

/// An agent's command, here or on a host.
fn agent_command(k: HarnessKind, name: &str, host: Option<&Host>, args: &[&str]) -> Result<std::process::Command, String> {
    match host {
        Some(h) => {
            let words: Vec<String> = args.iter().map(|a| harlo_core::remote::quote(a)).collect();
            Ok(harlo_core::remote::command(h, &format!("{name} {}", words.join(" "))))
        }
        None => {
            let mut c = std::process::Command::new(program(k).ok_or_else(|| format!("{name} isn't installed."))?);
            c.args(args);
            Ok(c)
        }
    }
}

/// A sign-in that asks as it goes (its answers on its input): as
/// [`agent_command`], but on a host it ends when harlo's connection does
/// (cancelled, say). Without that it would wait there for a sign-in no one
/// can finish any more, a long while.
fn sign_in_command(k: HarnessKind, name: &str, host: Option<&Host>, args: &[&str]) -> Result<std::process::Command, String> {
    match host {
        Some(h) => {
            let words: Vec<String> = args.iter().map(|a| harlo_core::remote::quote(a)).collect();
            Ok(harlo_core::remote::command(h, &ends_with_input(&format!("{name} {}", words.join(" ")))))
        }
        None => agent_command(k, name, None, args),
    }
}

/// A shell script that runs `command` (a simple one) with what comes on
/// its input passed on, and stops it once its input ends.
fn ends_with_input(command: &str) -> String {
    // (A command run in the background reads nothing; its input goes
    // through a pipe of its own, fed from fd 3.)
    format!(
        r#"exec 3<&0; d=$(mktemp -d) || exit 1; f="$d/in"; mkfifo -m 600 "$f" || exit 1; {command} < "$f" 3<&- & c=$!; {{ while IFS= read -r l <&3; do printf '%s\n' "$l"; done; kill $c 2>/dev/null; }} > "$f" & r=$!; wait $c; s=$?; kill $r 2>/dev/null; rm -rf "$d"; exit $s"#
    )
}

/// `pi_auth.mjs` on a host, for [`pi_on_host`].
fn pi_helper_to_host(host: &Host) -> Result<(), String> {
    let write = r#"mkdir -p "$HOME/.cache/harlo" && umask 077 && cat > "$HOME/.cache/harlo/pi-auth.mjs""#;
    harlo_core::remote::run_ok_with_input(host, write, include_str!("pi_auth.mjs").as_bytes(), Duration::from_secs(20)).map_err(|e| format!("{e:#}"))
}

/// The script that runs the helper on a host, as [`pi_runtime`] finds pi's
/// files here. (What goes wrong is said as the helper would say it.)
fn pi_on_host(args: &[&str]) -> String {
    let words: Vec<String> = args.iter().map(|a| harlo_core::remote::quote(a)).collect();
    format!(
        r#"real=$(readlink -f "$(command -v pi)"); a=$(dirname "$(dirname "$real")"); pkg=
if [ -f "$a/install/current-version" ]; then v=$(cat "$a/install/current-version"); for n in @earendil-works/pi-coding-agent @mariozechner/pi-coding-agent; do [ -f "$a/install/releases/$v/node_modules/$n/package.json" ] && pkg="$a/install/releases/$v/node_modules/$n"; done; fi
if [ -z "$pkg" ]; then d=$(dirname "$real"); while [ "$d" != / ] && [ "$d" != . ]; do if [ -f "$d/package.json" ] && [ -f "$d/dist/index.js" ]; then pkg=$d; break; fi; d=$(dirname "$d"); done; fi
[ -n "$pkg" ] || {{ echo '{{"error":"harlo could not find the files of pi there."}}'; exit 0; }}
node="${{XDG_DATA_HOME:-$HOME/.local/share}}/pi-node/current/bin/node"; [ -x "$node" ] || node=$(command -v node) || {{ echo '{{"error":"The sign-in for pi needs Node.js there, which harlo could not find."}}'; exit 0; }}
exec "$node" "$HOME/.cache/harlo/pi-auth.mjs" "$pkg" {}"#,
        words.join(" ")
    )
}

/// [`pi_helper`] on a host.
fn pi_helper_on(host: &Host, args: &[&str], limit: Duration) -> Result<Value, String> {
    pi_helper_to_host(host)?;
    let out = harlo_core::remote::run_ok(host, &pi_on_host(args), limit).map_err(|e| format!("{e:#}"))?;
    let v: Value = out.lines().rev().find_map(|l| serde_json::from_str(l).ok()).ok_or("pi's sign-in helper said nothing.")?;
    match v["error"].as_str() {
        Some(e) => Err(e.to_owned()),
        None => Ok(v),
    }
}

/// The helper's one answer to `list` or `logout`.
fn pi_helper(args: &[&str], limit: Duration) -> Result<Value, String> {
    let (package, node) = pi_runtime()?;
    let helper = pi_helper_file()?;
    let mut all = vec![helper.to_string_lossy().into_owned(), package.to_string_lossy().into_owned()];
    all.extend(args.iter().map(|a| a.to_string()));
    let words: Vec<&str> = all.iter().map(String::as_str).collect();
    let out = run(&node, &words, limit)?;
    let v: Value = out.lines().rev().find_map(|l| serde_json::from_str(l).ok()).ok_or("pi's sign-in helper said nothing.")?;
    match v["error"].as_str() {
        Some(e) if args[0] == "list" => Err(e.to_owned()),
        _ => Ok(v),
    }
}

// ---- OpenCode -----------------------------------------------------------------

/// A private `opencode serve` for signing in, kept until Settings closes.
static OPENCODE: Mutex<Option<Arc<harlo_harness::opencode::Server>>> = Mutex::new(None);

/// The same on hosts, for signing in there.
static OPENCODE_HOSTS: Mutex<Option<HashMap<String, Arc<harlo_harness::opencode::Server>>>> = Mutex::new(None);

fn opencode_server_on(host: Option<&Host>) -> Result<Arc<harlo_harness::opencode::Server>, String> {
    let Some(host) = host else { return opencode_server() };
    let mut all = OPENCODE_HOSTS.lock().unwrap();
    let all = all.get_or_insert_with(HashMap::new);
    if let Some(server) = all.get(&host.name).filter(|s| s.alive()) {
        return Ok(server.clone());
    }
    let server = Arc::new(harlo_harness::opencode::Server::start_remote(&harlo_core::remote::launch(host)).map_err(|e| format!("{e:#}"))?);
    all.insert(host.name.clone(), server.clone());
    Ok(server)
}

fn opencode_server() -> Result<Arc<harlo_harness::opencode::Server>, String> {
    let mut s = OPENCODE.lock().unwrap();
    if let Some(server) = s.as_ref().filter(|s| s.alive()) {
        return Ok(server.clone());
    }
    let opencode = program(HarnessKind::OpenCode).ok_or("OpenCode isn't installed.")?;
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let server = Arc::new(harlo_harness::opencode::Server::start(&opencode, &home).map_err(|e| format!("{e:#}"))?);
    *s = Some(server.clone());
    Ok(server)
}

/// A device code in OpenCode's instructions ("Enter code: ABCD-1234").
fn device_code(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|w| {
            let parts: Vec<&str> = w.split('-').collect();
            parts.len() == 2 && parts[0].len() == 4 && (4..=5).contains(&parts[1].len()) && w.chars().all(|c| c == '-' || c.is_ascii_uppercase() || c.is_ascii_digit())
        })
        .map(str::to_owned)
}

// ---- Hermes ------------------------------------------------------------------------

/// How Hermes signs in to a provider.
#[derive(Clone, Copy)]
enum HermesWay {
    /// A link and a code (`hermes auth add <id> --type oauth`).
    Code,
    /// An API key.
    Key,
    /// An API key; its subscription sign-in is a Claude one.
    KeyNotSubscription,
}

/// The providers Hermes signs in to without its own setup (`hermes auth
/// add`), by its ids. (A sign-in that ends on `localhost` isn't here: on
/// another machine it wouldn't reach Hermes.)
const HERMES_PROVIDERS: &[(&str, &str, HermesWay)] = &[
    ("nous", "Nous Portal", HermesWay::Code),
    ("openai-codex", "OpenAI Codex (ChatGPT plan)", HermesWay::Code),
    ("xai-oauth", "xAI Grok (SuperGrok / Premium+)", HermesWay::Code),
    ("openrouter", "OpenRouter", HermesWay::Key),
    ("anthropic", "Anthropic", HermesWay::KeyNotSubscription),
    ("openai-api", "OpenAI API", HermesWay::Key),
    ("gemini", "Google AI Studio", HermesWay::Key),
    ("deepseek", "DeepSeek", HermesWay::Key),
    ("xai", "xAI API", HermesWay::Key),
    ("zai", "Z.AI / GLM", HermesWay::Key),
    ("kimi-coding", "Kimi / Moonshot", HermesWay::Key),
    ("minimax", "MiniMax", HermesWay::Key),
    ("alibaba", "Qwen Cloud", HermesWay::Key),
    ("huggingface", "Hugging Face", HermesWay::Key),
    ("nvidia", "NVIDIA NIM", HermesWay::Key),
    ("ai-gateway", "Vercel AI Gateway", HermesWay::Key),
    ("opencode-zen", "OpenCode Zen", HermesWay::Key),
    ("opencode-go", "OpenCode Go", HermesWay::Key),
    ("kilocode", "Kilo Code", HermesWay::Key),
    ("ollama-cloud", "Ollama Cloud", HermesWay::Key),
    ("arcee", "Arcee AI", HermesWay::Key),
    ("xiaomi", "Xiaomi MiMo", HermesWay::Key),
    ("stepfun", "StepFun Step Plan", HermesWay::Key),
];

/// The providers `hermes auth list` has credentials for (blocking; none if
/// it doesn't answer).
fn hermes_signed_in(host: Option<&Host>) -> Vec<String> {
    let out = match host {
        Some(h) => harlo_core::remote::run(h, "hermes auth list </dev/null", Duration::from_secs(30)).ok().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()),
        None => program(HarnessKind::Hermes).and_then(|p| run(&p, &["auth", "list"], Duration::from_secs(30)).ok()),
    };
    hermes_listed(&out.unwrap_or_default())
}

/// "deepseek (1 credentials):" → "deepseek".
fn hermes_listed(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| !l.starts_with(' '))
        .filter_map(|l| {
            let (id, rest) = l.trim_end().split_once(" (")?;
            (rest.ends_with("):") && rest.contains("credential") && !id.contains(' ')).then(|| id.to_owned())
        })
        .collect()
}

/// Adds an API key to Hermes (blocking). It's handed over on Hermes's
/// input, so it isn't in the list of processes, here or there.
fn hermes_add_key(host: Option<&Host>, provider: &str, key: &str) -> Result<(), String> {
    use std::io::Write;
    let mut cmd = agent_command(HarnessKind::Hermes, "hermes", host, &["auth", "add", provider, "--type", "api-key", "--label", "harlo"])?;
    // (Here, without the terminal harlo may have been started from: Hermes
    // would ask for the key there.)
    if host.is_none() {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("hermes: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(format!("{key}\n").as_bytes()).map_err(|e| e.to_string())?;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while child.try_wait().ok().flatten().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return Err("Hermes didn't answer in time.".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let said = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    Err(signin::plain(&said)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("Paste your API key") && !l.contains("may be echoed"))
        .last()
        .unwrap_or("Hermes didn't take the key.")
        .to_owned())
}

// ---- signing in ------------------------------------------------------------------

/// Signs in; what to say when it's done.
async fn sign_in(k: HarnessKind, provider: &str, method: &str, answers: &Value, report: &Report, rx: mpsc::UnboundedReceiver<String>) -> Result<String, String> {
    match k {
        HarnessKind::Claude => {
            let mut args = vec!["auth", "login"];
            if method == "console" {
                args.push("--console");
            }
            report.text("Claude Code opens Anthropic's sign-in page in your browser. Approve it there; Claude Code keeps the sign-in.");
            command(sign_in_command(k, "claude", report.host.as_ref(), &args)?, None, report, rx).await?;
            Ok("Claude Code is signed in.".into())
        }
        HarnessKind::Omp => {
            let number: u32 = provider.parse().map_err(|_| "Pick one of omp's providers.".to_owned())?;
            command(sign_in_command(k, "omp", report.host.as_ref(), &["login"])?, Some(number), report, rx).await?;
            Ok("omp is signed in.".into())
        }
        HarnessKind::Pi => pi_sign_in(provider, method, report, rx).await,
        HarnessKind::OpenCode => {
            let (provider, method, answers, report) = (provider.to_owned(), method.to_owned(), answers.clone(), report.clone());
            tokio::task::spawn_blocking(move || opencode_sign_in(&provider, &method, &answers, &report, rx)).await.map_err(|e| e.to_string())?
        }
        HarnessKind::Hermes => {
            let name = HERMES_PROVIDERS.iter().find(|p| p.0 == provider).map_or(provider, |p| p.1).to_owned();
            if answers["type"] == "key" {
                let key = answers["key"].as_str().unwrap_or_default().trim().to_owned();
                if key.is_empty() {
                    return Err("Paste the API key first.".into());
                }
                let (host, provider) = (report.host.clone(), provider.to_owned());
                tokio::task::spawn_blocking(move || hermes_add_key(host.as_ref(), &provider, &key)).await.map_err(|e| e.to_string())??;
                return Ok(format!("Hermes saved the key for {name}. Pick one of its models in the model list."));
            }
            report.text(&format!("Hermes signs in to {name}: open the link and enter the code there."));
            command(sign_in_command(k, "hermes", report.host.as_ref(), &["auth", "add", provider, "--type", "oauth", "--no-browser"])?, None, report, rx).await?;
            Ok(format!("Hermes is signed in to {name}. Pick one of its models in the model list."))
        }
        HarnessKind::Demo => Err("harlo can't sign this agent in.".into()),
    }
}

/// Runs an agent's sign-in command, showing what it prints (links, what it
/// asks) and handing it the answers; `choose`: the number to answer its
/// list with (omp). Ok when it ends well.
async fn command(program: std::process::Command, choose: Option<u32>, report: &Report, mut rx: mpsc::UnboundedReceiver<String>) -> Result<(), String> {
    let name = program.get_program().to_string_lossy().into_owned();
    let mut child = tokio::process::Command::from(program)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("{name}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no input")?;
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    for mut pipe in [Box::new(child.stdout.take().ok_or("no output")?) as Box<dyn tokio::io::AsyncRead + Unpin + Send>, Box::new(child.stderr.take().ok_or("no output")?)] {
        let tx = out_tx.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            while let Ok(n) = pipe.read(&mut buf).await {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
    }
    drop(out_tx);
    let mut pending = String::new();
    let mut last: Vec<String> = Vec::new();
    let mut chosen = false;
    let mut in_list = choose.is_some();
    let mut open = true;
    let status = loop {
        tokio::select! {
            chunk = out_rx.recv(), if open => match chunk {
                Some(bytes) => {
                    pending.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(at) = pending.find('\n') {
                        let line = signin::plain(&pending[..at]);
                        pending.drain(..=at);
                        let t = line.trim();
                        if t.is_empty() {
                            continue;
                        }
                        // omp's list: the dialog already showed it.
                        if in_list && (t.starts_with("Select a provider") || signin::omp_providers(t).len() == 1) {
                            continue;
                        }
                        last.push(t.to_owned());
                        match signin::url_in(t) {
                            Some(url) => report.link(&url, t.split("http").next().unwrap_or_default(), false),
                            // (The link to enter it on came just before.)
                            None => match device_code(t) {
                                Some(code) => report.device(&code, ""),
                                None => report.text(t),
                            },
                        }
                    }
                }
                None => open = false,
            },
            // A prompt (text left without a newline) once output pauses.
            _ = tokio::time::sleep(Duration::from_millis(300)), if !pending.trim().is_empty() => {
                let text = signin::plain(&pending);
                if let Some(mut ask) = signin::prompt(&text) {
                    pending.clear();
                    if let (Some(n), false) = (choose, chosen) && ask["message"].as_str().is_some_and(|m| m.starts_with("Enter number")) {
                        chosen = true;
                        in_list = false;
                        stdin.write_all(format!("{n}\n").as_bytes()).await.map_err(|e| e.to_string())?;
                        continue;
                    }
                    // Claude Code's code box: only if the page shows one.
                    ask["optional"] = json!(ask["message"].as_str().is_some_and(|m| m.contains("if prompted")));
                    report.ask(ask);
                }
            },
            answer = rx.recv() => match answer {
                Some(text) => {
                    report.text(if text.trim().is_empty() { "(nothing)" } else { "✓ Sent" });
                    stdin.write_all(format!("{}\n", text.trim()).as_bytes()).await.map_err(|e| e.to_string())?;
                    stdin.flush().await.map_err(|e| e.to_string())?;
                }
                // Cancelled: the command goes (killed when dropped).
                None => return Err("Cancelled.".into()),
            },
            status = child.wait(), if !open => break status,
        }
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(last.iter().rev().find(|l| l.to_lowercase().contains("fail") || l.to_lowercase().contains("error")).or(last.last()).cloned().unwrap_or_else(|| "It didn't finish.".into())),
    }
}

/// pi's own login through the helper.
async fn pi_sign_in(provider: &str, method: &str, report: &Report, mut rx: mpsc::UnboundedReceiver<String>) -> Result<String, String> {
    let mut run = match report.host.clone() {
        Some(host) => {
            let on = host.clone();
            tokio::task::spawn_blocking(move || pi_helper_to_host(&on)).await.map_err(|e| e.to_string())??;
            tokio::process::Command::from(harlo_core::remote::command(&host, &pi_on_host(&["login", provider, method])))
        }
        None => {
            let (package, node) = pi_runtime()?;
            let mut c = tokio::process::Command::new(&node);
            c.arg(pi_helper_file()?).arg(&package).args(["login", provider, method]);
            c
        }
    };
    let mut child = run
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("pi's sign-in: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no input")?;
    let mut lines = tokio::io::BufReader::new(child.stdout.take().ok_or("no output")?);
    let mut waiting: Option<u64> = None;
    loop {
        let mut line = String::new();
        tokio::select! {
            n = tokio::io::AsyncBufReadExt::read_line(&mut lines, &mut line) => {
                if n.unwrap_or(0) == 0 {
                    return Err("pi's sign-in stopped.".into());
                }
                let Ok(m) = serde_json::from_str::<Value>(&line) else { continue };
                if m["done"] == true {
                    return Ok("pi is signed in.".into());
                }
                if let Some(e) = m["error"].as_str() {
                    return Err(e.to_owned());
                }
                if m["dismiss"].as_u64().is_some() && m["dismiss"].as_u64() == waiting {
                    waiting = None;
                    report.ask(Value::Null);
                }
                let e = &m["event"];
                match e["type"].as_str() {
                    Some("auth_url") => {
                        if let Some(i) = e["instructions"].as_str() {
                            report.text(i);
                        }
                        report.link(e["url"].as_str().unwrap_or_default(), "Open the sign-in page", true);
                    }
                    Some("device_code") => report.device(e["userCode"].as_str().unwrap_or_default(), e["verificationUri"].as_str().unwrap_or_default()),
                    Some("info") => {
                        report.text(e["message"].as_str().unwrap_or_default());
                        for l in e["links"].as_array().into_iter().flatten() {
                            report.link(l["url"].as_str().unwrap_or_default(), l["label"].as_str().unwrap_or("Open"), false);
                        }
                    }
                    Some("progress") => report.text(e["message"].as_str().unwrap_or_default()),
                    _ => {}
                }
                let p = &m["prompt"];
                if let Some(id) = p["id"].as_u64() {
                    waiting = Some(id);
                    let kind = match p["type"].as_str() { Some("secret") => "secret", Some("manual_code") => "code", Some("select") => "select", _ => "text" };
                    report.ask(json!({ "kind": kind, "message": p["message"], "placeholder": p["placeholder"], "optional": kind == "code",
                                       "options": p["options"].as_array().into_iter().flatten().map(|o| json!({ "id": o["id"], "label": o["label"] })).collect::<Vec<_>>() }));
                }
            }
            answer = rx.recv() => match answer {
                Some(text) => {
                    if let Some(id) = waiting.take() {
                        stdin.write_all(format!("{}\n", json!({ "id": id, "value": text.trim() })).as_bytes()).await.map_err(|e| e.to_string())?;
                        stdin.flush().await.map_err(|e| e.to_string())?;
                    }
                }
                None => return Err("Cancelled.".into()),
            },
        }
    }
}

/// OpenCode's integrations: an API key (with its form's answers), a
/// browser sign-in (an attempt: opened, then waited for, or its code
/// asked), an external one, or a command it runs (blocking).
fn opencode_sign_in(provider: &str, method: &str, answers: &Value, report: &Report, mut rx: mpsc::UnboundedReceiver<String>) -> Result<String, String> {
    let server = opencode_server_on(report.host.as_ref())?;
    let id = harlo_harness::opencode::url_encode(provider);
    let answer = answers["answer"].as_object().filter(|a| !a.is_empty()).map(|a| Value::Object(a.clone())).unwrap_or(Value::Null);
    let call = |m: &str, path: &str, body: Option<&Value>| server.request(m, path, body, None).map_err(|e| format!("{e:#}"));
    let cancelled = |rx: &mut mpsc::UnboundedReceiver<String>| matches!(rx.try_recv(), Err(mpsc::error::TryRecvError::Disconnected));
    let kind = answers["type"].as_str().unwrap_or("oauth");
    match kind {
        "key" => {
            let key = answers["key"].as_str().unwrap_or_default().trim().to_owned();
            if key.is_empty() {
                return Err("Paste the API key first.".into());
            }
            call("POST", &format!("/api/integration/{id}/connect/key"), Some(&json!({ "key": key, "answer": answer })))?;
            Ok("OpenCode saved the key.".into())
        }
        "external" => {
            call("POST", &format!("/api/integration/{id}/connect/external"), Some(&json!({ "methodID": method, "answer": answer })))?;
            Ok("OpenCode is set up.".into())
        }
        "command" => {
            let a = call("POST", &format!("/api/integration/{id}/connect/command"), Some(&json!({ "methodID": method })))?;
            let attempt = a["data"]["attemptID"].as_str().unwrap_or_default().to_owned();
            report.text("OpenCode runs the provider's own sign-in; finish it where it opens.");
            loop {
                std::thread::sleep(Duration::from_millis(1500));
                if cancelled(&mut rx) {
                    let _ = call("DELETE", &format!("/api/integration/{id}/connect/command/{attempt}"), None);
                    return Err("Cancelled.".into());
                }
                let s = call("GET", &format!("/api/integration/{id}/connect/command/{attempt}"), None)?;
                match s["data"]["status"].as_str() {
                    Some("complete") => return Ok("OpenCode is signed in.".into()),
                    Some("pending") => {}
                    _ => return Err(s["data"]["message"].as_str().unwrap_or("It didn't finish.").to_owned()),
                }
            }
        }
        _ => {
            let a = call("POST", &format!("/api/integration/{id}/connect/oauth"), Some(&json!({ "methodID": method, "answer": answer })))?;
            let d = &a["data"];
            let attempt = d["attemptID"].as_str().unwrap_or_default().to_owned();
            let instructions = d["instructions"].as_str().unwrap_or_default();
            report.text(instructions);
            let url = d["url"].as_str().unwrap_or_default();
            match device_code(instructions) {
                Some(code) => report.device(&code, url),
                None => report.link(url, "Open the sign-in page", true),
            }
            if d["mode"] == "code" {
                report.ask(json!({ "kind": "code", "message": "Paste the code the page shows", "optional": false }));
                let Some(code) = rx.blocking_recv() else {
                    let _ = call("DELETE", &format!("/api/integration/{id}/connect/oauth/{attempt}"), None);
                    return Err("Cancelled.".into());
                };
                call("POST", &format!("/api/integration/{id}/connect/oauth/{attempt}/complete"), Some(&json!({ "code": code.trim() })))?;
                return Ok("OpenCode is signed in.".into());
            }
            loop {
                std::thread::sleep(Duration::from_millis(1500));
                if cancelled(&mut rx) {
                    let _ = call("DELETE", &format!("/api/integration/{id}/connect/oauth/{attempt}"), None);
                    return Err("Cancelled.".into());
                }
                let s = call("GET", &format!("/api/integration/{id}/connect/oauth/{attempt}"), None)?;
                match s["data"]["status"].as_str() {
                    Some("complete") => return Ok("OpenCode is signed in.".into()),
                    Some("pending") => {}
                    Some("expired") => return Err("The sign-in expired. Try again.".into()),
                    _ => return Err(s["data"]["message"].as_str().unwrap_or("The sign-in failed.").to_owned()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use harlo_harness::HarnessKind;

    #[test]
    fn says_why_a_server_did_not_start() {
        let said = super::start_failure("12:05:13 FATAL server failed during startup | Device budget cannot hold model weights and required execution resources");
        assert!(said.starts_with("The model and this context don't fit"), "{said}");
        assert_eq!(super::start_failure("error: no such file"), "error: no such file");
    }

    /// The server a session's model is on, among an agent's servers.
    #[test]
    fn finds_a_models_server() {
        use harlo_core::model_servers::Server;
        let list = vec![
            Server { name: "llamacpp".into(), base_url: "http://127.0.0.1:8090/v1".into(), models: vec!["SmolLM2".into()] },
            Server { name: "ollama".into(), base_url: "http://127.0.0.1:11434/v1".into(), models: vec!["qwen3:4b".into()] },
        ];
        let name = |k, m| super::server_among(k, m, &list).map(|(p, s)| (p, s.map(|s| s.base_url)));
        assert_eq!(name(HarnessKind::Pi, "llamacpp/SmolLM2"), Some(("llamacpp".into(), Some("http://127.0.0.1:8090/v1".into()))));
        assert_eq!(name(HarnessKind::Pi, "anthropic/claude"), Some(("anthropic".into(), None)));
        assert_eq!(name(HarnessKind::Pi, "no-provider"), None);
        assert_eq!(name(HarnessKind::Hermes, "custom:ollama:qwen3:4b").unwrap().0, "ollama");
        assert_eq!(name(HarnessKind::Hermes, "custom:SmolLM2").unwrap().0, "llamacpp");
        assert_eq!(super::omp_usual("ollama").unwrap()[0], "http://127.0.0.1:11434/v1");
        assert!(super::omp_usual("anthropic").is_none());
    }

    /// Which server a Hermes model is on, from this computer's configs.
    #[test]
    #[ignore = "this computer's configs"]
    fn finds_hermes_servers() {
        for id in ["custom/llamacpp:Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL", "custom:llamacpp:Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL", "custom:Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL", "custom/Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL", "llamacpp/Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL"] {
            println!("{id} → {:?}", super::managed_server("hermes", id));
        }
    }

    /// What's left running on this computer (looks only).
    #[test]
    #[ignore = "this computer's processes"]
    fn finds_leftover_servers() {
        println!("8080: {:?}, 1: {:?}", super::leftovers("http://127.0.0.1:8080/v1"), super::leftovers("http://127.0.0.1:1/v1"));
    }

    #[test]
    fn reads_the_model_path_of_a_start_command() {
        assert_eq!(super::shell_word("'/a/my model'\\''s/x.gguf' --alias q"), "/a/my model's/x.gguf");
        assert_eq!(super::shell_word(" /plain/x.gguf --port 1"), "/plain/x.gguf");
    }

    #[test]
    fn where_pi_found_a_key() {
        assert_eq!(super::pi_source("models_json_command"), "Command in pi's models.json");
        assert_eq!(super::pi_source("ZAI_API_KEY"), "From $ZAI_API_KEY");
        assert_eq!(super::pi_source(""), "Set up outside pi");
    }

    #[test]
    fn a_sign_in_there_ends_with_its_connection() {
        use std::io::{Read, Write};
        use std::time::Duration;
        let run = |command: &str| {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(super::ends_with_input(command))
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        };
        // Answered: what it's given gets there, and it ends by itself
        // (its input still open).
        let mut c = run("sh -c 'read x; echo got:$x'");
        let mut input = c.stdin.take().unwrap();
        input.write_all(b"hi\n").unwrap();
        let t = std::time::Instant::now();
        while c.try_wait().unwrap().is_none() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut out = String::new();
        c.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        assert_eq!(out, "got:hi\n");
        assert!(c.wait().unwrap().success());
        drop(input);
        // Its input gone (the connection closed): it's stopped.
        let mut c = run("sleep 30");
        drop(c.stdin.take());
        let t = std::time::Instant::now();
        while c.try_wait().unwrap().is_none() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(t.elapsed() < Duration::from_secs(4), "still running after {:?}", t.elapsed());
        let _ = c.kill();
    }

    #[test]
    fn hermes_says_what_it_is_signed_in_to() {
        let list = "openai-codex (1 credentials):\n  #1  device_code          oauth   id=53fd36 priority=0 device_code ←\n\ndeepseek (2 credentials):\n  #1  harlo  api_key id=944702 priority=0 manual ←\nSome other line (with brackets):\n";
        assert_eq!(super::hermes_listed(list), ["openai-codex", "deepseek"]);
        assert!(super::hermes_listed("").is_empty());
    }

    #[test]
    fn device_codes_in_instructions() {
        assert_eq!(super::device_code("Enter code: ABCD-1234 at github.com/login/device").as_deref(), Some("ABCD-1234"));
        assert_eq!(super::device_code("Waiting for browser authorization (code: ABCD-12345)…").as_deref(), Some("ABCD-12345"));
        assert_eq!(super::device_code("Complete authorization in your browser."), None);
        assert_eq!(super::device_code("use-this lower-case"), None);
        assert_eq!(super::device_code("    WLBI-LMLXF").as_deref(), Some("WLBI-LMLXF"));
    }
}
