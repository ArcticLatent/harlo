//! `Mcp` (QML singleton): Settings → Tools for agents. Lists, saves and removes the MCP
//! servers pi reads (globally or in a project, here or on a remote host),
//! and tests them for a status (pi doesn't report one).

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
        /// Bumped when servers are saved or a test finishes.
        #[qproperty(i32, revision)]
        type Mcp = super::McpRust;
    }

    impl cxx_qt::Threading for Mcp {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// A scope ("global" or a project's path): `{ servers: [{ name,
        /// form }], errors, file }`.
        #[qinvokable]
        fn list_json(self: &Mcp, scope: &QString) -> QString;
        /// Saves a server (`form` JSON); renames from `old_name`. An error,
        /// or "".
        #[qinvokable]
        fn save(self: Pin<&mut Mcp>, scope: &QString, old_name: &QString, form: &QString) -> QString;
        #[qinvokable]
        fn remove(self: Pin<&mut Mcp>, scope: &QString, name: &QString) -> QString;
        /// The terminal command that signs pi in to a server of `scope`
        /// (`pi mcp login`, in the project's folder for a project's; on
        /// its machine for a host's).
        #[qinvokable]
        fn pi_login_command(self: &Mcp, scope: &QString, name: &QString) -> QString;
        /// Turns a server on or off (tests it when turned on). An error, or "".
        #[qinvokable]
        fn set_enabled(self: Pin<&mut Mcp>, scope: &QString, name: &QString, on: bool) -> QString;
        /// Starts testing a server (`statusJson` follows).
        #[qinvokable]
        fn test(self: Pin<&mut Mcp>, scope: &QString, name: &QString);
        /// Tests every enabled server of a scope.
        #[qinvokable]
        fn test_all(self: Pin<&mut Mcp>, scope: &QString);
        /// `{ name: { state, error, server, tools: [{ name, description }] } }`;
        /// state "testing", "connected", "failed" or "needs_auth".
        #[qinvokable]
        fn status_json(self: &Mcp, scope: &QString) -> QString;
        /// `{ forms: [...], error }` from a pasted snippet.
        #[qinvokable]
        fn parse_import(self: &Mcp, text: &QString) -> QString;
        /// A pasted command line as words (JSON array).
        #[qinvokable]
        fn split_command(self: &Mcp, line: &QString) -> QString;
        /// The agent is installed (its path in Settings → Agents, else
        /// found).
        #[qinvokable]
        fn installed(self: &Mcp, harness: &QString) -> bool;
        /// The agent is installed where a scope's servers run (here, or
        /// the host of a `host:` / remote project scope).
        #[qinvokable]
        fn installed_on(self: &Mcp, harness: &QString, scope: &QString) -> bool;
        /// The servers harlo installs with a click, in a scope ("global",
        /// or "host:<name>" for a host's): `[{ id, name, about, license,
        /// repo, version, on, taken, needs, node, nodeError, latest, stars,
        /// busy, progress, message, error }]` (as last looked:
        /// `catalog_refresh` looks again).
        #[qinvokable]
        fn catalog_json(self: &Mcp, scope: &QString) -> QString;
        /// Looks at them again there, and for a newer release (at most
        /// hourly unless `force`).
        #[qinvokable]
        fn catalog_refresh(self: Pin<&mut Mcp>, scope: &QString, force: bool);
        /// Turns one on there (downloading it first if it isn't yet), or off.
        #[qinvokable]
        fn catalog_set(self: Pin<&mut Mcp>, scope: &QString, id: &QString, on: bool);
        /// Updates one there to its newest release.
        #[qinvokable]
        fn catalog_update(self: Pin<&mut Mcp>, scope: &QString, id: &QString);
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::mcp_catalog;
use harlo_core::mcp::{self, Form, Scope};
use harlo_core::mcp_probe::{self, Probe};
use harlo_harness::HarnessKind;
use serde_json::{Value, json};

use qobject::Mcp;

#[derive(Default)]
pub struct McpRust {
    revision: i32,
    /// Test results by (scope, server).
    status: HashMap<(String, String), Probe>,
    /// The servers harlo offers, as last looked, with what's being done to
    /// them, by (scope, id).
    catalog: HashMap<(String, String), Value>,
    /// Their newest releases and stars, by id.
    releases: HashMap<String, Value>,
    /// When a newer release was last looked for.
    catalog_checked: Option<std::time::Instant>,
}

fn form_json(f: &Form) -> Value {
    json!({
        "name": f.name, "enabled": f.enabled, "transport": f.transport, "command": f.command, "url": f.url,
        "headers": f.headers, "env": f.env, "cwd": f.cwd, "timeoutSecs": f.timeout_secs,
    })
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p[0].as_str()?.to_owned(), p[1].as_str().unwrap_or_default().to_owned())))
        .collect()
}

