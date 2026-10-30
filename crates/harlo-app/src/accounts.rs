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
    }
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::signin;
use harlo_harness::HarnessKind;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use qobject::Accounts;

#[derive(Default)]
pub struct AccountsRust {
    revision: i32,
    flow: QString,
    /// The sign-in going on: its id, and where its answers go (dropped:
    /// cancelled).
    current: Option<(u64, mpsc::UnboundedSender<String>)>,
    next_id: u64,
    status: HashMap<String, Value>,
    providers: HashMap<String, Value>,
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
