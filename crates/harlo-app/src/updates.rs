//! `Updates` (QML singleton): newer versions of the agents, here and on
//! the remote hosts, shown as notices (Dismiss / Update), and the updates
//! themselves, run in the background (their output a click away).
//! Checked at startup, every few hours and when a host connects (Settings
//! → General → Check for updates, with Check now); a dismissed version
//! isn't offered again.

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
        /// `[{ key, host, agent, label, current, latest, state, message,
        /// notes, notesUrl }]`: state "available", "running", "done" or
        /// "failed"; host "" here; notes (harlo's): what's new, in short,
        /// and the full changelog's address.
        #[qproperty(QString, entries, cxx_name = "entriesJson")]
        /// Check now is looking.
        #[qproperty(bool, checking)]
        /// What Check now found ("" before it's used).
        #[qproperty(QString, check_status, cxx_name = "checkStatus")]
        type Updates = super::UpdatesRust;
    }

    impl cxx_qt::Threading for Updates {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Looks for newer versions of the agents installed here (the ones
        /// on in Settings → Providers).
        #[qinvokable]
        fn check_local(self: Pin<&mut Updates>);
        /// Check now (Settings): asks again (nothing remembered), and says
        /// what it found in `checkStatus`.
        #[qinvokable]
        fn check_now(self: Pin<&mut Updates>);
        /// The hosts as `Remotes.hostsJson()` has them: the connected ones'
        /// agents are compared with the newest versions.
        #[qinvokable]
        fn note_hosts(self: Pin<&mut Updates>, hosts: &QString);
        #[qinvokable]
        fn update(self: Pin<&mut Updates>, key: &QString);
        /// Not now: this version isn't offered again.
        #[qinvokable]
        fn dismiss(self: Pin<&mut Updates>, key: &QString);
        /// A command that shows an update's output in a terminal.
        #[qinvokable]
        fn output_command(self: &Updates, key: &QString) -> QString;
        /// harlo updated: starts the new one a moment after this one quits
        /// (QML quits).
        #[qinvokable]
        fn restart_harlo(self: &Updates);
    }
}

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::{providers, remote, updates};
use harlo_harness::HarnessKind;
use serde_json::{Value, json};

use qobject::Updates;

#[derive(Default, Clone)]
struct Item {
    /// "" here, else the host's name.
    host: String,
    agent: String,
    current: String,
    latest: String,
    state: String,
    message: String,
    /// harlo's: what's new (a few lines) and the full changelog.
    notes: String,
    notes_url: String,
    log: PathBuf,
    pid: u32,
}

#[derive(Default)]
pub struct UpdatesRust {
    entries: QString,
    checking: bool,
    check_status: QString,
    /// By "host/agent" ("local/…" here).
    items: BTreeMap<String, Item>,
    /// The versions last compared per host, so an unchanged host isn't
    /// looked at again.
    seen: HashMap<String, Value>,
}

const DISMISSED: &str = "settings.update_dismissed";

fn db() -> Option<harlo_core::db::Db> {
    harlo_core::db::Db::open(&harlo_core::db::default_path()).ok()
}

fn setting(key: &str) -> String {
    db().and_then(|d| d.get(&format!("settings.{key}")).ok().flatten()).unwrap_or_default()
}