fn form_from(v: &Value) -> Form {
    Form {
        name: v["name"].as_str().unwrap_or_default().trim().to_owned(),
        enabled: v["enabled"].as_bool().unwrap_or(true),
        transport: v["transport"].as_str().unwrap_or("stdio").to_owned(),
        command: v["command"].as_array().into_iter().flatten().filter_map(|w| w.as_str()).map(str::to_owned).filter(|w| !w.is_empty()).collect(),
        url: v["url"].as_str().unwrap_or_default().trim().to_owned(),
        headers: pairs(&v["headers"]),
        env: pairs(&v["env"]),
        cwd: v["cwd"].as_str().unwrap_or_default().to_owned(),
        timeout_secs: v["timeoutSecs"].as_f64().filter(|t| *t > 0.0),
    }
}

fn probe_json(p: &Probe) -> Value {
    json!({
        "state": p.state, "error": p.error, "server": p.server,
        "tools": p.tools.iter().map(|(n, d)| json!({ "name": n, "description": d })).collect::<Vec<_>>(),
    })
}

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
impl Mcp {
    fn bump(mut self: Pin<&mut Self>) {
        let next = self.revision + 1;
        self.as_mut().set_revision(next);
    }

    pub fn list_json(&self, scope: &QString) -> QString {
        let scope = Scope::parse(&scope.to_string());
        let listing = mcp::load(&scope);
        let servers: Vec<Value> = listing
            .servers
            .iter()
            // (harlo's own: under Ready to install.)
            .filter(|s| mcp_catalog::Place::of(&scope).is_none_or(|p| mcp_catalog::which(s, &p).is_none()))
            .map(|s| json!({ "name": s.name, "form": form_json(s) }))
            .collect();
        let file = harlo_core::sidebar::tilde(&mcp::config_path(&scope));
        QString::from(&json!({ "servers": servers, "errors": listing.errors, "file": file }).to_string())
    }

