//! `Accounts` (QML singleton): signing the agents in to their model
//! providers from harlo (Settings → Agents → Accounts, the sign-in
//! dialog), with each agent's own sign-in:
//!
//! - The Claude account pi's Claude subscription uses ("claude"): Claude
//!   Code's `claude auth login` (Anthropic's own flow in the browser; Claude
//!   Code keeps the token), `auth status`, `auth logout`.
//! - pi: `pi auth login` in harlo's terminal.
//!
//! The agents keep their credentials; harlo shows the links and hands what
//! the user types to the agent, as a terminal would, and never reads them.

// Rust guideline compliant 2026-10-09

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
        /// type: "login", label, blocked }] }], error }`.
        #[qinvokable]
        fn providers(self: &Accounts, agent: &QString) -> QString;
        /// Starts signing in (`flow` follows).
        #[qinvokable]
        fn start(self: Pin<&mut Accounts>, agent: &QString, provider: &QString, method: &QString);
        /// Answers what the sign-in asks (`flow.ask`).
        #[qinvokable]
        fn answer(self: Pin<&mut Accounts>, text: &QString);
        /// Stops the sign-in going on.
        #[qinvokable]
        fn cancel(self: Pin<&mut Accounts>);
        /// Signs an account out. An error, or "" (`status` follows).
        #[qinvokable]
        fn sign_out(self: Pin<&mut Accounts>, agent: &QString, id: &QString) -> QString;
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
        /// names it has already. An error, or "" (done).
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
        /// models as that agent has them): "" or why not.
        #[qinvokable]
        fn copy_server(self: &Accounts, from: &QString, to: &QString, name: &QString, taken: &QString) -> QString;
        /// Sets what a server's models cost in an agent, dollars per
        /// million tokens in and out (both 0 or less: no price): "" or why
        /// not.
        #[qinvokable]
        fn set_server_price(self: &Accounts, agent: &QString, name: &QString, input: f64, output: f64) -> QString;
        /// Says whether a server charges nothing (its own hardware's): ""
        /// or why not. Unpriced and not free, its cost is unknown.
        #[qinvokable]
        fn set_server_free(self: &Accounts, agent: &QString, name: &QString, free: bool) -> QString;
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
        /// Reads pi's cloud providers and whether each has a key, here or on
        /// a machine (`agent`: "pi" or "pi@host"; `cloud` follows).
        #[qinvokable]
        fn load_cloud(self: Pin<&mut Accounts>, agent: &QString);
        /// `{ state: "loading" | "ready", list: [{ id, name, key, … }],
        /// error, results: { <provider>: { state: "busy" | "ok" | "error",
        /// message } } }`.
        #[qinvokable]
        fn cloud(self: &Accounts, agent: &QString) -> QString;
        /// Checks a provider's saved key with the provider.
        #[qinvokable]
        fn test_key(self: Pin<&mut Accounts>, agent: &QString, provider: &QString);
        /// Forgets a provider's saved key.
        #[qinvokable]
        fn remove_key(self: Pin<&mut Accounts>, agent: &QString, provider: &QString);
        /// The terminal command that saves a key for pi's `provider`
        /// (`agent`: "pi" or "pi@host"): typed into pi, never seen by
        /// harlo. "" when it isn't one of pi's.
        #[qinvokable]
        fn pi_key_command(self: &Accounts, agent: &QString, provider: &QString) -> QString;
    }
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::remote::Host;
use harlo_core::servers::{saved_models, commands_of, engine_of, ollama_models_of, model_files, remove, set_up_servers, STARTED, log_path, launch, wait_up, leftovers, stop_group, stop_pids};
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
    /// `servers` per agent, as last read (`load_servers`).
    servers: HashMap<String, Value>,
    /// `cloud` per agent.
    cloud: HashMap<String, Value>,
}

/// An agent's program: the path set in Settings → Agents (if it's
/// there), else the installed one.
pub(crate) fn program(kind: HarnessKind) -> Option<PathBuf> {
    let set = harlo_core::db::Db::open(&harlo_core::db::default_path()).ok().and_then(|d| d.get(&format!("settings.{}_path", kind.as_str())).ok().flatten()).unwrap_or_default();
    if set.trim().is_empty() { harlo_harness::resolve_program(kind) } else { Some(PathBuf::from(set.trim())).filter(|p| p.is_file()) }
}