fn dismissed() -> serde_json::Map<String, Value> {
    db().and_then(|d| d.get(DISMISSED).ok().flatten()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// How long a looked-up version is trusted before asking again.
const REMEMBER: Duration = Duration::from_secs(3600);

/// The newest version of an agent (looked up at most hourly; `fresh`:
/// now).
fn latest(agent: &str, fresh: bool) -> Option<String> {
    // Self-tests never ask the registry (only `HARLO_TEST_LATEST`).
    if std::env::args().any(|a| a.starts_with("--selftest")) && std::env::var_os("HARLO_TEST_LATEST").is_none() {
        return None;
    }
    static CACHE: Mutex<Option<HashMap<String, (String, Instant)>>> = Mutex::new(None);
    if let Some((v, at)) = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(agent)
        && !fresh
        && at.elapsed() < REMEMBER
    {
        return Some(v.clone());
    }
    let v = updates::latest(agent)?;
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(agent.to_owned(), (v.clone(), Instant::now()));
    Some(v)
}

/// How this harlo was installed (None: built from source: no updates
/// offered; a package's or an AppImage's update themselves).
fn harlo_kind() -> Option<updates::InstallKind> {
    let exe = std::env::current_exe().ok()?;
    updates::InstallKind::detect(&exe, std::env::var("APPIMAGE").ok().as_deref())
}

/// harlo's newest release (looked up at most hourly; `fresh`: now;
/// self-tests only ask `HARLO_UPDATE_MANIFEST_URL`). Ok(None): not asked.
fn harlo_latest(fresh: bool) -> Result<Option<updates::Release>, String> {
    if std::env::args().any(|a| a.starts_with("--selftest")) && std::env::var_os("HARLO_UPDATE_MANIFEST_URL").is_none() {
        return Ok(None);
    }
    static CACHE: Mutex<Option<(updates::Release, Instant)>> = Mutex::new(None);
    if let Some((r, at)) = CACHE.lock().unwrap().as_ref()
        && !fresh
        && at.elapsed() < REMEMBER
    {
        return Ok(Some(r.clone()));
    }
    let r = updates::harlo_release().map_err(|e| {
        tracing::warn!("harlo's release: {e:#}");
        format!("{e:#}")
    })?;
    *CACHE.lock().unwrap() = Some((r.clone(), Instant::now()));
    Ok(Some(r))
}

/// How much of what's new an update's notice shows (the rest a click away).
const NOTES_SHOWN: usize = 240;

/// What's new from `current` to `latest`, in short (remembered: a
/// release's changelog doesn't change); "" if it can't be read.
fn harlo_notes(current: &str, latest: &str) -> String {
    static CACHE: Mutex<Option<(String, String)>> = Mutex::new(None);
    let key = format!("{current}>{latest}");
    if let Some((k, notes)) = CACHE.lock().unwrap().as_ref()
        && *k == key
    {
        return notes.clone();
    }
    match updates::harlo_changes(current, latest) {
        Ok(text) => {
            let notes = updates::excerpt(&text, NOTES_SHOWN);
            *CACHE.lock().unwrap() = Some((key, notes.clone()));
            notes
        }
        Err(e) => {
            tracing::warn!("harlo's changelog: {e:#}");
            String::new()
        }
    }
}

/// Whether a program is on PATH.
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// A downloaded package ready to install: the command, the file, and the
/// same command to type in a terminal should it fail here.
struct Install {
    command: std::process::Command,
    file: PathBuf,
    manual: String,
}

/// Downloads the new harlo for how it was installed, checks it against the
/// release's signed manifest, and puts it in place (an AppImage) or returns
/// what installs it (a package).
fn fetch_harlo(kind: &updates::InstallKind) -> Result<Option<Install>, String> {
    let release = updates::harlo_release().map_err(|e| format!("{e:#}."))?;
    let asset = release.asset(kind).cloned().ok_or("The release has no file for this kind of install.")?;
    let dir = harlo_core::db::default_path().parent().map(|d| d.join("updates")).ok_or("no data folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // (Only the file name: the manifest is signed, but a name is all it needs.)
    let name = std::path::Path::new(&asset.name).file_name().ok_or("The release's file has no name.")?;
    let file = dir.join(name);
    let got = updates::download(&asset.download_url, &file).map_err(|e| format!("{e:#}"))?;
    if got != asset.sha256 {
        let _ = std::fs::remove_file(&file);
        return Err("The download didn't match its checksum, so it wasn't installed.".into());
    }
    match kind {
        updates::InstallKind::AppImage(path) => {
            // Beside it, then over it (a running AppImage can be replaced).
            use std::os::unix::fs::PermissionsExt;
            let next = path.with_extension("new");
            std::fs::copy(&file, &next).map_err(|e| format!("writing {}: {e}", next.display()))?;
            std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            std::fs::rename(&next, path).map_err(|e| format!("replacing {}: {e}", path.display()))?;
            let _ = std::fs::remove_file(&file);
            Ok(None)
        }
        _ => {
            let argv = kind.install_command(&file, on_path, &updates::elevation()).ok_or("no installer")?;
            let mut command = std::process::Command::new(&argv[0]);
            command.args(&argv[1..]);
            let manual = kind.manual_command(&file, on_path).unwrap_or_default();
            Ok(Some(Install { command, file, manual }))
        }
    }
}

fn label(agent: &str) -> String {
    HarnessKind::parse(agent).map_or(agent.to_owned(), |k| k.label().to_owned())
}

/// The program an agent runs as here (Settings → Providers' path, else the
/// installed one), if it's on.
fn local_program(agent: &str) -> Option<PathBuf> {
    let kind = HarnessKind::parse(agent)?;
    if setting(&format!("provider_{agent}")) == "0" {
        return None;
    }
    let configured = setting(&format!("{agent}_path"));
    if !configured.is_empty() {
        return Some(PathBuf::from(configured)).filter(|p| p.is_file());
    }
    harlo_harness::resolve_program(kind)
}

fn local_version(program: &std::path::Path) -> String {
    providers::version(program).map(|v| providers::version_number(&v)).unwrap_or_default()
}

impl Updates {
    fn publish(mut self: Pin<&mut Self>) {
        let list: Vec<Value> = self
            .items
            .iter()
            .map(|(key, i)| {
                json!({ "key": key, "host": i.host, "agent": i.agent, "label": label(&i.agent), "current": i.current, "latest": i.latest, "state": i.state, "message": i.message,
                        "notes": i.notes, "notesUrl": i.notes_url, "output": i.pid > 0 })
            })
            .collect();
        self.as_mut().set_entries(QString::from(&Value::from(list).to_string()));
    }

    /// Newer versions found (host, agent, current, latest): offered unless
    /// dismissed (or already being updated).
    fn found(mut self: Pin<&mut Self>, host: &str, found: Vec<(String, String, String)>) {
        let skip = dismissed();
        for (agent, current, newest) in found {
            let key = format!("{}/{agent}", if host.is_empty() { "local" } else { host });
            if self.items.get(&key).is_some_and(|i| i.state == "running") {
                continue;
            }
            if !updates::newer(&newest, &current) || skip.get(&key).and_then(Value::as_str) == Some(newest.as_str()) {
                // (Up to date now: an old offer goes.)
                if self.items.get(&key).is_some_and(|i| i.state == "available") {
                    self.as_mut().rust_mut().items.remove(&key);
                }
                continue;
            }
            let where_ = if host.is_empty() { String::new() } else { format!(" on {host}") };
            let message = format!("Version {newest} is available{where_} (you have {current}).");
            self.as_mut().rust_mut().items.insert(key, Item { host: host.to_owned(), agent, current, latest: newest, state: "available".into(), message, ..Default::default() });
        }
        self.publish();
    }

    pub fn check_local(self: Pin<&mut Self>) {
        self.check(false);
    }

    pub fn check_now(mut self: Pin<&mut Self>) {
        if *self.checking() {
            return;
        }
        self.as_mut().set_checking(true);
        self.check(true);
    }

    fn check(self: Pin<&mut Self>, fresh: bool) {
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut found: Vec<(String, String, String)> = remote::AGENTS
                .iter()
                .filter_map(|a| {
                    let program = local_program(a)?;
                    let current = local_version(&program);
                    let newest = latest(a, fresh)?;
                    (!current.is_empty()).then(|| (a.to_string(), current, newest))
                })
                .collect();
            // harlo itself (installed from a package or an AppImage).
            let packaged = harlo_kind().is_some();
            let current = env!("CARGO_PKG_VERSION");
            let (mut error, mut notes) = (None, None);
            if packaged {
                match harlo_latest(fresh) {
                    Ok(Some(r)) => {
                        // A newer one: what's new in it.
                        if updates::newer(&r.version, current) {
                            notes = Some((harlo_notes(current, &r.version), updates::harlo_changelog_url(&r.version)));
                        }
                        found.push(("harlo".into(), current.into(), r.version));
                    }
                    Ok(None) => {}
                    Err(e) => error = Some(e),
                }
            }
            let _ = thread.queue(move |mut this| {
                this.as_mut().found("", found);
                if let Some((text, url)) = notes
                    && let Some(item) = this.as_mut().rust_mut().items.get_mut("local/harlo")
                    && item.state == "available"
                {
                    (item.notes, item.notes_url) = (text, url);
                    this.as_mut().publish();
                }
                if fresh {
                    this.checked(packaged, error);
                }
            });
        });
    }

    /// Check now's answer: what's offered, or that all is up to date.
    fn checked(mut self: Pin<&mut Self>, packaged: bool, error: Option<String>) {
        let offered = self.items.iter().filter(|(k, i)| k.starts_with("local/") && i.state == "available").count();
        let version = env!("CARGO_PKG_VERSION");
        let status = match (error, offered) {
            (Some(e), _) => format!("Couldn't check for a newer harlo: {e}"),
            (None, 0) if packaged => format!("Everything is up to date (harlo {version})."),
            (None, 0) => format!("Your agents are up to date. This harlo ({version}) was built from source, so it doesn't update itself."),
            (None, 1) => "1 update found: it's offered at the bottom right of the window.".to_owned(),
            (None, n) => format!("{n} updates found: they're offered at the bottom right of the window."),
        };
        self.as_mut().set_check_status(QString::from(&status));
        self.set_checking(false);
    }

    pub fn note_hosts(mut self: Pin<&mut Self>, hosts: &QString) {
        let hosts: Vec<Value> = serde_json::from_str(&hosts.to_string()).unwrap_or_default();
        for h in hosts {
            let (Some(name), true) = (h["name"].as_str().map(str::to_owned), h["state"] == "connected") else { continue };
            let versions = h["versions"].clone();
            if self.seen.get(&name) == Some(&versions) {
                continue;
            }
            self.as_mut().rust_mut().seen.insert(name.clone(), versions.clone());
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let found: Vec<(String, String, String)> = versions
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter_map(|(agent, v)| {
                        let current = v.as_str()?.to_owned();
                        let newest = latest(agent, false)?;
                        Some((agent.clone(), current, newest))
                    })
                    .collect();
                let _ = thread.queue(move |this| this.found(&name, found));
            });
        }
    }

    pub fn update(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        let Some(item) = self.items.get(&key).cloned().filter(|i| i.state != "running") else { return };
        if item.agent == "harlo" {
            return self.update_harlo(key, item);
        }
        let args = updates::update_args(&item.agent);
        let (name, host) = (label(&item.agent), item.host.clone());
        // Here: the agent's own program; on a host, over SSH.
        let command = if host.is_empty() {
            let Some(program) = local_program(&item.agent) else { return };
            let mut c = std::process::Command::new(&program);
            c.args(args);
            Some((c, Some(program)))
        } else {
            remote::host(&host).map(|h| (remote::command(&h, &format!("{} {} 2>&1", item.agent, args.join(" "))), None))
        };
        let Some((command, program)) = command else { return };
        let job = match crate::jobs::start(command, &format!("update-{}-{}", if host.is_empty() { "local" } else { &host }, item.agent)) {
            Ok(j) => j,
            Err(e) => {
                if let Some(i) = self.as_mut().rust_mut().items.get_mut(&key) {
                    i.state = "failed".into();
                    i.message = format!("Couldn't start the update: {e:#}");
                }
                self.publish();
                return;
            }
        };
        if let Some(i) = self.as_mut().rust_mut().items.get_mut(&key) {
            i.state = "running".into();
            i.message = format!("Updating {name} to {}…", item.latest);
            i.log = job.log.clone();
            i.pid = job.pid;
        }
        self.as_mut().publish();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = crate::jobs::wait(job, Duration::from_secs(20 * 60));
            // What it says it is now.
            let now = match (&program, remote::host(&host)) {
                (Some(p), _) => local_version(p),
                (None, Some(h)) => remote::run(&h, &format!("{} --version 2>/dev/null </dev/null | head -n 1", item.agent), Duration::from_secs(20))
                    .map(|o| providers::version_number(String::from_utf8_lossy(&o.stdout).trim()))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            let where_ = if host.is_empty() { String::new() } else { format!(" on {host}") };
            let (state, message) = match result {
                Ok(()) if !now.is_empty() && !updates::newer(&item.latest, &now) => ("done", format!("{name}{where_} is now {now}. New sessions use it.")),
                Ok(()) => ("done", format!("The update finished, but {name}{where_} still says {}.", if now.is_empty() { item.current.clone() } else { now.clone() })),
                Err(why) => ("failed", format!("Updating {name}{where_} failed: {why}")),
            };
            let _ = thread.queue(move |mut this| {
                if let Some(i) = this.as_mut().rust_mut().items.get_mut(&key) {
                    i.state = state.into();
                    i.message = message;
                    if !now.is_empty() {
                        i.current = now;
                    }
                }
                this.publish();
            });
        });
    }

    /// harlo's own update: downloaded and checked here, then installed (a
    /// package: its manager, with the system's password dialog; an
    /// AppImage: its file replaced).
    fn update_harlo(mut self: Pin<&mut Self>, key: String, item: Item) {
        let Some(kind) = harlo_kind() else { return };
        if let Some(i) = self.as_mut().rust_mut().items.get_mut(&key) {
            i.state = "running".into();
            i.message = format!("Downloading harlo {}…", item.latest);
        }
        self.as_mut().publish();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let set = |state: &'static str, message: String| {
                let key = key.clone();
                let _ = thread.queue(move |mut this| {
                    if let Some(i) = this.as_mut().rust_mut().items.get_mut(&key) {
                        i.state = state.into();
                        i.message = message;
                    }
                    this.publish();
                });
            };
            let installed = format!("harlo {} is installed. Restart harlo to use it.", item.latest);
            match fetch_harlo(&kind) {
                Err(why) => set("failed", format!("Updating harlo failed: {why}")),
                Ok(None) => set("done", installed),
                Ok(Some(Install { command, file, manual })) => {
                    let job = match crate::jobs::start(command, "update-local-harlo") {
                        Ok(j) => j,
                        Err(e) => return set("failed", format!("Couldn't start installing it: {e:#}")),
                    };
                    let (log, pid, k) = (job.log.clone(), job.pid, key.clone());
                    let _ = thread.queue(move |mut this| {
                        if let Some(i) = this.as_mut().rust_mut().items.get_mut(&k) {
                            i.message = "Installing (your password, if asked)…".into();
                            i.log = log;
                            i.pid = pid;
                        }
                        this.publish();
                    });
                    match crate::jobs::wait(job, Duration::from_secs(20 * 60)) {
                        Ok(()) => {
                            set("done", installed);
                            let _ = std::fs::remove_file(file);
                        }
                        // (The file stays, for installing it by hand.)
                        Err(why) => set("failed", format!("Installing harlo {} failed: {why}. To install it from a terminal: {manual}", item.latest)),
                    }
                }
            }
        });
    }

    /// The new harlo starts once this one has quit (QML quits next).
    pub fn restart_harlo(&self) {
        if let Some(program) = updates::this_program()
            && let Err(e) = updates::relaunch_after_exit(&program)
        {
            tracing::warn!("harlo won't start again by itself: {e}");
        }
    }

    pub fn dismiss(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        let Some(item) = self.as_mut().rust_mut().items.remove(&key) else { return };
        // An offer not taken: not again for this version.
        if item.state == "available"
            && let Some(db) = db()
        {
            let mut map = dismissed();
            map.insert(key, json!(item.latest));
            let _ = db.set(DISMISSED, &Value::Object(map).to_string());
        }
        self.publish();
    }

    pub fn output_command(&self, key: &QString) -> QString {
        let Some(i) = self.items.get(&key.to_string()).filter(|i| i.pid > 0) else { return QString::default() };
        QString::from(&crate::jobs::output_command(&i.log, i.pid, i.state == "running"))
    }
}
