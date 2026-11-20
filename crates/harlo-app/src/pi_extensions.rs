//! `PiExtensions` (QML singleton): pi's packages, the extensions it loads,
//! on this computer or a host (Settings → Agents → pi): listed with
//! harlo-core's [`pi_packages`], installed and removed with pi's own
//! `pi install` and `pi remove` (in the background, as agent installs are),
//! and switched on or off in pi's settings.

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
        /// Bumped when a list or a job changes.
        #[qproperty(i32, revision)]
        type PiExtensions = super::PiExtensionsRust;
    }

    extern "RustQt" {
        /// pi's packages changed on `host` ("" this computer): sessions
        /// started from now on load them (and its models may have changed).
        #[qsignal]
        fn changed(self: Pin<&mut PiExtensions>, host: QString);
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// `{ state: "" | "loading" | "ready" | "error", error, packages:
        /// [{ source, path, name, version, description, enabled }], jobs: {
        /// <source>: { state: "running" | "done" | "failed", message } },
        /// recommended: [{ source, title, detail }] }` for `host` ("" this
        /// computer).
        #[qinvokable]
        fn status(self: &PiExtensions, host: &QString) -> QString;
        /// Reads the list again.
        #[qinvokable]
        fn refresh(self: Pin<&mut PiExtensions>, host: &QString);
        /// Installs a package (`npm:…`, `git:…`, a URL or a path): "" when
        /// it started, else why not.
        #[qinvokable]
        fn install(self: Pin<&mut PiExtensions>, host: &QString, source: &QString) -> QString;
        #[qinvokable]
        fn remove(self: Pin<&mut PiExtensions>, host: &QString, source: &QString);
        #[qinvokable]
        fn set_enabled(self: Pin<&mut PiExtensions>, host: &QString, source: &QString, on: bool);
        /// harlo's built-in Claude subscription extension: `{ state: "" |
        /// "preparing" | "ready" | "error" | "missing", step: "sdk" |
        /// "claude" (what it's downloading), error, on, inPi }` (`inPi`:
        /// pi has its own copy installed, which it loads instead).
        #[qinvokable]
        fn bridge_status(self: &PiExtensions) -> QString;
        /// Gets it ready (the first time, fetching Anthropic's Agent SDK) and
        /// loads it into the pi sessions started from then on.
        #[qinvokable]
        fn prepare_bridge(self: Pin<&mut PiExtensions>);
        /// Switches it on or off for pi sessions started from now on.
        #[qinvokable]
        fn set_bridge_enabled(self: Pin<&mut PiExtensions>, on: bool);
    }

    impl cxx_qt::Threading for PiExtensions {}
}

use std::collections::HashMap;
use std::pin::Pin;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::pi_bridge;
use harlo_core::pi_packages::{self, LIST_SCRIPT, Package, RECOMMENDED};
use harlo_core::remote;
use harlo_harness::HarnessKind;
use serde_json::{Value, json};

use qobject::PiExtensions;

/// Writes stdin to pi's settings (through a file beside it, so a reader
/// never sees half of it).
const WRITE_SETTINGS: &str = r#"d="${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}"; mkdir -p "$d" && cat >"$d/settings.json.harlo" && mv "$d/settings.json.harlo" "$d/settings.json""#;

/// A package being installed or removed (or just done).
#[derive(Clone)]
struct Job {
    /// "running", "done" or "failed"
    state: String,
    message: String,
}

/// One machine's packages, as last read.
#[derive(Default)]
struct Machine {
    /// "", "loading", "ready" or "error"
    state: String,
    error: String,
    packages: Vec<Package>,
    jobs: HashMap<String, Job>,
}

#[derive(Default)]
pub struct PiExtensionsRust {
    revision: i32,
    machines: HashMap<String, Machine>,
    bridge: Bridge,
}

/// harlo's built-in Claude subscription extension, here.
#[derive(Default)]
struct Bridge {
    /// "", "preparing", "ready", "error" or "missing" (not shipped)
    state: String,
    /// While preparing: "sdk", or "claude" (Claude Code's copy, for
    /// someone without their own).
    step: String,
    error: String,
    path: Option<std::path::PathBuf>,
    /// pi has its own copy installed.
    in_pi: bool,
}