/// A server's address, key and models: as agent `from` has it, else (no
/// agent has it: `from` is its address) what harlo saved when it set it
/// up, else what it serves now.
fn server_details(from: &str, name: &str) -> anyhow::Result<(String, String, Vec<model_servers::Found>)> {
    if let Some(from) = kind(from) {
        return model_servers::details(from, name).ok_or_else(|| anyhow::anyhow!("{name} isn't in {}'s file any more.", from.label()));
    }
    let url = from.to_owned();
    let mut models = saved_models(&commands_of(&url));
    if models.is_empty() {
        models = model_servers::find(&url, "").map_err(|e| anyhow::anyhow!("Start it first: harlo asks it for its models. ({e})"))?.models;
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

fn read_host_servers(host: &str) -> anyhow::Result<Value> {
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

/// Whose sign-ins: an agent's, or the Claude account pi's Claude
/// subscription signs in with (`"claude"`, this computer's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    Agent(HarnessKind),
    Claude,
}

/// [`target`], or the Claude account.
fn who(agent: &str) -> Option<(Who, Option<Host>)> {
    if agent == "claude" {
        return Some((Who::Claude, None));
    }
    target(agent).map(|(k, host)| (Who::Agent(k), host))
}

/// What to sign in: `harlo`, or `harlo@arcticnas` for Harlo on that host (which keeps
/// its own logins).
fn target(agent: &str) -> Option<(HarnessKind, Option<Host>)> {
    match agent.split_once('@') {
        Some((a, h)) => Some((kind(a)?, Some(harlo_core::remote::host(h)?))),
        None => Some((kind(agent)?, None)),
    }
}

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
                tracing::event!(name: "account.port_forward.failed", tracing::Level::WARN, network.local.port = port, server.address = %host.name, error.message = %format_args!("{e:#}"), "forwarding port {{network.local.port}} to {{server.address}}: {{error.message}}");
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
        let Some((k, host)) = who(&agent) else { return };
        let was = self.rust().status.get(&agent).cloned().unwrap_or_else(|| json!({ "accounts": [] }));
        self.as_mut().rust_mut().status.insert(agent.clone(), json!({ "state": "loading", "accounts": was["accounts"] }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = match accounts_of(k, host.as_ref()) {
                Ok(list) => json!({ "state": "ready", "accounts": list, "error": "" }),
                Err(e) => json!({ "state": "ready", "accounts": [], "error": format!("{e:#}") }),
            };
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().status.insert(agent, v);
                this.bump();
            });
        });
    }

    pub fn load_providers(mut self: Pin<&mut Self>, agent: &QString) {
        let agent = agent.to_string();
        let Some((k, host)) = who(&agent) else { return };
        self.as_mut().rust_mut().providers.insert(agent.clone(), json!({ "state": "loading", "list": [] }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = json!({ "state": "ready", "list": providers_of(k, host.as_ref()), "error": "" });
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().providers.insert(agent, v);
                this.bump();
            });
        });
    }

    pub fn start(mut self: Pin<&mut Self>, agent: &QString, provider: &QString, method: &QString) {
        self.as_mut().cancel();
        let (agent, provider, method) = (agent.to_string(), provider.to_string(), method.to_string());
        let id = self.next_id + 1;
        self.as_mut().rust_mut().next_id = id;
        let (tx, rx) = mpsc::unbounded_channel();
        self.as_mut().rust_mut().current = Some((id, tx));
        // Its name, as the providers list has it.
        let title = self.rust().providers.get(&agent).and_then(|p| p["list"].as_array().cloned()).into_iter().flatten()
            .find(|p| p["id"] == provider.as_str()).and_then(|p| p["name"].as_str().map(str::to_owned)).unwrap_or_else(|| provider.clone());
        let flow = Flow { id, agent: agent.clone(), provider: provider.clone(), title, state: "running".into(), ask: Value::Null, ..Default::default() };
        self.as_mut().set_flow(QString::from(&flow.json()));
        let Some((k, host)) = who(&agent) else {
            let report = Report { flow: Arc::new(Mutex::new(flow)), thread: self.qt_thread(), host: None, forwards: Arc::default() };
            report.finish(false, "harlo can't sign this agent in.");
            return;
        };
        let report = Report { flow: Arc::new(Mutex::new(flow)), thread: self.qt_thread(), host, forwards: Arc::default() };
        crate::runtime().spawn(async move {
            let outcome = sign_in(k, &provider, &method, &report, rx).await;
            match outcome {
                Ok(said) => report.finish(true, &said),
                Err(e) => report.finish(false, &format!("{e:#}")),
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
        let Some((k, host)) = who(&agent_s) else { return QString::from("harlo can't sign this agent out.") };
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
                    v["error"] = json!(format!("{e:#}"));
                    this.as_mut().rust_mut().status.insert(agent_s, v);
                    this.bump();
                }
            });
        });
        QString::default()
    }

}

impl Accounts {
    pub fn cloud(&self, agent: &QString) -> QString {
        let v = self.rust().cloud.get(&agent.to_string()).cloned().unwrap_or_else(|| json!({ "state": "loading", "list": [], "results": {} }));
        QString::from(&v.to_string())
    }

