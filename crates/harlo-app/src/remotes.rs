//! `Remotes` (QML singleton): the SSH hosts of Settings → Remote Instances,
//! their connections and what was found on them. Hosts are saved in the
//! settings ("remote_hosts") and handed to `harlo_core::remote`, which the
//! rest of the app uses for `ssh://host/…` paths.

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
        /// Bumped whenever hosts or their state change.
        #[qproperty(i32, revision)]
        /// The last `request_folders` result: `{ path, display, dirs, error,
        /// loading }`.
        #[qproperty(QString, folders, cxx_name = "foldersJson")]
        type Remotes = super::RemotesRust;
    }

    impl cxx_qt::Threading for Remotes {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// `[{ name, ssh, state, message, os, home, pi, omp }]`; state is
        /// "", "connecting", "connected" or "error".
        #[qinvokable]
        fn hosts_json(self: &Remotes) -> QString;
        /// Adds (`old_name` "") or changes a host; an error, or "".
        #[qinvokable]
        fn save_host(self: Pin<&mut Remotes>, old_name: &QString, name: &QString, ssh: &QString) -> QString;
        #[qinvokable]
        fn delete_host(self: Pin<&mut Remotes>, name: &QString);
        /// Opens the shared connection and looks for pi and omp.
        #[qinvokable]
        fn connect_host(self: Pin<&mut Remotes>, name: &QString);
        #[qinvokable]
        fn disconnect_host(self: Pin<&mut Remotes>, name: &QString);
        /// Measures a connected host's round trip (`ping` in `hosts_json`).
        #[qinvokable]
        fn ping_host(self: Pin<&mut Remotes>, name: &QString);
        /// Lists the folders in `path` on a host (`ssh://…`, or a host name
        /// for its home) into `foldersJson`.
        #[qinvokable]
        fn request_folders(self: Pin<&mut Remotes>, path: &QString);
        /// Hosts in ~/.ssh/config: `[{ name, hostname, user, port, added }]`
        /// (`added`: a host here already uses that name or command).
        #[qinvokable]
        fn ssh_config_hosts_json(self: &Remotes) -> QString;
        /// `nas:~/proj` for an `ssh://` path (else the path).
        #[qinvokable]
        fn display_path(self: &Remotes, path: &QString) -> QString;
        /// Runs an agent's installer on a host in the background (its
        /// progress in `hosts_json`'s `installs`); the host is checked
        /// again when it's done.
        #[qinvokable]
        fn install_agent(self: Pin<&mut Remotes>, host: &QString, agent: &QString);
        /// A command that shows an install's output in a terminal (live
        /// while it runs), or "".
        #[qinvokable]
        fn install_output_command(self: &Remotes, host: &QString, agent: &QString) -> QString;
    }
}

use std::collections::HashMap;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::remote::{self, Host, HostInfo};
use serde_json::{Value, json};

use qobject::Remotes;

#[derive(Default, Clone)]
struct HostState {
    state: String,
    message: String,
    info: HostInfo,
    /// The last round trip, ms (None: not measured).
    ping: Option<u64>,
}

/// An agent being installed on a host (or just installed).
#[derive(Default, Clone)]
struct Install {
    /// "running", "done" or "failed"
    state: String,
    message: String,
    log: std::path::PathBuf,
    /// The local ssh running it (for following the log while it runs).
    pid: u32,
}

#[derive(Default)]
pub struct RemotesRust {
    /// By "host/agent".
    installs: HashMap<String, Install>,
    revision: i32,
    folders: QString,
    folders_generation: u64,
    states: HashMap<String, HostState>,
}

const SETTING: &str = "settings.remote_hosts";

fn db() -> Option<harlo_core::db::Db> {
    harlo_core::db::Db::open(&harlo_core::db::default_path()).ok()
}

/// Loads the saved hosts into `harlo_core::remote` (at startup).
pub fn load_saved() {
    let text = db().and_then(|d| d.get(SETTING).ok().flatten()).unwrap_or_default();
    let hosts: Vec<Host> = serde_json::from_str::<Vec<Value>>(&text)
        .unwrap_or_default()
        .iter()
        .filter_map(|h| {
            let name = h["name"].as_str()?.to_owned();
            let ssh = remote::parse_ssh_command(h["ssh"].as_str()?).ok()?;
            Some(Host { name, ssh })
        })
        .collect();
    remote::set_hosts(hosts);
}

fn save(hosts: &[Host]) {
    let list: Vec<Value> = hosts.iter().map(|h| json!({ "name": h.name, "ssh": ssh_text(h) })).collect();
    if let Some(d) = db() {
        let _ = d.set(SETTING, &Value::from(list).to_string());
    }
    remote::set_hosts(hosts.to_vec());
}