/// harlo's Claude subscription extension (`here`: loaded here) on `host`,
/// for a pi session there: put there, with the Claude Code there that it
/// starts. None when the machine has no Claude Code of its own or its pi
/// has the extension already; nor when it can't be put there (said in the
/// log: the session starts without it).
pub fn bridge_on_host(host: &str, here: &[String]) -> harlo_harness::PiExtra {
    let mut extra = harlo_harness::PiExtra::default();
    for path in here {
        match pi_bridge::on_host(host, std::path::Path::new(path)) {
            Ok(Some((there, claude))) => {
                extra.0.push(there);
                extra.1.push(("HARLO_CLAUDE".to_owned(), claude));
            }
            Ok(None) => {}
            Err(e) => {
                tracing::event!(name: "pi.bridge.host.failed", tracing::Level::WARN, server.address = %host, error.message = %format_args!("{e:#}"), "Claude subscription on {{server.address}}: {{error.message}}");
            }
        }
    }
    extra
}

fn bridge_on() -> bool {
    harlo_core::db::Db::open(&harlo_core::db::default_path()).ok().and_then(|d| d.get(&format!("settings.{}", pi_bridge::SETTING)).ok().flatten()).is_none_or(|v| v != "0")
}

/// A script's command on `host` ("" this computer, with pi's program as
/// harlo finds it in `$PI`).
fn command_on(host: &str, script: &str) -> Result<Command, String> {
    if host.is_empty() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(script);
        if let Some(pi) = crate::accounts::program(HarnessKind::Pi) {
            cmd.env("PI", pi);
        }
        return Ok(cmd);
    }
    let h = remote::host(host).ok_or_else(|| format!("{host} isn't one of your hosts any more."))?;
    Ok(remote::command(&h, script))
}

/// Runs a script on `host` and waits (feeding it `input`).
pub(crate) fn run_on(host: &str, script: &str, input: Option<&[u8]>) -> Result<Output, String> {
    if !host.is_empty() {
        let h = remote::host(host).ok_or_else(|| format!("{host} isn't one of your hosts any more."))?;
        return remote::run_with_input(&h, script, input, Duration::from_mins(1)).map_err(|e| format!("{e:#}"));
    }
    let mut cmd = command_on(host, script)?;
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("couldn't run sh: {e}"))?;
    if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
        use std::io::Write as _;
        let _ = stdin.write_all(data);
    }
    child.wait_with_output().map_err(|e| e.to_string())
}

/// The packages on `host`, with pi's settings as read.
fn read(host: &str) -> Result<(Vec<Package>, Value), String> {
    let out = run_on(host, LIST_SCRIPT, None)?;
    pi_packages::parse(&String::from_utf8_lossy(&out.stdout))
}

