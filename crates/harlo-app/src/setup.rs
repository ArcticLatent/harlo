//! `Setup` (QML singleton): what the setup guide (and Settings) do that other
//! parts don't already:
//!
//! - Installing an agent on this computer, with its official one-line
//!   installer (as Settings → Providers does on a remote host).
//! - Signing git in to GitHub, GitLab or Forgejo, so pushes and pulls over
//!   HTTPS work, the git hosting service picked in Settings → Git:
//!   - GitHub: the GitHub CLI (`gh auth login`, a one-time code to enter on
//!     github.com), then `gh auth setup-git` so git asks gh.
//!   - GitLab: glab (`glab auth login --device`), and git's credential
//!     helper for that host set to glab.
//!   - Forgejo: an access token, checked with the server's API, then kept by
//!     git's credential store for that host only (the desktop keyring when
//!     git has libsecret).
//!   - A token works for GitHub and GitLab too (handed to gh / glab).
//!
//! Tokens go straight to the tool or to git; harlo keeps none, and never
//! reads the ones they keep.

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
        /// Bumped when an install or a git service's state changes.
        #[qproperty(i32, revision)]
        /// The git sign-in going on (or the last one): `{ id, service,
        /// state: "running" | "done" | "failed", code, url, lines, message }`;
        /// "" before one.
        #[qproperty(QString, git_flow, cxx_name = "gitFlow")]
        type Setup = super::SetupRust;
    }

    impl cxx_qt::Threading for Setup {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Installs an agent here (`installState` follows).
        #[qinvokable]
        fn install_agent(self: Pin<&mut Setup>, agent: &QString);
        /// `{ state: "running" | "done" | "failed", message }`, or "".
        #[qinvokable]
        fn install_state(self: &Setup, agent: &QString) -> QString;
        /// The installer's command line (shown before it runs).
        #[qinvokable]
        fn install_command(self: &Setup, agent: &QString) -> QString;
        /// A shell command that shows the installer's output.
        #[qinvokable]
        fn install_output_command(self: &Setup, agent: &QString) -> QString;
        /// Reads each git service's sign-in again (`gitStatus` follows).
        #[qinvokable]
        fn refresh_git(self: Pin<&mut Setup>);
        /// `{ service, label, host, cli, cliFound, state: "checking" |
        /// "ready", account, error }`.
        #[qinvokable]
        fn git_status(self: &Setup, service: &QString) -> QString;
        /// Signs git in to GitHub or GitLab with their tool's one-time code
        /// (`gitFlow` follows).
        #[qinvokable]
        fn git_sign_in(self: Pin<&mut Setup>, service: &QString);
        /// Signs git in with an access token (`gitFlow` follows).
        #[qinvokable]
        fn git_sign_in_with_token(self: Pin<&mut Setup>, service: &QString, token: &QString);
        /// Stops the git sign-in going on.
        #[qinvokable]
        fn git_cancel(self: Pin<&mut Setup>);
        /// Signs git out of a service (`gitStatus` follows).
        #[qinvokable]
        fn git_sign_out(self: Pin<&mut Setup>, service: &QString);
    }
}

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::{git_hosting, remote, signin};
use harlo_harness::HarnessKind;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use qobject::Setup;

#[derive(Default, Clone)]
struct Install {
    state: String,
    message: String,
    log: PathBuf,
    pid: u32,
}

#[derive(Default)]
pub struct SetupRust {
    revision: i32,
    git_flow: QString,
    installs: HashMap<String, Install>,
    git: HashMap<String, Value>,
    /// The git sign-in going on: its id, and its line to stop it (dropped:
    /// cancelled).
    current: Option<(u64, mpsc::UnboundedSender<()>)>,
    next_id: u64,
}

fn db() -> Option<harlo_core::db::Db> {
    harlo_core::db::Db::open(&harlo_core::db::default_path()).ok()
}

pub(crate) fn setting(key: &str) -> String {
    db().and_then(|d| d.get(&format!("settings.{key}")).ok().flatten()).unwrap_or_default()
}

fn set_setting(key: &str, value: &str) {
    if let Some(d) = db() {
        let _ = d.set(&format!("settings.{key}"), value);
    }
}

/// The host a service is used at: GitHub's; GitLab's and Forgejo's as set
/// in Settings → Git (gitlab.com, codeberg.org until then).
fn host_of(service: &str) -> String {
    let set = match service {
        "gitlab" => git_hosting::normalize_host(&setting("gitlab_host")),
        "forgejo" => git_hosting::normalize_host(&setting("forgejo_host")),
        _ => String::new(),
    };
    if set.is_empty() { git_hosting::default_host(service).to_owned() } else { set }
}