/// The host's SSH command as typed (`ssh` + its arguments).
fn ssh_text(h: &Host) -> String {
    h.ssh.iter().map(|w| remote::quote(w)).collect::<Vec<_>>().join(" ")
}

impl Remotes {
    fn bump(mut self: Pin<&mut Self>) {
        let next = self.revision + 1;
        self.as_mut().set_revision(next);
    }

    pub fn hosts_json(&self) -> QString {
        let list: Vec<Value> = remote::hosts()
            .iter()
            .map(|h| {
                let s = self.states.get(&h.name).cloned().unwrap_or_default();
                json!({
                    "name": h.name, "ssh": ssh_text(h), "state": s.state, "message": s.message, "ping": s.ping,
                    "os": s.info.os, "home": s.info.home, "pi": s.info.pi, "omp": s.info.omp,
                    "claude": s.info.claude, "opencode": s.info.opencode, "versions": s.info.versions, "claudeAuth": s.info.claude_auth,
                    "install": remote::AGENTS.iter().map(|a| (a.to_string(), json!(remote::install_command(a)))).collect::<serde_json::Map<_, _>>(),
                    "installs": remote::AGENTS
                        .iter()
                        .filter_map(|a| self.installs.get(&format!("{}/{a}", h.name)).map(|i| (a.to_string(), json!({ "state": i.state, "message": i.message }))))
                        .collect::<serde_json::Map<_, _>>(),
                })
            })
            .collect();
        QString::from(&Value::from(list).to_string())
    }

    pub fn save_host(mut self: Pin<&mut Self>, old_name: &QString, name: &QString, ssh: &QString) -> QString {
        let (old, name) = (old_name.to_string(), name.to_string().trim().to_owned());
        if !remote::valid_name(&name) {
            return QString::from("Name it with letters, digits, - _ or . (no spaces).");
        }
        let args = match remote::parse_ssh_command(&ssh.to_string()) {
            Ok(a) => a,
            Err(e) => return QString::from(&e.to_string()),
        };
        let mut hosts = remote::hosts();
        if name != old && hosts.iter().any(|h| h.name == name) {
            return QString::from(&format!("There is already a host called {name}."));
        }
        let host = Host { name: name.clone(), ssh: args };
        match hosts.iter().position(|h| h.name == old).filter(|_| !old.is_empty()) {
            Some(i) => {
                // A changed command needs a new connection.
                if hosts[i].ssh != host.ssh {
                    remote::disconnect(&hosts[i]);
                }
                hosts[i] = host;
                let state = self.as_mut().rust_mut().states.remove(&old);
                if let Some(s) = state.filter(|_| old == name) {
                    self.as_mut().rust_mut().states.insert(name, s);
                }
            }
            None => hosts.push(host),
        }
        save(&hosts);
        self.bump();
        QString::default()
    }

    pub fn delete_host(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let mut hosts = remote::hosts();
        if let Some(h) = hosts.iter().find(|h| h.name == name) {
            remote::disconnect(h);
        }
        hosts.retain(|h| h.name != name);
        save(&hosts);
        self.as_mut().rust_mut().states.remove(&name);
        self.bump();
    }