    pub fn load_cloud(mut self: Pin<&mut Self>, agent: &QString) {
        let agent = agent.to_string();
        // pi's: the providers it has a key for, from its model list.
        let Some(host) = pi_host(&agent) else { return };
        {
            let mut was = self.rust().cloud.get(&agent).cloned().unwrap_or_else(|| json!({ "list": [], "results": {} }));
            was["state"] = json!("loading");
            self.as_mut().rust_mut().cloud.insert(agent.clone(), was);
            self.as_mut().bump();
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let listed = crate::pi_extensions::run_on(&host, harlo_core::pi_keys::LIST_SCRIPT, None).map(|o| harlo_core::pi_keys::list_json(&String::from_utf8_lossy(&o.stdout)));
                let _ = thread.queue(move |mut this| {
                    {
                        let mut rust = this.as_mut().rust_mut();
                        let entry = rust.cloud.entry(agent).or_insert_with(|| json!({ "results": {} }));
                        entry["state"] = json!("ready");
                        match listed {
                            Ok(list) => (entry["list"], entry["error"]) = (list, json!("")),
                            Err(e) => entry["error"] = json!(e),
                        }
                    }
                    this.bump();
                });
            });
        }
    }

    pub fn test_key(self: Pin<&mut Self>, agent: &QString, provider: &QString) {
        self.pi_key_job(agent, provider, "Checking…", |host, id| {
            harlo_core::pi_keys::check_said(&run_pi(host, &harlo_core::pi_keys::check_args(id))?).map(|_| "It works.".to_owned())
        });
    }

    pub fn remove_key(self: Pin<&mut Self>, agent: &QString, provider: &QString) {
        self.pi_key_job(agent, provider, "Removing…", |host, id| {
            run_pi(host, &harlo_core::pi_keys::logout_args(id))?;
            Ok("Removed: pi has no key of its own for it now (one in its environment or models.json stays).".to_owned())
        });
    }

    /// The command that saves a key for pi's `provider`, typed into pi in
    /// harlo's terminal ("" when it isn't one of pi's).
    #[expect(clippy::unused_self, reason = "QML calls it as a method of its object")]
    pub fn pi_key_command(&self, agent: &QString, provider: &QString) -> QString {
        let (agent, provider) = (agent.to_string(), provider.to_string());
        let Some(host) = pi_host(&agent) else { return QString::default() };
        if !harlo_core::pi_keys::known(&provider) {
            return QString::default();
        }
        let host = if host.is_empty() { None } else { harlo_core::remote::host(&host) };
        QString::from(&pi_terminal_command(&harlo_core::pi_keys::login_args(&provider), host.as_ref()))
    }

    /// A pi command about a provider's key in the background: `busy`
    /// meanwhile, then what came of it, pi's idle sessions started again
    /// (they read keys as they start) and the list read again.
    fn pi_key_job(mut self: Pin<&mut Self>, agent: &QString, provider: &QString, busy: &str, job: impl FnOnce(&str, &str) -> anyhow::Result<String> + Send + 'static) {
        let (agent, provider) = (agent.to_string(), provider.to_string());
        let Some(host) = pi_host(&agent).filter(|_| harlo_core::pi_keys::known(&provider)) else { return };
        key_result(&mut self.as_mut().rust_mut().cloud, &agent, &provider, json!({ "state": "busy", "message": busy }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let outcome = job(&host, &provider);
            let _ = thread.queue(move |mut this| {
                let result = match outcome {
                    Ok(said) => json!({ "state": "ok", "message": said }),
                    Err(e) => json!({ "state": "error", "message": format!("{e:#}") }),
                };
                key_result(&mut this.as_mut().rust_mut().cloud, &agent, &provider, result);
                this.as_mut().bump();
                if host.is_empty() {
                    crate::registry::restart_idle("pi");
                }
                this.load_cloud(&QString::from(&agent));
            });
        });
    }

    pub fn servers(&self, agent: &QString) -> QString {
        let list = self.rust().servers.get(&agent.to_string()).cloned().unwrap_or_else(|| json!([]));
        QString::from(&list.to_string())
    }

    pub fn load_servers(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // (Servers from before harlo said which are free: said now.)
            if let Err(e) = model_servers::mark_free(HarnessKind::Pi) {
                tracing::event!(name: "model_server.list.failed", tracing::Level::WARN, error.message = %e, "model servers: {{error.message}}");
            }
            let mut all: Vec<(String, Value)> = model_servers::AGENTS.iter().map(|k| (k.as_str().to_owned(), servers_json(*k))).collect();
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
                let contexts: Vec<Value> = model_servers::details(k, &s.name).map(|(_, _, m)| m.iter().map(|m| json!(m.context)).collect()).unwrap_or_default();
                // What each model costs ([in, out] dollars per million
                // tokens; null: no price).
                let prices: Vec<Value> = model_servers::prices(k, &s.name).into_iter().map(|p| p.map_or(Value::Null, |p| json!([p[0], p[1]]))).collect();
                // Another machine's, reached here through SSH: that machine.
                let via = model_servers::through(&s.base_url).map(|(h, _)| h).unwrap_or_default();
                // Said to charge nothing.
                let free = model_servers::free(k, &s.name) == Some(true);
                json!({
                    "name": s.name, "baseUrl": s.base_url, "models": s.models, "contexts": contexts, "prices": prices, "place": place, "managed": managed, "free": free,
                    "engine": if via.is_empty() { engine_of(&s.base_url).to_owned() } else { String::new() }, "via": via,
                })
            })
            .collect(),
    )
}

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
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
                Err(e) => json!({ "id": id, "state": "failed", "models": [], "error": format!("{e:#}") }),
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
            Ok(()) => {
                reread(k);
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }
}

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
impl Accounts {
    pub fn copy_server(&self, from: &QString, to: &QString, name: &QString, taken: &QString) -> QString {
        let Some(to) = kind(&to.to_string()) else { return QString::from("harlo can't add model servers to this agent.") };
        let name = name.to_string();
        let (url, key, models) = match server_details(&from.to_string(), &name) {
            Ok(d) => d,
            Err(e) => return QString::from(&format!("{e:#}")),
        };
        let taken: Vec<String> = serde_json::from_str(&taken.to_string()).unwrap_or_default();
        match model_servers::add(to, &name, &url, &key, &models, &taken) {
            Ok(()) => {
                reread(to);
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn set_server_free(&self, agent: &QString, name: &QString, free: bool) -> QString {
        let Some(k) = kind(&agent.to_string()) else { return QString::from("harlo can't change this agent's model servers.") };
        match model_servers::set_free(k, &name.to_string(), free) {
            Ok(()) => QString::default(),
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn set_server_price(&self, agent: &QString, name: &QString, input: f64, output: f64) -> QString {
        let Some(k) = kind(&agent.to_string()) else { return QString::from("harlo can't change this agent's model servers.") };
        let ok = |r: f64| r.is_finite() && r > 0.0;
        let price = (ok(input) || ok(output)).then_some((input.max(0.0), output.max(0.0)));
        // (Read for each request: sessions running take it at once.)
        match model_servers::set_price(k, &name.to_string(), price) {
            Ok(()) => QString::default(),
            Err(e) => QString::from(&format!("{e:#}")),
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
                        e["error"] = json!(format!("{why:#}"));
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
    fn change_server_on(mut self: Pin<&mut Self>, host: &QString, agent: &QString, change: impl FnOnce(&str, HarnessKind) -> anyhow::Result<()> + Send + 'static) {
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
                    e["note"] = json!(outcome.err().map(|e| format!("{e:#}")).unwrap_or_default());
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
            m.insert(host.clone(), Instant::now())
        };
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
                    e["note"] = json!(outcome.err().map(|e| format!("{e:#}")).unwrap_or_default());
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
            let v = harlo_core::engine_host::json(host, &["start", url], Duration::from_mins(20)).map_err(|e| anyhow::anyhow!("{e:#}"))?;
            match v["error"].as_str() {
                Some(e) => Err(anyhow::Error::msg(e.to_owned())),
                None => Ok(()),
            }
        });
    }

    pub fn stop_host_engine(self: Pin<&mut Self>, host: &QString, url: &QString) {
        self.host_engine_op(host, url, "stopping", |host, url| harlo_core::engine_host::run(host, &["stop", url], Duration::from_secs(120)).map(|_| ()).map_err(|e| anyhow::anyhow!("{e:#}")));
    }

    pub fn remove_host_engine(self: Pin<&mut Self>, host: &QString, url: &QString, name: &QString, delete_files: bool) {
        let name = name.to_string();
        self.host_engine_op(host, url, "removing", move |host, url| {
            // (Out of the agents there that have it first.)
            let mut errors = Vec::new();
            for kind in model_servers::ON_HOST {
                if servers_on_host_have(host, kind, &name, url)
                    && let Err(e) = model_servers::remove_on(host, kind, &name) {
                        errors.push(format!("{}: {e}", kind.label()));
                    }
            }
            // (And out of the agents here that reach it through SSH.)
            if let Some(here) = model_servers::used_here(host, url) {
                for kind in model_servers::AGENTS {
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
                return Err(anyhow::Error::msg(errors.join("\n")));
            }
            let v = harlo_core::engine_host::json(host, &["remove", url, &name, if delete_files { "1" } else { "0" }], Duration::from_secs(600)).map_err(|e| anyhow::anyhow!("{e:#}"))?;
            let errors: Vec<String> = v["errors"].as_array().into_iter().flatten().filter_map(|e| e.as_str().map(str::to_owned)).collect();
            if errors.is_empty() { Ok(()) } else { Err(anyhow::Error::msg(errors.join("\n"))) }
        });
    }

    /// Something done to one of a machine's servers, there, in the
    /// background; its list read again after.
    fn host_engine_op(mut self: Pin<&mut Self>, host: &QString, url: &QString, doing: &str, op: impl FnOnce(&str, &str) -> anyhow::Result<()> + Send + 'static) {
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
                    e["note"] = json!(outcome.err().map(|e| format!("{e:#}")).unwrap_or_default());
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
            Ok(()) => {
                reread(k);
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        };
        // (Another machine's, used here: its forward goes when nothing here has it.)
        model_servers::forget_unused_uses();
        self.as_mut().load_servers();
        out
    }

    pub fn remove_server(mut self: Pin<&mut Self>, name: &QString, url: &QString, delete_files: bool) {
        let (name, url) = (name.to_string(), url.to_string());
        let first = json!({ "url": url, "state": "running", "step": "Removing it", "done": 0, "total": 0, "error": "" });
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
                });
            };
            let (errors, summary) = remove(&name, &url, delete_files, &step, &reread);
            say(&|v| {
                v["state"] = json!("done");
                v["error"] = json!(errors.join("\n"));
                v["summary"] = json!(summary);
            });
            let _ = thread.queue(|mut this| this.as_mut().load_servers());
        });
    }
}