    pub fn save(mut self: Pin<&mut Self>, scope: &QString, old_name: &QString, form: &QString) -> QString {
        let scope_s = scope.to_string();
        let form = form_from(&serde_json::from_str(&form.to_string()).unwrap_or_default());
        match mcp::save(&Scope::parse(&scope_s), &old_name.to_string(), &form) {
            Ok(()) => {
                // A changed server's old test result no longer applies.
                let mut this = self.as_mut().rust_mut();
                this.status.remove(&(scope_s.clone(), old_name.to_string()));
                this.status.remove(&(scope_s, form.name.clone()));
                self.bump();
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn remove(mut self: Pin<&mut Self>, scope: &QString, name: &QString) -> QString {
        match mcp::remove(&Scope::parse(&scope.to_string()), &name.to_string()) {
            Ok(()) => {
                self.as_mut().rust_mut().status.remove(&(scope.to_string(), name.to_string()));
                self.bump();
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn set_enabled(mut self: Pin<&mut Self>, scope: &QString, name: &QString, on: bool) -> QString {
        match mcp::set_enabled(&Scope::parse(&scope.to_string()), &name.to_string(), on) {
            Ok(()) => {
                self.as_mut().rust_mut().status.remove(&(scope.to_string(), name.to_string()));
                if on {
                    self.as_mut().test(scope, name);
                }
                self.bump();
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn test(self: Pin<&mut Self>, scope: &QString, name: &QString) {
        self.test_names(&scope.to_string(), &[name.to_string()]);
    }

    pub fn test_all(self: Pin<&mut Self>, scope: &QString) {
        let s = scope.to_string();
        let names: Vec<String> = mcp::load(&Scope::parse(&s)).servers.into_iter().filter(|f| f.enabled).map(|f| f.name).collect();
        self.test_names(&s, &names);
    }

    fn test_names(mut self: Pin<&mut Self>, scope_s: &str, names: &[String]) {
        let scope = Scope::parse(scope_s);
        let listing = mcp::load(&scope);
        // Where a stdio server runs: the project, a host's home (over
        // SSH), or here.
        let project: Option<PathBuf> = match &scope {
            Scope::Project(p) => Some(p.clone()),
            Scope::Host(h) => Some(mcp::host_home(h)),
            Scope::Global => None,
        };
        for name in names {
            let Some(server) = listing.servers.iter().find(|s| s.name == *name) else { continue };
            let key = (scope_s.to_owned(), name.clone());
            if self.status.get(&key).is_some_and(|p| p.state == "testing") {
                continue;
            }
            self.as_mut().rust_mut().status.insert(key.clone(), Probe { state: "testing".into(), ..Default::default() });
            let form = server.clone();
            let project = project.clone();
            // At most the server's own timeout, else 30 s.
            let timeout = Duration::from_secs_f64(form.timeout_secs.unwrap_or(30.0).clamp(3.0, 30.0));
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let result = mcp_probe::probe(&form, project.as_deref(), timeout);
                let _ = thread.queue(move |mut this| {
                    this.as_mut().rust_mut().status.insert(key, result);
                    this.bump();
                });
            });
        }
        self.bump();
    }

    #[expect(clippy::unused_self, reason = "QML calls it as a method of its object")]
    pub fn pi_login_command(&self, scope: &QString, name: &QString) -> QString {
        let args = ["mcp".to_owned(), "login".to_owned(), name.to_string()];
        let (host, cwd) = match Scope::parse(&scope.to_string()) {
            Scope::Global => (None, None),
            Scope::Host(h) => (harlo_core::remote::host(&h), None),
            Scope::Project(p) => match harlo_core::remote::split(&p) {
                Some((h, dir)) => (harlo_core::remote::host(&h), Some(dir)),
                None => (None, Some(p.to_string_lossy().into_owned())),
            },
        };
        QString::from(&crate::accounts::pi_terminal_command_in(&args, host.as_ref(), cwd.as_deref()))
    }

    pub fn status_json(&self, scope: &QString) -> QString {
        let scope = scope.to_string();
        let map: serde_json::Map<String, Value> = self.status.iter().filter(|((s, _), _)| *s == scope).map(|((_, n), p)| (n.clone(), probe_json(p))).collect();
        QString::from(&Value::Object(map).to_string())
    }

    pub fn parse_import(&self, text: &QString) -> QString {
        let result = match mcp::parse_import(&text.to_string()) {
            Ok(forms) => json!({ "forms": forms.iter().map(form_json).collect::<Vec<_>>(), "error": "" }),
            Err(e) => json!({ "forms": [], "error": format!("{e:#}") }),
        };
        QString::from(&result.to_string())
    }

    pub fn split_command(&self, line: &QString) -> QString {
        QString::from(&Value::from(mcp::split_command(&line.to_string())).to_string())
    }

    pub fn installed(&self, harness: &QString) -> bool {
        HarnessKind::parse(&harness.to_string()).and_then(crate::accounts::program).is_some()
    }

    pub fn catalog_json(&self, scope: &QString) -> QString {
        let scope = scope.to_string();
        let list: Vec<Value> = mcp_catalog::OFFERED
            .iter()
            .map(|b| {
                let mut v = self.catalog.get(&(scope.clone(), b.id.to_owned())).cloned().unwrap_or_else(|| json!({ "id": b.id, "name": b.name, "about": b.about, "license": b.license, "repo": b.repo, "version": "", "loading": true }));
                if let (Some(v), Some(r)) = (v.as_object_mut(), self.releases.get(b.id).and_then(Value::as_object)) {
                    v.extend(r.clone());
                }
                v
            })
            .collect();
        QString::from(&Value::from(list).to_string())
    }

    /// Changes the fields of one's entry in a scope.
    fn catalog_put(mut self: Pin<&mut Self>, scope: &str, id: &str, fields: &Value) {
        let mut this = self.as_mut().rust_mut();
        let entry = this.catalog.entry((scope.to_owned(), id.to_owned())).or_insert_with(|| json!({}));
        if let (Some(e), Some(f)) = (entry.as_object_mut(), fields.as_object()) {
            e.extend(f.clone());
        }
        self.bump();
    }

    /// Its newest release and stars (`fields`).
    fn release_put(mut self: Pin<&mut Self>, id: &str, fields: &Value) {
        let mut this = self.as_mut().rust_mut();
        let entry = this.releases.entry(id.to_owned()).or_insert_with(|| json!({}));
        if let (Some(e), Some(f)) = (entry.as_object_mut(), fields.as_object()) {
            e.extend(f.clone());
        }
        self.bump();
    }

    pub fn catalog_refresh(mut self: Pin<&mut Self>, scope: &QString, force: bool) {
        let scope = scope.to_string();
        let Some(place) = mcp_catalog::Place::of(&Scope::parse(&scope)) else { return };
        let look_up = force || self.catalog_checked.is_none_or(|t| t.elapsed() > Duration::from_secs(3600));
        if look_up {
            self.as_mut().rust_mut().catalog_checked = Some(std::time::Instant::now());
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            for b in mcp_catalog::OFFERED {
                let mut st = mcp_catalog::status(&b, &place);
                st["loading"] = json!(false);
                let scope = scope.clone();
                let _ = thread.queue(move |this| this.catalog_put(&scope, b.id, &st));
            }
            if look_up {
                for b in mcp_catalog::OFFERED {
                    // (Offline, or GitHub's limit: nothing to say.)
                    if let Ok((latest, _)) = mcp_catalog::latest(&b) {
                        let _ = thread.queue(move |this| this.release_put(b.id, &json!({ "latest": latest })));
                    }
                    if let Ok(stars) = mcp_catalog::stars(&b) {
                        let _ = thread.queue(move |this| this.release_put(b.id, &json!({ "stars": stars })));
                    }
                }
            }
        });
    }

    /// Runs `work` for one in a scope (in the background, `busy` meanwhile,
    /// with its download's progress), then shows how it is and tests it
    /// when it's on.
    fn catalog_work(mut self: Pin<&mut Self>, scope: String, id: &str, busy: &str, work: impl FnOnce(&mcp_catalog::Offered, &mcp_catalog::Place, &mut dyn FnMut(u64, u64)) -> anyhow::Result<String> + Send + 'static) {
        let Some(b) = mcp_catalog::get(id) else { return };
        let Some(place) = mcp_catalog::Place::of(&Scope::parse(&scope)) else { return };
        self.as_mut().catalog_put(&scope, b.id, &json!({ "busy": busy, "progress": 0.0, "error": "", "message": "" }));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut shown = std::time::Instant::now();
            let t = thread.clone();
            let progress_scope = scope.clone();
            let mut progress = |done: u64, total: u64| {
                if total > 0 && shown.elapsed() > Duration::from_millis(150) {
                    shown = std::time::Instant::now();
                    let p = done as f64 / total as f64;
                    let scope = progress_scope.clone();
                    let _ = t.queue(move |this| this.catalog_put(&scope, b.id, &json!({ "progress": p })));
                }
            };
            let result = work(&b, &place, &mut progress);
            let mut st = mcp_catalog::status(&b, &place);
            st["busy"] = json!("");
            let (message, error) = match result {
                Ok(m) => (m, String::new()),
                Err(e) => (String::new(), format!("{e:#}")),
            };
            st["message"] = json!(message);
            st["error"] = json!(error);
            let _ = thread.queue(move |mut this| {
                let test = st["error"] == "" && st["on"] == true;
                this.as_mut().rust_mut().status.remove(&(scope.clone(), b.id.to_owned()));
                this.as_mut().catalog_put(&scope, b.id, &st);
                if test {
                    this.as_mut().test(&QString::from(&scope), &QString::from(b.id));
                }
            });
        });
    }

    pub fn catalog_set(self: Pin<&mut Self>, scope: &QString, id: &QString, on: bool) {
        self.catalog_work(scope.to_string(), &id.to_string(), if on { "on" } else { "off" }, move |b, place, progress| {
            if on { mcp_catalog::enable(b, place, progress) } else { mcp_catalog::disable(b, place) }.map(|()| String::new())
        });
    }

    pub fn catalog_update(self: Pin<&mut Self>, scope: &QString, id: &QString) {
        self.catalog_work(scope.to_string(), &id.to_string(), "update", |b, place, progress| mcp_catalog::update(b, place, progress));
    }

    pub fn installed_on(&self, harness: &QString, scope: &QString) -> bool {
        let scope = scope.to_string();
        let host = scope.strip_prefix("host:").map(str::to_owned).or_else(|| harlo_core::remote::host_of(std::path::Path::new(&scope)));
        match host {
            Some(h) => harlo_core::remote::found_on(&h, &harness.to_string()),
            None => self.installed(harness),
        }
    }
}
