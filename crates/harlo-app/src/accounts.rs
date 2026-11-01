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
        type Accounts = super::AccountsRust;
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
        /// pi's packages as seen from `folder` ("" for yours alone): `{
        /// state: "loading" | "ready", list: [{ source, path, project,
        /// version, description, adds }], error, job: { action, source,
        /// state: "running" | "done" | "failed", message } }`.
        #[qinvokable]
        fn packages(self: &Accounts, folder: &QString) -> QString;
        /// Reads pi's packages again (`packages` follows).
        #[qinvokable]
        fn load_packages(self: Pin<&mut Accounts>, folder: &QString);
        /// `pi install|update|remove <source>` ("update" of "all": every
        /// package), yours or (`project`) the project's own in `folder`.
        #[qinvokable]
        fn change_package(self: Pin<&mut Accounts>, folder: &QString, action: &QString, source: &QString, project: bool);
    }
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
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
    /// The sign-in going on: its id, and where its answers go (dropped:
    /// cancelled).
    current: Option<(u64, mpsc::UnboundedSender<String>)>,
    next_id: u64,
    status: HashMap<String, Value>,
    providers: HashMap<String, Value>,
    /// `extra_usage` per agent: the answer, and when it came (`None`:
    /// being checked).
    extra_usage: HashMap<String, (bool, Option<Instant>)>,
    /// `packages` per folder.
    packages: HashMap<String, Value>,
    /// `servers` per agent, as last read (`load_servers`).
    servers: HashMap<String, Value>,
}

/// An agent's program: the path set in Settings → Providers (if it's
/// there), else the installed one.
fn program(kind: HarnessKind) -> Option<PathBuf> {
    let set = harlo_core::db::Db::open(&harlo_core::db::default_path()).ok().and_then(|d| d.get(&format!("settings.{}_path", kind.as_str())).ok().flatten()).unwrap_or_default();
    if set.trim().is_empty() { harlo_harness::resolve_program(kind) } else { Some(PathBuf::from(set.trim())).filter(|p| p.is_file()) }
}