/// A program on PATH, or in the usual places a GUI's PATH leaves out.
fn find_program(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(h) = &home {
        dirs.push(h.join(".local/bin"));
    }
    dirs.extend(["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

/// A git service's tool, with the environment that keeps it quiet: no
/// colours, no update notices, no browser of its own (harlo shows the page).
pub(crate) fn tool(service: &str) -> Option<std::process::Command> {
    let path = find_program(git_hosting::cli(service)?)?;
    let mut c = std::process::Command::new(path);
    c.env("NO_COLOR", "1").env("GH_NO_UPDATE_NOTIFIER", "1").env("GH_BROWSER", "true").env("GLAB_CHECK_UPDATE", "false");
    Some(c)
}

/// Forgejo's API for a repository on `remote_host` (as its remote names it):
/// the server set in Settings → Git when it's that one (its port, http or
/// https), else Codeberg or another over https; with the token git keeps
/// for it (signing in put it there; harlo keeps none). None when git has
/// no token for it.
pub(crate) fn forgejo_api(remote_host: &str) -> Option<git_hosting::Forgejo> {
    let set = setting("forgejo_host");
    let name = |h: &str| h.split(':').next().unwrap_or(h).to_lowercase();
    let base = if !set.trim().is_empty() && name(&git_hosting::normalize_host(&set)) == name(remote_host) {
        git_hosting::forgejo_base(&set)
    } else {
        format!("https://{remote_host}")
    };
    let (protocol, host) = base.split_once("://")?;
    // (Never asks: no terminal, no password window.)
    let mut c = std::process::Command::new("git");
    c.args(["credential", "fill"]).env("GIT_TERMINAL_PROMPT", "0").env("GIT_ASKPASS", "true").env("SSH_ASKPASS", "true");
    let out = run(c, Some(&format!("protocol={protocol}\nhost={host}\n\n"))).ok()?;
    let token = out.lines().find_map(|l| l.strip_prefix("password=")).map(str::trim).filter(|t| !t.is_empty())?;
    Some(git_hosting::Forgejo::new(&base, token))
}

/// Runs a command, `input` on its stdin; its output, or why it failed.
pub(crate) fn run(mut c: std::process::Command, input: Option<&str>) -> Result<String, String> {
    c.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = signin::plain(&format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
    if out.status.success() {
        Ok(text)
    } else {
        Err(text.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("It didn't finish.").to_owned())
    }
}

fn git(args: &[&str], input: Option<&str>) -> Result<String, String> {
    let mut c = std::process::Command::new("git");
    c.args(args).env("GIT_TERMINAL_PROMPT", "0");
    run(c, input)
}

/// Who is signed in to a service, as its tool (or, for Forgejo, the last
/// sign-in here) says.
fn status_of(service: &str) -> Value {
    let host = host_of(service);
    let cli = git_hosting::cli(service).unwrap_or_default();
    let base = json!({ "service": service, "label": git_hosting::label(service), "host": host, "cli": cli, "state": "ready" });
    let with = |found: bool, account: &str, error: &str| {
        let mut v = base.clone();
        v["cliFound"] = json!(found);
        v["account"] = json!(account);
        v["error"] = json!(error);
        v
    };
    if service == "forgejo" {
        let saved = setting("forgejo_account");
        // (`host account`: the sign-in is for the host set then.)
        let account = saved.split_once(' ').filter(|(h, _)| *h == host).map(|(_, a)| a.to_owned()).unwrap_or_default();
        return with(false, &account, "");
    }
    let Some(mut c) = tool(service) else { return with(false, "", "") };
    c.args(["auth", "status", "--hostname", &host]);
    // (Not signed in, they exit non-zero; what they say is all that matters.)
    let text = match run(c, None) {
        Ok(t) => t,
        Err(t) => t,
    };
    with(true, &git_hosting::account_in_status(&text, &host).unwrap_or_default(), "")
}

/// The credential helper git asks for a host's username and password.
fn set_helper(host: &str, helper: &str) -> Result<(), String> {
    git(&["config", "--global", "--replace-all", &format!("credential.https://{host}.helper"), helper], None).map(|_| ())
}

/// After a sign-in: git asks the service's tool for that host.
fn connect_git(service: &str, host: &str) -> Result<(), String> {
    match service {
        "github" => {
            let mut c = tool(service).ok_or("The GitHub CLI isn't installed.")?;
            c.args(["auth", "setup-git", "--hostname", host]);
            run(c, None).map(|_| ())
        }
        "gitlab" => {
            let glab = find_program("glab").ok_or("glab isn't installed.")?;
            set_helper(host, &format!("!{} auth git-credential", glab.display()))
        }
        _ => Ok(()),
    }
}

/// Forgejo: the token checked, then kept by git for that host only (in the
/// desktop keyring when git has libsecret, else git's own file).
fn forgejo_sign_in(host: &str, token: &str) -> Result<String, String> {
    let base = git_hosting::forgejo_base(&setting("forgejo_host"));
    let base = if setting("forgejo_host").trim().is_empty() { format!("https://{host}") } else { base };
    let login = git_hosting::forgejo_user(&base, token).map_err(|e| format!("{e:#}"))?;
    let exec = git(&["--exec-path"], None).unwrap_or_default();
    let libsecret = PathBuf::from(exec.trim()).join("git-credential-libsecret").is_file();
    set_helper(host, if libsecret { "libsecret" } else { "store" })?;
    let protocol = base.split("://").next().unwrap_or("https");
    git(&["credential", "approve"], Some(&format!("protocol={protocol}\nhost={host}\nusername={login}\npassword={}\n\n", token.trim())))?;
    set_setting("forgejo_account", &format!("{host} {login}"));
    Ok(if libsecret {
        format!("Signed in to {host} as {login}. git keeps the token in your keyring.")
    } else {
        format!("Signed in to {host} as {login}. git keeps the token in ~/.git-credentials (it has no keyring helper here).")
    })
}

// ---- the git sign-in going on ------------------------------------------------

#[derive(Clone, Default)]
struct GitFlow {
    id: u64,
    service: String,
    state: String,
    code: String,
    url: String,
    lines: Vec<String>,
    message: String,
}

impl GitFlow {
    fn json(&self) -> String {
        json!({ "id": self.id, "service": self.service, "state": self.state, "code": self.code, "url": self.url,
                "lines": self.lines, "message": self.message })
        .to_string()
    }
}

/// How a sign-in tells the page what's happening.
#[derive(Clone)]
struct Report {
    flow: std::sync::Arc<std::sync::Mutex<GitFlow>>,
    thread: CxxQtThread<Setup>,
}

impl Report {
    fn update(&self, f: impl FnOnce(&mut GitFlow)) {
        let (id, json) = {
            let mut fl = self.flow.lock().unwrap();
            f(&mut fl);
            (fl.id, fl.json())
        };
        let _ = self.thread.queue(move |mut this| {
            if this.flow_id() == id {
                this.as_mut().set_git_flow(QString::from(&json));
            }
        });
    }

    fn finish(&self, result: Result<String, String>) {
        self.update(|f| {
            if f.state == "running" {
                let ok = result.is_ok();
                f.state = if ok { "done" } else { "failed" }.into();
                f.message = result.unwrap_or_else(|e| e);
            }
        });
        let _ = self.thread.queue(|mut this| {
            this.as_mut().rust_mut().current = None;
            this.refresh_git();
        });
    }
}

/// Runs a tool's device sign-in: shows the one-time code and the page it
/// prints, and waits for it to end (or to be cancelled).
async fn device_sign_in(mut cmd: tokio::process::Command, report: &Report, mut stop: mpsc::UnboundedReceiver<()>) -> Result<(), String> {
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("no input")?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    for mut pipe in [Box::new(child.stdout.take().ok_or("no output")?) as Box<dyn tokio::io::AsyncRead + Unpin + Send>, Box::new(child.stderr.take().ok_or("no output")?)] {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            while let Ok(n) = pipe.read(&mut buf).await {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let (mut pending, mut last, mut open) = (String::new(), Vec::<String>::new(), true);
    let status = loop {
        tokio::select! {
            chunk = rx.recv(), if open => match chunk {
                Some(bytes) => {
                    pending.push_str(&String::from_utf8_lossy(&bytes));
                    // (Lines, and a prompt left without a newline.)
                    while let Some(at) = pending.find(['\n', '\r']) {
                        let line = signin::plain(&pending[..at]).trim().to_owned();
                        pending.drain(..=at);
                        if line.is_empty() {
                            continue;
                        }
                        if line.to_lowercase().contains("press enter") {
                            let _ = stdin.write_all(b"\n").await;
                            continue;
                        }
                        let (code, url) = (git_hosting::one_time_code(&line), signin::url_in(&line));
                        last.push(line.clone());
                        report.update(|f| {
                            if let Some(c) = code { f.code = c; }
                            if let Some(u) = url { f.url = u; }
                            if f.lines.len() < 30 { f.lines.push(line); }
                        });
                    }
                }
                None => open = false,
            },
            _ = tokio::time::sleep(Duration::from_millis(400)), if pending.to_lowercase().contains("press enter") => {
                pending.clear();
                let _ = stdin.write_all(b"\n").await;
            },
            s = stop.recv() => if s.is_none() { return Err("Cancelled.".into()) },
            status = child.wait(), if !open => break status,
        }
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(last.iter().rev().find(|l| { let l = l.to_lowercase(); l.contains("error") || l.contains("fail") }).or(last.last()).cloned().unwrap_or_else(|| "It didn't finish.".into())),
    }
}

impl Setup {
    fn flow_id(&self) -> u64 {
        serde_json::from_str::<Value>(&self.git_flow.to_string()).ok().and_then(|v| v["id"].as_u64()).unwrap_or(0)
    }

    fn bump(mut self: Pin<&mut Self>) {
        let n = self.revision + 1;
        self.as_mut().set_revision(n);
    }

    pub fn install_agent(mut self: Pin<&mut Self>, agent: &QString) {
        let agent = agent.to_string();
        let command = remote::install_command(&agent);
        if command.is_empty() || self.installs.get(&agent).is_some_and(|i| i.state == "running") {
            return;
        }
        let label = HarnessKind::parse(&agent).map_or(agent.clone(), |k| k.label().to_owned());
        let mut c = std::process::Command::new("sh");
        c.args(["-c", &format!("{command} 2>&1")]);
        let job = match crate::jobs::start(c, &format!("install-local-{agent}")) {
            Ok(j) => j,
            Err(e) => {
                self.as_mut().rust_mut().installs.insert(agent, Install { state: "failed".into(), message: format!("Couldn't start the installer: {e:#}"), ..Default::default() });
                self.bump();
                return;
            }
        };
        self.as_mut().rust_mut().installs.insert(agent.clone(), Install { state: "running".into(), message: format!("Installing {label}…"), log: job.log.clone(), pid: job.pid });
        self.as_mut().bump();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // (Installers download a lot; a stuck one is stopped after 20 minutes.)
            let (state, message) = match crate::jobs::wait(job, Duration::from_secs(20 * 60)) {
                Ok(()) => ("done", format!("{label} is installed.")),
                Err(why) => ("failed", format!("Installing {label} failed: {why}")),
            };
            let _ = thread.queue(move |mut this| {
                if let Some(i) = this.as_mut().rust_mut().installs.get_mut(&agent) {
                    i.state = state.into();
                    i.message = message;
                }
                this.bump();
            });
        });
    }

    pub fn install_state(&self, agent: &QString) -> QString {
        match self.installs.get(&agent.to_string()) {
            Some(i) => QString::from(&json!({ "state": i.state, "message": i.message }).to_string()),
            None => QString::default(),
        }
    }

    pub fn install_command(&self, agent: &QString) -> QString {
        QString::from(remote::install_command(&agent.to_string()))
    }

    pub fn install_output_command(&self, agent: &QString) -> QString {
        let Some(i) = self.installs.get(&agent.to_string()).filter(|i| i.pid > 0) else { return QString::default() };
        QString::from(&crate::jobs::output_command(&i.log, i.pid, i.state == "running"))
    }

    pub fn refresh_git(mut self: Pin<&mut Self>) {
        for service in git_hosting::SERVICES {
            let was = self.git.get(service).cloned().unwrap_or_else(|| json!({ "service": service, "label": git_hosting::label(service), "host": host_of(service), "account": "" }));
            let mut checking = was;
            checking["state"] = json!("checking");
            self.as_mut().rust_mut().git.insert(service.to_owned(), checking);
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let v = status_of(service);
                let _ = thread.queue(move |mut this| {
                    this.as_mut().rust_mut().git.insert(service.to_owned(), v);
                    this.bump();
                });
            });
        }
        self.bump();
    }

    pub fn git_status(&self, service: &QString) -> QString {
        let s = service.to_string();
        let v = self.git.get(&s).cloned().unwrap_or_else(|| json!({ "service": s, "label": git_hosting::label(&s), "host": host_of(&s), "state": "checking", "account": "" }));
        QString::from(&v.to_string())
    }

    /// A new sign-in: its flow on the page, and its report.
    fn begin(mut self: Pin<&mut Self>, service: &str) -> (Report, mpsc::UnboundedReceiver<()>) {
        self.as_mut().rust_mut().next_id += 1;
        let id = self.next_id;
        let (tx, rx) = mpsc::unbounded_channel();
        self.as_mut().rust_mut().current = Some((id, tx));
        let flow = GitFlow { id, service: service.to_owned(), state: "running".into(), ..Default::default() };
        self.as_mut().set_git_flow(QString::from(&flow.json()));
        (Report { flow: std::sync::Arc::new(std::sync::Mutex::new(flow)), thread: self.qt_thread() }, rx)
    }

    pub fn git_sign_in(mut self: Pin<&mut Self>, service: &QString) {
        let service = service.to_string();
        let host = host_of(&service);
        let Some(std) = tool(&service) else { return };
        let mut cmd = tokio::process::Command::from(std);
        match service.as_str() {
            "github" => cmd.args(["auth", "login", "--hostname", &host, "--web", "--git-protocol", "https", "--skip-ssh-key"]),
            "gitlab" => cmd.args(["auth", "login", "--hostname", &host, "--device", "--git-protocol", "https"]),
            _ => return,
        };
        let (report, stop) = self.as_mut().begin(&service);
        crate::runtime().spawn(async move {
            let result = match device_sign_in(cmd, &report, stop).await {
                Ok(()) => {
                    let (s, h) = (service.clone(), host.clone());
                    tokio::task::spawn_blocking(move || connect_git(&s, &h)).await.map_err(|e| e.to_string()).and_then(|r| r)
                        .map(|()| format!("git is signed in to {host}."))
                        .map_err(|e| format!("Signed in, but setting up git failed: {e}"))
                }
                Err(e) => Err(e),
            };
            report.finish(result);
        });
    }

    pub fn git_sign_in_with_token(mut self: Pin<&mut Self>, service: &QString, token: &QString) {
        let (service, token) = (service.to_string(), token.to_string());
        if token.trim().is_empty() {
            return;
        }
        let host = host_of(&service);
        let (report, _stop) = self.as_mut().begin(&service);
        crate::runtime().spawn_blocking(move || {
            let result = match service.as_str() {
                "forgejo" => forgejo_sign_in(&host, &token),
                "github" | "gitlab" => (|| {
                    let mut c = tool(&service).ok_or_else(|| format!("{} isn't installed.", git_hosting::cli(&service).unwrap_or_default()))?;
                    if service == "github" {
                        c.args(["auth", "login", "--hostname", &host, "--with-token", "--git-protocol", "https"]);
                    } else {
                        c.args(["auth", "login", "--hostname", &host, "--stdin", "--git-protocol", "https"]);
                    }
                    run(c, Some(&format!("{}\n", token.trim())))?;
                    connect_git(&service, &host).map_err(|e| format!("Signed in, but setting up git failed: {e}"))?;
                    Ok(format!("git is signed in to {host}."))
                })(),
                _ => Err("Unknown service.".into()),
            };
            report.finish(result);
        });
    }

    pub fn git_cancel(mut self: Pin<&mut Self>) {
        // (Dropping its line stops the tool.)
        self.as_mut().rust_mut().current = None;
        let mut v: Value = serde_json::from_str(&self.git_flow.to_string()).unwrap_or_default();
        if v["state"] == "running" {
            v["state"] = json!("cancelled");
            self.as_mut().set_git_flow(QString::from(&v.to_string()));
        }
    }

    pub fn git_sign_out(mut self: Pin<&mut Self>, service: &QString) {
        let service = service.to_string();
        let host = host_of(&service);
        let account = self.git.get(&service).and_then(|v| v["account"].as_str().map(str::to_owned)).unwrap_or_default();
        let thread = self.qt_thread();
        let mut checking = self.git.get(&service).cloned().unwrap_or_default();
        checking["state"] = json!("checking");
        self.as_mut().rust_mut().git.insert(service.clone(), checking);
        self.as_mut().bump();
        crate::runtime().spawn_blocking(move || {
            let result = match service.as_str() {
                "forgejo" => {
                    let protocol = if setting("forgejo_host").trim().to_lowercase().starts_with("http://") { "http" } else { "https" };
                    let r = git(&["credential", "reject"], Some(&format!("protocol={protocol}\nhost={host}\nusername={account}\n\n"))).map(|_| ());
                    set_setting("forgejo_account", "");
                    r
                }
                _ => match tool(&service) {
                    Some(mut c) => {
                        c.args(["auth", "logout", "--hostname", &host]);
                        if service == "github" && !account.is_empty() {
                            c.args(["--user", &account]);
                        }
                        run(c, None).map(|_| ())
                    }
                    None => Ok(()),
                },
            };
            let v = {
                let mut v = status_of(&service);
                if let Err(e) = result {
                    v["error"] = json!(e);
                }
                v
            };
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().git.insert(service, v);
                this.bump();
            });
        });
    }
}