/// Switches a package on or off on `host` (its settings read again first).
fn switch(host: &str, source: &str, on: bool) -> Result<(), String> {
    let (_, settings) = read(host)?;
    let Some(new) = pi_packages::switched(&settings, source, on) else { return Ok(()) };
    let text = serde_json::to_string_pretty(&new).map_err(|e| e.to_string())? + "\n";
    let out = run_on(host, WRITE_SETTINGS, Some(text.as_bytes()))?;
    if out.status.success() { Ok(()) } else { Err(format!("couldn't write pi's settings: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

impl PiExtensions {
    fn bump(mut self: Pin<&mut Self>) {
        let r = self.revision + 1;
        self.as_mut().set_revision(r);
    }

    pub fn status(&self, host: &QString) -> QString {
        let m = self.machines.get(&host.to_string());
        let jobs: serde_json::Map<String, Value> = m.map(|m| m.jobs.iter().map(|(k, j)| (k.clone(), json!({ "state": j.state, "message": j.message }))).collect()).unwrap_or_default();
        let v = json!({
            "state": m.map_or("", |m| m.state.as_str()),
            "error": m.map_or("", |m| m.error.as_str()),
            "packages": m.map(|m| m.packages.iter().map(Package::to_json).collect::<Vec<_>>()).unwrap_or_default(),
            "jobs": jobs,
            "recommended": RECOMMENDED.iter().map(|(source, title, detail)| json!({ "source": source, "title": title, "detail": detail })).collect::<Vec<_>>(),
        });
        QString::from(&v.to_string())
    }

    pub fn refresh(mut self: Pin<&mut Self>, host: &QString) {
        let host = host.to_string();
        let m = self.as_mut().rust_mut().get_mut().machines.entry(host.clone()).or_default();
        if m.state == "loading" {
            return;
        }
        m.state = "loading".into();
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let read = read(&host);
            let _ = thread.queue(move |mut this| {
                let m = this.as_mut().rust_mut().get_mut().machines.entry(host.clone()).or_default();
                let in_pi = match read {
                    Ok((packages, settings)) => {
                        (m.state, m.error, m.packages) = ("ready".into(), String::new(), packages);
                        Some(pi_bridge::installed_in_pi(&settings))
                    }
                    Err(why) => {
                        (m.state, m.error) = ("error".into(), why);
                        None
                    }
                };
                // (pi's own copy here: harlo's stays out of its way.)
                if host.is_empty()
                    && let Some(in_pi) = in_pi
                    && in_pi != this.bridge.in_pi
                {
                    this.as_mut().rust_mut().get_mut().bridge.in_pi = in_pi;
                    this.as_mut().apply_bridge();
                }
                this.bump();
            });
        });
    }

    pub fn install(self: Pin<&mut Self>, host: &QString, source: &QString) -> QString {
        let source = source.to_string().trim().to_owned();
        if !pi_packages::valid_source(&source) {
            return QString::from("pi installs from npm:<name>, git:<repository>, an https:// address or a folder's path.");
        }
        self.job(&host.to_string(), &source, "install");
        QString::default()
    }

    pub fn remove(self: Pin<&mut Self>, host: &QString, source: &QString) {
        self.job(&host.to_string(), &source.to_string(), "remove");
    }

    /// Runs `pi <verb> <source>` on `host` in the background, then reads
    /// the list again.
    fn job(mut self: Pin<&mut Self>, host: &str, source: &str, verb: &'static str) {
        if self.machines.get(host).and_then(|m| m.jobs.get(source)).is_some_and(|j| j.state == "running") {
            return;
        }
        let script = format!("\"${{PI:-pi}}\" {verb} {} 2>&1", remote::quote(source));
        let started = command_on(host, &script).and_then(|cmd| crate::jobs::start(cmd, &format!("pi-{verb}")).map_err(|e| e.to_string()));
        let doing = if verb == "install" { "Installing" } else { "Removing" };
        let m = self.as_mut().rust_mut().get_mut().machines.entry(host.to_owned()).or_default();
        let job = match started {
            Ok(job) => job,
            Err(why) => {
                m.jobs.insert(source.to_owned(), Job { state: "failed".into(), message: format!("Couldn't start pi: {why}") });
                self.bump();
                return;
            }
        };
        m.jobs.insert(source.to_owned(), Job { state: "running".into(), message: format!("{doing}…") });
        self.as_mut().bump();
        let thread = self.qt_thread();
        let (host, source) = (host.to_owned(), source.to_owned());
        crate::runtime().spawn_blocking(move || {
            // (npm can take a while; a stuck one is stopped after 10 minutes.)
            let (state, message) = match crate::jobs::wait(job, Duration::from_mins(10)) {
                Ok(()) => ("done", if verb == "install" { "Installed. pi sessions started from now on load it." } else { "Removed." }.to_owned()),
                Err(why) => ("failed", format!("{doing} failed: {why}")),
            };
            let _ = thread.queue(move |mut this| {
                if let Some(j) = this.as_mut().rust_mut().get_mut().machines.get_mut(&host).and_then(|m| m.jobs.get_mut(&source)) {
                    (j.state, j.message) = (state.into(), message);
                }
                this.as_mut().bump();
                let q = QString::from(&host);
                this.as_mut().refresh(&q);
                this.changed(q);
            });
        });
    }

    pub fn set_enabled(mut self: Pin<&mut Self>, host: &QString, source: &QString, on: bool) {
        let (host, source) = (host.to_string(), source.to_string());
        // (Shown switched at once; read again when it's written.)
        if let Some(p) = self.as_mut().rust_mut().get_mut().machines.get_mut(&host).and_then(|m| m.packages.iter_mut().find(|p| p.source == source)) {
            p.enabled = on;
        }
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let done = switch(&host, &source, on);
            let _ = thread.queue(move |mut this| {
                if let Err(why) = done {
                    let m = this.as_mut().rust_mut().get_mut().machines.entry(host.clone()).or_default();
                    m.jobs.insert(source, Job { state: "failed".into(), message: why });
                }
                let q = QString::from(&host);
                this.as_mut().refresh(&q);
                this.changed(q);
            });
        });
    }

    pub fn bridge_status(&self) -> QString {
        let b = &self.bridge;
        QString::from(&json!({ "state": b.state, "step": b.step, "error": b.error, "on": bridge_on(), "inPi": b.in_pi }).to_string())
    }

    pub fn prepare_bridge(mut self: Pin<&mut Self>) {
        if matches!(self.bridge.state.as_str(), "preparing" | "ready" | "missing") {
            return;
        }
        if pi_bridge::shipped().is_none() {
            self.as_mut().rust_mut().get_mut().bridge.state = "missing".into();
            self.bump();
            return;
        }
        let b = &mut self.as_mut().rust_mut().get_mut().bridge;
        (b.state, b.step, b.error) = ("preparing".into(), "sdk".into(), String::new());
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // (No Claude Code of their own, and a shipped tree that names
            // the SDK's copy: that, too.)
            let prepared = pi_bridge::prepare().and_then(|path| {
                if harlo_core::claude::installed().is_some() || !path.starts_with(pi_bridge::data_root().unwrap_or_default()) {
                    return Ok(path);
                }
                let _ = thread.queue(|mut this| {
                    this.as_mut().rust_mut().get_mut().bridge.step = "claude".into();
                    this.bump();
                });
                pi_bridge::prepare_claude().map(|_| path)
            });
            let _ = thread.queue(move |mut this| {
                let b = &mut this.as_mut().rust_mut().get_mut().bridge;
                match prepared {
                    Ok(path) => (b.state, b.path) = ("ready".into(), Some(path)),
                    Err(why) => (b.state, b.error) = ("error".into(), why),
                }
                this.as_mut().apply_bridge();
                this.as_mut().bump();
                // (Whether pi has its own copy.)
                this.as_mut().refresh(&QString::default());
            });
        });
    }

    pub fn set_bridge_enabled(mut self: Pin<&mut Self>, on: bool) {
        if let Ok(db) = harlo_core::db::Db::open(&harlo_core::db::default_path()) {
            let _ = db.set(&format!("settings.{}", pi_bridge::SETTING), if on { "1" } else { "0" });
        }
        self.as_mut().apply_bridge();
        self.bump();
    }

    /// Loads the extension into the pi sessions started from now on, or
    /// not (off, not ready, or pi's own copy installed); says pi's models
    /// changed.
    fn apply_bridge(mut self: Pin<&mut Self>) {
        let b = &self.bridge;
        let path = b.path.as_ref().filter(|_| b.state == "ready" && !b.in_pi && bridge_on());
        match path {
            Some(path) => {
                // (The Claude Code harlo found: the extension's stand-in for
                // the Agent SDK's own copy starts it.)
                let claude = harlo_core::claude::program().map_or_else(|| "claude".to_owned(), |p| p.to_string_lossy().into_owned());
                harlo_harness::set_pi_extensions(vec![path.to_string_lossy().into_owned()], vec![("HARLO_CLAUDE".to_owned(), claude)]);
            }
            None => harlo_harness::set_pi_extensions(Vec::new(), Vec::new()),
        }
        self.as_mut().changed(QString::default());
    }
}