fn kind(agent: &str) -> Option<HarnessKind> {
    HarnessKind::parse(agent).filter(|k| HarnessKind::PROVIDERS.contains(k))
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
        let (url, label) = (url.to_owned(), label.trim().trim_end_matches(':').trim().to_owned());
        self.update(|f| {
            if !f.steps.iter().any(|s| s["url"] == url.as_str()) {
                f.steps.push(json!({ "kind": "link", "url": url, "text": label, "open": open }));
            }
        });
    }

    fn device(&self, code: &str, url: &str) {
        let (code, url) = (code.to_owned(), url.to_owned());
        self.update(|f| f.steps.push(json!({ "kind": "device", "code": code, "url": url })));
    }

    fn ask(&self, ask: Value) {
        self.update(|f| f.ask = ask);
    }

    fn finish(&self, ok: bool, message: &str) {
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
        let Some(k) = kind(&agent) else { return };
        // What it's signed in to may have changed.
        self.as_mut().rust_mut().extra_usage.remove(&agent);
        let was = self.rust().status.get(&agent).cloned().unwrap_or_else(|| json!({ "accounts": [] }));
        self.as_mut().rust_mut().status.insert(agent.clone(), json!({ "state": "loading", "accounts": was["accounts"] }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = match accounts_of(k) {
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
        let Some(k) = kind(&agent) else { return };
        self.as_mut().rust_mut().providers.insert(agent.clone(), json!({ "state": "loading", "list": [] }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = match providers_of(k) {
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
        let report = Report { flow: Arc::new(Mutex::new(flow)), thread: self.qt_thread() };
        let Some(k) = kind(&agent) else {
            report.finish(false, "harlo can't sign this agent in.");
            return;
        };
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
        let Some(k) = kind(&agent_s) else { return QString::from("harlo can't sign this agent out.") };
        // In the background (the agent's own command takes a moment); then
        // what it's signed in to, or why it couldn't.
        let was = self.rust().status.get(&agent_s).cloned().unwrap_or_else(|| json!({ "accounts": [] }));
        self.as_mut().rust_mut().status.insert(agent_s.clone(), json!({ "state": "loading", "accounts": was["accounts"] }));
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = sign_out(k, &id);
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
        }
    }

    pub fn extra_usage(mut self: Pin<&mut Self>, agent: &QString) -> bool {
        let agent = agent.to_string();
        let Some(k) = kind(&agent).filter(|k| matches!(k, HarnessKind::Pi | HarnessKind::Omp)) else { return false };
        let (on, fresh) = match self.rust().extra_usage.get(&agent) {
            Some((on, at)) => (*on, at.is_none_or(|t| t.elapsed() < Duration::from_secs(600))),
            None => (false, false),
        };
        if !fresh {
            self.as_mut().rust_mut().extra_usage.insert(agent.clone(), (on, None));
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let now = claude_subscription_in_use(k);
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
                json!({
                    "name": s.name, "baseUrl": s.base_url, "models": s.models, "contexts": contexts, "place": place, "managed": managed, "hermesOk": hermes,
                    "engine": engine_of(&s.base_url),
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
        let (url, key, models) = match kind(&from.to_string()) {
            Some(from) => match model_servers::details(from, &name) {
                Some(d) => d,
                None => return QString::from(&format!("{name} isn't in {}'s file any more.", from.label())),
            },
            // No agent has it: what harlo saved when it set it up (else what
            // it serves now).
            None => {
                let url = from.to_string();
                let mut models = saved_models(&commands_of(&url));
                if models.is_empty() {
                    match model_servers::find(&url, "") {
                        Ok(l) => models = l.models,
                        Err(e) => return QString::from(&format!("Start it first: harlo asks it for its models. ({e})")),
                    }
                    // (The context it serves: not in its list.)
                    for m in models.iter_mut().filter(|m| m.context.is_none()) {
                        m.context = harlo_core::engines::model_context(&url, &m.id);
                    }
                }
                (url, String::new(), models)
            }
        };
        let models = with_levels(&url, models);
        let taken: Vec<String> = serde_json::from_str(&taken.to_string()).unwrap_or_default();
        match model_servers::add(to, &name, &url, &key, &models, &taken) {
            Ok(()) if !reread(to) => QString::from("OPENCODE_BUSY"),
            Ok(()) => QString::default(),
            Err(e) => QString::from(&e),
        }
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
            let (errors, busy, summary) = remove(&name, &url, delete_files, &step);
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

/// Removes a server (blocking): out of each agent that has it; one harlo set
/// up stopped and forgotten, its models deleted with `delete_files`.
/// `step(what, done, of)` says how far. What went wrong, whether OpenCode
/// was busy (its open sessions keep the server until they close), and what
/// it did ("Taken out of OpenCode and pi. Deleted 2 models from Ollama
/// (15.2 GB).").
fn remove(name: &str, url: &str, delete_files: bool, step: &dyn Fn(&str, usize, usize)) -> (Vec<String>, bool, String) {
    let mut errors = Vec::new();
    let mut busy = false;
    let mut agents: Vec<&str> = Vec::new();
    let mut summary: Vec<String> = Vec::new();
    let gb = |b: u64| format!("{:.1} GB", b as f64 / 1e9);
    // (Ollama's models to delete: named before the agents' entries go.)
    let ollama = if delete_files && engine_of(url) == "Ollama" { ollama_models_of(url) } else { Vec::new() };
    for k in [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::OpenCode, HarnessKind::Hermes] {
        if !model_servers::servers(k).iter().any(|s| s.name == name && s.base_url == url) {
            continue;
        }
        step(&format!("Taking it out of {}'s config", k.label()), 0, 0);
        match model_servers::remove(k, name) {
            Ok(()) => {
                busy |= !reread(k);
                agents.push(k.label());
            }
            Err(e) => errors.push(format!("{}: {e}", k.label())),
        }
    }
    if !agents.is_empty() {
        summary.push(format!("Taken out of {}.", agents.join(", ")));
    }
    // Set up by harlo, and no agent has it now: stopped and forgotten.
    let still = [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::OpenCode, HarnessKind::Hermes].iter().any(|k| model_servers::servers(*k).iter().any(|s| s.base_url == url));
    if still || !commands_of(url)["start"].as_str().is_some_and(|c| !c.is_empty()) {
        return (errors, busy, summary.join(" "));
    }
    // Ollama's models out of its store first (it must run for that).
    if !ollama.is_empty() {
        if !model_servers::answers(url) {
            step("Starting Ollama to delete its models", 0, 0);
        }
        match ensure_running(url) {
            Ok(()) => {
                let sizes = harlo_core::engines::ollama_sizes(url);
                let (mut count, mut freed) = (0, 0);
                for (i, n) in ollama.iter().enumerate() {
                    step(&format!("Deleting {} from Ollama", n.strip_prefix("hf.co/").unwrap_or(n)), i, ollama.len());
                    match harlo_core::engines::ollama_delete(url, n) {
                        Ok(()) => {
                            count += 1;
                            freed += sizes.iter().find(|(s, _)| s == n).map_or(0, |(_, b)| *b);
                        }
                        Err(e) => errors.push(e),
                    }
                }
                if count > 0 {
                    summary.push(format!("Deleted {count} {} from Ollama ({}).", if count == 1 { "model" } else { "models" }, gb(freed)));
                }
            }
            Err(e) => errors.push(format!("Ollama didn't start to delete its models: {e}")),
        }
    }
    let files = if delete_files { model_files(url) } else { Vec::new() };
    step("Stopping the server", 0, 0);
    stop_any(url);
    // (A llama.cpp router's preset, Basalt's settings file, too.)
    for flag in [" --models-preset ", " serve --config "] {
        if let Some(file) = start_command(url).split(flag).nth(1).map(shell_word).filter(|f| f.contains("/harlo/")) {
            let _ = std::fs::remove_file(file);
        }
    }
    save_commands(url, "", "");
    let _ = std::fs::remove_file(log_path(url));
    let (mut count, mut freed) = (0, 0);
    for (i, f) in files.iter().enumerate() {
        step(&format!("Deleting {}", f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), i, files.len());
        let size = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(f) {
            Ok(()) => {
                count += 1;
                freed += size;
            }
            Err(e) => errors.push(format!("{}: {e}", f.display())),
        }
    }
    summary.push("Stopped it; harlo forgot how to start it.".into());
    if count > 0 {
        summary.push(format!("Deleted {count} model {} ({}).", if count == 1 { "file" } else { "files" }, gb(freed)));
    }
    // Its folder too, once empty (harlo's models folder stays).
    let root = models_dir();
    for f in &files {
        let mut dir = f.parent();
        while let Some(d) = dir.filter(|d| d.starts_with(&root) && *d != root) {
            if std::fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    (errors, busy, summary.join(" "))
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

/// The models an Ollama server serves in harlo: what its setup saved, and
/// what agents list for it (taken before they're taken out).
fn ollama_models_of(url: &str) -> Vec<String> {
    let mut names: Vec<String> = saved_models(&commands_of(url)).into_iter().map(|m| m.id).collect();
    for k in [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::OpenCode, HarnessKind::Hermes] {
        for s in model_servers::servers(k).into_iter().filter(|s| s.base_url == url) {
            names.extend(s.models);
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Where harlo keeps the models it downloads.
fn models_dir() -> PathBuf {
    harlo_core::engines::engines_dir().parent().map(|d| d.join("models")).unwrap_or_default()
}

/// The first word of a shell command line, unquoted ('…', \x).
pub(crate) fn shell_word(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.trim_start().chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => out.extend(chars.by_ref().take_while(|c| *c != '\'')),
            '\\' => out.extend(chars.next()),
            c if c.is_whitespace() => break,
            c => out.push(c),
        }
    }
    out
}

/// The model files of a server harlo set up with llama.cpp: the GGUF its
/// start command loads (`-m '…'`) and its other parts, in harlo's folder.
fn model_files(url: &str) -> Vec<PathBuf> {
    let start = commands_of(url)["start"].as_str().unwrap_or_default().to_owned();
    // (Basalt's: the pack its settings file names.)
    if let Some(rest) = start.split(" serve --config ").nth(1).filter(|_| start.contains("basalt")) {
        let settings: Value = std::fs::read_to_string(shell_word(rest)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        return settings["server"]["model"].as_str().map(PathBuf::from).filter(|p| p.starts_with(models_dir()) && p.is_file()).into_iter().collect();
    }
    // (NInfer's: the entry file its settings name, and the parts beside it.)
    if let Some(rest) = start.split(" --config ").nth(1).filter(|_| start.contains("ninfer-serve")) {
        let settings = std::fs::read_to_string(shell_word(rest)).unwrap_or_default();
        let entry = settings.lines().find_map(|l| serde_json::from_str::<String>(l.strip_prefix("path = ")?).ok()).map(PathBuf::from);
        let Some((dir, name)) = entry.filter(|p| p.starts_with(models_dir())).and_then(|p| Some((p.parent()?.to_path_buf(), p.file_name()?.to_string_lossy().into_owned()))) else { return Vec::new() };
        return std::fs::read_dir(&dir).into_iter().flatten().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&name)).map(|e| e.path()).collect();
    }
    // (vLLM's: the model's folder, all of it.)
    if let Some(rest) = start.split(" serve ").nth(1).filter(|_| start.contains("vllm")) {
        let dir = PathBuf::from(shell_word(rest));
        if !dir.starts_with(models_dir()) || dir == models_dir() {
            return Vec::new();
        }
        let mut files = Vec::new();
        let mut dirs = vec![dir];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() { dirs.push(p) } else { files.push(p) }
            }
        }
        return files;
    }
    // (A router's: each model of its preset.)
    let paths: Vec<PathBuf> = match start.split(" --models-preset ").nth(1) {
        Some(rest) => harlo_core::engines::read_preset(Path::new(&shell_word(rest))).into_iter().map(|m| m.path).collect(),
        None => start.split(" -m ").nth(1).map(|rest| PathBuf::from(shell_word(rest))).into_iter().collect(),
    };
    paths
        .iter()
        .filter_map(|path| Some((path.parent()?.to_path_buf(), path.file_name()?.to_string_lossy().into_owned())))
        .filter(|(dir, _)| dir.starts_with(models_dir()))
        .flat_map(|(dir, file)| harlo_core::engines::gguf_parts(&file).into_iter().map(move |p| dir.join(p)))
        .filter(|p| p.is_file())
        .collect()
}

// ---- starting and stopping model servers --------------------------------------

/// Servers harlo started, by address: their process (a group of its own).
static STARTED: Mutex<Option<HashMap<String, std::process::Child>>> = Mutex::new(None);

fn server_key(url: &str) -> String {
    format!("model_server.{}", url.trim_end_matches('/'))
}

fn commands_of(url: &str) -> Value {
    harlo_core::db::Db::open(&harlo_core::db::default_path())
        .ok()
        .and_then(|d| d.get(&server_key(url)).ok().flatten())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({ "start": "", "stop": "" }))
}

/// Where a server's output goes: harlo's logs folder.
pub(crate) fn log_path(url: &str) -> PathBuf {
    let name: String = url.split("://").last().unwrap_or(url).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    harlo_core::db::default_path().parent().map(|d| d.join("logs/servers")).unwrap_or_default().join(format!("{}.log", name.trim_matches('-')))
}

fn log_tail(url: &str) -> String {
    let text = std::fs::read_to_string(log_path(url)).unwrap_or_default();
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = &lines[lines.len().saturating_sub(3)..];
    // (Its error, without what it printed before it.)
    let from = tail.iter().position(|l| l.to_lowercase().contains("error:")).unwrap_or(0);
    start_failure(&tail[from..].join("\n"))
}

/// Why a server didn't start, said plainly where the engine's words don't.
fn start_failure(log: &str) -> String {
    if log.contains("Device budget cannot hold") {
        return "The model and this context don't fit on your graphics card together. Set it up with a smaller context, or stop what else is using the card.".to_owned();
    }
    log.to_owned()
}

/// Saves a server's start and stop commands (empty ones: harlo forgets it).
pub(crate) fn save_commands(url: &str, start: &str, stop: &str) {
    let v = json!({ "start": start.trim(), "stop": stop.trim() });
    if let Ok(db) = harlo_core::db::Db::open(&harlo_core::db::default_path()) {
        let _ = db.set(&server_key(url), &v.to_string());
    }
}

/// Saves the name and models of a server harlo set up (after its test), so
/// it's listed, and can go to agents, before any agent has it.
pub(crate) fn save_server(url: &str, name: &str, models: &[Value]) {
    let mut v = commands_of(url);
    v["name"] = json!(name);
    v["models"] = json!(models);
    if let Ok(db) = harlo_core::db::Db::open(&harlo_core::db::default_path()) {
        let _ = db.set(&server_key(url), &v.to_string());
    }
}

/// The servers harlo set up (it can start them): `(address, saved)`.
fn set_up_servers() -> Vec<(String, Value)> {
    let Ok(db) = harlo_core::db::Db::open(&harlo_core::db::default_path()) else { return Vec::new() };
    db.with_prefix("model_server.")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| Some((k.strip_prefix("model_server.")?.to_owned(), serde_json::from_str::<Value>(&v).ok()?)))
        .filter(|(_, v)| v["start"].as_str().is_some_and(|c| !c.is_empty()))
        .collect()
}

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

/// The engine of a server harlo set up (from its start command); "" for
/// one it didn't.
fn engine_of(url: &str) -> &'static str {
    let start = commands_of(url)["start"].as_str().unwrap_or_default().to_owned();
    if start.contains("llama-server") {
        "llama.cpp"
    } else if start.contains("ollama") {
        "Ollama"
    } else if start.contains("strata") {
        "Strata"
    } else if start.contains("basalt") {
        "Basalt"
    } else if start.contains("ninfer-serve") {
        "NInfer"
    } else if start.contains("vllm") {
        "vLLM"
    } else if start.is_empty() {
        ""
    } else {
        "Your command"
    }
}

/// What harlo keeps of a server it set up (its commands, name, models), to
/// put back if a setup over it doesn't finish.
pub(crate) fn server_record(url: &str) -> Value {
    commands_of(url)
}

pub(crate) fn restore_server_record(url: &str, v: &Value) {
    if let Ok(db) = harlo_core::db::Db::open(&harlo_core::db::default_path()) {
        let _ = db.set(&server_key(url), &v.to_string());
    }
}

/// The ports of this computer's model servers harlo knows (set up by it, or
/// in an agent's config), running or not: a new one doesn't take them.
pub(crate) fn ports_in_use() -> Vec<(u16, String)> {
    let port = |url: &str| url.split("://").nth(1)?.split('/').next()?.rsplit_once(':')?.1.parse::<u16>().ok();
    let mut out: Vec<(u16, String)> = Vec::new();
    for (url, v) in set_up_servers() {
        if let Some(p) = port(&url) {
            out.push((p, v["name"].as_str().filter(|n| !n.is_empty()).map_or_else(|| engine_of(&url).to_lowercase(), str::to_owned)));
        }
    }
    for k in [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::OpenCode, HarnessKind::Hermes] {
        for s in model_servers::servers(k) {
            let host = s.base_url.split("://").nth(1).unwrap_or_default().split(['/', ':']).next().unwrap_or_default().to_owned();
            if ["127.0.0.1", "localhost"].contains(&host.as_str())
                && let Some(p) = port(&s.base_url)
                && !out.iter().any(|(q, _)| *q == p)
            {
                out.push((p, s.name));
            }
        }
    }
    out
}

/// A server's start command ("" for one harlo didn't set up).
pub(crate) fn start_command(url: &str) -> String {
    commands_of(url)["start"].as_str().unwrap_or_default().to_owned()
}

/// Stops a server harlo set up, however it's running (started by this
/// harlo or left by an earlier one), until it stops answering.
pub(crate) fn stop_any(url: &str) {
    match STARTED.lock().unwrap().as_mut().and_then(|m| m.remove(url)) {
        Some(child) => stop_group(child),
        None => stop_pids(&leftovers(url)),
    }
    // (Its own stop command too, for a server not harlo's process: Strata's.)
    if model_servers::answers(url) && commands_of(url)["stop"].as_str().is_some_and(|c| !c.is_empty()) {
        run_stop(url);
    }
    for _ in 0..40 {
        if !model_servers::answers(url) {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// The models harlo saved for a server it set up (after its tests).
pub(crate) fn saved_models_of(url: &str) -> Vec<model_servers::Found> {
    saved_models(&commands_of(url))
}

fn saved_models(v: &Value) -> Vec<model_servers::Found> {
    v["models"].as_array().into_iter().flatten().filter_map(|m| Some(model_servers::Found { id: m["id"].as_str()?.to_owned(), context: m["context"].as_u64(), reasoning: m["reasoning"] == true, ..Default::default() })).collect()
}

/// A server's models as an agent's file gets them: Basalt and NInfer's Qwen3.8
/// tell thinking levels apart (its Qwen3.6 has only on and off).
fn with_levels(url: &str, models: Vec<model_servers::Found>) -> Vec<model_servers::Found> {
    let engine = engine_of(url);
    models.into_iter().map(|m| model_servers::Found { levels: engine == "Basalt" || (engine == "NInfer" && m.id.contains("3.8")), ..m }).collect()
}

/// Basalt servers agents got before harlo offered their levels: offered now,
/// before any agent reads its file.
pub(crate) fn offer_levels() {
    for (url, _) in set_up_servers().into_iter().filter(|(url, _)| engine_of(url) == "Basalt") {
        for k in [HarnessKind::Pi, HarnessKind::OpenCode] {
            for s in model_servers::servers(k).into_iter().filter(|s| s.base_url == url) {
                if let Err(e) = model_servers::offer_levels(k, &s.name) {
                    tracing::warn!("thinking levels for {}: {e}", s.name);
                }
            }
        }
    }
}

/// Starts a process from one thread that lives as long as harlo: the
/// parent-death signal goes with the thread that started it, and a worker
/// thread ending would stop the server.
fn spawn_from_launcher(cmd: std::process::Command) -> std::io::Result<std::process::Child> {
    type Job = (std::process::Command, std::sync::mpsc::Sender<std::io::Result<std::process::Child>>);
    static LAUNCHER: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<Job>>> = std::sync::OnceLock::new();
    let tx = LAUNCHER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("server-launcher".into())
            .spawn(move || {
                for (mut cmd, reply) in rx {
                    let _ = reply.send(cmd.spawn());
                }
            })
            .expect("a thread");
        Mutex::new(tx)
    });
    let (reply, answer) = std::sync::mpsc::channel();
    tx.lock().unwrap().send((cmd, reply)).map_err(|_| std::io::Error::other("the launcher stopped"))?;
    answer.recv().map_err(|_| std::io::Error::other("the launcher stopped"))?
}

/// Runs a server's start command in a shell from the home folder, its
/// output to its log, as one of harlo's (stopped when harlo quits).
pub(crate) fn launch(url: &str, command: &str) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let log = log_path(url);
    let _ = std::fs::create_dir_all(log.parent().unwrap_or(std::path::Path::new("/")));
    let out = std::fs::File::create(&log).map_err(|e| format!("Couldn't run it: {e}"))?;
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .current_dir(std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/")))
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone().map_err(|e| format!("Couldn't run it: {e}"))?)
        .stderr(out)
        .process_group(0);
    // It stops when harlo does, however harlo ends (Ctrl+C, a crash):
    // the kernel tells it when its parent goes.
    // SAFETY: only async-signal-safe calls between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let child = spawn_from_launcher(cmd).map_err(|e| format!("Couldn't run it: {e}"))?;
    STARTED.lock().unwrap().get_or_insert_with(HashMap::new).insert(url.to_owned(), child);
    Ok(())
}

/// Waits (blocking) until a launched server answers: whether harlo still
/// holds its process (a launcher like `strata launch` starts the server and
/// exits, which is fine), or why it didn't come up. `tick` gets its log's
/// last line now and then; `stop()` gives up.
pub(crate) fn wait_up(url: &str, limit: Duration, tick: &mut dyn FnMut(&str), stop: &dyn Fn() -> bool) -> Result<bool, String> {
    let deadline = Instant::now() + limit;
    let mut held = true;
    loop {
        if held {
            let exited = STARTED.lock().unwrap().as_mut().and_then(|m| m.get_mut(url)).map(|c| c.try_wait().ok().flatten());
            match exited {
                None => return Err("Stopped.".into()),
                Some(Some(status)) => {
                    STARTED.lock().unwrap().as_mut().map(|m| m.remove(url));
                    if !status.success() {
                        let tail = log_tail(url);
                        return Err(if tail.is_empty() { format!("It stopped ({status}).") } else { tail });
                    }
                    // A launcher: the server goes on without it.
                    held = false;
                }
                Some(None) => {}
            }
        }
        if model_servers::answers(url) {
            return Ok(held);
        }
        if stop() {
            return Err("Cancelled.".into());
        }
        if Instant::now() > deadline {
            return Err(format!("It hasn't answered in {} minutes.", limit.as_secs() / 60));
        }
        // (Its last line; of a progress bar redrawn in place with carriage
        // returns, as vLLM's, only its latest drawing.)
        let last = std::fs::read_to_string(log_path(url)).ok().and_then(|t| {
            let line = t.lines().rev().map(str::trim).find(|l| !l.is_empty())?.to_owned();
            line.split('\r').map(str::trim).rfind(|s| !s.is_empty()).map(str::to_owned)
        });
        if let Some(line) = last {
            tick(&line);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Stops a server harlo launched (if it did).
/// The server behind a session's model (`provider/id`) when harlo set it
/// up on this computer (it can start it): its name and address.
pub(crate) fn managed_server(agent: &str, model: &str) -> Option<(String, String)> {
    let k = kind(agent)?;
    // Hermes: "custom:<server>:<model>", or its current model as just
    // "custom:<model>" (the server whose models have it, then).
    let by_model;
    let provider = match k {
        HarnessKind::Hermes => match model_servers::hermes_server_of(model).filter(|n| model_servers::servers(k).iter().any(|s| s.name == *n)) {
            Some(name) => name,
            None => {
                let id = model.strip_prefix("custom:").or_else(|| model.strip_prefix("custom/")).unwrap_or(model);
                by_model = model_servers::servers(k).into_iter().find(|s| s.models.iter().any(|m| m == id))?.name;
                by_model.as_str()
            }
        },
        _ => model.split_once('/')?.0,
    };
    let managed = |url: &str| commands_of(url)["start"].as_str().is_some_and(|c| !c.is_empty());
    if let Some(s) = model_servers::servers(k).into_iter().find(|s| s.name == provider && managed(&s.base_url)) {
        return Some((s.name, s.base_url));
    }
    // omp's own "llama.cpp" and "ollama" providers: the engines' usual
    // addresses, where it finds a running server by itself.
    let usual = match provider {
        "llama.cpp" => 8080,
        "ollama" => 11434,
        _ => 0,
    };
    if k == HarnessKind::Omp && usual > 0 {
        let urls = [format!("http://127.0.0.1:{usual}/v1"), format!("http://localhost:{usual}/v1")];
        let url = urls.iter().find(|u| managed(u))?.clone();
        let name = [HarnessKind::Pi, HarnessKind::OpenCode, HarnessKind::Hermes, HarnessKind::Omp]
            .iter()
            .find_map(|k| model_servers::servers(*k).into_iter().find(|s| s.base_url.trim_end_matches('/') == url))
            .map_or_else(|| provider.to_owned(), |s| s.name);
        return Some((name, url));
    }
    None
}

/// Starts a server harlo set up unless it answers already (blocking, until
/// it serves).
pub(crate) fn ensure_running(url: &str) -> Result<(), String> {
    if model_servers::answers(url) {
        return Ok(());
    }
    let command = commands_of(url)["start"].as_str().unwrap_or_default().to_owned();
    launch(url, &command)?;
    wait_up(url, Duration::from_secs(15 * 60), &mut |_| {}, &|| false)?;
    for problem in sync_context(url) {
        tracing::warn!("{problem}");
    }
    Ok(())
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

pub(crate) fn stop_launched(url: &str) {
    if let Some(child) = STARTED.lock().unwrap().as_mut().and_then(|m| m.remove(url)) {
        stop_group(child);
    }
}

/// Stops the servers harlo started (it's quitting).
pub fn stop_started_servers() {
    for (_, child) in STARTED.lock().unwrap().take().unwrap_or_default() {
        stop_group(child);
    }
    // A server with its own stop command (Strata's launcher starts its
    // server and exits: nothing of harlo's to stop), still answering.
    for (url, v) in set_up_servers() {
        if v["stop"].as_str().is_some_and(|c| !c.is_empty()) && model_servers::answers(&url) {
            run_stop(&url);
        }
    }
    // And those an earlier harlo left running (it was stopped with Ctrl+C,
    // say, before servers went with it).
    let left: Vec<i32> = set_up_servers().iter().flat_map(|(u, _)| leftovers(u)).collect();
    if !left.is_empty() {
        stop_pids(&left);
    }
}

/// Runs a server's own stop command (blocking, a minute at most).
fn run_stop(url: &str) {
    let stop = commands_of(url)["stop"].as_str().unwrap_or_default().to_owned();
    let Ok(mut child) = std::process::Command::new("sh").arg("-c").arg(&stop).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() else { return };
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while child.try_wait().ok().flatten().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Servers harlo set up that an earlier harlo left running (it ended
/// before stopping them): one of harlo's engines serving the address's
/// port (`--port N`, or Ollama's `OLLAMA_HOST`).
fn leftovers(url: &str) -> Vec<i32> {
    let port = url.split("://").nth(1).and_then(|r| r.split('/').next()).and_then(|h| h.rsplit_once(':')).map(|(_, p)| p.to_owned()).unwrap_or_default();
    if port.parse::<u16>().is_err() {
        return Vec::new();
    }
    let engines = harlo_core::engines::engines_dir();
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| *pid != std::process::id() as i32)
        .filter(|pid| {
            if !std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| exe.starts_with(&engines)) {
                return false;
            }
            // A running harlo's (this one's, another window's): not left over.
            let parent = std::fs::read_to_string(format!("/proc/{pid}/status")).ok().and_then(|t| t.lines().find_map(|l| l.strip_prefix("PPid:")).and_then(|p| p.trim().parse::<i32>().ok()));
            if parent.is_some_and(|pp| std::fs::read_link(format!("/proc/{pp}/exe")).is_ok_and(|exe| exe.file_name().is_some_and(|n| n.to_string_lossy().starts_with("harlo")))) {
                return false;
            }
            let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
            let host = format!(":{port}");
            args.windows(2).any(|w| w[0] == b"--port" && w[1] == port.as_bytes())
                || std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default().split(|b| *b == 0).any(|v| v.starts_with(b"OLLAMA_HOST=") && v.ends_with(host.as_bytes()))
        })
        .collect()
}

/// Asks processes to stop, then makes them.
fn stop_pids(pids: &[i32]) {
    for pid in pids {
        // SAFETY: a signal to a server process harlo set up.
        unsafe { libc::kill(*pid, libc::SIGTERM) };
    }
    for _ in 0..50 {
        if pids.iter().all(|p| !Path::new(&format!("/proc/{p}")).exists()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for pid in pids {
        // SAFETY: as above.
        unsafe { libc::kill(*pid, libc::SIGKILL) };
    }
}

/// Asks the server's process group to stop, then makes it.
fn stop_group(mut child: std::process::Child) {
    let pid = child.id() as i32;
    // SAFETY: a signal to the group harlo started.
    unsafe { libc::kill(-pid, libc::SIGTERM) };
    for _ in 0..50 {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: as above.
    unsafe { libc::kill(-pid, libc::SIGKILL) };
    let _ = child.wait();
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

fn package_json(p: &harlo_core::pi_packages::Package) -> Value {
    json!({ "source": p.source, "path": p.path.to_string_lossy(), "project": p.project, "version": p.version, "description": p.description, "adds": p.adds })
}

impl Accounts {
    pub fn packages(&self, folder: &QString) -> QString {
        let v = self.rust().packages.get(&folder.to_string()).cloned().unwrap_or_else(|| json!({ "state": "loading", "list": [], "error": "", "job": null }));
        QString::from(&v.to_string())
    }

    pub fn load_packages(mut self: Pin<&mut Self>, folder: &QString) {
        let folder = folder.to_string();
        let mut v = self.rust().packages.get(&folder).cloned().unwrap_or_else(|| json!({ "list": [], "job": null }));
        v["state"] = json!("loading");
        self.as_mut().rust_mut().packages.insert(folder.clone(), v);
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = program(HarnessKind::Pi).ok_or_else(|| "pi isn't installed.".to_owned()).and_then(|pi| harlo_core::pi_packages::list(&pi, &package_dir(&folder)));
            let _ = thread.queue(move |mut this| {
                let mut v = this.rust().packages.get(&folder).cloned().unwrap_or_else(|| json!({ "job": null }));
                v["state"] = json!("ready");
                match result {
                    Ok(list) => {
                        v["list"] = Value::Array(list.iter().map(package_json).collect());
                        v["error"] = json!("");
                    }
                    Err(e) => v["error"] = json!(e),
                }
                this.as_mut().rust_mut().packages.insert(folder, v);
                this.bump();
            });
        });
    }

    pub fn change_package(mut self: Pin<&mut Self>, folder: &QString, action: &QString, source: &QString, project: bool) {
        let (folder, action, source) = (folder.to_string(), action.to_string(), source.to_string().trim().to_owned());
        let mut v = self.rust().packages.get(&folder).cloned().unwrap_or_else(|| json!({ "state": "ready", "list": [], "error": "" }));
        if v["job"]["state"] == "running" {
            return;
        }
        v["job"] = json!({ "action": action, "source": source, "state": "running", "message": "" });
        self.as_mut().rust_mut().packages.insert(folder.clone(), v);
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = program(HarnessKind::Pi)
                .ok_or_else(|| "pi isn't installed.".to_owned())
                .and_then(|pi| harlo_core::pi_packages::change(&pi, &package_dir(&folder), &action, &source, project));
            let _ = thread.queue(move |mut this| {
                let mut v = this.rust().packages.get(&folder).cloned().unwrap_or_default();
                let (state, message) = match result {
                    Ok(()) => ("done", match action.as_str() {
                        "install" => "Installed. New pi sessions have it.",
                        "remove" => "Removed. New pi sessions are without it.",
                        _ => "Updated. New pi sessions use the new version.",
                    }.to_owned()),
                    Err(e) => ("failed", e),
                };
                v["job"] = json!({ "action": action, "source": source, "state": state, "message": message });
                this.as_mut().rust_mut().packages.insert(folder.clone(), v);
                this.load_packages(&QString::from(&folder));
            });
        });
    }
}

/// Where pi's package commands run: the project, or (yours) your home.
fn package_dir(folder: &str) -> PathBuf {
    if folder.is_empty() { std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/")) } else { PathBuf::from(folder) }
}

/// Whether the agent uses Claude through a Claude subscription (blocking):
/// pi as its own warning checks (not when it's switched off in pi's
/// settings), omp from what it's signed in to.
fn claude_subscription_in_use(k: HarnessKind) -> bool {
    match k {
        HarnessKind::Pi => pi_helper(&["subscription"], Duration::from_secs(30)).ok().and_then(|v| v["subscription"].as_bool()).unwrap_or(false),
        HarnessKind::Omp => signin::omp_claude_subscription(None),
        _ => false,
    }
}

// ---- what each agent is signed in to ------------------------------------------

/// `[{ id, label, detail, signOut }]` (blocking).
fn accounts_of(k: HarnessKind) -> Result<Vec<Value>, String> {
    match k {
        HarnessKind::Claude => {
            let claude = program(k).ok_or("Claude Code isn't installed.")?;
            let out = run(&claude, &["auth", "status", "--json"], Duration::from_secs(15))?;
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
            let v = pi_helper(&["list"], Duration::from_secs(30))?;
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
            let server = opencode_server()?;
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
fn providers_of(k: HarnessKind) -> Result<Vec<Value>, String> {
    match k {
        HarnessKind::Claude => Ok(vec![
            json!({ "id": "claudeai", "name": "Claude account", "note": "Pro, Max, Team or Enterprise: sign in on claude.ai in your browser.",
                    "methods": [{ "id": "claudeai", "type": "login", "label": "Sign in" }] }),
            json!({ "id": "console", "name": "Anthropic Console", "note": "Pay as you go with API billing.",
                    "methods": [{ "id": "console", "type": "login", "label": "Sign in" }] }),
        ]),
        HarnessKind::Omp => {
            let omp = program(k).ok_or("omp isn't installed.")?;
            let list = omp_list(&omp)?;
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
            let v = pi_helper(&["list"], Duration::from_secs(30))?;
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
            let server = opencode_server()?;
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
        HarnessKind::Hermes => Ok(vec![json!({ "id": "hermes", "name": "Hermes' providers", "note": "Hermes chooses its provider and model in its own setup: run `hermes model` in a terminal.", "methods": [] })]),
        HarnessKind::Demo => Ok(Vec::new()),
    }
}

/// Signs an account out (blocking).
fn sign_out(k: HarnessKind, id: &str) -> Result<(), String> {
    match k {
        HarnessKind::Claude => {
            let claude = program(k).ok_or("Claude Code isn't installed.")?;
            run(&claude, &["auth", "logout"], Duration::from_secs(20)).map(|_| ())
        }
        HarnessKind::Pi => {
            let v = pi_helper(&["logout", id], Duration::from_secs(30))?;
            v["error"].as_str().map_or(Ok(()), |e| Err(e.to_owned()))
        }
        HarnessKind::OpenCode => opencode_server()?.request("DELETE", &format!("/api/credential/{}", harlo_harness::opencode::url_encode(id)), None, None).map(|_| ()).map_err(|e| format!("{e:#}")),
        _ => Err("Sign out in omp itself (its /logout).".into()),
    }
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
fn omp_list(omp: &Path) -> Result<Vec<(u32, String)>, String> {
    let mut child = std::process::Command::new(omp)
        .arg("login")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", omp.display()))?;
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

// ---- signing in ------------------------------------------------------------------

/// Signs in; what to say when it's done.
async fn sign_in(k: HarnessKind, provider: &str, method: &str, answers: &Value, report: &Report, rx: mpsc::UnboundedReceiver<String>) -> Result<String, String> {
    match k {
        HarnessKind::Claude => {
            let claude = program(k).ok_or("Claude Code isn't installed.")?;
            let mut args = vec!["auth", "login"];
            if method == "console" {
                args.push("--console");
            }
            report.text("Claude Code opens Anthropic's sign-in page in your browser. Approve it there; Claude Code keeps the sign-in.");
            command(&claude, &args, None, report, rx).await?;
            Ok("Claude Code is signed in.".into())
        }
        HarnessKind::Omp => {
            let omp = program(k).ok_or("omp isn't installed.")?;
            let number: u32 = provider.parse().map_err(|_| "Pick one of omp's providers.".to_owned())?;
            command(&omp, &["login"], Some(number), report, rx).await?;
            Ok("omp is signed in.".into())
        }
        HarnessKind::Pi => pi_sign_in(provider, method, report, rx).await,
        HarnessKind::OpenCode => {
            let (provider, method, answers, report) = (provider.to_owned(), method.to_owned(), answers.clone(), report.clone());
            tokio::task::spawn_blocking(move || opencode_sign_in(&provider, &method, &answers, &report, rx)).await.map_err(|e| e.to_string())?
        }
        HarnessKind::Hermes => Err("Hermes chooses its provider in its own setup: run `hermes model` in a terminal.".into()),
        HarnessKind::Demo => Err("harlo can't sign this agent in.".into()),
    }
}

/// Runs an agent's sign-in command, showing what it prints (links, what it
/// asks) and handing it the answers; `choose`: the number to answer its
/// list with (omp). Ok when it ends well.
async fn command(program: &Path, args: &[&str], choose: Option<u32>, report: &Report, mut rx: mpsc::UnboundedReceiver<String>) -> Result<(), String> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("{}: {e}", program.display()))?;
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
                            None => report.text(t),
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
    let (package, node) = pi_runtime()?;
    let helper = pi_helper_file()?;
    let mut child = tokio::process::Command::new(&node)
        .arg(&helper)
        .arg(&package)
        .args(["login", provider, method])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("{}: {e}", node.display()))?;
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
    let server = opencode_server()?;
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
    #[test]
    fn says_why_a_server_did_not_start() {
        let said = super::start_failure("12:05:13 FATAL server failed during startup | Device budget cannot hold model weights and required execution resources");
        assert!(said.starts_with("The model and this context don't fit"), "{said}");
        assert_eq!(super::start_failure("error: no such file"), "error: no such file");
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
    fn device_codes_in_instructions() {
        assert_eq!(super::device_code("Enter code: ABCD-1234 at github.com/login/device").as_deref(), Some("ABCD-1234"));
        assert_eq!(super::device_code("Complete authorization in your browser."), None);
        assert_eq!(super::device_code("use-this lower-case"), None);
    }
}