/// After harlo changed an agent's model servers: its idle sessions start
/// again when next used (they read their models only when they start).
fn reread(k: HarnessKind) {
    if k == HarnessKind::Pi {
        crate::registry::restart_idle(k.as_str());
    }
}

// ---- starting and stopping model servers --------------------------------------

/// Servers harlo set up that no agent has (closed before adding them, or
/// unticked everywhere): listed like the others, with no agent ticked.
fn unused_json() -> Value {
    let used: Vec<String> = model_servers::AGENTS.iter().flat_map(|k| model_servers::servers(*k)).map(|s| s.base_url).collect();
    Value::Array(
        set_up_servers()
            .into_iter()
            .filter(|(url, _)| !used.contains(url))
            .map(|(url, v)| {
                let models: Vec<model_servers::Found> = saved_models(&v);
                // (One set up before harlo saved names: its engine's.)
                let name = v["name"].as_str().filter(|n| !n.is_empty()).map_or_else(|| {
                    match engine_of(&url) {
                        "" | "Your command" => model_servers::suggest_name(&url),
                        engine => engine.to_lowercase(),
                    }
                }, str::to_owned);
                json!({
                    "name": name, "baseUrl": url, "models": models.iter().map(|m| m.id.clone()).collect::<Vec<_>>(), "contexts": models.iter().map(|m| json!(m.context)).collect::<Vec<_>>(),
                    "place": "here", "managed": true, "engine": engine_of(&url),
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
        for k in model_servers::AGENTS {
            for s in model_servers::servers(k).into_iter().filter(|s| s.base_url == url) {
                if let Err(e) = model_servers::offer_levels(k, &s.name) {
                    tracing::event!(name: "model_server.thinking.failed", tracing::Level::WARN, server.name = %s.name, error.message = %e, "thinking levels for {{server.name}}: {{error.message}}");
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
    pub fn start(&self) -> anyhow::Result<()> {
        match self {
            ModelServer::Here { url, .. } => harlo_core::servers::ensure_running(url),
            ModelServer::There { host, url, .. } => {
                let v = harlo_core::engine_host::json(host, &["start", url], Duration::from_mins(20)).map_err(|e| anyhow::anyhow!("{e:#}"))?;
                v["error"].as_str().map_or(Ok(()), |e| Err(anyhow::Error::msg(e.to_owned())))
            }
            ModelServer::Through { url, .. } => model_servers::reach(url),
        }
    }
}

/// The server a model (`provider/id`) is on, among an agent's `list`, and
/// the provider's name for it.
fn server_among(model: &str, list: &[model_servers::Server]) -> Option<(String, Option<model_servers::Server>)> {
    let provider = model.split_once('/')?.0.to_owned();
    let found = list.iter().find(|s| s.name == provider).cloned();
    Some((provider, found))
}

/// The server behind a session's model on this computer when harlo can
/// start it: one it set up here, or another machine's reached through SSH.
pub(crate) fn managed_server(agent: &str, model: &str) -> Option<ModelServer> {
    let k = kind(agent)?;
    let managed = |url: &str| commands_of(url)["start"].as_str().is_some_and(|c| !c.is_empty());
    let (_, found) = server_among(model, &model_servers::servers(k))?;
    let s = found?;
    if managed(&s.base_url) {
        return Some(ModelServer::Here { name: s.name, url: s.base_url });
    }
    model_servers::through(&s.base_url).is_some().then_some(ModelServer::Through { name: s.name, url: s.base_url })
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
    let (_, found) = server_among(model, &list)?;
    let (name, url) = found.map(|s| (s.name, s.base_url))?;
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
/// What couldn't be updated.
pub(crate) fn sync_context(url: &str) -> Vec<String> {
    let start = commands_of(url)["start"].as_str().unwrap_or_default().to_owned();
    let llama = start.contains("llama-server");
    if !llama && !start.contains("ollama") {
        return Vec::new();
    }
    let served = if llama { harlo_core::engines::served_context(url) } else { None };
    let mut problems = Vec::new();
    for k in model_servers::AGENTS {
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

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
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
            return self.set_state(&url, json!({ "state": "failed", "harlo": false, "message": format!("{e:#}") }));
        }
        self.as_mut().set_state(&url, json!({ "state": "starting", "harlo": true, "message": "" }));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let state = match wait_up(&url, Duration::from_mins(15), &mut |_| {}, &|| false) {
                // (The context it serves this time, into the agents.)
                Ok(held) => json!({ "state": "running", "harlo": held, "message": sync_context(&url).join("\n") }),
                Err(e) => json!({ "state": "failed", "harlo": false, "message": format!("{e:#}") }),
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
                None => std::process::Command::new("sh").arg("-c").arg(&stop).stdin(std::process::Stdio::null()).output().map_err(anyhow::Error::from).and_then(|o| {
                    if o.status.success() { Ok(()) } else { Err(anyhow::Error::msg(String::from_utf8_lossy(&o.stderr).trim().lines().last().unwrap_or("It failed.").to_owned())) }
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

// ---- what each agent is signed in to ------------------------------------------

/// `[{ id, label, detail, signOut }]` (blocking).
fn accounts_of(who: Who, host: Option<&Host>) -> anyhow::Result<Vec<Value>> {
    let k = match who {
        Who::Claude => return claude_accounts(),
        Who::Agent(k) => k,
    };
    match k {
        // pi's saved sign-ins: the providers in its auth file (only their
        // names and kinds are read). A host's: not listed yet.
        HarnessKind::Pi if host.is_some() => Ok(Vec::new()),
        HarnessKind::Pi => Ok(pi_accounts(&pi_auth_file())),
        HarnessKind::Demo => Ok(Vec::new()),
    }
}

/// The Claude account Claude Code is signed in to, from its `auth status`.
fn claude_accounts() -> anyhow::Result<Vec<Value>> {
    let out = run(&harlo_core::claude::program().ok_or_else(|| anyhow::Error::msg("Claude Code isn't installed."))?, &["auth", "status", "--json"], Duration::from_secs(15))?;
    let v: Value = serde_json::from_str(&out).ok().ok_or_else(|| anyhow::Error::msg("Claude Code didn't say whether it's signed in."))?;
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

/// What pi can sign in to: `(id, name, note)`. The rest take an API key
/// (Settings → Models).
const PI_SIGN_INS: [(&str, &str, &str); 7] = [
    ("anthropic", "Anthropic (Claude Pro/Max)", "For your Claude plan's limits, use the Claude subscription extension instead: it signs in through Claude Code."),
    ("openai-codex", "ChatGPT Plus/Pro (Codex)", ""),
    ("github-copilot", "GitHub Copilot", ""),
    ("xai", "xAI (Grok subscription)", ""),
    ("kimi-coding", "Kimi Code (subscription)", ""),
    ("openrouter", "OpenRouter", ""),
    ("meta", "Meta (Muse subscription)", ""),
];

/// The command that signs pi in to `id` in a terminal: pi's sign-in needs
/// one. On a host, over SSH.
fn pi_sign_in_command(id: &str, host: Option<&Host>) -> String {
    pi_terminal_command(&["auth".into(), "login".into(), id.to_owned()], host)
}

/// pi with `args`, as a command for harlo's terminal (here, or over SSH on
/// a host).
fn pi_terminal_command(args: &[String], host: Option<&Host>) -> String {
    pi_terminal_command_in(args, host, None)
}

/// pi with `args` in the folder `cwd` (on the machine), as a command for
/// harlo's terminal.
pub(crate) fn pi_terminal_command_in(args: &[String], host: Option<&Host>, cwd: Option<&str>) -> String {
    use harlo_core::remote::{quote, ssh_args, terminal_path};
    let args: Vec<String> = args.iter().map(|a| quote(a)).collect();
    let cd = cwd.map(|d| format!("cd {} && ", quote(d))).unwrap_or_default();
    match host {
        Some(h) => {
            let script = format!("{} {cd}pi {}", terminal_path(), args.join(" "));
            let ssh: Vec<String> = ssh_args(h).iter().map(|a| quote(a)).collect();
            format!("ssh -t {} {}", ssh.join(" "), quote(&script))
        }
        None => {
            let pi = program(HarnessKind::Pi).map_or_else(|| "pi".to_owned(), |p| p.display().to_string());
            format!("{cd}{} {}", quote(&pi), args.join(" "))
        }
    }
}

/// The machine of a pi target ("pi": "", "pi@box": "box"), when it's pi's.
fn pi_host(agent: &str) -> Option<String> {
    match agent.split_once('@') {
        Some(("pi", h)) => Some(h.to_owned()),
        None if agent == "pi" => Some(String::new()),
        _ => None,
    }
}

/// Runs pi with `args` on `host` ("": here) and waits: its output.
fn run_pi(host: &str, args: &[String]) -> anyhow::Result<String> {
    let line: Vec<String> = args.iter().map(|a| harlo_core::remote::quote(a)).collect();
    let script = format!("PI_OFFLINE=1 \"${{PI:-pi}}\" {} 2>&1", line.join(" "));
    let out = crate::pi_extensions::run_on(host, &script, None).map_err(anyhow::Error::msg)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What an agent can sign in to (blocking).
fn providers_of(who: Who, host: Option<&Host>) -> Vec<Value> {
    let k = match who {
        Who::Claude => {
            return vec![
                json!({ "id": "claudeai", "name": "Claude account", "note": "Pro, Max, Team or Enterprise: sign in on claude.ai in your browser.",
                        "methods": [{ "id": "claudeai", "type": "login", "label": "Sign in" }] }),
                json!({ "id": "console", "name": "Anthropic Console", "note": "Pay as you go with API billing.",
                        "methods": [{ "id": "console", "type": "login", "label": "Sign in" }] }),
            ];
        }
        Who::Agent(k) => k,
    };
    match k {
        // pi signs in itself, in a terminal (`pi auth login <provider>`):
        // harlo opens one with the command.
        HarnessKind::Pi => PI_SIGN_INS
            .iter()
            .map(|(id, name, note)| {
                json!({ "id": id, "name": name, "note": note,
                        "methods": [{ "id": "terminal", "type": "terminal", "label": "Sign in", "command": pi_sign_in_command(id, host) }] })
            })
            .collect(),
        HarnessKind::Demo => Vec::new(),
    }
}

/// Signs an account out (blocking).
fn sign_out(who: Who, id: &str, host: Option<&Host>) -> anyhow::Result<()> {
    let k = match who {
        Who::Claude => {
            let claude = harlo_core::claude::program().ok_or_else(|| anyhow::Error::msg("Claude Code isn't installed."))?;
            return run(&claude, &["auth", "logout"], Duration::from_secs(20)).map(|_| ());
        }
        Who::Agent(k) => k,
    };
    match k {
        HarnessKind::Pi => match host {
            Some(h) => run_there(h, "pi", &["auth", "logout", id], Duration::from_secs(30)).map(|_| ()),
            None => run(&program(k).ok_or_else(|| anyhow::Error::msg("pi isn't installed."))?, &["auth", "logout", id], Duration::from_secs(15)).map(|_| ()),
        },
        HarnessKind::Demo => Err(anyhow::Error::msg("harlo can't sign this agent out.")),
    }
}

/// pi's credentials file (`$PI_CODING_AGENT_DIR/auth.json`, else
/// `~/.pi/agent/auth.json`).
fn pi_auth_file() -> PathBuf {
    std::env::var_os("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".pi/agent"))
        .join("auth.json")
}

/// The accounts in pi's auth file: one per provider, with how it signed in.
/// Credentials are never kept or shown.
fn pi_accounts(file: &Path) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    let Ok(Value::Object(providers)) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
    providers
        .iter()
        .map(|(id, entry)| {
            let detail = match entry["type"].as_str() {
                Some("oauth") => "Signed in with an account",
                Some("api_key" | "api-key" | "apiKey") => "API key",
                _ => "Signed in",
            };
            json!({ "id": id, "label": id, "detail": detail, "signOut": true })
        })
        .collect()
}

/// Notes what's happening to `provider`'s key on `agent` (`cloud`'s
/// `results`).
fn key_result(cloud: &mut HashMap<String, Value>, agent: &str, provider: &str, result: Value) {
    let entry = cloud.entry(agent.to_owned()).or_insert_with(|| json!({ "state": "loading", "list": [] }));
    if !entry["results"].is_object() {
        entry["results"] = json!({});
    }
    entry["results"][provider] = result;
}

/// Runs an agent's command on another machine (stdout), at most `limit`.
fn run_there(host: &Host, program: &str, args: &[&str], limit: Duration) -> anyhow::Result<String> {
    let words: Vec<String> = std::iter::once(program).chain(args.iter().copied()).map(harlo_core::remote::quote).collect();
    let out = harlo_core::remote::run(host, &format!("{} </dev/null", words.join(" ")), limit).map_err(|e| anyhow::anyhow!("{e:#}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(anyhow::Error::msg(err.lines().map(str::trim).rfind(|l| !l.is_empty()).map_or_else(|| format!("{program} failed on {}.", host.name), str::to_owned)))
}

/// Runs a command to its end (stdout), at most `limit`.
fn run(program: &Path, args: &[&str], limit: Duration) -> anyhow::Result<String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("{}: {e}", program.display()))?;
    let deadline = std::time::Instant::now() + limit;
    while child.try_wait().ok().flatten().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return Err(anyhow::anyhow!("{} didn't answer in time.", program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() && text.trim().is_empty() {
        return Err(anyhow::Error::msg(signin::plain(&String::from_utf8_lossy(&out.stderr)).trim().lines().last().unwrap_or("It failed.").to_owned()));
    }
    Ok(text)
}

/// An agent's command, here or on a host.
fn agent_command(program: Option<PathBuf>, name: &str, host: Option<&Host>, args: &[&str]) -> anyhow::Result<std::process::Command> {
    match host {
        Some(h) => {
            let words: Vec<String> = args.iter().map(|a| harlo_core::remote::quote(a)).collect();
            Ok(harlo_core::remote::command(h, &format!("{name} {}", words.join(" "))))
        }
        None => {
            let mut c = std::process::Command::new(program.ok_or_else(|| anyhow::anyhow!("{name} isn't installed."))?);
            c.args(args);
            Ok(c)
        }
    }
}

/// A device code in an agent's instructions ("Enter code: ABCD-1234").
fn device_code(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|w| {
            let parts: Vec<&str> = w.split('-').collect();
            parts.len() == 2 && parts[0].len() == 4 && (4..=5).contains(&parts[1].len()) && w.chars().all(|c| c == '-' || c.is_ascii_uppercase() || c.is_ascii_digit())
        })
        .map(str::to_owned)
}

// ---- signing in ------------------------------------------------------------------

/// Signs in; what to say when it's done.
async fn sign_in(who: Who, provider: &str, method: &str, report: &Report, rx: mpsc::UnboundedReceiver<String>) -> anyhow::Result<String> {
    let k = match who {
        Who::Claude => {
            let mut args = vec!["auth", "login"];
            if method == "console" {
                args.push("--console");
            }
            report.text("Claude Code opens Anthropic's sign-in page in your browser. Approve it there; Claude Code keeps the sign-in.");
            command(agent_command(harlo_core::claude::program(), "claude", None, &args)?, report, rx).await?;
            return Ok("Claude Code is signed in.".into());
        }
        Who::Agent(k) => k,
    };
    match k {
        HarnessKind::Pi => anyhow::bail!("pi signs in itself: run `pi auth login {provider}` in a terminal."),
        HarnessKind::Demo => Err(anyhow::Error::msg("harlo can't sign this agent in.")),
    }
}

/// Runs an agent's sign-in command, showing what it prints (links, what it
/// asks) and handing it the answers. Ok when it ends well.
async fn command(program: std::process::Command, report: &Report, mut rx: mpsc::UnboundedReceiver<String>) -> anyhow::Result<()> {
    let name = program.get_program().to_string_lossy().into_owned();
    let mut child = tokio::process::Command::from(program)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("{name}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or_else(|| anyhow::Error::msg("no input"))?;
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    for mut pipe in [Box::new(child.stdout.take().ok_or_else(|| anyhow::Error::msg("no output"))?) as Box<dyn tokio::io::AsyncRead + Unpin + Send>, Box::new(child.stderr.take().ok_or_else(|| anyhow::Error::msg("no output"))?)] {
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
            () = tokio::time::sleep(Duration::from_millis(300)), if !pending.trim().is_empty() => {
                let text = signin::plain(&pending);
                if let Some(mut ask) = signin::prompt(&text) {
                    pending.clear();
                    // Claude Code's code box: only if the page shows one.
                    ask["optional"] = json!(ask["message"].as_str().is_some_and(|m| m.contains("if prompted")));
                    report.ask(ask);
                }
            },
            answer = rx.recv() => match answer {
                Some(text) => {
                    report.text(if text.trim().is_empty() { "(nothing)" } else { "✓ Sent" });
                    stdin.write_all(format!("{}\n", text.trim()).as_bytes()).await?;
                    stdin.flush().await?;
                }
                // Cancelled: the command goes (killed when dropped).
                None => return Err(anyhow::Error::msg("Cancelled.")),
            },
            status = child.wait(), if !open => break status,
        }
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(anyhow::Error::msg(last.iter().rev().find(|l| l.to_lowercase().contains("fail") || l.to_lowercase().contains("error")).or(last.last()).cloned().unwrap_or_else(|| "It didn't finish.".into()))),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn says_why_a_server_did_not_start() {
        let said = harlo_core::servers::start_failure("12:05:13 FATAL server failed during startup | Device budget cannot hold model weights and required execution resources");
        assert!(said.starts_with("The model and this context don't fit"), "{said}");
        assert_eq!(harlo_core::servers::start_failure("error: no such file"), "error: no such file");
    }

    #[test]
    fn finds_a_models_server() {
        use harlo_core::model_servers::Server;
        let list = vec![
            Server { name: "llamacpp".into(), base_url: "http://127.0.0.1:8090/v1".into(), models: vec!["SmolLM2".into()] },
            Server { name: "ollama".into(), base_url: "http://127.0.0.1:11434/v1".into(), models: vec!["qwen3:4b".into()] },
        ];
        let name = |m| super::server_among(m, &list).map(|(p, s)| (p, s.map(|s| s.base_url)));
        assert_eq!(name("llamacpp/SmolLM2"), Some(("llamacpp".into(), Some("http://127.0.0.1:8090/v1".into()))));
        assert_eq!(name("anthropic/claude"), Some(("anthropic".into(), None)));
        assert_eq!(name("no-provider"), None);
    }

    /// What's left running on this computer (looks only).
    #[test]
    #[ignore = "this computer's processes"]
    fn finds_leftover_servers() {
        println!("8080: {:?}, 1: {:?}", super::leftovers("http://127.0.0.1:8080/v1"), super::leftovers("http://127.0.0.1:1/v1"));
    }

    #[test]
    fn reads_the_model_path_of_a_start_command() {
        assert_eq!(harlo_core::servers::shell_word("'/a/my model'\\''s/x.gguf' --alias q"), "/a/my model's/x.gguf");
        assert_eq!(harlo_core::servers::shell_word(" /plain/x.gguf --port 1"), "/plain/x.gguf");
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