    pub fn connect_host(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let Some(host) = remote::host(&name) else { return };
        self.as_mut().rust_mut().states.insert(name.clone(), HostState { state: "connecting".into(), ..Default::default() });
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = remote::check(&host);
            let _ = thread.queue(move |mut this| {
                let state = match result {
                    Ok(info) => {
                        let message = match (info.pi.is_empty(), info.omp.is_empty()) {
                            (true, true) => "Connected, but neither pi nor omp was found on its PATH.".to_owned(),
                            _ => String::new(),
                        };
                        HostState { state: "connected".into(), message, info, ping: None }
                    }
                    Err(e) => HostState { state: "error".into(), message: format!("{e:#}"), info: HostInfo::default(), ping: None },
                };
                let connected = state.state == "connected";
                this.as_mut().rust_mut().states.insert(name.clone(), state);
                this.as_mut().bump();
                if connected {
                    this.ping_host(&QString::from(&name));
                }
            });
        });
    }

    pub fn install_agent(mut self: Pin<&mut Self>, host: &QString, agent: &QString) {
        let (name, agent) = (host.to_string(), agent.to_string());
        let Some(host) = remote::host(&name) else { return };
        let command = remote::install_command(&agent);
        let key = format!("{name}/{agent}");
        if command.is_empty() || self.installs.get(&key).is_some_and(|i| i.state == "running") {
            return;
        }
        let label = harlo_harness::HarnessKind::parse(&agent).map_or(agent.clone(), |k| k.label().to_owned());
        let job = match crate::jobs::start(remote::command(&host, &format!("{command} 2>&1")), &format!("install-{name}-{agent}")) {
            Ok(j) => j,
            Err(e) => {
                self.as_mut().rust_mut().installs.insert(key, Install { state: "failed".into(), message: format!("Couldn't start the installer: {e:#}"), ..Default::default() });
                self.bump();
                return;
            }
        };
        self.as_mut().rust_mut().installs.insert(key.clone(), Install { state: "running".into(), message: format!("Installing {label} on {name}…"), log: job.log.clone(), pid: job.pid });
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // (Installers download a lot; a stuck one is stopped after 20 minutes.)
            let (state, message) = match crate::jobs::wait(job, std::time::Duration::from_secs(20 * 60)) {
                Ok(()) => ("done", format!("{label} is installed on {name}.")),
                Err(why) => ("failed", format!("Installing {label} on {name} failed: {why}")),
            };
            let _ = thread.queue(move |mut this| {
                if let Some(i) = this.as_mut().rust_mut().installs.get_mut(&key) {
                    i.state = state.into();
                    i.message = message;
                }
                this.as_mut().bump();
                // What's there now (its version, or still missing).
                this.connect_host(&QString::from(&name));
            });
        });
    }

    pub fn install_output_command(&self, host: &QString, agent: &QString) -> QString {
        let Some(i) = self.installs.get(&format!("{host}/{agent}")) else { return QString::default() };
        QString::from(&crate::jobs::output_command(&i.log, i.pid, i.state == "running"))
    }

    pub fn ping_host(self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let Some(host) = remote::host(&name) else { return };
        if self.states.get(&name).is_none_or(|s| s.state != "connected") {
            return;
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // A command over the shared connection: what each request costs.
            let started = std::time::Instant::now();
            let ok = remote::run(&host, "true", std::time::Duration::from_secs(10)).is_ok_and(|o| o.status.success());
            let ms = started.elapsed().as_millis() as u64;
            let _ = thread.queue(move |mut this| {
                if let Some(s) = this.as_mut().rust_mut().states.get_mut(&name) {
                    if ok {
                        s.ping = Some(ms);
                    } else {
                        s.state = "error".into();
                        s.message = format!("{name} stopped answering.");
                        s.ping = None;
                    }
                }
                this.bump();
            });
        });
    }

    pub fn disconnect_host(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        if let Some(host) = remote::host(&name) {
            remote::disconnect(&host);
        }
        self.as_mut().rust_mut().states.remove(&name);
        self.bump();
    }

    pub fn request_folders(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        self.as_mut().rust_mut().folders_generation += 1;
        let generation = self.folders_generation;
        self.as_mut().set_folders(QString::from(&json!({ "path": path, "display": "", "dirs": [], "error": "", "loading": true }).to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = list_folders(&path);
            let _ = thread.queue(move |mut this| {
                if this.folders_generation == generation {
                    this.as_mut().set_folders(QString::from(&result.to_string()));
                }
            });
        });
    }

    pub fn ssh_config_hosts_json(&self) -> QString {
        let ours = remote::hosts();
        let list: Vec<Value> = remote::ssh_config_hosts()
            .into_iter()
            .map(|h| {
                let added = ours.iter().any(|o| o.name == h.name || o.ssh.last().is_some_and(|d| *d == h.name));
                json!({ "name": h.name, "hostname": h.hostname, "user": h.user, "port": h.port, "added": added })
            })
            .collect();
        QString::from(&Value::from(list).to_string())
    }

    pub fn display_path(&self, path: &QString) -> QString {
        QString::from(&remote::display(std::path::Path::new(&path.to_string())))
    }
}

/// Folders in `path` (`ssh://…`, or a host name for its home).
fn list_folders(path: &str) -> Value {
    let target = if path.starts_with("ssh://") {
        std::path::PathBuf::from(path)
    } else {
        match remote::host(path).map(|h| remote::home(&h).map(|home| remote::join(&h.name, &home))) {
            Some(Ok(p)) => p,
            Some(Err(e)) => return json!({ "path": path, "display": path, "dirs": [], "error": format!("{e:#}"), "loading": false }),
            None => return json!({ "path": path, "display": path, "dirs": [], "error": "Unknown host.", "loading": false }),
        }
    };
    let display = remote::display(&target);
    match remote::list_dir(&target) {
        Ok(entries) => {
            let mut dirs: Vec<String> = entries.into_iter().filter(|e| e.is_dir).map(|e| e.name).collect();
            dirs.sort_by_key(|d| (d.starts_with('.'), d.to_lowercase()));
            json!({ "path": target.to_string_lossy(), "display": display, "dirs": dirs, "error": "", "loading": false })
        }
        Err(e) => json!({ "path": target.to_string_lossy(), "display": display, "dirs": [], "error": format!("{e:#}"), "loading": false }),
    }
}

